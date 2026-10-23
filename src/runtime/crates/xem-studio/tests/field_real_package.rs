//! Gated M6 package verification against both original raw tracks.
//! The package bytes are the exact set exported by the real Studio build UI.
use serde_json::{Value, json};
use std::env;
use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};
use xem_disc::mods::{Package, PackageSet};
use xem_disc::{Index, KnownDisc, SectorSource, identify, open_path, read_data, verify};

fn required(name: &str) -> Option<PathBuf> {
    env::var_os(name).map(PathBuf::from)
}

fn sha256(bytes: &[u8]) -> String {
    use sha2::{Digest, Sha256};
    Sha256::digest(bytes)
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

fn write_new(path: &Path, bytes: &[u8]) {
    let mut file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(path)
        .expect("private package evidence file is fresh");
    file.write_all(bytes)
        .expect("write private package evidence");
    file.sync_all().expect("flush private package evidence");
}

fn prepared(
    set: &PackageSet,
    identity: &xem_disc::Identification,
    source: &mut Box<dyn SectorSource>,
) -> std::sync::Arc<xem_disc::mods::PreparedOverlay> {
    let mut preparation = set
        .preparation(identity)
        .expect("package set matches original disc identity");
    for _ in 0..4096 {
        if preparation
            .advance(source.as_mut())
            .expect("prepare original source package overlay")
        {
            return preparation
                .overlay()
                .expect("completed preparation publishes overlay");
        }
    }
    panic!("package overlay preparation exceeds its bounded retry count");
}

fn expected_lbas(files: &[Value], serial: &str, original_sectors: u32) -> Vec<(u32, u32, u32)> {
    let mut files: Vec<_> = files.iter().filter(|file| file["disc"] == serial).collect();
    files.sort_by_key(|file| file["slot"].as_u64().expect("manifest slot is unsigned"));
    let mut next = original_sectors;
    files
        .into_iter()
        .map(|file| {
            let slot = u32::try_from(file["slot"].as_u64().expect("manifest slot is unsigned"))
                .expect("slot fits u32");
            let size = u32::try_from(
                file["result_size"]
                    .as_u64()
                    .expect("manifest result size is unsigned"),
            )
            .expect("result size fits u32");
            let lba = next;
            next = next
                .checked_add(size.div_ceil(2048).max(1))
                .expect("virtual LBA extent fits u32");
            (slot, lba, size)
        })
        .collect()
}

#[test]
fn real_ui_package_preserves_map22_bytes_and_disc2_passthrough() {
    let (Some(set_path), Some(disc1_path), Some(disc2_path), Some(fixture_root), Some(output_root)) = (
        required("XEM_STUDIO_M6_SET"),
        required("XEM_STUDIO_M6_DISC1"),
        required("XEM_STUDIO_M6_DISC2"),
        required("XEM_STUDIO_M6_FIXTURE_ROOT"),
        required("XEM_STUDIO_M6_PACKAGE_ARTIFACT_ROOT"),
    ) else {
        eprintln!(
            "skipping gated M6 real package verification: explicit private fixture inputs are not set"
        );
        return;
    };

    let root = Path::new(env!("CARGO_MANIFEST_DIR"))
        .ancestors()
        .nth(4)
        .expect("workspace root is present");
    let milestone_root = root.join("tmp/studio/milestone-6");
    for path in [&set_path, &fixture_root, &output_root] {
        assert!(
            path.starts_with(&milestone_root),
            "private package fixture/output stays under ignored milestone storage"
        );
    }
    assert!(
        output_root != fixture_root,
        "package verification writes to a fresh evidence leaf"
    );
    assert!(
        output_root.parent().is_some_and(Path::exists),
        "private output parent already exists"
    );
    assert!(
        !output_root.exists(),
        "package verification evidence leaf is fresh"
    );
    let evidence: Value = serde_json::from_slice(
        &fs::read(fixture_root.join("evidence.json")).expect("read private fixture evidence"),
    )
    .expect("private fixture evidence is JSON");
    let set_bytes = fs::read(&set_path).expect("read exact package set exported by Studio UI");
    let package_set_sha256 = sha256(&set_bytes);
    let set =
        PackageSet::from_bytes(&set_bytes).expect("strictly decode the Studio UI package set");
    let identities = set.identities();
    assert_eq!(
        identities.len(),
        1,
        "the accepted UI fixture contains one package"
    );
    let package_identity = identities[0].clone();
    let package_count = u32::from_le_bytes(set_bytes[8..12].try_into().expect("set package count"));
    assert_eq!(
        package_count, 1,
        "package set framing declares exactly one package"
    );
    let member_size =
        u32::from_le_bytes(set_bytes[12..16].try_into().expect("member size")) as usize;
    let package =
        Package::decode(&set_bytes[16..16 + member_size]).expect("decode package manifest");
    let manifest = package.manifest();
    let manifest_json = serde_json::to_value(manifest).expect("manifest serializes as JSON");
    let map23_slot = u32::try_from(evidence["slot"].as_u64().expect("map 23 slot is unsigned"))
        .expect("map 23 slot fits u32");
    let map22_slot = u32::try_from(
        evidence["untouched_map"]["slot"]
            .as_u64()
            .expect("map 22 slot is unsigned"),
    )
    .expect("map 22 slot fits u32");
    assert!(
        !manifest
            .files
            .iter()
            .any(|file| file.disc == KnownDisc::Disc2.serial()),
        "the UI package declares disc 2 identity without owning any disc 2 files"
    );
    let map23 = manifest
        .files
        .iter()
        .find(|file| file.disc == KnownDisc::Disc1.serial() && file.slot == map23_slot)
        .expect("UI package contains authored map 23 delta");
    let map22 = manifest
        .files
        .iter()
        .find(|file| file.disc == KnownDisc::Disc1.serial() && file.slot == map22_slot);
    assert_eq!(
        map23.result_sha256,
        evidence["result_bundle_sha256"]
            .as_str()
            .expect("result bundle hash"),
        "map 23 delta targets the verified service fixture result"
    );
    if let Some(map22) = map22 {
        assert_eq!(
            map22.source_sha256.as_deref(),
            Some(map22.result_sha256.as_str()),
            "any map 22 package payload is an exact source-equals-result delta"
        );
        assert_eq!(
            map22.result_sha256,
            evidence["untouched_map"]["bundle_sha256"]
                .as_str()
                .expect("untouched map hash"),
            "any map 22 result matches the service fixture's full source bundle"
        );
    }

    let mut disc1_original = open_path(&disc1_path).expect("open original disc 1 track");
    let id1 = identify(disc1_original.as_mut()).expect("identify original disc 1");
    assert_eq!(
        id1.disc,
        Some(KnownDisc::Disc1),
        "disc 1 raw track has known identity"
    );
    verify(disc1_original.as_mut(), KnownDisc::Disc1)
        .expect("verify complete original disc 1 track");
    let index1_original =
        Index::from_disc(disc1_original.as_mut()).expect("read original disc 1 index");
    let source22_record = index1_original
        .record(map22_slot as usize)
        .expect("original map 22 record");
    let source23_record = index1_original
        .record(map23_slot as usize)
        .expect("original map 23 record");
    assert!(
        source22_record.size > 0,
        "original map 22 index record is an ordinary file"
    );
    assert!(
        source23_record.size > 0,
        "original map 23 index record is an ordinary file"
    );
    let source22 = read_data(
        disc1_original.as_mut(),
        source22_record.lba,
        source22_record.size as usize,
    )
    .expect("read every original map 22 byte");
    let source23 = read_data(
        disc1_original.as_mut(),
        source23_record.lba,
        source23_record.size as usize,
    )
    .expect("read every original map 23 byte");
    if let Some(map22) = map22 {
        assert_eq!(
            sha256(&source22),
            map22.source_sha256.as_deref().expect("map 22 source hash"),
            "map 22 delta binds to every original indexed byte"
        );
    } else {
        assert_eq!(
            sha256(&source22),
            evidence["untouched_map"]["bundle_sha256"]
                .as_str()
                .expect("untouched map hash"),
            "the omitted map 22 remains the original full indexed file"
        );
    }
    assert_eq!(
        sha256(&source23),
        map23.source_sha256.as_deref().expect("map 23 source hash"),
        "map 23 delta binds to every original indexed byte"
    );
    assert_eq!(
        sha256(&source23),
        evidence["source_bundle_sha256"]
            .as_str()
            .expect("source bundle hash"),
        "map 23 delta source is the service fixture input"
    );

    let mut disc1_for_overlay =
        open_path(&disc1_path).expect("reopen original disc 1 for preparation");
    let overlay1 = prepared(&set, &id1, &mut disc1_for_overlay);
    let mut disc1_virtual = overlay1
        .wrap(disc1_for_overlay)
        .expect("wrap exact original disc 1 source");
    let index1_virtual =
        Index::from_disc(disc1_virtual.as_mut()).expect("read overlaid disc 1 index");
    let virtual22_record = index1_virtual
        .record(map22_slot as usize)
        .expect("overlaid map 22 record");
    let virtual23_record = index1_virtual
        .record(map23_slot as usize)
        .expect("overlaid map 23 record");
    let lbas = expected_lbas(
        manifest_json["files"]
            .as_array()
            .expect("manifest files array"),
        KnownDisc::Disc1.serial(),
        id1_sector_count(&disc1_path),
    );
    for (slot, lba, size) in &lbas {
        let record = index1_virtual
            .record(*slot as usize)
            .expect("manifest slot has virtual index record");
        assert_eq!(
            record.lba, *lba,
            "sorted manifest file receives its deterministic appended LBA"
        );
        assert_eq!(
            record.size, *size as i32,
            "virtual index record carries manifest result_size"
        );
    }
    if let Some(map22) = map22 {
        assert_eq!(
            virtual22_record.size, map22.result_size as i32,
            "map 22 record size matches its source-equals-result manifest"
        );
    } else {
        assert_eq!(
            virtual22_record, source22_record,
            "omitted map 22 retains its original index record"
        );
    }
    assert_eq!(
        virtual23_record.size, map23.result_size as i32,
        "map 23 record size matches the delta manifest"
    );
    let result22 = read_data(
        disc1_virtual.as_mut(),
        virtual22_record.lba,
        virtual22_record.size as usize,
    )
    .expect("read every overlaid map 22 byte");
    let result23 = read_data(
        disc1_virtual.as_mut(),
        virtual23_record.lba,
        virtual23_record.size as usize,
    )
    .expect("read every overlaid map 23 byte");
    assert!(
        result22 == source22,
        "the exact UI package preserves all map 22 bytes"
    );
    let expected_map22_hash =
        map22.map_or_else(|| sha256(&source22), |file| file.result_sha256.clone());
    assert_eq!(
        sha256(&result22),
        expected_map22_hash,
        "overlaid map 22 retains the exact manifest or original hash"
    );
    assert_eq!(
        sha256(&result23),
        map23.result_sha256,
        "overlaid map 23 matches its manifest result hash"
    );

    let mut disc2_original = open_path(&disc2_path).expect("open original disc 2 track");
    let id2 = identify(disc2_original.as_mut()).expect("identify original disc 2");
    assert_eq!(
        id2.disc,
        Some(KnownDisc::Disc2),
        "disc 2 raw track has known identity"
    );
    verify(disc2_original.as_mut(), KnownDisc::Disc2)
        .expect("verify complete original disc 2 track");
    let index2_original =
        Index::from_disc(disc2_original.as_mut()).expect("read original disc 2 index");
    let mut disc2_for_overlay =
        open_path(&disc2_path).expect("reopen original disc 2 for package preparation");
    let overlay2 = prepared(&set, &id2, &mut disc2_for_overlay);
    assert!(
        overlay2.executable() == id2.executable,
        "disc 2 boot executable remains byte-identical"
    );
    let mut disc2_virtual = overlay2
        .wrap(disc2_for_overlay)
        .expect("wrap exact original disc 2 source");
    let index2_virtual =
        Index::from_disc(disc2_virtual.as_mut()).expect("read overlaid disc 2 index");
    assert!(
        index2_virtual.records() == index2_original.records(),
        "disc 2 file-index records remain identical"
    );
    assert!(
        index2_virtual.directory_table() == index2_original.directory_table(),
        "disc 2 directory table remains identical"
    );
    for slot in [map22_slot, map23_slot] {
        let record = index2_original
            .record(slot as usize)
            .expect("disc 2 slot exists");
        let original = read_data(disc2_original.as_mut(), record.lba, record.size as usize)
            .expect("read original disc 2 file");
        let pass_through = read_data(disc2_virtual.as_mut(), record.lba, record.size as usize)
            .expect("read overlaid disc 2 file");
        assert!(
            pass_through == original,
            "disc 2 package pass-through preserves every indexed file byte"
        );
    }

    fs::create_dir(&output_root).expect("create fresh private package evidence leaf");
    let report = json!({
        "acceptance":"exact UI package set verified against original disc 1 and disc 2 raw tracks",
        "package_set_sha256":package_set_sha256,
        "package_identity":package_identity,
        "disc1":{"serial":KnownDisc::Disc1.serial(),"boot_sha256":evidence["disc1"]["boot_sha256"],"source_map22_sha256":sha256(&source22),"result_map22_sha256":sha256(&result22),"result_map23_sha256":sha256(&result23),"map22_original_record":{"lba":source22_record.lba,"size":source22_record.size},"map22_record":{"lba":virtual22_record.lba,"size":virtual22_record.size,"included_in_manifest":map22.is_some()},"map23_original_record":{"lba":source23_record.lba,"size":source23_record.size},"map23_record":{"lba":virtual23_record.lba,"size":virtual23_record.size},"sorted_manifest_lbas":lbas},
        "disc2":{"serial":KnownDisc::Disc2.serial(),"boot_sha256":evidence["disc2"]["boot_sha256"],"index_records_unchanged":true,"indexed_files_passthrough":true},
        "checks":{"allMap22BytesUnchanged":true,"map23ResultMatchesManifest":true,"map22IndexMatchesManifestOrOriginalWhenOmitted":true,"disc2IndexAndFilesUnchanged":true}
    });
    write_new(
        &output_root.join("package-preservation.json"),
        &serde_json::to_vec_pretty(&report).expect("serialize private evidence"),
    );
}

fn id1_sector_count(path: &Path) -> u32 {
    let bytes = fs::metadata(path).expect("disc 1 metadata").len();
    assert_eq!(bytes % 2352, 0, "disc 1 raw track has whole-sector extent");
    u32::try_from(bytes / 2352).expect("disc 1 sector count fits u32")
}
