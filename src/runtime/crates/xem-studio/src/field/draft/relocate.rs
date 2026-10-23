//! Append only proven instruction islands. Opaque source bytes are never moved.
use super::{Compiled, Draft, Node, State, error, original_id, unsupported};
use crate::field::{FieldError, Instruction, decode};
use std::collections::{BTreeMap, BTreeSet};

fn instruction(code: &[u8], pc: usize) -> Result<Instruction, FieldError> {
    crate::field::decode::instruction(code, pc)
}
fn expanded(
    state: &State,
    id: &str,
    result: &mut Vec<String>,
    depth: usize,
) -> Result<(), FieldError> {
    if depth > 128 {
        return Err(error("insertion nesting exceeds 128"));
    }
    result.push(id.to_owned());
    if let Some(after) = state.after.get(id) {
        for id in after {
            expanded(state, id, result, depth + 1)?;
        }
    }
    Ok(())
}
fn root_anchor<'a>(state: &'a State, id: &'a str) -> Result<Option<&'a Node>, FieldError> {
    let mut current = id;
    for _ in 0..129 {
        let node = state
            .nodes
            .get(current)
            .ok_or_else(|| error("insertion anchor missing"))?;
        if node.source_pc.is_some() {
            return Ok(Some(node));
        }
        if let Some((parent, _)) = state
            .after
            .iter()
            .find(|(_, children)| children.iter().any(|child| child == current))
        {
            current = parent;
        } else if state
            .events
            .values()
            .any(|ids| ids.iter().any(|id| id == current))
        {
            return Ok(None);
        } else {
            return Err(error("authored node has no event or source owner"));
        }
    }
    Err(error("cyclic insertion ownership"))
}
fn bound_node<'a>(state: &'a State, id: &str) -> Result<&'a Node, FieldError> {
    state
        .nodes
        .get(id)
        .ok_or_else(|| error(format!("unknown node binding {id}")))
}
fn read_pc(code: &[u8], pc: usize) -> Result<usize, FieldError> {
    let bytes = code
        .get(pc + 1..pc + 3)
        .ok_or_else(|| error("truncated compiler jump"))?;
    Ok(u16::from_le_bytes([bytes[0], bytes[1]]) as usize)
}
fn jump(code: &mut Vec<u8>, target: usize) -> Result<(), FieldError> {
    let target =
        u16::try_from(target).map_err(|_| unsupported("jump target exceeds u16 PC space"))?;
    code.push(1);
    code.extend_from_slice(&target.to_le_bytes());
    Ok(())
}
fn validate_operand(ins: &Instruction, index: usize, actor_count: usize) -> Result<(), FieldError> {
    let desc = &ins.spec.operands[index];
    let value = ins.operands[index];
    let variable = desc.kind == "var"
        || (matches!(desc.kind.as_str(), "iv" | "sel") && ins.immediate[index] == Some(false));
    if (variable && value >= 2048) || (desc.kind == "bit" && (value >> 4) >= 2048) {
        return Err(error(format!(
            "operand {index}: variable byte reference outside 0..2047"
        )));
    }
    if desc.kind == "actor" && value != 0xfb && value < 0xfd && value as usize >= actor_count {
        return Err(error(format!(
            "operand {index}: direct actor outside the existing actor table"
        )));
    }
    Ok(())
}
fn changed(draft: &Draft, state: &State, pc: usize) -> bool {
    let node = &state.nodes[&original_id(pc)];
    let source = &draft.base.instructions[draft
        .base
        .instructions
        .binary_search_by_key(&pc, |i| i.pc)
        .expect("base node")];
    let original = super::canonical_node(&draft.base, &draft.bytes, source);
    node.bytes != original.bytes || node.targets != original.targets
}

