use serde_json::{Value, json};
use xem_studio::Studio;

#[path = "support/scene_bundle.rs"]
mod scene_bundle;

fn command(studio: &Studio, op: &str, args: Value) -> Value {
    let request = json!({"op":op,"args":args});
    serde_json::from_str(&studio.command(&request.to_string())).expect("Studio replies with JSON")
}

fn decode(studio: &Studio, bytes: Vec<u8>) -> Value {
    command(
        studio,
        "field.scene.decode",
        scene_bundle::scene_args(bytes),
    )
}

fn scene_draft(studio: &Studio, args: Value) -> Value {
    command(studio, "field.scene.draft", args)
}

fn scene_open_args(bytes: Vec<u8>) -> Value {
    let args = scene_bundle::scene_args(bytes);
    json!({"kind":"open","input":args["input"],"entrances":args["entrances"]})
}

fn record_wire(studio: &Studio, trace: &mut Vec<Value>, request_json: String) -> Value {
    let request: Value = serde_json::from_str(&request_json).expect("recorded request is JSON");
    let response: Value = serde_json::from_str(&studio.command(&request_json))
        .expect("Studio command response is JSON");
    trace.push(json!({
        "request":request,
        "request_json":request_json,
        "response":response
    }));
    response
}

fn record(studio: &Studio, trace: &mut Vec<Value>, op: &str, args: Value) -> Value {
    record_wire(studio, trace, json!({"op":op,"args":args}).to_string())
}

#[test]
fn composed_scene_command_decodes_all_nine_components_and_linked_format_views() {
    let studio = Studio::new();
    let reply = decode(&studio, scene_bundle::valid_bundle());
    let scene = reply.get("ok").expect("synthetic scene decode succeeds");

    assert_eq!(scene["schema_version"], 1);
    assert_eq!(scene["source"], Value::Null);
    assert_eq!(scene["context_module_sha256"], Value::Null);
    assert_eq!(scene["components"].as_array().map(Vec::len), Some(9));
    assert_eq!(
        scene["geometry"]["groups"].as_array().map(Vec::len),
        Some(1)
    );
    let face = &scene["geometry"]["groups"][0]["meshes"][0]["faces"][0];
    assert_eq!(face["kind"], 5);
    assert_eq!(face["raw_indices"], json!([0, 1, 2, 0xFFFF]));
    assert_eq!(face["vertices"], json!([0, 1, 2]));
    assert_eq!(face["declared_tpage"], 0x3456);
    assert_eq!(face["declared_clut"], 0x789A);
    assert_eq!(face["controls"][0]["code"], 0xC4);
    assert_eq!(face["controls"][1]["code"], 0xC8);
    assert_eq!(
        face["uv"],
        json!([[0x24, 0x25], [0x26, 0x27], [0x20, 0x21]])
    );

    assert_eq!(
        scene["collision"]["layers"].as_array().map(Vec::len),
        Some(1)
    );
    assert_eq!(
        scene["collision"]["layers"][0]["triangles"]
            .as_array()
            .map(Vec::len),
        Some(1)
    );
    assert_eq!(
        scene["collision"]["layers"][0]["triangles"][0]["height_bump_raw"],
        0xE4
    );
    assert_eq!(scene["zones"]["zones"].as_array().map(Vec::len), Some(1));
    assert_eq!(scene["zones"]["zones"][0]["corners"][0], json!([1, -2, 3]));
    assert_eq!(
        scene["palettes"]["uploads"].as_array().map(Vec::len),
        Some(2)
    );
    assert_eq!(
        scene["palettes"]["uploads"][0]["words"],
        json!([0x1234, 0xABCD])
    );
    assert_eq!(scene["images"]["uploads"].as_array().map(Vec::len), Some(1));
    assert_eq!(scene["images"]["uploads"][0]["rect"], json!([10, 20, 2, 1]));
    assert_eq!(
        scene["images"]["uploads"][0]["words"],
        json!([0xCAFE, 0xBEEF])
    );

    assert_eq!(scene["events"]["instructions"][0]["pc"], 15);
    assert_eq!(scene["actors"][0]["source_index"], 0);
    assert_eq!(scene["entrances"].as_array().map(Vec::len), Some(2));
    assert_eq!(scene["entrances"][0]["x"], 0x1234);
    assert_eq!(scene["entrances"][0]["z"], -0x1234);
    assert_eq!(scene["entrances"][0]["camera_raw"], 0xFF);
    assert_eq!(scene["entrances"][0]["facing_raw"], 0xFF);
    assert_eq!(scene["entrances"][1]["selector"], 1);
}

