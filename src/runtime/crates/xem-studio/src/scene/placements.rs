//! Explicit source instruction bindings, not inferred final/live actor poses.
//! Source: field_event.c::field_event_place (9301-9308), place_on_layer
//! (9277-9285), place_at_height (9251-9258), set_position (7040-7057), and
//! selected operand readers (8727-8767). Canonical metadata owns opcodes,
//! operand forms, flags and widths; this adapter owns only scene meaning.
use crate::field::{FieldError, draft, structure::StructureIndex};
use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;
use xem_disc::field_context::hash;

fn unsupported(message: impl Into<String>) -> FieldError {
    FieldError::new("unsupported_scene_placement", message)
}
#[derive(Clone, Debug, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Coordinate {
    Immediate {
        value: i16,
    },
    Variable {
        id: u16,
    },
    /// The floor height is evaluated by the game, never synthesized here.
    Floor {
        explicit_layer: Option<u8>,
    },
}
#[derive(Clone, Debug, Serialize)]
pub struct Owner {
    pub actor: String,
    pub event: u8,
}
#[derive(Clone, Debug, Serialize)]
pub struct Binding {
    pub node: String,
    pub pc: u32,
    pub handler: String,
    /// Source/world axis order x,y,z; handlers store their operands x,z,y.
    pub position: [Coordinate; 3],
    pub owners: Vec<Owner>,
    pub can_transform: bool,
    pub diagnostics: Vec<String>,
}
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Change {
    pub node: String,
    pub actor: String,
    pub event: u8,
    /// None leaves that axis untouched. A floor-derived Y cannot be entered.
    pub position: [Option<i16>; 3],
    pub layer: Option<u8>,
}
struct Form {
    coordinates: [Option<usize>; 3],
    layer: Option<usize>,
}
fn form(handler: &str) -> Result<Form, FieldError> {
    Ok(match handler {
        "field_event_place" => Form {
            coordinates: [Some(0), None, Some(1)],
            layer: None,
        },
        "field_event_place_on_layer" => Form {
            coordinates: [Some(0), None, Some(1)],
            layer: Some(2),
        },
        "field_event_place_at_height" | "field_event_set_position" => Form {
            coordinates: [Some(0), Some(2), Some(1)],
            layer: None,
        },
        _ => {
            return Err(unsupported(
                "selected instruction has no qualified actor placement binding",
            ));
        }
    })
}
pub fn describe(
    compiled: &draft::Compiled,
    authored: &draft::Draft,
    node: &str,
    actors: &[String],
    layer_count: usize,
) -> Result<Binding, FieldError> {
    let index = StructureIndex::new(&compiled.graph)?;
    if compiled.declared_size != compiled.graph.declared_size
        || compiled.bytes.len() != compiled.graph.stream_size
        || hash(&compiled.bytes) != compiled.graph.source_sha256
    {
        return Err(unsupported(
            "compiled placement bytes/declared boundary differ from their canonical graph",
        ));
    }
    if actors.len() != compiled.graph.actors.len()
        || actors.iter().any(|id| id.is_empty() || id.len() > 128)
        || actors.iter().collect::<BTreeSet<_>>().len() != actors.len()
        || !(1..=4).contains(&layer_count)
    {
        return Err(unsupported(
            "placement requires unique stable actor rows and the decoded 1..4 collision layers",
        ));
    }
    let pc = *compiled
        .bindings
        .get(node)
        .ok_or_else(|| unsupported("unknown stable placement node"))?;
    let graph = &compiled.graph;
    let instruction = graph
        .instructions
        .binary_search_by_key(&pc, |i| i.pc)
        .ok()
        .and_then(|n| graph.instructions.get(n))
        .ok_or_else(|| unsupported("placement node is not a reachable instruction"))?;
    let form = form(&instruction.spec.handler)?;
    let layer = match form.layer {
        Some(index) if instruction.spec.operands[index].kind == "u8" => Some(
            u8::try_from(instruction.operands[index])
                .map_err(|_| unsupported("placement layer exceeds its encoded byte"))?,
        ),
        None => None,
        _ => {
            return Err(unsupported(
                "canonical layer operand differs from its qualified byte reader",
            ));
        }
    };
    let mut position: [Coordinate; 3] = std::array::from_fn(|_| Coordinate::Floor {
        explicit_layer: layer,
    });
    for (axis, operand) in form.coordinates.iter().enumerate() {
        if let Some(index) = operand {
            let raw = u16::try_from(instruction.operands[*index])
                .map_err(|_| unsupported("placement coordinate exceeds its encoded halfword"))?;
            let descriptor = &instruction.spec.operands[*index];
            position[axis] = match descriptor.kind.as_str() {
                "s16" => Coordinate::Immediate { value: raw as i16 },
                "sel" if instruction.immediate[*index] == Some(true) => {
                    Coordinate::Immediate { value: raw as i16 }
                }
                "sel" if instruction.immediate[*index] == Some(false) => {
                    Coordinate::Variable { id: raw }
                }
                _ => {
                    return Err(unsupported(
                        "canonical placement operand form differs from its qualified reader",
                    ));
                }
            };
        }
    }
    let owners = index
        .instruction_owners(pc)?
        .into_iter()
        .map(|owner| {
            let actor = actors
                .get(owner.actor)
                .cloned()
                .ok_or_else(|| unsupported("placement actor row lacks a stable scene entity"))?;
            Ok(Owner {
                actor,
                event: owner.event as u8,
            })
        })
        .collect::<Result<Vec<_>, FieldError>>()?;
    let distinct: BTreeSet<_> = owners.iter().map(|o| &o.actor).collect();
    let mut diagnostics = vec!["Explicit authored instruction binding; control flow, other writers and floor evaluation can change the runtime pose.".into()];
    let mut can_transform = distinct.len() == 1;
    if let Err(cause) = authored.check_writable_node(node) {
        can_transform = false;
        diagnostics.push(format!("{}: {}", cause.kind, cause.message));
    }
    if let Some(block) = graph
        .blocks
        .iter()
        .find(|b| b.kind == "pinned" && b.start <= pc && pc < b.end)
    {
        can_transform = false;
        diagnostics.push(format!(
            "Compiled storage is pinned: {}",
            block.reason.as_deref().unwrap_or("opaque storage")
        ));
    }
    if distinct.len() != 1 {
        diagnostics.push("This source instruction is shared by multiple actor contexts; an independent script binding is required for a single-actor transform.".into());
    }
    if position
        .iter()
        .any(|p| matches!(p, Coordinate::Variable { .. }))
    {
        can_transform = false;
        diagnostics.push("Variable-derived coordinates remain symbolic; a viewport transform cannot silently replace their binding with a constant.".into());
    }
    if layer.is_some_and(|layer| usize::from(layer) >= layer_count) {
        can_transform = false;
        diagnostics
            .push("The source layer selector is outside the decoded collision namespace.".into());
    }
    Ok(Binding {
        node: node.into(),
        pc: pc as u32,
        handler: instruction.spec.handler.clone(),
        position,
        owners,
        can_transform,
        diagnostics,
    })
}
/// Build a single M6 batch, preserving unrelated fields/flags. The caller uses
/// its normal scene candidate/allocator validation before publishing it.
pub fn prepare(
    compiled: &draft::Compiled,
    authored: &draft::Draft,
    actors: &[String],
    layer_count: usize,
    change: &Change,
) -> Result<draft::Action, FieldError> {
    let binding = describe(compiled, authored, &change.node, actors, layer_count)?;
    if !binding
        .owners
        .iter()
        .any(|o| o.actor == change.actor && o.event == change.event)
    {
        return Err(unsupported(
            "placement instruction is not reachable from the selected actor/event binding",
        ));
    }
    if !binding.can_transform {
        return Err(unsupported(binding.diagnostics.join(" ")));
    }
    let form = form(&binding.handler)?;
    let pc = compiled.bindings[&change.node];
    let instruction = &compiled.graph.instructions[compiled
        .graph
        .instructions
        .binary_search_by_key(&pc, |i| i.pc)
        .map_err(|_| unsupported("placement instruction disappeared"))?];
    let mut actions = Vec::new();
    for (axis, value) in change.position.iter().enumerate() {
        if let Some(value) = value {
            let operand = form.coordinates[axis].ok_or_else(||unsupported("Y is floor-derived for this instruction; select a height-writing source binding instead"))?;
            actions.push(draft::Action::SetOperand {
                node: change.node.clone(),
                operand,
                value: draft::OperandValue::Literal {
                    value: u32::from(u16::from_le_bytes(value.to_le_bytes())),
                    immediate: if instruction.spec.operands[operand].kind == "sel" {
                        Some(true)
                    } else {
                        None
                    },
                },
            });
        }
    }
    if let Some(layer) = change.layer {
        let operand = form
            .layer
            .ok_or_else(|| unsupported("this placement instruction has no explicit layer field"))?;
        if usize::from(layer) >= layer_count {
            return Err(unsupported(
                "placement layer is outside the decoded collision namespace",
            ));
        }
        actions.push(draft::Action::SetOperand {
            node: change.node.clone(),
            operand,
            value: draft::OperandValue::Literal {
                value: u32::from(layer),
                immediate: None,
            },
        });
    }
    if actions.is_empty() {
        return Err(unsupported("placement transform has no changed fields"));
    }
    Ok(draft::Action::Batch { actions })
}
