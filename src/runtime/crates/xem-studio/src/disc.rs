//! Read-only, bounded disc sessions and the original map-bundle event loader.

use std::cell::RefCell;
use std::collections::BTreeMap;
use std::rc::Rc;

use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use xem_disc::{BinSource, ChdSource, Error, Index, PrefetchedFile, ReadAt, Record, SectorSource};

use super::{StudioError, packed};

pub const CHUNK_BYTES: usize = 256 << 10;
pub const CHUNK_BUDGET: usize = 8 << 20;
pub const HUNK_BUDGET: usize = 4 << 20;
const READ_SECTORS: usize = 64;
const MAX_BUNDLE_BYTES: usize = 32 << 20;
const MAX_COMPONENT_BYTES: usize = 2 << 20;
const HEADER: usize = 0x154;
const MAP_FILE: u32 = 0xb8;
const FORM1_BYTES: usize = xem_disc::sector::FORM1_DATA_SIZE;
const MAX_SAFE_INTEGER: u64 = 9_007_199_254_740_991;

#[derive(Debug, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum OpenArgs {
    Native { path: String },
    Pull { name: String, bytes: u64 },
}

#[derive(Clone, Debug)]
pub struct MapRecord {
    pub map: u32,
    pub file: u32,
    pub slot: usize,
    pub record: Record,
}

/// The record before the first bundle counts bundle/companion pairs; this is
/// deliberately not the directory's ordinary file-zero marker.
pub fn map_registry(index: &Index) -> Result<Vec<MapRecord>, StudioError> {
    let first = index
        .slot(4, MAP_FILE)
        .ok_or_else(|| invalid_index("missing field map directory"))?;
    let marker = first
        .checked_sub(1)
        .and_then(|slot| index.record(slot))
        .ok_or_else(|| invalid_index("missing map directory marker"))?;
    let count = marker
        .directory_len()
        .ok_or_else(|| invalid_index("map directory marker has no negative file count"))?;
    if count % 2 != 0 {
        return Err(invalid_index(
            "map directory file count is not bundle/companion pairs",
        ));
    }
    let end = first
        .checked_add(count as usize)
        .ok_or_else(|| invalid_index("map directory length overflow"))?;
    if end > index.records().len() {
        return Err(invalid_index("map directory extends past the index"));
    }
    (0..count / 2)
        .map(|map| {
            let file = MAP_FILE + 2 * map;
            let slot = index
                .slot(4, file)
                .ok_or_else(|| invalid_index("map slot outside index"))?;
            let record = index
                .record(slot)
                .ok_or_else(|| invalid_index("missing map record"))?;
            if record.size < 0 || record.is_unused() {
                return Err(invalid_index(format!("map {map} has no file record")));
            }
            if record.size as usize > MAX_BUNDLE_BYTES {
                return Err(invalid_index(format!(
                    "map {map} bundle exceeds {MAX_BUNDLE_BYTES} bytes"
                )));
            }
            Ok(MapRecord {
                map,
                file,
                slot,
                record,
            })
        })
        .collect()
}

fn invalid_index(message: impl Into<String>) -> StudioError {
    StudioError::new("invalid_disc_index", message)
}

#[derive(Clone, Debug, Serialize)]
pub struct EventComponent {
    /// The complete decoded stream, including the measured retained tail.
    pub bytes: Vec<u8>,
    pub declared_size: usize,
    pub bytecode_offset: usize,
    /// The declared boundary relative to the bytecode, as Python reports it.
    pub component_end: usize,
    pub actor_count: usize,
}

