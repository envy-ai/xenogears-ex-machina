//! Synthetic tests for read-only observations around the original field loader.
//! No game files or runtime module are used.
use xem_core::{
    field_source::FieldSources,
    memory::{GameMemory, OutOfBounds},
};
use xem_disc::{
    field_context::{self as facts, SourceContext},
    packed,
};

const ROOT: u32 = facts::RAM_START + 0x4000;
const ENTRY: u32 = facts::RAM_START + 0x800;
const MAP: u32 = 23;
const SLOT: u32 = 652;
const MEDIA: u64 = 9;
const VBLANK: u64 = 733;
const MODULE: &str = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";

struct SparseRam(Vec<u8>);
impl SparseRam {
    fn new() -> Self {
        Self(vec![0; (facts::RAM_END - facts::RAM_START) as usize])
    }
    fn range(&self, address: u32, len: usize) -> Result<std::ops::Range<usize>, OutOfBounds> {
        let Some(start) = address.checked_sub(facts::RAM_START).map(|n| n as usize) else {
            return Err(OutOfBounds { address, len });
        };
        match start.checked_add(len) {
            Some(end) if end <= self.0.len() => Ok(start..end),
            _ => Err(OutOfBounds { address, len }),
        }
    }
}
impl GameMemory for SparseRam {
    fn read(&self, address: u32, out: &mut [u8]) -> Result<(), OutOfBounds> {
        out.copy_from_slice(&self.0[self.range(address, out.len())?]);
        Ok(())
    }
    fn write(&mut self, address: u32, data: &[u8]) -> Result<(), OutOfBounds> {
        let range = self.range(address, data.len())?;
        self.0[range].copy_from_slice(data);
        Ok(())
    }
}

struct Fixture {
    memory: SparseRam,
    bundle: Vec<u8>,
    full_source: Vec<u8>,
    offsets: [usize; 9],
    outputs: [Vec<u8>; 9],
    destinations: [u32; 9],
    allocation: u32,
    source_sha256: String,
}

fn literal_block(bytes: &[u8]) -> Vec<u8> {
    let mut result = (bytes.len() as u32).to_le_bytes().to_vec();
    result.push(0);
    result.extend_from_slice(bytes);
    result.push(0); // The original decoder reads one terminal flag after output.
    result
}

fn measured_event_stream() -> Vec<u8> {
    let mut stream = 78_u32.to_le_bytes().to_vec();
    stream.push(0);
    stream.extend_from_slice(b"ABCDEFGH");
    stream.push(1);
    stream.extend_from_slice(&[1, 0xc0]); // Fifteen-byte back-reference.
    stream.extend_from_slice(b"ijklmno");
    for group in 0..5 {
        stream.push(0);
        stream.extend_from_slice(&[b'0' + group; 8]);
    }
    stream.push(0);
    stream.extend_from_slice(b"QRSTUVWX");
    stream.push(0);
    assert_eq!(stream.len(), 78);
    stream
}

fn make_fixture() -> Fixture {
    let mut bundle = vec![0_u8; 0x154];
    let mut offsets = [0_usize; 9];
    let mut outputs: [Vec<u8>; 9] = std::array::from_fn(|_| Vec::new());
    let mut destinations = [0_u32; 9];
    let mut cursor = bundle.len();
    for index in 0..8 {
        let data = [index as u8 + 1; 8];
        let block = literal_block(&data);
        offsets[index] = cursor;
        bundle[0x10c + index * 4..0x110 + index * 4]
            .copy_from_slice(&(data.len() as u32).to_le_bytes());
        bundle[0x130 + index * 4..0x134 + index * 4]
            .copy_from_slice(&(cursor as u32).to_le_bytes());
        outputs[index] = packed::decode_block(&block, 24).unwrap().data;
        bundle.extend_from_slice(&block);
        cursor += block.len();
    }
    let event = measured_event_stream();
    offsets[8] = cursor;
    bundle[0x10c + 8 * 4..0x10c + 9 * 4].copy_from_slice(&72_u32.to_le_bytes());
    bundle[0x130 + 8 * 4..0x130 + 9 * 4].copy_from_slice(&(cursor as u32).to_le_bytes());
    let indexed_prefix = event.len() - 5;
    bundle.extend_from_slice(&event[..indexed_prefix]);
    let mut full_source = bundle.clone();
    full_source.extend_from_slice(&event[indexed_prefix..]);
    outputs[8] = packed::decode_block(&event, 88).unwrap().data;
    for (index, address) in destinations.iter_mut().enumerate() {
        *address = facts::RAM_START + 0x10_000 + index as u32 * 0x1000;
    }

    let allocation = bundle.len().div_ceil(4) * 4;
    let mut memory = SparseRam::new();
    memory.write(ROOT, &full_source).unwrap();
    let mut index_record = [0_u8; 7];
    index_record[..3].copy_from_slice(&[0x34, 0x12, 0]);
    index_record[3..].copy_from_slice(&(bundle.len() as i32).to_le_bytes());
    memory.write(ENTRY, &index_record).unwrap();
    let source_sha256 = facts::hash(&bundle);
    Fixture {
        memory,
        bundle,
        full_source,
        offsets,
        outputs,
        destinations,
        allocation: allocation as u32,
        source_sha256,
    }
}

