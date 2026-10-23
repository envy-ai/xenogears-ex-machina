//! Field model groups in their unrelocated source representation.
//!
//! Field load selects a group offset then views group+0x10 as SpriteModel;
//! pointer fields remain group-relative (model_relocate_group). Primitive facts
//! are generated from the original C-table reader, never maintained here.
use crate::field::FieldError;
use serde::{Deserialize, Serialize};
use std::sync::OnceLock;
use xem_disc::field_context::MAX_COMPONENT;

fn invalid(message: impl Into<String>) -> FieldError {
    FieldError::new("invalid_scene_geometry", message)
}
fn region(data: &[u8], at: usize, size: usize) -> Result<&[u8], FieldError> {
    super::span(data, at, size).ok_or_else(|| {
        invalid(format!(
            "model span +0x{at:x}, {size} bytes exceeds {}",
            data.len()
        ))
    })
}

fn half(data: &[u8], at: usize) -> Result<u16, FieldError> {
    let b = region(data, at, 2)?;
    Ok(u16::from_le_bytes([b[0], b[1]]))
}
fn word(data: &[u8], at: usize) -> Result<usize, FieldError> {
    let b = region(data, at, 4)?;
    Ok(u32::from_le_bytes([b[0], b[1], b[2], b[3]]) as usize)
}
fn relative(data: &[u8], base: usize, field: usize) -> Result<usize, FieldError> {
    let offset = word(data, field)?;
    if base > data.len() || offset > data.len() - base {
        return Err(invalid("group-relative offset exceeds component"));
    }
    Ok(base + offset)
}