/// `events.py::map_events` / `field.py::event_package`, without an instruction
/// walk. Files shorter than the original bundle header are placeholders.
pub fn extract_events(bundle: &[u8]) -> Result<Option<EventComponent>, StudioError> {
    if bundle.len() < HEADER {
        return Ok(None);
    }
    let declared_size = u32_at(bundle, 0x10c + 4 * 5)? as usize;
    let offset = u32_at(bundle, 0x130 + 4 * 5)? as usize;
    let limit = declared_size
        .checked_add(16)
        .ok_or_else(|| StudioError::new("invalid_header", "event allocation size overflow"))?;
    if limit > MAX_COMPONENT_BYTES {
        return Err(StudioError::new(
            "invalid_header",
            format!("event allocation exceeds {MAX_COMPONENT_BYTES} bytes"),
        ));
    }
    // Python slicing past the bundle gives an empty source, which reports the
    // missing packed length. It does not invent bytes adjacent to the bundle.
    let source = bundle.get(offset..).unwrap_or(&[]);
    let bytes = packed::decode_block(source, limit)
        .map_err(packed_error)?
        .data;
    let actor_count = u32_at(&bytes, 128)? as usize;
    let bytecode_offset = actor_count
        .checked_mul(64)
        .and_then(|n| n.checked_add(132))
        .ok_or_else(|| StudioError::new("invalid_header", "actor event table size overflow"))?;
    if bytecode_offset > bytes.len() {
        return Err(StudioError::new(
            "truncated_input",
            "actor event table extends past event stream",
        ));
    }
    let code_length = bytes.len() - bytecode_offset;
    if code_length > 65536 {
        return Err(StudioError::new(
            "invalid_header",
            "bytecode exceeds the original u16 PC address space",
        ));
    }
    if declared_size < bytecode_offset || declared_size > bytes.len() {
        return Err(StudioError::new(
            "invalid_declared_size",
            "declared event bytecode is outside the decoded stream",
        ));
    }
    for (actor, row) in bytes[132..bytecode_offset].chunks_exact(64).enumerate() {
        for (event, raw) in row.chunks_exact(2).enumerate() {
            let pc = u16::from_le_bytes([raw[0], raw[1]]) as usize;
            if pc >= code_length {
                return Err(StudioError::new(
                    "entry_outside",
                    format!("actor {actor} event {event}: PC +0x{pc:x} outside bytecode"),
                ));
            }
        }
    }
    Ok(Some(EventComponent {
        bytes,
        declared_size,
        bytecode_offset,
        component_end: declared_size - bytecode_offset,
        actor_count,
    }))
}

fn u32_at(bytes: &[u8], offset: usize) -> Result<u32, StudioError> {
    let raw = bytes.get(offset..offset + 4).ok_or_else(|| {
        StudioError::new("truncated_input", format!("missing u32 at +0x{offset:x}"))
    })?;
    Ok(u32::from_le_bytes(raw.try_into().expect("four bytes")))
}

fn packed_error(error: packed::PackedError) -> StudioError {
    let kind = match &error {
        packed::PackedError::MissingLength | packed::PackedError::Truncated { .. } => {
            "packed_truncated"
        }
        packed::PackedError::OutputLimit { .. } => "packed_output_limit",
        packed::PackedError::InvalidDistance { .. } => "packed_invalid_distance",
        packed::PackedError::CopyCrossesOutput { .. } => "packed_copy_crosses_output",
        packed::PackedError::FlagGroupCrossesOutput { .. } => "packed_group_crosses_output",
    };
    StudioError::new(kind, error.to_string())
}

fn disc_error(error: Error) -> StudioError {
    StudioError::new("disc_read", error.to_string())
}

#[derive(Clone, Debug, Serialize)]
struct MapSummary {
    map: u32,
    file: u32,
    slot: usize,
    placeholder: bool,
    actor_count: Option<usize>,
    script_size: Option<usize>,
    stream_size: Option<usize>,
}

/// One bounded, incremental Form 1 file read. Completed slices survive missing
/// ranges and cache eviction; no retry restarts a large bundle from its start.
struct BundleRead {
    map: u32,
    record: Record,
    bytes: Vec<u8>,
}

impl BundleRead {
    fn new(map: &MapRecord) -> Self {
        Self {
            map: map.map,
            record: map.record,
            bytes: Vec::with_capacity(map.record.size as usize),
        }
    }

