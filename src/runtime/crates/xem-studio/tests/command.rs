use serde_json::{Value, json};
use xem_studio::Studio;

fn command(studio: &Studio, input: &str) -> Value {
    serde_json::from_str(&studio.command(input)).expect("Studio always returns JSON")
}

fn assert_error(studio: &Studio, input: &str, expected_kind: &str) {
    let response = command(studio, input);
    let error = response
        .get("error")
        .and_then(Value::as_object)
        .expect("error response has an error object");
    assert_eq!(response.as_object().map(serde_json::Map::len), Some(1));
    assert_eq!(error.len(), 2, "error contains only kind and message");
    assert_eq!(
        error.get("kind").and_then(Value::as_str),
        Some(expected_kind)
    );
    let message = error
        .get("message")
        .and_then(Value::as_str)
        .expect("error response has a string message");
    assert!(
        !message.trim().is_empty(),
        "error message must explain the failure"
    );
}

#[test]
fn version_reports_package_version_and_supported_operations() {
    let studio = Studio::new();
    let expected = json!({
        "version": "0.5.0",
        "operations": [
            "version", "echo", "field.decode", "field.encode", "disc.open",
            "disc.poll", "disc.missing", "disc.close", "disc.maps", "field.map_events",
            "field.structure", "names.validate", "field.lanes", "disc.cue"
        ]
    });

    assert_eq!(
        command(&studio, r#"{"op":"version"}"#),
        json!({"ok": expected})
    );
    assert_eq!(
        command(&studio, r#"{"op":"version","args":null}"#),
        json!({"ok": expected})
    );
}

#[test]
fn cue_command_returns_the_first_mode2_track_and_rejects_time_overflow() {
    let studio = Studio::new();
    let overflow = concat!(
        "FILE x.bin BINARY\n",
        "TRACK 01 MODE2/2352\n",
        "INDEX 01 4294967295:00:00\n",
    );
    let text = concat!(
        "FILE \"disc.bin\" BINARY\n",
        "TRACK 01 MODE2/2352\n",
        "INDEX 01 00:02:00\n",
        "TRACK 02 AUDIO\n",
        "INDEX 00 09:59:00\n",
    );
    assert_eq!(
        command(
            &studio,
            &json!({"op":"disc.cue","args":{"text":text}}).to_string()
        ),
        json!({
            "ok": {"file":"disc.bin","number":1,"start":150,"end":44925}
        })
    );
    assert_error(
        &studio,
        &json!({"op":"disc.cue","args":{"text":overflow}}).to_string(),
        "invalid_cue",
    );
    assert_error(
        &studio,
        &json!({"op":"disc.cue","args":{"text":" ".repeat(65_537)}}).to_string(),
        "invalid_args",
    );
}

#[test]
fn echo_round_trips_json_values_without_changing_them() {
    let studio = Studio::new();
    let values = [
        Value::Null,
        json!(true),
        json!(false),
        json!(-17),
        json!(u64::MAX),
        json!(3.125),
        json!(""),
        json!("雪だるま ☃️ — café 🐉"),
        json!([null, false, 0, "é", [1, {"nested": "値"}]]),
        json!({
            "unicode key 🧭": "Zażółć gęślą jaźń",
            "nested": {"empty": [], "null": null, "values": [1, 2, 3]},
            "large_integer": u64::MAX
        }),
        json!({
            "ok": false,
            "error": {"kind": "user-data", "message": "ordinary JSON content"},
            "nested": {"ok": null, "error": null}
        }),
    ];

    for value in values {
        let input = json!({ "op": "echo", "args": value });
        let response = command(&studio, &input.to_string());
        assert_eq!(
            response,
            json!({"ok": input["args"]}),
            "echo changed {input}"
        );
    }
}

#[test]
fn malformed_and_invalid_commands_return_structured_errors() {
    let studio = Studio::new();

    assert_error(&studio, r#"{"op":"echo","args":]"#, "malformed_json");
    assert_error(&studio, r#"{"args":null}"#, "invalid_command");
    assert_error(&studio, r#"{"op":7}"#, "invalid_command");
    assert_error(&studio, r#"["echo",{}]"#, "invalid_command");
    assert_error(&studio, "null", "invalid_command");
    assert_error(&studio, r#"{"op":"missing"}"#, "unknown_operation");

    let malformed = command(&studio, r#"{"op":"echo","args":]"#);
    let message = malformed["error"]["message"]
        .as_str()
        .expect("malformed JSON has a message");
    assert!(message.contains("line") || message.contains("column"));
}

#[test]
fn invalid_arguments_and_unknown_fields_are_rejected() {
    let studio = Studio::new();

    assert_error(&studio, r#"{"op":"version","args":false}"#, "invalid_args");
    assert_error(&studio, r#"{"op":"echo"}"#, "invalid_args");
    assert_error(
        &studio,
        r#"{"op":"echo","args":null,"extra":true}"#,
        "invalid_args",
    );
    assert_error(&studio, r#"{"op":"version","extra":true}"#, "invalid_args");
}

#[test]
fn an_error_does_not_poison_later_valid_commands() {
    let studio = Studio::new();
    assert_error(&studio, "not json", "malformed_json");
    assert_eq!(
        command(&studio, r#"{"op":"echo","args":{"ok":true}}"#),
        json!({"ok": {"ok": true}})
    );
}
