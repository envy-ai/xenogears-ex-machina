use std::collections::BTreeMap;
use xem_studio::{
    field,
    scene::{self, bundle, formats::SpriteSlot, sprites},
};

#[path = "support/scene_bundle.rs"]
#[allow(dead_code)]
mod fixture;

fn put_u16(bytes: &mut [u8], at: usize, value: u16) {
    bytes[at..at + 2].copy_from_slice(&value.to_le_bytes());
}

fn put_u32(bytes: &mut [u8], at: usize, value: u32) {
    bytes[at..at + 4].copy_from_slice(&value.to_le_bytes());
}

fn slots() -> Vec<SpriteSlot> {
    let mut slots = vec![
        SpriteSlot {
            x: 0,
            y: 0,
            reserved: 0,
            shared: 0
        };
        32
    ];
    slots[0] = SpriteSlot {
        x: 130,
        y: 250,
        reserved: 0xA55A,
        shared: 0,
    };
    slots[1] = SpriteSlot {
        x: 511,
        y: 300,
        reserved: 0x5AA5,
        shared: -1,
    };
    slots
}

fn animation(program: &[u8], table: &[u16]) -> Vec<u8> {
    // One animation at +4. Its command stream begins at +14; the single facing
    // table begins at +10 and is relative to its own halfword at +8.
    let mut bytes = vec![0; 14 + program.len()];
    put_u16(&mut bytes, 0, 1);
    put_u16(&mut bytes, 2, 4);
    put_u16(&mut bytes, 4, 0); // One facing group.
    put_u16(&mut bytes, 6, 8);
    put_u16(&mut bytes, 8, 2);
    for (index, value) in table.iter().copied().enumerate() {
        let at = 10 + index * 2;
        if bytes.len() < at + 2 {
            bytes.resize(at + 2, 0);
        }
        put_u16(&mut bytes, at, value);
    }
    bytes[14..14 + program.len()].copy_from_slice(program);
    bytes
}

fn cell_directory(first_image: [u8; 3], extra_controls: &[u8], cell: [u8; 5]) -> Vec<u8> {
    const CELL_AT: usize = 64;
    let mut bytes = vec![0; CELL_AT + 5];
    put_u16(&mut bytes, 0, 0x8001); // One frame, cell directory.
    put_u16(&mut bytes, 2, 4);
    bytes[4] = 1; // One part.
    put_u16(&mut bytes, 8, CELL_AT as u16); // Cell record follows frame metadata.
    let controls_at = 10;
    bytes[controls_at..controls_at + extra_controls.len()].copy_from_slice(extra_controls);
    let fields_at = controls_at + extra_controls.len();
    bytes[fields_at..fields_at + 3].copy_from_slice(&first_image);
    bytes[CELL_AT..CELL_AT + 5].copy_from_slice(&cell);
    bytes
}

fn palette(count: u16, banks: usize) -> Vec<u8> {
    let stride = usize::from(count) * 32;
    let mut bytes = vec![0; 4 + stride * banks];
    put_u16(&mut bytes, 0, count);
    for bank in 0..banks {
        for index in 0..stride / 2 {
            put_u16(
                &mut bytes,
                4 + bank * stride + index * 2,
                (bank * 0x100 + index) as u16,
            );
        }
    }
    bytes
}

fn resource(animation: &[u8], directory: &[u8], palette: &[u8]) -> Vec<u8> {
    const BLOCK_AT: usize = 8;
    const SECTION_COUNT: usize = 3;
    let first = 8 + 4 * SECTION_COUNT;
    let a = first;
    let d = a + animation.len();
    let p = d + directory.len();
    let end = p + palette.len();
    let mut bytes = vec![0; BLOCK_AT + end];
    put_u32(&mut bytes, 0, 1);
    put_u32(&mut bytes, 4, BLOCK_AT as u32);
    put_u32(&mut bytes, BLOCK_AT, SECTION_COUNT as u32);
    put_u32(&mut bytes, BLOCK_AT + 4, a as u32);
    put_u32(&mut bytes, BLOCK_AT + 8, d as u32);
    put_u32(&mut bytes, BLOCK_AT + 12, p as u32);
    put_u32(&mut bytes, BLOCK_AT + 16, end as u32);
    bytes[BLOCK_AT + a..BLOCK_AT + d].copy_from_slice(animation);
    bytes[BLOCK_AT + d..BLOCK_AT + p].copy_from_slice(directory);
    bytes[BLOCK_AT + p..BLOCK_AT + end].copy_from_slice(palette);
    bytes
}

