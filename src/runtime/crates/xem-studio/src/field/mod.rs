//! Lossless inspection of unedited original field-event components.

mod decode;
mod graph;
mod opcodes;

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::fmt;

pub use decode::{decode, decode_instruction};
pub use graph::encode;
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

pub(super) fn source_hash(bytes: &[u8]) -> String {
    Sha256::digest(bytes)
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}
