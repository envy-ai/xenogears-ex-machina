//! Original field texture uploads, before the runtime mutates VRAM.
//!
//! Palette lists follow field.c:field_load_tim_list and port/libgpu.c:get_tim_addr.
//! Image lists follow resident/sprite_construction.c:sprite_upload_images_side_by_side.
//! Uploads contain raw VRAM words, not guessed RGBA or inherited/shared pixels.
use super::formats::SpriteSlot;
use crate::field::FieldError;
use serde::{Deserialize, Serialize};
use xem_disc::field_context::MAX_COMPONENT;

// Service work/output budgets, not claims about a source-format entity maximum.
const MAX_UPLOADS: usize = 4096;
const MAX_UPLOAD_WORDS: usize = 4 * MAX_COMPONENT;
#[derive(Default)]
struct Budget {
    uploads: usize,
    words: usize,
}

fn invalid(message: impl Into<String>) -> FieldError {
    FieldError::new("invalid_scene_texture", message)
}
fn bytes(data: &[u8], at: usize, size: usize) -> Result<&[u8], FieldError> {
    super::span(data, at, size).ok_or_else(|| {
        invalid(format!(
            "texture span +0x{at:x}, {size} bytes exceeds {}",
            data.len()
        ))
    })
}

fn half(data: &[u8], at: usize) -> Result<u16, FieldError> {
    let b = bytes(data, at, 2)?;
    Ok(u16::from_le_bytes([b[0], b[1]]))
}
fn word(data: &[u8], at: usize) -> Result<usize, FieldError> {
    let b = bytes(data, at, 4)?;
    Ok(u32::from_le_bytes([b[0], b[1], b[2], b[3]]) as usize)
}
fn table(data: &[u8]) -> Result<(usize, usize), FieldError> {
    let count = word(data, 0)?;
    if count > (data.len() - 4) / 4 {
        return Err(invalid("texture offset table exceeds component"));
    }
    Ok((count, 4 + count * 4))
}
fn offsets(data: &[u8]) -> Result<Vec<usize>, FieldError> {
    let (count, _) = table(data)?;
    (0..count).map(|index| word(data, 4 + index * 4)).collect()
}
fn start(data: &[u8], at: usize) -> Result<(), FieldError> {
    let (_, table_end) = table(data)?;
    if at < table_end || at >= data.len() {
        return Err(invalid(
            "texture entry offset overlaps its table or exceeds source",
        ));
    }
    Ok(())
}

