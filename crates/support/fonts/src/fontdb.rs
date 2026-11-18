//! Font database: bundled OFL fonts, user fonts, the installed system fonts (cataloged the first
//! time a lookup needs them, native only), outline cache.

use std::collections::HashMap;
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::{Arc, Mutex, RwLock};

use kurbo::BezPath;
use skrifa::instance::{Location, LocationRef, Size};
use skrifa::outline::{DrawSettings, OutlinePen};
use skrifa::raw::{FileRef, TableProvider};
use skrifa::string::StringId;
use skrifa::{GlyphId, MetadataProvider};

/// The family used when a requested family is unknown and has no substitute.
pub const FALLBACK_FAMILY: &str = "Inter";

/// The always-available last-resort face (Ubuntu Light, compiled in through
/// `epaint_default_fonts`; licence in docs/attribution.md).
pub const LAST_RESORT_FAMILY: &str = "Ubuntu";

/// Compiled-in font files: the kition-fonts faces (Latin slide fonts, then Japanese;
/// empty without `KITION_FONTS_DIR`), then the last-resort face.
pub fn bundled() -> Vec<&'static [u8]> {
    let mut v: Vec<&'static [u8]> = crate::KITION_FONTS.iter().map(|f| f.bytes).collect();
    v.push(epaint_default_fonts::UBUNTU_LIGHT);
    v
}

/// Open substitutes for families documents often ask for but we can't ship (metric-compatible
/// where one exists), tried in order after the family itself. East Asian families have their
/// own list for East Asian text ([`east_asian_substitutes`]).
pub fn substitutes(family: &str) -> &'static [&'static str] {
    match family.to_ascii_lowercase().as_str() {
        "calibri" | "calibri light" => &["Carlito", "Liberation Sans", "Arial", "Inter"],
        "cambria" | "cambria math" => &["Caladea", "Liberation Serif", "Times New Roman", "Merriweather"],
        // The author's math companion to DejaVu Serif. These are serif fallbacks,
        // not claims of matching metrics or OpenType MATH capabilities.
        // https://ctan.org/tex-archive/fonts/tex-gyre-math
        "dejavu math tex gyre" | "tex gyre dejavu math" => &["DejaVu Serif", "Caladea", "Liberation Serif", "Times New Roman", "Merriweather"],
        "arial" | "helvetica" | "helvetica neue" | "arial nova" => &["Arimo", "Liberation Sans", "Arial", "Helvetica Neue", "Helvetica", "Inter"],
        "times new roman" | "times" => &["Tinos", "Liberation Serif", "Times New Roman", "Times", "Merriweather"],
        "courier new" | "courier" | "consolas" | "menlo" => &["Cousine", "Liberation Mono", "Courier New", "Menlo", "Courier"],
        "aptos" | "aptos display" | "aptos narrow" | "segoe ui" | "segoe ui light" | "segoe ui semibold" | "tahoma" | "verdana" => {
            &["Inter", "Helvetica Neue", "Arial", "Liberation Sans"]
        }
        "georgia" | "garamond" | "book antiqua" | "palatino" | "palatino linotype" | "constantia" => &["Merriweather", "Liberation Serif", "Georgia"],
        "century gothic" | "avenir" | "avenir next" | "futura" | "gill sans" | "gill sans mt" | "trebuchet ms" | "corbel" | "candara" => {
            &["Montserrat", "Poppins", "Avenir Next", "Inter"]
        }
        "franklin gothic" | "franklin gothic book" | "franklin gothic medium" | "univers" => &["Open Sans", "Inter"],
        "open sans" | "lato" | "roboto" | "montserrat" | "poppins" | "nunito sans" => &["Inter"],
        "playfair display" | "merriweather" | "source serif 4" | "source serif pro" => &["Merriweather", "Liberation Serif", "Georgia"],
        "source sans 3" | "source sans pro" => &["Open Sans", "Inter"],
        "impact" | "arial black" => &["Montserrat", "Inter"],
        _ => &[],
    }
}

/// The bundled face (kition-fonts, Apache-2.0) that draws Chinese, Japanese and Korean text when
/// the family a document asks for is missing.
pub const CJK_FALLBACK_FAMILY: &str = "Droid Sans Fallback";

/// What kind of East Asian face a family name asks for.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum EastAsianKind {
    /// Japanese gothic (sans): MS Gothic, Meiryo, Yu Gothic, Hiragino Kaku Gothic…
    JapaneseGothic,
    /// Japanese mincho (serif): MS Mincho, Yu Mincho…
    JapaneseMincho,
    /// Chinese or Korean, any style (hei, song, kai…).
    Other,
}

/// Latin names of Chinese and Korean families documents ask for (lowercase, letters and digits
/// only), matched as prefixes so weights (`… Light`, `… Bold`) match too.
const CJK_LATIN_PREFIXES: &[&str] = &[
    "microsoftyahei",
    "microsoftjhenghei",
    "dengxian",
    "simsun",
    "nsimsun",
    "simhei",
    "simkai",
    "simfang",
    "kaiti",
    "fangsong",
    "youyuan",
    "lisu",
    "stxihei",
    "stheiti",
    "stsong",
    "stkaiti",
    "stfangsong",
    "stzhongsong",
    "pingfang",
    "sourcehansans",
    "sourcehanserif",
    "notosanscjk",
    "notoserifcjk",
    "notosanssc",
    "notoserifsc",
    "misans",
    "harmonyossans",
    "hiraginosansgb",
    "heiti",
    "songti",
    "pmingliu",
    "mingliu",
    "dfkai",
    "fzhei",
    "fzshusong",
    "fzlantinghei",
    "hanyi",
    "wenquanyi",
    "gulim",
    "batang",
    "dotum",
    "gungsuh",
    "malgungothic",
    "nanum",
];

/// Latin names of Japanese families (normalized like [`CJK_LATIN_PREFIXES`]).
const JAPANESE_LATIN_PREFIXES: &[&str] =
    &["msgothic", "mspgothic", "msuigothic", "msmincho", "mspmincho", "meiryo", "yugothic", "yumincho", "hiraginokaku", "hiraginomincho", "bizud"];

/// A family name with a PDF subset prefix (`ABCDEF+DengXian-Bold`) reduced to the family.
fn strip_subset_prefix(family: &str) -> &str {
    match family.split_once('+') {
        Some((tag, rest)) if tag.len() == 6 && tag.bytes().all(|b| b.is_ascii_uppercase()) => rest,
        _ => family,
    }
}

