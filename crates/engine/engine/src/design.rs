//! Design advice for an agent: what a designer would point out on a slide, each with the commands
//! that change it. khp-bench's generation runs found judges faulting agent-made decks for empty
//! space, small text, flat slides and loose alignment, and that agents act on what an answer says
//! at the step, not on what they would have to look for. `slide.inspect` lists the advice for the
//! slide, and the KitDrawing text commands answer the advice on the object they made or changed.
//!
//! Advice is not a diagnostic: nothing here is wrong, only plainer than a designer would leave it.

use kition_slide_model::resolve::{self, Ctx};
use kition_slide_model::{Fill, Shape, ShapeId, ShapeKind, SlideFile};
use serde_json::{Value, json};

/// The largest text under this share of the slide height is a small title (32–44 pt on 540).
const TITLE_SHARE: f64 = 0.052;
/// Text under this share of the slide height is hard to read when presented (14 pt on 540).
const MIN_SHARE: f64 = 0.026;
/// The largest text should be at least this many times the next size.
const HIERARCHY: f64 = 1.4;
/// Empty space above or below the content worth pointing out, as a share of the slide height.
const EMPTY_SHARE: f64 = 0.35;
/// WCAG's contrast ratio for normal text, and for large text (24 pt and up).
const CONTRAST: f64 = 4.5;
const CONTRAST_LARGE: f64 = 3.0;
const LARGE: f64 = 24.0;
/// Content spanning less of the slide than this reads as sparse.
const COVERAGE: f64 = 0.45;
/// A title at or above this size should be at least SemiBold.
const TITLE_WEIGHT: f32 = 600.0;
/// Left edges this close (points) look like a missed alignment rather than a choice.
const NEAR: (f64, f64) = (1.0, 10.0);

fn advice(code: &str, message: String, shapes: &[ShapeId], try_: &[&str]) -> Value {
    json!({ "code": code, "message": message, "shapes": shapes.iter().map(|id| id.0).collect::<Vec<_>>(), "try": try_ })
}

/// One text object as laid out: its box, its largest and smallest text, and its text colour.
struct Text {
    id: ShapeId,
    name: String,
    left: f64,
    largest: f64,
    smallest: f64,
    /// The weight its largest text is set in (700 for faux bold).
    weight: f32,
    color: Option<[u8; 3]>,
    /// Characters set at each size, for the slide's body size.
    chars: Vec<(f64, usize)>,
    /// The colour right behind the text: the topmost solid shape under its centre, if any.
    behind: Option<[u8; 3]>,
}

fn luminance(c: [u8; 3]) -> f64 {
    let ch = |v: u8| {
        let v = f64::from(v) / 255.0;
        if v <= 0.03928 { v / 12.92 } else { ((v + 0.055) / 1.055).powf(2.4) }
    };
    0.2126 * ch(c[0]) + 0.7152 * ch(c[1]) + 0.0722 * ch(c[2])
}

fn contrast(a: [u8; 3], b: [u8; 3]) -> f64 {
    let (x, y) = (luminance(a), luminance(b));
    (x.max(y) + 0.05) / (x.min(y) + 0.05)
}

/// Whether a shape covers (nearly) the whole slide: a background panel, not content.
fn full_slide(x: &kition_slide_geom::Xfrm, w: f64, h: f64) -> bool {
    x.w >= w * 0.95 && x.h >= h * 0.95
}

/// The colour a shape's fill shows behind its text, as drawn: its own fill or the one it takes
/// from its style or placeholder; a gradient's stops averaged, a pattern's two colours mixed. A
/// picture, no fill or a mostly transparent fill shows what is behind it (`None`).
fn fill_color(ctx: &Ctx, shape: &Shape) -> Option<[u8; 3]> {
    let (fill, ph) = resolve::fill(ctx, shape);
    let mean = |colors: Vec<kition_slide_color::Rgba>| -> Option<[u8; 3]> {
        let opaque: Vec<_> = colors.into_iter().filter(|c| c.a >= 128).collect();
        if opaque.is_empty() {
            return None;
        }
        let n = opaque.len() as f64;
        let avg = |f: fn(&kition_slide_color::Rgba) -> u8| (opaque.iter().map(|c| f64::from(f(c))).sum::<f64>() / n).round() as u8;
        Some([avg(|c| c.r), avg(|c| c.g), avg(|c| c.b)])
    };
    match fill? {
        Fill::Solid { color } => mean(vec![ctx.color(&color, ph)]),
        Fill::Gradient(g) => mean(g.stops.iter().map(|stop| ctx.color(&stop.color, ph)).collect()),
        Fill::Pattern(pattern) => mean(vec![ctx.color(&pattern.fg, ph), ctx.color(&pattern.bg, ph)]),
        _ => None,
    }
}