    fn advance(&mut self, source: &mut dyn SectorSource) -> xem_disc::Result<bool> {
        let size = self.record.size as usize;
        while self.bytes.len() < size {
            let count = (size - self.bytes.len()).min(READ_SECTORS * FORM1_BYTES);
            let lba = self
                .record
                .lba
                .checked_add((self.bytes.len() / FORM1_BYTES) as u32)
                .ok_or_else(|| Error::Format("bundle LBA overflow".into()))?;
            match xem_disc::read_data(source, lba, count) {
                Ok(bytes) => self.bytes.extend_from_slice(&bytes),
                Err(Error::NotReady) => return Ok(false),
                Err(error) => return Err(error),
            }
        }
        Ok(true)
    }
}

type SharedFile = Rc<RefCell<PrefetchedFile>>;

#[derive(Clone, Copy, PartialEq, Eq)]
enum Stage {
    Detect,
    Open,
    Identify,
    Index,
    Ready,
}

struct Session {
    name: String,
    bytes: u64,
    file: Option<SharedFile>,
    chd: bool,
    stage: Stage,
    source: Option<Box<dyn SectorSource>>,
    identity: Option<Value>,
    failure: Option<(&'static str, String)>,
    registry: Vec<MapRecord>,
    summaries: Vec<MapSummary>,
    map_read: Option<BundleRead>,
    event_read: Option<BundleRead>,
}

impl Session {
    fn new(
        name: String,
        bytes: u64,
        file: Option<SharedFile>,
        source: Option<Box<dyn SectorSource>>,
    ) -> Self {
        let stage = if source.is_some() {
            Stage::Identify
        } else {
            Stage::Detect
        };
        Self {
            name,
            bytes,
            file,
            chd: false,
            stage,
            source,
            identity: None,
            failure: None,
            registry: Vec::new(),
            summaries: Vec::new(),
            map_read: None,
            event_read: None,
        }
    }

    fn missing(&mut self) -> Vec<[u64; 2]> {
        self.file
            .as_ref()
            .map(|file| {
                file.borrow_mut()
                    .take_missing()
                    .into_iter()
                    .map(|r| [r.start, r.end])
                    .collect()
            })
            .unwrap_or_default()
    }

    fn check_failure(&self) -> Result<(), StudioError> {
        if let Some((kind, message)) = &self.failure {
            return Err(StudioError::new(kind, message.clone()));
        }
        Ok(())
    }

    fn require_ready(&self) -> Result<(), StudioError> {
        self.check_failure()?;
        if self.stage != Stage::Ready {
            return Err(StudioError::new(
                "disc_not_ready",
                "poll the disc and fill its missing ranges before reading maps",
            ));
        }
        Ok(())
    }

    fn status(&mut self, session: u32) -> Value {
        let phase = match self.stage {
            Stage::Detect => "detecting",
            Stage::Open => "opening",
            Stage::Identify => "identifying",
            Stage::Index => "indexing",
            Stage::Ready => "ready",
        };
        json!({
            "session": session, "name": self.name, "bytes": self.bytes,
            "state": if self.stage == Stage::Ready { "ready" } else { "pending" },
            "phase": phase, "identity": self.identity,
            "missing": self.missing(), "chunk_bytes": CHUNK_BYTES,
            "chunk_budget": CHUNK_BUDGET, "hunk_budget": HUNK_BUDGET,
            "resident_bytes": self.file.as_ref().map_or(0, |file| file.borrow().resident_bytes()),
        })
    }

