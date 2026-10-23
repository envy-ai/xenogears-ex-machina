//! Source-owned, revisioned scene transactions. The immutable container is never
//! replaced by a compiled result. History replays actions, not opaque byte blobs.
use super::{bindings, bundle, collision, entrances, formats, geometry};
use crate::field::{self, FieldError, FieldGraph, draft};
use serde::{Deserialize, Serialize};
use std::{collections::BTreeMap, sync::Arc};
use xem_disc::field_context::hash;

const HISTORY_LIMIT: usize = 128;
fn invalid(message: impl Into<String>) -> FieldError {
    FieldError::new("invalid_scene_edit", message)
}
fn unsupported(message: impl Into<String>) -> FieldError {
    FieldError::new("unsupported_scene_edit", message)
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum Action {
    Batch {
        actions: Vec<Action>,
    },
    SetDescriptor {
        id: String,
        position: [u16; 3],
        rotation: [u16; 3],
    },
    AddInstance {
        template: String,
        model: u16,
        position: [u16; 3],
        rotation: [u16; 3],
    },
    DuplicateInstance {
        id: String,
    },
    DeleteInstance {
        id: String,
    },
    SetZone {
        id: String,
        corners: [[i16; 3]; 4],
    },
    AddZone {
        corners: [[i16; 3]; 4],
    },
    DuplicateZone {
        id: String,
    },
    DeleteZone {
        id: String,
    },
    SetCollisionVertex {
        id: String,
        position: [i16; 3],
    },
    SetEntrance {
        value: entrances::Entrance,
    },
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Document {
    pub schema_version: u32,
    pub input: bundle::Input,
    pub entrance_selectors: Vec<u16>,
    pub actions: Vec<Action>,
    pub allocation_starts: Vec<u32>,
    pub cursor: usize,
    pub next_id: u32,
}
#[derive(Clone, Debug, Serialize)]
pub struct Entity<T> {
    pub id: String,
    pub value: T,
}
#[derive(Clone)]
struct State {
    descriptors: Vec<Entity<formats::Descriptor>>,
    zones: Vec<Entity<formats::Zone>>,
    collision: collision::Collision,
    entrances: Vec<entrances::Entrance>,
}
struct Base {
    source: bundle::Source,
    events: FieldGraph,
    references: bindings::References,
    model_count: usize,
    zones_tail: Vec<u8>,
    initial: State,
    entrance_selectors: Vec<u16>,
}
#[derive(Clone)]
pub struct Draft {
    base: Arc<Base>,
    state: State,
    actions: Vec<Action>,
    allocation_starts: Vec<u32>,
    cursor: usize,
    next_id: u32,
    revision: u32,
}
#[derive(Clone, Debug, Serialize)]
pub struct Snapshot {
    pub revision: u32,
    pub can_undo: bool,
    pub can_redo: bool,
    pub modified: bool,
    pub descriptors: Vec<Entity<formats::Descriptor>>,
    pub zones: Vec<Entity<formats::Zone>>,
    pub collision: collision::Collision,
    pub entrances: Vec<entrances::Entrance>,
    pub source_sha256: String,
}
#[derive(Clone, Debug, Serialize)]
pub struct Compiled {
    pub bundle: Vec<u8>,
    pub bundle_sha256: String,
    pub source_sha256: String,
    pub descriptor_ids: Vec<String>,
    pub zone_ids: Vec<String>,
    pub events: draft::Compiled,
}
fn index<T>(values: &[Entity<T>], id: &str) -> Result<usize, FieldError> {
    values
        .iter()
        .position(|e| e.id == id)
        .ok_or_else(|| invalid(format!("unknown scene entity {id}")))
}
fn new_id(next: &mut u32, kind: &str) -> Result<String, FieldError> {
    if *next == 0 {
        return Err(invalid("scene allocation watermark is zero"));
    }
    let id = *next;
    *next = next
        .checked_add(1)
        .ok_or_else(|| invalid("scene identity allocator exhausted"))?;
    Ok(format!("scene:{id}:{kind}"))
}
fn entity_ids<T>(values: &[Entity<T>]) -> Vec<String> {
    values.iter().map(|e| e.id.clone()).collect()
}
impl Draft {
    pub fn open(input: bundle::Input, entrance_selectors: Vec<u16>) -> Result<Self, FieldError> {
        let source = bundle::Source::load(input)?;
        let c = &source.components;
        let events = field::decode(&c[5].bytes, Some(c[5].logical_size))?;
        if events.actors.len() > source.header.descriptors.len() {
            return Err(invalid("event actors exceed source descriptors"));
        }
        let model_count = geometry::decode_geometry(&c[2].bytes, c[2].logical_size)?
            .groups
            .len();
        for d in &source.header.descriptors {
            if d.flags & 0x40 == 0 && usize::from(d.model) >= model_count {
                return Err(invalid("source model selector exceeds geometry groups"));
            }
        }
        let collision = collision::decode_collision(&c[1].bytes, c[1].logical_size)?;
        let zones = formats::decode_zones(&c[8].bytes, c[8].logical_size)?;
        let entrance_records = if entrance_selectors.is_empty() {
            Vec::new()
        } else {
            entrances::decode_entrances(&events, &entrance_selectors)?
        };
        if entrance_records
            .iter()
            .any(|e| usize::from(e.layer) >= collision.layers.len())
        {
            return Err(invalid("source entrance layer exceeds collision layers"));
        }
        let initial = State {
            descriptors: source
                .header
                .descriptors
                .iter()
                .enumerate()
                .map(|(i, value)| Entity {
                    id: format!("descriptor:{i}"),
                    value: value.clone(),
                })
                .collect(),
            zones: zones
                .zones
                .into_iter()
                .enumerate()
                .map(|(i, value)| Entity {
                    id: format!("zone:{i}"),
                    value,
                })
                .collect(),
            collision,
            entrances: entrance_records,
        };
        let references = bindings::capture(
            &events,
            &entity_ids(&initial.descriptors),
            &entity_ids(&initial.zones),
        )?;
        Ok(Self {
            base: Arc::new(Base {
                source,
                events,
                references,
                model_count,
                zones_tail: zones.tail,
                initial: initial.clone(),
                entrance_selectors,
            }),
            state: initial,
            actions: Vec::new(),
            allocation_starts: Vec::new(),
            cursor: 0,
            next_id: 1,
            revision: 0,
        })
    }
    pub fn revision(&self) -> u32 {
        self.revision
    }
    pub fn snapshot(&self) -> Snapshot {
        Snapshot {
            revision: self.revision,
            can_undo: self.cursor > 0,
            can_redo: self.cursor < self.actions.len(),
            modified: self.cursor > 0,
            descriptors: self.state.descriptors.clone(),
            zones: self.state.zones.clone(),
            collision: self.state.collision.clone(),
            entrances: self.state.entrances.clone(),
            source_sha256: self.base.source.header.source_sha256.clone(),
        }
    }
    pub fn document(&self) -> Document {
        Document {
            schema_version: 1,
            input: self.base.source.input().clone(),
            entrance_selectors: self.base.entrance_selectors.clone(),
            actions: self.actions.clone(),
            allocation_starts: self.allocation_starts.clone(),
            cursor: self.cursor,
            next_id: self.next_id,
        }
    }
    pub fn import(document: Document) -> Result<Self, FieldError> {
        if document.schema_version != 1
            || document.actions.len() > HISTORY_LIMIT
            || document.cursor > document.actions.len()
            || document.actions.len() != document.allocation_starts.len()
            || document.next_id == 0
        {
            return Err(invalid("invalid scene document schema/history"));
        }
        let mut result = Self::open(document.input, document.entrance_selectors)?;
        for (action, start) in document.actions.into_iter().zip(document.allocation_starts) {
            if start < result.next_id {
                return Err(invalid("scene history reuses allocated identities"));
            }
            result.next_id = start;
            result.edit(result.revision, action)?;
        }
        if document.next_id < result.next_id {
            return Err(invalid("scene watermark reuses allocated identities"));
        }
        result.next_id = document.next_id;
        result.state = result.replay(document.cursor)?;
        result.cursor = document.cursor;
        result.revision = 0;
        Ok(result)
    }
    fn next_revision(&self, revision: u32) -> Result<u32, FieldError> {
        if revision != self.revision {
            return Err(FieldError::new(
                "stale_revision",
                format!(
                    "expected scene revision {}, received {revision}",
                    self.revision
                ),
            ));
        }
        self.revision
            .checked_add(1)
            .ok_or_else(|| invalid("scene revision exhausted"))
    }
    pub fn edit(&mut self, revision: u32, action: Action) -> Result<(), FieldError> {
        let next_revision = self.next_revision(revision)?;
        if self.cursor >= HISTORY_LIMIT {
            return Err(invalid("scene history limit is 128 applied transactions"));
        }
        let mut candidate = self.state.clone();
        let mut next_id = self.next_id;
        self.apply(&mut candidate, &action, &mut next_id)?;
        self.compile_state(&candidate)?;
        // No state, history, revision or allocator mutation precedes validation.
        self.actions.truncate(self.cursor);
        self.allocation_starts.truncate(self.cursor);
        self.actions.push(action);
        self.allocation_starts.push(self.next_id);
        self.state = candidate;
        self.next_id = next_id;
        self.cursor += 1;
        self.revision = next_revision;
        Ok(())
    }
    fn replay(&self, cursor: usize) -> Result<State, FieldError> {
        let mut state = self.base.initial.clone();
        for (action, start) in self.actions[..cursor]
            .iter()
            .zip(&self.allocation_starts[..cursor])
        {
            let mut next = *start;
            self.apply(&mut state, action, &mut next)?;
        }
        self.compile_state(&state)?;
        Ok(state)
    }
    pub fn undo(&mut self, revision: u32) -> Result<(), FieldError> {
        let next = self.next_revision(revision)?;
        if self.cursor == 0 {
            return Err(invalid("no scene edit to undo"));
        }
        let state = self.replay(self.cursor - 1)?;
        self.state = state;
        self.cursor -= 1;
        self.revision = next;
        Ok(())
    }
    pub fn redo(&mut self, revision: u32) -> Result<(), FieldError> {
        let next = self.next_revision(revision)?;
        if self.cursor == self.actions.len() {
            return Err(invalid("no scene edit to redo"));
        }
        let state = self.replay(self.cursor + 1)?;
        self.state = state;
        self.cursor += 1;
        self.revision = next;
        Ok(())
    }
    fn static_instance(&self, state: &State, id: &str) -> Result<usize, FieldError> {
        let at = index(&state.descriptors, id)?;
        if at < self.base.events.actors.len() || state.descriptors[at].value.flags & 0x40 != 0 {
            return Err(unsupported(
                "actor CRUD requires a reference-safe event-row transaction, not instance CRUD",
            ));
        }
        Ok(at)
    }
    fn apply(&self, state: &mut State, action: &Action, next: &mut u32) -> Result<(), FieldError> {
        match action {
            Action::Batch { actions } => {
                if actions.is_empty()
                    || actions.len() > 32
                    || actions.iter().any(|a| matches!(a, Action::Batch { .. }))
                {
                    return Err(invalid("scene batch requires 1..32 non-batch actions"));
                }
                for action in actions {
                    self.apply(state, action, next)?;
                }
            }
            Action::SetDescriptor {
                id,
                position,
                rotation,
            } => {
                let at = index(&state.descriptors, id)?;
                let d = &mut state.descriptors[at].value;
                if d.flags & 0x40 != 0 && d.rotation != *rotation {
                    return Err(unsupported(
                        "sprite rotation is reset by the loader; use a proven script-facing binding",
                    ));
                }
                d.position = *position;
                d.rotation = *rotation;
            }
            Action::AddInstance {
                template,
                model,
                position,
                rotation,
            } => {
                let at = self.static_instance(state, template)?;
                if usize::from(*model) >= self.base.model_count {
                    return Err(invalid("model selector exceeds source geometry groups"));
                }
                let mut value = state.descriptors[at].value.clone();
                if *model != value.model {
                    return Err(unsupported(
                        "new instances reuse a proven source model/template; changing model requires compatible morph/material proof",
                    ));
                }
                value.model = *model;
                value.position = *position;
                value.rotation = *rotation;
                state.descriptors.push(Entity {
                    id: new_id(next, "descriptor")?,
                    value,
                });
            }
            Action::DuplicateInstance { id } => {
                let at = self.static_instance(state, id)?;
                state.descriptors.push(Entity {
                    id: new_id(next, "descriptor")?,
                    value: state.descriptors[at].value.clone(),
                });
            }
            Action::DeleteInstance { id } => {
                let at = self.static_instance(state, id)?;
                state.descriptors.remove(at);
            }
            Action::SetZone { id, corners } => {
                let at = index(&state.zones, id)?;
                state.zones[at].value.corners = *corners;
            }
            Action::AddZone { corners } => state.zones.push(Entity {
                id: new_id(next, "zone")?,
                value: formats::Zone { corners: *corners },
            }),
            Action::DuplicateZone { id } => {
                let at = index(&state.zones, id)?;
                state.zones.push(Entity {
                    id: new_id(next, "zone")?,
                    value: state.zones[at].value.clone(),
                });
            }
            Action::DeleteZone { id } => {
                let at = index(&state.zones, id)?;
                state.zones.remove(at);
            }
            Action::SetCollisionVertex { id, position } => {
                // Canonical source-derived identity, not an arbitrary mutable offset.
                let parts: Vec<_> = id.split(':').collect();
                if parts.len() != 4 || parts[0] != "collision" || parts[2] != "vertex" {
                    return Err(invalid("invalid collision vertex identity"));
                }
                let layer = parts[1]
                    .parse::<usize>()
                    .map_err(|_| invalid("invalid collision layer identity"))?;
                let vertex = parts[3]
                    .parse::<usize>()
                    .map_err(|_| invalid("invalid collision vertex identity"))?;
                if id != &format!("collision:{layer}:vertex:{vertex}") {
                    return Err(invalid("noncanonical collision vertex identity"));
                }
                let value = state
                    .collision
                    .layers
                    .get_mut(layer)
                    .and_then(|l| l.vertices.get_mut(vertex))
                    .ok_or_else(|| invalid("collision vertex outside source referenced prefix"))?;
                value[..3].copy_from_slice(position);
            }
            Action::SetEntrance { value } => {
                let at = state
                    .entrances
                    .iter()
                    .position(|e| e.selector == value.selector)
                    .ok_or_else(|| invalid("entrance selector lacks an explicit source binding"))?;
                if value.source_span != state.entrances[at].source_span {
                    return Err(invalid("entrance source span differs"));
                }
                state.entrances[at] = value.clone();
            }
        }
        Ok(())
    }
    pub fn compile(&self) -> Result<Compiled, FieldError> {
        self.compile_state(&self.state)
    }
    fn compile_state(&self, state: &State) -> Result<Compiled, FieldError> {
        let descriptor_ids = entity_ids(&state.descriptors);
        let zone_ids = entity_ids(&state.zones);
        let patches = self
            .base
            .references
            .remap(&self.base.events, &descriptor_ids, &zone_ids)?;
        let event_draft = draft::Draft::open(self.base.events.clone())?;
        let rewrites = patches
            .iter()
            .map(|p| (format!("pc:{}", p.pc), p.operand, p.value as u8))
            .collect::<Vec<_>>();
        let mut events =
            event_draft.compile_with_scene_operands(&rewrites, descriptor_ids.len())?;
        if !state.entrances.is_empty() {
            events.bytes = entrances::encode_entrances(
                &events.graph,
                &state.entrances,
                state.collision.layers.len(),
            )?;
            events.graph = field::decode(&events.bytes, Some(events.declared_size))?;
        }
        let mut header = self.base.source.header.clone();
        header.descriptors = state.descriptors.iter().map(|e| e.value.clone()).collect();
        let collision =
            collision::encode_collision(&self.base.source.components[1].bytes, &state.collision)?;
        let zones = formats::encode_zones(&formats::Zones {
            zones: state.zones.iter().map(|e| e.value.clone()).collect(),
            tail: self.base.zones_tail.clone(),
        })?;
        let mut replacements = BTreeMap::new();
        replacements.insert(
            1,
            bundle::Replacement {
                logical_size: self.base.source.components[1].logical_size,
                bytes: collision,
            },
        );
        replacements.insert(
            5,
            bundle::Replacement {
                logical_size: events.declared_size,
                bytes: events.bytes.clone(),
            },
        );
        replacements.insert(
            8,
            bundle::Replacement {
                logical_size: state
                    .zones
                    .len()
                    .checked_mul(24)
                    .ok_or_else(|| invalid("zone table size overflow"))?,
                bytes: zones,
            },
        );
        let bundle = self.base.source.repack(&header, &replacements)?;
        Ok(Compiled {
            bundle_sha256: hash(&bundle),
            source_sha256: self.base.source.header.source_sha256.clone(),
            bundle,
            descriptor_ids,
            zone_ids,
            events,
        })
    }
}
