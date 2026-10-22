//! Public-contract tests for deterministic packages and indexed file overlays.
//!
//! The disc image here is wholly synthetic and contains no game data. It has
//! the same index copies the game reads: in the boot EXE, at LBA 24, and the
//! directory table at LBA 40.

use sha2::{Digest, Sha256};
use std::cell::RefCell;
use std::rc::Rc;
use xem_disc::identify::{Identification, identify};
use xem_disc::index::{DIRECTORY_BYTES, DIRECTORY_LBA, INDEX_BYTES, INDEX_LBA, Index, game_index};
use xem_disc::mods::{BuildFile, BuildSpec, DiscTarget, Package, PackageSet};
use xem_disc::read_at::PrefetchedFile;
use xem_disc::sector::{FORM1_DATA_SIZE, SECTOR_SIZE, Subheader, build_mode2_sector, submode};
use xem_disc::source::BinSource;
use xem_disc::{Error, MemorySource, SectorSource, read_data};

const EXE_LBA: u32 = 50;
const TARGET_LBA: u32 = 70;
const UNTOUCHED_LBA: u32 = 72;
const TARGET_SLOT: u32 = 1;
const UNTOUCHED_SLOT: usize = 2;
const ORIGINAL_SIZE: usize = 2_700;
const RESULT_SIZE: usize = 5_193;
const LARGE_SOURCE_SIZE: usize = 32 << 20;

fn dir_record(name: &[u8], lba: u32, size: u32, directory: bool) -> Vec<u8> {
    let length = 33 + name.len() + (name.len() + 1) % 2;
    let mut record = vec![0; length];
    record[0] = length as u8;
    record[2..6].copy_from_slice(&lba.to_le_bytes());
    record[6..10].copy_from_slice(&lba.to_be_bytes());
    record[10..14].copy_from_slice(&size.to_le_bytes());
    record[14..18].copy_from_slice(&size.to_be_bytes());
    record[25] = if directory { 2 } else { 0 };
    record[32] = name.len() as u8;
    record[33..33 + name.len()].copy_from_slice(name);
    record
}

fn write_index_record(index: &mut [u8], slot: usize, lba: u32, size: i32) {
    let at = slot * 7;
    let lba = lba.to_le_bytes();
    index[at..at + 3].copy_from_slice(&lba[..3]);
    index[at + 3..at + 7].copy_from_slice(&size.to_le_bytes());
}

fn write_record_to_exe(exe: &mut [u8], slot: usize, lba: u32, size: i32) {
    write_index_record(&mut exe[0x804..0x804 + INDEX_BYTES], slot, lba, size);
}

fn pattern(len: usize, salt: u8) -> Vec<u8> {
    (0..len)
        .map(|i| (i.wrapping_mul(37).wrapping_add(11) as u8) ^ salt)
        .collect()
}

fn reference_edc(data: &[u8]) -> u32 {
    let mut table = [0u32; 256];
    for (index, item) in table.iter_mut().enumerate() {
        let mut value = index as u32;
        for _ in 0..8 {
            value = (value >> 1) ^ if value & 1 == 0 { 0 } else { 0xD801_8001 };
        }
        *item = value;
    }
    data.iter().fold(0, |edc, byte| {
        (edc >> 8) ^ table[((edc ^ u32::from(*byte)) & 0xFF) as usize]
    })
}

fn reference_ecc_plane(
    data: &[u8],
    major: usize,
    minor: usize,
    major_step: usize,
    minor_step: usize,
) -> Vec<u8> {
    let mut forward = [0u8; 256];
    let mut inverse = [0u8; 256];
    for (value, item) in forward.iter_mut().enumerate() {
        let doubled = (value as u16) << 1;
        *item = (doubled ^ if doubled & 0x100 == 0 { 0 } else { 0x11D }) as u8;
    }
    for value in 0..256 {
        inverse[(value as u8 ^ forward[value]) as usize] = value as u8;
    }
    let size = major * minor;
    let mut parity = vec![0u8; major * 2];
    for row in 0..major {
        let mut at = (row / 2) * major_step + (row & 1);
        let (mut a, mut b) = (0u8, 0u8);
        for _ in 0..minor {
            let byte = data[at];
            a ^= byte;
            b ^= byte;
            a = forward[a as usize];
            at = (at + minor_step) % size;
        }
        a = inverse[(forward[a as usize] ^ b) as usize];
        parity[row] = a;
        parity[row + major] = a ^ b;
    }
    parity
}

