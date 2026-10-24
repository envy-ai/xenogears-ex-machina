use std::collections::BTreeMap;
use xem_disc::field_context::{
    ComponentContext, MAX_COMPONENT, RAM_START, SourceContext, SourceIdentity, hash,
};
use xem_disc::{KnownDisc, packed};
use xem_studio::scene::{
    bundle,
    formats::{self, Descriptor},
};

const HEADER_END: usize = 0x190;
const COMPONENT_SIZES: [usize; 9] = [3, 4, 5, 6, 7, 8, 0, 2, 9];

fn put_u16(bytes: &mut [u8], at: usize, value: u16) {
    bytes[at..at + 2].copy_from_slice(&value.to_le_bytes());
}

fn put_u32(bytes: &mut [u8], at: usize, value: u32) {
    bytes[at..at + 4].copy_from_slice(&value.to_le_bytes());
}

fn descriptor(index: usize) -> Descriptor {
    Descriptor {
        flags: 0x2000 | index as u16,
        rotation: [0x1000 + index as u16, 0x2000, 0x3000],
        position: [index as u16, 0x4567, 0x89AB],
        model: 0x5000 + index as u16,
    }
}

fn bundle_fixture(descriptor_count: usize, truncate_last_terminal_flag: bool) -> Vec<u8> {
    let logical: Vec<Vec<u8>> = COMPONENT_SIZES
        .iter()
        .enumerate()
        .map(|(index, size)| vec![0x30 + index as u8; *size])
        .collect();
    let encoded: Vec<Vec<u8>> = logical
        .iter()
        .map(|component| packed::encode_literals(component, MAX_COMPONENT).expect("literal pack"))
        .collect();
    let mut bundle = vec![0xA5; HEADER_END + descriptor_count * 16];

    for index in 0..32 {
        put_u16(&mut bundle, index * 8, 0x100 + index as u16);
        put_u16(&mut bundle, index * 8 + 2, 0x200 + index as u16);
        put_u16(&mut bundle, index * 8 + 4, 0xA000 + index as u16);
        put_u16(&mut bundle, index * 8 + 6, index as u16);
    }
    for index in 0..28 {
        put_u16(
            &mut bundle,
            0x154 + index * 2,
            (index as u16).wrapping_mul(0x111),
        );
    }
    put_u16(&mut bundle, 0x18c, descriptor_count as u16);
    for index in 0..descriptor_count {
        let value = descriptor(index);
        let at = HEADER_END + index * 16;
        put_u16(&mut bundle, at, value.flags);
        for (axis, word) in value.rotation.into_iter().enumerate() {
            put_u16(&mut bundle, at + 2 + axis * 2, word);
        }
        for (axis, word) in value.position.into_iter().enumerate() {
            put_u16(&mut bundle, at + 8 + axis * 2, word);
        }
        put_u16(&mut bundle, at + 14, value.model);
    }

    for index in 0..9 {
        while !bundle.len().is_multiple_of(4) {
            bundle.push(0xCC);
        }
        let offset = bundle.len();
        put_u32(
            &mut bundle,
            0x10c + index * 4,
            COMPONENT_SIZES[index] as u32,
        );
        put_u32(&mut bundle, 0x130 + index * 4, offset as u32);
        let mut block = encoded[index].clone();
        if truncate_last_terminal_flag && index == 8 {
            block.pop();
            block.pop();
        }
        bundle.extend_from_slice(&block);
        if index < 8 {
            bundle.extend_from_slice(&[0xE0 + index as u8, 0x70 + index as u8, 0xD0]);
        }
    }
    bundle
}

fn empty_terminal_c8_fixture(descriptor_count: usize) -> Vec<u8> {
    let mut bundle = bundle_fixture(descriptor_count, false);
    let offset = u32::from_le_bytes(
        bundle[0x130 + 8 * 4..0x134 + 8 * 4]
            .try_into()
            .expect("c8 source offset"),
    ) as usize;
    put_u32(&mut bundle, 0x10c + 8 * 4, 0);
    put_u32(&mut bundle, offset, 0);
    bundle.truncate(offset + 4);
    bundle
}

fn message_table(text: &[u8], entry: usize, text_end: usize, count_word: u16) -> Vec<u8> {
    let mut bytes = Vec::new();
    bytes.extend_from_slice(&count_word.to_le_bytes());
    bytes.extend_from_slice(&0u16.to_le_bytes());
    if count_word != u16::MAX {
        bytes.extend_from_slice(&(entry as u16).to_le_bytes());
        bytes.extend_from_slice(&(text_end as u16).to_le_bytes());
        bytes.extend_from_slice(&[1, 1]); // one message, one by one layout
        bytes.resize(text_end, 0x55);
        bytes[entry..entry + text.len()].copy_from_slice(text);
    }
    bytes
}