/// Is `family` an East Asian family, and which kind? Names written in Han, kana or Hangul count
/// whatever they say; Latin names are matched against the families documents commonly use.
fn east_asian_kind(family: &str) -> Option<EastAsianKind> {
    let family = strip_subset_prefix(family.trim());
    let n = norm(family);
    let kana = family.chars().any(|c| matches!(c as u32, 0x3040..=0x30FF | 0x31F0..=0x31FF | 0xFF66..=0xFF9F));
    let han_or_hangul = family.chars().any(|c| matches!(c as u32, 0x3400..=0x4DBF | 0x4E00..=0x9FFF | 0xF900..=0xFAFF | 0xAC00..=0xD7AF));
    // Serif Japanese faces say "Mincho", in Latin letters or in Han (U+660E U+671D).
    let mincho = n.contains("mincho") || (family.contains('\u{660E}') && family.contains('\u{671D}'));
    if kana || JAPANESE_LATIN_PREFIXES.iter().any(|p| n.starts_with(p)) {
        return Some(if mincho { EastAsianKind::JapaneseMincho } else { EastAsianKind::JapaneseGothic });
    }
    if han_or_hangul || CJK_LATIN_PREFIXES.iter().any(|p| n.starts_with(p)) {
        return Some(EastAsianKind::Other);
    }
    None
}

/// Open substitutes for East Asian families documents ask for but we can't ship (Microsoft
/// YaHei, DengXian, SimSun, PingFang, Source Han…): bundled or openly licensed CJK faces that
/// draw their Han, kana and Hangul characters, tried in order (the Japanese BIZ UD faces when
/// kition-fonts provides them, then the bundled CJK face). Empty for other families. Only East
/// Asian text uses them: these faces have no Latin letters, so the family's Latin text takes
/// the general fallback of [`FontDb::face`].
pub fn east_asian_substitutes(family: &str) -> &'static [&'static str] {
    match east_asian_kind(family) {
        Some(EastAsianKind::JapaneseGothic) => &["BIZ UDPGothic", "BIZ UDGothic", CJK_FALLBACK_FAMILY],
        Some(EastAsianKind::JapaneseMincho) => &["BIZ UDPMincho", "BIZ UDMincho", CJK_FALLBACK_FAMILY],
        Some(EastAsianKind::Other) => &[CJK_FALLBACK_FAMILY],
        None => &[],
    }
}

#[derive(Clone)]
enum FontBytes {
    Static(&'static [u8]),
    Owned(Arc<Vec<u8>>),
}

/// One loaded font face.
pub struct FontFace {
    id: u32,
    /// Typographic family name (e.g. "Source Sans 3").
    pub family: String,
    /// Typographic style name (e.g. "Semibold", "Italic").
    pub style: String,
    /// The family's names in other languages ([`family_aliases`]).
    aliases: Vec<String>,
    /// usWeightClass-style weight (400 = regular).
    pub weight: f32,
    pub italic: bool,
    bytes: FontBytes,
    index: u32,
    pub upem: f64,
    /// Ascender in font units (positive = up).
    pub ascent: f64,
    /// Descender in font units (positive = down).
    pub descent: f64,
    /// Cap height and x height in font units (estimated from the ascent when the font has no OS/2
    /// values).
    pub cap_height: f64,
    pub x_height: f64,
    pub shaper: harfrust::ShaperData,
    /// Variable fonts: the named instance's axis settings (user units; empty = default instance),
    /// the normalised location and the shaper's view of it.
    pub coords: Vec<([u8; 4], f32)>,
    location: Location,
    pub instance: Option<harfrust::ShaperInstance>,
    /// Basic Multilingual Plane coverage bitset, built on first use.
    bmp: std::sync::OnceLock<Box<[u64]>>,
    /// OpenType generic range symbols do not establish real character coverage.
    symbolic_ranges: bool,
}

/// A cheap `Copy` handle to a face. Faces are never unloaded, so the handle lives for the rest of
/// the process; unlike cloning an `Arc`, copying it touches no shared reference count (which made
/// parallel composition scale negatively).
#[derive(Clone, Copy)]
pub struct FaceRef(&'static FontFace);

impl FaceRef {
    /// The handle for a loaded face (memoised per face).
    pub fn of(face: &Arc<FontFace>) -> FaceRef {
        static LEAKED: RwLock<Vec<Option<&'static FontFace>>> = RwLock::new(Vec::new());
        let id = face.id as usize;
        if let Some(Some(f)) = LEAKED.read().unwrap_or_else(|e| e.into_inner()).get(id) {
            return FaceRef(f);
        }
        let mut w = LEAKED.write().unwrap_or_else(|e| e.into_inner());
        if w.len() <= id {
            w.resize(id + 1, None);
        }
        let f: &'static FontFace = w[id].unwrap_or_else(|| {
            let keep: &'static Arc<FontFace> = Box::leak(Box::new(face.clone()));
            keep
        });
        w[id] = Some(f);
        FaceRef(f)
    }
    pub fn get(self) -> &'static FontFace {
        self.0
    }
}

impl std::ops::Deref for FaceRef {
    type Target = FontFace;
    fn deref(&self) -> &FontFace {
        self.0
    }
}

impl std::fmt::Debug for FaceRef {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.0.fmt(f)
    }
}

impl PartialEq for FaceRef {
    fn eq(&self, o: &FaceRef) -> bool {
        self.0.id == o.0.id
    }
}

impl std::fmt::Debug for FontFace {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "FontFace({} {})", self.family, self.style)
    }
}

impl FontFace {
    /// Is this face of the family asked for as `family` (by its English or another name)?
    pub fn is_named(&self, family: &str) -> bool {
        named(&self.family, &self.aliases, family)
    }