fn primary_opcode(handler: &str) -> u8 {
    let metadata: serde_json::Value =
        serde_json::from_str(include_str!("../data/field-opcodes.json")).unwrap();
    metadata["primary"]
        .as_object()
        .unwrap()
        .iter()
        .find_map(|(opcode, forms)| {
            forms
                .as_array()?
                .iter()
                .any(|form| form["handler"] == handler)
                .then(|| opcode.parse::<u8>().unwrap())
        })
        .unwrap_or_else(|| panic!("canonical primary metadata has {handler}"))
}

fn event_component(handler: &str, immediates: &[Option<u16>]) -> Vec<u8> {
    const HEADER: usize = 132;
    const CODE: usize = 196;
    let metadata: serde_json::Value =
        serde_json::from_str(include_str!("../data/field-opcodes.json")).unwrap();
    let (table, opcode, spec) = ["primary", "extended"]
        .into_iter()
        .find_map(|table| {
            metadata[table]
                .as_object()
                .unwrap()
                .iter()
                .find_map(|(opcode, forms)| {
                    forms
                        .as_array()?
                        .iter()
                        .find(|form| form["handler"] == handler)
                        .map(|form| (table, opcode.parse::<u8>().unwrap(), form.clone()))
                })
        })
        .unwrap_or_else(|| panic!("canonical field metadata has {handler}"));
    let size = spec["size"].as_u64().unwrap() as usize;
    let prefix_size = if table == "extended" { 1 } else { 0 };
    let mut instruction = vec![0; size + prefix_size];
    if prefix_size != 0 {
        instruction[0] = metadata["prefix"].as_u64().unwrap() as u8;
    }
    instruction[prefix_size] = opcode;
    let mut value_index = 0;
    let widths = metadata["widths"].as_object().unwrap();
    for operand in spec["operands"].as_array().unwrap() {
        if operand["kind"] != "iv" {
            continue;
        }
        let offset = operand["offset"].as_u64().unwrap() as usize;
        let width = widths[operand["kind"].as_str().unwrap()].as_u64().unwrap() as usize;
        let immediate = immediates.get(value_index).copied().flatten();
        let value = immediate.map(|value| value | 0x8000).unwrap_or(0);
        let offset = offset + prefix_size;
        instruction[offset..offset + width].copy_from_slice(&value.to_le_bytes()[..width]);
        value_index += 1;
    }
    assert_eq!(
        value_index,
        immediates.len(),
        "test supplies one value per iv operand"
    );
    let end = primary_opcode("field_event_end_slot");
    let mut bytes = vec![0; CODE + instruction.len() + 1];
    put_u32(&mut bytes, 128, 1);
    put_u16(&mut bytes, HEADER, 0);
    bytes[CODE..CODE + instruction.len()].copy_from_slice(&instruction);
    bytes[CODE + instruction.len()] = end;
    bytes
}

fn event_component_after(constructor: &str, constructor_immediates: &[Option<u16>]) -> Vec<u8> {
    const CODE: usize = 196;
    let before = event_component(constructor, constructor_immediates);
    let after = event_component("field_event_set_sprite", &[Some(1)]);
    let mut bytes = before[..CODE].to_vec();
    bytes.extend_from_slice(&before[CODE..before.len() - 1]);
    bytes.extend_from_slice(&after[CODE..]);
    bytes
}

fn input(bundle: Vec<u8>) -> bundle::Input {
    bundle::Input {
        bundle,
        context: None,
        source: None,
        module_sha256: None,
    }
}

