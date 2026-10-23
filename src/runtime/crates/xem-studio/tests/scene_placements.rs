#[path = "support/scene_bundle.rs"]
#[allow(dead_code)]
mod scene_bundle;

use serde_json::{Value, json};
use std::collections::BTreeSet;
use xem_studio::{
    Studio, field,
    scene::{bundle, domain, placements},
};

const EVENT_HEADER: usize = 132;
const ACTOR_ROW: usize = 64;
const EVENT_BYTECODE_OFFSET: usize = EVENT_HEADER + ACTOR_ROW;

fn input(bytes: Vec<u8>) -> bundle::Input {
    bundle::Input {
        bundle: bytes,
        context: None,
        source: None,
        module_sha256: None,
    }
}

fn source_bundle(actor_count: usize, entries: &[(usize, usize, usize)], code: &[u8]) -> Vec<u8> {
    let bytecode_offset = EVENT_HEADER + actor_count * ACTOR_ROW;
    let mut event = vec![0; bytecode_offset + code.len()];
    event[128..132].copy_from_slice(&(actor_count as u32).to_le_bytes());
    for &(actor, event_id, pc) in entries {
        assert!(actor < actor_count && event_id < 32 && pc <= u16::MAX as usize);
        let at = EVENT_HEADER + actor * ACTOR_ROW + event_id * 2;
        event[at..at + 2].copy_from_slice(&(pc as u16).to_le_bytes());
    }
    event[bytecode_offset..].copy_from_slice(code);

    let mut components = scene_bundle::components();
    components[5] = event;
    let original = scene_bundle::bundle(&components);
    if actor_count == 1 {
        return original;
    }

    let source = bundle::Source::load(input(original)).expect("load complete fixture");
    let mut header = source.header.clone();
    while header.descriptors.len() < actor_count {
        header.descriptors.push(header.descriptors[0].clone());
    }
    source
        .repack(&header, &Default::default())
        .expect("add source-backed descriptors for actor rows")
}

fn scene(bytes: Vec<u8>) -> domain::Draft {
    domain::Draft::open(input(bytes), Vec::new()).expect("open synthetic scene")
}

fn command(studio: &Studio, op: &str, args: Value) -> Value {
    serde_json::from_str(&studio.command(&json!({"op":op,"args":args}).to_string()))
        .expect("Studio returns JSON")
}

fn open_args(bytes: Vec<u8>) -> Value {
    let args = scene_bundle::scene_args(bytes);
    json!({"kind":"open","input":args["input"],"entrances":[]})
}

fn binding(scene: &domain::Draft, node: &str) -> Value {
    serde_json::to_value(scene.placement(node).expect("describe qualified placement"))
        .expect("serialize placement binding")
}

fn change(
    node: &str,
    actor: &str,
    event: u8,
    position: [Option<i16>; 3],
    layer: Option<u8>,
) -> domain::Action {
    domain::Action::SetPlacement {
        change: placements::Change {
            node: node.into(),
            actor: actor.into(),
            event,
            position,
            layer,
        },
    }
}

fn assert_same_snapshot_and_bundle(scene: &domain::Draft, before: &Value, bytes: &[u8]) {
    assert_eq!(
        serde_json::to_value(scene.snapshot().unwrap()).unwrap(),
        *before
    );
    assert_eq!(scene.compile().unwrap().bundle, bytes);
}

fn source_components(bytes: Vec<u8>) -> Vec<bundle::Component> {
    bundle::Source::load(input(bytes))
        .expect("load complete scene bundle")
        .components
}

fn immediate(value: i16) -> Value {
    json!({"kind":"immediate","value":value})
}

