//! Actor/event table preparation with stable row ownership and unchanged code.
//! This primitive does not claim script-reference closure or independent script
//! copies: a template binding explicitly shares the source row's entry PCs.
use crate::field::{self, FieldError, FieldGraph};
use serde::Serialize;
use std::collections::{BTreeMap, BTreeSet};
use xem_disc::field_context::MAX_COMPONENT;

fn invalid(message: impl Into<String>) -> FieldError {
    FieldError::new("invalid_scene_actor", message)
}
#[derive(Clone, Debug, Serialize)]
pub struct Row {
    pub id: String,
    /// The actual source actor row, not a guessed slot/entity limit.
    pub source_actor: u32,
}
#[derive(Clone, Debug, Serialize)]
pub struct Prepared {
    pub bytes: Vec<u8>,
    pub declared_size: usize,
    pub graph: FieldGraph,
    pub actor_indices: BTreeMap<String, u32>,
}
/// Rebind existing entry rows while retaining the complete original bytecode,
/// including gaps, unreachable bytes and measured tails. Reference repair and
/// matching descriptor order are mandatory caller responsibilities. Actor zero
/// stays source-qualified because the engine invokes its events 2/3 directly.
pub fn rebuild(source: &FieldGraph, rows: &[Row]) -> Result<Prepared, FieldError> {
    let bytes = field::encode(source)?;
    if !source.actors.is_empty()
        && rows
            .first()
            .is_none_or(|r| r.id != "descriptor:0" || r.source_actor != 0)
    {
        return Err(FieldError::new(
            "unsupported_scene_actor",
            "original actor zero must retain its identity and index; the engine invokes its events directly",
        ));
    }
    let header = rows
        .len()
        .checked_mul(64)
        .and_then(|n| n.checked_add(132))
        .ok_or_else(|| invalid("actor event table size overflow"))?;
    let code = bytes
        .get(source.bytecode_offset..)
        .ok_or_else(|| invalid("source bytecode offset exceeds component"))?;
    let full_size = header
        .checked_add(code.len())
        .ok_or_else(|| invalid("actor component size overflow"))?;
    let logical_code = source
        .declared_size
        .checked_sub(source.bytecode_offset)
        .ok_or_else(|| invalid("source declared boundary precedes bytecode"))?;
    let declared_size = header
        .checked_add(logical_code)
        .ok_or_else(|| invalid("actor logical size overflow"))?;
    if declared_size > MAX_COMPONENT - 16 || full_size > MAX_COMPONENT {
        return Err(invalid(
            "actor table growth exceeds original logical+16 component allocation",
        ));
    }
    let mut ids = BTreeSet::new();
    let mut indices = BTreeMap::new();
    let mut selected = Vec::with_capacity(rows.len());
    for (index, row) in rows.iter().enumerate() {
        if row.id.is_empty() || row.id.len() > 128 || !ids.insert(&row.id) {
            return Err(invalid(
                "actor stable identity is empty, duplicated or oversized",
            ));
        }
        let values = source
            .actors
            .get(row.source_actor as usize)
            .ok_or_else(|| invalid("actor template is outside source event rows"))?;
        selected.push(*values);
        indices.insert(
            row.id.clone(),
            u32::try_from(index).map_err(|_| invalid("actor index exceeds original u32 count"))?,
        );
    }
    let count = u32::try_from(selected.len())
        .map_err(|_| invalid("actor count exceeds original u32 field"))?;
    let mut result = bytes[..128].to_vec();
    result.extend_from_slice(&count.to_le_bytes());
    for row in selected {
        for pc in row {
            result.extend_from_slice(&pc.to_le_bytes());
        }
    }
    result.extend_from_slice(code);
    let graph = field::decode(&result, Some(declared_size))?;
    // Rebinding rows can remove an exclusively owned instruction from reachability
    // but cannot introduce or reinterpret an instruction. Storage itself is exact.
    for ins in &graph.instructions {
        if source
            .instructions
            .binary_search_by_key(&ins.pc, |i| i.pc)
            .ok()
            .and_then(|index| source.instructions.get(index))
            != Some(ins)
        {
            return Err(invalid(
                "actor table preparation activates or changes bytecode decoding",
            ));
        }
    }
    Ok(Prepared {
        bytes: result,
        declared_size,
        graph,
        actor_indices: indices,
    })
}
