#![allow(clippy::unwrap_used, clippy::expect_used)]

use super::rasterize;

/// A synthetic WMF with complete file lengths, maximum record size, and an EOF record.
fn wmf(placeable: bool, records: &[(u16, Vec<u16>)]) -> Vec<u8> {
    let mut body = Vec::new();
    let mut maximum = 3;
    for (function, parameters) in records.iter().chain(std::iter::once(&(0, vec![]))) {
        let words = 3 + parameters.len() as u32;
        maximum = maximum.max(words);
        body.extend(words.to_le_bytes());
        body.extend(function.to_le_bytes());
        for parameter in parameters {
            body.extend(parameter.to_le_bytes());
        }
    }
    let mut bytes = Vec::new();
    if placeable {
        bytes.extend(0x9AC6_CDD7u32.to_le_bytes());
        bytes.extend(0u16.to_le_bytes());
        for value in [0i16, 0, 1000, 500] {
            bytes.extend(value.to_le_bytes());
        }
        bytes.extend(1440u16.to_le_bytes());
        bytes.extend(0u32.to_le_bytes());
        let checksum = bytes.as_chunks::<2>().0.iter().fold(0u16, |sum, pair| sum ^ u16::from_le_bytes([pair[0], pair[1]]));
        bytes.extend(checksum.to_le_bytes());
    }
    bytes.extend(1u16.to_le_bytes());
    bytes.extend(9u16.to_le_bytes());
    bytes.extend(0x0300u16.to_le_bytes());
    bytes.extend((9 + body.len() as u32 / 2).to_le_bytes());
    bytes.extend(16u16.to_le_bytes());
    bytes.extend(maximum.to_le_bytes());
    bytes.extend(0u16.to_le_bytes());
    bytes.extend(body);
    bytes
}

fn mapping() -> Vec<(u16, Vec<u16>)> {
    vec![(0x020B, vec![0, 0]), (0x020C, vec![500, 1000])]
}

fn comment(payload: &[u8]) -> (u16, Vec<u16>) {
    let mut bytes = payload.to_vec();
    bytes.resize(bytes.len().next_multiple_of(2), 0);
    let mut words = vec![15, payload.len() as u16];
    words.extend(bytes.as_chunks::<2>().0.iter().map(|pair| u16::from_le_bytes([pair[0], pair[1]])));
    (0x0626, words)
}

fn assert_transparent(bytes: &[u8]) {
    let image = rasterize(bytes, 200.0).expect("a valid empty metafile has a transparent preview");
    assert_eq!(image.dimensions(), (200, 100));
    assert!(image.pixels().all(|pixel| pixel.0 == [0, 0, 0, 0]));
}

#[test]
fn valid_empty_placeable_and_plain_wmf_have_transparent_previews() {
    assert_transparent(&wmf(true, &[]));
    for placeable in [false, true] {
        let mut records = mapping();
        records.extend([(0x0103, vec![8]), (0x001E, vec![]), (0x0214, vec![20, 20]), (0x0127, vec![u16::MAX])]);
        assert_transparent(&wmf(placeable, &records));
        let mut older_version = wmf(placeable, &records);
        let header = if placeable { 22 } else { 0 };
        older_version[header + 4..header + 6].copy_from_slice(&0x0100u16.to_le_bytes());
        assert_transparent(&older_version);
        older_version[header..header + 2].copy_from_slice(&2u16.to_le_bytes());
        assert_transparent(&older_version);
    }
}

#[test]
fn ordinary_metafile_comments_do_not_make_an_empty_preview_invalid() {
    for payload in [b"".as_slice(), b"a", b"metadata"] {
        let mut records = mapping();
        records.push(comment(payload));
        for placeable in [false, true] {
            assert_transparent(&wmf(placeable, &records));
        }
    }
}

#[test]
fn invalid_empty_wmf_headers_do_not_hide_broken_images() {
    for placeable in [false, true] {
        let valid = wmf(placeable, &mapping());
        let header = if placeable { 22 } else { 0 };
        for (offset, replacement) in [(0, 0u16), (2, 8), (4, 0x0200), (16, 1)] {
            let mut bytes = valid.clone();
            bytes[header + offset..header + offset + 2].copy_from_slice(&replacement.to_le_bytes());
            assert!(rasterize(&bytes, 200.0).is_none(), "invalid header word {offset}, placeable={placeable}");
        }
        for offset in [6, 12] {
            let mut bytes = valid.clone();
            bytes[header + offset..header + offset + 4].copy_from_slice(&0u32.to_le_bytes());
            assert!(rasterize(&bytes, 200.0).is_none(), "invalid header length {offset}, placeable={placeable}");
        }
    }
    let mut bytes = wmf(true, &mapping());
    bytes[20] ^= 1;
    assert!(rasterize(&bytes, 200.0).is_none(), "invalid placeable checksum");
    for (offset, replacement) in [(4, 1u16), (14, 0), (16, 1)] {
        let mut bytes = wmf(true, &mapping());
        bytes[offset..offset + 2].copy_from_slice(&replacement.to_le_bytes());
        let checksum = bytes[..20].as_chunks::<2>().0.iter().fold(0u16, |sum, pair| sum ^ u16::from_le_bytes([pair[0], pair[1]]));
        bytes[20..22].copy_from_slice(&checksum.to_le_bytes());
        assert!(rasterize(&bytes, 200.0).is_none(), "invalid placeable header word {offset}");
    }
}

