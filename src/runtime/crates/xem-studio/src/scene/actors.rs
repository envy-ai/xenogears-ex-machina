//! Stable active actor identities over an explicit private event-row derivation.
//! Generic creation is qualified against actual byte/sentinel consumers, not an
//! invented actor-count maximum. Wider original tables remain inspectable.
use super::actor_rows;
use crate::field::{FieldError, FieldGraph, structure::StructureIndex};
use serde::Serialize;
use std::collections::BTreeSet;

#[derive(Clone, Debug, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Origin {
    Original { actor: u32 },
    IndependentCopy { template: String },
}
#[derive(Clone, Debug, Serialize)]
pub struct Actor {
    pub id: String,
    /// Private M6 source-table row, not the final runtime index or a public edit
    /// argument. Original rows remain fixed; authored rows have typed origins.
    pub event_source_row: u32,
    pub origin: Origin,
}
pub(crate) fn initial(count: usize) -> Vec<Actor> {
    (0..count)
        .map(|i| Actor {
            id: format!("descriptor:{i}"),
            event_source_row: i as u32,
            origin: Origin::Original { actor: i as u32 },
        })
        .collect()
}
pub(crate) fn rows(actors: &[Actor]) -> Vec<actor_rows::Row> {
    actors
        .iter()
        .map(|a| actor_rows::Row {
            id: a.id.clone(),
            source_actor: a.event_source_row,
        })
        .collect()
}
pub fn runtime_index(index: usize) -> Result<(), FieldError> {
    // FieldActor.ridden_actor / linked actor are bytes; 0xff is absent.
    // Also field_event_read_actor_index(0xfb) resolves current actor, whose
    // resolved0xff consumers treat as absent. Source: actors.h:70,
    // field_motion.c:3598,3654, field_event.c:3092,8681-8695,9046,9434.
    if index >= usize::from(u8::MAX) {
        return Err(FieldError::new(
            "unsupported_scene_actor",
            "generic actor creation requires runtime indices below255 because byte actor links and resolved0xff self selectors use an absent sentinel; this is not a universal original actor-count limit",
        ));
    }
    Ok(())
}
pub(crate) fn qualify_template(graph: &FieldGraph, actor: usize) -> Result<(), FieldError> {
    let index = StructureIndex::new(graph)?;
    let row = graph
        .actors
        .get(actor)
        .ok_or_else(|| FieldError::new("invalid_scene_actor", "actor template is not active"))?;
    let mut roots = BTreeSet::new();
    for (event, pc) in row.iter().enumerate() {
        if !roots.insert(*pc) {
            continue;
        }
        for pc in index.reachable_pcs(actor, event)? {
            let instruction = &graph.instructions[graph
                .instructions
                .binary_search_by_key(&pc, |i| i.pc)
                .map_err(|_| {
                    FieldError::new("invalid_scene_actor", "template instruction is missing")
                })?];
            if [
                "field_event_become_party_slot",
                "field_event_become_party_character",
            ]
            .contains(&instruction.spec.handler.as_str())
            {
                return Err(FieldError::new(
                    "unsupported_scene_actor",
                    "party ownership initialization cannot be duplicated or removed by generic actor CRUD",
                ));
            }
        }
    }
    Ok(())
}
