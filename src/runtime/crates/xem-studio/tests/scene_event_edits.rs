#[path = "support/scene_bundle.rs"]
#[allow(dead_code)]
mod fixture;

use std::collections::BTreeMap;
use xem_studio::{
    field::{self, draft},
    scene::{
        bundle,
        domain::{self, Action},
    },
};

const BYTECODE_OFFSET: usize = 132 + 64;

fn input(bytes: Vec<u8>) -> bundle::Input {
    bundle::Input {
        bundle: bytes,
        context: None,
        source: None,
        module_sha256: None,
    }
}

fn zone_event_bundle(selector: u8, zone_count: usize) -> Vec<u8> {
    let mut components = fixture::components();
    components[5][132..134].copy_from_slice(&15_u16.to_le_bytes());
    components[5].resize(BYTECODE_OFFSET + 23, 0);
    components[5][BYTECODE_OFFSET + 15..BYTECODE_OFFSET + 23]
        .copy_from_slice(&[0x0e, 0x0a, selector, 22, 0, 0x00, 0x0e, 0x0d]);
    for value in 20_i16..32 {
        components[8].extend_from_slice(&value.to_le_bytes());
    }
    components[8].truncate(zone_count * 24);
    fixture::bundle(&components)
}

fn actor_alias_bundle() -> Vec<u8> {
    let mut components = fixture::components();
    components[5][132..134].copy_from_slice(&15_u16.to_le_bytes());
    components[5].resize(BYTECODE_OFFSET + 20, 0);
    // Extended store_actor_flags has actor and var operands over the same low
    // byte, followed by an end instruction.
    components[5][BYTECODE_OFFSET + 15..BYTECODE_OFFSET + 20]
        .copy_from_slice(&[0xfe, 0x2c, 0, 0, 0]);
    let packed = fixture::bundle(&components);
    let source = bundle::Source::load(input(packed)).expect("load alias fixture");
    let mut header = source.header.clone();
    for _ in 0..3 {
        header.descriptors.push(header.descriptors[0].clone());
    }
    source
        .repack(&header, &BTreeMap::new())
        .expect("append a static template descriptor")
}

fn reachability_bundle() -> Vec<u8> {
    let mut components = fixture::components();
    components[5][132..134].copy_from_slice(&15_u16.to_le_bytes());
    components[5].resize(BYTECODE_OFFSET + 25, 0);
    // jump@15 -> call_in_zone@18 (zone 1, return@24), whose fallthrough is
    // end_slot@22; nop@23 is unreachable storage and return@24 is the call
    // destination. Retargeting the jump to 22 hides the selector temporarily.
    components[5][BYTECODE_OFFSET + 15..BYTECODE_OFFSET + 25]
        .copy_from_slice(&[0x01, 18, 0, 0x0a, 1, 24, 0, 0x00, 0x0e, 0x0d]);
    for value in 20_i16..32 {
        components[8].extend_from_slice(&value.to_le_bytes());
    }
    fixture::bundle(&components)
}

fn scene(bytes: Vec<u8>) -> domain::Draft {
    domain::Draft::open(input(bytes), Vec::new()).expect("open synthetic scene")
}

fn event_action(action: draft::Action) -> Action {
    Action::Event {
        action: Box::new(action),
    }
}

fn set_literal(node: &str, operand: usize, value: u32) -> Action {
    event_action(draft::Action::SetOperand {
        node: node.to_owned(),
        operand,
        value: draft::OperandValue::Literal {
            value,
            immediate: None,
        },
    })
}

fn set_target(node: &str, operand: usize, target: &str) -> Action {
    event_action(draft::Action::SetOperand {
        node: node.to_owned(),
        operand,
        value: draft::OperandValue::Target {
            node: target.to_owned(),
        },
    })
}

fn zone_call<'a>(events: &'a draft::Compiled, id: &str) -> &'a field::Instruction {
    let pc = events.bindings[id];
    events
        .graph
        .instructions
        .iter()
        .find(|instruction| instruction.pc == pc && instruction.opcode == 0x0a)
        .expect("stable node is a call_in_zone instruction")
}

