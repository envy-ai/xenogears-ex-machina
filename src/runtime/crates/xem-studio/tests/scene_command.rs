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

fn scene_args_without_entrances(bytes: Vec<u8>) -> Value {
    let mut args = scene_bundle::scene_args(bytes);
    args["entrances"] = json!([]);
    args
}

fn scene_open_args_without_entrances(bytes: Vec<u8>) -> Value {
    let args = scene_args_without_entrances(bytes);
    json!({"kind":"open","input":args["input"],"entrances":args["entrances"]})
}

fn placement_open_args(bytes: Vec<u8>) -> Value {
    let args = scene_bundle::scene_args(bytes);
    json!({"kind":"open","input":args["input"],"entrances":[]})
}

const BYTECODE_OFFSET: usize = 132 + 64;

fn primary_opcode(handler: &str) -> u8 {
    let metadata: Value = serde_json::from_str(include_str!("../data/field-opcodes.json")).unwrap();
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
        .unwrap_or_else(|| panic!("canonical primary opcode metadata contains {handler}"))
}

fn placement_scene_bundle(
    actor_count: usize,
    entries: &[(usize, usize, usize)],
    code: &[u8],
) -> Vec<u8> {
    const HEADER: usize = 132;
    const ROW: usize = 64;
    let mut events = vec![0; HEADER + actor_count * ROW + code.len()];
    events[128..132].copy_from_slice(&(actor_count as u32).to_le_bytes());
    for &(actor, event, pc) in entries {
        assert!(actor < actor_count && event < 32 && pc <= u16::MAX as usize);
        let offset = HEADER + actor * ROW + event * 2;
        events[offset..offset + 2].copy_from_slice(&(pc as u16).to_le_bytes());
    }
    let code_start = HEADER + actor_count * ROW;
    events[code_start..].copy_from_slice(code);

    let mut components = scene_bundle::components();
    components[5] = events;
    let original = scene_bundle::bundle(&components);
    if actor_count == 1 {
        return original;
    }
    let source = xem_studio::scene::bundle::Source::load(xem_studio::scene::bundle::Input {
        bundle: original,
        context: None,
        source: None,
        module_sha256: None,
    })
    .expect("load complete synthetic source bundle");
    let mut header = source.header.clone();
    while header.descriptors.len() < actor_count {
        header.descriptors.push(header.descriptors[0].clone());
    }
    source
        .repack(&header, &Default::default())
        .expect("add source-backed descriptors for synthetic actor rows")
}

fn scene_components_with_zone_event() -> Vec<Vec<u8>> {
    let mut components = scene_bundle::components();
    // The original event is nop@15, call_in_zone@16 (zone 1, target return@22),
    // end_slot@20, an unreachable nop@21, and return@22. The call returns to
    // end_slot@20. Keep zone 1 explicit so deleting zone:0 must repair the
    // stable source operand while the M6 insertion relocates the instruction.
    components[5][132..134].copy_from_slice(&15_u16.to_le_bytes());
    components[5].resize(BYTECODE_OFFSET + 23, 0);
    components[5][BYTECODE_OFFSET + 15..BYTECODE_OFFSET + 23]
        .copy_from_slice(&[0x0e, 0x0a, 1, 22, 0, 0x00, 0x0e, 0x0d]);
    for value in [20_i16, 21, 22, 23, 24, 25, 26, 27, 28, 29, 30, 31] {
        components[8].extend_from_slice(&value.to_le_bytes());
    }
    components
}

fn scene_with_zone_event() -> Vec<u8> {
    scene_bundle::bundle(&scene_components_with_zone_event())
}

fn byte_array(value: &Value) -> Vec<u8> {
    value
        .as_array()
        .expect("byte buffer is an array")
        .iter()
        .map(|value| value.as_u64().expect("byte is an integer") as u8)
        .collect()
}

fn source_event_component(bundle: &[u8]) -> (Vec<u8>, usize) {
    let source = xem_studio::scene::bundle::Source::load(xem_studio::scene::bundle::Input {
        bundle: bundle.to_vec(),
        context: None,
        source: None,
        module_sha256: None,
    })
    .expect("load complete synthetic source bundle");
    (
        source.components[5].bytes.clone(),
        source.components[5].logical_size,
    )
}

fn export_m6_event_document(studio: &Studio, bundle: &[u8]) -> Value {
    let (event_component, logical_size) = source_event_component(bundle);
    let graph = command(
        studio,
        "field.decode",
        json!({"bytes":event_component,"declared_size":logical_size}),
    )["ok"]
        .clone();
    let opened = command(studio, "field.draft.open", json!({"graph":graph}));
    let draft = opened["ok"]["draft"].clone();
    let new_event = command(
        studio,
        "field.draft.edit",
        json!({
            "draft":draft,"revision":0,
            "action":{"kind":"new_event","actor":0,"event":1,
                "nodes":[{"bytes":[14]},{"bytes":[0]}]}
        }),
    );
    assert!(
        new_event["ok"].is_object(),
        "M6 new-event edit succeeds: {new_event}"
    );
    let inserted = command(
        studio,
        "field.draft.edit",
        json!({
            "draft":draft,"revision":1,
            "action":{"kind":"insert_after","after":"pc:15","bytes":[14]}
        }),
    );
    assert!(
        inserted["ok"].is_object(),
        "M6 insertion succeeds: {inserted}"
    );
    command(studio, "field.draft.export", json!({"draft":draft}))["ok"].clone()
}

fn record_m6_event_document(studio: &Studio, trace: &mut Vec<Value>, bundle: &[u8]) -> Value {
    let (event_component, logical_size) = source_event_component(bundle);
    let graph = record(
        studio,
        trace,
        "field.decode",
        json!({"bytes":event_component,"declared_size":logical_size}),
    )["ok"]
        .clone();
    let opened = record(studio, trace, "field.draft.open", json!({"graph":graph}));
    let draft = opened["ok"]["draft"].clone();
    record(
        studio,
        trace,
        "field.draft.edit",
        json!({
            "draft":draft,"revision":0,
            "action":{"kind":"new_event","actor":0,"event":1,
                "nodes":[{"bytes":[14]},{"bytes":[0]}]}
        }),
    );
    record(
        studio,
        trace,
        "field.draft.edit",
        json!({
            "draft":draft,"revision":1,
            "action":{"kind":"insert_after","after":"pc:15","bytes":[14]}
        }),
    );
    record(studio, trace, "field.draft.export", json!({"draft":draft}))["ok"].clone()
}

fn assert_scene_open_event_document(studio: &Studio, bundle: Vec<u8>, document: Value) -> Value {
    let mut args = scene_open_args(bundle);
    args["event_document"] = document;
    scene_draft(studio, args)
}

fn assert_empty_m6_root(document: &Value) {
    assert_eq!(document["event_document"]["schema_version"], 1);
    assert_eq!(document["event_document"]["actions"], json!([]));
    assert_eq!(document["event_document"]["allocation_starts"], json!([]));
    assert_eq!(document["event_document"]["cursor"], 0);
    assert_eq!(document["event_document"]["next_id"], 1);
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

    let valid_document = scene_draft(&studio, json!({"kind":"export","scene":1}))["ok"].clone();
    let mut bad_document = valid_document.clone();
    bad_document["schema_version"] = json!(99);
    let failed_import = scene_draft(&studio, json!({"kind":"import","document":bad_document}));
    assert_eq!(failed_import["error"]["kind"], "invalid_scene_edit");
    let mut missing_event = valid_document;
    missing_event
        .as_object_mut()
        .unwrap()
        .remove("event_document");
    let failed_missing = scene_draft(&studio, json!({"kind":"import","document":missing_event}));
    assert_eq!(failed_missing["error"]["kind"], "invalid_args");

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
    let vanilla_bundle = scene_bundle::valid_bundle();
    let scene = scene_draft(&studio, scene_open_args(vanilla_bundle.clone()));
    assert_eq!(scene["ok"]["scene"], 1);
    assert_eq!(scene["ok"]["state"]["modified"], false);
    let vanilla_compiled = scene_draft(&studio, json!({"kind":"compile","scene":1}));
    assert_eq!(
        byte_array(&vanilla_compiled["ok"]["bundle"]),
        vanilla_bundle
    );
    let vanilla_document = scene_draft(&studio, json!({"kind":"export","scene":1}));
    assert_eq!(vanilla_document["ok"]["schema_version"], 4);
    assert_eq!(vanilla_document["ok"]["imported_event_actions"], 0);
    assert_empty_m6_root(&vanilla_document["ok"]);

    scene_draft(&studio, json!({"kind":"close","scene":1}));
    assert!(command(&studio, "field.draft.snapshot", json!({"draft":1}))["ok"].is_object());
    command(&studio, "field.draft.close", json!({"draft":1}));
    let reopened = scene_draft(&studio, scene_open_args(scene_bundle::valid_bundle()));
    assert_eq!(reopened["ok"]["scene"], 2);
}

