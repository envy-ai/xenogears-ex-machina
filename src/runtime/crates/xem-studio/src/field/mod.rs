//! Lossless inspection of unedited original field-event components.

mod decode;
pub mod draft;
mod graph;
pub mod lanes;
mod opcodes;
pub mod structure;

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::fmt;

pub use decode::{decode, decode_instruction};
pub use graph::encode;
pub(crate) use opcodes::metadata as opcode_metadata;
pub use opcodes::{OpcodeForm, OperandSpec};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FieldError {
    pub kind: &'static str,
    pub message: String,
}

impl FieldError {
    pub(crate) fn new(kind: &'static str, message: impl Into<String>) -> Self {
        Self {
            kind,
            message: message.into(),
        }
    }
}
impl fmt::Display for FieldError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.message)
    }
}
impl std::error::Error for FieldError {}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct Instruction {
    pub pc: usize,
    pub opcode: u8,
    pub extended: Option<u8>,
    pub size: usize,
    /// Physical consumed ranges; an extended dispatch at ffff can wrap to zero.
    pub ranges: Vec<[usize; 2]>,
    pub spec: OpcodeForm,
    pub operands: Vec<u32>,
    pub immediate: Vec<Option<bool>>,
    pub successors: Vec<usize>,
    pub skips: Vec<usize>,
    pub inner: Vec<usize>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct EventEntry {
    pub actor: usize,
    pub event: usize,
    pub pc: usize,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct Diagnostics {
    pub overlaps: Vec<[usize; 2]>,
    pub unpaired: Vec<usize>,
    pub inner: Vec<[usize; 2]>,
    pub tables: Vec<[usize; 2]>,
    pub into: Vec<[usize; 2]>,
    pub outside: Vec<[usize; 2]>,
    pub beyond_declared: Vec<usize>,
    pub followed_skips: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct Block {
    /// Bytecode-relative half-open storage span. Blocks partition the stream.
    pub start: usize,
    pub end: usize,
    /// code, pair, table, or pinned; pinned/compound reasons remain inspectable.
    pub kind: String,
    pub reason: Option<String>,
    pub bytes: Vec<u8>,
    pub instruction_pcs: Vec<usize>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct Edge {
    pub from_block: usize,
    pub to_block: Option<usize>,
    pub from_pc: usize,
    pub to_pc: usize,
    pub kind: String,
    /// followed, not_followed, or outside_stream.
    pub status: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct FieldGraph {
    pub schema_version: u32,
    pub source_sha256: String,
    pub declared_size: usize,
    pub stream_size: usize,
    pub bytecode_offset: usize,
    pub variable_unsigned_bits: Vec<u8>,
    pub actors: Vec<[u16; 32]>,
    pub entries: Vec<EventEntry>,
    pub skipped_zero_entries: usize,
    pub instructions: Vec<Instruction>,
    pub blocks: Vec<Block>,
    pub edges: Vec<Edge>,
    pub diagnostics: Diagnostics,
}

/// A validated actor-table edit can add an entry at an existing instruction,
/// splitting its canonical basic block. Block IDs then change by construction.
/// Keep the complete ordered PC/kind/status edge facts exact; each graph's own
/// block associations are still checked by the normal strict encoder. This is
/// private to the authored table derivation, not a relaxed graph/byte encoder.
pub(crate) fn execution_edges_match(left: &FieldGraph, right: &FieldGraph) -> bool {
    left.edges.len() == right.edges.len()
        && left.edges.iter().zip(&right.edges).all(|(a, b)| {
            a.from_pc == b.from_pc && a.to_pc == b.to_pc && a.kind == b.kind && a.status == b.status
        })
}

pub(super) fn source_hash(bytes: &[u8]) -> String {
    Sha256::digest(bytes)
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

/// Packing can extend the complete output beyond an authored logical stream.
/// Retain the M6 rule: those bytes must not activate unresolved destinations or
/// change any reachable instruction/diagnostic. Shared by event and scene packs.
pub(crate) fn validate_packing_tail(graph: &FieldGraph, output: &[u8]) -> Result<(), FieldError> {
    let padded = decode(output, Some(graph.declared_size))?;
    if graph
        .diagnostics
        .outside
        .iter()
        .chain(&graph.diagnostics.inner)
        .any(|[_, target]| {
            *target >= graph.stream_size - graph.bytecode_offset
                && *target < output.len() - graph.bytecode_offset
        })
        || padded.instructions != graph.instructions
        || padded.diagnostics != graph.diagnostics
    {
        return Err(FieldError::new(
            "unsupported_relocation",
            "packing tail would activate an unresolved destination or change decoded execution",
        ));
    }
    Ok(())
}