fn with_truncated_c7(logical: &[u8]) -> Vec<u8> {
    let advertised_size = (logical.len() + 1).div_ceil(8) * 8;
    assert!(advertised_size > logical.len());
    assert!(advertised_size <= logical.len() + 16);
    let mut advertised = logical.to_vec();
    advertised.resize(advertised_size, 0xA7);
    let encoded = packed::encode_literals(&advertised, MAX_COMPONENT).expect("encode C7 fixture");
    let prefix = packed::decode_prefix(&encoded, logical.len(), MAX_COMPONENT)
        .expect("logical C7 prefix is source-available");
    let stream = &encoded[..prefix.source_bytes_read];

    let mut bundle = bundle_fixture(1, false);
    let offset = bundle.len();
    put_u32(&mut bundle, 0x10c + 7 * 4, logical.len() as u32);
    put_u32(&mut bundle, 0x130 + 7 * 4, offset as u32);
    bundle.extend_from_slice(stream);
    bundle
}

fn missing_logical_c8_output() -> Vec<u8> {
    let mut bundle = bundle_fixture(0, false);
    let offset = formats::decode_header(&bundle).unwrap().offsets[8];
    put_u32(&mut bundle, 0x10c + 8 * 4, 24);
    let mut packed = packed::encode_literals(&[0; 24], MAX_COMPONENT).expect("literal pack");
    packed.truncate(packed.len() - 2); // 23 output bytes; final token and terminal flag absent.
    bundle.truncate(offset);
    bundle.extend_from_slice(&packed);
    bundle
}

fn full_c8_output_missing_terminal_flag() -> Vec<u8> {
    let mut bundle = bundle_fixture(1, false);
    let offset = formats::decode_header(&bundle).unwrap().offsets[8];
    put_u32(&mut bundle, 0x10c + 8 * 4, 24);
    let mut packed = packed::encode_literals(&[0; 24], MAX_COMPONENT).expect("literal pack");
    packed.pop(); // All 24 advertised outputs are available; only the terminal lookahead is absent.
    bundle.truncate(offset);
    bundle.extend_from_slice(&packed);
    bundle
}

fn c8_advertised_output_truncated_mid_flag_group() -> Vec<u8> {
    let mut bundle = empty_terminal_c8_fixture(1);
    let offset = formats::decode_header(&bundle).expect("header").offsets[8];
    put_u32(&mut bundle, offset, 10);
    // The first group is complete. Two more literals provide all ten declared
    // bytes, but the second group's remaining six token decisions are absent.
    bundle.extend_from_slice(&[
        0, 0x41, 0x42, 0x43, 0x44, 0x45, 0x46, 0x47, 0x48, 0, 0x49, 0x4A,
    ]);
    bundle
}

fn c8_full_output_back_reference_missing_terminal() -> Vec<u8> {
    let mut bundle = empty_terminal_c8_fixture(1);
    let offset = formats::decode_header(&bundle).expect("header").offsets[8];
    put_u32(&mut bundle, offset, 10);
    // Seven literals followed by a three-byte back-reference completes all
    // eight tokens and ten advertised outputs, but omits the next flag.
    bundle.extend_from_slice(&[0x80, 0x41, 0x42, 0x43, 0x44, 0x45, 0x46, 0x47, 0x01, 0x00]);
    bundle
}

fn c8_partial_group_cannot_reach_advertised_output() -> Vec<u8> {
    let mut bundle = empty_terminal_c8_fixture(1);
    let offset = formats::decode_header(&bundle).expect("header").offsets[8];
    put_u32(&mut bundle, offset, 10);
    // Nine literal outputs are available. Completing this second eight-token
    // group needs seven more tokens, so even seven one-byte literals exceed
    // the advertised length of ten.
    bundle.extend_from_slice(&[0, 0x41, 0x42, 0x43, 0x44, 0x45, 0x46, 0x47, 0, 0x48]);
    bundle
}

