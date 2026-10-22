use super::delta::{self, Reader};
use crate::{
    Identification,
    source::{Result, format_error},
};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, BTreeSet};
use std::sync::Arc;

pub const MAX_PACKAGE_BYTES: usize = 64 << 20;
pub(super) const MAX_TOTAL_BYTES: usize = 128 << 20;
const MAX_MANIFEST: usize = 64 << 10;
const MAX_FILES: usize = 1024;
const MAX_PACKAGES: usize = 32;
const MAGIC: &[u8; 8] = b"XEMMOD1\0";
const SET_MAGIC: &[u8; 8] = b"XEMSET1\0";

pub(super) fn hash(bytes: &[u8]) -> String {
    crate::identify::hex(&Sha256::digest(bytes))
}
fn valid_hash(text: &str) -> bool {
    text.len() == 64
        && text
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
}
fn valid_id(text: &str) -> bool {
    !text.is_empty()
        && text.len() <= 64
        && text
            .bytes()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b"._-".contains(&b))
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct DiscTarget {
    pub serial: String,
    pub sha256: String,
}
impl DiscTarget {
    pub fn from_identification(id: &Identification) -> Self {
        Self {
            serial: id
                .disc
                .map(|d| d.serial().to_owned())
                .unwrap_or_else(|| id.boot_path.clone()),
            sha256: crate::identify::hex(&id.sha256),
        }
    }
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BuildSpec {
    pub id: String,
    pub discs: Vec<DiscTarget>,
    pub requires: Vec<String>,
    pub overrides: Vec<String>,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BuildFile {
    pub disc: String,
    pub slot: u32,
    pub source: Option<Vec<u8>>,
    pub result: Vec<u8>,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PayloadKind {
    Delta,
    Authored,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FileEntry {
    pub disc: String,
    pub slot: u32,
    pub kind: PayloadKind,
    pub source_sha256: Option<String>,
    pub result_sha256: String,
    pub result_size: u32,
    pub offset: u32,
    pub length: u32,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Manifest {
    pub id: String,
    pub discs: Vec<DiscTarget>,
    pub requires: Vec<String>,
    pub overrides: Vec<String>,
    pub files: Vec<FileEntry>,
}
#[derive(Debug, Clone)]
pub struct Package {
    manifest: Manifest,
    bytes: Arc<[u8]>,
    payload_start: usize,
    digest: String,
}
impl Package {
    pub fn build(spec: BuildSpec, mut files: Vec<BuildFile>) -> Result<Vec<u8>> {
        if files.len() > MAX_FILES {
            return Err(format_error("too many mod files"));
        }
        let input_bytes = files
            .iter()
            .try_fold(0usize, |sum, file| {
                sum.checked_add(file.result.len())?
                    .checked_add(file.source.as_ref().map_or(0, Vec::len))
            })
            .ok_or_else(|| format_error("mod builder input overflow"))?;
        if input_bytes > MAX_TOTAL_BYTES {
            return Err(format_error("mod builder inputs exceed 128 MiB"));
        }
        files.sort_by(|a, b| (&a.disc, a.slot).cmp(&(&b.disc, b.slot)));
        let mut payload = Vec::new();
        let mut entries = Vec::new();
        for file in files {
            if file.result.len() > delta::MAX_FILE_BYTES {
                return Err(format_error("mod result exceeds 32 MiB"));
            }
            let (kind, source_sha256, data) = match file.source {
                Some(source) => (
                    PayloadKind::Delta,
                    Some(hash(&source)),
                    delta::build(&source, &file.result)?,
                ),
                None => (PayloadKind::Authored, None, file.result.clone()),
            };
            entries.push(FileEntry {
                disc: file.disc,
                slot: file.slot,
                kind,
                source_sha256,
                result_sha256: hash(&file.result),
                result_size: file.result.len() as u32,
                offset: payload.len() as u32,
                length: data.len() as u32,
            });
            if payload
                .len()
                .checked_add(data.len())
                .is_none_or(|n| n > MAX_PACKAGE_BYTES)
            {
                return Err(format_error("mod package exceeds 64 MiB"));
            }
            payload.extend_from_slice(&data);
        }
        let manifest = Manifest {
            id: spec.id,
            discs: spec.discs,
            requires: spec.requires,
            overrides: spec.overrides,
            files: entries,
        };
        let json = serde_json::to_vec(&manifest)
            .map_err(|e| format_error(format!("mod manifest: {e}")))?;
        if json.len() > MAX_MANIFEST {
            return Err(format_error("mod manifest exceeds 64 KiB"));
        }
        let mut bytes = MAGIC.to_vec();
        bytes.extend_from_slice(&(json.len() as u32).to_le_bytes());
        bytes.extend_from_slice(&json);
        bytes.extend_from_slice(&payload);
        bytes.extend_from_slice(&Sha256::digest(&bytes));
        Self::decode(&bytes)?;
        Ok(bytes)
    }
    pub fn decode(bytes: &[u8]) -> Result<Self> {
        if bytes.len() > MAX_PACKAGE_BYTES || bytes.len() < 44 {
            return Err(format_error("mod package size outside bounds"));
        }
        let sealed = bytes.len() - 32;
        if Sha256::digest(&bytes[..sealed])[..] != bytes[sealed..] {
            return Err(format_error("mod package checksum differs"));
        }
        let mut reader = Reader::new(&bytes[..sealed]);
        if reader.take(8)? != MAGIC {
            return Err(format_error("unknown mod package format"));
        }
        let length = reader.word()? as usize;
        if length > MAX_MANIFEST {
            return Err(format_error("mod manifest exceeds 64 KiB"));
        }
        let manifest: Manifest = serde_json::from_slice(reader.take(length)?)
            .map_err(|e| format_error(format!("mod manifest: {e}")))?;
        let payload_start = 12 + length;
        let payload_len = sealed - payload_start;
        if !valid_id(&manifest.id)
            || manifest.discs.is_empty()
            || manifest.discs.len() > 2
            || manifest.files.len() > MAX_FILES
        {
            return Err(format_error("mod id, disc list or file count is invalid"));
        }
        let mut discs = BTreeSet::new();
        for disc in &manifest.discs {
            if disc.serial.is_empty()
                || disc.serial.len() > 64
                || !valid_hash(&disc.sha256)
                || !discs.insert(&disc.serial)
            {
                return Err(format_error("invalid or duplicate mod disc identity"));
            }
        }
        for ids in [&manifest.requires, &manifest.overrides] {
            let mut seen = BTreeSet::new();
            if ids.len() > MAX_PACKAGES
                || ids
                    .iter()
                    .any(|id| !valid_id(id) || id == &manifest.id || !seen.insert(id))
            {
                return Err(format_error(
                    "invalid, self or duplicate mod dependency/override",
                ));
            }
        }
        let mut cursor = 0usize;
        let mut previous = None;
        for entry in &manifest.files {
            let key = (&entry.disc, entry.slot);
            if !discs.contains(&entry.disc)
                || entry.slot as usize >= crate::index::INDEX_BYTES / crate::index::RECORD_SIZE
                || previous.is_some_and(|p| p >= key)
            {
                return Err(format_error(
                    "mod files must be unique, sorted and name a declared disc/valid index slot",
                ));
            }
            previous = Some(key);
            if entry.result_size as usize > delta::MAX_FILE_BYTES
                || !valid_hash(&entry.result_sha256)
            {
                return Err(format_error("invalid mod result hash/size"));
            }
            match (entry.kind, &entry.source_sha256) {
                (PayloadKind::Delta, Some(digest)) if valid_hash(digest) => {}
                (PayloadKind::Authored, None) => {}
                _ => return Err(format_error("mod source hash disagrees with payload kind")),
            }
            if entry.offset as usize != cursor {
                return Err(format_error("mod payload intervals overlap or have gaps"));
            }
            cursor = cursor
                .checked_add(entry.length as usize)
                .ok_or_else(|| format_error("mod payload overflow"))?;
            if cursor > payload_len
                || (entry.kind == PayloadKind::Authored && entry.length != entry.result_size)
            {
                return Err(format_error(
                    "mod payload outside package or authored size differs",
                ));
            }
        }
        for entry in &manifest.files {
            let start = payload_start + entry.offset as usize;
            if entry.kind == PayloadKind::Delta
                && delta::validate(&bytes[start..start + entry.length as usize])?.1
                    != entry.result_size as usize
            {
                return Err(format_error(
                    "mod delta declared result size differs from manifest",
                ));
            }
            if entry.kind == PayloadKind::Authored
                && hash(&bytes[start..start + entry.length as usize]) != entry.result_sha256
            {
                return Err(format_error("mod authored result SHA-256 differs"));
            }
        }
        if cursor != payload_len {
            return Err(format_error("mod package has unclaimed payload bytes"));
        }
        Ok(Self {
            manifest,
            bytes: Arc::from(bytes),
            payload_start,
            digest: hash(bytes),
        })
    }
    pub fn manifest(&self) -> &Manifest {
        &self.manifest
    }
    pub fn bytes(&self) -> &[u8] {
        &self.bytes
    }
    pub fn identity(&self) -> String {
        format!("{}@{}", self.manifest.id, self.digest)
    }
    pub(super) fn apply(&self, entry: &FileEntry, source: &[u8]) -> Result<Vec<u8>> {
        if let Some(expected) = &entry.source_sha256 {
            if hash(source) != *expected {
                return Err(format_error(format!(
                    "mod {} file {} source SHA-256 differs",
                    self.manifest.id, entry.slot
                )));
            }
        }
        let at = self.payload_start + entry.offset as usize;
        let bytes = &self.bytes[at..at + entry.length as usize];
        let result = match entry.kind {
            PayloadKind::Delta => delta::apply(source, bytes)?,
            PayloadKind::Authored => bytes.to_vec(),
        };
        if result.len() != entry.result_size as usize || hash(&result) != entry.result_sha256 {
            return Err(format_error(format!(
                "mod {} file {} result size/SHA-256 differs",
                self.manifest.id, entry.slot
            )));
        }
        Ok(result)
    }
}

#[derive(Debug, Clone, Default)]
pub struct PackageSet {
    pub(super) packages: Vec<Package>,
}
impl PackageSet {
    pub fn new(packages: Vec<Package>) -> Result<Self> {
        if packages.len() > MAX_PACKAGES
            || packages.iter().map(|p| p.bytes.len()).sum::<usize>() > MAX_TOTAL_BYTES
        {
            return Err(format_error("mod package set exceeds bounds"));
        }
        let mut loaded = BTreeSet::new();
        let mut owners: BTreeMap<(&str, u32), Vec<&str>> = BTreeMap::new();
        let mut total = packages.iter().map(|p| p.bytes.len()).sum::<usize>();
        for package in &packages {
            let m = package.manifest();
            if loaded.contains(m.id.as_str()) {
                return Err(format_error(format!("duplicate mod id {}", m.id)));
            }
            for id in m.requires.iter().chain(&m.overrides) {
                if !loaded.contains(id.as_str()) {
                    return Err(format_error(format!(
                        "mod {} requires/overrides {id} before it (missing, cycle or wrong order)",
                        m.id
                    )));
                }
            }
            for file in &m.files {
                total = total
                    .checked_add(file.result_size as usize)
                    .ok_or_else(|| format_error("mod total result size overflow"))?;
                if total > MAX_TOTAL_BYTES {
                    return Err(format_error("mod packages and results exceed 128 MiB"));
                }
                let previous = owners.entry((&file.disc, file.slot)).or_default();
                for owner in previous.iter() {
                    if !m.overrides.iter().any(|id| id == owner) {
                        return Err(format_error(format!(
                            "mod {} file {} conflicts with {owner} without explicit override",
                            m.id, file.slot
                        )));
                    }
                }
                previous.push(&m.id);
            }
            loaded.insert(m.id.as_str());
        }
        Ok(Self { packages })
    }
    pub fn identities(&self) -> Vec<String> {
        self.packages.iter().map(Package::identity).collect()
    }
    pub fn is_empty(&self) -> bool {
        self.packages.is_empty()
    }
    pub fn from_bytes(bytes: &[u8]) -> Result<Self> {
        if bytes.len() > MAX_TOTAL_BYTES + 4 * MAX_PACKAGES + 12 {
            return Err(format_error("mod set exceeds 128 MiB"));
        }
        let mut reader = Reader::new(bytes);
        if reader.take(8)? != SET_MAGIC {
            return Err(format_error("unknown mod package-set format"));
        }
        let count = reader.word()? as usize;
        if count > MAX_PACKAGES {
            return Err(format_error("too many mod packages"));
        }
        let mut packages = Vec::with_capacity(count);
        for _ in 0..count {
            let len = reader.word()? as usize;
            packages.push(Package::decode(reader.take(len)?)?);
        }
        if !reader.done() {
            return Err(format_error("trailing mod package-set bytes"));
        }
        Self::new(packages)
    }
    pub fn to_bytes(&self) -> Vec<u8> {
        let mut out = SET_MAGIC.to_vec();
        out.extend_from_slice(&(self.packages.len() as u32).to_le_bytes());
        for p in &self.packages {
            out.extend_from_slice(&(p.bytes.len() as u32).to_le_bytes());
            out.extend_from_slice(&p.bytes);
        }
        out
    }
    pub fn preparation(&self, id: &Identification) -> Result<super::Preparation> {
        super::Preparation::new(self.clone(), id)
    }
}