#[test]
fn qualified_placement_forms_report_world_axes_and_explicit_source_bindings() {
    let cases = [
        (
            "floor placement",
            vec![0xff, 0x19, 0x34, 0x12, 0x78, 0x56, 0xc0, 0x00],
            "field_event_place",
            vec![
                immediate(0x1234),
                json!({"kind":"floor","explicit_layer":null}),
                immediate(0x5678),
            ],
        ),
        (
            "layer floor placement",
            vec![0xff, 0x1b, 0x34, 0x12, 0x78, 0x56, 0, 0xc0, 0x00],
            "field_event_place_on_layer",
            vec![
                immediate(0x1234),
                json!({"kind":"floor","explicit_layer":0}),
                immediate(0x5678),
            ],
        ),
        (
            "height placement",
            vec![0xff, 0x1d, 0xfe, 0xff, 0x0d, 0, 0xf9, 0xff, 0x00],
            "field_event_place_at_height",
            vec![immediate(-2), immediate(-7), immediate(13)],
        ),
        (
            "extended position",
            vec![
                0xff, 0xfe, 0x1c, 0xfe, 0xff, 0x0d, 0, 0xf9, 0xff, 0xe0, 0x00,
            ],
            "field_event_set_position",
            vec![immediate(-2), immediate(-7), immediate(13)],
        ),
    ];

    for (label, code, handler, expected_position) in cases {
        let current = scene(source_bundle(1, &[(0, 0, 1)], &code));
        let found = binding(&current, "pc:1");
        assert_eq!(found["handler"], handler, "{label}");
        assert_eq!(found["position"], json!(expected_position), "{label}");
        assert_eq!(
            found["owners"],
            json!([{"actor":"descriptor:0","event":0}]),
            "{label}"
        );
        assert_eq!(found["can_transform"], true, "{label}");
    }
}

#[test]
fn immediate_position_edit_changes_only_the_selected_axis_and_preserves_the_full_scene() {
    let code = vec![
        0xff, 0xfe, 0x1c, 0xfe, 0xff, 0x0d, 0, 0xf9, 0xff, 0xe0, 0x00,
    ];
    let original_bundle = source_bundle(1, &[(0, 0, 1)], &code);
    let original_components = source_components(original_bundle.clone());
    let mut current = scene(original_bundle);
    let next_event_id = current.snapshot().unwrap().next_event_id;
    current
        .edit(
            0,
            change("pc:1", "descriptor:0", 0, [Some(-123), None, None], None),
        )
        .expect("authored X change uses the selected immediate source operand");
    let compiled = current.compile().unwrap();
    let updated_components = source_components(compiled.bundle.clone());

    for index in [0, 1, 2, 3, 4, 6, 7, 8] {
        assert_eq!(
            updated_components[index].bytes, original_components[index].bytes,
            "component {index}"
        );
    }
    assert_eq!(
        updated_components[5].logical_size,
        original_components[5].logical_size
    );
    assert!(updated_components[5].bytes.len() >= original_components[5].bytes.len());
    let changed: BTreeSet<_> = original_components[5]
        .bytes
        .iter()
        .zip(&updated_components[5].bytes[..original_components[5].bytes.len()])
        .enumerate()
        .filter_map(|(index, (before, after))| (before != after).then_some(index))
        .collect();
    let code_start = EVENT_BYTECODE_OFFSET;
    assert_eq!(changed, BTreeSet::from([code_start + 3]));
    assert_eq!(
        serde_json::to_value(current.placement("pc:1").unwrap().position).unwrap(),
        json!([immediate(-123), immediate(-7), immediate(13)])
    );
    assert_eq!(current.snapshot().unwrap().next_event_id, next_event_id);

    current.undo(1).expect("undo placement");
    assert_eq!(
        current.compile().unwrap().bundle,
        source_bundle(1, &[(0, 0, 1)], &code)
    );
    current.redo(2).expect("redo placement");
    assert_eq!(current.compile().unwrap().bundle, compiled.bundle);
    assert!(
        serde_json::to_value(current.document()).unwrap()["actions"][0]["kind"] == "set_placement"
    );
    let exported = current.document();
    let imported = domain::Draft::import(exported).expect("import placement history");
    assert_eq!(imported.compile().unwrap().bundle, compiled.bundle);
    assert_eq!(
        serde_json::to_value(imported.placement("pc:1").unwrap().position).unwrap(),
        serde_json::to_value(current.placement("pc:1").unwrap().position).unwrap()
    );
}