#[derive(Deserialize)]
struct Metadata {
    schema_version: u32,
    types: Vec<PrimitiveType>,
    override_codes: Vec<u8>,
}
#[derive(Deserialize)]
struct PrimitiveType {
    draw: Vec<String>,
    prepare: String,
    stride: usize,
    aux_stride: usize,
    packet_size: usize,
    overrides: bool,
}
fn metadata() -> Result<&'static Metadata, FieldError> {
    static FACTS: OnceLock<Result<Metadata, String>> = OnceLock::new();
    FACTS
        .get_or_init(|| {
            serde_json::from_str::<Metadata>(include_str!("../../data/field-models.json"))
                .map_err(|e| e.to_string())
        })
        .as_ref()
        .map_err(|e| FieldError::new("invalid_model_metadata", e.clone()))
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Control {
    pub source_span: [usize; 2],
    pub code: u8,
    pub value: u16,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Face {
    pub source_span: [usize; 2],
    pub auxiliary_span: [usize; 2],
    pub kind: usize,
    pub raw_indices: [u16; 4],
    /// Draw index 1 uses only thirteen low bits, per model_draw.s.
    pub vertices: Vec<usize>,
    pub auxiliary: Vec<u8>,
    pub controls: Vec<Control>,
    /// Explicit source commands only; None means inherited/unresolved state.
    pub declared_tpage: Option<u16>,
    pub declared_clut: Option<u16>,
    pub uv: Option<Vec<[u8; 2]>>,
    pub diagnostic: Option<String>,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Mesh {
    pub source_span: [usize; 2],
    pub group_offset: usize,
    pub model_index: usize,
    pub flags: u16,
    pub vertices_offset: usize,
    pub vertices: Vec<[i16; 4]>,
    pub normals_offset: usize,
    pub primitive_groups_offset: usize,
    pub morph_offset: Option<usize>,
    pub faces: Vec<Face>,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ModelGroup {
    pub source_offset: usize,
    pub flags: u32,
    pub meshes: Vec<Mesh>,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Geometry {
    pub groups: Vec<ModelGroup>,
}

// Expanded decoder budgets, not source-format actor/model maximums. They also
// bound malicious offset-table aliases before allocating repeated payloads.
#[derive(Default)]
struct Budget {
    models: usize,
    vertices: usize,
    faces: usize,
    controls: usize,
    groups: usize,
}
impl Budget {
    fn check(&self) -> Result<(), FieldError> {
        if self.models > 4096
            || self.vertices > 262144
            || self.faces > 262144
            || self.controls > 1048576
            || self.groups > 262144
        {
            return Err(FieldError::new(
                "scene_resource_limit",
                "expanded model decode exceeds service work/output budget",
            ));
        }
        Ok(())
    }
}
fn arity(spec: &PrimitiveType) -> Result<usize, FieldError> {
    let draw = spec
        .draw
        .first()
        .ok_or_else(|| invalid("primitive has no draw entry"))?;
    // These verified draw entry-point families share model_draw.s records.
    for family in ["f", "ft", "g", "gt"] {
        if draw.starts_with(&format!("model_draw_{family}3_")) {
            return Ok(3);
        }
        if draw.starts_with(&format!("model_draw_{family}4_")) {
            return Ok(4);
        }
    }
    Err(FieldError::new(
        "unsupported_scene_geometry",
        format!("unqualified primitive draw entry {draw}"),
    ))
}
fn mesh(
    data: &[u8],
    base: usize,
    index: usize,
    facts: &Metadata,
    budget: &mut Budget,
) -> Result<Mesh, FieldError> {
    let at = base
        .checked_add(16 + index * 56)
        .ok_or_else(|| invalid("model header overflow"))?;
    region(data, at, 56)?;
    let flags = half(data, at)?;
    let vertex_count = usize::from(half(data, at + 2)?);
    let expected_faces = usize::from(half(data, at + 4)?);
    let group_count = usize::from(half(data, at + 6)?);
    budget.models += 1;
    budget.vertices += vertex_count;
    budget.faces += expected_faces;
    budget.groups += group_count;
    budget.check()?;
    let vertex_at = relative(data, base, at + 8)?;
    let normal_at = relative(data, base, at + 12)?;
    let groups_at = relative(data, base, at + 16)?;
    let mut aux = relative(data, base, at + 20)?;
    let morph = word(data, at + 28)?;
    let morph_offset = if morph == 0 {
        None
    } else {
        Some(relative(data, base, at + 28)?)
    };
    if let Some(morph) = morph_offset {
        region(data, morph, 4)?;
    }
    let raw_vertices = region(data, vertex_at, vertex_count * 8)?;
    let vertices = raw_vertices
        .chunks_exact(8)
        .map(|r| {
            [
                i16::from_le_bytes([r[0], r[1]]),
                i16::from_le_bytes([r[2], r[3]]),
                i16::from_le_bytes([r[4], r[5]]),
                i16::from_le_bytes([r[6], r[7]]),
            ]
        })
        .collect();
    if flags & 0x10 != 0 {
        region(data, normal_at, vertex_count * 8)?;
    }
    let mut record = groups_at;
    let mut faces = Vec::with_capacity(expected_faces);
    let mut tpage = None;
    let mut clut = None;
    let mut packet_bytes = 0usize;
    for _ in 0..group_count {
        let header = region(data, record, 4)?;
        let kind = usize::from(header[0]);
        let count = i16::from_le_bytes([header[2], header[3]]);
        if count < 0 {
            return Err(invalid("negative primitive-group count"));
        }
        let spec = facts.types.get(kind).ok_or_else(|| {
            invalid(format!(
                "primitive type {kind} exceeds original dispatch table"
            ))
        })?;
        if spec.stride != 8 {
            return Err(FieldError::new(
                "unsupported_scene_geometry",
                "unqualified primitive record stride",
            ));
        }
        let corners = arity(spec)?;
        record += 4;
        for _ in 0..count {
            if faces.len() >= expected_faces {
                return Err(invalid("primitive groups exceed declared face count"));
            }
            let raw = region(data, record, spec.stride)?;
            let raw_indices = [half(raw, 0)?, half(raw, 2)?, half(raw, 4)?, half(raw, 6)?];
            let mut indices = raw_indices[..corners]
                .iter()
                .map(|i| usize::from(*i))
                .collect::<Vec<_>>();
            indices[1] &= 0x1fff;
            if indices.iter().any(|i| *i >= vertex_count) {
                return Err(invalid("face vertex index outside model vertex table"));
            }
            let mut controls = Vec::new();
            if spec.overrides {
                loop {
                    let command = region(data, aux, 4)?;
                    if !facts.override_codes.contains(&command[3]) {
                        break;
                    }
                    let value = half(command, 0)?;
                    match command[3] {
                        0xc4 => tpage = Some(value),
                        0xc8 => clut = Some(value),
                        _ => return Err(invalid("unqualified texture override command")),
                    }
                    budget.controls += 1;
                    budget.check()?;
                    controls.push(Control {
                        source_span: [aux, aux + 4],
                        code: command[3],
                        value,
                    });
                    aux += 4;
                }
            }
            let auxiliary = region(data, aux, spec.aux_stride)?.to_vec();
            let textured = spec.prepare.contains("ft") || spec.prepare.contains("gt");
            let (uv, mut diagnostic) = if spec.prepare.ends_with("_envmap") {
                (
                    None,
                    Some("Environment-map UVs depend on runtime normals and overrides".into()),
                )
            } else if textured {
                let offsets = if corners == 3 {
                    vec![4, 6, 0]
                } else {
                    vec![4, 6, 8, 10]
                };
                let coords = offsets
                    .into_iter()
                    .map(|at| region(&auxiliary, at, 2).map(|r| [r[0], r[1]]))
                    .collect::<Result<Vec<_>, _>>()?;
                let diagnostic = if tpage.is_none() || clut.is_none() {
                    Some("Texture page/CLUT is inherited; no source is invented".into())
                } else {
                    None
                };
                (Some(coords), diagnostic)
            } else {
                (None, None)
            };
            let gouraud = spec.prepare.starts_with("model_prepare_g");
            if gouraud {
                // Prepare uses signed, unmasked normals indices even without
                // the optional flag. Draw index masking does not prove safety.
                if raw_indices[..corners]
                    .iter()
                    .any(|i| (*i as i16) < 0 || usize::from(*i) >= vertex_count)
                {
                    return Err(invalid(
                        "Gouraud prepare normal index exceeds declared vertex/normal table",
                    ));
                }
                region(data, normal_at, vertex_count * 8)?;
            } else if spec.prepare.ends_with("_lit")
                && raw_indices[..3]
                    .iter()
                    .any(|i| (*i as i16) < 0 || usize::from(*i) >= vertex_count)
            {
                let message = "Conditional lit preparation uses raw signed vertex indices outside the model; only draw geometry is qualified";
                diagnostic = Some(match diagnostic {
                    Some(previous) => format!("{previous}; {message}"),
                    None => message.into(),
                });
            }
            faces.push(Face {
                source_span: [record, record + spec.stride],
                auxiliary_span: [aux, aux + spec.aux_stride],
                kind,
                raw_indices,
                vertices: indices,
                auxiliary,
                controls,
                declared_tpage: tpage,
                declared_clut: clut,
                uv,
                diagnostic,
            });
            record += spec.stride;
            aux += spec.aux_stride;
            packet_bytes = packet_bytes
                .checked_add(spec.packet_size)
                .ok_or_else(|| invalid("packet size overflow"))?;
        }
    }
    if faces.len() != expected_faces || packet_bytes != word(data, at + 52)? {
        return Err(invalid(
            "model primitive/packet declarations disagree with source group walk",
        ));
    }
    Ok(Mesh {
        source_span: [at, at + 56],
        group_offset: base,
        model_index: index,
        flags,
        vertices_offset: vertex_at,
        vertices,
        normals_offset: normal_at,
        primitive_groups_offset: groups_at,
        morph_offset,
        faces,
    })
}

pub fn decode_geometry(full: &[u8], logical_size: usize) -> Result<Geometry, FieldError> {
    if logical_size > MAX_COMPONENT - 16
        || logical_size > full.len()
        || full.len() - logical_size > 16
    {
        return Err(invalid("geometry exceeds original logical+16 allocation"));
    }
    let count = word(full, 0)?;
    if count > 4096 {
        return Err(FieldError::new(
            "scene_resource_limit",
            "geometry offset table exceeds decoder group budget",
        ));
    }
    let table_size = 4 + count * 4;
    region(full, 0, table_size)?;
    let facts = metadata()?;
    if facts.schema_version != 1 {
        return Err(invalid("unsupported model metadata schema"));
    }
    let mut budget = Budget::default();
    let mut groups = Vec::with_capacity(count);
    for index in 0..count {
        let base = word(full, 4 + index * 4)?;
        if base < table_size {
            return Err(invalid("model group overlaps geometry offset table"));
        }
        let count = word(full, base)?;
        let flags = word(full, base + 4)? as u32;
        if count == 0 || flags & 1 != 0 {
            return Err(invalid(
                "expected an unrelocated nonempty field model group",
            ));
        }
        if count > 4096 || budget.models.checked_add(count).is_none_or(|n| n > 4096) {
            return Err(FieldError::new(
                "scene_resource_limit",
                "model headers exceed decoder budget",
            ));
        }
        let extent = count
            .checked_mul(56)
            .and_then(|n| n.checked_add(16))
            .ok_or_else(|| invalid("model headers overflow"))?;
        region(full, base, extent)?;
        let mut meshes = Vec::with_capacity(count);
        for index in 0..count {
            meshes.push(mesh(full, base, index, facts, &mut budget)?);
        }
        groups.push(ModelGroup {
            source_offset: base,
            flags,
            meshes,
        });
    }
    Ok(Geometry { groups })
}