#[test]
fn scene_draft_command_owns_handles_and_roundtrips_revisions_history_and_compilation() {
    let studio = Studio::new();
    let opened = scene_draft(&studio, scene_open_args(scene_bundle::valid_bundle()));
    let scene = opened["ok"]["scene"].as_u64().expect("scene handle") as u32;
    assert_eq!(scene, 1);
    assert_eq!(opened["ok"]["state"]["revision"], 0);

    let entrance_span_wire = json!({
        "op":"field.scene.draft",
        "args":{"kind":"edit","scene":scene,"revision":0,
            "action":{"kind":"set_entrance","value":{
                "selector":0,"source_span":[4294967296_u64,4294967297_u64],
                "x":0,"z":0,"layer":0,"camera_raw":255,"facing_raw":255
            }}
        }
    })
    .to_string();
    let entrance_overflow: Value =
        serde_json::from_str(&studio.command(&entrance_span_wire)).unwrap();
    assert_eq!(entrance_overflow["error"]["kind"], "invalid_args");

    let edited = scene_draft(
        &studio,
        json!({
            "kind":"edit", "scene":scene, "revision":0,
            "action":{"kind":"set_zone","id":"zone:0","corners":[[9,8,7],[6,5,4],[3,2,1],[0,-1,-2]]}
        }),
    );
    assert_eq!(edited["ok"]["state"]["revision"], 1);
    assert_eq!(
        edited["ok"]["state"]["zones"][0]["value"]["corners"][0],
        json!([9, 8, 7])
    );
    let stale = scene_draft(
        &studio,
        json!({
            "kind":"edit", "scene":scene, "revision":0,
            "action":{"kind":"set_zone","id":"zone:0","corners":[[0,0,0],[0,0,0],[0,0,0],[0,0,0]]}
        }),
    );
    assert_eq!(stale["error"]["kind"], "stale_revision");

    let snapshot = scene_draft(&studio, json!({"kind":"snapshot","scene":scene}));
    assert_eq!(snapshot["ok"]["state"], edited["ok"]["state"]);
    let undone = scene_draft(&studio, json!({"kind":"undo","scene":scene,"revision":1}));
    assert_eq!(undone["ok"]["state"]["revision"], 2);
    assert_eq!(
        undone["ok"]["state"]["zones"][0]["value"]["corners"][0],
        json!([1, -2, 3])
    );
    let redone = scene_draft(&studio, json!({"kind":"redo","scene":scene,"revision":2}));
    assert_eq!(redone["ok"]["state"]["revision"], 3);
    let mut redone_state = redone["ok"]["state"].clone();
    let mut edited_state = edited["ok"]["state"].clone();
    redone_state.as_object_mut().unwrap().remove("revision");
    edited_state.as_object_mut().unwrap().remove("revision");
    assert_eq!(redone_state, edited_state);

    let exported = scene_draft(&studio, json!({"kind":"export","scene":scene}));
    assert_eq!(exported["ok"]["cursor"], 1);
    let compiled = scene_draft(&studio, json!({"kind":"compile","scene":scene}));
    assert!(compiled["ok"]["bundle"].is_array());
    assert_eq!(compiled["ok"]["descriptor_ids"], json!(["descriptor:0"]));
    assert_eq!(compiled["ok"]["zone_ids"], json!(["zone:0"]));

    assert_eq!(
        scene_draft(&studio, json!({"kind":"close","scene":scene})),
        json!({"ok":{"closed":scene}})
    );
    let imported = scene_draft(&studio, json!({"kind":"import","document":exported["ok"]}));
    assert_eq!(
        imported["ok"]["scene"], 2,
        "handles are monotonic after close"
    );
    assert_eq!(imported["ok"]["state"]["revision"], 0);
    let recompiled = scene_draft(
        &studio,
        json!({"kind":"compile","scene":imported["ok"]["scene"]}),
    );
    assert_eq!(recompiled["ok"]["bundle"], compiled["ok"]["bundle"]);
    assert_eq!(
        scene_draft(
            &studio,
            json!({"kind":"close","scene":imported["ok"]["scene"]})
        ),
        json!({"ok":{"closed":2}})
    );
}