fn missing_opaque_c6_suffix() -> Vec<u8> {
    let mut bundle = bundle_fixture(1, false);
    let logical = [0x6A; 8];
    let mut packed = packed::encode_literals(&[0x6A; 16], MAX_COMPONENT).expect("literal pack");
    let prefix = packed::decode_prefix(&packed, logical.len(), MAX_COMPONENT)
        .expect("C6 logical prefix is source-available");
    packed.truncate(prefix.source_bytes_read);
    let offset = bundle.len();
    put_u32(&mut bundle, 0x10c + 6 * 4, logical.len() as u32);
    put_u32(&mut bundle, 0x130 + 6 * 4, offset as u32);
    bundle.extend_from_slice(&packed);
    bundle
}

fn input(bundle: Vec<u8>) -> bundle::Input {
    bundle::Input {
        bundle,
        context: None,
        source: None,
        module_sha256: None,
    }
}

fn identity(bundle: &[u8]) -> SourceIdentity {
    SourceIdentity {
        disc: 1,
        boot_sha256: xem_disc::identify::hex(&KnownDisc::Disc1.executable_sha256()),
        map: 23,
        slot: 120,
        lba: 100,
        indexed_size: bundle.len() as u32,
        source_sha256: hash(bundle),
    }
}

fn mismatched_context(bundle: &[u8], expected: &SourceIdentity, module: &str) -> SourceContext {
    let mut wrong = expected.clone();
    wrong.map += 1;
    SourceContext {
        schema_version: 1,
        provenance: "runtime_field_decompress".into(),
        module_sha256: module.to_owned(),
        identity: wrong,
        source_address: RAM_START,
        allocation_size: ((bundle.len() + 3) & !3) as u32,
        load_vblank: "1".into(),
        components: Vec::<ComponentContext>::new(),
    }
}

fn measured_context(bundle: &[u8], extra: &[u8], module: &str) -> SourceContext {
    let header = formats::decode_header(bundle).expect("decode measured fixture header");
    let components = (0..9)
        .map(|index| {
            let start = header.offsets[index];
            let mut supplied = bundle[start..].to_vec();
            if index == 8 {
                supplied.extend_from_slice(extra);
            }
            let decoded = packed::decode_block(&supplied, header.logical_sizes[index] + 16)
                .expect("component decodes with its measured source tail");
            ComponentContext {
                index,
                source_offset: start,
                logical_size: header.logical_sizes[index],
                packed_size: u32::from_le_bytes(
                    bundle[start..start + 4]
                        .try_into()
                        .expect("packed size word"),
                ) as usize,
                source_bytes_read: decoded.source_bytes_read,
                output_size: decoded.data.len(),
                output_sha256: hash(&decoded.data),
                destination_address: RAM_START + 0x10000 + index as u32 * 0x1000,
                extra: if index == 8 {
                    extra.to_vec()
                } else {
                    Vec::new()
                },
            }
        })
        .collect();
    SourceContext {
        schema_version: 1,
        provenance: "runtime_field_decompress".into(),
        module_sha256: module.to_owned(),
        identity: identity(bundle),
        source_address: RAM_START + 0x1000,
        allocation_size: ((bundle.len() + 3) & !3) as u32,
        load_vblank: "1".into(),
        components,
    }
}

#[test]
fn all_nine_components_load_and_no_edit_repack_preserves_the_exact_bundle() {
    let original = bundle_fixture(2, false);
    let source = bundle::Source::load(input(original.clone())).expect("load all nine components");

    assert_eq!(source.components.len(), 9);
    assert_eq!(source.header.descriptors, [descriptor(0), descriptor(1)]);
    for (index, component) in source.components.iter().enumerate() {
        assert_eq!(component.logical_size, COMPONENT_SIZES[index]);
        assert_eq!(
            &component.bytes[..component.logical_size],
            &vec![0x30 + index as u8; component.logical_size]
        );
        assert!(component.bytes.len() - component.logical_size <= 16);
    }
    let rebuilt = source
        .repack(&source.header, &BTreeMap::new())
        .expect("no edit returns original source");
    assert_eq!(rebuilt, original);

    let source_only = bundle::Source::load(bundle::Input {
        bundle: rebuilt,
        context: None,
        source: Some(identity(&original)),
        module_sha256: None,
    })
    .expect("source identity may be validated without measured context");
    assert_eq!(source_only.input().bundle, original);
}

