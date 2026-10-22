//! Public Studio package protocol tests using invented disc identities and bytes.
use serde_json::{Value, json};
use xem_studio::Studio;

fn command(studio: &Studio, input: Value) -> Value {
    serde_json::from_str(&studio.command(&input.to_string())).expect("Studio returns JSON")
}

fn target() -> Value {
    json!({"serial":"SYNTH-1", "sha256":"a".repeat(64)})
}

fn build(studio: &Studio, id: &str, requires: &[&str], files: Vec<Value>) -> Value {
    command(
        studio,
        json!({
            "op":"package.build",
            "args":{
                "spec":{"id":id,"discs":[target()],"requires":requires,"overrides":[]},
                "files":files
            }
        }),
    )
}

fn authored(slot: u32, result: &[u8]) -> Value {
    json!({"disc":"SYNTH-1","slot":slot,"source":null,"result":result})
}

fn delta(slot: u32, source: &[u8], result: &[u8]) -> Value {
    json!({"disc":"SYNTH-1","slot":slot,"source":source,"result":result})
}

fn assert_error(response: &Value, kind: &str, contains: &str) {
    let error = response
        .get("error")
        .and_then(Value::as_object)
        .expect("error envelope");
    assert_eq!(response.as_object().map(serde_json::Map::len), Some(1));
    assert_eq!(error.len(), 2);
    assert_eq!(error.get("kind").and_then(Value::as_str), Some(kind));
    assert!(
        error
            .get("message")
            .and_then(Value::as_str)
            .unwrap()
            .contains(contains)
    );
}

fn sealed_bytes(value: &Value) -> Vec<u8> {
    value
        .as_array()
        .unwrap()
        .iter()
        .map(|byte| byte.as_u64().unwrap() as u8)
        .collect()
}

fn reseal(bytes: &mut [u8]) {
    use sha2::{Digest, Sha256};
    let end = bytes.len() - 32;
    let digest = Sha256::digest(&bytes[..end]);
    bytes[end..].copy_from_slice(&digest);
}

fn manifest_payload_start(bytes: &[u8]) -> usize {
    12 + u32::from_le_bytes(bytes[8..12].try_into().unwrap()) as usize
}

#[test]
fn build_is_deterministic_and_inspect_distinguishes_authored_from_delta_payloads() {
    let studio = Studio::new();
    let files = vec![
        delta(8, b"invented source", b"invented patched bytes"),
        authored(3, b"authored fixture bytes"),
    ];
    let first = build(&studio, "synthetic-mod", &[], files.clone());
    let second = build(&studio, "synthetic-mod", &[], files);
    let first_ok = first.get("ok").expect("build succeeds");
    let second_ok = second.get("ok").expect("second build succeeds");
    assert_eq!(first_ok["bytes"], second_ok["bytes"]);
    assert_eq!(first_ok["identity"], second_ok["identity"]);

    let manifest = &first_ok["manifest"];
    assert_eq!(manifest["files"][0]["slot"], 3);
    assert_eq!(manifest["files"][0]["kind"], "authored");
    assert!(manifest["files"][0]["source_sha256"].is_null());
    assert_eq!(manifest["files"][1]["slot"], 8);
    assert_eq!(manifest["files"][1]["kind"], "delta");
    assert!(manifest["files"][1]["source_sha256"].as_str().is_some());

    let inspected = command(
        &studio,
        json!({"op":"package.inspect","args":{"bytes":first_ok["bytes"]}}),
    );
    assert_eq!(
        inspected,
        json!({"ok":{"identity":first_ok["identity"],"manifest":manifest}})
    );
}

#[test]
fn package_arguments_are_strict_and_package_failures_keep_the_error_envelope() {
    let studio = Studio::new();
    let valid = build(&studio, "valid", &[], vec![authored(1, b"synthetic")]);
    let bytes = valid["ok"]["bytes"].clone();

    let unknown_build_field = json!({
        "op":"package.build",
        "args":{"spec":{"id":"x","discs":[target()],"requires":[],"overrides":[]},"files":[],"surprise":true}
    });
    assert_error(
        &command(&studio, unknown_build_field),
        "invalid_args",
        "unknown field",
    );
    assert_error(
        &command(
            &studio,
            json!({"op":"package.inspect","args":{"bytes":bytes,"extra":1}}),
        ),
        "invalid_args",
        "unknown field",
    );
    assert_error(
        &command(
            &studio,
            json!({"op":"package.bundle","args":{"packages":[],"extra":1}}),
        ),
        "invalid_args",
        "unknown field",
    );

    assert_error(
        &command(&studio, json!({"op":"package.inspect","args":{"bytes":[]}})),
        "invalid_package",
        "size outside bounds",
    );
    assert_error(
        &command(
            &studio,
            json!({"op":"package.inspect","args":{"bytes":[88,69,77,77,79,68,49,0]}}),
        ),
        "invalid_package",
        "size outside bounds",
    );
    assert_error(
        &command(
            &studio,
            json!({"op":"package.build","args":{"spec":{"id":"Bad ID","discs":[target()],"requires":[],"overrides":[]},"files":[]}}),
        ),
        "invalid_package",
        "invalid",
    );

    let mut invalid_delta = sealed_bytes(
        &build(
            &studio,
            "delta-check",
            &[],
            vec![delta(1, b"source", b"result")],
        )["ok"]["bytes"],
    );
    let payload_start = manifest_payload_start(&invalid_delta);
    invalid_delta[payload_start] = 0;
    reseal(&mut invalid_delta);
    assert_error(
        &command(
            &studio,
            json!({"op":"package.inspect","args":{"bytes":invalid_delta}}),
        ),
        "invalid_package",
        "mod delta",
    );

    let mut invalid_result_hash = sealed_bytes(&bytes);
    let payload_start = manifest_payload_start(&invalid_result_hash);
    invalid_result_hash[payload_start] ^= 0xff;
    reseal(&mut invalid_result_hash);
    assert_error(
        &command(
            &studio,
            json!({"op":"package.inspect","args":{"bytes":invalid_result_hash}}),
        ),
        "invalid_package",
        "SHA-256 differs",
    );
}