    fn advance(&mut self) -> Result<(), StudioError> {
        self.check_failure()?;
        loop {
            match self.stage {
                Stage::Detect => {
                    let mut magic = [0; 8];
                    let mut file = self.file.as_ref().expect("pull file").clone();
                    if file.len() >= 8 {
                        match file.read_at(0, &mut magic).map_err(Error::from) {
                            Ok(()) => {}
                            Err(Error::NotReady) => return Ok(()),
                            Err(error) => return Err(disc_error(error)),
                        }
                    }
                    self.chd = &magic == b"MComprHD";
                    self.stage = Stage::Open;
                }
                Stage::Open => {
                    let file = self.file.as_ref().expect("pull file").clone();
                    let result = if self.chd {
                        ChdSource::open(file, HUNK_BUDGET)
                            .map(|source| Box::new(source) as Box<dyn SectorSource>)
                    } else {
                        BinSource::whole(file)
                            .map(|source| Box::new(source) as Box<dyn SectorSource>)
                    };
                    match result {
                        Ok(source) => self.source = Some(source),
                        Err(Error::NotReady) => return Ok(()),
                        Err(error) => return Err(disc_error(error)),
                    }
                    self.stage = Stage::Identify;
                }
                Stage::Identify => {
                    let source = self.source.as_mut().expect("opened source");
                    let result = bounded_identity_layout(source.as_mut())
                        .and_then(|()| xem_disc::identify(source.as_mut()));
                    let id = match result {
                        Ok(id) => id,
                        Err(Error::NotReady) => return Ok(()),
                        Err(error) => return Err(disc_error(error)),
                    };
                    let test_fixture = fixture_identity(&id);
                    if id.disc.is_none() && !test_fixture {
                        return Err(StudioError::new(
                            "unknown_disc",
                            format!(
                                "unrecognized boot executable {} (SHA-256 {})",
                                id.boot_path,
                                xem_disc::identify::hex(&id.sha256)
                            ),
                        ));
                    }
                    // A field-only package leaves the other known game disc
                    // untouched, but must still declare it for native changer
                    // preparation. Actual image validation remains unchanged.
                    let package_discs = if id.disc.is_some() {
                        [xem_disc::KnownDisc::Disc1, xem_disc::KnownDisc::Disc2]
                            .into_iter()
                            .map(|disc| json!({"serial":disc.serial(),"sha256":xem_disc::identify::hex(&disc.executable_sha256())}))
                            .collect::<Vec<_>>()
                    } else {
                        Vec::new()
                    };
                    self.identity = Some(json!({
                        "disc": id.disc.map(|disc| disc.number()),
                        "serial": id.disc.map(|disc| disc.serial()).or(test_fixture.then_some("SLUS-99999")),
                        "boot_path": id.boot_path, "boot_lba": id.boot_lba,
                        "boot_sha256": xem_disc::identify::hex(&id.sha256),
                        "executable_bytes": id.executable.len(), "test_fixture": test_fixture,
                        "package_discs": package_discs,
                    }));
                    self.stage = Stage::Index;
                }
                Stage::Index => {
                    let source = self.source.as_mut().expect("identified source");
                    let index = match Index::from_disc(source.as_mut()) {
                        Ok(index) => index,
                        Err(Error::NotReady) => return Ok(()),
                        Err(error) => return Err(disc_error(error)),
                    };
                    self.registry = map_registry(&index)?;
                    self.stage = Stage::Ready;
                }
                Stage::Ready => return Ok(()),
            }
        }
    }
}

/// Check sizes used by the synchronous identity reader before it allocates or
/// requests them. Known discs keep boot files in the root. These bounds fit a
/// single identification attempt inside the fixed pull-cache budget.
fn bounded_identity_layout(source: &mut dyn SectorSource) -> xem_disc::Result<()> {
    let pvd = xem_disc::read_data(source, 16, FORM1_BYTES)?;
    if &pvd[..6] != b"\x01CD001" {
        return Err(Error::Format(
            "no ISO9660 primary volume descriptor at sector 16".into(),
        ));
    }
    let root_size = u32::from_le_bytes(pvd[166..170].try_into().expect("four bytes"));
    if root_size > 256 << 10 {
        return Err(Error::Format(
            "ISO root directory exceeds 256 KiB identity bound".into(),
        ));
    }
    let cnf = xem_disc::identify::iso_find(source, "SYSTEM.CNF")?
        .ok_or_else(|| Error::Format("no SYSTEM.CNF in the ISO9660 root".into()))?;
    if cnf.is_directory || cnf.size > 64 << 10 {
        return Err(Error::Format(
            "SYSTEM.CNF is not a file within the 64 KiB identity bound".into(),
        ));
    }
    let text = xem_disc::read_data(source, cnf.lba, cnf.size as usize)?;
    let path = xem_disc::identify::boot_path(&String::from_utf8_lossy(&text))
        .ok_or_else(|| Error::Format("SYSTEM.CNF names no BOOT program".into()))?;
    if path.contains(['/', '\\']) {
        return Err(Error::Format(
            "boot executable must be in the ISO root directory".into(),
        ));
    }
    let boot = xem_disc::identify::iso_find(source, &path)?
        .ok_or_else(|| Error::Format(format!("boot program {path} not on the disc")))?;
    if boot.is_directory || boot.size > 2 << 20 {
        return Err(Error::Format(
            "boot executable is not a file within the 2 MiB identity bound".into(),
        ));
    }
    Ok(())
}

#[cfg(not(feature = "test-fixtures"))]
fn fixture_identity(_id: &xem_disc::Identification) -> bool {
    false
}

#[cfg(feature = "test-fixtures")]
fn fixture_identity(id: &xem_disc::Identification) -> bool {
    // Independently invented by tests/support/synthetic_disc.rs. A fixture
    // build accepts this exact executable only; no caller supplies identities.
    id.boot_path == "SLUS_999.99;1"
        && xem_disc::identify::hex(&id.sha256)
            == "dfe782b25d3e17220c592d2646d05ac78b3d42dd110d291a46c1f3d87c13ba6d"
}

/// The session handles belong to one Studio instance and are never reused.
#[derive(Default)]
pub struct DiscSessions {
    next: u32,
    sessions: BTreeMap<u32, Session>,
}

impl DiscSessions {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn open(&mut self, args: OpenArgs) -> Result<Value, StudioError> {
        let session = match args {
            OpenArgs::Pull { name, bytes } => {
                if bytes > MAX_SAFE_INTEGER {
                    return Err(StudioError::new(
                        "invalid_args",
                        "pull file length must be a safe JavaScript integer",
                    ));
                }
                let file = Rc::new(RefCell::new(PrefetchedFile::new(
                    bytes,
                    CHUNK_BYTES,
                    CHUNK_BUDGET,
                )));
                Session::new(name, bytes, Some(file), None)
            }
            OpenArgs::Native { path } => native_session(path)?,
        };
        let handle = self
            .next
            .checked_add(1)
            .ok_or_else(|| StudioError::new("session_limit", "disc session handles exhausted"))?;
        self.next = handle;
        self.sessions.insert(handle, session);
        let result = self.poll(handle);
        if result.is_err() {
            // Opening failed before the caller obtained a handle.
            self.sessions.remove(&handle);
        }
        result
    }