#[test]
fn descriptor_growth_and_shrink_relocate_all_components_and_keep_opaque_gaps() {
    let original = bundle_fixture(2, false);
    let source = bundle::Source::load(input(original.clone())).expect("load source");
    let old_end = HEADER_END + 2 * 16;

    let mut grown_header = source.header.clone();
    grown_header.descriptors.push(descriptor(2));
    let grown = source
        .repack(&grown_header, &BTreeMap::new())
        .expect("grow descriptor table");
    let grown_end = HEADER_END + 3 * 16;
    assert_eq!(&grown[grown_end..], &original[old_end..]);
    let grown_source = bundle::Source::load(input(grown.clone())).expect("reload grown bundle");
    assert_eq!(grown_source.header.descriptors, grown_header.descriptors);
    for index in 0..9 {
        assert_eq!(
            grown_source.header.offsets[index],
            source.header.offsets[index] + 16
        );
        assert_eq!(
            grown_source.components[index].bytes,
            source.components[index].bytes
        );
    }

    let mut shrunk_header = source.header.clone();
    shrunk_header.descriptors.truncate(1);
    let shrunk = source
        .repack(&shrunk_header, &BTreeMap::new())
        .expect("shrink descriptor table");
    let shrunk_end = HEADER_END + 16;
    assert_eq!(&shrunk[shrunk_end..], &original[old_end..]);
    let shrunk_source = bundle::Source::load(input(shrunk)).expect("reload shrunk bundle");
    assert_eq!(shrunk_source.header.descriptors, [descriptor(0)]);
    for index in 0..9 {
        assert_eq!(
            shrunk_source.header.offsets[index],
            source.header.offsets[index] - 16
        );
        assert_eq!(
            shrunk_source.components[index].bytes,
            source.components[index].bytes
        );
    }
}

#[test]
fn component_replacement_repacks_one_zone_stream_and_preserves_all_other_outputs() {
    let original = bundle_fixture(1, false);
    let source = bundle::Source::load(input(original)).expect("load source");
    let old_components: Vec<Vec<u8>> = source.components.iter().map(|c| c.bytes.clone()).collect();
    let mut zone_bytes = Vec::new();
    for value in [1i16, -2, 3, 4, 5, -6, 7, 8, 9, -10, 11, 12] {
        zone_bytes.extend_from_slice(&value.to_le_bytes());
    }
    let replacement = bundle::Replacement {
        logical_size: zone_bytes.len(),
        bytes: zone_bytes.clone(),
    };
    let mut replacements = BTreeMap::new();
    replacements.insert(8, replacement);

    let original_input = source.input().bundle.clone();
    let result = source
        .repack(&source.header, &replacements)
        .expect("replace zone component");
    assert_eq!(source.input().bundle, original_input);
    let updated = bundle::Source::load(input(result)).expect("reload replacement");
    assert_eq!(updated.components[8].logical_size, zone_bytes.len());
    assert_eq!(updated.components[8].bytes, zone_bytes);
    for (index, old_bytes) in old_components.iter().enumerate() {
        if index != 8 {
            assert_eq!(updated.components[index].bytes, *old_bytes);
        }
    }
}

#[test]
fn replacement_padding_cannot_activate_an_unresolved_event_destination() {
    let original = bundle_fixture(1, false);
    let source = bundle::Source::load(input(original.clone())).expect("load source");

    // A branch at bytecode PC 0 targets PC 10 while the authored stream ends at
    // PC 8. Literal packing rounds the component from 140 to 144 bytes, making
    // that target reachable through the zero-filled packing tail unless refused.
    let mut component = vec![0_u8; 196];
    component[128..132].copy_from_slice(&1_u32.to_le_bytes());
    component.extend_from_slice(&[0x02, 0, 0, 0, 0, 0, 10, 0]);
    let logical_size = component.len();
    let mut replacements = BTreeMap::new();
    replacements.insert(
        5,
        bundle::Replacement {
            logical_size,
            bytes: component,
        },
    );

    let error = source
        .repack(&source.header, &replacements)
        .expect_err("packed padding must not activate an outside branch target");
    assert_eq!(error.kind, "unsupported_relocation");
    assert_eq!(
        source.input().bundle,
        original,
        "rejection leaves source unchanged"
    );
}

