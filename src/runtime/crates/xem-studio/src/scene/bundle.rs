//! Complete nine-component source and deterministic scene container preparation.
//!
//! Measured context is verified against original source/module/disc identity
//! before any append. Header growth relocates packed source bytes together;
//! unchanged streams must reproduce their complete original outputs.
use super::formats::{Header, decode_header};
use crate::field::FieldError;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use xem_disc::{
    field_context::{MAX_COMPONENT, MAX_FILE, SourceContext, SourceIdentity},
    packed,
};

fn invalid(message: impl Into<String>) -> FieldError {
    FieldError::new("invalid_scene_bundle", message)
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Input {
    pub bundle: Vec<u8>,
    #[serde(default, deserialize_with = "portable_context")]
    pub context: Option<SourceContext>,
    pub source: Option<SourceIdentity>,
    pub module_sha256: Option<String>,
}
// These sizes originate in original u32 fields/bounded RAM observations.
// Reject oversized wire lengths before usize deserialization so malformed input
// has the same structured diagnostic on 32-bit WASM and 64-bit native hosts.
fn portable_context<'de, D: serde::Deserializer<'de>>(
    deserializer: D,
) -> Result<Option<SourceContext>, D::Error> {
    let value = Option::<serde_json::Value>::deserialize(deserializer)?;
    let Some(value) = value else {
        return Ok(None);
    };
    if value
        .get("components")
        .and_then(serde_json::Value::as_array)
        .is_some_and(|components| {
            components.iter().any(|c| {
                [
                    "index",
                    "source_offset",
                    "logical_size",
                    "packed_size",
                    "source_bytes_read",
                    "output_size",
                ]
                .iter()
                .any(|key| {
                    c.get(key)
                        .and_then(serde_json::Value::as_u64)
                        .is_some_and(|n| n > u64::from(u32::MAX))
                })
            })
        })
    {
        return Err(serde::de::Error::custom(
            "source context lengths must fit portable u32 byte ranges",
        ));
    }
    serde_json::from_value(value)
        .map(Some)
        .map_err(serde::de::Error::custom)
}
#[derive(Clone, Debug)]
pub struct Component {
    pub logical_size: usize,
    pub bytes: Vec<u8>,
    pub source_offset: usize,
    pub source_bytes_read: usize,
}
#[derive(Clone, Debug)]
pub struct Replacement {
    pub logical_size: usize,
    pub bytes: Vec<u8>,
}
#[derive(Clone, Debug)]
pub struct Source {
    pub header: Header,
    pub components: Vec<Component>,
    input: Input,
    guard: Vec<u8>,
}
fn decode(
    data: &[u8],
    index: usize,
    logical: usize,
    start: usize,
) -> Result<packed::PackedBlock, FieldError> {
    let source = data
        .get(start..)
        .ok_or_else(|| invalid("component source offset exceeds bundle"))?;
    let result = packed::decode_block(
        source,
        logical
            .checked_add(16)
            .ok_or_else(|| invalid("component allocation overflow"))?,
    )
    .map_err(|e| {
        FieldError::new(
            if matches!(e, packed::PackedError::Truncated { .. }) {
                "source_context_required"
            } else {
                "invalid_scene_bundle"
            },
            format!("component {index}: complete original source decode failed: {e}"),
        )
    })?;
    if result.data.len() < logical {
        return Err(invalid(format!(
            "component {index}: complete output is shorter than logical size"
        )));
    }
    Ok(result)
}
impl Source {
    pub fn load(input: Input) -> Result<Self, FieldError> {
        let header = decode_header(&input.bundle)?;
        let guard = match (&input.context, &input.source, &input.module_sha256) {
            (None, source, None) => {
                // Disc identity is meaningful for ordinary asset editing even
                // when no runtime source-memory extension is needed.
                if let Some(source) = source {
                    source
                        .validate(&input.bundle)
                        .map_err(|e| FieldError::new("source_mismatch", e))?;
                }
                Vec::new()
            }
            (Some(context), Some(source), Some(module)) => context
                .guard(&input.bundle, source, module)
                .map_err(|e| FieldError::new("invalid_source_context", e))?,
            _ => {
                return Err(invalid(
                    "measured context requires selected source identity and module_sha256; a module alone supplies no provenance",
                ));
            }
        };
        let mut measured = input.bundle.clone();
        measured.extend_from_slice(&guard);
        let mut components = Vec::with_capacity(9);
        for index in 0..9 {
            let logical = header.logical_sizes[index];
            let start = header.offsets[index];
            let block = decode(&measured, index, logical, start)?;
            components.push(Component {
                logical_size: logical,
                bytes: block.data,
                source_offset: start,
                source_bytes_read: block.source_bytes_read,
            });
        }
        Ok(Self {
            header,
            components,
            input,
            guard,
        })
    }
    pub fn input(&self) -> &Input {
        &self.input
    }

    /// All changes are derived from an immutable source. This primitive does not
    /// authorize reference-changing edits; the domain transaction validates them.
    /// New packing may add an explicit authored tail (at most logical+16), never
    /// a fabricated original-source guard. Complete unaffected outputs are exact.
    pub fn repack(
        &self,
        header: &Header,
        replacements: &BTreeMap<usize, Replacement>,
    ) -> Result<Vec<u8>, FieldError> {
        if header.source_sha256 != self.header.source_sha256
            || header.offsets != self.header.offsets
            || header.logical_sizes != self.header.logical_sizes
            || header.slots != self.header.slots
            || header.view != self.header.view
        {
            return Err(invalid(
                "scene container header changes immutable source/layout fields",
            ));
        }
        if replacements.keys().any(|index| *index >= 9) {
            return Err(invalid("component index exceeds nine original descriptors"));
        }
        let changed = header != &self.header
            || replacements.iter().any(|(index, value)| {
                value.logical_size != self.components[*index].logical_size
                    || value.bytes != self.components[*index].bytes
            });
        if !changed {
            return Ok(self.input.bundle.clone());
        }
        let count = u16::try_from(header.descriptors.len())
            .map_err(|_| invalid("descriptor count exceeds original u16 field"))?;
        let old_end = 0x190 + self.header.descriptors.len() * 16;
        let new_end = 0x190 + header.descriptors.len() * 16;
        let body = self
            .input
            .bundle
            .get(old_end..)
            .ok_or_else(|| invalid("original descriptor table exceeds source"))?;
        let base_size = new_end
            .checked_add(body.len())
            .and_then(|n| n.checked_add(self.guard.len()))
            .ok_or_else(|| invalid("scene header growth overflow"))?;
        if base_size > MAX_FILE {
            return Err(invalid("scene header growth exceeds 32 MiB bundle budget"));
        }
        let mut result = Vec::with_capacity(base_size);
        result.extend_from_slice(&self.input.bundle[..0x190]);
        result[0x18c..0x18e].copy_from_slice(&count.to_le_bytes());
        for descriptor in &header.descriptors {
            result.extend_from_slice(&descriptor.flags.to_le_bytes());
            for value in descriptor.rotation.iter().chain(&descriptor.position) {
                result.extend_from_slice(&value.to_le_bytes());
            }
            result.extend_from_slice(&descriptor.model.to_le_bytes());
        }
        result.extend_from_slice(body);
        result.extend_from_slice(&self.guard);
        // Group-relative component contents stay fixed; only their packed source
        // offsets shift when the header's descriptor table changes length.
        for index in 0..9 {
            let shifted = self.header.offsets[index]
                .checked_sub(old_end)
                .and_then(|n| n.checked_add(new_end))
                .ok_or_else(|| invalid("component relocation overflow"))?;
            let value =
                u32::try_from(shifted).map_err(|_| invalid("component relocation exceeds u32"))?;
            result[0x130 + index * 4..0x134 + index * 4].copy_from_slice(&value.to_le_bytes());
        }
        let mut expected = BTreeMap::new();
        for (&index, value) in replacements {
            if value.logical_size > MAX_COMPONENT - 16
                || value.logical_size > value.bytes.len()
                || value.bytes.len() - value.logical_size > 16
            {
                return Err(invalid(format!(
                    "component {index}: replacement exceeds logical+16 allocation"
                )));
            }
            if value.logical_size == self.components[index].logical_size
                && value.bytes == self.components[index].bytes
            {
                continue;
            }
            let encoded = packed::encode_literals(&value.bytes, MAX_COMPONENT)
                .map_err(|e| invalid(e.to_string()))?;
            let block = packed::decode_block(&encoded, MAX_COMPONENT)
                .map_err(|e| invalid(e.to_string()))?;
            if block.data.len() - value.logical_size > 16
                || block.data.get(..value.bytes.len()) != Some(value.bytes.as_slice())
            {
                return Err(invalid(format!(
                    "component {index}: authored packing exceeds allocation or changes requested bytes"
                )));
            }
            if index == 5 {
                let graph = crate::field::decode(&value.bytes, Some(value.logical_size))?;
                crate::field::validate_packing_tail(&graph, &block.data)?;
            }
            let start = result
                .len()
                .checked_add(3)
                .map(|n| n & !3)
                .ok_or_else(|| invalid("component append offset overflow"))?;
            let end = start
                .checked_add(encoded.len())
                .ok_or_else(|| invalid("component append size overflow"))?;
            if end > MAX_FILE {
                return Err(invalid("authored scene bundle exceeds 32 MiB"));
            }
            result.resize(start, 0);
            result.extend_from_slice(&encoded);
            result[0x10c + index * 4..0x110 + index * 4]
                .copy_from_slice(&(value.logical_size as u32).to_le_bytes());
            result[0x130 + index * 4..0x134 + index * 4]
                .copy_from_slice(&(start as u32).to_le_bytes());
            expected.insert(index, block.data);
        }
        let final_header = decode_header(&result)?;
        for index in 0..9 {
            let block = decode(
                &result,
                index,
                final_header.logical_sizes[index],
                final_header.offsets[index],
            )?;
            let expected = expected
                .get(&index)
                .unwrap_or(&self.components[index].bytes);
            if &block.data != expected {
                return Err(invalid(format!(
                    "scene preparation changes complete output of component {index}"
                )));
            }
        }
        Ok(result)
    }
}
