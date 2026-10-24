#[path = "support/scene_bundle.rs"]
#[allow(dead_code)]
mod fixture;

use std::collections::BTreeMap;
use xem_studio::field;
use xem_studio::scene::{
    bundle,
    domain::{self, Action},
    placements,
};

const EVENT_HEADER: usize = 132;
const ACTOR_ROW: usize = 64;
const BYTECODE: usize = EVENT_HEADER + ACTOR_ROW;

fn primary_opcode(handler: &str) -> u8 {
    let metadata: serde_json::Value =
        serde_json::from_str(include_str!("../data/field-opcodes.json")).unwrap();
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

fn input(bundle: Vec<u8>) -> bundle::Input {
    bundle::Input {
        bundle,
        context: None,
        source: None,
        module_sha256: None,
    }
}

fn source_bundle() -> Vec<u8> {
    let mut components = fixture::components();
    let mut events = vec![0; BYTECODE + 2];
    events[128..132].copy_from_slice(&1_u32.to_le_bytes());
    events[EVENT_HEADER..EVENT_HEADER + 2].copy_from_slice(&1_u16.to_le_bytes());
    events[BYTECODE..].copy_from_slice(&[0xff, 0x00]);
    components[5] = events;
    fixture::bundle(&components)
}

fn event_component(bytes: Vec<u8>) -> bundle::Component {
    bundle::Source::load(input(bytes))
        .expect("load complete synthetic scene bundle")
        .components[5]
        .clone()
}

fn event_bundle(code: &[u8], actor_entries: &[u16], zones: usize) -> Vec<u8> {
    let mut components = fixture::components();
    let bytecode = EVENT_HEADER + actor_entries.len() * ACTOR_ROW;
    let mut events = vec![0; bytecode + code.len()];
    events[128..132].copy_from_slice(&(actor_entries.len() as u32).to_le_bytes());
    for (actor, entry) in actor_entries.iter().enumerate() {
        events[EVENT_HEADER + actor * ACTOR_ROW..EVENT_HEADER + actor * ACTOR_ROW + 2]
            .copy_from_slice(&entry.to_le_bytes());
    }
    events[bytecode..].copy_from_slice(code);
    components[5] = events;
    components[8].clear();
    for zone in 0..zones {
        for value in 0..12_i16 {
            components[8].extend_from_slice(&(zone as i16 * 100 + value).to_le_bytes());
        }
    }
    fixture::bundle(&components)
}

fn with_static_descriptor(bytes: Vec<u8>) -> Vec<u8> {
    let source = bundle::Source::load(input(bytes)).expect("load source for static template");
    let mut header = source.header.clone();
    let mut descriptor = header.descriptors[0].clone();
    descriptor.position = [400, 500, 600];
    header.descriptors.push(descriptor);
    source
        .repack(&header, &BTreeMap::new())
        .expect("append a valid static descriptor")
}

fn with_descriptor_count(bytes: Vec<u8>, count: usize) -> Vec<u8> {
    let source = bundle::Source::load(input(bytes)).expect("load source for descriptor table");
    let mut header = source.header.clone();
    let template = header.descriptors[0].clone();
    header.descriptors.resize(count, template);
    source
        .repack(&header, &BTreeMap::new())
        .expect("write coherent descriptor table")
}

fn with_event_tail(bytes: Vec<u8>, tail: usize) -> Vec<u8> {
    let source = bundle::Source::load(input(bytes)).expect("load source for measured tail");
    let component = &source.components[5];
    assert!(tail > 0 && tail <= 16 && tail < component.logical_size);
    let mut replacements = BTreeMap::new();
    replacements.insert(
        5,
        bundle::Replacement {
            logical_size: component.logical_size - tail,
            bytes: component.bytes.clone(),
        },
    );
    source
        .repack(&source.header, &replacements)
        .expect("retain complete component with a smaller logical boundary")
}

fn assert_atomic_error(scene: &mut domain::Draft, action: Action, kind: &str) {
    let before_snapshot = serde_json::to_value(scene.snapshot().unwrap()).unwrap();
    let before_document = serde_json::to_value(scene.document()).unwrap();
    let before_bundle = scene.compile().unwrap().bundle;
    let before_revision = scene.revision();
    let error = scene
        .edit(before_revision, action)
        .expect_err("invalid edit is rejected");
    assert_eq!(error.kind, kind);
    assert_eq!(scene.revision(), before_revision);
    assert_eq!(
        serde_json::to_value(scene.snapshot().unwrap()).unwrap(),
        before_snapshot
    );
    assert_eq!(
        serde_json::to_value(scene.document()).unwrap(),
        before_document
    );
    assert_eq!(scene.compile().unwrap().bundle, before_bundle);
}

fn semantic_edges(graph: &field::FieldGraph) -> Vec<(usize, usize, String, String)> {
    graph
        .edges
        .iter()
        .map(|edge| {
            (
                edge.from_pc,
                edge.to_pc,
                edge.kind.clone(),
                edge.status.clone(),
            )
        })
        .collect::<Vec<_>>()
}

#[test]
fn duplicate_terminal_only_actor_adds_empty_stable_row_and_undo_restores_source() {
    let original = source_bundle();
    let original_events = event_component(original.clone());
    let mut scene = domain::Draft::open(input(original.clone()), Vec::new())
        .expect("open one actor with one terminal-only event");
    assert_eq!(scene.compile().unwrap().bundle, original);

    scene
        .edit(
            0,
            Action::DuplicateActor {
                id: "descriptor:0".into(),
            },
        )
        .expect("duplicate the source actor with empty event slots");
    let duplicate = scene.compile().expect("compile duplicated actor");
    let state = scene.snapshot().unwrap();
    assert_eq!(state.actors.len(), 2);
    assert_eq!(state.actors[0].id, "descriptor:0");
    assert_eq!(state.actors[1].id, "scene:1:actor");
    assert_eq!(state.actors[1].event_source_row, 1);
    assert_eq!(
        state.next_event_id, 1,
        "empty slots allocate no script nodes"
    );
    assert_ne!(duplicate.bundle, original);
    assert_eq!(
        duplicate.events.declared_size,
        original_events.logical_size + ACTOR_ROW
    );
    assert_eq!(
        &duplicate.events.bytes[..128],
        &original_events.bytes[..128],
        "non-count event header bytes are unchanged"
    );
    assert_eq!(
        &duplicate.events.bytes[EVENT_HEADER..BYTECODE],
        &original_events.bytes[EVENT_HEADER..BYTECODE],
        "the source actor's complete event row stays exact"
    );
    assert_eq!(
        duplicate.events.bytes[BYTECODE..BYTECODE + ACTOR_ROW]
            .chunks_exact(2)
            .map(|entry| u16::from_le_bytes([entry[0], entry[1]]))
            .collect::<Vec<_>>(),
        vec![1; 32],
        "new slots point at the proven terminal instruction, not copied scripts"
    );
    assert_eq!(
        &duplicate.events.bytes[BYTECODE + ACTOR_ROW..],
        &original_events.bytes[BYTECODE..],
        "full original code storage is preserved after the expanded table"
    );

    scene.undo(1).expect("undo actor creation");
    assert_eq!(scene.compile().unwrap().bundle, original);
    assert_eq!(scene.snapshot().unwrap().actors.len(), 1);
    scene.redo(2).expect("redo actor creation");
    assert_eq!(scene.compile().unwrap().bundle, duplicate.bundle);

    let document = scene.document();
    assert_eq!(document.next_id, 2);
    assert_eq!(document.next_event_id, 1);
    let imported = domain::Draft::import(document).expect("import actor history");
    assert_eq!(imported.compile().unwrap().bundle, duplicate.bundle);
    let imported_state = imported.snapshot().unwrap();
    assert_eq!(imported_state.actors.len(), 2);
    assert_eq!(imported_state.actors[1].id, "scene:1:actor");
    assert_eq!(imported_state.actors[1].event_source_row, 1);
    assert_eq!(imported_state.next_event_id, 1);
}

#[test]
fn adding_virtual_actor_terminal_row_preserves_instruction_flow_and_complete_code_storage() {
    let selector = primary_opcode("field_event_disable_actor");
    let original = with_event_tail(
        event_bundle(&[0xff, selector, 1, 0, 14, 0, 0xa5, 0x5a], &[1], 1),
        2,
    );
    let source = bundle::Source::load(input(original)).expect("load static-selector source");
    let component = &source.components[5];
    let source_graph = field::decode(&component.bytes, Some(component.logical_size)).unwrap();
    let terminal = source_graph
        .instructions
        .iter()
        .find(|instruction| instruction.spec.handler == "field_event_end_slot")
        .expect("source contains a terminal instruction for virtual rows");

    let mut extended = component.bytes[..128].to_vec();
    extended.extend_from_slice(&2_u32.to_le_bytes());
    extended.extend_from_slice(&component.bytes[EVENT_HEADER..source_graph.bytecode_offset]);
    for _ in 0..32 {
        extended.extend_from_slice(&(terminal.pc as u16).to_le_bytes());
    }
    extended.extend_from_slice(&component.bytes[source_graph.bytecode_offset..]);
    let extended_graph = field::decode(&extended, Some(component.logical_size + ACTOR_ROW))
        .expect("a new actor row may point at the proven terminal instruction");

    assert_eq!(
        &extended[BYTECODE + ACTOR_ROW..],
        &component.bytes[BYTECODE..],
        "complete original code storage and tail bytes are unchanged"
    );
    assert_eq!(extended_graph.instructions, source_graph.instructions);
    assert_eq!(extended_graph.diagnostics, source_graph.diagnostics);
    assert_ne!(extended_graph.edges, source_graph.edges);
    assert_eq!(
        semantic_edges(&extended_graph),
        semantic_edges(&source_graph)
    );
    assert_ne!(extended_graph.blocks, source_graph.blocks);
    let original_partitioned_bytes = source_graph
        .blocks
        .iter()
        .flat_map(|block| block.bytes.iter().copied())
        .collect::<Vec<_>>();
    let extended_partitioned_bytes = extended_graph
        .blocks
        .iter()
        .flat_map(|block| block.bytes.iter().copied())
        .collect::<Vec<_>>();
    assert_eq!(extended_partitioned_bytes, original_partitioned_bytes);
}

#[test]
fn duplicate_repairs_static_descriptor_references_in_source_and_copy() {
    // disable_actor's opcode is read from canonical generated metadata; its
    // source-qualified reader accesses descriptor flags without requiring an
    // actor row for that static descriptor.
    // Inserting an actor before descriptor:1 must move this static target to 2.
    let selector = primary_opcode("field_event_disable_actor");
    let original = with_static_descriptor(with_event_tail(
        event_bundle(&[0xff, selector, 1, 0, 14, 0, 0xa5, 0x5a], &[1], 1),
        2,
    ));
    let mut scene = domain::Draft::open(input(original.clone()), Vec::new()).unwrap();
    let before = scene.compile().unwrap();
    assert_eq!(
        before
            .events
            .graph
            .instructions
            .iter()
            .find(|instruction| instruction.pc == 1)
            .unwrap()
            .operands[0],
        1
    );

    scene
        .edit(
            0,
            Action::DuplicateActor {
                id: "descriptor:0".into(),
            },
        )
        .expect("duplicate script-facing actor before static descriptor");
    let compiled = scene.compile().unwrap();
    let selectors = compiled
        .events
        .graph
        .instructions
        .iter()
        .filter(|instruction| instruction.opcode == selector)
        .map(|instruction| instruction.operands[0])
        .collect::<Vec<_>>();
    assert_eq!(
        selectors,
        [2, 2],
        "source and copied references retain descriptor:1"
    );
    assert_eq!(
        compiled
            .descriptor_ids
            .iter()
            .map(String::as_str)
            .collect::<Vec<_>>(),
        ["descriptor:0", "scene:1:actor", "descriptor:1"]
    );
    assert!(compiled.events.declared_size >= before.events.declared_size + ACTOR_ROW);
    let preserved_source = &compiled.events.bytes
        [BYTECODE + ACTOR_ROW..BYTECODE + ACTOR_ROW + before.events.bytes.len() - BYTECODE];
    let mut expected_source = before.events.bytes[BYTECODE..].to_vec();
    expected_source[2] = 2;
    assert_eq!(preserved_source, expected_source);
    assert_eq!(
        &preserved_source[4..6],
        [14, 0],
        "unreachable source gap survives"
    );
    assert_eq!(
        &preserved_source[6..8],
        [0xa5, 0x5a],
        "opaque tail bytes survive"
    );
    assert_eq!(
        &preserved_source[8..],
        &before.events.bytes[BYTECODE + 8..],
        "any remaining literal-output padding is preserved"
    );
    let copied_row = compiled.events.graph.actors[1];
    assert_ne!(copied_row[0], compiled.events.graph.actors[0][0]);
    let copied = compiled
        .events
        .graph
        .instructions
        .iter()
        .find(|instruction| instruction.pc == copied_row[0] as usize)
        .expect("copied event entry points to decoded instruction");
    assert_eq!(copied.opcode, selector);
    assert_eq!(copied.operands[0], 2);
    let source = bundle::Source::load(input(compiled.bundle)).unwrap();
    assert_eq!(source.header.descriptors.len(), 3);
    assert_eq!(source.header.descriptors[1].position, [0, 0, 0]);
    assert_eq!(source.header.descriptors[2].position, [400, 500, 600]);
    for index in 0..9 {
        if index != 5 {
            assert_eq!(
                source.components[index].bytes,
                bundle::Source::load(input(before.bundle.clone()))
                    .unwrap()
                    .components[index]
                    .bytes,
                "unrelated full component {index} is preserved"
            );
        }
    }
}

#[test]
fn zone_compaction_precedes_actor_script_copy_and_keeps_the_stable_zone_target() {
    // call_in_zone@1 calls return@7, falls through to end_slot@5, and leaves
    // the source NOP at PC6 as an unreachable byte of code storage.
    let call = primary_opcode("field_event_call_in_zone");
    let original = event_bundle(&[0xff, call, 1, 7, 0, 0, 14, 13], &[1], 2);
    let mut scene = domain::Draft::open(input(original.clone()), Vec::new()).unwrap();
    scene
        .edit(
            0,
            Action::DeleteZone {
                id: "zone:0".into(),
            },
        )
        .expect("remove the unreferenced first zone");
    scene
        .edit(
            1,
            Action::DuplicateActor {
                id: "descriptor:0".into(),
            },
        )
        .expect("clone the already-repaired source event");
    let compiled = scene.compile().unwrap();
    let calls = compiled
        .events
        .graph
        .instructions
        .iter()
        .filter(|instruction| instruction.opcode == call)
        .collect::<Vec<_>>();
    assert_eq!(calls.len(), 2);
    assert!(calls.iter().all(|instruction| instruction.operands[0] == 0));
    assert_eq!(
        compiled
            .zone_ids
            .iter()
            .map(String::as_str)
            .collect::<Vec<_>>(),
        ["zone:1"]
    );
    let source_call = calls
        .iter()
        .find(|instruction| instruction.pc == 1)
        .unwrap();
    assert_eq!(
        source_call.operands[1], 7,
        "source call retains its original return target"
    );
    let copied_call_pc = compiled.events.bindings["new:1"];
    let copied_call = calls
        .iter()
        .find(|instruction| instruction.pc == copied_call_pc)
        .expect("copied call has a stable node binding");
    assert_eq!(
        copied_call.operands[1] as usize, compiled.events.bindings["new:3"],
        "copied call returns to the copied return node"
    );
    assert_ne!(copied_call.operands[1], source_call.operands[1]);
    assert_eq!(compiled.events.graph.actors[1][0], copied_call.pc as u16);
}

#[test]
fn duplicate_uses_current_m6_source_edits_and_actor_event_uses_the_new_stable_identity() {
    let face = primary_opcode("field_event_face_actor");
    let original = with_static_descriptor(event_bundle(&[0xff, face, 0, 0], &[1], 1));
    let mut scene = domain::Draft::open(input(original), Vec::new()).unwrap();
    scene
        .edit(
            0,
            Action::Event {
                action: Box::new(xem_studio::field::draft::Action::SetOperand {
                    node: "pc:1".into(),
                    operand: 0,
                    value: xem_studio::field::draft::OperandValue::Literal {
                        value: 0xfb,
                        immediate: None,
                    },
                }),
            },
        )
        .expect("change the source event before duplicating its actor");
    scene
        .edit(
            1,
            Action::DuplicateActor {
                id: "descriptor:0".into(),
            },
        )
        .expect("copy the current authored source bytes");
    let copied = scene.compile().unwrap();
    let selectors = copied
        .events
        .graph
        .instructions
        .iter()
        .filter(|instruction| instruction.opcode == face)
        .map(|instruction| instruction.operands[0])
        .collect::<Vec<_>>();
    assert_eq!(
        selectors,
        [0xfb, 0xfb],
        "the copy follows the edited self selector"
    );

    let state = scene.snapshot().unwrap();
    assert_eq!(state.actors[1].id, "scene:1:actor");
    assert_atomic_error(
        &mut scene,
        Action::Event {
            action: Box::new(xem_studio::field::draft::Action::NewEvent {
                actor: 1,
                event: 1,
                nodes: vec![
                    xem_studio::field::draft::NodeInput {
                        bytes: vec![14],
                        targets: BTreeMap::new(),
                    },
                    xem_studio::field::draft::NodeInput {
                        bytes: vec![0],
                        targets: BTreeMap::new(),
                    },
                ],
            }),
        },
        "unsupported_scene_edit",
    );
    assert_eq!(scene.revision(), 2);
    assert_eq!(scene.snapshot().unwrap().next_event_id, 3);
    scene
        .edit(
            2,
            Action::ActorEvent {
                actor: "scene:1:actor".into(),
                event: 1,
                nodes: vec![
                    xem_studio::field::draft::NodeInput {
                        bytes: vec![14],
                        targets: BTreeMap::new(),
                    },
                    xem_studio::field::draft::NodeInput {
                        bytes: vec![0],
                        targets: BTreeMap::new(),
                    },
                ],
            },
        )
        .expect("author an event through the stable actor identity");
    let authored = scene.compile().unwrap();
    let state = scene.snapshot().unwrap();
    assert_eq!(state.next_event_id, 5);
    assert_eq!(state.actors[1].event_source_row, 1);
    assert_ne!(authored.events.graph.actors[1][1], 1);
    assert!(
        authored
            .events
            .graph
            .instructions
            .iter()
            .any(|instruction| instruction.pc == authored.events.graph.actors[1][1] as usize)
    );
    scene.undo(3).expect("undo authored virtual-actor event");
    assert_eq!(scene.snapshot().unwrap().next_event_id, 5);
    scene
        .edit(
            4,
            Action::ActorEvent {
                actor: "scene:1:actor".into(),
                event: 2,
                nodes: vec![
                    xem_studio::field::draft::NodeInput {
                        bytes: vec![14],
                        targets: BTreeMap::new(),
                    },
                    xem_studio::field::draft::NodeInput {
                        bytes: vec![0],
                        targets: BTreeMap::new(),
                    },
                ],
            },
        )
        .expect("alternate event consumes fresh identities after undo");
    let alternate = scene.compile().unwrap();
    let alternate_state = scene.snapshot().unwrap();
    assert_eq!(alternate_state.next_event_id, 7);
    let alternate_entry = alternate.events.graph.actors[1][2] as usize;
    assert!(alternate.events.bindings.contains_key("new:5"));
    assert!(alternate.events.bindings.contains_key("new:6"));
    assert_ne!(alternate_entry, 1);
    let exported = scene.document();
    assert_eq!(exported.next_id, 2);
    assert_eq!(exported.next_event_id, 7);
    let imported = domain::Draft::import(exported).expect("import alternate actor-event history");
    assert_eq!(imported.compile().unwrap().bundle, alternate.bundle);
    assert_eq!(imported.snapshot().unwrap().actors[1].id, "scene:1:actor");
    assert_eq!(imported.snapshot().unwrap().next_event_id, 7);
    assert_atomic_error(
        &mut scene,
        Action::DeleteActor {
            id: "descriptor:0".into(),
        },
        "unsupported_scene_edit",
    );
}

#[test]
fn undo_alternate_actor_copy_preserves_global_actor_and_event_watermarks() {
    let place = primary_opcode("field_event_place_at_height");
    let original = event_bundle(&[0xff, place, 1, 0, 2, 0, 3, 0, 0], &[1], 1);
    let mut scene = domain::Draft::open(input(original), Vec::new()).unwrap();
    scene
        .edit(
            0,
            Action::DuplicateActor {
                id: "descriptor:0".into(),
            },
        )
        .unwrap();
    let first = scene.compile().unwrap();
    assert_eq!(scene.snapshot().unwrap().actors[1].id, "scene:1:actor");
    assert!(first.events.bindings.contains_key("new:1"));
    assert!(first.events.bindings.contains_key("new:2"));
    assert_eq!(scene.snapshot().unwrap().next_event_id, 3);

    scene
        .undo(1)
        .expect("undo the first script-bearing actor copy");
    assert_eq!(scene.snapshot().unwrap().actors.len(), 1);
    assert_eq!(scene.snapshot().unwrap().next_event_id, 3);
    scene
        .edit(
            2,
            Action::DuplicateActor {
                id: "descriptor:0".into(),
            },
        )
        .expect("alternate copy uses new public and event identities");
    let alternate_actor = scene.snapshot().unwrap().actors[1].clone();
    assert_eq!(alternate_actor.id, "scene:2:actor");
    assert_eq!(
        alternate_actor.event_source_row, 1,
        "private derived row may be reused"
    );
    let alternate_copy = scene.compile().unwrap();
    assert!(alternate_copy.events.bindings.contains_key("new:3"));
    assert!(alternate_copy.events.bindings.contains_key("new:4"));
    assert!(!alternate_copy.events.bindings.contains_key("new:1"));
    assert!(!alternate_copy.events.bindings.contains_key("new:2"));

    scene
        .edit(
            3,
            Action::ActorEvent {
                actor: "scene:2:actor".into(),
                event: 1,
                nodes: vec![
                    xem_studio::field::draft::NodeInput {
                        bytes: vec![1, 0, 0],
                        targets: BTreeMap::from([(0, "new:6".into())]),
                    },
                    xem_studio::field::draft::NodeInput {
                        bytes: vec![0],
                        targets: BTreeMap::new(),
                    },
                ],
            },
        )
        .expect("new actor event may use a stable jump target");
    let final_compiled = scene.compile().unwrap();
    let snapshot = scene.snapshot().unwrap();
    assert_eq!(snapshot.actors[1].id, "scene:2:actor");
    assert_eq!(snapshot.actors[1].event_source_row, 1);
    assert_eq!(snapshot.next_event_id, 7);
    assert!(!final_compiled.events.bindings.contains_key("new:1"));
    assert!(!final_compiled.events.bindings.contains_key("new:2"));
    assert!(final_compiled.events.bindings.contains_key("new:3"));
    assert!(final_compiled.events.bindings.contains_key("new:4"));
    let jump_pc = final_compiled.events.bindings["new:5"];
    let jump = final_compiled
        .events
        .graph
        .instructions
        .iter()
        .find(|instruction| instruction.pc == jump_pc)
        .unwrap();
    assert_eq!(jump.opcode, 1);
    assert_eq!(
        jump.operands[0] as usize,
        final_compiled.events.bindings["new:6"]
    );
    assert_eq!(snapshot.actors[0].id, "descriptor:0");

    let document = scene.document();
    assert_eq!(document.next_id, 3);
    assert_eq!(document.next_event_id, 7);
    let imported = domain::Draft::import(document).unwrap();
    let imported_compiled = imported.compile().unwrap();
    assert_eq!(imported_compiled.bundle, final_compiled.bundle);
    assert_eq!(
        imported_compiled.events.bindings,
        final_compiled.events.bindings
    );
    assert_eq!(imported.snapshot().unwrap().actors[1].id, "scene:2:actor");
    assert_eq!(imported.snapshot().unwrap().actors[1].event_source_row, 1);
    assert_eq!(imported.snapshot().unwrap().next_event_id, 7);
}

#[test]
fn deleting_an_actor_row_repairs_shared_event_owners_and_keeps_the_survivor_stable() {
    let face = primary_opcode("field_event_face_actor");
    let place = primary_opcode("field_event_place_at_height");
    let original = event_bundle(
        &[
            0xff, face, 2, 0, // actor 0 references the stable third descriptor
            place, 1, 0, 2, 0, 3, 0, 0,
        ],
        &[1, 4, 4],
        1,
    );
    let original = with_descriptor_count(original, 3);
    let mut scene = domain::Draft::open(input(original.clone()), Vec::new()).unwrap();
    let shared = scene.placement("pc:4").unwrap();
    assert!(!shared.can_transform);
    assert_eq!(
        shared
            .owners
            .iter()
            .map(|owner| owner.actor.as_str())
            .collect::<Vec<_>>(),
        ["descriptor:1", "descriptor:2"]
    );

    scene
        .edit(
            0,
            Action::DeleteActor {
                id: "descriptor:1".into(),
            },
        )
        .expect("remove the unreferenced middle actor and repair references");
    let after_delete = scene.compile().unwrap();
    let state = scene.snapshot().unwrap();
    assert_eq!(state.actors.len(), 2);
    assert_eq!(state.actors[0].id, "descriptor:0");
    assert_eq!(state.actors[0].event_source_row, 0);
    assert_eq!(state.actors[1].id, "descriptor:2");
    assert_eq!(state.actors[1].event_source_row, 2);
    assert_eq!(state.descriptors[0].id, "descriptor:0");
    assert_eq!(state.descriptors[1].id, "descriptor:2");
    let face_instruction = after_delete
        .events
        .graph
        .instructions
        .iter()
        .find(|instruction| instruction.pc == 1)
        .unwrap();
    assert_eq!(face_instruction.opcode, face);
    assert_eq!(face_instruction.operands[0], 1);
    assert_eq!(after_delete.events.graph.actors[0][0], 1);
    assert_eq!(after_delete.events.graph.actors[1][0], 4);

    let surviving_placement = scene.placement("pc:4").unwrap();
    assert!(surviving_placement.can_transform);
    assert_eq!(surviving_placement.owners.len(), 1);
    assert_eq!(surviving_placement.owners[0].actor, "descriptor:2");
    scene
        .edit(
            1,
            Action::SetPlacement {
                change: placements::Change {
                    node: "pc:4".into(),
                    actor: "descriptor:2".into(),
                    event: 0,
                    position: [Some(9), Some(10), Some(11)],
                    layer: None,
                },
            },
        )
        .expect("placement becomes writable under the sole surviving actor owner");
    let transformed = scene.compile().unwrap();
    let placement = transformed
        .events
        .graph
        .instructions
        .iter()
        .find(|instruction| instruction.pc == 4)
        .unwrap();
    assert_eq!(placement.opcode, place);
    assert_eq!(placement.operands, [9, 11, 10]);
    assert_eq!(transformed.events.graph.actors[0][0], 1);
    assert_eq!(transformed.events.graph.actors[1][0], 4);

    scene.undo(2).unwrap();
    assert_eq!(scene.compile().unwrap().bundle, after_delete.bundle);
    scene.undo(3).unwrap();
    assert_eq!(scene.compile().unwrap().bundle, original);
    scene.redo(4).unwrap();
    scene.redo(5).unwrap();
    assert_eq!(scene.compile().unwrap().bundle, transformed.bundle);
    let imported = domain::Draft::import(scene.document()).unwrap();
    let imported_compiled = imported.compile().unwrap();
    assert_eq!(imported_compiled.bundle, transformed.bundle);
    assert_eq!(
        imported_compiled.events.bindings,
        transformed.events.bindings
    );
    assert_eq!(
        imported
            .snapshot()
            .unwrap()
            .actors
            .iter()
            .map(|actor| actor.id.as_str())
            .collect::<Vec<_>>(),
        ["descriptor:0", "descriptor:2"]
    );
}

#[test]
fn copied_placement_is_owned_by_the_new_actor_and_transforms_independently() {
    let place = primary_opcode("field_event_place_at_height");
    let original = event_bundle(&[0xff, place, 0xfe, 0xff, 0x0d, 0, 0xf9, 0xff, 0], &[1], 1);
    let mut scene = domain::Draft::open(input(original), Vec::new()).unwrap();
    scene
        .edit(
            0,
            Action::DuplicateActor {
                id: "descriptor:0".into(),
            },
        )
        .expect("copy one qualified placement script to a new actor");
    let before = scene.compile().unwrap();
    let copied_binding = scene.placement("new:1").unwrap();
    assert!(copied_binding.can_transform);
    assert_eq!(copied_binding.owners.len(), 1);
    assert_eq!(copied_binding.owners[0].actor, "scene:1:actor");
    assert_eq!(copied_binding.owners[0].event, 0);
    assert_eq!(
        scene.placement("pc:1").unwrap().owners[0].actor,
        "descriptor:0"
    );

    let bytecode = before.events.graph.bytecode_offset;
    let original_instruction = &before.events.bytes[bytecode + 1..bytecode + 8];
    assert_eq!(
        original_instruction,
        &[place, 0xfe, 0xff, 0x0d, 0, 0xf9, 0xff]
    );
    scene
        .edit(
            1,
            Action::SetPlacement {
                change: placements::Change {
                    node: "new:1".into(),
                    actor: "scene:1:actor".into(),
                    event: 0,
                    position: [Some(10), Some(20), Some(30)],
                    layer: None,
                },
            },
        )
        .expect("transform the copied instruction in its owning actor context");
    let edited = scene.compile().unwrap();
    assert_eq!(
        &edited.events.bytes
            [edited.events.graph.bytecode_offset + 1..edited.events.graph.bytecode_offset + 8],
        original_instruction,
        "the original source instruction remains byte-exact"
    );
    let copied_pc = edited.events.bindings["new:1"];
    let copied_instruction = edited
        .events
        .graph
        .instructions
        .iter()
        .find(|instruction| instruction.pc == copied_pc)
        .unwrap();
    assert_eq!(copied_instruction.opcode, place);
    assert_eq!(copied_instruction.operands, [10, 30, 20]);
    assert_eq!(
        scene.placement("new:1").unwrap().owners[0].actor,
        "scene:1:actor"
    );

    scene.undo(2).expect("undo independent placement transform");
    assert_eq!(scene.compile().unwrap().bundle, before.bundle);
    scene.redo(3).expect("redo independent placement transform");
    assert_eq!(scene.compile().unwrap().bundle, edited.bundle);
    let imported = domain::Draft::import(scene.document()).unwrap();
    let imported_compiled = imported.compile().unwrap();
    assert_eq!(imported_compiled.bundle, edited.bundle);
    assert_eq!(imported_compiled.events.bindings, edited.events.bindings);
    assert_eq!(imported.snapshot().unwrap().actors[1].id, "scene:1:actor");
}

#[test]
fn unsafe_templates_and_dangling_actor_references_reject_atomically() {
    for code in [&[0xff, 0x16, 0, 0, 0][..], &[0xff, 0x5c, 0, 0, 0][..]] {
        let mut scene =
            domain::Draft::open(input(event_bundle(code, &[1], 1)), Vec::new()).unwrap();
        assert_atomic_error(
            &mut scene,
            Action::DuplicateActor {
                id: "descriptor:0".into(),
            },
            "unsupported_scene_actor",
        );
    }

    let paired_code = [
        0xff, 0x10, 0, 1, 0, 2, 0, 3, 0, 0, // walk_to setup
        0x10, 1, // walk_step consumes the setup
        0, // end slot
    ];
    let mut paired =
        domain::Draft::open(input(event_bundle(&paired_code, &[1], 1)), Vec::new()).unwrap();
    assert_atomic_error(
        &mut paired,
        Action::DuplicateActor {
            id: "descriptor:0".into(),
        },
        "unsupported_scene_script_clone",
    );

    let no_seed = event_bundle(&[0xff, 14, 13], &[1], 1);
    let mut no_seed = domain::Draft::open(input(no_seed), Vec::new()).unwrap();
    assert_atomic_error(
        &mut no_seed,
        Action::DuplicateActor {
            id: "descriptor:0".into(),
        },
        "unsupported_scene_actor",
    );

    // Two real actor rows; actor 0's face_actor reference makes deleting row 1
    // a dangling reference, which is rejected before adoption.
    let face = primary_opcode("field_event_face_actor");
    let original = event_bundle(&[0xff, face, 1, 0], &[1, 1], 1);
    let original = with_descriptor_count(original, 2);
    let mut scene = domain::Draft::open(input(original), Vec::new()).unwrap();
    assert_atomic_error(
        &mut scene,
        Action::DeleteActor {
            id: "descriptor:1".into(),
        },
        "invalid_scene_reference",
    );
    assert_atomic_error(
        &mut scene,
        Action::DeleteActor {
            id: "descriptor:0".into(),
        },
        "unsupported_scene_edit",
    );
    assert_atomic_error(
        &mut scene,
        Action::DuplicateActor {
            id: "missing".into(),
        },
        "invalid_scene_edit",
    );

    let sprite = with_static_descriptor(event_bundle(&[0xff, 0], &[1], 1));
    let source = bundle::Source::load(input(sprite)).unwrap();
    let mut header = source.header.clone();
    header.descriptors[0].flags |= 0x40;
    let sprite = source.repack(&header, &BTreeMap::new()).unwrap();
    let mut scene = domain::Draft::open(input(sprite), Vec::new()).unwrap();
    assert_atomic_error(
        &mut scene,
        Action::AddActor {
            template: "descriptor:0".into(),
            position: [1, 2, 3],
            rotation: [1, 0, 0],
        },
        "unsupported_scene_edit",
    );
}

#[test]
fn add_actor_applies_requested_transform_and_preserves_source_descriptor() {
    let original = event_bundle(&[0xff, 0], &[1], 1);
    let mut scene = domain::Draft::open(input(original.clone()), Vec::new()).unwrap();
    scene
        .edit(
            0,
            Action::AddActor {
                template: "descriptor:0".into(),
                position: [11, 22, 33],
                rotation: [3, 4, 5],
            },
        )
        .expect("create transformed independent actor");
    let state = scene.snapshot().unwrap();
    assert_eq!(state.actors[1].id, "scene:1:actor");
    assert_eq!(state.actors[1].event_source_row, 1);
    assert_eq!(state.descriptors[0].value.position, [0, 0, 0]);
    assert_eq!(state.descriptors[1].value.position, [11, 22, 33]);
    assert_eq!(state.descriptors[1].value.rotation, [3, 4, 5]);
    assert_eq!(state.next_event_id, 1);
    let added = scene.compile().unwrap().bundle;
    scene
        .edit(
            1,
            Action::DeleteActor {
                id: "scene:1:actor".into(),
            },
        )
        .expect("delete an unreferenced independent actor");
    assert_eq!(scene.snapshot().unwrap().actors.len(), 1);
    assert_eq!(scene.compile().unwrap().bundle, original);
    scene.undo(2).expect("undo actor deletion");
    assert_eq!(scene.compile().unwrap().bundle, added);
}

#[test]
fn runtime_index_254_is_supported_255_is_refused_and_wide_originals_stay_readable() {
    let entries = vec![1_u16; 254];
    let original = with_descriptor_count(event_bundle(&[0xff, 0], &entries, 1), 254);
    let mut scene = domain::Draft::open(input(original), Vec::new()).unwrap();
    scene
        .edit(
            0,
            Action::DuplicateActor {
                id: "descriptor:0".into(),
            },
        )
        .expect("runtime index 254 is an ordinary byte value");
    assert_eq!(scene.snapshot().unwrap().actors.len(), 255);
    assert_eq!(scene.snapshot().unwrap().actors[254].id, "scene:1:actor");
    assert_atomic_error(
        &mut scene,
        Action::DuplicateActor {
            id: "descriptor:0".into(),
        },
        "unsupported_scene_actor",
    );

    let entries = vec![1_u16; 256];
    let original = with_descriptor_count(event_bundle(&[0xff, 0], &entries, 1), 256);
    let mut wide = domain::Draft::open(input(original.clone()), Vec::new()).unwrap();
    assert_eq!(wide.compile().unwrap().bundle, original);
    assert_atomic_error(
        &mut wide,
        Action::DuplicateActor {
            id: "descriptor:0".into(),
        },
        "unsupported_scene_edit",
    );
}
