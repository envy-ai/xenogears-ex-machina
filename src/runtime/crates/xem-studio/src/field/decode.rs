//! Table-driven counterpart of tools.analysis.events' component parser and walk.
use super::{
    Diagnostics, EventEntry, FieldError, FieldGraph, Instruction, OpcodeForm, graph, opcodes,
    source_hash,
};
use std::collections::BTreeMap;

fn region<'a>(
    bytes: &'a [u8],
    start: usize,
    size: usize,
    label: &str,
) -> Result<&'a [u8], FieldError> {
    bytes
        .get(
            start..start.checked_add(size).ok_or_else(|| {
                FieldError::new("invalid_range", format!("{label}: range overflow"))
            })?,
        )
        .ok_or_else(|| {
            FieldError::new(
                "truncated_input",
                format!(
                    "{label}: range +0x{start:x}, {size} bytes exceeds {}",
                    bytes.len()
                ),
            )
        })
}

fn pick<'a>(
    forms: &'a [OpcodeForm],
    code: &[u8],
    base: usize,
) -> Result<&'a OpcodeForm, FieldError> {
    for spec in forms {
        if let Some((offset, mask, Some(value))) = spec.select
            && u32::from(region(code, base + offset, 1, "form byte")?[0]) & mask != value
        {
            continue;
        }
        return Ok(spec);
    }
    Err(FieldError::new(
        "unsupported_instruction",
        "no opcode form matches",
    ))
}

pub(super) fn instruction(code: &[u8], pc: usize) -> Result<Instruction, FieldError> {
    if pc > 0xffff || code.len() > 0x10000 {
        return Err(FieldError::new(
            "invalid_pc",
            "PC or bytecode exceeds original u16 address space",
        ));
    }
    let metadata = opcodes::metadata()?;
    let opcode = region(code, pc, 1, "opcode")?[0];
    let (extended, base, prefix, forms) = if opcode == metadata.prefix {
        let base = (pc + 1) & 0xffff;
        let extended = region(code, base, 1, "extended opcode")?[0];
        (Some(extended), base, 1, metadata.extended.get(&extended))
    } else {
        (None, pc, 0, metadata.primary.get(&opcode))
    };
    let forms = forms.ok_or_else(|| {
        FieldError::new(
            "unsupported_instruction",
            format!(
                "unknown {}opcode 0x{:02x} at +0x{base:04x}",
                if extended.is_some() { "extended " } else { "" },
                extended.unwrap_or(opcode)
            ),
        )
    })?;
    let spec = pick(forms, code, base)?;
    region(code, base, spec.size.max(1), "instruction operands")?;
    let mut operands = Vec::new();
    let mut immediate = Vec::new();
    for operand in &spec.operands {
        let width = metadata
            .widths
            .get(&operand.kind)
            .ok_or_else(|| FieldError::new("invalid_metadata", "unknown operand width"))?;
        let bytes = region(code, base + operand.offset, *width, "operand")?;
        let value = bytes
            .iter()
            .enumerate()
            .fold(0_u32, |value, (index, byte)| {
                value | (u32::from(*byte) << (index * 8))
            });
        operands.push(value);
        immediate.push(match operand.kind.as_str() {
            "sel" => {
                let flags = operand.flags.ok_or_else(|| {
                    FieldError::new("invalid_metadata", "selector has no flags offset")
                })?;
                let bit = operand.bit.ok_or_else(|| {
                    FieldError::new("invalid_metadata", "selector has no flag bit")
                })?;
                Some(u32::from(region(code, base + flags, 1, "selector flags")?[0]) & bit != 0)
            }
            "iv" => Some(value & 0x8000 != 0),
            _ => None,
        });
    }
    let size = prefix + spec.size;
    let following = (pc + size) & 0xffff;
    let targets: Vec<_> = spec
        .operands
        .iter()
        .zip(&operands)
        .filter(|(operand, _)| operand.kind == "addr")
        .map(|(_, value)| *value as usize)
        .collect();
    let successors = match spec.flow.as_str() {
        "next" => vec![following],
        "wait" => vec![pc, following],
        "end" | "return" => vec![],
        "hang" => vec![pc],
        "rerun" => vec![base],
        "jump" => targets,
        "branch" => [vec![following], targets].concat(),
        "call" => [targets, vec![following]].concat(),
        "table" => {
            let index = *operands
                .first()
                .ok_or_else(|| FieldError::new("invalid_metadata", "table has no index"))?
                as usize;
            vec![if index & 0x8000 != 0 {
                following + 3 * (index & 0x7fff)
            } else {
                following
            }]
        }
        _ => return Err(FieldError::new("invalid_metadata", "unknown flow")),
    };
    let mut unique = Vec::new();
    for target in successors {
        let target = target & 0xffff;
        if !unique.contains(&target) {
            unique.push(target);
        }
    }
    let mut skips = Vec::new();
    for (advance, condition) in &spec.also {
        let allowed = match condition.as_deref() {
            None => true,
            Some("zero") => {
                let index = spec
                    .operands
                    .iter()
                    .position(|operand| operand.kind == "iv")
                    .ok_or_else(|| {
                        FieldError::new("invalid_metadata", "zero skip has no iv operand")
                    })?;
                immediate[index] == Some(false) || operands[index] == 0x8000
            }
            Some("party") => {
                let index = spec
                    .operands
                    .iter()
                    .position(|operand| operand.kind == "actor")
                    .ok_or_else(|| {
                        FieldError::new("invalid_metadata", "party skip has no actor operand")
                    })?;
                [0xfd, 0xfe, 0xff].contains(&operands[index])
            }
            Some(_) => {
                return Err(FieldError::new(
                    "invalid_metadata",
                    "unknown skip condition",
                ));
            }
        };
        if allowed {
            skips.push((base + advance) & 0xffff);
        }
    }
    let inner = spec
        .inner
        .iter()
        .map(|advance| (base + advance) & 0xffff)
        .collect();
    let ranges = if prefix == 1 && base == 0 {
        let mut ranges = vec![[pc, pc + 1]];
        if spec.size != 0 {
            ranges.push([0, spec.size]);
        }
        ranges
    } else {
        vec![[pc, pc + size]]
    };
    Ok(Instruction {
        pc,
        opcode,
        extended,
        size,
        ranges,
        spec: spec.clone(),
        operands,
        immediate,
        successors: unique,
        skips,
        inner,
    })
}

