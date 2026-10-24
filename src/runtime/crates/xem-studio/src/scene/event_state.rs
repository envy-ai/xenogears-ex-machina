//! Scene-owned M6 transactions and stable selector bindings. The original M6
//! document remains the sole script representation/compiler. Captured selectors
//! retain entity identities through scene renumbering and code relocation.
use super::{
    actor_rows,
    bindings::{self, Reference, References},
    virtual_actors,
};
use crate::field::{self, FieldError, draft};
use std::{
    collections::{BTreeMap, BTreeSet},
    sync::Arc,
};

#[derive(Clone)]
struct Binding {
    capture: usize,
    entry: usize,
}
#[derive(Clone)]
pub(crate) struct Events {
    pub draft: draft::Draft,
    captures: Vec<Arc<References>>,
    bindings: BTreeMap<(String, usize), Binding>,
}
fn invalid(message: impl Into<String>) -> FieldError {
    FieldError::new("invalid_scene_reference", message)
}
fn nodes(compiled: &draft::Compiled) -> BTreeMap<usize, String> {
    let mut result = BTreeMap::new();
    for (id, pc) in &compiled.bindings {
        result.entry(*pc).or_insert_with(|| id.clone());
    }
    result
}
fn capture(
    compiled: &draft::Compiled,
    descriptors: &[String],
    zones: &[String],
) -> Result<References, FieldError> {
    let mut refs = bindings::capture_virtual(&compiled.graph, descriptors, zones)?;
    let nodes = nodes(compiled);
    for reference in &mut refs.entries {
        reference.node = nodes.get(&reference.pc).cloned().ok_or_else(|| {
            invalid("scene reference belongs to compiler storage without a stable authored node")
        })?;
    }
    Ok(refs)
}
fn writes(action: &draft::Action, result: &mut BTreeSet<(String, usize)>) {
    match action {
        draft::Action::Batch { actions } => {
            for action in actions {
                writes(action, result);
            }
        }
        draft::Action::SetOperand { node, operand, .. } => {
            result.insert((node.clone(), *operand));
        }
        _ => {}
    }
}
fn touched(
    reference: &Reference,
    compiled: &draft::Compiled,
    writes: &BTreeSet<(String, usize)>,
) -> Result<bool, FieldError> {
    let instruction = compiled
        .graph
        .instructions
        .binary_search_by_key(&reference.pc, |i| i.pc)
        .ok()
        .and_then(|n| compiled.graph.instructions.get(n))
        .ok_or_else(|| invalid("reference instruction is missing"))?;
    let selector = instruction
        .spec
        .operands
        .get(reference.operand)
        .ok_or_else(|| invalid("reference operand is missing"))?;
    let widths = &field::opcode_metadata()?.widths;
    let selector_end = selector.offset + widths[&selector.kind];
    for (node, operand) in writes {
        if node != &reference.node {
            continue;
        }
        let written = instruction
            .spec
            .operands
            .get(*operand)
            .ok_or_else(|| invalid("edited reference operand is missing"))?;
        let written_end = written.offset + widths[&written.kind];
        if selector.offset < written_end && written.offset < selector_end
            || written
                .flags
                .is_some_and(|at| selector.offset <= at && at < selector_end)
        {
            return Ok(true);
        }
    }
    Ok(false)
}
impl Events {
    pub fn open(
        draft: draft::Draft,
        descriptors: &[String],
        zones: &[String],
    ) -> Result<Self, FieldError> {
        let compiled = draft.compile()?;
        let refs = capture(&compiled, descriptors, zones)?;
        let bindings = refs
            .entries
            .iter()
            .enumerate()
            .map(|(entry, r)| ((r.node.clone(), r.operand), Binding { capture: 0, entry }))
            .collect();
        Ok(Self {
            draft,
            captures: vec![Arc::new(refs)],
            bindings,
        })
    }
    pub fn extend_actor_source(&mut self) -> Result<u32, FieldError> {
        let row = u32::try_from(self.draft.source_graph().actors.len())
            .map_err(|_| invalid("virtual actor row exceeds u32"))?;
        let base = virtual_actors::extend_empty(self.draft.source_graph())?;
        self.draft = self.draft.with_extended_actor_table(base)?;
        Ok(row)
    }
    pub fn edit(
        &mut self,
        action: draft::Action,
        descriptors: &[String],
        zones: &[String],
    ) -> Result<(), FieldError> {
        // The scene calls this on an unpublished candidate. Any failure here or
        // in final bundle compilation leaves its live state and both allocators.
        let mut written = BTreeSet::new();
        writes(&action, &mut written);
        self.draft.edit(self.draft.revision(), action)?;
        let compiled = self.draft.compile()?;
        let mut fresh = capture(&compiled, descriptors, zones)?;
        let capture_id = self.captures.len();
        // Store only new/changed references in this capture. A one-field edit
        // must not retain a second complete graph's reference list merely to
        // share one freshly selected namespace.
        for reference in std::mem::take(&mut fresh.entries) {
            let key = (reference.node.clone(), reference.operand);
            let keep = if let Some(previous) = self.bindings.get(&key) {
                let previous = &self.captures[previous.capture].entries[previous.entry];
                previous.raw == reference.raw
                    && previous.role == reference.role
                    && previous.aliases == reference.aliases
                    && !touched(&reference, &compiled, &written)?
            } else {
                false
            };
            if !keep {
                if let bindings::Target::Unknown { reason } = &reference.target {
                    return Err(FieldError::new(
                        "unsupported_scene_reference",
                        format!("{} operand {}: {reason}", reference.node, reference.operand),
                    ));
                }
                self.bindings.insert(
                    key,
                    Binding {
                        capture: capture_id,
                        entry: fresh.entries.len(),
                    },
                );
                fresh.entries.push(reference);
            }
        }
        // Keep unreachable bindings: a later event edit can restore reachability
        // without changing their stable targets. One Arc owns a namespace per
        // edit capture; never clone the complete namespace per reference.
        if !fresh.entries.is_empty() {
            self.captures.push(Arc::new(fresh));
        }
        Ok(())
    }
    pub fn compile(
        &self,
        descriptors: &[String],
        zones: &[String],
        rows: &[actor_rows::Row],
    ) -> Result<draft::Compiled, FieldError> {
        let virtual_compiled = self.draft.compile()?;
        let compiled = runtime_rows(virtual_compiled, rows)?;
        let current = capture(&compiled, descriptors, zones)?;
        let mut selected: BTreeMap<usize, Vec<usize>> = BTreeMap::new();
        for reference in current.entries {
            let binding = self
                .bindings
                .get(&(reference.node, reference.operand))
                .ok_or_else(|| {
                    invalid("reachable instruction lacks a captured stable scene reference")
                })?;
            selected
                .entry(binding.capture)
                .or_default()
                .push(binding.entry);
        }
        let mut rewrites = Vec::new();
        for (capture_id, entries) in selected {
            let capture = &self.captures[capture_id];
            let repaired = capture.remap_selected(&entries, descriptors, zones)?;
            // remap_selected reports the stable node identifier captured with
            // each reference, not a current/relocated PC.
            rewrites.extend(repaired);
        }
        if rewrites.is_empty() {
            return Ok(compiled);
        }
        let limit = descriptors
            .len()
            .max(self.draft.source_graph().actors.len());
        runtime_rows(
            self.draft.compile_with_scene_operands(&rewrites, limit)?,
            rows,
        )
    }
}

fn runtime_rows(
    mut compiled: draft::Compiled,
    rows: &[actor_rows::Row],
) -> Result<draft::Compiled, FieldError> {
    let rebuilt = actor_rows::rebuild(&compiled.graph, rows)?;
    compiled.bytes = rebuilt.bytes;
    compiled.declared_size = rebuilt.declared_size;
    compiled.graph = rebuilt.graph;
    Ok(compiled)
}