#[test]
fn cell_frame_preserves_signed_origin_controls_flips_palette_and_page_facts() {
    let animation = animation(&[0xBE, 0x01, 0x06], &[]); // Frame 1, flip X and Y.
    let directory = cell_directory(
        [0x61, 0xF6, 0x0C],
        &[0xE4, 0xF9, 0x05, 0x84],
        [1, 8, 9, 12, 16],
    );
    let bytes = resource(&animation, &directory, &palette(2, 2));
    let decoded = sprites::decode(&bytes, bytes.len(), &slots()).expect("decode source sprite");
    assert_eq!(decoded.resources.len(), 1);
    let resource = &decoded.resources[0];
    assert_eq!(resource.slot, 0);
    assert_eq!(resource.palette_count, 2);
    assert_eq!(resource.palette_banks.len(), 2);
    assert_eq!(resource.palette_banks[1].bank, 1);
    assert_eq!(resource.palette_banks[1].words[0], 0x100);

    let frame = resource
        .first_frame
        .as_ref()
        .expect("qualified first frame");
    assert_eq!((frame.frame, frame.animation, frame.facing), (1, 0, 0));
    assert!(frame.flip_x && frame.flip_y);
    assert!(
        frame.uploads.is_empty(),
        "cell-directory frames reuse source VRAM uploads"
    );
    assert_eq!(frame.parts.len(), 1);
    let part = &frame.parts[0];
    assert_eq!(part.position, [-17, 17]);
    assert_eq!(part.size, [12, 16]);
    assert_eq!(part.uv, [9, 3]);
    assert_eq!(part.uv_size, [12, 16]);
    assert_eq!(part.tpage, 162);
    assert_eq!(part.clut, (480 << 6) | 17);
    assert_eq!(part.palette, 1);
    assert!(part.flip_x && part.flip_y);
    assert_eq!(part.blend, 2);
    assert_eq!(part.cell_span[0], 8 + 20 + animation.len() + 64);
}

#[test]
fn cell_page_uses_masked_native_tpage_bits_while_embedded_uploads_stay_bounded() {
    let animation = animation(&[0], &[]);
    let directory = cell_directory([0x30, 0, 0], &[], [2, 0, 0, 8, 8]);
    let bytes = resource(&animation, &directory, &palette(0, 0));
    let mut origins = slots();
    origins[0].x = 960;
    origins[0].y = 0x200;
    let frame = sprites::decode(&bytes, bytes.len(), &origins)
        .unwrap()
        .resources
        .remove(0)
        .first_frame
        .unwrap();
    assert_eq!(frame.parts[0].tpage, 0x840);
    assert_eq!(frame.parts[0].blend, 3);

    let mut embedded = vec![0; 36];
    put_u16(&mut embedded, 0, 1);
    put_u16(&mut embedded, 2, 4);
    embedded[4] = 1;
    put_u16(&mut embedded, 10, 6);
    embedded[14..17].copy_from_slice(&[0, 0, 0]);
    embedded[24..28].copy_from_slice(&[8, 2, 8, 0]);
    let bytes = resource(&animation, &embedded, &palette(0, 0));
    origins[0].x = 1023;
    origins[0].y = 250;
    let error = sprites::decode(&bytes, bytes.len(), &origins).unwrap_err();
    assert_eq!(error.kind, "invalid_scene_sprite");
    assert!(error.message.contains("sprite cell upload exceeds VRAM"));
}

