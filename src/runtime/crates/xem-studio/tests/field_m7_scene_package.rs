//! Opt-in preservation check for the exact M7 Scene UI package set.
//!
//! The package and original raw tracks stay private under ignored M7 storage;
//! the public result contains only identities, sizes, and hashes.
use serde_json::json;
use std::env;
use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};
use xem_disc::mods::{Package, PackageSet};
use xem_disc::{Index, KnownDisc, SectorSource, identify, open_path, read_data, verify};

const EXPECTED_SET_SHA256: &str =
    "d242f77413949e6dfa32db17f3815fc4adf35c7f0ede5f1d2e5d96b08e229489";

fn required(name: &str) -> PathBuf {
    env::var_os(name).map(PathBuf::from).unwrap_or_else(|| {
        panic!("{name} is required for this explicitly gated private-input check")
    })
}

fn fresh_leaf() -> PathBuf {
    let output = required("XEM_STUDIO_M7_PACKAGE_ARTIFACT_ROOT");
    let root = Path::new(env!("CARGO_MANIFEST_DIR"))
        .ancestors()
        .nth(4)
        .expect("workspace root is present");
    assert!(output.starts_with(root.join("tmp/studio/milestone-7")));
    assert!(
        !output.exists(),
        "private M7 package evidence leaf must be fresh"
    );
    assert!(output.parent().is_some_and(Path::exists));
    output
}

fn write_new(path: &Path, bytes: &[u8]) {
    let mut file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(path)
        .expect("private M7 evidence file is fresh");
    file.write_all(bytes).expect("write private M7 evidence");
    file.sync_all().expect("flush private M7 evidence");
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|byte| format!("{byte:02x}")).collect()
}

fn prepare(
    packages: &PackageSet,
    identity: &xem_disc::Identification,
    source: &mut Box<dyn SectorSource>,
) -> std::sync::Arc<xem_disc::mods::PreparedOverlay> {
    let mut preparation = packages
        .preparation(identity)
        .expect("package family matches source disc");
    for _ in 0..4096 {
        if preparation
            .advance(source.as_mut())
            .expect("prepare package overlay")
        {
            return preparation
                .overlay()
                .expect("completed overlay is available");
        }
    }
    panic!("package overlay preparation exceeded its bounded retry count");
}

