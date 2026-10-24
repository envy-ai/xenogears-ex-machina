//! Opt-in original-disc format/source-context preflight. This is not runtime
//! playback acceptance and writes only a private numeric/hash summary.
use serde_json::{Value, json};
use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};
use xem_disc::field_context::{SourceContext, SourceIdentity, hash};
use xem_disc::packed;
use xem_disc::{Index, KnownDisc, open_path, read_data};
use xem_studio::Studio;
use xem_studio::scene::bundle::{Input, Replacement, Source};
use xem_studio::scene::formats::decode_zones;

const ORIGINAL_BUNDLE_SHA256: &str =
    "0f0a131acdf052cae070a64912fc09eb00e0b5e6a81f9d8cb4719d6416213add";
const MODULE_SHA256: &str = "613c3a747e1c07e0e0e2632710eae1f873e4b14dda880cd6ce1c556b31c152a8";

fn command(studio: &Studio, op: &str, args: Value) -> Value {
    let request = json!({"op":op,"args":args});
    serde_json::from_str(&studio.command(&request.to_string())).expect("Studio response is JSON")
}

fn ok(response: &Value, label: &str) -> Value {
    response
        .get("ok")
        .unwrap_or_else(|| {
            panic!(
                "{label} failed: {}",
                serde_json::to_string(&response["error"]).unwrap()
            )
        })
        .clone()
}

fn bytes(value: &Value) -> Vec<u8> {
    value
        .as_array()
        .expect("byte array")
        .iter()
        .map(|x| u8::try_from(x.as_u64().expect("integer byte")).expect("byte range"))
        .collect()
}

fn packed_prefix_ends_after_complete_flag_group(stream: &[u8], end: usize) -> bool {
    if end == 4 {
        return true;
    }
    if end > stream.len() {
        return false;
    }
    let mut at = 4usize;
    while at < end {
        let Some(&flags) = stream.get(at) else {
            return false;
        };
        at += 1;
        for token in 0..8 {
            at += if flags & (1 << token) == 0 { 1 } else { 2 };
            if at >= end {
                return at == end && token == 7;
            }
        }
    }
    false
}

fn fresh_output() -> PathBuf {
    let path = PathBuf::from(
        std::env::var_os("XEM_STUDIO_M7_PREFLIGHT_OUTPUT")
            .expect("explicit ignored M7 output leaf is required"),
    );
    let repo = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .ancestors()
        .nth(4)
        .expect("crate is inside repository")
        .to_path_buf();
    assert!(
        path.starts_with(repo.join("tmp/studio/milestone-7")),
        "private output stays under ignored milestone-7"
    );
    assert!(
        !path.exists(),
        "fresh output leaf preserves previous evidence"
    );
    fs::create_dir_all(path.parent().expect("output has a parent")).expect("create output parent");
    fs::create_dir(&path).expect("create fresh output leaf");
    path
}

fn write_summary(path: &Path, summary: &Value) {
    let file = path.join("preflight-summary.json");
    let mut output = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(file)
        .expect("summary output is new");
    output
        .write_all(&serde_json::to_vec_pretty(summary).unwrap())
        .expect("write private summary");
    output.sync_all().expect("flush private summary");
}

fn operation_outcome(response: &Value) -> Value {
    if let Some(value) = response.get("ok") {
        return json!({"status":"accepted", "result_kind":value.as_object().map(|_| "object").unwrap_or("value")});
    }
    let error = &response["error"];
    json!({
        "status":"rejected",
        "kind":error.get("kind").and_then(Value::as_str).unwrap_or("unknown"),
        "reason":error.get("message").and_then(Value::as_str).unwrap_or("unspecified")
    })
}

fn outcome_counts(rows: &[Value], field: &str) -> Value {
    let mut counts = std::collections::BTreeMap::<String, usize>::new();
    for row in rows {
        let outcome = &row[field];
        let key = if outcome["status"] == "accepted" {
            "accepted".to_owned()
        } else {
            format!(
                "{}: {}",
                outcome["kind"].as_str().unwrap_or("unknown"),
                outcome["reason"].as_str().unwrap_or("unspecified")
            )
        };
        *counts.entry(key).or_default() += 1;
    }
    json!(counts)
}

fn offline_audit_output() -> PathBuf {
    let path = PathBuf::from(
        std::env::var_os("XEM_STUDIO_OFFLINE_AUDIT_OUTPUT")
            .expect("explicit ignored offline-audit output directory is required"),
    );
    let repo = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .ancestors()
        .nth(4)
        .expect("crate is inside repository")
        .to_path_buf();
    assert!(
        path.starts_with(repo.join("tmp/studio/offline-components")),
        "private output stays under ignored offline-components"
    );
    assert!(
        !path.exists(),
        "fresh audit output preserves prior evidence"
    );
    fs::create_dir_all(path.parent().expect("output has a parent"))
        .expect("create ignored output parent");
    fs::create_dir(&path).expect("create fresh ignored output directory");
    path
}

