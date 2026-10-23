//! Reference-preserving collision table preparation, without a guessed source
//! vertex count. Old component storage remains opaque and unchanged except its
//! pointer/count header. New tables are appended with the exact pre-table record.
use super::collision::{self, Collision, Layer};
use crate::field::FieldError;
use xem_disc::field_context::MAX_COMPONENT;

fn invalid(message: impl Into<String>) -> FieldError {
    FieldError::new("invalid_scene_collision", message)
}
fn append(bytes: &mut Vec<u8>, value: &[u8]) -> Result<usize, FieldError> {
    let at = bytes
        .len()
        .checked_add(3)
        .map(|n| n & !3)
        .ok_or_else(|| invalid("collision append offset overflow"))?;
    let end = at
        .checked_add(value.len())
        .ok_or_else(|| invalid("collision append size overflow"))?;
    if end > MAX_COMPONENT - 16 {
        return Err(invalid("authored collision exceeds logical+16 allocation"));
    }
    bytes.resize(at, 0);
    bytes.extend_from_slice(value);
    Ok(at)
}
fn write_word(bytes: &mut [u8], at: usize, value: usize) -> Result<(), FieldError> {
    let value = u32::try_from(value).map_err(|_| invalid("collision pointer/count exceeds u32"))?;
    bytes
        .get_mut(at..at + 4)
        .ok_or_else(|| invalid("collision header span unavailable"))?
        .copy_from_slice(&value.to_le_bytes());
    Ok(())
}
fn validate(layer: &Layer, attributes: usize) -> Result<(), FieldError> {
    // Indices are signed halfwords in the original walkers, not inferred editor
    // entity maxima. A negative neighbor is only the -1 boundary sentinel.
    if layer.vertices.len() > (i16::MAX as usize) + 1
        || layer.triangles.len() > (i16::MAX as usize) + 1
    {
        return Err(invalid("collision table exceeds signed i16 index space"));
    }
    for t in &layer.triangles {
        if t.vertices
            .iter()
            .any(|i| *i < 0 || *i as usize >= layer.vertices.len())
            || t.neighbors
                .iter()
                .any(|i| *i < -1 || (*i >= 0 && *i as usize >= layer.triangles.len()))
            || usize::from(t.attribute) >= attributes
        {
            return Err(invalid(
                "authored collision has a dangling vertex, neighbor or attribute",
            ));
        }
    }
    if !layer.triangles.is_empty() && usize::from(layer.missing_neighbor_attribute) >= attributes {
        return Err(invalid(
            "pre-table collision sentinel attribute is unavailable",
        ));
    }
    Ok(())
}
/// Rebuild selected layer topology against a canonical source decode. Layer
/// count and attribute table stay fixed; their unsupported semantics are not
/// guessed. Vertex/triangle references are repaired by the domain's stable IDs
/// before reaching this bounded container operation.
pub fn rebuild(source: &[u8], value: &Collision) -> Result<(Vec<u8>, usize), FieldError> {
    let base = collision::decode_collision(source, value.logical_size)?;
    if value.source_sha256 != base.source_sha256
        || value.layers.len() != base.layers.len()
        || value.attributes != base.attributes
        || value.attributes_offset != base.attributes_offset
        || value.reserved_triangle_sizes != base.reserved_triangle_sizes
    {
        return Err(invalid(
            "collision rebuild changes source identity, layers or attribute layout",
        ));
    }
    if value == &base {
        return Ok((source.to_vec(), value.logical_size));
    }
    let mut result = source.to_vec();
    for (index, (before, after)) in base.layers.iter().zip(&value.layers).enumerate() {
        if before == after {
            continue;
        }
        if before.triangles_offset != after.triangles_offset
            || before.vertices_offset != after.vertices_offset
            || before.missing_neighbor_attribute != after.missing_neighbor_attribute
        {
            return Err(invalid(
                "collision topology changes immutable source spans/sentinel",
            ));
        }
        if !before.triangles.is_empty()
            && base
                .attributes
                .get(usize::from(before.missing_neighbor_attribute))
                .is_some_and(|bits| bits & 0x400000 != 0)
        {
            return Err(FieldError::new(
                "unsupported_scene_collision",
                "conditional triangle[-1] plane reads require source vertex-reference closure before relocating this layer",
            ));
        }
        validate(after, base.attributes.len())?;
        let pre_start = before
            .triangles_offset
            .checked_sub(14)
            .ok_or_else(|| invalid("original pre-table triangle record unavailable"))?;
        let prefix = source
            .get(pre_start..before.triangles_offset)
            .ok_or_else(|| invalid("original pre-table triangle record unavailable"))?;
        // Do not synthesize triangle[-1]. The original may read byte +12 before
        // checking the sentinel, and the bytes may alias original header fields.
        let mut records = prefix.to_vec();
        for triangle in &after.triangles {
            for word in triangle.vertices.iter().chain(&triangle.neighbors) {
                records.extend_from_slice(&word.to_le_bytes());
            }
            records.extend_from_slice(&[triangle.attribute, triangle.height_bump_raw]);
        }
        let start = append(&mut result, &records)?;
        let triangles = start + 14;
        let mut vertices = Vec::with_capacity(after.vertices.len() * 8);
        for vertex in &after.vertices {
            for word in vertex {
                vertices.extend_from_slice(&word.to_le_bytes());
            }
        }
        let vertices_at = append(&mut result, &vertices)?;
        write_word(&mut result, 4 + index * 4, after.triangles.len() * 14)?;
        write_word(&mut result, 24 + index * 8, triangles)?;
        write_word(&mut result, 28 + index * 8, vertices_at)?;
    }
    let logical_size = result.len();
    let decoded = collision::decode_collision(&result, logical_size)?;
    for (actual, expected) in decoded.layers.iter().zip(&value.layers) {
        if actual.triangles != expected.triangles
            || actual.missing_neighbor_attribute != expected.missing_neighbor_attribute
            || actual.vertices != expected.vertices[..actual.vertices.len()]
        {
            return Err(invalid(
                "rebuilt collision differs from authored referenced topology",
            ));
        }
    }
    let attribute_bytes = base.attributes.len() * 4;
    if super::span(&result, base.attributes_offset, attribute_bytes)
        != super::span(source, base.attributes_offset, attribute_bytes)
        || decoded.attributes != value.attributes[..decoded.attributes.len()]
        || decoded.reserved_triangle_sizes != value.reserved_triangle_sizes
    {
        return Err(invalid(
            "rebuilt collision changes attributes or reserved header words",
        ));
    }
    Ok((result, logical_size))
}