fn assert_regenerated_form1_sector(raw: &[u8; SECTOR_SIZE]) {
    assert_eq!(
        u32::from_le_bytes(raw[2072..2076].try_into().unwrap()),
        reference_edc(&raw[16..2072]),
        "Form1 EDC"
    );
    let mut p_data = raw[12..2076].to_vec();
    p_data[..4].fill(0);
    let p = reference_ecc_plane(&p_data, 86, 24, 2, 86);
    assert_eq!(&raw[2076..2248], p, "Form1 P parity");
    let mut q_data = raw[12..2248].to_vec();
    q_data[..4].fill(0);
    let q = reference_ecc_plane(&q_data, 52, 43, 86, 88);
    assert_eq!(&raw[2248..2352], q, "Form1 Q parity");
}

fn put_file(user: &mut [[u8; FORM1_DATA_SIZE]], lba: usize, bytes: &[u8]) {
    for (sector, chunk) in bytes.chunks(FORM1_DATA_SIZE).enumerate() {
        user[lba + sector][..chunk.len()].copy_from_slice(chunk);
    }
}

fn synthetic_disc() -> (MemorySource, Vec<u8>, Vec<u8>, Vec<u8>) {
    let original = pattern(ORIGINAL_SIZE, 0x31);
    let untouched = pattern(1_003, 0xA7);
    let mut exe = vec![0; 0x9000];
    exe[..8].copy_from_slice(b"PS-X EXE");
    exe[0x18..0x1C].copy_from_slice(&0x8001_0000u32.to_le_bytes());
    // Directory slot 0, followed by the two file records at slots 1 and 2.
    write_record_to_exe(&mut exe, 0, 41, -3);
    write_record_to_exe(
        &mut exe,
        TARGET_SLOT as usize,
        TARGET_LBA,
        ORIGINAL_SIZE as i32,
    );
    write_record_to_exe(
        &mut exe,
        UNTOUCHED_SLOT,
        UNTOUCHED_LBA,
        untouched.len() as i32,
    );
    let table_at = 0x8804;
    exe[table_at..table_at + 2].copy_from_slice(&2u16.to_le_bytes());
    exe[table_at + 2 * 0x3C..table_at + 2 * 0x3C + 2].copy_from_slice(&1u16.to_le_bytes());

    let mut user = vec![[0; FORM1_DATA_SIZE]; 80];
    let mut root = Vec::new();
    root.extend(dir_record(&[0], 18, FORM1_DATA_SIZE as u32, true));
    root.extend(dir_record(&[1], 18, FORM1_DATA_SIZE as u32, true));
    root.extend(dir_record(
        b"SLUS_999.99;1",
        EXE_LBA,
        exe.len() as u32,
        false,
    ));
    let cnf = b"BOOT = cdrom:\\SLUS_999.99;1\r\nTCB = 4\r\n";
    root.extend(dir_record(b"SYSTEM.CNF;1", 19, cnf.len() as u32, false));
    user[16][..6].copy_from_slice(b"\x01CD001");
    user[16][156..190].copy_from_slice(&dir_record(&[0], 18, 2048, true));
    user[18][..root.len()].copy_from_slice(&root);
    user[19][..cnf.len()].copy_from_slice(cnf);
    for (n, chunk) in exe.chunks(FORM1_DATA_SIZE).enumerate() {
        user[EXE_LBA as usize + n][..chunk.len()].copy_from_slice(chunk);
    }

    // The game also loads the index and directory table from these fixed
    // sectors, rather than relying only on the copy embedded in the EXE.
    for (n, chunk) in exe[0x804..0x804 + INDEX_BYTES]
        .chunks_exact(FORM1_DATA_SIZE)
        .enumerate()
    {
        user[INDEX_LBA as usize + n].copy_from_slice(chunk);
    }
    user[DIRECTORY_LBA as usize][..DIRECTORY_BYTES]
        .copy_from_slice(&exe[0x8804..0x8804 + DIRECTORY_BYTES]);
    put_file(&mut user, TARGET_LBA as usize, &original);
    put_file(&mut user, UNTOUCHED_LBA as usize, &untouched);

    let subheader = Subheader {
        file: 0,
        channel: 0,
        submode: submode::DATA,
        coding: 0,
    };
    let source = MemorySource::from_sectors(
        user.iter()
            .enumerate()
            .map(|(lba, data)| build_mode2_sector(lba as u32, subheader, data)),
    );
    (source, exe, original, untouched)
}