fn audit_indexed_disc(number: u8, path: &Path) -> Value {
    const MAP_FIRST_FILE: u32 = 0xB8;
    let mut track = open_path(path).expect("open explicitly supplied original raw disc");
    let index = Index::from_disc(track.as_mut()).expect("read raw on-disc file index");
    assert_eq!(index.disc_number(), Some(u16::from(number)));
    let first_slot = index
        .slot(4, MAP_FIRST_FILE)
        .expect("field map directory starts in the index");
    let marker = index
        .record(first_slot - 1)
        .expect("map directory has its preceding count record");
    let map_file_count = marker
        .directory_len()
        .expect("map count record is negative");
    assert_eq!(map_file_count % 2, 0, "maps use bundle/companion pairs");
    let map_count = map_file_count / 2;

    let known_disc = match number {
        1 => KnownDisc::Disc1,
        2 => KnownDisc::Disc2,
        _ => panic!("only the two original discs are supported"),
    };
    let boot_sha256 = xem_disc::identify::hex(&known_disc.executable_sha256());
    let mut placeholders = 0usize;
    let mut short_headers = 0usize;
    let mut valid_bundles = 0usize;
    let mut rejected_context_maps = Vec::new();
    let mut strict_components = 0usize;
    let mut normalized_components = 0usize;
    let mut optional_tail_bytes = 0usize;
    let mut exact_noops = 0usize;
    let mut strict_reloads = 0usize;
    let mut zero_descriptor_headers = 0usize;
    let mut map0_c8 = None;
    let mut public_decode_outcomes = Vec::new();
    let mut public_draft_open_outcomes = Vec::new();

    for map in 0..map_count {
        let file = MAP_FIRST_FILE + map * 2;
        let slot = index.slot(4, file).expect("map bundle slot");
        let record = index.record(slot).expect("map bundle index record");
        if record.is_unused() || record.size <= 0 {
            placeholders += 1;
            let outcome = json!({
                "status":"not_attempted",
                "kind":"empty_index_slot",
                "reason":"raw index record has no positive map bundle"
            });
            public_decode_outcomes.push(json!({"map":map,"outcome":outcome.clone()}));
            public_draft_open_outcomes.push(json!({
                "map":map,
                "outcome":outcome,
                "close":{"status":"not_applicable"}
            }));
            continue;
        }
        let size = usize::try_from(record.size).expect("positive index size");
        assert!(size <= 32 << 20, "map bundle remains within reader budget");
        let bundle = read_data(track.as_mut(), record.lba, size).expect("read indexed map bytes");
        assert_eq!(bundle.len(), size, "read exact index length");
        if bundle.len() < 0x190 {
            short_headers += 1;
            let outcome = json!({
                "status":"not_attempted",
                "kind":"short_header_placeholder",
                "reason":"indexed map bytes are shorter than the complete field header"
            });
            public_decode_outcomes.push(json!({"map":map,"outcome":outcome.clone()}));
            public_draft_open_outcomes.push(json!({
                "map":map,
                "outcome":outcome,
                "close":{"status":"not_applicable"}
            }));
            continue;
        }
        valid_bundles += 1;
        let source_identity = SourceIdentity {
            disc: number,
            boot_sha256: boot_sha256.clone(),
            map,
            slot: u32::try_from(slot).expect("source slot fits u32"),
            lba: record.lba,
            indexed_size: u32::try_from(bundle.len()).expect("bundle size fits u32"),
            source_sha256: hash(&bundle),
        };
        let scene_studio = Studio::new();
        let public_input = json!({
            "bundle":bundle.clone(),
            "context":null,
            "source":source_identity.clone(),
            "module_sha256":null
        });
        let decoded_scene = command(
            &scene_studio,
            "field.scene.decode",
            json!({"input":public_input.clone(), "entrances":[]}),
        );
        public_decode_outcomes.push(json!({
            "map":map,
            "outcome":operation_outcome(&decoded_scene)
        }));
        let opened_scene = command(
            &scene_studio,
            "field.scene.draft",
            json!({"kind":"open", "input":public_input, "entrances":[]}),
        );
        let open_outcome = operation_outcome(&opened_scene);
        let close_outcome = if let Some(scene) = opened_scene
            .get("ok")
            .and_then(|value| value.get("scene"))
            .and_then(Value::as_u64)
        {
            operation_outcome(&command(
                &scene_studio,
                "field.scene.draft",
                json!({"kind":"close", "scene":scene}),
            ))
        } else {
            json!({"status":"not_applicable"})
        };
        public_draft_open_outcomes.push(json!({
            "map":map,
            "outcome":open_outcome,
            "close":close_outcome
        }));
        if number == 1 && map == 0 {
            assert_eq!(slot, 606);
            assert_eq!(record.lba, 120_698);
            assert_eq!(bundle.len(), 105_980);
            assert_eq!(
                hash(&bundle),
                "c7cdbefd834e1d0405482d95204c822c8f4ef35249eee62c54a3af09cfd52921"
            );
        }
        let source = match Source::load(Input {
            bundle: bundle.clone(),
            context: None,
            source: Some(source_identity),
            module_sha256: None,
        }) {
            Ok(source) => source,
            Err(error) if error.kind == "source_context_required" => {
                rejected_context_maps.push(map);
                continue;
            }
            Err(error) => panic!(
                "disc {number} map {map} rejected unexpectedly: {} ({})",
                error.kind, error.message
            ),
        };
        assert_eq!(source.components.len(), 9);
        let unchanged = source
            .repack(&source.header, &std::collections::BTreeMap::new())
            .unwrap_or_else(|error| panic!("disc {number} map {map} no-op failed: {}", error.kind));
        assert_eq!(
            hash(&unchanged),
            hash(&bundle),
            "disc {number} map {map} exact no-op"
        );
        exact_noops += 1;

        let zones = decode_zones(
            &source.components[8].bytes,
            source.components[8].logical_size,
        )
        .unwrap_or_else(|error| panic!("disc {number} map {map} zones: {}", error.kind));
        let _zone_count = zones.zones.len();
        assert!(source.components[5].bytes.len() >= source.components[5].logical_size);
        assert!(
            !source.components[5].needs_stream_completion,
            "disc {number} map {map} event output stays complete"
        );
        if number == 1 && map == 0 {
            let component = &source.components[8];
            let offset = source.header.offsets[8];
            assert_eq!(component.logical_size, 0);
            assert!(component.bytes.is_empty());
            assert!(component.needs_stream_completion);
            assert_eq!(source.input().bundle.len(), offset + 4);
            assert_eq!(
                u32::from_le_bytes(bundle[offset..offset + 4].try_into().unwrap()),
                0
            );
            map0_c8 = Some(json!({
                "slot":slot,"lba":record.lba,"source_sha256":hash(&bundle),
                "c8_offset":offset,"c8_logical_size":component.logical_size,
                "c8_needs_stream_completion":component.needs_stream_completion,
                "terminal_header_at_eof":offset + 4 == bundle.len()
            }));
        }

        let mut edited_header = source.header.clone();
        if let Some(first) = edited_header.descriptors.first_mut() {
            first.position[0] ^= 1;
        } else {
            zero_descriptor_headers += 1;
            edited_header
                .descriptors
                .push(xem_studio::scene::formats::Descriptor {
                    flags: 0,
                    rotation: [0; 3],
                    position: [0; 3],
                    model: 0,
                });
        }
        let candidate = source
            .repack(&edited_header, &std::collections::BTreeMap::new())
            .unwrap_or_else(|error| {
                panic!(
                    "disc {number} map {map} re-encode failed: {} ({})",
                    error.kind, error.message
                )
            });
        assert_eq!(
            hash(&source.input().bundle),
            hash(&bundle),
            "repack does not mutate source"
        );
        let decoded = Source::load(Input {
            bundle: candidate,
            context: None,
            source: None,
            module_sha256: None,
        })
        .unwrap_or_else(|error| {
            panic!(
                "disc {number} map {map} candidate did not strict-reload: {} ({})",
                error.kind, error.message
            )
        });
        assert_eq!(
            decoded.header.descriptors.len(),
            edited_header.descriptors.len()
        );
        for (component, (before, after)) in source
            .components
            .iter()
            .zip(&decoded.components)
            .enumerate()
        {
            assert!(
                !after.needs_stream_completion,
                "disc {number} map {map} c{component} reloaded strictly"
            );
            assert_eq!(
                after.optional_tail_bytes, 0,
                "disc {number} map {map} c{component} normalized"
            );
            if before.needs_stream_completion {
                assert!(after.bytes.len() >= before.bytes.len());
                assert_eq!(
                    hash(&after.bytes[..before.bytes.len()]),
                    hash(&before.bytes),
                    "disc {number} map {map} c{component} retains all known output"
                );
                assert!(after.bytes.len() - before.bytes.len() <= 16);
                assert!(
                    after.bytes[before.bytes.len()..]
                        .iter()
                        .all(|byte| *byte == 0)
                );
                normalized_components += 1;
            } else {
                assert_eq!(
                    hash(&after.bytes),
                    hash(&before.bytes),
                    "disc {number} map {map} c{component} full decoded output preserved"
                );
                strict_components += 1;
            }
        }
        strict_reloads += 1;
        optional_tail_bytes += source
            .components
            .iter()
            .map(|c| c.optional_tail_bytes)
            .sum::<usize>();
    }

    let expected_rejected = if number == 1 {
        vec![54, 84, 128, 140, 162, 168, 183, 453, 473]
    } else {
        Vec::new()
    };
    assert_eq!(
        rejected_context_maps, expected_rejected,
        "disc {number} C7 context-required maps match the font-independent audit"
    );
    assert_eq!(valid_bundles, if number == 1 { 730 } else { 205 });
    assert_eq!(short_headers, if number == 1 { 0 } else { 525 });
    assert_eq!(
        placeholders, 0,
        "all non-placeholder map records have a positive index length"
    );
    assert_eq!(exact_noops, valid_bundles - rejected_context_maps.len());
    assert_eq!(strict_reloads, exact_noops);
    if number == 1 {
        assert!(
            map0_c8.is_some(),
            "actual Map 0 empty terminal was exercised"
        );
    }
    json!({
        "disc":number,"map_records":map_count,"placeholder_index_records":placeholders,
        "short_header_placeholders":short_headers,"valid_bundles":valid_bundles,
        "exact_noop_bundles":exact_noops,"strict_reloaded_candidates":strict_reloads,
        "context_required_c7_maps":rejected_context_maps,
        "strict_components_preserved":strict_components,
        "incomplete_components_normalized":normalized_components,
        "omitted_optional_bytes":optional_tail_bytes,"zero_descriptor_headers":zero_descriptor_headers,
        "map0_empty_terminal":map0_c8,
        "field_scene_decode_outcome_counts":outcome_counts(&public_decode_outcomes, "outcome"),
        "field_scene_draft_open_outcome_counts":outcome_counts(&public_draft_open_outcomes, "outcome"),
        "field_scene_decode_per_map":public_decode_outcomes,
        "field_scene_draft_open_per_map":public_draft_open_outcomes
    })
}

