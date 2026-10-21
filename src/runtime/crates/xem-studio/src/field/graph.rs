use super::{Block, Diagnostics, Edge, EventEntry, FieldError, FieldGraph, Instruction};
use std::collections::{BTreeMap, BTreeSet};

fn invalid(message: impl Into<String>) -> FieldError {
    FieldError::new("invalid_graph", message)
}

#[derive(Clone)]
struct Unit {
    start: usize,
    end: usize,
    kind: &'static str,
    reasons: BTreeSet<&'static str>,
}

impl Unit {
    fn new(start: usize, end: usize, kind: &'static str, reason: Option<&'static str>) -> Self {
        Self {
            start,
            end,
            kind,
            reasons: reason.into_iter().collect(),
        }
    }

    fn merge(&mut self, other: Self) {
        self.end = self.end.max(other.end);
        self.reasons.extend(other.reasons);
        self.kind = match (self.kind, other.kind) {
            ("pinned", _) | (_, "pinned") => "pinned",
            ("code", kind) | (kind, "code") => kind,
            (left, right) if left == right => left,
            _ => {
                self.reasons.insert("intersecting_compound_units");
                "pinned"
            }
        };
    }
}

fn block_for(blocks: &[Block], pc: usize) -> Option<usize> {
    let index = blocks.partition_point(|block| block.end <= pc);
    blocks
        .get(index)
        .filter(|block| block.start <= pc)
        .map(|_| index)
}

