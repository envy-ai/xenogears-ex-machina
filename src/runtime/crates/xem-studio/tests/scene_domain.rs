#[path = "support/scene_bundle.rs"]
#[allow(dead_code)]
mod fixture;

use std::collections::BTreeMap;
use xem_studio::scene::{
    bundle, collision,
    domain::{self, Action},
    entrances,
    formats::{self, Descriptor},
};

const BYTECODE_OFFSET: usize = 132 + 64;

fn input(bundle: Vec<u8>) -> bundle::Input {
    bundle::Input {
        bundle,
        context: None,
        source: None,
        module_sha256: None,
    }
}

fn source_bundle(mode: EventReference, sprite_template: bool) -> Vec<u8> {
    let mut components = fixture::components();
    match mode {
        EventReference::None => {}
        EventReference::Actor => set_event_code(&mut components[5], &[0x07, 1, 0, 0]),
        EventReference::Zone => {
            for value in [20_i16, 21, 22, 23, 24, 25, 26, 27, 28, 29, 30, 31] {
                components[8].extend_from_slice(&value.to_le_bytes());
            }
            set_event_code(&mut components[5], &[0x0a, 1, 19, 0, 0]);
        }
        EventReference::RawEffectOwner => {
            set_event_code(&mut components[5], &canonical_raw_effect_code());
        }
    }
    let original = fixture::bundle(&components);
    let source = bundle::Source::load(input(original)).expect("load coherent fixture");
    let mut header = source.header.clone();
    let mut static_template = header.descriptors[0].clone();
    if sprite_template {
        static_template.flags |= 0x40;
    }
    header.descriptors.push(static_template);
    source
        .repack(&header, &BTreeMap::new())
        .expect("append descriptor-only static template")
}

#[derive(Clone, Copy)]
enum EventReference {
    None,
    Actor,
    Zone,
    RawEffectOwner,
}

fn canonical_raw_effect_code() -> Vec<u8> {
    let metadata: serde_json::Value =
        serde_json::from_str(include_str!("../data/field-opcodes.json"))
            .expect("canonical generated opcode metadata");
    let mut selected = None;
    for table in ["primary", "extended"] {
        let entries = metadata[table].as_object().expect("canonical opcode table");
        if let Some((opcode, form)) = entries
            .iter()
            .flat_map(|(opcode, forms)| {
                forms
                    .as_array()
                    .unwrap()
                    .iter()
                    .map(move |form| (opcode.clone(), form.clone()))
            })
            .find(|(_, form)| form["handler"] == "field_event_begin_effect")
        {
            selected = Some((table, opcode, form));
            break;
        }
    }
    let (table, opcode, form) =
        selected.expect("canonical metadata contains the indirect effect-owner reader");
    assert!(
        form["select"].is_null(),
        "fixture uses the metadata's unselected instruction form"
    );
    let prefix_size = if table == "extended" { 1 } else { 0 };
    let prefix = if prefix_size == 1 {
        u8::try_from(metadata["prefix"].as_u64().expect("extended prefix"))
            .expect("prefix fits byte")
    } else {
        0
    };
    let opcode = opcode.parse::<u8>().expect("canonical decimal opcode key");
    let size = usize::try_from(form["size"].as_u64().expect("instruction size"))
        .expect("instruction size fits usize");
    let widths = metadata["widths"].as_object().expect("operand widths");
    let operands = form["operands"].as_array().expect("operand descriptions");
    let mut bytes = vec![0_u8; size + prefix_size];
    if prefix_size == 1 {
        bytes[0] = prefix;
    }
    bytes[prefix_size] = opcode;
    for operand in operands {
        let offset = usize::try_from(operand["offset"].as_u64().expect("operand offset"))
            .expect("offset fits usize");
        let width = usize::try_from(
            widths[operand["kind"].as_str().unwrap()]
                .as_u64()
                .expect("canonical operand width"),
        )
        .expect("width fits usize");
        assert!(
            offset >= 1 && offset + width <= size,
            "metadata operand lies inside instruction"
        );
    }
    bytes
}