/// What a designer would point out on slide `index`, most telling first.
pub fn slide_advice(pres: &SlideFile, index: usize) -> Vec<Value> {
    let Some(slide) = pres.slides.get(index) else { return vec![] };
    let Some(ctx) = Ctx::for_slide(pres, slide) else { return vec![] };
    let (w, h) = (pres.slide_size.width, pres.slide_size.height);
    let mut texts: Vec<Text> = vec![];
    let (mut top, mut bottom, mut left, mut right) = (f64::MAX, f64::MIN, f64::MAX, f64::MIN);
    let mut accents = 0usize;
    // Solid shapes drawn so far (bottom to top): what text drawn later sits on.
    let mut panels: Vec<(kition_slide_geom::Xfrm, [u8; 3])> = vec![];
    for shape in slide.shapes.iter().filter(|s| !s.hidden) {
        let x = shape.xfrm.unwrap_or_else(|| resolve::xfrm(&ctx, shape));
        let solid = fill_color(&ctx, shape);
        if x.w <= 0.0 || x.h <= 0.0 || full_slide(&x, w, h) {
            if let Some(c) = solid {
                panels.push((x, c));
            }
            if full_slide(&x, w, h) && (solid.is_some() || !matches!(shape.fill, None | Some(Fill::None))) {
                accents += 1; // a full-slide panel or picture still gives the slide a look
            }
            continue;
        }
        let laid = laid_text(&ctx, shape, &x);
        let has_text = laid.is_some();
        if let Some(mut t) = laid {
            let (cx, cy) = (x.x + x.w / 2.0, x.y + x.h / 2.0);
            t.behind =
                solid.or_else(|| panels.iter().rev().find(|(p, _)| cx >= p.x && cx <= p.x + p.w && cy >= p.y && cy <= p.y + p.h).map(|(_, c)| *c));
            texts.push(t);
        }
        if let Some(c) = solid {
            panels.push((x, c));
        }
        if !has_text || !matches!(shape.fill, None | Some(Fill::None)) || !matches!(shape.kind, ShapeKind::Shape) {
            accents += 1;
        }
        top = top.min(x.y);
        bottom = bottom.max(x.y + x.h);
        left = left.min(x.x);
        right = right.max(x.x + x.w);
    }
    let mut out = vec![];
    if texts.is_empty() {
        return out;
    }
    let largest = texts.iter().map(|t| t.largest).fold(0.0, f64::max);
    // The body size: the size most characters on the slide are set in.
    let mut by_size: Vec<(f64, usize)> = vec![];
    for (size, n) in texts.iter().flat_map(|t| t.chars.iter().copied()) {
        match by_size.iter_mut().find(|(s, _)| (*s - size).abs() < 0.5) {
            Some((_, count)) => *count += n,
            None => by_size.push((size, n)),
        }
    }
    let body = by_size.iter().max_by_key(|(_, n)| *n).map_or(largest, |(s, _)| *s);
    let title = texts.iter().max_by(|a, b| a.largest.total_cmp(&b.largest)).map(|t| t.id);
    if largest < h * TITLE_SHARE {
        out.push(advice(
            "design.small-title",
            format!(
                "The largest text on this slide is {:.0} pt. Slide titles are usually {:.0}–{:.0} pt, in a bold weight; enlarge its box with it so the text still fits.",
                largest,
                h * 0.06,
                h * 0.082
            ),
            &title.into_iter().collect::<Vec<_>>(),
            &["format.size {ids, size}", "format.bold {ids}", "drawing.transform {ids, frame}"],
        ));
    }
    if (largest - body).abs() >= 0.5 && largest < body * HIERARCHY {
        out.push(advice(
            "design.weak-hierarchy",
            format!("The largest text is {largest:.0} pt and the body text {body:.0} pt: too close to read as title and body. Make the title at least {HIERARCHY}× the body, or bolder."),
            &title.into_iter().collect::<Vec<_>>(),
            &["format.size {ids, size}", "format.bold {ids}"],
        ));
    }
    if let Some(t) = texts.iter().max_by(|a, b| a.largest.total_cmp(&b.largest))
        && t.largest >= LARGE
        && t.weight < TITLE_WEIGHT
    {
        out.push(advice(
            "design.thin-type",
            format!(
                "The title \"{}\" is set in a regular weight ({:.0}); titles look finished in SemiBold or Bold (judges called thin decks plain).",
                t.name, t.weight
            ),
            &[t.id],
            &["format.bold {ids}"],
        ));
    }
    let small: Vec<&Text> = texts.iter().filter(|t| t.smallest < h * MIN_SHARE).collect();
    if !small.is_empty() {
        out.push(advice(
            "design.small-text",
            format!(
                "{} {} text under {:.0} pt, hard to read when presented. Keep body text at {:.0} pt or more.",
                small.iter().map(|t| format!("\"{}\"", t.name)).collect::<Vec<_>>().join(", "),
                if small.len() == 1 { "has" } else { "have" },
                (h * MIN_SHARE).ceil(),
                (h * 0.033).round()
            ),
            &small.iter().map(|t| t.id).collect::<Vec<_>>(),
            &["format.size {ids, size}"],
        ));
    }
    let (above, below) = (top.max(0.0), (h - bottom).max(0.0));
    // The share of the slide the content spans (its bounding box): judges called decks "empty"
    // and "sparse" more often for KHP (58 against 39 judgements).
    let spanned = ((right.min(w) - left.max(0.0)).max(0.0) * (bottom.min(h) - top.max(0.0)).max(0.0)) / (w * h).max(1.0);
    if above <= h * EMPTY_SHARE && below <= h * EMPTY_SHARE && spanned < COVERAGE {
        out.push(advice(
            "design.empty-space",
            format!(
                "The content spans only {:.0}% of the slide. Use the space: larger text and visuals, a wider layout, or a supporting image or chart.",
                spanned * 100.0
            ),
            &[],
            &["drawing.transform {ids, frame}", "format.size {ids, size}", "drawing.insert {kind: shape|chart, frame}"],
        ));
    }
    if above > h * EMPTY_SHARE || below > h * EMPTY_SHARE {
        let (where_, share) = if below >= above { ("below", below / h) } else { ("above", above / h) };
        out.push(advice(
            "design.empty-space",
            format!(
                "{:.0}% of the slide's height {where_} the content is empty. Spread the content over the slide (larger text, bigger visuals) or add a supporting visual.",
                share * 100.0
            ),
            &[],
            &["drawing.transform {ids, frame}", "format.size {ids, size}", "drawing.insert {kind: shape|chart, frame}"],
        ));
    }
    let (background, solid) = match resolve::background(&ctx, Some(slide)) {
        (Fill::Solid { color }, ph) => (Some(ctx.color(&color, ph)), true),
        _ => (None, false),
    };
    if solid && accents == 0 {
        out.push(advice(
            "design.plain-slide",
            "The slide is text on a plain background, with no shape, picture or chart. An accent bar, a coloured panel, an icon or a gradient background gives it a designed look.".into(),
            &[],
            &["drawing.insert {kind: shape, geometry, frame, fill}", "design.background {gradient}"],
        ));
    }
    let background = background.map(|b| [b.r, b.g, b.b]);
    {
        // Each text against what is right behind it: a panel under it, else the slide background
        // (khp-bench: light text on dark cards was measured against the light slide).
        let low: Vec<&Text> = texts
            .iter()
            .filter(|t| {
                let needed = if t.largest >= LARGE { CONTRAST_LARGE } else { CONTRAST };
                matches!((t.color, t.behind.or(background)), (Some(c), Some(back)) if contrast(c, back) < needed)
            })
            .collect();
        if !low.is_empty() {
            out.push(advice(
                "design.low-contrast",
                format!(
                    "{} {} too little contrast with what is behind {} (under {CONTRAST}:1, or {CONTRAST_LARGE}:1 for text of {LARGE} pt and up). Darken or lighten the text, or put it on a panel.",
                    low.iter().map(|t| format!("\"{}\"", t.name)).collect::<Vec<_>>().join(", "),
                    if low.len() == 1 { "has" } else { "have" },
                    if low.len() == 1 { "it" } else { "them" }
                ),
                &low.iter().map(|t| t.id).collect::<Vec<_>>(),
                &["format.color {ids, color}"],
            ));
        }
    }
    let mut lefts: Vec<&Text> = texts.iter().collect();
    lefts.sort_by(|a, b| a.left.total_cmp(&b.left));
    let near: Vec<ShapeId> =
        lefts.windows(2).filter(|p| (NEAR.0..=NEAR.1).contains(&(p[1].left - p[0].left))).flat_map(|p| [p[0].id, p[1].id]).collect();
    if !near.is_empty() {
        out.push(advice(
            "design.near-alignment",
            "Some text starts a few points left or right of the text next to it: it reads as a misalignment. Line the left edges up.".into(),
            &near,
            &["arrange.align {edge: left, to: selection, ids}"],
        ));
    }
    out
}

