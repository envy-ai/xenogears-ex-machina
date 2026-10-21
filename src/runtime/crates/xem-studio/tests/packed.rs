use xem_studio::packed::{self, PackedError};

fn block(size: u32, tokens: &[u8], final_flags: u8) -> Vec<u8> {
    let mut source = size.to_le_bytes().to_vec();
    source.extend_from_slice(tokens);
    source.push(final_flags);
    source
}

#[test]
fn literal_group_reads_the_speculative_terminal_flag() {
    let tokens = [0, b'a', b'b', b'c', b'd', b'e', b'f', b'g', b'h'];
    let source = block(8, &tokens, 0xA5);
    let decoded = packed::decode_block(&source, 8).expect("complete literal group");

    assert_eq!(decoded.data, b"abcdefgh");
    assert_eq!(decoded.groups, 1);
    assert_eq!(decoded.token_bytes, 13);
    assert_eq!(decoded.source_bytes_read, 14);
}

#[test]
fn overlapping_back_reference_expands_from_recent_output() {
    // Token 0 is a literal A; token 1 copies distance 1, length 3. The rest
    // are literals, so the group ends exactly at the declared output length.
    let tokens = [
        0b0000_0010,
        b'A',
        1,
        0x00,
        b'B',
        b'C',
        b'D',
        b'E',
        b'F',
        b'G',
    ];
    let source = block(10, &tokens, 0);
    let decoded = packed::decode_block(&source, 10).expect("overlapping copy");

    assert_eq!(decoded.data, b"AAAABCDEFG");
    assert_eq!(decoded.groups, 1);
    assert_eq!(decoded.token_bytes, 14);
    assert_eq!(decoded.source_bytes_read, 15);
}

#[test]
fn rejects_missing_length_and_output_limit() {
    assert_eq!(
        packed::decode_block(&[1, 2, 3], 8),
        Err(PackedError::MissingLength)
    );
    assert_eq!(
        packed::decode_block(&9u32.to_le_bytes(), 8),
        Err(PackedError::OutputLimit { size: 9, limit: 8 })
    );
}

#[test]
fn reports_truncation_with_the_exact_source_offset_and_partial_output() {
    assert_eq!(
        packed::decode_block(&8u32.to_le_bytes(), 8),
        Err(PackedError::Truncated {
            position: 4,
            output: Vec::new()
        })
    );

    let source = block(8, &[0, b'x'], 0);
    assert_eq!(
        packed::decode_block(&source[..6], 8),
        Err(PackedError::Truncated {
            position: 6,
            output: vec![b'x']
        })
    );
}

#[test]
fn rejects_zero_or_too_far_back_references() {
    let zero_distance = block(8, &[1, 0, 0], 0);
    assert_eq!(
        packed::decode_block(&zero_distance, 8),
        Err(PackedError::InvalidDistance {
            distance: 0,
            position: 5
        })
    );

    let too_far = block(8, &[1, 2, 0], 0);
    assert_eq!(
        packed::decode_block(&too_far, 8),
        Err(PackedError::InvalidDistance {
            distance: 2,
            position: 5
        })
    );
}

#[test]
fn rejects_back_references_and_literal_groups_that_cross_output_length() {
    let copy_crosses = block(8, &[0b0000_0010, b'A', 1, 0x50], 0);
    assert_eq!(
        packed::decode_block(&copy_crosses, 8),
        Err(PackedError::CopyCrossesOutput { position: 6 })
    );

    let literal_crosses = block(1, &[0, b'A', b'B'], 0);
    assert_eq!(
        packed::decode_block(&literal_crosses, 1),
        Err(PackedError::FlagGroupCrossesOutput { position: 6 })
    );
}

#[test]
fn does_not_synthesize_the_speculative_terminal_flag() {
    let source = block(8, &[0, b'a', b'b', b'c', b'd', b'e', b'f', b'g', b'h'], 0);
    assert_eq!(
        packed::decode_block(&source[..source.len() - 1], 8),
        Err(PackedError::Truncated {
            position: 13,
            output: b"abcdefgh".to_vec()
        })
    );
}