#[test]
fn replacement_bounds_and_indices_fail_atomically() {
    let original = bundle_fixture(1, false);
    let source = bundle::Source::load(input(original.clone())).expect("load source");
    let mut replacements = BTreeMap::new();
    replacements.insert(
        9,
        bundle::Replacement {
            logical_size: 0,
            bytes: vec![],
        },
    );
    assert!(source.repack(&source.header, &replacements).is_err());

    replacements.clear();
    replacements.insert(
        3,
        bundle::Replacement {
            logical_size: usize::MAX,
            bytes: vec![],
        },
    );
    assert!(source.repack(&source.header, &replacements).is_err());

    replacements.clear();
    replacements.insert(
        3,
        bundle::Replacement {
            logical_size: 0,
            bytes: vec![0; 17],
        },
    );
    assert!(source.repack(&source.header, &replacements).is_err());

    replacements.clear();
    replacements.insert(
        3,
        bundle::Replacement {
            logical_size: MAX_COMPONENT,
            bytes: vec![],
        },
    );
    assert!(source.repack(&source.header, &replacements).is_err());
    assert_eq!(source.input().bundle, original);
}

#[test]
fn descriptor_growth_respects_the_bundle_file_limit_before_allocating_output() {
    let mut original = bundle_fixture(1, false);
    original.resize((32 << 20) - 8, 0xC7);
    let source = bundle::Source::load(input(original.clone())).expect("load bounded large bundle");
    let mut grown = source.header.clone();
    grown.descriptors.push(descriptor(1));
    assert!(source.repack(&grown, &BTreeMap::new()).is_err());
    assert_eq!(source.input().bundle, original);
}

#[test]
fn missing_source_guard_requests_measured_context_instead_of_fabricating_tail_bytes() {
    let truncated = missing_logical_c8_output();
    let error = bundle::Source::load(input(truncated)).expect_err("final source bytes are absent");
    assert_eq!(error.kind, "source_context_required");
}

#[test]
fn final_empty_component_is_a_valid_offline_source_and_no_edit_is_exact() {
    let original = empty_terminal_c8_fixture(1);
    let header = formats::decode_header(&original).expect("header remains complete");
    assert_eq!(header.offsets[8] + 4, original.len());
    assert_eq!(header.logical_sizes[8], 0);

    let source = bundle::Source::load(input(original.clone()))
        .expect("an empty final packed component has no missing payload");
    let component = &source.components[8];
    assert_eq!(component.logical_size, 0);
    assert!(component.bytes.is_empty());
    assert!(component.needs_stream_completion);
    assert_eq!(component.optional_tail_bytes, 0);

    let unchanged = source
        .repack(&source.header, &BTreeMap::new())
        .expect("no edit preserves original source");
    assert_eq!(hash(&unchanged), hash(&original));
}

#[test]
fn full_advertised_zone_output_survives_missing_terminal_lookahead() {
    let original = full_c8_output_missing_terminal_flag();
    let source = bundle::Source::load(input(original.clone()))
        .expect("the full advertised zone output is known without its terminal flag");
    assert_eq!(source.components[8].logical_size, 24);
    assert_eq!(source.components[8].bytes.len(), 24);
    assert!(source.components[8].needs_stream_completion);
    assert_eq!(source.components[8].optional_tail_bytes, 0);
    assert!(formats::decode_zones(&source.components[8].bytes, 24).is_ok());

    let mut edited = source.header.clone();
    edited.descriptors.push(descriptor(1));
    let candidate = source
        .repack(&edited, &BTreeMap::new())
        .expect("an edit authors a strict terminal flag");
    let reloaded = bundle::Source::load(input(candidate)).expect("strict candidate reloads");
    assert_eq!(reloaded.components[8].bytes.len(), 24);
    assert!(!reloaded.components[8].needs_stream_completion);
    assert_eq!(
        hash(&reloaded.components[8].bytes),
        hash(&source.components[8].bytes)
    );
}

#[test]
fn full_advertised_output_inside_an_incomplete_flag_group_is_rejected() {
    let malformed = c8_advertised_output_truncated_mid_flag_group();
    let error = bundle::Source::load(input(malformed))
        .expect_err("known literal byte does not prove the missing flag-group framing");
    assert_eq!(error.kind, "invalid_scene_bundle");
    assert!(error.message.contains("incomplete eight-token flag group"));
}

#[test]
fn complete_back_reference_flag_group_is_admitted_without_terminal_lookahead() {
    let original = c8_full_output_back_reference_missing_terminal();
    let source = bundle::Source::load(input(original.clone()))
        .expect("a complete eight-token back-reference group has known output");
    let component = &source.components[8];
    assert_eq!(component.logical_size, 0);
    assert_eq!(
        component.bytes,
        [0x41, 0x42, 0x43, 0x44, 0x45, 0x46, 0x47, 0x47, 0x47, 0x47]
    );
    assert!(component.needs_stream_completion);
    assert_eq!(component.optional_tail_bytes, 0);
    assert_eq!(
        source.repack(&source.header, &BTreeMap::new()).unwrap(),
        original
    );
}