pub(super) fn compile(draft: &Draft, state: &State) -> Result<Compiled, FieldError> {
    let base = &draft.base;
    let offset = base.bytecode_offset;
    let original = &draft.bytes[offset..];
    if state.after.is_empty()
        && state.events.is_empty()
        && !base
            .instructions
            .iter()
            .any(|ins| changed(draft, state, ins.pc))
    {
        return Ok(Compiled {
            bytes: draft.bytes.clone(),
            declared_size: base.declared_size,
            graph: base.clone(),
            bindings: base
                .instructions
                .iter()
                .map(|ins| (original_id(ins.pc), ins.pc))
                .collect(),
        });
    }
    let mut code = original.to_vec();
    let mut bindings: BTreeMap<String, usize> = base
        .instructions
        .iter()
        .map(|ins| (original_id(ins.pc), ins.pc))
        .collect();
    let mut modified = BTreeSet::new();
    for ins in &base.instructions {
        let node = bound_node(state, &original_id(ins.pc))?;
        if changed(draft, state, ins.pc) {
            draft.writable(ins.pc)?;
            if ins.ranges.len() != 1 || node.bytes.len() != ins.size {
                return Err(unsupported(
                    "operand editing requires a contiguous, same-size instruction",
                ));
            }
            code[ins.pc..ins.pc + ins.size].copy_from_slice(&node.bytes);
            modified.insert(node.id.clone());
        }
    }
    // Read changed selectors/modes before deciding fixed-relative dependencies.
    let source_instructions = base
        .instructions
        .iter()
        .map(|ins| instruction(&code, ins.pc))
        .collect::<Result<Vec<_>, _>>()?;
    for (before, after) in base.instructions.iter().zip(&source_instructions) {
        if before.size != after.size || before.spec != after.spec {
            return Err(unsupported(format!(
                "PC +0x{:04x}: operand edit changes instruction form/size",
                before.pc
            )));
        }
    }
    let mut islands: Vec<(usize, usize)> = Vec::new();
    for anchor in state.after.keys() {
        let Some(node) = root_anchor(state, anchor)? else {
            continue;
        };
        let pc = node.source_pc.expect("source anchor");
        let block = base
            .blocks
            .iter()
            .find(|b| b.start <= pc && pc < b.end)
            .ok_or_else(|| error("anchor block missing"))?;
        if block.kind == "pinned" {
            return Err(unsupported("cannot insert into pinned storage"));
        }
        islands.push((block.start, block.end));
    }
    // Dependency closure includes predecessors, paired units and fixed-relative
    // advances. An island may grow through ordinary units, never opaque gaps.
    loop {
        let previous = islands.clone();
        for (start, end) in &mut islands {
            for ins in &source_instructions {
                let ins_end = ins.pc + ins.size;
                let reach = ins
                    .skips
                    .iter()
                    .copied()
                    .max()
                    .unwrap_or(ins_end)
                    .max(ins_end);
                if ins.pc < *end && reach >= *start && (reach > ins_end || ins.spec.setup.is_some())
                {
                    *start = (*start).min(ins.pc.saturating_sub(ins.spec.setup.unwrap_or(0)));
                    *end = (*end).max(ins_end).max(reach);
                    if let Some(target) = source_instructions.iter().find(|i| i.pc == reach) {
                        *end = (*end).max(target.pc + target.size);
                    }
                }
            }
            // Include each intersected atomic block completely.
            for b in &base.blocks {
                if b.start < *end && *start < b.end {
                    *start = (*start).min(b.start);
                    *end = (*end).max(b.end);
                }
            }
            while *end - *start < 3 {
                let next = base
                    .blocks
                    .iter()
                    .find(|b| b.start == *end && b.kind != "pinned");
                if let Some(b) = next {
                    *end = b.end;
                } else if let Some(b) = base
                    .blocks
                    .iter()
                    .find(|b| b.end == *start && b.kind != "pinned")
                {
                    *start = b.start;
                } else {
                    return Err(unsupported(
                        "no three-byte trampoline fits without touching pinned storage",
                    ));
                }
            }
        }
        islands.sort_unstable();
        let mut merged: Vec<(usize, usize)> = Vec::new();
        for (start, end) in islands {
            if let Some(last) = merged.last_mut()
                && start < last.1
            {
                last.1 = last.1.max(end);
            } else {
                merged.push((start, end));
            }
        }
        islands = merged;
        if islands == previous {
            break;
        }
    }
    for &(start, end) in &islands {
        for b in &base.blocks {
            if b.start < end && start < b.end && b.kind == "pinned" {
                return Err(unsupported(format!(
                    "island intersects pinned storage: {}",
                    b.reason.as_deref().unwrap_or("opaque")
                )));
            }
        }
        for ins in &source_instructions {
            if ins.pc < end
                && start < ins.pc + ins.size
                && (!ins.inner.is_empty() || ins.ranges.len() != 1 || ins.size == 0)
            {
                return Err(unsupported(
                    "island contains an unresolved inner or wrapped instruction",
                ));
            }
            if ins.pc < end && start < ins.pc + ins.size && ins.spec.flow == "table" {
                return Err(unsupported(
                    "jump-table relocation needs an independently established index bound",
                ));
            }
            for (i, desc) in ins.spec.operands.iter().enumerate() {
                if desc.kind == "data" && (start..end).contains(&(ins.operands[i] as usize)) {
                    return Err(unsupported(
                        "a data operand references relocated instruction storage",
                    ));
                }
            }
        }
    }
    // Insertion cannot separate fixed-relative reads, setup/consumer pairs or
    // skip windows. Emit a complete unit before inserting at its outer boundary.
    for anchor in state.after.keys() {
        let node = bound_node(state, anchor)?;
        if let Some(pc) = node.source_pc {
            let ins = source_instructions
                .iter()
                .find(|i| i.pc == pc)
                .expect("source instruction");
            if matches!(
                ins.spec.flow.as_str(),
                "end" | "return" | "jump" | "hang" | "table"
            ) {
                return Err(unsupported("insertion anchor has no ordinary fallthrough"));
            }
            let position = pc + ins.size;
            for other in &source_instructions {
                if let Some(distance) = other.spec.setup
                    && other.pc >= distance
                    && position > other.pc - distance
                    && position <= other.pc
                {
                    return Err(unsupported("insertion would split a setup/consumer pair"));
                }
                if other.pc < position && other.skips.iter().any(|target| position <= *target) {
                    return Err(unsupported(
                        "insertion would change a fixed-relative alternate advance",
                    ));
                }
            }
        }
    }
    let mut scaffolds = BTreeMap::new();
    for &(start, end) in &islands {
        let destination = code.len();
        let mut ids = Vec::new();
        for ins in source_instructions
            .iter()
            .filter(|ins| start <= ins.pc && ins.pc < end)
        {
            expanded(state, &original_id(ins.pc), &mut ids, 0)?;
        }
        let mut expected = start;
        for id in &ids {
            let node = bound_node(state, id)?;
            if let Some(pc) = node.source_pc {
                if pc != expected {
                    return Err(unsupported(
                        "island is not completely covered by whole instructions",
                    ));
                }
                expected += node.bytes.len();
            }
            bindings.insert(id.clone(), code.len());
            code.extend_from_slice(&node.bytes);
        }
        if expected != end {
            return Err(unsupported("island ends in non-instruction bytes"));
        }
        // Explicit out-and-back bridge. A terminal/jump may leave it unreachable;
        // retaining it keeps ordinary and conditional fallthrough explicit.
        if end < original.len() {
            scaffolds.insert(code.len(), end);
            jump(&mut code, end)?;
        }
        let mut trampoline = Vec::new();
        jump(&mut trampoline, destination)?;
        code[start..start + 3].copy_from_slice(&trampoline);
        scaffolds.insert(start, destination);
    }
    for roots in state.events.values() {
        let mut ids = Vec::new();
        for root in roots {
            expanded(state, root, &mut ids, 0)?;
        }
        for id in &ids {
            let node = bound_node(state, id)?;
            bindings.insert(id.clone(), code.len());
            code.extend_from_slice(&node.bytes);
        }
    }
    if code.len() > 0x10000 {
        return Err(unsupported(
            "compiled bytecode exceeds the u16 PC address space",
        ));
    }
    let grew = code.len() != original.len();
    if grew {
        for [_, target] in base
            .diagnostics
            .outside
            .iter()
            .chain(&base.diagnostics.inner)
        {
            if *target >= original.len() && *target < code.len() {
                return Err(unsupported(
                    "append would activate a formerly unresolved destination",
                ));
            }
        }
        if !base.diagnostics.into.is_empty() {
            return Err(unsupported(
                "append is unsafe with a non-whole-instruction skip diagnostic",
            ));
        }
    }
    // Apply stable target bindings to active nodes only. This includes untouched
    // source callers of an instruction in an appended compound island.
    for (id, node) in &state.nodes {
        let pc = *bindings
            .get(id)
            .ok_or_else(|| error("authored node has no placement"))?;
        let ins = instruction(&code, pc)?;
        if ins.size != node.bytes.len() {
            return Err(unsupported(format!(
                "{id}: template is not one complete instruction"
            )));
        }
        if state.after.contains_key(id)
            && matches!(
                ins.spec.flow.as_str(),
                "end" | "return" | "jump" | "hang" | "table"
            )
        {
            return Err(unsupported("insertion anchor has no ordinary fallthrough"));
        }
        if node.source_pc.is_none()
            && (ins.spec.setup.is_some()
                || !ins.inner.is_empty()
                || !ins.skips.is_empty()
                || ins.spec.flow == "table")
        {
            return Err(unsupported(
                "new nodes require an ordinary instruction; compound creation is unsupported",
            ));
        }
        if node.source_pc.is_none()
            && ins
                .spec
                .operands
                .iter()
                .enumerate()
                .any(|(index, operand)| {
                    operand.kind == "addr" && !node.targets.contains_key(&index)
                })
        {
            return Err(error(format!(
                "{id}: every authored address operand requires an explicit stable-node target binding"
            )));
        }
        if (node.source_pc.is_none() || modified.contains(id))
            && ins.spec.operands.iter().enumerate().any(|(index, op)| {
                op.kind == "data"
                    && (ins.operands[index] as usize >= original.len()
                        || islands.iter().any(|(start, end)| {
                            (*start..*end).contains(&(ins.operands[index] as usize))
                        }))
            })
        {
            return Err(unsupported(
                "authored data operands must reference preserved source storage outside relocation islands",
            ));
        }
        for (index, target) in &node.targets {
            let desc = ins
                .spec
                .operands
                .get(*index)
                .ok_or_else(|| error("target operand index outside instruction"))?;
            if desc.kind != "addr" {
                return Err(error("only address operands accept node bindings"));
            }
            let target = *bindings
                .get(target)
                .ok_or_else(|| error("address binding names an unknown node"))?;
            let operand = pc + usize::from(ins.extended.is_some()) + desc.offset;
            let target = u16::try_from(target).map_err(|_| unsupported("target PC overflow"))?;
            if code[operand..operand + 2] != target.to_le_bytes() {
                if let Some(original_pc) = node.source_pc {
                    draft.writable(original_pc)?;
                }
                code[operand..operand + 2].copy_from_slice(&target.to_le_bytes());
            }
        }
        if node.source_pc.is_none() || modified.contains(id) {
            let ins = instruction(&code, pc)?;
            for index in 0..ins.operands.len() {
                validate_operand(&ins, index, base.actors.len())?;
            }
        }
    }
    let mut bytes = draft.bytes[..offset].to_vec();
    for (actor, row) in base.actors.iter().enumerate() {
        for (event, entry) in row.iter().enumerate() {
            let target = if let Some(ids) = state.events.get(&(actor, event)) {
                bindings[&ids[0]]
            } else {
                let skipped = *entry == 0 && original.first() == Some(&0xff);
                if skipped {
                    0
                } else {
                    *bindings
                        .get(&original_id(*entry as usize))
                        .ok_or_else(|| error("event entry has no source instruction"))?
                }
            };
            let target =
                u16::try_from(target).map_err(|_| unsupported("event entry PC overflow"))?;
            bytes[132 + actor * 64 + event * 2..134 + actor * 64 + event * 2]
                .copy_from_slice(&target.to_le_bytes());
        }
    }
    bytes.extend_from_slice(&code);
    let declared_size = if grew {
        bytes.len()
    } else {
        base.declared_size
    };
    let graph = decode(&bytes, Some(declared_size))?;
    // New/changed instructions must not introduce unresolved execution paths.
    // Existing untouched diagnostics remain visible, not normalized away.
    for [pc, target] in &graph.diagnostics.outside {
        if !base.diagnostics.outside.contains(&[*pc, *target]) {
            return Err(error(format!(
                "compiled instruction +0x{pc:04x} has an outside destination"
            )));
        }
    }
    for [pc, target] in &graph.diagnostics.inner {
        if !base.diagnostics.inner.contains(&[*pc, *target]) {
            return Err(unsupported(
                "edit introduced an unresolved inner continuation",
            ));
        }
    }
    if graph
        .diagnostics
        .overlaps
        .iter()
        .any(|entry| !base.diagnostics.overlaps.contains(entry))
        || graph
            .diagnostics
            .into
            .iter()
            .any(|entry| !base.diagnostics.into.contains(entry))
        || graph
            .diagnostics
            .unpaired
            .iter()
            .any(|entry| !base.diagnostics.unpaired.contains(entry))
    {
        return Err(unsupported(
            "edit introduces an overlapping instruction, non-whole-instruction skip or unpaired consumer",
        ));
    }
    for id in modified.iter().chain(
        state
            .nodes
            .keys()
            .filter(|id| state.nodes[*id].source_pc.is_none()),
    ) {
        let pc = bindings[id];
        let ins = instruction(&code, pc)?;
        for target in ins.successors.iter().chain(&ins.skips) {
            if *target >= code.len() {
                return Err(error("edited node targets outside bytecode"));
            }
        }
    }
    // Assert relocation preserved the original fixed-relative successor topology,
    // modulo inserted fallthrough nodes and explicit compiler bridge jumps.
    for &(start, end) in &islands {
        for before in source_instructions
            .iter()
            .filter(|i| start <= i.pc && i.pc < end)
        {
            let id = original_id(before.pc);
            let after = instruction(&code, bindings[&id])?;
            if before.spec != after.spec
                || before.immediate != after.immediate
                || before
                    .spec
                    .operands
                    .iter()
                    .enumerate()
                    .any(|(index, desc)| {
                        desc.kind != "addr" && before.operands[index] != after.operands[index]
                    })
            {
                return Err(unsupported(
                    "relocation changes an instruction form or a neighboring-byte operand read",
                ));
            }
            for old_target in &before.successors {
                // Explicit targets are authored bindings, not fixed-relative
                // interpreter paths. They were encoded from the target table.
                if before
                    .spec
                    .operands
                    .iter()
                    .enumerate()
                    .any(|(i, op)| op.kind == "addr" && before.operands[i] as usize == *old_target)
                    && *old_target != before.pc + before.size
                {
                    continue;
                }
                let new_target = if *old_target == before.pc {
                    after.pc
                } else {
                    after.pc + after.size
                };
                let following = before.pc + before.size;
                let expected = if *old_target == following && state.after.contains_key(&id) {
                    bindings[&state.after[&id][0]]
                } else {
                    *bindings
                        .get(&original_id(*old_target))
                        .unwrap_or(old_target)
                };
                let mut actual = new_target;
                let mut seen = BTreeSet::new();
                while let Some(target) = scaffolds.get(&actual) {
                    if !seen.insert(actual) {
                        return Err(unsupported("compiler bridge cycle"));
                    }
                    actual = *target;
                }
                if actual != expected {
                    return Err(unsupported(format!(
                        "relocation changes a fixed-relative successor at +0x{:04x}",
                        before.pc
                    )));
                }
            }
        }
    }
    // Keep the assertion local to our own bridge bytes, never follow an original
    // game jump speculatively while comparing authored identities.
    for (pc, target) in scaffolds {
        if code[pc] != 1 || read_pc(&code, pc)? != target {
            return Err(error("compiler bridge was overwritten"));
        }
    }
    Ok(Compiled {
        bytes,
        declared_size,
        graph,
        bindings,
    })
}
