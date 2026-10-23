use std::collections::BTreeMap;
use xem_studio::field::{
    self, FieldGraph,
    draft::{Action, Draft, NodeInput},
};
use xem_studio::scene::clone_event;

const BYTECODE_OFFSET: usize = 132 + 64;

fn event_component(code: &[u8]) -> (Vec<u8>, FieldGraph) {
    let entry = 15_u16;
    let mut bytes = vec![0; BYTECODE_OFFSET + usize::from(entry) + code.len()];
    bytes[128..132].copy_from_slice(&1_u32.to_le_bytes());
    bytes[132..134].copy_from_slice(&entry.to_le_bytes());
    bytes[BYTECODE_OFFSET] = 0xff;
    let start = BYTECODE_OFFSET + usize::from(entry);
    bytes[start..].copy_from_slice(code);
    let graph = field::decode(&bytes, Some(bytes.len())).expect("synthetic M6 event decodes");
    (bytes, graph)
}

fn assert_clone_refused(result: Result<Option<clone_event::Prepared>, field::FieldError>) {
    let error = result.expect_err("unsafe event shape is refused");
    assert_eq!(error.kind, "unsupported_scene_script_clone");
    assert!(!error.message.is_empty());
}

fn new_event(nodes: Vec<NodeInput>, event: usize) -> Action {
    Action::NewEvent {
        actor: 0,
        event,
        nodes,
    }
}

#[test]
fn qualified_copy_retains_source_literals_and_rebinds_only_address_targets() {
    // nop@15, call_in_zone@16 (zone 1, call return@21), nop@20, return@21.
    let source_code = [0x0e, 0x0a, 1, 21, 0, 0x0e, 0x0d];
    let (source, graph) = event_component(&source_code);
    let prepared = clone_event::prepare(&graph, 0, 0, 1)
        .expect("qualified simple event can be prepared")
        .expect("event 0 is present");
    assert_eq!(
        prepared.source_nodes,
        BTreeMap::from([
            (15, "new:1".to_owned()),
            (16, "new:2".to_owned()),
            (20, "new:3".to_owned()),
            (21, "new:4".to_owned()),
        ])
    );
    assert_eq!(prepared.next_id, 5);
    assert_eq!(prepared.nodes.len(), 4);
    assert_eq!(prepared.nodes[1].bytes, [0x0a, 1, 21, 0]);
    assert_eq!(
        prepared.nodes[1].targets,
        BTreeMap::from([(1, "new:4".to_owned())]),
        "the address operand follows the stable source return node"
    );
    assert_eq!(prepared.nodes[1].bytes[1], 1, "zone literal stays literal");

    let mut draft = Draft::open(graph.clone()).expect("open source event draft");
    draft
        .edit(0, new_event(prepared.nodes.clone(), 1))
        .expect("M6 accepts the prepared nodes in an absent event slot");
    assert_eq!(draft.document().next_id, prepared.next_id);
    let compiled = draft.compile().expect("copied event compiles");
    assert!(compiled.bytes.len() >= source.len());
    assert_eq!(
        &compiled.bytes[..132],
        &source[..132],
        "fixed header is unchanged"
    );
    assert_eq!(
        &compiled.bytes[132..134],
        &source[132..134],
        "original event slot is unchanged"
    );
    assert_eq!(
        &compiled.bytes[136..BYTECODE_OFFSET],
        &source[136..BYTECODE_OFFSET],
        "unrelated event slots are unchanged"
    );
    assert_eq!(
        &compiled.bytes[BYTECODE_OFFSET..source.len()],
        &source[BYTECODE_OFFSET..],
        "complete original bytecode storage is preserved"
    );
    let original_call = compiled
        .graph
        .instructions
        .iter()
        .find(|instruction| instruction.pc == 16)
        .expect("original call remains in source storage");
    assert_eq!(original_call.operands[0], 1);

    for id in prepared.source_nodes.values() {
        assert!(
            compiled.bindings.contains_key(id),
            "stable copied node {id}"
        );
    }
    let copied_call_pc = compiled.bindings["new:2"];
    let copied_call = compiled
        .graph
        .instructions
        .iter()
        .find(|instruction| instruction.pc == copied_call_pc)
        .expect("copied call is decoded");
    assert_eq!(copied_call.opcode, 0x0a);
    assert_eq!(
        copied_call.operands[0], 1,
        "copied literal remains unchanged"
    );
    assert_eq!(
        copied_call.operands[1] as usize, compiled.bindings["new:4"],
        "copied call resolves to the copied return, not the source return"
    );
}