fn set_event_code(component: &mut Vec<u8>, code: &[u8]) {
    component[132..134].copy_from_slice(&15_u16.to_le_bytes());
    component.resize(BYTECODE_OFFSET + 15 + code.len(), 0);
    component[BYTECODE_OFFSET + 15..].copy_from_slice(code);
}

fn open(mode: EventReference, sprite_template: bool) -> (Vec<u8>, domain::Draft) {
    let bytes = source_bundle(mode, sprite_template);
    let draft = domain::Draft::open(input(bytes.clone()), vec![0, 1]).expect("open scene draft");
    (bytes, draft)
}

fn descriptor(position: [u16; 3], rotation: [u16; 3], model: u16) -> Descriptor {
    Descriptor {
        flags: 0,
        rotation,
        position,
        model,
    }
}

#[test]
fn no_edit_compile_is_byte_exact_and_scene_edits_preserve_unrelated_components() {
    let (original, mut draft) = open(EventReference::None, false);
    let initial = draft.snapshot().unwrap();
    assert_eq!(
        initial
            .descriptors
            .iter()
            .map(|x| x.id.as_str())
            .collect::<Vec<_>>(),
        ["descriptor:0", "descriptor:1"]
    );
    assert_eq!(
        initial
            .zones
            .iter()
            .map(|x| x.id.as_str())
            .collect::<Vec<_>>(),
        ["zone:0"]
    );
    assert_eq!(
        draft.compile().unwrap().bundle,
        original,
        "no edit preserves every source byte"
    );

    let mut revision = draft.revision();
    draft
        .edit(
            revision,
            Action::SetDescriptor {
                id: "descriptor:0".into(),
                position: [0xFEDC, 0x1234, 0x8001],
                rotation: [11, 22, 33],
            },
        )
        .unwrap();
    revision += 1;
    draft
        .edit(
            revision,
            Action::SetZone {
                id: "zone:0".into(),
                corners: [[-1, 2, -3], [4, -5, 6], [7, 8, 9], [-10, 11, 12]],
            },
        )
        .unwrap();
    revision += 1;
    draft
        .edit(
            revision,
            Action::SetCollisionVertex {
                id: "collision:0:vertex:1".into(),
                position: [-300, 401, -502],
            },
        )
        .unwrap();
    revision += 1;
    let mut entrance = draft.snapshot().unwrap().entrances[0].clone();
    entrance.x = -1234;
    entrance.z = 2345;
    entrance.camera_raw = 0xFF;
    entrance.facing_raw = 0xFF;
    draft
        .edit(revision, Action::SetEntrance { value: entrance })
        .unwrap();

    let compiled = draft
        .compile()
        .expect("compile transforms, collision and entrance edits");
    let before = bundle::Source::load(input(original)).unwrap();
    let after = bundle::Source::load(input(compiled.bundle.clone())).unwrap();
    assert_eq!(
        after.header.descriptors[0],
        descriptor([0xFEDC, 0x1234, 0x8001], [11, 22, 33], 0)
    );
    for index in 0..9 {
        if ![1, 5, 8].contains(&index) {
            assert_eq!(
                after.components[index].bytes, before.components[index].bytes,
                "unrelated complete component {index}"
            );
        }
    }
    let decoded_zone =
        formats::decode_zones(&after.components[8].bytes, after.components[8].logical_size)
            .unwrap();
    assert_eq!(
        decoded_zone.zones[0].corners,
        [[-1, 2, -3], [4, -5, 6], [7, 8, 9], [-10, 11, 12]]
    );
    let decoded_collision = xem_studio::scene::collision::decode_collision(
        &after.components[1].bytes,
        after.components[1].logical_size,
    )
    .unwrap();
    assert_eq!(
        &decoded_collision.layers[0].vertices[1][..3],
        &[-300, 401, -502]
    );
    assert_eq!(
        compiled.events.graph.instructions,
        draft.compile().unwrap().events.graph.instructions
    );
    let decoded_entrance = entrances::decode_entrances(&compiled.events.graph, &[0]).unwrap();
    assert_eq!(
        (
            decoded_entrance[0].x,
            decoded_entrance[0].z,
            decoded_entrance[0].camera_raw,
            decoded_entrance[0].facing_raw
        ),
        (-1234, 2345, 0xFF, 0xFF)
    );
}