pub(super) fn build_blocks(
    code: &[u8],
    instructions: &BTreeMap<usize, Instruction>,
    entries: &[EventEntry],
    diagnostics: &Diagnostics,
) -> Result<(Vec<Block>, Vec<Edge>), FieldError> {
    let mut units = Vec::new();
    let mut physical = Vec::new();
    let mut boundaries = BTreeSet::from([0, code.len()]);
    for (&pc, instruction) in instructions {
        if instruction.pc != pc || pc >= code.len() || instruction.ranges.is_empty() {
            return Err(invalid(
                "instruction has an invalid storage PC or no consumed range",
            ));
        }
        let end = pc
            .checked_add(instruction.size)
            .ok_or_else(|| invalid("instruction span overflows"))?;
        let wrapped = instruction.ranges.len() != 1
            || end > code.len()
            || (instruction.extended.is_some() && pc == 0xffff);
        for &[start, end] in &instruction.ranges {
            if start >= end || end > code.len() {
                return Err(invalid("instruction consumed range is outside bytecode"));
            }
            let unit = if wrapped {
                Unit::new(start, end, "pinned", Some("wrapped_instruction"))
            } else {
                Unit::new(start, end, "code", None)
            };
            units.push(unit);
            physical.push((start, end));
        }
        if let Some(distance) = instruction.spec.setup
            && let Some(setup_pc) = pc.checked_sub(distance)
            && let Some(setup) = instructions.get(&setup_pc)
            && setup.opcode == instruction.opcode
            && setup.spec.setup.is_none()
            && end <= code.len()
        {
            units.push(Unit::new(setup_pc, end, "pair", Some("setup_step_pair")));
        }
        let linear = end & 0xffff;
        if instruction.spec.flow != "next"
            || instruction.successors.as_slice() != [linear]
            || !instruction.skips.is_empty()
            || !instruction.inner.is_empty()
        {
            if end <= code.len() {
                boundaries.insert(end);
            }
            boundaries.extend(instruction.successors.iter().copied());
            boundaries.extend(instruction.skips.iter().copied());
            boundaries.extend(instruction.inner.iter().copied());
        }
    }
    boundaries.extend(entries.iter().map(|entry| entry.pc));
    for &[pc, count] in &diagnostics.tables {
        let instruction = instructions
            .get(&pc)
            .ok_or_else(|| invalid("table diagnostic has no instruction"))?;
        let end = count
            .checked_mul(3)
            .and_then(|size| instruction.size.checked_add(size))
            .and_then(|size| pc.checked_add(size))
            .ok_or_else(|| invalid("inline jump table span overflows"))?;
        if end > code.len() {
            return Err(invalid("inline jump table exceeds bytecode"));
        }
        units.push(Unit::new(pc, end, "table", Some("inline_jump_table")));
    }
    // Physical intersections can include a reachable instruction beginning in
    // another instruction's operands. Keep every view, with one pinned owner.
    physical.sort_unstable();
    let mut previous: Option<(usize, usize)> = None;
    for (start, end) in physical {
        if let Some((before, before_end)) = previous
            && start < before_end
        {
            units.push(Unit::new(
                before,
                before_end.max(end),
                "pinned",
                Some("overlapping_instructions"),
            ));
            previous = Some((before, before_end.max(end)));
            continue;
        }
        previous = Some((start, end));
    }
    units.sort_by_key(|unit| (unit.start, std::cmp::Reverse(unit.end)));
    let mut atomic: Vec<Unit> = Vec::new();
    for unit in units {
        if let Some(last) = atomic.last_mut()
            && unit.start < last.end
        {
            last.merge(unit);
            continue;
        }
        atomic.push(unit);
    }
    let mut spans: Vec<Unit> = Vec::new();
    let mut position = 0;
    for unit in atomic {
        if unit.start > position {
            let reason = if position == 0 && code.first() == Some(&0xff) {
                "map_entry_records"
            } else {
                "unreachable_bytes"
            };
            spans.push(Unit::new(position, unit.start, "pinned", Some(reason)));
        }
        position = unit.end;
        if let Some(last) = spans.last_mut()
            && last.kind == "code"
            && unit.kind == "code"
            && last.end == unit.start
            && !boundaries.contains(&unit.start)
        {
            last.end = unit.end;
            continue;
        }
        spans.push(unit);
    }
    if position < code.len() {
        let reason = if position == 0 && code.first() == Some(&0xff) {
            "map_entry_records"
        } else {
            "unreachable_bytes"
        };
        spans.push(Unit::new(position, code.len(), "pinned", Some(reason)));
    }
    let blocks: Vec<Block> = spans
        .into_iter()
        .map(|unit| Block {
            start: unit.start,
            end: unit.end,
            kind: unit.kind.to_owned(),
            reason: if unit.reasons.is_empty() {
                None
            } else {
                Some(unit.reasons.into_iter().collect::<Vec<_>>().join(","))
            },
            bytes: code[unit.start..unit.end].to_vec(),
            instruction_pcs: instructions
                .range(unit.start..unit.end)
                .map(|(&pc, _)| pc)
                .collect(),
        })
        .collect();
    let into: BTreeSet<[usize; 2]> = diagnostics.into.iter().copied().collect();
    let mut edges = Vec::new();
    for (&pc, instruction) in instructions {
        let from_block =
            block_for(&blocks, pc).ok_or_else(|| invalid("instruction has no storage block"))?;
        let following = pc
            .checked_add(instruction.size)
            .ok_or_else(|| invalid("instruction continuation overflows"))?
            & 0xffff;
        let mut append = |target, kind: &str, not_followed| {
            let status = if target >= code.len() {
                "outside_stream"
            } else if not_followed || !instructions.contains_key(&target) {
                "not_followed"
            } else {
                "followed"
            };
            edges.push(Edge {
                from_block,
                to_block: block_for(&blocks, target),
                from_pc: pc,
                to_pc: target,
                kind: kind.to_owned(),
                status: status.to_owned(),
            });
        };
        let flow = instruction.spec.flow.as_str();
        if flow == "branch" || flow == "call" {
            // Walk successors are deduplicated, but distinct execution paths
            // remain distinct even when the branch/call target is following.
            if flow == "branch" {
                append(following, "fallthrough", false);
            }
            for (operand, &target) in instruction.spec.operands.iter().zip(&instruction.operands) {
                if operand.kind == "addr" {
                    append(target as usize, flow, false);
                }
            }
            if flow == "call" {
                append(following, "fallthrough", false);
            }
        } else {
            for &target in &instruction.successors {
                let kind = match flow {
                    "next" => "fallthrough",
                    "wait" if target != pc => "fallthrough",
                    "table" => "table_entry",
                    other => other,
                };
                append(target, kind, false);
            }
        }
        for &target in &instruction.skips {
            append(target, "skip", into.contains(&[pc, target]));
        }
        for &target in &instruction.inner {
            append(target, "inner", true);
        }
        if let Some(&[_, count]) = diagnostics.tables.iter().find(|record| record[0] == pc) {
            let first = pc
                .checked_add(instruction.size)
                .ok_or_else(|| invalid("table entry PC overflows"))?;
            for index in 0..count {
                let target = index
                    .checked_mul(3)
                    .and_then(|offset| first.checked_add(offset))
                    .ok_or_else(|| invalid("table entry PC overflows"))?;
                if !instruction.successors.contains(&target) {
                    append(target, "table_entry", false);
                }
            }
        }
    }
    Ok((blocks, edges))
}