    fn session(&mut self, session: u32) -> Result<&mut Session, StudioError> {
        self.sessions.get_mut(&session).ok_or_else(|| {
            StudioError::new("unknown_session", format!("unknown disc session {session}"))
        })
    }

    pub fn poll(&mut self, session: u32) -> Result<Value, StudioError> {
        let state = self.session(session)?;
        if let Err(error) = state.advance() {
            state.failure = Some((error.kind, error.message.clone()));
            return Err(error);
        }
        Ok(state.status(session))
    }

    pub fn missing(&mut self, session: u32) -> Result<Value, StudioError> {
        let state = self.session(session)?;
        state.check_failure()?;
        Ok(json!({"session": session, "missing": state.missing()}))
    }

    pub fn fill(&mut self, session: u32, offset: u64, data: &[u8]) -> Result<Value, StudioError> {
        let state = self.session(session)?;
        state.check_failure()?;
        let file = state.file.as_ref().ok_or_else(|| {
            StudioError::new("invalid_args", "native sessions do not take range fills")
        })?;
        let end = offset
            .checked_add(data.len() as u64)
            .ok_or_else(|| StudioError::new("invalid_args", "range fill end overflow"))?;
        if offset > state.bytes
            || end > state.bytes
            || !offset.is_multiple_of(CHUNK_BYTES as u64)
            || (end != state.bytes && !data.len().is_multiple_of(CHUNK_BYTES))
            || data.len() > CHUNK_BUDGET
        {
            return Err(StudioError::new(
                "invalid_args",
                "fill must contain whole requested chunks within the file and cache budget",
            ));
        }
        file.borrow_mut()
            .fill(offset, data)
            .map_err(|error| StudioError::new("disc_fill", error.to_string()))?;
        Ok(json!({"session": session, "filled": data.len()}))
    }

