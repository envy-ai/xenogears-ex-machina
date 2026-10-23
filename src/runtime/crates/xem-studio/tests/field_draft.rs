use serde_json::{Value, json};
use xem_studio::Studio;

const BYTECODE_OFFSET: usize = 132 + 64;

fn component(code: &[u8], entries: &[u16; 32]) -> Vec<u8> {
    let mut bytes = vec![0_u8; BYTECODE_OFFSET + code.len()];
    bytes[128..132].copy_from_slice(&1_u32.to_le_bytes());
    for (event, pc) in entries.iter().enumerate() {
        let offset = 132 + event * 2;
        bytes[offset..offset + 2].copy_from_slice(&pc.to_le_bytes());
    }
    bytes[BYTECODE_OFFSET..].copy_from_slice(code);
    bytes
}

fn request(studio: &Studio, op: &str, args: Value) -> Value {
    let input = json!({ "op": op, "args": args });
    serde_json::from_str(&studio.command(&input.to_string())).expect("Studio returns JSON")
}

fn ok(response: &Value) -> Value {
    response
        .get("ok")
        .unwrap_or_else(|| panic!("Studio operation failed: {response}"))
        .clone()
}

fn open(studio: &Studio, graph: Value) -> Value {
    ok(&request(
        studio,
        "field.draft.open",
        json!({ "graph": graph }),
    ))
}

fn compile(studio: &Studio, draft: &Value) -> Value {
    ok(&request(
        studio,
        "field.draft.compile",
        json!({ "draft": draft["draft"] }),
    ))
}

fn assert_error(response: &Value) {
    assert_eq!(response.as_object().map(serde_json::Map::len), Some(1));
    assert!(response.get("error").and_then(Value::as_object).is_some());
    assert!(
        response["error"]["message"]
            .as_str()
            .is_some_and(|s| !s.is_empty())
    );
}

fn graph_for(studio: &Studio, code: &[u8], entries: &[u16; 32]) -> (Vec<u8>, Value) {
    let bytes = component(code, entries);
    let graph = ok(&request(
        studio,
        "field.decode",
        json!({ "bytes": bytes, "declared_size": bytes.len() }),
    ));
    (bytes, graph)
}

fn basic_graph(studio: &Studio) -> (Vec<u8>, Value) {
    // message_box (pc 0, size 4), branch_if_false (pc 4, size 8), nop (12), end (13).
    let mut entries = [0_u16; 32];
    entries[1] = 13; // an existing terminal-only slot for new_event coverage
    graph_for(
        studio,
        &[0x03, 0, 0, 5, 0x02, 0, 0, 0, 0, 0, 13, 0, 0x0e, 0],
        &entries,
    )
}

#[test]
fn no_edit_compile_is_canonical_and_export_import_preserve_the_draft() {
    let studio = Studio::new();
    let (source, graph) = basic_graph(&studio);
    let draft = open(&studio, graph);
    let initial = draft.clone();

    assert_eq!(initial["revision"], 0);
    assert_eq!(initial["can_undo"], false);
    assert_eq!(initial["can_redo"], false);
    assert_eq!(initial["bindings"]["pc:0"], 0);

    let compiled = ok(&request(
        &studio,
        "field.draft.compile",
        json!({ "draft": draft["draft"] }),
    ));
    let compiled_bytes: Vec<u8> = compiled["bytes"]
        .as_array()
        .expect("compiled bytes are an array")
        .iter()
        .map(|value| value.as_u64().expect("byte is an integer") as u8)
        .collect();
    assert_eq!(compiled["declared_size"], source.len());
    assert_eq!(
        compiled_bytes, source,
        "unmodified draft must remain byte-identical"
    );

    let document = ok(&request(
        &studio,
        "field.draft.export",
        json!({ "draft": draft["draft"] }),
    ));
    let imported = ok(&request(
        &studio,
        "field.draft.import",
        json!({ "document": document }),
    ));
    let imported_snapshot = imported.clone();
    assert_eq!(imported_snapshot["graph"], initial["graph"]);
    assert_eq!(imported_snapshot["bindings"], initial["bindings"]);
    let imported_compile = ok(&request(
        &studio,
        "field.draft.compile",
        json!({ "draft": imported["draft"] }),
    ));
    assert_eq!(imported_compile["bytes"], compiled["bytes"]);
}

