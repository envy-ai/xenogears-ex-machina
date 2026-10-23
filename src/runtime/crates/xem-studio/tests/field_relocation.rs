use serde_json::{Value, json};
use std::collections::BTreeMap;
use xem_studio::field::draft::{Action, Document, Draft, NodeInput, OperandValue};
use xem_studio::field::{self, FieldGraph};
use xem_studio::{Studio, disc, packed};

const BYTECODE_OFFSET: usize = 132 + 64;
const EVENT_SIZE_OFFSET: usize = 0x10c + 4 * 5;
const EVENT_START_OFFSET: usize = 0x130 + 4 * 5;
const BUNDLE_HEADER: usize = 0x154;

fn component(code: &[u8], entries: &[(usize, u16)]) -> Vec<u8> {
    let mut bytes = vec![0_u8; BYTECODE_OFFSET + code.len()];
    bytes[128..132].copy_from_slice(&1_u32.to_le_bytes());
    for &(event, pc) in entries {
        let offset = 132 + event * 2;
        bytes[offset..offset + 2].copy_from_slice(&pc.to_le_bytes());
    }
    bytes[BYTECODE_OFFSET..].copy_from_slice(code);
    bytes
}

fn synthetic_graph(code: &[u8], entries: &[(usize, u16)]) -> (Vec<u8>, FieldGraph) {
    let bytes = component(code, entries);
    let graph =
        field::decode(&bytes, Some(bytes.len())).expect("synthetic field component decodes");
    (bytes, graph)
}

fn assert_error<T>(result: Result<T, field::FieldError>, context: &str) {
    let error = match result {
        Err(error) => error,
        Ok(_) => panic!("operation should be refused with {context}"),
    };
    assert!(
        error.kind.contains(context) || error.message.contains(context),
        "expected {context} error, got {}: {}",
        error.kind,
        error.message
    );
}

fn bytes(value: &[u8]) -> Vec<u8> {
    value.to_vec()
}

fn inserted_after(after: &str, template: &[u8], targets: BTreeMap<usize, String>) -> Action {
    Action::InsertAfter {
        after: after.to_owned(),
        bytes: bytes(template),
        targets,
    }
}

fn node_binding(compiled: &xem_studio::field::draft::Compiled, id: &str) -> usize {
    *compiled
        .bindings
        .get(id)
        .expect("stable node binding exists")
}

fn trace_fallthrough_to(compiled: &xem_studio::field::draft::Compiled, start: usize, end: usize) {
    let mut pc = start;
    for _ in 0..=compiled.graph.instructions.len() {
        if pc == end {
            return;
        }
        let instruction = compiled
            .graph
            .instructions
            .iter()
            .find(|instruction| instruction.pc == pc)
            .expect("compiled path reaches a decoded instruction");
        assert_eq!(
            instruction.successors.len(),
            1,
            "fallthrough/bridge path is unambiguous"
        );
        pc = instruction.successors[0];
    }
    panic!("compiled path did not reach original continuation pc {end}");
}

#[test]
fn authored_jump_requires_a_stable_target_and_tracks_relocated_source_pc() {
    let (source, graph) = synthetic_graph(&[0x0e, 0x0e, 0], &[]);
    let mut draft = Draft::open(graph).expect("open synthetic graph");
    let before = draft.compile().expect("original compiles");

    let missing = draft.edit(0, inserted_after("pc:0", &[0x01, 1, 0], BTreeMap::new()));
    assert_error(missing, "address operand");
    assert_eq!(draft.snapshot(1).unwrap()["revision"], 0);
    assert_eq!(
        draft.compile().unwrap().bytes,
        before.bytes,
        "refusal is atomic"
    );

    let mut targets = BTreeMap::new();
    targets.insert(0, "pc:1".to_owned());
    draft
        .edit(0, inserted_after("pc:0", &[0x01, 1, 0], targets))
        .expect("explicit stable target allows an authored jump");
    let compiled = draft.compile().expect("relocated source target compiles");
    let jump_pc = node_binding(&compiled, "new:1");
    let source_target_pc = node_binding(&compiled, "pc:1");
    assert_ne!(
        source_target_pc, 1,
        "the target source instruction moved into the append island"
    );
    let jump = compiled
        .graph
        .instructions
        .iter()
        .find(|instruction| instruction.pc == jump_pc)
        .expect("authored jump is decoded");
    assert_eq!(jump.operands[0] as usize, source_target_pc);
    assert_eq!(jump.successors, vec![source_target_pc]);
    assert_ne!(compiled.bytes, source);
}

