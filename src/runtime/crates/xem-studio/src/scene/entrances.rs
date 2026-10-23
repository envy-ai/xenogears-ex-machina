//! Referenced map-entry records, never an inferred count of ff-prefix data.
//!
//! field_event_place_at_map_entry reads seven bytes at 1+7*selector, signed X/Z,
//! layer, camera octant and facing. Angle 0xff defers to variables 8/6.
use crate::field::{self, FieldError, FieldGraph};
use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;

fn invalid(message: impl Into<String>) -> FieldError {
    FieldError::new("invalid_scene_entrance", message)
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Entrance {
    pub selector: u16,
    pub source_span: [u32; 2],
    pub x: i16,
    pub z: i16,
    pub layer: u8,
    pub camera_raw: u8,
    pub facing_raw: u8,
}
fn range(graph: &FieldGraph, selector: u16) -> Result<[usize; 2], FieldError> {
    let start = usize::from(selector)
        .checked_mul(7)
        .and_then(|n| n.checked_add(1))
        .ok_or_else(|| invalid("entrance selector range overflow"))?;
    let end = start
        .checked_add(7)
        .ok_or_else(|| invalid("entrance range overflow"))?;
    // Pinned prefix ownership is only a safety bound, not proof of a record
    // count. The caller supplies selectors established by a reference/observation.
    if !graph.blocks.iter().any(|b| {
        b.start <= start
            && b.end >= end
            && b.kind == "pinned"
            && b.reason.as_deref() == Some("map_entry_records")
    }) {
        return Err(invalid(
            "entrance selector is not wholly inside the pinned ff prefix",
        ));
    }
    if graph
        .instructions
        .iter()
        .flat_map(|i| &i.ranges)
        .any(|r| start < r[1] && end > r[0])
    {
        return Err(invalid(
            "entrance record overlaps executable instruction storage",
        ));
    }
    Ok([start, end])
}

/// Decode only explicitly identified selectors. This proves byte interpretation,
/// not completeness of incoming references (which can be variable/saved state).
pub fn decode_entrances(
    graph: &FieldGraph,
    selectors: &[u16],
) -> Result<Vec<Entrance>, FieldError> {
    let component = field::encode(graph)?;
    let code = component
        .get(graph.bytecode_offset..)
        .ok_or_else(|| invalid("event bytecode offset exceeds source"))?;
    if code.first() != Some(&0xff) {
        return Err(FieldError::new(
            "unsupported_scene_entrance",
            "event source has no ff map-entry marker",
        ));
    }
    if selectors.len() > code.len() / 7 {
        return Err(invalid(
            "entrance selector list exceeds bounded prefix capacity",
        ));
    }
    let mut seen = BTreeSet::new();
    let mut result = Vec::with_capacity(selectors.len());
    for selector in selectors {
        if !seen.insert(*selector) {
            return Err(invalid("duplicate entrance selector"));
        }
        let [start, end] = range(graph, *selector)?;
        let data = code
            .get(start..end)
            .ok_or_else(|| invalid("truncated entrance record"))?;
        result.push(Entrance {
            selector: *selector,
            source_span: [
                u32::try_from(start).map_err(|_| invalid("entrance span exceeds portable u32"))?,
                u32::try_from(end).map_err(|_| invalid("entrance span exceeds portable u32"))?,
            ],
            x: i16::from_le_bytes([data[0], data[1]]),
            z: i16::from_le_bytes([data[2], data[3]]),
            layer: data[4],
            camera_raw: data[5],
            facing_raw: data[6],
        });
    }
    Ok(result)
}

/// Patch records at stable selectors. Structural deletion/renumbering needs
/// reference closure and is deliberately not implemented by this primitive.
pub fn encode_entrances(
    graph: &FieldGraph,
    entrances: &[Entrance],
    layer_count: usize,
) -> Result<Vec<u8>, FieldError> {
    let selectors = entrances.iter().map(|e| e.selector).collect::<Vec<_>>();
    let base = decode_entrances(graph, &selectors)?;
    if !(1..=4).contains(&layer_count) {
        return Err(invalid(
            "entrance requires a decoded original collision layer count",
        ));
    }
    for (base, value) in base.iter().zip(entrances) {
        if base.source_span != value.source_span || usize::from(value.layer) >= layer_count {
            return Err(invalid(
                "entrance span differs or layer exceeds decoded collision layers",
            ));
        }
    }
    let mut bytes = field::encode(graph)?;
    for value in entrances {
        let at = graph.bytecode_offset + value.source_span[0] as usize;
        bytes[at..at + 2].copy_from_slice(&value.x.to_le_bytes());
        bytes[at + 2..at + 4].copy_from_slice(&value.z.to_le_bytes());
        bytes[at + 4] = value.layer;
        bytes[at + 5] = value.camera_raw;
        bytes[at + 6] = value.facing_raw;
    }
    let decoded = field::decode(&bytes, Some(graph.declared_size))?;
    if decoded.instructions != graph.instructions
        || decoded.actors != graph.actors
        || decoded.diagnostics != graph.diagnostics
    {
        return Err(invalid(
            "entrance patch changes executable decoding or actor/event entries",
        ));
    }
    Ok(bytes)
}