/// The advice that names `id`, for the answer of a command that made or changed it.
pub fn shape_advice(pres: &SlideFile, index: usize, id: ShapeId) -> Vec<Value> {
    slide_advice(pres, index)
        .into_iter()
        .filter(|a| a["shapes"].as_array().is_some_and(|s| s.iter().any(|v| v.as_u64() == Some(u64::from(id.0)))))
        .collect()
}

fn laid_text(ctx: &Ctx, shape: &Shape, x: &kition_slide_geom::Xfrm) -> Option<Text> {
    let body = shape.text.as_ref()?;
    if body.text().trim().is_empty() || !matches!(shape.kind, ShapeKind::Shape) {
        return None;
    }
    let geo = kition_slide_render::shape_geometry(shape, x.w, x.h);
    let opts = kition_slide_text::Opts { rect: geo.text_rect, fields: &kition_slide_text::NoFields, prompt_color: None, no_shrink: false };
    let laid = kition_slide_text::layout(ctx, shape, body, &opts);
    let runs: Vec<&kition_slide_text::GlyphRun> = laid.runs.iter().filter(|r| r.cells.iter().any(|(_, c)| !c.is_whitespace())).collect();
    let largest = runs.iter().map(|r| r.size).fold(0.0, f64::max);
    let smallest = runs.iter().map(|r| r.size).fold(f64::MAX, f64::min);
    if largest <= 0.0 {
        return None;
    }
    let biggest = runs.iter().max_by(|a, b| a.size.total_cmp(&b.size));
    let color = biggest.map(|r| [r.color.r, r.color.g, r.color.b]);
    let weight = biggest.map_or(400.0, |r| if r.fake_bold { 700.0 } else { r.face.weight });
    // Line extents are shape-local, like the inner box.
    let left = x.x + laid.lines.iter().map(|l| l.extent.0).fold(laid.inner.x1, f64::min).max(laid.inner.x0.min(laid.inner.x1));
    let mut chars: Vec<(f64, usize)> = vec![];
    for r in &runs {
        let n = r.cells.iter().filter(|(_, c)| !c.is_whitespace()).count();
        match chars.iter_mut().find(|(s, _)| (*s - r.size).abs() < 0.5) {
            Some((_, count)) => *count += n,
            None => chars.push((r.size, n)),
        }
    }
    Some(Text { id: shape.id, name: shape.name.clone(), left, largest, smallest, weight, color, chars, behind: None })
}

