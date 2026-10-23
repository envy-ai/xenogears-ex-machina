//! Proven scene operands over the single canonical opcode registry.
//!
//! Capture is inspectable even for unresolved source references. Repair is
//! deliberately stricter: unresolved semantics, aliases and dangling stable IDs
//! cannot be silently rewritten. Party/self selectors remain symbolic.
use crate::field::{
    self, FieldError, FieldGraph, OperandSpec,
    lanes::{SourceEvidence, SymbolicActor, access_metadata, symbolic_actor},
};
use serde::{Deserialize, Serialize};
use std::{
    collections::{BTreeMap, BTreeSet},
    sync::OnceLock,
};

fn invalid(message: impl Into<String>) -> FieldError {
    FieldError::new("invalid_scene_reference", message)
}
fn unsupported(message: impl Into<String>) -> FieldError {
    FieldError::new("unsupported_scene_reference", message)
}
fn digest(value: &str) -> bool {
    value.len() == 64
        && value
            .bytes()
            .all(|b| b.is_ascii_hexdigit() && !b.is_ascii_uppercase())
}
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ActorFact {
    pub handler: String,
    pub operand: OperandSpec,
    pub role: String,
    pub aliases: Vec<OperandSpec>,
    pub provenance: Vec<SourceEvidence>,
    pub reason: Option<String>,
}
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ZoneFact {
    pub handler: String,
    pub operand: OperandSpec,
    pub role: String,
    pub provenance: Vec<SourceEvidence>,
    pub macro_sha256: String,
}
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct BindingMetadata {
    pub schema_version: u32,
    pub generator: String,
    pub sources: BTreeMap<String, String>,
    pub actor_reader_sha256: String,
    pub actor_readers: BTreeMap<String, String>,
    pub scope: String,
    pub actors: Vec<ActorFact>,
    pub zones: Vec<ZoneFact>,
}
/// Facts are generated from current source and checked against the canonical
/// opcode/access provenance. The stale-output Python check verifies source bytes.
pub fn metadata() -> Result<&'static BindingMetadata, FieldError> {
    static VALUE: OnceLock<Result<BindingMetadata, String>> = OnceLock::new();
    match VALUE.get_or_init(|| {
        let m: BindingMetadata =
            serde_json::from_str(include_str!("../../data/field-scene-bindings.json"))
                .map_err(|e| e.to_string())?;
        let op = field::opcode_metadata().map_err(|e| e.to_string())?;
        let access = access_metadata().map_err(|e| e.to_string())?;
        if m.schema_version != 1
            || m.generator != "tools/studio/scene_bindings.py"
            || m.scope.is_empty()
            || m.sources.values().any(|s| !digest(s))
            || m.sources.get("tools/analysis/events.py") != Some(&op.source_sha256)
            || m.sources
                .iter()
                .any(|(s, h)| access.sources.get(s) != Some(h))
            || m.actor_readers.len() != 2
            || m.actor_readers.get("field_event_read_actor_index") != Some(&m.actor_reader_sha256)
            || !m
                .actor_readers
                .contains_key("field_event_read_actor_index_or_leader")
            || m.actor_readers.values().any(|s| !digest(s))
        {
            return Err("invalid scene binding provenance".into());
        }
        let forms: Vec<_> = op
            .primary
            .values()
            .chain(op.extended.values())
            .flatten()
            .collect();
        let proof_valid = |p: &SourceEvidence| {
            m.sources.contains_key(&p.source)
                && !p.function.is_empty()
                && !p.helper.is_empty()
                && digest(&p.function_sha256)
        };
        let mut keys = BTreeSet::new();
        for fact in &m.actors {
            let related: Vec<_> = forms
                .iter()
                .filter(|f| f.handler == fact.handler && f.operands.contains(&fact.operand))
                .collect();
            if fact.operand.kind != "actor"
                || related.is_empty()
                || !keys.insert((fact.handler.clone(), fact.operand.offset))
                || !["actor_selector", "raw_and_actor_selector", "unknown"]
                    .contains(&fact.role.as_str())
                || (fact.role == "unknown"
                    && (fact.reason.as_ref().is_none_or(String::is_empty)
                        || !fact.provenance.is_empty()))
                || (fact.role != "unknown" && (fact.provenance.is_empty() || fact.reason.is_some()))
                || fact
                    .provenance
                    .iter()
                    .any(|p| !proof_valid(p) || !m.actor_readers.contains_key(&p.helper))
                || fact.aliases.iter().any(|alias| {
                    !related.iter().any(|f| f.operands.contains(alias))
                        || alias == &fact.operand
                        || alias.offset > fact.operand.offset
                        || op
                            .widths
                            .get(&alias.kind)
                            .is_none_or(|w| fact.operand.offset - alias.offset >= *w)
                })
            {
                return Err("invalid scene actor binding proof".into());
            }
        }
        for form in &forms {
            for operand in &form.operands {
                if operand.kind == "actor"
                    && !m
                        .actors
                        .iter()
                        .any(|f| f.handler == form.handler && f.operand == *operand)
                {
                    return Err("missing scene actor operand proof".into());
                }
            }
        }
        let mut zones = BTreeSet::new();
        for fact in &m.zones {
            if fact.role != "zone_index"
                || fact.operand.kind != "u8"
                || !zones.insert(&fact.handler)
                || !digest(&fact.macro_sha256)
                || fact.provenance.is_empty()
                || fact.provenance.iter().any(|p| !proof_valid(p))
                || !forms
                    .iter()
                    .any(|f| f.handler == fact.handler && f.operands.contains(&fact.operand))
            {
                return Err("invalid scene zone binding proof".into());
            }
        }
        Ok(m)
    }) {
        Ok(m) => Ok(m),
        Err(e) => Err(FieldError::new("invalid_metadata", e.clone())),
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Target {
    Entity { id: String },
    SelfActor,
    Party { slot: u8 },
    Unknown { reason: String },
}
#[derive(Clone, Debug, Serialize)]
pub struct Reference {
    pub node: String,
    pub pc: usize,
    pub operand: usize,
    pub role: String,
    pub raw: u8,
    pub target: Target,
    /// Some handlers read the same byte both symbolically and as a raw descriptor.
    pub raw_descriptor: Option<String>,
    pub aliases: Vec<OperandSpec>,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct ReferencePatch {
    pub pc: usize,
    pub operand: usize,
    pub value: u32,
}
#[derive(Clone, Debug, Serialize)]
pub struct References {
    source_sha256: String,
    descriptors: Vec<String>,
    zones: Vec<String>,
    pub entries: Vec<Reference>,
}
fn ids(values: &[String], name: &str) -> Result<BTreeMap<String, usize>, FieldError> {
    if values.len() > usize::from(u16::MAX) {
        return Err(invalid(format!(
            "{name} namespace exceeds the service budget of 65535 stable entities"
        )));
    }
    let mut result = BTreeMap::new();
    for (index, id) in values.iter().enumerate() {
        if id.is_empty() || id.len() > 128 || result.insert(id.clone(), index).is_some() {
            return Err(invalid(format!(
                "{name} has an empty, oversized or duplicate stable identity"
            )));
        }
    }
    Ok(result)
}
fn literal(raw: u8, values: &[String], label: &str) -> Target {
    values.get(usize::from(raw)).map_or_else(
        || Target::Unknown {
            reason: format!("Literal {label} {raw} is outside the source namespace."),
        },
        |id| Target::Entity { id: id.clone() },
    )
}
/// Capture one immutable, canonical source graph. Authored event changes must
/// explicitly update their bindings; a different compiled graph is not accepted
/// as a substitute for the source this capture describes.
pub fn capture(
    graph: &FieldGraph,
    descriptors: &[String],
    zones: &[String],
) -> Result<References, FieldError> {
    field::encode(graph)?;
    ids(descriptors, "descriptor")?;
    ids(zones, "zone")?;
    if graph.actors.len() > descriptors.len() {
        return Err(invalid("actor rows exceed descriptor namespace"));
    }
    let m = metadata()?;
    let mut entries = Vec::new();
    for ins in &graph.instructions {
        for (operand, spec) in ins.spec.operands.iter().enumerate() {
            let actor = m
                .actors
                .iter()
                .find(|f| f.handler == ins.spec.handler && f.operand == *spec);
            let zone = m
                .zones
                .iter()
                .find(|f| f.handler == ins.spec.handler && f.operand == *spec);
            if actor.is_none() && zone.is_none() {
                continue;
            }
            let raw = u8::try_from(ins.operands[operand])
                .map_err(|_| invalid("scene selector exceeds byte"))?;
            let (role, target, raw_descriptor, aliases) = if let Some(f) = actor {
                let target = if f.role == "unknown" {
                    Target::Unknown {
                        reason: f.reason.clone().unwrap_or_default(),
                    }
                } else {
                    match symbolic_actor(raw) {
                        Some(SymbolicActor::SelfActor) => Target::SelfActor,
                        Some(SymbolicActor::Party(slot)) => Target::Party { slot },
                        None => literal(raw, descriptors, "descriptor"),
                    }
                };
                (
                    f.role.clone(),
                    target,
                    if f.role == "raw_and_actor_selector" {
                        descriptors.get(usize::from(raw)).cloned()
                    } else {
                        None
                    },
                    f.aliases.clone(),
                )
            } else {
                let op = field::opcode_metadata()?;
                let aliases = ins
                    .spec
                    .operands
                    .iter()
                    .filter(|other| {
                        *other != spec
                            && other.offset <= spec.offset
                            && op
                                .widths
                                .get(&other.kind)
                                .is_some_and(|w| spec.offset - other.offset < *w)
                    })
                    .cloned()
                    .collect();
                (
                    "zone_index".into(),
                    literal(raw, zones, "zone"),
                    None,
                    aliases,
                )
            };
            entries.push(Reference {
                node: format!("pc:{}", ins.pc),
                pc: ins.pc,
                operand,
                role,
                raw,
                target,
                raw_descriptor,
                aliases,
            });
        }
    }
    Ok(References {
        source_sha256: graph.source_sha256.clone(),
        descriptors: descriptors.to_vec(),
        zones: zones.to_vec(),
        entries,
    })
}
impl References {
    /// Derive literal rewrites without mutating either source or authored state.
    /// An unchanged namespace does not turn an original diagnostic into a guessed
    /// success. Any affected unresolved reference prevents structural mutation.
    pub fn remap(
        &self,
        graph: &FieldGraph,
        descriptors: &[String],
        zones: &[String],
    ) -> Result<Vec<ReferencePatch>, FieldError> {
        field::encode(graph)?;
        if graph.source_sha256 != self.source_sha256 {
            return Err(invalid(
                "reference capture belongs to a different source graph",
            ));
        }
        self.remap_entries(self.entries.iter(), descriptors, zones)
    }
    /// Private scene history adapter. Captures retain immutable namespaces, but
    /// only currently reachable stable nodes participate in a compilation.
    pub(crate) fn remap_selected(
        &self,
        selected: &[usize],
        descriptors: &[String],
        zones: &[String],
    ) -> Result<Vec<(String, usize, u8)>, FieldError> {
        let entries = selected
            .iter()
            .map(|&i| {
                self.entries
                    .get(i)
                    .ok_or_else(|| invalid("captured scene reference is missing"))
            })
            .collect::<Result<Vec<_>, _>>()?;
        let nodes: BTreeMap<_, _> = entries
            .iter()
            .map(|r| ((r.pc, r.operand), r.node.clone()))
            .collect();
        self.remap_entries(entries.into_iter(), descriptors, zones)?
            .into_iter()
            .map(|p| {
                let node = nodes
                    .get(&(p.pc, p.operand))
                    .cloned()
                    .ok_or_else(|| invalid("repaired scene reference lost its stable node"))?;
                let value = u8::try_from(p.value)
                    .map_err(|_| invalid("scene repair exceeds a selector byte"))?;
                Ok((node, p.operand, value))
            })
            .collect()
    }
    fn remap_entries<'a>(
        &self,
        entries: impl Iterator<Item = &'a Reference>,
        descriptors: &[String],
        zones: &[String],
    ) -> Result<Vec<ReferencePatch>, FieldError> {
        let descriptor_map = ids(descriptors, "descriptor")?;
        let zone_map = ids(zones, "zone")?;
        let mut patches = Vec::new();
        for reference in entries {
            let (before, after, indices) = if reference.role == "zone_index" {
                (&self.zones, zones, &zone_map)
            } else {
                (&self.descriptors, descriptors, &descriptor_map)
            };
            if before == after {
                continue;
            }
            let value = match &reference.target {
                Target::Unknown { reason } => {
                    return Err(unsupported(format!(
                        "{} operand {}: {reason}",
                        reference.node, reference.operand
                    )));
                }
                Target::Entity { id } => *indices.get(id).ok_or_else(|| {
                    invalid(format!(
                        "{} operand {} still references removed entity {id}",
                        reference.node, reference.operand
                    ))
                })?,
                Target::SelfActor | Target::Party { .. } => {
                    if reference.role == "raw_and_actor_selector"
                        && reference
                            .raw_descriptor
                            .as_ref()
                            .and_then(|id| indices.get(id))
                            .copied()
                            != Some(usize::from(reference.raw))
                    {
                        return Err(unsupported(format!(
                            "{} operand {} has conflicting raw descriptor and symbolic actor meanings",
                            reference.node, reference.operand
                        )));
                    }
                    continue;
                }
            };
            let byte =
                u8::try_from(value).map_err(|_| invalid("remapped scene operand exceeds u8"))?;
            if reference.role != "zone_index" && symbolic_actor(byte).is_some() {
                return Err(unsupported(format!(
                    "{}: literal descriptor index {byte} collides with a symbolic actor selector",
                    reference.node
                )));
            }
            if byte != reference.raw {
                if !reference.aliases.is_empty() {
                    return Err(unsupported(format!(
                        "{} operand {} overlaps another operand; automatic repair would change its meaning",
                        reference.node, reference.operand
                    )));
                }
                patches.push(ReferencePatch {
                    pc: reference.pc,
                    operand: reference.operand,
                    value: u32::from(byte),
                });
            }
        }
        Ok(patches)
    }
}