#[test]
fn first_image_commands_cover_index_stack_call_and_explicit_frame_flips() {
    let directory = cell_directory([0, 0, 0], &[], [0, 0, 0, 8, 8]);
    let palette_bytes = palette(1, 1);
    type FirstImageCase<'a> = (&'a [u8], &'a [u16], (u16, bool, bool));
    let cases: &[FirstImageCase<'_>] = &[
        (&[0x00], &[], (1, false, false)),
        (&[0xB3, 0, 0x10], &[0, 1], (1, false, false)),
        (&[0xB4, 2, 0x00], &[], (1, false, false)),
        (&[0xE1, 3, 0, 0x00], &[], (1, false, false)),
        (&[0xE2, 5, 0, 0, 0, 0x85, 0], &[], (1, false, false)),
        (&[0xBE, 1, 6], &[], (1, true, true)),
    ];
    for (program, table, expected) in cases {
        let animation = animation(program, table);
        let bytes = resource(&animation, &directory, &palette_bytes);
        let decoded = sprites::decode(&bytes, bytes.len(), &slots()).expect("supported command");
        let frame = decoded.resources[0]
            .first_frame
            .as_ref()
            .expect("first frame");
        assert_eq!(
            (frame.frame, frame.flip_x, frame.flip_y),
            *expected,
            "program {program:02x?}"
        );
    }

    let directory = cell_directory([0, 0, 0], &[], [0, 0, 0, 8, 8]);
    // A frame-zero BE carries both whole-frame flips. The indexed frame replaces
    // only X, while the next-frame command preserves both incoming flips.
    for (program, table, expected) in [
        (
            &[0xBE, 0, 6, 0xB3, 0, 0x10][..],
            &[0, 0x0201][..],
            (true, true),
        ),
        (&[0xBE, 0, 6, 0x00][..], &[][..], (true, true)),
    ] {
        let animation = animation(program, table);
        let bytes = resource(&animation, &directory, &palette(1, 1));
        let decoded = sprites::decode(&bytes, bytes.len(), &slots()).expect("frame flip carry");
        let frame = decoded.resources[0]
            .first_frame
            .as_ref()
            .expect("selected image");
        assert_eq!((frame.flip_x, frame.flip_y), expected);
    }
}

#[test]
fn embedded_cells_emit_exact_upload_spans_and_words() {
    let animation = animation(&[0], &[]);
    let mut directory = vec![0; 36];
    put_u16(&mut directory, 0, 1);
    put_u16(&mut directory, 2, 4);
    directory[4] = 1;
    put_u16(&mut directory, 10, 6); // Cell at +24, scaled by four.
    put_u16(&mut directory, 12, (4 << 5) | 3);
    directory[8] = 1; // ClearImage width/height live at frame header +4.
    directory[9] = 2;
    directory[14..17].copy_from_slice(&[0, 0, 0]);
    directory[24..28].copy_from_slice(&[8, 2, 8, 0]);
    for (index, word) in [0x1234, 0x5678, 0x9ABC, 0xDEF0].into_iter().enumerate() {
        put_u16(&mut directory, 28 + index * 2, word);
    }
    let bytes = resource(&animation, &directory, &palette(1, 1));
    let decoded =
        sprites::decode(&bytes, bytes.len(), &slots()).expect("decode embedded sprite cell");
    let frame = decoded.resources[0]
        .first_frame
        .as_ref()
        .expect("first frame");
    assert_eq!(frame.uploads.len(), 1);
    assert_eq!(frame.clear.as_ref().unwrap().rect, [130, 250, 1, 2]);
    assert_eq!(
        frame.clear.as_ref().unwrap().source_span[1] - frame.clear.as_ref().unwrap().source_span[0],
        2
    );
    let upload = &frame.uploads[0];
    assert_eq!(upload.kind, "sprite_cell");
    assert_eq!(upload.slot, Some(0));
    assert_eq!(upload.rect, [133, 254, 2, 2]);
    assert_eq!(upload.words, [0x1234, 0x5678, 0x9ABC, 0xDEF0]);
    assert_eq!(upload.source_span[1] - upload.source_span[0], 12);
    assert_eq!(frame.parts[0].uv_size, [8, 2]);
}

