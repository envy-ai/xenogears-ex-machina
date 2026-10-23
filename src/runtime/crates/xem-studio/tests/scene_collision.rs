use xem_studio::scene::collision;

const TRIANGLE_AT: usize = 40;
const VERTEX_AT: usize = 96;
const ATTRIBUTE_AT: usize = 136;
const LOGICAL_SIZE: usize = 168;
const FULL_SIZE: usize = 176;

#[derive(Clone, Copy)]
enum Layout {
    AttributesBeforeTriangles,
    AttributesAfterVertices,
    TriangleAndVertexAlias,
}

fn put_u32(bytes: &mut [u8], at: usize, value: u32) {
    bytes[at..at + 4].copy_from_slice(&value.to_le_bytes());
}

fn put_i16(bytes: &mut [u8], at: usize, value: i16) {
    bytes[at..at + 2].copy_from_slice(&value.to_le_bytes());
}

fn fixture(layout: Layout, vertices: [i16; 3], neighbors: [i16; 3]) -> (Vec<u8>, usize) {
    let (triangles_at, vertices_at, attributes_at) = match layout {
        Layout::AttributesBeforeTriangles => (72, VERTEX_AT, 32),
        Layout::AttributesAfterVertices => (TRIANGLE_AT, VERTEX_AT, ATTRIBUTE_AT),
        Layout::TriangleAndVertexAlias => (TRIANGLE_AT, TRIANGLE_AT, ATTRIBUTE_AT),
    };
    let mut bytes = vec![0xD0; FULL_SIZE];
    put_u32(&mut bytes, 0, 1);
    put_u32(&mut bytes, 4, 14);
    put_u32(&mut bytes, 8, 0x1111_2222);
    put_u32(&mut bytes, 12, 0x3333_4444);
    put_u32(&mut bytes, 16, 0x5555_6666);
    put_u32(&mut bytes, 20, attributes_at as u32);
    put_u32(&mut bytes, 24, triangles_at as u32);
    put_u32(&mut bytes, 28, vertices_at as u32);

    // One nonempty collision layer reads the byte immediately before the
    // first triangle table even when every stored neighbor is nonnegative.
    bytes[triangles_at - 2] = 7;
    for index in 0..8 {
        put_u32(
            &mut bytes,
            attributes_at + index * 4,
            0xA000_0000 + index as u32,
        );
    }
    for (index, value) in vertices.into_iter().enumerate() {
        put_i16(&mut bytes, triangles_at + index * 2, value);
    }
    for (index, value) in neighbors.into_iter().enumerate() {
        put_i16(&mut bytes, triangles_at + 6 + index * 2, value);
    }
    bytes[triangles_at + 12] = 2;
    bytes[triangles_at + 13] = 0xE4;

    // The file physically contains five vertices. The triangle references
    // only indices 0..2, so the remaining records must stay opaque.
    for index in 0..5 {
        let at = VERTEX_AT + index * 8;
        for axis in 0..4 {
            put_i16(&mut bytes, at + axis * 2, (index * 10 + axis) as i16 - 12);
        }
    }
    // Restore the triangle after writing the independent vertex source. In
    // the alias case the vertex pointer intentionally targets these bytes.
    for (index, value) in vertices.into_iter().enumerate() {
        put_i16(&mut bytes, triangles_at + index * 2, value);
    }
    for (index, value) in neighbors.into_iter().enumerate() {
        put_i16(&mut bytes, triangles_at + 6 + index * 2, value);
    }
    bytes[triangles_at + 12] = 2;
    bytes[triangles_at + 13] = 0xE4;
    (bytes, LOGICAL_SIZE)
}

#[test]
fn collision_attributes_before_triangles_round_trip_without_editing_opaque_bytes() {
    let (source, logical_size) = fixture(Layout::AttributesBeforeTriangles, [0, 2, 1], [0, 0, 0]);
    let decoded = collision::decode_collision(&source, logical_size).expect("decode collision");
    let layer = &decoded.layers[0];

    assert_eq!(
        decoded.source_sha256,
        xem_disc::field_context::hash(&source)
    );
    assert_eq!(decoded.logical_size, LOGICAL_SIZE);
    assert_eq!(
        decoded.reserved_triangle_sizes,
        [0x1111_2222, 0x3333_4444, 0x5555_6666]
    );
    assert_eq!(decoded.attributes_offset, 32);
    assert_eq!(decoded.attributes.len(), 8);
    assert_eq!(decoded.attributes[2], 0xA000_0002);
    assert_eq!(layer.triangles_offset, 72);
    assert_eq!(layer.vertices_offset, VERTEX_AT);
    assert_eq!(layer.missing_neighbor_attribute, 7);
    assert_eq!(layer.triangles[0].neighbors, [0, 0, 0]);
    assert_eq!(
        collision::encode_collision(&source, &decoded).expect("encode no-edit collision"),
        source
    );
}

