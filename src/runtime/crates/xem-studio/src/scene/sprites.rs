//! Original field sprite resources and a bounded first-image source preview.
//!
//! Sources: resident/sprite_construction.c resource/header/facing/frame lookup;
//! resident/sprite.c sprite_build_frame and sprite_build_cell_frame;
//! field/field.c field_actor_create_sprite; docs/scripts/sprite-vm.md.
//! No field gameplay, party state, incoming heap or sequencer state is invented.
use super::{formats::SpriteSlot, textures::Upload};
use crate::field::{FieldError, FieldGraph};
use serde::Serialize;
use std::collections::{BTreeMap, BTreeSet};

const MAX_RESOURCES: usize = 32; // Original field_sprite_slots table, not actors.
const MAX_STEPS: usize = 1024;
const MAX_WORDS: usize = 8 << 20; // Service output/work bound, not a game count.
fn output_words(budget: &mut usize, count: usize) -> Result<(), FieldError> {
    *budget = budget
        .checked_add(count)
        .ok_or_else(|| invalid("sprite output size overflow"))?;
    if *budget > MAX_WORDS {
        return Err(FieldError::new(
            "scene_resource_limit",
            "sprite previews exceed8Mi decoded words",
        ));
    }
    Ok(())
}
fn invalid(message: impl Into<String>) -> FieldError {
    FieldError::new("invalid_scene_sprite", message)
}
fn unsupported(message: impl Into<String>) -> FieldError {
    FieldError::new("unsupported_scene_sprite", message)
}
fn span(data: &[u8], at: usize, n: usize) -> Result<&[u8], FieldError> {
    super::span(data, at, n).ok_or_else(|| {
        invalid(format!(
            "sprite source span +0x{at:x}, {n} bytes exceeds {}",
            data.len()
        ))
    })
}
fn half(data: &[u8], at: usize) -> Result<u16, FieldError> {
    Ok(u16::from_le_bytes(
        span(data, at, 2)?.try_into().expect("two bytes"),
    ))
}
fn word(data: &[u8], at: usize) -> Result<usize, FieldError> {
    Ok(u32::from_le_bytes(span(data, at, 4)?.try_into().expect("four bytes")) as usize)
}
fn relative(data: &[u8], base: usize, delta: usize) -> Result<usize, FieldError> {
    if base > data.len() || delta > data.len() - base {
        return Err(invalid("sprite relative offset exceeds its section"));
    }
    Ok(base + delta)
}
#[derive(Clone, Debug, Serialize)]
pub struct Part {
    pub source_span: [usize; 2],
    pub cell_span: [usize; 2],
    pub position: [i32; 2],
    pub size: [i16; 2],
    pub uv: [u8; 2],
    pub uv_size: [u8; 2],
    pub tpage: u16,
    pub clut: u16,
    pub palette: u8,
    pub group: u8,
    pub flip_x: bool,
    pub flip_y: bool,
    pub blend: u8,
}
#[derive(Clone, Debug, Serialize)]
pub struct Clear {
    pub source_span: [usize; 2],
    pub rect: [u16; 4],
}
#[derive(Clone, Debug, Serialize)]
pub struct Frame {
    pub frame: u16,
    pub animation: u16,
    pub facing: u16,
    pub flip_x: bool,
    pub flip_y: bool,
    pub source_span: [usize; 2],
    pub parts: Vec<Part>,
    pub uploads: Vec<Upload>,
    pub clear: Option<Clear>,
}
#[derive(Clone, Debug, Serialize)]
pub struct PaletteBank {
    pub bank: u8,
    pub source_span: [usize; 2],
    pub words: Vec<u16>,
}
#[derive(Clone, Debug, Serialize)]
pub struct Resource {
    pub slot: usize,
    pub source_span: [usize; 2],
    pub palette_count: usize,
    pub palette_banks: Vec<PaletteBank>,
    pub first_frame: Option<Frame>,
    pub diagnostic: Option<String>,
}
#[derive(Clone, Debug, Serialize)]
pub struct Assets {
    pub resources: Vec<Resource>,
}