fn install_data_sector(raw_track: &mut [u8], lba: u32, bytes: &[u8]) {
    let subheader = Subheader {
        file: 0,
        channel: 0,
        submode: submode::DATA,
        coding: 0,
    };
    let sector = build_mode2_sector(lba, subheader, bytes);
    let start = lba as usize * SECTOR_SIZE;
    raw_track[start..start + SECTOR_SIZE].copy_from_slice(&sector);
}

fn index_mirrors(executable: &[u8], raw_track: &mut [u8]) {
    for (n, bytes) in executable[0x804..0x804 + INDEX_BYTES]
        .chunks_exact(FORM1_DATA_SIZE)
        .enumerate()
    {
        install_data_sector(raw_track, INDEX_LBA + n as u32, bytes);
    }
    for (n, bytes) in executable.chunks(FORM1_DATA_SIZE).enumerate() {
        install_data_sector(raw_track, EXE_LBA + n as u32, bytes);
    }
}

type SparseSource = BinSource<Rc<RefCell<PrefetchedFile>>>;

fn sparse_large_target_source(target_count: usize) -> (SparseSource, Identification, Vec<u32>) {
    let (mut base, _, _, _) = synthetic_disc();
    let mut identification = identified(&mut base);
    let mut executable = identification.executable.clone();
    let mut raw_track = base.into_inner();
    let mut target_lbas = Vec::new();
    for slot in 1..=target_count {
        let lba = 1_000 + (slot as u32 - 1) * 16_400;
        target_lbas.push(lba);
        write_record_to_exe(&mut executable, slot, lba, LARGE_SOURCE_SIZE as i32);
    }
    identification.executable = executable;
    identification.sha256 = Sha256::digest(&identification.executable).into();
    index_mirrors(&identification.executable, &mut raw_track);

    let file_sectors = target_lbas[0] + 64;
    let source_sectors = (target_lbas.last().copied().unwrap() + 16_384) as usize;
    let source_len = source_sectors as u64 * SECTOR_SIZE as u64;
    let mut prefetched = PrefetchedFile::new(source_len, SECTOR_SIZE, 1 << 20);
    // Identification and overlay preparation read only these original ranges.
    for lba in [16, 18, 19]
        .into_iter()
        .chain(INDEX_LBA..INDEX_LBA + 16)
        .chain([DIRECTORY_LBA])
        .chain(EXE_LBA..EXE_LBA + identification.executable.len().div_ceil(FORM1_DATA_SIZE) as u32)
    {
        let at = lba as usize * SECTOR_SIZE;
        prefetched
            .fill(
                lba as u64 * SECTOR_SIZE as u64,
                &raw_track[at..at + SECTOR_SIZE],
            )
            .unwrap();
    }
    for lba in target_lbas[0]..file_sectors {
        let data = pattern(FORM1_DATA_SIZE, lba as u8);
        let raw = build_mode2_sector(
            lba,
            Subheader {
                file: 0,
                channel: 0,
                submode: submode::DATA,
                coding: 0,
            },
            &data,
        );
        prefetched
            .fill(lba as u64 * SECTOR_SIZE as u64, &raw)
            .unwrap();
    }
    let source = BinSource::whole(Rc::new(RefCell::new(prefetched))).unwrap();
    (source, identification, target_lbas)
}

