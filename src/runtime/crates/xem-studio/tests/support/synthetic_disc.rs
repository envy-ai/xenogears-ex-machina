//! Invented, deterministic MODE2/2352 disc fixture for the studio tests.
//!
//! The image has a tiny ISO9660 filesystem, a fixed synthetic boot executable
//! with an embedded file index, one valid field bundle and one placeholder.
//! No original image, extracted file, or game byte is used.

pub const BOOT_PATH: &str = "SLUS_999.99;1";
pub const DISC_NUMBER: u16 = 1;
pub const COMPONENT_OFFSET: usize = 0x154;
pub const COMPONENT_SIZE: usize = 197;
pub const STREAM_SIZE: usize = 200;
pub const BYTECODE_OFFSET: usize = 196;
pub const IMAGE_SECTORS: usize = 80;
pub const SECTOR_SIZE: usize = 2352;
const USER_SIZE: usize = 2048;
const EXE_HEADER: usize = 0x800;
const INDEX_BYTES: usize = 0x8000;
const DIRECTORY_BYTES: usize = 0x7C;
const EXE_LBA: usize = 50;
const BUNDLE_LBAS: [usize; 1] = [70];
const PLACEHOLDER_LBA: usize = 71;
#[cfg(feature = "test-fixtures")]
const COMPANION_LBA: usize = 120;
#[cfg(feature = "test-fixtures")]
const STREAM_IMAGE_SECTORS: usize = 240;

pub struct SyntheticDisc {
    pub image: Vec<u8>,
    pub executable: Vec<u8>,
    pub component: Vec<u8>,
    pub bundle: Vec<u8>,
}

fn index_record(lba: u32, size: i32) -> [u8; 7] {
    let lba = lba.to_le_bytes();
    let size = size.to_le_bytes();
    [lba[0], lba[1], lba[2], size[0], size[1], size[2], size[3]]
}

fn executable(bundle_lba: usize, bundle_size: usize) -> Vec<u8> {
    let index_offset = EXE_HEADER + 4;
    let directory_offset = index_offset + INDEX_BYTES;
    let mut exe = vec![0; directory_offset + DIRECTORY_BYTES];
    exe[..8].copy_from_slice(b"PS-X EXE");
    exe[0x18..0x1C].copy_from_slice(&0x8001_0000u32.to_le_bytes());
    exe[EXE_HEADER..EXE_HEADER + 4].copy_from_slice(&(-1i32).to_le_bytes());

    // Directory (4, 0) starts at record 2. Its preceding record describes
    // four files, i.e. two map/companion pairs at file numbers 0xb8+2*n.
    let record_offset = |slot: usize| index_offset + slot * 7;
    exe[record_offset(0xB7)..record_offset(0xB7) + 7].copy_from_slice(&index_record(0, -4));
    exe[record_offset(0xB8)..record_offset(0xB8) + 7]
        .copy_from_slice(&index_record(bundle_lba as u32, bundle_size as i32));
    // Companion file 0xb9 is unused; map 1 is a real short placeholder file.
    exe[record_offset(0xB9)..record_offset(0xB9) + 7].copy_from_slice(&index_record(0xFF_FFFF, 0));
    exe[record_offset(0xBA)..record_offset(0xBA) + 7]
        .copy_from_slice(&index_record(PLACEHOLDER_LBA as u32, 4));
    exe[record_offset(0xBB)..record_offset(0xBB) + 7].copy_from_slice(&index_record(0xFF_FFFF, 0));

    let mut directories = [0xFFu8; DIRECTORY_BYTES];
    directories[4 * 2..4 * 2 + 2].copy_from_slice(&2u16.to_le_bytes());
    directories[0x3C * 2..0x3C * 2 + 2].copy_from_slice(&DISC_NUMBER.to_le_bytes());
    exe[directory_offset..directory_offset + DIRECTORY_BYTES].copy_from_slice(&directories);
    exe
}

fn literal_block(stream: &[u8]) -> Vec<u8> {
    let mut packed = (stream.len() as u32).to_le_bytes().to_vec();
    for group in stream.chunks_exact(8) {
        packed.push(0); // all literal tokens
        packed.extend_from_slice(group);
    }
    // decode_block speculatively reads one terminal flags byte after output.
    packed.push(0);
    packed
}

fn component() -> Vec<u8> {
    let mut bytes = vec![0; STREAM_SIZE];
    bytes[128..132].copy_from_slice(&1u32.to_le_bytes());
    bytes[BYTECODE_OFFSET] = 0; // end_slot
    bytes[COMPONENT_SIZE..].copy_from_slice(&[0xA5, 0x5A, 0xC3]); // measured retained tail
    bytes
}

fn bundle(component: &[u8]) -> Vec<u8> {
    let packed = literal_block(component);
    let mut bytes = vec![0; COMPONENT_OFFSET + packed.len()];
    let size_offset = 0x10C + 4 * 5;
    let data_offset = 0x130 + 4 * 5;
    bytes[size_offset..size_offset + 4].copy_from_slice(&(COMPONENT_SIZE as u32).to_le_bytes());
    bytes[data_offset..data_offset + 4].copy_from_slice(&(COMPONENT_OFFSET as u32).to_le_bytes());
    bytes[COMPONENT_OFFSET..].copy_from_slice(&packed);
    bytes
}

fn iso_record(name: &[u8], lba: u32, size: u32, directory: bool) -> Vec<u8> {
    let length = 33 + name.len() + (name.len() + 1) % 2;
    let mut record = vec![0; length];
    record[0] = length as u8;
    record[2..6].copy_from_slice(&lba.to_le_bytes());
    record[6..10].copy_from_slice(&lba.to_be_bytes());
    record[10..14].copy_from_slice(&size.to_le_bytes());
    record[14..18].copy_from_slice(&size.to_be_bytes());
    record[25] = u8::from(directory) * 2;
    record[32] = name.len() as u8;
    record[33..33 + name.len()].copy_from_slice(name);
    record
}

