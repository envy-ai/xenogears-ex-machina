#[path = "support/synthetic_disc.rs"]
mod synthetic_disc;

use serde_json::Value;
#[cfg(feature = "test-fixtures")]
use serde_json::json;
#[cfg(feature = "test-fixtures")]
use std::io::Write;
#[cfg(feature = "test-fixtures")]
use std::time::{SystemTime, UNIX_EPOCH};
#[cfg(feature = "test-fixtures")]
use xem_studio::Studio;
use xem_studio::disc::extract_events;
use xem_studio::disc::{DiscSessions, OpenArgs};

fn pull_fixture(sessions: &mut DiscSessions, image: &[u8]) -> (u32, Value) {
    let opened = sessions
        .open(OpenArgs::Pull {
            name: "invented-fixture.bin".into(),
            bytes: image.len() as u64,
        })
        .expect("pull session opens pending");
    let session = opened["session"].as_u64().expect("session id") as u32;
    sessions
        .fill(session, 0, image)
        .expect("fixture fills its one requested range");
    (session, opened)
}

#[cfg(feature = "test-fixtures")]
fn command(studio: &Studio, op: &str, args: Value) -> Value {
    let input = json!({"op": op, "args": args}).to_string();
    serde_json::from_str(&studio.command(&input)).expect("Studio returns JSON")
}

#[cfg(feature = "test-fixtures")]
fn write_fixture(image: &[u8]) -> std::path::PathBuf {
    let repository = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .ancestors()
        .nth(4)
        .expect("crate is inside the repository")
        .to_path_buf();
    let parent = repository.join("tmp/studio/milestone-3");
    std::fs::create_dir_all(&parent).expect("create ignored fixture parent");
    let nonce = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system clock after Unix epoch")
        .as_nanos();
    let directory = parent.join(format!("native-fixture-{}-{nonce}", std::process::id()));
    std::fs::create_dir(&directory).expect("create a fresh ignored fixture directory");
    let path = directory.join("invented-fixture.bin");
    let mut file = std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&path)
        .expect("create a new invented ISO fixture");
    file.write_all(image).expect("write synthetic sectors");
    file.sync_all().expect("finish synthetic fixture");
    path
}

#[test]
fn extracts_the_complete_component_and_keeps_declared_tail_boundary() {
    let fixture = synthetic_disc::build();
    let events = extract_events(&fixture.bundle)
        .expect("synthetic bundle has a valid event component")
        .expect("bundle is not a placeholder");

    assert_eq!(events.bytes, fixture.component);
    assert_eq!(events.declared_size, synthetic_disc::COMPONENT_SIZE);
    assert_eq!(events.bytecode_offset, synthetic_disc::BYTECODE_OFFSET);
    assert_eq!(events.component_end, 1);
    assert_eq!(events.actor_count, 1);
}

#[test]
fn a_short_bundle_is_reported_as_a_placeholder() {
    assert!(
        extract_events(&[0; 4])
            .expect("short map files are valid placeholders")
            .is_none()
    );
}

#[test]
fn map_registry_uses_map_bundle_companion_pairs_and_keeps_placeholder_files() {
    let fixture = synthetic_disc::build();
    let index = xem_disc::game_index(&fixture.executable).expect("synthetic executable index");
    let maps = xem_studio::disc::map_registry(&index).expect("valid synthetic map registry");

    assert_eq!(maps.len(), 2);
    assert_eq!((maps[0].map, maps[0].file, maps[0].slot), (0, 0xB8, 0xB8));
    assert_eq!((maps[0].record.lba, maps[0].record.size), (70, 570));
    assert_eq!((maps[1].map, maps[1].file, maps[1].slot), (1, 0xBA, 0xBA));
    assert_eq!((maps[1].record.lba, maps[1].record.size), (71, 4));
}

#[cfg(feature = "test-fixtures")]
#[test]
fn pull_session_identifies_fixture_lists_maps_and_extracts_components() {
    let fixture = synthetic_disc::build();
    let mut sessions = DiscSessions::new();
    let (session, opened) = pull_fixture(&mut sessions, &fixture.image);

    assert_eq!(opened["state"], "pending");
    assert_eq!(opened["missing"], json!([[0, fixture.image.len()]]));
    let ready = sessions
        .poll(session)
        .expect("filled image advances to ready");
    assert_eq!(ready["state"], "ready");
    assert_eq!(ready["phase"], "ready");
    assert_eq!(ready["identity"]["disc"], Value::Null);
    assert_eq!(ready["identity"]["test_fixture"], true);
    assert_eq!(
        ready["identity"]["boot_sha256"],
        "dfe782b25d3e17220c592d2646d05ac78b3d42dd110d291a46c1f3d87c13ba6d"
    );

    let maps = sessions.maps(session).expect("both indexed maps load");
    assert_eq!(maps["state"], "ready");
    assert_eq!(maps["maps"].as_array().map(Vec::len), Some(2));
    assert_eq!(maps["maps"][0]["map"], 0);
    assert_eq!(maps["maps"][0]["file"], 0xB8);
    assert_eq!(maps["maps"][0]["placeholder"], false);
    assert_eq!(maps["maps"][0]["actor_count"], 1);
    assert_eq!(maps["maps"][0]["script_size"], 1);
    assert_eq!(maps["maps"][0]["stream_size"], 200);
    assert_eq!(maps["maps"][1]["placeholder"], true);
    assert_eq!(maps["maps"][1]["actor_count"], Value::Null);

    let events = sessions.map_events(session, 0).expect("map 0 events load");
    assert_eq!(events["state"], "ready");
    assert_eq!(events["placeholder"], false);
    assert_eq!(events["component"]["bytes"], json!(fixture.component));
    assert_eq!(events["component"]["declared_size"], 197);
    assert_eq!(events["component"]["component_end"], 1);

    let placeholder = sessions
        .map_events(session, 1)
        .expect("placeholder map remains addressable");
    assert_eq!(placeholder["placeholder"], true);
    assert_eq!(placeholder["component"], Value::Null);
    assert_eq!(
        sessions.close(session).expect("close"),
        json!({"session": session, "closed": true})
    );
    assert_eq!(sessions.poll(session).unwrap_err().kind, "unknown_session");
}

