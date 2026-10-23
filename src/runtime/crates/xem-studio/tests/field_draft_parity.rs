//! Optional native reply trace for replay through the browser's wasm protocol.
//! Every input byte is invented by this test; the fixture is written only when
//! the execution owner supplies a fresh ignored-output path.
use serde_json::{Value, json};
use xem_disc::{
    KnownDisc,
    field_context::{ComponentContext, SourceContext, SourceIdentity, hash},
};
use xem_studio::{Studio, disc, packed};

const BYTECODE_OFFSET: usize = 132 + 64;
const MODULE_HASH: &str = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";

fn component() -> Vec<u8> {
    let code = [0x03, 0, 0, 1, 0x00]; // message_box followed by a terminal-only slot
    let mut bytes = vec![0_u8; BYTECODE_OFFSET + code.len()];
    bytes[128..132].copy_from_slice(&1_u32.to_le_bytes());
    bytes[132 + 2..132 + 4].copy_from_slice(&4_u16.to_le_bytes());
    bytes[BYTECODE_OFFSET..].copy_from_slice(&code);
    bytes
}

fn literal_block(bytes: &[u8]) -> Vec<u8> {
    let mut block = (bytes.len() as u32).to_le_bytes().to_vec();
    block.push(0);
    block.extend_from_slice(bytes);
    block.push(0);
    block
}

fn bundle(bytes: &[u8]) -> Vec<u8> {
    let mut result = vec![0_u8; 0x154];
    for index in 0..9 {
        let (logical_size, block) = if index == 5 {
            (
                bytes.len(),
                packed::encode_literals(bytes, 1 << 20).expect("bounded event component encoding"),
            )
        } else {
            let data = [index as u8 + 1; 8];
            (data.len(), literal_block(&data))
        };
        let offset = result.len();
        result[0x10c + index * 4..0x110 + index * 4]
            .copy_from_slice(&(logical_size as u32).to_le_bytes());
        result[0x130 + index * 4..0x134 + index * 4]
            .copy_from_slice(&(offset as u32).to_le_bytes());
        result.extend_from_slice(&block);
    }
    result
}

fn measured_context(bundle: &[u8], module: &str) -> (SourceContext, SourceIdentity) {
    let disc = KnownDisc::Disc1;
    let identity = SourceIdentity {
        disc: 1,
        boot_sha256: xem_disc::identify::hex(&disc.executable_sha256()),
        map: 0,
        slot: 652,
        lba: 0x1234,
        indexed_size: bundle.len() as u32,
        source_sha256: hash(bundle),
    };
    let components = (0..9)
        .map(|index| {
            let logical_size = u32::from_le_bytes(
                bundle[0x10c + index * 4..0x110 + index * 4]
                    .try_into()
                    .unwrap(),
            ) as usize;
            let source_offset = u32::from_le_bytes(
                bundle[0x130 + index * 4..0x134 + index * 4]
                    .try_into()
                    .unwrap(),
            ) as usize;
            let decoded = packed::decode_block(&bundle[source_offset..], logical_size + 16)
                .expect("all nine synthetic streams decode");
            ComponentContext {
                index,
                source_offset,
                logical_size,
                packed_size: u32::from_le_bytes(
                    bundle[source_offset..source_offset + 4].try_into().unwrap(),
                ) as usize,
                source_bytes_read: decoded.source_bytes_read,
                output_size: decoded.data.len(),
                output_sha256: hash(&decoded.data),
                destination_address: xem_disc::field_context::RAM_START
                    + 0x10_000
                    + index as u32 * 0x1000,
                extra: Vec::new(),
            }
        })
        .collect();
    let context = SourceContext {
        schema_version: 1,
        provenance: "runtime_field_decompress".into(),
        module_sha256: module.into(),
        identity: identity.clone(),
        source_address: xem_disc::field_context::RAM_START + 0x1000,
        allocation_size: bundle.len().div_ceil(4) as u32 * 4,
        load_vblank: "733".into(),
        components,
    };
    (context, identity)
}

fn record(studio: &Studio, trace: &mut Vec<Value>, op: &str, args: Value) -> Value {
    let request = json!({"op":op,"args":args});
    let response: Value = serde_json::from_str(&studio.command(&request.to_string()))
        .expect("Studio command response is JSON");
    trace.push(json!({"request":request,"response":response}));
    response
}

fn record_rejection(
    studio: &Studio,
    trace: &mut Vec<Value>,
    op: &str,
    args: Value,
    semantic: bool,
) -> Value {
    let request = json!({"op":op,"args":args});
    let response: Value = serde_json::from_str(&studio.command(&request.to_string()))
        .expect("Studio command response is JSON");
    assert!(
        response.get("error").is_some(),
        "malformed context must be rejected: {response}"
    );
    if semantic {
        assert_eq!(
            response["error"]["kind"], "invalid_source_context",
            "representable extreme context reaches semantic range validation"
        );
    }
    trace.push(json!({"request":request,"response":response,"rejection_only":true,"semantic_rejection":semantic}));
    trace.last().unwrap()["response"].clone()
}

