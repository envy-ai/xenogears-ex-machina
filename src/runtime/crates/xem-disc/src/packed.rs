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
    RequiredOutput { required: usize, size: usize },
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
            Self::RequiredOutput { required, size } => {
                write!(f, "required output {required} exceeds declared size {size}")
            }
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

/// Decode only a caller-qualified required prefix, without reading optional
/// subsequent tokens or a terminal flag. Each encountered token is still fully
/// validated against the declared stream length; a boundary-crossing copy must
/// have a valid distance and fit the complete advertised output. This is not the
/// original runtime decoder and supplies no missing bytes or RAM provenance.
pub fn decode_prefix(
    source: &[u8],
    required: usize,
    output_limit: usize,
) -> Result<PackedBlock, PackedError> {
    let raw = source.get(..4).ok_or(PackedError::MissingLength)?;
    let size = u32::from_le_bytes(raw.try_into().expect("four bytes")) as usize;
    if size > output_limit {
        return Err(PackedError::OutputLimit {
            size,
            limit: output_limit,
        });
    }
    if required > size {
        return Err(PackedError::RequiredOutput { required, size });
    }
    let mut position = 4;
    let mut groups = 0;
    let mut output = Vec::with_capacity(required);
    while output.len() < required {
        let flags = read_byte(source, &mut position, &output)?;
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
                for _ in 0..length.min(required - output.len()) {
                    output.push(output[output.len() - distance]);
                }
            } else {
                output.push(low);
            }
            if output.len() == required {
                return Ok(PackedBlock {
                    data: output,
                    token_bytes: position,
                    source_bytes_read: position,
                    groups,
                });
            }
        }
    }
    Ok(PackedBlock {
        data: output,
        token_bytes: position,
        source_bytes_read: position,
        groups,
    })
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

/// Deterministic literal-only encoding for newly authored components. The last
/// group is explicitly completed with authored zero literals and a terminal flag
/// byte. This does not claim to reproduce the original compressor's token choice.
pub fn encode_literals(data: &[u8], output_limit: usize) -> Result<Vec<u8>, PackedError> {
    let size = data
        .len()
        .checked_add(7)
        .map(|n| n & !7)
        .ok_or(PackedError::OutputLimit {
            size: data.len(),
            limit: output_limit,
        })?;
    if size > output_limit || size > u32::MAX as usize {
        return Err(PackedError::OutputLimit {
            size,
            limit: output_limit,
        });
    }
    let mut result = Vec::with_capacity(5 + size + size / 8);
    result.extend_from_slice(&(size as u32).to_le_bytes());
    for start in (0..size).step_by(8) {
        result.push(0);
        for index in start..start + 8 {
            result.push(data.get(index).copied().unwrap_or(0));
        }
    }
    result.push(0); // Measured final speculative flag read, even for empty data.
    Ok(result)
}
