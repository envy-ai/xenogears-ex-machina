use xem_studio::{field, scene::bindings};

const BYTECODE_OFFSET: usize = 132 + 64;

fn graph(code: &[u8]) -> field::FieldGraph {
    let mut bytes = vec![0_u8; BYTECODE_OFFSET + code.len()];
    bytes[128..132].copy_from_slice(&1_u32.to_le_bytes());
    bytes[BYTECODE_OFFSET..].copy_from_slice(code);
    field::decode(&bytes, Some(bytes.len())).expect("synthetic field graph decodes")
}

fn ids(prefix: &str, count: usize) -> Vec<String> {
    (0..count)
        .map(|index| format!("{prefix}-{index}"))
        .collect()
}

#[test]
fn literal_actor_and_zone_targets_remap_by_stable_identity() {
    // 0xfc is a literal actor index; it is not one of the symbolic selectors.
    let mut descriptors = ids("actor", 253);
    let target = "authored-actor".to_owned();
    descriptors[252] = target.clone();
    let actor_graph = graph(&[0x07, 0xfc, 0, 0]); // request_event, then end
    let captured = bindings::capture(&actor_graph, &descriptors, &[]).expect("capture actor");
    assert_eq!(captured.entries.len(), 1);
    assert_eq!(
        captured.entries[0].target,
        bindings::Target::Entity { id: target.clone() }
    );

    let mut moved_descriptors = vec![target];
    moved_descriptors.extend(descriptors.into_iter().filter(|id| id != "authored-actor"));
    assert_eq!(
        captured
            .remap(&actor_graph, &moved_descriptors, &[])
            .unwrap(),
        [bindings::ReferencePatch {
            pc: 0,
            operand: 0,
            value: 0
        }]
    );

    let zones = vec!["north".to_owned(), "south".to_owned()];
    // call_in_zone zone 1 -> pc 4, then terminal end.
    let zone_graph = graph(&[0x0a, 1, 4, 0, 0]);
    let actor_namespace = vec!["actor-0".to_owned()];
    let zone_capture =
        bindings::capture(&zone_graph, &actor_namespace, &zones).expect("capture zone");
    assert_eq!(
        zone_capture.entries[0].target,
        bindings::Target::Entity { id: "south".into() }
    );
    assert_eq!(
        zone_capture
            .remap(
                &zone_graph,
                &actor_namespace,
                &["south".into(), "north".into()]
            )
            .unwrap(),
        [bindings::ReferencePatch {
            pc: 0,
            operand: 0,
            value: 0
        }]
    );
}

#[test]
fn self_and_party_selectors_stay_symbolic_and_raw_descriptor_meaning_is_protected() {
    let mut descriptors = ids("actor", 256);
    descriptors[251] = "self-raw-slot".into();
    descriptors[255] = "party-raw-slot".into();
    let self_graph = graph(&[0x54, 0xfb, 0, 0, 0, 0]); // move_to_actor, then end
    let captured =
        bindings::capture(&self_graph, &descriptors, &[]).expect("capture self selector");
    assert_eq!(captured.entries[0].target, bindings::Target::SelfActor);
    assert_eq!(
        captured.entries[0].raw_descriptor.as_deref(),
        Some("self-raw-slot")
    );

    let mut stable_reorder = descriptors.clone();
    stable_reorder.swap(0, 1);
    assert!(
        captured
            .remap(&self_graph, &stable_reorder, &[])
            .unwrap()
            .is_empty()
    );

    let mut conflicting_reorder = descriptors.clone();
    conflicting_reorder.swap(250, 251);
    let error = captured
        .remap(&self_graph, &conflicting_reorder, &[])
        .expect_err("raw descriptor cannot move under a symbolic selector");
    assert_eq!(error.kind, "unsupported_scene_reference");

    let party_graph = graph(&[0x54, 0xff, 0, 0, 0, 0]);
    let party = bindings::capture(&party_graph, &descriptors, &[]).expect("capture party selector");
    assert_eq!(party.entries[0].target, bindings::Target::Party { slot: 0 });
    assert!(
        party
            .remap(&party_graph, &stable_reorder, &[])
            .unwrap()
            .is_empty()
    );
}

#[test]
fn aliased_actor_variable_byte_refuses_automatic_repair() {
    let mut descriptors = vec!["before".into(), "after".into()];
    let code = [0xfe, 0x2c, 0, 0, 0]; // store_actor_flags, then end
    let source = graph(&code);
    let captured = bindings::capture(&source, &descriptors, &[]).expect("capture aliased actor");
    assert_eq!(captured.entries[0].aliases.len(), 1);
    assert_eq!(captured.entries[0].aliases[0].kind, "var");

    descriptors.swap(0, 1);
    let error = captured
        .remap(&source, &descriptors, &[])
        .expect_err("changing the aliased byte must not silently alter its variable meaning");
    assert_eq!(error.kind, "unsupported_scene_reference");
}

#[test]
fn unknown_bindings_remain_inspectable_and_only_block_the_affected_namespace() {
    let source = graph(&[0x67, 0, 0, 0, 0]); // actor_face_direction, then end
    let descriptors = vec!["actor".into()];
    let captured = bindings::capture(&source, &descriptors, &["zone-a".into()])
        .expect("unknown source binding is still inspectable");
    assert!(matches!(
        captured.entries[0].target,
        bindings::Target::Unknown { .. }
    ));

    assert!(
        captured
            .remap(&source, &descriptors, &["zone-b".into()])
            .unwrap()
            .is_empty()
    );
    let error = captured
        .remap(&source, &["actor-moved".into()], &["zone-a".into()])
        .expect_err("unknown actor meaning blocks structural actor remap");
    assert_eq!(error.kind, "unsupported_scene_reference");
}

#[test]
fn duplicate_dangling_and_foreign_graph_references_are_rejected() {
    let source = graph(&[0x07, 0, 0, 0]);
    assert!(bindings::capture(&source, &["same".into(), "same".into()], &[]).is_err());
    let captured = bindings::capture(&source, &["stable".into()], &[]).expect("capture source");
    assert!(captured.remap(&source, &["different".into()], &[]).is_err());

    let duplicate_namespace = ["duplicate".to_owned(), "duplicate".to_owned()];
    assert!(captured.remap(&source, &duplicate_namespace, &[]).is_err());

    let other_graph = graph(&[0x07, 0, 1, 0]);
    let error = captured
        .remap(&other_graph, &["stable".into()], &[])
        .expect_err("capture cannot be reused for a different source graph");
    assert_eq!(error.kind, "invalid_scene_reference");
}