#[test]
fn static_instance_and_zone_crud_keep_allocated_ids_monotonic_through_history_and_import() {
    let (_, mut draft) = open(EventReference::None, false);
    let initial_bytes = draft.compile().unwrap().bundle;
    draft
        .edit(
            0,
            Action::AddInstance {
                template: "descriptor:1".into(),
                model: 0,
                position: [1, 2, 3],
                rotation: [4, 5, 6],
            },
        )
        .unwrap();
    assert_eq!(
        draft.snapshot().unwrap().descriptors.last().unwrap().id,
        "scene:1:descriptor"
    );
    let first_branch_bytes = draft.compile().unwrap().bundle;
    draft.undo(1).unwrap();
    assert_eq!(
        draft.compile().unwrap().bundle,
        initial_bytes,
        "undo restores exact source bytes"
    );
    draft.redo(2).unwrap();
    assert_eq!(
        draft.compile().unwrap().bundle,
        first_branch_bytes,
        "redo restores exact authored bytes"
    );
    draft.undo(3).unwrap();
    assert_eq!(draft.compile().unwrap().bundle, initial_bytes);
    draft
        .edit(
            4,
            Action::AddInstance {
                template: "descriptor:1".into(),
                model: 0,
                position: [7, 8, 9],
                rotation: [10, 11, 12],
            },
        )
        .unwrap();
    assert_eq!(
        draft.snapshot().unwrap().descriptors.last().unwrap().id,
        "scene:2:descriptor"
    );
    draft
        .edit(
            5,
            Action::AddZone {
                corners: [[1; 3]; 4],
            },
        )
        .unwrap();
    assert_eq!(
        draft.snapshot().unwrap().zones.last().unwrap().id,
        "scene:3:zone"
    );
    draft
        .edit(
            6,
            Action::DuplicateZone {
                id: "zone:0".into(),
            },
        )
        .unwrap();
    assert_eq!(
        draft.snapshot().unwrap().zones.last().unwrap().id,
        "scene:4:zone"
    );
    draft
        .edit(
            7,
            Action::DeleteZone {
                id: "scene:4:zone".into(),
            },
        )
        .unwrap();
    draft
        .edit(
            8,
            Action::DuplicateInstance {
                id: "scene:2:descriptor".into(),
            },
        )
        .unwrap();
    assert_eq!(
        draft.snapshot().unwrap().descriptors.last().unwrap().id,
        "scene:5:descriptor"
    );

    let document = draft.document();
    let compiled = draft.compile().unwrap();
    let restored =
        domain::Draft::import(document.clone()).expect("restore complete action history");
    assert_eq!(restored.compile().unwrap().bundle, compiled.bundle);
    assert_eq!(
        restored
            .snapshot()
            .unwrap()
            .descriptors
            .iter()
            .map(|x| &x.id)
            .collect::<Vec<_>>(),
        draft
            .snapshot()
            .unwrap()
            .descriptors
            .iter()
            .map(|x| &x.id)
            .collect::<Vec<_>>()
    );
    assert_eq!(
        restored
            .snapshot()
            .unwrap()
            .zones
            .iter()
            .map(|x| &x.id)
            .collect::<Vec<_>>(),
        draft
            .snapshot()
            .unwrap()
            .zones
            .iter()
            .map(|x| &x.id)
            .collect::<Vec<_>>()
    );
    assert_eq!(
        serde_json::to_value(restored.document()).unwrap(),
        serde_json::to_value(document.clone()).unwrap()
    );
    let mut restored = restored;
    restored
        .edit(
            restored.revision(),
            Action::DuplicateZone {
                id: "zone:0".into(),
            },
        )
        .unwrap();
    assert_eq!(
        restored.snapshot().unwrap().zones.last().unwrap().id,
        "scene:6:zone"
    );

    let mut reused = document.clone();
    reused.allocation_starts[1] = reused.allocation_starts[0];
    assert!(domain::Draft::import(reused).is_err());
    let mut bad_watermark = document.clone();
    bad_watermark.next_id = 0;
    assert!(domain::Draft::import(bad_watermark).is_err());
    let mut serialized = serde_json::to_value(document).unwrap();
    serialized.as_object_mut().unwrap().remove("next_id");
    assert!(serde_json::from_value::<domain::Document>(serialized).is_err());

    let (_, mut unreferenced) = open(EventReference::None, false);
    unreferenced
        .edit(
            0,
            Action::DeleteInstance {
                id: "descriptor:1".into(),
            },
        )
        .unwrap();
    assert_eq!(
        unreferenced.compile().unwrap().descriptor_ids,
        ["descriptor:0"]
    );
}

