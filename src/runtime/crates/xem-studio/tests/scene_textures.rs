use xem_studio::scene::{formats::SpriteSlot, textures};

fn put_u16(bytes: &mut [u8], at: usize, value: u16) {
    bytes[at..at + 2].copy_from_slice(&value.to_le_bytes());
}

fn put_u32(bytes: &mut [u8], at: usize, value: u32) {
    bytes[at..at + 4].copy_from_slice(&value.to_le_bytes());
}

fn put_tim_block(bytes: &mut [u8], at: usize, rect: [u16; 4], words: &[u16]) -> usize {
    let size = 12 + words.len() * 2;
    assert_eq!(size % 4, 0);
    put_u32(bytes, at, size as u32);
    for (index, value) in rect.into_iter().enumerate() {
        put_u16(bytes, at + 4 + index * 2, value);
    }
    for (index, &value) in words.iter().enumerate() {
        put_u16(bytes, at + 12 + index * 2, value);
    }
    at + size
}

fn palette_tim() -> (Vec<u8>, usize, u32) {
    let logical_size = 48;
    let mut full = vec![0; logical_size + 4];
    put_u32(&mut full, 0, 1);
    put_u32(&mut full, 4, 8);
    let mode = 0xABC0_0008;
    put_u32(&mut full, 8, 0x10);
    put_u32(&mut full, 12, mode);
    let end_clut = put_tim_block(&mut full, 16, [10, 20, 2, 1], &[0x1111, 0x2222]);
    assert_eq!(end_clut, 32);
    let end_image = put_tim_block(&mut full, end_clut, [30, 40, 2, 1], &[0x3456, 0x789A]);
    assert_eq!(end_image, logical_size);

    // The original ReadTIM loop stops when the next word is not 0x10.
    // Keep this marker inside the bounded logical data and put a valid-looking
    // number immediately after it so the decoder cannot silently continue.
    put_u32(&mut full, logical_size, 0x20);
    (full, logical_size, mode)
}

fn slot(x: u16, y: u16, shared: i16) -> SpriteSlot {
    SpriteSlot {
        x,
        y,
        reserved: 0xA55A,
        shared,
    }
}

fn image_lists_fixture() -> Vec<u8> {
    // Outer slot table (2 records), then one nested list. The second outer
    // pointer is intentionally invalid: slot 1 is shared and its image list is
    // not read by the original field loader.
    let mut full = vec![0; 40];
    put_u32(&mut full, 0, 2);
    put_u32(&mut full, 4, 12);
    put_u32(&mut full, 8, u32::MAX);

    // Nested count and two offsets, each relative to this list's start.
    put_u32(&mut full, 12, 2);
    put_u32(&mut full, 16, 12);
    put_u32(&mut full, 20, 20);

    // Width/height followed by the raw words, matching the sprite loader.
    put_u16(&mut full, 24, 2);
    put_u16(&mut full, 26, 1);
    put_u16(&mut full, 28, 0x1234);
    put_u16(&mut full, 30, 0x5678);
    put_u16(&mut full, 32, 2);
    put_u16(&mut full, 34, 1);
    put_u16(&mut full, 36, 0x9ABC);
    put_u16(&mut full, 38, 0xDEF0);
    full
}

#[test]
fn palette_tim_emits_exact_clut_and_image_words_and_stops_at_non_tim_marker() {
    let (full, logical_size, mode) = palette_tim();
    let decoded = textures::decode_palettes(&full, logical_size).expect("decode palette TIM");

    assert!(decoded.shared_slots.is_empty());
    assert_eq!(decoded.uploads.len(), 2);
    let clut = &decoded.uploads[0];
    assert_eq!(clut.kind, "clut");
    assert_eq!(clut.source_span, [16, 32]);
    assert_eq!(clut.pixels_offset, 28);
    assert_eq!(clut.rect, [10, 20, 2, 1]);
    assert_eq!(clut.words, [0x1111, 0x2222]);
    assert_eq!(clut.tim_mode, Some(mode));

    let image = &decoded.uploads[1];
    assert_eq!(image.kind, "tim_image");
    assert_eq!(image.source_span, [32, 48]);
    assert_eq!(image.pixels_offset, 44);
    assert_eq!(image.rect, [30, 40, 2, 1]);
    assert_eq!(image.words, [0x3456, 0x789A]);
    assert_eq!(image.tim_mode, Some(mode));
}

#[test]
fn image_lists_use_nested_offsets_and_place_columns_sixty_four_words_apart() {
    let full = image_lists_fixture();
    let mut slots = vec![slot(0, 0, 0); 32];
    slots[0] = slot(10, 30, 0);
    slots[1] = slot(0, 0, 1);
    let decoded = textures::decode_images(&full, full.len(), &slots).expect("decode image lists");

    assert_eq!(decoded.shared_slots, [1]);
    assert_eq!(decoded.uploads.len(), 2);
    assert_eq!(decoded.uploads[0].slot, Some(0));
    assert_eq!(decoded.uploads[0].source_span, [24, 32]);
    assert_eq!(decoded.uploads[0].pixels_offset, 28);
    assert_eq!(decoded.uploads[0].rect, [10, 30, 2, 1]);
    assert_eq!(decoded.uploads[0].words, [0x1234, 0x5678]);
    assert_eq!(decoded.uploads[1].source_span, [32, 40]);
    assert_eq!(decoded.uploads[1].pixels_offset, 36);
    assert_eq!(decoded.uploads[1].rect, [74, 30, 2, 1]);
    assert_eq!(decoded.uploads[1].words, [0x9ABC, 0xDEF0]);
}

