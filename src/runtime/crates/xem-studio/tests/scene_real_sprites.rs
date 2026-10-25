//! Opt-in, hash-only inventory of component-3 sprite resources on both raw discs.
//! This reads original map bundles without opening them as editable scenes and
//! never writes source data or emits frame pixels.
use serde_json::{Value, json};
use std::collections::{BTreeMap, BTreeSet};
use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};
use xem_disc::field_context::{MAX_COMPONENT, hash};
use xem_disc::{Index, KnownDisc, open_path, packed, read_data};
use xem_studio::scene::{formats, sprites};

const FIRST_MAP_FILE: u32 = 0xB8;

fn required(name: &str) -> PathBuf {
    PathBuf::from(std::env::var_os(name).unwrap_or_else(|| panic!("{name} is required")))
}

fn fresh_output() -> PathBuf {
    let path = required("XEM_STUDIO_SPRITE_INVENTORY_OUTPUT");
    let repo = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .ancestors()
        .nth(4)
        .expect("crate is inside repository")
        .to_path_buf();
    assert!(path.starts_with(repo.join("tmp/studio/scene-sprites-20261024")));
    assert!(!path.exists(), "fresh output keeps previous evidence");
    fs::create_dir_all(path.parent().expect("output parent exists"))
        .expect("create ignored output parent");
    fs::create_dir(&path).expect("create fresh ignored inventory leaf");
    path
}

fn record_error(counts: &mut BTreeMap<String, usize>, kind: &str) {
    *counts.entry(kind.to_owned()).or_default() += 1;
}

fn audit_disc(number: u8, path: &Path, selected_maps: Option<&BTreeSet<u32>>) -> Value {
    let mut track = open_path(path).expect("open supplied raw disc read-only");
    let index = Index::from_disc(track.as_mut()).expect("read raw disc file index");
    assert_eq!(index.disc_number(), Some(u16::from(number)));
    let known = match number {
        1 => KnownDisc::Disc1,
        2 => KnownDisc::Disc2,
        _ => panic!("sprite inventory accepts only original discs 1 and 2"),
    };
    let first = index.slot(4, FIRST_MAP_FILE).expect("map directory entry");
    let marker = index.record(first - 1).expect("preceding map count entry");
    let file_count = marker
        .directory_len()
        .expect("map count is a negative record");
    assert_eq!(file_count % 2, 0, "map files are bundle/companion pairs");
    let maps = file_count / 2;

    let mut placeholders = 0usize;
    let mut short_headers = 0usize;
    let mut header_errors = BTreeMap::new();
    let mut packed_errors = BTreeMap::new();
    let mut sprite_errors = BTreeMap::new();
    let mut maps_with_resources = 0usize;
    let mut resource_count = 0usize;
    let mut qualified_resources = 0usize;
    let mut unsupported_resources = 0usize;
    let mut map_rows = Vec::with_capacity(maps as usize);

    for map in 0..maps {
        if selected_maps.is_some_and(|selected| !selected.contains(&map)) {
            continue;
        }
        let file = FIRST_MAP_FILE + map * 2;
        let slot = index.slot(4, file).expect("indexed bundle slot");
        let record = index.record(slot).expect("indexed bundle record");
        // The original map directory uses short indexed records as explicit
        // placeholders; some have nonempty companions, so size<=0 is not the
        // placeholder criterion.
        if record.is_unused() || record.size < 0x154 {
            placeholders += 1;
            map_rows.push(json!({"map":map,"status":"placeholder"}));
            continue;
        }
        let size = usize::try_from(record.size).expect("positive bundle size");
        if size > 32 << 20 {
            record_error(&mut header_errors, "bundle_exceeds_32_mib");
            map_rows.push(json!({"map":map,"status":"bundle_exceeds_32_mib"}));
            continue;
        }
        let bundle = match read_data(track.as_mut(), record.lba, size) {
            Ok(bytes) if bytes.len() == size => bytes,
            Ok(_) => {
                record_error(&mut header_errors, "indexed_length_mismatch");
                map_rows.push(json!({"map":map,"status":"indexed_length_mismatch"}));
                continue;
            }
            Err(_) => {
                record_error(&mut header_errors, "disc_read_error");
                map_rows.push(json!({"map":map,"status":"disc_read_error"}));
                continue;
            }
        };
        if bundle.len() < 0x190 {
            short_headers += 1;
            map_rows.push(json!({"map":map,"status":"short_header","bundle_sha256":hash(&bundle)}));
            continue;
        }
        let header = match formats::decode_header(&bundle) {
            Ok(header) => header,
            Err(error) => {
                record_error(&mut header_errors, error.kind);
                map_rows.push(
                    json!({"map":map,"status":"invalid_header","bundle_sha256":hash(&bundle)}),
                );
                continue;
            }
        };
        let offset = header.offsets[3];
        let logical = header.logical_sizes[3];
        let packed = match packed::decode_block(&bundle[offset..], MAX_COMPONENT) {
            Ok(block) => block,
            Err(error) => {
                // PackedError::Debug for a truncation contains partial output;
                // serialize only its stable category and aggregate count.
                let kind = match error {
                    packed::PackedError::MissingLength => "missing_length",
                    packed::PackedError::RequiredOutput { .. } => "required_output",
                    packed::PackedError::OutputLimit { .. } => "output_limit",
                    packed::PackedError::Truncated { .. } => "truncated",
                    packed::PackedError::InvalidDistance { .. } => "invalid_distance",
                    packed::PackedError::CopyCrossesOutput { .. } => "copy_crosses_output",
                    packed::PackedError::FlagGroupCrossesOutput { .. } => {
                        "flag_group_crosses_output"
                    }
                };
                record_error(&mut packed_errors, kind);
                map_rows.push(json!({"map":map,"status":"packed_error","kind":kind,"bundle_sha256":hash(&bundle)}));
                continue;
            }
        };
        let assets = match sprites::decode(&packed.data, logical, &header.slots) {
            Ok(assets) => assets,
            Err(error) => {
                record_error(&mut sprite_errors, error.kind);
                map_rows.push(json!({"map":map,"status":"sprite_error","kind":error.kind,"message":error.message,"component_sha256":hash(&packed.data)}));
                continue;
            }
        };
        let resources = assets.resources.len();
        let qualified = assets
            .resources
            .iter()
            .filter(|resource| resource.first_frame.is_some())
            .count();
        let unsupported = resources - qualified;
        if resources > 0 {
            maps_with_resources += 1;
        }
        resource_count += resources;
        qualified_resources += qualified;
        unsupported_resources += unsupported;
        let rows: Vec<_> = assets
            .resources
            .iter()
            .map(|resource| {
                json!({
                    "slot":resource.slot,
                    "source_span":resource.source_span,
                    "palette_count":resource.palette_count,
                    "palette_banks":resource.palette_banks.len(),
                    "first_frame":resource.first_frame.as_ref().map(|frame| json!({
                        "frame":frame.frame,"animation":frame.animation,"facing":frame.facing,
                        "flip_x":frame.flip_x,"flip_y":frame.flip_y,"parts":frame.parts.len(),
                        "uploads":frame.uploads.len(),"clear":frame.clear.is_some()
                    })),
                    "diagnostic":resource.diagnostic
                })
            })
            .collect();
        map_rows.push(json!({
            "map":map,"status":"decoded","bundle_sha256":hash(&bundle),
            "component_sha256":hash(&packed.data),"component_logical_size":logical,
            "component_output_size":packed.data.len(),"component_bytes_read":packed.source_bytes_read,
            "resource_count":resources,"qualified_count":qualified,"unsupported_count":unsupported,
            "resources":rows
        }));
    }
    json!({
        "disc":number,"boot_sha256":hex(&known.executable_sha256()),"map_count":maps,
        "scanned_map_count":map_rows.len(),
        "placeholders":placeholders,"short_headers":short_headers,
        "header_errors":header_errors,"packed_errors":packed_errors,"sprite_errors":sprite_errors,
        "maps_with_resources":maps_with_resources,"resources":resource_count,
        "qualified_resources":qualified_resources,"unsupported_resources":unsupported_resources,
        "maps":map_rows
    })
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|byte| format!("{byte:02x}")).collect()
}