#[test]
fn scene_m6_event_history_survives_zone_reference_repair_and_export_import() {
    let studio = Studio::new();
    let original = scene_with_zone_event();
    let event_document = export_m6_event_document(&studio, &original);
    assert_eq!(event_document["schema_version"], 1);
    assert_eq!(event_document["actions"].as_array().map(Vec::len), Some(2));
    assert_eq!(event_document["actions"][0]["kind"], "new_event");
    assert_eq!(event_document["actions"][1]["kind"], "insert_after");

    // The existing M6 API remains byte exact on its own. Scene.open imports the
    // authored document against the original component, then scene edits repair
    // its stable call-in-zone binding after deleting the first zone.
    let (legacy_event_bytes, legacy_event_size) = source_event_component(&original);
    let legacy_graph = command(
        &studio,
        "field.decode",
        json!({"bytes":legacy_event_bytes,"declared_size":legacy_event_size}),
    )["ok"]
        .clone();
    let legacy = command(&studio, "field.draft.open", json!({"graph":legacy_graph}));
    let legacy_compiled = command(
        &studio,
        "field.draft.compile",
        json!({"draft":legacy["ok"]["draft"]}),
    );
    assert!(legacy_compiled["ok"].is_object());
    assert_eq!(
        byte_array(&legacy_compiled["ok"]["bytes"]),
        legacy_event_bytes
    );

    let opened =
        assert_scene_open_event_document(&studio, original.clone(), event_document.clone());
    assert!(
        opened["ok"].is_object(),
        "scene opens with M6 history: {opened}"
    );
    assert_eq!(opened["ok"]["state"]["modified"], true);
    let scene = opened["ok"]["scene"].as_u64().unwrap() as u32;
    let edited = scene_draft(
        &studio,
        json!({"kind":"edit","scene":scene,"revision":0,
            "action":{"kind":"delete_zone","id":"zone:0"}}),
    );
    assert_eq!(edited["ok"]["state"]["revision"], 1);
    assert_eq!(
        edited["ok"]["state"]["zones"].as_array().map(Vec::len),
        Some(1)
    );
    let undone = scene_draft(&studio, json!({"kind":"undo","scene":scene,"revision":1}));
    assert_eq!(undone["ok"]["state"]["revision"], 2);
    assert_eq!(
        undone["ok"]["state"]["zones"].as_array().map(Vec::len),
        Some(2)
    );
    assert_eq!(
        undone["ok"]["state"]["modified"], true,
        "undoing scene history does not erase the retained M6 authoring history"
    );
    let redone = scene_draft(&studio, json!({"kind":"redo","scene":scene,"revision":2}));
    assert_eq!(redone["ok"]["state"]["revision"], 3);
    assert_eq!(redone["ok"]["state"]["modified"], true);

    let compiled = scene_draft(&studio, json!({"kind":"compile","scene":scene}));
    assert!(
        compiled["ok"].is_object(),
        "scene compile succeeds: {compiled}"
    );
    let event = &compiled["ok"]["events"];
    let bindings = event["bindings"]
        .as_object()
        .expect("compiled stable bindings");
    let call_pc = bindings["pc:16"]
        .as_u64()
        .expect("source call keeps stable ID") as usize;
    assert!(call_pc > 16, "the M6 insertion relocates the source call");
    let event_bytes = byte_array(&event["bytes"]);
    assert_eq!(event_bytes[BYTECODE_OFFSET + call_pc], 0x0a);
    assert_eq!(
        event_bytes[BYTECODE_OFFSET + call_pc + 1],
        0,
        "deleting zone:0 remaps the original zone:1 reference"
    );
    assert!(
        bindings.contains_key("new:1"),
        "authored event node ID survives"
    );
    assert!(
        bindings.contains_key("new:2"),
        "authored terminal node ID survives"
    );
    assert!(bindings.contains_key("new:3"), "inserted node ID survives");

    let exported = scene_draft(&studio, json!({"kind":"export","scene":scene}));
    let exported_document = &exported["ok"];
    assert_eq!(exported_document["schema_version"], 4);
    assert_eq!(exported_document["imported_event_actions"], 2);
    assert_empty_m6_root(exported_document);
    assert_eq!(exported_document["actions"][0]["kind"], "event");
    assert_eq!(
        exported_document["actions"][0]["action"]["kind"],
        "new_event"
    );
    assert_eq!(exported_document["actions"][1]["kind"], "event");
    assert_eq!(
        exported_document["actions"][1]["action"]["kind"],
        "insert_after"
    );
    let imported = scene_draft(&studio, json!({"kind":"import","document":exported["ok"]}));
    assert!(
        imported["ok"].is_object(),
        "scene import preserves M6 history: {imported}"
    );
    let imported_scene = imported["ok"]["scene"].as_u64().unwrap() as u32;
    let recompiled = scene_draft(&studio, json!({"kind":"compile","scene":imported_scene}));
    assert_eq!(recompiled["ok"]["bundle"], compiled["ok"]["bundle"]);
    assert_eq!(
        recompiled["ok"]["events"]["bytes"],
        compiled["ok"]["events"]["bytes"]
    );
    let reexported = scene_draft(&studio, json!({"kind":"export","scene":imported_scene}));
    assert_eq!(reexported["ok"]["imported_event_actions"], 2);
    assert_empty_m6_root(&reexported["ok"]);
    assert_eq!(
        reexported["ok"]["actions"][0]["action"],
        exported_document["actions"][0]["action"]
    );
    assert_eq!(
        reexported["ok"]["actions"][1]["action"],
        exported_document["actions"][1]["action"]
    );

    // The scene changed zones and event storage only; all other complete source
    // component streams remain identical to the original input.
    let before = xem_studio::scene::bundle::Source::load(xem_studio::scene::bundle::Input {
        bundle: original,
        context: None,
        source: None,
        module_sha256: None,
    })
    .unwrap();
    let after = xem_studio::scene::bundle::Source::load(xem_studio::scene::bundle::Input {
        bundle: byte_array(&compiled["ok"]["bundle"]),
        context: None,
        source: None,
        module_sha256: None,
    })
    .unwrap();
    for index in 0..9 {
        if ![5, 8].contains(&index) {
            assert_eq!(
                after.components[index].bytes, before.components[index].bytes,
                "unrelated complete component {index} survives M6/scene compilation"
            );
        }
    }
}

