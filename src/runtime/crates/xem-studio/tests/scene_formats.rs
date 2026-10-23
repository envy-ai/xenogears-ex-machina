use xem_disc::field_context::hash;
use xem_studio::scene::formats::{self, SpriteSlot, Zone, Zones};

const HEADER_END: usize = 0x190;
const DESCRIPTOR_COUNT_OFFSET: usize = 0x18c;
const DESCRIPTOR_START: usize = 0x190;
const PACKED_START: usize = DESCRIPTOR_START + 2 * 16;

fn put_u16(bytes: &mut [u8], offset: usize, value: u16) {
    bytes[offset..offset + 2].copy_from_slice(&value.to_le_bytes());
}

fn put_i16(bytes: &mut [u8], offset: usize, value: i16) {
    bytes[offset..offset + 2].copy_from_slice(&value.to_le_bytes());
}

fn put_i32(bytes: &mut [u8], offset: usize, value: i32) {
    bytes[offset..offset + 4].copy_from_slice(&value.to_le_bytes());
}

fn descriptor(
    bytes: &mut [u8],
    index: usize,
    flags: u16,
    rotation: [u16; 3],
    position: [u16; 3],
    model: u16,
) {
    let start = DESCRIPTOR_START + index * 16;
    put_u16(bytes, start, flags);
    for (axis, value) in rotation.into_iter().enumerate() {
        put_u16(bytes, start + 2 + axis * 2, value);
    }
    for (axis, value) in position.into_iter().enumerate() {
        put_u16(bytes, start + 8 + axis * 2, value);
    }
    put_u16(bytes, start + 14, model);
}

fn synthetic_bundle() -> Vec<u8> {
    let mut bytes = vec![0xA7; PACKED_START + 9 * 4];

    // Sprite slots are 8-byte records. Include signed and reserved values so
    // the decoder and encoder prove that they preserve the complete record.
    for slot in 0..32 {
        let start = slot * 8;
        put_u16(&mut bytes, start, 0x100 + slot as u16);
        put_u16(&mut bytes, start + 2, 0x200 + slot as u16);
        put_u16(&mut bytes, start + 4, 0xA000 + slot as u16);
        put_i16(
            &mut bytes,
            start + 6,
            if slot == 3 { -2 } else { slot as i16 },
        );
    }

    // Keep opaque pre-size bytes distinct from structured words.
    bytes[0x100..0x10c].copy_from_slice(b"opaque-bytes");
    for component in 0..9 {
        put_i32(&mut bytes, 0x10c + component * 4, 4);
        put_i32(
            &mut bytes,
            0x130 + component * 4,
            (PACKED_START + component * 4) as i32,
        );
        bytes[PACKED_START + component * 4..PACKED_START + component * 4 + 4].copy_from_slice(&[
            component as u8,
            0x31,
            0x52,
            0x73,
        ]);
    }
    for word in 0..28 {
        put_i16(&mut bytes, 0x154 + word * 2, (word as i16) - 14);
    }

    put_u16(&mut bytes, DESCRIPTOR_COUNT_OFFSET, 2);
    put_u16(&mut bytes, DESCRIPTOR_COUNT_OFFSET + 2, 0x5A3C);
    descriptor(
        &mut bytes,
        0,
        0x2012,
        [0x8001, 0x4567, 0xFFFF],
        [0x0001, 0x7FFF, 0xABCD],
        0x1234,
    );
    descriptor(
        &mut bytes,
        1,
        0x0040,
        [0, 0, 0],
        [0xFFFE, 0x0002, 0x8000],
        0xFEDC,
    );
    bytes
}

fn sha256(bytes: &[u8]) -> String {
    hash(bytes)
}

fn zone(corners: [[i16; 3]; 4]) -> Zone {
    Zone { corners }
}

fn zones_fixture() -> (Vec<u8>, usize, Zones) {
    let zones = vec![
        zone([[1, -2, 3], [4, 5, -6], [7, 8, 9], [-10, 11, 12]]),
        zone([[-13, 14, 15], [16, -17, 18], [19, 20, -21], [22, 23, 24]]),
    ];
    let tail = vec![0xD1, 0x00, 0xEE, 0x45, 0x89, 0xAB, 0xCD];
    let mut full = Vec::new();
    for zone in &zones {
        for corner in zone.corners {
            for coordinate in corner {
                full.extend_from_slice(&coordinate.to_le_bytes());
            }
        }
    }
    let logical_size = full.len();
    full.extend_from_slice(&tail);
    (full, logical_size, Zones { zones, tail })
}

#[test]
fn header_no_edit_round_trip_preserves_all_bytes_and_exposes_original_words() {
    let source = synthetic_bundle();
    let header = formats::decode_header(&source).expect("decode synthetic header");

    assert_eq!(header.source_sha256, sha256(&source));
    assert_eq!(header.slots.len(), 32);
    assert_eq!(
        header.slots[3],
        SpriteSlot {
            x: 0x103,
            y: 0x203,
            reserved: 0xA003,
            shared: -2
        }
    );
    assert_eq!(header.view.words[0], -14);
    assert_eq!(header.view.words[27], 13);
    assert_eq!(header.descriptors.len(), 2);
    assert_eq!(header.descriptors[0].flags, 0x2012);
    assert_eq!(header.descriptors[0].rotation, [0x8001, 0x4567, 0xFFFF]);
    assert_eq!(header.descriptors[0].position, [1, 0x7FFF, 0xABCD]);
    assert_eq!(header.descriptors[0].model, 0x1234);
    assert_eq!(header.descriptors[1].flags, 0x0040);
    assert_eq!(header.offsets[0], PACKED_START);
    assert_eq!(header.logical_sizes[8], 4);
    assert_eq!(
        formats::encode_header(&source, &header).expect("encode no-edit header"),
        source
    );
}