#[test]
fn new_event_is_reachable_through_an_existing_cross_event_call() {
    let (_, graph) = synthetic_graph(&[0x05, 4, 0, 0x0e, 0], &[(0, 0), (1, 4)]);
    let mut draft = Draft::open(graph).expect("open synthetic graph");
    draft
        .edit(
            0,
            Action::NewEvent {
                actor: 0,
                event: 1,
                nodes: vec![
                    NodeInput {
                        bytes: vec![0x0e],
                        targets: BTreeMap::new(),
                    },
                    NodeInput {
                        bytes: vec![0],
                        targets: BTreeMap::new(),
                    },
                ],
            },
        )
        .expect("replace explicit terminal event slot");
    draft
        .edit(
            1,
            Action::SetOperand {
                node: "pc:0".into(),
                operand: 0,
                value: OperandValue::Target {
                    node: "new:1".into(),
                },
            },
        )
        .expect("bind existing call to the authored event");
    let compiled = draft.compile().expect("cross-event call compiles");
    let event_pc = node_binding(&compiled, "new:1");
    let call = compiled
        .graph
        .instructions
        .iter()
        .find(|instruction| instruction.pc == node_binding(&compiled, "pc:0"))
        .expect("existing call remains in compiled graph");
    assert_eq!(call.spec.mnemonic, "call");
    assert_eq!(call.operands[0] as usize, event_pc);
    assert!(call.successors.contains(&event_pc));
    assert!(
        compiled
            .graph
            .entries
            .iter()
            .any(|entry| entry.actor == 0 && entry.event == 1 && entry.pc == event_pc)
    );
}

fn paired_graph() -> (Vec<u8>, FieldGraph) {
    synthetic_graph(&[0x10, 0, 1, 0, 2, 0, 3, 0, 0, 0x10, 1, 0], &[(0, 0)])
}

#[test]
fn same_size_compound_operand_edit_and_complete_pair_relocation_are_supported() {
    let (source, graph) = paired_graph();
    let mut operand_edit = Draft::open(graph.clone()).expect("open paired graph");
    operand_edit
        .edit(
            0,
            Action::SetOperand {
                node: "pc:0".into(),
                operand: 1,
                value: OperandValue::Literal {
                    value: 9,
                    immediate: None,
                },
            },
        )
        .expect("same-form selector operand in a compound unit remains editable");
    let changed = operand_edit
        .compile()
        .expect("in-place compound edit compiles");
    assert_eq!(changed.bytes[BYTECODE_OFFSET + 2], 9);
    assert_eq!(changed.graph.diagnostics.unpaired, Vec::<usize>::new());
    assert_eq!(
        changed
            .graph
            .instructions
            .iter()
            .find(|ins| ins.pc == 0)
            .unwrap()
            .spec
            .mnemonic,
        "walk_to",
        "same-size operand edit must retain the original selected form"
    );
    assert_eq!(
        changed
            .graph
            .instructions
            .iter()
            .find(|ins| ins.pc == 9)
            .unwrap()
            .spec
            .mnemonic,
        "walk_step"
    );

    let mut relocation = Draft::open(graph).expect("open paired graph");
    relocation
        .edit(0, inserted_after("pc:9", &[0x0e], BTreeMap::new()))
        .expect("insertion after the full setup/consumer pair is valid");
    let compiled = relocation
        .compile()
        .expect("whole pair relocation compiles");
    let inserted_pc = node_binding(&compiled, "new:1");
    trace_fallthrough_to(&compiled, inserted_pc, node_binding(&compiled, "pc:11"));
    assert_eq!(compiled.graph.diagnostics.unpaired, Vec::<usize>::new());
    assert_eq!(
        &source[BYTECODE_OFFSET..BYTECODE_OFFSET + 11],
        &[0x10, 0, 1, 0, 2, 0, 3, 0, 0, 0x10, 1]
    );
}