#[test]
fn final_group_state_and_lifo_upload_order_match_the_source_build_pipeline() {
    let animation = animation(&[0], &[]);
    let mut cells = vec![0; 74];
    put_u16(&mut cells, 0, 0x8001);
    put_u16(&mut cells, 2, 4);
    cells[4] = 2;
    put_u16(&mut cells, 8, 64);
    put_u16(&mut cells, 10, 69);
    cells[12..18].copy_from_slice(&[0xE4, 1, 2, 0, 10, 20]);
    cells[18..24].copy_from_slice(&[0xE4, 5, 6, 0, 30, 40]);
    cells[64..69].copy_from_slice(&[0, 0, 0, 8, 8]);
    cells[69..74].copy_from_slice(&[0, 0, 0, 8, 8]);
    let bytes = resource(&animation, &cells, &palette(1, 1));
    let frame = sprites::decode(&bytes, bytes.len(), &slots())
        .unwrap()
        .resources
        .remove(0)
        .first_frame
        .unwrap();
    assert_eq!(
        frame
            .parts
            .iter()
            .map(|part| part.group)
            .collect::<Vec<_>>(),
        [4, 4]
    );
    assert_eq!(
        frame
            .parts
            .iter()
            .map(|part| part.position)
            .collect::<Vec<_>>(),
        [[15, 26], [35, 46]]
    );

    // Normal frames upload two independent cells. Queue-prepending means the
    // exposed Upload list is execution order (second cell, then first); clear
    // is an explicit operation represented separately, before both uploads.
    let mut normal = vec![0; 48];
    put_u16(&mut normal, 0, 1);
    put_u16(&mut normal, 2, 4);
    normal[4] = 2;
    normal[8] = 2;
    normal[9] = 1;
    put_u16(&mut normal, 10, 8);
    put_u16(&mut normal, 14, 10);
    normal[18..24].copy_from_slice(&[0, 0, 0, 0, 0, 0]);
    normal[24..30].copy_from_slice(&[0, 0, 0, 0, 0, 0]);
    normal[32..40].copy_from_slice(&[4, 1, 1, 0, 0x34, 0x12, 0x78, 0x56]);
    normal[40..48].copy_from_slice(&[4, 1, 1, 0, 0xBC, 0x9A, 0xF0, 0xDE]);
    let bytes = resource(&animation, &normal, &palette(1, 1));
    let frame = sprites::decode(&bytes, bytes.len(), &slots())
        .unwrap()
        .resources
        .remove(0)
        .first_frame
        .unwrap();
    assert_eq!(frame.clear.as_ref().unwrap().rect, [130, 250, 2, 1]);
    assert_eq!(frame.uploads.len(), 2);
    assert_eq!(frame.uploads[0].words, [0x9ABC, 0xDEF0]);
    assert_eq!(frame.uploads[1].words, [0x1234, 0x5678]);
    assert!(frame.uploads[0].source_span[0] > frame.uploads[1].source_span[0]);
}

#[test]
fn resource_and_component_tables_reject_out_of_bounds_without_partial_frames() {
    assert!(
        sprites::decode(&[], 0, &slots())
            .unwrap()
            .resources
            .is_empty()
    );
    assert_eq!(
        sprites::decode(&[0; 4], 4, &slots())
            .unwrap()
            .resources
            .len(),
        0
    );

    let mut too_many = vec![0; 4];
    put_u32(&mut too_many, 0, 33);
    let error = sprites::decode(&too_many, too_many.len(), &slots()).unwrap_err();
    assert_eq!(error.kind, "invalid_scene_sprite");

    let error = sprites::decode(&[0; 4], 5, &slots()).unwrap_err();
    assert_eq!(error.kind, "invalid_scene_sprite");
    let mut oversized_tail = vec![0; 21];
    put_u32(&mut oversized_tail, 0, 0);
    let error = sprites::decode(&oversized_tail, 4, &slots()).unwrap_err();
    assert_eq!(error.kind, "invalid_scene_sprite");

    let animation_bytes = animation(&[0xBE, 1, 0], &[]);
    let directory = cell_directory([0, 0, 0], &[], [0, 0, 0, 8, 8]);
    let mut bytes = resource(&animation_bytes, &directory, &palette(1, 1));
    put_u32(&mut bytes, 4, u32::MAX); // Outer resource offset outside the component.
    let error = sprites::decode(&bytes, bytes.len(), &slots()).unwrap_err();
    assert_eq!(error.kind, "invalid_scene_sprite");

    let mut bad_palette = palette(49, 0);
    let bytes = resource(&animation_bytes, &directory, &bad_palette);
    let error = sprites::decode(&bytes, bytes.len(), &slots()).unwrap_err();
    assert_eq!(error.kind, "invalid_scene_sprite");
    bad_palette = palette(1, 0); // Header without the required first bank.
    let bytes = resource(&animation_bytes, &directory, &bad_palette);
    let error = sprites::decode(&bytes, bytes.len(), &slots()).unwrap_err();
    assert_eq!(error.kind, "invalid_scene_sprite");

    let mut no_facing = animation(&[0], &[]);
    put_u16(&mut no_facing, 4, 3);
    let bytes = resource(&no_facing, &directory, &palette(1, 1));
    let decoded =
        sprites::decode(&bytes, bytes.len(), &slots()).expect("unsupported facing is diagnostic");
    assert!(decoded.resources[0].first_frame.is_none());
    assert!(
        decoded.resources[0]
            .diagnostic
            .as_deref()
            .unwrap()
            .contains("facing mode3")
    );
}