#[test]
fn deleting_an_unreferenced_zone_remaps_a_stable_target_and_preserves_other_outputs() {
    let (original, mut draft) = open(EventReference::Zone, false);
    let before = bundle::Source::load(input(original)).unwrap();
    assert_eq!(draft.snapshot().unwrap().zones.len(), 2);
    draft
        .edit(
            0,
            Action::DeleteZone {
                id: "zone:0".into(),
            },
        )
        .unwrap();

    let compiled = draft.compile().unwrap();
    assert_eq!(compiled.zone_ids, ["zone:1"]);
    let call = compiled
        .events
        .graph
        .instructions
        .iter()
        .find(|i| i.pc == 15)
        .unwrap();
    assert_eq!(
        call.operands[0], 0,
        "stable zone:1 reference moved from source index one to zero"
    );
    let mut expected_events = before.components[5].bytes.clone();
    expected_events[BYTECODE_OFFSET + 16] = 0;
    assert_eq!(
        compiled.events.bytes, expected_events,
        "the compiler patches only the zone selector byte"
    );

    let after = bundle::Source::load(input(compiled.bundle)).unwrap();
    for index in 0..9 {
        if index != 5 && index != 8 {
            assert_eq!(
                after.components[index].bytes, before.components[index].bytes,
                "unrelated complete component {index}"
            );
        }
    }
    assert_eq!(after.components[8].logical_size, 24);
    assert_eq!(
        formats::decode_zones(&after.components[8].bytes, 24)
            .unwrap()
            .zones[0]
            .corners,
        [[20, 21, 22], [23, 24, 25], [26, 27, 28], [29, 30, 31]]
    );
}

#[test]
fn dangling_actor_and_zone_references_fail_without_committing_deletes() {
    let (_, mut actor_draft) = open(EventReference::Actor, false);
    let before_actor = serde_json::to_value(actor_draft.snapshot().unwrap()).unwrap();
    let before_actor_bytes = actor_draft.compile().unwrap().bundle;
    let error = actor_draft
        .edit(
            0,
            Action::DeleteInstance {
                id: "descriptor:1".into(),
            },
        )
        .unwrap_err();
    assert_eq!(error.kind, "invalid_scene_reference");
    assert_eq!(
        serde_json::to_value(actor_draft.snapshot().unwrap()).unwrap(),
        before_actor
    );
    assert_eq!(actor_draft.compile().unwrap().bundle, before_actor_bytes);

    let (_, mut zone_draft) = open(EventReference::Zone, false);
    let before_zone = serde_json::to_value(zone_draft.snapshot().unwrap()).unwrap();
    let before_zone_bytes = zone_draft.compile().unwrap().bundle;
    let error = zone_draft
        .edit(
            0,
            Action::DeleteZone {
                id: "zone:1".into(),
            },
        )
        .unwrap_err();
    assert_eq!(error.kind, "invalid_scene_reference");
    assert_eq!(
        serde_json::to_value(zone_draft.snapshot().unwrap()).unwrap(),
        before_zone
    );
    assert_eq!(zone_draft.compile().unwrap().bundle, before_zone_bytes);
}

