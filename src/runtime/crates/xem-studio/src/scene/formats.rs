//! Bounded original field header and trigger-zone records.
//!
//! Sources: field/field_load.h, field.c:field_load_from_bundle and
//! field_event.c:field_event_call_in_zone. Unknown header bytes and decoded tails
//! are retained; no geometry or source-memory padding is invented.
use crate::field::FieldError;
use serde::{Deserialize, Serialize};
use xem_disc::field_context::{MAX_COMPONENT, MAX_FILE, hash};

const HEADER: usize = 0x190;
const DESCRIPTOR_SIZE: usize = 16;

fn invalid(message: impl Into<String>) -> FieldError {
    FieldError::new("invalid_scene", message)
}
fn region(bytes: &[u8], at: usize, size: usize) -> Result<&[u8], FieldError> {
    super::span(bytes, at, size).ok_or_else(|| {
        invalid(format!(
            "scene range +0x{at:x}, {size} bytes exceeds {}",
            bytes.len()
        ))
    })
}

fn half(bytes: &[u8], at: usize) -> Result<u16, FieldError> {
    let data = region(bytes, at, 2)?;
    Ok(u16::from_le_bytes([data[0], data[1]]))
}
fn word(bytes: &[u8], at: usize) -> Result<usize, FieldError> {
    let data = region(bytes, at, 4)?;
    Ok(u32::from_le_bytes([data[0], data[1], data[2], data[3]]) as usize)
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SpriteSlot {
    pub x: u16,
    pub y: u16,
    pub reserved: u16,
    pub shared: i16,
}

/// Raw signed view/light words. Three eight-word light records are followed by
/// background RGB and one preserved word. This record does not hold the camera
/// eye/target, which are constructed elsewhere. The original copies light 0 over
/// cached lights 1/2 after applying their earlier model_set_light calls.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct View {
    pub words: [i16; 28],
}

/// Source halfwords as read by the loader's u16 pointer. Actor reset later
/// interprets the low X/Z halfwords as signed during floor lookup; a live actor's
/// 16.16 position must not be confused with this authored record.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Descriptor {
    pub flags: u16,
    pub rotation: [u16; 3],
    pub position: [u16; 3],
    pub model: u16,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Header {
    pub source_sha256: String,
    pub slots: Vec<SpriteSlot>,
    pub view: View,
    pub descriptors: Vec<Descriptor>,
    pub offsets: [usize; 9],
    pub logical_sizes: [usize; 9],
}

pub fn decode_header(bundle: &[u8]) -> Result<Header, FieldError> {
    if bundle.len() > MAX_FILE {
        return Err(invalid("field bundle exceeds 32 MiB"));
    }
    region(bundle, 0, HEADER)?;
    let count = usize::from(half(bundle, 0x18c)?);
    let table_size = count
        .checked_mul(DESCRIPTOR_SIZE)
        .ok_or_else(|| invalid("descriptor table overflow"))?;
    region(bundle, HEADER, table_size)?;
    let table_end = HEADER + table_size;
    let mut offsets = [0; 9];
    let mut logical_sizes = [0; 9];
    for index in 0..9 {
        offsets[index] = word(bundle, 0x130 + index * 4)?;
        logical_sizes[index] = word(bundle, 0x10c + index * 4)?;
        if offsets[index] < table_end || offsets[index] >= bundle.len() {
            return Err(invalid(format!(
                "component {index}: source offset overlaps header/descriptors or exceeds bundle"
            )));
        }
        if logical_sizes[index] > (MAX_COMPONENT - 16) {
            return Err(invalid(format!(
                "component {index}: logical size plus original allocation guard exceeds 2 MiB"
            )));
        }
        region(bundle, offsets[index], 4)?;
    }
    let mut slots = Vec::with_capacity(32);
    for index in 0..32 {
        let at = index * 8;
        slots.push(SpriteSlot {
            x: half(bundle, at)?,
            y: half(bundle, at + 2)?,
            reserved: half(bundle, at + 4)?,
            shared: half(bundle, at + 6)? as i16,
        });
    }
    let mut words = [0; 28];
    for (index, value) in words.iter_mut().enumerate() {
        *value = half(bundle, 0x154 + index * 2)? as i16;
    }
    let mut descriptors = Vec::with_capacity(count);
    for index in 0..count {
        let at = HEADER + index * DESCRIPTOR_SIZE;
        descriptors.push(Descriptor {
            flags: half(bundle, at)?,
            rotation: [
                half(bundle, at + 2)?,
                half(bundle, at + 4)?,
                half(bundle, at + 6)?,
            ],
            position: [
                half(bundle, at + 8)?,
                half(bundle, at + 10)?,
                half(bundle, at + 12)?,
            ],
            model: half(bundle, at + 14)?,
        });
    }
    Ok(Header {
        source_sha256: hash(bundle),
        slots,
        view: View { words },
        descriptors,
        offsets,
        logical_sizes,
    })
}