#[test]
fn remaining_flag_group_tokens_cannot_exceed_advertised_output() {
    let malformed = c8_partial_group_cannot_reach_advertised_output();
    let error = bundle::Source::load(input(malformed))
        .expect_err("mandatory remaining tokens exceed the declared length");
    assert_eq!(error.kind, "invalid_scene_bundle");
}

#[test]
fn unqualified_opaque_missing_suffix_still_requires_measured_context() {
    let error = bundle::Source::load(input(missing_opaque_c6_suffix()))
        .expect_err("available logical bytes do not qualify an opaque C6 suffix");
    assert_eq!(error.kind, "source_context_required");
}

#[test]
fn any_header_edit_normalizes_an_empty_terminal_to_a_strict_stream() {
    let original = empty_terminal_c8_fixture(1);
    let source = bundle::Source::load(input(original.clone())).expect("load empty terminal");
    let before: Vec<Vec<u8>> = source.components.iter().map(|c| c.bytes.clone()).collect();
    let mut edited_header = source.header.clone();
    edited_header.descriptors.push(descriptor(1));

    let candidate = source
        .repack(&edited_header, &BTreeMap::new())
        .expect("descriptor growth completes the missing terminal read");
    assert_eq!(hash(&source.input().bundle), hash(&original));
    let final_header = formats::decode_header(&candidate).expect("candidate header");
    let c8 = packed::decode_block(&candidate[final_header.offsets[8]..], 16)
        .expect("authored empty stream contains its terminal flag");
    assert!(c8.data.is_empty());

    let reloaded =
        bundle::Source::load(input(candidate)).expect("all candidate streams strict-load");
    assert_eq!(reloaded.header.descriptors.len(), 2);
    assert!(!reloaded.components[8].needs_stream_completion);
    for (index, old) in before.iter().enumerate() {
        if source.components[index].needs_stream_completion {
            assert_eq!(
                hash(&reloaded.components[index].bytes[..old.len()]),
                hash(old)
            );
            assert!(reloaded.components[index].bytes.len() - old.len() <= 16);
        } else {
            assert_eq!(hash(&reloaded.components[index].bytes), hash(old));
        }
    }
}

#[test]
fn other_component_edit_also_normalizes_the_empty_terminal() {
    let original = empty_terminal_c8_fixture(1);
    let source = bundle::Source::load(input(original.clone())).expect("load empty terminal");
    let mut replacement = source.components[0].bytes.clone();
    replacement[0] ^= 1;
    let replacements = BTreeMap::from([(
        0,
        bundle::Replacement {
            logical_size: source.components[0].logical_size,
            bytes: replacement.clone(),
        },
    )]);

    let candidate = source
        .repack(&source.header, &replacements)
        .expect("a sparse replacement normalizes every incomplete stream");
    let reloaded = bundle::Source::load(input(candidate)).expect("all streams strict-load");
    assert!(!reloaded.components[8].needs_stream_completion);
    assert_eq!(
        hash(&reloaded.components[0].bytes[..replacement.len()]),
        hash(&replacement)
    );
    for index in 1..8 {
        assert_eq!(
            hash(&reloaded.components[index].bytes),
            hash(&source.components[index].bytes)
        );
    }
    let reverted = source
        .repack(
            &source.header,
            &BTreeMap::from([(
                0,
                bundle::Replacement {
                    logical_size: source.components[0].logical_size,
                    bytes: source.components[0].bytes.clone(),
                },
            )]),
        )
        .expect("reverting the source replacement is an exact no-op");
    assert_eq!(hash(&reverted), hash(&original));
}

#[test]
fn empty_terminal_admission_does_not_accept_malformed_or_missing_output() {
    let empty = empty_terminal_c8_fixture(1);
    let offset = formats::decode_header(&empty).unwrap().offsets[8];

    let short_header = empty[..offset + 3].to_vec();
    assert_eq!(
        bundle::Source::load(input(short_header)).unwrap_err().kind,
        "invalid_scene"
    );

    let mut logical_without_declared_output = empty.clone();
    put_u32(&mut logical_without_declared_output, 0x10c + 8 * 4, 1);
    assert_eq!(
        bundle::Source::load(input(logical_without_declared_output))
            .unwrap_err()
            .kind,
        "invalid_scene_bundle"
    );

    let missing_declared_output = missing_logical_c8_output();
    assert_eq!(
        bundle::Source::load(input(missing_declared_output))
            .unwrap_err()
            .kind,
        "source_context_required"
    );

    let mut terminal_present = empty_terminal_c8_fixture(1);
    terminal_present.push(0);
    let source = bundle::Source::load(input(terminal_present))
        .expect("a present terminal flag follows the strict decoder path");
    assert!(!source.components[8].needs_stream_completion);
}