fn component(full: &[u8], logical: usize) -> Result<(), FieldError> {
    if logical > MAX_COMPONENT - 16 || logical > full.len() || full.len() - logical > 16 {
        return Err(invalid(
            "texture component exceeds original logical+16 allocation",
        ));
    }
    Ok(())
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Upload {
    pub source_span: [usize; 2],
    pub pixels_offset: usize,
    /// VRAM rectangle in 16-bit words, not texture pixel width.
    pub rect: [u16; 4],
    pub words: Vec<u16>,
    pub kind: String,
    pub slot: Option<usize>,
    pub tim_mode: Option<u32>,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TextureUploads {
    pub uploads: Vec<Upload>,
    /// Shared sprite slots intentionally do not upload local images. Rendering
    /// their VRAM requires an explicitly identified inherited source.
    pub shared_slots: Vec<usize>,
}
struct UploadKind<'a> {
    kind: &'a str,
    slot: Option<usize>,
    mode: Option<u32>,
}
fn upload(
    data: &[u8],
    span: [usize; 2],
    pixel_at: usize,
    rect: [u16; 4],
    meta: UploadKind<'_>,
    budget: &mut Budget,
) -> Result<Upload, FieldError> {
    let [x, y, w, h] = rect;
    if w == 0 || h == 0 || u32::from(x) + u32::from(w) > 1024 || u32::from(y) + u32::from(h) > 512 {
        return Err(invalid(
            "texture rectangle is empty or outside original VRAM",
        ));
    }
    let count = usize::from(w) * usize::from(h);
    budget.uploads = budget
        .uploads
        .checked_add(1)
        .ok_or_else(|| invalid("upload count overflow"))?;
    budget.words = budget
        .words
        .checked_add(count)
        .ok_or_else(|| invalid("upload word count overflow"))?;
    if budget.uploads > MAX_UPLOADS || budget.words > MAX_UPLOAD_WORDS {
        return Err(FieldError::new(
            "scene_resource_limit",
            "texture uploads exceed service budget (4096 uploads / 8 Mi words)",
        ));
    }
    let raw = bytes(data, pixel_at, count * 2)?;
    let pixel_end = pixel_at
        .checked_add(raw.len())
        .ok_or_else(|| invalid("pixel extent overflow"))?;
    if pixel_at < span[0] || pixel_end > span[1] || span[1] > data.len() {
        return Err(invalid("texture pixels exceed their source block"));
    }
    Ok(Upload {
        source_span: span,
        pixels_offset: pixel_at,
        rect,
        words: raw
            .chunks_exact(2)
            .map(|b| u16::from_le_bytes([b[0], b[1]]))
            .collect(),
        kind: meta.kind.into(),
        slot: meta.slot,
        tim_mode: meta.mode,
    })
}
fn tim_block(
    data: &[u8],
    at: usize,
    kind: &str,
    mode: u32,
    budget: &mut Budget,
) -> Result<(Upload, usize), FieldError> {
    let size = word(data, at)?;
    if size < 12 || !size.is_multiple_of(4) {
        return Err(invalid(
            "TIM block length must be aligned and include its rectangle",
        ));
    }
    bytes(data, at, size)?;
    let end = at + size;
    let rect = [
        half(data, at + 4)?,
        half(data, at + 6)?,
        half(data, at + 8)?,
        half(data, at + 10)?,
    ];
    Ok((
        upload(
            data,
            [at, end],
            at + 12,
            rect,
            UploadKind {
                kind,
                slot: None,
                mode: Some(mode),
            },
            budget,
        )?,
        end,
    ))
}

/// Palette component's outer offset table points to consecutive TIM records.
/// ReadTIM stops on a non-0x10 marker; a missing marker/source read is an error.
pub fn decode_palettes(full: &[u8], logical_size: usize) -> Result<TextureUploads, FieldError> {
    component(full, logical_size)?;
    let starts = offsets(full)?;
    let mut uploads = Vec::new();
    let mut budget = Budget::default();
    for offset in starts {
        start(full, offset)?;
        let mut at = offset;
        loop {
            if word(full, at)? != 0x10 {
                break;
            }
            let mode = word(full, at + 4)? as u32;
            at += 8;
            if mode & 8 != 0 {
                let (clut, end) = tim_block(full, at, "clut", mode, &mut budget)?;
                uploads.push(clut);
                at = end;
            }
            let (image, end) = tim_block(full, at, "tim_image", mode, &mut budget)?;
            uploads.push(image);
            at = end;
        }
    }
    Ok(TextureUploads {
        uploads,
        shared_slots: Vec::new(),
    })
}

/// Image component's outer entries each select one sprite slot's offset list.
/// List images are uploaded 0x40 VRAM words apart, in original list order.
pub fn decode_images(
    full: &[u8],
    logical_size: usize,
    slots: &[SpriteSlot],
) -> Result<TextureUploads, FieldError> {
    component(full, logical_size)?;
    if slots.len() != 32 {
        return Err(invalid(
            "field image upload requires the original 32 sprite slots (not an actor bound)",
        ));
    }
    let data = full;
    let starts = offsets(data)?;
    if starts.len() > slots.len() {
        return Err(invalid("field image list indexes outside sprite slots"));
    }
    let mut uploads = Vec::new();
    let mut shared_slots = Vec::new();
    let mut budget = Budget::default();
    for (slot, offset) in starts.into_iter().enumerate() {
        if slots[slot].shared != 0 {
            shared_slots.push(slot);
            continue;
        }
        start(data, offset)?;
        let list = &data[offset..];
        let image_starts = offsets(list)?;
        for (column, offset) in image_starts.into_iter().enumerate() {
            start(list, offset)?;
            let at = offset
                .checked_add(data.len() - list.len())
                .ok_or_else(|| invalid("image offset overflow"))?;
            let width = half(data, at)?;
            let height = half(data, at + 2)?;
            let x = usize::from(slots[slot].x)
                .checked_add(
                    column
                        .checked_mul(0x40)
                        .ok_or_else(|| invalid("image column overflow"))?,
                )
                .ok_or_else(|| invalid("image X overflow"))?;
            let x = u16::try_from(x).map_err(|_| invalid("image X exceeds original halfword"))?;
            let size = usize::from(width)
                .checked_mul(usize::from(height))
                .and_then(|n| n.checked_mul(2))
                .and_then(|n| n.checked_add(4))
                .ok_or_else(|| invalid("image size overflow"))?;
            bytes(data, at, size)?;
            uploads.push(upload(
                data,
                [at, at + size],
                at + 4,
                [x, slots[slot].y, width, height],
                UploadKind {
                    kind: "sprite_image",
                    slot: Some(slot),
                    mode: None,
                },
                &mut budget,
            )?);
        }
    }
    Ok(TextureUploads {
        uploads,
        shared_slots,
    })
}

/// Original field companion stream, in consecutive 2048-byte user sectors.
///
/// field.c:field_map_stream_start selects file0xb9+2*map with both placement
/// modes/base offsets zero. resident/cd_reads_and_streams.c:stream_load_image_strip
/// reads a header sector followed by one sector per strip. The first header's
/// image count is authoritative; later headers do not replace it. A non-image
/// header stops the original reader, including placeholder companion files.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MapStream {
    pub uploads: Vec<Upload>,
    pub source_bytes_read: usize,
    pub image_count: usize,
    pub stop_reason: String,
}