#[test]
fn same_size_edits_cannot_activate_a_skip_into_an_instruction_operand() {
    let cases = [
        (
            "zero skip",
            vec![
                0x9a, 0x01, 0x80, // immediate nonzero: no alternate advance
                0x10, 0, 1, 0, 2, 0, 3, 0, 0, // walk_to spans target PC 6
                0x10, 1, 0, // walk_step
                0x00,
            ],
            Action::SetOperand {
                node: "pc:0".into(),
                operand: 0,
                value: OperandValue::Literal {
                    value: 0,
                    immediate: Some(true),
                },
            },
        ),
        (
            "party skip",
            vec![
                0xfc, 0, 0, 0, 0, // direct actor: no party alternate advance
                0x03, 0, 0, 5, // message_box spans target PC 6
                0x00,
            ],
            Action::SetOperand {
                node: "pc:0".into(),
                operand: 0,
                value: OperandValue::Literal {
                    value: 0xfd,
                    immediate: None,
                },
            },
        ),
    ];

    for (label, code, action) in cases {
        let (_, graph) = synthetic_graph(&code, &[]);
        assert!(
            graph.diagnostics.into.is_empty(),
            "{label} starts without an into edge"
        );
        assert!(
            graph.instructions[0].skips.is_empty(),
            "{label} starts inactive"
        );
        let mut draft = Draft::open(graph).expect("open inactive skip graph");
        let before = draft.compile().expect("inactive source compiles");
        let refused = draft.edit(0, action);
        let error = refused.expect_err("new relative skip into an instruction operand is unsafe");
        assert_eq!(error.kind, "unsupported_relocation", "{label} refusal kind");
        assert_eq!(
            draft.snapshot(1).unwrap()["revision"],
            0,
            "{label} edit is atomic"
        );
        assert_eq!(
            draft.compile().unwrap().bytes,
            before.bytes,
            "{label} bytes remain unchanged"
        );
    }
}

#[test]
fn changing_a_selector_to_another_instruction_form_is_refused_atomically() {
    let (source, graph) = paired_graph();
    let mut draft = Draft::open(graph).expect("open paired graph");
    let refused = draft.edit(
        0,
        Action::SetOperand {
            node: "pc:0".into(),
            operand: 0,
            value: OperandValue::Literal {
                value: 1,
                immediate: None,
            },
        },
    );
    assert_error(refused, "instruction form");
    assert_eq!(draft.snapshot(1).unwrap()["revision"], 0);
    assert_eq!(draft.compile().unwrap().bytes, source);
}

#[test]
fn fixed_relative_skip_windows_and_pinned_overlaps_refuse_edits_atomically() {
    let (source, graph) = synthetic_graph(&[0x9a, 0, 0, 0x01, 6, 0, 0], &[(0, 0)]);
    let mut skip = Draft::open(graph).expect("open skip graph");
    assert_eq!(skip.compile().unwrap().graph.instructions[0].skips, vec![6]);
    let refused = skip.edit(0, inserted_after("pc:0", &[0x0e], BTreeMap::new()));
    assert_error(refused, "fixed-relative");
    assert_eq!(skip.snapshot(1).unwrap()["revision"], 0);
    assert_eq!(skip.compile().unwrap().bytes, source);

    let (_, overlap_graph) = synthetic_graph(&[0x02, 0, 0, 0, 0, 0, 2, 0, 0], &[(0, 0)]);
    let mut overlap = Draft::open(overlap_graph).expect("open overlap graph");
    let pinned = overlap.edit(
        0,
        Action::SetOperand {
            node: "pc:0".into(),
            operand: 3,
            value: OperandValue::Target {
                node: "pc:2".into(),
            },
        },
    );
    assert_error(pinned, "pinned");
    assert_eq!(overlap.snapshot(1).unwrap()["revision"], 0);
}