    pub fn close(&mut self, session: u32) -> Result<Value, StudioError> {
        self.session(session)?;
        self.sessions.remove(&session);
        Ok(json!({"session": session, "closed": true}))
    }

    pub fn maps(&mut self, session: u32) -> Result<Value, StudioError> {
        let state = self.session(session)?;
        state.require_ready()?;
        while state.summaries.len() < state.registry.len() {
            let map = &state.registry[state.summaries.len()];
            let read = state.map_read.get_or_insert_with(|| BundleRead::new(map));
            if !read
                .advance(state.source.as_mut().expect("ready source").as_mut())
                .map_err(disc_error)?
            {
                return Ok(
                    json!({"session": session, "state": "pending", "completed": state.summaries.len(), "total": state.registry.len(), "missing": state.missing()}),
                );
            }
            let component = extract_events(&read.bytes)?;
            state.summaries.push(MapSummary {
                map: map.map,
                file: map.file,
                slot: map.slot,
                placeholder: component.is_none(),
                actor_count: component.as_ref().map(|c| c.actor_count),
                script_size: component.as_ref().map(|c| c.component_end),
                stream_size: component.as_ref().map(|c| c.bytes.len()),
            });
            state.map_read = None;
        }
        Ok(json!({"session": session, "state": "ready", "maps": state.summaries}))
    }

    pub fn map_events(&mut self, session: u32, map: u32) -> Result<Value, StudioError> {
        self.read_map(session, map, false)
    }

    pub fn map_bundle(&mut self, session: u32, map: u32) -> Result<Value, StudioError> {
        self.read_map(session, map, true)
    }

