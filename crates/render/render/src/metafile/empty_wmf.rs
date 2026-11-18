//! Conservative recognition of complete WMFs containing only state and private comments.
//!
//! This is separate from permissive playback: a skipped or truncated drawing record
//! must not turn a failed preview into a successful transparent image.

use super::{MAX_RECORDS, is_placeable, plain_wmf_header, u16_at, u32_at};

pub(super) fn validated_empty(b: &[u8]) -> bool {
    validate(b).is_some()
}

fn validate(b: &[u8]) -> Option<()> {
    let start = if is_placeable(b) {
        let header = b.get(..22)?;
        let checksum = header.as_chunks::<2>().0.iter().fold(0u16, |sum, word| sum ^ u16::from_le_bytes([word[0], word[1]]));
        if checksum != 0 || u16_at(b, 4)? != 0 || u16_at(b, 14)? == 0 || u32_at(b, 16)? != 0 {
            return None;
        }
        22
    } else {
        0
    };
    let data = b.get(start..)?;
    if !plain_wmf_header(data) || u16_at(data, 16)? != 0 {
        return None;
    }
    let bytes = usize::try_from(u32_at(data, 6)?).ok()?.checked_mul(2)?;
    if bytes != data.len() {
        return None;
    }
    let declared_max = u32_at(data, 12)?;
    let mut largest = 0;
    let mut at = 18usize;
    for _ in 0..MAX_RECORDS {
        let words = u32_at(data, at)?;
        if words < 3 {
            return None;
        }
        let size = usize::try_from(words).ok()?.checked_mul(2)?;
        let end = at.checked_add(size)?;
        let record = data.get(at..end)?;
        let function = u16_at(record, 4)?;
        let params = record.get(6..)?;
        largest = largest.max(words);
        if function == 0 {
            return (words == 3 && end == data.len() && largest == declared_max).then_some(());
        }
        let valid = match function {
            // Background/fill mode and text alignment allow one ignored reserved WORD.
            0x0102 | 0x0106 | 0x012E => matches!(params.len(), 2 | 4),
            0x0103 => params.len() == 2,
            // Colors, window/viewport mapping and current position.
            0x0201 | 0x0209 | 0x020B | 0x020C | 0x020D | 0x020E | 0x0214 => params.len() == 4,
            0x001E => params.is_empty(), // SaveDC.
            0x0127 => params.len() == 2, // RestoreDC.
            0x0626 => private_comment(params),
            // Object creation, drawing, and unrecognized extensions are not proven empty.
            _ => false,
        };
        if !valid {
            return None;
        }
        at = end;
    }
    None
}

fn private_comment(params: &[u8]) -> bool {
    if u16_at(params, 0) != Some(15) {
        return false;
    }
    let Some(count) = u16_at(params, 2).map(usize::from) else { return false };
    if params.len() != 4 + count + count % 2 {
        return false;
    }
    let Some(comment) = params.get(4..4 + count) else { return false };
    // Legacy MFCOMMENT stores private metadata. The reserved WMFC signature
    // instead carries embedded EMF drawing data; even a short prefix is not empty.
    let signature = b"WMFC";
    comment.is_empty() || !comment.iter().zip(signature).all(|(a, b)| a == b)
}
