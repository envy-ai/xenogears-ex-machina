//! Complete nine-component source and deterministic scene container preparation.
//!
//! Measured context is verified against original source/module/disc identity
//! before any append. Header growth relocates packed source bytes together;
//! unchanged streams reproduce their complete known outputs. Offline truncated
//! streams require validated logical data and explicit authored completion on edit.
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
    /// Advertised suffix omitted by offline prefix decoding; never measured RAM.
    pub optional_tail_bytes: usize,
    /// Missing original source reads require a complete authored stream on edit.
    pub needs_stream_completion: bool,
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
// Boundary validation only: no glyph translation or message authoring. Sources:
// text_get_resource_entry/text_get_message_columns/text_get_message_rows and
// window_reveal_text in resident/text_windows_and_pads.c, mirrored by
// tools.analysis.text_control.text_table/decode_token. Input carries no verified
// loaded-font context: every non-control glyph is conservatively allowed to
// consume either one or two bytes. Both paths must terminate within the payload;
// a zero second glyph byte must not be mistaken for a message terminator.
fn message_prefix_complete(data: &[u8]) -> Result<(), FieldError> {
    let half = |at: usize| -> Result<usize, FieldError> {
        let bytes = super::span(data, at, 2)
            .ok_or_else(|| invalid("message table header/offset is truncated"))?;
        Ok(usize::from(u16::from_le_bytes([bytes[0], bytes[1]])))
    };
    let count = half(0)?;
    if half(2)? != 0 {
        return Err(invalid("message table reserved halfword is not zero"));
    }
    if count == 0xffff {
        return Ok(()); // Explicit original no-messages sentinel; four bytes suffice.
    }
    let table_end = 6 + 4 * count; // count+1 offsets, then count dimension pairs.
    if table_end > data.len() {
        return Err(invalid(
            "message table offsets/dimensions exceed logical size",
        ));
    }
    let text_end = half(4 + 2 * count)?;
    if text_end < table_end || text_end > data.len() {
        return Err(invalid(
            "message table final offset exceeds its logical payload",
        ));
    }
    // Backward boundary proof bounds work to payload length + entry count.
    // It deliberately over-approximates any font threshold: different glyphs
    // may take either width, so success does not assume canonical font bytes.
    // The reader uses start offsets, not the final offset as an execution
    // fence. A terminator at that final offset is valid if still in the known
    // logical payload. Never read beyond that payload or invent font bytes.
    let mut terminates = vec![false; data.len()];
    for at in (table_end..data.len()).rev() {
        let next = |size: usize| terminates.get(at + size).copied().unwrap_or(false);
        let byte = data[at];
        terminates[at] = match byte {
            0 => true,
            1..=3 => next(1),
            0x0f => match data.get(at + 1) {
                Some(3) => next(4),
                // Unsupported selection categories re-read 04 as a glyph.
                Some(4) => next(2) && next(1),
                // 0F 0B checks its operand but reads it again as text.
                Some(11) if at + 2 < data.len() => next(2),
                Some(0..=15) => next(3),
                _ => false,
            },
            _ => next(1) && next(2),
        };
    }
    for index in 0..count {
        let at = half(4 + 2 * index)?;
        if at < table_end || at >= data.len() {
            return Err(invalid(
                "message entry offset is outside its bounded text region",
            ));
        }
        if !terminates[at] {
            return Err(FieldError::new(
                "source_context_required",
                "component 7: message termination cannot be proved within the known payload without qualified loaded-font/source context",
            ));
        }
    }
    Ok(())
}

// Strict decoding has validated every available complete token. Inspect only
// their compressed framing to prove an absent continuation could fit the header:
// a known flag must not hide inevitable overflow behind a missing token byte.
fn unfinished_stream_fits(source: &[u8], remaining: usize, known_output: usize) -> bool {
    let future_groups_fit = |size: usize| size == 0 || size == 8 || size >= 10;
    let remaining_group_fits = |flags: u8, tokens: usize| {
        if known_output == 0 && flags & 1 != 0 {
            return false; // First output cannot be a backward reference.
        }
        let mask = ((1_u16 << tokens) - 1) as u8;
        let references = (flags & mask).count_ones() as usize;
        let literals = tokens - references;
        let minimum = literals + 3 * references;
        let maximum = literals + 18 * references;
        if remaining < minimum {
            return false;
        }
        // Reference lengths span 3..18; literal widths are fixed. Remaining
        // complete groups add 0, 8, or any size >=10 (never 1..7 or 9).
        remaining <= maximum
            || remaining - minimum >= 10
            || remaining
                .checked_sub(8)
                .is_some_and(|n| (minimum..=maximum).contains(&n))
    };
    let mut at = 4;
    loop {
        let Some(&flags) = source.get(at) else {
            return future_groups_fit(remaining);
        };
        at += 1;
        for token in 0..8 {
            let Some(&low) = source.get(at) else {
                return remaining_group_fits(flags >> token, 8 - token);
            };
            at += 1;
            if flags & (1 << token) != 0 {
                if source.get(at).is_none() {
                    // A missing high byte cannot repair an already impossible
                    // low distance. Zero low requires a distance of at least256.
                    let minimum_distance = if low == 0 { 256 } else { usize::from(low) };
                    return minimum_distance <= known_output
                        && remaining_group_fits(flags >> token, 8 - token);
                }
                at += 1;
            }
        }
    }
}

