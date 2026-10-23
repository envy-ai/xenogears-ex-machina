use xem_studio::{field, scene::entrances};

const BYTECODE_OFFSET: usize = 132 + 64;
const CODE_PREFIX_SIZE: usize = 1 + 2 * 7;

fn put_u16(bytes: &mut [u8], at: usize, value: u16) {
    bytes[at..at + 2].copy_from_slice(&value.to_le_bytes());
}

fn record(x: i16, z: i16, layer: u8, camera: u8, facing: u8) -> [u8; 7] {
    let mut bytes = [0; 7];
    bytes[..2].copy_from_slice(&x.to_le_bytes());
    bytes[2..4].copy_from_slice(&z.to_le_bytes());
    bytes[4] = layer;
    bytes[5] = camera;
    bytes[6] = facing;
    bytes
}

fn fixture() -> Vec<u8> {
    let code = [
        0xFF, // Map-entry prefix; the record count is not encoded here.
        0x34, 0x12, 0xCC, 0xED, 1, 0xFF, 0xFF, // Selector 0.
        0xFE, 0xFF, 0x20, 0, 0, 7, 9,    // Selector 1.
        0x00, // One reachable terminating instruction after both records.
    ];
    assert_eq!(code.len(), CODE_PREFIX_SIZE + 1);
    let mut bytes = vec![0; BYTECODE_OFFSET + code.len()];
    bytes[128..132].copy_from_slice(&1_u32.to_le_bytes());
    // Only event 0 reaches executable code. The other zero entries are skipped
    // by the decoder for an ff-prefixed field, as in the original event table.
    put_u16(&mut bytes, 132, CODE_PREFIX_SIZE as u16);
    bytes[BYTECODE_OFFSET..].copy_from_slice(&code);
    bytes
}

fn decode_graph(bytes: &[u8]) -> field::FieldGraph {
    field::decode(bytes, Some(bytes.len())).expect("synthetic ff-prefix field decodes")
}

#[test]
fn explicitly_selected_records_decode_signed_coordinates_and_raw_dynamic_angles() {
    let bytes = fixture();
    let graph = decode_graph(&bytes);

    let first = entrances::decode_entrances(&graph, &[0]).expect("decode selected selector 0");
    assert_eq!(first.len(), 1, "the decoder must not infer a record count");
    assert_eq!(first[0].selector, 0);
    assert_eq!(first[0].source_span, [1, 8]);
    assert_eq!(first[0].x, 0x1234);
    assert_eq!(first[0].z, -0x1234);
    assert_eq!(first[0].layer, 1);
    assert_eq!(first[0].camera_raw, 0xFF);
    assert_eq!(first[0].facing_raw, 0xFF);

    let both = entrances::decode_entrances(&graph, &[0, 1]).expect("decode two selected records");
    assert_eq!(both.len(), 2);
    assert_eq!(both[1].source_span, [8, 15]);
    assert_eq!(both[1].x, -2);
    assert_eq!(both[1].z, 0x20);
    assert_eq!(both[1].layer, 0);
    assert_eq!(both[1].camera_raw, 7);
    assert_eq!(both[1].facing_raw, 9);

    let decoded = entrances::encode_entrances(&graph, &both, 2).expect("no-edit encode");
    assert_eq!(decoded, bytes, "no-edit entrance encode is byte exact");
}

#[test]
fn same_selector_patch_changes_only_its_seven_byte_span_and_preserves_code_graph() {
    let source = fixture();
    let graph = decode_graph(&source);
    let original_graph = graph.clone();
    let mut selected = entrances::decode_entrances(&graph, &[0]).expect("decode selector 0");
    selected[0].x = -300;
    selected[0].z = 700;
    selected[0].layer = 0;
    // 0xff remains a raw runtime-dynamic angle in both fields.
    assert_eq!(selected[0].camera_raw, 0xFF);
    assert_eq!(selected[0].facing_raw, 0xFF);

    let encoded = entrances::encode_entrances(&graph, &selected, 2).expect("patch selector 0");
    let selected_at = BYTECODE_OFFSET + 1;
    assert_eq!(&encoded[..selected_at], &source[..selected_at]);
    assert_eq!(&encoded[selected_at + 7..], &source[selected_at + 7..]);
    assert_eq!(
        &encoded[selected_at..selected_at + 7],
        &record(-300, 700, 0, 0xFF, 0xFF)
    );

    let after = decode_graph(&encoded);
    assert_eq!(after.instructions, original_graph.instructions);
    assert_eq!(after.actors, original_graph.actors);
    assert_eq!(after.diagnostics, original_graph.diagnostics);
}

#[test]
fn duplicate_out_of_prefix_overlapping_and_unbounded_selectors_are_rejected() {
    let bytes = fixture();
    let graph = decode_graph(&bytes);

    assert!(entrances::decode_entrances(&graph, &[0, 0]).is_err());
    // Selector 2 starts at the executable terminator and crosses the pinned
    // prefix boundary; there is no inferred third record.
    assert!(entrances::decode_entrances(&graph, &[2]).is_err());
    assert!(entrances::decode_entrances(&graph, &[u16::MAX]).is_err());

    let mut selected =
        entrances::decode_entrances(&graph, &[0, 1]).expect("decode selected records");
    // A caller cannot make selectors alias or overlap by supplying another
    // source span; the encoded span must match the graph's exact record.
    selected[1].source_span = [7, 14];
    assert!(entrances::encode_entrances(&graph, &selected, 2).is_err());
}

#[test]
fn invalid_layer_counts_and_layer_values_are_rejected_without_changing_source() {
    let bytes = fixture();
    let graph = decode_graph(&bytes);
    let selected = entrances::decode_entrances(&graph, &[0]).expect("decode selector");

    assert!(entrances::encode_entrances(&graph, &selected, 0).is_err());
    assert!(entrances::encode_entrances(&graph, &selected, 5).is_err());
    let mut invalid_layer = selected;
    invalid_layer[0].layer = 2;
    assert!(entrances::encode_entrances(&graph, &invalid_layer, 2).is_err());
    assert_eq!(
        field::encode(&graph).expect("source graph remains encodable"),
        bytes
    );
}
