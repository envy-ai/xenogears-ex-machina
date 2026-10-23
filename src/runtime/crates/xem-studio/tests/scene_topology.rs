use xem_studio::scene::{collision, topology};

const TRIANGLES_AT: usize = 48;
const VERTICES_AT: usize = 88;
const ATTRIBUTES_AT: usize = 144;
const LOGICAL_SIZE: usize = 176;
const FULL_SIZE: usize = 184;

fn put_u32(bytes: &mut [u8], at: usize, value: u32) {
    bytes[at..at + 4].copy_from_slice(&value.to_le_bytes());
}

fn put_i16(bytes: &mut [u8], at: usize, value: i16) {
    bytes[at..at + 2].copy_from_slice(&value.to_le_bytes());
}

fn fixture(conditional_plane_read: bool) -> Vec<u8> {
    let mut bytes = vec![0xD7; FULL_SIZE];
    put_u32(&mut bytes, 0, 1);
    put_u32(&mut bytes, 4, 28);
    put_u32(&mut bytes, 8, 0x1111_2222);
    put_u32(&mut bytes, 12, 0x3333_4444);
    put_u32(&mut bytes, 16, 0x5555_6666);
    put_u32(&mut bytes, 20, ATTRIBUTES_AT as u32);
    put_u32(&mut bytes, 24, TRIANGLES_AT as u32);
    put_u32(&mut bytes, 28, VERTICES_AT as u32);

    for (index, value) in (0..14).map(|index| (index, 0x80 + index as u8)) {
        bytes[TRIANGLES_AT - 14 + index] = value;
    }
    // The original triangle[-1] reader uses the byte at +12 of this exact
    // preceding record, even when the stored neighbor indices are all -1.
    bytes[TRIANGLES_AT - 2] = 0;
    for (index, vertices) in [[0_i16, 1, 2], [2, 3, 4]].into_iter().enumerate() {
        let at = TRIANGLES_AT + index * 14;
        for (axis, value) in vertices.into_iter().enumerate() {
            put_i16(&mut bytes, at + axis * 2, value);
        }
        for (side, value) in [-1_i16, -1, -1].into_iter().enumerate() {
            put_i16(&mut bytes, at + 6 + side * 2, value);
        }
        bytes[at + 12] = if index == 0 { 2 } else { 1 };
        bytes[at + 13] = 0xE4 + index as u8;
    }
    for index in 0..5 {
        let at = VERTICES_AT + index * 8;
        for (axis, value) in [index as i16 - 12, index as i16 + 20, -3, 0x1234]
            .into_iter()
            .enumerate()
        {
            put_i16(&mut bytes, at + axis * 2, value);
        }
    }
    for (index, mut value) in [0x0000_0011, 0x0000_0022, 0x0000_0033]
        .into_iter()
        .enumerate()
    {
        if conditional_plane_read && index == 0 {
            value |= 0x0040_0000;
        }
        put_u32(&mut bytes, ATTRIBUTES_AT + index * 4, value);
    }
    bytes
}

fn decoded(source: &[u8]) -> collision::Collision {
    collision::decode_collision(source, LOGICAL_SIZE).expect("decode synthetic collision source")
}

fn assert_original_bytes_preserved(
    source: &[u8],
    output: &[u8],
    changed_header_ranges: &[std::ops::Range<usize>],
) {
    assert!(output.len() >= source.len());
    for index in 0..source.len() {
        if changed_header_ranges
            .iter()
            .any(|range| range.contains(&index))
        {
            continue;
        }
        assert_eq!(
            output[index], source[index],
            "old source byte {index:#x} is preserved"
        );
    }
}

#[test]
fn no_edit_is_exact_and_topology_rebuild_appends_reindexed_tables_with_source_sentinel() {
    let source = fixture(false);
    let original = decoded(&source);
    let (round_trip, original_logical) =
        topology::rebuild(&source, &original).expect("no-edit rebuild");
    assert_eq!(round_trip, source);
    assert_eq!(original_logical, LOGICAL_SIZE);

    let before_layer = &original.layers[0];
    let prefix_at = before_layer.triangles_offset - 14;
    let prefix = source[prefix_at..before_layer.triangles_offset].to_vec();
    let mut edited = original.clone();
    edited.layers[0].vertices.push([101, -202, 303, 404]);
    edited.layers[0].triangles[0].vertices = [5, 4, 3];
    edited.layers[0].triangles[0].neighbors = [1, -1, -1];
    edited.layers[0].triangles[0].attribute = 1;
    edited.layers[0].triangles[0].height_bump_raw = 0xA6;
    edited.layers[0].triangles.push(collision::Triangle {
        vertices: [0, 5, 4],
        neighbors: [0, 1, -1],
        attribute: 1,
        height_bump_raw: 0x5B,
    });

    let original_copy = source.clone();
    let (rebuilt, logical_size) =
        topology::rebuild(&source, &edited).expect("append edited topology");
    assert_eq!(
        source, original_copy,
        "rebuild does not mutate its borrowed source"
    );
    assert_eq!(logical_size, rebuilt.len());
    let result =
        collision::decode_collision(&rebuilt, logical_size).expect("decode rebuilt topology");
    let layer = &result.layers[0];
    assert_eq!(layer.triangles, edited.layers[0].triangles);
    assert_eq!(layer.vertices, edited.layers[0].vertices);
    assert_eq!(layer.missing_neighbor_attribute, 0);
    assert_eq!(result.attributes_offset, original.attributes_offset);
    assert_eq!(result.attributes, original.attributes[..2]);
    assert_eq!(
        result.reserved_triangle_sizes,
        original.reserved_triangle_sizes
    );
    assert_eq!(
        &rebuilt[layer.triangles_offset - 14..layer.triangles_offset],
        prefix.as_slice(),
        "the exact original pre-table record is carried forward"
    );
    assert_eq!(
        &rebuilt[ATTRIBUTES_AT..ATTRIBUTES_AT + 12],
        &source[ATTRIBUTES_AT..ATTRIBUTES_AT + 12]
    );
    assert_original_bytes_preserved(&source, &rebuilt, &[4..8, 24..32]);
    assert_ne!(layer.triangles_offset, original.layers[0].triangles_offset);
    assert_ne!(layer.vertices_offset, original.layers[0].vertices_offset);
}