#[test]
fn typed_operand_and_branch_target_edits_are_undoable_and_redoable() {
    let studio = Studio::new();
    let (_, graph) = basic_graph(&studio);
    let draft = open(&studio, graph);
    let id = draft["draft"].clone();

    let edited = ok(&request(
        &studio,
        "field.draft.edit",
        json!({
            "draft": id,
            "revision": 0,
            "action": {
                "kind": "set_operand",
                "node": "pc:0",
                "operand": 1,
                "value": { "kind": "literal", "value": 9 }
            }
        }),
    ));
    assert_eq!(edited["revision"], 1);
    assert_eq!(edited["can_undo"], true);

    let branch = ok(&request(
        &studio,
        "field.draft.edit",
        json!({
            "draft": id,
            "revision": 1,
            "action": {
                "kind": "set_operand",
                "node": "pc:4",
                "operand": 3,
                "value": { "kind": "target", "node": "pc:12" }
            }
        }),
    ));
    assert_eq!(branch["revision"], 2);
    assert_eq!(branch["bindings"]["pc:12"], 12);

    let undone = ok(&request(
        &studio,
        "field.draft.undo",
        json!({ "draft": id, "revision": 2 }),
    ));
    assert_eq!(undone["revision"], 3);
    assert_eq!(undone["can_redo"], true);
    let redone = ok(&request(
        &studio,
        "field.draft.redo",
        json!({ "draft": id, "revision": 3 }),
    ));
    assert_eq!(redone["revision"], 4);

    let compiled = ok(&request(
        &studio,
        "field.draft.compile",
        json!({ "draft": id }),
    ));
    let output: Vec<u8> = compiled["bytes"]
        .as_array()
        .expect("compiled bytes are an array")
        .iter()
        .map(|value| value.as_u64().expect("byte is an integer") as u8)
        .collect();
    let code = &output[BYTECODE_OFFSET..];
    assert_eq!(
        code[3], 9,
        "literal edit must affect the encoded u8 operand"
    );
    assert_eq!(code[10], 12, "branch target must bind to the selected node");
}

#[test]
fn insertion_and_new_event_keep_stable_ids_across_undo_and_import() {
    let studio = Studio::new();
    let (source, graph) = basic_graph(&studio);
    let draft = open(&studio, graph);
    let id = draft["draft"].clone();

    let inserted = ok(&request(
        &studio,
        "field.draft.edit",
        json!({
            "draft": id,
            "revision": 0,
            "action": { "kind": "insert_after", "after": "pc:0", "bytes": [14] }
        }),
    ));
    assert_eq!(inserted["revision"], 1);
    let inserted_compile = compile(&studio, &inserted);
    assert_ne!(
        inserted_compile["bytes"],
        json!(source),
        "insertion must change compiled storage"
    );
    let inserted_pc = inserted["bindings"]["new:1"]
        .as_u64()
        .expect("inserted node has a stable binding") as usize;
    let continuation_pc = inserted["bindings"]["pc:4"]
        .as_u64()
        .expect("original continuation retains a binding") as usize;
    let inserted_instructions = inserted["graph"]["instructions"]
        .as_array()
        .expect("edited graph has decoded instructions");
    let mut current_pc = inserted_pc;
    let mut reached_continuation = false;
    for _ in 0..=inserted_instructions.len() {
        if current_pc == continuation_pc {
            reached_continuation = true;
            break;
        }
        let instruction = inserted_instructions
            .iter()
            .find(|instruction| instruction["pc"].as_u64() == Some(current_pc as u64))
            .expect("compiled insertion path points to a decoded instruction");
        let successors = instruction["successors"]
            .as_array()
            .expect("instruction has successor list");
        assert_eq!(
            successors.len(),
            1,
            "inserted fallthrough/bridge path is unambiguous"
        );
        current_pc = successors[0].as_u64().expect("successor PC is an integer") as usize;
    }
    assert!(
        reached_continuation,
        "inserted node must reach the original continuation through compiler bridges"
    );
    let inserted_bytes: Vec<u8> = inserted_compile["bytes"]
        .as_array()
        .expect("compiled bytes are an array")
        .iter()
        .map(|value| value.as_u64().expect("byte is an integer") as u8)
        .collect();
    let source_tail_start = BYTECODE_OFFSET + continuation_pc;
    let source_tail_len = source.len() - source_tail_start;
    assert_eq!(
        &inserted_bytes[source_tail_start..source_tail_start + source_tail_len],
        &source[source_tail_start..],
        "original source storage after the relocated island remains unchanged"
    );

    let undone = ok(&request(
        &studio,
        "field.draft.undo",
        json!({ "draft": id, "revision": 1 }),
    ));
    assert_eq!(undone["revision"], 2);
    let event = ok(&request(
        &studio,
        "field.draft.edit",
        json!({
            "draft": id,
            "revision": 2,
            "action": {
                "kind": "new_event",
                "actor": 0,
                "event": 1,
                "nodes": [ { "bytes": [14] }, { "bytes": [0] } ]
            }
        }),
    ));
    assert_eq!(event["revision"], 3);
    assert!(event["bindings"]["new:2"].as_u64().is_some());
    assert!(event["bindings"]["new:3"].as_u64().is_some());
    assert!(
        event["bindings"].get("new:1").is_none(),
        "undo must remove the authored node without reusing its ID"
    );

    let compiled = compile(&studio, &event);
    let entry = compiled["graph"]["entries"]
        .as_array()
        .expect("compiled graph has entries")
        .iter()
        .find(|entry| entry["actor"] == 0 && entry["event"] == 1)
        .expect("authored event slot is present");
    assert_ne!(
        entry["pc"], 13,
        "terminal-only slot should point at the authored event"
    );
    assert_eq!(
        compiled["declared_size"].as_u64().unwrap() as usize,
        compiled["bytes"].as_array().unwrap().len()
    );

    let document = ok(&request(
        &studio,
        "field.draft.export",
        json!({ "draft": id }),
    ));
    let imported = ok(&request(
        &studio,
        "field.draft.import",
        json!({ "document": document }),
    ));
    let imported_revision = imported["revision"]
        .as_u64()
        .expect("imported revision is present");
    let after_import = ok(&request(
        &studio,
        "field.draft.edit",
        json!({
            "draft": imported["draft"],
            "revision": imported_revision,
            "action": { "kind": "insert_after", "after": "pc:0", "bytes": [14] }
        }),
    ));
    assert!(
        after_import["bindings"]["new:4"].as_u64().is_some(),
        "import must preserve the next unique node ID"
    );
}

