//! Source-owned, revisioned scene transactions. The immutable container is never
//! replaced by a compiled result. History replays actions, not opaque byte blobs.
use super::{
    bundle, collision, entities, entrances, event_document, event_state, formats, geometry,
    topology,
};
use crate::field::{self, FieldError, draft};
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
    Event {
        #[serde(deserialize_with = "event_document::action_from_json")]
        action: Box<draft::Action>,
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
    AddCollisionVertex {
        layer: String,
        position: [i16; 3],
    },
    DuplicateCollisionVertex {
        id: String,
    },
    DeleteCollisionVertex {
        id: String,
    },
    AddCollisionTriangle {
        layer: String,
        vertices: [String; 3],
        attribute: u8,
        height_bump_raw: u8,
    },
    DuplicateCollisionTriangle {
        id: String,
    },
    DeleteCollisionTriangle {
        id: String,
    },
    SetEntrance {
        value: entrances::Entrance,
    },
}
#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Document {
    pub schema_version: u32,
    pub input: bundle::Input,
    #[serde(deserialize_with = "event_document::from_json")]
    pub event_document: Box<draft::Document>,
    pub entrance_selectors: Vec<u16>,
    pub actions: Vec<Action>,
    pub allocation_starts: Vec<u32>,
    pub cursor: u32,
    pub next_id: u32,
    pub event_allocation_starts: Vec<u32>,
    pub next_event_id: u32,
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
    collision: entities::Collision,
    entrances: Vec<entrances::Entrance>,
    events: Arc<event_state::Events>,
}
struct Base {
    source: bundle::Source,
    collision: collision::Collision,
    event_actor_count: usize,
    event_modified: bool,
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
    event_allocation_starts: Vec<u32>,
    next_event_id: u32,
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
    pub collision_entities: entities::Collision,
    pub entrances: Vec<entrances::Entrance>,
    pub source_sha256: String,
    pub events: draft::Compiled,
    pub next_event_id: u32,
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
        Self::open_with_events(input, entrance_selectors, None)
    }
    pub fn open_with_events(
        input: bundle::Input,
        entrance_selectors: Vec<u16>,
        event_document: Option<Box<draft::Document>>,
    ) -> Result<Self, FieldError> {
        let source = bundle::Source::load(input)?;
        let c = &source.components;
        let original_events = field::decode(&c[5].bytes, Some(c[5].logical_size))?;
        let event_draft = event_document::open(&original_events, event_document)?;
        let event_modified = event_draft.document().cursor > 0;
        let events = event_draft.compile()?.graph;
        let next_event_id = event_draft.next_node_id();
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
        let descriptor_ids: Vec<_> = (0..source.header.descriptors.len())
            .map(|i| format!("descriptor:{i}"))
            .collect();
        let zone_ids: Vec<_> = (0..zones.zones.len())
            .map(|i| format!("zone:{i}"))
            .collect();
        let authored_events = Arc::new(event_state::Events::open(
            event_draft,
            &descriptor_ids,
            &zone_ids,
        )?);
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
            collision: entities::Collision::from_source(&collision),
            entrances: entrance_records,
            events: authored_events,
        };
        Ok(Self {
            base: Arc::new(Base {
                source,
                collision,
                event_actor_count: events.actors.len(),
                event_modified,
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
            event_allocation_starts: Vec::new(),
            next_event_id,
        })
    }
    pub fn revision(&self) -> u32 {
        self.revision
    }
    pub fn snapshot(&self) -> Result<Snapshot, FieldError> {
        Ok(Snapshot {
            revision: self.revision,
            can_undo: self.cursor > 0,
            can_redo: self.cursor < self.actions.len(),
            modified: self.cursor > 0 || self.base.event_modified,
            descriptors: self.state.descriptors.clone(),
            zones: self.state.zones.clone(),
            collision: self.state.collision.resolve(&self.base.collision)?,
            collision_entities: self.state.collision.clone(),
            entrances: self.state.entrances.clone(),
            source_sha256: self.base.source.header.source_sha256.clone(),
            events: self.compile_events(&self.state)?,
            next_event_id: self.next_event_id,
        })
    }
    pub fn document(&self) -> Document {
        Document {
            schema_version: 3,
            input: self.base.source.input().clone(),
            event_document: Box::new(self.base.initial.events.draft.document()),
            entrance_selectors: self.base.entrance_selectors.clone(),
            actions: self.actions.clone(),
            allocation_starts: self.allocation_starts.clone(),
            cursor: self.cursor as u32,
            next_id: self.next_id,
            event_allocation_starts: self.event_allocation_starts.clone(),
            next_event_id: self.next_event_id,
        }
    }
    pub fn import(document: Document) -> Result<Self, FieldError> {
        let cursor = document.cursor as usize;
        if document.schema_version != 3
            || document.actions.len() > HISTORY_LIMIT
            || cursor > document.actions.len()
            || document.actions.len() != document.allocation_starts.len()
            || document.next_id == 0
            || document.actions.len() != document.event_allocation_starts.len()
            || document.next_event_id == 0
        {
            return Err(invalid("invalid scene document schema/history"));
        }
        let mut result = Self::open_with_events(
            document.input,
            document.entrance_selectors,
            Some(document.event_document),
        )?;
        for ((action, start), event_start) in document
            .actions
            .into_iter()
            .zip(document.allocation_starts)
            .zip(document.event_allocation_starts)
        {
            if start < result.next_id || event_start < result.next_event_id {
                return Err(invalid("scene history reuses allocated identities"));
            }
            result.next_id = start;
            result.next_event_id = event_start;
            result.edit(result.revision, action)?;
        }
        if document.next_id < result.next_id || document.next_event_id < result.next_event_id {
            return Err(invalid("scene watermark reuses allocated identities"));
        }
        result.next_id = document.next_id;
        result.next_event_id = document.next_event_id;
        result.state = result.replay(cursor)?;
        result.cursor = cursor;
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
        let mut next_event_id = self.next_event_id;
        self.apply(&mut candidate, &action, &mut next_id, &mut next_event_id)?;
        self.compile_state(&candidate)?;
        // No state, history, revision or allocator mutation precedes validation.
        self.actions.truncate(self.cursor);
        self.allocation_starts.truncate(self.cursor);
        self.event_allocation_starts.truncate(self.cursor);
        self.actions.push(action);
        self.allocation_starts.push(self.next_id);
        self.event_allocation_starts.push(self.next_event_id);
        self.state = candidate;
        self.next_id = next_id;
        self.next_event_id = next_event_id;
        self.cursor += 1;
        self.revision = next_revision;
        Ok(())
    }
    fn replay(&self, cursor: usize) -> Result<State, FieldError> {
        let mut state = self.base.initial.clone();
        for ((action, start), event_start) in self.actions[..cursor]
            .iter()
            .zip(&self.allocation_starts[..cursor])
            .zip(&self.event_allocation_starts[..cursor])
        {
            let mut next = *start;
            let mut next_event = *event_start;
            self.apply(&mut state, action, &mut next, &mut next_event)?;
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
        if at < self.base.event_actor_count || state.descriptors[at].value.flags & 0x40 != 0 {
            return Err(unsupported(
                "actor CRUD requires a reference-safe event-row transaction, not instance CRUD",
            ));
        }
        Ok(at)
    }
    fn apply(
        &self,
        state: &mut State,
        action: &Action,
        next: &mut u32,
        next_event: &mut u32,
    ) -> Result<(), FieldError> {
        match action {
            Action::Batch { actions } => {
                if actions.is_empty()
                    || actions.len() > 32
                    || actions.iter().any(|a| matches!(a, Action::Batch { .. }))
                {
                    return Err(invalid("scene batch requires 1..32 non-batch actions"));
                }
                for action in actions {
                    self.apply(state, action, next, next_event)?;
                }
            }
            Action::Event { action } => {
                let descriptors = entity_ids(&state.descriptors);
                let zones = entity_ids(&state.zones);
                let events = Arc::make_mut(&mut state.events);
                events.draft.advance_allocator(*next_event)?;
                events.edit((**action).clone(), &descriptors, &zones)?;
                *next_event = events.draft.next_node_id();
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
                state
                    .collision
                    .vertex_layer(id)?
                    .set_vertex(id, *position)?;
            }
            Action::AddCollisionVertex { layer, position } => {
                state
                    .collision
                    .layer(layer)?
                    .add_vertex(new_id(next, "collision_vertex")?, *position)?;
            }
            Action::DuplicateCollisionVertex { id } => {
                let layer = state.collision.vertex_layer(id)?;
                let value = layer
                    .vertices
                    .iter()
                    .find(|v| v.id == *id)
                    .ok_or_else(|| invalid("collision vertex not found"))?
                    .value;
                layer.vertices.push(Entity {
                    id: new_id(next, "collision_vertex")?,
                    value,
                });
            }
            Action::DeleteCollisionVertex { id } => {
                state.collision.vertex_layer(id)?.delete_vertex(id)?;
            }
            Action::AddCollisionTriangle {
                layer,
                vertices,
                attribute,
                height_bump_raw,
            } => {
                state.collision.layer(layer)?.add_triangle(
                    new_id(next, "collision_triangle")?,
                    vertices.clone(),
                    *attribute,
                    *height_bump_raw,
                )?;
            }
            Action::DuplicateCollisionTriangle { id } => {
                let layer = state.collision.triangle_layer(id)?;
                let face = layer
                    .triangles
                    .iter()
                    .find(|t| t.id == *id)
                    .ok_or_else(|| invalid("collision triangle not found"))?
                    .value
                    .clone();
                let mut vertices: [String; 3] = std::array::from_fn(|_| String::new());
                for (target, original) in vertices.iter_mut().zip(&face.vertices) {
                    let value = layer
                        .vertices
                        .iter()
                        .find(|v| v.id == *original)
                        .ok_or_else(|| invalid("collision source triangle has a dangling vertex"))?
                        .value;
                    *target = new_id(next, "collision_vertex")?;
                    layer.vertices.push(Entity {
                        id: target.clone(),
                        value,
                    });
                }
                layer.add_triangle(
                    new_id(next, "collision_triangle")?,
                    vertices,
                    face.attribute,
                    face.height_bump_raw,
                )?;
            }
            Action::DeleteCollisionTriangle { id } => {
                state.collision.triangle_layer(id)?.delete_triangle(id)?;
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
    fn compile_events(&self, state: &State) -> Result<draft::Compiled, FieldError> {
        let descriptor_ids = entity_ids(&state.descriptors);
        let zone_ids = entity_ids(&state.zones);
        // field_event_begin_effect reads iv@1 as a raw effect owner, stores it
        // in field_effect_launch.actor and emitter.unk52, then field_effect.c
        // indexes descriptors with it. It is not an actor-selector byte. Keep
        // this source-proven indirect dependency explicit until its variable /
        // 0xff dual semantics have qualified stable bindings.
        if descriptor_ids != entity_ids(&self.base.initial.descriptors)
            && state
                .events
                .draft
                .compile()?
                .graph
                .instructions
                .iter()
                .any(|i| i.spec.handler == "field_event_begin_effect")
        {
            return Err(FieldError::new(
                "unsupported_scene_reference",
                "field_event_begin_effect has raw/variable effect-owner descriptor indices; structural descriptor edits require qualified effect reference closure",
            ));
        }
        let mut events = state.events.compile(&descriptor_ids, &zone_ids)?;
        if !state.entrances.is_empty() {
            events.bytes = entrances::encode_entrances(
                &events.graph,
                &state.entrances,
                state.collision.layers.len(),
            )?;
            events.graph = field::decode(&events.bytes, Some(events.declared_size))?;
        }
        Ok(events)
    }
    fn compile_state(&self, state: &State) -> Result<Compiled, FieldError> {
        let descriptor_ids = entity_ids(&state.descriptors);
        let zone_ids = entity_ids(&state.zones);
        let events = self.compile_events(state)?;
        let mut header = self.base.source.header.clone();
        header.descriptors = state.descriptors.iter().map(|e| e.value.clone()).collect();
        let collision_value = state.collision.resolve(&self.base.collision)?;
        let structural = state
            .collision
            .layers
            .iter()
            .zip(&self.base.initial.collision.layers)
            .any(|(a, b)| {
                entity_ids(&a.vertices) != entity_ids(&b.vertices)
                    || entity_ids(&a.triangles) != entity_ids(&b.triangles)
            });
        let (collision, collision_logical) = if structural {
            topology::rebuild(&self.base.source.components[1].bytes, &collision_value)?
        } else {
            (
                collision::encode_collision(
                    &self.base.source.components[1].bytes,
                    &collision_value,
                )?,
                self.base.collision.logical_size,
            )
        };
        let zones = formats::encode_zones(&formats::Zones {
            zones: state.zones.iter().map(|e| e.value.clone()).collect(),
            tail: self.base.zones_tail.clone(),
        })?;
        let mut replacements = BTreeMap::new();
        replacements.insert(
            1,
            bundle::Replacement {
                logical_size: collision_logical,
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