fn target_map_scene_summary(bundle: Vec<u8>, identity: SourceIdentity) -> Value {
    let source = Source::load(Input {
        bundle: bundle.clone(),
        context: None,
        source: Some(identity),
        module_sha256: None,
    })
    .expect("target map has an offline-qualified source");
    let component_status = source
        .components
        .iter()
        .map(|component| {
            json!({
                "logical_size":component.logical_size,
                "output_size":component.bytes.len(),
                "needs_stream_completion":component.needs_stream_completion,
                "optional_tail_bytes":component.optional_tail_bytes
            })
        })
        .collect::<Vec<_>>();
    let studio = Studio::new();
    let input = json!({
        "bundle":bundle,
        "context":null,
        "source":source.input().source.clone(),
        "module_sha256":null
    });
    let decoded = command(
        &studio,
        "field.scene.decode",
        json!({"input":input.clone(),"entrances":[]}),
    );
    let decoded = ok(&decoded, "decode target map through public Scene API");
    let opened = command(
        &studio,
        "field.scene.draft",
        json!({"kind":"open","input":input,"entrances":[]}),
    );
    let opened = ok(&opened, "open target map through public Scene draft API");
    let scene = opened["scene"].as_u64().expect("Scene handle") as u32;
    let state = &opened["state"];
    let mut transformable_placements = Vec::new();
    let bindings = state["events"]["bindings"]
        .as_object()
        .expect("Scene event bindings map");
    for node in bindings.keys() {
        let response = command(
            &studio,
            "field.scene.draft",
            json!({"kind":"placement","scene":scene,"node":node}),
        );
        if response["ok"]["can_transform"] == true {
            let placement = &response["ok"];
            transformable_placements.push(json!({
                "node":placement["node"],
                "pc":placement["pc"],
                "handler":placement["handler"],
                "owners":placement["owners"],
                "position":placement["position"]
            }));
        }
    }
    let closed = command(
        &studio,
        "field.scene.draft",
        json!({"kind":"close","scene":scene}),
    );
    ok(&closed, "close target Scene handle");
    json!({
        "source_sha256":hash(&source.input().bundle),
        "source_components":component_status,
        "decoded":{
            "descriptor_count":decoded["header"]["descriptors"].as_array().map(Vec::len),
            "actor_count":decoded["actors"].as_array().map(Vec::len),
            "zone_count":decoded["zones"]["zones"].as_array().map(Vec::len),
            "collision_layer_count":decoded["collision"]["layers"].as_array().map(Vec::len),
            "event_instruction_count":decoded["events"]["instructions"].as_array().map(Vec::len)
        },
        "draft_actors":state["actors"],
        "transformable_placements":transformable_placements
    })
}

#[test]
#[ignore = "requires explicit original Disc 1; writes only structural metadata and hashes"]
fn original_disc1_map0_and_map145_scene_editability_preflight() {
    let disc_path = PathBuf::from(
        std::env::var_os("XEM_STUDIO_OFFLINE_AUDIT_DISC1")
            .expect("explicit original Disc 1 raw image path is required"),
    );
    assert!(disc_path.is_file());
    let output = fresh_output();
    let mut track = open_path(&disc_path).expect("open explicitly supplied original Disc 1");
    let index = Index::from_disc(track.as_mut()).expect("read original Disc 1 raw index");
    assert_eq!(index.disc_number(), Some(1));
    let boot_sha256 = xem_disc::identify::hex(&KnownDisc::Disc1.executable_sha256());
    let mut maps = Vec::new();
    for map in [0u32, 145] {
        let slot = index.slot(4, 0xB8 + map * 2).expect("map bundle slot");
        let record = index.record(slot).expect("map bundle record");
        assert!(record.size >= 0x190, "selected map is a complete bundle");
        let size = usize::try_from(record.size).expect("positive original file size");
        let bundle = read_data(track.as_mut(), record.lba, size).expect("read indexed map bytes");
        let identity = SourceIdentity {
            disc: 1,
            boot_sha256: boot_sha256.clone(),
            map,
            slot: u32::try_from(slot).expect("slot fits source identity"),
            lba: record.lba,
            indexed_size: u32::try_from(bundle.len()).expect("bundle size fits source identity"),
            source_sha256: hash(&bundle),
        };
        maps.push(
            json!({"map":map,"slot":slot,"lba":record.lba,"bundle_size":bundle.len(),
            "scene":target_map_scene_summary(bundle, identity)}),
        );
    }
    write_summary(
        &output,
        &json!({
            "kind":"original_disc1_offline_scene_target_preflight",
            "maps":maps,
            "raw_component_bytes_written_or_printed":false
        }),
    );
}

