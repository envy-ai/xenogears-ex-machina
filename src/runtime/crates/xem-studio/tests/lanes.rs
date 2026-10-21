use serde_json::json;
use xem_studio::Studio;
use xem_studio::field::lanes::{self, MapLanes};
use xem_studio::field::{self, FieldGraph};

const HEADER_BYTES: usize = 132;
const ACTOR_BYTES: usize = 64;

fn decode(actor_count: usize, code: &[u8]) -> FieldGraph {
    let bytecode_offset = HEADER_BYTES + actor_count * ACTOR_BYTES;
    let mut bytes = vec![0; bytecode_offset + code.len()];
    bytes[128..132].copy_from_slice(&(actor_count as u32).to_le_bytes());
    bytes[bytecode_offset..].copy_from_slice(code);
    field::decode(&bytes, Some(bytes.len())).expect("synthetic event component decodes")
}

fn actor_event(lanes: &MapLanes, actor: usize, event: usize) -> &lanes::EventLane {
    &lanes.actors[actor].events[event]
}

#[test]
fn access_facts_are_generated_and_match_the_declared_sources() {
    let metadata = lanes::access_metadata().expect("generated access metadata validates");
    assert_eq!(metadata.schema_version, 1);
    assert_eq!(metadata.generator, "tools/studio/field_access.py");
    assert_eq!(metadata.facts.len(), 658);
    assert!(metadata.sources.contains_key("tools/analysis/events.py"));
    assert!(
        metadata
            .facts
            .iter()
            .all(|fact| fact.direction == "unknown" || !fact.provenance.is_empty())
    );
}

#[test]
fn lanes_resolve_variable_writes_reads_and_odd_byte_aliases() {
    // set_variable_one var 5, increment_variable var 4, then end_slot.
    let graph = decode(1, &[0x36, 5, 0, 0x3c, 4, 0, 0]);
    let result = lanes::lanes(&graph).expect("variable accesses derive from metadata");
    let event = actor_event(&result, 0, 0);

    assert_eq!(event.instruction_pcs, [0, 3, 6]);
    assert_eq!(event.variable_accesses.len(), 2);
    let write = &event.variable_accesses[0];
    assert_eq!((write.pc, write.operand_offset, write.raw), (0, 1, 5));
    assert_eq!(write.slot, Some(2));
    assert!(write.odd_alias);
    assert_eq!(write.direction, "write");
    assert_eq!(write.resolution, "resolved");
    assert!(!write.provenance.is_empty());

    let read_write = &event.variable_accesses[1];
    assert_eq!(
        (read_write.pc, read_write.operand_offset, read_write.raw),
        (3, 1, 4)
    );
    assert_eq!(read_write.slot, Some(2));
    assert!(!read_write.odd_alias);
    assert_eq!(read_write.direction, "read_write");

    let dependency = result
        .variable_dependencies
        .iter()
        .find(|dependency| dependency.slot == 2)
        .expect("accesses are grouped by shared variable slot");
    assert_eq!(dependency.writers.len(), 64);
    assert_eq!(dependency.readers.len(), 32);
    assert_eq!(dependency.writers[0].pc, 0);
    assert_eq!(dependency.readers[0].pc, 3);
}

#[test]
fn request_links_resolve_direct_self_and_party_actor_selectors() {
    let graph = decode(
        2,
        &[
            0x08, 1, 0xA5, // direct actor 1, event 5, priority 5
            0x09, 0xFB, 0xFF, // self actor, event 31, priority 7
            0x08, 0xFE, 0x22, // party slot 1, event 2, priority 1
            0,
        ],
    );
    let result = lanes::lanes(&graph).expect("request selectors resolve without party state");
    let requests: Vec<_> = result
        .requests
        .iter()
        .filter(|request| request.source.actor == 0 && request.source.event == 0)
        .collect();
    assert_eq!(requests.len(), 3);

    assert_eq!(requests[0].kind, "request_event_started");
    assert_eq!(requests[0].actor.kind, "direct");
    assert_eq!(requests[0].actor.resolved_actor, Some(1));
    assert_eq!((requests[0].event, requests[0].priority), (5, 5));
    assert_eq!(
        requests[0]
            .target
            .as_ref()
            .map(|target| (target.actor, target.event)),
        Some((1, 5))
    );

    assert_eq!(requests[1].kind, "request_event_finished");
    assert_eq!(requests[1].actor.kind, "self");
    assert_eq!(requests[1].actor.resolved_actor, Some(0));
    assert_eq!((requests[1].event, requests[1].priority), (31, 7));
    assert_eq!(
        requests[1]
            .target
            .as_ref()
            .map(|target| (target.actor, target.event)),
        Some((0, 31))
    );

    assert_eq!(requests[2].actor.kind, "party");
    assert_eq!(requests[2].actor.party_slot, Some(1));
    assert_eq!(requests[2].actor.resolved_actor, None);
    assert!(requests[2].actor.unresolved.is_some());
    assert_eq!((requests[2].event, requests[2].priority), (2, 1));
    assert_eq!(requests[2].target, None);
}

#[test]
fn invalid_and_indirect_variable_operands_keep_their_resolution_reason() {
    let invalid = decode(1, &[0x37, 0x00, 0x08, 0]);
    let invalid_lanes = lanes::lanes(&invalid).expect("invalid references remain visible");
    let access = &actor_event(&invalid_lanes, 0, 0).variable_accesses[0];
    assert_eq!(access.raw, 2048);
    assert_eq!(access.reference, Some(2048));
    assert_eq!(access.slot, None);
    assert_eq!(access.resolution, "invalid_reference");
    assert!(access.reason.is_some());

    // load_code_byte uses an indirect data-array reference and writes var 6.
    let indirect = decode(1, &[0x48, 0x34, 0x12, 6, 0, 0, 0, 0]);
    let indirect_lanes = lanes::lanes(&indirect).expect("indirect references remain inspectable");
    let accesses = &actor_event(&indirect_lanes, 0, 0).variable_accesses;
    let data = accesses
        .iter()
        .find(|access| access.operand_kind == "data")
        .unwrap();
    assert_eq!(data.raw, 0x1234);
    assert_eq!(data.reference, None);
    assert_eq!(data.resolution, "indirect");
    assert_eq!(data.direction, "unknown");
    assert!(data.reason.is_some());
    let destination = accesses
        .iter()
        .find(|access| access.operand_kind == "var")
        .unwrap();
    assert_eq!(destination.slot, Some(3));
    assert_eq!(destination.direction, "write");
}

#[test]
fn field_lanes_command_returns_generated_access_facts() {
    let graph = decode(1, &[0x36, 5, 0, 0]);
    let request = json!({
        "op": "field.lanes",
        "args": {"graph": graph}
    });
    let response: serde_json::Value =
        serde_json::from_str(&Studio::new().command(&request.to_string()))
            .expect("field lanes command returns JSON");

    assert_eq!(response["ok"]["schema_version"], 1);
    assert_eq!(
        response["ok"]["actors"][0]["events"][0]["instruction_pcs"],
        json!([0, 3])
    );
    let accesses = response["ok"]["actors"][0]["events"][0]["variable_accesses"]
        .as_array()
        .expect("event has a variable access list");
    assert_eq!(accesses.len(), 1);
    assert_eq!(accesses[0]["direction"], "write");
    assert_eq!(accesses[0]["slot"], 2);
}