/// Patch supported same-layout descriptor transforms, preserving all unrelated
/// bytes. Structural edits require the reference-aware scene compiler, not this
/// primitive. Source identity and every immutable field are checked first.
pub fn encode_header(source: &[u8], header: &Header) -> Result<Vec<u8>, FieldError> {
    let base = decode_header(source)?;
    if header.source_sha256 != base.source_sha256
        || header.offsets != base.offsets
        || header.logical_sizes != base.logical_sizes
        || header.slots != base.slots
        || header.view != base.view
        || header.descriptors.len() != base.descriptors.len()
    {
        return Err(invalid(
            "scene header source identity or immutable layout differs",
        ));
    }
    for (index, (before, after)) in base.descriptors.iter().zip(&header.descriptors).enumerate() {
        if before.flags != after.flags || before.model != after.model {
            return Err(invalid(format!(
                "descriptor {index}: flags/model changes require reference-aware transaction"
            )));
        }
        if before.rotation != after.rotation && after.flags & 0x40 != 0 {
            return Err(FieldError::new(
                "unsupported_scene_edit",
                format!(
                    "descriptor {index}: sprite rotation is reset by the original loader; edit a proven script-facing binding instead"
                ),
            ));
        }
    }
    let mut result = source.to_vec();
    for (index, descriptor) in header.descriptors.iter().enumerate() {
        let at = HEADER + index * DESCRIPTOR_SIZE;
        for (axis, value) in descriptor
            .rotation
            .iter()
            .chain(&descriptor.position)
            .enumerate()
        {
            result[at + 2 + axis * 2..at + 4 + axis * 2].copy_from_slice(&value.to_le_bytes());
        }
    }
    Ok(result)
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Zone {
    /// Ordered corners, Y down. Planar readers use XZ half planes; height-tested
    /// readers additionally use corner 0's Y. Winding is never normalized.
    pub corners: [[i16; 3]; 4],
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Zones {
    pub zones: Vec<Zone>,
    /// Complete packed-stream output beyond the declared logical component.
    pub tail: Vec<u8>,
}
pub fn decode_zones(full: &[u8], logical_size: usize) -> Result<Zones, FieldError> {
    if full.len() > MAX_COMPONENT
        || logical_size > full.len()
        || full.len() - logical_size > 16
        || !logical_size.is_multiple_of(24)
    {
        return Err(invalid(
            "zone logical size must fit full output and contain complete 24-byte records within 2 MiB",
        ));
    }
    let mut zones = Vec::with_capacity(logical_size / 24);
    for at in (0..logical_size).step_by(24) {
        let mut corners = [[0; 3]; 4];
        for (corner, xyz) in corners.iter_mut().enumerate() {
            for (axis, value) in xyz.iter_mut().enumerate() {
                *value = half(full, at + corner * 6 + axis * 2)? as i16;
            }
        }
        zones.push(Zone { corners });
    }
    Ok(Zones {
        zones,
        tail: full[logical_size..].to_vec(),
    })
}
pub fn encode_zones(zones: &Zones) -> Result<Vec<u8>, FieldError> {
    let size = zones
        .zones
        .len()
        .checked_mul(24)
        .and_then(|n| n.checked_add(zones.tail.len()))
        .ok_or_else(|| invalid("zone output size overflow"))?;
    if size > MAX_COMPONENT || zones.tail.len() > 16 {
        return Err(invalid("zone output exceeds 2 MiB"));
    }
    let mut bytes = Vec::with_capacity(size);
    for zone in &zones.zones {
        for value in zone.corners.iter().flatten() {
            bytes.extend_from_slice(&value.to_le_bytes());
        }
    }
    bytes.extend_from_slice(&zones.tail);
    Ok(bytes)
}
