//! Bounded port of `tools/analysis/packed.py`, the original eight-token decoder.

use std::fmt;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PackedBlock {
    pub data: Vec<u8>,
    pub token_bytes: usize,
    pub source_bytes_read: usize,
    pub groups: usize,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum PackedError {
    MissingLength,
    OutputLimit { size: usize, limit: usize },
    Truncated { position: usize, output: Vec<u8> },
    InvalidDistance { distance: usize, position: usize },
    CopyCrossesOutput { position: usize },
    FlagGroupCrossesOutput { position: usize },
}

impl fmt::Display for PackedError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::MissingLength => write!(f, "missing u32le output length at source +0x0"),
            Self::OutputLimit { size, limit } => {
                write!(f, "declared output {size} exceeds bound {limit}")
            }
            Self::Truncated { position, .. } => write!(f, "source truncated at +0x{position:x}"),
            Self::InvalidDistance { distance, position } => {
                write!(f, "invalid backward distance {distance} at +0x{position:x}")
            }
            Self::CopyCrossesOutput { position } => {
                write!(f, "copy crosses output length at +0x{position:x}")
            }
            Self::FlagGroupCrossesOutput { position } => {
                write!(f, "flag group crosses output length at +0x{position:x}")
            }
        }
    }
}

impl std::error::Error for PackedError {}

/// Decode whole flag groups, including the final speculative flag read.
/// Neither a partial group nor absent source padding is synthesized.
pub fn decode_block(source: &[u8], output_limit: usize) -> Result<PackedBlock, PackedError> {
    let raw = source.get(..4).ok_or(PackedError::MissingLength)?;
    let size = u32::from_le_bytes(raw.try_into().expect("four bytes")) as usize;
    if size > output_limit {
        return Err(PackedError::OutputLimit {
            size,
            limit: output_limit,
        });
    }
    let mut position = 4;
    let mut groups = 0;
    let mut output = Vec::with_capacity(size);
    loop {
        let flag_position = position;
        let flags = read_byte(source, &mut position, &output)?;
        if output.len() == size {
            return Ok(PackedBlock {
                data: output,
                token_bytes: flag_position,
                source_bytes_read: position,
                groups,
            });
        }
        groups += 1;
        for token in 0..8 {
            let token_offset = position;
            let low = read_byte(source, &mut position, &output)?;
            if flags & (1 << token) != 0 {
                let high = read_byte(source, &mut position, &output)?;
                let distance = usize::from(low) | (usize::from(high & 0x0f) << 8);
                let length = usize::from(high >> 4) + 3;
                if distance == 0 || distance > output.len() {
                    return Err(PackedError::InvalidDistance {
                        distance,
                        position: token_offset,
                    });
                }
                if length > size - output.len() {
                    return Err(PackedError::CopyCrossesOutput {
                        position: token_offset,
                    });
                }
                for _ in 0..length {
                    output.push(output[output.len() - distance]);
                }
            } else {
                if output.len() >= size {
                    return Err(PackedError::FlagGroupCrossesOutput {
                        position: token_offset,
                    });
                }
                output.push(low);
            }
        }
    }
}

fn read_byte(source: &[u8], position: &mut usize, output: &[u8]) -> Result<u8, PackedError> {
    let value = source
        .get(*position)
        .copied()
        .ok_or_else(|| PackedError::Truncated {
            position: *position,
            output: output.to_vec(),
        })?;
    *position += 1;
    Ok(value)
}
