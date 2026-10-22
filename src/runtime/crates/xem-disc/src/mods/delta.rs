//! Deterministic source-copy/literal deltas; never contain a source file member.
use crate::source::{Result, format_error};
use std::collections::HashMap;

pub const MAX_FILE_BYTES: usize = 32 << 20;
const MAGIC: &[u8; 8] = b"XEMDLT1\0";
const MATCH: usize = 8;

fn size(bytes: &[u8]) -> Result<()> {
    if bytes.len() > MAX_FILE_BYTES {
        return Err(format_error("mod delta file exceeds 32 MiB"));
    }
    Ok(())
}
fn word(out: &mut Vec<u8>, value: usize) {
    out.extend_from_slice(&(value as u32).to_le_bytes());
}
fn copy(out: &mut Vec<u8>, start: usize, len: usize) {
    if len > 0 {
        out.push(0);
        word(out, start);
        word(out, len);
    }
}
fn literal(out: &mut Vec<u8>, bytes: &[u8]) {
    if !bytes.is_empty() {
        out.push(1);
        word(out, bytes.len());
        out.extend_from_slice(bytes);
    }
}

pub fn build(source: &[u8], result: &[u8]) -> Result<Vec<u8>> {
    size(source)?;
    size(result)?;
    let mut out = MAGIC.to_vec();
    word(&mut out, source.len());
    word(&mut out, result.len());
    let prefix = source
        .iter()
        .zip(result)
        .take_while(|(a, b)| a == b)
        .count();
    let suffix = source[prefix..]
        .iter()
        .rev()
        .zip(result[prefix..].iter().rev())
        .take_while(|(a, b)| a == b)
        .count();
    copy(&mut out, 0, prefix);
    let end = result.len() - suffix;
    // Earliest source offset wins for an anchor; map iteration never determines output.
    let mut anchors = HashMap::new();
    if end - prefix >= MATCH {
        let stride = source
            .len()
            .saturating_sub(MATCH)
            .saturating_add(1)
            .div_ceil(1 << 20)
            .max(1);
        for (at, bytes) in source.windows(MATCH).enumerate().step_by(stride) {
            anchors
                .entry(u64::from_le_bytes(bytes.try_into().expect("8 bytes")))
                .or_insert(at);
        }
    }
    let mut at = prefix;
    let mut pending = at;
    while at < end {
        let found = result
            .get(at..at + MATCH)
            .filter(|_| at + MATCH <= end)
            .and_then(|bytes| anchors.get(&u64::from_le_bytes(bytes.try_into().expect("8 bytes"))))
            .copied();
        if let Some(start) = found {
            let len = source[start..]
                .iter()
                .zip(&result[at..end])
                .take_while(|(a, b)| a == b)
                .count();
            literal(&mut out, &result[pending..at]);
            copy(&mut out, start, len);
            at += len;
            pending = at;
        } else {
            at += 1;
        }
    }
    literal(&mut out, &result[pending..end]);
    copy(&mut out, source.len() - suffix, suffix);
    out.push(2);
    Ok(out)
}

enum Operation<'a> {
    Copy(usize, usize),
    Literal(&'a [u8]),
}
fn visit<'a>(
    delta: &'a [u8],
    source_len: Option<usize>,
    mut operation: impl FnMut(Operation<'a>),
) -> Result<(usize, usize)> {
    let mut reader = Reader::new(delta);
    if reader.take(8)? != MAGIC {
        return Err(format_error("unknown mod delta format"));
    }
    let expected = reader.word()? as usize;
    let length = reader.word()? as usize;
    if expected > MAX_FILE_BYTES || length > MAX_FILE_BYTES {
        return Err(format_error("mod delta source/result exceeds 32 MiB"));
    }
    if source_len.is_some_and(|n| n != expected) {
        return Err(format_error("mod delta source length differs"));
    }
    let mut written = 0usize;
    loop {
        match reader.byte()? {
            0 => {
                let start = reader.word()? as usize;
                let count = reader.word()? as usize;
                if count == 0 || start.checked_add(count).is_none_or(|end| end > expected) {
                    return Err(format_error("mod delta copy outside source or empty"));
                }
                if count > length - written {
                    return Err(format_error("mod delta copy crosses result boundary"));
                }
                operation(Operation::Copy(start, count));
                written += count;
            }
            1 => {
                let count = reader.word()? as usize;
                if count == 0 || count > length - written {
                    return Err(format_error(
                        "mod delta literal crosses result boundary or is empty",
                    ));
                }
                operation(Operation::Literal(reader.take(count)?));
                written += count;
            }
            2 => {
                if written != length || !reader.done() {
                    return Err(format_error("mod delta size differs or trailing bytes"));
                }
                return Ok((expected, length));
            }
            _ => return Err(format_error("unknown mod delta instruction")),
        }
    }
}
/// Validate every instruction without allocating or requiring the source bytes.
pub fn validate(delta: &[u8]) -> Result<(usize, usize)> {
    visit(delta, None, |_| {})
}
pub fn apply(source: &[u8], delta: &[u8]) -> Result<Vec<u8>> {
    size(source)?;
    let mut result = Vec::new();
    visit(delta, Some(source.len()), |op| match op {
        Operation::Copy(start, count) => result.extend_from_slice(&source[start..start + count]),
        Operation::Literal(bytes) => result.extend_from_slice(bytes),
    })?;
    Ok(result)
}

pub(super) struct Reader<'a> {
    bytes: &'a [u8],
    at: usize,
}
impl<'a> Reader<'a> {
    pub(super) fn new(bytes: &'a [u8]) -> Self {
        Self { bytes, at: 0 }
    }
    pub(super) fn take(&mut self, len: usize) -> Result<&'a [u8]> {
        let end = self
            .at
            .checked_add(len)
            .ok_or_else(|| format_error("mod input length overflow"))?;
        let bytes = self
            .bytes
            .get(self.at..end)
            .ok_or_else(|| format_error("truncated mod input"))?;
        self.at = end;
        Ok(bytes)
    }
    pub(super) fn byte(&mut self) -> Result<u8> {
        Ok(self.take(1)?[0])
    }
    pub(super) fn word(&mut self) -> Result<u32> {
        Ok(u32::from_le_bytes(
            self.take(4)?.try_into().expect("4 bytes"),
        ))
    }
    pub(super) fn done(&self) -> bool {
        self.at == self.bytes.len()
    }
}
