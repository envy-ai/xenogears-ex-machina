use xem_disc::field_context::{
    ComponentContext, MAX_EXTRA, RAM_END, RAM_START, SourceContext, SourceIdentity, hash,
};
use xem_disc::{KnownDisc, packed};

const HEADER: usize = 0x154;
const MODULE: &str = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";

struct Fixture {
    bundle: Vec<u8>,
    identity: SourceIdentity,
    context: SourceContext,
    complete_event_stream: Vec<u8>,
}

fn literal_block(bytes: &[u8]) -> Vec<u8> {
    assert_eq!(bytes.len(), 8);
    let mut block = (bytes.len() as u32).to_le_bytes().to_vec();
    block.push(0);
    block.extend_from_slice(bytes);
    block.push(0); // Original decoder's measured final speculative flag.
    block
}

/// A synthetic component with 72 logical bytes and 78 decoded bytes. Its
/// indexed prefix contains 74 bytes; the observed tail contains the final four
/// output literals and the decoder's speculative terminal flag.
fn measured_event_stream() -> Vec<u8> {
    let mut stream = 78_u32.to_le_bytes().to_vec();
    stream.push(0);
    stream.extend_from_slice(b"ABCDEFGH");
    stream.push(1);
    stream.extend_from_slice(&[1, 0xc0]); // Copy 15 bytes at distance one.
    stream.extend_from_slice(b"ijklmno");
    for group in 0..5 {
        stream.push(0);
        stream.extend_from_slice(&[b'0' + group; 8]);
    }
    stream.push(0);
    stream.extend_from_slice(b"QRSTUVWX");
    stream.push(0); // Final flag read after the output reaches 78 bytes.
    assert_eq!(stream.len(), 78);
    let decoded = packed::decode_block(&stream, 88).expect("synthetic measured stream decodes");
    assert_eq!(decoded.data.len(), 78);
    assert_eq!(decoded.source_bytes_read, 78);
    let prefix = packed::decode_block(&stream[..73], 88).expect_err("indexed prefix is incomplete");
    match prefix {
        packed::PackedError::Truncated { output, .. } => assert_eq!(output.len(), 74),
        other => panic!("expected four-byte-short prefix after 74 output bytes, got {other:?}"),
    }
    stream
}

fn fixture() -> Fixture {
    let event_stream = measured_event_stream();
    let mut bundle = vec![0_u8; HEADER];
    let mut contexts = Vec::new();
    let mut cursor = HEADER;

    for index in 0..8 {
        let logical = [index as u8; 8];
        let block = literal_block(&logical);
        bundle[0x10c + index * 4..0x10c + index * 4 + 4]
            .copy_from_slice(&(logical.len() as u32).to_le_bytes());
        bundle[0x130 + index * 4..0x130 + index * 4 + 4]
            .copy_from_slice(&(cursor as u32).to_le_bytes());
        let decoded = packed::decode_block(&block, 24).unwrap();
        contexts.push(ComponentContext {
            index,
            source_offset: cursor,
            logical_size: logical.len(),
            packed_size: u32::from_le_bytes(block[..4].try_into().unwrap()) as usize,
            source_bytes_read: decoded.source_bytes_read,
            output_size: decoded.data.len(),
            output_sha256: hash(&decoded.data),
            destination_address: RAM_START + 0x10_000 + (index as u32) * 0x1000,
            extra: Vec::new(),
        });
        bundle.extend_from_slice(&block);
        cursor += block.len();
    }

    let event_offset = cursor;
    let indexed_prefix = event_stream.len() - 5;
    bundle[0x10c + 8 * 4..0x10c + 9 * 4].copy_from_slice(&72_u32.to_le_bytes());
    bundle[0x130 + 8 * 4..0x130 + 9 * 4].copy_from_slice(&(event_offset as u32).to_le_bytes());
    bundle.extend_from_slice(&event_stream[..indexed_prefix]);
    let event_decoded = packed::decode_block(&event_stream, 88).unwrap();
    contexts.push(ComponentContext {
        index: 8,
        source_offset: event_offset,
        logical_size: 72,
        packed_size: 78,
        source_bytes_read: event_decoded.source_bytes_read,
        output_size: event_decoded.data.len(),
        output_sha256: hash(&event_decoded.data),
        destination_address: RAM_START + 0x18_000,
        extra: event_stream[indexed_prefix..].to_vec(),
    });
    contexts.sort_by_key(|component| component.index);

    let identity = SourceIdentity {
        disc: 1,
        boot_sha256: xem_disc::identify::hex(&KnownDisc::Disc1.executable_sha256()),
        map: 23,
        slot: 652,
        lba: 0x1234,
        indexed_size: bundle.len() as u32,
        source_sha256: hash(&bundle),
    };
    let allocation_size = bundle.len().div_ceil(4) * 4;
    let context = SourceContext {
        schema_version: 1,
        provenance: "runtime_field_decompress".into(),
        module_sha256: MODULE.into(),
        identity: identity.clone(),
        source_address: RAM_START + 0x1000,
        allocation_size: allocation_size as u32,
        load_vblank: "733".into(),
        components: contexts,
    };
    Fixture {
        bundle,
        identity,
        context,
        complete_event_stream: event_stream,
    }
}