#[test]
fn stale_revision_invalid_model_sprite_rotation_and_failed_batch_are_atomic() {
    let (_, mut draft) = open(EventReference::None, false);
    let before = serde_json::to_value(draft.snapshot().unwrap()).unwrap();
    let before_bytes = draft.compile().unwrap().bundle;
    assert!(
        draft
            .edit(
                0,
                Action::AddInstance {
                    template: "descriptor:1".into(),
                    model: u16::MAX,
                    position: [0; 3],
                    rotation: [0; 3],
                }
            )
            .is_err()
    );
    assert_eq!(
        serde_json::to_value(draft.snapshot().unwrap()).unwrap(),
        before
    );
    assert_eq!(draft.compile().unwrap().bundle, before_bytes);

    draft
        .edit(
            0,
            Action::SetZone {
                id: "zone:0".into(),
                corners: [[9; 3]; 4],
            },
        )
        .unwrap();
    let after_first = serde_json::to_value(draft.snapshot().unwrap()).unwrap();
    let after_first_bytes = draft.compile().unwrap().bundle;
    assert_eq!(
        draft
            .edit(
                0,
                Action::SetZone {
                    id: "zone:0".into(),
                    corners: [[8; 3]; 4]
                }
            )
            .unwrap_err()
            .kind,
        "stale_revision"
    );
    assert_eq!(
        serde_json::to_value(draft.snapshot().unwrap()).unwrap(),
        after_first
    );
    assert_eq!(draft.compile().unwrap().bundle, after_first_bytes);

    let (_, mut batch) = open(EventReference::None, false);
    let before_batch = serde_json::to_value(batch.snapshot().unwrap()).unwrap();
    let before_batch_document = serde_json::to_value(batch.document()).unwrap();
    let before_batch_bytes = batch.compile().unwrap().bundle;
    let error = batch
        .edit(
            0,
            Action::Batch {
                actions: vec![
                    Action::AddInstance {
                        template: "descriptor:1".into(),
                        model: 0,
                        position: [1; 3],
                        rotation: [2; 3],
                    },
                    Action::SetZone {
                        id: "missing-zone".into(),
                        corners: [[3; 3]; 4],
                    },
                ],
            },
        )
        .unwrap_err();
    assert_eq!(error.kind, "invalid_scene_edit");
    assert_eq!(
        serde_json::to_value(batch.snapshot().unwrap()).unwrap(),
        before_batch
    );
    assert_eq!(
        serde_json::to_value(batch.document()).unwrap(),
        before_batch_document
    );
    assert_eq!(batch.compile().unwrap().bundle, before_batch_bytes);

    let (_, mut sprite) = open(EventReference::None, true);
    let before_sprite = serde_json::to_value(sprite.snapshot().unwrap()).unwrap();
    let before_sprite_bytes = sprite.compile().unwrap().bundle;
    let mut rotated = sprite.snapshot().unwrap().descriptors[1].value.rotation;
    rotated[0] = rotated[0].wrapping_add(1);
    let error = sprite
        .edit(
            0,
            Action::SetDescriptor {
                id: "descriptor:1".into(),
                position: [8, 9, 10],
                rotation: rotated,
            },
        )
        .unwrap_err();
    assert_eq!(error.kind, "unsupported_scene_edit");
    assert_eq!(
        serde_json::to_value(sprite.snapshot().unwrap()).unwrap(),
        before_sprite
    );
    assert_eq!(sprite.compile().unwrap().bundle, before_sprite_bytes);
}

