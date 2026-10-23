//! Revision-checked authored transactions over the immutable M2 field graph.
//! Compiler output is derived; it never becomes the editable source graph.
use super::{FieldError, FieldGraph, Instruction, encode};
use serde::{Deserialize, Serialize, de};
use std::collections::BTreeMap;

mod relocate;
const MAX_HISTORY: usize = 128;
const MAX_DRAFTS: usize = 16;
pub(crate) const MAX_EVENT_NODES: usize = 1024;

pub(super) fn error(message: impl Into<String>) -> FieldError {
    FieldError::new("invalid_edit", message)
}
pub(super) fn unsupported(message: impl Into<String>) -> FieldError {
    FieldError::new("unsupported_relocation", message)
}

// Tagged command payloads buffer JSON object keys as strings. Parse operand
// indices explicitly, as names::NamesContext does for numeric map IDs.
fn targets_from_json<'de, D: de::Deserializer<'de>>(
    deserializer: D,
) -> Result<BTreeMap<usize, String>, D::Error> {
    BTreeMap::<String, String>::deserialize(deserializer)?
        .into_iter()
        .map(|(key, node)| {
            let index = key.parse::<usize>().map_err(de::Error::custom)?;
            if key != index.to_string() {
                return Err(de::Error::custom(
                    "target keys must be canonical decimal operand indices",
                ));
            }
            Ok((index, node))
        })
        .collect()
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum OperandValue {
    Literal { value: u32, immediate: Option<bool> },
    Target { node: String },
}
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct NodeInput {
    pub bytes: Vec<u8>,
    #[serde(default, deserialize_with = "targets_from_json")]
    pub targets: BTreeMap<usize, String>,
}
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum Action {
    Batch {
        actions: Vec<Action>,
    },
    SetOperand {
        node: String,
        operand: usize,
        value: OperandValue,
    },
    InsertAfter {
        after: String,
        bytes: Vec<u8>,
        #[serde(default, deserialize_with = "targets_from_json")]
        targets: BTreeMap<usize, String>,
    },
    NewEvent {
        actor: usize,
        event: usize,
        nodes: Vec<NodeInput>,
    },
}
#[derive(Clone, Debug)]
pub(super) struct Node {
    pub id: String,
    pub source_pc: Option<usize>,
    pub bytes: Vec<u8>,
    pub targets: BTreeMap<usize, String>,
}
#[derive(Clone, Debug, Default)]
pub(super) struct State {
    pub nodes: BTreeMap<String, Node>,
    pub after: BTreeMap<String, Vec<String>>,
    pub events: BTreeMap<(usize, usize), Vec<String>>,
}
#[derive(Clone, Debug, Serialize)]
pub struct Compiled {
    pub bytes: Vec<u8>,
    pub declared_size: usize,
    pub graph: FieldGraph,
    pub bindings: BTreeMap<String, usize>,
}
#[derive(Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Document {
    pub schema_version: u32,
    pub base: FieldGraph,
    pub actions: Vec<Action>,
    pub cursor: usize,
    pub allocation_starts: Vec<u32>,
    pub next_id: u32,
}

#[derive(Clone)]
pub struct Draft {
    pub(super) base: FieldGraph,
    pub(super) bytes: Vec<u8>,
    history: Vec<State>,
    actions: Vec<Action>,
    allocation_starts: Vec<u32>,
    cursor: usize,
    revision: u32,
    next_id: u32,
}