    pub fn data(&self) -> &[u8] {
        match &self.bytes {
            FontBytes::Static(b) => b,
            FontBytes::Owned(v) => v.as_slice(),
        }
    }
    pub fn skrifa(&self) -> Option<skrifa::FontRef<'_>> {
        skrifa::FontRef::from_index(self.data(), self.index).ok()
    }
    pub fn hb(&self) -> Option<harfrust::FontRef<'_>> {
        harfrust::FontRef::from_index(self.data(), self.index).ok()
    }
    /// Where in the design space glyphs, metrics and outlines come from.
    pub fn location(&self) -> LocationRef<'_> {
        (&self.location).into()
    }
    /// Is this a named instance of a variable font?
    pub fn is_variable(&self) -> bool {
        !self.coords.is_empty()
    }
    /// Face index within the font file (collections); 0 for plain fonts.
    pub fn index(&self) -> u32 {
        self.index
    }
    /// Unique id of this face within the process.
    pub fn id(&self) -> u32 {
        self.id
    }
    /// Does the face support `c` with a character glyph rather than a range placeholder?
    pub fn covers(&self, c: char) -> bool {
        if self.symbolic_ranges {
            return false;
        }
        let cp = c as u32;
        if cp < 0x1_0000 {
            let bits = self.bmp.get_or_init(|| {
                let mut b = vec![0u64; 1024].into_boxed_slice();
                if let Some(f) = self.skrifa() {
                    for (cp, _) in f.charmap().mappings() {
                        if cp < 0x1_0000 {
                            b[(cp / 64) as usize] |= 1 << (cp % 64);
                        }
                    }
                }
                b
            });
            return bits[(cp / 64) as usize] & (1 << (cp % 64)) != 0;
        }
        self.skrifa().is_some_and(|f| f.charmap().map(c).is_some())
    }
    /// Units per em.
    pub fn units_per_em(&self) -> f64 {
        self.upem
    }
    /// (ascent, descent) in font units, both positive.
    pub fn vertical_metrics(&self) -> (f64, f64) {
        (self.ascent, self.descent)
    }
    /// Every mapped character and its glyph id, sorted by code point (the Glyphs panel).
    pub fn chars(&self) -> Vec<(char, u32)> {
        let Some(f) = self.skrifa() else { return vec![] };
        let mut v: Vec<(char, u32)> = f.charmap().mappings().filter_map(|(cp, g)| char::from_u32(cp).map(|c| (c, g.to_u32()))).collect();
        v.sort_unstable_by_key(|x| x.0);
        v.dedup_by_key(|x| x.0);
        v
    }
    /// Advance width of glyph `gid` in font units.
    pub fn advance(&self, gid: u32) -> f64 {
        self.skrifa()
            .and_then(|f| f.glyph_metrics(Size::unscaled(), self.location()).advance_width(GlyphId::new(gid)))
            .map(|a| a as f64)
            .unwrap_or(self.upem * 0.5)
    }
    /// Glyph id for `c` (0 = .notdef).
    pub fn glyph_for(&self, c: char) -> u32 {
        self.skrifa().and_then(|f| f.charmap().map(c)).map(|g| g.to_u32()).unwrap_or(0)
    }
}

#[cfg(not(target_arch = "wasm32"))]
#[derive(Clone, Debug)]
struct CatalogEntry {
    family: String,
    style: String,
    /// The family's names in other languages ([`family_aliases`]).
    aliases: Vec<String>,
    path: std::path::PathBuf,
}

#[cfg(not(target_arch = "wasm32"))]
impl CatalogEntry {
    fn is_named(&self, family: &str) -> bool {
        named(&self.family, &self.aliases, family)
    }
}

/// Process-wide font database.
pub struct FontDb {
    faces: RwLock<Vec<Arc<FontFace>>>,
    outlines: Mutex<HashMap<(u32, u32), Arc<BezPath>>>,
    #[cfg(not(target_arch = "wasm32"))]
    catalog: RwLock<Vec<CatalogEntry>>,
    /// The folders the system font scan reads (the platform's font folders for [`FontDb::global`]).
    #[cfg_attr(target_arch = "wasm32", allow(dead_code))]
    font_dirs: Vec<std::path::PathBuf>,
    /// Set once the font folders have been scanned. Lookups by family name wait for the first
    /// scan, so what they find doesn't depend on what ran before them.
    #[cfg(not(target_arch = "wasm32"))]
    cataloged: std::sync::OnceLock<()>,
    /// System fallback state: enabled, and characters no system font covers.
    #[cfg(not(target_arch = "wasm32"))]
    sys: Mutex<SysFallback>,
}

#[cfg(not(target_arch = "wasm32"))]
#[derive(Default)]
struct SysFallback {
    enabled: bool,
    misses: std::collections::HashSet<char>,
}

/// Families tried (when installed) for characters the loaded fonts lack: CJK, symbols, emoji.
#[cfg(not(target_arch = "wasm32"))]
const SYSTEM_FALLBACKS: &[&str] = &[
    "Helvetica Neue",
    "Arial",
    "Segoe UI",
    "Noto Sans",
    "DejaVu Sans",
    "PingFang SC",
    "Hiragino Sans",
    "Hiragino Kaku Gothic ProN",
    "Apple SD Gothic Neo",
    "Heiti SC",
    "STHeiti",
    "Microsoft YaHei",
    "Yu Gothic",
    "Malgun Gothic",
    "Noto Sans CJK SC",
    "Noto Sans CJK JP",
    "Arial Unicode MS",
    "Apple Symbols",
    "Segoe UI Symbol",
    "Noto Sans Symbols",
    "Noto Sans Symbols2",
    "Noto Emoji",
    "Segoe UI Emoji",
    "Apple Color Emoji",
    "Noto Color Emoji",
];

static NEXT_ID: AtomicU32 = AtomicU32::new(1);
const OUTLINE_CACHE_MAX: usize = 50_000;

fn name(font: &skrifa::FontRef<'_>, ids: &[StringId]) -> Option<String> {
    ids.iter().find_map(|id| font.localized_strings(*id).english_or_first().map(|s| s.to_string()).filter(|s| !s.is_empty()))
}

/// The family names a face gives in languages other than English (Chinese `PingFang SC` names
/// itself in Chinese too). Documents name fonts in the language they were made in, so a face is
/// found by any of its names.
fn family_aliases(font: &skrifa::FontRef<'_>, family: &str) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    for id in [StringId::TYPOGRAPHIC_FAMILY_NAME, StringId::FAMILY_NAME] {
        for s in font.localized_strings(id) {
            let s = s.to_string();
            if !s.is_empty() && !s.eq_ignore_ascii_case(family) && !out.iter().any(|o| o.eq_ignore_ascii_case(&s)) {
                out.push(s);
            }
        }
    }
    out
}

/// Is a face of `family` (also called `aliases`) asked for as `wanted`?
fn named(family: &str, aliases: &[String], wanted: &str) -> bool {
    family.eq_ignore_ascii_case(wanted) || aliases.iter().any(|a| a.eq_ignore_ascii_case(wanted))
}

/// A face's family and style names (the default instance's style for a variable font).
fn face_names(f: &skrifa::FontRef<'_>) -> Option<(String, String)> {
    let family = name(f, &[StringId::TYPOGRAPHIC_FAMILY_NAME, StringId::FAMILY_NAME])?;
    let style = name(f, &[StringId::TYPOGRAPHIC_SUBFAMILY_NAME, StringId::SUBFAMILY_NAME]).unwrap_or_else(|| "Regular".into());
    Some((family, style))
}