#[test]
fn descriptor_transform_edit_changes_only_its_rotation_and_position_words() {
    let source = synthetic_bundle();
    let mut header = formats::decode_header(&source).expect("decode synthetic header");
    header.descriptors[0].rotation = [0x8001, 0x2222, 0xFFFF];
    header.descriptors[0].position = [0x1111, 0x7FFF, 0xABCD];

    let encoded = formats::encode_header(&source, &header).expect("encode transform edit");
    let mut expected = source;
    put_u16(&mut expected, DESCRIPTOR_START + 4, 0x2222);
    put_u16(&mut expected, DESCRIPTOR_START + 8, 0x1111);
    assert_eq!(encoded, expected);
}

#[test]
fn header_encoder_rejects_changes_to_immutable_layout_and_identity() {
    let source = synthetic_bundle();
    let original = formats::decode_header(&source).expect("decode synthetic header");

    let mut changed = original.clone();
    changed.descriptors[0].flags ^= 1;
    assert!(formats::encode_header(&source, &changed).is_err());

    let mut changed = original.clone();
    changed.descriptors[0].model ^= 1;
    assert!(formats::encode_header(&source, &changed).is_err());

    let mut changed = original.clone();
    changed.slots[0].reserved ^= 1;
    assert!(formats::encode_header(&source, &changed).is_err());

    let mut changed = original.clone();
    changed.view.words[4] ^= 1;
    assert!(formats::encode_header(&source, &changed).is_err());

    let mut changed = original.clone();
    changed.offsets[0] += 1;
    assert!(formats::encode_header(&source, &changed).is_err());

    let mut changed = original;
    changed.logical_sizes[0] += 1;
    assert!(formats::encode_header(&source, &changed).is_err());

    let mut changed_source = source.clone();
    let last = changed_source.len() - 1;
    changed_source[last] ^= 1;
    let header = formats::decode_header(&source).expect("decode source identity");
    assert!(formats::encode_header(&changed_source, &header).is_err());
}

#[test]
fn sprite_descriptor_rotation_edits_are_refused() {
    let source = synthetic_bundle();
    let mut header = formats::decode_header(&source).expect("decode synthetic header");
    assert_eq!(header.descriptors[1].flags & 0x40, 0x40);
    header.descriptors[1].rotation[2] = 1;
    assert!(formats::encode_header(&source, &header).is_err());
}

#[test]
fn header_decoder_rejects_truncated_tables_and_invalid_component_bounds() {
    let source = synthetic_bundle();
    assert!(formats::decode_header(&source[..HEADER_END - 1]).is_err());

    let mut short_descriptors = source.clone();
    put_u16(&mut short_descriptors, DESCRIPTOR_COUNT_OFFSET, 3);
    short_descriptors.truncate(DESCRIPTOR_START + 2 * 16);
    assert!(formats::decode_header(&short_descriptors).is_err());

    let mut component_inside_descriptor_table = source.clone();
    put_i32(
        &mut component_inside_descriptor_table,
        0x130,
        DESCRIPTOR_START as i32,
    );
    assert!(formats::decode_header(&component_inside_descriptor_table).is_err());

    let mut outside_file = source.clone();
    let last = outside_file.len() - 1;
    put_i32(&mut outside_file, 0x130 + 8 * 4, last as i32);
    put_i32(&mut outside_file, 0x10c + 8 * 4, 4);
    assert!(formats::decode_header(&outside_file).is_err());

    let mut oversized_component = source;
    put_i32(&mut oversized_component, 0x10c, i32::MAX);
    assert!(formats::decode_header(&oversized_component).is_err());
}

#[test]
fn zones_round_trip_preserves_signed_corners_and_component_tail() {
    let (full, logical_size, expected) = zones_fixture();
    let decoded = formats::decode_zones(&full, logical_size).expect("decode synthetic zones");
    assert_eq!(decoded, expected);
    assert_eq!(formats::encode_zones(&decoded).expect("encode zones"), full);
}

#[test]
fn zone_corner_edit_preserves_every_other_corner_and_opaque_tail_byte() {
    let (full, logical_size, mut zones) = zones_fixture();
    zones.zones[1].corners[2][1] = -1234;

    let encoded = formats::encode_zones(&zones).expect("encode changed zone");
    let offset = 24 + (2 * 3 + 1) * 2;
    let mut expected = full;
    put_i16(&mut expected, offset, -1234);
    assert_eq!(encoded, expected);
    assert_eq!(
        formats::decode_zones(&encoded, logical_size).expect("decode edited zone"),
        zones
    );
}

#[test]
fn zone_decoder_rejects_a_logical_region_beyond_the_available_output() {
    let (full, _, _) = zones_fixture();
    assert!(formats::decode_zones(&full, full.len() + 1).is_err());
    assert!(formats::decode_zones(&full, usize::MAX).is_err());
}

#[test]
fn zone_decoding_and_encoding_reject_more_than_the_original_sixteen_byte_tail() {
    let corners = [[0, 1, 2], [3, 4, 5], [6, 7, 8], [9, 10, 11]];
    let oversized_tail = vec![0xA5; 17];
    let mut full = vec![0; 24];
    full.extend_from_slice(&oversized_tail);
    assert!(formats::decode_zones(&full, 24).is_err());

    let zones = Zones {
        zones: vec![zone(corners)],
        tail: oversized_tail,
    };
    assert!(formats::encode_zones(&zones).is_err());
}
