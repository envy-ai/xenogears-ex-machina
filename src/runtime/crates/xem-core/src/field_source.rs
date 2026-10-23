//! Read-only observations at original field-load/decompression seams.
//! This bounded host metadata is neither game state nor snapshot/replay input.
use crate::memory::GameMemory;
use std::collections::BTreeMap;
use xem_disc::{
    KnownDisc,
    field_context::{self as facts, ComponentContext, SourceContext, SourceIdentity},
    packed,
};

#[derive(Default)]
pub struct FieldSources {
    current: Option<Load>,
    ready: Option<(SourceContext, u64)>,
    failure: Option<String>,
}
struct Load {
    bytes: Vec<u8>,
    identity: SourceIdentity,
    root: u32,
    allocation: u32,
    vblank: u64,
    media: u64,
    components: BTreeMap<usize, ComponentContext>,
    pending: Option<(ComponentContext, Vec<u8>)>,
}
fn read(memory: &dyn GameMemory, address: u32, size: usize) -> Result<Vec<u8>, String> {
    facts::ram(address, size)?;
    let mut bytes = vec![0; size];
    memory
        .read(address, &mut bytes)
        .map_err(|e| e.to_string())?;
    Ok(bytes)
}
impl FieldSources {
    pub fn clear(&mut self) {
        *self = Self::default();
    }
    pub fn invalidate(&mut self, error: String) {
        self.clear();
        self.failure = Some(error);
    }
    /// Observer errors invalidate publication and never trap/change execution.
    pub fn observe(
        &mut self,
        r: [u32; 8],
        disc: u8,
        media: u64,
        vblank: u64,
        memory: &dyn GameMemory,
    ) {
        if r[0] == 0 {
            self.clear();
        }
        if let Err(error) = self.advance(r, disc, media, vblank, memory) {
            self.current = None;
            self.ready = None;
            self.failure = Some(error);
        }
    }
    pub fn query(
        &self,
        map: u32,
        source: &str,
        module: &str,
        disc: u8,
        media: u64,
    ) -> Result<SourceContext, String> {
        if let Some(error) = &self.failure {
            return Err(format!("field source observation failed: {error}"));
        }
        let (context,generation)=self.ready.as_ref().ok_or("no complete original field-load observation; load the selected map in a vanilla runtime")?;
        if context.identity.map != map
            || context.identity.source_sha256 != source
            || context.identity.disc != disc
            || *generation != media
            || !facts::digest(module)
        {
            return Err("field source observation is stale or differs from the selected map/disc/source/module".into());
        }
        let mut result = context.clone();
        result.module_sha256 = module.into();
        Ok(result)
    }
    fn advance(
        &mut self,
        r: [u32; 8],
        disc: u8,
        media: u64,
        vblank: u64,
        memory: &dyn GameMemory,
    ) -> Result<(), String> {
        let [
            phase,
            root,
            allocation,
            map,
            slot,
            entry,
            source,
            destination,
        ] = r;
        if phase == 0 {
            let known = match disc {
                1 => KnownDisc::Disc1,
                2 => KnownDisc::Disc2,
                _ => return Err("field observation requires a known original disc".into()),
            };
            let raw = read(memory, entry, 7)?;
            let size = i32::from_le_bytes(raw[3..7].try_into().map_err(|_| "index size")?);
            if size < 0x154
                || size as usize > facts::MAX_FILE
                || allocation as usize != ((size as usize + 3) & !3)
                || map > 0xfff
                || slot > 0xffff
            {
                return Err("field load index/allocation/map bounds differ from the original word-rounded file contract".into());
            }
            let bytes = read(memory, root, size as usize)?;
            let identity = SourceIdentity {
                disc,
                boot_sha256: xem_disc::identify::hex(&known.executable_sha256()),
                map,
                slot,
                lba: u32::from(raw[0]) | (u32::from(raw[1]) << 8) | (u32::from(raw[2]) << 16),
                indexed_size: size as u32,
                source_sha256: facts::hash(&bytes),
            };
            identity.validate(&bytes)?;
            self.current = Some(Load {
                bytes,
                identity,
                root,
                allocation,
                vblank,
                media,
                components: BTreeMap::new(),
                pending: None,
            });
            return Ok(());
        }
        let load = self
            .current
            .as_mut()
            .ok_or("component observation outside a complete load generation")?;
        if root != load.root
            || allocation != load.allocation
            || map != load.identity.map
            || slot != load.identity.slot
            || disc != load.identity.disc
            || media != load.media
        {
            return Err("field source/allocation/media changed during load".into());
        }
        match phase {
            1 => {
                if load.pending.is_some() {
                    return Err("nested/unfinished field decompression observation".into());
                }
                if read(memory, root, load.bytes.len())? != load.bytes {
                    return Err("indexed source bytes changed during original field load".into());
                }
                let offset = source
                    .checked_sub(root)
                    .ok_or("component source precedes load root")?
                    as usize;
                let matches = (0..9)
                    .filter(|&i| facts::word(&load.bytes, 0x130 + i * 4).ok() == Some(offset))
                    .collect::<Vec<_>>();
                if matches.len() != 1 || offset < 0x154 || offset >= load.bytes.len() {
                    return Err("ambiguous/out-of-range component source descriptor".into());
                }
                let index = matches[0];
                if load.components.contains_key(&index) {
                    return Err("duplicate component in original load observation".into());
                }
                let logical = facts::word(&load.bytes, 0x10c + index * 4)?;
                if logical > facts::MAX_COMPONENT - 16 {
                    return Err("component allocation exceeds observation bound".into());
                }
                let available = (load.bytes.len() - offset + facts::MAX_EXTRA)
                    .min((facts::RAM_END - source) as usize);
                let input = read(memory, source, available)?;
                let decoded = packed::decode_block(&input, logical + 16)
                    .map_err(|e| format!("component {index} measured RAM: {e}"))?;
                if decoded.data.len() < logical {
                    return Err("original component is shorter than its logical descriptor".into());
                }
                facts::ram(destination, decoded.data.len())?;
                let extra_at = load.bytes.len() - offset;
                let extra = if decoded.source_bytes_read > extra_at {
                    input[extra_at..decoded.source_bytes_read].to_vec()
                } else {
                    Vec::new()
                };
                let component = ComponentContext {
                    index,
                    source_offset: offset,
                    logical_size: logical,
                    packed_size: facts::word(&input, 0)?,
                    source_bytes_read: decoded.source_bytes_read,
                    output_size: decoded.data.len(),
                    output_sha256: facts::hash(&decoded.data),
                    destination_address: destination,
                    extra,
                };
                load.pending = Some((component, decoded.data));
            }
            2 => {
                let (component, expected) = load
                    .pending
                    .take()
                    .ok_or("output observation without original input")?;
                if root.checked_add(component.source_offset as u32) != Some(source)
                    || destination != component.destination_address
                {
                    return Err(
                        "decompressor source/destination changed between observations".into(),
                    );
                }
                if read(memory, destination, expected.len())? != expected {
                    return Err(format!(
                        "component {} original runtime output differs from strict decoder",
                        component.index
                    ));
                }
                load.components.insert(component.index, component);
            }
            3 => {
                if load.pending.is_some() || load.components.len() != 9 {
                    return Err("incomplete nine-component original field load".into());
                }
                let mut context = SourceContext {
                    schema_version: 1,
                    provenance: "runtime_field_decompress".into(),
                    module_sha256: "0".repeat(64),
                    identity: load.identity.clone(),
                    source_address: root,
                    allocation_size: allocation,
                    load_vblank: load.vblank.to_string(),
                    components: load.components.values().cloned().collect(),
                };
                context.guard(&load.bytes, &load.identity, &context.module_sha256)?;
                context.module_sha256.clear();
                self.ready = Some((context, media));
                self.current = None;
            }
            _ => return Err("unknown field observation phase".into()),
        }
        Ok(())
    }
}