#[test]
fn unsupported_initial_state_is_diagnostic_while_malformed_spans_are_errors() {
    let directory = cell_directory([0, 0, 0], &[], [0, 0, 0, 8, 8]);
    for program in [&[0x90][..], &[0x20][..]] {
        let animation = animation(program, &[]);
        let bytes = resource(&animation, &directory, &palette(1, 1));
        let decoded =
            sprites::decode(&bytes, bytes.len(), &slots()).expect("unsupported state is retained");
        assert!(decoded.resources[0].first_frame.is_none());
        assert!(decoded.resources[0].diagnostic.is_some());
    }

    let animation = animation(&[0xBE, 1, 0], &[]);
    let mut bad_directory = directory.clone();
    put_u16(&mut bad_directory, 8, u16::MAX);
    let bytes = resource(&animation, &bad_directory, &palette(1, 1));
    let error = sprites::decode(&bytes, bytes.len(), &slots()).unwrap_err();
    assert_eq!(error.kind, "invalid_scene_sprite");

    let mut bad_resource = resource(&animation, &directory, &palette(1, 1));
    put_u32(&mut bad_resource, 8, 17); // Unsupported section count.
    let error = sprites::decode(&bad_resource, bad_resource.len(), &slots()).unwrap_err();
    assert_eq!(error.kind, "invalid_scene_sprite");

    let inherited_directory = cell_directory([0, 0, 0], &[], [0x10, 0, 0, 8, 8]);
    let bytes = resource(&animation, &inherited_directory, &palette(1, 1));
    let decoded = sprites::decode(&bytes, bytes.len(), &slots())
        .expect("unknown inherited state is diagnostic");
    assert!(decoded.resources[0].first_frame.is_none());
    assert!(
        decoded.resources[0]
            .diagnostic
            .as_deref()
            .unwrap()
            .contains("resident sprite pages")
    );

    let rotated_directory = cell_directory([0, 0, 0], &[0xD4, 1], [0, 0, 0, 8, 8]);
    let bytes = resource(&animation, &rotated_directory, &palette(1, 1));
    let decoded =
        sprites::decode(&bytes, bytes.len(), &slots()).expect("unqualified rotation is diagnostic");
    assert!(
        decoded.resources[0]
            .diagnostic
            .as_deref()
            .unwrap()
            .contains("rotated part group")
    );
}