#[test]
fn malformed_scene_open_import_and_capacity_fail_without_adopting_handles() {
    let studio = Studio::new();
    let failed_open = scene_draft(&studio, scene_open_args(vec![0; 8]));
    assert!(failed_open["error"].is_object());
    let opened = scene_draft(&studio, scene_open_args(scene_bundle::valid_bundle()));
    assert_eq!(
        opened["ok"]["scene"], 1,
        "failed open did not consume a handle"
    );

    let bad_document = json!({
        "schema_version": 99,
        "input": scene_bundle::scene_args(scene_bundle::valid_bundle())["input"],
        "entrance_selectors": [0,1], "actions": [], "allocation_starts": [],
        "cursor": 0, "next_id": 1
    });
    let failed_import = scene_draft(&studio, json!({"kind":"import","document":bad_document}));
    assert_eq!(failed_import["error"]["kind"], "invalid_scene_edit");

    let second = scene_draft(&studio, scene_open_args(scene_bundle::valid_bundle()));
    let third = scene_draft(&studio, scene_open_args(scene_bundle::valid_bundle()));
    let fourth = scene_draft(&studio, scene_open_args(scene_bundle::valid_bundle()));
    assert_eq!(
        json!([
            second["ok"]["scene"],
            third["ok"]["scene"],
            fourth["ok"]["scene"]
        ]),
        json!([2, 3, 4])
    );
    let at_capacity = scene_draft(&studio, scene_open_args(scene_bundle::valid_bundle()));
    assert_eq!(at_capacity["error"]["kind"], "scene_resource_limit");
    assert_eq!(
        scene_draft(&studio, json!({"kind":"close","scene":2})),
        json!({"ok":{"closed":2}})
    );
    let after_close = scene_draft(&studio, scene_open_args(scene_bundle::valid_bundle()));
    assert_eq!(
        after_close["ok"]["scene"], 5,
        "close frees capacity without reusing IDs"
    );
    assert_eq!(
        scene_draft(&studio, json!({"kind":"snapshot","scene":2}))["error"]["kind"],
        "unknown_scene"
    );
}

#[test]
fn scene_handles_are_separate_from_legacy_event_draft_handles() {
    let studio = Studio::new();
    let event = scene_bundle::components()[5].clone();
    let event_size = event.len();
    let graph = command(
        &studio,
        "field.decode",
        json!({"bytes":event,"declared_size":event_size}),
    )["ok"]
        .clone();
    let field_draft = command(&studio, "field.draft.open", json!({"graph":graph}));
    assert_eq!(field_draft["ok"]["draft"], 1);
    let scene = scene_draft(&studio, scene_open_args(scene_bundle::valid_bundle()));
    assert_eq!(scene["ok"]["scene"], 1);

    scene_draft(&studio, json!({"kind":"close","scene":1}));
    assert!(command(&studio, "field.draft.snapshot", json!({"draft":1}))["ok"].is_object());
    command(&studio, "field.draft.close", json!({"draft":1}));
    let reopened = scene_draft(&studio, scene_open_args(scene_bundle::valid_bundle()));
    assert_eq!(reopened["ok"]["scene"], 2);
}

#[test]
fn scene_import_rejects_portable_context_and_cursor_overflow_before_handle_adoption() {
    let studio = Studio::new();
    let mut args = scene_open_args(scene_bundle::valid_bundle());
    args["input"]["context"] = json!({"components":[{"index":u64::MAX}]});
    let portable = scene_draft(&studio, args);
    assert_eq!(portable["error"]["kind"], "invalid_args");
    assert!(
        portable["error"]["message"]
            .as_str()
            .unwrap()
            .contains("portable u32")
    );

    let input = scene_bundle::scene_args(scene_bundle::valid_bundle())["input"].clone();
    let cursor_wire = json!({
        "op":"field.scene.draft",
        "args":{"kind":"import","document":{
            "schema_version":1,"input":input,"entrance_selectors":[0,1],
            "actions":[],"allocation_starts":[],"cursor":4294967296_u64,"next_id":1
        }}
    })
    .to_string();
    let cursor: Value = serde_json::from_str(&studio.command(&cursor_wire)).unwrap();
    assert_eq!(cursor["error"]["kind"], "invalid_args");
    let opened = scene_draft(&studio, scene_open_args(scene_bundle::valid_bundle()));
    assert_eq!(
        opened["ok"]["scene"], 1,
        "malformed opens/imports do not adopt handles"
    );
}