#[test]
fn empty_zone_prefix_can_omit_a_bounded_advertised_tail_but_not_exceed_allocation() {
    let empty = empty_terminal_c8_fixture(1);
    let offset = formats::decode_header(&empty).unwrap().offsets[8];
    for declared in [8u32, 16] {
        let mut source_bytes = empty.clone();
        put_u32(&mut source_bytes, offset, declared);
        let source = bundle::Source::load(input(source_bytes))
            .expect("zero complete zones need no absent optional zone suffix");
        assert!(source.components[8].bytes.is_empty());
        assert!(source.components[8].needs_stream_completion);
        assert_eq!(source.components[8].optional_tail_bytes, declared as usize);
    }

    let mut oversized = empty;
    put_u32(&mut oversized, offset, 17);
    assert_eq!(
        bundle::Source::load(input(oversized)).unwrap_err().kind,
        "invalid_scene_bundle",
        "an omitted suffix cannot exceed the original logical+16 allocation"
    );
}

#[test]
fn impossible_nonempty_packed_outputs_are_rejected_before_prefix_admission() {
    let empty = empty_terminal_c8_fixture(1);
    let offset = formats::decode_header(&empty).unwrap().offsets[8];
    for declared in [1u32, 2, 3, 4, 5, 6, 7, 9] {
        let mut malformed = empty.clone();
        put_u32(&mut malformed, offset, declared);
        let error = bundle::Source::load(input(malformed))
            .expect_err("no complete nonempty eight-token groups fit this output length");
        assert_eq!(error.kind, "invalid_scene_bundle", "declared={declared}");
    }
}

#[test]
fn measured_context_still_supplies_the_terminal_flag_for_empty_component() {
    let original = empty_terminal_c8_fixture(1);
    let module = "cd".repeat(32);
    let context = measured_context(&original, &[0], &module);
    let source = bundle::Source::load(bundle::Input {
        bundle: original,
        context: Some(context),
        source: Some(identity(&empty_terminal_c8_fixture(1))),
        module_sha256: Some(module),
    })
    .expect("measured one-byte source tail validates and completes the stream");
    assert!(source.components[8].bytes.is_empty());
    assert!(!source.components[8].needs_stream_completion);
    assert_eq!(source.components[8].optional_tail_bytes, 0);
}

#[test]
fn incomplete_c7_prefix_accepts_the_font_independent_terminator_proofs() {
    let cases = [
        (
            "one_byte_and_two_byte_paths",
            message_table(&[0x44, 0, 0], 10, 13, 1),
        ),
        ("final_offset_is_a_logical_nul", {
            let mut bytes = message_table(&[], 12, 12, 1);
            bytes.resize(13, 0x55);
            bytes[12] = 0;
            bytes
        }),
        (
            "selection_control_rereads_04",
            message_table(&[0x0F, 0x04, 0, 0], 10, 14, 1),
        ),
        (
            "set_6d_operand_is_text",
            message_table(&[0x0F, 0x0B, 0], 10, 13, 1),
        ),
        (
            "no_message_sentinel_needs_only_four_bytes",
            vec![0xFF, 0xFF, 0, 0],
        ),
    ];
    for (label, logical) in cases {
        let source = bundle::Source::load(input(with_truncated_c7(&logical)))
            .unwrap_or_else(|error| panic!("{label}: {} ({})", error.kind, error.message));
        assert!(source.components[7].needs_stream_completion, "{label}");
        assert_eq!(source.components[7].logical_size, logical.len(), "{label}");
    }
}

#[test]
fn incomplete_c7_prefix_rejects_ambiguous_or_incomplete_message_boundaries() {
    let cases = [
        (
            "truncated_unsupported_selection_reread",
            message_table(&[0x0F, 0x04, 0], 10, 13, 1),
        ),
        (
            "single_zero_only_proves_one_glyph_width",
            message_table(&[0x44, 0], 10, 12, 1),
        ),
        (
            "truncated_extended_control_subcode",
            message_table(&[0x0F], 10, 11, 1),
        ),
        (
            "truncated_two_byte_glyph",
            message_table(&[0xFE], 10, 11, 1),
        ),
    ];
    for (label, logical) in cases {
        let error = bundle::Source::load(input(with_truncated_c7(&logical)))
            .expect_err("one possible message path lacks a logical terminator");
        assert_eq!(error.kind, "source_context_required", "{label}");
    }

    let mut malformed = message_table(&[0], 10, 11, 1);
    malformed[2..4].copy_from_slice(&1u16.to_le_bytes());
    let error = bundle::Source::load(input(with_truncated_c7(&malformed)))
        .expect_err("reserved message-table halfword must stay zero");
    assert_eq!(error.kind, "invalid_scene_bundle");
}