#[test]
#[ignore = "requires explicit original disc and measured runtime source context"]
fn original_disc_scene_formats_context_and_static_transform_preflight() {
    let disc_path = PathBuf::from(
        std::env::var_os("XEM_STUDIO_M7_DISC1")
            .expect("explicit original Disc 1 BIN path is required"),
    );
    let context_path = PathBuf::from(
        std::env::var_os("XEM_STUDIO_M7_CONTEXT")
            .expect("explicit M6 measured source-context path is required"),
    );
    let qualified_project_path = PathBuf::from(
        std::env::var_os("XEM_STUDIO_M7_QUALIFIED_PROJECT")
            .expect("explicit historical qualified Scene project path is required"),
    );
    assert!(
        disc_path.is_file() && context_path.is_file() && qualified_project_path.is_file(),
        "original disc, measured context, and historical project exist at explicit paths"
    );
    let output = fresh_output();

    let context_bytes = fs::read(&context_path).expect("read measured source-context evidence");
    assert_eq!(
        hash(&context_bytes),
        "aa604c7853eef79306df0c42c75c1aeba7720fa90115a470393a3e27d827d66a",
        "use the reviewed measured context capture"
    );
    let context: SourceContext =
        serde_json::from_slice(&context_bytes).expect("strict source context");
    assert_eq!(context.module_sha256, MODULE_SHA256);

    let studio = Studio::new();
    let opened = ok(
        &command(
            &studio,
            "disc.open",
            json!({"kind":"native","path":disc_path.display().to_string()}),
        ),
        "open original Disc 1",
    );
    let session = opened["session"].as_u64().expect("session handle") as u32;
    let mut state = opened.clone();
    for _ in 0..20_000 {
        if state["state"] == "ready" {
            break;
        }
        state = ok(
            &command(&studio, "disc.poll", json!({"session":session})),
            "poll original Disc 1",
        )
        .clone();
    }
    assert_eq!(state["state"], "ready", "original disc index becomes ready");
    assert_eq!(state["identity"]["disc"], 1);
    assert_eq!(state["identity"]["test_fixture"], false);

    let mut maps = None;
    for _ in 0..20_000 {
        let reply = command(&studio, "disc.maps", json!({"session":session}));
        if reply["ok"]["state"] == "ready" {
            maps = Some(reply);
            break;
        }
        assert_eq!(
            reply["ok"]["state"], "pending",
            "map index progresses through public pending/ready state"
        );
    }
    let maps = ok(
        &maps.expect("original map index reaches ready"),
        "read original map index",
    );
    assert_eq!(maps["state"], "ready");
    let map23 = maps["maps"]
        .as_array()
        .unwrap()
        .iter()
        .find(|m| m["map"] == 23)
        .expect("identified original disc has map 23");
    assert_eq!(map23["placeholder"], false);
    let mut map = None;
    for _ in 0..20_000 {
        let reply = command(
            &studio,
            "field.map_bundle",
            json!({"session":session,"map":23}),
        );
        if reply["ok"]["state"] == "ready" {
            map = Some(reply);
            break;
        }
        assert_eq!(
            reply["ok"]["state"], "pending",
            "map bundle progresses through public pending/ready state"
        );
    }
    let map = ok(
        &map.expect("map bundle read reaches ready"),
        "read original map 23 bundle",
    );
    let original_bundle = bytes(&map["bytes"]);
    let identity = map["source"].clone();
    assert_eq!(identity["disc"], 1);
    assert_eq!(identity["map"], 23);
    assert_eq!(identity["slot"], 652);
    assert_eq!(identity["lba"], 124_528);
    assert_eq!(identity["indexed_size"], 110_052);
    assert_eq!(identity["source_sha256"], ORIGINAL_BUNDLE_SHA256);
    let source_identity = serde_json::from_value(identity.clone()).expect("identity schema");
    assert_eq!(context.identity, source_identity);
    let measured_guard = context
        .guard(&original_bundle, &context.identity, MODULE_SHA256)
        .expect("source context verifies all nine components and exact tail bytes");

    let measured_input = json!({"bundle":original_bundle,"context":context,"source":identity,"module_sha256":MODULE_SHA256});
    let offline_input =
        json!({"bundle":original_bundle,"context":null,"source":identity,"module_sha256":null});
    let offline = ok(
        &command(
            &studio,
            "field.scene.decode",
            json!({"input":offline_input.clone(),"entrances":[]}),
        ),
        "decode qualified Map 23 without measured runtime context",
    );
    assert_eq!(offline["source"]["source_sha256"], ORIGINAL_BUNDLE_SHA256);
    assert_eq!(offline["context_module_sha256"], Value::Null);
    let offline_components = offline["components"]
        .as_array()
        .expect("offline component summaries");
    assert_eq!(offline_components.len(), 9);

    let decoded = ok(
        &command(
            &studio,
            "field.scene.decode",
            json!({"input":measured_input,"entrances":[]}),
        ),
        "decode original scene with measured source context",
    );
    assert_eq!(decoded["source"]["source_sha256"], ORIGINAL_BUNDLE_SHA256);
    assert_eq!(decoded["context_module_sha256"], MODULE_SHA256);
    let components = decoded["components"]
        .as_array()
        .expect("nine component summaries");
    assert_eq!(components.len(), 9);
    assert_eq!(
        context.components.len(),
        9,
        "measured guard covers every original component"
    );
    let component_sha256 = context
        .components
        .iter()
        .map(|fact| fact.output_sha256.clone())
        .collect::<Vec<_>>();
    let component_logical_sizes = context
        .components
        .iter()
        .map(|fact| fact.logical_size)
        .collect::<Vec<_>>();
    for fact in &context.components {
        let output = &components[fact.index];
        assert_eq!(
            output["output_size"].as_u64(),
            Some(fact.output_size as u64)
        );
        assert_eq!(output["output_sha256"], fact.output_sha256);
    }
    for fact in context.components.iter().filter(|fact| fact.index < 8) {
        let output = &offline_components[fact.index];
        assert_eq!(
            output["output_size"].as_u64(),
            Some(fact.output_size as u64)
        );
        assert_eq!(output["output_sha256"], fact.output_sha256);
    }
    let c8 = &context.components[8];
    assert_eq!(c8.index, 8);
    assert_eq!(c8.logical_size, 72);
    assert_eq!(c8.output_size, 78);
    assert_eq!(offline_components[8]["logical_size"], 72);
    assert_eq!(offline_components[8]["output_size"], 72);
    assert_eq!(offline_components[8]["optional_tail_bytes"], 6);
    assert!(
        offline["diagnostics"]
            .as_array()
            .expect("scene diagnostics")
            .iter()
            .any(|message| message.as_str().is_some_and(|message| {
                message.contains("6 advertised tail bytes")
                    && message.contains("not measured runtime context")
            }))
    );
    assert!(
        components
            .iter()
            .all(|component| component.get("optional_tail_bytes").is_none()),
        "a measured complete stream has no optional tail field"
    );
    let mut measured_bundle = original_bundle.clone();
    measured_bundle.extend_from_slice(&measured_guard);
    let c8_output =
        packed::decode_block(&measured_bundle[c8.source_offset..], c8.logical_size + 16)
            .expect("measured context decodes complete component 8");
    assert_eq!(c8_output.data.len(), 78);
    assert_eq!(
        offline_components[8]["output_sha256"],
        hash(&c8_output.data[..c8.logical_size]),
        "offline c8 keeps the known logical prefix only"
    );

    let opened_scene = ok(
        &command(
            &studio,
            "field.scene.draft",
            json!({"kind":"open","input":offline_input,"entrances":[]}),
        ),
        "open qualified original Scene offline",
    );
    let scene = opened_scene["scene"].as_u64().expect("scene handle") as u32;
    let initial = command(
        &studio,
        "field.scene.draft",
        json!({"kind":"compile","scene":scene}),
    );
    let initial = ok(&initial, "compile unedited original scene");
    let compiled_original = bytes(&initial["bundle"]);
    assert!(
        compiled_original == original_bundle,
        "offline no-edit compile preserves indexed bundle bytes (source {}, compiled {})",
        hash(&original_bundle),
        hash(&compiled_original)
    );

    let measured_scene = ok(
        &command(
            &studio,
            "field.scene.draft",
            json!({"kind":"open","input":measured_input.clone(),"entrances":[]}),
        ),
        "import the compatible measured-context Scene document",
    );
    let measured_scene_id = measured_scene["scene"]
        .as_u64()
        .expect("measured scene handle") as u32;
    let measured_compile = ok(
        &command(
            &studio,
            "field.scene.draft",
            json!({"kind":"compile","scene":measured_scene_id}),
        ),
        "compile measured-context original scene",
    );
    let measured_original = bytes(&measured_compile["bundle"]);
    assert!(
        measured_original == original_bundle,
        "measured-context no-edit compile preserves the exact indexed source ({})",
        hash(&measured_original)
    );
    let measured_document = ok(
        &command(
            &studio,
            "field.scene.draft",
            json!({"kind":"export","scene":measured_scene_id}),
        ),
        "export the qualified measured-context Scene document",
    );
    let measured_import = ok(
        &command(
            &studio,
            "field.scene.draft",
            json!({"kind":"import","document":measured_document}),
        ),
        "import the qualified measured-context Scene document",
    );
    let measured_import_id = measured_import["scene"]
        .as_u64()
        .expect("imported measured Scene handle") as u32;
    let measured_import_compile = ok(
        &command(
            &studio,
            "field.scene.draft",
            json!({"kind":"compile","scene":measured_import_id}),
        ),
        "compile the imported measured-context Scene before editing",
    );
    let measured_import_bundle = bytes(&measured_import_compile["bundle"]);
    assert!(
        measured_import_bundle == original_bundle,
        "qualified context document import retains the original compiled bundle ({})",
        hash(&measured_import_bundle)
    );
    ok(
        &command(
            &studio,
            "field.scene.draft",
            json!({"kind":"close","scene":measured_scene_id}),
        ),
        "close the first measured-context Scene owner",
    );

    // Retain the exact M7 saved-context project from the previously accepted
    // Stage B workflow. It is read-only historical input from another checkout;
    // the test never copies or writes that artifact.
    let qualified_project_bytes =
        fs::read(&qualified_project_path).expect("read historical qualified Scene project");
    assert_eq!(
        hash(&qualified_project_bytes),
        "3cffb561e775730c7152a83d524729e02bdbcaeff13a57007763752a37581f9a",
        "use the exact previously accepted saved Scene project"
    );
    let qualified_project: Value =
        serde_json::from_slice(&qualified_project_bytes).expect("historical saved project is JSON");
    assert_eq!(qualified_project["schema_version"], 2);
    assert_eq!(qualified_project["disc"], 1);
    assert_eq!(qualified_project["map"], 23);
    let qualified_document = &qualified_project["document"];
    assert_eq!(qualified_document["schema_version"], 4);
    assert_eq!(qualified_document["input"]["source"], identity);
    assert_eq!(
        qualified_document["input"]["context"]["module_sha256"],
        MODULE_SHA256
    );
    assert_eq!(
        qualified_document["actions"].as_array().map(Vec::len),
        Some(1)
    );
    assert_eq!(qualified_document["cursor"], 1);
    assert_eq!(qualified_document["actions"][0]["kind"], "set_placement");
    assert_eq!(
        qualified_document["actions"][0]["change"]["node"],
        "pc:1076"
    );
    assert_eq!(
        qualified_document["actions"][0]["change"]["position"][0],
        61
    );
    let qualified_input_bundle = bytes(&qualified_document["input"]["bundle"]);
    assert!(
        qualified_input_bundle == original_bundle,
        "historical Scene document has the exact original bundle input ({})",
        hash(&qualified_input_bundle)
    );
    let qualified_document_hash =
        hash(&serde_json::to_vec(qualified_document).expect("serialize historical document"));
    let qualified_import = ok(
        &command(
            &studio,
            "field.scene.draft",
            json!({"kind":"import","document":qualified_document}),
        ),
        "import the prior saved qualified-context Scene project",
    );
    let qualified_scene = qualified_import["scene"]
        .as_u64()
        .expect("qualified Scene handle") as u32;
    let qualified_export = ok(
        &command(
            &studio,
            "field.scene.draft",
            json!({"kind":"export","scene":qualified_scene}),
        ),
        "export the imported historical Scene document",
    );
    let qualified_export_hash =
        hash(&serde_json::to_vec(&qualified_export).expect("serialize re-exported document"));
    assert!(
        qualified_export == *qualified_document,
        "saved qualified-context document import/export preserves exact authored history ({qualified_document_hash} vs {qualified_export_hash})"
    );
    let qualified_initial = ok(
        &command(
            &studio,
            "field.scene.draft",
            json!({"kind":"compile","scene":qualified_scene}),
        ),
        "compile the imported historical project",
    );
    let qualified_initial_bundle = bytes(&qualified_initial["bundle"]);
    assert_eq!(
        hash(&qualified_initial_bundle),
        "2a50712b42368305b3365e3dff5751ebbb7abc07cc93bcce5155e0b2a91ca1f9",
        "historical imported project reproduces its previously accepted compiled candidate"
    );
    let qualified_state = ok(
        &command(
            &studio,
            "field.scene.draft",
            json!({"kind":"snapshot","scene":qualified_scene}),
        ),
        "read the imported project revision before a normal edit",
    );
    let qualified_revision = qualified_state["state"]["revision"]
        .as_u64()
        .expect("imported revision");
    let restored = ok(
        &command(
            &studio,
            "field.scene.draft",
            json!({
                "kind":"edit","scene":qualified_scene,"revision":qualified_revision,
                "action":{"kind":"set_placement","change":{"actor":"descriptor:3","event":0,
                    "layer":null,"node":"pc:1076","position":[29,null,1270]}}
            }),
        ),
        "edit the existing placement back to its original source operand",
    );
    assert_eq!(restored["state"]["revision"], qualified_revision + 1);
    let qualified_restored = ok(
        &command(
            &studio,
            "field.scene.draft",
            json!({"kind":"compile","scene":qualified_scene}),
        ),
        "compile the restored historical placement",
    );
    let qualified_restored_bundle = bytes(&qualified_restored["bundle"]);
    assert!(
        qualified_restored_bundle == original_bundle,
        "editing the saved placement back to source data restores original bytes ({})",
        hash(&qualified_restored_bundle)
    );
    ok(
        &command(
            &studio,
            "field.scene.draft",
            json!({"kind":"close","scene":qualified_scene}),
        ),
        "close the imported historical Scene project",
    );

    let descriptors = decoded["header"]["descriptors"]
        .as_array()
        .expect("source descriptors");
    let actor_count = decoded["actors"]
        .as_array()
        .expect("source actor rows")
        .len();
    let static_index = descriptors.iter().enumerate().position(|(index, d)| {
        index >= actor_count && d["flags"].as_u64().unwrap_or(0) & 0x40 == 0
    });
    let Some(index) = static_index else {
        write_summary(
            &output,
            &json!({
                "status":"no_static_descriptor",
                "disc":1,"map":23,"slot":652,"indexed_bytes":original_bundle.len(),
                "bundle_sha256":ORIGINAL_BUNDLE_SHA256,"context_module_sha256":MODULE_SHA256,
                "component_count":components.len(),"component_logical_sizes":component_logical_sizes,
                "component_output_sha256":component_sha256,"no_edit_byte_exact":true
            }),
        );
        return;
    };
    let descriptor_id = format!("descriptor:{index}");
    let d = &descriptors[index];
    let mut position = [0_u16; 3];
    let mut rotation = [0_u16; 3];
    for axis in 0..3 {
        position[axis] = u16::try_from(d["position"][axis].as_u64().expect("position word"))
            .expect("u16 position");
        rotation[axis] = u16::try_from(d["rotation"][axis].as_u64().expect("rotation word"))
            .expect("u16 rotation");
    }
    position[0] = if position[0] == u16::MAX {
        position[0] - 1
    } else {
        position[0] + 1
    };
    let edited = ok(
        &command(
            &studio,
            "field.scene.draft",
            json!({
                "kind":"edit","scene":scene,"revision":0,
                "action":{"kind":"set_descriptor","id":descriptor_id,"position":position,"rotation":rotation}
            }),
        ),
        "edit one existing static descriptor",
    );
    assert_eq!(edited["state"]["revision"], 1);
    let compiled = ok(
        &command(
            &studio,
            "field.scene.draft",
            json!({"kind":"compile","scene":scene}),
        ),
        "compile static descriptor transform",
    );
    let transformed_bundle = bytes(&compiled["bundle"]);
    assert!(
        transformed_bundle != original_bundle,
        "the selected descriptor transform changes the scene header (candidate {})",
        hash(&transformed_bundle)
    );
    assert_eq!(
        compiled["source_sha256"], ORIGINAL_BUNDLE_SHA256,
        "compiled candidate remains bound to the original source identity"
    );
    let transformed = ok(
        &command(
            &studio,
            "field.scene.decode",
            json!({
                "input":{"bundle":transformed_bundle,"context":null,"source":null,"module_sha256":null},"entrances":[]
            }),
        ),
        "decode materialized candidate outputs",
    );
    let transformed_components = transformed["components"]
        .as_array()
        .expect("candidate component summaries");
    assert_eq!(transformed_components.len(), 9);
    for (before, after) in offline_components.iter().zip(transformed_components) {
        assert_eq!(
            before["logical_size"], after["logical_size"],
            "descriptor edit does not alter component boundaries"
        );
        assert_eq!(
            before["output_size"], after["output_size"],
            "the offline source outputs are materialized unchanged"
        );
        assert_eq!(
            before["output_sha256"], after["output_sha256"],
            "the offline source outputs are preserved"
        );
    }
    assert_eq!(
        transformed_components[8]["optional_tail_bytes"],
        Value::Null,
        "an edited c8 is self-contained and no longer reports a discarded source tail"
    );
    assert_eq!(
        transformed["header"]["descriptors"][index]["position"][0],
        position[0]
    );
    let changed_position_byte = 0x190 + index * 16 + 8;
    let c8_offset = u32::from_le_bytes(
        transformed_bundle[0x150..0x154]
            .try_into()
            .expect("component 8 offset word"),
    ) as usize;
    assert!(
        c8_offset >= original_bundle.len(),
        "edited c8 is redirected to its new self-contained stream"
    );
    let c8_packed_size = u32::from_le_bytes(
        transformed_bundle[c8_offset..c8_offset + 4]
            .try_into()
            .expect("complete c8 output-size word"),
    );
    assert_eq!(c8_packed_size, 72);
    for (offset, (&before, &after)) in original_bundle.iter().zip(&transformed_bundle).enumerate() {
        let is_edited_position =
            (changed_position_byte..changed_position_byte + 2).contains(&offset);
        let is_relocated_c8_pointer = (0x150..0x154).contains(&offset);
        if !is_edited_position && !is_relocated_c8_pointer {
            assert_eq!(
                before, after,
                "original source byte at indexed offset {offset} is preserved"
            );
        }
    }

    let measured_edited = ok(
        &command(
            &studio,
            "field.scene.draft",
            json!({
                "kind":"edit","scene":measured_import_id,"revision":0,
                "action":{"kind":"set_descriptor","id":descriptor_id,
                    "position":position,"rotation":rotation}
            }),
        ),
        "edit the imported qualified-context Scene document",
    );
    assert_eq!(measured_edited["state"]["revision"], 1);
    let measured_edited_compile = ok(
        &command(
            &studio,
            "field.scene.draft",
            json!({"kind":"compile","scene":measured_import_id}),
        ),
        "compile the edited imported qualified-context Scene",
    );
    let measured_edited_bundle = bytes(&measured_edited_compile["bundle"]);
    let measured_edited_hash = hash(&measured_edited_bundle);
    assert_eq!(
        measured_edited_hash, "3f1120edd015a016b2c7d414bbb176f4f61aa0197089020b710a49769da1f89d",
        "measured-context import/edit preserves the previously qualified candidate identity"
    );
    let measured_edited_document = ok(
        &command(
            &studio,
            "field.scene.draft",
            json!({"kind":"export","scene":measured_import_id}),
        ),
        "export the edited qualified-context Scene",
    );
    let measured_reimport = ok(
        &command(
            &studio,
            "field.scene.draft",
            json!({"kind":"import","document":measured_edited_document}),
        ),
        "re-import the edited qualified-context Scene project",
    );
    let measured_reimport_id = measured_reimport["scene"]
        .as_u64()
        .expect("re-imported measured Scene handle") as u32;
    let measured_reimport_compile = ok(
        &command(
            &studio,
            "field.scene.draft",
            json!({"kind":"compile","scene":measured_reimport_id}),
        ),
        "compile the re-imported qualified-context Scene project",
    );
    let measured_reimport_bundle = bytes(&measured_reimport_compile["bundle"]);
    assert!(
        measured_reimport_bundle == measured_edited_bundle,
        "qualified context project import reproduces the edited candidate ({})",
        hash(&measured_reimport_bundle)
    );

    let undo = ok(
        &command(
            &studio,
            "field.scene.draft",
            json!({"kind":"undo","scene":scene,"revision":1}),
        ),
        "undo the header-only descriptor transform",
    );
    assert_eq!(undo["state"]["can_redo"], true);
    let undone = ok(
        &command(
            &studio,
            "field.scene.draft",
            json!({"kind":"compile","scene":scene}),
        ),
        "compile the exact indexed source after undo",
    );
    let undone_bundle = bytes(&undone["bundle"]);
    assert!(
        undone_bundle == original_bundle,
        "undo restores the exact indexed source ({})",
        hash(&undone_bundle)
    );
    let redo = ok(
        &command(
            &studio,
            "field.scene.draft",
            json!({"kind":"redo","scene":scene,"revision":2}),
        ),
        "redo the header-only descriptor transform",
    );
    assert_eq!(redo["state"]["revision"], 3);
    let redone = ok(
        &command(
            &studio,
            "field.scene.draft",
            json!({"kind":"compile","scene":scene}),
        ),
        "compile the deterministic re-applied transform",
    );
    let redone_bundle = bytes(&redone["bundle"]);
    assert!(
        redone_bundle == transformed_bundle,
        "redo reproduces the same self-contained candidate ({})",
        hash(&redone_bundle)
    );

    let document = ok(
        &command(
            &studio,
            "field.scene.draft",
            json!({"kind":"export","scene":scene}),
        ),
        "export source-owned document",
    );
    let imported = ok(
        &command(
            &studio,
            "field.scene.draft",
            json!({"kind":"import","document":document}),
        ),
        "re-import source-owned document",
    );
    let imported_scene = imported["scene"].as_u64().expect("imported scene handle") as u32;
    let replay = ok(
        &command(
            &studio,
            "field.scene.draft",
            json!({"kind":"compile","scene":imported_scene}),
        ),
        "compile imported document",
    );
    let replay_bundle = bytes(&replay["bundle"]);
    assert!(
        replay_bundle == transformed_bundle,
        "export/import reproduces the same complete candidate (expected {}, actual {})",
        hash(&transformed_bundle),
        hash(&replay_bundle)
    );

    write_summary(
        &output,
        &json!({
            "status":"passed",
            "disc":1,"map":23,"slot":652,"lba":124528,"indexed_bytes":original_bundle.len(),
            "bundle_sha256":ORIGINAL_BUNDLE_SHA256,"context_sha256":hash(&context_bytes),
            "context_module_sha256":MODULE_SHA256,"component_count":components.len(),
            "component_logical_sizes":component_logical_sizes,
            "component_output_sha256":component_sha256,
            "descriptor_count":descriptors.len(),"transformed_descriptor_index":index,
            "candidate_sha256":hash(&transformed_bundle),
            "offline_no_edit_byte_exact":true,"offline_static_transform_changed_bundle":true,
            "offline_full_components_0_through_7_preserved":true,
            "offline_c8_logical_prefix_preserved":true,"offline_c8_optional_tail_bytes_not_preserved":6,
            "measured_context_candidate_sha256":measured_edited_hash,
            "historical_project_sha256":hash(&qualified_project_bytes),
            "historical_project_import_candidate_sha256":hash(&qualified_initial_bundle),
            "historical_project_restored_source_sha256":hash(&qualified_restored_bundle),
            "qualified_project_import_edit_compile_equal":true,
            "offline_export_import_bundle_equal":true
        }),
    );
}