pub fn decode_map_stream(data: &[u8]) -> Result<MapStream, FieldError> {
    const SECTOR: usize = 2048;
    if data.len() > 32 << 20 {
        return Err(invalid("map image stream exceeds32MiB source budget"));
    }
    let mut at = 0;
    let mut uploads = Vec::new();
    let mut budget = Budget::default();
    let mut remaining = None;
    let mut image_count = 0;
    loop {
        let kind = word(data, at)?;
        if kind != 0x1200 && kind != 0x1201 {
            return Ok(MapStream {
                uploads,
                source_bytes_read: at + 4,
                image_count,
                stop_reason: "non_image_header".into(),
            });
        }
        let header = bytes(data, at, SECTOR)?;
        if remaining.is_none() {
            let count = word(header, 20)?;
            if count == 0 || count > MAX_UPLOADS {
                return Err(invalid("map image count is zero or exceeds service budget"));
            }
            remaining = Some(count);
        }
        let strips = word(header, 24)?;
        if strips == 0 || strips > (SECTOR - 28) / 2 {
            return Err(invalid(
                "map strip count is zero or exceeds its header sector",
            ));
        }
        let x = u32::from(half(header, 4)?) + u32::from(half(header, 8)?);
        let mut y = u32::from(half(header, 6)?) + u32::from(half(header, 10)?);
        let width = half(header, 12)?;
        if x > 1023 || y > 511 || width == 0 || x + u32::from(width) > 1024 {
            return Err(invalid("map image placement lies outside original VRAM"));
        }
        at += SECTOR;
        for strip in 0..strips {
            let height = half(header, 28 + strip * 2)?;
            if height == 0
                || usize::from(width) * usize::from(height) > SECTOR / 2
                || y + u32::from(height) > 512
            {
                return Err(invalid(
                    "map image strip exceeds its sector or original VRAM",
                ));
            }
            bytes(data, at, SECTOR)?;
            uploads.push(upload(
                data,
                [at, at + SECTOR],
                at,
                [x as u16, y as u16, width, height],
                UploadKind {
                    kind: if kind == 0x1200 {
                        "map_image_strip"
                    } else {
                        "map_palette_strip"
                    },
                    slot: None,
                    mode: None,
                },
                &mut budget,
            )?);
            y += u32::from(height);
            at += SECTOR;
        }
        image_count += 1;
        let left = remaining.as_mut().expect("first header count");
        *left -= 1;
        if *left == 0 {
            return Ok(MapStream {
                uploads,
                source_bytes_read: at,
                image_count,
                stop_reason: "image_count_complete".into(),
            });
        }
    }
}