    fn read_map(
        &mut self,
        session: u32,
        map: u32,
        include_bundle: bool,
    ) -> Result<Value, StudioError> {
        let state = self.session(session)?;
        state.require_ready()?;
        let record = state
            .registry
            .get(map as usize)
            .filter(|record| record.map == map)
            .ok_or_else(|| {
                StudioError::new(
                    "unknown_map",
                    format!("map {map} outside this disc's map registry"),
                )
            })?;
        let slot = record.slot;
        let lba = record.record.lba;
        if state.event_read.as_ref().is_none_or(|read| read.map != map) {
            state.event_read = Some(BundleRead::new(record));
        }
        let read = state.event_read.as_mut().expect("event bundle read");
        if !read
            .advance(state.source.as_mut().expect("ready source").as_mut())
            .map_err(disc_error)?
        {
            return Ok(
                json!({"session": session, "map": map, "state": "pending", "missing": state.missing()}),
            );
        }
        let bytes = std::mem::take(&mut read.bytes);
        let component = extract_events(&bytes)?;
        state.event_read = None;
        if include_bundle {
            let identity = state.identity.as_ref().expect("ready identity");
            let source = if identity["test_fixture"] == false && identity["disc"].as_u64().is_some()
            {
                json!(xem_disc::field_context::SourceIdentity {
                    disc: identity["disc"].as_u64().expect("disc") as u8,
                    boot_sha256: identity["boot_sha256"].as_str().expect("boot hash").into(),
                    map,
                    slot: slot as u32,
                    lba,
                    indexed_size: bytes.len() as u32,
                    source_sha256: xem_disc::field_context::hash(&bytes)
                })
            } else {
                Value::Null
            };
            return Ok(
                json!({"session":session,"map":map,"slot":slot,"state":"ready","placeholder":component.is_none(),"bytes":bytes,"source":source}),
            );
        }
        Ok(
            json!({"session": session, "map": map, "state": "ready", "placeholder": component.is_none(), "component": component}),
        )
    }
}

#[cfg(not(target_arch = "wasm32"))]
fn native_session(path: String) -> Result<Session, StudioError> {
    let source = xem_disc::open_path(std::path::Path::new(&path)).map_err(disc_error)?;
    let bytes = std::fs::metadata(&path)
        .map_err(|error| StudioError::new("disc_open", error.to_string()))?
        .len();
    Ok(Session::new(path, bytes, None, Some(source)))
}

#[cfg(target_arch = "wasm32")]
fn native_session(_path: String) -> Result<Session, StudioError> {
    Err(StudioError::new(
        "unsupported_host",
        "wasm disc sessions require pull ranges",
    ))
}

/// Append an authored event stream without moving or rewriting any other
/// component. Its immutable source must be this bundle's actual event stream.
pub fn replace_events(
    bundle: &[u8],
    base: &crate::field::FieldGraph,
    compiled: &crate::field::draft::Compiled,
) -> Result<Vec<u8>, StudioError> {
    replace_events_inner(bundle, base, compiled, None)
}

/// A measured original load supplies exact source bytes beyond indexed EOF.
pub fn replace_events_with_context(
    bundle: &[u8],
    base: &crate::field::FieldGraph,
    compiled: &crate::field::draft::Compiled,
    context: &xem_disc::field_context::SourceContext,
    source: &xem_disc::field_context::SourceIdentity,
    module: &str,
) -> Result<Vec<u8>, StudioError> {
    replace_events_inner(bundle, base, compiled, Some((context, source, module)))
}
fn replace_events_inner(
    bundle: &[u8],
    base: &crate::field::FieldGraph,
    compiled: &crate::field::draft::Compiled,
    context: Option<(
        &xem_disc::field_context::SourceContext,
        &xem_disc::field_context::SourceIdentity,
        &str,
    )>,
) -> Result<Vec<u8>, StudioError> {
    if bundle.len() > MAX_BUNDLE_BYTES {
        return Err(StudioError::new(
            "invalid_bundle",
            "source bundle exceeds the supported bound",
        ));
    }
    let guard = match context {
        Some((context, source, module)) => context
            .guard(bundle, source, module)
            .map_err(|e| StudioError::new("invalid_source_context", e))?,
        None => Vec::new(),
    };
    let mut measured = bundle.to_vec();
    measured.extend_from_slice(&guard);
    if bundle.len() > MAX_BUNDLE_BYTES {
        return Err(StudioError::new(
            "invalid_bundle",
            "source bundle exceeds the supported bound",
        ));
    }
    let original = extract_events(bundle)?
        .ok_or_else(|| StudioError::new("invalid_bundle", "placeholder has no event component"))?;
    if original.bytes != crate::field::encode(base)? || original.declared_size != base.declared_size
    {
        return Err(StudioError::new(
            "source_mismatch",
            "draft base does not match this bundle's complete event stream and declared boundary",
        ));
    }
    if compiled.bytes == original.bytes && compiled.declared_size == original.declared_size {
        return Ok(bundle.to_vec());
    }
    if compiled.graph != crate::field::decode(&compiled.bytes, Some(compiled.declared_size))? {
        return Err(StudioError::new(
            "invalid_graph",
            "compiled graph differs from its component bytes",
        ));
    }
    // The original loader reads NINE descriptors. Appending can expose bytes
    // to a previously truncated final stream, so establish complete bounded
    // decoding of each unaffected stream before changing the container.
    let mut unaffected = Vec::new();
    for index in 0..9 {
        if index == 5 {
            continue;
        }
        let logical = u32_at(bundle, 0x10c + index * 4)? as usize;
        let start = u32_at(bundle, 0x130 + index * 4)? as usize;
        // Explicit absent descriptors in synthetic/public containers carry no
        // stream. Nonempty real descriptors must follow the loader contract.
        if logical == 0 && start == 0 {
            continue;
        }
        if start < HEADER || start >= bundle.len() || logical > MAX_COMPONENT_BYTES {
            return Err(StudioError::new(
                "invalid_bundle",
                format!("component {index} has an invalid size or packed-stream offset"),
            ));
        }
        let limit = logical
            .checked_add(16)
            .ok_or_else(|| StudioError::new("invalid_bundle", "component allocation overflow"))?
            .min(MAX_COMPONENT_BYTES);
        let decoded = packed::decode_block(&measured[start..], limit).map_err(|cause| {
            StudioError::new(
                if matches!(cause,packed::PackedError::Truncated{..}) {"source_context_required"} else {"unsupported_bundle"},
                format!(
                    "component {index} cannot be preserved without complete measured source memory: {cause}"
                ),
            )
        })?;
        if decoded.data.len() < logical {
            return Err(StudioError::new(
                "invalid_bundle",
                format!("component {index} decodes shorter than its declared size"),
            ));
        }
        unaffected.push((index, start, limit, decoded.data));
    }
    let encoded =
        packed::encode_literals(&compiled.bytes, MAX_COMPONENT_BYTES).map_err(packed_error)?;
    let decoded = packed::decode_block(&encoded, MAX_COMPONENT_BYTES).map_err(packed_error)?;
    if decoded.data.len().saturating_sub(compiled.declared_size) > 16
        || decoded.data.get(..compiled.bytes.len()) != Some(compiled.bytes.as_slice())
    {
        return Err(StudioError::new(
            "invalid_bundle",
            "authored packing exceeded the allocation tail contract",
        ));
    }
    // The event decoder includes the explicit packing tail in u16 PC bounds.
    let padded_graph = crate::field::decode(&decoded.data, Some(compiled.declared_size))?;
    if compiled
        .graph
        .diagnostics
        .outside
        .iter()
        .chain(&compiled.graph.diagnostics.inner)
        .any(|[_, target]| {
            *target >= compiled.graph.stream_size - compiled.graph.bytecode_offset
                && *target < decoded.data.len() - compiled.graph.bytecode_offset
        })
        || padded_graph.instructions != compiled.graph.instructions
        || padded_graph.diagnostics != compiled.graph.diagnostics
    {
        return Err(StudioError::new(
            "unsupported_relocation",
            "packing tail would activate an unresolved destination or change decoded execution",
        ));
    }
    let start = measured
        .len()
        .checked_add(3)
        .map(|n| n & !3)
        .ok_or_else(|| StudioError::new("invalid_bundle", "bundle append offset overflow"))?;
    let size = start
        .checked_add(encoded.len())
        .ok_or_else(|| StudioError::new("invalid_bundle", "bundle append size overflow"))?;
    if size > MAX_BUNDLE_BYTES {
        return Err(StudioError::new(
            "invalid_bundle",
            "authored bundle exceeds the supported bound",
        ));
    }
    let mut result = measured;
    result.resize(start, 0);
    result.extend_from_slice(&encoded);
    let declared = u32::try_from(compiled.declared_size)
        .map_err(|_| StudioError::new("invalid_bundle", "component size exceeds u32"))?;
    let start = u32::try_from(start)
        .map_err(|_| StudioError::new("invalid_bundle", "component offset exceeds u32"))?;
    result[0x120..0x124].copy_from_slice(&declared.to_le_bytes());
    result[0x144..0x148].copy_from_slice(&start.to_le_bytes());
    for (index, start, limit, expected) in unaffected {
        let actual = packed::decode_block(&result[start..], limit).map_err(packed_error)?;
        if actual.data != expected {
            return Err(StudioError::new(
                "invalid_bundle",
                format!("appending changed decoded component {index}"),
            ));
        }
    }
    if let Some((context, _, _)) = context {
        context
            .verify_outputs(&result, Some(5))
            .map_err(|e| StudioError::new("invalid_source_context", e))?;
    }
    let actual = extract_events(&result)?
        .ok_or_else(|| StudioError::new("invalid_bundle", "replacement became a placeholder"))?;
    if actual.bytes != decoded.data || actual.declared_size != compiled.declared_size {
        return Err(StudioError::new(
            "invalid_bundle",
            "replacement event stream verification failed",
        ));
    }
    Ok(result)
}
