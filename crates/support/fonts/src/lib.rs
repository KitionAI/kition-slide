//! Kition Slide fonts: the font database (kition-fonts families, user and system fonts, and open
//! substitutes for the Office fonts slides ask for), vertical metrics, glyph outlines and
//! OpenType shaping, script/slot classification and per-cluster fallback.
//!
//! Shaping here is style-agnostic: [`shape`] and [`shape_range`] turn text in one face into glyph
//! ids, clusters and advances in font units. `kition-slide-text` applies sizes, tracking, scaling and
//! justification on top.
#![deny(clippy::unwrap_used, clippy::expect_used, clippy::panic, clippy::unimplemented, clippy::todo, clippy::unreachable)]
#![forbid(unsafe_code)]

mod fallback;
mod fontdb;
pub mod script;
mod shape;

pub use fallback::{bundled_families, covers_cluster, ligature_carets};
pub use fontdb::{
    CJK_FALLBACK_FAMILY, FALLBACK_FAMILY, FaceRef, FontDb, FontFace, LAST_RESORT_FAMILY, base_style, bundled, east_asian_substitutes,
    no_system_fonts, substitutes, system_font_dirs,
};
pub use harfrust::Feature;
use harfrust::Tag;
pub use kurbo::BezPath;
pub use script::{FontSlot, ScriptTag};
pub use shape::{ShapeParams, ShapedGlyph, shape_range};

/// A font from the optional kition-fonts build input (https://github.com/KitionAI/kition-fonts;
/// empty unless built with `KITION_FONTS_DIR`, see `build.rs`).
pub struct KitionFont {
    pub family: &'static str,
    pub style: &'static str,
    /// ISO 15924 scripts the font is for, e.g. `"Jpan"`.
    pub scripts: &'static [&'static str],
    pub bytes: &'static [u8],
}

include!(concat!(env!("OUT_DIR"), "/kition_fonts.rs"));

/// The kition-fonts faces for Japanese (`"Jpan"`), in manifest order. Empty without kition-fonts.
pub fn japanese_fonts() -> impl Iterator<Item = &'static KitionFont> {
    KITION_FONTS.iter().filter(|f| f.scripts.contains(&"Jpan"))
}

/// Japanese faces in document-fallback order: Mincho (serif) families first, matching the
/// serif default text font, then the others; Regular before other styles.
pub fn japanese_document_fonts() -> Vec<&'static KitionFont> {
    let mut v: Vec<_> = japanese_fonts().collect();
    v.sort_by_key(|f| (!f.family.contains("Mincho"), f.style != "Regular"));
    v
}

/// Japanese faces in UI order: BIZ UDPGothic first (the UI face), then the others; `bold` puts
/// bold styles before regular ones.
pub fn japanese_ui_fonts(bold: bool) -> Vec<&'static KitionFont> {
    let mut v: Vec<_> = japanese_fonts().collect();
    v.sort_by_key(|f| (f.family != "BIZ UDPGothic", (f.style == "Bold") != bold));
    v
}

/// The default theme's font.
pub const DEFAULT_FAMILY: &str = "Inter";

/// An OpenType feature setting: `"liga"`, `"-kern"`, `"ss01"`.
pub fn feature(tag: &str) -> Option<Feature> {
    let (on, t) = match tag.strip_prefix('-') {
        Some(r) => (false, r),
        None => (true, tag.strip_prefix('+').unwrap_or(tag)),
    };
    let b = t.as_bytes();
    if b.len() != 4 {
        return None;
    }
    Some(Feature::new(Tag::new(&[b[0], b[1], b[2], b[3]]), on as u32, ..))
}

/// An OpenType feature with a value (`ssty` = 2 for second-level script glyphs).
pub fn feature_value(tag: &[u8; 4], value: u32) -> Feature {
    Feature::new(Tag::new(tag), value, ..)
}

/// A strong right-to-left character (Hebrew, Arabic, …)?
pub fn is_rtl(c: char) -> bool {
    use unicode_bidi::BidiClass::{AL, R};
    matches!(unicode_bidi::bidi_class(c), R | AL)
}