#[test]
fn bundle_preserves_input_order_and_rejects_duplicate_ids_and_unsatisfied_dependencies() {
    let studio = Studio::new();
    let base = build(&studio, "base", &[], vec![authored(1, b"base bytes")]);
    let addon = build(
        &studio,
        "addon",
        &["base"],
        vec![authored(2, b"addon bytes")],
    );
    let base_bytes = base["ok"]["bytes"].clone();
    let addon_bytes = addon["ok"]["bytes"].clone();

    let bundled = command(
        &studio,
        json!({"op":"package.bundle","args":{"packages":[base_bytes,addon_bytes]}}),
    );
    let bundled_again = command(
        &studio,
        json!({"op":"package.bundle","args":{"packages":[base["ok"]["bytes"],addon["ok"]["bytes"]]}}),
    );
    assert_eq!(bundled["ok"]["bytes"], bundled_again["ok"]["bytes"]);
    assert_eq!(bundled["ok"]["identities"][0], base["ok"]["identity"]);
    assert_eq!(bundled["ok"]["identities"][1], addon["ok"]["identity"]);

    let duplicate = build(
        &studio,
        "base",
        &[],
        vec![authored(3, b"separate synthetic payload")],
    );
    assert_error(
        &command(
            &studio,
            json!({"op":"package.bundle","args":{"packages":[base["ok"]["bytes"],duplicate["ok"]["bytes"]]}}),
        ),
        "invalid_package",
        "duplicate mod id",
    );
    assert_error(
        &command(
            &studio,
            json!({"op":"package.bundle","args":{"packages":[addon["ok"]["bytes"],base["ok"]["bytes"]]}}),
        ),
        "invalid_package",
        "before it",
    );

    let missing = build(
        &studio,
        "orphan",
        &["absent"],
        vec![authored(4, b"orphan bytes")],
    );
    assert_error(
        &command(
            &studio,
            json!({"op":"package.bundle","args":{"packages":[missing["ok"]["bytes"]]}}),
        ),
        "invalid_package",
        "before it",
    );
}

#[cfg(feature = "cli")]
#[test]
fn native_cli_build_inspect_and_bundle_accept_synthetic_recipe_files() {
    use std::{
        fs,
        path::PathBuf,
        process::Command,
        time::{SystemTime, UNIX_EPOCH},
    };

    let unique = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let directory = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../../../tmp/studio/milestone-5")
        .join(format!("cli-package-test-{}-{unique}", std::process::id()));
    fs::create_dir_all(&directory).expect("create isolated CLI fixture directory");
    fs::write(directory.join("result.bin"), b"synthetic CLI result").unwrap();
    fs::write(
        directory.join("recipe.json"),
        serde_json::to_vec(&json!({
            "spec":{"id":"cli-synthetic","discs":[target()],"requires":[],"overrides":[]},
            "files":[{"disc":"SYNTH-1","slot":5,"source":null,"result":"result.bin"}]
        }))
        .unwrap(),
    )
    .unwrap();

    let executable = env!("CARGO_BIN_EXE_xem-package");
    let package = directory.join("first.xemmod");
    let output = Command::new(executable)
        .args([
            "build",
            directory.join("recipe.json").to_str().unwrap(),
            package.to_str().unwrap(),
        ])
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "CLI build failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let inspect = Command::new(executable)
        .args(["inspect", package.to_str().unwrap()])
        .output()
        .unwrap();
    assert!(
        inspect.status.success(),
        "CLI inspect failed: {}",
        String::from_utf8_lossy(&inspect.stderr)
    );
    let inspected: Value = serde_json::from_slice(&inspect.stdout).unwrap();
    assert_eq!(inspected["manifest"]["id"], "cli-synthetic");
    assert_eq!(inspected["manifest"]["files"][0]["kind"], "authored");

    let bundle = directory.join("bundle.xemmods");
    let output = Command::new(executable)
        .args([
            "bundle",
            bundle.to_str().unwrap(),
            package.to_str().unwrap(),
        ])
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "CLI bundle failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(bundle.metadata().unwrap().len() > package.metadata().unwrap().len());
}
