//! Actor/event membership and possible dependencies, without timeline claims.

use super::{FieldError, FieldGraph, Instruction, OperandSpec, structure::StructureIndex};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};
use std::sync::OnceLock;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct SourceEvidence {
    pub source: String,
    pub function: String,
    pub function_sha256: String,
    pub helper: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct AccessFact {
    pub handler: String,
    pub operand: OperandSpec,
    pub direction: String,
    pub reason: Option<String>,
    pub provenance: Vec<SourceEvidence>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct AccessMetadata {
    pub schema_version: u32,
    pub generator: String,
    pub sources: BTreeMap<String, String>,
    pub scope: String,
    pub facts: Vec<AccessFact>,
}

/// Public, inspectable operand proofs generated from the original opcode tables.
pub fn access_metadata() -> Result<&'static AccessMetadata, FieldError> {
    static VALUE: OnceLock<Result<AccessMetadata, String>> = OnceLock::new();
    match VALUE.get_or_init(|| {
        let value: AccessMetadata =
            serde_json::from_str(include_str!("../../data/field-access.json"))
                .map_err(|error| error.to_string())?;
        let opcode_metadata = super::opcodes::metadata().map_err(|error| error.to_string())?;
        if value.schema_version != 1
            || value.generator != "tools/studio/field_access.py"
            || value.sources.get("tools/analysis/events.py") != Some(&opcode_metadata.source_sha256)
            || value.sources.values().any(|hash| hash.len() != 64)
        {
            return Err("invalid field access metadata provenance".to_owned());
        }
        let forms: Vec<_> = opcode_metadata
            .primary
            .values()
            .chain(opcode_metadata.extended.values())
            .flatten()
            .collect();
        let mut keys = BTreeSet::new();
        for fact in &value.facts {
            if !["read", "write", "read_write", "unknown"].contains(&fact.direction.as_str())
                || !forms.iter().any(|form| {
                    form.handler == fact.handler && form.operands.contains(&fact.operand)
                })
                || !keys.insert((
                    &fact.handler,
                    &fact.operand.kind,
                    fact.operand.offset,
                    fact.operand.flags,
                    fact.operand.bit,
                ))
                || (fact.direction != "unknown" && fact.provenance.is_empty())
                || fact.provenance.iter().any(|proof| {
                    proof.function_sha256.len() != 64 || !value.sources.contains_key(&proof.source)
                })
            {
                return Err("invalid field operand access proof".to_owned());
            }
        }
        for form in forms {
            for operand in &form.operands {
                if ["var", "iv", "sel", "bit", "data"].contains(&operand.kind.as_str())
                    && !value
                        .facts
                        .iter()
                        .any(|fact| fact.handler == form.handler && fact.operand == *operand)
                {
                    return Err("missing field operand access proof".to_owned());
                }
            }
        }
        Ok(value)
    }) {
        Ok(value) => Ok(value),
        Err(message) => Err(FieldError::new("invalid_metadata", message.clone())),
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct EventLocation {
    pub actor: usize,
    pub event: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct InstructionLocation {
    pub actor: usize,
    pub event: usize,
    pub pc: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct ActorSelector {
    pub raw: u8,
    /// direct, self, or party. Party membership is runtime state.
    pub kind: String,
    pub direct_actor: Option<usize>,
    pub party_slot: Option<u8>,
    pub resolved_actor: Option<usize>,
    pub unresolved: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct EventRequest {
    pub source: InstructionLocation,
    pub kind: String,
    pub actor: ActorSelector,
    pub packed_event: u8,
    pub event: u8,
    pub priority: u8,
    pub target: Option<EventLocation>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct VariableAccess {
    pub pc: usize,
    pub operand_index: usize,
    pub operand_offset: usize,
    pub operand_kind: String,
    pub raw: u32,
    /// Byte reference before the bank's >>1 indexing, including odd aliases.
    pub reference: Option<u32>,
    pub slot: Option<usize>,
    pub odd_alias: bool,
    pub bit: Option<u8>,
    pub direction: String,
    /// resolved, invalid_reference, indirect, or unknown_role.
    pub resolution: String,
    pub reason: Option<String>,
    pub provenance: Vec<SourceEvidence>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct InstructionSkip {
    pub pc: usize,
    pub targets: Vec<usize>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct EventLane {
    pub event: usize,
    pub entry_pc: usize,
    pub skipped: bool,
    pub instruction_count: usize,
    pub instruction_pcs: Vec<usize>,
    pub skips: Vec<InstructionSkip>,
    pub variable_accesses: Vec<VariableAccess>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct ActorLane {
    pub actor: usize,
    pub events: Vec<EventLane>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct AccessLocation {
    pub actor: usize,
    pub event: usize,
    pub pc: usize,
    pub operand_index: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct VariableDependency {
    pub slot: usize,
    pub writers: Vec<AccessLocation>,
    pub readers: Vec<AccessLocation>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct MapLanes {
    pub schema_version: u32,
    pub actors: Vec<ActorLane>,
    pub requests: Vec<EventRequest>,
    /// Groups retain source references, avoiding a quadratic set of link pairs.
    /// They express potential sharing, never execution order or causality.
    pub variable_dependencies: Vec<VariableDependency>,
}

/// Source-qualified symbolic forms shared with scene reference repair. Literal
/// 0xfc is deliberately not a party/self selector.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum SymbolicActor {
    SelfActor,
    Party(u8),
}
pub(crate) fn symbolic_actor(raw: u8) -> Option<SymbolicActor> {
    match raw {
        0xfd..=0xff => Some(SymbolicActor::Party(0xff - raw)),
        0xfb => Some(SymbolicActor::SelfActor),
        _ => None,
    }
}

fn selector(raw: u8, source_actor: usize, actor_count: usize) -> ActorSelector {
    let mut value = ActorSelector {
        raw,
        kind: "direct".to_owned(),
        direct_actor: None,
        party_slot: None,
        resolved_actor: None,
        unresolved: None,
    };
    match symbolic_actor(raw) {
        Some(SymbolicActor::Party(slot)) => {
            value.kind = "party".to_owned();
            value.party_slot = Some(slot);
            value.unresolved = Some("Party actor binding requires runtime state.".to_owned());
        }
        Some(SymbolicActor::SelfActor) => {
            value.kind = "self".to_owned();
            value.resolved_actor = Some(source_actor);
        }
        None => {
            value.direct_actor = Some(usize::from(raw));
            if usize::from(raw) < actor_count {
                value.resolved_actor = Some(usize::from(raw));
            } else {
                value.unresolved = Some("Literal actor id is outside this map.".to_owned());
            }
        }
    }
    value
}

fn accesses(
    instruction: &Instruction,
    facts: &AccessMetadata,
) -> Result<Vec<VariableAccess>, FieldError> {
    let mut result = Vec::new();
    for (index, operand) in instruction.spec.operands.iter().enumerate() {
        if !["var", "iv", "sel", "bit", "data"].contains(&operand.kind.as_str()) {
            continue;
        }
        if ["iv", "sel"].contains(&operand.kind.as_str())
            && instruction.immediate[index] == Some(true)
        {
            continue;
        }
        let fact = facts
            .facts
            .iter()
            .find(|fact| fact.handler == instruction.spec.handler && fact.operand == *operand)
            .ok_or_else(|| FieldError::new("invalid_metadata", "missing operand access fact"))?;
        let raw = instruction.operands[index];
        let reference = match operand.kind.as_str() {
            "data" => None,
            "bit" => Some(raw >> 4),
            _ => Some(raw),
        };
        let slot = reference
            .filter(|reference| *reference < 2048)
            .map(|reference| (reference >> 1) as usize);
        let resolution = if reference.is_none() {
            "indirect"
        } else if slot.is_none() {
            "invalid_reference"
        } else if fact.direction == "unknown" {
            "unknown_role"
        } else {
            "resolved"
        };
        result.push(VariableAccess {
            pc: instruction.pc,
            operand_index: index,
            operand_offset: operand.offset,
            operand_kind: operand.kind.clone(),
            raw,
            reference,
            slot,
            odd_alias: reference.is_some_and(|reference| reference & 1 != 0),
            bit: (operand.kind == "bit").then_some((raw & 15) as u8),
            direction: fact.direction.clone(),
            resolution: resolution.to_owned(),
            reason: if resolution == "invalid_reference" {
                Some("Variable byte reference is outside the 2048-byte bank.".to_owned())
            } else {
                fact.reason.clone()
            },
            provenance: fact.provenance.clone(),
        });
    }
    Ok(result)
}

pub fn lanes(graph: &FieldGraph) -> Result<MapLanes, FieldError> {
    let index = StructureIndex::new(graph)?;
    let facts = access_metadata()?;
    let instructions: BTreeMap<_, _> = graph
        .instructions
        .iter()
        .map(|item| (item.pc, item))
        .collect();
    // Shared event entries often cover the same code. Prove/resolve operands
    // once per instruction, independently of how many events can reach it.
    let access_by_pc: BTreeMap<_, _> = graph
        .instructions
        .iter()
        .map(|instruction| Ok((instruction.pc, accesses(instruction, facts)?)))
        .collect::<Result<_, FieldError>>()?;
    let entries: BTreeSet<_> = graph
        .entries
        .iter()
        .map(|entry| (entry.actor, entry.event))
        .collect();
    let mut actors = Vec::with_capacity(graph.actors.len());
    let mut requests = Vec::new();
    let mut dependencies = BTreeMap::<usize, VariableDependency>::new();
    for (actor, event_entries) in graph.actors.iter().enumerate() {
        let mut events = Vec::with_capacity(32);
        for (event, entry_pc) in event_entries.iter().enumerate() {
            let pcs = index.reachable_pcs(actor, event)?;
            let mut variable_accesses = Vec::new();
            let mut skips = Vec::new();
            for pc in &pcs {
                let instruction = instructions.get(pc).ok_or_else(|| {
                    FieldError::new("invalid_graph", "reachable instruction missing from graph")
                })?;
                if !instruction.skips.is_empty() {
                    skips.push(InstructionSkip {
                        pc: *pc,
                        targets: instruction.skips.clone(),
                    });
                }
                if [
                    "field_event_request_event_started",
                    "field_event_request_event_finished",
                ]
                .contains(&instruction.spec.handler.as_str())
                {
                    let actor_operand = instruction
                        .spec
                        .operands
                        .iter()
                        .position(|operand| operand.kind == "actor" && operand.offset == 1)
                        .ok_or_else(|| {
                            FieldError::new("invalid_metadata", "request actor descriptor missing")
                        })?;
                    let event_operand = instruction
                        .spec
                        .operands
                        .iter()
                        .position(|operand| operand.kind == "u8" && operand.offset == 2)
                        .ok_or_else(|| {
                            FieldError::new("invalid_metadata", "request event descriptor missing")
                        })?;
                    let selected = selector(
                        instruction.operands[actor_operand] as u8,
                        actor,
                        graph.actors.len(),
                    );
                    let packed_event = instruction.operands[event_operand] as u8;
                    let target = selected.resolved_actor.map(|actor| EventLocation {
                        actor,
                        event: usize::from(packed_event & 31),
                    });
                    requests.push(EventRequest {
                        source: InstructionLocation {
                            actor,
                            event,
                            pc: *pc,
                        },
                        kind: instruction.spec.mnemonic.clone(),
                        actor: selected,
                        packed_event,
                        event: packed_event & 31,
                        priority: packed_event >> 5,
                        target,
                    });
                }
                for access in &access_by_pc[pc] {
                    if let Some(slot) = access.slot.filter(|_| access.resolution == "resolved") {
                        let group =
                            dependencies
                                .entry(slot)
                                .or_insert_with(|| VariableDependency {
                                    slot,
                                    writers: Vec::new(),
                                    readers: Vec::new(),
                                });
                        let location = AccessLocation {
                            actor,
                            event,
                            pc: *pc,
                            operand_index: access.operand_index,
                        };
                        if ["write", "read_write"].contains(&access.direction.as_str()) {
                            group.writers.push(location.clone());
                        }
                        if ["read", "read_write"].contains(&access.direction.as_str()) {
                            group.readers.push(location);
                        }
                    }
                    variable_accesses.push(access.clone());
                }
            }
            events.push(EventLane {
                event,
                entry_pc: usize::from(*entry_pc),
                skipped: !entries.contains(&(actor, event)),
                instruction_count: pcs.len(),
                instruction_pcs: pcs,
                skips,
                variable_accesses,
            });
        }
        actors.push(ActorLane { actor, events });
    }
    Ok(MapLanes {
        schema_version: 1,
        actors,
        requests,
        variable_dependencies: dependencies
            .into_values()
            .filter(|group| !group.writers.is_empty() && !group.readers.is_empty())
            .collect(),
    })
}
