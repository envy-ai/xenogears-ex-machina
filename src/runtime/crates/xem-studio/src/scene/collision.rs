//! Source-spanned original walkmesh, with no inferred complete vertex count.
//!
//! field.c loads triangle/vertex/attribute pointers independently. Decode the
//! vertex prefix required by actual signed triangle indices; unused bytes remain
//! opaque in the source. Neighbor -1 also has an original pre-table attribute
//! read in some walkers, so that source byte is retained explicitly.
use crate::field::FieldError;
use serde::{Deserialize, Serialize};
use xem_disc::field_context::{MAX_COMPONENT, hash};

fn invalid(message: impl Into<String>) -> FieldError {
    FieldError::new("invalid_scene_collision", message)
}
fn region(data: &[u8], at: usize, size: usize) -> Result<&[u8], FieldError> {
    super::span(data, at, size).ok_or_else(|| {
        invalid(format!(
            "collision span +0x{at:x}, {size} bytes exceeds {}",
            data.len()
        ))
    })
}

fn half(data: &[u8], at: usize) -> Result<i16, FieldError> {
    let b = region(data, at, 2)?;
    Ok(i16::from_le_bytes([b[0], b[1]]))
}
fn word(data: &[u8], at: usize) -> Result<usize, FieldError> {
    let b = region(data, at, 4)?;
    Ok(u32::from_le_bytes([b[0], b[1], b[2], b[3]]) as usize)
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Triangle {
    pub vertices: [i16; 3],
    pub neighbors: [i16; 3],
    pub attribute: u8,
    /// Some queries use signed byte +13 times four as a nonnegative height bump.
    /// Preserve its exact bits rather than treating it as part of an attribute.
    pub height_bump_raw: u8,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Layer {
    pub triangles_offset: usize,
    pub vertices_offset: usize,
    pub triangles: Vec<Triangle>,
    pub vertices: Vec<[i16; 4]>,
    pub missing_neighbor_attribute: u8,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Collision {
    pub source_sha256: String,
    pub logical_size: usize,
    pub layers: Vec<Layer>,
    pub attributes_offset: usize,
    pub attributes: Vec<u32>,
    pub reserved_triangle_sizes: Vec<u32>,
}

pub fn decode_collision(full: &[u8], logical_size: usize) -> Result<Collision, FieldError> {
    if logical_size > MAX_COMPONENT - 16
        || logical_size > full.len()
        || full.len() - logical_size > 16
    {
        return Err(invalid("collision exceeds original logical+16 allocation"));
    }
    let count = word(full, 0)?;
    if !(1..=4).contains(&count) {
        return Err(invalid("original collision layer count must be 1..4"));
    }
    region(full, 0, 24 + count * 8)?;
    let attributes_offset = word(full, 20)?;
    let mut layers = Vec::with_capacity(count);
    let mut max_attribute = None;
    let mut reserved_triangle_sizes = Vec::new();
    for index in count..4 {
        reserved_triangle_sizes.push(word(full, 4 + index * 4)? as u32);
    }
    for index in 0..count {
        let size = word(full, 4 + index * 4)?;
        if !size.is_multiple_of(14) {
            return Err(invalid("triangle byte count is not a multiple of 14"));
        }
        let triangles_offset = word(full, 24 + index * 8)?;
        let vertices_offset = word(full, 28 + index * 8)?;
        if triangles_offset < 24 + count * 8 || vertices_offset < 24 + count * 8 {
            return Err(invalid("collision payload overlaps pointer table"));
        }
        let raw = region(full, triangles_offset, size)?;
        let missing_neighbor_attribute = *region(full, triangles_offset - 2, 1)?
            .first()
            .ok_or_else(|| invalid("missing neighbor attribute unavailable"))?;
        // Even a zero-triangle layer has no implicit vertex 0.
        let mut max_vertex = None;
        let mut triangles = Vec::with_capacity(size / 14);
        for data in raw.chunks_exact(14) {
            let vertices = [half(data, 0)?, half(data, 2)?, half(data, 4)?];
            let neighbors = [half(data, 6)?, half(data, 8)?, half(data, 10)?];
            if vertices.iter().any(|v| *v < 0) {
                return Err(invalid("negative collision vertex index"));
            }
            if neighbors
                .iter()
                .any(|v| *v < -1 || (*v >= 0 && *v as usize >= size / 14))
            {
                return Err(invalid(
                    "collision neighbor exceeds its layer triangle table",
                ));
            }
            for value in vertices {
                max_vertex =
                    Some(max_vertex.map_or(value as usize, |n: usize| n.max(value as usize)));
            }
            max_attribute = Some(max_attribute.map_or(data[12], |n: u8| n.max(data[12])));
            triangles.push(Triangle {
                vertices,
                neighbors,
                attribute: data[12],
                height_bump_raw: data[13],
            });
        }
        // side==7 can select -1 even without a stored -1 neighbor.
        if !triangles.is_empty() {
            max_attribute = Some(max_attribute.map_or(missing_neighbor_attribute, |n| {
                n.max(missing_neighbor_attribute)
            }));
        }
        let vertex_count = max_vertex.map_or(0, |n| n + 1);
        let raw = region(full, vertices_offset, vertex_count * 8)?;
        let vertices = raw
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
        layers.push(Layer {
            triangles_offset,
            vertices_offset,
            triangles,
            vertices,
            missing_neighbor_attribute,
        });
    }
    let attribute_count = max_attribute.map_or(0, |n| usize::from(n) + 1);
    let raw = region(full, attributes_offset, attribute_count * 4)?;
    let attributes = raw
        .chunks_exact(4)
        .map(|r| u32::from_le_bytes([r[0], r[1], r[2], r[3]]))
        .collect();
    Ok(Collision {
        source_sha256: hash(full),
        logical_size,
        layers,
        attributes_offset,
        attributes,
        reserved_triangle_sizes,
    })
}

/// Same-layout vertex/triangle patching. Validate every immutable field and all
/// indices first, then re-decode the candidate before returning bytes. Opaque
/// bytes (including unused vertices, padding and pre-table reads) are preserved.
/// Structural topology edits belong to the reference-aware scene transaction.
pub fn encode_collision(source: &[u8], value: &Collision) -> Result<Vec<u8>, FieldError> {
    let base = decode_collision(source, value.logical_size)?;
    if value.source_sha256 != base.source_sha256
        || value.layers.len() != base.layers.len()
        || value.attributes_offset != base.attributes_offset
        || value.attributes != base.attributes
        || value.reserved_triangle_sizes != base.reserved_triangle_sizes
    {
        return Err(invalid(
            "collision source identity or immutable layout differs",
        ));
    }
    for (before, after) in base.layers.iter().zip(&value.layers) {
        if before.triangles_offset != after.triangles_offset
            || before.vertices_offset != after.vertices_offset
            || before.triangles.len() != after.triangles.len()
            || before.vertices.len() != after.vertices.len()
            || before.missing_neighbor_attribute != after.missing_neighbor_attribute
        {
            return Err(invalid(
                "collision layer layout differs; structural edit requires transaction",
            ));
        }
    }
    let mut bytes = source.to_vec();
    for layer in &value.layers {
        for (index, triangle) in layer.triangles.iter().enumerate() {
            let at = layer.triangles_offset + index * 14;
            for (offset, value) in triangle
                .vertices
                .iter()
                .chain(&triangle.neighbors)
                .enumerate()
            {
                bytes[at + offset * 2..at + offset * 2 + 2].copy_from_slice(&value.to_le_bytes());
            }
            bytes[at + 12] = triangle.attribute;
            bytes[at + 13] = triangle.height_bump_raw;
        }
        for (index, vertex) in layer.vertices.iter().enumerate() {
            for (axis, value) in vertex.iter().enumerate() {
                let at = layer.vertices_offset + index * 8 + axis * 2;
                bytes[at..at + 2].copy_from_slice(&value.to_le_bytes());
            }
        }
    }
    let candidate = decode_collision(&bytes, value.logical_size)?;
    let mut expected = value.clone();
    expected.source_sha256 = candidate.source_sha256.clone();
    if candidate != expected {
        return Err(invalid(
            "collision patch changes aliased storage or inferred referenced spans",
        ));
    }
    Ok(bytes)
}
