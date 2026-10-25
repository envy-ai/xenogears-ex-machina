//! Source-owned, revisioned scene transactions. The immutable container is never
//! replaced by a compiled result. History replays actions, not opaque byte blobs.
use super::{
    actors, bundle, clone_event, collision, entities, entrances, event_document, event_state,
    formats, geometry, placements, topology,
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
    AddActor {
        template: String,
        position: [u16; 3],
        rotation: [u16; 3],
    },
    DuplicateActor {
        id: String,
    },
    DeleteActor {
        id: String,
    },
    ActorEvent {
        actor: String,
        event: u8,
        #[serde(deserialize_with = "event_document::nodes_from_json")]
        nodes: Vec<draft::NodeInput>,
    },
    SetPlacement {
        change: placements::Change,
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
    pub imported_event_actions: u32,
}
#[derive(Clone, Debug, Serialize)]
pub struct Entity<T> {
    pub id: String,
    pub value: T,
}
#[derive(Clone)]
struct State {
    descriptors: Vec<Entity<formats::Descriptor>>,
    actors: Vec<actors::Actor>,
    zones: Vec<Entity<formats::Zone>>,
    collision: entities::Collision,
    entrances: Vec<entrances::Entrance>,
    events: Arc<event_state::Events>,
}
struct Base {
    source: bundle::Source,
    collision: collision::Collision,
    event_actor_count: usize,
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
    imported_event_actions: usize,
}
#[derive(Clone, Debug, Serialize)]
pub struct Snapshot {
    pub revision: u32,
    pub can_undo: bool,
    pub can_redo: bool,
    pub modified: bool,
    pub descriptors: Vec<Entity<formats::Descriptor>>,
    pub actors: Vec<actors::Actor>,
    pub zones: Vec<Entity<formats::Zone>>,
    pub collision: collision::Collision,
    pub collision_entities: entities::Collision,
    pub entrances: Vec<entrances::Entrance>,
    pub source_sha256: String,
    pub events: draft::Compiled,
    pub next_event_id: u32,
    pub reference_diagnostics: Vec<String>,
    pub sprite_bindings: Vec<super::sprites::Binding>,
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
        Self::open_root(input, entrance_selectors)
    }
    pub fn open_with_events(
        input: bundle::Input,
        entrance_selectors: Vec<u16>,
        event_document: Option<Box<draft::Document>>,
    ) -> Result<Self, FieldError> {
        let result = Self::open_root(input, entrance_selectors)?;
        let Some(document) = event_document else {
            return Ok(result);
        };
        // Validate all M6 actions against the actual original stream first.
        // Then place them on the one scene timeline, retaining exact IDs,
        // cursor/redo and reserved watermarks instead of hiding an editable base.
        let imported = event_document::open(
            result.base.initial.events.draft.source_graph(),
            Some(document),
        )?
        .document();
        let mut document = result.document();
        document.imported_event_actions = imported.actions.len() as u32;
        document.actions = imported
            .actions
            .into_iter()
            .map(|action| Action::Event {
                action: Box::new(action),
            })
            .collect();
        document.allocation_starts = vec![1; document.actions.len()];
        document.event_allocation_starts = imported.allocation_starts;
        document.cursor = imported.cursor as u32;
        document.next_event_id = imported.next_id;
        Self::import(document)
    }
    fn open_root(input: bundle::Input, entrance_selectors: Vec<u16>) -> Result<Self, FieldError> {
        let source = bundle::Source::load(input)?;
        let c = &source.components;
        let original_events = field::decode(&c[5].bytes, Some(c[5].logical_size))?;
        let event_draft = draft::Draft::open(original_events)?;
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
            actors: actors::initial(events.actors.len()),
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
            imported_event_actions: 0,
        })
    }
    pub fn revision(&self) -> u32 {
        self.revision
    }
    pub fn snapshot(&self) -> Result<Snapshot, FieldError> {
        let events = self.compile_events(&self.state)?;
        let reference_diagnostics = self.state.events.reference_diagnostics(
            &events,
            &entity_ids(&self.state.descriptors),
            &entity_ids(&self.state.zones),
        )?;
        Ok(Snapshot {
            revision: self.revision,
            can_undo: self.cursor > 0,
            can_redo: self.cursor < self.actions.len(),
            modified: self.cursor > 0,
            descriptors: self.state.descriptors.clone(),
            actors: self.state.actors.clone(),
            zones: self.state.zones.clone(),
            collision: self.state.collision.resolve(&self.base.collision)?,
            collision_entities: self.state.collision.clone(),
            entrances: self.state.entrances.clone(),
            source_sha256: self.base.source.header.source_sha256.clone(),
            sprite_bindings: super::sprites::bindings(
                &events.graph,
                &self
                    .state
                    .actors
                    .iter()
                    .map(|a| a.id.clone())
                    .collect::<Vec<_>>(),
                &events.bindings,
            ),
            events,
            next_event_id: self.next_event_id,
            reference_diagnostics,
        })
    }
    pub fn document(&self) -> Document {
        Document {
            schema_version: 4,
            input: self.base.source.input().clone(),
            event_document: Box::new(self.base.initial.events.draft.document()),
            entrance_selectors: self.base.entrance_selectors.clone(),
            actions: self.actions.clone(),
            allocation_starts: self.allocation_starts.clone(),
            cursor: self.cursor as u32,
            next_id: self.next_id,
            event_allocation_starts: self.event_allocation_starts.clone(),
            next_event_id: self.next_event_id,
            imported_event_actions: self.imported_event_actions as u32,
        }
    }
    pub fn import(document: Document) -> Result<Self, FieldError> {
        let cursor = document.cursor as usize;
        let imported = document.imported_event_actions as usize;
        if document.schema_version != 4
            || document.actions.len() > HISTORY_LIMIT
            || cursor > document.actions.len()
            || document.actions.len() != document.allocation_starts.len()
            || document.next_id == 0
            || document.actions.len() != document.event_allocation_starts.len()
            || document.next_event_id == 0
            || imported > document.actions.len()
            || document
                .actions
                .iter()
                .take(imported)
                .any(|a| !matches!(a, Action::Event { .. }))
            || document
                .allocation_starts
                .iter()
                .take(imported)
                .any(|s| *s != 1)
            || document.event_document.schema_version != 1
            || !document.event_document.actions.is_empty()
            || !document.event_document.allocation_starts.is_empty()
            || document.event_document.cursor != 0
            || document.event_document.next_id != 1
        {
            return Err(invalid("invalid scene document schema/history"));
        }
        let mut result = Self::open_root(document.input, document.entrance_selectors)?;
        event_document::open(
            result.base.initial.events.draft.source_graph(),
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
            let source_history = result.actions.len() < imported;
            result.edit_recorded(result.revision, action, source_history)?;
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
        self.edit_recorded(revision, action, false)
    }
    fn edit_recorded(
        &mut self,
        revision: u32,
        action: Action,
        source_history: bool,
    ) -> Result<(), FieldError> {
        let next_revision = self.next_revision(revision)?;
        if self.cursor >= HISTORY_LIMIT {
            return Err(invalid("scene history limit is 128 applied transactions"));
        }
        let mut candidate = self.state.clone();
        let mut next_id = self.next_id;
        let mut next_event_id = self.next_event_id;
        self.apply_recorded(
            &mut candidate,
            &action,
            &mut next_id,
            &mut next_event_id,
            source_history,
        )?;
        self.compile_state(&candidate)?;
        // No state, history, revision or allocator mutation precedes validation.
        self.imported_event_actions = self.imported_event_actions.min(self.cursor);
        if source_history {
            self.imported_event_actions = self.cursor + 1;
        }
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
        for (at, ((action, start), event_start)) in self.actions[..cursor]
            .iter()
            .zip(&self.allocation_starts[..cursor])
            .zip(&self.event_allocation_starts[..cursor])
            .enumerate()
        {
            let mut next = *start;
            let mut next_event = *event_start;
            self.apply_recorded(
                &mut state,
                action,
                &mut next,
                &mut next_event,
                at < self.imported_event_actions,
            )?;
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
    fn apply_recorded(
        &self,
        state: &mut State,
        action: &Action,
        next: &mut u32,
        next_event: &mut u32,
        source_history: bool,
    ) -> Result<(), FieldError> {
        if !source_history {
            return self.apply(state, action, next, next_event);
        }
        let Action::Event { action } = action else {
            return Err(invalid(
                "imported M6 history prefix must contain only event transactions",
            ));
        };
        let descriptors = entity_ids(&state.descriptors);
        let zones = entity_ids(&state.zones);
        if descriptors != entity_ids(&self.base.initial.descriptors)
            || zones != entity_ids(&self.base.initial.zones)
            || state
                .actors
                .iter()
                .enumerate()
                .any(|(i, a)| a.event_source_row as usize != i || a.id != format!("descriptor:{i}"))
            || state.actors.len() != self.base.event_actor_count
        {
            return Err(invalid(
                "imported M6 history requires the unchanged original entity namespace",
            ));
        }
        self.validate_source_event_action(state, action)?;
        let events = Arc::make_mut(&mut state.events);
        events.draft.advance_allocator(*next_event)?;
        events.edit_source_history((**action).clone(), &descriptors, &zones)?;
        *next_event = events.draft.next_node_id();
        Ok(())
    }
    fn static_instance(&self, state: &State, id: &str) -> Result<usize, FieldError> {
        let at = index(&state.descriptors, id)?;
        if at < state.actors.len() || state.descriptors[at].value.flags & 0x40 != 0 {
            return Err(unsupported(
                "actor CRUD requires a reference-safe event-row transaction, not instance CRUD",
            ));
        }
        Ok(at)
    }
    fn actor_at(state: &State, id: &str) -> Result<usize, FieldError> {
        state
            .actors
            .iter()
            .position(|a| a.id == id)
            .ok_or_else(|| invalid(format!("actor {id} is not active")))
    }
    fn validate_source_event_action(
        &self,
        state: &State,
        action: &draft::Action,
    ) -> Result<(), FieldError> {
        match action {
            draft::Action::Batch { actions } => {
                for action in actions {
                    self.validate_source_event_action(state, action)?;
                }
            }
            draft::Action::NewEvent { actor, .. }
                if *actor >= self.base.event_actor_count
                    || !state
                        .actors
                        .iter()
                        .any(|a| a.event_source_row as usize == *actor) =>
            {
                return Err(unsupported(
                    "new scene actor events require the stable actor_event action; numeric M6 rows address active original source actors only",
                ));
            }
            _ => {}
        }
        Ok(())
    }
    fn edit_events(
        state: &mut State,
        action: draft::Action,
        next_event: &mut u32,
        descriptors: &[String],
        zones: &[String],
    ) -> Result<(), FieldError> {
        let events = Arc::make_mut(&mut state.events);
        events.draft.advance_allocator(*next_event)?;
        events.edit(action, descriptors, zones)?;
        *next_event = events.draft.next_node_id();
        Ok(())
    }
    fn add_actor(
        &self,
        state: &mut State,
        template: &str,
        transform: Option<([u16; 3], [u16; 3])>,
        next: &mut u32,
        next_event: &mut u32,
    ) -> Result<(), FieldError> {
        if self.base.event_actor_count > usize::from(u8::MAX) {
            return Err(unsupported(
                "generic actor CRUD on wider original tables needs additional runtime byte-link qualification; original decoding remains supported",
            ));
        }
        actors::runtime_index(state.actors.len())?;
        let at = Self::actor_at(state, template)?;
        let compiled = self.compile_events(state)?;
        actors::qualify_template(&compiled.graph, at)?;
        let before_descriptors = entity_ids(&state.descriptors);
        let before_zones = entity_ids(&state.zones);
        let mut value = state.descriptors[at].value.clone();
        if let Some((position, rotation)) = transform {
            if value.flags & 0x40 != 0 && value.rotation != rotation {
                return Err(unsupported(
                    "sprite rotation is reset by the loader; use an independent script-facing binding",
                ));
            }
            value.position = position;
            value.rotation = rotation;
        }
        let row = Arc::make_mut(&mut state.events).extend_actor_source()?;
        let mut actions = Vec::new();
        let mut watermark = *next_event;
        for event in 0..32 {
            let entry = usize::from(compiled.graph.actors[at][usize::from(event)]);
            if compiled.graph.instructions.iter().any(|i| {
                i.pc == entry
                    && i.spec.handler == "field_event_end_slot"
                    && i.spec.operands.is_empty()
                    && i.size == 1
            }) {
                // Keep terminal-only slots empty so the stable actor can
                // receive a genuinely new authored event later.
                continue;
            }
            if let Some(prepared) =
                clone_event::prepare(&compiled.graph, at as u32, event, watermark)?
            {
                watermark = prepared.next_id;
                actions.push(draft::Action::NewEvent {
                    actor: row as usize,
                    event: usize::from(event),
                    nodes: prepared.nodes,
                });
            }
        }
        if !actions.is_empty() {
            // Copied bytes already contain current namespace repairs. Capture
            // their literals before inserting the new descriptor into its actor
            // prefix, so shifted static indices cannot silently rebind a copy.
            Self::edit_events(
                state,
                draft::Action::Batch { actions },
                next_event,
                &before_descriptors,
                &before_zones,
            )?;
        }
        if *next_event != watermark {
            return Err(invalid(
                "actor script copy allocation differs from prepared stable identities",
            ));
        }
        let id = new_id(next, "actor")?;
        let insert = state.actors.len();
        state.descriptors.insert(
            insert,
            Entity {
                id: id.clone(),
                value,
            },
        );
        state.actors.push(actors::Actor {
            id,
            event_source_row: row,
            origin: actors::Origin::IndependentCopy {
                template: template.into(),
            },
        });
        Ok(())
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
                self.validate_source_event_action(state, action)?;
                Self::edit_events(state, (**action).clone(), next_event, &descriptors, &zones)?;
            }
            Action::AddActor {
                template,
                position,
                rotation,
            } => self.add_actor(
                state,
                template,
                Some((*position, *rotation)),
                next,
                next_event,
            )?,
            Action::DuplicateActor { id } => self.add_actor(state, id, None, next, next_event)?,
            Action::DeleteActor { id } => {
                if self.base.event_actor_count > usize::from(u8::MAX) {
                    return Err(unsupported(
                        "generic actor CRUD on wider original tables needs runtime byte-link qualification",
                    ));
                }
                let at = Self::actor_at(state, id)?;
                if at == 0 {
                    return Err(unsupported(
                        "original actor zero retains engine-owned events2/3 and cannot be deleted",
                    ));
                }
                let compiled = self.compile_events(state)?;
                actors::qualify_template(&compiled.graph, at)?;
                state.actors.remove(at);
                state.descriptors.remove(at);
            }
            Action::ActorEvent {
                actor,
                event,
                nodes,
            } => {
                let at = Self::actor_at(state, actor)?;
                let row = state.actors[at].event_source_row as usize;
                let descriptors = entity_ids(&state.descriptors);
                let zones = entity_ids(&state.zones);
                Self::edit_events(
                    state,
                    draft::Action::NewEvent {
                        actor: row,
                        event: usize::from(*event),
                        nodes: nodes.clone(),
                    },
                    next_event,
                    &descriptors,
                    &zones,
                )?;
            }
            Action::SetPlacement { change } => {
                let compiled = self.compile_events(state)?;
                let actors = state
                    .actors
                    .iter()
                    .map(|a| a.id.clone())
                    .collect::<Vec<_>>();
                let action = placements::prepare(
                    &compiled,
                    &state.events.draft,
                    &actors,
                    state.collision.layers.len(),
                    change,
                )?;
                self.apply(
                    state,
                    &Action::Event {
                        action: Box::new(action),
                    },
                    next,
                    next_event,
                )?;
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
    pub fn placement(&self, node: &str) -> Result<placements::Binding, FieldError> {
        let compiled = self.compile_events(&self.state)?;
        let actors = self
            .state
            .actors
            .iter()
            .map(|a| a.id.clone())
            .collect::<Vec<_>>();
        placements::describe(
            &compiled,
            &self.state.events.draft,
            node,
            &actors,
            self.state.collision.layers.len(),
        )
    }
    pub fn compile(&self) -> Result<Compiled, FieldError> {
        self.compile_state(&self.state)
    }
    fn compile_events(&self, state: &State) -> Result<draft::Compiled, FieldError> {
        let descriptor_ids = entity_ids(&state.descriptors);
        let zone_ids = entity_ids(&state.zones);
        if state.actors.len() > state.descriptors.len()
            || state
                .actors
                .iter()
                .zip(&state.descriptors)
                .any(|(a, d)| a.id != d.id)
        {
            return Err(invalid(
                "active actor rows and descriptor prefix identities differ",
            ));
        }
        let mut events =
            state
                .events
                .compile(&descriptor_ids, &zone_ids, &actors::rows(&state.actors))?;
        // field_event_begin_effect reads iv@1 as a raw effect owner, stores it
        // in field_effect_launch.actor and emitter.unk52, then field_effect.c
        // indexes descriptors with it. It is not an actor-selector byte. Keep
        // this source-proven indirect dependency explicit until its variable /
        // 0xff dual semantics have qualified stable bindings.
        if descriptor_ids != entity_ids(&self.base.initial.descriptors)
            && events
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
