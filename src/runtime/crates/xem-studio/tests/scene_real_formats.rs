//! Ignored local reader-oracle comparison for an explicitly supplied bundle.
//! The exercised M6-derived map-23 asset is not a gameplay/playback proof.

use std::{env, fs};
use xem_disc::field_context::hash;
use xem_studio::scene::{bundle, geometry};

fn first_difference(
    expected: &serde_json::Value,
    actual: &serde_json::Value,
    path: &str,
) -> Option<String> {
    match (expected, actual) {
        (serde_json::Value::Object(left), serde_json::Value::Object(right)) => {
            if left.len() != right.len() {
                return Some(path.to_owned());
            }
            for (key, value) in left {
                let next = format!("{path}.{key}");
                let Some(other) = right.get(key) else {
                    return Some(next);
                };
                if let Some(difference) = first_difference(value, other, &next) {
                    return Some(difference);
                }
            }
            None
        }
        (serde_json::Value::Array(left), serde_json::Value::Array(right)) => {
            if left.len() != right.len() {
                return Some(path.to_owned());
            }
            left.iter()
                .zip(right)
                .enumerate()
                .find_map(|(index, (left, right))| {
                    first_difference(left, right, &format!("{path}[{index}]"))
                })
        }
        _ if expected == actual => None,
        _ => Some(path.to_owned()),
    }
}

#[test]
#[ignore = "requires an explicitly supplied private bundle and Python reader oracle"]
fn real_map23_geometry_matches_python_reader_oracle() {
    let bundle_path = env::var_os("XEM_STUDIO_M7_REAL_BUNDLE")
        .map(std::path::PathBuf::from)
        .expect("set XEM_STUDIO_M7_REAL_BUNDLE to the existing local bundle");
    let oracle_path = env::var_os("XEM_STUDIO_M7_SCENE_ORACLE")
        .map(std::path::PathBuf::from)
        .expect("set XEM_STUDIO_M7_SCENE_ORACLE to fresh ignored oracle JSON");
    let original = fs::read(bundle_path).expect("read the supplied bundle in place");
    let oracle: serde_json::Value = serde_json::from_slice(
        &fs::read(oracle_path).expect("read the private Python oracle result"),
    )
    .expect("parse oracle JSON");

    assert_eq!(oracle["schema_version"], 1);
    let bundle_sha256 = hash(&original);
    assert_eq!(
        oracle["bundle_sha256"].as_str(),
        Some(bundle_sha256.as_str())
    );
    let source = bundle::Source::load(bundle::Input {
        bundle: original.clone(),
        context: None,
        source: None,
        module_sha256: None,
    })
    .expect("load all nine components using the production bundle reader");
    let component = &source.components[2];
    let component_report = serde_json::json!({
        "index": 2,
        "source_offset": component.source_offset,
        "logical_size": component.logical_size,
        "decoded_output_size": component.bytes.len(),
        "source_bytes_read": component.source_bytes_read,
        "decoded_sha256": hash(&component.bytes),
    });
    let actual_geometry = geometry::decode_geometry(&component.bytes, component.logical_size)
        .expect("decode unrelocated model groups through the Rust reader");
    let actual_geometry = serde_json::to_value(actual_geometry).expect("serialize Rust geometry");
    let actual_group_count = actual_geometry["groups"]
        .as_array()
        .expect("Rust geometry groups")
        .len();
    let actual_mesh_count: usize = actual_geometry["groups"]
        .as_array()
        .expect("Rust geometry groups")
        .iter()
        .map(|group| group["meshes"].as_array().expect("group meshes").len())
        .sum();
    let actual_group_count = serde_json::json!(actual_group_count);
    let actual_mesh_count = serde_json::json!(actual_mesh_count);

    let checks = [
        ("component", &oracle["component"], &component_report),
        ("geometry", &oracle["geometry"], &actual_geometry),
        ("group_count", &oracle["group_count"], &actual_group_count),
        ("mesh_count", &oracle["mesh_count"], &actual_mesh_count),
    ];
    for (path, expected, actual) in checks {
        if let Some(difference) = first_difference(expected, actual, path) {
            panic!("independent scene reader mismatch at {difference}");
        }
    }
}