fn assert_snapshot_compile_match(scene: &domain::Draft) -> draft::Compiled {
    let snapshot = scene.snapshot().expect("scene snapshot compiles events");
    let compiled = scene.compile().expect("scene compile succeeds");
    assert_eq!(
        serde_json::to_value(&snapshot.events).unwrap(),
        serde_json::to_value(&compiled.events).unwrap(),
        "snapshot exposes the same fully patched event document as compile"
    );
    assert_eq!(snapshot.next_event_id, scene.document().next_event_id);
    compiled.events
}

#[test]
fn source_zone_selector_survives_namespace_delete_then_unrelated_event_insertion() {
    let mut scene = scene(zone_event_bundle(1, 2));
    let initial = scene.snapshot().unwrap();
    assert_eq!(zone_call(&initial.events, "pc:16").operands[0], 1);

    scene
        .edit(
            0,
            Action::DeleteZone {
                id: "zone:0".into(),
            },
        )
        .unwrap();
    let after_delete = assert_snapshot_compile_match(&scene);
    assert_eq!(zone_call(&after_delete, "pc:16").operands[0], 0);
    assert_eq!(scene.snapshot().unwrap().zones[0].id, "zone:1");

    scene
        .edit(
            1,
            event_action(draft::Action::InsertAfter {
                after: "pc:15".into(),
                bytes: vec![0x0e],
                targets: BTreeMap::new(),
            }),
        )
        .unwrap();
    let edited = assert_snapshot_compile_match(&scene);
    let call = zone_call(&edited, "pc:16");
    assert!(call.pc > 16, "the M6 insertion relocates the source call");
    assert_eq!(call.operands[0], 0, "the call still resolves to zone:1");
    assert!(edited.bindings.contains_key("new:1"));
}

#[test]
fn explicit_same_raw_selector_recaptures_the_current_zone_namespace() {
    // Source raw selector 1 is initially outside its one-zone namespace. Add a
    // zone, then explicitly write the same encoded value. That write binds the
    // selector to the newly present stable zone instead of retaining Unknown.
    let mut scene = scene(zone_event_bundle(1, 1));
    let result = scene.edit(
        0,
        Action::Batch {
            actions: vec![
                Action::AddZone {
                    corners: [[40, 41, 42]; 4],
                },
                set_literal("pc:16", 0, 1),
            ],
        },
    );
    assert!(
        result.is_ok(),
        "same-value authored selector is recaptured: {result:?}"
    );
    let compiled = assert_snapshot_compile_match(&scene);
    assert_eq!(zone_call(&compiled, "pc:16").operands[0], 1);
    assert_eq!(scene.snapshot().unwrap().zones[1].id, "scene:1:zone");

    scene
        .edit(
            1,
            Action::DeleteZone {
                id: "zone:0".into(),
            },
        )
        .expect("captured new zone moves to index zero");
    let compiled = assert_snapshot_compile_match(&scene);
    assert_eq!(zone_call(&compiled, "pc:16").operands[0], 0);
    assert_eq!(scene.snapshot().unwrap().zones[0].id, "scene:1:zone");
}

#[test]
fn authored_new_event_selectors_bind_to_current_stable_entities() {
    let mut scene = scene(zone_event_bundle(1, 2));
    scene
        .edit(
            0,
            event_action(draft::Action::NewEvent {
                actor: 0,
                event: 1,
                nodes: vec![
                    draft::NodeInput {
                        bytes: vec![0x0a, 1, 0, 0],
                        targets: BTreeMap::from([(1, "new:2".into())]),
                    },
                    draft::NodeInput {
                        bytes: vec![0],
                        targets: BTreeMap::new(),
                    },
                ],
            }),
        )
        .unwrap();
    let before = assert_snapshot_compile_match(&scene);
    assert_eq!(zone_call(&before, "new:1").operands[0], 1);
    assert_eq!(
        zone_call(&before, "new:1").operands[1] as usize,
        before.bindings["new:2"]
    );

    scene
        .edit(
            1,
            Action::DeleteZone {
                id: "zone:0".into(),
            },
        )
        .expect("newly authored selector tracks zone:1");
    let after = assert_snapshot_compile_match(&scene);
    assert_eq!(zone_call(&after, "pc:16").operands[0], 0);
    assert_eq!(zone_call(&after, "new:1").operands[0], 0);
    assert_eq!(
        zone_call(&after, "new:1").operands[1] as usize,
        after.bindings["new:2"],
        "selector repair does not disturb the authored address target"
    );
}

