//! Finding the cases of a reference corpus.
//!
//! A case is a slide (`<stem>.pptx`, any depth below the reference root) next to a folder
//! of its reference renders: a sibling directory named `<stem>_<anything>` holding one image per
//! slide, numbered by the last run of digits in the file name (`…1.png`, `…2.png`, …). When a
//! slide has several such folders, `KITION_SLIDE_REFERENCE_SUFFIX` (the part after `<stem>`)
//! picks one; otherwise the first by name is used. A slide whose stem extends another slide's stem
//! with a dot (`<stem>.<more>.pptx`) is a derived copy of that slide (a conversion round trip) and
//! forms the separate `derived` set. Office lock files (`~$…`) and hidden files are skipped.

use std::path::{Path, PathBuf};

#[derive(Clone, Debug)]
pub struct Case {
    /// Stable ASCII id (hash of `rel`).
    pub id: String,
    /// Path of the slide relative to the root.
    pub rel: String,
    /// First component of `rel` (the corpus category).
    pub category: String,
    pub slide_file: PathBuf,
    /// Reference images by slide number (1-based), sorted.
    pub references: Vec<(u32, PathBuf)>,
    /// A derived copy of another slide.
    pub derived: bool,
}

/// FNV-1a 64 of `s` as hex: ids that stay ASCII and stable across runs.
pub fn stable_id(s: &str) -> String {
    let mut h: u64 = 0xcbf29ce484222325;
    for b in s.bytes() {
        h ^= b as u64;
        h = h.wrapping_mul(0x100000001b3);
    }
    format!("c{h:016x}")
}

fn walk(dir: &Path, out: &mut Vec<PathBuf>) {
    let Ok(rd) = std::fs::read_dir(dir) else { return };
    let mut entries: Vec<_> = rd.flatten().collect();
    entries.sort_by_key(|e| e.file_name());
    for e in entries {
        let name = e.file_name().to_string_lossy().to_string();
        if name.starts_with('.') || name.starts_with("~$") || name.starts_with("__") {
            continue;
        }
        let p = e.path();
        if p.is_dir() {
            walk(&p, out);
        } else if p.extension().is_some_and(|x| x.eq_ignore_ascii_case("pptx")) {
            out.push(p);
        }
    }
}

/// The trailing number of a file stem (`slide12` → 12).
fn trailing_number(stem: &str) -> Option<u32> {
    let digits: String = stem.chars().rev().take_while(char::is_ascii_digit).collect::<Vec<_>>().into_iter().rev().collect();
    digits.parse().ok()
}

fn images(dir: &Path) -> Vec<(u32, PathBuf)> {
    let Ok(rd) = std::fs::read_dir(dir) else { return vec![] };
    let mut v: Vec<(u32, PathBuf)> = rd
        .flatten()
        .map(|e| e.path())
        .filter(|p| p.extension().is_some_and(|x| x.eq_ignore_ascii_case("png") || x.eq_ignore_ascii_case("jpg")))
        .filter_map(|p| Some((trailing_number(&p.file_stem()?.to_string_lossy())?, p)))
        .collect();
    v.sort();
    v.dedup_by_key(|(n, _)| *n);
    v
}

/// Every case under `root`, sorted by relative path.
pub fn discover(root: &Path, suffix: Option<&str>) -> Vec<Case> {
    let mut slide_files = Vec::new();
    walk(root, &mut slide_files);
    let stems: std::collections::HashSet<PathBuf> = slide_files.iter().map(|d| d.with_extension("")).collect();
    let mut cases = Vec::new();
    for slide_file in slide_files {
        let Some(dir) = slide_file.parent() else { continue };
        let Some(stem) = slide_file.file_stem().map(|s| s.to_string_lossy().to_string()) else { continue };
        let derived = {
            let mut s = stem.as_str();
            let mut found = false;
            while let Some((head, _)) = s.rsplit_once('.') {
                if stems.contains(&dir.join(head)) {
                    found = true;
                    break;
                }
                s = head;
            }
            found
        };
        let prefix = format!("{stem}_");
        let mut dirs: Vec<PathBuf> = std::fs::read_dir(dir)
            .into_iter()
            .flatten()
            .flatten()
            .map(|e| e.path())
            .filter(|p| p.is_dir() && p.file_name().is_some_and(|n| n.to_string_lossy().starts_with(&prefix)))
            .collect();
        dirs.sort();
        let chosen = match suffix {
            Some(sfx) => dirs.into_iter().find(|p| p.file_name().is_some_and(|n| n.to_string_lossy() == format!("{stem}{sfx}"))),
            None => dirs.into_iter().find(|p| !images(p).is_empty()),
        };
        let Some(refs) = chosen else { continue };
        let references = images(&refs);
        if references.is_empty() {
            continue;
        }
        let rel = slide_file.strip_prefix(root).unwrap_or(&slide_file).to_string_lossy().replace('\\', "/");
        let category = rel.split('/').next().unwrap_or("").to_string();
        let category = if rel.contains('/') { category } else { String::new() };
        cases.push(Case { id: stable_id(&rel), rel, category, slide_file, references, derived });
    }
    cases.sort_by(|a, b| a.rel.cmp(&b.rel));
    cases
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn finds_cases_references_and_derived_copies() {
        let base = std::env::temp_dir().join(format!("kition-visual-corpus-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&base);
        let cat = base.join("text/deep");
        std::fs::create_dir_all(cat.join("slide_render")).unwrap();
        std::fs::create_dir_all(cat.join("slide.converted_render")).unwrap();
        std::fs::create_dir_all(cat.join("lonely_other")).unwrap();
        for p in [
            "slide.pptx",
            "slide.converted.pptx",
            "~$slide.pptx",
            "lonely.pptx",
            "slide_render/page2.png",
            "slide_render/page10.png",
            "slide_render/page1.png",
        ] {
            std::fs::write(cat.join(p), b"").unwrap();
        }
        std::fs::write(cat.join("slide.converted_render/page1.png"), b"").unwrap();
        let cases = discover(&base, None);
        assert_eq!(cases.len(), 2, "{cases:?}");
        let slide_file = cases.iter().find(|c| c.rel == "text/deep/slide.pptx").unwrap();
        assert_eq!(slide_file.category, "text");
        assert!(!slide_file.derived);
        assert_eq!(slide_file.references.iter().map(|r| r.0).collect::<Vec<_>>(), vec![1, 2, 10]);
        assert!(cases.iter().any(|c| c.rel == "text/deep/slide.converted.pptx" && c.derived));
        assert_eq!(discover(&base, Some("_render")).len(), 2);
        assert!(discover(&base, Some("_other")).is_empty());
        assert_eq!(stable_id("a"), stable_id("a"));
        assert_ne!(stable_id("a"), stable_id("b"));
        let _ = std::fs::remove_dir_all(&base);
    }
}