#[test]
#[ignore = "set explicit raw disc paths and fresh output leaf for full both-disc sprite inventory"]
fn original_disc_sprite_first_frame_inventory() {
    let disc1 = required("XEM_STUDIO_SPRITE_DISC1");
    let disc2 = required("XEM_STUDIO_SPRITE_DISC2");
    assert_eq!(disc1.file_name(), Some(std::ffi::OsStr::new("disc1.bin")));
    assert_eq!(disc2.file_name(), Some(std::ffi::OsStr::new("disc2.bin")));
    let output = fresh_output();
    let result = json!({
        "disc1":audit_disc(1,&disc1,None),
        "disc2":audit_disc(2,&disc2,None)
    });
    let encoded = serde_json::to_vec_pretty(&result).expect("metadata summary serializes");
    let mut file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(output.join("sprite-inventory.json"))
        .expect("create summary once");
    file.write_all(&encoded)
        .expect("write hash/numeric summary");
    file.sync_all().expect("flush summary");
    println!("both-disc sprite inventory written; source payloads and pixels were not emitted");
}

#[test]
#[ignore = "set explicit original Disc 1 path and fresh output leaf to inspect known decoder diagnostics"]
fn original_disc_sprite_selected_map_diagnostics() {
    let disc1 = required("XEM_STUDIO_SPRITE_DISC1");
    assert_eq!(disc1.file_name(), Some(std::ffi::OsStr::new("disc1.bin")));
    let selected = BTreeSet::from([180, 250, 260]);
    let output = fresh_output();
    let result = audit_disc(1, &disc1, Some(&selected));
    let rows = result["maps"].as_array().expect("selected map rows");
    assert_eq!(rows.len(), selected.len());
    assert!(rows.iter().all(|row| row["status"] == "sprite_error"));
    let encoded = serde_json::to_vec_pretty(&result).expect("diagnostic summary serializes");
    let mut file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(output.join("sprite-diagnostics.json"))
        .expect("create diagnostics once");
    file.write_all(&encoded)
        .expect("write numeric/hash diagnostics");
    file.sync_all().expect("flush diagnostics");
    println!(
        "selected Disc 1 sprite diagnostics written; source payloads and pixels were not emitted"
    );
}