#[test]
#[ignore = "requires explicit original Disc 1 and Disc 2 images; writes only counts/hashes"]
fn both_disc_offline_component_load_noop_and_strict_repack_conformance() {
    let disc1 = PathBuf::from(
        std::env::var_os("XEM_STUDIO_OFFLINE_AUDIT_DISC1")
            .expect("explicit original Disc 1 raw image path is required"),
    );
    let disc2 = PathBuf::from(
        std::env::var_os("XEM_STUDIO_OFFLINE_AUDIT_DISC2")
            .expect("explicit original Disc 2 raw image path is required"),
    );
    assert!(
        disc1.is_file() && disc2.is_file(),
        "both explicit original images exist"
    );
    let output = offline_audit_output();
    let summary = json!({
        "kind":"both_disc_offline_component_conformance",
        "disc1":audit_indexed_disc(1, &disc1),
        "disc2":audit_indexed_disc(2, &disc2),
        "raw_component_bytes_written":false
    });
    write_summary(&output, &summary);
    eprintln!("both-disc offline component conformance summary written under ignored output");
}

#[test]
#[ignore = "requires both explicit original Disc images; writes only counts and hashes"]
fn both_disc_nonempty_full_output_completions_end_on_complete_flag_groups() {
    const MAP_FIRST_FILE: u32 = 0xB8;
    let paths = [
        (
            1u8,
            PathBuf::from(
                std::env::var_os("XEM_STUDIO_OFFLINE_AUDIT_DISC1")
                    .expect("explicit original Disc 1 raw image path is required"),
            ),
        ),
        (
            2u8,
            PathBuf::from(
                std::env::var_os("XEM_STUDIO_OFFLINE_AUDIT_DISC2")
                    .expect("explicit original Disc 2 raw image path is required"),
            ),
        ),
    ];
    let output = offline_audit_output();
    let mut per_disc = Vec::new();
    let mut completions = Vec::new();
    let mut admitted_total = 0usize;
    let mut raw_full_outputs_total = 0usize;
    let mut malformed_group_total = 0usize;
    let mut context_required_total = 0usize;
    for (number, path) in paths {
        assert!(path.is_file(), "explicit Disc {number} input exists");
        let mut track = open_path(&path).expect("open original raw disc");
        let index = Index::from_disc(track.as_mut()).expect("read raw on-disc file index");
        assert_eq!(index.disc_number(), Some(u16::from(number)));
        let marker_slot = index
            .slot(4, MAP_FIRST_FILE)
            .expect("map directory starts in the index");
        let map_count = index
            .record(marker_slot - 1)
            .and_then(|record| record.directory_len())
            .expect("map count record is negative")
            / 2;
        let known_disc = match number {
            1 => KnownDisc::Disc1,
            2 => KnownDisc::Disc2,
            _ => unreachable!(),
        };
        let boot_sha256 = xem_disc::identify::hex(&known_disc.executable_sha256());
        let mut disc_completions = Vec::new();
        let mut context_required = 0usize;
        let mut disc_raw_full_outputs = 0usize;
        let mut disc_malformed_groups = 0usize;
        let mut placeholder_maps = 0usize;
        let mut short_header_maps = 0usize;
        let mut valid_bundle_count = 0usize;
        let mut source_admitted_map_count = 0usize;
        for map in 0..map_count {
            let slot = index.slot(4, MAP_FIRST_FILE + map * 2).expect("map slot");
            let record = index.record(slot).expect("map bundle record");
            if record.is_unused() || record.size <= 0 {
                placeholder_maps += 1;
                continue;
            }
            if record.size < 0x190 {
                short_header_maps += 1;
                continue;
            }
            let size = usize::try_from(record.size).expect("positive bundle size");
            assert!(size <= 32 << 20, "map stays inside the source-reader bound");
            let bytes =
                read_data(track.as_mut(), record.lba, size).expect("read indexed map bundle");
            assert_eq!(bytes.len(), size, "read the complete indexed bundle");
            valid_bundle_count += 1;
            let header = match xem_studio::scene::formats::decode_header(&bytes) {
                Ok(header) => header,
                Err(_) => continue,
            };
            // Scan every component before Source::load, which can stop at an
            // earlier context-required component. This keeps the denominator
            // independent of public-scene admission outcomes.
            for component_index in 0..9 {
                let logical = header.logical_sizes[component_index];
                let start = header.offsets[component_index];
                let Some(stream) = bytes.get(start..) else {
                    continue;
                };
                let limit = match logical.checked_add(16) {
                    Some(limit) => limit,
                    None => continue,
                };
                if let Err(packed::PackedError::Truncated { position, output }) =
                    packed::decode_block(stream, limit)
                {
                    let declared = stream
                        .get(..4)
                        .and_then(|raw| raw.try_into().ok())
                        .map(u32::from_le_bytes)
                        .map(|value| value as usize);
                    if let Some(declared) =
                        declared.filter(|declared| *declared > 0 && output.len() == *declared)
                    {
                        if packed_prefix_ends_after_complete_flag_group(stream, position) {
                            disc_raw_full_outputs += 1;
                            raw_full_outputs_total += 1;
                            completions.push(json!({
                                "disc":number,
                                "map":map,
                                "component":component_index,
                                "logical_size":logical,
                                "declared_output_size":declared,
                                "output_sha256":hash(&output)
                            }));
                        } else {
                            disc_malformed_groups += 1;
                            malformed_group_total += 1;
                        }
                    }
                }
            }
            let identity = SourceIdentity {
                disc: number,
                boot_sha256: boot_sha256.clone(),
                map,
                slot: u32::try_from(slot).expect("slot fits source identity"),
                lba: record.lba,
                indexed_size: u32::try_from(size).expect("bundle size fits source identity"),
                source_sha256: hash(&bytes),
            };
            let source = match Source::load(Input {
                bundle: bytes,
                context: None,
                source: Some(identity),
                module_sha256: None,
            }) {
                Ok(source) => source,
                Err(error) if error.kind == "source_context_required" => {
                    context_required += 1;
                    context_required_total += 1;
                    continue;
                }
                Err(error) => panic!(
                    "Disc {number} map {map} Source::load failed: {} ({})",
                    error.kind, error.message
                ),
            };
            source_admitted_map_count += 1;
            for (component_index, component) in source.components.iter().enumerate() {
                if !component.needs_stream_completion
                    || component.optional_tail_bytes != 0
                    || component.bytes.is_empty()
                {
                    continue;
                }
                assert_eq!(
                    component.bytes.len(),
                    xem_disc::field_context::word(&source.input().bundle, component.source_offset)
                        .expect("component packed header retains its advertised output size")
                        as usize,
                    "Disc {number} map {map} C{component_index} retains every advertised output byte"
                );
                admitted_total += 1;
                disc_completions.push(json!({
                    "map":map,
                    "component":component_index,
                    "output_size":component.bytes.len(),
                    "output_sha256":hash(&component.bytes)
                }));
            }
        }
        per_disc.push(json!({
            "disc":number,
            "directory_map_count":map_count,
            "placeholder_map_records":placeholder_maps,
            "short_header_map_records":short_header_maps,
            "valid_bundle_count":valid_bundle_count,
            "source_admitted_map_count":source_admitted_map_count,
            "source_context_required_maps":context_required,
            "source_admitted_full_output_completion_count":disc_completions.len(),
            "raw_full_output_complete_flag_group_count":disc_raw_full_outputs,
            "raw_full_output_incomplete_flag_group_count":disc_malformed_groups
        }));
    }
    write_summary(
        &output,
        &json!({
            "kind":"both_disc_nonempty_full_output_flag_group_audit",
            "source_admitted_full_output_completion_count":admitted_total,
            "raw_full_output_complete_flag_group_count":raw_full_outputs_total,
            "raw_full_output_incomplete_flag_group_count":malformed_group_total,
            "source_context_required_map_count":context_required_total,
            "per_disc":per_disc,
            "components":completions,
            "raw_component_bytes_written":false
        }),
    );
    assert_eq!(
        raw_full_outputs_total, 160,
        "independent all-map scan retains the prior 160 full-output truncation candidates"
    );
    assert_eq!(
        malformed_group_total, 0,
        "no audited full-output truncation ends inside a flag group"
    );
    assert!(
        admitted_total <= raw_full_outputs_total,
        "Source admission count cannot exceed the raw full-output candidate count"
    );
}