#[test]
fn append_cannot_capture_an_existing_outside_destination() {
    let (source, graph) = synthetic_graph(&[0x02, 0, 0, 0, 0, 0, 10, 0, 0x0e, 0], &[(0, 0)]);
    assert_eq!(graph.diagnostics.outside, vec![[0, 10]]);
    let mut draft = Draft::open(graph).expect("open outside-destination graph");
    let result = draft.edit(0, inserted_after("pc:8", &[0x0e], BTreeMap::new()));
    assert_error(result, "activate a formerly unresolved destination");
    assert_eq!(draft.snapshot(1).unwrap()["revision"], 0);
    assert_eq!(draft.compile().unwrap().bytes, source);
}

#[test]
fn append_rejects_bytecode_growth_past_u16_address_space() {
    let code = vec![0x0e; 32_768];
    let (source, graph) = synthetic_graph(&code, &[(0, 0)]);
    let mut draft = Draft::open(graph).expect("open bounded synthetic graph");
    let result = draft.edit(0, inserted_after("pc:0", &[0x0e], BTreeMap::new()));
    assert_error(result, "u16 PC address space");
    assert_eq!(draft.snapshot(1).unwrap()["revision"], 0);
    assert_eq!(draft.compile().unwrap().bytes, source);
}

#[test]
fn batch_edits_are_bounded_non_nested_and_atomic() {
    let (_, graph) = synthetic_graph(&[0x03, 0, 0, 1, 0], &[(0, 0)]);
    let mut draft = Draft::open(graph).expect("open batch graph");
    let before = draft.compile().unwrap();

    let partial = draft.edit(
        0,
        Action::Batch {
            actions: vec![
                inserted_after("pc:0", &[0x0e], BTreeMap::new()),
                Action::SetOperand {
                    node: "pc:0".into(),
                    operand: 99,
                    value: OperandValue::Literal {
                        value: 1,
                        immediate: None,
                    },
                },
            ],
        },
    );
    assert_error(partial, "operand index");
    assert_eq!(draft.snapshot(1).unwrap()["revision"], 0);
    assert_eq!(draft.compile().unwrap().bytes, before.bytes);

    let nested = draft.edit(
        0,
        Action::Batch {
            actions: vec![Action::Batch {
                actions: vec![inserted_after("pc:0", &[0x0e], BTreeMap::new())],
            }],
        },
    );
    assert_error(nested, "batch requires");
    let oversized = draft.edit(
        0,
        Action::Batch {
            actions: (0..33)
                .map(|_| Action::SetOperand {
                    node: "pc:0".into(),
                    operand: 1,
                    value: OperandValue::Literal {
                        value: 1,
                        immediate: None,
                    },
                })
                .collect(),
        },
    );
    assert_error(oversized, "batch requires");
    assert_eq!(draft.compile().unwrap().bytes, before.bytes);

    draft
        .edit(
            0,
            Action::Batch {
                actions: vec![
                    Action::SetOperand {
                        node: "pc:0".into(),
                        operand: 1,
                        value: OperandValue::Literal {
                            value: 6,
                            immediate: None,
                        },
                    },
                    Action::SetOperand {
                        node: "pc:0".into(),
                        operand: 1,
                        value: OperandValue::Literal {
                            value: 7,
                            immediate: None,
                        },
                    },
                ],
            },
        )
        .expect("valid batch commits once");
    assert_eq!(draft.snapshot(1).unwrap()["revision"], 1);
    let compiled = draft.compile().unwrap();
    let pc = compiled.bindings["pc:0"];
    assert_eq!(compiled.bytes[BYTECODE_OFFSET + pc + 3], 7);
}

