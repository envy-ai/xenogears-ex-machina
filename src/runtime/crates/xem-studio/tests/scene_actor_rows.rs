use xem_studio::field::{self, FieldGraph};
use xem_studio::scene::actor_rows::{self, Row};

fn component(actor_count: usize, code: &[u8], declared_tail: usize) -> (Vec<u8>, usize) {
    let offset = 132 + actor_count * 64;
    let mut bytes = vec![0_u8; offset + code.len()];
    bytes[128..132].copy_from_slice(&(actor_count as u32).to_le_bytes());
    for actor in 0..actor_count {
        for event in 0..32 {
            let pc = if actor == 1 && event == 0 {
                4_u16
            } else {
                0_u16
            };
            let at = 132 + actor * 64 + event * 2;
            bytes[at..at + 2].copy_from_slice(&pc.to_le_bytes());
        }
    }
    bytes[offset..].copy_from_slice(code);
    let declared_size = bytes.len() - declared_tail;
    (bytes, declared_size)
}

fn source() -> (Vec<u8>, FieldGraph) {
    // message_box at pc 0, then nop/end. Both source actors enter existing code.
    let (bytes, declared) = component(2, &[0x03, 0, 0, 5, 0x0e, 0], 2);
    let graph = field::decode(&bytes, Some(declared)).expect("synthetic source graph decodes");
    (bytes, graph)
}

fn rows() -> Vec<Row> {
    vec![
        Row {
            id: "descriptor:0".into(),
            source_actor: 0,
        },
        Row {
            id: "descriptor:1".into(),
            source_actor: 1,
        },
    ]
}

#[test]
fn unchanged_rows_preserve_the_complete_component_byte_for_byte() {
    let (bytes, graph) = source();
    let prepared = actor_rows::rebuild(&graph, &rows()).expect("unchanged rows prepare");

    assert_eq!(prepared.bytes, bytes);
    assert_eq!(prepared.declared_size, graph.declared_size);
    assert_eq!(prepared.graph.instructions, graph.instructions);
    assert_eq!(prepared.graph.blocks, graph.blocks);
    assert_eq!(prepared.actor_indices["descriptor:0"], 0);
    assert_eq!(prepared.actor_indices["descriptor:1"], 1);
}

#[test]
fn template_rows_can_expand_reorder_and_remove_without_changing_code_or_tail() {
    let (bytes, graph) = source();
    let original_code = &bytes[graph.bytecode_offset..];
    let mut expanded = vec![Row {
        id: "descriptor:0".into(),
        source_actor: 0,
    }];
    expanded.extend((1..56).map(|index| Row {
        id: format!("template:{index}"),
        source_actor: if index % 2 == 0 { 0 } else { 1 },
    }));

    let prepared = actor_rows::rebuild(&graph, &expanded).expect("56 rows use source templates");
    assert_eq!(prepared.graph.actors.len(), 56);
    assert_eq!(
        prepared.bytes.len() - prepared.graph.bytecode_offset,
        original_code.len()
    );
    assert_eq!(
        &prepared.bytes[prepared.graph.bytecode_offset..],
        original_code
    );
    assert_eq!(prepared.declared_size, graph.declared_size + 54 * 64);
    assert_eq!(prepared.actor_indices["descriptor:0"], 0);
    assert_eq!(prepared.actor_indices["template:55"], 55);
    assert_eq!(prepared.graph.actors[2], graph.actors[0]);
    assert_eq!(prepared.graph.actors[3], graph.actors[1]);

    let reordered = vec![
        expanded[0].clone(),
        expanded[2].clone(),
        expanded[1].clone(),
        expanded[3].clone(),
    ];
    let reordered = actor_rows::rebuild(&graph, &reordered).expect("rows reorder by identity");
    assert_eq!(reordered.actor_indices["template:2"], 1);
    assert_eq!(reordered.actor_indices["template:1"], 2);
    assert_eq!(reordered.graph.actors[1], graph.actors[0]);
    assert_eq!(reordered.graph.actors[2], graph.actors[1]);
    assert_eq!(
        &reordered.bytes[reordered.graph.bytecode_offset..],
        original_code
    );

    let removed = actor_rows::rebuild(&graph, &[expanded[0].clone()])
        .expect("unreferenced template row can be removed");
    assert_eq!(removed.graph.actors.len(), 1);
    assert_eq!(removed.actor_indices.len(), 1);
    assert_eq!(
        &removed.bytes[removed.graph.bytecode_offset..],
        original_code
    );
}

#[test]
fn reachable_instructions_remain_the_same_source_facts() {
    let (_, graph) = source();
    let rows = vec![
        Row {
            id: "descriptor:0".into(),
            source_actor: 0,
        },
        Row {
            id: "only-second-template".into(),
            source_actor: 1,
        },
    ];
    let prepared = actor_rows::rebuild(&graph, &rows).expect("template binding succeeds");
    for instruction in &prepared.graph.instructions {
        assert_eq!(
            graph
                .instructions
                .iter()
                .find(|source| source.pc == instruction.pc),
            Some(instruction),
            "every reachable instruction retains its source PC and decoded facts"
        );
    }
    assert_eq!(prepared.graph.blocks, graph.blocks);
}

#[test]
fn invalid_actor_rows_and_component_growth_fail_without_mutating_source() {
    let (bytes, graph) = source();
    let original_graph = graph.clone();

    let invalid_actor = vec![
        Row {
            id: "descriptor:0".into(),
            source_actor: 0,
        },
        Row {
            id: "bad-source".into(),
            source_actor: u32::MAX,
        },
    ];
    assert_eq!(
        actor_rows::rebuild(&graph, &invalid_actor)
            .unwrap_err()
            .kind,
        "invalid_scene_actor"
    );

    let duplicate = vec![
        Row {
            id: "descriptor:0".into(),
            source_actor: 0,
        },
        Row {
            id: "same".into(),
            source_actor: 1,
        },
        Row {
            id: "same".into(),
            source_actor: 0,
        },
    ];
    assert_eq!(
        actor_rows::rebuild(&graph, &duplicate).unwrap_err().kind,
        "invalid_scene_actor"
    );

    let empty = vec![
        Row {
            id: "descriptor:0".into(),
            source_actor: 0,
        },
        Row {
            id: String::new(),
            source_actor: 1,
        },
    ];
    assert_eq!(
        actor_rows::rebuild(&graph, &empty).unwrap_err().kind,
        "invalid_scene_actor"
    );

    let replaced_zero = vec![Row {
        id: "new-zero".into(),
        source_actor: 1,
    }];
    assert_eq!(
        actor_rows::rebuild(&graph, &replaced_zero)
            .unwrap_err()
            .kind,
        "unsupported_scene_actor"
    );
    let removed_zero = vec![Row {
        id: "descriptor:1".into(),
        source_actor: 1,
    }];
    assert_eq!(
        actor_rows::rebuild(&graph, &removed_zero).unwrap_err().kind,
        "unsupported_scene_actor"
    );

    let mut too_many = vec![Row {
        id: "descriptor:0".into(),
        source_actor: 0,
    }];
    too_many.extend((1..40_000).map(|_| Row {
        id: String::new(),
        source_actor: 0,
    }));
    assert_eq!(
        actor_rows::rebuild(&graph, &too_many).unwrap_err().kind,
        "invalid_scene_actor"
    );

    let mut malformed = graph.clone();
    malformed.declared_size = malformed.bytecode_offset - 1;
    assert!(actor_rows::rebuild(&malformed, &rows()).is_err());
    assert_eq!(graph, original_graph);
    assert_eq!(field::encode(&graph).unwrap(), bytes);
}
