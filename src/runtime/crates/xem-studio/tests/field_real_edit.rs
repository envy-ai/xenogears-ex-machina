//! Gated real-disc M6 candidate discovery using only public Studio operations.
//! This emits a private package candidate; it does not claim runtime playback.
use serde_json::{Value, json};
use std::collections::BTreeSet;
use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};
use xem_disc::field_context::{SourceContext, SourceIdentity};
use xem_studio::{Studio, packed};

const MAP: u32 = 23;
const COMPONENT_EVENTS: usize = 5;
const FIELD_VARIABLES: std::ops::Range<usize> = 512..1024;
const MARKER: u16 = 0x1234;

fn command(studio: &Studio, op: &str, args: Value) -> Value {
    let request = json!({"op":op,"args":args});
    serde_json::from_str(&studio.command(&request.to_string())).expect("Studio response is JSON")
}

fn ok(response: &Value, label: &str) -> Value {
    response
        .get("ok")
        .unwrap_or_else(|| {
            panic!(
                "{label} failed with structured protocol error: {}",
                serde_json::to_string(&response["error"])
                    .expect("protocol error serializes as JSON")
            )
        })
        .clone()
}

fn integer(value: &Value, label: &str) -> usize {
    value
        .as_u64()
        .unwrap_or_else(|| panic!("{label} is an unsigned integer")) as usize
}

fn byte_array(value: &Value, label: &str) -> Vec<u8> {
    value
        .as_array()
        .unwrap_or_else(|| panic!("{label} is a byte array"))
        .iter()
        .map(|byte| {
            u8::try_from(byte.as_u64().expect("byte is an integer")).expect("byte is in range")
        })
        .collect()
}

fn open_disc(studio: &Studio, path: &Path, expected_disc: u64) -> (u32, Value) {
    let opened = ok(
        &command(
            studio,
            "disc.open",
            json!({
                "kind":"native", "path":path.display().to_string()
            }),
        ),
        "open original disc image",
    );
    let session = u32::try_from(integer(&opened["session"], "disc session"))
        .expect("session identifier fits u32");
    let mut state = opened;
    for _ in 0..20_000 {
        if state["state"] == "ready" {
            break;
        }
        state = ok(
            &command(studio, "disc.poll", json!({"session":session})),
            "identify original disc",
        );
    }
    assert!(
        state["state"] == "ready",
        "identified disc reaches ready state"
    );
    assert_eq!(
        state["identity"]["disc"].as_u64(),
        Some(expected_disc),
        "the selected known original disc identity matches its path"
    );
    let serial = if expected_disc == 1 {
        "SLUS-00664"
    } else {
        "SLUS-00669"
    };
    assert_eq!(
        state["identity"]["serial"], serial,
        "the selected known original disc serial matches its identity"
    );
    assert_eq!(
        state["identity"]["test_fixture"].as_bool(),
        Some(false),
        "real-source fixture does not use the test identity bypass"
    );
    let expected_package_discs = json!([
        {
            "serial": xem_disc::KnownDisc::Disc1.serial(),
            "sha256": xem_disc::identify::hex(&xem_disc::KnownDisc::Disc1.executable_sha256())
        },
        {
            "serial": xem_disc::KnownDisc::Disc2.serial(),
            "sha256": xem_disc::identify::hex(&xem_disc::KnownDisc::Disc2.executable_sha256())
        }
    ]);
    assert_eq!(
        state["identity"]["package_discs"], expected_package_discs,
        "either known original disc identity lists both exact package-family targets"
    );
    (session, state["identity"].clone())
}

fn map_bundle(studio: &Studio, session: u32, map: u32) -> Value {
    for _ in 0..20_000 {
        let result = command(
            studio,
            "field.map_bundle",
            json!({"session":session,"map":map}),
        );
        if result["ok"]["state"] == "ready" {
            return ok(&result, "read original map bundle");
        }
        assert_eq!(
            result["ok"]["state"], "pending",
            "map bundle read remains a bounded public pending/ready operation"
        );
    }
    panic!("map bundle read did not complete within its bounded poll count");
}