#[test]
fn export_import_preserves_full_bindings_target_history_and_rejects_bad_watermarks() {
    let (_, graph) = synthetic_graph(
        &[0x03, 0, 0, 5, 0x02, 0, 0, 0, 0, 0, 13, 0, 0x0e, 0],
        &[(1, 13)],
    );
    let mut draft = Draft::open(graph).expect("open import graph");
    draft
        .edit(0, inserted_after("pc:0", &[0x0e], BTreeMap::new()))
        .unwrap();
    draft.undo(1).unwrap();
    draft
        .edit(2, inserted_after("pc:0", &[0x0e], BTreeMap::new()))
        .unwrap();
    draft
        .edit(
            3,
            Action::SetOperand {
                node: "pc:4".into(),
                operand: 3,
                value: OperandValue::Target {
                    node: "new:2".into(),
                },
            },
        )
        .unwrap();
    let expected = draft.compile().unwrap();
    let document = draft.document();
    let imported = Draft::import(document.clone()).expect("valid history imports");
    let actual = imported.compile().expect("imported history compiles");
    assert_eq!(
        actual.bindings, expected.bindings,
        "all stable node bindings survive import"
    );
    assert_eq!(
        actual.bytes, expected.bytes,
        "compiled output survives import exactly"
    );
    let branch = actual
        .graph
        .instructions
        .iter()
        .find(|ins| ins.pc == actual.bindings["pc:4"])
        .unwrap();
    assert_eq!(branch.operands[3] as usize, actual.bindings["new:2"]);

    let mut json = serde_json::to_value(&document).unwrap();
    json.as_object_mut().unwrap().remove("next_id");
    assert!(
        serde_json::from_value::<Document>(json).is_err(),
        "required next_id must not default"
    );
    let mut json = serde_json::to_value(&document).unwrap();
    json.as_object_mut().unwrap().remove("allocation_starts");
    assert!(
        serde_json::from_value::<Document>(json).is_err(),
        "required allocation history must not default"
    );

    let mut reused = document.clone();
    reused.next_id = 2;
    assert_error(Draft::import(reused), "reuses");
    let mut invalid_start = document;
    invalid_start.allocation_starts[0] = 0;
    assert_error(Draft::import(invalid_start), "reuses");
}

#[test]
fn literal_packer_round_trips_padding_and_reads_the_explicit_terminal_flag() {
    for input in [Vec::new(), vec![1, 2, 3, 4, 5, 6, 7, 8, 9]] {
        let encoded = packed::encode_literals(&input, 1024).expect("bounded literal encoding");
        assert_eq!(
            encoded.last(),
            Some(&0),
            "encoded source includes terminal flag byte"
        );
        let decoded = packed::decode_block(&encoded, 1024).expect("explicit terminal flag decodes");
        let expected_len = (input.len() + 7) & !7;
        assert_eq!(decoded.data.len(), expected_len);
        assert_eq!(&decoded.data[..input.len()], input.as_slice());
        assert!(decoded.data[input.len()..].iter().all(|byte| *byte == 0));
        assert_eq!(decoded.source_bytes_read, encoded.len());
    }
}

fn event_bundle(event_bytes: &[u8], declared_size: usize) -> Vec<u8> {
    const OTHER_INDEX: usize = 8;
    let other_component = [0x31, 0x42, 0x53, 0x64, 0x75];
    let other_encoded =
        packed::encode_literals(&other_component, 1 << 20).expect("synthetic component 8 pack");
    let other_start = 0x180;
    let event_start = (other_start + other_encoded.len() + 3) & !3;
    assert!(event_start >= BUNDLE_HEADER);
    let mut bundle: Vec<u8> = (0..event_start)
        .map(|index| (index as u8).wrapping_mul(17))
        .collect();
    bundle[0x10c..0x130].fill(0);
    bundle[0x130..0x154].fill(0);
    bundle[0x10c + OTHER_INDEX * 4..0x10c + OTHER_INDEX * 4 + 4]
        .copy_from_slice(&(other_component.len() as u32).to_le_bytes());
    bundle[0x130 + OTHER_INDEX * 4..0x130 + OTHER_INDEX * 4 + 4]
        .copy_from_slice(&(other_start as u32).to_le_bytes());
    bundle[EVENT_SIZE_OFFSET..EVENT_SIZE_OFFSET + 4]
        .copy_from_slice(&(declared_size as u32).to_le_bytes());
    bundle[EVENT_START_OFFSET..EVENT_START_OFFSET + 4]
        .copy_from_slice(&(event_start as u32).to_le_bytes());
    bundle[other_start..other_start + other_encoded.len()].copy_from_slice(&other_encoded);
    let encoded = packed::encode_literals(event_bytes, 1 << 20).expect("synthetic event pack");
    bundle.extend_from_slice(&encoded);
    bundle
}