/// Shape `text` with `face`. `chars` lets callers substitute characters (e.g. uppercase for All
/// Caps) while keeping clusters pointing into the original string. Glyphs come in logical
/// order: each direction run is shaped with [`shape_range`] (right-to-left runs right to left,
/// with the whole string as context), then its clusters put back in text order (each cluster's
/// glyphs keep the shaper's order). Paragraph layout uses [`shape_range`] directly and keeps the
/// visual order.
pub fn shape(face: &FontFace, text: &str, features: &[Feature], map: impl Fn(char) -> char) -> Vec<ShapedGlyph> {
    let mut out = Vec::with_capacity(text.len());
    for (r, rtl) in direction_runs(text) {
        let mut g = shape_range(face, text, r, &ShapeParams { features, rtl, ..Default::default() }, &map);
        if rtl {
            // Visual (clusters descending) → logical; the sort is stable, so each cluster's
            // glyphs keep the shaper's order.
            g.sort_by_key(|x| x.cluster);
        }
        out.extend(g);
    }
    out
}

/// Maximal runs of one direction (neutrals join the run they're in): (byte range, right-to-left).
fn direction_runs(text: &str) -> Vec<(std::ops::Range<usize>, bool)> {
    let mut runs: Vec<(std::ops::Range<usize>, bool)> = Vec::new();
    let mut cur: Option<bool> = None;
    let mut start = 0;
    for (i, c) in text.char_indices() {
        let strong = if is_rtl(c) {
            Some(true)
        } else if unicode_bidi::bidi_class(c) == unicode_bidi::BidiClass::L {
            Some(false)
        } else {
            None
        };
        match (cur, strong) {
            (None, Some(d)) => cur = Some(d),
            (Some(a), Some(d)) if a != d => {
                runs.push((start..i, a));
                start = i;
                cur = Some(d);
            }
            _ => {}
        }
    }
    runs.push((start..text.len(), cur.unwrap_or(false)));
    runs
}