#[test]
fn truncated_or_inconsistent_empty_wmf_records_remain_invalid() {
    for placeable in [false, true] {
        let valid = wmf(placeable, &mapping());
        let header = if placeable { 22 } else { 0 };
        for end in 0..valid.len() {
            assert!(rasterize(&valid[..end], 200.0).is_none(), "truncated at {end}, placeable={placeable}");
        }
        for record_words in [0u32, 2, 4, u32::MAX] {
            let mut bytes = valid.clone();
            bytes[header + 18..header + 22].copy_from_slice(&record_words.to_le_bytes());
            assert!(rasterize(&bytes, 200.0).is_none(), "bad record size {record_words}, placeable={placeable}");
        }
        let mut missing_eof = valid.clone();
        missing_eof.truncate(missing_eof.len() - 6);
        let words = ((missing_eof.len() - header) / 2) as u32;
        missing_eof[header + 6..header + 10].copy_from_slice(&words.to_le_bytes());
        assert!(rasterize(&missing_eof, 200.0).is_none(), "missing EOF");

        let mut trailing_record = valid.clone();
        trailing_record.extend(3u32.to_le_bytes());
        trailing_record.extend(0u16.to_le_bytes());
        let words = ((trailing_record.len() - header) / 2) as u32;
        trailing_record[header + 6..header + 10].copy_from_slice(&words.to_le_bytes());
        assert!(rasterize(&trailing_record, 200.0).is_none(), "records after EOF");
    }
}

#[test]
fn background_fill_mode_and_text_alignment_allow_an_ignored_reserved_word() {
    for function in [0x0102, 0x0106, 0x012E] {
        for parameters in [vec![1], vec![1, 0xABCD]] {
            let mut records = mapping();
            records.push((function, parameters));
            for placeable in [false, true] {
                assert_transparent(&wmf(placeable, &records));
            }
        }
        for parameters in [vec![], vec![1, 0xABCD, 0]] {
            let mut records = mapping();
            records.push((function, parameters));
            for placeable in [false, true] {
                assert!(rasterize(&wmf(placeable, &records), 200.0).is_none());
            }
        }
    }
}

#[test]
fn malformed_state_and_comment_payloads_remain_invalid() {
    for record in [(0x0103, vec![]), (0x020C, vec![500]), (0x001E, vec![0]), (0x0626, vec![15]), (0x0626, vec![15, 3, 0])] {
        let mut records = mapping();
        records.push(record);
        assert!(rasterize(&wmf(true, &records), 200.0).is_none());
    }
}

#[test]
fn unsupported_drawing_and_escape_records_do_not_become_empty_previews() {
    let mut embedded_emf = b"WMFC".to_vec();
    embedded_emf.resize(34, 0);
    for record in [
        (0x7777, vec![]),
        (0x0817, vec![0; 8]),
        (0x0626, vec![19, 0]),
        comment(&embedded_emf),
        comment(b"WMFC"),
        comment(b"W"),
        comment(b"WM"),
        comment(b"WMF"),
    ] {
        let mut records = mapping();
        records.push(record);
        for placeable in [false, true] {
            assert!(rasterize(&wmf(placeable, &records), 200.0).is_none());
        }
    }
}

#[test]
fn empty_wmf_record_budget_includes_the_end_record() {
    let mut records = vec![(0x001E, vec![]); super::MAX_RECORDS - 1];
    assert!(super::empty_wmf::validated_empty(&wmf(false, &records)));
    records.push((0x001E, vec![]));
    assert!(!super::empty_wmf::validated_empty(&wmf(false, &records)));
}

#[test]
fn valid_wmf_drawing_records_still_render_pixels() {
    let mut records = mapping();
    records.extend([
        (0x02FC, vec![0, 0x00FF, 0, 0]),
        (0x012D, vec![0]),
        (0x02FA, vec![5, 0, 0, 0, 0]),
        (0x012D, vec![1]),
        (0x0324, vec![4, 0, 0, 500, 0, 500, 500, 0, 500]),
    ]);
    for placeable in [false, true] {
        let image = rasterize(&wmf(placeable, &records), 200.0).expect("supported drawing remains visible");
        assert_eq!(image.get_pixel(50, 50).0, [255, 0, 0, 255]);
        assert_eq!(image.get_pixel(150, 50).0, [0, 0, 0, 0]);
    }
}