fn record(phase: u32, fixture: &Fixture, index: Option<usize>) -> [u32; 8] {
    let (source, destination) = index.map_or((0, 0), |i| {
        (ROOT + fixture.offsets[i] as u32, fixture.destinations[i])
    });
    [
        phase,
        ROOT,
        fixture.allocation,
        MAP,
        SLOT,
        ENTRY,
        source,
        destination,
    ]
}

fn observe_all(sources: &mut FieldSources, fixture: &mut Fixture) {
    sources.observe(record(0, fixture, None), 1, MEDIA, VBLANK, &fixture.memory);
    // Vary synthetic call order to ensure descriptors, not call ordinal, identify outputs.
    for index in [8, 0, 3, 1, 7, 2, 5, 4, 6] {
        sources.observe(
            record(1, fixture, Some(index)),
            1,
            MEDIA,
            VBLANK,
            &fixture.memory,
        );
        fixture
            .memory
            .write(fixture.destinations[index], &fixture.outputs[index])
            .unwrap();
        sources.observe(
            record(2, fixture, Some(index)),
            1,
            MEDIA,
            VBLANK,
            &fixture.memory,
        );
    }
    sources.observe(record(3, fixture, None), 1, MEDIA, VBLANK, &fixture.memory);
}

fn published_context(fixture: &Fixture, sources: &FieldSources) -> SourceContext {
    sources
        .query(MAP, &fixture.source_sha256, MODULE, 1, MEDIA)
        .expect("complete synthetic original field load publishes")
}

#[test]
fn observer_publishes_all_nine_components_in_actual_call_order_with_exact_tail() {
    let mut fixture = make_fixture();
    let mut sources = FieldSources::default();
    observe_all(&mut sources, &mut fixture);
    let context = published_context(&fixture, &sources);
    assert_eq!(context.schema_version, 1);
    assert_eq!(context.provenance, "runtime_field_decompress");
    assert_eq!(context.module_sha256, MODULE);
    assert_eq!(context.identity.map, MAP);
    assert_eq!(context.identity.slot, SLOT);
    assert_eq!(context.identity.indexed_size as usize, fixture.bundle.len());
    assert_eq!(context.source_address, ROOT);
    assert_eq!(context.allocation_size, fixture.allocation);
    assert_eq!(context.load_vblank, VBLANK.to_string());
    assert_eq!(context.components.len(), 9);
    let event = context
        .components
        .iter()
        .find(|component| component.index == 8)
        .unwrap();
    assert_eq!(event.logical_size, 72);
    assert_eq!(event.packed_size, 78);
    assert_eq!(event.output_size, 78);
    assert_eq!(event.source_bytes_read, 78);
    assert_eq!(event.extra, fixture.full_source[fixture.bundle.len()..]);
    assert_eq!(
        context
            .guard(&fixture.bundle, &context.identity, MODULE)
            .unwrap(),
        event.extra
    );
}