/// (family, style) of every face in the font file at `path`, reading only its table directories
/// and `name` tables: a scan opens hundreds of font files, many of them megabytes long. A
/// variable font is cataloged under its default style; its named instances appear once it loads.
#[cfg(not(target_arch = "wasm32"))]
fn file_face_names(path: &std::path::Path) -> Vec<(String, String, Vec<String>)> {
    use std::io::{Read, Seek, SeekFrom};
    /// Caps on what a (possibly damaged) file can make the scan read.
    const MAX_FACES: u32 = 256;
    const MAX_NAME_TABLE: u32 = 1 << 20;
    let Ok(mut file) = std::fs::File::open(path) else { return vec![] };
    let mut read_at = |offset: u64, len: usize| -> Option<Vec<u8>> {
        let mut buf = vec![0; len];
        file.seek(SeekFrom::Start(offset)).ok()?;
        file.read_exact(&mut buf).ok()?;
        Some(buf)
    };
    let be32 = |b: &[u8], at: usize| b.get(at..at + 4).and_then(|s| s.try_into().ok()).map(u32::from_be_bytes);
    let Some(head) = read_at(0, 12) else { return vec![] };
    // A collection lists where each face's table directory starts.
    let starts: Vec<u32> = if head.starts_with(b"ttcf") {
        let n = be32(&head, 8).unwrap_or(0).min(MAX_FACES) as usize;
        read_at(12, n * 4).map(|b| b.as_chunks::<4>().0.iter().map(|c| u32::from_be_bytes(*c)).collect()).unwrap_or_default()
    } else {
        vec![0]
    };
    starts
        .into_iter()
        .filter_map(|start| {
            let dir = read_at(start.into(), 12)?;
            let tables = u16::from_be_bytes(dir.get(4..6)?.try_into().ok()?) as usize;
            let records = read_at(u64::from(start) + 12, tables * 16)?;
            let rec = records.as_chunks::<16>().0.iter().find(|r| r.starts_with(b"name"))?;
            let (offset, len) = (be32(rec, 8)?, be32(rec, 12)?);
            if len > MAX_NAME_TABLE {
                return None;
            }
            let table = read_at(offset.into(), len as usize)?;
            // A one-table font holding just the `name` table, to read it as the font itself would.
            let mut font = Vec::with_capacity(28 + table.len());
            font.extend_from_slice(&0x0001_0000_u32.to_be_bytes());
            font.extend_from_slice(&[0, 1, 0, 16, 0, 0, 0, 0]);
            font.extend_from_slice(b"name");
            for v in [0, 28, len] {
                font.extend_from_slice(&u32::to_be_bytes(v));
            }
            font.extend_from_slice(&table);
            let f = skrifa::FontRef::new(&font).ok()?;
            let (family, style) = face_names(&f)?;
            let aliases = family_aliases(&f, &family);
            Some((family, style, aliases))
        })
        .collect()
}

/// Set once this process serves a host that owns every file ([`no_system_fonts`]).
static NO_SYSTEM_FONTS: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

/// Stops this process reading the machine's font folders, for good: a kit served by a host reads
/// nothing outside its workspace and its own package (KHP A29, section 10.6), and what its text
/// is drawn with and its answers say must not depend on the fonts this machine has. From here on
/// [`system_font_dirs`] is empty, the system font catalog of [`FontDb::global`] is empty and never
/// scanned again, and no system font is loaded as a fallback; the compiled-in faces and the fonts
/// added with [`FontDb::add_font`] stay. Faces read from the machine before the call stay
/// loaded, so call it before the first document is opened.
pub fn no_system_fonts() {
    NO_SYSTEM_FONTS.store(true, std::sync::atomic::Ordering::SeqCst);
    let db = FontDb::global();
    db.set_system_fallback(false);
    #[cfg(not(target_arch = "wasm32"))]
    {
        // Mark the catalog made (an empty scan) and forget anything a scan before found.
        db.ensure_catalog();
        db.catalog.write().unwrap_or_else(|e| e.into_inner()).clear();
    }
}

/// Whether [`no_system_fonts`] was called in this process.
fn system_fonts_off() -> bool {
    NO_SYSTEM_FONTS.load(std::sync::atomic::Ordering::SeqCst)
}

/// The platform's font folders (the system's and the user's), scanned by [`FontDb::global`].
/// Empty on wasm, and once [`no_system_fonts`] was called.
pub fn system_font_dirs() -> Vec<std::path::PathBuf> {
    let mut dirs: Vec<std::path::PathBuf> = Vec::new();
    if cfg!(target_arch = "wasm32") || system_fonts_off() {
        return dirs;
    }
    let home = std::env::var_os("HOME").map(std::path::PathBuf::from);
    if cfg!(target_os = "macos") {
        dirs.extend(["/System/Library/Fonts", "/Library/Fonts"].map(Into::into));
        if let Some(h) = &home {
            dirs.push(h.join("Library/Fonts"));
        }
        // Fonts macOS ships as assets (PingFang, Yu Mincho, …): com_apple_MobileAsset_Font<N>.
        for parent in ["/System/Library/AssetsV2", "/System/Library/AssetsV2/PreinstalledAssetsV2/InstallWithOs"] {
            let Ok(entries) = std::fs::read_dir(parent) else { continue };
            for e in entries.flatten() {
                if e.file_name().to_string_lossy().starts_with("com_apple_MobileAsset_Font") {
                    dirs.push(e.path());
                }
            }
        }
    } else if cfg!(windows) {
        let root = std::env::var_os("WINDIR").map(std::path::PathBuf::from).unwrap_or_else(|| "C:\\Windows".into());
        dirs.push(root.join("Fonts"));
        if let Some(l) = std::env::var_os("LOCALAPPDATA") {
            dirs.push(std::path::PathBuf::from(l).join("Microsoft\\Windows\\Fonts"));
        }
    } else {
        dirs.extend(["/usr/share/fonts", "/usr/local/share/fonts"].map(Into::into));
        if let Some(h) = &home {
            dirs.push(h.join(".fonts"));
            dirs.push(h.join(".local/share/fonts"));
        }
    }
    dirs
}

/// One face found in a font file: index, family, style and (variable fonts) the named instance's
/// axis settings.
type Found = (u32, String, String, Vec<([u8; 4], f32)>);

/// Parse every face in `data` (a font file or collection); a variable font yields one face per
/// named instance (InDesign lists them as styles).
fn enumerate_faces(data: &[u8]) -> Vec<Found> {
    let count = match FileRef::new(data) {
        Ok(FileRef::Font(_)) => 1,
        Ok(FileRef::Collection(c)) => c.len(),
        Err(_) => 0,
    };
    let mut out = Vec::new();
    for i in 0..count {
        let Ok(f) = skrifa::FontRef::from_index(data, i) else { continue };
        let Some((family, style)) = face_names(&f) else { continue };
        let axes = f.axes();
        let mut named = Vec::new();
        for ni in f.named_instances().iter() {
            let Some(style) = name(&f, &[ni.subfamily_name_id()]) else { continue };
            if named.iter().any(|(_, s, _): &(u32, String, _)| s.eq_ignore_ascii_case(&style)) {
                continue;
            }
            let coords: Vec<([u8; 4], f32)> = axes.iter().zip(ni.user_coords()).map(|(a, v)| (a.tag().to_be_bytes(), v)).collect();
            named.push((i, style, coords));
        }
        if named.is_empty() {
            out.push((i, family, style, Vec::new()));
        } else {
            out.extend(named.into_iter().map(|(i, s, c)| (i, family.clone(), s, c)));
        }
    }
    out
}

