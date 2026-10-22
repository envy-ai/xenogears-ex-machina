use super::{
    DiscTarget, FileEntry, PackageSet, integrity,
    package::{MAX_TOTAL_BYTES, hash},
};
use crate::index::{
    DIRECTORY_BYTES, DIRECTORY_LBA, EMBEDDED_DIRECTORIES, EMBEDDED_INDEX, INDEX_BYTES, INDEX_LBA,
    RECORD_SIZE,
};
use crate::{
    Identification, Index, Record, SectorSource,
    sector::{self, FORM1_DATA_SIZE, Form, SECTOR_SIZE, Sector, Subheader},
    source::{Result, format_error},
};
use std::collections::BTreeMap;
use std::sync::Arc;

/// Last valid 99:59:74 raw-sector position minus the two-second lead-in.
const MAX_LBA: u32 = 100 * 60 * 75 - 151;
const BATCH: usize = 64;
#[derive(Debug, Clone)]
struct FileData {
    lba: u32,
    bytes: Arc<[u8]>,
    sub: Subheader,
    last_flags: u8,
}

/// Validated immutable data outside the original-byte cache.
#[derive(Debug)]
pub struct PreparedOverlay {
    original_sectors: u32,
    sectors: u32,
    boot_lba: u32,
    boot_sha256: String,
    executable: Vec<u8>,
    patches: BTreeMap<u32, [u8; SECTOR_SIZE]>,
    files: Vec<FileData>,
}
impl PreparedOverlay {
    pub fn executable(&self) -> &[u8] {
        &self.executable
    }
    pub fn wrap(
        self: &Arc<Self>,
        mut source: Box<dyn SectorSource>,
    ) -> Result<Box<dyn SectorSource>> {
        if source.sector_count() != self.original_sectors {
            return Err(format_error("mod overlay belongs to another source extent"));
        }
        if self.patches.is_empty() && self.files.is_empty() {
            return Ok(source);
        }
        if hash(&crate::read_data(
            source.as_mut(),
            self.boot_lba,
            self.executable.len(),
        )?) != self.boot_sha256
        {
            return Err(format_error("mod overlay belongs to another boot image"));
        }
        Ok(Box::new(VirtualDisc {
            source,
            overlay: self.clone(),
        }))
    }
}
struct VirtualDisc {
    source: Box<dyn SectorSource>,
    overlay: Arc<PreparedOverlay>,
}
impl SectorSource for VirtualDisc {
    fn sector_count(&self) -> u32 {
        self.overlay.sectors
    }
    fn read_sector(&mut self, lba: u32, out: &mut [u8; SECTOR_SIZE]) -> Result<()> {
        crate::source::check_span(lba, SECTOR_SIZE, self.sector_count())?;
        if let Some(patch) = self.overlay.patches.get(&lba) {
            *out = *patch;
            return Ok(());
        }
        if lba < self.overlay.original_sectors {
            return self.source.read_sector(lba, out);
        }
        let file = self
            .overlay
            .files
            .iter()
            .find(|file| {
                lba >= file.lba
                    && lba - file.lba < file.bytes.len().div_ceil(FORM1_DATA_SIZE).max(1) as u32
            })
            .ok_or_else(|| format_error("unassigned mod virtual sector"))?;
        let offset = (lba - file.lba) as usize * FORM1_DATA_SIZE;
        let data = file.bytes.get(offset..).unwrap_or(&[]);
        let mut sub = file.sub;
        if data.len() <= FORM1_DATA_SIZE {
            sub.submode |= file.last_flags;
        }
        *out = sector::build_mode2_sector(lba, sub, &data[..data.len().min(FORM1_DATA_SIZE)]);
        integrity::regenerate(out);
        Ok(())
    }
}
struct Pending {
    entry: FileEntry,
    validators: Vec<(usize, FileEntry)>,
    record: Record,
    bytes: Vec<u8>,
    sub: Option<Subheader>,
    last_flags: u8,
}
/// Stateful preparation survives missing-range retries without replaying a large read.
pub struct Preparation {
    set: PackageSet,
    executable: Vec<u8>,
    boot_lba: u32,
    boot_sha256: String,
    index: Vec<u8>,
    disc_index: Vec<u8>,
    directories: Vec<u8>,
    pending: Vec<Pending>,
    at: usize,
    results: Vec<(u32, Arc<[u8]>, Subheader, u8)>,
    original_sectors: Option<u32>,
    metadata_checked: bool,
    ready: Option<Arc<PreparedOverlay>>,
}
impl Preparation {
    pub(super) fn new(set: PackageSet, id: &Identification) -> Result<Self> {
        let target = DiscTarget::from_identification(id);
        let mut files: BTreeMap<u32, Vec<(usize, FileEntry)>> = BTreeMap::new();
        for (p, package) in set.packages.iter().enumerate() {
            let declared = package
                .manifest()
                .discs
                .iter()
                .find(|disc| disc.serial == target.serial)
                .ok_or_else(|| {
                    format_error(format!(
                        "mod {} does not declare disc {}",
                        package.manifest().id,
                        target.serial
                    ))
                })?;
            if declared.sha256 != target.sha256 {
                return Err(format_error(format!(
                    "mod {} boot SHA-256 differs for {}",
                    package.manifest().id,
                    target.serial
                )));
            }
            for file in package
                .manifest()
                .files
                .iter()
                .filter(|file| file.disc == target.serial)
            {
                files.entry(file.slot).or_default().push((p, file.clone()));
            }
        }
        let mut pending = Vec::new();
        let mut index = Vec::new();
        let mut directories = Vec::new();
        if !files.is_empty() {
            let parsed = crate::game_index(&id.executable)?;
            index = id.executable[EMBEDDED_INDEX..EMBEDDED_INDEX + INDEX_BYTES].to_vec();
            directories = id.executable
                [EMBEDDED_DIRECTORIES..EMBEDDED_DIRECTORIES + DIRECTORY_BYTES]
                .to_vec();
            for (slot, validators) in files {
                let entry = validators.last().expect("nonempty file owners").1.clone();
                let record = parsed
                    .record(slot as usize)
                    .ok_or_else(|| format_error("mod index slot absent"))?;
                if record.is_unused()
                    || record.size <= 0
                    || record.size as usize > super::delta::MAX_FILE_BYTES
                {
                    return Err(format_error(
                        "mod target is unused, empty, a directory or exceeds the file limit",
                    ));
                }
                pending.push(Pending {
                    entry,
                    validators,
                    record,
                    bytes: Vec::new(),
                    sub: None,
                    last_flags: 0,
                });
            }
        }
        Ok(Self {
            set,
            executable: id.executable.clone(),
            boot_lba: id.boot_lba,
            boot_sha256: target.sha256,
            index,
            disc_index: Vec::new(),
            directories,
            pending,
            at: 0,
            results: Vec::new(),
            original_sectors: None,
            metadata_checked: false,
            ready: None,
        })
    }
    pub fn overlay(&self) -> Option<Arc<PreparedOverlay>> {
        self.ready.clone()
    }
    /// Source-buffer allocation retained across missing-range retries. Only the active file may hold bytes.
    pub fn resident_source_bytes(&self) -> usize {
        self.pending.iter().map(|file| file.bytes.capacity()).sum()
    }
    pub fn advance(&mut self, source: &mut dyn SectorSource) -> Result<bool> {
        if self.ready.is_some() {
            return Ok(true);
        }
        let count = source.sector_count();
        if self.original_sectors.is_some_and(|n| n != count) {
            return Err(format_error("mod source extent changed during preparation"));
        }
        self.original_sectors = Some(count);
        let boot_end = self
            .boot_lba
            .checked_add(self.executable.len().div_ceil(FORM1_DATA_SIZE) as u32)
            .ok_or_else(|| format_error("boot extent overflow"))?;
        if boot_end > count {
            return Err(format_error("boot extent outside source"));
        }
        if !self.metadata_checked && !self.pending.is_empty() {
            let disc_index = crate::read_data(source, INDEX_LBA, INDEX_BYTES)?;
            // The index ends with one opaque byte outside the complete 7-byte
            // records. The original discs differ from their embedded EXEs in
            // that byte; validate every record and preserve each copy's tail.
            let records_end = INDEX_BYTES / RECORD_SIZE * RECORD_SIZE;
            if disc_index[..records_end] != self.index[..records_end]
                || crate::read_data(source, DIRECTORY_LBA, DIRECTORY_BYTES)? != self.directories
            {
                return Err(format_error(
                    "embedded and on-disc mod index records/directories differ",
                ));
            }
            self.disc_index = disc_index;
            let mut directory_sector = [0; SECTOR_SIZE];
            source.read_sector(DIRECTORY_LBA, &mut directory_sector)?;
            form1(&directory_sector, DIRECTORY_LBA)?;
            let parsed = Index::parse(&self.index, &self.directories);
            for file in &self.pending {
                let end = file
                    .record
                    .lba
                    .checked_add(file.record.sectors())
                    .ok_or_else(|| format_error("mod source sector overflow"))?;
                if end > count
                    || intersects(file.record.lba, end, INDEX_LBA, INDEX_LBA + 16)
                    || intersects(file.record.lba, end, DIRECTORY_LBA, DIRECTORY_LBA + 1)
                    || intersects(file.record.lba, end, self.boot_lba, boot_end)
                {
                    return Err(format_error(
                        "mod source extent outside image or overlapping index/boot metadata",
                    ));
                }
                for (slot, other) in parsed.entries().filter(|(_, r)| r.size >= 0) {
                    if slot != file.entry.slot as usize
                        && intersects(
                            file.record.lba,
                            end,
                            other.lba,
                            other
                                .lba
                                .checked_add(other.sectors())
                                .ok_or_else(|| format_error("indexed extent overflow"))?,
                        )
                    {
                        return Err(format_error(
                            "mod source file overlaps another indexed extent (unsupported layout)",
                        ));
                    }
                }
            }
            self.metadata_checked = true;
        }
        while self.at < self.pending.len() {
            let file = &mut self.pending[self.at];
            let sectors = file.record.sectors() as usize;
            let mut sector_at = file.bytes.len() / FORM1_DATA_SIZE;
            while sector_at < sectors {
                let batch = (sectors - sector_at).min(BATCH);
                let mut raw = vec![0; batch * SECTOR_SIZE];
                source.read_sectors(file.record.lba + sector_at as u32, &mut raw)?;
                for (n, raw) in raw.chunks_exact(SECTOR_SIZE).enumerate() {
                    let lba = file.record.lba + (sector_at + n) as u32;
                    let sub = form1(raw, lba)?;
                    let flags = sector::submode::END_OF_FILE | sector::submode::END_OF_RECORD;
                    let base = Subheader {
                        submode: sub.submode & !flags,
                        ..sub
                    };
                    if file.sub.is_some_and(|previous| previous != base) {
                        return Err(format_error(
                            "mod source has changing XA subheaders (unsupported layout)",
                        ));
                    }
                    file.sub = Some(base);
                    file.last_flags = sub.submode & flags;
                    file.bytes
                        .extend_from_slice(Sector::new(raw).expect("raw sector").form1_window());
                }
                sector_at += batch;
            }
            file.bytes.truncate(file.record.size as usize);
            let mut result = Vec::new();
            for (package, entry) in &file.validators {
                result = self.set.packages[*package].apply(entry, &file.bytes)?;
            }
            let sub = file
                .sub
                .ok_or_else(|| format_error("mod source has no Form1 sector"))?;
            self.results
                .push((file.entry.slot, Arc::from(result), sub, file.last_flags));
            file.bytes.clear();
            file.bytes.shrink_to_fit();
            self.at += 1;
        }
        let mut index = self.index.clone();
        let mut disc_index = self.disc_index.clone();
        let mut files = Vec::new();
        let mut next = count;
        let mut total = 0usize;
        for (slot, bytes, sub, last_flags) in &self.results {
            total = total
                .checked_add(bytes.len())
                .ok_or_else(|| format_error("mod result sum overflow"))?;
            if total > MAX_TOTAL_BYTES {
                return Err(format_error("mod disc results exceed 128 MiB"));
            }
            let sectors = bytes.len().div_ceil(FORM1_DATA_SIZE).max(1) as u32;
            let end = next
                .checked_add(sectors)
                .ok_or_else(|| format_error("mod virtual sector overflow"))?;
            if next > 0xff_ffff
                || end - 1 > 0xff_ffff
                || end - 1 > MAX_LBA
                || bytes.len() > i32::MAX as usize
            {
                return Err(format_error(
                    "mod virtual extent exceeds 24-bit LBA, BCD position or signed file-size limits",
                ));
            }
            let at = *slot as usize * RECORD_SIZE;
            index[at..at + 3].copy_from_slice(&next.to_le_bytes()[..3]);
            index[at + 3..at + 7].copy_from_slice(&(bytes.len() as i32).to_le_bytes());
            disc_index[at..at + RECORD_SIZE].copy_from_slice(&index[at..at + RECORD_SIZE]);
            files.push(FileData {
                lba: next,
                bytes: bytes.clone(),
                sub: *sub,
                last_flags: *last_flags,
            });
            next = end;
        }
        let mut executable = self.executable.clone();
        let mut patches = BTreeMap::new();
        if !files.is_empty() {
            executable[EMBEDDED_INDEX..EMBEDDED_INDEX + INDEX_BYTES].copy_from_slice(&index);
            for (n, data) in disc_index.chunks(FORM1_DATA_SIZE).enumerate() {
                let start = n * FORM1_DATA_SIZE;
                if data != &self.disc_index[start..start + data.len()] {
                    patch(source, &mut patches, INDEX_LBA + n as u32, data)?;
                }
            }
            for (n, data) in executable.chunks(FORM1_DATA_SIZE).enumerate() {
                if data != &self.executable[n * FORM1_DATA_SIZE..n * FORM1_DATA_SIZE + data.len()] {
                    patch(source, &mut patches, self.boot_lba + n as u32, data)?;
                }
            }
        }
        self.ready = Some(Arc::new(PreparedOverlay {
            original_sectors: count,
            sectors: next,
            boot_lba: self.boot_lba,
            boot_sha256: self.boot_sha256.clone(),
            executable,
            patches,
            files,
        }));
        Ok(true)
    }
}
fn intersects(a: u32, b: u32, c: u32, d: u32) -> bool {
    a < d && c < b
}
fn form1(raw: &[u8], lba: u32) -> Result<Subheader> {
    let sector = Sector::new(raw).ok_or_else(|| format_error("truncated mod source sector"))?;
    let sub = sector
        .subheader()
        .ok_or_else(|| format_error("mod source is not MODE2"))?;
    if !sector.has_sync()
        || sector.header().lba() != Some(lba)
        || sub.form() != Form::Form1
        || raw[16..20] != raw[20..24]
        || sub.submode & sector::submode::DATA == 0
        || sub.submode
            & (sector::submode::AUDIO | sector::submode::VIDEO | sector::submode::REAL_TIME)
            != 0
    {
        return Err(format_error(
            "mod source is not a supported ordinary MODE2 Form1 data sector",
        ));
    }
    Ok(sub)
}
fn patch(
    source: &mut dyn SectorSource,
    patches: &mut BTreeMap<u32, [u8; SECTOR_SIZE]>,
    lba: u32,
    data: &[u8],
) -> Result<()> {
    let mut raw = [0; SECTOR_SIZE];
    source.read_sector(lba, &mut raw)?;
    form1(&raw, lba)?;
    raw[sector::USER_DATA_OFFSET..sector::USER_DATA_OFFSET + data.len()].copy_from_slice(data);
    integrity::regenerate(&mut raw);
    patches.insert(lba, raw);
    Ok(())
}
