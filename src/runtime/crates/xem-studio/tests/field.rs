use serde_json::{Value, json};
use xem_studio::Studio;

const BYTECODE_OFFSET: usize = 132 + 64;

fn component(code: &[u8], entries: &[u16], declared_code_size: usize) -> Vec<u8> {
    assert!(code.len() <= 0x1_0000);
    assert!(declared_code_size <= code.len());
    let mut bytes = vec![0_u8; BYTECODE_OFFSET + code.len()];
    bytes[128..132].copy_from_slice(&1_u32.to_le_bytes());
    for event in 0..32 {
        let pc = entries.get(event).copied().unwrap_or(0);
        bytes[132 + event * 2..134 + event * 2].copy_from_slice(&pc.to_le_bytes());
    }
    bytes[BYTECODE_OFFSET..].copy_from_slice(code);
    bytes
}

fn request(studio: &Studio, op: &str, args: Value) -> Value {
    let input = json!({ "op": op, "args": args });
    serde_json::from_str(&studio.command(&input.to_string())).expect("Studio always returns JSON")
}

fn decode(studio: &Studio, bytes: &[u8], declared_size: usize) -> Value {
    let response = request(
        studio,
        "field.decode",
        json!({ "bytes": bytes, "declared_size": declared_size }),
    );
    response
        .get("ok")
        .expect("synthetic component should decode successfully")
        .clone()
}

fn encode(studio: &Studio, graph: Value) -> Vec<u8> {
    let response = request(studio, "field.encode", json!({ "graph": graph }));
    response
        .get("ok")
        .and_then(Value::as_array)
        .expect("canonical graph should encode to a byte array")
        .iter()
        .map(|byte| byte.as_u64().expect("encoded byte is an integer") as u8)
        .collect()
}

fn assert_round_trip(studio: &Studio, bytes: &[u8], declared_size: usize) -> Value {
    let graph = decode(studio, bytes, declared_size);
    let reconstructed = encode(studio, graph.clone());
    assert_eq!(
        reconstructed.len(),
        bytes.len(),
        "round-trip length changed"
    );
    assert_eq!(reconstructed, bytes, "synthetic round-trip changed bytes");
    graph
}

fn assert_error(response: &Value, kind: &str) {
    let error = response
        .get("error")
        .and_then(Value::as_object)
        .expect("failure returns a structured error");
    assert_eq!(response.as_object().map(serde_json::Map::len), Some(1));
    assert_eq!(error.get("kind").and_then(Value::as_str), Some(kind));
    assert!(
        error
            .get("message")
            .and_then(Value::as_str)
            .is_some_and(|s| !s.trim().is_empty())
    );
}

#[test]
fn decodes_and_round_trips_header_entries_and_basic_control_flow() {
    let studio = Studio::new();
    let empty = vec![0_u8; 132];
    let empty_graph = assert_round_trip(&studio, &empty, empty.len());
    assert_eq!(empty_graph["actors"], json!([]));
    assert_eq!(empty_graph["instructions"], json!([]));

    let mut bytes = component(&[0x0e, 0x00], &[], 2);
    bytes[0] = 0x20;
    bytes[1] = 0x80;
    let graph = assert_round_trip(&studio, &bytes, BYTECODE_OFFSET + 2);

    assert_eq!(graph["schema_version"], 1);
    assert_eq!(graph["bytecode_offset"], BYTECODE_OFFSET);
    assert_eq!(graph["stream_size"], bytes.len());
    assert_eq!(graph["declared_size"], BYTECODE_OFFSET + 2);
    assert_eq!(graph["variable_unsigned_bits"][0], 0x20);
    assert_eq!(graph["variable_unsigned_bits"][1], 0x80);
    assert_eq!(graph["actors"].as_array().map(Vec::len), Some(1));
    assert_eq!(graph["entries"].as_array().map(Vec::len), Some(32));
    assert_eq!(graph["instructions"][0]["pc"], 0);
    assert_eq!(graph["instructions"][0]["spec"]["mnemonic"], "nop");
    assert_eq!(graph["instructions"][0]["successors"], json!([1]));
    assert_eq!(graph["instructions"][1]["spec"]["flow"], "end");

    let selected = [0x10, 0, 0x34, 0x12, 0x78, 0x56, 0xbc, 0x9a, 0xe0, 0];
    let selected_graph = assert_round_trip(
        &studio,
        &component(&selected, &[], selected.len()),
        BYTECODE_OFFSET + selected.len(),
    );
    assert_eq!(
        selected_graph["instructions"][0]["operands"],
        json!([0, 0x1234, 0x5678, 0x9abc, 0xe0])
    );
    assert_eq!(
        selected_graph["instructions"][0]["immediate"],
        json!([null, true, true, true, null])
    );

    let immediate = [0x0b, 1, 0x80, 0];
    let immediate_graph = assert_round_trip(
        &studio,
        &component(&immediate, &[], immediate.len()),
        BYTECODE_OFFSET + immediate.len(),
    );
    assert_eq!(
        immediate_graph["instructions"][0]["operands"],
        json!([0x8001])
    );
    assert_eq!(
        immediate_graph["instructions"][0]["immediate"],
        json!([true])
    );
}