/// The compiled-in last-resort face, for a database that somehow has no usable faces at all.
pub(crate) fn last_resort_face() -> Arc<FontFace> {
    static FACE: std::sync::OnceLock<Arc<FontFace>> = std::sync::OnceLock::new();
    FACE.get_or_init(|| {
        // The font is compiled in (`include_bytes!`), so parsing it can't depend on input; the
        // `last_resort_face_parses` test proves it on every run.
        #[allow(clippy::expect_used)]
        let face = make_face(FontBytes::Static(epaint_default_fonts::UBUNTU_LIGHT), 0, LAST_RESORT_FAMILY.into(), "Regular".into(), Vec::new())
            .expect("the compiled-in last-resort face parses");
        Arc::new(face)
    })
    .clone()
}

/// OpenType head.flags bit 14 marks generic code-point range symbols, not coverage.
/// https://learn.microsoft.com/en-us/typography/opentype/spec/head
fn symbolic_ranges(font: &skrifa::FontRef<'_>) -> bool {
    font.head().is_ok_and(|head| head.flags().contains(skrifa::raw::tables::head::Flags::LAST_RESORT_FONT))
}

fn make_face(bytes: FontBytes, index: u32, family: String, style: String, coords: Vec<([u8; 4], f32)>) -> Option<FontFace> {
    let data: &[u8] = match &bytes {
        FontBytes::Static(b) => b,
        FontBytes::Owned(v) => v.as_slice(),
    };
    let f = skrifa::FontRef::from_index(data, index).ok()?;
    let settings: Vec<(skrifa::Tag, f32)> = coords.iter().map(|(t, v)| (skrifa::Tag::new(t), *v)).collect();
    let location = if coords.is_empty() { Location::default() } else { f.axes().location(settings.iter().copied()) };
    let m = f.metrics(Size::unscaled(), &location);
    let a = f.attributes();
    let hb = harfrust::FontRef::from_index(data, index).ok()?;
    let shaper = harfrust::ShaperData::new(&hb);
    let instance = (!coords.is_empty())
        .then(|| harfrust::ShaperInstance::from_variations(&hb, settings.iter().map(|(t, v)| harfrust::Variation { tag: *t, value: *v })));
    let axis = |tag: &[u8; 4]| coords.iter().find(|(t, _)| t == tag).map(|(_, v)| *v);
    let weight = axis(b"wght").unwrap_or(if coords.is_empty() { a.weight.value() } else { style_weight(&style) });
    let italic = axis(b"ital")
        .map(|v| v >= 0.5)
        .or(axis(b"slnt").map(|v| v.abs() > 0.1))
        .unwrap_or(!matches!(a.style, skrifa::attribute::Style::Normal) || style_italic(&style));
    let aliases = family_aliases(&f, &family);
    let symbolic_ranges = symbolic_ranges(&f);
    Some(FontFace {
        id: NEXT_ID.fetch_add(1, Ordering::Relaxed),
        family,
        style,
        aliases,
        weight,
        italic,
        upem: m.units_per_em.max(1) as f64,
        ascent: m.ascent as f64,
        descent: -(m.descent as f64),
        cap_height: m.cap_height.map(|v| v as f64).filter(|v| *v > 0.0).unwrap_or(m.ascent as f64 * 0.72),
        x_height: m.x_height.map(|v| v as f64).filter(|v| *v > 0.0).unwrap_or(m.ascent as f64 * 0.5),
        shaper,
        coords,
        location,
        instance,
        bytes,
        index,
        bmp: std::sync::OnceLock::new(),
        symbolic_ranges,
    })
}

/// The named style part of a style with axis settings (`Bold {wght:650}` → `Bold`).
pub fn base_style(style: &str) -> &str {
    style.split('{').next().unwrap_or(style).trim()
}

fn norm(s: &str) -> String {
    s.chars().filter(|c| c.is_alphanumeric()).flat_map(|c| c.to_lowercase()).collect()
}

/// Weight implied by a style name.
fn style_weight(style: &str) -> f32 {
    let s = norm(style);
    const TABLE: &[(&str, f32)] = &[
        ("extralight", 200.0),
        ("ultralight", 200.0),
        ("semibold", 600.0),
        ("demibold", 600.0),
        ("extrabold", 800.0),
        ("ultrabold", 800.0),
        ("hairline", 100.0),
        ("thin", 100.0),
        ("light", 300.0),
        ("medium", 500.0),
        ("bold", 700.0),
        ("black", 900.0),
        ("heavy", 900.0),
    ];
    TABLE.iter().find(|(k, _)| s.contains(k)).map(|(_, w)| *w).unwrap_or(400.0)
}

fn style_italic(style: &str) -> bool {
    let s = norm(style);
    s.contains("italic") || s.contains("oblique") || s == "it"
}

impl FontDb {
    /// A database holding the bundled fonts, then the kition-fonts Japanese faces (Mincho first,
    /// so they are the fallback for Japanese text after the requested and bundled fonts; empty
    /// without `KITION_FONTS_DIR`), whose system font scan reads `font_dirs`.
    pub fn with_font_dirs(font_dirs: Vec<std::path::PathBuf>) -> Self {
        let mut faces = Vec::new();
        for data in bundled() {
            for (i, family, style, coords) in enumerate_faces(data) {
                if let Some(f) = make_face(FontBytes::Static(data), i, family, style, coords) {
                    faces.push(Arc::new(f));
                }
            }
        }
        Self {
            faces: RwLock::new(faces),
            outlines: Mutex::new(HashMap::new()),
            #[cfg(not(target_arch = "wasm32"))]
            catalog: RwLock::new(Vec::new()),
            font_dirs,
            #[cfg(not(target_arch = "wasm32"))]
            cataloged: std::sync::OnceLock::new(),
            #[cfg(not(target_arch = "wasm32"))]
            sys: Mutex::new(SysFallback { enabled: true, ..Default::default() }),
        }
    }

    /// Enable or disable the lazy system-font fallback for characters the loaded fonts lack
    /// (native only; on by default).
    pub fn set_system_fallback(&self, on: bool) {
        #[cfg(not(target_arch = "wasm32"))]
        {
            self.sys.lock().unwrap_or_else(|e| e.into_inner()).enabled = on;
        }
        #[cfg(target_arch = "wasm32")]
        let _ = on;
    }