fn identified(source: &mut dyn SectorSource) -> Identification {
    identify(source).expect("synthetic ISO and PS-X EXE identify")
}

fn package_bytes(id: &str, source: Option<Vec<u8>>, result: &[u8], target: &DiscTarget) -> Vec<u8> {
    Package::build(
        BuildSpec {
            id: id.into(),
            discs: vec![target.clone()],
            requires: vec![],
            overrides: vec![],
        },
        vec![BuildFile {
            disc: target.serial.clone(),
            slot: TARGET_SLOT,
            source,
            result: result.to_vec(),
        }],
    )
    .expect("build a package from the source and result bytes")
}

#[test]
fn size_changing_package_updates_disc_and_boot_executable_index_views() {
    let (mut source, original_exe, original, untouched) = synthetic_disc();
    let identification = identified(&mut source);
    let target = DiscTarget::from_identification(&identification);
    let result = pattern(RESULT_SIZE, 0xE2);
    let package_bytes = package_bytes("test-size-change", Some(original.clone()), &result, &target);
    let package = Package::decode(&package_bytes).expect("package round-trips");
    assert_eq!(
        Package::decode(&package_bytes).unwrap().manifest(),
        package.manifest()
    );

    let set = PackageSet::new(vec![package]).expect("single valid package set");
    assert_eq!(set.identities().len(), 1);
    let bundled = set.to_bytes();
    let restored_set = PackageSet::from_bytes(&bundled).expect("ordered package set round-trips");
    assert_eq!(restored_set.identities(), set.identities());
    assert_eq!(restored_set.to_bytes(), bundled);
    let mut preparation = set
        .preparation(&identification)
        .expect("declared disc matches");
    while !preparation
        .advance(&mut source)
        .expect("preparation reads synthetic sectors")
    {}
    let overlay = preparation.overlay().expect("prepared immutable overlay");

    let embedded = game_index(overlay.executable()).expect("patched boot EXE remains valid");
    let changed = embedded.file(0, 1).expect("target file remains indexed");
    assert_eq!(changed.size, RESULT_SIZE as i32);
    assert_ne!(
        changed.lba, TARGET_LBA,
        "size changes use an appended virtual extent"
    );
    assert_eq!(embedded.file(0, 2).unwrap().lba, UNTOUCHED_LBA);
    assert_eq!(embedded.file(0, 2).unwrap().size, untouched.len() as i32);
    assert_ne!(overlay.executable(), original_exe.as_slice());

    let mut virtual_disc = overlay
        .wrap(Box::new(source))
        .expect("overlay wraps the actual sector source");
    let disc_index =
        Index::from_disc(virtual_disc.as_mut()).expect("game's LBA24/LBA40 index view");
    assert_eq!(disc_index.records(), embedded.records());
    assert_eq!(
        disc_index.directory_table(),
        &embedded.directory_table()[..disc_index.directory_table().len()]
    );
    let disk_boot = read_data(virtual_disc.as_mut(), EXE_LBA, overlay.executable().len())
        .expect("boot EXE's indexed sector view");
    let disk_boot_index = game_index(&disk_boot).expect("boot copy is patched too");
    assert_eq!(disk_boot_index.records(), embedded.records());
    assert_eq!(
        read_data(virtual_disc.as_mut(), changed.lba, changed.size as usize).unwrap(),
        result
    );
    assert_eq!(
        read_data(virtual_disc.as_mut(), UNTOUCHED_LBA, untouched.len()).unwrap(),
        untouched
    );
    for (sector_index, expected) in result.chunks(FORM1_DATA_SIZE).enumerate() {
        let lba = changed.lba + sector_index as u32;
        let mut raw = [0; SECTOR_SIZE];
        virtual_disc.read_sector(lba, &mut raw).unwrap();
        assert_eq!(xem_disc::sector::Sector(&raw).header().lba(), Some(lba));
        assert_eq!(&raw[24..24 + expected.len()], expected);
        assert!(
            raw[24 + expected.len()..2072].iter().all(|byte| *byte == 0),
            "final sector padding is zero"
        );
        assert_regenerated_form1_sector(&raw);
    }

    // Ordinary sectors remain raw passthrough, including headers/subheaders.
    let mut wrapped_sector = [0; SECTOR_SIZE];
    let mut original_source = synthetic_disc().0;
    for lba in [0, 16, 18, 19, 23, 41, 68, 69, 72, 79] {
        virtual_disc.read_sector(lba, &mut wrapped_sector).unwrap();
        let mut original_sector = [0; SECTOR_SIZE];
        original_source
            .read_sector(lba, &mut original_sector)
            .unwrap();
        assert_eq!(
            wrapped_sector, original_sector,
            "untouched raw sector {lba}"
        );
    }
}