#[test]
fn observer_does_not_publish_when_output_or_indexed_source_changes_mid_load() {
    let mut fixture = make_fixture();
    let mut sources = FieldSources::default();
    sources.observe(record(0, &fixture, None), 1, MEDIA, VBLANK, &fixture.memory);
    sources.observe(
        record(1, &fixture, Some(8)),
        1,
        MEDIA,
        VBLANK,
        &fixture.memory,
    );
    let mut wrong_output = fixture.outputs[8].clone();
    wrong_output[0] ^= 0xff;
    fixture
        .memory
        .write(fixture.destinations[8], &wrong_output)
        .unwrap();
    sources.observe(
        record(2, &fixture, Some(8)),
        1,
        MEDIA,
        VBLANK,
        &fixture.memory,
    );
    assert!(
        sources
            .query(MAP, &fixture.source_sha256, MODULE, 1, MEDIA)
            .is_err()
    );

    let mut fixture = make_fixture();
    let mut sources = FieldSources::default();
    sources.observe(record(0, &fixture, None), 1, MEDIA, VBLANK, &fixture.memory);
    let changed = [0xaa];
    fixture.memory.write(ROOT, &changed).unwrap();
    sources.observe(
        record(1, &fixture, Some(8)),
        1,
        MEDIA,
        VBLANK,
        &fixture.memory,
    );
    assert!(
        sources
            .query(MAP, &fixture.source_sha256, MODULE, 1, MEDIA)
            .is_err()
    );
}

#[test]
fn observer_rejects_incomplete_stale_and_malformed_generations_without_panicking() {
    let mut fixture = make_fixture();
    let mut sources = FieldSources::default();
    sources.observe(record(0, &fixture, None), 1, MEDIA, VBLANK, &fixture.memory);
    for index in [8, 0, 3, 1, 7, 2, 5, 4] {
        sources.observe(
            record(1, &fixture, Some(index)),
            1,
            MEDIA,
            VBLANK,
            &fixture.memory,
        );
        fixture
            .memory
            .write(fixture.destinations[index], &fixture.outputs[index])
            .unwrap();
        sources.observe(
            record(2, &fixture, Some(index)),
            1,
            MEDIA,
            VBLANK,
            &fixture.memory,
        );
    }
    sources.observe(record(3, &fixture, None), 1, MEDIA, VBLANK, &fixture.memory);
    assert!(
        sources
            .query(MAP, &fixture.source_sha256, MODULE, 1, MEDIA)
            .is_err()
    );

    let mut fixture = make_fixture();
    let mut sources = FieldSources::default();
    observe_all(&mut sources, &mut fixture);
    assert!(
        sources
            .query(MAP, &fixture.source_sha256, MODULE, 1, MEDIA + 1)
            .is_err()
    );
    assert!(
        sources
            .query(MAP + 1, &fixture.source_sha256, MODULE, 1, MEDIA)
            .is_err()
    );
    assert!(
        sources
            .query(MAP, "b".repeat(64).as_str(), MODULE, 1, MEDIA)
            .is_err()
    );
    assert!(
        sources
            .query(MAP, &fixture.source_sha256, "invalid-module-hash", 1, MEDIA)
            .is_err()
    );
    assert!(
        sources
            .query(MAP, &fixture.source_sha256, MODULE, 2, MEDIA)
            .is_err()
    );
    sources.observe(
        record(0, &fixture, None),
        1,
        MEDIA + 1,
        VBLANK + 1,
        &fixture.memory,
    );
    assert!(
        sources
            .query(MAP, &fixture.source_sha256, MODULE, 1, MEDIA + 1)
            .is_err()
    );

    let mut fixture = make_fixture();
    let mut sources = FieldSources::default();
    let mut bad_index = [0_u8; 7];
    bad_index[3..].copy_from_slice(&i32::MAX.to_le_bytes());
    fixture.memory.write(ENTRY, &bad_index).unwrap();
    let mut bad_begin = record(0, &fixture, None);
    bad_begin[2] = u32::MAX;
    sources.observe(bad_begin, 1, MEDIA, VBLANK, &fixture.memory);
    assert!(
        sources
            .query(MAP, &fixture.source_sha256, MODULE, 1, MEDIA)
            .is_err()
    );
}