#[test]
fn deleting_high_attribute_triangles_keeps_attribute_storage_but_allows_decoded_prefix_to_shrink() {
    let source = fixture(false);
    let original = decoded(&source);
    assert_eq!(original.attributes.len(), 3);
    let mut edited = original.clone();
    edited.layers[0].triangles.remove(0);
    edited.layers[0].triangles[0].neighbors = [-1, -1, -1];
    edited.layers[0].triangles[0].attribute = 1;

    let (rebuilt, logical_size) =
        topology::rebuild(&source, &edited).expect("delete one source triangle");
    let result =
        collision::decode_collision(&rebuilt, logical_size).expect("decode reduced topology");
    assert_eq!(result.layers[0].triangles, edited.layers[0].triangles);
    assert_eq!(
        result.attributes.len(),
        2,
        "only the semantically referenced prefix is decoded"
    );
    assert_eq!(result.attributes, original.attributes[..2]);
    assert_eq!(
        &rebuilt[ATTRIBUTES_AT..ATTRIBUTES_AT + 12],
        &source[ATTRIBUTES_AT..ATTRIBUTES_AT + 12]
    );
    assert_original_bytes_preserved(&source, &rebuilt, &[4..8, 24..32]);
}

#[test]
fn malformed_topology_identity_and_signed_index_bounds_fail_without_source_mutation() {
    let source = fixture(false);
    let original = decoded(&source);
    let original_copy = source.clone();

    let mut wrong_identity = original.clone();
    wrong_identity.source_sha256.push('0');
    assert!(topology::rebuild(&source, &wrong_identity).is_err());

    let mut wrong_layout = original.clone();
    wrong_layout.attributes_offset += 4;
    assert!(topology::rebuild(&source, &wrong_layout).is_err());
    let mut wrong_attributes = original.clone();
    wrong_attributes.attributes[0] ^= 1;
    assert!(topology::rebuild(&source, &wrong_attributes).is_err());

    for (label, mutate) in [
        ("negative vertex", 0_u8),
        ("past-end vertex", 1_u8),
        ("negative neighbor", 2_u8),
        ("past-end neighbor", 3_u8),
        ("missing attribute", 4_u8),
    ] {
        let mut invalid = original.clone();
        match mutate {
            0 => invalid.layers[0].triangles[0].vertices[0] = -1,
            1 => invalid.layers[0].triangles[0].vertices[0] = i16::MAX,
            2 => invalid.layers[0].triangles[0].neighbors[0] = -2,
            3 => invalid.layers[0].triangles[0].neighbors[0] = 2,
            _ => invalid.layers[0].triangles[0].attribute = u8::MAX,
        }
        assert!(
            topology::rebuild(&source, &invalid).is_err(),
            "{label} is rejected"
        );
    }

    let mut too_many_vertices = original.clone();
    too_many_vertices.layers[0].vertices.resize(32_769, [0; 4]);
    assert!(topology::rebuild(&source, &too_many_vertices).is_err());
    let mut too_many_triangles = original.clone();
    too_many_triangles.layers[0]
        .triangles
        .resize(32_769, original.layers[0].triangles[0].clone());
    assert!(topology::rebuild(&source, &too_many_triangles).is_err());

    let mut wrong_layers = original.clone();
    wrong_layers.layers.clear();
    assert!(topology::rebuild(&source, &wrong_layers).is_err());
    assert_eq!(source, original_copy);
}

#[test]
fn conditional_triangle_minus_one_plane_read_refuses_relocated_nonempty_layer() {
    let source = fixture(true);
    let original = decoded(&source);
    assert_ne!(original.attributes[0] & 0x0040_0000, 0);
    let mut edited = original.clone();
    edited.layers[0].triangles[0].vertices.swap(0, 1);

    let source_copy = source.clone();
    let error = topology::rebuild(&source, &edited)
        .expect_err("conditional plane vertex references are not closed");
    assert_eq!(error.kind, "unsupported_scene_collision");
    assert_eq!(source, source_copy);
}