#[cfg(not(feature = "test-fixtures"))]
#[test]
fn default_build_rejects_the_invented_executable_identity() {
    let fixture = synthetic_disc::build();
    let mut sessions = DiscSessions::new();
    let (session, _) = pull_fixture(&mut sessions, &fixture.image);

    let error = sessions
        .poll(session)
        .expect_err("default builds accept only known discs");
    assert_eq!(error.kind, "unknown_disc");
    assert!(error.message.contains("SLUS_999.99;1"));
}

#[cfg(feature = "test-fixtures")]
#[test]
fn studio_protocol_opens_binary_pull_ranges_and_returns_map_components() {
    let fixture = synthetic_disc::build();
    let studio = Studio::new();
    let opened = command(
        &studio,
        "disc.open",
        json!({
            "kind": "pull",
            "name": "invented-fixture.bin",
            "bytes": fixture.image.len()
        }),
    );
    let session = opened["ok"]["session"]
        .as_u64()
        .expect("open returns a session") as u32;
    assert_eq!(opened["ok"]["state"], "pending");
    assert_eq!(opened["ok"]["missing"], json!([[0, fixture.image.len()]]));

    let filled: Value = serde_json::from_str(&studio.disc_fill(session, 0.0, &fixture.image))
        .expect("binary fill returns JSON");
    assert_eq!(filled["ok"]["filled"], fixture.image.len());
    let bad_offset: Value = serde_json::from_str(&studio.disc_fill(session, f64::NAN, &[]))
        .expect("invalid binary fill returns JSON");
    assert_eq!(bad_offset["error"]["kind"], "invalid_args");

    let ready = command(&studio, "disc.poll", json!({"session": session}));
    assert_eq!(ready["ok"]["identity"]["test_fixture"], true);
    let maps = command(&studio, "disc.maps", json!({"session": session}));
    assert_eq!(maps["ok"]["maps"].as_array().map(Vec::len), Some(2));
    assert_eq!(maps["ok"]["maps"][1]["placeholder"], true);
    let events = command(
        &studio,
        "field.map_events",
        json!({"session": session, "map": 0}),
    );
    assert_eq!(events["ok"]["component"]["bytes"], json!(fixture.component));
    let placeholder = command(
        &studio,
        "field.map_events",
        json!({"session": session, "map": 1}),
    );
    assert_eq!(placeholder["ok"]["placeholder"], true);
    assert_eq!(placeholder["ok"]["component"], Value::Null);

    let fixture_path = write_fixture(&fixture.image);
    let native = command(
        &studio,
        "disc.open",
        json!({"kind": "native", "path": fixture_path}),
    );
    assert_eq!(native["ok"]["state"], "ready");
    assert_eq!(native["ok"]["identity"]["test_fixture"], true);
    let native_session = native["ok"]["session"].as_u64().unwrap() as u32;
    let native_maps = command(&studio, "disc.maps", json!({"session": native_session}));
    assert_eq!(native_maps["ok"]["maps"], maps["ok"]["maps"]);
    let native_events = command(
        &studio,
        "field.map_events",
        json!({"session": native_session, "map": 0}),
    );
    assert_eq!(native_events["ok"]["component"], events["ok"]["component"]);
    let component = &native_events["ok"]["component"];
    let graph = command(
        &studio,
        "field.decode",
        json!({
            "bytes": component["bytes"],
            "declared_size": component["declared_size"]
        }),
    );
    assert!(graph.get("ok").is_some());
    let structure = command(
        &studio,
        "field.structure",
        json!({"graph": graph["ok"], "actor": 0, "event": 0}),
    );
    let lanes = command(&studio, "field.lanes", json!({"graph": graph["ok"]}));
    let names_text = r#"{"schema_version":1,"variables":{"0":"HP"}}"#;
    let names = command(&studio, "names.validate", json!({"json": names_text}));

    if let Ok(expected_path) = std::env::var("XEM_STUDIO_WASM_DISC_PARITY_FIXTURE") {
        let repository = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .ancestors()
            .nth(4)
            .expect("crate is inside the repository")
            .to_path_buf();
        let expected_path = std::path::PathBuf::from(expected_path);
        assert!(
            expected_path.starts_with(repository.join("tmp/studio")),
            "synthetic parity result must remain under ignored tmp/studio"
        );
        std::fs::create_dir_all(expected_path.parent().expect("fixture has a parent"))
            .expect("create ignored parity fixture parent");
        let mut file = std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&expected_path)
            .expect("create a fresh native parity result");
        serde_json::to_writer(
            &mut file,
            &json!({
                "image_path": fixture_path.display().to_string(),
                "native_status": native["ok"],
                "maps": native_maps["ok"],
                "component": native_events["ok"]["component"],
                "placeholder_component": placeholder["component"],
                "graph": graph["ok"],
                "structure": structure["ok"],
                "lanes": lanes["ok"],
                "names": names["ok"],
            }),
        )
        .expect("write synthetic native parity result");
        file.sync_all()
            .expect("finish synthetic native parity result");
    }
}
