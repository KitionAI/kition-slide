//! Text bodies in the paragraph form: a `TextBody` object
//! (`props`: the body properties and list style) lists `Paragraph` objects, each a `props`
//! register (level, paragraph properties, end-of-paragraph run properties) and a `text` field
//! whose marks are the run properties, so an edit writes only the characters it changed.
//!
//! Runs as characters and marks:
//! - a text run is its characters; each run property is its own mark, under its serde name
//!   (`bold`, `size`, `font`, `link`, …, [`RUN_MARKS`]); a property this build does not know goes
//!   into one `extra` JSON mark;
//! - a line break with no text of its own is one `U+000B`;
//! - every other run (a field, an equation, an empty text run, a text run holding `U+000B`, a
//!   break with text) is one `U+FFFC` with an `inline` mark holding the run without its
//!   properties (expand `none`, so typing next to it never makes another);
//! - two adjacent text runs with the same properties are kept apart by a `seg` mark on every
//!   second one, so a round trip keeps a file's runs as they were.
//!
//! [`align_paragraphs`] pairs the paragraphs of two versions of a body (unchanged ones first, then
//! by shared content), so typing, a split or a merge keeps the paragraphs' objects.

use std::collections::BTreeMap;

use kition_kit::session::TextRun;
use kition_slide_model::text::{Paragraph, Run, RunKind, RunProps, TextBody};
use serde_json::{Map, Value};

use crate::{KdmResult, kdm_err};

/// A line break run.
pub(crate) const BREAK: char = '\u{b}';
/// The anchor of an inline run (see the module docs).
pub(crate) const INLINE: char = '\u{fffc}';

/// The run properties stored as their own mark, by their serde name (`RunProps`'s fields).
pub(crate) const RUN_MARKS: &[&str] = &[
    "font",
    "fontEa",
    "fontCs",
    "fontSym",
    "size",
    "bold",
    "italic",
    "underline",
    "underlineColor",
    "strike",
    "baseline",
    "caps",
    "spacing",
    "kern",
    "fill",
    "outline",
    "highlight",
    "link",
    "lang",
    "shadow",
    "glow",
    "noProof",
];
pub(crate) const EXTRA: &str = "extra";
pub(crate) const INLINE_MARK: &str = "inline";
pub(crate) const SEG: &str = "seg";

type Marks = BTreeMap<String, Value>;

fn value(v: &impl serde::Serialize) -> KdmResult<Value> {
    serde_json::to_value(v).map_err(kdm_err)
}

/// A run's properties as marks (`fix`: local ids to object ids).
fn props_marks(props: &RunProps, fix: &dyn Fn(&mut Value)) -> KdmResult<Marks> {
    let mut v = value(props)?;
    fix(&mut v);
    let mut out = Marks::new();
    let mut extra = Map::new();
    if let Value::Object(map) = v {
        for (key, v) in map {
            if v.is_null() {
                continue;
            }
            if RUN_MARKS.contains(&key.as_str()) {
                out.insert(key, v);
            } else {
                extra.insert(key, v);
            }
        }
    }
    if !extra.is_empty() {
        out.insert(EXTRA.into(), Value::Object(extra));
    }
    Ok(out)
}

/// Whether `run` is written as its own characters (else as one inline anchor).
fn plain(run: &Run) -> bool {
    match run.kind {
        RunKind::Text => !run.text.is_empty() && !run.text.contains(BREAK),
        RunKind::Break => run.text.is_empty(),
        _ => false,
    }
}

