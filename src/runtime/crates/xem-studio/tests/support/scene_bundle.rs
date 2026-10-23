//! One coherent synthetic nine-component bundle for scene-command coverage.
//! All bytes are invented and kept local to tests.

use serde_json::Value;
use xem_disc::{field_context::MAX_COMPONENT, packed};

const HEADER: usize = 0x190;
const DESCRIPTOR_END: usize = HEADER + 16;
const GEOMETRY_GROUP: usize = 8;
const GEOMETRY_MODEL: usize = GEOMETRY_GROUP + 16;
const GEOMETRY_MODEL_SIZE: usize = 56;

fn put_u16(bytes: &mut [u8], offset: usize, value: u16) {
    bytes[offset..offset + 2].copy_from_slice(&value.to_le_bytes());
}

fn put_i16(bytes: &mut [u8], offset: usize, value: i16) {
    bytes[offset..offset + 2].copy_from_slice(&value.to_le_bytes());
}

fn put_u32(bytes: &mut [u8], offset: usize, value: u32) {
    bytes[offset..offset + 4].copy_from_slice(&value.to_le_bytes());
}

fn palette_component() -> Vec<u8> {
    let mut bytes = vec![0; 52];
    put_u32(&mut bytes, 0, 1);
    put_u32(&mut bytes, 4, 8);
    put_u32(&mut bytes, 8, 0x10);
    put_u32(&mut bytes, 12, 8);
    for (index, start) in [16, 32].into_iter().enumerate() {
        put_u32(&mut bytes, start, 16);
        for (axis, value) in [2 + index as u16 * 8, 3 + index as u16 * 8, 2, 1]
            .into_iter()
            .enumerate()
        {
            put_u16(&mut bytes, start + 4 + axis * 2, value);
        }
        put_u16(&mut bytes, start + 12, 0x1234 + index as u16);
        put_u16(&mut bytes, start + 14, 0xABCD - index as u16);
    }
    put_u32(&mut bytes, 48, 0x20);
    bytes
}

fn collision_component() -> Vec<u8> {
    let mut bytes = vec![0; 82];
    put_u32(&mut bytes, 0, 1);
    put_u32(&mut bytes, 4, 14);
    put_u32(&mut bytes, 20, 70);
    put_u32(&mut bytes, 24, 32);
    put_u32(&mut bytes, 28, 46);
    // Byte 30 is the actual high byte of the vertices pointer word. Its current
    // value is zero; do not overwrite header storage to fabricate a sentinel.
    for (index, value) in [0_i16, 1, 2, -1, -1, -1].into_iter().enumerate() {
        put_i16(&mut bytes, 32 + index * 2, value);
    }
    bytes[44] = 2;
    bytes[45] = 0xE4;
    for index in 0..3 {
        for axis in 0..4 {
            put_i16(
                &mut bytes,
                46 + index * 8 + axis * 2,
                (index * 10 + axis) as i16 - 7,
            );
        }
    }
    for index in 0..3 {
        put_u32(&mut bytes, 70 + index * 4, 0x1000 + index as u32);
    }
    bytes
}

fn geometry_component() -> Vec<u8> {
    let model_facts: Value = serde_json::from_str(include_str!("../../data/field-models.json"))
        .expect("generated primitive facts");
    let fact = &model_facts["types"][5]; // Original FT3 form with C4/C8 override reads.
    let aux_stride = fact["aux_stride"].as_u64().expect("FT3 auxiliary stride") as usize;
    let packet_size = fact["packet_size"].as_u64().expect("FT3 packet size") as u32;
    assert!(fact["overrides"].as_bool().expect("FT3 override dispatch"));

    let groups_at = GEOMETRY_MODEL + GEOMETRY_MODEL_SIZE;
    let records_at = groups_at + 4;
    let vertices_at = records_at + 8;
    let normals_at = vertices_at + 3 * 8;
    let controls_at = normals_at + 3 * 8;
    let aux_at = controls_at + 8;
    let mut bytes = vec![0; aux_at + aux_stride];
    put_u32(&mut bytes, 0, 1);
    put_u32(&mut bytes, 4, GEOMETRY_GROUP as u32);
    put_u32(&mut bytes, GEOMETRY_GROUP, 1);
    put_u32(&mut bytes, GEOMETRY_GROUP + 4, 0);

    put_u16(&mut bytes, GEOMETRY_MODEL + 2, 3);
    put_u16(&mut bytes, GEOMETRY_MODEL + 4, 1);
    put_u16(&mut bytes, GEOMETRY_MODEL + 6, 1);
    put_u32(
        &mut bytes,
        GEOMETRY_MODEL + 8,
        (vertices_at - GEOMETRY_GROUP) as u32,
    );
    put_u32(
        &mut bytes,
        GEOMETRY_MODEL + 12,
        (normals_at - GEOMETRY_GROUP) as u32,
    );
    put_u32(
        &mut bytes,
        GEOMETRY_MODEL + 16,
        (groups_at - GEOMETRY_GROUP) as u32,
    );
    put_u32(
        &mut bytes,
        GEOMETRY_MODEL + 20,
        (controls_at - GEOMETRY_GROUP) as u32,
    );
    put_u32(&mut bytes, GEOMETRY_MODEL + 52, packet_size);

    bytes[groups_at] = 5;
    put_i16(&mut bytes, groups_at + 2, 1);
    for (index, value) in [0_u16, 1, 2, 0xFFFF].into_iter().enumerate() {
        put_u16(&mut bytes, records_at + index * 2, value);
    }
    for index in 0..3 {
        for (axis, value) in [index as i16, 10 + index as i16, -(index as i16), 0]
            .into_iter()
            .enumerate()
        {
            put_i16(&mut bytes, vertices_at + index * 8 + axis * 2, value);
        }
    }
    put_u16(&mut bytes, controls_at, 0x3456);
    bytes[controls_at + 3] = 0xC4;
    put_u16(&mut bytes, controls_at + 4, 0x789A);
    bytes[controls_at + 7] = 0xC8;
    for index in 0..aux_stride {
        bytes[aux_at + index] = index as u8 + 0x20;
    }
    bytes
}