#[test]
fn final_image_pixels_use_measured_tail_and_missing_tail_bytes_are_rejected() {
    let full = image_lists_fixture();
    let logical_size = full.len() - 2;
    let slots = {
        let mut slots = vec![slot(0, 0, 0); 32];
        slots[0] = slot(10, 30, 0);
        slots[1] = slot(0, 0, 1);
        slots
    };

    let decoded = textures::decode_images(&full, logical_size, &slots)
        .expect("last image may read the exact two-byte measured tail");
    let last = decoded.uploads.last().expect("second image uploaded");
    assert_eq!(last.source_span, [32, 40]);
    assert_eq!(last.pixels_offset, 36);
    assert_eq!(last.words, [0x9ABC, 0xDEF0]);

    let truncated = &full[..full.len() - 1];
    assert!(textures::decode_images(truncated, logical_size, &slots).is_err());
}

#[test]
fn shared_slots_skip_an_invalid_nested_pointer_but_require_all_32_slot_records() {
    let full = image_lists_fixture();
    let mut slots = vec![slot(0, 0, 0); 32];
    slots[1].shared = -1;
    let decoded =
        textures::decode_images(&full, full.len(), &slots).expect("shared slot skips its pointer");
    assert_eq!(decoded.shared_slots, [1]);
    assert_eq!(decoded.uploads.len(), 2);

    assert!(textures::decode_images(&full, full.len(), &slots[..31]).is_err());
}

#[test]
fn palette_decoder_rejects_truncated_offset_tables_and_tim_blocks() {
    assert!(textures::decode_palettes(&[1, 0, 0, 0], 4).is_err());

    // The TIM header is present, but its following block declares 16 bytes and
    // only the four-byte block-size word is available.
    let mut truncated_block = vec![0; 20];
    put_u32(&mut truncated_block, 0, 1);
    put_u32(&mut truncated_block, 4, 8);
    put_u32(&mut truncated_block, 8, 0x10);
    put_u32(&mut truncated_block, 12, 0);
    put_u32(&mut truncated_block, 16, 16);
    assert!(textures::decode_palettes(&truncated_block, truncated_block.len()).is_err());
}

#[test]
fn palette_decoder_rejects_a_truncated_tim_marker_and_invalid_vram_rectangle() {
    let mut incomplete_marker = vec![0; 9];
    put_u32(&mut incomplete_marker, 0, 1);
    put_u32(&mut incomplete_marker, 4, 8);
    incomplete_marker[8] = 0x10;
    assert!(textures::decode_palettes(&incomplete_marker, incomplete_marker.len()).is_err());

    let logical_size = 32;
    let mut bad_rect = vec![0; logical_size];
    put_u32(&mut bad_rect, 0, 1);
    put_u32(&mut bad_rect, 4, 8);
    put_u32(&mut bad_rect, 8, 0x10);
    put_u32(&mut bad_rect, 12, 0);
    put_tim_block(&mut bad_rect, 16, [1023, 20, 2, 1], &[0x1111, 0x2222]);
    assert!(textures::decode_palettes(&bad_rect, logical_size).is_err());
}

#[test]
fn aliased_image_lists_hit_the_decoded_word_budget_before_more_upload_data_is_allocated() {
    let outer_count = 9usize;
    let nested_count = 15usize;
    let outer_bytes = 4 + outer_count * 4;
    let nested_bytes = 4 + nested_count * 4;
    let image_at = outer_bytes + nested_bytes;
    let image_words = 128usize * 512;
    let image_bytes = 4 + image_words * 2;
    let logical_size = image_at + image_bytes;
    let mut full = vec![0; logical_size];

    put_u32(&mut full, 0, outer_count as u32);
    for slot_index in 0..outer_count {
        put_u32(&mut full, 4 + slot_index * 4, outer_bytes as u32);
    }
    put_u32(&mut full, outer_bytes, nested_count as u32);
    for image_index in 0..nested_count {
        // Every list entry aliases one bounded 128x512 image block.
        put_u32(
            &mut full,
            outer_bytes + 4 + image_index * 4,
            nested_bytes as u32,
        );
    }
    put_u16(&mut full, image_at, 128);
    put_u16(&mut full, image_at + 2, 512);

    let slots = vec![slot(0, 0, 0); 32];
    let error = textures::decode_images(&full, logical_size, &slots)
        .expect_err("aliased uploads exceed the service word budget");
    assert_eq!(error.kind, "scene_resource_limit");
}