#[test]
fn undo_alternate_event_insertion_export_import_preserves_targets_and_watermark() {
    let mut scene = scene(zone_event_bundle(1, 2));
    scene
        .edit(
            0,
            event_action(draft::Action::NewEvent {
                actor: 0,
                event: 1,
                nodes: vec![
                    draft::NodeInput {
                        bytes: vec![0x01, 0, 0],
                        targets: BTreeMap::from([(0, "new:2".into())]),
                    },
                    draft::NodeInput {
                        bytes: vec![0],
                        targets: BTreeMap::new(),
                    },
                ],
            }),
        )
        .unwrap();
    scene
        .edit(
            1,
            event_action(draft::Action::InsertAfter {
                after: "pc:15".into(),
                bytes: vec![0x0e],
                targets: BTreeMap::new(),
            }),
        )
        .unwrap();
    assert!(
        scene
            .snapshot()
            .unwrap()
            .events
            .bindings
            .contains_key("new:3")
    );

    scene.undo(2).unwrap();
    assert_eq!(scene.snapshot().unwrap().next_event_id, 4);
    scene
        .edit(
            3,
            event_action(draft::Action::InsertAfter {
                after: "pc:15".into(),
                bytes: vec![0x0e],
                targets: BTreeMap::new(),
            }),
        )
        .unwrap();

    let compiled = assert_snapshot_compile_match(&scene);
    assert!(compiled.bindings.contains_key("new:4"));
    assert!(!compiled.bindings.contains_key("new:3"));
    let branch_pc = compiled.bindings["new:1"];
    let branch = compiled
        .graph
        .instructions
        .iter()
        .find(|instruction| instruction.pc == branch_pc)
        .unwrap();
    assert_eq!(branch.opcode, 0x01);
    assert_eq!(branch.operands[0] as usize, compiled.bindings["new:2"]);

    let document = scene.document();
    assert_eq!(document.schema_version, 3);
    assert_eq!(document.event_allocation_starts, [1, 4]);
    assert_eq!(document.next_event_id, 5);
    let restored = domain::Draft::import(document.clone()).expect("scene event history imports");
    assert_eq!(
        restored.compile().unwrap().bundle,
        scene.compile().unwrap().bundle
    );
    assert_eq!(restored.document().event_allocation_starts, [1, 4]);
    assert_eq!(restored.snapshot().unwrap().next_event_id, 5);
    assert!(
        restored
            .snapshot()
            .unwrap()
            .events
            .bindings
            .contains_key("new:4")
    );
    assert_eq!(
        serde_json::to_value(restored.snapshot().unwrap().events).unwrap(),
        serde_json::to_value(scene.snapshot().unwrap().events).unwrap()
    );

    let mut reused_start = document.clone();
    reused_start.event_allocation_starts[1] = 1;
    assert!(domain::Draft::import(reused_start).is_err());
    let mut reused_watermark = document.clone();
    reused_watermark.next_event_id = 4;
    assert!(domain::Draft::import(reused_watermark).is_err());
    let mut missing = serde_json::to_value(document).unwrap();
    missing.as_object_mut().unwrap().remove("next_event_id");
    assert!(serde_json::from_value::<domain::Document>(missing).is_err());
}

