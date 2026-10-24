//! Private source-table derivation for independent authored actors. Original
//! code, gaps and measured tails stay exact; compiled edits are never adopted
//! as source. The scene document retains the actual original input/history.
use crate::field::{self, FieldError, FieldGraph};
use xem_disc::field_context::MAX_COMPONENT;

fn invalid(message: impl Into<String>) -> FieldError {
    FieldError::new("unsupported_scene_actor", message)
}
/// Append one empty virtual row using an existing, unpinned original end-slot
/// instruction. This is an intermediate compiler namespace, not a runtime row
/// insertion or an assertion that arbitrary zero/ff data is an empty event.
pub fn extend_empty(source: &FieldGraph) -> Result<FieldGraph, FieldError> {
    let original = field::encode(source)?;
    let terminal = source.instructions.iter().find(|i|
        i.spec.handler == "field_event_end_slot" && i.spec.flow == "end"
        && i.spec.operands.is_empty() && i.size==1 && i.ranges==[[i.pc,i.pc+1]]
        && source.blocks.iter().any(|b|b.kind=="code" && b.start<=i.pc && i.pc<b.end)
    ).ok_or_else(||invalid("actor creation needs a proven original unpinned end-slot instruction for its virtual empty rows"))?;
    let pc = u16::try_from(terminal.pc)
        .map_err(|_| invalid("empty actor entry exceeds u16 PC space"))?;
    let count = source
        .actors
        .len()
        .checked_add(1)
        .ok_or_else(|| invalid("virtual actor count overflow"))?;
    let full = original
        .len()
        .checked_add(64)
        .ok_or_else(|| invalid("virtual actor component size overflow"))?;
    let declared = source
        .declared_size
        .checked_add(64)
        .ok_or_else(|| invalid("virtual actor logical size overflow"))?;
    if declared > MAX_COMPONENT - 16 || full > MAX_COMPONENT {
        return Err(invalid(
            "virtual actor table exceeds the existing component allocation budget",
        ));
    }
    let count = u32::try_from(count).map_err(|_| invalid("virtual actor count exceeds u32"))?;
    let mut bytes = Vec::with_capacity(full);
    bytes.extend_from_slice(&original[..128]);
    bytes.extend_from_slice(&count.to_le_bytes());
    bytes.extend_from_slice(&original[132..source.bytecode_offset]);
    for _ in 0..32 {
        bytes.extend_from_slice(&pc.to_le_bytes());
    }
    bytes.extend_from_slice(&original[source.bytecode_offset..]);
    let derived = field::decode(&bytes, Some(declared))?;
    if derived.instructions != source.instructions
        || !field::execution_edges_match(&derived, source)
        || derived.diagnostics != source.diagnostics
    {
        return Err(invalid(
            "virtual empty row would activate or change original instruction decoding",
        ));
    }
    Ok(derived)
}