fn original_id(pc: usize) -> String {
    format!("pc:{pc}")
}
fn canonical_node(base: &FieldGraph, bytes: &[u8], ins: &Instruction) -> Node {
    Node {
        id: original_id(ins.pc),
        source_pc: Some(ins.pc),
        bytes: ins
            .ranges
            .iter()
            .flat_map(|&[start, end]| {
                bytes[base.bytecode_offset + start..base.bytecode_offset + end]
                    .iter()
                    .copied()
            })
            .collect(),
        targets: ins
            .spec
            .operands
            .iter()
            .enumerate()
            .filter(|(index, operand)| {
                operand.kind == "addr"
                    && base
                        .instructions
                        .binary_search_by_key(&(ins.operands[*index] as usize), |instruction| {
                            instruction.pc
                        })
                        .is_ok()
            })
            .map(|(index, _)| (index, original_id(ins.operands[index] as usize)))
            .collect(),
    }
}
impl Draft {
    pub fn open(base: FieldGraph) -> Result<Self, FieldError> {
        let bytes = encode(&base)?;
        let state = State {
            nodes: base
                .instructions
                .iter()
                .map(|ins| {
                    let node = canonical_node(&base, &bytes, ins);
                    (node.id.clone(), node)
                })
                .collect(),
            ..State::default()
        };
        Ok(Self {
            base,
            bytes,
            history: vec![state],
            actions: vec![],
            allocation_starts: vec![],
            cursor: 0,
            revision: 0,
            next_id: 1,
        })
    }
    pub(crate) fn revision(&self) -> u32 {
        self.revision
    }
    pub(crate) fn next_node_id(&self) -> u32 {
        self.next_id
    }
    /// Scene history may have allocated IDs on an undone branch. Reserve its
    /// retained watermark before the next M6 transaction without replaying old
    /// actions under new identities.
    pub(crate) fn advance_allocator(&mut self, next: u32) -> Result<(), FieldError> {
        if next == 0 || next < self.next_id {
            return Err(error(
                "scene event allocation watermark reuses node identities",
            ));
        }
        self.next_id = next;
        Ok(())
    }
    pub fn compile(&self) -> Result<Compiled, FieldError> {
        relocate::compile(self, &self.history[self.cursor], self.base.actors.len())
    }
    /// Scene compilation repairs only source-proven literal references. Reuse
    /// this compiler's writable spans, form validation and stable address repair;
    /// this private adapter does not add a second instruction encoder or a public
    /// relaxation of the M6 actor-table bound. A descriptor-only original object
    /// may have a transform without an event actor row.
    pub(crate) fn compile_with_scene_operands(
        &self,
        patches: &[(String, usize, u8)],
        descriptor_count: usize,
    ) -> Result<Compiled, FieldError> {
        if descriptor_count < self.base.actors.len() || descriptor_count > usize::from(u16::MAX) {
            return Err(error(
                "scene descriptor count cannot contain event actor rows",
            ));
        }
        if patches.is_empty() {
            return self.compile();
        }
        let compiled = self.compile()?;
        let mut state = self.history[self.cursor].clone();
        let mut seen = std::collections::BTreeSet::new();
        for (id, operand, value) in patches {
            if !seen.insert((id, operand)) {
                return Err(error("duplicate scene operand repair"));
            }
            let pc = compiled
                .bindings
                .get(id)
                .ok_or_else(|| error("scene repair names an unknown node"))?;
            let ins = compiled
                .graph
                .instructions
                .iter()
                .find(|ins| ins.pc == *pc)
                .ok_or_else(|| error("scene repair node is not reachable"))?;
            let descriptor = ins
                .spec
                .operands
                .get(*operand)
                .ok_or_else(|| error("scene repair operand outside instruction"))?;
            if !["actor", "u8"].contains(&descriptor.kind.as_str()) {
                return Err(error("scene reference repair requires a byte selector"));
            }
            let selected = state
                .nodes
                .get_mut(id)
                .ok_or_else(|| error("scene repair node missing"))?;
            if let Some(pc) = selected.source_pc {
                self.writable(pc)?;
            }
            write_operand(
                &mut selected.bytes,
                &mut selected.targets,
                ins,
                *operand,
                &OperandValue::Literal {
                    value: u32::from(*value),
                    immediate: None,
                },
            )?;
        }
        relocate::compile(self, &state, descriptor_count)
    }
    pub fn document(&self) -> Document {
        Document {
            schema_version: 1,
            base: self.base.clone(),
            actions: self.actions.clone(),
            cursor: self.cursor,
            allocation_starts: self.allocation_starts.clone(),
            next_id: self.next_id,
        }
    }
    pub fn import(document: Document) -> Result<Self, FieldError> {
        if document.schema_version != 1
            || document.actions.len() > MAX_HISTORY
            || document.cursor > document.actions.len()
            || document.allocation_starts.len() != document.actions.len()
        {
            return Err(error("invalid draft document schema/history"));
        }
        let mut draft = Self::open(document.base)?;
        for (action, start) in document.actions.into_iter().zip(document.allocation_starts) {
            if start < draft.next_id {
                return Err(error("draft allocation history reuses node identities"));
            }
            draft.next_id = start;
            draft.edit(draft.revision, action)?;
        }
        if document.next_id < draft.next_id {
            return Err(error(
                "draft next node identity reuses an allocated identity",
            ));
        }
        draft.next_id = document.next_id;
        while draft.cursor > document.cursor {
            draft.undo(draft.revision)?;
        }
        // Import establishes a new service identity/revision, not a continuation
        // of delayed requests against the exported handle.
        draft.revision = 0;
        Ok(draft)
    }
    fn check_revision(&self, revision: u32) -> Result<u32, FieldError> {
        if revision != self.revision {
            return Err(FieldError::new(
                "stale_revision",
                format!("expected revision {}, received {revision}", self.revision),
            ));
        }
        self.revision
            .checked_add(1)
            .ok_or_else(|| error("draft revision exhausted"))
    }
    pub fn undo(&mut self, revision: u32) -> Result<(), FieldError> {
        let next = self.check_revision(revision)?;
        if self.cursor == 0 {
            return Err(error("no edit to undo"));
        }
        self.cursor -= 1;
        self.revision = next;
        Ok(())
    }
    pub fn redo(&mut self, revision: u32) -> Result<(), FieldError> {
        let next = self.check_revision(revision)?;
        if self.cursor == self.actions.len() {
            return Err(error("no edit to redo"));
        }
        self.cursor += 1;
        self.revision = next;
        Ok(())
    }
    pub fn edit(&mut self, revision: u32, action: Action) -> Result<(), FieldError> {
        let next_revision = self.check_revision(revision)?;
        if self.cursor >= MAX_HISTORY {
            return Err(error(
                "draft history limit is 128 applied transactions; export preserves the project, and undo permits an alternate edit",
            ));
        }
        if let Action::Batch { actions } = &action {
            if actions.is_empty()
                || actions.len() > 32
                || actions
                    .iter()
                    .any(|action| matches!(action, Action::Batch { .. }))
            {
                return Err(error("batch requires 1..32 non-batch transactions"));
            }
            let mut candidate = self.clone();
            for step in actions {
                candidate.edit(candidate.revision, step.clone())?;
            }
            let final_state = candidate.history[candidate.cursor].clone();
            self.history.truncate(self.cursor + 1);
            self.actions.truncate(self.cursor);
            self.allocation_starts.truncate(self.cursor);
            self.history.push(final_state);
            self.actions.push(action);
            self.allocation_starts.push(self.next_id);
            self.next_id = candidate.next_id;
            self.cursor += 1;
            self.revision = next_revision;
            return Ok(());
        }
        let mut state = self.history[self.cursor].clone();
        let mut next_id = self.next_id;
        match &action {
            Action::Batch { .. } => unreachable!("batch handled before candidate dispatch"),
            Action::SetOperand {
                node,
                operand,
                value,
            } => {
                let compiled = self.compile()?;
                let pc = *compiled
                    .bindings
                    .get(node)
                    .ok_or_else(|| error(format!("unknown node {node}")))?;
                let ins = compiled
                    .graph
                    .instructions
                    .iter()
                    .find(|ins| ins.pc == pc)
                    .ok_or_else(|| error("selected node is not a reachable instruction"))?;
                let descriptor = ins
                    .spec
                    .operands
                    .get(*operand)
                    .ok_or_else(|| error("operand index outside instruction"))?;
                let selected = state
                    .nodes
                    .get_mut(node)
                    .ok_or_else(|| error("unknown selected node"))?;
                if let Some(pc) = selected.source_pc {
                    self.writable(pc)?;
                }
                let bound = match value {
                    OperandValue::Target { node: target } => {
                        if !compiled.bindings.contains_key(target) {
                            return Err(error("target names an unknown instruction"));
                        }
                        value.clone()
                    }
                    OperandValue::Literal { value, .. } if descriptor.kind == "addr" => {
                        let target = compiled
                            .bindings
                            .iter()
                            .find(|(_, pc)| **pc == *value as usize)
                            .map(|(id, _)| id.clone())
                            .ok_or_else(|| error("address operand must target an instruction"))?;
                        OperandValue::Target { node: target }
                    }
                    _ => value.clone(),
                };
                write_operand(
                    &mut selected.bytes,
                    &mut selected.targets,
                    ins,
                    *operand,
                    &bound,
                )?;
            }
            Action::InsertAfter {
                after,
                bytes,
                targets,
            } => {
                let anchor = state
                    .nodes
                    .get(after)
                    .ok_or_else(|| error("unknown insertion anchor"))?;
                if let Some(pc) = anchor.source_pc {
                    self.writable(pc)?;
                }
                let id = insert_node(
                    &mut state,
                    &mut next_id,
                    NodeInput {
                        bytes: bytes.clone(),
                        targets: targets.clone(),
                    },
                )?;
                state.after.entry(after.clone()).or_default().push(id);
            }
            Action::NewEvent {
                actor,
                event,
                nodes,
            } => {
                let row = self
                    .base
                    .actors
                    .get(*actor)
                    .ok_or_else(|| error("actor outside existing actor table"))?;
                let entry = *row
                    .get(*event)
                    .ok_or_else(|| error("event slot outside 0..31"))?
                    as usize;
                if state.events.contains_key(&(*actor, *event)) {
                    return Err(error("event slot already has authored code"));
                }
                let skipped =
                    entry == 0 && self.bytes.get(self.base.bytecode_offset) == Some(&0xff);
                let terminal = self
                    .base
                    .instructions
                    .iter()
                    .find(|ins| ins.pc == entry)
                    .is_some_and(|ins| {
                        matches!(ins.spec.flow.as_str(), "end" | "return")
                            && ins.spec.operands.is_empty()
                    });
                if !skipped && !terminal {
                    return Err(error(
                        "new event requires an explicitly selected absent or terminal-only slot",
                    ));
                }
                if nodes.is_empty() || nodes.len() > MAX_EVENT_NODES {
                    return Err(error("new event requires 1..1024 instructions"));
                }
                let ids = nodes
                    .iter()
                    .cloned()
                    .map(|input| insert_node(&mut state, &mut next_id, input))
                    .collect::<Result<Vec<_>, _>>()?;
                state.events.insert((*actor, *event), ids);
            }
        }
        relocate::compile(self, &state, self.base.actors.len())?; // Candidate validation happens before publishing any state or ID.
        self.history.truncate(self.cursor + 1);
        self.actions.truncate(self.cursor);
        self.allocation_starts.truncate(self.cursor);
        self.allocation_starts.push(self.next_id);
        self.history.push(state);
        self.actions.push(action);
        self.cursor += 1;
        self.next_id = next_id;
        self.revision = next_revision;
        Ok(())
    }
    /// Source-aware capability check for adapters. A compiled PC can have moved
    /// out of the original pinned span; retain the stable node's actual owner.
    pub(crate) fn check_writable_node(&self, id: &str) -> Result<(), FieldError> {
        let node = self.history[self.cursor]
            .nodes
            .get(id)
            .ok_or_else(|| error("unknown selected node"))?;
        if let Some(pc) = node.source_pc {
            self.writable(pc)?;
        }
        Ok(())
    }
    pub(super) fn writable(&self, pc: usize) -> Result<(), FieldError> {
        let block = self
            .base
            .blocks
            .iter()
            .find(|b| b.start <= pc && pc < b.end)
            .ok_or_else(|| error("instruction storage missing"))?;
        if block.kind == "pinned" {
            return Err(unsupported(format!(
                "PC +0x{pc:04x} is pinned: {}",
                block.reason.as_deref().unwrap_or("opaque storage")
            )));
        }
        Ok(())
    }
    pub fn snapshot(&self, handle: u32) -> Result<serde_json::Value, FieldError> {
        let compiled = self.compile()?;
        Ok(
            serde_json::json!({"draft":handle,"revision":self.revision,"graph":compiled.graph,"bindings":compiled.bindings,"can_undo":self.cursor>0,"can_redo":self.cursor<self.actions.len(),"modified":self.cursor>0}),
        )
    }
}
fn insert_node(state: &mut State, next: &mut u32, input: NodeInput) -> Result<String, FieldError> {
    if input.bytes.is_empty() || input.bytes.len() > 256 {
        return Err(error("instruction template requires 1..256 bytes"));
    }
    let id = format!("new:{next}");
    *next = next
        .checked_add(1)
        .ok_or_else(|| error("node identity exhausted"))?;
    state.nodes.insert(
        id.clone(),
        Node {
            id: id.clone(),
            source_pc: None,
            bytes: input.bytes,
            targets: input.targets,
        },
    );
    Ok(id)
}