#[test]
fn writes_synthetic_native_draft_trace_for_wasm_replay_when_requested() {
    let Ok(path) = std::env::var("XEM_STUDIO_WASM_DRAFT_PARITY_FIXTURE") else {
        eprintln!("native/WASM draft parity fixture skipped: env path is unset");
        return;
    };
    let repository = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .ancestors()
        .nth(4)
        .expect("crate is inside the repository")
        .to_path_buf();
    let path = std::path::PathBuf::from(path);
    assert!(
        path.starts_with(repository.join("tmp/studio")),
        "synthetic draft parity fixture must remain under ignored tmp/studio"
    );

    let raw_component = component();
    let bundle = bundle(&raw_component);
    let source = disc::extract_events(&bundle)
        .expect("synthetic bundle header is valid")
        .expect("synthetic bundle contains an event component");
    let bytes = source.bytes;
    let studio = Studio::new();
    let mut trace = Vec::new();
    let declared_size = source.declared_size;
    record(&studio, &mut trace, "version", Value::Null);
    record(
        &studio,
        &mut trace,
        "echo",
        json!({"error":{"kind":"ordinary-data","message":"not an envelope"},"ok":null}),
    );
    let decoded = record(
        &studio,
        &mut trace,
        "field.decode",
        json!({"bytes":bytes,"declared_size":declared_size}),
    );
    assert!(
        decoded.get("ok").is_some(),
        "synthetic field decode succeeds: {decoded}"
    );
    let graph = decoded["ok"].clone();
    let opened = record(
        &studio,
        &mut trace,
        "field.draft.open",
        json!({"graph":graph}),
    );
    assert!(
        opened.get("ok").is_some(),
        "synthetic draft opens: {opened}"
    );
    let draft = opened["ok"]["draft"].clone();
    for (revision, action) in (0_u32..).zip([
        json!({"kind":"insert_after","after":"pc:0","bytes":[14],"targets":{}}),
        json!({"kind":"new_event","actor":0,"event":1,"nodes":[{"bytes":[14],"targets":{}},{"bytes":[13],"targets":{}}]}),
        json!({"kind":"insert_after","after":"new:2","bytes":[15],"targets":{}}),
    ]) {
        let response = record(
            &studio,
            &mut trace,
            "field.draft.edit",
            json!({"draft":draft,"revision":revision,"action":action}),
        );
        assert!(
            response.get("ok").is_some(),
            "synthetic action succeeds: {response}"
        );
    }
    let compiled = record(
        &studio,
        &mut trace,
        "field.draft.compile",
        json!({"draft":draft}),
    );
    assert!(
        compiled.get("ok").is_some(),
        "edited draft compiles: {compiled}"
    );
    let exported = record(
        &studio,
        &mut trace,
        "field.draft.export",
        json!({"draft":draft}),
    );
    assert!(
        exported.get("ok").is_some(),
        "edited draft exports: {exported}"
    );
    record(
        &studio,
        &mut trace,
        "field.draft.import",
        json!({"document":exported["ok"]}),
    );
    let imported = trace
        .last()
        .and_then(|entry| entry["response"]["ok"]["draft"].as_u64())
        .expect("import returns a new draft handle");
    let imported_compiled = record(
        &studio,
        &mut trace,
        "field.draft.compile",
        json!({"draft":imported}),
    );
    assert!(
        imported_compiled.get("ok").is_some(),
        "imported draft compiles: {imported_compiled}"
    );
    let repacked = record(
        &studio,
        &mut trace,
        "field.draft.repack",
        json!({"draft":imported,"bundle":bundle}),
    );
    assert!(
        repacked.get("ok").is_some(),
        "edited draft repacks: {repacked}"
    );
    let module = MODULE_HASH.to_owned();
    let (context, source_identity) = measured_context(&bundle, &module);
    assert!(
        context.guard(&bundle, &source_identity, &module).is_ok(),
        "synthetic measurement fixture is valid"
    );
    let measured_repack = record(
        &studio,
        &mut trace,
        "field.draft.repack",
        json!({"draft":imported,"bundle":bundle,"context":context,"source":source_identity,"module_sha256":module}),
    );
    assert!(
        measured_repack.get("ok").is_some(),
        "valid measured repack succeeds: {measured_repack}"
    );
    for field in ["output_size", "source_bytes_read"] {
        let mut values = vec![(json!(u32::MAX), true)];
        if usize::MAX != u32::MAX as usize {
            values.push((json!(usize::MAX), false));
        }
        for (value, semantic) in values {
            let mut malformed =
                serde_json::to_value(&context).expect("serialize synthetic context");
            malformed["components"][5][field] = value;
            record_rejection(
                &studio,
                &mut trace,
                "field.draft.repack",
                json!({"draft":imported,"bundle":bundle,"context":malformed,"source":source_identity,"module_sha256":module}),
                semantic,
            );
        }
    }
    let stale = record(
        &studio,
        &mut trace,
        "field.draft.edit",
        json!({
            "draft":draft,
            "revision":0,
            "action":{"kind":"insert_after","after":"pc:0","bytes":[14],"targets":{}}
        }),
    );
    assert_eq!(stale["error"]["kind"], "stale_revision");

    let fixture = json!({"synthetic_only":true,"trace":trace});
    std::fs::create_dir_all(path.parent().expect("fixture path has a parent"))
        .expect("create ignored synthetic fixture directory");
    let encoded = serde_json::to_vec(&fixture).expect("serialize synthetic protocol trace");
    let mut file = std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&path)
        .expect("fixture output must be a fresh file; preserve previous evidence");
    std::io::Write::write_all(&mut file, &encoded).expect("write synthetic protocol fixture");
    file.sync_all().expect("finish synthetic protocol fixture");
}