#[test]
fn empty_package_set_preserves_vanilla_index_and_file_bytes() {
    let (mut source, original_exe, original, untouched) = synthetic_disc();
    let identification = identified(&mut source);
    let set = PackageSet::new(vec![]).expect("empty means vanilla");
    assert!(set.identities().is_empty());
    let mut preparation = set.preparation(&identification).unwrap();
    while !preparation.advance(&mut source).unwrap() {}
    let overlay = preparation.overlay().expect("ready empty-set view");
    assert_eq!(overlay.executable(), original_exe);
    let mut virtual_disc = overlay.wrap(Box::new(source)).unwrap();
    assert_eq!(
        read_data(virtual_disc.as_mut(), TARGET_LBA, original.len()).unwrap(),
        original
    );
    assert_eq!(
        read_data(virtual_disc.as_mut(), UNTOUCHED_LBA, untouched.len()).unwrap(),
        untouched
    );
    let mut vanilla_source = synthetic_disc().0;
    let mut observed = [0; SECTOR_SIZE];
    let mut expected = [0; SECTOR_SIZE];
    for lba in 0..vanilla_source.sector_count() {
        virtual_disc.read_sector(lba, &mut observed).unwrap();
        vanilla_source.read_sector(lba, &mut expected).unwrap();
        assert_eq!(
            observed, expected,
            "empty package set preserves raw sector {lba}"
        );
    }
}

#[test]
fn delta_encoding_is_deterministic_and_checks_both_content_hashes() {
    use xem_disc::mods::delta;

    let source = pattern(8_193, 0x45);
    let mut result = source.clone();
    result[1_023..1_061].copy_from_slice(&pattern(38, 0x92));
    result.extend(pattern(711, 0xCE));
    let first = delta::build(&source, &result).unwrap();
    let second = delta::build(&source, &result).unwrap();
    assert_eq!(
        first, second,
        "the same source/result pair has a stable delta"
    );
    assert_eq!(delta::apply(&source, &first).unwrap(), result);
    assert!(
        delta::apply(&source[1..], &first).is_err(),
        "wrong source is refused"
    );
    let mut corrupt = first.clone();
    let last = corrupt.len() - 1;
    corrupt[last] ^= 0x80;
    assert!(
        delta::apply(&source, &corrupt).is_err(),
        "corrupt result is refused"
    );
}

#[test]
fn package_application_rejects_wrong_source_and_corrupt_authored_result_hashes() {
    let (source, _, original, _) = synthetic_disc();
    let mut boot_source = source;
    let identification = identified(&mut boot_source);
    let target = DiscTarget::from_identification(&identification);
    let result = pattern(RESULT_SIZE, 0x12);

    // A same-size source mutation reaches package application, where its
    // declared source hash must fail before a prepared overlay is published.
    let delta_bytes = package_bytes("hash-source", Some(original), &result, &target);
    let mut changed_image = synthetic_disc().0.into_inner();
    let changed_at = TARGET_LBA as usize * SECTOR_SIZE + 24 + 17;
    changed_image[changed_at] ^= 1;
    let mut wrong_source = MemorySource::whole(changed_image).unwrap();
    let wrong_identification = identified(&mut wrong_source);
    let delta_set = PackageSet::new(vec![Package::decode(&delta_bytes).unwrap()]).unwrap();
    let mut preparation = delta_set.preparation(&wrong_identification).unwrap();
    let error = preparation
        .advance(&mut wrong_source)
        .unwrap_err()
        .to_string();
    assert!(error.contains("source SHA-256 differs"), "{error}");
    assert!(
        preparation.overlay().is_none(),
        "a failed package never publishes an overlay"
    );

    // An authored payload can pass the outer seal while disagreeing with its
    // manifest result hash; application must still reject it.
    let mut authored = package_bytes("hash-result", None, &result, &target);
    let manifest_size = u32::from_le_bytes(authored[8..12].try_into().unwrap()) as usize;
    let payload_start = 12 + manifest_size;
    authored[payload_start + 3] ^= 0x40;
    let seal_at = authored.len() - 32;
    let seal = Sha256::digest(&authored[..seal_at]);
    authored[seal_at..].copy_from_slice(&seal);
    assert!(
        Package::decode(&authored).is_err(),
        "a sealed but corrupt authored payload is refused"
    );
}