#[derive(Default)]
pub struct Drafts {
    next: u32,
    items: BTreeMap<u32, Draft>,
}
impl Drafts {
    pub fn open(&mut self, graph: FieldGraph) -> Result<serde_json::Value, FieldError> {
        self.add(Draft::open(graph)?)
    }
    pub fn import(&mut self, document: Document) -> Result<serde_json::Value, FieldError> {
        self.add(Draft::import(document)?)
    }
    fn add(&mut self, draft: Draft) -> Result<serde_json::Value, FieldError> {
        if self.items.len() >= MAX_DRAFTS {
            return Err(error(
                "at most 16 drafts may be open; close an unused draft",
            ));
        }
        let id = self
            .next
            .checked_add(1)
            .ok_or_else(|| error("draft handles exhausted"))?;
        let reply = draft.snapshot(id)?;
        self.items.insert(id, draft);
        self.next = id;
        Ok(reply)
    }
    pub fn get(&self, id: u32) -> Result<&Draft, FieldError> {
        self.items
            .get(&id)
            .ok_or_else(|| FieldError::new("unknown_draft", format!("draft {id} is not open")))
    }
    pub fn get_mut(&mut self, id: u32) -> Result<&mut Draft, FieldError> {
        self.items
            .get_mut(&id)
            .ok_or_else(|| FieldError::new("unknown_draft", format!("draft {id} is not open")))
    }
    pub fn close(&mut self, id: u32) -> Result<(), FieldError> {
        self.items
            .remove(&id)
            .ok_or_else(|| FieldError::new("unknown_draft", format!("draft {id} is not open")))?;
        Ok(())
    }
}

