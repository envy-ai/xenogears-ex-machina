//! Typed scene-document commands through the existing Studio JSON envelope.
//! Separate handles keep an event draft from being mistaken for a scene owner.
use super::{bundle, domain};
use crate::field::FieldError;
use serde::Deserialize;
use serde_json::{Value, json};
use std::collections::BTreeMap;

// Full scene sources are larger than event components. This is an explicit
// service residency budget, not an original scene/entity maximum.
const MAX_SCENES: usize = 4;
#[derive(Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum Request {
    Open {
        input: Box<bundle::Input>,
        entrances: Vec<u16>,
    },
    Import {
        document: Box<domain::Document>,
    },
    Edit {
        scene: u32,
        revision: u32,
        action: domain::Action,
    },
    Undo {
        scene: u32,
        revision: u32,
    },
    Redo {
        scene: u32,
        revision: u32,
    },
    Snapshot {
        scene: u32,
    },
    Compile {
        scene: u32,
    },
    Export {
        scene: u32,
    },
    Close {
        scene: u32,
    },
}
#[derive(Default)]
pub struct Scenes {
    next: u32,
    items: BTreeMap<u32, domain::Draft>,
}
impl Scenes {
    fn admission(&self) -> Result<u32, FieldError> {
        if self.items.len() >= MAX_SCENES {
            return Err(FieldError::new(
                "scene_resource_limit",
                "at most 4 source-owned scene documents may be open; close an unused scene",
            ));
        }
        self.next
            .checked_add(1)
            .ok_or_else(|| FieldError::new("scene_resource_limit", "scene handles exhausted"))
    }
    fn get(&self, id: u32) -> Result<&domain::Draft, FieldError> {
        self.items
            .get(&id)
            .ok_or_else(|| FieldError::new("unknown_scene", format!("scene {id} is not open")))
    }
    fn get_mut(&mut self, id: u32) -> Result<&mut domain::Draft, FieldError> {
        self.items
            .get_mut(&id)
            .ok_or_else(|| FieldError::new("unknown_scene", format!("scene {id} is not open")))
    }
    fn snapshot(&self, id: u32) -> Result<Value, FieldError> {
        Ok(json!({"scene":id,"state":self.get(id)?.snapshot()?}))
    }
    fn adopt(&mut self, draft: domain::Draft) -> Result<Value, FieldError> {
        let id = self.admission()?;
        let reply = json!({"scene":id,"state":draft.snapshot()?});
        self.items.insert(id, draft);
        self.next = id;
        Ok(reply)
    }
    pub fn execute(&mut self, request: Request) -> Result<Value, FieldError> {
        match request {
            Request::Open { input, entrances } => {
                self.admission()?;
                self.adopt(domain::Draft::open(*input, entrances)?)
            }
            Request::Import { document } => {
                self.admission()?;
                self.adopt(domain::Draft::import(*document)?)
            }
            Request::Edit {
                scene,
                revision,
                action,
            } => {
                self.get_mut(scene)?.edit(revision, action)?;
                self.snapshot(scene)
            }
            Request::Undo { scene, revision } => {
                self.get_mut(scene)?.undo(revision)?;
                self.snapshot(scene)
            }
            Request::Redo { scene, revision } => {
                self.get_mut(scene)?.redo(revision)?;
                self.snapshot(scene)
            }
            Request::Snapshot { scene } => self.snapshot(scene),
            Request::Compile { scene } => Ok(json!(self.get(scene)?.compile()?)),
            Request::Export { scene } => Ok(json!(self.get(scene)?.document())),
            Request::Close { scene } => {
                self.get(scene)?;
                self.items.remove(&scene);
                Ok(json!({"closed":scene}))
            }
        }
    }
}