/// A paragraph's runs as the runs of its `text` field.
pub(crate) fn to_runs(p: &Paragraph, fix: &dyn Fn(&mut Value)) -> KdmResult<Vec<TextRun>> {
    let mut out: Vec<TextRun> = Vec::with_capacity(p.runs.len());
    // The previous text run's marks (without `seg`) and whether it had `seg`.
    let mut last_text: Option<(Marks, bool)> = None;
    for run in &p.runs {
        let mut marks = props_marks(&run.props, fix)?;
        if plain(run) && run.kind == RunKind::Text {
            let seg = matches!(&last_text, Some((prev, prev_seg)) if *prev == marks && !*prev_seg);
            last_text = Some((marks.clone(), seg));
            if seg {
                marks.insert(SEG.into(), Value::from(1));
            }
            out.push(TextRun { insert: run.text.clone(), attributes: marks });
        } else if plain(run) {
            last_text = None;
            out.push(TextRun { insert: BREAK.to_string(), attributes: marks });
        } else {
            last_text = None;
            let mut inline = value(run)?;
            if let Some(map) = inline.as_object_mut() {
                map.remove("props");
            }
            marks.insert(INLINE_MARK.into(), inline);
            out.push(TextRun { insert: INLINE.to_string(), attributes: marks });
        }
    }
    Ok(out)
}

/// Run properties from a run's marks (`fix`: object ids to local ids). A value this build cannot
/// read is dropped, not the whole run.
fn marks_props(attributes: &Marks, fix: &dyn Fn(&mut Value)) -> KdmResult<RunProps> {
    let mut map = Map::new();
    if let Some(Value::Object(extra)) = attributes.get(EXTRA) {
        for (key, v) in extra {
            map.insert(key.clone(), v.clone());
        }
    }
    for (key, v) in attributes {
        if RUN_MARKS.contains(&key.as_str()) {
            map.insert(key.clone(), v.clone());
        }
    }
    let mut v = Value::Object(map);
    fix(&mut v);
    match serde_json::from_value::<RunProps>(v.clone()) {
        Ok(props) => Ok(props),
        Err(_) => {
            // A run mark that does not read would be lost on save: refuse it, with the reason.
            if let Value::Object(map) = &v {
                for (key, value) in map {
                    if !RUN_MARKS.contains(&key.as_str()) {
                        continue;
                    }
                    let mut one = Map::new();
                    one.insert(key.clone(), value.clone());
                    if let Err(error) = serde_json::from_value::<RunProps>(Value::Object(one)) {
                        return Err(kdm_err(format!("text mark `{key}` cannot be stored ({error}); read a run that has it to see its form")));
                    }
                }
            }
            // Key by key: keep what reads.
            let mut ok = Map::new();
            if let Value::Object(map) = v {
                for (key, v) in map {
                    let mut one = Map::new();
                    one.insert(key.clone(), v.clone());
                    if serde_json::from_value::<RunProps>(Value::Object(one)).is_ok() {
                        ok.insert(key, v);
                    }
                }
            }
            Ok(serde_json::from_value(Value::Object(ok)).unwrap_or_default())
        }
    }
}

/// A `text` field's runs back as a paragraph's runs (see the module docs).
pub(crate) fn from_runs(delta: &[TextRun], fix: &dyn Fn(&mut Value)) -> KdmResult<Vec<Run>> {
    let mut out = Vec::with_capacity(delta.len());
    for run in delta {
        let props = marks_props(&run.attributes, fix)?;
        let push_text = |out: &mut Vec<Run>, text: &mut String| {
            if !text.is_empty() {
                out.push(Run::with(std::mem::take(text), props.clone()));
            }
        };
        let mut text = String::new();
        let inline = run.attributes.get(INLINE_MARK);
        for ch in run.insert.chars() {
            match (ch, inline) {
                (INLINE, Some(inline)) => {
                    push_text(&mut out, &mut text);
                    let mut r: Run = serde_json::from_value(inline.clone()).unwrap_or_else(|_| Run::new(""));
                    r.props = props.clone();
                    // A run another writer anchored though it reads as plain characters: keep the
                    // anchor as a character, so the view's text stays the field's.
                    if plain(&r) {
                        r = Run::with(INLINE.to_string(), props.clone());
                    }
                    out.push(r);
                }
                (BREAK, _) => {
                    push_text(&mut out, &mut text);
                    out.push(Run { text: String::new(), props: props.clone(), kind: RunKind::Break });
                }
                _ => text.push(ch),
            }
        }
        push_text(&mut out, &mut text);
    }
    Ok(out)
}