struct Choice {
    frame: u16,
    flip_x: bool,
    flip_y: bool,
}
fn first_image(animation: &[u8]) -> Result<Choice, FieldError> {
    let directory = half(animation, 0)?;
    let count = usize::from(directory & 63);
    if count == 0 {
        return Err(unsupported("resource has no directory animation0"));
    }
    span(animation, 2, count * 2)?;
    let header = usize::from(half(animation, 2)?);
    if header < 2 + count * 2 {
        return Err(invalid("animation0 header overlaps its directory"));
    }
    let flags = half(animation, header)?;
    let group = match flags & 3 {
        0 => 0,
        1 => 1,
        2 => 2,
        _ => {
            return Err(unsupported(
                "animation facing mode3 requires incoming frame-table state",
            ));
        }
    };
    let table_word = header + 4 + 2 * group;
    let table = relative(
        animation,
        table_word,
        usize::from(half(animation, table_word)?),
    )?;
    let mut pc = relative(
        animation,
        header + 2,
        usize::from(half(animation, header + 2)?),
    )?;
    if table < header + 6 || pc < header + 6 {
        return Err(invalid(
            "animation command/frame-table offset overlaps its header",
        ));
    }
    let mut index = 63u8;
    let mut flip_x = false;
    let mut flip_y = false;
    let mut stack: Vec<(usize, bool)> = Vec::new();
    let mut stack_bytes = 0usize;
    for _ in 0..MAX_STEPS {
        let op = span(animation, pc, 1)?[0];
        if op < 0x40 {
            match op >> 4 {
                0 => {
                    return Ok(Choice {
                        frame: 1,
                        flip_x,
                        flip_y,
                    });
                }
                1 => {
                    index = index.wrapping_add(1) & 63;
                    let entry = half(animation, table + usize::from(index) * 2)?;
                    flip_x = entry & 512 != 0;
                    if entry & 511 != 0 {
                        return Ok(Choice {
                            frame: entry & 511,
                            flip_x,
                            flip_y,
                        });
                    }
                }
                2 => {
                    return Err(unsupported(
                        "previous-frame command precedes a known displayed frame",
                    ));
                }
                _ => {}
            }
            pc += 1;
            continue;
        }
        match op {
            0x84 => pc += 1, // Original field dispatch has no handler: one-byte nop.
            0xb3 => {
                index = span(animation, pc + 1, 1)?[0] & 63;
                pc += 2;
            }
            0xb4 => {
                if stack_bytes >= 16 {
                    return Err(invalid("sprite byte stack overflow before first frame"));
                }
                stack.push((usize::from(span(animation, pc + 1, 1)?[0]), false));
                stack_bytes += 1;
                pc += 2;
            }
            0xbe => {
                // Both ordinary VMs consume3 despite the table's2-byte entry.
                let value = half(animation, pc + 1)?;
                let mut frame = value & 511;
                if value & 0x8000 != 0 && frame != 0 {
                    frame =
                        u16::from(span(animation, 2 + count * 2 + usize::from(frame) - 1, 1)?[0]);
                }
                flip_x = value & 512 != 0;
                flip_y = value & 1024 != 0;
                if frame != 0 {
                    return Ok(Choice {
                        frame,
                        flip_x,
                        flip_y,
                    });
                }
                pc += 3;
            }
            0xe1 | 0xe2 | 0xe4 => {
                let delta = half(animation, pc + 1)? as i16;
                if op == 0xe2 {
                    if stack_bytes + 3 > 16 {
                        return Err(invalid("sprite call stack overflow before first frame"));
                    }
                    stack.push((pc + 3, true));
                    stack_bytes += 3;
                }
                if op == 0xe4 {
                    let Some((count, false)) = stack.pop() else {
                        return Err(invalid("sprite loop has no byte count"));
                    };
                    stack_bytes -= 1;
                    if count == 0 {
                        pc += 3;
                        continue;
                    }
                    stack.push((count - 1, false));
                    stack_bytes += 1;
                }
                pc = pc
                    .checked_add_signed(isize::from(delta))
                    .ok_or_else(|| invalid("sprite jump underflows animation section"))?;
            }
            0x85 => {
                let Some((next, true)) = stack.pop() else {
                    return Err(invalid("sprite return has no call target"));
                };
                stack_bytes -= 3;
                pc = next;
            }
            0xa7 if span(animation, pc + 1, 1)?[0] & 128 == 0 => pc += 2,
            _ => {
                return Err(unsupported(format!(
                    "animation0 first image needs state/effects from sprite command0x{op:02x}"
                )));
            }
        }
    }
    Err(unsupported(
        "initial sprite animation exceeds bounded first-image inspection",
    ))
}
fn tpage(depth: u8, blend: u8, x: u16, y: u16) -> u16 {
    let rate = if blend == 0 { 0 } else { blend - 1 };
    (u16::from(depth & 1) << 7)
        | (u16::from(rate & 3) << 5)
        | ((y & 256) >> 4)
        | ((x & 1023) >> 6)
        | ((y & 512) << 2)
}
fn clut(x: u16, y: u16) -> u16 {
    ((y & 511) << 6) | ((x >> 4) & 63)
}
fn frame_parts(
    directory: &[u8],
    directory_at: usize,
    frame: Choice,
    slot: usize,
    origin: &SpriteSlot,
    budget: &mut usize,
) -> Result<Frame, FieldError> {
    let flags = half(directory, 0)?;
    if frame.frame == 0 || frame.frame > flags & 511 {
        return Err(invalid("initial frame outside sprite directory"));
    }
    let at = usize::from(half(directory, usize::from(frame.frame) * 2)?);
    let header = span(directory, at, if flags & 0x8000 != 0 { 4 } else { 6 })?;
    let count = usize::from(header[0] & 63);
    let wide = header[0] & 128 != 0;
    let cell_directory = flags & 0x8000 != 0;
    let refs_at = at + if cell_directory { 4 } else { 6 };
    let ref_stride = if cell_directory { 2 } else { 4 };
    span(directory, refs_at, count * ref_stride)?;
    let mut pc = refs_at + count * ref_stride;
    let mut groups = [[0i32; 3]; 8];
    let mut group = 4usize;
    let mut parts = Vec::with_capacity(count);
    let mut uploads = Vec::new();
    for index in 0..count {
        let control_start = pc;
        let mut dx = 0i8;
        let mut dy = 0i8;
        let mut flip_y = false;
        for step in 0..MAX_STEPS {
            let cmd = span(directory, pc, 1)?[0];
            if cmd & 128 == 0 {
                break;
            }
            if step + 1 == MAX_STEPS {
                return Err(invalid("sprite part control prefix exceeds service budget"));
            }
            pc += 1;
            if cmd & 64 != 0 {
                group = usize::from(cmd & 7);
                if cmd & 32 != 0 {
                    let v = span(directory, pc, 2)?;
                    groups[group][0] = i32::from(v[0] as i8);
                    groups[group][1] = i32::from(v[1] as i8);
                    pc += 2;
                }
                groups[group][2] = if cmd & 16 != 0 {
                    let v = span(directory, pc, 1)?[0];
                    pc += 1;
                    i32::from(v) << 4
                } else {
                    0
                };
            } else {
                if cmd & 4 != 0 {
                    flip_y = true;
                }
                if cmd & 1 != 0 {
                    dx = span(directory, pc, 1)?[0] as i8;
                    pc += 1;
                }
                if cmd & 2 != 0 {
                    dy = span(directory, pc, 1)?[0] as i8;
                    pc += 1;
                }
            }
        }
        let fields = span(directory, pc, if wide { 5 } else { 3 })?;
        let palette = fields[0] & 15;
        let position = if wide {
            [
                i32::from(i16::from_le_bytes([fields[1], fields[2]])),
                i32::from(i16::from_le_bytes([fields[3], fields[4]])),
            ]
        } else {
            [i32::from(fields[1] as i8), i32::from(fields[2] as i8)]
        };
        pc += if wide { 5 } else { 3 };
        let reference = refs_at + index * ref_stride;
        let cell_at = usize::from(half(directory, reference)?) * if cell_directory { 1 } else { 4 };
        let cell = span(directory, cell_at, if cell_directory { 5 } else { 4 })?;
        let (kind, u, v, w, h) = if cell_directory {
            let kind = cell[0];
            if kind & 16 != 0 {
                return Err(unsupported(
                    "first frame needs resident sprite pages/palette; no inherited pixels are invented",
                ));
            }
            // Initial field constructors have no sequencer buffer. Commands
            // allocating/binding one are rejected before first_image returns.
            let ubase = if kind & 1 != 0 {
                (origin.x & 63) >> 1
            } else {
                (origin.x & 63) >> 2
            };
            (
                kind,
                ((ubase + u16::from(cell[1])) & 255) as u8,
                (((origin.y & 255) + u16::from(cell[2])) & 255) as u8,
                cell[3],
                cell[4],
            )
        } else {
            (cell[2], 0, 0, cell[0], cell[1])
        };
        let page_x = if cell_directory {
            u32::from(origin.x) + ((u32::from(kind) << 5) & 0x1c0)
        } else {
            u32::from(origin.x)
        };
        // Cell-directory page selection calls GetTPage, whose source macro
        // masks x&0x3ff and encodes y's0x100/0x200 bits. Origin960 plus a64-word cell page therefore
        // intentionally selects page0; this is address encoding, not padding
        // or guessed VRAM. Embedded-cell upload rectangles remain bounded below.
        if !cell_directory && (page_x > 1023 || origin.y > 511) {
            return Err(invalid("sprite source page lies outside VRAM"));
        }
        let (u, v) = if cell_directory {
            (u, v)
        } else {
            let placed = half(directory, reference + 2)?;
            let px = placed & 31;
            let py = (placed >> 5) & 63;
            let word_width = u16::from(w) >> if kind & 1 != 0 { 1 } else { 2 };
            let pixel_bytes = usize::from(word_width) * usize::from(h) * 2;
            let pixels_at = cell_at + 4;
            let pixels = span(directory, pixels_at, pixel_bytes)?;
            output_words(budget, pixel_bytes / 2)?;
            if u32::from(origin.x) + u32::from(px) + u32::from(word_width) > 1024
                || u32::from(origin.y) + u32::from(py) + u32::from(h) > 512
            {
                return Err(invalid("sprite cell upload exceeds VRAM"));
            }
            uploads.push(Upload {
                source_span: [
                    directory_at + cell_at,
                    directory_at + pixels_at + pixel_bytes,
                ],
                pixels_offset: directory_at + pixels_at,
                rect: [origin.x + px, origin.y + py, word_width, u16::from(h)],
                words: pixels
                    .chunks_exact(2)
                    .map(|v| u16::from_le_bytes([v[0], v[1]]))
                    .collect(),
                kind: "sprite_cell".into(),
                slot: Some(slot),
                tim_mode: None,
            });
            (
                (((origin.x & 63) * if kind & 1 != 0 { 2 } else { 4 }
                    + px * if kind & 1 != 0 { 2 } else { 4 })
                    & 255) as u8,
                (((origin.y & 255) + py) & 255) as u8,
            )
        };
        if w == 0 || h == 0 {
            return Err(invalid("sprite part has an empty pixel rectangle"));
        }
        parts.push(Part {
            source_span: [directory_at + control_start, directory_at + pc],
            cell_span: [
                directory_at + cell_at,
                directory_at + cell_at + if cell_directory { 5 } else { 4 },
            ],
            position,
            size: [i16::from(w) + i16::from(dx), i16::from(h) + i16::from(dy)],
            uv: [u, v],
            uv_size: [w, h],
            tpage: tpage(kind, (fields[0] >> 4) & 3, page_x as u16, origin.y),
            clut: clut(256 + u16::from(palette) * 16, 480 + slot as u16),
            palette,
            group: group as u8,
            flip_x: fields[0] & 64 != 0,
            flip_y,
            blend: (fields[0] >> 4) & 3,
        });
    }
    // Build mutates a shared group table; draw reads its FINAL state, so a
    // later control also changes earlier parts in the same group.
    for part in &mut parts {
        let state = groups[usize::from(part.group)];
        if state[2] != 0 {
            return Err(unsupported(
                "first frame uses a rotated part group; no unqualified flat pose is substituted",
            ));
        }
        part.position[0] += state[0];
        part.position[1] += state[1];
    }
    // Upload queue prepends: the final clear executes first, then cells in
    // reverse enqueue order. Palette upload (outside this reader) executes last.
    let mut clear_command = None;
    if !cell_directory {
        uploads.reverse();
        let clear = span(directory, at + 4, 2)?;
        let w = u16::from(clear[0]);
        let h = u16::from(clear[1]);
        if w > 0 && h > 0 {
            if u32::from(origin.x) + u32::from(w) > 1024 || u32::from(origin.y) + u32::from(h) > 512
            {
                return Err(invalid("sprite frame clear exceeds original VRAM"));
            }
            let words = usize::from(w) * usize::from(h);
            output_words(budget, words)?;
            clear_command = Some(Clear {
                source_span: [directory_at + at + 4, directory_at + at + 6],
                rect: [origin.x, origin.y, w, h],
            });
        }
    }
    Ok(Frame {
        frame: frame.frame,
        animation: 0,
        facing: 0,
        flip_x: frame.flip_x,
        flip_y: frame.flip_y,
        source_span: [directory_at + at, directory_at + pc],
        parts,
        uploads,
        clear: clear_command,
    })
}