/// Public authoring templates are derived from the same opcode facts as decode.
/// No second opcode registry or game bytes are introduced by this palette.
pub fn templates() -> Result<serde_json::Value, FieldError> {
    let metadata = super::opcodes::metadata()?;
    let mut result = Vec::new();
    for (extended, table) in [(false, &metadata.primary), (true, &metadata.extended)] {
        for (&opcode, forms) in table {
            for (form_index, form) in forms.iter().enumerate() {
                if form.size == 0
                    || form.setup.is_some()
                    || !form.inner.is_empty()
                    || form.flow == "table"
                {
                    continue;
                }
                let prefix = usize::from(extended);
                let mut bytes = vec![0u8; 256];
                bytes[0] = if extended { metadata.prefix } else { opcode };
                if extended {
                    bytes[1] = opcode;
                }
                let mut found = None;
                for value in 0..=255u8 {
                    if let Some((offset, _, _)) = form.select {
                        if offset + prefix >= bytes.len() {
                            continue;
                        }
                        bytes[offset + prefix] = value;
                    }
                    if let Ok(instruction) = super::decode::instruction(&bytes, 0)
                        && instruction.spec == *form
                        && instruction.size == prefix + form.size
                        && instruction.skips.is_empty()
                        && form.operands.iter().all(|op| {
                            prefix + op.offset + metadata.widths[&op.kind] <= instruction.size
                        })
                    {
                        found = Some((bytes[..instruction.size].to_vec(), instruction));
                        break;
                    }
                    if form.select.is_none() {
                        break;
                    }
                }
                if let Some((bytes, instruction)) = found {
                    result.push(serde_json::json!({"opcode":if extended {metadata.prefix}else{opcode},"extended":if extended{Some(opcode)}else{None},"form":form_index,"bytes":bytes,"instruction":instruction}));
                }
            }
        }
    }
    Ok(serde_json::json!({"source_sha256":metadata.source_sha256,"templates":result}))
}