#[cfg(test)]
mod tests {
    use serde_json::{Value, json};

    use crate::Session;

    fn codes(advice: &Value) -> Vec<String> {
        advice.as_array().into_iter().flatten().filter_map(|a| a["code"].as_str().map(str::to_owned)).collect()
    }

    fn text_box(s: &mut Session, frame: Value, size: f64, text: &str) -> Value {
        s.execute(
            "drawing.insert",
            &json!({ "kind": "textBox", "frame": frame, "paragraphs": [{ "runs": [{ "text": text, "size": size, "color": "#222222" }] }] }),
        )
        .unwrap()
    }

    /// khp-bench design batch: light text on a dark card on a light slide was reported as low
    /// contrast in 19 of 24 decks, measured against the slide. It is measured against the card;
    /// light text straight on the light slide still is.
    #[test]
    fn contrast_is_measured_against_the_panel_behind_the_text() {
        let mut s = Session::with_new();
        s.execute("slide.new", &json!({ "layout": "blank" })).unwrap();
        s.execute("design.background", &json!({ "color": "#F8FAFC" })).unwrap();
        s.execute("drawing.insert", &json!({ "kind": "shape", "geometry": { "preset": "roundRect" }, "frame": { "x": 360, "y": 180, "w": 240, "h": 260 }, "fill": "#0F172A" }))
            .unwrap();
        let on_card = s
            .execute("drawing.insert", &json!({ "kind": "textBox", "frame": { "x": 376, "y": 250, "w": 208, "h": 60 }, "paragraphs": [{ "runs": [{ "text": "$12", "size": 40, "color": "#E2E8F0" }] }] }))
            .unwrap()["id"]
            .clone();
        let off_card = s
            .execute("drawing.insert", &json!({ "kind": "textBox", "frame": { "x": 60, "y": 470, "w": 280, "h": 30 }, "paragraphs": [{ "runs": [{ "text": "Footnote", "size": 18, "color": "#E2E8F0" }] }] }))
            .unwrap()["id"]
            .clone();
        let advice = s.execute("slide.inspect", &json!({})).unwrap()["design"].clone();
        let low = advice.as_array().unwrap().iter().find(|a| a["code"] == "design.low-contrast").expect("the text off the card");
        let st = s.doc().unwrap();
        let local = |v: &Value| crate::cmd::drawing::local_id(st, v.as_str().unwrap()).unwrap().0;
        let named: Vec<u64> = low["shapes"].as_array().unwrap().iter().filter_map(Value::as_u64).collect();
        assert!(named.contains(&u64::from(local(&off_card))) && !named.contains(&u64::from(local(&on_card))), "{low}");
    }