fn rejected(result: Result<Vec<u8>, String>, expected: &str) {
    let error = result.expect_err("malformed source context is rejected");
    assert!(
        error.contains(expected),
        "expected {expected:?}, got {error:?}"
    );
}

#[test]
fn measured_context_materializes_exact_eof_bytes_and_checks_all_original_outputs() {
    let fixture = fixture();
    let context = &fixture.context;
    let event = context
        .components
        .iter()
        .find(|component| component.index == 8)
        .unwrap();
    assert_eq!(event.logical_size, 72);
    assert_eq!(event.packed_size, 78);
    assert_eq!(event.output_size, 78);
    assert_eq!(event.source_bytes_read, 78);
    assert_eq!(event.extra.len(), 5);
    assert_eq!(fixture.bundle.len(), event.source_offset + 73);

    let guard = context
        .guard(&fixture.bundle, &fixture.identity, MODULE)
        .expect("all nine original decoder outputs have a consistent measured guard");
    assert_eq!(guard.as_slice(), &fixture.complete_event_stream[73..]);

    // The edited stream may differ, while the other eight original complete
    // outputs remain bound to the measured runtime results.
    let mut result = fixture.bundle.clone();
    result.extend_from_slice(&guard);
    let event_offset = context.components[8].source_offset;
    result[event_offset + 5] ^= 0x7f;
    assert!(context.verify_outputs(&result, Some(8)).is_ok());
    assert!(context.verify_outputs(&result, None).is_err());

    let mut wrong_untouched = fixture.bundle.clone();
    wrong_untouched.extend_from_slice(&guard);
    let untouched_component = context
        .components
        .iter()
        .find(|component| component.index == 5)
        .unwrap();
    wrong_untouched[untouched_component.source_offset + 5] ^= 0x7f;
    assert!(context.verify_outputs(&wrong_untouched, Some(8)).is_err());
}

#[test]
fn context_rejects_wrong_source_identity_module_provenance_and_noncanonical_time() {
    let fixture = fixture();
    let mut context = fixture.context.clone();
    context.identity.map = 24;
    rejected(
        context.guard(&fixture.bundle, &fixture.identity, MODULE),
        "identity/module/provenance",
    );

    let mut context = fixture.context.clone();
    context.module_sha256 = "b".repeat(64);
    rejected(
        context.guard(&fixture.bundle, &fixture.identity, MODULE),
        "identity/module/provenance",
    );

    rejected(
        fixture
            .context
            .guard(&fixture.bundle, &fixture.identity, &"b".repeat(64)),
        "identity/module/provenance",
    );

    let mut context = fixture.context.clone();
    context.provenance = "synthetic".into();
    rejected(
        context.guard(&fixture.bundle, &fixture.identity, MODULE),
        "identity/module/provenance",
    );

    let mut context = fixture.context.clone();
    context.schema_version = 2;
    rejected(
        context.guard(&fixture.bundle, &fixture.identity, MODULE),
        "identity/module/provenance",
    );

    let mut context = fixture.context.clone();
    context.load_vblank = "0733".into();
    rejected(
        context.guard(&fixture.bundle, &fixture.identity, MODULE),
        "noncanonical context load time",
    );

    let mut expected = fixture.identity.clone();
    expected.source_sha256 = "b".repeat(64);
    rejected(
        fixture.context.guard(&fixture.bundle, &expected, MODULE),
        "source size/hash",
    );
}