#[test]
fn scene_preserves_m6_redo_cursor_and_identity_watermark_across_import() {
    let studio = Studio::new();
    let original = scene_with_zone_event();
    let (event_bytes, logical_size) = source_event_component(&original);
    let graph = command(
        &studio,
        "field.decode",
        json!({"bytes":event_bytes,"declared_size":logical_size}),
    )["ok"]
        .clone();
    let source_graph = graph.clone();
    let opened_m6 = command(&studio, "field.draft.open", json!({"graph":graph}));
    let m6 = opened_m6["ok"]["draft"].clone();
    assert!(
        command(
            &studio,
            "field.draft.edit",
            json!({"draft":m6,"revision":0,
            "action":{"kind":"new_event","actor":0,"event":1,
                "nodes":[{"bytes":[1,0,0],"targets":{"0":"new:2"}},{"bytes":[0]}]}})
        )["ok"]
            .is_object()
    );
    assert!(
        command(
            &studio,
            "field.draft.edit",
            json!({"draft":m6,"revision":1,
            "action":{"kind":"insert_after","after":"pc:15","bytes":[14]}})
        )["ok"]
            .is_object()
    );
    let undone = command(
        &studio,
        "field.draft.undo",
        json!({"draft":m6,"revision":2}),
    );
    assert_eq!(undone["ok"]["revision"], 3);
    assert_eq!(undone["ok"]["can_redo"], true);
    let authored = command(&studio, "field.draft.export", json!({"draft":m6}))["ok"].clone();
    assert_eq!(authored["cursor"], 1);
    assert_eq!(authored["actions"].as_array().map(Vec::len), Some(2));
    assert_eq!(authored["next_id"], 4);
    let m6_applied = command(&studio, "field.draft.snapshot", json!({"draft":m6}))["ok"].clone();

    let opened_scene =
        assert_scene_open_event_document(&studio, original.clone(), authored.clone());
    assert_eq!(opened_scene["ok"]["state"]["modified"], true);
    assert_eq!(
        opened_scene["ok"]["state"]["events"]["graph"],
        m6_applied["graph"]
    );
    assert_eq!(
        opened_scene["ok"]["state"]["events"]["bindings"],
        m6_applied["bindings"]
    );
    let scene = opened_scene["ok"]["scene"].as_u64().unwrap() as u32;
    let initial_snapshot = opened_scene["ok"]["state"].clone();
    assert!(initial_snapshot["events"]["bindings"]["new:1"].is_number());
    assert!(initial_snapshot["events"]["bindings"]["new:3"].is_null());
    assert_eq!(initial_snapshot["next_event_id"], 4);
    let initial_branch_pc = initial_snapshot["events"]["bindings"]["new:1"]
        .as_u64()
        .unwrap() as usize;
    let initial_branch = initial_snapshot["events"]["graph"]["instructions"]
        .as_array()
        .unwrap()
        .iter()
        .find(|instruction| instruction["pc"] == initial_branch_pc)
        .unwrap();
    assert_eq!(initial_branch["opcode"], 1);
    assert_eq!(
        initial_branch["operands"][0].as_u64(),
        initial_snapshot["events"]["bindings"]["new:2"].as_u64()
    );

    // The imported M6 redo action lives on the same scene timeline. Redo and
    // undo operate on it directly while retaining the allocated identity.
    let redone = scene_draft(&studio, json!({"kind":"redo","scene":scene,"revision":0}));
    assert_eq!(redone["ok"]["state"]["revision"], 1);
    assert!(redone["ok"]["state"]["events"]["bindings"]["new:3"].is_number());
    let m6_redone = command(
        &studio,
        "field.draft.redo",
        json!({"draft":m6,"revision":3}),
    )["ok"]
        .clone();
    assert_eq!(redone["ok"]["state"]["events"]["graph"], m6_redone["graph"]);
    assert_eq!(
        redone["ok"]["state"]["events"]["bindings"],
        m6_redone["bindings"]
    );
    let redone_events = redone["ok"]["state"]["events"].clone();
    let undone_scene = scene_draft(&studio, json!({"kind":"undo","scene":scene,"revision":1}));
    assert_eq!(undone_scene["ok"]["state"]["revision"], 2);
    assert!(undone_scene["ok"]["state"]["events"]["bindings"]["new:3"].is_null());
    assert_eq!(undone_scene["ok"]["state"]["next_event_id"], 4);
    let undone_prefix = scene_draft(&studio, json!({"kind":"undo","scene":scene,"revision":2}));
    assert_eq!(undone_prefix["ok"]["state"]["revision"], 3);
    assert_eq!(undone_prefix["ok"]["state"]["modified"], false);
    assert_eq!(
        undone_prefix["ok"]["state"]["events"]["graph"],
        source_graph
    );
    assert_eq!(undone_prefix["ok"]["state"]["next_event_id"], 4);
    let source_compile = scene_draft(&studio, json!({"kind":"compile","scene":scene}));
    assert_eq!(
        byte_array(&source_compile["ok"]["events"]["bytes"]),
        event_bytes
    );
    assert_eq!(byte_array(&source_compile["ok"]["bundle"]), original);
    let redone_prefix = scene_draft(&studio, json!({"kind":"redo","scene":scene,"revision":3}));
    assert_eq!(redone_prefix["ok"]["state"]["revision"], 4);
    assert_eq!(
        redone_prefix["ok"]["state"]["events"],
        initial_snapshot["events"]
    );
    let redone_full_prefix =
        scene_draft(&studio, json!({"kind":"redo","scene":scene,"revision":4}));
    assert_eq!(redone_full_prefix["ok"]["state"]["revision"], 5);
    assert_eq!(redone_full_prefix["ok"]["state"]["events"], redone_events);
    let undo_for_export = scene_draft(&studio, json!({"kind":"undo","scene":scene,"revision":5}));
    assert_eq!(undo_for_export["ok"]["state"]["revision"], 6);
    assert_eq!(
        undo_for_export["ok"]["state"]["events"],
        initial_snapshot["events"]
    );
    assert_eq!(undo_for_export["ok"]["state"]["next_event_id"], 4);

    let document = scene_draft(&studio, json!({"kind":"export","scene":scene}))["ok"].clone();
    assert_eq!(document["schema_version"], 4);
    assert_eq!(document["imported_event_actions"], 2);
    assert_eq!(document["cursor"], 1);
    assert_eq!(document["next_event_id"], 4);
    assert_empty_m6_root(&document);
    assert_eq!(document["actions"][0]["kind"], "event");
    assert_eq!(document["actions"][1]["kind"], "event");

    let imported_scene = scene_draft(&studio, json!({"kind":"import","document":document}));
    assert!(imported_scene["ok"].is_object());
    let imported_scene_id = imported_scene["ok"]["scene"].as_u64().unwrap() as u32;
    let imported_snapshot = scene_draft(
        &studio,
        json!({"kind":"snapshot","scene":imported_scene_id}),
    );
    assert_eq!(imported_snapshot["ok"]["state"]["can_redo"], true);
    assert_eq!(imported_snapshot["ok"]["state"]["next_event_id"], 4);
    assert!(imported_snapshot["ok"]["state"]["events"]["bindings"]["new:3"].is_null());
    // Export/import retained the source M6 redo branch on the scene timeline.
    // A new scene event edit truncates it but keeps its watermark reserved.
    let alternate = scene_draft(
        &studio,
        json!({"kind":"edit","scene":imported_scene_id,"revision":0,
            "action":{"kind":"event","action":{"kind":"insert_after","after":"pc:15","bytes":[14],"targets":{}}}}),
    );
    assert!(
        alternate["ok"].is_object(),
        "alternate scene event edit: {alternate}"
    );
    assert_eq!(alternate["ok"]["state"]["next_event_id"], 5);
    assert!(alternate["ok"]["state"]["events"]["bindings"]["new:4"].is_number());
    let alternate_compile_before_import =
        scene_draft(&studio, json!({"kind":"compile","scene":imported_scene_id}));
    assert!(alternate_compile_before_import["ok"].is_object());
    let alternate_document =
        scene_draft(&studio, json!({"kind":"export","scene":imported_scene_id}))["ok"].clone();
    assert_eq!(alternate_document["imported_event_actions"], 1);
    assert_eq!(alternate_document["cursor"], 2);
    assert_eq!(alternate_document["next_event_id"], 5);
    assert_empty_m6_root(&alternate_document);
    let alternate_import = scene_draft(
        &studio,
        json!({"kind":"import","document":alternate_document}),
    );
    assert!(alternate_import["ok"].is_object());
    let alternate_id = alternate_import["ok"]["scene"].as_u64().unwrap() as u32;
    let alternate_compile = scene_draft(&studio, json!({"kind":"compile","scene":alternate_id}));
    assert_eq!(
        alternate_compile["ok"]["bundle"],
        alternate_compile_before_import["ok"]["bundle"]
    );
    assert_eq!(
        alternate_compile["ok"]["events"],
        alternate_compile_before_import["ok"]["events"]
    );
    assert_eq!(
        alternate_compile["ok"]["events"]["bindings"]["new:4"],
        alternate["ok"]["state"]["events"]["bindings"]["new:4"]
    );
    let alternate_events = &alternate_compile["ok"]["events"];
    let branch_pc = alternate_events["bindings"]["new:1"].as_u64().unwrap();
    let branch = alternate_events["graph"]["instructions"]
        .as_array()
        .unwrap()
        .iter()
        .find(|instruction| instruction["pc"].as_u64() == Some(branch_pc))
        .unwrap();
    assert_eq!(
        branch["operands"][0].as_u64(),
        alternate_events["bindings"]["new:2"].as_u64()
    );
}