    /// khp-bench: a plain slide (small close sizes, empty lower half, nothing but text, edges a few
    /// points apart) gets advice, and the object a command made carries the advice on it; a
    /// designed slide gets none, and the suggested commands run as written.
    #[test]
    fn a_plain_slide_gets_advice_and_a_designed_one_none() {
        let mut s = Session::with_new();
        s.execute("slide.new", &json!({ "layout": "blank" })).unwrap();
        let title = text_box(&mut s, json!({ "x": 60, "y": 40, "w": 600, "h": 40 }), 24.0, "Quarterly review");
        assert!(codes(&title["design"]).contains(&"design.small-title".to_string()), "{title}");
        text_box(&mut s, json!({ "x": 64, "y": 90, "w": 600, "h": 40 }), 12.0, "Revenue grew in every region");
        let advice = &s.execute("slide.inspect", &json!({})).unwrap()["design"];
        for code in ["design.small-title", "design.small-text", "design.empty-space", "design.plain-slide", "design.near-alignment"] {
            assert!(codes(advice).contains(&code.to_string()), "{code}: {advice}");
        }

        let mut s = Session::with_new();
        s.execute("slide.new", &json!({ "layout": "blank" })).unwrap();
        s.execute("design.background", &json!({ "gradient": { "stops": [[0, "#ffffff"], [1, "#e8ecf4"]] } })).unwrap();
        s.execute(
            "drawing.insert",
            &json!({ "kind": "shape", "geometry": { "preset": "rect" }, "frame": { "x": 0, "y": 0, "w": 12, "h": 540 }, "fill": "#5645D4" }),
        )
        .unwrap();
        let title = s
            .execute(
                "drawing.insert",
                &json!({ "kind": "textBox", "frame": { "x": 60, "y": 60, "w": 840, "h": 70 }, "paragraphs": [{ "runs": [{ "text": "Quarterly review", "size": 40, "bold": true, "color": "#222222" }] }] }),
            )
            .unwrap();
        assert!(codes(&title["design"]).is_empty(), "{title}");
        text_box(&mut s, json!({ "x": 60, "y": 170, "w": 840, "h": 300 }), 20.0, "Revenue grew in every region\nMargins held");
        s.execute("drawing.insert", &json!({ "kind": "chart", "frame": { "x": 500, "y": 200, "w": 400, "h": 300 } })).unwrap();
        let advice = &s.execute("slide.inspect", &json!({})).unwrap()["design"];
        assert!(codes(advice).is_empty(), "{advice}");
    }
}