// Only absent source reads can qualify for offline completion. Malformed
// references/output bounds are never swallowed. A terminal flag can be missing
// even when every advertised output byte is known; retain that complete output.
fn offline_component(
    data: &[u8],
    index: usize,
    logical: usize,
    start: usize,
) -> Result<(packed::PackedBlock, bool, usize), FieldError> {
    let source = data
        .get(start..)
        .ok_or_else(|| invalid("component source offset exceeds bundle"))?;
    let limit = logical
        .checked_add(16)
        .ok_or_else(|| invalid("component allocation overflow"))?;
    match packed::decode_block(source, limit) {
        Ok(block) if block.data.len() >= logical => Ok((block, false, 0)),
        Ok(_) => Err(invalid(format!(
            "component {index}: complete output is shorter than logical size"
        ))),
        Err(packed::PackedError::Truncated { ref output, .. }) => {
            let declared =
                xem_disc::field_context::word(source, 0).map_err(|e| invalid(e.to_string()))?;
            let remaining = declared
                .checked_sub(output.len())
                .ok_or_else(|| invalid("known output exceeds advertised size"))?;
            if !unfinished_stream_fits(source, remaining, output.len()) {
                return Err(invalid(format!(
                    "component {index}: incomplete eight-token flag group cannot fit advertised output"
                )));
            }
            // This validates the length header, all required tokens/distances,
            // and full advertised bounds of a copy straddling the logical end.
            let prefix = packed::decode_prefix(source, logical, limit).map_err(|e| {
                FieldError::new(
                    if matches!(e, packed::PackedError::Truncated { .. }) {
                        "source_context_required"
                    } else {
                        "invalid_scene_bundle"
                    },
                    format!("component {index}: required logical-prefix decode failed: {e}"),
                )
            })?;
            if output.len() == declared {
                let complete = packed::decode_prefix(source, declared, limit)
                    .map_err(|e| invalid(format!("component {index}: {e}")))?;
                return Ok((complete, true, 0));
            }
            // Logical availability alone is not semantic qualification.
            // Only validated message tables and whole zone records have a
            // bounded consumer here. Events can traverse retained tails;
            // sprite/encounter/other opaque suffixes remain context-required.
            match index {
                7 => message_prefix_complete(&prefix.data)?,
                8 => {
                    super::formats::decode_zones(&prefix.data, logical)?;
                }
                _ => {
                    return Err(FieldError::new(
                        "source_context_required",
                        format!(
                            "component {index}: unavailable output suffix has no qualified offline consumer boundary; source context is required"
                        ),
                    ));
                }
            }
            Ok((prefix, true, declared - logical))
        }
        Err(e) => Err(invalid(format!(
            "component {index}: complete original source decode failed: {e}"
        ))),
    }
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
            let (block, needs_stream_completion, optional_tail_bytes) = if input.context.is_none() {
                offline_component(&measured, index, logical, start)?
            } else {
                (decode(&measured, index, logical, start)?, false, 0)
            };
            components.push(Component {
                logical_size: logical,
                bytes: block.data,
                source_offset: start,
                source_bytes_read: block.source_bytes_read,
                optional_tail_bytes,
                needs_stream_completion,
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
    /// a fabricated original-source guard. Complete unaffected outputs are exact;
    /// admitted offline prefixes retain their known payload and author complete
    /// self-contained streams whenever any part of the bundle changes.
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
        // Every admitted incomplete source stream is redirected on ANY edit,
        // including sparse replacements and header-only changes. Otherwise its
        // original missing read could consume newly appended authored bytes.
        let completions: BTreeMap<usize, Replacement> = self
            .components
            .iter()
            .enumerate()
            .filter(|(index, component)| {
                component.needs_stream_completion && !replacements.contains_key(index)
            })
            .map(|(index, component)| {
                (
                    index,
                    Replacement {
                        logical_size: component.logical_size,
                        bytes: component.bytes.clone(),
                    },
                )
            })
            .collect();
        for (index, value) in replacements.iter().chain(&completions) {
            let index = *index;
            if self.components[index].optional_tail_bytes > 0
                && value.bytes.len() != value.logical_size
            {
                return Err(invalid(
                    "offline optional-tail replacement must contain only its known logical payload",
                ));
            }
            if value.logical_size > MAX_COMPONENT - 16
                || value.logical_size > value.bytes.len()
                || value.bytes.len() - value.logical_size > 16
            {
                return Err(invalid(format!(
                    "component {index}: replacement exceeds logical+16 allocation"
                )));
            }
            if !self.components[index].needs_stream_completion
                && value.logical_size == self.components[index].logical_size
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