/// Encode operand fields once, in Rust, for both source edits and new templates.
fn write_operand(
    bytes: &mut [u8],
    targets: &mut BTreeMap<usize, String>,
    ins: &Instruction,
    index: usize,
    value: &OperandValue,
) -> Result<(), FieldError> {
    let descriptor = ins
        .spec
        .operands
        .get(index)
        .ok_or_else(|| error("operand index outside instruction"))?;
    let prefix = usize::from(ins.extended.is_some());
    let offset = prefix + descriptor.offset;
    let width = *super::opcodes::metadata()?
        .widths
        .get(&descriptor.kind)
        .ok_or_else(|| error("unknown operand width"))?;
    if offset + width > bytes.len() {
        return Err(unsupported(
            "operand reads neighboring bytes; edit its owning compound instruction",
        ));
    }
    match value {
        OperandValue::Target { node } => {
            if descriptor.kind != "addr" {
                return Err(error("only address operands accept stable node targets"));
            }
            targets.insert(index, node.clone());
        }
        OperandValue::Literal { value, immediate } => {
            if descriptor.kind == "addr" {
                return Err(error(
                    "authored address operands require an explicit stable target",
                ));
            }
            if width < 4 && *value >= (1u32 << (width * 8)) {
                return Err(error("operand exceeds its encoded width"));
            }
            let mut raw = *value;
            match descriptor.kind.as_str() {
                "iv" => {
                    if let Some(mode) = immediate {
                        raw = if *mode { raw | 0x8000 } else { raw & 0x7fff };
                    }
                }
                "sel" => {
                    if let Some(mode) = immediate {
                        let flag = prefix
                            + descriptor
                                .flags
                                .ok_or_else(|| error("selector flags missing"))?;
                        let bit = descriptor
                            .bit
                            .ok_or_else(|| error("selector bit missing"))?
                            as u8;
                        let byte = bytes.get_mut(flag).ok_or_else(|| {
                            unsupported("selector flags outside owning instruction")
                        })?;
                        if *mode {
                            *byte |= bit;
                        } else {
                            *byte &= !bit;
                        }
                    }
                }
                _ if immediate.is_some() => {
                    return Err(error("immediate mode only applies to iv/sel operands"));
                }
                _ => {}
            }
            for byte in 0..width {
                bytes[offset + byte] = (raw >> (byte * 8)) as u8;
            }
        }
    }
    Ok(())
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct InstructionArgs {
    pub bytes: Vec<u8>,
    pub operands: Vec<OperandValue>,
}
/// Prepare one authored instruction from typed operands. Graph placement and
/// stable target existence are checked by the subsequent atomic transaction.
pub fn instruction_template(args: InstructionArgs) -> Result<NodeInput, FieldError> {
    if args.bytes.is_empty() || args.bytes.len() > 256 {
        return Err(error("instruction template requires 1..256 bytes"));
    }
    let mut context = args.bytes.clone();
    context.resize(512, 0);
    let ins = super::decode::instruction(&context, 0)?;
    if ins.size != args.bytes.len() || args.operands.len() != ins.operands.len() {
        return Err(error(
            "template span or operand count differs from opcode schema",
        ));
    }
    let mut bytes = args.bytes;
    let mut targets = BTreeMap::new();
    for (index, value) in args.operands.iter().enumerate() {
        write_operand(&mut bytes, &mut targets, &ins, index, value)?;
    }
    context[..bytes.len()].copy_from_slice(&bytes);
    let selected = super::decode::instruction(&context, 0)?;
    if selected.spec != ins.spec || selected.size != bytes.len() {
        return Err(unsupported(
            "typed fields select a different instruction form",
        ));
    }
    Ok(NodeInput { bytes, targets })
}