fn decoded_bundle_component(bundle: &[u8], index: usize) -> Vec<u8> {
    let size_offset = 0x10c + index * 4;
    let start_offset = 0x130 + index * 4;
    let size =
        u32::from_le_bytes(bundle[size_offset..size_offset + 4].try_into().unwrap()) as usize;
    let start =
        u32::from_le_bytes(bundle[start_offset..start_offset + 4].try_into().unwrap()) as usize;
    packed::decode_block(&bundle[start..], size + 16)
        .expect("synthetic component decodes")
        .data
}

#[test]
fn replacement_rejects_compression_padding_that_activates_an_outside_jump() {
    let code = [
        0x03, 0, 0, 0, // message_box
        0x01, 16, 0, // jump to PC 16, outside this 12-byte code
        0x00, 0, 0, 0, 0, // end and opaque tail
    ];
    let raw_component = component(&code, &[(1, 7)]);
    let original_block = packed::encode_literals(&raw_component, 1 << 20).unwrap();
    let original_decoded = packed::decode_block(&original_block, 1 << 20).unwrap();
    let declared_size = raw_component.len();
    let base = field::decode(&original_decoded.data, Some(declared_size)).unwrap();
    assert_eq!(base.diagnostics.outside, vec![[4, 16]]);
    let bundle = event_bundle(&original_decoded.data, declared_size);

    let mut draft = Draft::open(base.clone()).unwrap();
    draft
        .edit(
            0,
            Action::NewEvent {
                actor: 0,
                event: 1,
                nodes: vec![
                    NodeInput {
                        bytes: vec![0x0e],
                        targets: BTreeMap::new(),
                    },
                    NodeInput {
                        bytes: vec![0],
                        targets: BTreeMap::new(),
                    },
                ],
            },
        )
        .expect("appended event remains outside the original jump target");
    let compiled = draft.compile().unwrap();
    assert_eq!(compiled.graph.diagnostics.outside, base.diagnostics.outside);
    assert_eq!(
        compiled.graph.stream_size - compiled.graph.bytecode_offset,
        14
    );
    let error = disc::replace_events(&bundle, &base, &compiled)
        .expect_err("final literal padding must not activate PC 16");
    assert_eq!(error.kind, "unsupported_relocation");
}

#[test]
fn insertion_inside_authored_event_preserves_return_as_its_terminal_instruction() {
    let (_, graph) = synthetic_graph(&[0x0e, 0x00], &[(0, 0), (1, 1)]);
    let mut draft = Draft::open(graph).expect("open synthetic field");
    draft
        .edit(
            0,
            Action::NewEvent {
                actor: 0,
                event: 1,
                nodes: vec![
                    NodeInput {
                        bytes: vec![0x0e],
                        targets: BTreeMap::new(),
                    },
                    NodeInput {
                        bytes: vec![0x0d],
                        targets: BTreeMap::new(),
                    },
                ],
            },
        )
        .expect("explicit terminal-only slot accepts authored NOP and return");
    draft
        .edit(1, inserted_after("new:1", &[0x0f], BTreeMap::new()))
        .expect("insert_after can target a node inside the authored event");

    let compiled = draft.compile().expect("edited authored event compiles");
    let first = node_binding(&compiled, "new:1");
    let inserted = node_binding(&compiled, "new:3");
    let returned = node_binding(&compiled, "new:2");
    let instruction = |pc| {
        compiled
            .graph
            .instructions
            .iter()
            .find(|instruction| instruction.pc == pc)
            .expect("authored stable binding decodes")
    };
    assert_eq!(instruction(first).spec.mnemonic, "nop");
    assert_eq!(instruction(inserted).spec.mnemonic, "nop");
    assert_eq!(instruction(returned).spec.mnemonic, "return");
    assert_eq!(instruction(first).successors, vec![inserted]);
    assert_eq!(instruction(inserted).successors, vec![returned]);
    assert!(instruction(returned).successors.is_empty());
    assert!(
        compiled
            .graph
            .entries
            .iter()
            .any(|entry| { entry.actor == 0 && entry.event == 1 && entry.pc == first })
    );
}

fn studio_reply(studio: &Studio, op: &str, args: Value) -> Value {
    serde_json::from_str(&studio.command(&json!({"op": op, "args": args}).to_string()))
        .expect("Studio command returns JSON")
}

