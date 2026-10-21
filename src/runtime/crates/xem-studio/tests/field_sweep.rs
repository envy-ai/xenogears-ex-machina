use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::{
    collections::BTreeMap,
    env, fs,
    fs::OpenOptions,
    io::{BufRead, BufReader, Read},
    path::PathBuf,
    process::{Command, Stdio},
};
use xem_studio::Studio;

fn sha256(bytes: &[u8]) -> String {
    Sha256::digest(bytes)
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

fn value_sha256(value: &Value) -> String {
    sha256(&serde_json::to_vec(value).expect("JSON value serializes"))
}

fn instruction_semantics(instruction: &Value) -> Value {
    let mut semantics = serde_json::Map::new();
    for key in [
        "pc",
        "opcode",
        "extended",
        "size",
        "spec",
        "operands",
        "immediate",
        "successors",
        "skips",
        "inner",
    ] {
        semantics.insert(
            key.to_owned(),
            instruction
                .get(key)
                .expect("decoded instruction has every semantic field")
                .clone(),
        );
    }
    Value::Object(semantics)
}

fn header_semantics(graph: &Value) -> Value {
    let mut header = serde_json::Map::new();
    for key in [
        "variable_unsigned_bits",
        "actors",
        "entries",
        "skipped_zero_entries",
        "stream_size",
        "bytecode_offset",
        "declared_size",
    ] {
        header.insert(
            key.to_owned(),
            graph
                .get(key)
                .expect("decoded graph has every header field")
                .clone(),
        );
    }
    Value::Object(header)
}

fn error_kind(response: &Value) -> Option<&str> {
    response.get("error")?.get("kind")?.as_str()
}

#[test]
fn matches_python_walk_for_both_discs_when_private_extracts_are_available() {
    let Ok(extract_root) = env::var("XEM_STUDIO_EXTRACT_ROOT") else {
        eprintln!("field sweep cross-check skipped: XEM_STUDIO_EXTRACT_ROOT is unset");
        return;
    };

    let repository = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .ancestors()
        .nth(4)
        .expect("crate is inside the repository")
        .to_path_buf();
    let run_id = std::process::id();
    let run_directory = repository
        .join("tmp/studio/milestone-2-20261020")
        .join(format!("oracle-run-{run_id}"));
    fs::create_dir_all(run_directory.parent().expect("oracle run has parent"))
        .expect("create ignored parent for the local oracle output");
    fs::create_dir(&run_directory)
        .expect("fresh ignored oracle run directory must not overwrite prior private inputs");
    let output_directory = run_directory.join("components");
    let stderr_path = run_directory.join("python.stderr.log");
    let stderr = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&stderr_path)
        .expect("preserve a fresh oracle stderr log without overwriting prior evidence");

    let oracle = repository.join("tools/studio/tests/field_oracle.py");
    let python = Command::new("python3")
        .arg(&oracle)
        .current_dir(&repository)
        .env("PYTHONPATH", &repository)
        .env("XEM_STUDIO_EXTRACT_ROOT", &extract_root)
        .env("XEM_STUDIO_ORACLE_DIR", &output_directory)
        .stdout(Stdio::piped())
        .stderr(Stdio::from(stderr))
        .spawn()
        .expect("launch pinned Python field oracle");
    let mut child = python;
    let stdout = child.stdout.take().expect("oracle stdout pipe is present");
    let mut reader = BufReader::new(stdout);
    let studio = Studio::new();
    let mut maps = [0_usize; 2];
    let mut placeholders = [0_usize; 2];
    let mut instructions_total = 0_usize;
    let mut line = String::new();

    loop {
        line.clear();
        let read = reader
            .read_line(&mut line)
            .expect("read one metadata-only oracle summary");
        if read == 0 {
            break;
        }
        let record: Value =
            serde_json::from_str(&line).expect("oracle emits one JSON summary per map");
        let disc = record["disc"]
            .as_u64()
            .expect("oracle summary includes disc") as usize;
        let map = record["map"].as_u64().expect("oracle summary includes map") as usize;
        assert!(
            (1..=2).contains(&disc),
            "oracle reported an invalid disc number"
        );
        if record["placeholder"] == true {
            placeholders[disc - 1] += 1;
            continue;
        }
        maps[disc - 1] += 1;

        let private_path = PathBuf::from(record["path"].as_str().expect("oracle summary path"));
        assert!(
            private_path.starts_with(&output_directory),
            "oracle component path escaped its fresh ignored directory"
        );
        let component = fs::read(&private_path).expect("read local private component input");
        assert_eq!(
            component.len(),
            record["component_size"].as_u64().unwrap() as usize
        );
        let component_hash = sha256(&component);
        if component_hash != record["source_sha256"] {
            panic!("disc {disc} map {map}: private component SHA-256 differs from oracle summary");
        }
        let declared_size = record["declared_size"].as_u64().unwrap() as usize;
        assert!(declared_size <= component.len());

        let request = json!({
            "op": "field.decode",
            "args": { "bytes": component, "declared_size": declared_size }
        });
        let response: Value = serde_json::from_str(&studio.command(&request.to_string()))
            .expect("field decoder returns JSON");
        if let Some(kind) = error_kind(&response) {
            panic!("disc {disc} map {map}: Rust decode returned {kind}");
        }
        let graph = response
            .get("ok")
            .expect("successful field decode has ok response");

        if graph["source_sha256"] != record["source_sha256"] {
            panic!(
                "disc {disc} map {map}: source hash differs (python {}, Rust {})",
                record["source_sha256"].as_str().unwrap_or("missing"),
                graph["source_sha256"].as_str().unwrap_or("missing")
            );
        }
        if value_sha256(&header_semantics(graph)) != record["header_sha256"] {
            panic!("disc {disc} map {map}: actor/entry/header semantics differ");
        }

        let expected: BTreeMap<usize, String> = record["instruction_hashes"]
            .as_array()
            .expect("oracle summary instruction digests")
            .iter()
            .map(|item| {
                (
                    item["pc"].as_u64().unwrap() as usize,
                    item["sha256"].as_str().unwrap().to_owned(),
                )
            })
            .collect();
        let actual = graph["instructions"]
            .as_array()
            .expect("Rust graph instruction list");
        if actual.len() != expected.len() {
            panic!(
                "disc {disc} map {map}: instruction count differs (python {}, Rust {})",
                expected.len(),
                actual.len()
            );
        }
        instructions_total += actual.len();
        for instruction in actual {
            let pc = instruction["pc"].as_u64().unwrap() as usize;
            let expected_hash = expected.get(&pc).unwrap_or_else(|| {
                panic!("disc {disc} map {map} PC {pc}: Python walk omitted Rust instruction")
            });
            let actual_hash = value_sha256(&instruction_semantics(instruction));
            if &actual_hash != expected_hash {
                panic!(
                    "disc {disc} map {map} PC {pc}: semantic digest differs (python {expected_hash}, Rust {actual_hash})"
                );
            }
        }

        let diagnostics = graph.get("diagnostics").expect("Rust diagnostics object");
        let diagnostics_hash = value_sha256(diagnostics);
        if diagnostics_hash != record["diagnostics_sha256"] {
            panic!("disc {disc} map {map}: diagnostic digest differs");
        }

        let encoded_request = json!({ "op": "field.encode", "args": { "graph": graph } });
        let encoded_response: Value =
            serde_json::from_str(&studio.command(&encoded_request.to_string()))
                .expect("field encoder returns JSON");
        if let Some(kind) = error_kind(&encoded_response) {
            panic!("disc {disc} map {map}: Rust encode returned {kind}");
        }
        let encoded = encoded_response["ok"]
            .as_array()
            .expect("field.encode returns a JSON byte array")
            .iter()
            .map(|byte| byte.as_u64().expect("encoded byte is an integer") as u8)
            .collect::<Vec<_>>();
        if encoded != fs::read(&private_path).expect("re-read private source component") {
            panic!(
                "disc {disc} map {map}: full-stream round trip differs (expected {}, actual {})",
                record["source_sha256"].as_str().unwrap_or("missing"),
                sha256(&encoded)
            );
        }
        if sha256(&encoded[..declared_size]) != record["declared_sha256"] {
            panic!("disc {disc} map {map}: declared-component round trip differs");
        }
    }

    let status = child.wait().expect("wait for Python field oracle");
    if !status.success() {
        let mut stderr = String::new();
        fs::File::open(&stderr_path)
            .expect("read preserved oracle stderr")
            .read_to_string(&mut stderr)
            .expect("oracle stderr is UTF-8");
        panic!(
            "Python field oracle exited with {status}: {}",
            stderr.trim()
        );
    }
    assert_eq!(maps, [730, 205], "cross-check covers each bundled map");
    assert_eq!(
        placeholders,
        [0, 525],
        "placeholder map slots are accounted for"
    );
    println!(
        "Python/Rust field cross-check passed: disc1 maps={} placeholders={}; disc2 maps={} placeholders={}; instructions={}",
        maps[0], placeholders[0], maps[1], placeholders[1], instructions_total
    );
}