#[test]
fn context_rejects_bad_ram_bounds_allocation_and_component_descriptors() {
    let fixture = fixture();

    let mut context = fixture.context.clone();
    context.source_address = RAM_START - 1;
    rejected(
        context.guard(&fixture.bundle, &fixture.identity, MODULE),
        "outside original main RAM",
    );

    let mut context = fixture.context.clone();
    context.allocation_size += 4;
    rejected(
        context.guard(&fixture.bundle, &fixture.identity, MODULE),
        "allocation differs",
    );

    let mut context = fixture.context.clone();
    context.components[0].destination_address = RAM_END - 1;
    rejected(
        context.guard(&fixture.bundle, &fixture.identity, MODULE),
        "outside original main RAM",
    );

    let mut context = fixture.context.clone();
    context.components[0].source_offset += 4;
    rejected(
        context.guard(&fixture.bundle, &fixture.identity, MODULE),
        "descriptor/bound mismatch",
    );

    let mut context = fixture.context.clone();
    context.components[0].extra.resize(MAX_EXTRA + 1, 0);
    rejected(
        context.guard(&fixture.bundle, &fixture.identity, MODULE),
        "descriptor/bound mismatch",
    );

    let mut context = fixture.context.clone();
    context.components[8].source_bytes_read = usize::MAX;
    rejected(
        context.guard(&fixture.bundle, &fixture.identity, MODULE),
        "source range overflow",
    );

    let mut context = fixture.context.clone();
    context.components[8].source_bytes_read = usize::MAX - context.components[8].source_offset;
    rejected(
        context.guard(&fixture.bundle, &fixture.identity, MODULE),
        "outside original main RAM",
    );

    let mut context = fixture.context.clone();
    context.components[8].output_size = usize::MAX;
    rejected(
        context.guard(&fixture.bundle, &fixture.identity, MODULE),
        "outside original main RAM",
    );
}

#[test]
fn context_rejects_incomplete_duplicate_and_tampered_runtime_observations() {
    let fixture = fixture();

    let mut context = fixture.context.clone();
    context.components.pop();
    rejected(
        context.guard(&fixture.bundle, &fixture.identity, MODULE),
        "all nine",
    );

    let mut context = fixture.context.clone();
    context.components[1].index = context.components[0].index;
    rejected(
        context.guard(&fixture.bundle, &fixture.identity, MODULE),
        "duplicate/out-of-range",
    );

    let mut context = fixture.context.clone();
    context.components[8].extra.pop();
    rejected(
        context.guard(&fixture.bundle, &fixture.identity, MODULE),
        "source truncated",
    );

    let mut context = fixture.context.clone();
    context.components[4].output_sha256 = "b".repeat(64);
    rejected(
        context.guard(&fixture.bundle, &fixture.identity, MODULE),
        "complete output/consumption",
    );
}

#[test]
fn context_rejects_conflicting_measured_bytes_past_indexed_eof() {
    let mut fixture = fixture();
    let event_offset = u32::from_le_bytes(
        fixture.bundle[0x130 + 8 * 4..0x130 + 9 * 4]
            .try_into()
            .unwrap(),
    );
    fixture.bundle[0x10c..0x10c + 4].copy_from_slice(&72_u32.to_le_bytes());
    fixture.bundle[0x130..0x130 + 4].copy_from_slice(&event_offset.to_le_bytes());
    fixture.identity.source_sha256 = hash(&fixture.bundle);
    fixture.context.identity = fixture.identity.clone();

    let event = fixture.context.components[8].clone();
    let mut duplicate = event;
    duplicate.index = 0;
    duplicate.destination_address -= 0x8000;
    duplicate.extra[4] ^= 1; // Only the final speculative flag differs; decoded output is unchanged.
    fixture.context.components[0] = duplicate;

    rejected(
        fixture
            .context
            .guard(&fixture.bundle, &fixture.identity, MODULE),
        "observations conflict beyond indexed EOF",
    );
}
#[test]
fn result_verification_rejects_extreme_component_indices_and_descriptors() {
    let fixture = fixture();

    let mut context = fixture.context.clone();
    context.components[0].index = usize::MAX;
    assert!(context.verify_outputs(&fixture.bundle, None).is_err());

    let mut bundle = fixture.bundle.clone();
    bundle[0x10c..0x110].copy_from_slice(&u32::MAX.to_le_bytes());
    assert!(fixture.context.verify_outputs(&bundle, None).is_err());
}
