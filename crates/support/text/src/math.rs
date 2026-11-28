//! Equations in text: the equation model, OMML reader and layout the Office kits share
//! ([`kition_office_core::math`]), set in Slide's fonts. A run's OMML is laid out as one inline
//! box; a display equation (`m:oMathPara`) is set in display style and may justify its paragraph
//! ([`paragraph_alignment`]).

use std::collections::HashMap;
use std::sync::{Arc, Mutex, OnceLock};

use kition_office_core::math::layout::{self, Base, MItem, MathFonts, Shaped, TextFace};
use kition_office_core::math::table::{MATH_SUBSTITUTES, MathTable};
use kition_office_core::math::{Math, MathProps, omml};
use kition_office_core::xml::{Element, Node};
use kition_slide_geom::KurboShape;

use super::*;

mod paragraph;

/// Largest OMML a run lays out, bytes.
const MAX_BYTES: usize = 262_144;
/// Most glyphs and rules an equation draws.
const MAX_ITEMS: usize = 16_384;
/// The line width display equations are laid out against: wide enough that none wraps (a
/// slide's paragraph places the equation as one box).
const UNBOUNDED: f32 = 100_000.0;

pub(super) fn paragraph_alignment(xml: &str) -> Option<Align> {
    paragraph::alignment(xml)
}

/// A run's OMML as one tree under a synthetic root (the run may hold several top-level elements).
fn parse(xml: &str) -> Option<Element> {
    if xml.len() > MAX_BYTES {
        return None;
    }
    kition_office_core::xml::parse(&format!("<root>{xml}</root>")).ok().map(|d| d.root)
}

/// Child elements of `e`.
fn elements(e: &Element) -> impl Iterator<Item = &Element> {
    e.children.iter().filter_map(|n| match n {
        Node::Element(e) => Some(e),
        _ => None,
    })
}

/// The equations of a run: its `m:oMath` elements (also inside `m:oMathPara`) in order, and
/// whether they form a display equation.
fn equations(root: &Element) -> (Vec<&Element>, bool) {
    let mut out = Vec::new();
    let mut display = false;
    for e in elements(root) {
        match e.name.as_str() {
            "m:oMath" => out.push(e),
            "m:oMathPara" => {
                display = true;
                out.extend(elements(e).filter(|m| m.name == "m:oMath"));
            }
            _ => {}
        }
    }
    (out, display)
}

/// Slide's fonts as equation layout needs them.
struct SlideFonts;

/// A face's `MATH` table, read once per face.
fn math_table(face: &FontFace) -> Option<Arc<MathTable>> {
    static CACHE: OnceLock<Mutex<HashMap<u32, Option<Arc<MathTable>>>>> = OnceLock::new();
    let cache = CACHE.get_or_init(|| Mutex::new(HashMap::new()));
    if let Some(t) = cache.lock().unwrap_or_else(|e| e.into_inner()).get(&face.id()) {
        return t.clone();
    }
    let t = ttf_parser::RawFace::parse(face.data(), face.index())
        .ok()
        .and_then(|raw| raw.table(ttf_parser::Tag::from_bytes(b"MATH")))
        .and_then(kition_office_core::math::table::parse)
        .map(Arc::new);
    cache.lock().unwrap_or_else(|e| e.into_inner()).insert(face.id(), t.clone());
    t
}