/// Assemble the original, unedited graph. Canonical decoding checks all derived
/// fields after the typed header and storage blocks establish the actual bytes.
pub fn encode(graph: &FieldGraph) -> Result<Vec<u8>, FieldError> {
    if graph.schema_version != 1 {
        return Err(invalid("unsupported field graph schema"));
    }
    if graph.variable_unsigned_bits.len() != 128 {
        return Err(invalid("variable type map must contain exactly 128 bytes"));
    }
    let actor_count = u32::try_from(graph.actors.len())
        .map_err(|_| invalid("actor count exceeds the header field"))?;
    let offset = graph
        .actors
        .len()
        .checked_mul(64)
        .and_then(|size| size.checked_add(0x84))
        .ok_or_else(|| invalid("actor table size overflows"))?;
    if graph.bytecode_offset != offset
        || graph.stream_size < offset
        || graph.declared_size < offset
        || graph.declared_size > graph.stream_size
    {
        return Err(invalid("inconsistent component header or declared size"));
    }
    let code_size = graph.stream_size - offset;
    if code_size > 0x10000 || (code_size == 0 && !graph.actors.is_empty()) {
        return Err(invalid(
            "bytecode size is outside the original PC address space",
        ));
    }
    if graph.stream_size - graph.declared_size > super::opcodes::metadata()?.allocation_slack {
        return Err(invalid(
            "decoded tail exceeds the original allocation slack",
        ));
    }
    let mut bytes = Vec::new();
    bytes
        .try_reserve_exact(graph.stream_size)
        .map_err(|error| invalid(format!("cannot allocate component: {error}")))?;
    bytes.extend_from_slice(&graph.variable_unsigned_bits);
    bytes.extend_from_slice(&actor_count.to_le_bytes());
    for actor in &graph.actors {
        for entry in actor {
            bytes.extend_from_slice(&entry.to_le_bytes());
        }
    }
    let mut position = 0;
    for block in &graph.blocks {
        if block.start != position
            || block.end <= block.start
            || block.end > code_size
            || block.bytes.len() != block.end - block.start
        {
            return Err(invalid(
                "storage blocks must partition bytecode in order without gaps or overlaps",
            ));
        }
        if !["code", "pair", "table", "pinned"].contains(&block.kind.as_str()) {
            return Err(invalid("unknown storage block kind"));
        }
        bytes.extend_from_slice(&block.bytes);
        position = block.end;
    }
    if position != code_size || bytes.len() != graph.stream_size {
        return Err(invalid("storage blocks do not cover the complete stream"));
    }
    let mut previous_pc = None;
    for instruction in &graph.instructions {
        if instruction.pc >= code_size
            || previous_pc.is_some_and(|pc| instruction.pc <= pc)
            || instruction.ranges.is_empty()
        {
            return Err(invalid(
                "instructions must have unique ordered PCs inside bytecode",
            ));
        }
        previous_pc = Some(instruction.pc);
        let mut consumed = 0usize;
        let mut contains_pc = false;
        for (index, &[start, end]) in instruction.ranges.iter().enumerate() {
            if start >= end || end > code_size {
                return Err(invalid("instruction range is outside bytecode"));
            }
            if instruction.ranges[..index]
                .iter()
                .any(|&[left, right]| start < right && left < end)
            {
                return Err(invalid("one instruction consumes the same byte twice"));
            }
            contains_pc |= start <= instruction.pc && instruction.pc < end;
            consumed = consumed
                .checked_add(end - start)
                .ok_or_else(|| invalid("instruction consumed size overflows"))?;
        }
        if !contains_pc || consumed != instruction.size {
            return Err(invalid(
                "instruction ranges do not agree with its PC and size",
            ));
        }
    }
    for block in &graph.blocks {
        let first = graph
            .instructions
            .partition_point(|instruction| instruction.pc < block.start);
        let end = graph
            .instructions
            .partition_point(|instruction| instruction.pc < block.end);
        if !block
            .instruction_pcs
            .iter()
            .copied()
            .eq(graph.instructions[first..end]
                .iter()
                .map(|instruction| instruction.pc))
        {
            return Err(invalid(
                "block instruction PCs disagree with instruction storage",
            ));
        }
    }
    for entry in &graph.entries {
        if entry.actor >= graph.actors.len()
            || entry.event >= 32
            || entry.pc >= code_size
            || usize::from(graph.actors[entry.actor][entry.event]) != entry.pc
        {
            return Err(invalid("event entry disagrees with the actor table"));
        }
    }
    for edge in &graph.edges {
        if block_for(&graph.blocks, edge.from_pc) != Some(edge.from_block)
            || block_for(&graph.blocks, edge.to_pc) != edge.to_block
            || graph
                .instructions
                .binary_search_by_key(&edge.from_pc, |instruction| instruction.pc)
                .is_err()
            || !["followed", "not_followed", "outside_stream"].contains(&edge.status.as_str())
        {
            return Err(invalid("edge disagrees with instruction or storage spans"));
        }
    }
    if super::source_hash(&bytes) != graph.source_sha256 {
        return Err(invalid(
            "reconstructed stream differs from the source SHA-256",
        ));
    }
    let canonical = super::decode(&bytes, Some(graph.declared_size)).map_err(|error| {
        invalid(format!(
            "reconstructed component cannot be decoded: {}",
            error.message
        ))
    })?;
    if canonical != *graph {
        return Err(invalid(
            "graph differs from the canonical unedited component",
        ));
    }
    Ok(bytes)
}
