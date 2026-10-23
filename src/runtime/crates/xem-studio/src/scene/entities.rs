//! Stable identities and explicit reference repair for collision topology.
//! Layers retain their source identity; vertices and faces are authored entities.
use super::{collision, domain::Entity};
use crate::field::FieldError;
use serde::Serialize;
use std::collections::BTreeMap;

fn invalid(message: impl Into<String>) -> FieldError {
    FieldError::new("invalid_scene_edit", message)
}
#[derive(Clone, Debug, Serialize)]
pub struct Face {
    pub vertices: [String; 3],
    pub neighbors: [Option<String>; 3],
    pub attribute: u8,
    pub height_bump_raw: u8,
}
#[derive(Clone, Debug, Serialize)]
pub struct Layer {
    pub id: String,
    pub vertices: Vec<Entity<[i16; 4]>>,
    pub triangles: Vec<Entity<Face>>,
}
#[derive(Clone, Debug, Serialize)]
pub struct Collision {
    pub layers: Vec<Layer>,
}
impl Collision {
    pub fn from_source(source: &collision::Collision) -> Self {
        Self {
            layers: source
                .layers
                .iter()
                .enumerate()
                .map(|(index, layer)| {
                    let id = format!("collision:{index}");
                    Layer {
                        id: id.clone(),
                        vertices: layer
                            .vertices
                            .iter()
                            .enumerate()
                            .map(|(n, v)| Entity {
                                id: format!("{id}:vertex:{n}"),
                                value: *v,
                            })
                            .collect(),
                        triangles: layer
                            .triangles
                            .iter()
                            .enumerate()
                            .map(|(n, t)| Entity {
                                id: format!("{id}:triangle:{n}"),
                                value: Face {
                                    vertices: t.vertices.map(|v| format!("{id}:vertex:{v}")),
                                    neighbors: t
                                        .neighbors
                                        .map(|v| (v >= 0).then(|| format!("{id}:triangle:{v}"))),
                                    attribute: t.attribute,
                                    height_bump_raw: t.height_bump_raw,
                                },
                            })
                            .collect(),
                    }
                })
                .collect(),
        }
    }
    pub fn resolve(
        &self,
        source: &collision::Collision,
    ) -> Result<collision::Collision, FieldError> {
        if self.layers.len() != source.layers.len() {
            return Err(invalid("collision layer count differs from source"));
        }
        let mut result = source.clone();
        for (index, layer) in self.layers.iter().enumerate() {
            if layer.id != format!("collision:{index}") {
                return Err(invalid("collision source layer identity differs"));
            }
            let vertices = indices(&layer.vertices)?;
            let triangles = indices(&layer.triangles)?;
            let target = &mut result.layers[index];
            target.vertices = layer.vertices.iter().map(|v| v.value).collect();
            target.triangles = layer
                .triangles
                .iter()
                .map(|t| {
                    Ok(collision::Triangle {
                        vertices: resolve_vertices(&t.value.vertices, &vertices)?,
                        neighbors: resolve_neighbors(&t.value.neighbors, &triangles)?,
                        attribute: t.value.attribute,
                        height_bump_raw: t.value.height_bump_raw,
                    })
                })
                .collect::<Result<_, FieldError>>()?;
        }
        Ok(result)
    }
    pub fn layer(&mut self, id: &str) -> Result<&mut Layer, FieldError> {
        self.layers
            .iter_mut()
            .find(|l| l.id == id)
            .ok_or_else(|| invalid(format!("unknown collision layer {id}")))
    }
    pub fn vertex_layer(&mut self, id: &str) -> Result<&mut Layer, FieldError> {
        self.layers
            .iter_mut()
            .find(|l| l.vertices.iter().any(|v| v.id == id))
            .ok_or_else(|| invalid(format!("unknown collision vertex {id}")))
    }
    pub fn triangle_layer(&mut self, id: &str) -> Result<&mut Layer, FieldError> {
        self.layers
            .iter_mut()
            .find(|l| l.triangles.iter().any(|t| t.id == id))
            .ok_or_else(|| invalid(format!("unknown collision triangle {id}")))
    }
}
fn indices<T>(entities: &[Entity<T>]) -> Result<BTreeMap<String, i16>, FieldError> {
    let mut result = BTreeMap::new();
    for (index, entity) in entities.iter().enumerate() {
        let index = i16::try_from(index)
            .map_err(|_| invalid("collision index exceeds signed i16 field"))?;
        if entity.id.is_empty() || result.insert(entity.id.clone(), index).is_some() {
            return Err(invalid("collision identity is empty or duplicated"));
        }
    }
    Ok(result)
}
fn resolve_vertices(
    ids: &[String; 3],
    indices: &BTreeMap<String, i16>,
) -> Result<[i16; 3], FieldError> {
    let mut values = [0; 3];
    for (value, id) in values.iter_mut().zip(ids) {
        *value = *indices
            .get(id)
            .ok_or_else(|| invalid("dangling collision vertex"))?;
    }
    Ok(values)
}
fn resolve_neighbors(
    ids: &[Option<String>; 3],
    indices: &BTreeMap<String, i16>,
) -> Result<[i16; 3], FieldError> {
    let mut values = [-1; 3];
    for (value, id) in values.iter_mut().zip(ids) {
        if let Some(id) = id {
            *value = *indices
                .get(id)
                .ok_or_else(|| invalid("dangling collision neighbor"))?;
        }
    }
    Ok(values)
}
impl Layer {
    pub fn set_vertex(&mut self, id: &str, position: [i16; 3]) -> Result<(), FieldError> {
        let vertex = self
            .vertices
            .iter_mut()
            .find(|v| v.id == id)
            .ok_or_else(|| invalid("collision vertex not found"))?;
        vertex.value[..3].copy_from_slice(&position);
        Ok(())
    }
    pub fn add_vertex(&mut self, id: String, position: [i16; 3]) -> Result<(), FieldError> {
        if self.vertices.iter().any(|v| v.id == id) {
            return Err(invalid("duplicate collision vertex identity"));
        }
        self.vertices.push(Entity {
            id,
            value: [position[0], position[1], position[2], 0],
        });
        Ok(())
    }
    pub fn delete_vertex(&mut self, id: &str) -> Result<(), FieldError> {
        if self
            .triangles
            .iter()
            .any(|t| t.value.vertices.iter().any(|v| v == id))
        {
            return Err(invalid(
                "collision vertex is still referenced by a triangle",
            ));
        }
        let at = self
            .vertices
            .iter()
            .position(|v| v.id == id)
            .ok_or_else(|| invalid("collision vertex not found"))?;
        self.vertices.remove(at);
        Ok(())
    }
    pub fn delete_triangle(&mut self, id: &str) -> Result<(), FieldError> {
        let at = self
            .triangles
            .iter()
            .position(|v| v.id == id)
            .ok_or_else(|| invalid("collision triangle not found"))?;
        self.triangles.remove(at);
        for face in &mut self.triangles {
            for neighbor in &mut face.value.neighbors {
                if neighbor.as_deref() == Some(id) {
                    *neighbor = None;
                }
            }
        }
        Ok(())
    }
    /// Joining is explicit by shared stable vertex identities, never proximity.
    /// A shared edge must have opposite winding and no third owner. No boundary
    /// is bridged after deletion; existing neighbor references become -1.
    pub fn add_triangle(
        &mut self,
        id: String,
        vertices: [String; 3],
        attribute: u8,
        height_bump_raw: u8,
    ) -> Result<(), FieldError> {
        if self.triangles.iter().any(|t| t.id == id) {
            return Err(invalid("duplicate collision triangle identity"));
        }
        if vertices[0] == vertices[1]
            || vertices[1] == vertices[2]
            || vertices[2] == vertices[0]
            || vertices
                .iter()
                .any(|id| !self.vertices.iter().any(|v| v.id == *id))
        {
            return Err(invalid(
                "triangle requires three distinct vertices from its own layer",
            ));
        }
        let mut neighbors: [Option<String>; 3] = [None, None, None];
        let mut joins = Vec::new();
        for edge in 0..3 {
            let (a, b) = (&vertices[edge], &vertices[(edge + 1) % 3]);
            for (other, triangle) in self.triangles.iter().enumerate() {
                for side in 0..3 {
                    let (c, d) = (
                        &triangle.value.vertices[side],
                        &triangle.value.vertices[(side + 1) % 3],
                    );
                    if (a == c && b == d) || (a == d && b == c) {
                        if neighbors[edge].is_some() || triangle.value.neighbors[side].is_some() {
                            return Err(invalid("collision edge would have more than two owners"));
                        }
                        if a == c {
                            return Err(invalid("shared collision edge requires opposite winding"));
                        }
                        neighbors[edge] = Some(triangle.id.clone());
                        joins.push((other, side));
                    }
                }
            }
        }
        // Candidate validation above completes before changing adjacency.
        for (other, side) in joins {
            self.triangles[other].value.neighbors[side] = Some(id.clone());
        }
        self.triangles.push(Entity {
            id,
            value: Face {
                vertices,
                neighbors,
                attribute,
                height_bump_raw,
            },
        });
        Ok(())
    }
}