#[test]
#[ignore = "requires explicit original-disc and M7 UI package paths"]
fn exact_m7_scene_package_preserves_map22_and_disc2_files() {
    let set_path = required("XEM_STUDIO_M7_SET");
    let disc1_path = required("XEM_STUDIO_M7_DISC1");
    let disc2_path = required("XEM_STUDIO_M7_DISC2");
    let output_root = fresh_leaf();
    let root = Path::new(env!("CARGO_MANIFEST_DIR"))
        .ancestors()
        .nth(4)
        .expect("workspace root is present");
    for input in [&set_path, &disc1_path, &disc2_path] {
        assert!(
            input.starts_with(root.join("tmp/studio/milestone-7"))
                || input.starts_with("/home/bart/xenogears_toolkit/.local/discs"),
            "private M7 set and original raw discs stay in approved local paths"
        );
    }
    let package_bytes = fs::read(&set_path).expect("read exact set exported from M7 UI");
    let set_sha256 = xem_disc::field_context::hash(&package_bytes);
    assert_eq!(
        set_sha256, EXPECTED_SET_SHA256,
        "verification consumes the exact checkpoint86 UI package set"
    );
    let packages = PackageSet::from_bytes(&package_bytes).expect("strictly decode package set");
    let identities = packages.identities();
    assert_eq!(identities.len(), 1, "Stage B exports one ordered package");
    let package_identity = identities[0].clone();
    assert_eq!(package_bytes.get(..8), Some(b"XEMSET1\0".as_slice()));
    assert_eq!(
        u32::from_le_bytes(package_bytes[8..12].try_into().expect("package count")),
        1
    );
    let member_size =
        u32::from_le_bytes(package_bytes[12..16].try_into().expect("member size")) as usize;
    assert_eq!(package_bytes.len(), 16 + member_size);
    let package = Package::decode(&package_bytes[16..]).expect("decode sole package member");
    let manifest = package.manifest();
    assert_eq!(identities[0], package.identity());
    assert_eq!(
        manifest.files.len(),
        1,
        "exact edited package contains one map delta"
    );
    let delta = &manifest.files[0];
    assert_eq!(delta.disc, KnownDisc::Disc1.serial());
    assert_eq!(
        delta.slot, 652,
        "M7 edited map 23 occupies the verified index slot"
    );
    assert!(
        !manifest
            .files
            .iter()
            .any(|file| file.disc == KnownDisc::Disc2.serial())
    );

    let mut source1 = open_path(&disc1_path).expect("open original disc 1 raw track");
    let identity1 = identify(source1.as_mut()).expect("identify original disc 1");
    assert_eq!(identity1.disc, Some(KnownDisc::Disc1));
    verify(source1.as_mut(), KnownDisc::Disc1).expect("verify original disc 1");
    let index1 = Index::from_disc(source1.as_mut()).expect("read original disc 1 index");
    let map22_source_record = index1.record(650).expect("original map 22 record");
    let map23_source_record = index1.record(652).expect("original map 23 record");
    let map22_source_hash = hex(
        &xem_disc::file_sha256(source1.as_mut(), map22_source_record)
            .expect("hash every original map 22 byte"),
    );
    let map23_source_hash = hex(
        &xem_disc::file_sha256(source1.as_mut(), map23_source_record)
            .expect("hash every original map 23 byte"),
    );
    assert_eq!(
        map23_source_hash,
        delta.source_sha256.as_deref().expect("delta source hash")
    );
    let map22_source = read_data(
        source1.as_mut(),
        map22_source_record.lba,
        map22_source_record.size as usize,
    )
    .expect("read full original map 22 file");

    let overlay1 = prepare(&packages, &identity1, &mut source1);
    let mut virtual1 = overlay1.wrap(source1).expect("wrap original disc 1");
    let index1_virtual = Index::from_disc(virtual1.as_mut()).expect("read virtual disc 1 index");
    let map22_virtual_record = index1_virtual.record(650).expect("virtual map 22 record");
    let map23_virtual_record = index1_virtual.record(652).expect("virtual map 23 record");
    assert_eq!(
        map22_virtual_record, map22_source_record,
        "map 22 index record is unchanged"
    );
    assert_eq!(map23_virtual_record.size, delta.result_size as i32);
    assert_ne!(
        map23_virtual_record.lba, map23_source_record.lba,
        "map 23 uses appended virtual storage"
    );
    let map22_virtual_hash = hex(
        &xem_disc::file_sha256(virtual1.as_mut(), map22_virtual_record)
            .expect("hash every virtual map 22 byte"),
    );
    assert_eq!(
        map22_virtual_hash, map22_source_hash,
        "all map 22 bytes are identical"
    );
    let map22_virtual = read_data(
        virtual1.as_mut(),
        map22_virtual_record.lba,
        map22_virtual_record.size as usize,
    )
    .expect("read full virtual map 22 file");
    assert!(
        map22_virtual == map22_source,
        "complete map 22 file passthrough; source hash {map22_source_hash}"
    );
    let map23_virtual_hash = hex(
        &xem_disc::file_sha256(virtual1.as_mut(), map23_virtual_record)
            .expect("hash every virtual map 23 byte"),
    );
    assert_eq!(
        map23_virtual_hash, delta.result_sha256,
        "virtual map 23 matches package output hash"
    );

    let mut source2 = open_path(&disc2_path).expect("open original disc 2 raw track");
    let identity2 = identify(source2.as_mut()).expect("identify original disc 2");
    assert_eq!(identity2.disc, Some(KnownDisc::Disc2));
    verify(source2.as_mut(), KnownDisc::Disc2).expect("verify original disc 2");
    let index2 = Index::from_disc(source2.as_mut()).expect("read original disc 2 index");
    let disc2_source_records = [650usize, 652].map(|slot| {
        let record = index2
            .record(slot)
            .expect("disc 2 corresponding map record");
        let hash = hex(&xem_disc::file_sha256(source2.as_mut(), record)
            .expect("hash every original disc 2 map byte"));
        (slot, record, hash)
    });
    let overlay2 = prepare(&packages, &identity2, &mut source2);
    assert!(
        overlay2.executable() == identity2.executable,
        "disc 2 executable is untouched"
    );
    let mut virtual2 = overlay2.wrap(source2).expect("wrap original disc 2");
    let index2_virtual = Index::from_disc(virtual2.as_mut()).expect("read virtual disc 2 index");
    assert!(
        index2.records() == index2_virtual.records(),
        "all disc 2 index records are unchanged"
    );
    assert!(
        index2.directory_table() == index2_virtual.directory_table(),
        "disc 2 directory table is unchanged (source entries {}, virtual entries {})",
        index2.directory_table().len(),
        index2_virtual.directory_table().len()
    );
    let disc2_records = disc2_source_records.map(|(slot, record, original)| {
        let virtual_record = index2_virtual
            .record(slot)
            .expect("virtual disc 2 map record");
        let actual = hex(&xem_disc::file_sha256(virtual2.as_mut(), virtual_record)
            .expect("hash full virtual disc 2 map"));
        assert_eq!(actual, original, "disc 2 indexed map bytes pass through");
        json!({"slot":slot,"original_sha256":original,"virtual_sha256":actual,"size":record.size})
    });

    fs::create_dir(&output_root).expect("create fresh M7 evidence directory");
    let report = json!({
        "package_set_sha256":set_sha256,
        "package_identity":package_identity,
        "disc1":{"serial":KnownDisc::Disc1.serial(),"map22":{"slot":650,"source_sha256":map22_source_hash,"virtual_sha256":map22_virtual_hash,"full_bytes_equal":true,"index_record_equal":true},"map23":{"slot":652,"source_sha256":map23_source_hash,"virtual_sha256":map23_virtual_hash,"source_lba":map23_source_record.lba,"virtual_lba":map23_virtual_record.lba,"result_size":map23_virtual_record.size}},
        "disc2":{"serial":KnownDisc::Disc2.serial(),"index_records_equal":true,"maps":disc2_records},
        "checks":{"package_has_one_disc1_map23_delta":true,"map22_full_file_unchanged":true,"disc2_index_and_maps_unchanged":true}
    });
    write_new(
        &output_root.join("package-preservation.json"),
        &serde_json::to_vec_pretty(&report).expect("serialize private evidence"),
    );
}
