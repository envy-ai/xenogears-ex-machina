use serde_json::Value;
use std::sync::OnceLock;
use xem_studio::scene::geometry;

const GROUP_BASE: usize = 8;
const MODEL_AT: usize = GROUP_BASE + 16;
const MODEL_SIZE: usize = 56;

#[derive(Clone)]
struct PrimitiveGroup {
    kind: u8,
    records: Vec<[u16; 4]>,
}

fn put_u16(bytes: &mut [u8], at: usize, value: u16) {
    bytes[at..at + 2].copy_from_slice(&value.to_le_bytes());
}

fn put_i16(bytes: &mut [u8], at: usize, value: i16) {
    bytes[at..at + 2].copy_from_slice(&value.to_le_bytes());
}

fn put_u32(bytes: &mut [u8], at: usize, value: u32) {
    bytes[at..at + 4].copy_from_slice(&value.to_le_bytes());
}

fn model_type(kind: u8) -> &'static Value {
    static TYPES: OnceLock<Vec<Value>> = OnceLock::new();
    &TYPES.get_or_init(|| {
        serde_json::from_str::<Value>(include_str!("../data/field-models.json"))
            .expect("generated field model facts")["types"]
            .as_array()
            .expect("model types array")
            .clone()
    })[usize::from(kind)]
}

fn aux_stride(kind: u8) -> usize {
    model_type(kind)["aux_stride"].as_u64().unwrap() as usize
}

fn packet_size(kind: u8) -> usize {
    model_type(kind)["packet_size"].as_u64().unwrap() as usize
}

fn has_overrides(kind: u8) -> bool {
    model_type(kind)["overrides"].as_bool().unwrap()
}

fn fixture(groups: &[PrimitiveGroup], flags: u16, vertex_count: u16, controls: bool) -> Vec<u8> {
    let face_count: usize = groups.iter().map(|g| g.records.len()).sum();
    let group_bytes: usize = groups.iter().map(|g| 4 + g.records.len() * 8).sum();
    let vertices_at = MODEL_AT + MODEL_SIZE + group_bytes;
    let vertices_bytes = usize::from(vertex_count) * 8;
    let normals_at = vertices_at + vertices_bytes;
    let aux_at = normals_at + vertices_bytes;

    let aux_bytes: usize = groups
        .iter()
        .map(|g| {
            g.records.len()
                * (aux_stride(g.kind) + usize::from(controls && has_overrides(g.kind)) * 8)
        })
        .sum();
    let mut bytes = vec![0; aux_at + aux_bytes];
    put_u32(&mut bytes, 0, 1);
    put_u32(&mut bytes, 4, GROUP_BASE as u32);
    put_u32(&mut bytes, GROUP_BASE, 1);
    put_u32(&mut bytes, GROUP_BASE + 4, 0);

    let model = MODEL_AT;
    put_u16(&mut bytes, model, flags);
    put_u16(&mut bytes, model + 2, vertex_count);
    put_u16(&mut bytes, model + 4, face_count as u16);
    put_u16(&mut bytes, model + 6, groups.len() as u16);
    put_u32(&mut bytes, model + 8, (vertices_at - GROUP_BASE) as u32);
    put_u32(&mut bytes, model + 12, (normals_at - GROUP_BASE) as u32);
    put_u32(
        &mut bytes,
        model + 16,
        (MODEL_AT + MODEL_SIZE - GROUP_BASE) as u32,
    );
    put_u32(&mut bytes, model + 20, (aux_at - GROUP_BASE) as u32);
    put_u32(&mut bytes, model + 28, 0);
    let packets: usize = groups
        .iter()
        .map(|g| g.records.len() * packet_size(g.kind))
        .sum();
    put_u32(&mut bytes, model + 52, packets as u32);

    let mut group_at = MODEL_AT + MODEL_SIZE;
    for group in groups {
        bytes[group_at] = group.kind;
        put_i16(&mut bytes, group_at + 2, group.records.len() as i16);
        group_at += 4;
        for raw in &group.records {
            for (index, value) in raw.iter().copied().enumerate() {
                put_u16(&mut bytes, group_at + index * 2, value);
            }
            group_at += 8;
        }
    }

    for index in 0..usize::from(vertex_count) {
        let vertex_at = vertices_at + index * 8;
        for (axis, value) in [index as i16, (index as i16) + 10, -((index as i16) + 1), 0]
            .into_iter()
            .enumerate()
        {
            put_i16(&mut bytes, vertex_at + axis * 2, value);
            put_i16(&mut bytes, normals_at + index * 8 + axis * 2, -value);
        }
    }

    let mut aux = aux_at;
    for group in groups {
        for face in 0..group.records.len() {
            if controls && has_overrides(group.kind) {
                put_u16(&mut bytes, aux, 0x1234);
                bytes[aux + 3] = 0xC4;
                aux += 4;
                put_u16(&mut bytes, aux, 0xABCD);
                bytes[aux + 3] = 0xC8;
                aux += 4;
            }
            let stride = aux_stride(group.kind);
            for index in 0..stride {
                bytes[aux + index] = (face as u8).wrapping_mul(16).wrapping_add(index as u8);
            }
            aux += stride;
        }
    }
    bytes
}

