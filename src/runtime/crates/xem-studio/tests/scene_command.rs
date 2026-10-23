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
    let record = |studio: &Studio, trace: &mut Vec<Value>, op: &str, args: Value| {
        let request = json!({"op":op,"args":args});
        let response: Value = serde_json::from_str(&studio.command(&request.to_string()))
            .expect("Studio command response is JSON");
        trace.push(json!({"request":request,"response":response}));
        response
    };

    let version = record(&studio, &mut trace, "version", Value::Null);
    assert_eq!(
        version["ok"]["operations"].as_array().map(Vec::len),
        Some(31)
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
