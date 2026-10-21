use std::collections::BTreeMap;

use serde_json::json;
use xem_studio::Studio;
use xem_studio::names::{self, NamesContext};

fn context(entries: &[(u32, usize)]) -> NamesContext {
    NamesContext {
        actor_counts: entries.iter().copied().collect::<BTreeMap<_, _>>(),
    }
}

fn assert_kind(input: &str, ctx: &NamesContext, expected: &str) {
    let error = names::validate(input, ctx).expect_err("invalid table must be rejected");
    assert_eq!(error.kind, expected, "input: {input}");
    assert!(!error.message.trim().is_empty());
}

#[test]
fn validates_counts_and_keeps_names_in_their_separate_scopes() {
    let input = r#"{
        "schema_version":1,
        "variables":{"0":"Shared","1":"最大 HP"},
        "maps":{"7":{"actors":{
            "0":{"name":"Shared","events":{"0":"Shared","31":"Farewell"}},
            "1":{"events":{"2":"Speak"}}
        }}},
        "messages":{"65535":"Shared"}
    }"#;
    let validated = names::validate(input, &context(&[(7, 2)])).expect("valid table");

    assert_eq!(validated.variable_count, 2);
    assert_eq!(validated.actor_count, 1);
    assert_eq!(validated.event_count, 3);
    assert_eq!(validated.message_count, 1);
    assert_eq!(
        validated.table.maps["7"].actors["0"].events["31"],
        "Farewell"
    );
}

#[test]
fn duplicate_raw_json_keys_are_rejected_before_object_parsing_discards_them() {
    assert_kind(
        r#"{"schema_version":1,"schema_version":1}"#,
        &NamesContext::default(),
        "invalid_names_json",
    );
    assert_kind(
        r#"{"schema_version":1,"maps":{"7":{"actors":{},"actors":{}}}}"#,
        &context(&[(7, 0)]),
        "invalid_names_json",
    );
}

#[test]
fn rejects_unknown_fields_and_noncanonical_or_out_of_range_ids() {
    let ctx = context(&[(7, 2)]);
    assert_kind(
        r#"{"schema_version":1,"extra":true}"#,
        &ctx,
        "invalid_names",
    );
    assert_kind(
        r#"{"schema_version":1,"maps":{"7":{"actors":{},"other":[]}}}"#,
        &ctx,
        "invalid_names",
    );
    assert_kind(
        r#"{"schema_version":1,"variables":{"00":"Bad"}}"#,
        &ctx,
        "invalid_names",
    );
    assert_kind(
        r#"{"schema_version":1,"variables":{"1024":"Bad"}}"#,
        &ctx,
        "invalid_names",
    );
    assert_kind(
        r#"{"schema_version":1,"messages":{"65536":"Bad"}}"#,
        &ctx,
        "invalid_names",
    );
    assert_kind(
        r#"{"schema_version":1,"maps":{"4096":{"actors":{}}}}"#,
        &context(&[(4095, 0)]),
        "invalid_names",
    );
}

#[test]
fn map_actor_and_event_names_require_loaded_in_range_context() {
    assert_kind(
        r#"{"schema_version":1,"maps":{"7":{"actors":{}}}}"#,
        &NamesContext::default(),
        "names_context_required",
    );
    assert_kind(
        r#"{"schema_version":1,"maps":{"7":{"actors":{"2":{"name":"Out of range"}}}}}"#,
        &context(&[(7, 2)]),
        "invalid_names",
    );
    assert_kind(
        r#"{"schema_version":1,"maps":{"7":{"actors":{"0":{"events":{"32":"Out of range"}}}}}}"#,
        &context(&[(7, 1)]),
        "invalid_names",
    );
    assert_kind(
        r#"{"schema_version":1}"#,
        &context(&[(4096, 0)]),
        "invalid_names",
    );
}

#[test]
fn rejects_casefold_duplicates_within_each_scope_and_invalid_text() {
    assert_kind(
        r#"{"schema_version":1,"variables":{"0":"HP","1":"hp"}}"#,
        &NamesContext::default(),
        "invalid_names",
    );
    assert_kind(
        r#"{"schema_version":1,"maps":{"7":{"actors":{"0":{"name":"Hero","events":{"0":"Speak","1":"speak"}}}}}}"#,
        &context(&[(7, 1)]),
        "invalid_names",
    );
    for name in ["", " padded", "padded ", "line\nbreak"] {
        let input = serde_json::json!({"schema_version": 1, "variables": {"0": name}});
        assert_kind(
            &input.to_string(),
            &NamesContext::default(),
            "invalid_names",
        );
    }
}

#[test]
fn names_validate_command_keeps_invalid_sidecar_text_unmodified() {
    let sidecar = r#"{"schema_version":1,"variables":{"0":"HP","1":"hp"}}"#;
    let original = sidecar.to_owned();
    let input = json!({
        "op": "names.validate",
        "args": {"json": sidecar, "context": {"actor_counts": {}}}
    });
    let response: serde_json::Value =
        serde_json::from_str(&Studio::new().command(&input.to_string()))
            .expect("names command returns JSON");

    assert_eq!(response["error"]["kind"], "invalid_names");
    assert_eq!(sidecar, original);
}

#[test]
fn names_command_parses_json_object_map_ids_as_canonical_u32_keys() {
    let studio = Studio::new();
    let sidecar = r#"{"schema_version":1,"maps":{"0":{"actors":{"1":{"name":"Hero","events":{"5":"Talk"}}}}}}"#;
    let valid = json!({
        "op": "names.validate",
        "args": {"json": sidecar, "context": {"actor_counts": {"0": 2}}}
    });
    let response: serde_json::Value =
        serde_json::from_str(&studio.command(&valid.to_string())).expect("JSON response");
    assert_eq!(response["ok"]["actor_count"], 1);
    assert_eq!(response["ok"]["event_count"], 1);
    assert_eq!(
        response["ok"]["table"]["maps"]["0"]["actors"]["1"]["name"],
        "Hero"
    );

    let noncanonical = json!({
        "op": "names.validate",
        "args": {"json": "{\"schema_version\":1}", "context": {"actor_counts": {"00": 2}}}
    });
    let response: serde_json::Value =
        serde_json::from_str(&studio.command(&noncanonical.to_string())).expect("JSON response");
    assert_eq!(response["error"]["kind"], "invalid_args");

    let out_of_range = json!({
        "op": "names.validate",
        "args": {"json": "{\"schema_version\":1}", "context": {"actor_counts": {"4096": 0}}}
    });
    let response: serde_json::Value =
        serde_json::from_str(&studio.command(&out_of_range.to_string())).expect("JSON response");
    assert_eq!(response["error"]["kind"], "invalid_names");

    let missing_context = json!({
        "op": "names.validate",
        "args": {"json": "{\"schema_version\":1,\"maps\":{\"0\":{\"actors\":{\"0\":{\"name\":\"Hero\"}}}}}"}
    });
    let response: serde_json::Value =
        serde_json::from_str(&studio.command(&missing_context.to_string())).expect("JSON response");
    assert_eq!(response["error"]["kind"], "names_context_required");
}