#[test]
fn collision_crud_uses_stable_ids_preserves_other_components_and_roundtrips_history() {
    let original = fixture::valid_bundle();
    let mut draft = domain::Draft::open(input(original.clone()), vec![0, 1]).unwrap();
    let initial = draft.snapshot().unwrap();
    assert_eq!(initial.collision.layers[0].triangles.len(), 1);
    assert_eq!(
        initial.collision_entities.layers[0].vertices[0].id,
        "collision:0:vertex:0"
    );
    let original_components = bundle::Source::load(input(original.clone())).unwrap();

    draft
        .edit(
            0,
            Action::AddCollisionVertex {
                layer: "collision:0".into(),
                position: [123, -456, 789],
            },
        )
        .unwrap();
    let after_vertex = draft.snapshot().unwrap();
    assert_eq!(
        after_vertex.collision_entities.layers[0]
            .vertices
            .last()
            .unwrap()
            .id,
        "scene:1:collision_vertex"
    );
    let after_vertex_bytes = draft.compile().unwrap().bundle;

    draft
        .edit(
            1,
            Action::AddCollisionTriangle {
                layer: "collision:0".into(),
                vertices: [
                    "collision:0:vertex:2".into(),
                    "collision:0:vertex:1".into(),
                    "scene:1:collision_vertex".into(),
                ],
                attribute: 1,
                height_bump_raw: 0xF3,
            },
        )
        .unwrap();
    let after_triangle = draft.snapshot().unwrap();
    let layer = &after_triangle.collision_entities.layers[0];
    assert_eq!(
        layer.triangles.last().unwrap().id,
        "scene:2:collision_triangle"
    );
    assert_eq!(
        layer.triangles[0].value.neighbors[1].as_deref(),
        Some("scene:2:collision_triangle")
    );
    assert_eq!(
        layer.triangles[1].value.neighbors[0].as_deref(),
        Some("collision:0:triangle:0")
    );

    let compiled = draft.compile().unwrap();
    let source = bundle::Source::load(input(compiled.bundle.clone())).unwrap();
    let decoded = collision::decode_collision(
        &source.components[1].bytes,
        source.components[1].logical_size,
    )
    .unwrap();
    assert_eq!(decoded.layers[0].vertices.len(), 4);
    assert_eq!(decoded.layers[0].vertices[3], [123, -456, 789, 0]);
    assert_eq!(decoded.layers[0].triangles.len(), 2);
    assert_eq!(decoded.layers[0].triangles[0].neighbors, [-1, 1, -1]);
    assert_eq!(decoded.layers[0].triangles[1].vertices, [2, 1, 3]);
    assert_eq!(decoded.layers[0].triangles[1].neighbors, [0, -1, -1]);
    assert_eq!(decoded.layers[0].triangles[1].height_bump_raw, 0xF3);
    for index in 0..9 {
        if index != 1 {
            assert_eq!(
                source.components[index].bytes, original_components.components[index].bytes,
                "collision CRUD preserves complete unrelated component {index}"
            );
        }
    }

    let authored_bytes = compiled.bundle;
    draft.undo(2).unwrap();
    assert_eq!(
        draft.compile().unwrap().bundle,
        after_vertex_bytes,
        "undoing the face retains the preceding vertex transaction"
    );
    draft.undo(3).unwrap();
    assert_eq!(draft.compile().unwrap().bundle, original);
    draft.redo(4).unwrap();
    assert_eq!(draft.compile().unwrap().bundle, after_vertex_bytes);
    draft.redo(5).unwrap();
    assert_eq!(draft.compile().unwrap().bundle, authored_bytes);

    let restored = domain::Draft::import(draft.document()).unwrap();
    let restored_snapshot = restored.snapshot().unwrap();
    assert_eq!(
        restored_snapshot.revision, 0,
        "imports begin at revision zero"
    );
    let mut restored_snapshot = serde_json::to_value(restored_snapshot).unwrap();
    let mut authored_snapshot = serde_json::to_value(draft.snapshot().unwrap()).unwrap();
    restored_snapshot
        .as_object_mut()
        .unwrap()
        .remove("revision");
    authored_snapshot
        .as_object_mut()
        .unwrap()
        .remove("revision");
    assert_eq!(restored_snapshot, authored_snapshot);
    assert_eq!(restored.compile().unwrap().bundle, authored_bytes);
}

