//! Prepare an independent, qualified event copy using M6 NodeInput/targets.
//! This does not authorize actor creation or claim absence of global effects.
//! Literal references retain their original meaning; they are not guessed self.
use crate::field::{
    self, FieldError, FieldGraph,
    draft::{self, NodeInput},
    structure::StructureIndex,
};
use serde::Serialize;
use std::collections::BTreeMap;

fn unsupported(message: impl Into<String>) -> FieldError {
    FieldError::new("unsupported_scene_script_clone", message)
}
#[derive(Clone, Debug, Serialize)]
pub struct Prepared {
    pub nodes: Vec<NodeInput>,
    pub source_nodes: BTreeMap<usize, String>,
    pub next_id: u32,
}
/// Allocation starts must be the M6 document's current monotonic watermark.
/// The subsequent M6 NewEvent transaction validates ownership, field widths,
/// target existence, complete flow and the final u16-PC allocation.
pub fn prepare(
    graph: &FieldGraph,
    actor: u32,
    event: u8,
    allocation_start: u32,
) -> Result<Option<Prepared>, FieldError> {
    let source = field::encode(graph)?;
    if allocation_start == 0 {
        return Err(unsupported("event allocation watermark must be nonzero"));
    }
    let index = StructureIndex::new(graph)?;
    let mut pcs = index.reachable_pcs(actor as usize, usize::from(event))?;
    if pcs.is_empty() {
        return Ok(None);
    } // Canonical omitted ff-prefix entry.
    pcs.sort_unstable();
    let entry = usize::from(graph.actors[actor as usize][usize::from(event)]);
    // A flat M6 event starts at its first node. Do not reorder arbitrary CFGs to
    // pretend the smallest PC is their entry; qualify that form separately.
    if pcs.first() != Some(&entry) {
        return Err(unsupported(
            "event entry is not its first source storage unit; an explicit block-layout proof is required",
        ));
    }
    if pcs.len() > draft::MAX_EVENT_NODES {
        return Err(unsupported(
            "event copy exceeds the existing M6 per-event node budget",
        ));
    }
    let count =
        u32::try_from(pcs.len()).map_err(|_| unsupported("event identity count exceeds u32"))?;
    let next_id = allocation_start
        .checked_add(count)
        .ok_or_else(|| unsupported("event identity allocator exhausted"))?;
    let source_nodes: BTreeMap<_, _> = pcs
        .iter()
        .enumerate()
        .map(|(n, pc)| (*pc, format!("new:{}", allocation_start + n as u32)))
        .collect();
    let mut nodes = Vec::with_capacity(pcs.len());
    for pc in pcs {
        let ins = graph
            .instructions
            .binary_search_by_key(&pc, |i| i.pc)
            .ok()
            .and_then(|n| graph.instructions.get(n))
            .ok_or_else(|| unsupported("reachable source instruction is missing"))?;
        let end = pc
            .checked_add(ins.size)
            .ok_or_else(|| unsupported("source instruction span overflow"))?;
        if ins.ranges.as_slice() != [[pc, end]]
            || ins.size == 0
            || !ins.inner.is_empty()
            || !ins.spec.also.is_empty()
            || ins.spec.setup.is_some()
            || !["next", "wait", "jump", "branch", "call", "return", "end"]
                .contains(&ins.spec.flow.as_str())
            || (end > 0xffff && !["end", "return", "jump"].contains(&ins.spec.flow.as_str()))
            || graph
                .blocks
                .iter()
                .any(|b| b.start <= pc && pc < b.end && b.kind != "code")
        {
            return Err(unsupported(format!(
                "PC +0x{pc:04x}: paired, inline, wrapped, pinned or fixed-relative source storage cannot be independently copied"
            )));
        }
        // Appending an event changes what lies after its final byte. An old
        // outside-stream fallthrough must not acquire a new authored successor.
        if ["next", "wait", "branch", "call"].contains(&ins.spec.flow.as_str())
            && !source_nodes.contains_key(&end)
        {
            return Err(unsupported(format!(
                "PC +0x{pc:04x}: implicit continuation is outside the qualified event copy"
            )));
        }
        if [
            "field_event_become_party_slot",
            "field_event_become_party_character",
        ]
        .contains(&ins.spec.handler.as_str())
        {
            return Err(unsupported(
                "party ownership initialization is not an independently duplicable actor template",
            ));
        }
        let mut targets = BTreeMap::new();
        for (operand, spec) in ins.spec.operands.iter().enumerate() {
            if spec.kind == "addr" {
                let target = ins.operands[operand] as usize;
                let id = source_nodes.get(&target).ok_or_else(|| {
                    unsupported(format!(
                        "PC +0x{pc:04x}: address target is outside the qualified event copy"
                    ))
                })?;
                targets.insert(operand, id.clone());
            }
        }
        let bytes = super::span(&source, graph.bytecode_offset + pc, ins.size)
            .ok_or_else(|| unsupported("source instruction bytes are truncated"))?
            .to_vec();
        nodes.push(NodeInput { bytes, targets });
    }
    Ok(Some(Prepared {
        nodes,
        source_nodes,
        next_id,
    }))
}
