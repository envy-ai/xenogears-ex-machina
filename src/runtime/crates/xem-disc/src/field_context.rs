//! Measured field-loader source memory, shared by runtime observation and Studio.
use crate::{KnownDisc, packed};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

pub const MAX_FILE: usize = 32 << 20;
pub const MAX_COMPONENT: usize = 2 << 20;
pub const MAX_EXTRA: usize = 64 << 10;
pub const RAM_START: u32 = 0x8000_0000;
pub const RAM_END: u32 = 0x8020_0000;

pub fn hash(bytes: &[u8]) -> String {
    crate::identify::hex(&Sha256::digest(bytes))
}
pub fn word(bytes: &[u8], at: usize) -> Result<usize, String> {
    let raw = bytes
        .get(at..at.checked_add(4).ok_or("word range overflow")?)
        .ok_or("truncated field descriptor")?;
    Ok(u32::from_le_bytes(raw.try_into().map_err(|_| "invalid word")?) as usize)
}
pub fn ram(address: u32, size: usize) -> Result<(), String> {
    if address < RAM_START
        || u64::from(address)
            .checked_add(size as u64)
            .is_none_or(|end| end > u64::from(RAM_END))
    {
        return Err("field observation range is outside original main RAM".into());
    }
    Ok(())
}
pub fn digest(value: &str) -> bool {
    value.len() == 64
        && value
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SourceIdentity {
    pub disc: u8,
    pub boot_sha256: String,
    pub map: u32,
    pub slot: u32,
    pub lba: u32,
    pub indexed_size: u32,
    pub source_sha256: String,
}
impl SourceIdentity {
    pub fn validate(&self, bundle: &[u8]) -> Result<(), String> {
        let disc = match self.disc {
            1 => KnownDisc::Disc1,
            2 => KnownDisc::Disc2,
            _ => return Err("context requires a known original disc".into()),
        };
        if u64::from(self.lba) + u64::from(self.indexed_size).div_ceil(2048)
            > disc.track_bytes() / 2352
        {
            return Err("context index range exceeds original disc".into());
        }
        if self.boot_sha256 != crate::identify::hex(&disc.executable_sha256())
            || self.map > 0xfff
            || self.slot > 0xffff
            || self.lba == 0
        {
            return Err("invalid original disc/map/index identity".into());
        }
        if bundle.len() < 0x154
            || bundle.len() > MAX_FILE
            || self.indexed_size as usize != bundle.len()
            || self.source_sha256 != hash(bundle)
        {
            return Err("context source size/hash differs from the indexed original bundle".into());
        }
        Ok(())
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ComponentContext {
    pub index: usize,
    pub source_offset: usize,
    pub logical_size: usize,
    pub packed_size: usize,
    pub source_bytes_read: usize,
    pub output_size: usize,
    pub output_sha256: String,
    pub destination_address: u32,
    /// Exact measured bytes consumed beyond indexed EOF, not sector padding.
    pub extra: Vec<u8>,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SourceContext {
    pub schema_version: u32,
    pub provenance: String,
    pub module_sha256: String,
    pub identity: SourceIdentity,
    pub source_address: u32,
    pub allocation_size: u32,
    pub load_vblank: String,
    pub components: Vec<ComponentContext>,
}
impl SourceContext {
    /// Validate complete original output observations and return the consistent
    /// measured guard to materialize before appending authored packed bytes.
    pub fn guard(
        &self,
        bundle: &[u8],
        expected: &SourceIdentity,
        module: &str,
    ) -> Result<Vec<u8>, String> {
        expected.validate(bundle)?;
        if &self.identity != expected
            || self.schema_version != 1
            || self.provenance != "runtime_field_decompress"
            || !digest(module)
            || self.module_sha256 != module
        {
            return Err("field source context identity/module/provenance mismatch".into());
        }
        let tick = self
            .load_vblank
            .parse::<u64>()
            .map_err(|_| "invalid context load time")?;
        if self.load_vblank != tick.to_string() {
            return Err("noncanonical context load time".into());
        }
        let aligned = (bundle.len() + 3) & !3;
        if self.allocation_size as usize != aligned {
            return Err("context allocation differs from word-rounded indexed size".into());
        }
        ram(self.source_address, aligned)?;
        if self.components.len() != 9 {
            return Err("context must contain all nine original components".into());
        }
        let mut seen = [false; 9];
        let mut guard = Vec::<u8>::new();
        for c in &self.components {
            if c.index >= 9 || seen[c.index] {
                return Err("duplicate/out-of-range context component".into());
            }
            seen[c.index] = true;
            let size = word(bundle, 0x10c + c.index * 4)?;
            let offset = word(bundle, 0x130 + c.index * 4)?;
            if size > MAX_COMPONENT - 16
                || offset < 0x154
                || offset >= bundle.len()
                || c.logical_size != size
                || c.source_offset != offset
                || c.extra.len() > MAX_EXTRA
            {
                return Err(format!(
                    "component {} context descriptor/bound mismatch",
                    c.index
                ));
            }
            ram(
                self.source_address,
                c.source_offset
                    .checked_add(c.source_bytes_read)
                    .ok_or("context source range overflow")?,
            )?;
            ram(c.destination_address, c.output_size)?;
            let mut input = bundle[offset..].to_vec();
            input.extend_from_slice(&c.extra);
            let decoded = packed::decode_block(&input, size + 16)
                .map_err(|e| format!("component {} measured source: {e}", c.index))?;
            let consumed = offset
                .checked_add(decoded.source_bytes_read)
                .ok_or("context consumption overflow")?;
            if c.extra.len() != consumed.saturating_sub(bundle.len())
                || c.source_bytes_read != decoded.source_bytes_read
                || c.packed_size != word(&input, 0)?
                || c.output_size != decoded.data.len()
                || c.output_size < size
                || c.output_sha256 != hash(&decoded.data)
            {
                return Err(format!(
                    "component {} complete output/consumption differs from original runtime observation",
                    c.index
                ));
            }
            for (i, &byte) in c.extra.iter().enumerate() {
                if let Some(&prior) = guard.get(i) {
                    if prior != byte {
                        return Err("original load observations conflict beyond indexed EOF".into());
                    }
                } else {
                    guard.push(byte);
                }
            }
        }
        // All nine decoders must also agree when the individually measured
        // observations are represented by one actual contiguous source guard.
        let mut measured = bundle.to_vec();
        measured.extend_from_slice(&guard);
        self.verify_outputs(&measured, None)?;
        Ok(guard)
    }
    pub fn verify_outputs(&self, bundle: &[u8], edited: Option<usize>) -> Result<(), String> {
        if bundle.len() > MAX_FILE
            || self.components.len() != 9
            || edited.is_some_and(|index| index >= 9)
        {
            return Err("result/context component bounds differ".into());
        }
        let mut seen = [false; 9];
        for c in &self.components {
            if c.index >= 9 || seen[c.index] {
                return Err("duplicate/out-of-range context component".into());
            }
            seen[c.index] = true;
            if Some(c.index) == edited {
                continue;
            }
            let offset = word(bundle, 0x130 + c.index * 4)?;
            let size = word(bundle, 0x10c + c.index * 4)?;
            if size > MAX_COMPONENT - 16 || c.output_size > MAX_COMPONENT || offset < 0x154 {
                return Err("result component exceeds descriptor/output bounds".into());
            }
            let source = bundle
                .get(offset..)
                .ok_or("component source outside result")?;
            let decoded = packed::decode_block(
                source,
                size.checked_add(16).ok_or("component size overflow")?,
            )
            .map_err(|e| format!("component {} result: {e}", c.index))?;
            if size != c.logical_size
                || offset != c.source_offset
                || decoded.data.len() != c.output_size
                || hash(&decoded.data) != c.output_sha256
            {
                return Err(format!(
                    "complete original runtime output changed for component {}",
                    c.index
                ));
            }
        }
        Ok(())
    }
}
