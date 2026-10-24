//! Opt-in original-disc format/source-context preflight. This is not runtime
//! playback acceptance and writes only a private numeric/hash summary.
use serde_json::{Value, json};
use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};
use xem_disc::field_context::{SourceContext, hash};
use xem_disc::packed;
use xem_studio::Studio;
use xem_studio::scene::bundle::{Input, Replacement, Source};

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
                message.contains("six decoded tail bytes are not preserved")
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
    let mut rejected_selectors = Vec::new();
    for (field, value) in [("map", 22), ("slot", 651), ("lba", 124_527)] {
        let mut changed = input.clone();
        let identity = changed.source.as_mut().expect("qualified source identity");
        match field {
            "map" => identity.map = value,
            "slot" => identity.slot = value,
            "lba" => identity.lba = value,
            _ => unreachable!("fixed identity selector set"),
        }
        assert!(
            Source::load(changed).is_err(),
            "changing source {field} must disable the exact-source exception"
        );
        rejected_selectors.push(field);
    }

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
            "modified_identity_selectors_rejected":rejected_selectors,
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
