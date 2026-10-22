//! Host-side package binding for ordinary memory cards. Original card bytes
//! remain unchanged inside the envelope; vanilla cards remain raw .mcd files.
use crate::devices::card::{CARD_SIZE, CardStorage, formatted};
use crate::snapshot::{StateIdentity, digest};
use serde::{Deserialize, Serialize};
use std::io;

const MAGIC: &[u8; 8] = b"XEMSAVE1";
const MAX_METADATA: usize = 16 << 10;
pub const MAX_CARD_ARCHIVE: usize = CARD_SIZE + MAX_METADATA + 44;

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SaveBinding {
    pub disc: String,
    pub packages: Vec<String>,
}
impl From<&StateIdentity> for SaveBinding {
    fn from(identity: &StateIdentity) -> Self {
        Self {
            disc: identity.disc.clone(),
            packages: identity.packages.clone(),
        }
    }
}

fn archive(bytes: &[u8]) -> Result<Option<(SaveBinding, &[u8])>, String> {
    if !bytes.starts_with(MAGIC) {
        return Ok(None);
    }
    if bytes.len() < 44 || bytes.len() > MAX_CARD_ARCHIVE {
        return Err("invalid package-bound card length".into());
    }
    let length = u32::from_le_bytes(bytes[8..12].try_into().unwrap()) as usize;
    if length > MAX_METADATA || bytes.len() != 12 + length + CARD_SIZE + 32 {
        return Err("invalid package-bound card metadata/card length".into());
    }
    let sealed = bytes.len() - 32;
    if digest(&bytes[..sealed]).as_slice() != &bytes[sealed..] {
        return Err("package-bound card checksum mismatch".into());
    }
    let binding: SaveBinding = serde_json::from_slice(&bytes[12..12 + length])
        .map_err(|e| format!("package-bound card metadata: {e}"))?;
    if binding.packages.is_empty() {
        return Err("package-bound card must declare its packages".into());
    }
    Ok(Some((binding, &bytes[12 + length..sealed])))
}

/// Structural check before storage when no session identity exists yet.
pub fn validate_card_image(bytes: &[u8]) -> Result<(), String> {
    if archive(bytes)?.is_none() && bytes.len() != CARD_SIZE {
        return Err("memory card image must be exactly 128 KiB or a package-bound card".into());
    }
    Ok(())
}

/// Legacy card/save imports are accepted only by a vanilla session. Their
/// format is still decoded by card_file, rather than changed here.
pub fn unwrap_card(bytes: &[u8], expected: &SaveBinding) -> Result<Vec<u8>, String> {
    match archive(bytes)? {
        Some((binding, raw)) if &binding == expected => Ok(raw.to_vec()),
        Some((binding, _)) => Err(format!("card package identity mismatch: stored {binding:?}; active {expected:?}")),
        None if expected.packages.is_empty() => Ok(bytes.to_vec()),
        None => Err("untagged original card/save cannot be loaded into a modded session; use a matching package-bound card or a fresh card".into()),
    }
}

/// Explicit file imports retain the original card-file bound. The larger
/// allowance exists only for a recognised package envelope, not raw formats.
pub fn unwrap_card_file(bytes: &[u8], expected: &SaveBinding) -> Result<Vec<u8>, String> {
    if !bytes.starts_with(MAGIC) && bytes.len() > crate::devices::card_file::MAX_FILE {
        return Err("original card/save file exceeds size limit".into());
    }
    unwrap_card(bytes, expected)
}

pub fn wrap_card(raw: &[u8], binding: &SaveBinding) -> Result<Vec<u8>, String> {
    if raw.len() != CARD_SIZE {
        return Err("card payload must be exactly 128 KiB".into());
    }
    if binding.packages.is_empty() {
        return Ok(raw.to_vec());
    }
    let metadata = serde_json::to_vec(binding).map_err(|e| e.to_string())?;
    if metadata.len() > MAX_METADATA {
        return Err("card identity metadata exceeds 16 KiB".into());
    }
    let mut out = Vec::with_capacity(12 + metadata.len() + raw.len() + 32);
    out.extend_from_slice(MAGIC);
    out.extend_from_slice(&(metadata.len() as u32).to_le_bytes());
    out.extend_from_slice(&metadata);
    out.extend_from_slice(raw);
    let hash = digest(&out);
    out.extend_from_slice(&hash);
    Ok(out)
}

/// Validate identity before Card::attach_storage (whose ordinary I/O errors
/// intentionally leave a slot empty). The immutable binding wraps every write
/// at its origin, so a queued write cannot acquire a later session's identity.
pub struct BoundCardStorage {
    inner: Box<dyn CardStorage>,
    binding: SaveBinding,
    loaded: [Option<io::Result<Option<Vec<u8>>>>; 2],
}
impl BoundCardStorage {
    pub fn prepare(mut inner: Box<dyn CardStorage>, binding: SaveBinding) -> Result<Self, String> {
        let mut loaded = [None, None];
        for (slot, entry) in loaded.iter_mut().enumerate() {
            *entry = Some(match inner.load(slot) {
                Ok(Some(bytes)) => Ok(Some(
                    unwrap_card(&bytes, &binding).map_err(|e| format!("card {}: {e}", slot + 1))?,
                )),
                Ok(None) => Ok(Some(formatted())),
                Err(error) => Err(error),
            });
        }
        Ok(Self {
            inner,
            binding,
            loaded,
        })
    }
}
impl CardStorage for BoundCardStorage {
    fn load(&mut self, slot: usize) -> io::Result<Option<Vec<u8>>> {
        self.loaded
            .get_mut(slot)
            .and_then(Option::take)
            .ok_or_else(|| io::Error::other("bound card storage loaded more than once"))?
    }
    fn store(&mut self, slot: usize, image: &[u8]) -> io::Result<()> {
        let bytes = wrap_card(image, &self.binding).map_err(io::Error::other)?;
        self.inner.store(slot, &bytes)
    }
}