/// A body's `props` register: the body without its paragraphs.
pub(crate) fn body_props(b: &TextBody, fix: &dyn Fn(&mut Value)) -> KdmResult<Value> {
    let mut v = value(b)?;
    if let Some(map) = v.as_object_mut() {
        map.remove("paragraphs");
    }
    fix(&mut v);
    Ok(v)
}

/// A paragraph's `props` register: the paragraph without its runs.
pub(crate) fn paragraph_props(p: &Paragraph, fix: &dyn Fn(&mut Value)) -> KdmResult<Value> {
    let mut v = value(p)?;
    if let Some(map) = v.as_object_mut() {
        map.remove("runs");
    }
    fix(&mut v);
    Ok(v)
}

/// How much of two paragraphs' text is shared at their ends (common prefix plus suffix).
fn shared(a: &[char], b: &[char]) -> usize {
    let prefix = a.iter().zip(b).take_while(|(x, y)| x == y).count();
    let rest = a.len().min(b.len()) - prefix;
    let suffix = a.iter().rev().zip(b.iter().rev()).take(rest).take_while(|(x, y)| x == y).count();
    prefix + suffix
}

/// Pairs the paragraphs of `before` and `after`: first the longest common subsequence of
/// unchanged paragraphs, then, between consecutive unchanged ones, the changed paragraphs by
/// shared content (the pairing that keeps the most characters, then the most pairs), so typing
/// keeps a paragraph's object, and a split keeps it for the half with more of its text. Answers,
/// for each `after` paragraph, its `before` partner.
pub(crate) fn align_paragraphs(before: &[Paragraph], after: &[Paragraph]) -> Vec<Option<usize>> {
    let (n, m) = (before.len(), after.len());
    let mut partner: Vec<Option<usize>> = vec![None; m];
    // Common prefix and suffix first (the usual edit touches one paragraph), then the LCS of
    // the middle, capped as the bodies are file data.
    let prefix = before.iter().zip(after).take_while(|(x, y)| x == y).count();
    let rest = n.min(m) - prefix;
    let suffix = before.iter().rev().zip(after.iter().rev()).take(rest).take_while(|(x, y)| x == y).count();
    let mut anchors: Vec<(usize, usize)> = (0..prefix).map(|i| (i, i)).collect();
    let (bn, am) = (n - prefix - suffix, m - prefix - suffix);
    if bn > 0 && am > 0 && bn.saturating_mul(am) <= 1_000_000 {
        let b = &before[prefix..prefix + bn];
        let a = &after[prefix..prefix + am];
        let mut dp = vec![vec![0u32; am + 1]; bn + 1];
        for i in (0..bn).rev() {
            for j in (0..am).rev() {
                dp[i][j] = if b[i] == a[j] { dp[i + 1][j + 1] + 1 } else { dp[i + 1][j].max(dp[i][j + 1]) };
            }
        }
        let (mut i, mut j) = (0, 0);
        while i < bn && j < am {
            if b[i] == a[j] {
                anchors.push((prefix + i, prefix + j));
                i += 1;
                j += 1;
            } else if dp[i + 1][j] >= dp[i][j + 1] {
                i += 1;
            } else {
                j += 1;
            }
        }
    }
    anchors.extend((0..suffix).map(|k| (n - suffix + k, m - suffix + k)));
    let texts_b: Vec<Vec<char>> = before.iter().map(|p| p.text().chars().collect()).collect();
    let texts_a: Vec<Vec<char>> = after.iter().map(|p| p.text().chars().collect()).collect();
    let mut start = (0, 0);
    for &(i, j) in anchors.iter().chain(std::iter::once(&(n, m))) {
        pair_gap(&texts_b, &texts_a, start.0..i, start.1..j, &mut partner);
        if i < n && j < m {
            partner[j] = Some(i);
        }
        start = (i + 1, j + 1);
    }
    partner
}