#[test]
fn floor_y_variable_coordinates_and_nonwritable_placement_nodes_refuse_atomically() {
    let floor_code = vec![0xff, 0x19, 1, 0, 2, 0, 0xc0, 0x00];
    let mut floor_scene = scene(source_bundle(1, &[(0, 0, 1)], &floor_code));
    let before_snapshot = serde_json::to_value(floor_scene.snapshot().unwrap()).unwrap();
    let before_bundle = floor_scene.compile().unwrap().bundle;
    let floor_y = floor_scene.edit(
        0,
        change("pc:1", "descriptor:0", 0, [None, Some(12), None], None),
    );
    assert_eq!(floor_y.unwrap_err().kind, "unsupported_scene_placement");
    assert_same_snapshot_and_bundle(&floor_scene, &before_snapshot, &before_bundle);

    let variable_code = vec![0xff, 0x19, 5, 0, 9, 0, 0, 0x00];
    let mut variable_scene = scene(source_bundle(1, &[(0, 0, 1)], &variable_code));
    let found = binding(&variable_scene, "pc:1");
    assert_eq!(found["can_transform"], false);
    assert_eq!(found["position"][0], json!({"kind":"variable","id":5}));
    let before_snapshot = serde_json::to_value(variable_scene.snapshot().unwrap()).unwrap();
    let before_bundle = variable_scene.compile().unwrap().bundle;
    let rejected = variable_scene.edit(
        0,
        change("pc:1", "descriptor:0", 0, [Some(100), None, None], None),
    );
    assert_eq!(rejected.unwrap_err().kind, "unsupported_scene_placement");
    assert_same_snapshot_and_bundle(&variable_scene, &before_snapshot, &before_bundle);

    let unsupported = vec![0xff, 0x0e, 0x00];
    let unsupported_scene = scene(source_bundle(1, &[(0, 0, 1)], &unsupported));
    assert_eq!(
        unsupported_scene.placement("pc:1").unwrap_err().kind,
        "unsupported_scene_placement"
    );
    assert_eq!(
        unsupported_scene.placement("pc:99").unwrap_err().kind,
        "unsupported_scene_placement"
    );
    let unreachable = vec![0xff, 0x00, 0x1d, 1, 0, 2, 0, 3, 0, 0x00];
    let unreachable_scene = scene(source_bundle(1, &[(0, 0, 1)], &unreachable));
    assert_eq!(
        unreachable_scene.placement("pc:2").unwrap_err().kind,
        "unsupported_scene_placement"
    );
}

#[test]
fn explicit_layer_bounds_and_selector_flags_preserve_other_fields() {
    let code = vec![0xff, 0x1b, 1, 0, 2, 0, 0, 0xc0, 0x00];
    let original = source_bundle(1, &[(0, 0, 1)], &code);
    let mut current = scene(original.clone());
    let before = serde_json::to_value(current.snapshot().unwrap()).unwrap();
    let original_outputs = source_components(original.clone());
    let invalid = current.edit(
        0,
        change("pc:1", "descriptor:0", 0, [Some(7), None, None], Some(1)),
    );
    assert_eq!(invalid.unwrap_err().kind, "unsupported_scene_placement");
    assert_same_snapshot_and_bundle(&current, &before, &original);

    current
        .edit(
            0,
            change(
                "pc:1",
                "descriptor:0",
                0,
                [Some(7), None, Some(-11)],
                Some(0),
            ),
        )
        .expect("layer zero is the explicit decoded collision namespace");
    let compiled = current.compile().unwrap();
    let updated = source_components(compiled.bundle);
    for index in [0, 1, 2, 3, 4, 6, 7, 8] {
        assert_eq!(
            updated[index].bytes, original_outputs[index].bytes,
            "component {index}"
        );
    }
    assert_eq!(updated[5].logical_size, original_outputs[5].logical_size);
    assert!(updated[5].bytes.len() >= original_outputs[5].bytes.len());
    let changed: BTreeSet<_> = original_outputs[5]
        .bytes
        .iter()
        .zip(&updated[5].bytes[..original_outputs[5].bytes.len()])
        .enumerate()
        .filter_map(|(index, (before, after))| (before != after).then_some(index))
        .collect();
    let code_start = EVENT_BYTECODE_OFFSET;
    assert_eq!(
        changed,
        BTreeSet::from([code_start + 2, code_start + 4, code_start + 5])
    );
    assert_eq!(
        updated[5].bytes[code_start + 6],
        original_outputs[5].bytes[code_start + 6]
    );
    assert_eq!(updated[5].bytes[code_start + 7], 0xc0);
}