fn outside(ins: &Instruction, len: usize) -> Vec<usize> {
    let mut targets = Vec::new();
    for target in ins.successors.iter().chain(&ins.skips) {
        if *target >= len && !targets.contains(target) {
            targets.push(*target);
        }
    }
    targets
}

/// Strict single-instruction dispatch. Like Python, rejects outside successors.
/// Component decoding retains those instructions with explicit diagnostics.
pub fn decode_instruction(code: &[u8], pc: usize) -> Result<Instruction, FieldError> {
    let ins = instruction(code, pc)?;
    if let Some(target) = outside(&ins, code.len()).first() {
        return Err(FieldError::new(
            "successor_outside",
            format!("opcode at +0x{pc:04x} targets +0x{target:04x} outside bytecode"),
        ));
    }
    Ok(ins)
}

/// Decode an entire already-decompressed event component without editing it.
/// `declared_size` includes the header and excludes the loader's retained tail.
pub fn decode(bytes: &[u8], declared_size: Option<usize>) -> Result<FieldGraph, FieldError> {
    let metadata = opcodes::metadata()?;
    let bits = region(bytes, 0, 128, "variable type map")?.to_vec();
    let raw_count = region(bytes, 128, 4, "actor count")?;
    let count =
        u32::from_le_bytes([raw_count[0], raw_count[1], raw_count[2], raw_count[3]]) as usize;
    let table_size = count
        .checked_mul(64)
        .ok_or_else(|| FieldError::new("invalid_header", "actor table size overflow"))?;
    let offset = 132_usize
        .checked_add(table_size)
        .ok_or_else(|| FieldError::new("invalid_header", "bytecode offset overflow"))?;
    let table = region(bytes, 132, table_size, "actor event table")?;
    let code = bytes
        .get(offset..)
        .ok_or_else(|| FieldError::new("truncated_input", "missing event bytecode"))?;
    if code.len() > 65536 {
        return Err(FieldError::new(
            "invalid_header",
            "bytecode exceeds original u16 PC address space",
        ));
    }
    let declared_size = declared_size.unwrap_or(bytes.len());
    if declared_size < offset
        || declared_size > bytes.len()
        || bytes.len() - declared_size > metadata.allocation_slack
    {
        return Err(FieldError::new(
            "invalid_declared_size",
            "declared size must include the header and lie within the stream, with at most 16 tail bytes",
        ));
    }
    let mut actors = Vec::with_capacity(count);
    let mut entries = Vec::new();
    let mut skipped_zero_entries = 0;
    for (actor, raw_row) in table.chunks_exact(64).enumerate() {
        let mut row = [0_u16; 32];
        for (event, raw) in raw_row.chunks_exact(2).enumerate() {
            let pc = u16::from_le_bytes([raw[0], raw[1]]);
            if usize::from(pc) >= code.len() {
                return Err(FieldError::new(
                    "entry_outside",
                    format!("actor {actor} event {event}: PC +0x{pc:04x} outside bytecode"),
                ));
            }
            row[event] = pc;
            if pc == 0 && code.first() == Some(&0xff) {
                skipped_zero_entries += 1;
            } else {
                entries.push(EventEntry {
                    actor,
                    event,
                    pc: usize::from(pc),
                });
            }
        }
        actors.push(row);
    }
    let mut instructions = BTreeMap::new();
    let mut pending: Vec<_> = entries.iter().map(|entry| entry.pc).collect();
    let mut visited = Vec::new();
    let mut diagnostics = Diagnostics::default();
    while let Some(pc) = pending.pop() {
        if instructions.contains_key(&pc) {
            continue;
        }
        let ins = instruction(code, pc).map_err(|error| {
            FieldError::new(error.kind, format!("at PC +0x{pc:04x}: {}", error.message))
        })?;
        diagnostics.outside.extend(
            outside(&ins, code.len())
                .into_iter()
                .map(|target| [pc, target]),
        );
        pending.extend(
            ins.successors
                .iter()
                .copied()
                .filter(|target| *target < code.len()),
        );
        diagnostics
            .inner
            .extend(ins.inner.iter().map(|target| [pc, *target]));
        for target in ins
            .skips
            .iter()
            .copied()
            .filter(|target| *target < code.len())
        {
            // Python uses strict look-ahead: an outside successor there prevents
            // treating this alternative advance as a whole-instruction skip.
            let next = pc + ins.size;
            let lands =
                decode_instruction(code, next).is_ok_and(|next_ins| target == next + next_ins.size);
            if lands {
                diagnostics.followed_skips += 1;
                pending.push(target);
            } else {
                diagnostics.into.push([pc, target]);
            }
        }
        if ins.spec.flow == "table" && ins.operands[0] & 0x8000 == 0 {
            let mut slot = pc + ins.size;
            let mut slots = 0;
            while slot + 3 <= code.len() && code[slot] == metadata.jump {
                pending.push(slot);
                slot += 3;
                slots += 1;
            }
            diagnostics.tables.push([pc, slots]);
        }
        if pc + ins.size > declared_size - offset {
            diagnostics.beyond_declared.push(pc);
        }
        visited.push(pc);
        instructions.insert(pc, ins);
    }
    let ordered: Vec<_> = instructions.keys().copied().collect();
    for pair in ordered.windows(2) {
        if pair[1] < pair[0] + instructions[&pair[0]].size {
            diagnostics.overlaps.push([pair[1], pair[0]]);
        }
    }
    for pc in visited {
        let ins = &instructions[&pc];
        if let Some(distance) = ins.spec.setup {
            let setup = pc
                .checked_sub(distance)
                .and_then(|pc| instructions.get(&pc));
            if !setup.is_some_and(|setup| setup.opcode == ins.opcode && setup.spec.setup.is_none())
            {
                diagnostics.unpaired.push(pc);
            }
        }
    }
    let (blocks, edges) = graph::build_blocks(code, &instructions, &entries, &diagnostics)?;
    Ok(FieldGraph {
        schema_version: 1,
        source_sha256: source_hash(bytes),
        declared_size,
        stream_size: bytes.len(),
        bytecode_offset: offset,
        variable_unsigned_bits: bits,
        actors,
        entries,
        skipped_zero_entries,
        instructions: instructions.into_values().collect(),
        blocks,
        edges,
        diagnostics,
    })
}
