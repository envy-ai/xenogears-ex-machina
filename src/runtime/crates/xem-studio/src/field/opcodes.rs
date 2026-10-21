use super::FieldError;
use serde::{Deserialize, Serialize};
use std::{collections::BTreeMap, sync::OnceLock};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct OperandSpec {
    pub kind: String,
    pub offset: usize,
    pub flags: Option<usize>,
    pub bit: Option<u32>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct OpcodeForm {
    pub mnemonic: String,
    pub handler: String,
    pub size: usize,
    pub flow: String,
    pub operands: Vec<OperandSpec>,
    pub effect: String,
    pub select: Option<(usize, u32, Option<u32>)>,
    pub also: Vec<(usize, Option<String>)>,
    pub inner: Vec<usize>,
    pub setup: Option<usize>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Metadata {
    pub schema_version: u32,
    pub source: String,
    pub source_sha256: String,
    pub generator: String,
    pub prefix: u8,
    pub prefix_handler: String,
    pub jump: u8,
    pub allocation_slack: usize,
    pub widths: BTreeMap<String, usize>,
    pub flows: Vec<String>,
    pub comparisons: Vec<String>,
    pub condition_text: Vec<String>,
    pub primary: BTreeMap<u8, Vec<OpcodeForm>>,
    pub extended: BTreeMap<u8, Vec<OpcodeForm>>,
}

pub(super) fn metadata() -> Result<&'static Metadata, FieldError> {
    static VALUE: OnceLock<Result<Metadata, String>> = OnceLock::new();
    match VALUE.get_or_init(|| {
        let metadata: Metadata =
            serde_json::from_str(include_str!("../../data/field-opcodes.json"))
                .map_err(|error| error.to_string())?;
        if metadata.schema_version != 1
            || metadata.source != "tools/analysis/events.py"
            || metadata.source_sha256.len() != 64
            || metadata.generator.is_empty()
            || metadata.prefix_handler.is_empty()
            || metadata.comparisons.len() != metadata.condition_text.len()
            || metadata
                .widths
                .values()
                .any(|width| ![1, 2, 4].contains(width))
        {
            return Err("invalid field opcode metadata schema".to_owned());
        }
        for forms in metadata.primary.values().chain(metadata.extended.values()) {
            if forms.is_empty()
                || forms.iter().any(|form| {
                    !metadata.flows.contains(&form.flow)
                        || form
                            .operands
                            .iter()
                            .any(|operand| !metadata.widths.contains_key(&operand.kind))
                })
            {
                return Err("invalid field opcode form metadata".to_owned());
            }
        }
        Ok(metadata)
    }) {
        Ok(metadata) => Ok(metadata),
        Err(message) => Err(FieldError::new("invalid_metadata", message.clone())),
    }
}