#[test]
fn shared_actor_contexts_and_owner_mismatch_cannot_author_a_single_actor_transform() {
    let code = vec![0xff, 0x1d, 1, 0, 2, 0, 3, 0, 0x00];
    let shared = source_bundle(2, &[(0, 0, 1), (1, 0, 1)], &code);
    let mut current = scene(shared);
    let found = binding(&current, "pc:1");
    assert_eq!(found["owners"].as_array().unwrap().len(), 2);
    assert_eq!(found["can_transform"], false);
    let before = serde_json::to_value(current.snapshot().unwrap()).unwrap();
    let before_bundle = current.compile().unwrap().bundle;
    let refused = current.edit(
        0,
        change("pc:1", "descriptor:0", 0, [Some(9), None, None], None),
    );
    assert_eq!(refused.unwrap_err().kind, "unsupported_scene_placement");
    assert_same_snapshot_and_bundle(&current, &before, &before_bundle);

    let mut owned = scene(source_bundle(2, &[(0, 0, 1)], &code));
    let before = serde_json::to_value(owned.snapshot().unwrap()).unwrap();
    let before_bundle = owned.compile().unwrap().bundle;
    let mismatch = owned.edit(
        0,
        change("pc:1", "descriptor:1", 0, [Some(9), None, None], None),
    );
    assert_eq!(mismatch.unwrap_err().kind, "unsupported_scene_placement");
    assert_same_snapshot_and_bundle(&owned, &before, &before_bundle);
}

#[test]
fn reverse_followed_edge_owners_match_reachable_pcs_across_calls_cycles_and_alias_entries() {
    // actor0 events 0/1 call placement@8; the call returns to end@4. The
    // placement then jumps back to itself, and actor1 event0 aliases it directly.
    let code = vec![
        0xff, 0x05, 8, 0, 0x00, 0x0e, 0x0e, 0x0e, 0x1d, 1, 0, 2, 0, 3, 0, 0x01, 8, 0, 0x00,
    ];
    let bytes = source_bundle(2, &[(0, 0, 1), (0, 1, 1), (1, 0, 8)], &code);
    let current = scene(bytes);
    let compiled = current.compile().unwrap().events;
    let found = current.placement("pc:8").unwrap();
    let index = field::structure::StructureIndex::new(&compiled.graph).unwrap();
    let mut expected = Vec::new();
    for actor in 0..compiled.graph.actors.len() {
        for event in 0..32 {
            if index
                .reachable_pcs(actor, event)
                .unwrap()
                .contains(&(found.pc as usize))
            {
                expected.push((format!("descriptor:{actor}"), event as u8));
            }
        }
    }
    let actual: Vec<_> = found
        .owners
        .iter()
        .map(|owner| (owner.actor.clone(), owner.event))
        .collect();
    assert_eq!(actual, expected);
    assert_eq!(
        actual,
        vec![
            ("descriptor:0".into(), 0),
            ("descriptor:0".into(), 1),
            ("descriptor:1".into(), 0),
        ]
    );
}