#[test]
fn stale_and_invalid_edits_leave_revision_graph_and_compiled_output_unchanged() {
    let studio = Studio::new();
    let (_, graph) = basic_graph(&studio);
    let draft = open(&studio, graph);
    let id = draft["draft"].clone();
    let first = ok(&request(
        &studio,
        "field.draft.edit",
        json!({
            "draft": id,
            "revision": 0,
            "action": {
                "kind": "set_operand",
                "node": "pc:0",
                "operand": 1,
                "value": { "kind": "literal", "value": 8 }
            }
        }),
    ));
    let before_compiled = compile(&studio, &first);
    let before_bytes = before_compiled["bytes"].clone();

    let stale = request(
        &studio,
        "field.draft.edit",
        json!({
            "draft": id,
            "revision": 0,
            "action": {
                "kind": "set_operand",
                "node": "pc:0",
                "operand": 1,
                "value": { "kind": "literal", "value": 7 }
            }
        }),
    );
    assert_error(&stale);
    let invalid_index = request(
        &studio,
        "field.draft.edit",
        json!({
            "draft": id,
            "revision": 1,
            "action": {
                "kind": "set_operand",
                "node": "pc:0",
                "operand": 8,
                "value": { "kind": "literal", "value": 7 }
            }
        }),
    );
    assert_error(&invalid_index);
    let invalid_type = request(
        &studio,
        "field.draft.edit",
        json!({
            "draft": id,
            "revision": 1,
            "action": {
                "kind": "set_operand",
                "node": "pc:0",
                "operand": 1,
                "value": { "kind": "target", "node": "pc:12" }
            }
        }),
    );
    assert_error(&invalid_type);
    let invalid_target = request(
        &studio,
        "field.draft.edit",
        json!({
            "draft": id,
            "revision": 1,
            "action": {
                "kind": "set_operand",
                "node": "pc:4",
                "operand": 3,
                "value": { "kind": "target", "node": "pc:65535" }
            }
        }),
    );
    assert_error(&invalid_target);

    let after_compiled = compile(&studio, &first);
    assert_eq!(after_compiled["graph"], before_compiled["graph"]);
    assert_eq!(
        after_compiled["bytes"], before_bytes,
        "rejected edits must not publish partial output"
    );
    let current_revision_edit = request(
        &studio,
        "field.draft.edit",
        json!({
            "draft": id,
            "revision": 1,
            "action": {
                "kind": "set_operand",
                "node": "pc:0",
                "operand": 1,
                "value": { "kind": "literal", "value": 10 }
            }
        }),
    );
    assert_eq!(
        ok(&current_revision_edit)["revision"],
        2,
        "refusals must not advance revision"
    );
}

#[test]
fn insertion_inside_a_paired_setup_unit_is_refused_atomically() {
    let studio = Studio::new();
    let entries = [0_u16; 32];
    let code = [0x10, 0, 1, 0, 2, 0, 3, 0, 0, 0x10, 1, 0x00];
    let (_, graph) = graph_for(&studio, &code, &entries);
    let draft = open(&studio, graph);
    let id = draft["draft"].clone();
    let before = draft.clone();

    let refused = request(
        &studio,
        "field.draft.edit",
        json!({
            "draft": id,
            "revision": 0,
            "action": { "kind": "insert_after", "after": "pc:0", "bytes": [14] }
        }),
    );
    assert_error(&refused);
    let after_compiled = compile(&studio, &draft);
    assert_eq!(after_compiled["graph"], before["graph"]);
}