#[test]
fn malformed_max_offsets_and_table_counts_return_structured_scene_errors() {
    let studio = Studio::new();
    for (label, bytes) in scene_bundle::malformed_cases() {
        let reply = decode(&studio, bytes);
        let error = reply
            .get("error")
            .and_then(Value::as_object)
            .unwrap_or_else(|| panic!("{label} should be rejected by the composed scene decoder"));
        assert_eq!(reply.as_object().map(serde_json::Map::len), Some(1));
        let kind = error
            .get("kind")
            .and_then(Value::as_str)
            .expect("error kind");
        if label == "geometry_group_count" {
            assert_eq!(kind, "scene_resource_limit");
        } else {
            assert!(
                kind.starts_with("invalid_scene"),
                "unexpected error class for {label}"
            );
        }
        assert!(
            error
                .get("message")
                .and_then(Value::as_str)
                .is_some_and(|message| !message.is_empty())
        );
    }
}

#[test]
#[ignore = "writes a synthetic-only command trace for exact native/WASM replay"]
fn writes_native_scene_command_trace_for_wasm_replay_when_requested() {
    let path = std::env::var_os("XEM_STUDIO_SCENE_PARITY_FIXTURE")
        .map(std::path::PathBuf::from)
        .expect("set XEM_STUDIO_SCENE_PARITY_FIXTURE to a fresh ignored output path");
    let repository = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .ancestors()
        .nth(4)
        .expect("crate is inside the repository")
        .to_path_buf();
    assert!(
        path.starts_with(repository.join("tmp/studio")),
        "scene parity output must remain in ignored tmp/studio"
    );

    let studio = Studio::new();
    let mut trace = Vec::new();
    let version = record(&studio, &mut trace, "version", Value::Null);
    assert_eq!(
        version["ok"]["operations"].as_array().map(Vec::len),
        Some(32)
    );

    let valid = record(
        &studio,
        &mut trace,
        "field.scene.decode",
        scene_bundle::scene_args(scene_bundle::valid_bundle()),
    );
    assert!(valid.get("ok").is_some(), "synthetic scene decode succeeds");

    for (label, bytes) in scene_bundle::malformed_cases() {
        let rejected = record(
            &studio,
            &mut trace,
            "field.scene.decode",
            scene_bundle::scene_args(bytes),
        );
        assert!(
            rejected.get("error").is_some(),
            "synthetic malformed case {label} is rejected"
        );
    }

    // Exercise the source-owned stateful API in the same exact request/reply
    // stream that is replayed through the production-default WASM backend.
    let opened = record(
        &studio,
        &mut trace,
        "field.scene.draft",
        scene_open_args(scene_bundle::valid_bundle()),
    );
    assert!(opened["ok"].is_object(), "synthetic scene draft opens");
    let scene = opened["ok"]["scene"].as_u64().unwrap() as u32;
    let edited = record(
        &studio,
        &mut trace,
        "field.scene.draft",
        json!({
            "kind":"edit","scene":scene,"revision":0,
            "action":{"kind":"set_zone","id":"zone:0","corners":[[-9,8,-7],[6,-5,4],[-3,2,-1],[0,1,2]]}
        }),
    );
    assert_eq!(edited["ok"]["state"]["revision"], 1);
    record(
        &studio,
        &mut trace,
        "field.scene.draft",
        json!({"kind":"snapshot","scene":scene}),
    );
    record(
        &studio,
        &mut trace,
        "field.scene.draft",
        json!({"kind":"undo","scene":scene,"revision":1}),
    );
    record(
        &studio,
        &mut trace,
        "field.scene.draft",
        json!({"kind":"redo","scene":scene,"revision":2}),
    );
    record(
        &studio,
        &mut trace,
        "field.scene.draft",
        json!({"kind":"edit","scene":scene,"revision":3,
            "action":{"kind":"add_collision_vertex","layer":"collision:0","position":[21,-32,43]}}),
    );
    record(
        &studio,
        &mut trace,
        "field.scene.draft",
        json!({"kind":"edit","scene":scene,"revision":4,
            "action":{"kind":"set_collision_vertex","id":"scene:1:collision_vertex","position":[123,-456,789]}}),
    );
    record(
        &studio,
        &mut trace,
        "field.scene.draft",
        json!({"kind":"edit","scene":scene,"revision":5,
            "action":{"kind":"add_collision_triangle","layer":"collision:0",
            "vertices":["collision:0:vertex:2","collision:0:vertex:1","scene:1:collision_vertex"],
            "attribute":1,"height_bump_raw":243}}),
    );
    record(
        &studio,
        &mut trace,
        "field.scene.draft",
        json!({"kind":"edit","scene":scene,"revision":6,
            "action":{"kind":"duplicate_collision_triangle","id":"scene:2:collision_triangle"}}),
    );
    record(
        &studio,
        &mut trace,
        "field.scene.draft",
        json!({"kind":"undo","scene":scene,"revision":7}),
    );
    record(
        &studio,
        &mut trace,
        "field.scene.draft",
        json!({"kind":"redo","scene":scene,"revision":8}),
    );
    let exported = record(
        &studio,
        &mut trace,
        "field.scene.draft",
        json!({"kind":"export","scene":scene}),
    );
    let compiled = record(
        &studio,
        &mut trace,
        "field.scene.draft",
        json!({"kind":"compile","scene":scene}),
    );
    record(
        &studio,
        &mut trace,
        "field.scene.draft",
        json!({"kind":"close","scene":scene}),
    );
    let imported = record(
        &studio,
        &mut trace,
        "field.scene.draft",
        json!({"kind":"import","document":exported["ok"]}),
    );
    assert_eq!(imported["ok"]["scene"], 2);
    let recompiled = record(
        &studio,
        &mut trace,
        "field.scene.draft",
        json!({"kind":"compile","scene":2}),
    );
    assert_eq!(recompiled["ok"]["bundle"], compiled["ok"]["bundle"]);
    record(
        &studio,
        &mut trace,
        "field.scene.draft",
        json!({"kind":"close","scene":2}),
    );

    // Keep raw request text in the trace: JS JSON.parse/serialize would round
    // u64::MAX before the backend can validate its portable u32 boundary.
    let portable_request = format!(
        "{{\"op\":\"field.scene.draft\",\"args\":{{\"kind\":\"open\",\"input\":{{\"bundle\":{},\"context\":{{\"components\":[{{\"index\":18446744073709551615}}]}},\"source\":null,\"module_sha256\":null}},\"entrances\":[0,1]}}}}",
        serde_json::to_string(&scene_bundle::valid_bundle()).unwrap()
    );
    let portable = record_wire(&studio, &mut trace, portable_request);
    assert_eq!(portable["error"]["kind"], "invalid_args");
    let cursor_request = json!({
        "op":"field.scene.draft",
        "args":{"kind":"import","document":{
            "schema_version":1,
            "input":scene_bundle::scene_args(scene_bundle::valid_bundle())["input"],
            "entrance_selectors":[0,1],"actions":[],"allocation_starts":[],
            "cursor":4294967296_u64,"next_id":1
        }}
    })
    .to_string();
    let cursor = record_wire(&studio, &mut trace, cursor_request);
    assert_eq!(cursor["error"]["kind"], "invalid_args");
    let after_malformed = record(
        &studio,
        &mut trace,
        "field.scene.draft",
        scene_open_args(scene_bundle::valid_bundle()),
    );
    assert_eq!(after_malformed["ok"]["scene"], 3);
    let entrance_span_wire = json!({
        "op":"field.scene.draft",
        "args":{"kind":"edit","scene":3,"revision":0,
            "action":{"kind":"set_entrance","value":{
                "selector":0,"source_span":[4294967296_u64,4294967297_u64],
                "x":0,"z":0,"layer":0,"camera_raw":255,"facing_raw":255
            }}
        }
    })
    .to_string();
    let entrance_overflow = record_wire(&studio, &mut trace, entrance_span_wire);
    assert_eq!(entrance_overflow["error"]["kind"], "invalid_args");
    record(
        &studio,
        &mut trace,
        "field.scene.draft",
        json!({"kind":"close","scene":3}),
    );

    let fixture = json!({"schema_version":1,"synthetic_only":true,"trace":trace});
    let parent = path.parent().expect("fixture output has parent");
    std::fs::create_dir_all(parent).expect("create ignored fixture parent");
    let mut output = std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(path)
        .expect("fixture output path is fresh");
    serde_json::to_writer(&mut output, &fixture).expect("write synthetic trace JSON");
    use std::io::Write;
    output.write_all(b"\n").expect("finish trace newline");
}