fn studio_ok(studio: &Studio, op: &str, args: Value) -> Value {
    let response = studio_reply(studio, op, args);
    response
        .get("ok")
        .unwrap_or_else(|| panic!("{op} failed with {}", response["error"]["message"]))
        .clone()
}

#[test]
fn command_boundary_preserves_inserted_address_targets_across_export_import() {
    let studio = Studio::new();
    let source = component(&[0x03, 0, 0, 1, 0], &[(0, 0), (1, 4)]);
    let graph = studio_ok(
        &studio,
        "field.decode",
        json!({"bytes":source,"declared_size":source.len()}),
    );
    let target_template = studio_ok(
        &studio,
        "field.instruction",
        json!({
            "bytes":[0x05,0,0],
            "operands":[{"kind":"target","node":"pc:0"}]
        }),
    );
    assert_eq!(target_template["targets"]["0"], "pc:0");
    let opened = studio_ok(&studio, "field.draft.open", json!({"graph":graph}));
    let inserted = studio_ok(
        &studio,
        "field.draft.edit",
        json!({
            "draft":opened["draft"],
            "revision":opened["revision"],
            "action":{
                "kind":"insert_after",
                "after":"pc:0",
                "bytes":target_template["bytes"],
                "targets":target_template["targets"]
            }
        }),
    );
    let compiled = studio_ok(
        &studio,
        "field.draft.compile",
        json!({"draft":opened["draft"]}),
    );
    let call_binding = inserted["bindings"]["new:1"]
        .as_u64()
        .expect("inserted call has a stable binding");
    let source_binding = inserted["bindings"]["pc:0"]
        .as_u64()
        .expect("source target has a stable binding");
    let call = compiled["graph"]["instructions"]
        .as_array()
        .unwrap()
        .iter()
        .find(|instruction| instruction["pc"] == call_binding)
        .expect("typed inserted call is present in compiled graph");
    assert_eq!(call["spec"]["mnemonic"], "call");
    assert_eq!(
        call["operands"][0], source_binding,
        "address operand resolves to the selected source node"
    );

    let document = studio_ok(
        &studio,
        "field.draft.export",
        json!({"draft":opened["draft"]}),
    );
    let imported = studio_ok(&studio, "field.draft.import", json!({"document":document}));
    let imported_compile = studio_ok(
        &studio,
        "field.draft.compile",
        json!({"draft":imported["draft"]}),
    );
    let imported_binding = imported["bindings"]["new:1"]
        .as_u64()
        .expect("import preserves the inserted call ID");
    let imported_call = imported_compile["graph"]["instructions"]
        .as_array()
        .unwrap()
        .iter()
        .find(|instruction| instruction["pc"] == imported_binding)
        .expect("imported call remains in the compiled graph");
    let imported_source = imported["bindings"]["pc:0"]
        .as_u64()
        .expect("import preserves the original target binding");
    assert_eq!(
        imported_call["operands"][0], imported_source,
        "export/import preserves the nonempty target map"
    );
    assert_eq!(
        imported_compile["bytes"], compiled["bytes"],
        "target relocation remains byte-identical after export/import"
    );
}