#[test]
fn pinned_overlapping_instruction_and_invalid_wire_changes_are_atomic() {
    // Every zero event row enters the jump at PC0. Its target is PC2, inside
    // the jump operand, so the placement view exists but belongs to pinned
    // overlapping source storage and is not writable.
    let overlap = vec![0xff, 0x1d, 1, 0, 0x1d, 2, 0, 3, 0, 0, 0x00];
    let mut current = scene(source_bundle(1, &[(0, 0, 1), (0, 1, 3)], &overlap));
    let found = binding(&current, "pc:1");
    assert_eq!(found["can_transform"], false);
    assert!(
        found["diagnostics"]
            .as_array()
            .unwrap()
            .iter()
            .any(|value| { value.as_str().unwrap().contains("pinned") })
    );
    let before = serde_json::to_value(current.snapshot().unwrap()).unwrap();
    let before_bundle = current.compile().unwrap().bundle;
    let refused = current.edit(
        0,
        change("pc:1", "descriptor:0", 0, [Some(8), None, None], None),
    );
    assert_eq!(refused.unwrap_err().kind, "unsupported_scene_placement");
    assert_same_snapshot_and_bundle(&current, &before, &before_bundle);

    let studio = Studio::new();
    let original = source_bundle(1, &[(0, 0, 1)], &[0xff, 0x1d, 1, 0, 2, 0, 3, 0, 0x00]);
    let opened = command(&studio, "field.scene.draft", open_args(original));
    let scene_id = opened["ok"]["scene"].as_u64().unwrap();
    let snapshot = command(
        &studio,
        "field.scene.draft",
        json!({"kind":"snapshot","scene":scene_id}),
    );
    let malformed = command(
        &studio,
        "field.scene.draft",
        json!({"kind":"edit","scene":scene_id,"revision":0,
        "action":{"kind":"set_placement","change":{
            "node":"pc:1","actor":"descriptor:0","event":0,
            "position":[32768,null,null],"layer":null
        }}}),
    );
    assert_eq!(malformed["error"]["kind"], "invalid_args");
    let after = command(
        &studio,
        "field.scene.draft",
        json!({"kind":"snapshot","scene":scene_id}),
    );
    assert_eq!(after, snapshot, "bad wire values do not mutate scene state");
    let bad_layer = command(
        &studio,
        "field.scene.draft",
        json!({"kind":"edit","scene":scene_id,"revision":0,
        "action":{"kind":"set_placement","change":{
            "node":"pc:1","actor":"descriptor:0","event":0,
            "position":[null,null,null],"layer":256
        }}}),
    );
    assert_eq!(bad_layer["error"]["kind"], "invalid_args");
    assert_eq!(
        command(
            &studio,
            "field.scene.draft",
            json!({"kind":"snapshot","scene":scene_id})
        ),
        snapshot
    );
    let bad_event = command(
        &studio,
        "field.scene.draft",
        json!({"kind":"edit","scene":scene_id,"revision":0,
        "action":{"kind":"set_placement","change":{
            "node":"pc:1","actor":"descriptor:0","event":256,
            "position":[null,null,null],"layer":null
        }}}),
    );
    assert_eq!(bad_event["error"]["kind"], "invalid_args");
    assert_eq!(
        command(
            &studio,
            "field.scene.draft",
            json!({"kind":"snapshot","scene":scene_id})
        ),
        snapshot
    );
}