#[test]
#[ignore = "requires explicit saved Map 0/145 projects produced by the current M7 UI"]
fn saved_offline_map0_and_map145_projects_replay_with_exact_candidate_hashes() {
    let artifacts = PathBuf::from(
        std::env::var_os("XEM_STUDIO_M7_OFFLINE_REPLAY_DIR")
            .expect("explicit checkpoint13 offline Scene project directory is required"),
    );
    assert!(artifacts.is_dir(), "saved Scene artifact directory exists");
    let output = fresh_output();
    let studio = Studio::new();
    let mut maps = Vec::new();
    for map in [0u16, 145u16] {
        let project_bytes = fs::read(artifacts.join(format!("offline-map{map}-scene.json")))
            .expect("read UI-saved Scene project");
        let project_sha256 = hash(&project_bytes);
        let summary_bytes = fs::read(artifacts.join(format!("offline-map{map}-summary.json")))
            .expect("read UI package receipt");
        let summary: Value =
            serde_json::from_slice(&summary_bytes).expect("package receipt is JSON");
        assert_eq!(summary["saved_project_sha256"], project_sha256);
        let project: Value =
            serde_json::from_slice(&project_bytes).expect("saved Scene project is JSON");
        assert_eq!(project["map"], map);
        let document = &project["document"];
        assert_eq!(document["schema_version"], 4);
        assert!(document["input"]["context"].is_null());
        assert_eq!(document["actions"].as_array().map(Vec::len), Some(1));
        assert_eq!(document["cursor"], 1);
        let source_sha256 = document["input"]["source"]["source_sha256"]
            .as_str()
            .expect("saved project retains source identity");
        assert_eq!(source_sha256, summary["original_source_sha256"]);
        let expected_bundle_sha256 = summary["edited_compiled_bundle_sha256"]
            .as_str()
            .expect("UI receipt records the edited bundle hash");

        let imported = ok(
            &command(
                &studio,
                "field.scene.draft",
                json!({"kind":"import","document":document}),
            ),
            "import the UI-saved offline Scene project",
        );
        let scene = imported["scene"].as_u64().expect("imported scene handle") as u32;
        let compiled = ok(
            &command(
                &studio,
                "field.scene.draft",
                json!({"kind":"compile","scene":scene}),
            ),
            "compile the imported offline Scene project",
        );
        let bundle = bytes(&compiled["bundle"]);
        let observed_bundle_sha256 = hash(&bundle);
        assert_eq!(
            observed_bundle_sha256, expected_bundle_sha256,
            "current native Scene backend reproduces UI-produced Map {map} candidate"
        );
        assert_eq!(compiled["source_sha256"], source_sha256);
        ok(
            &command(
                &studio,
                "field.scene.draft",
                json!({"kind":"close","scene":scene}),
            ),
            "close imported offline Scene project",
        );
        maps.push(json!({
            "map":map,
            "saved_project_sha256":project_sha256,
            "source_sha256":source_sha256,
            "expected_ui_candidate_sha256":expected_bundle_sha256,
            "native_replay_candidate_sha256":observed_bundle_sha256,
            "candidate_hashes_match":observed_bundle_sha256 == expected_bundle_sha256
        }));
    }
    write_summary(
        &output,
        &json!({
            "kind":"saved_offline_scene_current_backend_replay",
            "maps":maps,
            "raw_bundle_bytes_written":false
        }),
    );
}

