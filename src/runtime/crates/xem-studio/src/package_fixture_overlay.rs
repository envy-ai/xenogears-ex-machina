//! Synthetic-only native/WASM parity probe for the shared package overlay path.
//! This module is compiled only with the `test-fixtures` feature.

use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::cell::RefCell;
use std::rc::Rc;
use xem_disc::index::{
    DIRECTORY_BYTES, DIRECTORY_LBA, EMBEDDED_DIRECTORIES, EMBEDDED_INDEX, INDEX_BYTES, INDEX_LBA,
};
use xem_disc::mods::PackageSet;
use xem_disc::{BinSource, Error, PrefetchedFile, game_index, identify, read_data};

const SYNTHETIC_BOOT: &str = "SLUS_999.99;1";
const SYNTHETIC_BOOT_SHA256: &str =
    "dfe782b25d3e17220c592d2646d05ac78b3d42dd110d291a46c1f3d87c13ba6d";
const CHUNK_BYTES: usize = xem_disc::SECTOR_SIZE;
const SOURCE_CACHE_BYTES: usize = 40 * CHUNK_BYTES;
const MAX_IMAGE_BYTES: usize = 2 * 1024 * 1024;
const MAX_PACKAGE_BYTES: usize = 1024 * 1024;
const MAX_PREPARATION_ATTEMPTS: usize = 32;

fn sha256(bytes: &[u8]) -> String {
    xem_disc::identify::hex(&Sha256::digest(bytes))
}

fn record_json(record: xem_disc::Record) -> Value {
    json!({ "lba": record.lba, "size": record.size })
}

fn fill_missing(
    reader: &Rc<RefCell<PrefetchedFile>>,
    image: &[u8],
) -> Result<(usize, usize), String> {
    let missing = reader.borrow_mut().take_missing();
    if missing.is_empty() {
        return Err("preparation requested no fillable source ranges".into());
    }
    let mut total = 0usize;
    for range in missing.iter() {
        let start = usize::try_from(range.start).map_err(|_| "missing range start overflow")?;
        let end = usize::try_from(range.end).map_err(|_| "missing range end overflow")?;
        if start >= end || end > image.len() || !start.is_multiple_of(CHUNK_BYTES) {
            return Err("preparation requested an invalid synthetic image range".into());
        }
        let bytes = image
            .get(start..end)
            .ok_or_else(|| "missing range exceeds the synthetic image".to_owned())?;
        reader
            .borrow_mut()
            .fill(range.start, bytes)
            .map_err(|error| format!("fill synthetic source range: {error}"))?;
        total = total
            .checked_add(bytes.len())
            .ok_or_else(|| "missing-byte metric overflow".to_owned())?;
    }
    Ok((missing.len(), total))
}

fn read_data_filling(
    source: &mut dyn xem_disc::SectorSource,
    reader: &Rc<RefCell<PrefetchedFile>>,
    image: &[u8],
    lba: u32,
    size: usize,
    missing_ranges: &mut usize,
    missing_bytes: &mut usize,
) -> Result<Vec<u8>, String> {
    for _ in 0..MAX_PREPARATION_ATTEMPTS {
        match read_data(source, lba, size) {
            Ok(bytes) => return Ok(bytes),
            Err(Error::NotReady) => {
                let (ranges, bytes) = fill_missing(reader, image)?;
                *missing_ranges = (*missing_ranges)
                    .checked_add(ranges)
                    .ok_or_else(|| "missing-range metric overflow".to_owned())?;
                *missing_bytes = (*missing_bytes)
                    .checked_add(bytes)
                    .ok_or_else(|| "missing-byte metric overflow".to_owned())?;
            }
            Err(error) => return Err(format!("read synthetic overlay LBA {lba}: {error}")),
        }
    }
    Err("synthetic overlay read exceeded its retry bound".into())
}