pub fn decode(data: &[u8], logical: usize, slots: &[SpriteSlot]) -> Result<Assets, FieldError> {
    if logical > xem_disc::field_context::MAX_COMPONENT - 16
        || logical > data.len()
        || data.len() - logical > 16
    {
        return Err(invalid("sprite component exceeds its declared allocation"));
    }
    if data.is_empty() && logical == 0 {
        return Ok(Assets {
            resources: Vec::new(),
        });
    }
    let count = word(data, 0)?;
    if count > MAX_RESOURCES || count > slots.len() {
        return Err(invalid(
            "sprite resource count exceeds the original32 texture slots",
        ));
    }
    let table_end = 4 + count * 4;
    span(data, 0, table_end)?;
    let mut resources = Vec::with_capacity(count);
    let mut budget = 0;
    for (slot, origin) in slots.iter().take(count).enumerate() {
        let at = word(data, 4 + slot * 4)?;
        if at < table_end {
            return Err(invalid("sprite resource overlaps outer directory"));
        }
        let n = word(data, at)?;
        if !(3..=16).contains(&n) {
            return Err(invalid(
                "sprite resource section count outside bounded supported layout",
            ));
        }
        let first = 8 + 4 * n;
        span(data, at, first)?;
        let mut offsets = Vec::with_capacity(n + 1);
        for i in 0..=n {
            offsets.push(word(data, at + 4 + i * 4)?);
        }
        if offsets[0] != first || offsets.windows(2).any(|p| p[0] > p[1]) {
            return Err(invalid(
                "sprite resource sections are unordered or overlap header",
            ));
        }
        let size = offsets[n];
        let block = span(data, at, size)?;
        let animation = &block[offsets[0]..offsets[1]];
        let directory = &block[offsets[1]..offsets[2]];
        let palette = &block[offsets[2]..offsets[3]];
        let palette_count = usize::from(half(palette, 0)?);
        if palette_count > 48 {
            return Err(invalid("sprite palette width exceeds its field CLUT row"));
        }
        let mut palette_banks = Vec::new();
        if palette_count != 0 {
            let stride = palette_count * 32;
            span(palette, 0, 4)?;
            let banks = ((palette.len() - 4) / stride).min(16);
            for bank in 0..banks {
                let begin = 4 + bank * stride;
                output_words(&mut budget, stride / 2)?;
                let words = span(palette, begin, stride)?
                    .chunks_exact(2)
                    .map(|p| u16::from_le_bytes([p[0], p[1]]))
                    .collect();
                palette_banks.push(PaletteBank {
                    bank: bank as u8,
                    source_span: [at + offsets[2] + begin, at + offsets[2] + begin + stride],
                    words,
                });
            }
            if palette_banks.is_empty() {
                return Err(invalid("sprite palette colours are truncated"));
            }
        }
        let result = first_image(animation).and_then(|frame| {
            frame_parts(directory, at + offsets[1], frame, slot, origin, &mut budget)
        });
        let (first_frame, diagnostic) = match result {
            Ok(frame) => (Some(frame), None),
            Err(error) if error.kind == "unsupported_scene_sprite" => (None, Some(error.message)),
            Err(error) => return Err(error),
        };
        resources.push(Resource {
            slot,
            source_span: [at, at + size],
            palette_count,
            palette_banks,
            first_frame,
            diagnostic,
        });
    }
    Ok(Assets { resources })
}