#[test]
fn binding_follows_only_unconditional_immediate_local_construction() {
    let code = event_component("field_event_set_sprite", &[Some(1)]);
    let graph = field::decode(&code, Some(code.len())).expect("decode event graph");
    let nodes = BTreeMap::from([("scene-node:source".to_owned(), 0)]);
    let bindings = sprites::bindings(&graph, &["scene:7:actor".into()], &nodes);
    assert_eq!(bindings.len(), 1);
    assert_eq!(bindings[0].actor, "scene:7:actor");
    assert_eq!(bindings[0].node.as_deref(), Some("scene-node:source"));
    assert_eq!(bindings[0].pc, Some(0));
    assert_eq!(bindings[0].resource, Some(1));
    assert_eq!(bindings[0].bank, Some(0));
    assert!(bindings[0].diagnostic.is_none());

    let variable = event_component("field_event_set_sprite", &[None]);
    let graph = field::decode(&variable, Some(variable.len())).expect("decode variable source");
    let binding = sprites::bindings(&graph, &["actor:0".into()], &BTreeMap::new());
    assert!(binding[0].resource.is_none());
    assert!(
        binding[0]
            .diagnostic
            .as_deref()
            .unwrap()
            .contains("runtime variable")
    );

    let banked = event_component("field_event_set_sprite_parameter", &[Some(31), Some(15)]);
    let graph =
        field::decode(&banked, Some(banked.len())).expect("decode banked local construction");
    let binding = sprites::bindings(&graph, &["stable:actor".into()], &BTreeMap::new());
    assert_eq!(binding[0].resource, Some(31));
    assert_eq!(binding[0].bank, Some(15));

    let bad_resource = event_component("field_event_set_sprite", &[Some(32)]);
    let graph = field::decode(&bad_resource, Some(bad_resource.len()))
        .expect("decode out-of-range resource");
    let binding = sprites::bindings(&graph, &["actor:0".into()], &BTreeMap::new());
    assert!(binding[0].resource.is_none());
    assert!(
        binding[0]
            .diagnostic
            .as_deref()
            .unwrap()
            .contains("exceeds")
    );

    let bad_bank = event_component("field_event_set_sprite_parameter", &[Some(0), Some(16)]);
    let graph = field::decode(&bad_bank, Some(bad_bank.len())).expect("decode out-of-range bank");
    let binding = sprites::bindings(&graph, &["actor:0".into()], &BTreeMap::new());
    assert!(binding[0].resource.is_none());
    assert!(
        binding[0]
            .diagnostic
            .as_deref()
            .unwrap()
            .contains("exceeds")
    );
}

#[test]
fn earlier_unsupported_sprite_constructors_do_not_fall_through_to_a_later_set_sprite() {
    for (constructor, immediates) in [
        ("field_event_show_first_sprite", &[][..]),
        ("field_event_set_layer_sprite", &[Some(1)][..]),
        ("field_event_set_party_sprite", &[Some(1)][..]),
    ] {
        let code = event_component_after(constructor, immediates);
        let graph =
            field::decode(&code, Some(code.len())).expect("decode canonical constructor sequence");
        assert_eq!(graph.instructions[0].spec.handler, constructor);
        assert_eq!(graph.instructions[1].spec.handler, "field_event_set_sprite");

        let bindings = sprites::bindings(&graph, &["scene:7:actor".into()], &BTreeMap::new());
        assert_eq!(bindings.len(), 1);
        assert_eq!(
            bindings[0].resource, None,
            "unsupported earlier constructor must not be skipped"
        );
        assert_eq!(bindings[0].bank, None);
        assert_eq!(
            bindings[0].pc, None,
            "later set_sprite is not the first source construction"
        );
        assert!(bindings[0].diagnostic.is_some());
    }
}

#[test]
fn scene_decode_exposes_component3_frames_and_snapshot_bindings_without_editing_source() {
    let animation = animation(&[0xBE, 1, 0], &[]);
    let directory = cell_directory([0, 0, 0], &[], [0, 0, 0, 8, 8]);
    let sprite_bytes = resource(&animation, &directory, &palette(1, 1));
    let mut components = fixture::components();
    components[3] = sprite_bytes;
    components[5] = event_component("field_event_set_sprite", &[Some(0)]);
    let original = fixture::bundle(&components);
    let input = input(original.clone());
    let decoded = scene::decode(scene::DecodeArgs {
        input: input.clone(),
        entrances: Vec::new(),
    })
    .expect("decode source scene and component3");
    assert_eq!(decoded.sprites.resources.len(), 1);
    assert!(decoded.sprites.resources[0].first_frame.is_some());

    let draft = scene::domain::Draft::open(input, Vec::new()).expect("open source draft");
    let snapshot = draft.snapshot().expect("snapshot source actor bindings");
    assert_eq!(snapshot.sprite_bindings.len(), 1);
    assert_eq!(snapshot.sprite_bindings[0].actor, snapshot.actors[0].id);
    assert_eq!(snapshot.sprite_bindings[0].resource, Some(0));
    assert_eq!(
        draft.compile().expect("compile no-edit scene").bundle,
        original
    );
}