/// Run the same package validation/preparation code used by the actual disc host.
pub(super) fn run(image: &[u8], package_bytes: &[u8], slot: u32) -> Result<String, String> {
    if image.is_empty() || image.len() > MAX_IMAGE_BYTES || !image.len().is_multiple_of(CHUNK_BYTES)
    {
        return Err("synthetic image size is invalid or exceeds its fixture bound".into());
    }
    if package_bytes.is_empty() || package_bytes.len() > MAX_PACKAGE_BYTES {
        return Err("synthetic package-set size is invalid or exceeds its fixture bound".into());
    }

    // First identify the boot program from a complete in-memory synthetic image.
    // The actual preparation source below starts empty so its missing-read retry
    // path is exercised independently and deterministically.
    let mut identification_source = BinSource::whole(image.to_vec())
        .map_err(|error| format!("open synthetic image: {error}"))?;
    let identification = identify(&mut identification_source)
        .map_err(|error| format!("identify synthetic image: {error}"))?;
    if identification.boot_path != SYNTHETIC_BOOT {
        return Err("fixture overlay helper accepts only SLUS_999.99;1".into());
    }
    if xem_disc::identify::hex(&identification.sha256) != SYNTHETIC_BOOT_SHA256 {
        return Err("fixture overlay helper requires the pinned invented executable".into());
    }
    let original_index = game_index(&identification.executable)
        .map_err(|error| format!("parse synthetic executable index: {error}"))?;
    let original_record = original_index
        .record(slot as usize)
        .ok_or_else(|| format!("synthetic slot {slot} is absent"))?;
    let untouched_slot = if slot == 0xB8 { 0xBA } else { 0xB8 };
    let untouched_record = original_index
        .record(untouched_slot)
        .ok_or_else(|| "synthetic untouched slot is absent".to_owned())?;
    if original_record.size <= 0 || untouched_record.size <= 0 {
        return Err("fixture overlay source slots must contain data".into());
    }

    let set = PackageSet::from_bytes(package_bytes)
        .map_err(|error| format!("decode synthetic package set: {error}"))?;
    let mut preparation = set
        .preparation(&identification)
        .map_err(|error| format!("prepare synthetic package set: {error}"))?;

    let reader = Rc::new(RefCell::new(PrefetchedFile::new(
        image.len() as u64,
        CHUNK_BYTES,
        SOURCE_CACHE_BYTES,
    )));
    let mut source = BinSource::whole(reader.clone())
        .map_err(|error| format!("open prefetched synthetic image: {error}"))?;
    let mut attempts = 0usize;
    let mut missing_ranges = 0usize;
    let mut missing_bytes = 0usize;
    loop {
        attempts += 1;
        if attempts > MAX_PREPARATION_ATTEMPTS {
            return Err("synthetic package preparation exceeded its retry bound".into());
        }
        match preparation.advance(&mut source) {
            Ok(true) => break,
            Ok(false) => {
                let (ranges, bytes) = fill_missing(&reader, image)?;
                missing_ranges += ranges;
                missing_bytes = missing_bytes
                    .checked_add(bytes)
                    .ok_or_else(|| "missing-byte metric overflow".to_owned())?;
            }
            Err(Error::NotReady) => {
                let (ranges, bytes) = fill_missing(&reader, image)?;
                missing_ranges += ranges;
                missing_bytes = missing_bytes
                    .checked_add(bytes)
                    .ok_or_else(|| "missing-byte metric overflow".to_owned())?;
            }
            Err(error) => return Err(format!("prepare synthetic overlay: {error}")),
        }
    }
    let resident_source_bytes = preparation.resident_source_bytes();
    let preparation_progress = json!({
        "attempts": attempts,
        "ranges": missing_ranges,
        "bytes": missing_bytes,
        "resident_source_bytes": resident_source_bytes,
        "cache_budget_bytes": SOURCE_CACHE_BYTES,
    });
    let overlay = preparation
        .overlay()
        .ok_or_else(|| "preparation completed without a prepared overlay".to_owned())?;

    let executable = overlay.executable().to_vec();
    let embedded_index = executable
        .get(EMBEDDED_INDEX..EMBEDDED_INDEX + INDEX_BYTES)
        .ok_or_else(|| "prepared executable has no complete embedded index".to_owned())?;
    let embedded_directories = executable
        .get(EMBEDDED_DIRECTORIES..EMBEDDED_DIRECTORIES + DIRECTORY_BYTES)
        .ok_or_else(|| "prepared executable has no directory table".to_owned())?;
    let embedded_record = game_index(&executable)
        .map_err(|error| format!("parse prepared executable index: {error}"))?
        .record(slot as usize)
        .ok_or_else(|| "prepared executable target slot is absent".to_owned())?;

    // Keep an unwrapped source view and wrap a second view over the same bounded
    // prefetched file so original bytes and virtual bytes can be compared.
    let mut original_view = BinSource::whole(reader.clone())
        .map_err(|error| format!("open original synthetic view: {error}"))?;
    let mut inspection_ranges = 0;
    let mut inspection_bytes = 0;
    let original_index_bytes = read_data_filling(
        &mut original_view,
        &reader,
        image,
        INDEX_LBA,
        INDEX_BYTES,
        &mut inspection_ranges,
        &mut inspection_bytes,
    )?;
    let original_directories = read_data_filling(
        &mut original_view,
        &reader,
        image,
        DIRECTORY_LBA,
        DIRECTORY_BYTES,
        &mut inspection_ranges,
        &mut inspection_bytes,
    )?;
    let original_embedded_index = identification
        .executable
        .get(EMBEDDED_INDEX..EMBEDDED_INDEX + INDEX_BYTES)
        .ok_or_else(|| "original executable has no complete embedded index".to_owned())?;
    let original_embedded_directories = identification
        .executable
        .get(EMBEDDED_DIRECTORIES..EMBEDDED_DIRECTORIES + DIRECTORY_BYTES)
        .ok_or_else(|| "original executable has no directory table".to_owned())?;
    if original_embedded_index != original_index_bytes
        || original_embedded_directories != original_directories
    {
        return Err("synthetic fixture executable and disc mirror metadata differ".into());
    }
    let original_source_bytes = read_data_filling(
        &mut original_view,
        &reader,
        image,
        original_record.lba,
        original_record.size as usize,
        &mut inspection_ranges,
        &mut inspection_bytes,
    )?;
    let untouched_source_bytes = read_data_filling(
        &mut original_view,
        &reader,
        image,
        untouched_record.lba,
        untouched_record.size as usize,
        &mut inspection_ranges,
        &mut inspection_bytes,
    )?;
    let _original_boot_bytes = read_data_filling(
        &mut original_view,
        &reader,
        image,
        identification.boot_lba,
        identification.executable.len(),
        &mut inspection_ranges,
        &mut inspection_bytes,
    )?;

    let mut wrapped = overlay
        .wrap(Box::new(source))
        .map_err(|error| format!("wrap synthetic source: {error}"))?;
    let mirror_index = read_data_filling(
        wrapped.as_mut(),
        &reader,
        image,
        INDEX_LBA,
        INDEX_BYTES,
        &mut inspection_ranges,
        &mut inspection_bytes,
    )?;
    let mirror_directories = read_data_filling(
        wrapped.as_mut(),
        &reader,
        image,
        DIRECTORY_LBA,
        DIRECTORY_BYTES,
        &mut inspection_ranges,
        &mut inspection_bytes,
    )?;
    let boot_mirror = read_data_filling(
        wrapped.as_mut(),
        &reader,
        image,
        identification.boot_lba,
        executable.len(),
        &mut inspection_ranges,
        &mut inspection_bytes,
    )?;
    let mirror_record = xem_disc::Index::parse(&mirror_index, &mirror_directories)
        .record(slot as usize)
        .ok_or_else(|| "prepared mirror target slot is absent".to_owned())?;
    let replacement_bytes = read_data_filling(
        wrapped.as_mut(),
        &reader,
        image,
        mirror_record.lba,
        mirror_record.size as usize,
        &mut inspection_ranges,
        &mut inspection_bytes,
    )?;
    let untouched_overlay_bytes = read_data_filling(
        wrapped.as_mut(),
        &reader,
        image,
        untouched_record.lba,
        untouched_record.size as usize,
        &mut inspection_ranges,
        &mut inspection_bytes,
    )?;

    if embedded_index != mirror_index.as_slice() || embedded_directories != mirror_directories {
        return Err("prepared executable and disc mirror metadata differ".into());
    }
    if embedded_record != mirror_record {
        return Err("prepared executable and disc mirror target records differ".into());
    }
    if executable != boot_mirror {
        return Err("prepared executable and raw boot mirror differ".into());
    }
    if sha256(&untouched_source_bytes) != sha256(&untouched_overlay_bytes) {
        return Err("overlay changed the untouched synthetic file".into());
    }

    let executable_sha256 = sha256(&executable);
    let boot_mirror_sha256 = sha256(&boot_mirror);
    let embedded_index_sha256 = sha256(embedded_index);
    let mirror_index_sha256 = sha256(&mirror_index);
    let replacement_sha256 = sha256(&replacement_bytes);
    let source_sha256 = sha256(&original_source_bytes);
    let untouched_source_sha256 = sha256(&untouched_source_bytes);
    let untouched_overlay_sha256 = sha256(&untouched_overlay_bytes);
    let summary = json!({
        "boot_path": identification.boot_path,
        "slot": slot,
        "executable": executable,
        "executable_sha256": executable_sha256,
        "boot_mirror": boot_mirror,
        "boot_mirror_sha256": boot_mirror_sha256,
        "embedded_index_sha256": embedded_index_sha256,
        "mirror_index": mirror_index,
        "mirror_index_sha256": mirror_index_sha256,
        "embedded_record": record_json(embedded_record),
        "mirror_record": record_json(mirror_record),
        "replacement_bytes": replacement_bytes,
        "replacement_sha256": replacement_sha256,
        "source_sha256": source_sha256,
        "untouched_source_sha256": untouched_source_sha256,
        "untouched_overlay_sha256": untouched_overlay_sha256,
        "missing_read_progress": {
            "preparation": preparation_progress,
            "inspection": {
                "ranges": inspection_ranges,
                "bytes": inspection_bytes,
                "cache_budget_bytes": SOURCE_CACHE_BYTES,
            },
        },
    });
    serde_json::to_string(&summary).map_err(|error| format!("serialize parity summary: {error}"))
}