#[test]
fn public_templates_and_typed_instruction_encoding_match_the_decoder_metadata() {
    let studio = Studio::new();
    let templates: Value = serde_json::from_str(&studio.command(r#"{"op":"field.templates"}"#))
        .expect("Studio returns template JSON");
    let templates = templates
        .get("ok")
        .unwrap_or_else(|| panic!("field.templates failed: {templates}"));
    let rows = templates["templates"]
        .as_array()
        .expect("template palette is an array");
    assert!(!rows.is_empty(), "metadata exposes standalone forms");

    for (row_index, template) in rows.iter().enumerate() {
        let bytes = template["bytes"]
            .as_array()
            .expect("template bytes are an array")
            .iter()
            .map(|value| value.as_u64().expect("template byte is numeric") as u8)
            .collect::<Vec<_>>();
        let mut context = vec![0_u8; 512];
        context[..bytes.len()].copy_from_slice(&bytes);
        let decoded = field::decode_instruction(&context, 0)
            .unwrap_or_else(|error| panic!("template {row_index} decode failed: {error}"));
        assert_eq!(
            serde_json::to_value(&decoded).unwrap(),
            template["instruction"],
            "template {row_index} exposes the exact decoder instruction schema"
        );

        let operands = template["instruction"]["spec"]["operands"]
            .as_array()
            .expect("instruction operand metadata is an array")
            .iter()
            .enumerate()
            .map(|(index, descriptor)| match descriptor["kind"].as_str() {
                Some("addr") => json!({"kind": "target", "node": "pc:0"}),
                Some(_) => json!({
                    "kind": "literal",
                    "value": template["instruction"]["operands"][index],
                }),
                None => panic!("template {row_index} operand kind missing"),
            })
            .collect::<Vec<_>>();
        let encoded = studio_reply(
            &studio,
            "field.instruction",
            json!({"bytes": bytes, "operands": operands}),
        );
        let encoded = encoded
            .get("ok")
            .unwrap_or_else(|| panic!("field.instruction template {row_index} failed: {encoded}"));
        assert_eq!(encoded["bytes"], template["bytes"], "template {row_index}");
        assert_eq!(
            encoded["targets"]
                .as_object()
                .is_some_and(|targets| !targets.is_empty()),
            template["instruction"]["spec"]["operands"]
                .as_array()
                .is_some_and(|operands| operands.iter().any(|operand| operand["kind"] == "addr")),
            "template {row_index} address fields are represented by stable-target bindings"
        );
    }
}

#[test]
fn replacing_events_checks_source_identity_and_preserves_other_bundle_bytes() {
    let raw_component = component(&[0x0e, 0x0e, 0], &[(1, 2)]);
    let original_block = packed::encode_literals(&raw_component, 1 << 20).unwrap();
    let original_decoded = packed::decode_block(&original_block, 1 << 20).unwrap();
    let declared_size = raw_component.len();
    let base = field::decode(&original_decoded.data, Some(declared_size)).unwrap();
    let bundle = event_bundle(&original_decoded.data, declared_size);
    let original_component8 = decoded_bundle_component(&bundle, 8);

    let unchanged = Draft::open(base.clone()).unwrap().compile().unwrap();
    assert_eq!(
        disc::replace_events(&bundle, &base, &unchanged).unwrap(),
        bundle
    );

    let mut draft = Draft::open(base.clone()).unwrap();
    draft
        .edit(0, inserted_after("pc:0", &[0x0e], BTreeMap::new()))
        .unwrap();
    let compiled = draft.compile().unwrap();
    let replaced =
        disc::replace_events(&bundle, &base, &compiled).expect("valid authored stream repacks");
    let changed_ranges = [
        EVENT_SIZE_OFFSET..EVENT_SIZE_OFFSET + 4,
        EVENT_START_OFFSET..EVENT_START_OFFSET + 4,
    ];
    for index in 0..bundle.len() {
        if changed_ranges.iter().any(|range| range.contains(&index)) {
            continue;
        }
        assert_eq!(
            replaced[index], bundle[index],
            "unrelated bundle byte at {index:#x} changed"
        );
    }
    let result = disc::extract_events(&replaced)
        .unwrap()
        .expect("replacement event component");
    assert_eq!(result.declared_size, compiled.declared_size);
    assert_eq!(
        &result.bytes[..compiled.bytes.len()],
        compiled.bytes.as_slice()
    );
    assert!(
        result.bytes[compiled.bytes.len()..]
            .iter()
            .all(|byte| *byte == 0)
    );
    let replaced_component8 = decoded_bundle_component(&replaced, 8);
    assert!(
        original_component8 == replaced_component8,
        "unaffected ninth component keeps identical decoded bytes"
    );

    let wrong_component = component(&[0, 0], &[(1, 1)]);
    let wrong_padded = packed::decode_block(
        &packed::encode_literals(&wrong_component, 1 << 20).unwrap(),
        1 << 20,
    )
    .unwrap();
    let wrong_base = field::decode(&wrong_padded.data, Some(wrong_component.len())).unwrap();
    let mismatch = disc::replace_events(&bundle, &wrong_base, &compiled).unwrap_err();
    assert_eq!(mismatch.kind, "source_mismatch");
}
