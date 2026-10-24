use xem_disc::packed::{self, PackedError};

fn literal_group_stream(declared: u32, bytes: &[u8]) -> Vec<u8> {
    assert_eq!(bytes.len() as u32, declared);
    assert_eq!(bytes.len() % 8, 0);
    let mut stream = declared.to_le_bytes().to_vec();
    for group in bytes.chunks_exact(8) {
        stream.push(0);
        stream.extend_from_slice(group);
    }
    stream.push(0); // The strict decoder's final speculative flag read.
    stream
}

fn literals(count: usize) -> Vec<u8> {
    (0..count)
        .map(|index| (index as u8).wrapping_mul(17))
        .collect()
}

#[test]
fn prefix_stops_at_required_output_without_reading_the_next_flag() {
    let data = literals(80);
    let complete = literal_group_stream(80, &data);
    let prefix_end = 4 + 9 * 9;
    let indexed_prefix = &complete[..prefix_end]; // exactly 72 literals, no tenth group flag

    let decoded =
        packed::decode_prefix(indexed_prefix, 72, 96).expect("72-byte prefix is complete");
    assert_eq!(decoded.data, data[..72]);
    assert_eq!(decoded.source_bytes_read, prefix_end);
    assert!(matches!(
        packed::decode_block(indexed_prefix, 96),
        Err(PackedError::Truncated { .. })
    ));

    let strict =
        packed::decode_block(&complete, 96).expect("strict complete block remains readable");
    assert_eq!(strict.data, data);
    assert_eq!(strict.source_bytes_read, complete.len());
}

#[test]
fn prefix_decodes_a_whole_copy_token_that_crosses_required_boundary() {
    let mut stream = 80_u32.to_le_bytes().to_vec();
    let first_64 = literals(64);
    for group in first_64.chunks_exact(8) {
        stream.push(0);
        stream.extend_from_slice(group);
    }
    stream.push(0x40); // token 6 is a three-byte back-reference
    stream.extend_from_slice(&[0xE1, 0xE2, 0xE3, 0xE4, 0xE5, 0xE6]);
    stream.extend_from_slice(&[3, 0]); // copy 3 bytes at distance 3 (output 70..73)

    let decoded =
        packed::decode_prefix(&stream, 72, 96).expect("the straddling token is available");
    let mut expected = first_64;
    expected.extend_from_slice(&[0xE1, 0xE2, 0xE3, 0xE4, 0xE5, 0xE6, 0xE4, 0xE5]);
    assert_eq!(decoded.data, expected);
    assert_eq!(decoded.source_bytes_read, stream.len());
}

#[test]
fn prefix_rejects_truncated_required_tokens_and_backrefs_beyond_advertised_output() {
    let data = literals(80);
    let complete = literal_group_stream(80, &data);
    let missing_required_token = &complete[..(4 + 8 * 9 + 8)];
    assert!(packed::decode_prefix(missing_required_token, 72, 96).is_err());

    let mut crossing = 72_u32.to_le_bytes().to_vec();
    for group in literals(64).chunks_exact(8) {
        crossing.push(0);
        crossing.extend_from_slice(group);
    }
    crossing.push(0x40);
    crossing.extend_from_slice(&[0xA1, 0xA2, 0xA3, 0xA4, 0xA5, 0xA6]);
    crossing.extend_from_slice(&[3, 0]); // this token would end at 73, beyond declared 72
    assert!(packed::decode_prefix(&crossing, 72, 88).is_err());

    let mut invalid_distance = 80_u32.to_le_bytes().to_vec();
    for group in literals(64).chunks_exact(8) {
        invalid_distance.push(0);
        invalid_distance.extend_from_slice(group);
    }
    invalid_distance.push(0x40);
    invalid_distance.extend_from_slice(&[1, 2, 3, 4, 5, 6]);
    invalid_distance.extend_from_slice(&[71, 0]); // distance exceeds the 70 decoded bytes
    assert!(matches!(
        packed::decode_prefix(&invalid_distance, 72, 96),
        Err(PackedError::InvalidDistance { .. })
    ));
}

#[test]
fn prefix_validates_header_required_size_and_output_limit() {
    let data = literals(80);
    let complete = literal_group_stream(80, &data);
    assert!(packed::decode_prefix(&complete[..3], 72, 96).is_err());
    assert!(packed::decode_prefix(&complete, 81, 96).is_err());
    assert!(packed::decode_prefix(&complete, 72, 79).is_err());
}