#[test]
fn placement_protocol_supports_history_export_import_and_checks_actor_event_owner() {
    let original = source_bundle(1, &[(0, 0, 1)], &[0xff, 0x1d, 1, 0, 2, 0, 3, 0, 0x00]);
    let studio = Studio::new();
    let opened = command(&studio, "field.scene.draft", open_args(original.clone()));
    let scene_id = opened["ok"]["scene"].as_u64().unwrap();
    let found = command(
        &studio,
        "field.scene.draft",
        json!({"kind":"placement","scene":scene_id,"node":"pc:1"}),
    );
    assert_eq!(found["ok"]["handler"], "field_event_place_at_height");
    assert_eq!(
        found["ok"]["owners"],
        json!([{"actor":"descriptor:0","event":0}])
    );

    let mismatch = command(
        &studio,
        "field.scene.draft",
        json!({"kind":"edit","scene":scene_id,"revision":0,
        "action":{"kind":"set_placement","change":{
            "node":"pc:1","actor":"descriptor:1","event":0,
            "position":[9,null,null],"layer":null
        }}}),
    );
    assert_eq!(mismatch["error"]["kind"], "unsupported_scene_placement");
    let bad_event = command(
        &studio,
        "field.scene.draft",
        json!({"kind":"edit","scene":scene_id,"revision":0,
        "action":{"kind":"set_placement","change":{
            "node":"pc:1","actor":"descriptor:0","event":1,
            "position":[9,null,null],"layer":null
        }}}),
    );
    assert_eq!(bad_event["error"]["kind"], "unsupported_scene_placement");

    let before_event_id = opened["ok"]["state"]["next_event_id"].clone();
    let edit = command(
        &studio,
        "field.scene.draft",
        json!({"kind":"edit","scene":scene_id,"revision":0,
        "action":{"kind":"set_placement","change":{
            "node":"pc:1","actor":"descriptor:0","event":0,
            "position":[-77,88,-99],"layer":null
        }}}),
    );
    assert_eq!(edit["ok"]["state"]["revision"], 1);
    assert_eq!(edit["ok"]["state"]["next_event_id"], before_event_id);
    let edited_compile = command(
        &studio,
        "field.scene.draft",
        json!({"kind":"compile","scene":scene_id}),
    );
    assert_eq!(edited_compile["ok"]["events"]["bindings"]["pc:1"], 1);
    command(
        &studio,
        "field.scene.draft",
        json!({"kind":"undo","scene":scene_id,"revision":1}),
    );
    let undo_compile = command(
        &studio,
        "field.scene.draft",
        json!({"kind":"compile","scene":scene_id}),
    );
    assert_eq!(undo_compile["ok"]["bundle"], json!(original));
    command(
        &studio,
        "field.scene.draft",
        json!({"kind":"redo","scene":scene_id,"revision":2}),
    );
    command(
        &studio,
        "field.scene.draft",
        json!({"kind":"undo","scene":scene_id,"revision":3}),
    );
    let alternate = command(
        &studio,
        "field.scene.draft",
        json!({"kind":"edit","scene":scene_id,"revision":4,
        "action":{"kind":"set_placement","change":{
            "node":"pc:1","actor":"descriptor:0","event":0,
            "position":[-66,77,-88],"layer":null
        }}}),
    );
    assert_eq!(alternate["ok"]["state"]["revision"], 5);
    assert_eq!(alternate["ok"]["state"]["next_event_id"], before_event_id);
    let alternate_compile = command(
        &studio,
        "field.scene.draft",
        json!({"kind":"compile","scene":scene_id}),
    );
    let exported = command(
        &studio,
        "field.scene.draft",
        json!({"kind":"export","scene":scene_id}),
    );
    command(
        &studio,
        "field.scene.draft",
        json!({"kind":"close","scene":scene_id}),
    );
    let imported = command(
        &studio,
        "field.scene.draft",
        json!({"kind":"import","document":exported["ok"]}),
    );
    let imported_id = imported["ok"]["scene"].as_u64().unwrap();
    assert_eq!(imported["ok"]["state"]["next_event_id"], before_event_id);
    let recompiled = command(
        &studio,
        "field.scene.draft",
        json!({"kind":"compile","scene":imported_id}),
    );
    assert_eq!(
        recompiled["ok"]["bundle"],
        alternate_compile["ok"]["bundle"]
    );
    assert_ne!(
        alternate_compile["ok"]["bundle"],
        edited_compile["ok"]["bundle"]
    );
}