#[derive(Clone, Debug, Serialize)]
pub struct Binding {
    pub actor: String,
    pub node: Option<String>,
    pub pc: Option<usize>,
    pub resource: Option<usize>,
    pub bank: Option<u8>,
    pub diagnostic: Option<String>,
}
pub fn bindings(
    graph: &FieldGraph,
    actors: &[String],
    nodes: &BTreeMap<String, usize>,
) -> Vec<Binding> {
    actors
        .iter()
        .enumerate()
        .map(|(actor, id)| {
            let mut result = Binding {
                actor: id.clone(),
                node: None,
                pc: None,
                resource: None,
                bank: None,
                diagnostic: None,
            };
            let Some(entries) = graph.actors.get(actor) else {
                result.diagnostic = Some("sprite actor has no canonical event row".into());
                return result;
            };
            let mut pc = usize::from(entries[0]);
            let mut stack = Vec::new();
            let mut seen = BTreeSet::new();
            for _ in 0..MAX_STEPS {
                if !seen.insert((pc, stack.clone())) {
                    result.diagnostic =
                        Some("initialization loops before a qualified sprite construction".into());
                    return result;
                }
                let Some(ins) = graph
                    .instructions
                    .binary_search_by_key(&pc, |i| i.pc)
                    .ok()
                    .and_then(|index| graph.instructions.get(index))
                else {
                    result.diagnostic =
                        Some("sprite initialization does not name a decoded instruction".into());
                    return result;
                };
                match ins.spec.handler.as_str() {
                    "field_event_set_sprite" | "field_event_set_sprite_parameter" => {
                        let banked = ins.spec.handler == "field_event_set_sprite_parameter";
                        if ins.immediate.first() != Some(&Some(true))
                            || banked && ins.immediate.get(1) != Some(&Some(true))
                        {
                            result.diagnostic =
                                Some("sprite resource/bank depends on a runtime variable".into());
                            return result;
                        }
                        let Some(raw) = ins.operands.first() else {
                            result.diagnostic =
                                Some("sprite construction has no decoded resource operand".into());
                            return result;
                        };
                        let resource = (raw & 0x7fff) as usize;
                        let bank = if banked {
                            let Some(raw) = ins.operands.get(1) else {
                                result.diagnostic =
                                    Some("sprite construction has no decoded bank operand".into());
                                return result;
                            };
                            raw & 0x7fff
                        } else {
                            0
                        };
                        if resource >= 32 || bank > 15 {
                            result.diagnostic = Some(
                                "sprite selector/bank exceeds the qualified source slot/mode"
                                    .into(),
                            );
                            return result;
                        }
                        result.resource = Some(resource);
                        result.bank = Some(bank as u8);
                        result.pc = Some(pc);
                        result.node = nodes
                            .iter()
                            .find(|(_, p)| **p == pc)
                            .map(|(n, _)| n.clone());
                        return result;
                    }
                    "field_event_show_first_sprite"
                    | "field_event_set_layer_sprite"
                    => {
                        result.diagnostic = Some(format!(
                            "initialization uses unqualified sprite constructor {}; no later replacement is inferred",
                            ins.spec.handler
                        ));
                        return result;
                    }
                    "field_event_set_party_sprite"
                    | "field_event_become_party_slot"
                    | "field_event_become_party_character" => {
                        result.diagnostic = Some(
                            "party sprite appearance requires identified party/character state"
                                .into(),
                        );
                        return result;
                    }
                    _ => {}
                }
                match ins.spec.flow.as_str() {
                    "call" => {
                        if stack.len() >= 4 {
                            break;
                        }
                        let Some(target) = ins
                            .spec
                            .operands
                            .iter()
                            .zip(&ins.operands)
                            .find(|(s, _)| s.kind == "addr")
                            .map(|(_, v)| *v as usize)
                        else {
                            break;
                        };
                        stack.push((pc + ins.size) & 0xffff);
                        pc = target;
                    }
                    "return" => {
                        let Some(next) = stack.pop() else {
                            break;
                        };
                        pc = next;
                    }
                    "branch" | "switch" | "hang" | "stop" => {
                        result.diagnostic = Some(
                            "sprite appearance depends on conditional or runtime initialization"
                                .into(),
                        );
                        return result;
                    }
                    _ => {
                        if ins.successors.len() != 1 {
                            break;
                        }
                        pc = ins.successors[0];
                    }
                }
            }
            result.diagnostic = Some(
                "no unconditional local sprite construction is established by initialization"
                    .into(),
            );
            result
        })
        .collect()
}