/// Pairs the changed paragraphs of one gap between unchanged ones (see [`align_paragraphs`]).
fn pair_gap(b: &[Vec<char>], a: &[Vec<char>], bs: std::ops::Range<usize>, as_: std::ops::Range<usize>, partner: &mut [Option<usize>]) {
    let (n, m) = (bs.len(), as_.len());
    if n == 0 || m == 0 {
        return;
    }
    if n.saturating_mul(m) > 40_000 {
        // Too large to search: in order.
        for k in 0..n.min(m) {
            partner[as_.start + k] = Some(bs.start + k);
        }
        return;
    }
    // dp[i][j]: the best (characters kept, pairs) pairing of b[i..] with a[j..].
    let mut dp = vec![vec![(0usize, 0usize); m + 1]; n + 1];
    for i in (0..n).rev() {
        for j in (0..m).rev() {
            let s = shared(&b[bs.start + i], &a[as_.start + j]);
            let take = (dp[i + 1][j + 1].0 + s, dp[i + 1][j + 1].1 + 1);
            dp[i][j] = take.max(dp[i + 1][j]).max(dp[i][j + 1]);
        }
    }
    let (mut i, mut j) = (0, 0);
    while i < n && j < m {
        let s = shared(&b[bs.start + i], &a[as_.start + j]);
        let take = (dp[i + 1][j + 1].0 + s, dp[i + 1][j + 1].1 + 1);
        if dp[i][j] == take {
            partner[as_.start + j] = Some(bs.start + i);
            i += 1;
            j += 1;
        } else if dp[i][j] == dp[i + 1][j] {
            i += 1;
        } else {
            j += 1;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn none(_: &mut Value) {}

    fn round(p: &Paragraph) -> Vec<Run> {
        let runs = to_runs(p, &none).unwrap_or_else(|error| panic!("{error}"));
        // As the engine would merge them: adjacent runs with the same marks are one.
        let mut merged: Vec<TextRun> = Vec::new();
        for run in runs {
            match merged.last_mut() {
                Some(last) if last.attributes == run.attributes => last.insert.push_str(&run.insert),
                _ => merged.push(run),
            }
        }
        from_runs(&merged, &none).unwrap_or_else(|error| panic!("{error}"))
    }

    #[test]
    fn runs_round_trip_through_marks() {
        let bold = RunProps { bold: Some(true), ..Default::default() };
        let p = Paragraph {
            runs: vec![
                Run::new("ab"),
                Run::new("cd"),
                Run::new("ef"),
                Run::with("B", bold.clone()),
                Run { text: String::new(), props: bold.clone(), kind: RunKind::Break },
                Run::with("", bold.clone()),
                Run { text: "3".into(), props: RunProps::default(), kind: RunKind::Field { field: "slidenum".into() } },
                Run { text: "4".into(), props: RunProps::default(), kind: RunKind::Field { field: "slidenum".into() } },
                Run::new("x\u{b}y"),
            ],
            ..Default::default()
        };
        assert_eq!(round(&p), p.runs);
    }

    #[test]
    fn typing_keeps_the_paragraph_and_a_split_keeps_the_larger_half() {
        let p = |s: &str| Paragraph::new(s);
        assert_eq!(align_paragraphs(&[p("a"), p("hello"), p("c")], &[p("a"), p("hello!"), p("c")]), vec![Some(0), Some(1), Some(2)]);
        assert_eq!(align_paragraphs(&[p("hello world")], &[p("hello wo"), p("rld")]), vec![Some(0), None]);
        assert_eq!(align_paragraphs(&[p("ab"), p("cdefgh")], &[p("abcdefgh")]), vec![Some(1)]);
        assert_eq!(align_paragraphs(&[p("x"), p("y")], &[p("y"), p("x")]), vec![Some(1), None]);
    }
}