/// Glyph id of the first of `chars` the face has (0 = .notdef).
pub fn first_glyph(face: &FontFace, chars: &[char]) -> u32 {
    chars.iter().map(|c| face.glyph_for(*c)).find(|g| *g != 0).unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn any_face() -> std::sync::Arc<FontFace> {
        FontDb::global().face(DEFAULT_FAMILY, "Regular")
    }

    #[test]
    fn last_resort_face_parses() {
        let f = fontdb::last_resort_face();
        assert_eq!(f.family, fontdb::LAST_RESORT_FAMILY);
        assert!(f.upem > 0.0);
    }

    #[test]
    fn some_face_always_resolves() {
        let db = FontDb::with_font_dirs(Vec::new());
        db.set_system_fallback(false);
        let f = db.face("No Such Font Anywhere", "Regular");
        assert!(!f.family.is_empty());
        assert!(!shape(&f, "Hello", &[], |c| c).is_empty());
    }

    #[test]
    fn office_fonts_have_substitutes() {
        assert!(substitutes("Calibri").contains(&"Carlito"));
        assert!(substitutes("Cambria").contains(&"Caladea"));
        assert!(substitutes("Times New Roman").contains(&"Liberation Serif"));
        assert!(substitutes("Aptos").contains(&"Inter"));
        assert!(substitutes("zzz").is_empty());
    }

    #[test]
    fn east_asian_families_have_open_substitutes() {
        // Chinese and Korean families, by Latin name (weights and PDF subset tags included) or
        // written in Han or Hangul, take the bundled CJK face.
        for family in
            ["Microsoft YaHei", "Microsoft YaHei Light", "DengXian", "SimSun", "PingFang SC", "MiSans Bold", "ABCDEF+DengXian-Bold", "Gulim"]
        {
            assert_eq!(east_asian_substitutes(family), &[CJK_FALLBACK_FAMILY], "{family}");
        }
        let han_named: String = ['\u{9ED1}', '\u{4F53}'].iter().collect();
        assert_eq!(east_asian_substitutes(&han_named), &[CJK_FALLBACK_FAMILY]);
        let hangul_named: String = ['\u{AD74}', '\u{B9BC}'].iter().collect();
        assert_eq!(east_asian_substitutes(&hangul_named), &[CJK_FALLBACK_FAMILY]);
        // Japanese ones prefer the openly licensed BIZ UD faces when present.
        assert_eq!(east_asian_substitutes("MS PGothic").first(), Some(&"BIZ UDPGothic"));
        assert_eq!(east_asian_substitutes("Yu Mincho").first(), Some(&"BIZ UDPMincho"));
        let mincho_named: String = ['\u{30D2}', '\u{660E}', '\u{671D}'].iter().collect();
        assert_eq!(east_asian_substitutes(&mincho_named).first(), Some(&"BIZ UDPMincho"));
        assert!(east_asian_substitutes("Yu Mincho").contains(&CJK_FALLBACK_FAMILY));
        // Never a Microsoft, Adobe or Noto CJK face, and nothing for Latin families.
        for family in ["Microsoft YaHei", "Meiryo", "Batang", "Source Han Sans CN"] {
            assert!(east_asian_substitutes(family).iter().all(|s| !s.contains("Microsoft") && !s.contains("Noto") && !s.contains("Source")));
        }
        assert!(east_asian_substitutes("Calibri").is_empty());
        assert!(east_asian_substitutes("Inter").is_empty());
        assert!(east_asian_substitutes("ABCDEF+Arial").is_empty());
        // The general substitutes leave East Asian families alone: their Latin text keeps the
        // general fallback, as the CJK face has no Latin letters.
        assert!(substitutes("Microsoft YaHei").is_empty());
    }

    #[test]
    fn an_installed_family_is_found_by_its_name_in_another_language() {
        // macOS ships PingFang SC, which also names itself in Chinese; documents made in Chinese
        // ask for it by that name.
        let db = FontDb::global();
        db.load_system_fonts();
        if !db.has_family("PingFang SC") {
            eprintln!("skipped: PingFang SC is not installed");
            return;
        }
        let chinese: String = ['\u{82F9}', '\u{65B9}', '-', '\u{7B80}'].iter().collect();
        assert!(db.has_family(&chinese));
        let face = db.face(&chinese, "Regular");
        assert_eq!(face.family, "PingFang SC");
        assert!(face.is_named("pingfang sc") && face.is_named(&chinese));
    }

    #[test]
    fn shaping_produces_clusters_and_advances() {
        let face = any_face();
        let g = shape(&face, "Hello", &[], |c| c);
        assert_eq!(g.len(), 5);
        assert_eq!(g.iter().map(|g| g.cluster).collect::<Vec<_>>(), vec![0, 1, 2, 3, 4]);
        assert!(g.iter().all(|g| g.x_advance > 0 && g.gid != 0));
    }

    #[test]
    fn mapping_keeps_clusters() {
        let face = any_face();
        let g = shape(&face, "ab", &[], |c| c.to_ascii_uppercase());
        assert_eq!(g[0].gid, face.glyph_for('A'));
        assert_eq!(g[1].cluster, 1);
    }

    #[test]
    fn outlines_and_metrics() {
        let db = FontDb::global();
        let face = any_face();
        let o = db.outline(&face, face.glyph_for('O'));
        assert!(!o.elements().is_empty());
        assert!(face.ascent > 0.0 && face.descent > 0.0);
        assert!(feature("abc").is_none());
        assert!(feature("liga").is_some());
    }

    #[test]
    fn kition_fonts_latin_when_present() {
        if !KITION_FONTS.iter().any(|f| f.family == "Inter") {
            eprintln!("skipped: built without kition-fonts' Latin manifest (set KITION_FONTS_DIR)");
            return;
        }
        let db = FontDb::with_font_dirs(Vec::new());
        db.set_system_fallback(false);
        assert_eq!(db.face("Inter", "Regular").family, "Inter");
        assert_eq!(db.face("Aptos", "Regular").family, "Inter");
        // The metric-compatible stand-ins (decision F1), where the checkout has them.
        for (asked, open) in
            [("Calibri", "Carlito"), ("Cambria", "Caladea"), ("Arial", "Arimo"), ("Times New Roman", "Tinos"), ("Courier New", "Cousine")]
        {
            if KITION_FONTS.iter().any(|f| f.family == open) {
                assert_eq!(db.face(asked, "Bold").family, open, "{asked}");
            }
        }
    }
}