fn bcd(value: usize) -> u8 {
    (((value / 10) << 4) | (value % 10)) as u8
}

fn mode2_sector(lba: usize, user: &[u8]) -> [u8; SECTOR_SIZE] {
    let mut sector = [0; SECTOR_SIZE];
    sector[1..11].fill(0xFF);
    let absolute = lba + 150;
    sector[12..16].copy_from_slice(&[
        bcd(absolute / (60 * 75)),
        bcd(absolute / 75 % 60),
        bcd(absolute % 75),
        2,
    ]);
    sector[16..24].copy_from_slice(&[0, 0, 0x08, 0, 0, 0, 0x08, 0]);
    let count = user.len().min(USER_SIZE);
    sector[24..24 + count].copy_from_slice(&user[..count]);
    sector
}

fn put_sector(image: &mut [u8], lba: usize, user: &[u8]) {
    let start = lba * SECTOR_SIZE;
    image[start..start + SECTOR_SIZE].copy_from_slice(&mode2_sector(lba, user));
}

pub fn build() -> SyntheticDisc {
    let component = component();
    let bundle = bundle(&component);
    let executable = executable(BUNDLE_LBAS[0], bundle.len());
    let mut image = vec![0; IMAGE_SECTORS * SECTOR_SIZE];

    let mut pvd = vec![0; USER_SIZE];
    pvd[..7].copy_from_slice(b"\x01CD001\x01");
    pvd[156..190].copy_from_slice(&iso_record(&[0], 18, USER_SIZE as u32, true));
    put_sector(&mut image, 16, &pvd);

    let cnf = b"BOOT = cdrom:\\SLUS_999.99;1\r\nTCB = 4\r\n";
    let mut directory = Vec::new();
    directory.extend(iso_record(&[0], 18, USER_SIZE as u32, true));
    directory.extend(iso_record(&[1], 18, USER_SIZE as u32, true));
    directory.extend(iso_record(
        BOOT_PATH.as_bytes(),
        EXE_LBA as u32,
        executable.len() as u32,
        false,
    ));
    directory.extend(iso_record(b"SYSTEM.CNF;1", 19, cnf.len() as u32, false));
    put_sector(&mut image, 18, &directory);
    put_sector(&mut image, 19, cnf);

    for (offset, chunk) in executable.chunks(USER_SIZE).enumerate() {
        put_sector(&mut image, EXE_LBA + offset, chunk);
    }
    for sector in 0..INDEX_BYTES / USER_SIZE {
        let start = EXE_HEADER + 4 + sector * USER_SIZE;
        put_sector(
            &mut image,
            24 + sector,
            &executable[start..start + USER_SIZE],
        );
    }
    let directories_start = EXE_HEADER + 4 + INDEX_BYTES;
    put_sector(
        &mut image,
        40,
        &executable[directories_start..directories_start + DIRECTORY_BYTES],
    );
    put_sector(&mut image, BUNDLE_LBAS[0], &bundle);
    put_sector(&mut image, PLACEHOLDER_LBA, &[0, 0, 0, 0]);

    SyntheticDisc {
        image,
        executable,
        component,
        bundle,
    }
}

/// Add one indexed map companion to an otherwise unchanged synthetic disc.
/// The index lives in its own raw sectors; leave the boot executable untouched.
#[cfg(feature = "test-fixtures")]
pub fn with_map_stream(mut fixture: SyntheticDisc, stream: &[u8]) -> SyntheticDisc {
    assert!(!stream.is_empty() && stream.len().is_multiple_of(USER_SIZE));
    assert!(COMPANION_LBA + stream.len() / USER_SIZE <= STREAM_IMAGE_SECTORS);
    fixture.image.resize(STREAM_IMAGE_SECTORS * SECTOR_SIZE, 0);
    let companion_slot = 0xB9usize;
    let index_start = EXE_HEADER + 4;
    let mut index = fixture.executable[index_start..index_start + INDEX_BYTES].to_vec();
    let record_at = companion_slot * 7;
    index[record_at..record_at + 7]
        .copy_from_slice(&index_record(COMPANION_LBA as u32, stream.len() as i32));

    for (sector, chunk) in index.chunks_exact(USER_SIZE).enumerate() {
        put_sector(&mut fixture.image, 24 + sector, chunk);
    }
    for (offset, chunk) in stream.chunks(USER_SIZE).enumerate() {
        put_sector(&mut fixture.image, COMPANION_LBA + offset, chunk);
    }
    fixture
}

#[cfg(feature = "test-fixtures")]
pub fn sample_map_stream() -> Vec<u8> {
    let mut stream = vec![0; 2 * USER_SIZE];
    stream[0..4].copy_from_slice(&0x1200u32.to_le_bytes());
    stream[4..6].copy_from_slice(&12u16.to_le_bytes());
    stream[6..8].copy_from_slice(&30u16.to_le_bytes());
    stream[12..14].copy_from_slice(&2u16.to_le_bytes());
    stream[20..24].copy_from_slice(&1u32.to_le_bytes());
    stream[24..28].copy_from_slice(&1u32.to_le_bytes());
    stream[28..30].copy_from_slice(&1u16.to_le_bytes());
    stream[USER_SIZE..USER_SIZE + 2].copy_from_slice(&0x1234u16.to_le_bytes());
    stream[USER_SIZE + 2..USER_SIZE + 4].copy_from_slice(&0x5678u16.to_le_bytes());
    stream
}