impl MathFonts for SlideFonts {
    type Face = Arc<FontFace>;
    fn math_face(&self, family: &str) -> (Arc<FontFace>, Option<Arc<MathTable>>) {
        let db = FontDb::global();
        for fam in std::iter::once(family).chain(MATH_SUBSTITUTES.iter().copied()) {
            if !db.has_family(fam) {
                continue;
            }
            let face = db.face(fam, "Regular");
            if face.is_named(fam)
                && let Some(t) = math_table(&face)
            {
                return (face, Some(t));
            }
        }
        (db.face(family, "Regular"), None)
    }
    fn text_face(&self, family: &str, bold: bool, italic: bool) -> TextFace<Arc<FontFace>> {
        let face = FontDb::global().face(family, style_name(bold, italic));
        let (synth_bold, synth_italic) = (bold && face.weight < 550.0, italic && !face.italic);
        TextFace { face, synth_bold, synth_italic }
    }
    fn fallback(&self, c: char, face: &Arc<FontFace>) -> Option<Arc<FontFace>> {
        FontDb::global().fallback_for(c, face.id())
    }
    fn same(&self, a: &Arc<FontFace>, b: &Arc<FontFace>) -> bool {
        a.id() == b.id()
    }
    fn covers(&self, face: &Arc<FontFace>, c: char) -> bool {
        face.covers(c)
    }
    fn glyph(&self, face: &Arc<FontFace>, c: char) -> u32 {
        face.glyph_for(c)
    }
    fn upem(&self, face: &Arc<FontFace>) -> f32 {
        face.upem as f32
    }
    fn advance(&self, face: &Arc<FontFace>, gid: u32) -> f32 {
        face.advance(gid) as f32
    }
    fn bounds(&self, face: &Arc<FontFace>, gid: u32) -> Option<(f32, f32, f32, f32)> {
        let path = FontDb::global().outline(face, gid);
        if path.elements().is_empty() {
            return None;
        }
        // Outlines are cached y-down.
        let r = path.bounding_box();
        let b = (r.x0 as f32, -r.y1 as f32, r.x1 as f32, -r.y0 as f32);
        (b.0.is_finite() && b.1.is_finite() && b.2.is_finite() && b.3.is_finite()).then_some(b)
    }
    fn weight(&self, face: &Arc<FontFace>) -> f32 {
        face.weight
    }
    fn shape(&self, face: &Arc<FontFace>, text: &str, script_level: u8) -> Vec<Shaped> {
        let features = if script_level > 0 { vec![kition_slide_fonts::feature_value(b"ssty", u32::from(script_level))] } else { Vec::new() };
        kition_slide_fonts::shape(face, text, &features, |c| c)
            .into_iter()
            .map(|g| Shaped {
                gid: g.gid,
                cluster: g.cluster,
                x_advance: g.x_advance as f32,
                x_offset: g.x_offset as f32,
                y_offset: g.y_offset as f32,
            })
            .collect()
    }
}

/// Glyphs of an equation in one face, size and colour.
#[derive(Clone)]
struct Part {
    style: Style,
    face: Arc<FontFace>,
    fake: (bool, bool),
    /// Glyph id and origin (x right, y down from the equation's baseline).
    glyphs: Vec<(u32, f64, f64)>,
}

/// A laid-out equation: an inline box (y down from its baseline) with its glyphs and rules.
#[derive(Clone, Default)]
pub(super) struct BoxLayout {
    pub width: f64,
    pub ascent: f64,
    pub descent: f64,
    parts: Vec<Part>,
    rules: Vec<(Rect, Rgba)>,
}

impl BoxLayout {
    pub(super) fn emit(&self, out: &mut TextLayout, x: f64, baseline: f64, para: usize, chars: (usize, usize)) {
        for p in &self.parts {
            let st = &p.style;
            out.runs.push(GlyphRun {
                face: p.face.clone(),
                size: st.size,
                color: st.color,
                glyphs: p.glyphs.iter().map(|(g, gx, gy)| (*g, gx + x, gy + baseline)).collect(),
                cells: vec![],
                fake_bold: p.fake.0,
                fake_italic: p.fake.1,
                upright: false,
                outline: st.outline,
                para,
                chars,
                link: st.link,
                alpha: st.alpha,
                gradient: st.gradient.clone(),
                fill: st.fill.clone(),
                shadow: st.shadow.clone(),
                glow: st.glow.clone(),
                reflection: st.reflection.clone(),
            });
        }
        out.decos.extend(self.rules.iter().map(|(r, c)| Deco {
            rect: *r + kition_slide_geom::Vec2::new(x, baseline),
            color: *c,
            behind: false,
            para,
        }));
    }
}