#[test]
fn preserves_setup_pairs_inner_continuations_and_alternative_skips() {
    let studio = Studio::new();

    let paired = [0x10, 0, 1, 0, 2, 0, 3, 0, 0, 0x10, 1, 0x00];
    let pair_graph = assert_round_trip(
        &studio,
        &component(&paired, &[], paired.len()),
        BYTECODE_OFFSET + paired.len(),
    );
    assert_eq!(pair_graph["instructions"][0]["spec"]["mnemonic"], "walk_to");
    assert_eq!(
        pair_graph["instructions"][1]["spec"]["mnemonic"],
        "walk_step"
    );
    assert_eq!(pair_graph["instructions"][1]["spec"]["setup"], 9);
    assert_eq!(pair_graph["diagnostics"]["unpaired"], json!([]));
    assert!(pair_graph["blocks"].as_array().is_some_and(|blocks| {
        blocks.iter().any(|block| {
            block["kind"] == "pair"
                && block["start"] == 0
                && block["end"] == 11
                && block["instruction_pcs"] == json!([0, 9])
        })
    }));

    let paired_limited = [0x11, 0, 1, 0, 2, 0, 3, 0, 0, 0x11, 1, 0, 0, 0x00];
    let limited_graph = assert_round_trip(
        &studio,
        &component(&paired_limited, &[], paired_limited.len()),
        BYTECODE_OFFSET + paired_limited.len(),
    );
    assert_eq!(
        limited_graph["instructions"][0]["spec"]["mnemonic"],
        "walk_to_limited"
    );
    assert_eq!(
        limited_graph["instructions"][1]["spec"]["mnemonic"],
        "walk_step_limited"
    );
    assert_eq!(limited_graph["diagnostics"]["unpaired"], json!([]));

    let mut paired_arc = vec![0_u8; 14];
    paired_arc[0] = 0x57;
    paired_arc[11] = 0x57;
    paired_arc[12] = 3;
    paired_arc[13] = 0;
    let arc_graph = assert_round_trip(
        &studio,
        &component(&paired_arc, &[], paired_arc.len()),
        BYTECODE_OFFSET + paired_arc.len(),
    );
    assert_eq!(arc_graph["instructions"][0]["spec"]["mnemonic"], "arc_jump");
    assert_eq!(arc_graph["instructions"][1]["spec"]["mnemonic"], "arc_step");
    assert_eq!(arc_graph["diagnostics"]["unpaired"], json!([]));

    let mut inner = vec![0_u8; 10];
    inner[0] = 0x12;
    inner[9] = 0x00;
    let inner_graph = assert_round_trip(
        &studio,
        &component(&inner, &[], inner.len()),
        BYTECODE_OFFSET + inner.len(),
    );
    assert_eq!(inner_graph["diagnostics"]["inner"], json!([[0, 4]]));
    assert_eq!(inner_graph["instructions"][0]["inner"], json!([4]));

    let skips = [0xfc, 0xff, 0, 0, 0, 0, 0];
    let skip_graph = assert_round_trip(
        &studio,
        &component(&skips, &[], skips.len()),
        BYTECODE_OFFSET + skips.len(),
    );
    assert_eq!(skip_graph["instructions"][0]["skips"], json!([6]));
    assert_eq!(skip_graph["diagnostics"]["followed_skips"], 1);
    assert_eq!(skip_graph["instructions"].as_array().map(Vec::len), Some(3));

    let zero_skip = [0x9a, 0, 0, 0x01, 6, 0, 0x00];
    let zero_graph = assert_round_trip(
        &studio,
        &component(&zero_skip, &[], zero_skip.len()),
        BYTECODE_OFFSET + zero_skip.len(),
    );
    assert_eq!(zero_graph["instructions"][0]["skips"], json!([6]));
    assert_eq!(zero_graph["diagnostics"]["followed_skips"], 1);

    for opcode in [0xd4, 0xfc] {
        let party_skip = [opcode, 0xff, 0, 0, 0, 0, 0];
        let graph = assert_round_trip(
            &studio,
            &component(&party_skip, &[], party_skip.len()),
            BYTECODE_OFFSET + party_skip.len(),
        );
        assert_eq!(graph["instructions"][0]["skips"], json!([6]));
        assert_eq!(graph["diagnostics"]["followed_skips"], 1);
    }

    let ext18 = [0xfe, 0x18, 0, 0xfe, 0x1a, 0];
    let ext18_graph = assert_round_trip(
        &studio,
        &component(&ext18, &[], ext18.len()),
        BYTECODE_OFFSET + ext18.len(),
    );
    assert_eq!(ext18_graph["instructions"][0]["skips"], json!([5]));
    assert_eq!(ext18_graph["diagnostics"]["followed_skips"], 1);

    let extc6 = [0xfe, 0xc6, 0, 0, 0xfe, 0x1a, 0];
    let extc6_graph = assert_round_trip(
        &studio,
        &component(&extc6, &[], extc6.len()),
        BYTECODE_OFFSET + extc6.len(),
    );
    assert_eq!(extc6_graph["instructions"][0]["skips"], json!([6]));
    assert_eq!(extc6_graph["diagnostics"]["followed_skips"], 1);
}