    /// Process-wide database: the bundled fonts, then the installed system fonts, cataloged the
    /// first time a lookup by family name needs them (or ahead of time by
    /// [`FontDb::scan_in_background`]).
    pub fn global() -> &'static FontDb {
        static DB: std::sync::OnceLock<FontDb> = std::sync::OnceLock::new();
        DB.get_or_init(|| FontDb::with_font_dirs(system_font_dirs()))
    }

    pub(crate) fn read_faces(&self) -> std::sync::RwLockReadGuard<'_, Vec<Arc<FontFace>>> {
        self.faces.read().unwrap_or_else(|e| e.into_inner())
    }

    /// The system font catalog, scanned first if it hasn't been yet.
    #[cfg(not(target_arch = "wasm32"))]
    fn read_catalog(&self) -> std::sync::RwLockReadGuard<'_, Vec<CatalogEntry>> {
        self.ensure_catalog();
        self.catalog.read().unwrap_or_else(|e| e.into_inner())
    }

    /// Family names available (loaded plus cataloged system fonts), sorted and deduplicated.
    pub fn families(&self) -> Vec<String> {
        let mut v: Vec<String> = self.read_faces().iter().map(|f| f.family.clone()).collect();
        #[cfg(not(target_arch = "wasm32"))]
        v.extend(self.read_catalog().iter().map(|c| c.family.clone()));
        v.sort_by_key(|a| a.to_lowercase());
        v.dedup_by(|a, b| a.eq_ignore_ascii_case(b));
        v
    }

    /// Style names available for `family` (Regular first, then by weight).
    pub fn styles(&self, family: &str) -> Vec<String> {
        let mut v: Vec<(bool, f32, String)> = self
            .read_faces()
            .iter()
            .filter(|f| f.is_named(family) && !f.style.contains('{'))
            .map(|f| (f.italic, f.weight, f.style.clone()))
            .collect();
        #[cfg(not(target_arch = "wasm32"))]
        for c in self.read_catalog().iter() {
            if c.is_named(family) && !v.iter().any(|(_, _, s)| s.eq_ignore_ascii_case(&c.style)) {
                v.push((style_italic(&c.style), style_weight(&c.style), c.style.clone()));
            }
        }
        v.sort_by(|a, b| a.0.cmp(&b.0).then(a.1.total_cmp(&b.1)).then(a.2.cmp(&b.2)));
        v.dedup_by(|a, b| a.2 == b.2);
        v.into_iter().map(|t| t.2).collect()
    }

    /// Add a user font (TTF/OTF/TTC bytes). Returns the number of faces added (0 if unparseable or
    /// every face was already present).
    pub fn add_font(&self, bytes: Vec<u8>) -> usize {
        self.add_files(vec![bytes])
    }

    /// Add the faces of font files together: a lookup on another thread sees all of them or none,
    /// never a family part loaded (whose closest style would then be the wrong face). Faces
    /// already present are skipped; returns the number added.
    fn add_files(&self, files: Vec<Vec<u8>>) -> usize {
        let present =
            |faces: &[Arc<FontFace>], family: &str, style: &str| faces.iter().any(|f| f.is_named(family) && f.style.eq_ignore_ascii_case(style));
        let mut made: Vec<Arc<FontFace>> = Vec::new();
        for bytes in files {
            let data = Arc::new(bytes);
            for (i, family, style, coords) in enumerate_faces(&data) {
                if present(&made, &family, &style) || present(&self.read_faces(), &family, &style) {
                    continue;
                }
                if let Some(f) = make_face(FontBytes::Owned(data.clone()), i, family, style, coords) {
                    made.push(Arc::new(f));
                }
            }
        }
        if made.is_empty() {
            return 0;
        }
        let mut faces = self.faces.write().unwrap_or_else(|e| e.into_inner());
        let before = faces.len();
        for f in made {
            // Another thread may have added the same face since it was looked for above.
            if !present(&faces, &f.family, &f.style) {
                faces.push(f);
            }
        }
        faces.len() - before
    }

    /// Catalog the installed fonts unless that has been done: the first caller scans the font
    /// folders, any other waits for that scan to finish.
    #[cfg(not(target_arch = "wasm32"))]
    fn ensure_catalog(&self) {
        self.cataloged.get_or_init(|| {
            self.scan_font_dirs();
        });
    }

    /// Catalog the installed fonts on a background thread, so the first lookup by family name
    /// (opening a file, the font menus) doesn't wait for the scan. A no-op once they are
    /// cataloged, and on wasm.
    pub fn scan_in_background(&'static self) {
        #[cfg(not(target_arch = "wasm32"))]
        if self.cataloged.get().is_none() {
            // A failed spawn leaves the scan to the first lookup that needs it.
            let _ = std::thread::Builder::new().name("font-scan".into()).spawn(move || self.ensure_catalog());
        }
    }

    /// Scan the font folders again (fonts installed or removed since), cataloging the faces
    /// found (native only). Font data is loaded when a cataloged family is first resolved.
    /// Returns the number of faces cataloged.
    #[cfg(not(target_arch = "wasm32"))]
    pub fn load_system_fonts(&self) -> usize {
        // The first scan, or a rescan once it is done (never both at once).
        let mut first = None;
        self.cataloged.get_or_init(|| first = Some(self.scan_font_dirs()));
        first.unwrap_or_else(|| self.scan_font_dirs())
    }

    #[cfg(not(target_arch = "wasm32"))]
    fn scan_font_dirs(&self) -> usize {
        if system_fonts_off() {
            // A served kit reads no font folder of the machine (`no_system_fonts`).
            self.catalog.write().unwrap_or_else(|e| e.into_inner()).clear();
            return 0;
        }
        let mut found = Vec::new();
        let mut stack = self.font_dirs.clone();
        // Each folder once, however links lead back to it.
        let mut visited = std::collections::HashSet::new();
        while let Some(d) = stack.pop() {
            if !visited.insert(std::fs::canonicalize(&d).unwrap_or_else(|_| d.clone())) {
                continue;
            }
            let Ok(rd) = std::fs::read_dir(&d) else { continue };
            for e in rd.flatten() {
                let p = e.path();
                if p.is_dir() {
                    stack.push(p);
                    continue;
                }
                let ext = p.extension().and_then(|e| e.to_str()).map(|e| e.to_ascii_lowercase());
                if !matches!(ext.as_deref(), Some("ttf" | "otf" | "ttc" | "otc")) {
                    continue;
                }
                for (family, style, aliases) in file_face_names(&p) {
                    found.push(CatalogEntry { family, style, aliases, path: p.clone() });
                }
            }
        }
        let n = found.len();
        log::debug!("cataloged {n} system font faces");
        *self.catalog.write().unwrap_or_else(|e| e.into_inner()) = found;
        n
    }

    /// Load the files of the installed `family`, all its faces at once ([`FontDb::add_files`]).
    #[cfg(not(target_arch = "wasm32"))]
    fn load_cataloged(&self, family: &str) {
        let paths: Vec<std::path::PathBuf> = {
            let cat = self.read_catalog();
            let mut p: Vec<_> = cat.iter().filter(|c| c.is_named(family)).map(|c| c.path.clone()).collect();
            p.sort();
            p.dedup();
            p
        };
        let files: Vec<Vec<u8>> = paths.iter().filter_map(|p| std::fs::read(p).ok()).collect();
        self.add_files(files);
    }

    /// Resolve a family + style to a face, falling back to the closest style of the family, then to
    /// an open substitute ([`substitutes`]), then to Inter. Installed system fonts are found by name
    /// whatever ran before.
    pub fn face(&self, family: &str, style: &str) -> Arc<FontFace> {
        if let Some(f) = self.try_family(family, style) {
            return f;
        }
        for sub in substitutes(family) {
            if let Some(f) = self.try_family(sub, style) {
                return f;
            }
        }
        for sub in [FALLBACK_FAMILY, "Helvetica Neue", "Arial", "Liberation Sans", "DejaVu Sans", "Segoe UI"] {
            if let Some(f) = self.try_family(sub, style) {
                return f;
            }
        }
        self.find(LAST_RESORT_FAMILY, "Regular")
            .or_else(|| self.find(FALLBACK_FAMILY, "Regular"))
            .or_else(|| self.read_faces().first().cloned())
            .unwrap_or_else(last_resort_face)
    }

    fn try_family(&self, family: &str, style: &str) -> Option<Arc<FontFace>> {
        if let Some(f) = self.find(family, style) {
            return Some(f);
        }
        // Look again whether or not this call added anything: another thread may have loaded the
        // family meanwhile, and its faces are then only found here.
        #[cfg(not(target_arch = "wasm32"))]
        {
            self.load_cataloged(family);
            if let Some(f) = self.find(family, style) {
                return Some(f);
            }
        }
        None
    }

    /// Is `family` available (loaded, or installed on the system)?
    pub fn has_family(&self, family: &str) -> bool {
        if self.is_loaded(family) {
            return true;
        }
        #[cfg(not(target_arch = "wasm32"))]
        if self.read_catalog().iter().any(|c| c.is_named(family)) {
            return true;
        }
        false
    }

    pub(crate) fn is_loaded(&self, family: &str) -> bool {
        self.read_faces().iter().any(|f| f.is_named(family))
    }

    /// A variable font's axes: (tag, name, min, default, max), empty for static fonts.
    pub fn axes(&self, family: &str, style: &str) -> Vec<(String, String, f32, f32, f32)> {
        let face = self.face(family, base_style(style));
        let Some(f) = face.skrifa() else { return vec![] };
        f.axes()
            .iter()
            .map(|a| {
                let tag = String::from_utf8_lossy(&a.tag().to_be_bytes()).to_string();
                let name = f.localized_strings(a.name_id()).english_or_first().map(|s| s.to_string()).unwrap_or_else(|| tag.clone());
                (tag, name, a.min_value(), a.default_value(), a.max_value())
            })
            .collect()
    }

    /// `Style {wght:650,wdth:90}`: the named style's font at those axis values, made on first use.
    fn instance(&self, family: &str, style: &str) -> Option<Arc<FontFace>> {
        let open = style.find('{')?;
        let base = self.find(family, style[..open].trim())?;
        if base.skrifa().is_none_or(|f| f.axes().is_empty()) {
            return Some(base);
        }
        let mut coords = base.coords.clone();
        for kv in style[open + 1..].trim_end_matches('}').split(',') {
            let (k, v) = kv.split_once(':')?;
            let k = k.trim().as_bytes();
            let v: f32 = v.trim().parse().ok()?;
            if k.len() != 4 {
                return None;
            }
            let tag = [k[0], k[1], k[2], k[3]];
            match coords.iter_mut().find(|(t, _)| *t == tag) {
                Some(c) => c.1 = v,
                None => coords.push((tag, v)),
            }
        }
        let f = make_face(base.bytes.clone(), base.index, base.family.clone(), style.to_string(), coords)?;
        let f = Arc::new(f);
        self.faces.write().unwrap_or_else(|e| e.into_inner()).push(f.clone());
        Some(f)
    }

    fn find(&self, family: &str, style: &str) -> Option<Arc<FontFace>> {
        if style.contains('{') {
            {
                let faces = self.read_faces();
                if let Some(f) = faces.iter().find(|f| f.is_named(family) && f.style == style) {
                    return Some(f.clone());
                }
            }
            return self.instance(family, style);
        }
        let faces = self.read_faces();
        let cands: Vec<&Arc<FontFace>> = faces.iter().filter(|f| f.is_named(family)).collect();
        if cands.is_empty() {
            return None;
        }
        let ns = norm(style);
        if let Some(f) = cands.iter().find(|f| norm(&f.style) == ns) {
            return Some((*f).clone());
        }
        let (tw, ti) = (style_weight(style), style_italic(style));
        cands
            .iter()
            .min_by(|a, b| {
                let sa = (a.weight - tw).abs() + if a.italic != ti { 1000.0 } else { 0.0 };
                let sb = (b.weight - tw).abs() + if b.italic != ti { 1000.0 } else { 0.0 };
                sa.total_cmp(&sb)
            })
            .map(|f| (*f).clone())
    }

    /// First face (fallback family first, then load order) that covers `c`; on native, system
    /// fonts are cataloged and loaded lazily the first time no loaded face covers a character.
    pub fn fallback_for(&self, c: char, exclude: u32) -> Option<Arc<FontFace>> {
        if let Some(f) = self.loaded_fallback(c, exclude) {
            return Some(f);
        }
        #[cfg(not(target_arch = "wasm32"))]
        if self.system_fallback(c) {
            return self.loaded_fallback(c, exclude);
        }
        None
    }

    fn loaded_fallback(&self, c: char, exclude: u32) -> Option<Arc<FontFace>> {
        let faces = self.read_faces();
        let mut order: Vec<&Arc<FontFace>> = faces.iter().filter(|f| f.id != exclude).collect();
        order.sort_by_key(|f| (!f.family.eq_ignore_ascii_case(FALLBACK_FAMILY), f.italic, (f.weight - 400.0).abs() as i32));
        order.into_iter().find(|f| f.covers(c)).cloned()
    }

    /// Load a system font covering `c` (preferred fallback families first, then any cataloged
    /// file under 40 MB). Returns true if one was loaded. Misses are remembered.
    #[cfg(not(target_arch = "wasm32"))]
    pub(crate) fn system_fallback(&self, c: char) -> bool {
        if c.is_control() || c.is_whitespace() {
            return false;
        }
        {
            let sys = self.sys.lock().unwrap_or_else(|e| e.into_inner());
            if !sys.enabled || sys.misses.contains(&c) {
                return false;
            }
        }
        let covered = |db: &FontDb| db.read_faces().iter().any(|f| f.covers(c));
        let cataloged: Vec<String> = self.read_catalog().iter().map(|e| e.family.clone()).collect();
        for fam in SYSTEM_FALLBACKS {
            if !self.is_loaded(fam) && cataloged.iter().any(|f| f.eq_ignore_ascii_case(fam)) {
                self.load_cataloged(fam);
                if covered(self) {
                    return true;
                }
            }
        }
        let mut paths: Vec<std::path::PathBuf> = self.catalog.read().unwrap_or_else(|e| e.into_inner()).iter().map(|e| e.path.clone()).collect();
        paths.sort();
        paths.dedup();
        for p in paths {
            if std::fs::metadata(&p).map(|m| m.len() > 40 << 20).unwrap_or(true) {
                continue;
            }
            let Ok(data) = std::fs::read(&p) else { continue };
            let hit = enumerate_faces(&data)
                .iter()
                .any(|(i, _, _, _)| skrifa::FontRef::from_index(&data, *i).is_ok_and(|f| !symbolic_ranges(&f) && f.charmap().map(c).is_some()));
            if !hit {
                continue;
            }
            // The whole family of each face in the file, as a lookup by name would load it, so a
            // later lookup never finds the family with only this file's styles. (Whether this call
            // added the faces or another thread did, they cover `c` now.)
            let mut families: Vec<String> = enumerate_faces(&data).into_iter().map(|(_, family, _, _)| family).collect();
            families.dedup();
            for family in &families {
                self.load_cataloged(family);
            }
            self.add_font(data);
            if covered(self) {
                return true;
            }
        }
        self.sys.lock().unwrap_or_else(|e| e.into_inner()).misses.insert(c);
        false
    }

    /// Glyph outline in font units, y-down (flipped), cached per (face, glyph).
    pub fn outline(&self, face: &FontFace, gid: u32) -> Arc<BezPath> {
        let key = (face.id, gid);
        if let Some(p) = self.outlines.lock().unwrap_or_else(|e| e.into_inner()).get(&key) {
            return p.clone();
        }
        let mut pen = FlipPen(BezPath::new());
        if let Some(f) = face.skrifa()
            && let Some(g) = f.outline_glyphs().get(GlyphId::new(gid))
        {
            let _ = g.draw(DrawSettings::unhinted(Size::unscaled(), face.location()), &mut pen);
        }
        let p = Arc::new(pen.0);
        let mut cache = self.outlines.lock().unwrap_or_else(|e| e.into_inner());
        if cache.len() >= OUTLINE_CACHE_MAX {
            cache.clear();
        }
        cache.insert(key, p.clone());
        p
    }
}

