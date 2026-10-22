//! Form1 sector integrity, authored from the CD-ROM polynomial/parity rules.
//! Layout/parity cross-reference: pinned chd 0.3.4 compression/ecc.rs;
//! libchdr src/libchdr_cdrom.c. No source tables or implementation copied.
use crate::sector::SECTOR_SIZE;

pub(super) fn regenerate(raw: &mut [u8; SECTOR_SIZE]) {
    // Reflected CD-ROM EDC over the duplicated subheader and Form1 payload.
    let mut edc = 0u32;
    for byte in &raw[16..2072] {
        edc ^= u32::from(*byte);
        for _ in 0..8 {
            edc = (edc >> 1) ^ if edc & 1 != 0 { 0xd801_8001 } else { 0 };
        }
    }
    raw[2072..2076].copy_from_slice(&edc.to_le_bytes());
    let header: [u8; 4] = raw[12..16].try_into().expect("header");
    raw[12..16].fill(0); // MODE2 parity excludes the position/mode header.
    let mut forward = [0u8; 256];
    let mut inverse = [0u8; 256];
    for i in 0..256 {
        let doubled = (i as u16) << 1;
        let next = (doubled ^ if doubled & 0x100 != 0 { 0x11d } else { 0 }) as u8;
        forward[i] = next;
        inverse[i ^ next as usize] = i as u8;
    }
    parity(
        raw,
        Plane {
            major: 86,
            minor: 24,
            major_step: 2,
            minor_step: 86,
            out: 2076,
        },
        &forward,
        &inverse,
    );
    parity(
        raw,
        Plane {
            major: 52,
            minor: 43,
            major_step: 86,
            minor_step: 88,
            out: 2248,
        },
        &forward,
        &inverse,
    );
    raw[12..16].copy_from_slice(&header);
}
struct Plane {
    major: usize,
    minor: usize,
    major_step: usize,
    minor_step: usize,
    out: usize,
}
fn parity(raw: &mut [u8; SECTOR_SIZE], plane: Plane, forward: &[u8; 256], inverse: &[u8; 256]) {
    let Plane {
        major,
        minor,
        major_step,
        minor_step,
        out,
    } = plane;
    let size = major * minor;
    for n in 0..major {
        let mut index = (n / 2) * major_step + (n & 1);
        let (mut a, mut b) = (0u8, 0u8);
        for _ in 0..minor {
            let value = raw[12 + index];
            a ^= value;
            b ^= value;
            a = forward[a as usize];
            index = (index + minor_step) % size;
        }
        a = inverse[(forward[a as usize] ^ b) as usize];
        raw[out + n] = a;
        raw[out + n + major] = a ^ b;
    }
}