#[test]
fn package_conflicts_require_declared_ordered_overrides_and_use_original_source() {
    let (mut source, _, original, _) = synthetic_disc();
    let identification = identified(&mut source);
    let target = DiscTarget::from_identification(&identification);
    let first_result = pattern(3_901, 0x20);
    let second_result = pattern(4_203, 0x60);
    let first = Package::decode(&package_bytes(
        "first",
        Some(original.clone()),
        &first_result,
        &target,
    ))
    .unwrap();
    let second_bytes = package_bytes("second", Some(original), &second_result, &target);
    // Rebuild with the explicit override declaration: package payload bytes
    // remain deterministic, and the second delta still targets vanilla bytes.
    let second = Package::build(
        BuildSpec {
            id: "second".into(),
            discs: vec![target.clone()],
            requires: vec![],
            overrides: vec!["first".into()],
        },
        vec![BuildFile {
            disc: target.serial.clone(),
            slot: TARGET_SLOT,
            source: Some(pattern(ORIGINAL_SIZE, 0x31)),
            result: second_result.clone(),
        }],
    )
    .unwrap();
    assert!(PackageSet::new(vec![first.clone(), Package::decode(&second_bytes).unwrap()]).is_err());
    assert!(PackageSet::new(vec![Package::decode(&second_bytes).unwrap(), first.clone()]).is_err());
    let set = PackageSet::new(vec![first, Package::decode(&second).unwrap()]).unwrap();
    assert_eq!(set.identities().len(), 2);

    let (mut source, _, _, _) = synthetic_disc();
    let mut preparation = set.preparation(&identification).unwrap();
    while !preparation.advance(&mut source).unwrap() {}
    let mut overlaid = preparation
        .overlay()
        .unwrap()
        .wrap(Box::new(source))
        .unwrap();
    let index = Index::from_disc(overlaid.as_mut()).unwrap();
    let record = index.file(0, 1).unwrap();
    assert_eq!(record.size, second_result.len() as i32);
    assert_eq!(
        read_data(overlaid.as_mut(), record.lba, record.size as usize).unwrap(),
        second_result
    );
}

fn reseal_package(bytes: &mut [u8]) {
    let seal_at = bytes.len() - 32;
    let seal = Sha256::digest(&bytes[..seal_at]);
    bytes[seal_at..].copy_from_slice(&seal);
}