#[test]
#[ignore = "requires the exact previously qualified Map 23 Scene project"]
fn original_map23_prefix_identity_sparse_repack_regression() {
    let project_path = PathBuf::from(
        std::env::var_os("XEM_STUDIO_M7_QUALIFIED_PROJECT")
            .expect("explicit historical qualified Scene project path is required"),
    );
    assert!(project_path.is_file(), "historical Scene project exists");
    let output = fresh_output();
    let project_bytes = fs::read(project_path).expect("read historical Scene project");
    assert_eq!(
        hash(&project_bytes),
        "3cffb561e775730c7152a83d524729e02bdbcaeff13a57007763752a37581f9a",
        "use the exact previously accepted saved Scene project"
    );
    let project: Value =
        serde_json::from_slice(&project_bytes).expect("saved Scene project is JSON");
    let mut input: Input = serde_json::from_value(project["document"]["input"].clone())
        .expect("saved Scene input follows the typed bundle schema");
    assert_eq!(
        input
            .source
            .as_ref()
            .map(|source| source.source_sha256.as_str()),
        Some(ORIGINAL_BUNDLE_SHA256)
    );
    input.context = None;
    input.module_sha256 = None;

    let source = Source::load(input.clone())
        .expect("exact indexed source qualifies for offline prefix policy");
    assert_eq!(source.components.len(), 9);
    assert_eq!(source.components[8].logical_size, 72);
    assert_eq!(source.components[8].optional_tail_bytes, 6);
    assert_eq!(source.components[8].bytes.len(), 72);
    let unchanged = source
        .repack(&source.header, &std::collections::BTreeMap::new())
        .expect("empty repack is a byte-exact no-op");
    assert_eq!(hash(&unchanged), ORIGINAL_BUNDLE_SHA256);

    let component8 = &source.components[8];
    let c8_stream = input
        .bundle
        .get(component8.source_offset..)
        .expect("qualified c8 begins inside indexed source");
    assert!(component8.source_bytes_read > 1);
    let actual_required_prefix = c8_stream
        .get(..component8.source_bytes_read)
        .expect("actual c8 source has all compressed bytes needed for its logical prefix");
    let decoded_prefix = packed::decode_prefix(actual_required_prefix, 72, 88)
        .expect("actual source prefix decodes exactly the required logical bytes");
    assert_eq!(decoded_prefix.data.len(), 72);
    assert!(
        packed::decode_prefix(
            &actual_required_prefix[..actual_required_prefix.len() - 1],
            72,
            88
        )
        .is_err(),
        "removing one required compressed source byte must fail"
    );

    let mut tampered_identity = input.clone();
    tampered_identity
        .source
        .as_mut()
        .expect("qualified source identity")
        .source_sha256 = "00".repeat(32);
    let identity_error =
        Source::load(tampered_identity).expect_err("modified identity is rejected");
    assert_eq!(identity_error.kind, "source_mismatch");
    let mut structurally_accepted_metadata = Vec::new();
    for (field, value) in [("map", 22), ("slot", 651), ("lba", 124_527)] {
        let mut changed = input.clone();
        let identity = changed.source.as_mut().expect("qualified source identity");
        match field {
            "map" => identity.map = value,
            "slot" => identity.slot = value,
            "lba" => identity.lba = value,
            _ => unreachable!("fixed identity selector set"),
        }
        let accepted = Source::load(changed).unwrap_or_else(|error| {
            panic!("range-valid {field} metadata is structural: {}", error.kind)
        });
        assert_eq!(hash(&accepted.input().bundle), ORIGINAL_BUNDLE_SHA256);
        assert_eq!(accepted.components[8].logical_size, 72);
        assert_eq!(accepted.components[8].optional_tail_bytes, 6);
        structurally_accepted_metadata.push(field);
    }
    let mut bad_size = input.clone();
    bad_size
        .source
        .as_mut()
        .expect("qualified source identity")
        .indexed_size += 1;
    assert_eq!(
        Source::load(bad_size)
            .expect_err("changed indexed size is rejected")
            .kind,
        "source_mismatch"
    );
    let mut bad_boot = input.clone();
    bad_boot
        .source
        .as_mut()
        .expect("qualified source identity")
        .boot_sha256 = "00".repeat(32);
    assert_eq!(
        Source::load(bad_boot)
            .expect_err("changed boot identity is rejected")
            .kind,
        "source_mismatch"
    );

    let mut palette_bytes = source.components[0].bytes.clone();
    assert!(!palette_bytes.is_empty());
    palette_bytes[0] ^= 1;
    let palette_replacement = Replacement {
        logical_size: source.components[0].logical_size,
        bytes: palette_bytes,
    };
    let replacements = std::collections::BTreeMap::from([(0, palette_replacement)]);
    let sparse_candidate = source
        .repack(&source.header, &replacements)
        .expect("a sparse palette replacement normalizes optional c8 to a self-contained stream");
    assert_eq!(hash(&source.input().bundle), ORIGINAL_BUNDLE_SHA256);
    assert_ne!(hash(&sparse_candidate), ORIGINAL_BUNDLE_SHA256);
    let normalized = Source::load(Input {
        bundle: sparse_candidate,
        context: None,
        source: None,
        module_sha256: None,
    })
    .expect("sparse candidate strictly reloads without original source-tail access");
    assert_eq!(normalized.components[8].logical_size, 72);
    assert_eq!(normalized.components[8].bytes.len(), 72);
    assert_eq!(normalized.components[8].optional_tail_bytes, 0);
    assert_eq!(
        hash(&normalized.components[8].bytes),
        hash(&source.components[8].bytes)
    );
    assert!(normalized.components[8].source_offset >= input.bundle.len());
    assert_ne!(
        hash(&normalized.components[0].bytes),
        hash(&source.components[0].bytes)
    );
    for index in 1..8 {
        assert_eq!(
            hash(&normalized.components[index].bytes),
            hash(&source.components[index].bytes)
        );
    }

    write_summary(
        &output,
        &json!({
            "status":"passed",
            "disc":1,"map":23,"slot":652,"source_sha256":ORIGINAL_BUNDLE_SHA256,
            "historical_project_sha256":hash(&project_bytes),
            "empty_repack_sha256":hash(&unchanged),
            "required_prefix_size":decoded_prefix.data.len(),
            "one_byte_short_prefix_rejected":true,
            "tampered_source_identity_kind":identity_error.kind,
            "structurally_accepted_metadata_fields":structurally_accepted_metadata,
            "invalid_indexed_size_rejected":true,"invalid_boot_identity_rejected":true,
            "sparse_component_index":0,
            "sparse_candidate_sha256":hash(&normalized.input().bundle),
            "optional_c8_source_tail_bytes":6,
            "normalized_c8_output_size":normalized.components[8].bytes.len(),
            "normalized_c8_optional_tail_bytes":normalized.components[8].optional_tail_bytes,
            "unaffected_component_indices":[1,2,3,4,5,6,7],
            "original_source_unchanged":hash(&source.input().bundle)==ORIGINAL_BUNDLE_SHA256
        }),
    );
}