#[test]
fn source_identity_does_not_fabricate_missing_logical_component_output() {
    let truncated = missing_logical_c8_output();
    let near_match = identity(&truncated); // Correct disc and map, but synthetic index location/size.
    let error = bundle::Source::load(bundle::Input {
        bundle: truncated,
        context: None,
        source: Some(near_match),
        module_sha256: None,
    })
    .expect_err("the narrow real-source exception cannot be selected by a similar identity");
    assert_eq!(error.kind, "source_context_required");
}

#[test]
fn measured_synthetic_guard_materializes_full_outputs_through_header_growth_and_replacement() {
    // This is a synthetic context-contract fixture, not evidence of runtime
    // observation or identification for the fabricated component streams.
    let complete = bundle_fixture(1, false);
    let truncated = bundle_fixture(1, true);
    let extra = complete[complete.len() - 2..].to_vec();
    assert_eq!(truncated.len() + extra.len(), complete.len());
    let module = "ab".repeat(32);
    let context = measured_context(&truncated, &extra, &module);
    let source_identity = identity(&truncated);
    let complete_source =
        bundle::Source::load(input(complete)).expect("load full synthetic source");
    let source = bundle::Source::load(bundle::Input {
        bundle: truncated.clone(),
        context: Some(context.clone()),
        source: Some(source_identity.clone()),
        module_sha256: Some(module.clone()),
    })
    .expect("verified synthetic source guard completes the final decoder reads");
    for index in 0..9 {
        assert_eq!(
            source.components[index].bytes,
            complete_source.components[index].bytes
        );
    }

    let mut grown_header = source.header.clone();
    grown_header.descriptors.push(descriptor(1));
    let replacement = bundle::Replacement {
        logical_size: 1,
        bytes: vec![0x6E],
    };
    let replacements = BTreeMap::from([(6, replacement)]);
    let output = source
        .repack(&grown_header, &replacements)
        .expect("materialize measured tail before authored replacement");
    let updated = bundle::Source::load(input(output)).expect("reload materialized bundle");
    assert_eq!(updated.header.descriptors, [descriptor(0), descriptor(1)]);
    assert_eq!(updated.components[6].logical_size, 1);
    assert_eq!(&updated.components[6].bytes[..1], &[0x6E]);
    for index in 0..9 {
        if index != 6 {
            assert_eq!(
                updated.components[index].bytes,
                source.components[index].bytes
            );
        }
    }

    let mut wrong_guard = context;
    wrong_guard.components[8].extra[0] ^= 1;
    let bad = bundle::Input {
        bundle: truncated,
        context: Some(wrong_guard),
        source: Some(source_identity),
        module_sha256: Some(module),
    };
    assert_eq!(
        bundle::Source::load(bad).unwrap_err().kind,
        "invalid_source_context"
    );
}

#[test]
fn partial_or_identity_mismatched_measured_context_is_rejected() {
    let original = bundle_fixture(1, false);
    let expected = identity(&original);
    let module = "ab".repeat(32);
    let context = mismatched_context(&original, &expected, &module);
    let partial = bundle::Input {
        bundle: original.clone(),
        context: Some(context.clone()),
        source: None,
        module_sha256: Some(module.clone()),
    };
    assert_eq!(
        bundle::Source::load(partial).unwrap_err().kind,
        "invalid_scene_bundle"
    );

    let module_only = bundle::Input {
        bundle: original.clone(),
        context: None,
        source: None,
        module_sha256: Some(module.clone()),
    };
    assert_eq!(
        bundle::Source::load(module_only).unwrap_err().kind,
        "invalid_scene_bundle"
    );

    let wrong_context = bundle::Input {
        bundle: original,
        context: Some(context),
        source: Some(expected),
        module_sha256: Some(module),
    };
    assert_eq!(
        bundle::Source::load(wrong_context).unwrap_err().kind,
        "invalid_source_context"
    );
}