#[test]
fn overridden_owners_are_still_validated_against_the_vanilla_source() {
    let (mut source, _, original, _) = synthetic_disc();
    let identification = identified(&mut source);
    let target = DiscTarget::from_identification(&identification);
    let mut wrong_source = original.clone();
    wrong_source[37] ^= 0x7f;
    let earlier = Package::decode(&package_bytes(
        "earlier-owner",
        Some(wrong_source),
        &pattern(ORIGINAL_SIZE, 0x88),
        &target,
    ))
    .unwrap();
    let later = Package::build(
        BuildSpec {
            id: "later-owner".into(),
            discs: vec![target.clone()],
            requires: vec![],
            overrides: vec!["earlier-owner".into()],
        },
        vec![BuildFile {
            disc: target.serial.clone(),
            slot: TARGET_SLOT,
            source: Some(original),
            result: pattern(ORIGINAL_SIZE, 0x91),
        }],
    )
    .unwrap();
    let set = PackageSet::new(vec![earlier, Package::decode(&later).unwrap()]).unwrap();
    let mut preparation = set.preparation(&identification).unwrap();
    let error = preparation.advance(&mut source).unwrap_err().to_string();
    assert!(
        error.contains("source SHA-256 differs"),
        "overridden earlier owner: {error}"
    );
    assert!(preparation.overlay().is_none());

    let authored_result = pattern(RESULT_SIZE, 0xA1);
    let mut bad_result = package_bytes(
        "bad-result-owner",
        Some(pattern(ORIGINAL_SIZE, 0x31)),
        &authored_result,
        &target,
    );
    let manifest_size = u32::from_le_bytes(bad_result[8..12].try_into().unwrap()) as usize;
    // This fixture's delta emits the changed result as one literal operation.
    bad_result[12 + manifest_size + 21] ^= 0x10;
    reseal_package(&mut bad_result);
    let later = Package::build(
        BuildSpec {
            id: "later-result-owner".into(),
            discs: vec![target.clone()],
            requires: vec![],
            overrides: vec!["bad-result-owner".into()],
        },
        vec![BuildFile {
            disc: target.serial.clone(),
            slot: TARGET_SLOT,
            source: None,
            result: pattern(RESULT_SIZE, 0xD1),
        }],
    )
    .unwrap();
    let set = PackageSet::new(vec![
        Package::decode(&bad_result).unwrap(),
        Package::decode(&later).unwrap(),
    ])
    .unwrap();
    let (mut source, _, _, _) = synthetic_disc();
    let mut preparation = set.preparation(&identification).unwrap();
    let error = preparation.advance(&mut source).unwrap_err().to_string();
    assert!(
        error.contains("result size/SHA-256 differs"),
        "overridden earlier owner's result: {error}"
    );
    assert!(preparation.overlay().is_none());

    // Structural delta validity is checked when decoding each package, not
    // only when its result wins the final override set.
    let mut malformed = package_bytes(
        "malformed-owner",
        Some(pattern(ORIGINAL_SIZE, 0x31)),
        &pattern(ORIGINAL_SIZE, 0xB5),
        &target,
    );
    let manifest_size = u32::from_le_bytes(malformed[8..12].try_into().unwrap()) as usize;
    malformed[12 + manifest_size] ^= 0xFF;
    reseal_package(&mut malformed);
    assert!(
        Package::decode(&malformed).is_err(),
        "an overridden package cannot hide malformed delta instructions"
    );
}

#[test]
fn many_large_source_records_allocate_only_the_active_read_batch() {
    const TARGETS: usize = 16;
    let (mut source, identification, target_lbas) = sparse_large_target_source(TARGETS);
    let target = DiscTarget::from_identification(&identification);
    let files = (1..=TARGETS)
        .map(|slot| BuildFile {
            disc: target.serial.clone(),
            slot: slot as u32,
            source: None,
            result: vec![slot as u8; 8],
        })
        .collect();
    let bytes = Package::build(
        BuildSpec {
            id: "many-large-sources".into(),
            discs: vec![target],
            requires: vec![],
            overrides: vec![],
        },
        files,
    )
    .unwrap();
    let set = PackageSet::new(vec![Package::decode(&bytes).unwrap()]).unwrap();
    let mut preparation = set.preparation(&identification).unwrap();
    assert_eq!(target_lbas.len(), TARGETS);
    assert_eq!(
        preparation.resident_source_bytes(),
        0,
        "16 × 32 MiB source records reserve no eager source buffers"
    );

    // The 64-sector source batch is present; the next sector range is not.
    // NotReady must preserve only this first file's bounded progress.
    assert!(matches!(
        preparation.advance(&mut source),
        Err(Error::NotReady)
    ));
    let retained = preparation.resident_source_bytes();
    assert!(
        retained > 0,
        "the completed 64-sector batch is retained for retry"
    );
    assert!(
        retained <= 128 * FORM1_DATA_SIZE,
        "at most one 64-sector source batch is retained: {retained} bytes"
    );
}
