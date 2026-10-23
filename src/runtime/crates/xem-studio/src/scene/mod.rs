//! Original-format field scenes and source-aware authored transactions.
//!
//! Format readers are pure and shared by native and WASM. Runtime poses are not
//! authored placements; see docs/studio-milestone-7.md.
pub mod bindings;
pub mod bundle;
pub mod collision;
pub mod domain;
pub mod entrances;
pub mod formats;
pub mod geometry;
pub mod textures;
pub mod topology;

// Two bounded slices avoid architecture-dependent at+size overflow diagnostics.
fn span(data: &[u8], at: usize, size: usize) -> Option<&[u8]> {
    data.get(at..)?.get(..size)
}

use crate::field::{self, FieldError, FieldGraph};
use serde::{Deserialize, Serialize};
use xem_disc::field_context::hash;

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DecodeArgs {
    pub input: bundle::Input,
    /// Explicitly identified selectors, not a guessed ff-prefix record count.
    pub entrances: Vec<u16>,
}
#[derive(Clone, Debug, Serialize)]
pub struct ComponentSummary {
    pub index: usize,
    pub logical_size: usize,
    pub output_size: usize,
    pub source_offset: usize,
    pub source_bytes_read: usize,
    pub output_sha256: String,
}
#[derive(Clone, Debug, Serialize)]
pub struct Actor {
    pub id: String,
    pub descriptor: String,
    pub source_index: usize,
    pub placement: String,
}
#[derive(Clone, Debug, Serialize)]
pub struct Decoded {
    pub schema_version: u32,
    pub source_sha256: String,
    pub source: Option<xem_disc::field_context::SourceIdentity>,
    pub context_module_sha256: Option<String>,
    pub header: formats::Header,
    pub components: Vec<ComponentSummary>,
    pub geometry: geometry::Geometry,
    pub collision: collision::Collision,
    pub zones: formats::Zones,
    pub palettes: textures::TextureUploads,
    pub images: textures::TextureUploads,
    pub events: FieldGraph,
    pub actors: Vec<Actor>,
    pub entrances: Vec<entrances::Entrance>,
    pub diagnostics: Vec<String>,
}

/// Decode actual original structures through the same Rust path on every host.
/// This is an asset view; it does not simulate script placement or inherit VRAM
/// from an unidentified earlier scene. Such dependencies remain explicit.
pub fn decode(args: DecodeArgs) -> Result<Decoded, FieldError> {
    let source = bundle::Source::load(args.input)?;
    let c = &source.components;
    let geometry = geometry::decode_geometry(&c[2].bytes, c[2].logical_size)?;
    let collision = collision::decode_collision(&c[1].bytes, c[1].logical_size)?;
    let zones = formats::decode_zones(&c[8].bytes, c[8].logical_size)?;
    let palettes = textures::decode_palettes(&c[0].bytes, c[0].logical_size)?;
    let images = textures::decode_images(&c[4].bytes, c[4].logical_size, &source.header.slots)?;
    let events = field::decode(&c[5].bytes, Some(c[5].logical_size))?;
    if events.actors.len() > source.header.descriptors.len() {
        return Err(FieldError::new(
            "invalid_scene",
            "event actors exceed field descriptors",
        ));
    }
    for (index, descriptor) in source.header.descriptors.iter().enumerate() {
        if descriptor.flags & 0x40 == 0 && usize::from(descriptor.model) >= geometry.groups.len() {
            return Err(FieldError::new(
                "invalid_scene",
                format!("descriptor {index}: model selector exceeds original geometry group table"),
            ));
        }
    }
    let entrances = if args.entrances.is_empty() {
        Vec::new()
    } else {
        entrances::decode_entrances(&events, &args.entrances)?
    };
    if entrances
        .iter()
        .any(|e| usize::from(e.layer) >= collision.layers.len())
    {
        return Err(FieldError::new(
            "invalid_scene_entrance",
            "referenced entrance layer exceeds decoded collision layers",
        ));
    }
    let actors=(0..events.actors.len()).map(|index|Actor{id:format!("actor:{index}"),descriptor:format!("descriptor:{index}"),source_index:index,
        placement:"Descriptor source placement; scripts/floor initialization may override it. A live pose is not an authored edit.".into()}).collect();
    let components = c
        .iter()
        .enumerate()
        .map(|(index, c)| ComponentSummary {
            index,
            logical_size: c.logical_size,
            output_size: c.bytes.len(),
            source_offset: c.source_offset,
            source_bytes_read: c.source_bytes_read,
            output_sha256: hash(&c.bytes),
        })
        .collect();
    let mut diagnostics=vec!["Actor sprite VM frames and script-derived placements are not evaluated by this asset decoder; use explicit script bindings or a labelled live observation.".into(),
        "Entrances list contains only supplied selectors; variable/saved-state references can prevent structural reference closure.".into()];
    if !images.shared_slots.is_empty() {
        diagnostics.push("Shared image slots require an identified inherited VRAM source; missing pixels are not synthesized.".into());
    }
    if geometry
        .groups
        .iter()
        .flat_map(|g| &g.meshes)
        .any(|m| m.morph_offset.is_some())
    {
        diagnostics.push("Source morph data is retained, but evaluated runtime morph poses are not authored base geometry.".into());
    }
    let identity = source.input().source.clone();
    let context_module_sha256 = source.input().module_sha256.clone();
    Ok(Decoded {
        schema_version: 1,
        source_sha256: source.header.source_sha256.clone(),
        source: identity,
        context_module_sha256,
        header: source.header,
        components,
        geometry,
        collision,
        zones,
        palettes,
        images,
        events,
        actors,
        entrances,
        diagnostics,
    })
}
