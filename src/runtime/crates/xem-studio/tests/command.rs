use serde_json::{json, Value};
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
        "operations": ["version", "echo"]
    });

    assert_eq!(command(&studio, r#"{"op":"version"}"#), expected);
    assert_eq!(
        command(&studio, r#"{"op":"version","args":null}"#),
        expected
    );
}

#[test]
fn echo_round_trips_json_values_without_changing_them() {
    let studio = Studio::new();
    let values = [
        Value::Null,
        json!(true),
        json!(-17),
        json!(u64::MAX),
        json!(3.125),
        json!("雪だるま ☃️ — café 🐉"),
        json!([null, false, 0, "é", [1, {"nested": "値"}]]),
        json!({
            "unicode key 🧭": "Zażółć gęślą jaźń",
            "nested": {"empty": [], "null": null, "values": [1, 2, 3]},
            "large_integer": u64::MAX
        }),
    ];

    for value in values {
        let input = json!({ "op": "echo", "args": value });
        let response = command(&studio, &input.to_string());
        assert_eq!(response, input["args"], "echo changed {input}");
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
        json!({"ok": true})
    );
}