#[test]
fn copied_terminal_event_does_not_fall_through_into_the_next_authored_event() {
    let (source, graph) = event_component(&[0x0e, 0x00]);
    let first = clone_event::prepare(&graph, 0, 0, 1)
        .unwrap()
        .expect("source event exists");
    let second = clone_event::prepare(&graph, 0, 0, first.next_id)
        .unwrap()
        .expect("same source can be allocated at the next watermark");
    assert_eq!(first.next_id, 3);
    assert_eq!(second.source_nodes[&15], "new:3");
    assert_eq!(second.next_id, 5);

    let mut draft = Draft::open(graph.clone()).unwrap();
    draft.edit(0, new_event(first.nodes, 1)).unwrap();
    draft.edit(1, new_event(second.nodes, 2)).unwrap();
    let compiled = draft.compile().unwrap();
    assert_eq!(
        &compiled.bytes[..132],
        &source[..132],
        "fixed header is unchanged"
    );
    assert_eq!(
        &compiled.bytes[132..134],
        &source[132..134],
        "original event slot is unchanged"
    );
    assert_eq!(
        &compiled.bytes[138..BYTECODE_OFFSET],
        &source[138..BYTECODE_OFFSET],
        "unrelated event slots are unchanged"
    );
    assert_eq!(
        &compiled.bytes[BYTECODE_OFFSET..source.len()],
        &source[BYTECODE_OFFSET..],
        "appending events preserves complete original bytecode storage"
    );
    let index = xem_studio::field::structure::StructureIndex::new(&compiled.graph).unwrap();
    let first_event = index.reachable_pcs(0, 1).unwrap();
    let second_event = index.reachable_pcs(0, 2).unwrap();
    assert_eq!(first_event.len(), 2);
    assert_eq!(second_event.len(), 2);
    assert!(
        first_event.iter().all(|pc| !second_event.contains(pc)),
        "the first authored event terminates before the separately appended event"
    );
    let first_entry = compiled
        .graph
        .entries
        .iter()
        .find(|entry| entry.actor == 0 && entry.event == 1)
        .unwrap();
    let first_last_pc = *first_event.last().unwrap();
    assert_ne!(first_entry.pc, first_last_pc);
    let terminal = compiled
        .graph
        .instructions
        .iter()
        .find(|instruction| instruction.pc == first_last_pc)
        .unwrap();
    assert!(
        terminal.successors.is_empty(),
        "copied terminal has no fallthrough"
    );
}

#[test]
fn canonical_ff_skipped_entries_return_none_without_mutating_the_graph() {
    let mut source = vec![0; BYTECODE_OFFSET + 8];
    source[128..132].copy_from_slice(&1_u32.to_le_bytes());
    // An ff-prefixed map record makes zero event-table slots explicitly absent.
    source[BYTECODE_OFFSET] = 0xff;
    let graph = field::decode(&source, Some(source.len())).expect("empty ff map component");
    let original = field::encode(&graph).unwrap();
    assert!(clone_event::prepare(&graph, 0, 1, 1).unwrap().is_none());
    assert_eq!(field::encode(&graph).unwrap(), original);
    assert_eq!(original, source);
}

#[test]
fn malformed_actor_event_and_allocation_watermarks_are_rejected() {
    let (_, graph) = event_component(&[0x0e, 0x00]);
    let actor = clone_event::prepare(&graph, u32::MAX, 0, 1).unwrap_err();
    assert_eq!(actor.kind, "invalid_event");
    let event = clone_event::prepare(&graph, 0, 32, 1).unwrap_err();
    assert_eq!(event.kind, "invalid_event");
    assert_clone_refused(clone_event::prepare(&graph, 0, 0, 0));
    assert_clone_refused(clone_event::prepare(&graph, 0, 0, u32::MAX));
}

#[test]
fn paired_fixed_relative_party_owned_and_nonflat_events_are_refused() {
    let (_, paired) = event_component(&[
        0x10, 0, 1, 0, 2, 0, 3, 0, 0, // walk_to setup
        0x10, 1,    // walk_step consumer
        0x00, // end
    ]);
    assert_clone_refused(clone_event::prepare(&paired, 0, 0, 1));

    let (_, fixed_relative) = event_component(&[0x9a, 1, 0, 0x00]);
    assert_clone_refused(clone_event::prepare(&fixed_relative, 0, 0, 1));

    for party_opcode in [0x16, 0x5c] {
        let (_, party_owner) = event_component(&[party_opcode, 0, 0, 0x00]);
        assert_clone_refused(clone_event::prepare(&party_owner, 0, 0, 1));
    }

    // Entry@15 jumps to the lower, separately decoded storage at PC8. The
    // helper refuses to reorder this non-flat layout into a new event body.
    let mut bytes = vec![0; BYTECODE_OFFSET + 18];
    bytes[128..132].copy_from_slice(&1_u32.to_le_bytes());
    bytes[132..134].copy_from_slice(&15_u16.to_le_bytes());
    bytes[BYTECODE_OFFSET] = 0xff;
    bytes[BYTECODE_OFFSET + 8..BYTECODE_OFFSET + 10].copy_from_slice(&[0x0e, 0x00]);
    bytes[BYTECODE_OFFSET + 15..BYTECODE_OFFSET + 18].copy_from_slice(&[0x01, 8, 0]);
    let nonflat = field::decode(&bytes, Some(bytes.len())).expect("non-flat CFG decodes");
    assert_clone_refused(clone_event::prepare(&nonflat, 0, 0, 1));
}

#[test]
fn implicit_fallthrough_outside_the_source_event_is_not_cloned() {
    // A terminal-position nop's successor lies past the source byte stream.
    let (_, graph) = event_component(&[0x0e]);
    assert!(graph.diagnostics.outside.iter().any(|edge| edge[0] == 15));
    assert_clone_refused(clone_event::prepare(&graph, 0, 0, 1));
}