#[test]
fn follows_inline_jump_tables_and_reports_known_component_boundaries() {
    let studio = Studio::new();
    let table = [0xa6, 0, 0, 0x01, 9, 0, 0x01, 9, 0, 0x00];
    let table_graph = assert_round_trip(
        &studio,
        &component(&table, &[], table.len()),
        BYTECODE_OFFSET + table.len(),
    );
    assert_eq!(table_graph["diagnostics"]["tables"], json!([[0, 2]]));
    assert!(
        table_graph["instructions"]
            .as_array()
            .is_some_and(|items| items.iter().any(|ins| ins["pc"] == 3))
    );
    assert!(table_graph["blocks"].as_array().is_some_and(|blocks| {
        blocks.iter().any(|block| {
            block["kind"] == "table"
                && block["start"] == 0
                && block["end"] == 9
                && block["instruction_pcs"] == json!([0, 3, 6])
        })
    }));

    let tail = component(&[0x1a, 0x17], &[], 1);
    let tail_graph = assert_round_trip(&studio, &tail, BYTECODE_OFFSET + 1);
    assert_eq!(tail_graph["stream_size"], tail.len());
    assert_eq!(tail_graph["declared_size"], BYTECODE_OFFSET + 1);
    assert_eq!(tail_graph["diagnostics"]["beyond_declared"], json!([0]));

    let oversized_tail = [tail.as_slice(), &[0; 16]].concat();
    assert_error(
        &request(
            &studio,
            "field.decode",
            json!({ "bytes": oversized_tail, "declared_size": BYTECODE_OFFSET + 1 }),
        ),
        "invalid_declared_size",
    );
}

#[test]
fn reports_malformed_headers_truncation_unknown_opcodes_and_invalid_graphs() {
    let studio = Studio::new();
    for (bytes, declared_size, kind) in [
        (vec![0; 127], 127, "truncated_input"),
        (
            component(&[0xfe, 0xff], &[], 2),
            BYTECODE_OFFSET + 2,
            "unsupported_instruction",
        ),
        (
            component(&[0x01, 0x01], &[], 2),
            BYTECODE_OFFSET + 2,
            "truncated_input",
        ),
    ] {
        assert_error(
            &request(
                &studio,
                "field.decode",
                json!({ "bytes": bytes, "declared_size": declared_size }),
            ),
            kind,
        );
    }

    let mut incomplete_table = vec![0_u8; 132];
    incomplete_table[128] = 1;
    assert_error(
        &request(
            &studio,
            "field.decode",
            json!({ "bytes": incomplete_table, "declared_size": 132 }),
        ),
        "truncated_input",
    );

    let outside_entry = component(&[0x00], &[1], 1);
    assert_error(
        &request(
            &studio,
            "field.decode",
            json!({ "bytes": outside_entry, "declared_size": BYTECODE_OFFSET + 1 }),
        ),
        "entry_outside",
    );

    let graph = decode(
        &studio,
        &component(&[0x0b, 1, 0, 0], &[], 4),
        BYTECODE_OFFSET + 4,
    );
    let mut invalid_graph = graph.clone();
    invalid_graph["schema_version"] = json!(99);
    assert_error(
        &request(&studio, "field.encode", json!({ "graph": invalid_graph })),
        "invalid_graph",
    );

    let mut changed_storage = graph.clone();
    changed_storage["blocks"][0]["bytes"][1] = json!(2);
    assert_error(
        &request(&studio, "field.encode", json!({ "graph": changed_storage })),
        "invalid_graph",
    );

    let mut missing_storage = graph.clone();
    missing_storage["blocks"] = json!([]);
    assert_error(
        &request(&studio, "field.encode", json!({ "graph": missing_storage })),
        "invalid_graph",
    );

    let mut changed_instruction = graph;
    changed_instruction["instructions"][0]["operands"][0] = json!(2);
    assert_error(
        &request(
            &studio,
            "field.encode",
            json!({ "graph": changed_instruction }),
        ),
        "invalid_graph",
    );
}

