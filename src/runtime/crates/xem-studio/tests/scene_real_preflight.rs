//! Opt-in original-disc format/source-context preflight. This is not runtime
//! playback acceptance and writes only a private numeric/hash summary.
use serde_json::{Value, json};
use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};
use xem_disc::field_context::{SourceContext, hash};
use xem_studio::Studio;

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
    assert!(
        disc_path.is_file() && context_path.is_file(),
        "original inputs exist at explicit paths"
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

    let input = json!({"bundle":original_bundle,"context":context,"source":identity,"module_sha256":MODULE_SHA256});
    let no_context = command(
        &studio,
        "field.scene.decode",
        json!({
            "input":{"bundle":original_bundle,"context":null,"source":identity,"module_sha256":null},
            "entrances":[]
        }),
    );
    assert_eq!(
        no_context["error"]["kind"], "source_context_required",
        "indexed-only source cannot silently invent missing measured component bytes"
    );

    let decoded = ok(
        &command(
            &studio,
            "field.scene.decode",
            json!({"input":input,"entrances":[]}),
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

    let opened_scene = ok(
        &command(
            &studio,
            "field.scene.draft",
            json!({"kind":"open","input":input,"entrances":[]}),
        ),
        "open source-owned original scene document",
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
        "no-edit compile preserves indexed bundle bytes (source {}, compiled {})",
        hash(&original_bundle),
        hash(&compiled_original)
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
    for (before, after) in components.iter().zip(transformed_components) {
        assert_eq!(
            before["logical_size"], after["logical_size"],
            "descriptor edit does not alter component boundaries"
        );
        assert_eq!(
            before["output_size"], after["output_size"],
            "complete component output size is unchanged"
        );
        assert_eq!(
            before["output_sha256"], after["output_sha256"],
            "complete component output is preserved"
        );
    }
    assert_eq!(
        transformed["header"]["descriptors"][index]["position"][0],
        position[0]
    );
    let changed_position_byte = 0x190 + index * 16 + 8;
    assert_eq!(
        transformed_bundle.len(),
        original_bundle.len() + measured_guard.len(),
        "candidate adds only the measured source guard to the indexed bundle"
    );
    assert!(
        transformed_bundle[..changed_position_byte] == original_bundle[..changed_position_byte],
        "bytes before the selected position word are preserved"
    );
    assert!(
        transformed_bundle[changed_position_byte + 2..original_bundle.len()]
            == original_bundle[changed_position_byte + 2..],
        "remaining indexed bytes after the selected position word are preserved"
    );
    assert!(
        transformed_bundle[original_bundle.len()..] == measured_guard,
        "only the context-verified guard follows the original indexed bytes"
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
            "no_edit_byte_exact":true,"static_transform_changed_bundle":true,
            "all_component_outputs_preserved":true,"export_import_bundle_equal":true
        }),
    );
}