fn triangle() -> PrimitiveGroup {
    PrimitiveGroup {
        kind: 4,
        records: vec![[0, 0x8002, 3, 0xFFFF]],
    }
}

fn quad() -> PrimitiveGroup {
    PrimitiveGroup {
        kind: 12,
        records: vec![[0, 0x8001, 2, 3]],
    }
}

#[test]
fn group_relative_model_offsets_and_face_spans_decode_exact_raw_and_effective_indices() {
    let bytes = fixture(&[triangle(), quad()], 0, 4, false);
    let decoded = geometry::decode_geometry(&bytes, bytes.len()).expect("decode F3 model");
    let mesh = &decoded.groups[0].meshes[0];
    let face = &mesh.faces[0];

    assert_eq!(decoded.groups[0].source_offset, GROUP_BASE);
    assert_eq!(mesh.source_span, [MODEL_AT, MODEL_AT + MODEL_SIZE]);
    assert_eq!(mesh.group_offset, GROUP_BASE);
    assert_eq!(mesh.vertices_offset, MODEL_AT + MODEL_SIZE + 24);
    assert_eq!(mesh.vertices[2], [2, 12, -3, 0]);
    assert_eq!(
        face.source_span,
        [MODEL_AT + MODEL_SIZE + 4, MODEL_AT + MODEL_SIZE + 12]
    );
    assert_eq!(face.raw_indices, [0, 0x8002, 3, 0xFFFF]);
    assert_eq!(face.vertices, [0, 2, 3]);
    assert_eq!(
        face.auxiliary_span,
        [mesh.vertices_offset + 64, mesh.vertices_offset + 68]
    );
    assert_eq!(face.kind, 4);
    assert_eq!(face.uv, None);
    assert_eq!(face.diagnostic, None);

    let quad = &mesh.faces[1];
    assert_eq!(quad.kind, 12);
    assert_eq!(
        quad.source_span,
        [MODEL_AT + MODEL_SIZE + 16, MODEL_AT + MODEL_SIZE + 24]
    );
    assert_eq!(quad.raw_indices, [0, 0x8001, 2, 3]);
    assert_eq!(quad.vertices, [0, 1, 2, 3]);
    assert_eq!(
        quad.auxiliary_span,
        [mesh.vertices_offset + 68, mesh.vertices_offset + 72]
    );
}

#[test]
fn textured_triangle_records_controls_and_uv_bytes_without_inventing_other_state() {
    let group = PrimitiveGroup {
        kind: 5,
        records: vec![[0, 1, 2, 0x7777]],
    };
    let bytes = fixture(&[group], 0, 3, true);
    let decoded = geometry::decode_geometry(&bytes, bytes.len()).expect("decode FT3");
    let face = &decoded.groups[0].meshes[0].faces[0];

    assert_eq!(face.kind, 5);
    assert_eq!(face.controls.len(), 2);
    assert_eq!(face.controls[0].code, 0xC4);
    assert_eq!(face.controls[0].value, 0x1234);
    assert_eq!(face.controls[1].code, 0xC8);
    assert_eq!(face.controls[1].value, 0xABCD);
    assert_eq!(face.declared_tpage, Some(0x1234));
    assert_eq!(face.declared_clut, Some(0xABCD));
    assert_eq!(face.auxiliary, [0, 1, 2, 3, 4, 5, 6, 7]);
    assert_eq!(face.uv, Some(vec![[4, 5], [6, 7], [0, 1]]));
    assert_eq!(face.auxiliary_span[1] - face.auxiliary_span[0], 8);
    assert_eq!(face.diagnostic, None);
}

#[test]
fn environment_map_uv_remains_explicitly_unresolved() {
    let group = PrimitiveGroup {
        kind: 16,
        records: vec![[0, 1, 2, 0]],
    };
    let bytes = fixture(&[group], 0, 3, false);
    let decoded = geometry::decode_geometry(&bytes, bytes.len()).expect("decode envmap");
    let face = &decoded.groups[0].meshes[0].faces[0];

    assert_eq!(face.uv, None);
    assert_eq!(
        face.diagnostic.as_deref(),
        Some("Environment-map UVs depend on runtime normals and overrides")
    );
}