struct FlipPen(BezPath);

impl OutlinePen for FlipPen {
    fn move_to(&mut self, x: f32, y: f32) {
        self.0.move_to((x as f64, -y as f64));
    }
    fn line_to(&mut self, x: f32, y: f32) {
        self.0.line_to((x as f64, -y as f64));
    }
    fn quad_to(&mut self, cx0: f32, cy0: f32, x: f32, y: f32) {
        self.0.quad_to((cx0 as f64, -cy0 as f64), (x as f64, -y as f64));
    }
    fn curve_to(&mut self, cx0: f32, cy0: f32, cx1: f32, cy1: f32, x: f32, y: f32) {
        self.0.curve_to((cx0 as f64, -cy0 as f64), (cx1 as f64, -cy1 as f64), (x as f64, -y as f64));
    }
    fn close(&mut self) {
        self.0.close_path();
    }
}

#[cfg(test)]
mod alias_tests {
    use super::named;

    #[test]
    fn a_family_is_asked_for_by_any_of_its_names() {
        let chinese: String = ['\u{5FAE}', '\u{8F6F}', '\u{96C5}', '\u{9ED1}'].iter().collect();
        let aliases = vec![chinese.clone()];
        assert!(named("Microsoft YaHei", &aliases, "microsoft yahei"));
        assert!(named("Microsoft YaHei", &aliases, &chinese));
        assert!(!named("Microsoft YaHei", &aliases, "SimSun"));
        assert!(!named("Microsoft YaHei", &[], &chinese));
    }
}

