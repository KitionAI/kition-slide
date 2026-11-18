use super::*;

fn range_font() -> Vec<u8> {
    // Derive transient test bytes from the existing openly licensed bundled font.
    // No system font, new font asset or modified font file is required.
    let mut bytes = epaint_default_fonts::UBUNTU_LIGHT.to_vec();
    let count = usize::from(u16::from_be_bytes(bytes[4..6].try_into().unwrap()));
    let offset = bytes[12..12 + count * 16]
        .as_chunks::<16>()
        .0
        .iter()
        .find(|record| &record[..4] == b"head")
        .map(|record| u32::from_be_bytes(record[8..12].try_into().unwrap()) as usize)
        .unwrap();
    let flags = u16::from_be_bytes(bytes[offset + 16..offset + 18].try_into().unwrap()) | (1 << 14);
    bytes[offset + 16..offset + 18].copy_from_slice(&flags.to_be_bytes());
    bytes
}

fn face(symbolic: bool) -> Arc<FontFace> {
    let bytes = if symbolic { FontBytes::Owned(Arc::new(range_font())) } else { FontBytes::Static(epaint_default_fonts::UBUNTU_LIGHT) };
    Arc::new(make_face(bytes, 0, if symbolic { "Range Symbols" } else { "Character Face" }.into(), "Regular".into(), vec![]).unwrap())
}

#[test]
fn generic_range_font_keeps_glyph_mapping_without_claiming_coverage() {
    let symbolic = face(true);
    let normal = face(false);
    for ch in ['A', 'a', '7'] {
        assert_ne!(symbolic.glyph_for(ch), 0);
        assert_eq!(symbolic.glyph_for(ch), normal.glyph_for(ch));
        assert!(!symbolic.covers(ch));
        assert!(normal.covers(ch));
    }
    assert!(symbolic_ranges(&symbolic.skrifa().unwrap()));
    assert!(!symbolic_ranges(&normal.skrifa().unwrap()));
}

#[test]
fn range_symbols_do_not_block_later_character_fallback_or_requested_faces() {
    let db = FontDb::with_font_dirs(vec![]);
    db.set_system_fallback(false);
    let symbolic = face(true);
    let normal = face(false);
    *db.faces.write().unwrap() = vec![symbolic.clone(), normal.clone()];
    assert_eq!(db.fallback_for('A', u32::MAX).unwrap().id(), normal.id());
    assert_eq!(db.cascade(&['A'], std::slice::from_ref(&symbolic), "Regular").id(), normal.id());
    assert_eq!(db.cascade(&['A'], std::slice::from_ref(&normal), "Regular").id(), normal.id());
    assert!(!crate::covers_cluster(&symbolic, &['A']));
    *db.faces.write().unwrap() = vec![symbolic];
    assert!(db.fallback_for('A', u32::MAX).is_none());
}
