use serde_json::json;
use xem_studio::Studio;
use xem_studio::field::structure::{self, EventStructure, StructureIndex};
use xem_studio::field::{self, FieldGraph};

const BYTECODE_OFFSET: usize = 132 + 64;

fn component(code: &[u8]) -> Vec<u8> {
    let mut bytes = vec![0; BYTECODE_OFFSET + code.len()];
    bytes[128..132].copy_from_slice(&1u32.to_le_bytes());
    bytes[BYTECODE_OFFSET..].copy_from_slice(code);
    bytes
}

fn decode(code: &[u8]) -> FieldGraph {
    let bytes = component(code);
    field::decode(&bytes, Some(bytes.len())).expect("synthetic event graph decodes")
}

fn event(graph: &FieldGraph) -> EventStructure {
    structure::structure(graph, 0, 0).expect("actor 0 event 0 structures")
}

fn assert_lossless(graph: &FieldGraph, event: &EventStructure) {
    let index = StructureIndex::new(graph).expect("canonical field graph");
    let expected_pcs = index
        .reachable_pcs(event.actor, event.event)
        .expect("reachable PCs");
    let flattened = structure::flatten(event);
    assert_eq!(flattened.instruction_pcs, expected_pcs);

    let mut expected_edges: Vec<_> = graph
        .edges
        .iter()
        .filter(|edge| expected_pcs.contains(&edge.from_pc))
        .cloned()
        .collect();
    expected_edges.sort_by_key(|edge| edge.from_pc);
    assert_eq!(flattened.edges, expected_edges);
    assert_eq!(flattened.counts, event.counts);
}

fn branch(target: u16) -> [u8; 8] {
    [0x02, 0, 0, 0, 0, 0, target as u8, (target >> 8) as u8]
}

#[test]
fn folds_an_if_else_and_nested_if_without_losing_instructions_or_edges() {
    let mut code = vec![0x0e; 30];
    code[..8].copy_from_slice(&branch(28));
    code[8..16].copy_from_slice(&branch(20));
    code[17..20].copy_from_slice(&[1, 24, 0]);
    code[21..24].copy_from_slice(&[1, 24, 0]);
    code[25..28].copy_from_slice(&[1, 28, 0]);
    code[29] = 0;
    let graph = decode(&code);
    let tree = event(&graph);

    assert_eq!(tree.counts.ifs, 2);
    assert_lossless(&graph, &tree);
}

#[test]
fn folds_a_loop_with_a_back_edge_and_exit() {
    let mut code = vec![0x0e; 13];
    code[..8].copy_from_slice(&branch(12));
    code[9..12].copy_from_slice(&[1, 0, 0]);
    code[12] = 0;
    let graph = decode(&code);
    let tree = event(&graph);

    assert_eq!(tree.counts.loops, 1);
    assert_lossless(&graph, &tree);
}

#[test]
fn folds_an_inline_jump_table_as_a_switch() {
    let graph = decode(&[0xa6, 0, 0, 1, 9, 0, 1, 9, 0, 0]);
    let tree = event(&graph);

    assert_eq!(tree.counts.switches, 1);
    assert_lossless(&graph, &tree);
}

#[test]
fn irreducible_multi_entry_cycle_remains_explicit_gotos() {
    let mut code = vec![0x0e; 20];
    code[..8].copy_from_slice(&branch(16));
    code[8..11].copy_from_slice(&[1, 16, 0]);
    code[16..19].copy_from_slice(&[1, 8, 0]);
    code[19] = 0;
    let graph = decode(&code);
    let tree = event(&graph);

    assert!(tree.counts.gotos > 0);
    assert_eq!(tree.counts.loops, 0);
    assert_lossless(&graph, &tree);
}

#[test]
fn field_structure_command_returns_the_lossless_tree_envelope() {
    let graph = decode(&[0x0e, 0]);
    let expected = event(&graph);
    let request = json!({
        "op": "field.structure",
        "args": {"graph": graph, "actor": 0, "event": 0}
    });
    let response: serde_json::Value =
        serde_json::from_str(&Studio::new().command(&request.to_string()))
            .expect("field structure returns JSON");

    assert_eq!(response["ok"], serde_json::to_value(expected).unwrap());
}