#[cfg(test)]
mod math_substitution_tests {
    use super::*;

    fn fixture(families: &[(&str, &str)]) -> FontDb {
        let db = FontDb::with_font_dirs(Vec::new());
        db.set_system_fallback(false);
        {
            let mut faces = db.faces.write().unwrap();
            faces.clear();
            for &(family, style) in families {
                // Test routing with existing bundled bytes and synthetic metadata;
                // no installed fonts or additional font assets are required.
                let face = make_face(FontBytes::Static(epaint_default_fonts::UBUNTU_LIGHT), 0, family.into(), style.into(), Vec::new()).unwrap();
                faces.push(Arc::new(face));
            }
        }
        db
    }

    #[test]
    fn missing_dejavu_math_prefers_the_serif_companion_and_requested_style() {
        let db = fixture(&[("Inter", "Regular"), ("Liberation Serif", "Italic"), ("DejaVu Serif", "Regular"), ("DejaVu Serif", "Italic")]);
        for requested in ["DejaVu Math TeX Gyre", "TeX Gyre DejaVu Math", "DEJAVU MATH TEX GYRE"] {
            assert!(!db.has_family(requested));
            let face = db.face(requested, "Italic");
            assert_eq!(face.family, "DejaVu Serif");
            assert_eq!(face.style, "Italic");
        }
        // An unrelated unknown name keeps the existing generic fallback policy.
        assert_eq!(db.face("Unknown Math Family", "Regular").family, "Inter");
    }

    #[test]
    fn missing_companion_uses_serif_fallback_but_available_requested_family_wins() {
        let db = fixture(&[("Inter", "Regular"), ("Liberation Serif", "Regular")]);
        for requested in ["DejaVu Math TeX Gyre", "TeX Gyre DejaVu Math"] {
            assert_eq!(db.face(requested, "Regular").family, "Liberation Serif");
        }
        let db = fixture(&[("Inter", "Regular"), ("DejaVu Serif", "Italic"), ("DejaVu Math TeX Gyre", "Regular")]);
        let face = db.face("DejaVu Math TeX Gyre", "Italic");
        assert!(db.has_family("DejaVu Math TeX Gyre"));
        assert_eq!(face.family, "DejaVu Math TeX Gyre");
        assert_eq!(face.style, "Regular");
    }
}

#[cfg(test)]
mod coverage_tests;