/// Lay out a run's OMML set in style `st` (its size, colour and effects; normal text in its
/// family). `None` for OMML that holds no equation, or one too big to draw.
pub(super) fn layout(_ctx: &Ctx, xml: &str, st: &Style) -> Option<BoxLayout> {
    if !st.size.is_finite() || st.size <= 0.0 || st.size > 4000.0 {
        return None;
    }
    let root = parse(xml)?;
    let (equations, display) = equations(&root);
    if equations.is_empty() {
        return None;
    }
    let nodes = equations.into_iter().flat_map(omml::read).collect();
    let color = kition_office_core::model::Rgb(st.color.r, st.color.g, st.color.b);
    let text = Base { size: st.size as f32, color, font: st.family.clone() };
    let m = layout::layout_equation(&SlideFonts, &Math { nodes, ..Default::default() }, "", &text, display, UNBOUNDED, &MathProps::default(), &mut 0);
    let mut out = BoxLayout { width: f64::from(m.width), ascent: f64::from(m.ascent), descent: f64::from(m.descent), ..Default::default() };
    let paint = |c: kition_office_core::model::Rgb| Rgba { r: c.0, g: c.1, b: c.2, a: st.color.a };
    for item in m.items.into_iter().take(MAX_ITEMS) {
        match item {
            MItem::Glyphs { face, size, color, synth_bold, synth_italic, glyphs, .. } => {
                let mut style = st.clone();
                style.size = f64::from(size);
                style.color = paint(color);
                style.face = face.clone();
                let glyphs = glyphs.into_iter().map(|(g, x, y)| (g, f64::from(x), -f64::from(y))).collect();
                out.parts.push(Part { style, face, fake: (synth_bold, synth_italic), glyphs });
            }
            MItem::Rect { x, y, w, h, color } => {
                let (x, y, w, h) = (f64::from(x), f64::from(y), f64::from(w), f64::from(h));
                out.rules.push((Rect::new(x, -(y + h), x + w, -y), paint(color)));
            }
            MItem::Line { x0, y0, x1, y1, width, color, .. } => {
                line(&mut out.rules, (f64::from(x0), -f64::from(y0)), (f64::from(x1), -f64::from(y1)), f64::from(width), paint(color))
            }
            // Placeholders and hints are for an equation editor; slides draw what is there.
            MItem::ScreenOnly(_) => {}
        }
    }
    let finite = |v: f64| v.is_finite() && v.abs() <= 1_000_000.0;
    let ok = [out.width, out.ascent, out.descent].into_iter().all(finite)
        && out.parts.iter().all(|p| p.glyphs.iter().all(|(_, x, y)| finite(*x) && finite(*y)))
        && out.rules.iter().all(|(r, _)| [r.x0, r.y0, r.x1, r.y1].into_iter().all(finite));
    ok.then_some(out)
}