#[test]
fn duplicate_triangle_owns_new_vertices_and_failed_collision_actions_are_atomic() {
    let (original, mut draft) = open(EventReference::None, false);
    let before = serde_json::to_value(draft.snapshot().unwrap()).unwrap();
    let original_bytes = draft.compile().unwrap().bundle;
    assert!(
        draft
            .edit(
                0,
                Action::DeleteCollisionVertex {
                    id: "collision:0:vertex:0".into(),
                },
            )
            .is_err()
    );
    assert_eq!(
        serde_json::to_value(draft.snapshot().unwrap()).unwrap(),
        before
    );
    assert_eq!(draft.compile().unwrap().bundle, original_bytes);

    draft
        .edit(
            0,
            Action::AddCollisionVertex {
                layer: "collision:0".into(),
                position: [4, 5, 6],
            },
        )
        .unwrap();
    let after_vertex = draft.snapshot().unwrap();
    assert_eq!(
        after_vertex.collision_entities.layers[0]
            .vertices
            .last()
            .unwrap()
            .id,
        "scene:1:collision_vertex",
        "the rejected delete did not consume an identity"
    );
    let vertex_snapshot = serde_json::to_value(after_vertex).unwrap();
    assert!(
        draft
            .edit(
                1,
                Action::AddCollisionTriangle {
                    layer: "collision:0".into(),
                    vertices: [
                        "collision:0:vertex:1".into(),
                        "collision:0:vertex:2".into(),
                        "scene:1:collision_vertex".into(),
                    ],
                    attribute: 1,
                    height_bump_raw: 0,
                },
            )
            .is_err()
    );
    assert_eq!(
        serde_json::to_value(draft.snapshot().unwrap()).unwrap(),
        vertex_snapshot
    );

    draft
        .edit(
            1,
            Action::AddCollisionTriangle {
                layer: "collision:0".into(),
                vertices: [
                    "collision:0:vertex:2".into(),
                    "collision:0:vertex:1".into(),
                    "scene:1:collision_vertex".into(),
                ],
                attribute: 1,
                height_bump_raw: 0,
            },
        )
        .unwrap();
    draft
        .edit(
            2,
            Action::DuplicateCollisionTriangle {
                id: "scene:2:collision_triangle".into(),
            },
        )
        .unwrap();
    let duplicated = draft.snapshot().unwrap();
    let layer = &duplicated.collision_entities.layers[0];
    assert_eq!(layer.vertices.len(), 7);
    assert_eq!(layer.triangles.len(), 3);
    assert_eq!(
        layer
            .vertices
            .iter()
            .skip(4)
            .map(|vertex| vertex.id.as_str())
            .collect::<Vec<_>>(),
        [
            "scene:3:collision_vertex",
            "scene:4:collision_vertex",
            "scene:5:collision_vertex"
        ]
    );
    assert_eq!(layer.triangles[2].id, "scene:6:collision_triangle");
    assert!(layer.triangles[2].value.vertices.iter().all(|id| {
        [
            "scene:3:collision_vertex",
            "scene:4:collision_vertex",
            "scene:5:collision_vertex",
        ]
        .contains(&id.as_str())
    }));
    assert_eq!(layer.triangles[2].value.neighbors, [None, None, None]);
    assert_ne!(draft.compile().unwrap().bundle, original);

    let restored = domain::Draft::import(draft.document()).unwrap();
    let restored_snapshot = restored.snapshot().unwrap();
    assert_eq!(
        restored_snapshot.revision, 0,
        "imports begin at revision zero"
    );
    let mut restored_snapshot = serde_json::to_value(restored_snapshot).unwrap();
    let mut authored_snapshot = serde_json::to_value(draft.snapshot().unwrap()).unwrap();
    restored_snapshot
        .as_object_mut()
        .unwrap()
        .remove("revision");
    authored_snapshot
        .as_object_mut()
        .unwrap()
        .remove("revision");
    assert_eq!(restored_snapshot, authored_snapshot);
    assert_eq!(
        restored.compile().unwrap().bundle,
        draft.compile().unwrap().bundle
    );
}

