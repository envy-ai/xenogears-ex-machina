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
    let truncated = bundle_fixture(0, true);
    let error = bundle::Source::load(input(truncated)).expect_err("final source bytes are absent");
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