#[test]
fn failed_event_batch_preserves_snapshot_document_bytes_and_allocator() {
    let mut scene = scene(zone_event_bundle(1, 2));
    let before_snapshot = serde_json::to_value(scene.snapshot().unwrap()).unwrap();
    let before_document = serde_json::to_value(scene.document()).unwrap();
    let before_bundle = scene.compile().unwrap().bundle;
    let error = scene
        .edit(
            0,
            Action::Batch {
                actions: vec![
                    event_action(draft::Action::NewEvent {
                        actor: 0,
                        event: 1,
                        nodes: vec![
                            draft::NodeInput {
                                bytes: vec![0x0e],
                                targets: BTreeMap::new(),
                            },
                            draft::NodeInput {
                                bytes: vec![0],
                                targets: BTreeMap::new(),
                            },
                        ],
                    }),
                    Action::DeleteZone {
                        id: "missing-zone".into(),
                    },
                ],
            },
        )
        .unwrap_err();
    assert_eq!(error.kind, "invalid_scene_edit");
    assert_eq!(
        serde_json::to_value(scene.snapshot().unwrap()).unwrap(),
        before_snapshot
    );
    assert_eq!(
        serde_json::to_value(scene.document()).unwrap(),
        before_document
    );
    assert_eq!(scene.compile().unwrap().bundle, before_bundle);

    scene
        .edit(
            0,
            event_action(draft::Action::NewEvent {
                actor: 0,
                event: 1,
                nodes: vec![
                    draft::NodeInput {
                        bytes: vec![0x0e],
                        targets: BTreeMap::new(),
                    },
                    draft::NodeInput {
                        bytes: vec![0],
                        targets: BTreeMap::new(),
                    },
                ],
            }),
        )
        .unwrap();
    let snapshot = scene.snapshot().unwrap();
    assert!(snapshot.events.bindings.contains_key("new:1"));
    assert_eq!(snapshot.next_event_id, 3);
}

#[test]
fn overlapping_actor_var_operand_write_preserves_the_shared_selector_byte() {
    let mut scene = scene(actor_alias_bundle());
    let before = scene.snapshot().unwrap();
    let instruction = before
        .events
        .graph
        .instructions
        .iter()
        .find(|instruction| instruction.pc == 15)
        .expect("extended actor/var alias instruction");
    assert_eq!(instruction.spec.handler, "field_event_store_actor_flags");
    assert_eq!(instruction.spec.operands[0].kind, "actor");
    assert_eq!(instruction.spec.operands[1].kind, "var");
    assert_eq!(
        instruction.spec.operands[0].offset,
        instruction.spec.operands[1].offset
    );
    assert_eq!(instruction.operands[0], 0);
    assert_eq!(instruction.operands[1], 0);

    scene
        .edit(0, set_literal("pc:15", 1, 0x100))
        .expect("write through the overlapping var operand");
    let compiled = assert_snapshot_compile_match(&scene);
    let instruction = compiled
        .graph
        .instructions
        .iter()
        .find(|instruction| instruction.pc == 15)
        .unwrap();
    assert_eq!(instruction.operands[0], 0, "actor selector byte stays zero");
    assert_eq!(
        instruction.operands[1], 0x100,
        "variable operand gets its authored value"
    );
    assert_eq!(compiled.bytes[BYTECODE_OFFSET + 15 + 2], 0);
    assert_eq!(compiled.bytes[BYTECODE_OFFSET + 15 + 3], 1);
}

#[test]
fn restoring_a_hidden_zone_reference_after_target_deletion_is_atomic() {
    let mut scene = scene(reachability_bundle());
    let initial = scene.snapshot().unwrap();
    let call = initial
        .events
        .graph
        .instructions
        .iter()
        .find(|instruction| instruction.pc == 18 && instruction.opcode == 0x0a)
        .expect("source selector is initially reachable");
    assert_eq!(call.operands[0], 1);

    scene
        .edit(0, set_target("pc:15", 0, "pc:22"))
        .expect("jump can temporarily bypass the call");
    let hidden = scene.snapshot().unwrap();
    assert!(
        !hidden
            .events
            .graph
            .instructions
            .iter()
            .any(|instruction| { instruction.pc == 18 && instruction.opcode == 0x0a }),
        "the call is absent from the currently decoded reachable graph"
    );

    scene
        .edit(
            1,
            Action::DeleteZone {
                id: "zone:1".into(),
            },
        )
        .expect("temporarily unreachable reference does not block deletion");
    let before_restore = serde_json::to_value(scene.snapshot().unwrap()).unwrap();
    let before_document = serde_json::to_value(scene.document()).unwrap();
    let before_bundle = scene.compile().unwrap().bundle;

    let error = scene
        .edit(2, set_target("pc:15", 0, "pc:18"))
        .expect_err("restoring the source call must reject its removed stable target");
    assert_eq!(error.kind, "invalid_scene_reference");
    assert_eq!(
        serde_json::to_value(scene.snapshot().unwrap()).unwrap(),
        before_restore
    );
    assert_eq!(
        serde_json::to_value(scene.document()).unwrap(),
        before_document
    );
    assert_eq!(scene.compile().unwrap().bundle, before_bundle);
}