fn write_new(path: &Path, bytes: &[u8]) {
    let mut file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(path)
        .expect("private fixture output must be a fresh file");
    file.write_all(bytes).expect("write private fixture output");
    file.sync_all().expect("flush private fixture output");
}

fn hex_hash(bytes: &[u8]) -> String {
    use sha2::{Digest, Sha256};
    Sha256::digest(bytes)
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

fn u32_at(bytes: &[u8], offset: usize) -> usize {
    usize::try_from(u32::from_le_bytes(
        bytes[offset..offset + 4].try_into().expect("header word"),
    ))
    .expect("header offset fits usize")
}

fn decoded_component(bundle: &[u8], index: usize) -> Option<Vec<u8>> {
    let size = u32_at(bundle, 0x10c + index * 4);
    if size == 0 {
        return None;
    }
    let start = u32_at(bundle, 0x130 + index * 4);
    Some(
        packed::decode_block(&bundle[start..], size + 16)
            .expect("original bundle component decodes within its declared allocation")
            .data,
    )
}

fn output_directory() -> PathBuf {
    let requested = std::env::var_os("XEM_STUDIO_M6_REAL_OUTPUT")
        .expect("gated real fixture requires an explicit fresh output leaf");
    let path = PathBuf::from(requested);
    let repo = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .ancestors()
        .nth(4)
        .expect("crate is inside repository")
        .to_path_buf();
    assert!(
        path.starts_with(repo.join("tmp/studio/milestone-6")),
        "real-source fixture outputs remain under ignored tmp/studio/milestone-6"
    );
    assert!(
        !path.exists(),
        "preserve prior M6 evidence; output leaf must be new"
    );
    fs::create_dir_all(path.parent().expect("output leaf has parent"))
        .expect("create ignored fixture parent");
    fs::create_dir(&path).expect("create a fresh ignored fixture leaf");
    path
}

#[test]
fn gated_real_disc_service_discovers_and_packages_a_map23_edit_candidate() {
    if std::env::var("XEM_STUDIO_M6_REAL_FIXTURE").ok().as_deref() != Some("1") {
        eprintln!("M6 real-disc edit fixture skipped: XEM_STUDIO_M6_REAL_FIXTURE=1 is unset");
        return;
    }
    let root = PathBuf::from(
        std::env::var_os("XEM_STUDIO_EXTRACT_ROOT")
            .expect("gated M6 fixture requires explicit XEM_STUDIO_EXTRACT_ROOT"),
    );
    let disc1_path = root.join("discs/disc1.bin");
    let disc2_path = root.join("discs/disc2.bin");
    assert!(
        root.join("extract/disc1").is_dir(),
        "explicit disc 1 extraction is present"
    );
    assert!(
        root.join("extract/disc2").is_dir(),
        "explicit disc 2 extraction is present"
    );
    assert!(
        disc1_path.is_file() && disc2_path.is_file(),
        "both explicit original BIN images are present"
    );
    let output = output_directory();

    let studio = Studio::new();
    let (session1, identity1) = open_disc(&studio, &disc1_path, 1);
    let (_session2, identity2) = open_disc(&studio, &disc2_path, 2);
    let mut maps = None;
    for _ in 0..20_000 {
        let response = command(&studio, "disc.maps", json!({"session":session1}));
        if response["ok"]["state"] == "ready" {
            maps = Some(response);
            break;
        }
        assert_eq!(
            response["ok"]["state"], "pending",
            "map registry read remains a bounded public pending/ready operation"
        );
    }
    let maps = ok(
        &maps.expect("disc map registry read completes within its poll bound"),
        "read original disc 1 map registry",
    );
    assert_eq!(
        maps["state"], "ready",
        "original disc map registry finishes loading"
    );
    let map_summaries = maps["maps"].as_array().expect("map registry is an array");
    let map23_summary = map_summaries
        .iter()
        .find(|summary| summary["map"] == MAP)
        .expect("identified original disc exposes map 23");
    assert_eq!(
        map23_summary["placeholder"], false,
        "map 23 has a source bundle"
    );
    let target = map_bundle(&studio, session1, MAP);
    let source_bundle = byte_array(&target["bytes"], "map 23 source bundle");
    let context_path = PathBuf::from(
        std::env::var_os("XEM_STUDIO_M6_SOURCE_CONTEXT")
            .expect("gated M6 fixture requires explicit XEM_STUDIO_M6_SOURCE_CONTEXT"),
    );
    let context_bytes = fs::read(&context_path).expect("read private measured context capture");
    let source_context: SourceContext = serde_json::from_slice(&context_bytes)
        .expect("captured context follows the shared strict schema");
    let source_identity: SourceIdentity = serde_json::from_value(target["source"].clone())
        .expect("public map bundle identifies its verified original source");
    let module = source_context.module_sha256.clone();
    assert_eq!(
        source_context.identity, source_identity,
        "captured runtime context belongs to this map bundle"
    );
    let context_guard = source_context
        .guard(&source_bundle, &source_identity, &module)
        .expect("all nine original component outputs match the measured runtime context");
    let mut measured_source_bundle = source_bundle.clone();
    measured_source_bundle.extend_from_slice(&context_guard);
    let context_sha256 = hex_hash(&context_bytes);
    let slot23 =
        u32::try_from(integer(&target["slot"], "map 23 index slot")).expect("slot fits u32");
    let original_component = ok(
        &command(
            &studio,
            "field.map_events",
            json!({"session":session1,"map":MAP}),
        ),
        "read map 23 event component",
    );
    assert_eq!(
        original_component["placeholder"], false,
        "map 23 event component exists"
    );
    let component_bytes = byte_array(
        &original_component["component"]["bytes"],
        "map 23 event component",
    );
    let declared_size = integer(
        &original_component["component"]["declared_size"],
        "component declared size",
    );
    let original_message_table = decoded_component(&measured_source_bundle, 7)
        .expect("map 23 message-table component 7 is present");
    let message_count = usize::from(u16::from_le_bytes(
        original_message_table[0..2]
            .try_into()
            .expect("message-table count"),
    ));
    assert!(
        message_count > 1 && message_count < 0x1000,
        "map 23 component 7 declares multiple existing message IDs"
    );
    let table_zero = u16::from_le_bytes(
        original_message_table[2..4]
            .try_into()
            .expect("message-table reserved word"),
    );
    assert_eq!(
        table_zero, 0,
        "map 23 message table has its reserved zero word"
    );
    let offsets_end = 4 + (message_count + 1) * 2;
    let table_end = offsets_end + message_count * 2;
    assert!(
        table_end <= original_message_table.len(),
        "map 23 message table contains offsets and message sizes for every declared ID"
    );
    let message_offsets = (0..=message_count)
        .map(|index| {
            usize::from(u16::from_le_bytes(
                original_message_table[4 + index * 2..6 + index * 2]
                    .try_into()
                    .expect("message table offset"),
            ))
        })
        .collect::<Vec<_>>();
    assert!(
        message_offsets
            .iter()
            .all(|offset| *offset <= original_message_table.len())
            && message_offsets.windows(2).all(|pair| pair[0] <= pair[1]),
        "map 23 component 7 message offsets are ordered and in range"
    );
    let visible_message_evidence = |index: usize| {
        let span = &original_message_table[message_offsets[index]..message_offsets[index + 1]];
        let terminator = span.iter().position(|byte| *byte == 0).unwrap_or_else(|| {
            panic!("existing message {index} has a terminator within its declared span")
        });
        let bytes = &span[..=terminator];
        json!({
            "index":index,
            "byte_length":bytes.len(),
            "bytes_hex":bytes.iter().map(|byte| format!("{byte:02x}")).collect::<String>(),
            "sha256":hex_hash(bytes)
        })
    };
    let message_zero_evidence = visible_message_evidence(0);
    let message_one_evidence = visible_message_evidence(1);

    let graph = ok(
        &command(
            &studio,
            "field.decode",
            json!({"bytes":component_bytes,"declared_size":declared_size}),
        ),
        "decode original map 23 field graph",
    );
    let lanes = ok(
        &command(&studio, "field.lanes", json!({"graph":graph})),
        "inspect map 23 actor/event lanes",
    );
    let instructions = graph["instructions"]
        .as_array()
        .expect("decoded instruction list");
    let blocks = graph["blocks"].as_array().expect("decoded storage blocks");
    write_new(
        &output.join("source-graph.json"),
        &serde_json::to_vec_pretty(&graph).expect("serialize original source graph"),
    );
    write_new(
        &output.join("source-lanes.json"),
        &serde_json::to_vec_pretty(&lanes).expect("serialize original source lanes"),
    );
    write_new(
        &output.join("source-component-metadata.json"),
        &serde_json::to_vec_pretty(&json!({
            "map":MAP,"slot":slot23,"component_index":COMPONENT_EVENTS,
            "declared_size":declared_size,"returned_byte_count":component_bytes.len(),
            "sha256":hex_hash(&component_bytes),"source_bundle_sha256":hex_hash(&source_bundle),
            "decoded_instruction_count":instructions.len(),"decoded_block_count":blocks.len()
        }))
        .expect("serialize original source component metadata"),
    );
    let mut lane_owners = BTreeSet::<(usize, usize, usize)>::new();
    for (actor, actor_lane) in lanes["actors"]
        .as_array()
        .expect("actor lanes")
        .iter()
        .enumerate()
    {
        for event in actor_lane["events"].as_array().expect("event lanes") {
            let event_id = integer(&event["event"], "event id");
            for pc in event["instruction_pcs"]
                .as_array()
                .expect("lane instruction PCs")
            {
                lane_owners.insert((actor, event_id, integer(pc, "instruction PC")));
            }
        }
    }

    // M5 established visible message 1 at map 23 PC 665 (message_centred).
    // Find the actual message operand from authoritative instruction metadata,
    // then place the CALL after this ordinary wait instruction.
    let (message_pc, message_operand_index, old_message) = lane_owners
        .iter()
        .find_map(|(_actor, _event, owner_pc)| {
            let message = instructions.iter().find(|instruction| {
                integer(&instruction["pc"], "instruction PC") == *owner_pc
                    && instruction["spec"]["mnemonic"] == "message_centred"
            })?;
            let operand_specs = message["spec"]["operands"].as_array()?;
            let message_operand_index = operand_specs
                .iter()
                .position(|operand| operand["kind"] == "msg")?;
            let old_message = integer(&message["operands"][message_operand_index], "message ID");
            if old_message != 1 {
                return None;
            }
            let message_pc = integer(&message["pc"], "message instruction PC");
            let message_size = integer(&message["size"], "message instruction size");
            let message_code = blocks.iter().any(|block| {
                integer(&block["start"], "block start") <= message_pc
                    && message_pc < integer(&block["end"], "block end")
                    && block["kind"] == "code"
            });
            let message_successors = message["successors"].as_array()?;
            let ordinary_message_flow = match message["spec"]["flow"].as_str()? {
                "next" => {
                    message_successors.len() == 1
                        && integer(&message_successors[0], "message successor")
                            == message_pc + message_size
                }
                "wait" => {
                    message_successors.len() == 2
                        && message_successors
                            .iter()
                            .any(|target| integer(target, "message successor") == message_pc)
                        && message_successors.iter().any(|target| {
                            integer(target, "message successor") == message_pc + message_size
                        })
                }
                _ => false,
            };
            if !message_code || !ordinary_message_flow {
                return None;
            }
            Some((message_pc, message_operand_index, old_message))
        })
        .expect(
            "map 23 message 1 has a reachable message_centred wait instruction for CALL insertion",
        );
    assert!(
        old_message < message_count,
        "original map 23 message operand names a declared message"
    );
    let new_message = 0;
    assert!(
        new_message < message_count && new_message != old_message,
        "message 0 is another existing map 23 component 7 entry"
    );
    assert!(
        new_message < message_count && new_message != old_message,
        "replacement message ID is present in map 23 component 7"
    );

    // Find a real explicit terminal-only event entry with no potential incoming request.
    let requested_targets: BTreeSet<(usize, usize)> = lanes["requests"]
        .as_array()
        .expect("event request facts")
        .iter()
        .filter_map(|request| {
            request["target"].as_object().map(|target| {
                (
                    integer(&target["actor"], "request target actor"),
                    integer(&target["event"], "request target event"),
                )
            })
        })
        .collect();
    let terminal = graph["entries"]
        .as_array()
        .expect("field event entries")
        .iter()
        .find_map(|entry| {
            let actor = integer(&entry["actor"], "terminal actor");
            let event = integer(&entry["event"], "terminal event");
            let pc = integer(&entry["pc"], "terminal PC");
            let instruction = instructions
                .iter()
                .find(|instruction| integer(&instruction["pc"], "instruction PC") == pc)?;
            if event < 4
                || instruction["spec"]["mnemonic"] != "end_slot"
                || requested_targets.contains(&(actor, event))
            {
                return None;
            }
            let lane = lanes["actors"][actor]["events"]
                .as_array()?
                .iter()
                .find(|lane| integer(&lane["event"], "event id") == event)?;
            if lane["instruction_pcs"] != json!([pc]) || lane["skipped"] != false {
                return None;
            }
            Some((actor, event, pc))
        })
        .expect("map 23 has an unrequested explicit actor/event slot containing only end_slot");

    // The field bank is 0x400 signed halfwords; map-local state occupies 0x200..0x3ff.
    // Choose a slot for which lanes finds no direct variable reference in this map.
    let referenced: BTreeSet<usize> = lanes["actors"]
        .as_array()
        .expect("actor lanes")
        .iter()
        .flat_map(|actor| actor["events"].as_array().unwrap())
        .flat_map(|event| event["variable_accesses"].as_array().into_iter().flatten())
        .filter(|access| access["resolution"] == "resolved")
        .filter_map(|access| access["slot"].as_u64().map(|slot| slot as usize))
        .collect();
    let variable_slot = FIELD_VARIABLES
        .clone()
        .find(|slot| !referenced.contains(slot))
        .expect("map-local variable bank has a lane-unreferenced slot");
    assert!(
        variable_slot
            < graph["variable_unsigned_bits"]
                .as_array()
                .expect("variable bank bitset")
                .len()
                * 8,
        "selected variable slot fits the decoded 0x400-halfword bank"
    );

    let draft_open = ok(
        &command(&studio, "field.draft.open", json!({"graph":graph})),
        "open map 23 authored draft",
    );
    let draft = draft_open["draft"].clone();
    let variable_reference =
        u16::try_from(variable_slot * 2).expect("map-local byte reference fits the event operand");
    let set_template = ok(
        &command(
            &studio,
            "field.instruction",
            json!({
                "bytes":[0x35,0,0,0,0,0],
                "operands":[
                    {"kind":"literal","value":variable_reference},
                    {"kind":"literal","value":MARKER,"immediate":true},
                    {"kind":"literal","value":0x40}
                ]
            }),
        ),
        "encode the original set_variable opcode through typed Studio instruction fields",
    );
    let return_template = ok(
        &command(
            &studio,
            "field.instruction",
            json!({"bytes":[0x0d],"operands":[]}),
        ),
        "encode the original return opcode through Studio instruction metadata",
    );
    let call_template = ok(
        &command(
            &studio,
            "field.instruction",
            json!({
                "bytes":[0x05,0,0],"operands":[{"kind":"target","node":"new:1"}]
            }),
        ),
        "encode an address operand as a stable draft target",
    );
    let mut revision = 0;
    let event_edit = ok(
        &command(
            &studio,
            "field.draft.edit",
            json!({
                "draft":draft,"revision":revision,
                "action":{"kind":"new_event","actor":terminal.0,"event":terminal.1,"nodes":[
                    {"bytes":set_template["bytes"],"targets":set_template["targets"]},
                    {"bytes":return_template["bytes"],"targets":return_template["targets"]}
                ]}
            }),
        ),
        "author variable marker event in selected terminal-only slot",
    );
    revision += 1;
    let call_edit = ok(
        &command(
            &studio,
            "field.draft.edit",
            json!({
                "draft":draft,"revision":revision,
                "action":{"kind":"insert_after","after":format!("pc:{message_pc}"),"bytes":call_template["bytes"],"targets":call_template["targets"]}
            }),
        ),
        "insert a stable-bound call after the original visible message instruction",
    );
    revision += 1;
    let message_edit = ok(
        &command(
            &studio,
            "field.draft.edit",
            json!({
                "draft":draft,"revision":revision,
                "action":{"kind":"set_operand","node":format!("pc:{message_pc}"),"operand":message_operand_index,
                    "value":{"kind":"literal","value":new_message}}
            }),
        ),
        "change the message_centred operand to another existing message ID",
    );
    assert_eq!(
        message_edit["revision"],
        revision + 1,
        "all three authored changes advance revisions"
    );

    let compiled = ok(
        &command(&studio, "field.draft.compile", json!({"draft":draft})),
        "compile actual map 23 draft",
    );
    let compiled_bytes = byte_array(&compiled["bytes"], "compiled field component");
    let compiled_graph = &compiled["graph"];
    let bindings = &compiled["bindings"];
    let authored_set_pc = integer(&bindings["new:1"], "authored variable setter binding");
    let authored_return_pc = integer(&bindings["new:2"], "authored return binding");
    let setter = compiled_graph["instructions"]
        .as_array()
        .unwrap()
        .iter()
        .find(|instruction| integer(&instruction["pc"], "compiled PC") == authored_set_pc)
        .expect("compiled event contains the authored variable setter");
    assert_eq!(
        setter["spec"]["mnemonic"], "set_variable",
        "authored instruction uses original opcode 0x35"
    );
    assert_eq!(
        setter["operands"][0], variable_reference,
        "setter names chosen map-local slot"
    );
    assert_eq!(
        setter["operands"][1], MARKER,
        "setter writes the observable marker"
    );
    assert_eq!(
        setter["immediate"][1], true,
        "setter marker is an immediate operand"
    );
    let return_instruction = compiled_graph["instructions"]
        .as_array()
        .unwrap()
        .iter()
        .find(|instruction| integer(&instruction["pc"], "compiled PC") == authored_return_pc)
        .expect("authored subroutine contains return");
    assert_eq!(
        return_instruction["spec"]["mnemonic"], "return",
        "authored subroutine returns to the source event"
    );
    let call = compiled_graph["instructions"]
        .as_array()
        .unwrap()
        .iter()
        .find(|instruction| {
            instruction["spec"]["mnemonic"] == "call"
                && instruction["operands"][0] == authored_set_pc
        })
        .expect("compiled original event calls the authored variable-setting subroutine");
    let call_pc = integer(&call["pc"], "compiled call PC");
    let message = compiled_graph["instructions"]
        .as_array()
        .unwrap()
        .iter()
        .find(|instruction| {
            integer(&instruction["pc"], "compiled PC")
                == integer(
                    &bindings[&format!("pc:{message_pc}")],
                    "relocated message binding",
                )
        })
        .expect("original message instruction remains bound");
    assert_eq!(
        message["spec"]["mnemonic"], "message_centred",
        "the original message instruction keeps its metadata-defined form"
    );
    assert_eq!(
        message["operands"][message_operand_index], new_message,
        "compiled original message uses verified existing ID"
    );
    assert_eq!(
        call_edit["revision"], 2,
        "call insertion is second authored transaction"
    );
    assert_eq!(
        event_edit["revision"], 1,
        "new event is first authored transaction"
    );

    let repacked = ok(
        &command(
            &studio,
            "field.draft.repack",
            json!({"draft":draft,"bundle":source_bundle,"context":source_context.clone(),"source":source_identity.clone(),"module_sha256":module.clone()}),
        ),
        "repack authored events into original map 23 bundle",
    );
    let modified_bundle = byte_array(&repacked["bytes"], "repacked map 23 bundle");
    let repacked_events = xem_studio::disc::extract_events(&modified_bundle)
        .expect("modified bundle event extraction succeeds")
        .expect("modified bundle retains event component");
    assert!(
        repacked_events.bytes.starts_with(&compiled_bytes),
        "repacked map component begins with the public compile result"
    );
    assert!(
        repacked_events.bytes[compiled_bytes.len()..]
            .iter()
            .all(|byte| *byte == 0),
        "explicit packed tail is zero-filled"
    );
    assert_eq!(
        repacked_events.declared_size,
        integer(&compiled["declared_size"], "compiled declared size")
    );
    let repacked_graph = ok(
        &command(
            &studio,
            "field.decode",
            json!({"bytes":repacked_events.bytes,"declared_size":repacked_events.declared_size}),
        ),
        "decode repacked field stream",
    );
    assert!(
        repacked_graph["instructions"] == compiled_graph["instructions"],
        "literal packing preserves the authored instruction graph"
    );
    assert!(
        repacked_graph["diagnostics"] == compiled_graph["diagnostics"],
        "literal packing preserves field diagnostics"
    );
    let mut non_event_components = Vec::new();
    for index in 0..9 {
        if index == COMPONENT_EVENTS {
            continue;
        }
        let size_at = 0x10c + index * 4;
        let offset_at = 0x130 + index * 4;
        let source_size = u32_at(&source_bundle, size_at);
        let result_size = u32_at(&modified_bundle, size_at);
        let source_offset = u32_at(&source_bundle, offset_at);
        let result_offset = u32_at(&modified_bundle, offset_at);
        assert!(
            source_size == result_size,
            "non-event component {index} retains its source size descriptor"
        );
        assert!(
            source_offset == result_offset,
            "non-event component {index} retains its source offset descriptor"
        );
        let source_component = decoded_component(&measured_source_bundle, index);
        let result_component = decoded_component(&modified_bundle, index);
        let component_evidence = match (source_size, source_component, result_component) {
            (0, None, None) => json!({
                "index":index,"present":false,"source_size":source_size,"result_size":result_size,
                "source_offset":source_offset,"result_offset":result_offset,
                "source_decoded_length":null,"result_decoded_length":null,
                "source_sha256":null,"result_sha256":null,"equal":true
            }),
            (size, Some(source), Some(result)) if size > 0 => {
                assert!(
                    source.len() == result.len(),
                    "non-event component {index} retains decoded length"
                );
                assert!(
                    source == result,
                    "non-event component {index} retains identical decoded bytes"
                );
                json!({
                    "index":index,"present":true,"source_size":source_size,"result_size":result_size,
                    "source_offset":source_offset,"result_offset":result_offset,
                    "source_decoded_length":source.len(),"result_decoded_length":result.len(),
                    "source_sha256":hex_hash(&source),"result_sha256":hex_hash(&result),"equal":true
                })
            }
            _ => panic!(
                "non-event component {index} has inconsistent presence or could not be decoded"
            ),
        };
        non_event_components.push(component_evidence);
    }

    let untouched_map = [22_u32, 24_u32]
        .into_iter()
        .chain(
            map_summaries
                .iter()
                .filter_map(|summary| summary["map"].as_u64().map(|map| map as u32)),
        )
        .find(|map| {
            *map != MAP
                && map_summaries
                    .iter()
                    .any(|summary| summary["map"] == *map && summary["placeholder"] == false)
        })
        .expect("disc 1 has another real map for a source-equals-result package entry");
    let untouched = map_bundle(&studio, session1, untouched_map);
    let untouched_bytes = byte_array(&untouched["bytes"], "untouched original map bundle");
    let untouched_slot =
        u32::try_from(integer(&untouched["slot"], "untouched map slot")).expect("slot fits u32");
    assert!(
        decoded_component(&untouched_bytes, COMPONENT_EVENTS).is_some(),
        "untouched map has an event component"
    );

    let disc1_target = json!({"serial":identity1["serial"],"sha256":identity1["boot_sha256"]});
    let disc2_target = json!({"serial":identity2["serial"],"sha256":identity2["boot_sha256"]});
    let built = ok(
        &command(
            &studio,
            "package.build",
            json!({
                "spec":{"id":"m6-map23-field-edit-candidate","discs":[disc1_target,disc2_target],"requires":[],"overrides":[]},
                "files":[
                    {"disc":identity1["serial"],"slot":slot23,"source":source_bundle,"result":modified_bundle},
                    {"disc":identity1["serial"],"slot":untouched_slot,"source":untouched_bytes,"result":untouched_bytes}
                ]
            }),
        ),
        "build private M6 candidate package",
    );
    let package_bytes = byte_array(&built["bytes"], "built candidate package");
    let manifest_files = built["manifest"]["files"]
        .as_array()
        .expect("candidate package manifest files");
    assert_eq!(
        manifest_files.len(),
        2,
        "package contains only map 23 and the explicit unchanged map"
    );
    assert!(
        manifest_files
            .iter()
            .all(|file| file["kind"] == "delta" && file["source_sha256"].as_str().is_some()),
        "package payloads are source-checked deltas and contain no original-file members"
    );
    let set = ok(
        &command(
            &studio,
            "package.bundle",
            json!({"packages":[package_bytes]}),
        ),
        "bundle one private candidate package",
    );
    let set_bytes = byte_array(&set["bytes"], "candidate package set");
    let export = ok(
        &command(&studio, "field.draft.export", json!({"draft":draft})),
        "export candidate draft document",
    );
    let all_non_event_components_equal = non_event_components
        .iter()
        .all(|component| component["equal"] == true);
    assert!(
        all_non_event_components_equal,
        "all eight unaffected component streams were verified before reporting preservation"
    );
    write_new(&output.join("candidate.xemmods"), &package_bytes);
    write_new(&output.join("candidate.xemset"), &set_bytes);
    write_new(
        &output.join("candidate-draft.json"),
        &serde_json::to_vec_pretty(&export).expect("serialize candidate draft"),
    );
    write_new(&output.join("map23-result.bundle"), &modified_bundle);
    write_new(
        &output.join("visible-message0.json"),
        &serde_json::to_vec_pretty(&message_zero_evidence)
            .expect("serialize private visible-message evidence"),
    );
    write_new(
        &output.join("visible-message1.json"),
        &serde_json::to_vec_pretty(&message_one_evidence)
            .expect("serialize private visible-message evidence"),
    );
    let evidence = json!({
        "fixture":"private real-disc M6 service candidate; runtime playback not asserted",
        "disc1":{"serial":identity1["serial"],"boot_sha256":identity1["boot_sha256"]},
        "disc2":{"serial":identity2["serial"],"boot_sha256":identity2["boot_sha256"]},
        "map":MAP,"slot":slot23,"source_bundle_sha256":hex_hash(&source_bundle),
        "result_bundle_sha256":hex_hash(&modified_bundle),
        "source_event_component_sha256":hex_hash(&component_bytes),
        "result_event_component_sha256":hex_hash(&repacked_events.bytes),
        "message_count":message_count,"message_pc":message_pc,"message_mnemonic":"message_centred",
        "message_operand_index":message_operand_index,"call_inserted_after_message":true,
        "original_message":old_message,"edited_message":new_message,
        "visible_message0":message_zero_evidence,"visible_message1":message_one_evidence,
        "terminal_event":{"actor":terminal.0,"event":terminal.1,"pc":terminal.2,"original_lane_only_end_slot":true,"no_incoming_event_request_fact":true},
        "authored_variable":{"bank_index":variable_slot,"operand_reference":variable_reference,"marker":MARKER,"no_direct_lane_reference_in_map":true},
        "call_pc":call_pc,"subroutine_set_pc":authored_set_pc,"subroutine_return_pc":authored_return_pc,
        "untouched_map":{"map":untouched_map,"slot":untouched_slot,"bundle_sha256":hex_hash(&untouched_bytes),"source_equals_result":true},
        "package_sha256":hex_hash(&package_bytes),"package_set_sha256":hex_hash(&set_bytes),
        "package_files_are_deltas":true,"non_event_components":non_event_components,
        "non_event_components_equal":all_non_event_components_equal,
        "source_context_sha256":context_sha256,"source_context_module_sha256":module,
        "source_context_component_output_sha256":source_context.components.iter().map(|component| (component.index.to_string(),Value::String(component.output_sha256.clone()))).collect::<serde_json::Map<_,_>>()
    });
    write_new(
        &output.join("evidence.json"),
        &serde_json::to_vec_pretty(&evidence).expect("serialize private evidence"),
    );
}