#[test]
fn malformed_dispatch_groups_indices_packet_sizes_and_truncation_are_rejected() {
    let mut bad = fixture(&[triangle()], 0, 4, false);
    bad[MODEL_AT + MODEL_SIZE] = 0xFF;
    assert_eq!(
        geometry::decode_geometry(&bad, bad.len()).unwrap_err().kind,
        "invalid_scene_geometry"
    );

    let mut bad = fixture(&[triangle()], 0, 4, false);
    put_u32(&mut bad, 4, 4);
    assert!(geometry::decode_geometry(&bad, bad.len()).is_err());

    let mut bad = fixture(&[triangle()], 0, 4, false);
    put_i16(&mut bad, MODEL_AT + MODEL_SIZE + 2, -1);
    assert!(geometry::decode_geometry(&bad, bad.len()).is_err());

    let mut bad = fixture(&[triangle()], 0, 4, false);
    put_u16(&mut bad, MODEL_AT + 4, 0);
    assert!(geometry::decode_geometry(&bad, bad.len()).is_err());

    let mut bad = fixture(&[triangle()], 0, 4, false);
    put_u16(&mut bad, MODEL_AT + 52, 0);
    assert!(geometry::decode_geometry(&bad, bad.len()).is_err());

    let mut bad = fixture(&[triangle()], 0, 4, false);
    put_u16(&mut bad, MODEL_AT + MODEL_SIZE + 4, 99);
    assert!(geometry::decode_geometry(&bad, bad.len()).is_err());

    let bytes = fixture(&[triangle()], 0, 4, false);
    assert!(geometry::decode_geometry(&bytes[..bytes.len() - 1], bytes.len() - 1).is_err());
}

#[test]
fn gouraud_preparation_requires_normal_source_even_without_the_model_flag() {
    let group = PrimitiveGroup {
        kind: 2,
        records: vec![[0, 1, 2, 0]],
    };
    let mut bytes = fixture(&[group], 0, 3, false);
    put_u32(&mut bytes, MODEL_AT + 12, u32::MAX);
    assert!(geometry::decode_geometry(&bytes, bytes.len()).is_err());
}

#[test]
fn raw_out_of_range_indices_are_rejected_instead_of_truncated() {
    let group = PrimitiveGroup {
        kind: 4,
        records: vec![[0, 1, 0xFFFF, 0]],
    };
    let bytes = fixture(&[group], 0, 3, false);
    assert!(geometry::decode_geometry(&bytes, bytes.len()).is_err());
}

#[test]
fn gouraud_raw_signed_negative_index_is_rejected_even_when_draw_mask_would_fit() {
    let group = PrimitiveGroup {
        kind: 2,
        records: vec![[0, 0x8001, 2, 0]],
    };
    let bytes = fixture(&[group], 0, 3, false);
    assert!(geometry::decode_geometry(&bytes, bytes.len()).is_err());
}

#[test]
fn cumulative_primitive_group_budget_charges_aliases_before_walking_them() {
    const GROUPS: usize = 65_535;
    const MESHES: usize = 5;
    let table_end = GROUP_BASE + 16 + MESHES * MODEL_SIZE;
    let group_headers = GROUPS * 4;
    let mut bytes = vec![0; table_end + group_headers];
    put_u32(&mut bytes, 0, 1);
    put_u32(&mut bytes, 4, GROUP_BASE as u32);
    put_u32(&mut bytes, GROUP_BASE, MESHES as u32);
    put_u32(&mut bytes, GROUP_BASE + 4, 0);
    for mesh_index in 0..MESHES {
        let model = GROUP_BASE + 16 + mesh_index * MODEL_SIZE;
        put_u16(&mut bytes, model + 6, GROUPS as u16);
        put_u32(&mut bytes, model + 8, (table_end - GROUP_BASE) as u32);
        put_u32(&mut bytes, model + 12, (table_end - GROUP_BASE) as u32);
        put_u32(&mut bytes, model + 16, (table_end - GROUP_BASE) as u32);
        put_u32(&mut bytes, model + 20, (table_end - GROUP_BASE) as u32);
    }
    for group in 0..GROUPS {
        bytes[table_end + group * 4] = 4;
        put_i16(&mut bytes, table_end + group * 4 + 2, 0);
    }

    let error = geometry::decode_geometry(&bytes, bytes.len())
        .expect_err("five aliased 65535-group meshes exceed cumulative budget");
    assert_eq!(error.kind, "scene_resource_limit");
}