#[test]
fn scene_event_document_schema_base_and_portable_values_are_checked_before_adoption() {
    let studio = Studio::new();
    let original = scene_with_zone_event();
    let components = scene_components_with_zone_event();
    let event_document = export_m6_event_document(&studio, &original);
    let opened =
        assert_scene_open_event_document(&studio, original.clone(), event_document.clone());
    assert!(opened["ok"].is_object());
    let valid_export = scene_draft(
        &studio,
        json!({"kind":"export","scene":opened["ok"]["scene"]}),
    )["ok"]
        .clone();

    let mut wrong_schema = valid_export.clone();
    wrong_schema["schema_version"] = json!(99);
    assert_eq!(
        scene_draft(&studio, json!({"kind":"import","document":wrong_schema}))["error"]["kind"],
        "invalid_scene_edit"
    );
    let mut missing_event_document = valid_export.clone();
    missing_event_document
        .as_object_mut()
        .unwrap()
        .remove("event_document");
    assert_eq!(
        scene_draft(
            &studio,
            json!({"kind":"import","document":missing_event_document})
        )["error"]["kind"],
        "invalid_args"
    );
    let mut missing_prefix_count = valid_export.clone();
    missing_prefix_count
        .as_object_mut()
        .unwrap()
        .remove("imported_event_actions");
    assert_eq!(
        scene_draft(
            &studio,
            json!({"kind":"import","document":missing_prefix_count})
        )["error"]["kind"],
        "invalid_args"
    );

    // A syntactically valid M6 history from a different original event graph is
    // rejected before it can become the scene's authored-event base.
    let mut different_events = components[5].clone();
    different_events[BYTECODE_OFFSET + 17] = 0;
    let wrong_base_bundle = scene_bundle::bundle(&{
        let mut wrong_components = components.clone();
        wrong_components[5] = different_events;
        wrong_components
    });
    let wrong_base = export_m6_event_document(&studio, &wrong_base_bundle);
    let rejected = assert_scene_open_event_document(&studio, original.clone(), wrong_base);
    assert_eq!(rejected["error"]["kind"], "source_mismatch");

    // M6's custom deserializer rejects host-width numeric values before usize
    // parsing. Keep raw text so u64::MAX remains exact across the WASM trace.
    let mut oversized_cursor = valid_export.clone();
    oversized_cursor["cursor"] = json!(u64::MAX);
    let request = json!({
        "op":"field.scene.draft",
        "args":{"kind":"import","document":oversized_cursor}
    });
    let response = record_wire(&studio, &mut Vec::new(), request.to_string());
    assert_eq!(response["error"]["kind"], "invalid_args");

    let mut hidden_root = valid_export.clone();
    hidden_root["event_document"]["next_id"] = json!(2);
    assert_eq!(
        scene_draft(&studio, json!({"kind":"import","document":hidden_root}))["error"]["kind"],
        "invalid_scene_edit"
    );
    let mut hidden_history_root = valid_export.clone();
    hidden_history_root["event_document"] = event_document.clone();
    assert_eq!(
        scene_draft(
            &studio,
            json!({"kind":"import","document":hidden_history_root})
        )["error"]["kind"],
        "invalid_scene_edit",
        "M6 actions must be promoted into the scene timeline, not hidden in the root"
    );

    let mut non_event_prefix = valid_export.clone();
    non_event_prefix["actions"][0] = json!({"kind":"delete_zone","id":"zone:0"});
    assert_eq!(
        scene_draft(
            &studio,
            json!({"kind":"import","document":non_event_prefix})
        )["error"]["kind"],
        "invalid_scene_edit"
    );

    let mut reused_prefix_start = valid_export.clone();
    reused_prefix_start["allocation_starts"][0] = json!(2);
    assert_eq!(
        scene_draft(
            &studio,
            json!({"kind":"import","document":reused_prefix_start})
        )["error"]["kind"],
        "invalid_scene_edit"
    );

    let adopted = scene_draft(&studio, scene_open_args(original));
    assert_eq!(
        adopted["ok"]["scene"], 2,
        "rejected histories consume no handle"
    );
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

    let seed = Studio::new();
    let opened = scene_draft(&seed, scene_open_args(scene_bundle::valid_bundle()));
    let mut cursor_document = scene_draft(
        &seed,
        json!({"kind":"export","scene":opened["ok"]["scene"]}),
    )["ok"]
        .clone();
    cursor_document["cursor"] = json!(u64::MAX);
    let cursor_wire = json!({
        "op":"field.scene.draft",
        "args":{"kind":"import","document":cursor_document}
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
        Some(33)
    );

    let valid = record(
        &studio,
        &mut trace,
        "field.scene.decode",
        scene_bundle::scene_args(scene_bundle::valid_bundle()),
    );
    assert!(valid.get("ok").is_some(), "synthetic scene decode succeeds");

    let sprite_bundle = scene_bundle::sprite_bundle();
    let sprites = record(
        &studio,
        &mut trace,
        "field.scene.decode",
        scene_args_without_entrances(sprite_bundle.clone()),
    );
    let decoded_sprites = sprites.get("ok").unwrap_or_else(|| {
        panic!(
            "synthetic source sprite decode failed: kind={} message={}",
            sprites["error"]["kind"], sprites["error"]["message"]
        )
    });
    assert_eq!(
        decoded_sprites["sprites"]["resources"][0]["first_frame"]["parts"]
            .as_array()
            .map(Vec::len),
        Some(1)
    );
    let sprite_pc = decoded_sprites["events"]["instructions"][0]["pc"].clone();

    let malformed_sprite = record(
        &studio,
        &mut trace,
        "field.scene.decode",
        scene_args_without_entrances(scene_bundle::malformed_sprite_bundle()),
    );
    assert_eq!(malformed_sprite["error"]["kind"], "invalid_scene_sprite");

    let sprite_open = record(
        &studio,
        &mut trace,
        "field.scene.draft",
        scene_open_args_without_entrances(sprite_bundle),
    );
    let sprite_scene = sprite_open["ok"]["scene"]
        .as_u64()
        .expect("synthetic sprite scene handle") as u32;
    assert_eq!(
        sprite_open["ok"]["state"]["sprite_bindings"][0]["resource"],
        0
    );
    assert_eq!(
        sprite_open["ok"]["state"]["sprite_bindings"][0]["pc"],
        sprite_pc
    );
    let sprite_snapshot = record(
        &studio,
        &mut trace,
        "field.scene.draft",
        json!({"kind":"snapshot","scene":sprite_scene}),
    );
    assert_eq!(
        sprite_snapshot["ok"]["state"]["sprite_bindings"][0]["bank"],
        0
    );
    record(
        &studio,
        &mut trace,
        "field.scene.draft",
        json!({"kind":"close","scene":sprite_scene}),
    );

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
    let imported_scene = imported["ok"]["scene"]
        .as_u64()
        .expect("imported scene handle");
    assert_eq!(imported_scene, 3);
    let recompiled = record(
        &studio,
        &mut trace,
        "field.scene.draft",
        json!({"kind":"compile","scene":imported_scene}),
    );
    assert_eq!(recompiled["ok"]["bundle"], compiled["ok"]["bundle"]);
    record(
        &studio,
        &mut trace,
        "field.scene.draft",
        json!({"kind":"close","scene":imported_scene}),
    );

    // Keep raw request text in the trace: JS JSON.parse/serialize would round
    // u64::MAX before the backend can validate its portable u32 boundary.
    let portable_request = format!(
        "{{\"op\":\"field.scene.draft\",\"args\":{{\"kind\":\"open\",\"input\":{{\"bundle\":{},\"context\":{{\"components\":[{{\"index\":18446744073709551615}}]}},\"source\":null,\"module_sha256\":null}},\"entrances\":[0,1]}}}}",
        serde_json::to_string(&scene_bundle::valid_bundle()).unwrap()
    );
    let portable = record_wire(&studio, &mut trace, portable_request);
    assert_eq!(portable["error"]["kind"], "invalid_args");
    let seed = Studio::new();
    let opened = scene_draft(&seed, scene_open_args(scene_bundle::valid_bundle()));
    let mut cursor_document = scene_draft(
        &seed,
        json!({"kind":"export","scene":opened["ok"]["scene"]}),
    )["ok"]
        .clone();
    cursor_document["cursor"] = json!(u64::MAX);
    let cursor_request = json!({
        "op":"field.scene.draft",
        "args":{"kind":"import","document":cursor_document}
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
    let after_malformed_scene = after_malformed["ok"]["scene"]
        .as_u64()
        .expect("scene opened after malformed inputs");
    assert_eq!(after_malformed_scene, 4);
    let entrance_span_wire = json!({
        "op":"field.scene.draft",
        "args":{"kind":"edit","scene":after_malformed_scene,"revision":0,
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
        json!({"kind":"close","scene":after_malformed_scene}),
    );

    // Cross-field integration: an M6 authored document is the scene's event
    // source, and scene reference repair follows the M6 stable node through the
    // insertion-induced relocation.
    let scene_components = scene_components_with_zone_event();
    let scene_input_bundle = scene_bundle::bundle(&scene_components);
    let event_document = record_m6_event_document(&studio, &mut trace, &scene_input_bundle);
    let mut scene_args = scene_open_args(scene_input_bundle.clone());
    scene_args["event_document"] = event_document.clone();
    let integrated = record(&studio, &mut trace, "field.scene.draft", scene_args);
    assert!(
        integrated["ok"].is_object(),
        "integrated scene opens: {integrated}"
    );
    assert_eq!(integrated["ok"]["state"]["modified"], true);
    let integrated_scene = integrated["ok"]["scene"].as_u64().unwrap() as u32;
    record(
        &studio,
        &mut trace,
        "field.scene.draft",
        json!({"kind":"edit","scene":integrated_scene,"revision":0,
            "action":{"kind":"delete_zone","id":"zone:0"}}),
    );
    let scene_undo = record(
        &studio,
        &mut trace,
        "field.scene.draft",
        json!({"kind":"undo","scene":integrated_scene,"revision":1}),
    );
    assert_eq!(
        scene_undo["ok"]["state"]["zones"].as_array().map(Vec::len),
        Some(2)
    );
    assert_eq!(scene_undo["ok"]["state"]["modified"], true);
    record(
        &studio,
        &mut trace,
        "field.scene.draft",
        json!({"kind":"redo","scene":integrated_scene,"revision":2}),
    );
    let integrated_export = record(
        &studio,
        &mut trace,
        "field.scene.draft",
        json!({"kind":"export","scene":integrated_scene}),
    );
    assert_eq!(integrated_export["ok"]["schema_version"], 4);
    assert_eq!(integrated_export["ok"]["imported_event_actions"], 2);
    assert_empty_m6_root(&integrated_export["ok"]);
    assert_eq!(
        integrated_export["ok"]["actions"][0]["action"],
        event_document["actions"][0]
    );
    assert_eq!(
        integrated_export["ok"]["actions"][1]["action"],
        event_document["actions"][1]
    );
    let integrated_compile = record(
        &studio,
        &mut trace,
        "field.scene.draft",
        json!({"kind":"compile","scene":integrated_scene}),
    );
    assert!(integrated_compile["ok"].is_object());
    let call_pc = integrated_compile["ok"]["events"]["bindings"]["pc:16"]
        .as_u64()
        .expect("relocated source call retains its stable M6 node identity")
        as usize;
    let event_bytes = byte_array(&integrated_compile["ok"]["events"]["bytes"]);
    assert!(call_pc > 16);
    assert_eq!(event_bytes[BYTECODE_OFFSET + call_pc], 0x0a);
    assert_eq!(event_bytes[BYTECODE_OFFSET + call_pc + 1], 0);
    let integrated_snapshot = record(
        &studio,
        &mut trace,
        "field.scene.draft",
        json!({"kind":"snapshot","scene":integrated_scene}),
    );
    assert_eq!(
        integrated_snapshot["ok"]["state"]["events"],
        integrated_compile["ok"]["events"]
    );
    // A compiled scene bundle flows through the existing package boundary.
    // The second explicitly unchanged slot proves that the scene edit does not
    // turn unrelated package payloads into a different authored result.
    let scene_disc = json!({"serial":"SYNTHETIC-M7-SCENE","sha256":"a".repeat(64)});
    let package_args = json!({
        "spec":{"id":"m7-scene-history-integration","discs":[scene_disc],"requires":[],"overrides":[]},
        "files":[
            {"disc":"SYNTHETIC-M7-SCENE","slot":23,"source":scene_input_bundle,
                "result":integrated_compile["ok"]["bundle"]},
            {"disc":"SYNTHETIC-M7-SCENE","slot":24,"source":scene_input_bundle,
                "result":scene_input_bundle}
        ]
    });
    let package_first = record(&studio, &mut trace, "package.build", package_args.clone());
    let package_second = record(&studio, &mut trace, "package.build", package_args);
    assert!(
        package_first["ok"].is_object(),
        "scene package build succeeds: {package_first}"
    );
    assert_eq!(package_first["ok"]["bytes"], package_second["ok"]["bytes"]);
    assert_eq!(
        package_first["ok"]["identity"],
        package_second["ok"]["identity"]
    );
    assert_eq!(
        package_first["ok"]["manifest"]["files"]
            .as_array()
            .map(Vec::len),
        Some(2)
    );
    assert!(
        package_first["ok"]["manifest"]["files"]
            .as_array()
            .unwrap()
            .iter()
            .all(|file| file["kind"] == "delta")
    );
    let manifest_files = package_first["ok"]["manifest"]["files"].as_array().unwrap();
    assert_eq!(manifest_files[0]["slot"], 23);
    assert_eq!(
        manifest_files[0]["source_sha256"],
        integrated_compile["ok"]["source_sha256"]
    );
    assert_eq!(
        manifest_files[0]["result_sha256"],
        integrated_compile["ok"]["bundle_sha256"]
    );
    assert_eq!(manifest_files[1]["slot"], 24);
    assert_eq!(
        manifest_files[1]["source_sha256"],
        manifest_files[1]["result_sha256"]
    );
    let package_inspected = record(
        &studio,
        &mut trace,
        "package.inspect",
        json!({"bytes":package_first["ok"]["bytes"]}),
    );
    assert_eq!(
        package_inspected["ok"]["identity"],
        package_first["ok"]["identity"]
    );
    let package_set_a = record(
        &studio,
        &mut trace,
        "package.bundle",
        json!({"packages":[package_first["ok"]["bytes"]]}),
    );
    let package_set_b = record(
        &studio,
        &mut trace,
        "package.bundle",
        json!({"packages":[package_second["ok"]["bytes"]]}),
    );
    assert_eq!(package_set_a["ok"]["bytes"], package_set_b["ok"]["bytes"]);
    assert_eq!(
        package_set_a["ok"]["identities"],
        package_set_b["ok"]["identities"]
    );

    // The imported M6 prefix itself participates in scene undo/redo. Undoing
    // every transaction restores the exact original bundle; replaying the M6
    // prefix and scene edit restores the previously compiled candidate.
    let undo_zone = record(
        &studio,
        &mut trace,
        "field.scene.draft",
        json!({"kind":"undo","scene":integrated_scene,"revision":3}),
    );
    let undo_insert = record(
        &studio,
        &mut trace,
        "field.scene.draft",
        json!({"kind":"undo","scene":integrated_scene,"revision":4}),
    );
    let undo_new_event = record(
        &studio,
        &mut trace,
        "field.scene.draft",
        json!({"kind":"undo","scene":integrated_scene,"revision":5}),
    );
    assert_eq!(
        undo_zone["ok"]["state"]["zones"].as_array().map(Vec::len),
        Some(2)
    );
    assert!(undo_insert["ok"]["state"]["events"]["bindings"]["new:3"].is_null());
    assert_eq!(undo_new_event["ok"]["state"]["modified"], false);
    assert_eq!(undo_new_event["ok"]["state"]["next_event_id"], 4);
    assert!(undo_new_event["ok"]["state"]["events"]["bindings"]["new:1"].is_null());
    let original_scene_compile = record(
        &studio,
        &mut trace,
        "field.scene.draft",
        json!({"kind":"compile","scene":integrated_scene}),
    );
    assert_eq!(
        byte_array(&original_scene_compile["ok"]["bundle"]),
        scene_input_bundle
    );
    let redo_new_event = record(
        &studio,
        &mut trace,
        "field.scene.draft",
        json!({"kind":"redo","scene":integrated_scene,"revision":6}),
    );
    let redo_insert = record(
        &studio,
        &mut trace,
        "field.scene.draft",
        json!({"kind":"redo","scene":integrated_scene,"revision":7}),
    );
    let redo_zone = record(
        &studio,
        &mut trace,
        "field.scene.draft",
        json!({"kind":"redo","scene":integrated_scene,"revision":8}),
    );
    assert!(redo_new_event["ok"].is_object());
    assert!(redo_insert["ok"].is_object());
    assert!(redo_zone["ok"].is_object());
    let replayed_candidate = record(
        &studio,
        &mut trace,
        "field.scene.draft",
        json!({"kind":"compile","scene":integrated_scene}),
    );
    assert_eq!(
        replayed_candidate["ok"]["bundle"],
        integrated_compile["ok"]["bundle"]
    );
    assert_eq!(
        replayed_candidate["ok"]["events"],
        integrated_compile["ok"]["events"]
    );
    record(
        &studio,
        &mut trace,
        "field.scene.draft",
        json!({"kind":"close","scene":integrated_scene}),
    );
    let integrated_import = record(
        &studio,
        &mut trace,
        "field.scene.draft",
        json!({"kind":"import","document":integrated_export["ok"]}),
    );
    let integrated_import_scene = integrated_import["ok"]["scene"].as_u64().unwrap() as u32;
    let integrated_recompile = record(
        &studio,
        &mut trace,
        "field.scene.draft",
        json!({"kind":"compile","scene":integrated_import_scene}),
    );
    assert_eq!(
        integrated_recompile["ok"]["bundle"],
        integrated_compile["ok"]["bundle"]
    );
    record(
        &studio,
        &mut trace,
        "field.scene.draft",
        json!({"kind":"close","scene":integrated_import_scene}),
    );

    let mut wrong_events = scene_components[5].clone();
    wrong_events[BYTECODE_OFFSET + 17] = 0;
    let mut wrong_components = scene_components.clone();
    wrong_components[5] = wrong_events;
    let wrong_bundle = scene_bundle::bundle(&wrong_components);
    let wrong_document = record_m6_event_document(&studio, &mut trace, &wrong_bundle);
    let mut wrong_open = scene_open_args(scene_input_bundle.clone());
    wrong_open["event_document"] = wrong_document;
    let wrong_source = record(&studio, &mut trace, "field.scene.draft", wrong_open);
    assert_eq!(wrong_source["error"]["kind"], "source_mismatch");

    let mut schema99 = integrated_export["ok"].clone();
    schema99["schema_version"] = json!(99);
    let wrong_schema = record(
        &studio,
        &mut trace,
        "field.scene.draft",
        json!({"kind":"import","document":schema99}),
    );
    assert_eq!(wrong_schema["error"]["kind"], "invalid_scene_edit");
    let mut missing_event = integrated_export["ok"].clone();
    missing_event
        .as_object_mut()
        .unwrap()
        .remove("event_document");
    let missing = record(
        &studio,
        &mut trace,
        "field.scene.draft",
        json!({"kind":"import","document":missing_event}),
    );
    assert_eq!(missing["error"]["kind"], "invalid_args");

    let mut hidden_root = integrated_export["ok"].clone();
    hidden_root["event_document"]["next_id"] = json!(2);
    let hidden = record(
        &studio,
        &mut trace,
        "field.scene.draft",
        json!({"kind":"import","document":hidden_root}),
    );
    assert_eq!(hidden["error"]["kind"], "invalid_scene_edit");
    let mut hidden_history = integrated_export["ok"].clone();
    hidden_history["event_document"] = event_document.clone();
    let hidden_history_reply = record(
        &studio,
        &mut trace,
        "field.scene.draft",
        json!({"kind":"import","document":hidden_history}),
    );
    assert_eq!(hidden_history_reply["error"]["kind"], "invalid_scene_edit");
    let mut bad_prefix = integrated_export["ok"].clone();
    bad_prefix["actions"][0] = json!({"kind":"delete_zone","id":"zone:0"});
    let prefix = record(
        &studio,
        &mut trace,
        "field.scene.draft",
        json!({"kind":"import","document":bad_prefix}),
    );
    assert_eq!(prefix["error"]["kind"], "invalid_scene_edit");

    // Out-of-table source zone selectors are Unknown references. A validated
    // imported M6 history that preserves the numeric value remains inspectable,
    // while ordinary edits and namespace changes refuse to guess its target.
    let mut unknown_components = scene_components_with_zone_event();
    unknown_components[8].truncate(24);
    let unknown_bundle = scene_bundle::bundle(&unknown_components);
    let (unknown_event_bytes, unknown_logical_size) = source_event_component(&unknown_bundle);
    let unknown_graph = record(
        &studio,
        &mut trace,
        "field.decode",
        json!({"bytes":unknown_event_bytes,"declared_size":unknown_logical_size}),
    )["ok"]
        .clone();
    let unknown_draft = record(
        &studio,
        &mut trace,
        "field.draft.open",
        json!({"graph":unknown_graph}),
    )["ok"]["draft"]
        .clone();
    let unknown_source_edit = record(
        &studio,
        &mut trace,
        "field.draft.edit",
        json!({"draft":unknown_draft,"revision":0,
            "action":{"kind":"set_operand","node":"pc:16","operand":0,
                "value":{"kind":"literal","value":1,"immediate":null}}}),
    );
    assert!(
        unknown_source_edit["ok"].is_object(),
        "M6 retains the raw source selector: {unknown_source_edit}"
    );
    let unknown_document = record(
        &studio,
        &mut trace,
        "field.draft.export",
        json!({"draft":unknown_draft}),
    )["ok"]
        .clone();
    assert_eq!(
        unknown_document["actions"].as_array().map(Vec::len),
        Some(1)
    );
    let mut unknown_open_args = scene_open_args(unknown_bundle);
    unknown_open_args["event_document"] = unknown_document;
    let unknown_open = record(&studio, &mut trace, "field.scene.draft", unknown_open_args);
    assert!(
        unknown_open["ok"].is_object(),
        "unknown source history opens: {unknown_open}"
    );
    let unknown_scene = unknown_open["ok"]["scene"].as_u64().unwrap() as u32;
    assert!(
        unknown_open["ok"]["state"]["reference_diagnostics"]
            .as_array()
            .is_some_and(|diagnostics| diagnostics
                .iter()
                .any(|d| d.as_str().is_some_and(|s| s.contains("pc:16"))))
    );
    let ordinary_unknown_edit = record(
        &studio,
        &mut trace,
        "field.scene.draft",
        json!({"kind":"edit","scene":unknown_scene,"revision":0,
            "action":{"kind":"event","action":{"kind":"set_operand","node":"pc:16","operand":0,
                "value":{"kind":"literal","value":1,"immediate":null}}}}),
    );
    assert_eq!(
        ordinary_unknown_edit["error"]["kind"],
        "unsupported_scene_reference"
    );
    let unknown_namespace_change = record(
        &studio,
        &mut trace,
        "field.scene.draft",
        json!({"kind":"edit","scene":unknown_scene,"revision":0,
            "action":{"kind":"delete_zone","id":"zone:0"}}),
    );
    assert_eq!(
        unknown_namespace_change["error"]["kind"],
        "unsupported_scene_reference"
    );
    record(
        &studio,
        &mut trace,
        "field.scene.draft",
        json!({"kind":"close","scene":unknown_scene}),
    );

    let mut overflow_doc = event_document.clone();
    overflow_doc["cursor"] = json!(u64::MAX);
    let overflow_wire = json!({
        "op":"field.scene.draft",
        "args":{"kind":"open","input":scene_bundle::scene_args(scene_bundle::bundle(&scene_components))["input"],
            "entrances":[0,1],"event_document":overflow_doc}
    })
    .to_string();
    let overflow = record_wire(&studio, &mut trace, overflow_wire);
    assert_eq!(overflow["error"]["kind"], "invalid_args");

    let mut bad_target_doc = event_document.clone();
    let targets = bad_target_doc["actions"][1]["targets"]
        .as_object_mut()
        .expect("insert action exports target map");
    targets.insert("4294967296".into(), json!("pc:20"));
    let bad_target_wire = json!({
        "op":"field.scene.draft",
        "args":{"kind":"open","input":scene_bundle::scene_args(scene_bundle::bundle(&scene_components))["input"],
            "entrances":[0,1],"event_document":bad_target_doc}
    })
    .to_string();
    let bad_target = record_wire(&studio, &mut trace, bad_target_wire);
    assert_eq!(bad_target["error"]["kind"], "invalid_args");

    // Scene-authored M6 operations use the same typed JSON command protocol.
    // The alternate insertion retains the branch target to new:2 while the
    // event allocator advances past the undone new:3 identity.
    let event_bundle = scene_bundle::bundle(&scene_components_with_zone_event());
    let event_opened = record(
        &studio,
        &mut trace,
        "field.scene.draft",
        scene_open_args(event_bundle),
    );
    let event_scene = event_opened["ok"]["scene"].as_u64().unwrap() as u32;
    let new_event = record(
        &studio,
        &mut trace,
        "field.scene.draft",
        json!({"kind":"edit","scene":event_scene,"revision":0,
        "action":{"kind":"event","action":{"kind":"new_event","actor":0,"event":1,
            "nodes":[
                {"bytes":[1,0,0],"targets":{"0":"new:2"}},
                {"bytes":[0],"targets":{}}
            ]}}}),
    );
    assert_eq!(new_event["ok"]["state"]["next_event_id"], 3);
    let first_insert = record(
        &studio,
        &mut trace,
        "field.scene.draft",
        json!({"kind":"edit","scene":event_scene,"revision":1,
            "action":{"kind":"event","action":{"kind":"insert_after","after":"pc:15",
                "bytes":[14],"targets":{}}}}),
    );
    assert_eq!(first_insert["ok"]["state"]["next_event_id"], 4);
    record(
        &studio,
        &mut trace,
        "field.scene.draft",
        json!({"kind":"undo","scene":event_scene,"revision":2}),
    );
    let alternate_insert = record(
        &studio,
        &mut trace,
        "field.scene.draft",
        json!({"kind":"edit","scene":event_scene,"revision":3,
            "action":{"kind":"event","action":{"kind":"insert_after","after":"pc:15",
                "bytes":[14],"targets":{}}}}),
    );
    assert_eq!(alternate_insert["ok"]["state"]["next_event_id"], 5);
    let event_snapshot = record(
        &studio,
        &mut trace,
        "field.scene.draft",
        json!({"kind":"snapshot","scene":event_scene}),
    );
    let snapshot_bindings = event_snapshot["ok"]["state"]["events"]["bindings"]
        .as_object()
        .expect("snapshot exposes stable event bindings");
    assert!(snapshot_bindings.contains_key("new:1"));
    assert!(snapshot_bindings.contains_key("new:2"));
    assert!(snapshot_bindings.contains_key("new:4"));
    assert!(!snapshot_bindings.contains_key("new:3"));
    assert_eq!(event_snapshot["ok"]["state"]["next_event_id"], 5);
    let event_export = record(
        &studio,
        &mut trace,
        "field.scene.draft",
        json!({"kind":"export","scene":event_scene}),
    );
    assert_eq!(event_export["ok"]["schema_version"], 4);
    assert_eq!(event_export["ok"]["imported_event_actions"], 0);
    assert_empty_m6_root(&event_export["ok"]);
    assert_eq!(event_export["ok"]["event_allocation_starts"], json!([1, 4]));
    assert_eq!(event_export["ok"]["next_event_id"], 5);
    let event_compile = record(
        &studio,
        &mut trace,
        "field.scene.draft",
        json!({"kind":"compile","scene":event_scene}),
    );
    let event_bindings = &event_compile["ok"]["events"]["bindings"];
    let branch_pc = event_bindings["new:1"].as_u64().unwrap() as usize;
    let target_pc = event_bindings["new:2"].as_u64().unwrap();
    let event_bytes = byte_array(&event_compile["ok"]["events"]["bytes"]);
    assert_eq!(event_bytes[BYTECODE_OFFSET + branch_pc], 1);
    assert_eq!(
        u16::from_le_bytes([
            event_bytes[BYTECODE_OFFSET + branch_pc + 1],
            event_bytes[BYTECODE_OFFSET + branch_pc + 2]
        ]) as u64,
        target_pc
    );
    record(
        &studio,
        &mut trace,
        "field.scene.draft",
        json!({"kind":"close","scene":event_scene}),
    );

    let mut reused_event_start = event_export["ok"].clone();
    reused_event_start["event_allocation_starts"][1] = json!(1);
    let reused_start = record(
        &studio,
        &mut trace,
        "field.scene.draft",
        json!({"kind":"import","document":reused_event_start}),
    );
    assert_eq!(reused_start["error"]["kind"], "invalid_scene_edit");

    let mut too_small_event_watermark = event_export["ok"].clone();
    too_small_event_watermark["next_event_id"] = json!(4);
    let reused_watermark = record(
        &studio,
        &mut trace,
        "field.scene.draft",
        json!({"kind":"import","document":too_small_event_watermark}),
    );
    assert_eq!(reused_watermark["error"]["kind"], "invalid_scene_edit");

    let mut overflow_event_start = event_export["ok"].clone();
    overflow_event_start["event_allocation_starts"][1] = json!(u64::MAX);
    let overflow_start_wire = json!({
        "op":"field.scene.draft",
        "args":{"kind":"import","document":overflow_event_start}
    })
    .to_string();
    let overflow_start = record_wire(&studio, &mut trace, overflow_start_wire);
    assert_eq!(overflow_start["error"]["kind"], "invalid_args");

    let mut overflow_event_watermark = event_export["ok"].clone();
    overflow_event_watermark["next_event_id"] = json!(u64::MAX);
    let overflow_watermark_wire = json!({
        "op":"field.scene.draft",
        "args":{"kind":"import","document":overflow_event_watermark}
    })
    .to_string();
    let overflow_watermark = record_wire(&studio, &mut trace, overflow_watermark_wire);
    assert_eq!(overflow_watermark["error"]["kind"], "invalid_args");

    let event_imported = record(
        &studio,
        &mut trace,
        "field.scene.draft",
        json!({"kind":"import","document":event_export["ok"]}),
    );
    let imported_event_scene = event_imported["ok"]["scene"].as_u64().unwrap() as u32;
    let event_recompiled = record(
        &studio,
        &mut trace,
        "field.scene.draft",
        json!({"kind":"compile","scene":imported_event_scene}),
    );
    assert_eq!(
        event_recompiled["ok"]["bundle"],
        event_compile["ok"]["bundle"]
    );
    assert_eq!(
        event_recompiled["ok"]["events"]["bytes"],
        event_compile["ok"]["events"]["bytes"]
    );
    record(
        &studio,
        &mut trace,
        "field.scene.draft",
        json!({"kind":"close","scene":imported_event_scene}),
    );

    // Placement commands use the same typed scene protocol and remain bound
    // to an explicit actor/event owner. The successful route edits floor X/Z
    // while leaving the engine-derived Y untouched, then checks history and
    // export/import through the exact native/WASM request stream.
    let placement_bundle =
        placement_scene_bundle(2, &[(0, 0, 1)], &[0xff, 0x19, 1, 0, 2, 0, 0xc0, 0x00]);
    let placement_open = record(
        &studio,
        &mut trace,
        "field.scene.draft",
        placement_open_args(placement_bundle),
    );
    let placement_scene = placement_open["ok"]["scene"].as_u64().unwrap() as u32;
    let described = record(
        &studio,
        &mut trace,
        "field.scene.draft",
        json!({"kind":"placement","scene":placement_scene,"node":"pc:1"}),
    );
    assert_eq!(described["ok"]["handler"], "field_event_place");
    assert_eq!(
        described["ok"]["owners"],
        json!([{"actor":"descriptor:0","event":0}])
    );
    assert_eq!(described["ok"]["position"][1]["kind"], "floor");

    let owner_mismatch = record(
        &studio,
        &mut trace,
        "field.scene.draft",
        json!({"kind":"edit","scene":placement_scene,"revision":0,
        "action":{"kind":"set_placement","change":{
            "node":"pc:1","actor":"descriptor:1","event":0,
            "position":[9,null,null],"layer":null
        }}}),
    );
    assert_eq!(
        owner_mismatch["error"]["kind"],
        "unsupported_scene_placement"
    );
    let floor_y = record(
        &studio,
        &mut trace,
        "field.scene.draft",
        json!({"kind":"edit","scene":placement_scene,"revision":0,
        "action":{"kind":"set_placement","change":{
            "node":"pc:1","actor":"descriptor:0","event":0,
            "position":[null,99,null],"layer":null
        }}}),
    );
    assert_eq!(floor_y["error"]["kind"], "unsupported_scene_placement");
    let overflow = record_wire(
        &studio,
        &mut trace,
        format!(
            "{{\"op\":\"field.scene.draft\",\"args\":{{\"kind\":\"edit\",\"scene\":{placement_scene},\"revision\":0,\"action\":{{\"kind\":\"set_placement\",\"change\":{{\"node\":\"pc:1\",\"actor\":\"descriptor:0\",\"event\":0,\"position\":[32768,null,null],\"layer\":null}}}}}}}}"
        ),
    );
    assert_eq!(overflow["error"]["kind"], "invalid_args");

    let edited_placement = record(
        &studio,
        &mut trace,
        "field.scene.draft",
        json!({"kind":"edit","scene":placement_scene,"revision":0,
        "action":{"kind":"set_placement","change":{
            "node":"pc:1","actor":"descriptor:0","event":0,
            "position":[-123,null,456],"layer":null
        }}}),
    );
    assert_eq!(edited_placement["ok"]["state"]["revision"], 1);
    let edited_binding = record(
        &studio,
        &mut trace,
        "field.scene.draft",
        json!({"kind":"placement","scene":placement_scene,"node":"pc:1"}),
    );
    assert_eq!(
        edited_binding["ok"]["position"][0],
        json!({"kind":"immediate","value":-123})
    );
    assert_eq!(edited_binding["ok"]["position"][1]["kind"], "floor");
    assert_eq!(
        edited_binding["ok"]["position"][2],
        json!({"kind":"immediate","value":456})
    );
    record(
        &studio,
        &mut trace,
        "field.scene.draft",
        json!({"kind":"undo","scene":placement_scene,"revision":1}),
    );
    record(
        &studio,
        &mut trace,
        "field.scene.draft",
        json!({"kind":"redo","scene":placement_scene,"revision":2}),
    );
    let placement_compile = record(
        &studio,
        &mut trace,
        "field.scene.draft",
        json!({"kind":"compile","scene":placement_scene}),
    );
    let compiled_event_bytes = byte_array(&placement_compile["ok"]["events"]["bytes"]);
    let placement_code_offset = 132 + 2 * 64;
    assert_eq!(
        i16::from_le_bytes([
            compiled_event_bytes[placement_code_offset + 2],
            compiled_event_bytes[placement_code_offset + 3]
        ]),
        -123
    );
    assert_eq!(
        u16::from_le_bytes([
            compiled_event_bytes[placement_code_offset + 4],
            compiled_event_bytes[placement_code_offset + 5]
        ]),
        456
    );
    let placement_export = record(
        &studio,
        &mut trace,
        "field.scene.draft",
        json!({"kind":"export","scene":placement_scene}),
    );
    record(
        &studio,
        &mut trace,
        "field.scene.draft",
        json!({"kind":"close","scene":placement_scene}),
    );

    let variable_bundle =
        placement_scene_bundle(1, &[(0, 0, 1)], &[0xff, 0x19, 5, 0, 9, 0, 0, 0x00]);
    let variable_open = record(
        &studio,
        &mut trace,
        "field.scene.draft",
        placement_open_args(variable_bundle),
    );
    let variable_scene = variable_open["ok"]["scene"].as_u64().unwrap() as u32;
    let variable_binding = record(
        &studio,
        &mut trace,
        "field.scene.draft",
        json!({"kind":"placement","scene":variable_scene,"node":"pc:1"}),
    );
    assert_eq!(variable_binding["ok"]["can_transform"], false);
    let variable_edit = record(
        &studio,
        &mut trace,
        "field.scene.draft",
        json!({"kind":"edit","scene":variable_scene,"revision":0,
        "action":{"kind":"set_placement","change":{
            "node":"pc:1","actor":"descriptor:0","event":0,
            "position":[100,null,null],"layer":null
        }}}),
    );
    assert_eq!(
        variable_edit["error"]["kind"],
        "unsupported_scene_placement"
    );
    record(
        &studio,
        &mut trace,
        "field.scene.draft",
        json!({"kind":"close","scene":variable_scene}),
    );

    let pinned_bundle = placement_scene_bundle(
        1,
        &[(0, 0, 1), (0, 1, 3)],
        &[0xff, 0x1d, 1, 1, 0x1d, 2, 0, 3, 0, 0, 0x00],
    );
    let pinned_open = record(
        &studio,
        &mut trace,
        "field.scene.draft",
        placement_open_args(pinned_bundle),
    );
    let pinned_scene = pinned_open["ok"]["scene"].as_u64().unwrap() as u32;
    let pinned_binding = record(
        &studio,
        &mut trace,
        "field.scene.draft",
        json!({"kind":"placement","scene":pinned_scene,"node":"pc:1"}),
    );
    assert_eq!(pinned_binding["ok"]["can_transform"], false);
    let pinned_edit = record(
        &studio,
        &mut trace,
        "field.scene.draft",
        json!({"kind":"edit","scene":pinned_scene,"revision":0,
        "action":{"kind":"set_placement","change":{
            "node":"pc:1","actor":"descriptor:0","event":0,
            "position":[8,null,null],"layer":null
        }}}),
    );
    assert_eq!(pinned_edit["error"]["kind"], "unsupported_scene_placement");
    record(
        &studio,
        &mut trace,
        "field.scene.draft",
        json!({"kind":"close","scene":pinned_scene}),
    );

    let placement_imported = record(
        &studio,
        &mut trace,
        "field.scene.draft",
        json!({"kind":"import","document":placement_export["ok"]}),
    );
    let imported_placement_scene = placement_imported["ok"]["scene"].as_u64().unwrap() as u32;
    let placement_recompiled = record(
        &studio,
        &mut trace,
        "field.scene.draft",
        json!({"kind":"compile","scene":imported_placement_scene}),
    );
    assert_eq!(
        placement_recompiled["ok"]["bundle"],
        placement_compile["ok"]["bundle"]
    );
    record(
        &studio,
        &mut trace,
        "field.scene.draft",
        json!({"kind":"close","scene":imported_placement_scene}),
    );

    // Actor rows use the same public scene protocol. This fixture is copied
    // from the passing scene_actors placement test: one source actor owns a
    // qualified place_at_height event at pc:1 and a terminal slot at pc:8.
    // Its entrance list is empty because the code begins at pc:1, not at an
    // indexed 14-byte entrance prefix.
    let place_at_height = primary_opcode("field_event_place_at_height");
    let actor_bundle = placement_scene_bundle(
        1,
        &[(0, 0, 1)],
        &[0xff, place_at_height, 0xfe, 0xff, 0x0d, 0, 0xf9, 0xff, 0],
    );
    let actor_open = record(
        &studio,
        &mut trace,
        "field.scene.draft",
        placement_open_args(actor_bundle.clone()),
    );
    assert!(
        actor_open["ok"].is_object(),
        "actor fixture opens: {actor_open}"
    );
    let actor_scene = actor_open["ok"]["scene"].as_u64().unwrap() as u32;
    let first_copy = record(
        &studio,
        &mut trace,
        "field.scene.draft",
        json!({"kind":"edit","scene":actor_scene,"revision":0,
            "action":{"kind":"duplicate_actor","id":"descriptor:0"}}),
    );
    assert_eq!(
        first_copy["ok"]["state"]["actors"][1]["id"],
        "scene:1:actor"
    );
    assert_eq!(first_copy["ok"]["state"]["next_event_id"], 3);
    let actor_transform = record(
        &studio,
        &mut trace,
        "field.scene.draft",
        json!({"kind":"edit","scene":actor_scene,"revision":1,
            "action":{"kind":"set_placement","change":{
                "node":"new:1","actor":"scene:1:actor","event":0,
                "position":[10,20,30],"layer":null}}}),
    );
    assert_eq!(actor_transform["ok"]["state"]["revision"], 2);
    record(
        &studio,
        &mut trace,
        "field.scene.draft",
        json!({"kind":"undo","scene":actor_scene,"revision":2}),
    );
    record(
        &studio,
        &mut trace,
        "field.scene.draft",
        json!({"kind":"undo","scene":actor_scene,"revision":3}),
    );
    let alternate_actor = record(
        &studio,
        &mut trace,
        "field.scene.draft",
        json!({"kind":"edit","scene":actor_scene,"revision":4,
            "action":{"kind":"duplicate_actor","id":"descriptor:0"}}),
    );
    assert_eq!(
        alternate_actor["ok"]["state"]["actors"][1]["id"],
        "scene:2:actor"
    );
    assert_eq!(alternate_actor["ok"]["state"]["next_event_id"], 5);
    let authored_actor_event = record(
        &studio,
        &mut trace,
        "field.scene.draft",
        json!({"kind":"edit","scene":actor_scene,"revision":5,
        "action":{"kind":"actor_event","actor":"scene:2:actor","event":1,
            "nodes":[
                {"bytes":[1,0,0],"targets":{"0":"new:6"}},
                {"bytes":[0],"targets":{}}
            ]}}),
    );
    assert_eq!(authored_actor_event["ok"]["state"]["next_event_id"], 7);
    assert_eq!(
        authored_actor_event["ok"]["state"]["actors"][1]["id"],
        "scene:2:actor"
    );
    // Numeric M6 NewEvent addresses the immutable original source actor rows and may not
    // silently target a virtual actor row. The stable ActorEvent route above is
    // the supported way to author scripts on the new actor.
    let numeric_virtual_event = record(
        &studio,
        &mut trace,
        "field.scene.draft",
        json!({"kind":"edit","scene":actor_scene,"revision":6,
            "action":{"kind":"event","action":{"kind":"new_event","actor":1,"event":2,
                "nodes":[{"bytes":[14],"targets":{}},{"bytes":[0],"targets":{}}]}}}),
    );
    assert_eq!(
        numeric_virtual_event["error"]["kind"],
        "unsupported_scene_edit"
    );
    let malformed_actor_nodes = record(
        &studio,
        &mut trace,
        "field.scene.draft",
        json!({"kind":"edit","scene":actor_scene,"revision":6,
            "action":{"kind":"actor_event","actor":"scene:2:actor","event":2,
                "nodes":[{"bytes":[256],"targets":{}}]}}),
    );
    assert_eq!(malformed_actor_nodes["error"]["kind"], "invalid_args");
    let malformed_actor_targets = record_wire(
        &studio,
        &mut trace,
        format!(
            "{{\"op\":\"field.scene.draft\",\"args\":{{\"kind\":\"edit\",\"scene\":{actor_scene},\"revision\":6,\"action\":{{\"kind\":\"actor_event\",\"actor\":\"scene:2:actor\",\"event\":2,\"nodes\":[{{\"bytes\":[1,0,0],\"targets\":{{\"4294967296\":\"new:6\"}}}},{{\"bytes\":[0],\"targets\":{{}}}}]}}}}}}"
        ),
    );
    assert_eq!(malformed_actor_targets["error"]["kind"], "invalid_args");
    let actor_export = record(
        &studio,
        &mut trace,
        "field.scene.draft",
        json!({"kind":"export","scene":actor_scene}),
    );
    assert_eq!(actor_export["ok"]["next_id"], 3);
    assert_eq!(actor_export["ok"]["next_event_id"], 7);
    let actor_compile = record(
        &studio,
        &mut trace,
        "field.scene.draft",
        json!({"kind":"compile","scene":actor_scene}),
    );
    let actor_bindings = &actor_compile["ok"]["events"]["bindings"];
    assert!(!actor_bindings.as_object().unwrap().contains_key("new:1"));
    assert!(!actor_bindings.as_object().unwrap().contains_key("new:2"));
    assert!(actor_bindings.as_object().unwrap().contains_key("new:3"));
    assert!(actor_bindings.as_object().unwrap().contains_key("new:4"));
    assert!(actor_bindings.as_object().unwrap().contains_key("new:5"));
    assert!(actor_bindings.as_object().unwrap().contains_key("new:6"));
    let actor_event_bytes = byte_array(&actor_compile["ok"]["events"]["bytes"]);
    let actor_bytecode_offset = actor_compile["ok"]["events"]["graph"]["bytecode_offset"]
        .as_u64()
        .expect("compiled actor event graph exposes its bytecode base")
        as usize;
    assert_eq!(actor_bytecode_offset, 132 + 2 * 64);
    let actor_branch_pc = actor_bindings["new:5"].as_u64().unwrap() as usize;
    let actor_branch_target = actor_bindings["new:6"].as_u64().unwrap() as usize;
    assert_eq!(
        actor_event_bytes[actor_bytecode_offset + actor_branch_pc],
        1
    );
    assert_eq!(
        u16::from_le_bytes([
            actor_event_bytes[actor_bytecode_offset + actor_branch_pc + 1],
            actor_event_bytes[actor_bytecode_offset + actor_branch_pc + 2],
        ]) as usize,
        actor_branch_target
    );
    record(
        &studio,
        &mut trace,
        "field.scene.draft",
        json!({"kind":"close","scene":actor_scene}),
    );
    let actor_import = record(
        &studio,
        &mut trace,
        "field.scene.draft",
        json!({"kind":"import","document":actor_export["ok"]}),
    );
    let imported_actor_scene = actor_import["ok"]["scene"].as_u64().unwrap() as u32;
    let actor_recompile = record(
        &studio,
        &mut trace,
        "field.scene.draft",
        json!({"kind":"compile","scene":imported_actor_scene}),
    );
    assert_eq!(
        actor_recompile["ok"]["bundle"],
        actor_compile["ok"]["bundle"]
    );
    assert_eq!(
        actor_recompile["ok"]["events"]["bindings"],
        actor_compile["ok"]["events"]["bindings"]
    );
    record(
        &studio,
        &mut trace,
        "field.scene.draft",
        json!({"kind":"close","scene":imported_actor_scene}),
    );

    // AddActor/DeleteActor and actor-zero protection use a terminal-only source
    // row, matching the passing actor unit fixture and requiring no fake script.
    let crud_bundle = placement_scene_bundle(1, &[(0, 0, 1)], &[0xff, 0]);
    let crud_open = record(
        &studio,
        &mut trace,
        "field.scene.draft",
        placement_open_args(crud_bundle.clone()),
    );
    let crud_scene = crud_open["ok"]["scene"].as_u64().unwrap() as u32;
    let added_actor = record(
        &studio,
        &mut trace,
        "field.scene.draft",
        json!({"kind":"edit","scene":crud_scene,"revision":0,
            "action":{"kind":"add_actor","template":"descriptor:0",
                "position":[11,22,33],"rotation":[3,4,5]}}),
    );
    assert_eq!(
        added_actor["ok"]["state"]["actors"][1]["id"],
        "scene:1:actor"
    );
    let actor_zero_delete = record(
        &studio,
        &mut trace,
        "field.scene.draft",
        json!({"kind":"edit","scene":crud_scene,"revision":1,
            "action":{"kind":"delete_actor","id":"descriptor:0"}}),
    );
    assert_eq!(actor_zero_delete["error"]["kind"], "unsupported_scene_edit");
    let delete_actor = record(
        &studio,
        &mut trace,
        "field.scene.draft",
        json!({"kind":"edit","scene":crud_scene,"revision":1,
            "action":{"kind":"delete_actor","id":"scene:1:actor"}}),
    );
    assert_eq!(
        delete_actor["ok"]["state"]["actors"]
            .as_array()
            .unwrap()
            .len(),
        1
    );
    let crud_compile = record(
        &studio,
        &mut trace,
        "field.scene.draft",
        json!({"kind":"compile","scene":crud_scene}),
    );
    assert_eq!(byte_array(&crud_compile["ok"]["bundle"]), crud_bundle);
    record(
        &studio,
        &mut trace,
        "field.scene.draft",
        json!({"kind":"close","scene":crud_scene}),
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