#[test]
fn structural_collision_edit_refuses_conditional_source_semantics_atomically() {
    let mut components = fixture::components();
    // The source pre-table lookup reads byte 30, the high byte of the vertices
    // pointer word (46). It is zero and selects attribute-table entry zero.
    // Preserve the header/pointer and put the conditional bit in that entry.
    components[1][70..74].copy_from_slice(&0x400000_u32.to_le_bytes());
    let bytes = fixture::bundle(&components);
    let mut draft = domain::Draft::open(input(bytes), vec![0, 1]).unwrap();
    let before = serde_json::to_value(draft.snapshot().unwrap()).unwrap();
    let compiled = draft.compile().unwrap().bundle;
    let error = draft
        .edit(
            0,
            Action::AddCollisionVertex {
                layer: "collision:0".into(),
                position: [7, 8, 9],
            },
        )
        .unwrap_err();
    assert_eq!(error.kind, "unsupported_scene_collision");
    assert_eq!(
        serde_json::to_value(draft.snapshot().unwrap()).unwrap(),
        before
    );
    assert_eq!(draft.compile().unwrap().bundle, compiled);
}

#[test]
fn raw_effect_owner_blocks_descriptor_reindexing_but_allows_same_namespace_transform() {
    let (original, proof_draft) = open(EventReference::RawEffectOwner, false);
    assert!(
        proof_draft
            .compile()
            .unwrap()
            .events
            .graph
            .instructions
            .iter()
            .any(|instruction| instruction.spec.handler == "field_event_begin_effect")
    );

    let (_, mut transformed) = open(EventReference::RawEffectOwner, false);
    transformed
        .edit(
            0,
            Action::SetDescriptor {
                id: "descriptor:1".into(),
                position: [1, 2, 3],
                rotation: [0, 0, 0],
            },
        )
        .unwrap();
    assert!(
        transformed.compile().is_ok(),
        "a transform preserving the descriptor namespace does not shift the indirect raw owner"
    );

    let actions = [
        Action::AddInstance {
            template: "descriptor:1".into(),
            model: 0,
            position: [4, 5, 6],
            rotation: [0, 0, 0],
        },
        Action::DuplicateInstance {
            id: "descriptor:1".into(),
        },
        Action::DeleteInstance {
            id: "descriptor:1".into(),
        },
    ];
    for action in actions {
        let (_, mut draft) = open(EventReference::RawEffectOwner, false);
        let before_revision = draft.revision();
        let before_source = draft.compile().unwrap().bundle;
        let before_snapshot = serde_json::to_value(draft.snapshot().unwrap()).unwrap();
        let before_document = serde_json::to_value(draft.document()).unwrap();
        let error = draft.edit(before_revision, action).expect_err(
            "structural descriptor reindexing leaves an indirect effect owner unproven",
        );
        assert_eq!(error.kind, "unsupported_scene_reference");
        assert_eq!(
            serde_json::to_value(draft.snapshot().unwrap()).unwrap(),
            before_snapshot,
            "refused structural edit leaves the complete draft snapshot unchanged"
        );
        assert_eq!(
            serde_json::to_value(draft.document()).unwrap(),
            before_document,
            "refused structural edit does not consume stable IDs or alter history"
        );
        assert_eq!(draft.revision(), before_revision);
        assert_eq!(
            before_source, original,
            "failed structural compile leaves the source bundle byte-exact"
        );
        assert_eq!(
            draft.compile().unwrap().bundle,
            original,
            "failed structural edit leaves the original candidate compilable"
        );
    }
}