#[test]
fn attributes_after_vertices_decode_only_the_referenced_vertex_prefix() {
    let (source, logical_size) = fixture(Layout::AttributesAfterVertices, [0, 2, 1], [-1, 0, -1]);
    let decoded = collision::decode_collision(&source, logical_size).expect("decode collision");
    let layer = &decoded.layers[0];

    assert_eq!(decoded.attributes_offset, ATTRIBUTE_AT);
    assert_eq!(layer.triangles_offset, TRIANGLE_AT);
    assert_eq!(layer.vertices_offset, VERTEX_AT);
    assert_eq!(layer.vertices.len(), 3);
    assert_eq!(layer.vertices[0], [-12, -11, -10, -9]);
    assert_eq!(layer.vertices[2], [8, 9, 10, 11]);
    assert_eq!(layer.triangles[0].vertices, [0, 2, 1]);
    assert_eq!(layer.triangles[0].neighbors, [-1, 0, -1]);
    assert_eq!(layer.triangles[0].attribute, 2);
    assert_eq!(layer.triangles[0].height_bump_raw, 0xE4);

    let before = source.clone();
    let mut edited = decoded.clone();
    edited.layers[0].vertices[1][2] = -1234;
    let encoded = collision::encode_collision(&source, &edited).expect("patch referenced vertex");
    let mut expected = source.clone();
    put_i16(&mut expected, VERTEX_AT + 8 + 4, -1234);
    assert_eq!(encoded, expected);
    assert_eq!(source, before);
    assert_eq!(
        &encoded[VERTEX_AT + 3 * 8..VERTEX_AT + 5 * 8],
        &source[VERTEX_AT + 3 * 8..VERTEX_AT + 5 * 8]
    );
    assert_eq!(
        collision::encode_collision(&source, &decoded).expect("no-edit full-byte round trip"),
        source
    );
}

#[test]
fn missing_neighbor_attribute_is_required_even_when_all_stored_neighbors_are_nonnegative() {
    let (source, logical_size) = fixture(Layout::AttributesAfterVertices, [0, 1, 2], [0, 0, 0]);
    let decoded =
        collision::decode_collision(&source, logical_size).expect("decode side-seven input");
    assert_eq!(decoded.layers[0].missing_neighbor_attribute, 7);
    assert_eq!(decoded.attributes.len(), 8);
    assert_eq!(decoded.attributes[7], 0xA000_0007);
}

#[test]
fn encoder_rejects_source_identity_changes_and_alias_conflicts_without_mutating_source() {
    let (source, logical_size) = fixture(Layout::AttributesAfterVertices, [0, 2, 1], [-1, 0, -1]);
    let decoded = collision::decode_collision(&source, logical_size).expect("decode collision");
    let mut wrong_identity = decoded.clone();
    wrong_identity.source_sha256.push('0');
    assert!(collision::encode_collision(&source, &wrong_identity).is_err());

    let (aliased, logical_size) = fixture(Layout::TriangleAndVertexAlias, [0, 2, 1], [-1, 0, -1]);
    let mut edited = collision::decode_collision(&aliased, logical_size).expect("decode alias");
    edited.layers[0].vertices[0][0] = 123;
    let original = aliased.clone();
    assert!(collision::encode_collision(&aliased, &edited).is_err());
    assert_eq!(aliased, original);
}

#[test]
fn malformed_collision_tables_sizes_indices_and_bounds_are_rejected() {
    let (source, logical_size) = fixture(Layout::AttributesAfterVertices, [0, 2, 1], [-1, 0, -1]);

    let mut bad = source.clone();
    put_u32(&mut bad, 0, 0);
    assert!(collision::decode_collision(&bad, logical_size).is_err());

    let mut bad = source.clone();
    put_u32(&mut bad, 4, 13);
    assert!(collision::decode_collision(&bad, logical_size).is_err());

    let mut bad = source.clone();
    put_u32(&mut bad, 24, 31);
    assert!(collision::decode_collision(&bad, logical_size).is_err());

    let mut bad = source.clone();
    put_i16(&mut bad, TRIANGLE_AT, -1);
    assert!(collision::decode_collision(&bad, logical_size).is_err());

    let mut bad = source.clone();
    put_i16(&mut bad, TRIANGLE_AT + 6, -2);
    assert!(collision::decode_collision(&bad, logical_size).is_err());

    let mut bad = source.clone();
    put_i16(&mut bad, TRIANGLE_AT + 6, 1);
    assert!(collision::decode_collision(&bad, logical_size).is_err());

    let mut bad = source.clone();
    put_u32(&mut bad, 28, u32::MAX);
    assert!(collision::decode_collision(&bad, logical_size).is_err());

    let mut bad = source.clone();
    put_u32(&mut bad, 20, u32::MAX);
    assert!(collision::decode_collision(&bad, logical_size).is_err());

    let (all_neighbors_present, all_neighbors_logical) =
        fixture(Layout::AttributesAfterVertices, [0, 1, 2], [0, 0, 0]);
    let mut bad = all_neighbors_present;
    let near_eof = (bad.len() - 4) as u32;
    put_u32(&mut bad, 20, near_eof);
    assert!(collision::decode_collision(&bad, all_neighbors_logical).is_err());

    assert!(collision::decode_collision(&source[..31], logical_size).is_err());
    assert!(collision::decode_collision(&source, source.len() + 1).is_err());
    assert!(collision::decode_collision(&source, LOGICAL_SIZE - 16).is_err());
}