#[test]
fn keeps_map_records_overlap_outside_edges_and_unpaired_steps_explicit() {
    let studio = Studio::new();
    let records = [0xff, 1, 2, 3, 4, 5, 6];
    let record_graph = assert_round_trip(
        &studio,
        &component(&records, &[], records.len()),
        BYTECODE_OFFSET + records.len(),
    );
    assert_eq!(record_graph["entries"], json!([]));
    assert_eq!(record_graph["skipped_zero_entries"], 32);
    assert_eq!(record_graph["instructions"], json!([]));
    assert!(record_graph["blocks"].as_array().is_some_and(|blocks| {
        blocks
            .iter()
            .any(|block| block["reason"] == "map_entry_records")
    }));

    let active_records = [0xff, 1, 2, 3, 4, 5, 6, 0x00];
    let active_graph = assert_round_trip(
        &studio,
        &component(&active_records, &[7], active_records.len()),
        BYTECODE_OFFSET + active_records.len(),
    );
    assert_eq!(active_graph["entries"].as_array().map(Vec::len), Some(1));
    assert_eq!(active_graph["skipped_zero_entries"], 31);
    assert_eq!(active_graph["instructions"][0]["pc"], 7);

    let overlap = [0x02, 0, 0x00, 0, 0, 0, 2, 0, 0x00];
    let overlap_graph = assert_round_trip(
        &studio,
        &component(&overlap, &[], overlap.len()),
        BYTECODE_OFFSET + overlap.len(),
    );
    assert_eq!(overlap_graph["diagnostics"]["overlaps"], json!([[2, 0]]));

    let outside = [0x01, 0xff, 0x00];
    let outside_graph = assert_round_trip(
        &studio,
        &component(&outside, &[], outside.len()),
        BYTECODE_OFFSET + outside.len(),
    );
    assert_eq!(outside_graph["diagnostics"]["outside"], json!([[0, 255]]));
    assert!(
        outside_graph["edges"]
            .as_array()
            .is_some_and(|edges| { edges.iter().any(|edge| edge["status"] == "outside_stream") })
    );

    let gap = [0x01, 5, 0, 0xff, 0xff, 0x00];
    let gap_graph = assert_round_trip(
        &studio,
        &component(&gap, &[], gap.len()),
        BYTECODE_OFFSET + gap.len(),
    );
    assert!(gap_graph["blocks"].as_array().is_some_and(|blocks| {
        blocks.iter().any(|block| {
            block["kind"] == "pinned"
                && block["start"] == 3
                && block["end"] == 5
                && block["reason"] == "unreachable_bytes"
        })
    }));

    let unpaired = [0x10, 1, 0x00];
    let unpaired_graph = assert_round_trip(
        &studio,
        &component(&unpaired, &[], unpaired.len()),
        BYTECODE_OFFSET + unpaired.len(),
    );
    assert_eq!(unpaired_graph["diagnostics"]["unpaired"], json!([0]));
}