fn image_component() -> Vec<u8> {
    // One outer sprite slot points to a one-entry nested image list.
    let mut bytes = vec![0; 24];
    put_u32(&mut bytes, 0, 1);
    put_u32(&mut bytes, 4, 8);
    put_u32(&mut bytes, 8, 1);
    put_u32(&mut bytes, 12, 8);
    put_u16(&mut bytes, 16, 2);
    put_u16(&mut bytes, 18, 1);
    put_u16(&mut bytes, 20, 0xCAFE);
    put_u16(&mut bytes, 22, 0xBEEF);
    bytes
}

fn event_component() -> Vec<u8> {
    const BYTECODE: usize = 132 + 64;
    let mut bytes = vec![0; BYTECODE + 16];
    put_u32(&mut bytes, 128, 1);
    put_u16(&mut bytes, 132, 15);
    bytes[BYTECODE..].copy_from_slice(&[
        0xFF, 0x34, 0x12, 0xCC, 0xED, 0, 0xFF, 0xFF, 0xFE, 0xFF, 0x20, 0, 0, 7, 9, 0,
    ]);
    bytes
}

fn zone_component() -> Vec<u8> {
    let mut bytes = Vec::with_capacity(24);
    for value in [1_i16, -2, 3, 4, 5, -6, 7, 8, 9, -10, 11, 12] {
        bytes.extend_from_slice(&value.to_le_bytes());
    }
    bytes
}

pub fn components() -> Vec<Vec<u8>> {
    vec![
        palette_component(),
        collision_component(),
        geometry_component(),
        vec![0x33; 8],
        image_component(),
        event_component(),
        vec![0x66; 8],
        vec![0x77; 8],
        zone_component(),
    ]
}

pub fn bundle(components: &[Vec<u8>]) -> Vec<u8> {
    assert_eq!(components.len(), 9);
    let mut bytes = vec![0; DESCRIPTOR_END];
    put_u16(&mut bytes, 0, 10);
    put_u16(&mut bytes, 2, 20);
    put_u16(&mut bytes, 0x18C, 1);
    put_u16(&mut bytes, HEADER + 14, 0); // Descriptor selects geometry group zero.

    for (index, component) in components.iter().enumerate() {
        while !bytes.len().is_multiple_of(4) {
            bytes.push(0xCC);
        }
        let offset = bytes.len();
        let packed =
            packed::encode_literals(component, MAX_COMPONENT).expect("bounded literal stream");
        put_u32(&mut bytes, 0x10C + index * 4, component.len() as u32);
        put_u32(&mut bytes, 0x130 + index * 4, offset as u32);
        bytes.extend_from_slice(&packed);
    }
    bytes
}

pub fn valid_bundle() -> Vec<u8> {
    bundle(&components())
}

pub fn scene_args(bundle: Vec<u8>) -> Value {
    serde_json::json!({
        "input": { "bundle": bundle, "context": null, "source": null, "module_sha256": null },
        "entrances": [0, 1]
    })
}

pub fn malformed_cases() -> Vec<(&'static str, Vec<u8>)> {
    let mut bad_bundle_offset = valid_bundle();
    put_u32(&mut bad_bundle_offset, 0x130 + 2 * 4, u32::MAX);

    let mut bad_components = components();
    put_u32(&mut bad_components[0], 0, u32::MAX);
    let bad_palette_count = bundle(&bad_components);

    let mut bad_components = components();
    put_u32(&mut bad_components[2], 4, u32::MAX);
    let bad_group_offset = bundle(&bad_components);

    let mut bad_components = components();
    put_u32(&mut bad_components[2], 0, u32::MAX);
    let bad_group_count = bundle(&bad_components);

    let mut bad_components = components();
    // Group-relative source pointer in the model's vertex table.
    put_u32(&mut bad_components[2], GEOMETRY_MODEL + 8, u32::MAX);
    let bad_vertex_offset = bundle(&bad_components);

    let mut bad_components = components();
    // Group-relative source pointer in the model's normal table.
    put_u32(&mut bad_components[2], GEOMETRY_MODEL + 12, u32::MAX);
    let bad_normal_offset = bundle(&bad_components);

    vec![
        ("bundle_component_offset", bad_bundle_offset),
        ("palette_table_count", bad_palette_count),
        ("geometry_group_offset", bad_group_offset),
        ("geometry_group_count", bad_group_count),
        ("geometry_vertex_relative_offset", bad_vertex_offset),
        ("geometry_normal_relative_offset", bad_normal_offset),
    ]
}