/// A rule from `a` to `b`, `width` thick, as rectangles (slides draw rules as rectangles): one
/// for a level or upright rule, short steps along a slanted one.
fn line(rules: &mut Vec<(Rect, Rgba)>, a: (f64, f64), b: (f64, f64), width: f64, color: Rgba) {
    let h = width.max(0.25) / 2.0;
    let (dx, dy) = (b.0 - a.0, b.1 - a.1);
    if dy.abs() < 1e-6 || dx.abs() < 1e-6 {
        rules.push((Rect::new(a.0.min(b.0) - h, a.1.min(b.1) - h, a.0.max(b.0) + h, a.1.max(b.1) + h), color));
        return;
    }
    let steps = ((dx.hypot(dy) / h.max(0.5)).ceil() as usize).clamp(1, 256);
    for i in 0..steps {
        let t = (i as f64 + 0.5) / steps as f64;
        let (x, y) = (a.0 + dx * t, a.1 + dy * t);
        rules.push((Rect::new(x - h, y - h, x + h, y + h), color));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use kition_slide_model::text::{Paragraph, Run};
    use kition_slide_model::{LayoutType, SlideFile, defaults};

    fn laid(runs: Vec<Run>) -> TextLayout {
        let mut p = SlideFile::default();
        let id = defaults::layout_of_kind(&p, LayoutType::TitleAndContent).unwrap();
        let mut slide = defaults::new_slide(&mut p, id);
        let mut shape = slide.shapes.remove(1);
        shape.text = Some(TextBody { paragraphs: vec![Paragraph { runs, ..Default::default() }], ..Default::default() });
        slide.shapes = vec![shape.clone()];
        p.slides = vec![Arc::new(slide)];
        let ctx = Ctx::for_slide(&p, &p.slides[0]).unwrap();
        super::super::layout(
            &ctx,
            &shape,
            shape.text.as_ref().unwrap(),
            &Opts { rect: Rect::new(0.0, 0.0, 600.0, 400.0), fields: &NoFields, prompt_color: None, no_shrink: true, empty_bullets: false },
        )
    }

    fn props() -> RunProps {
        RunProps { size: Some(24.0), font: Some("Inter".into()), ..Default::default() }
    }

    fn formula(xml: &str, plain: &str) -> TextLayout {
        laid(vec![
            Run::with("before ", props()),
            Run { text: plain.into(), props: props(), kind: RunKind::Math { omml: xml.into() } },
            Run::with(" after", props()),
        ])
    }

    fn equation(xml: &str) -> BoxLayout {
        let mut p = SlideFile::default();
        let id = defaults::layout_of_kind(&p, LayoutType::TitleAndContent).unwrap();
        let slide = defaults::new_slide(&mut p, id);
        p.slides = vec![Arc::new(slide)];
        let ctx = Ctx::for_slide(&p, &p.slides[0]).unwrap();
        layout(&ctx, xml, &make_style(&ctx, &props(), 1.0, None)).unwrap()
    }

    const NS: &str =
        r#"xmlns:m="http://schemas.openxmlformats.org/officeDocument/2006/math" xmlns:a="http://schemas.openxmlformats.org/drawingml/2006/main""#;

    #[test]
    fn an_equation_is_one_inline_box_between_its_texts() {
        let xml = format!("<m:oMath {NS}><m:f><m:num><m:r><m:t>1</m:t></m:r></m:num><m:den><m:r><m:t>2</m:t></m:r></m:den></m:f></m:oMath>");
        let with = formula(&xml, "12");
        let after = with.runs.iter().find(|r| r.cells.iter().any(|(_, c)| *c == 'a')).unwrap();
        let box_ = equation(&xml);
        assert!(box_.width > 5.0 && box_.ascent > 10.0, "{} {}", box_.width, box_.ascent);
        // The fraction's rule is drawn, and the text after the equation starts past its box.
        assert!(!with.decos.is_empty());
        let before = with.runs.iter().find(|r| r.cells.iter().any(|(_, c)| *c == 'b')).unwrap();
        assert!(after.glyphs[0].1 > before.glyphs.last().unwrap().1 + box_.width * 0.9);
        // A fraction makes its line taller than a line of text.
        let text = laid(vec![Run::with("before 12 after", props())]);
        assert!(with.lines[0].bottom - with.lines[0].top > text.lines[0].bottom - text.lines[0].top);
    }

    #[test]
    fn display_equations_are_set_bigger_and_runs_keep_their_drawingml_size() {
        let fraction = "<m:f><m:num><m:r><m:t>1</m:t></m:r></m:num><m:den><m:r><m:t>2</m:t></m:r></m:den></m:f>";
        let inline = equation(&format!("<m:oMath {NS}>{fraction}</m:oMath>"));
        let display = equation(&format!("<m:oMathPara {NS}><m:oMath>{fraction}</m:oMath></m:oMathPara>"));
        assert!(display.ascent + display.descent > inline.ascent + inline.descent);
        let small = equation(&format!(r#"<m:oMath {NS}><m:r><a:rPr sz="1200"/><m:t>x</m:t></m:r></m:oMath>"#));
        assert_eq!(small.parts[0].style.size, 12.0, "a:rPr sz sets the run's size");
    }

    #[test]
    fn malformed_or_empty_omml_lays_out_nothing_and_never_panics() {
        let ctx_free = |xml: &str| {
            let mut p = SlideFile::default();
            let id = defaults::layout_of_kind(&p, LayoutType::TitleAndContent).unwrap();
            let slide = defaults::new_slide(&mut p, id);
            p.slides = vec![Arc::new(slide)];
            let ctx = Ctx::for_slide(&p, &p.slides[0]).unwrap();
            layout(&ctx, xml, &make_style(&ctx, &props(), 1.0, None)).is_some()
        };
        for xml in ["", "<m:oMath>", "<a:p/>", "<m:r><m:t>x</m:t></m:r>"] {
            assert!(!ctx_free(xml), "{xml}");
        }
        let deep = format!("<m:oMath>{}x{}</m:oMath>", "<m:rad><m:e>".repeat(300), "</m:e></m:rad>".repeat(300));
        let _ = ctx_free(&deep);
        assert!(!ctx_free(&"x".repeat(MAX_BYTES + 1)));
    }
}