#[test]
fn preserves_extended_rerun_and_coincident_control_flow_edge_kinds() {
    let studio = Studio::new();
    let rerun = [0xfe, 0x00, 0x00];
    let rerun_graph = assert_round_trip(
        &studio,
        &component(&rerun, &[], rerun.len()),
        BYTECODE_OFFSET + rerun.len(),
    );
    assert_eq!(rerun_graph["instructions"][0]["spec"]["flow"], "rerun");
    assert_eq!(rerun_graph["instructions"][0]["successors"], json!([1]));

    let call = [0x05, 3, 0, 0x00];
    let call_graph = assert_round_trip(
        &studio,
        &component(&call, &[], call.len()),
        BYTECODE_OFFSET + call.len(),
    );
    assert!(call_graph["edges"].as_array().is_some_and(|edges| {
        edges
            .iter()
            .any(|edge| edge["from_pc"] == 0 && edge["to_pc"] == 3 && edge["kind"] == "call")
    }));

    let branch = [0x02, 0, 0, 0, 0, 0, 8, 0, 0x00];
    let branch_graph = assert_round_trip(
        &studio,
        &component(&branch, &[], branch.len()),
        BYTECODE_OFFSET + branch.len(),
    );
    assert!(branch_graph["edges"].as_array().is_some_and(|edges| {
        edges
            .iter()
            .any(|edge| edge["from_pc"] == 0 && edge["to_pc"] == 8 && edge["kind"] == "branch")
    }));
}

#[test]
fn writes_synthetic_wasm_parity_fixture_when_requested() {
    let Ok(path) = std::env::var("XEM_STUDIO_WASM_PARITY_FIXTURE") else {
        eprintln!("native/WASM parity fixture skipped: XEM_STUDIO_WASM_PARITY_FIXTURE is unset");
        return;
    };
    let repository = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .ancestors()
        .nth(4)
        .expect("crate is inside the repository")
        .to_path_buf();
    let path = std::path::PathBuf::from(path);
    assert!(
        path.starts_with(repository.join("tmp/studio")),
        "synthetic parity fixture must remain under ignored tmp/studio"
    );

    let code = [0x10, 0, 1, 0, 2, 0, 3, 0, 0, 0x10, 1, 0, 0xfe, 0, 0];
    let bytes = component(&code, &[0, 12], code.len());
    let declared_size = BYTECODE_OFFSET + bytes.len() - BYTECODE_OFFSET;
    let studio = Studio::new();
    let graph = decode(&studio, &bytes, declared_size);
    let encoded = encode(&studio, graph.clone());
    assert_eq!(encoded, bytes, "synthetic native graph round trips");

    std::fs::create_dir_all(path.parent().expect("fixture has a parent"))
        .expect("create ignored parity fixture parent");
    std::fs::write(
        path,
        serde_json::to_vec(
            &json!({ "bytes": bytes, "declared_size": declared_size, "graph": graph }),
        )
        .expect("serialize synthetic parity fixture"),
    )
    .expect("write synthetic parity fixture");
}

#[test]
fn handles_wrapping_extended_dispatch_and_retains_precise_physical_ranges() {
    let studio = Studio::new();
    let mut code = vec![0_u8; 0x1_0000];
    code[0] = 0xa0;
    code[0x0d] = 0x00;
    code[0xffff] = 0xfe;
    let entries = [u16::MAX; 32];
    let bytes = component(&code, &entries, code.len());
    let graph = assert_round_trip(&studio, &bytes, BYTECODE_OFFSET + code.len());
    let instruction = graph["instructions"]
        .as_array()
        .and_then(|items| items.iter().find(|ins| ins["pc"] == u16::MAX))
        .expect("wrapped extended instruction remains represented");
    assert_eq!(instruction["extended"], 0xa0);
    assert_eq!(instruction["ranges"], json!([[65535, 65536], [0, 12]]));
    assert_eq!(instruction["size"], 13);
    assert!(graph["blocks"].as_array().is_some_and(|blocks| {
        blocks.iter().any(|block| {
            block["start"] == 12 && block["end"] == 13 && block["instruction_pcs"] == json!([12])
        }) && !blocks
            .iter()
            .any(|block| block["start"] == 0 && block["end"] == 65536)
    }));
}

#[test]
fn command_errors_do_not_change_later_field_results_or_echo_payloads() {
    let studio = Studio::new();
    assert_error(
        &request(&studio, "field.decode", json!({ "bytes": [] })),
        "truncated_input",
    );
    let payload =
        json!({ "error": { "kind": "user-value", "message": "ordinary data" }, "ok": null });
    assert_eq!(
        request(&studio, "echo", payload.clone()),
        json!({ "ok": payload })
    );
    let valid = decode(&studio, &component(&[0x00], &[], 1), BYTECODE_OFFSET + 1);
    assert_eq!(valid["instructions"][0]["spec"]["mnemonic"], "end_slot");
}
