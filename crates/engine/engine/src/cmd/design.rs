//! Design tab and masters: themes, colours, fonts, backgrounds, slide size, header & footer,
//! Slide Master view.

use std::sync::Arc;

use kition_slide_geom::Size;
use kition_slide_model::text::{Run, RunKind, TextBody};
use kition_slide_model::theme::{builtin_color_schemes, builtin_font_schemes, builtin_themes};
use kition_slide_model::{Background, Fill, PhType, Shape, ShapeId, SlideFile, defaults};
use serde_json::{Value, json};

use super::*;
use crate::{Result, Session, Target};

pub fn specs() -> Vec<CommandSpec> {
    vec![
        cmd!("theme.set", "Themes", ["Design", "Themes"], None, "{name} (see theme.list)", has_doc, theme),
        cmd!(query "theme.list", "Theme Gallery", [], None, "{} → themes, colour schemes and font schemes", always, themes),
        cmd!(
            "theme.colors",
            "Colors",
            ["Design", "Variants"],
            None,
            "{name?, colors?: {accent1: #RRGGBB, …} (slots dk1, lt1, dk2, lt2, accent1…accent6, hlink, folHlink)} → {name, colors} (the theme's colour scheme after the change; {} only reads it)",
            has_doc,
            colors
        ),
        cmd!("theme.fonts", "Fonts", ["Design", "Variants"], None, "{name} | {major, minor}", has_doc, fonts),
        cmd!(
            "slide.background",
            "Format Background",
            ["Design", "Customize"],
            None,
            "{color?, gradient?, picture?: base64, pattern?: {preset, fg, bg}, style?: 1..12, reset?, all?: bool (Apply to All: the masters and layouts take it, so new slides do too), index?} (one of color, gradient, picture, pattern, style or reset)",
            has_slide,
            background
        ),
        cmd!("slide.hideBackgroundGraphics", "Hide Background Graphics", ["Format Background"], None, "{hide?: bool}", has_slide, hide_bg_graphics),
        cmd!(
            "slide.size",
            "Slide Size",
            ["Design", "Customize"],
            None,
            "{preset?: widescreen|standard|…, w?: pt, h?: pt (w and h instead of preset), scale?: maximize|ensureFit|none}",
            has_doc,
            slide_size
        ),
        cmd!(
            "slide.headerFooter",
            "Header and Footer…",
            ["Insert", "Text"],
            None,
            "{date?: bool, dateText?: fixed text, slideNumber?: bool, footer?: bool, footerText?, hideOnTitle?: bool, all?: bool (default true)}",
            has_doc,
            header_footer
        ),
        cmd!(noundo "view.slideMaster", "Slide Master", ["View", "Master Views"], None, "{master?: index, layout?: index|null (null selects the master itself)}", has_doc, master_view),
        cmd!(noundo "view.closeMaster", "Close Master View", ["Slide Master", "Close"], None, "{}", has_doc, close_master),
        cmd!("master.insertLayout", "Insert Layout", ["Slide Master", "Edit Master"], None, "{name?}", has_doc, insert_layout),
        cmd!("master.renameLayout", "Rename Layout", ["Slide Master", "Edit Master"], None, "{name, master?, layout?}", has_doc, rename_layout),
        cmd!("master.deleteLayout", "Delete Layout", ["Slide Master", "Edit Master"], None, "{master?, layout?}", has_doc, delete_layout),
        cmd!(
            "master.insertPlaceholder",
            "Insert Placeholder",
            ["Slide Master", "Master Layout"],
            None,
            "{kind: content|text|picture|chart|table|media, rect?: [x,y,w,h]}",
            has_doc,
            insert_placeholder
        ),
    ]
}

fn theme(s: &mut Session, p: &Value) -> Result<Value> {
    let name = str_param(p, "name").ok_or_else(|| bad("theme.set", "missing `name`"))?;
    let t = builtin_themes().into_iter().find(|t| t.name.eq_ignore_ascii_case(name)).ok_or_else(|| bad("theme.set", format!("no theme `{name}`")))?;
    s.edit(|doc, sel| {
        let mi = master_index(doc, sel);
        let m = doc.masters.get_mut(mi).ok_or_else(|| bad("theme.set", "no master"))?;
        let m = Arc::make_mut(m);
        m.theme = t.clone();
        m.name = t.name.clone();
        m.color_map = kition_slide_model::theme::builtin_color_map(&t.name);
        Ok(())
    })?;
    Ok(json!({"theme": name}))
}

fn master_index(doc: &SlideFile, sel: &crate::Selection) -> usize {
    match sel.target {
        Target::Master { master } | Target::Layout { master, .. } => master,
        Target::Slides => doc.slides.get(sel.slide).and_then(|s| doc.masters.iter().position(|m| m.layout(s.layout).is_some())).unwrap_or(0),
    }
}

fn themes(_s: &mut Session, _p: &Value) -> Result<Value> {
    Ok(json!({
        "themes": builtin_themes().iter().map(|t| json!({"name": t.name, "major": t.fonts.major.latin, "minor": t.fonts.minor.latin, "colors": t.colors.colors.iter().map(|c| format!("#{}", c.hex())).collect::<Vec<_>>()})).collect::<Vec<_>>(),
        "colorSchemes": builtin_color_schemes().iter().map(|c| c.name.clone()).collect::<Vec<_>>(),
        "fontSchemes": builtin_font_schemes().iter().map(|f| json!({"name": f.name, "major": f.major.latin, "minor": f.minor.latin})).collect::<Vec<_>>(),
    }))
}

fn colors(s: &mut Session, p: &Value) -> Result<Value> {
    let scheme = if let Some(name) = str_param(p, "name") {
        Some(
            builtin_color_schemes()
                .into_iter()
                .find(|c| c.name.eq_ignore_ascii_case(name))
                .ok_or_else(|| bad("theme.colors", format!("no colour scheme `{name}`")))?,
        )
    } else {
        None
    };
    let mut custom = vec![];
    if let Some(given) = p.get("colors").filter(|v| !v.is_null()) {
        let map = given.as_object().ok_or_else(|| bad("theme.colors", "`colors` is {slot: #RRGGBB}"))?;
        for (k, v) in map {
            let slot = kition_slide_color::SchemeSlot::from_xml(k)
                .filter(|slot| kition_slide_color::SchemeSlot::THEME.contains(slot))
                .ok_or_else(|| bad("theme.colors", format!("no colour slot `{k}`: dk1, lt1, dk2, lt2, accent1 … accent6, hlink, folHlink")))?;
            let rgb = v
                .as_str()
                .and_then(kition_slide_color::Rgba::from_hex)
                .ok_or_else(|| bad("theme.colors", format!("`{k}` is not a #RRGGBB colour: {v}")))?;
            custom.push((slot, rgb));
        }
    }
    if scheme.is_some() || !custom.is_empty() {
        s.edit(|doc, sel| {
            let mi = master_index(doc, sel);
            let m = Arc::make_mut(doc.masters.get_mut(mi).ok_or_else(|| bad("theme.colors", "no master"))?);
            if let Some(sc) = &scheme {
                m.theme.colors = sc.clone();
            }
            for (slot, rgb) in &custom {
                m.theme.colors.set(*slot, *rgb);
            }
            if !custom.is_empty() {
                m.theme.colors.name = "Custom".into();
            }
            Ok(())
        })?;
    }
    let st = s.doc()?;
    let m = st.doc.masters.get(master_index(&st.doc, &st.selection)).ok_or_else(|| bad("theme.colors", "no master"))?;
    let c = &m.theme.colors;
    let colors: serde_json::Map<String, Value> =
        kition_slide_color::SchemeSlot::THEME.iter().map(|slot| (slot.xml_name().to_string(), json!(format!("#{}", c.get(*slot).hex())))).collect();
    Ok(json!({"name": c.name, "colors": colors}))
}

fn fonts(s: &mut Session, p: &Value) -> Result<Value> {
    let scheme = if let Some(name) = str_param(p, "name") {
        builtin_font_schemes()
            .into_iter()
            .find(|f| f.name.eq_ignore_ascii_case(name))
            .ok_or_else(|| {
                let names: Vec<String> = builtin_font_schemes().into_iter().map(|f| f.name).collect();
                bad("theme.fonts", format!("no font scheme `{name}`: `name` is a theme font scheme ({}); for a font family pass `major` and `minor`, e.g. {{\"major\": \"{name}\", \"minor\": \"{name}\"}}", names.join(", ")))
            })?
    } else {
        let major = str_param(p, "major").ok_or_else(|| bad("theme.fonts", "give `name`, or `major` and `minor`"))?;
        let minor = str_param(p, "minor").unwrap_or(major);
        // The theme's own sets with their Latin font changed: their East Asian, complex-script and
        // per-script fonts stay, and an East Asian family becomes the East Asian font too (with
        // only the Latin one, PowerPoint draws Han text in the old East Asian font; format.fontName).
        let current = {
            let st = s.doc()?;
            st.doc.masters.get(master_index(&st.doc, &st.selection)).map(|m| m.theme.fonts.clone()).unwrap_or_default()
        };
        let set = |mut f: kition_slide_model::theme::FontSet, family: &str| {
            f.latin = family.into();
            if !kition_slide_fonts::east_asian_substitutes(family).is_empty() {
                f.ea = family.into();
            }
            f
        };
        kition_slide_model::theme::FontScheme { name: "Custom".into(), major: set(current.major, major), minor: set(current.minor, minor) }
    };
    s.edit(|doc, sel| {
        let mi = master_index(doc, sel);
        Arc::make_mut(doc.masters.get_mut(mi).ok_or_else(|| bad("theme.fonts", "no master"))?).theme.fonts = scheme;
        Ok(())
    })?;
    // A family this machine lacks is said, as format.fontName says it.
    let notes: Vec<Value> = [str_param(p, "major"), str_param(p, "minor")].into_iter().flatten().filter_map(super::format::font_note).collect();
    if notes.is_empty() { ok() } else { Ok(json!({ "fonts": notes })) }
}

fn background(s: &mut Session, p: &Value) -> Result<Value> {
    let bg: Option<Background> = if bool_or(p, "reset", false) {
        None
    } else if let Some(c) = color_param(p, "color") {
        Some(Background::Fill { fill: Fill::solid(c) })
    } else if let Some(i) = usize_param(p, "style") {
        // Background styles: 3 theme bg fills × 4 colours (bg1, bg2, tx1, tx2).
        let slots = [
            kition_slide_color::SchemeSlot::Bg1,
            kition_slide_color::SchemeSlot::Bg2,
            kition_slide_color::SchemeSlot::Tx2,
            kition_slide_color::SchemeSlot::Tx1,
        ];
        let i = i.clamp(1, 12) - 1;
        Some(Background::Ref {
            idx: 1001 + (i / 4) as u32,
            color: kition_slide_model::ColorRef::scheme(slots.get(i % 4).copied().unwrap_or(kition_slide_color::SchemeSlot::Bg1)),
        })
    } else if let Some(g) = p.get("gradient") {
        let mut g = super::shape::gradient_param(g).ok_or_else(|| bad("slide.background", "gradient needs two or more `stops`"))?;
        g.rotate_with_shape = false;
        Some(Background::Fill { fill: Fill::Gradient(g) })
    } else if let Some(pt) = p.get("pattern") {
        Some(Background::Fill {
            fill: Fill::Pattern(kition_slide_model::style::PatternFill {
                preset: pt.get("preset").and_then(Value::as_str).unwrap_or("pct50").to_string(),
                fg: pt.get("fg").and_then(color_value).unwrap_or(kition_slide_model::ColorRef::scheme(kition_slide_color::SchemeSlot::Accent1)),
                bg: pt.get("bg").and_then(color_value).unwrap_or(kition_slide_model::ColorRef::scheme(kition_slide_color::SchemeSlot::Bg1)),
            }),
        })
    } else if p.get("picture").is_some() {
        let picture = with_param(p, "data", p.get("picture").cloned().unwrap_or_default());
        let (name, bytes) = super::insert::media_bytes(s, &picture, "slide.background")?;
        let ct = super::insert::content_type(&name, &bytes);
        let media = s.edit(|doc, _| Ok(doc.add_media(&name, ct, bytes)))?;
        Some(Background::Fill { fill: Fill::Picture(kition_slide_model::style::PictureFill { media, ..Default::default() }) })
    } else {
        return Err(bad("slide.background", "give `color`, `gradient`, `pattern`, `picture`, `style` or `reset`"));
    };
    let all = bool_or(p, "all", false);
    let index = usize_param(p, "index");
    s.edit(|doc, sel| {
        match sel.target {
            Target::Master { master } => {
                if let Some(m) = doc.masters.get_mut(master) {
                    Arc::make_mut(m).background = bg.clone();
                }
            }
            Target::Layout { master, layout } => {
                if let Some(l) = doc.masters.get_mut(master).and_then(|m| Arc::make_mut(m).layouts.get_mut(layout)) {
                    l.background = bg.clone();
                }
            }
            Target::Slides if all => {
                // Apply to All, as PowerPoint does it: the background goes on the slide masters
                // and their layouts, and the slides drop their own, so slides added later take
                // it too. Reset with `all` only drops the slides' own backgrounds.
                if bg.is_some() {
                    for m in doc.masters.iter_mut() {
                        let m = Arc::make_mut(m);
                        m.background = bg.clone();
                        for l in m.layouts.iter_mut() {
                            l.background = None;
                        }
                    }
                }
                for sl in doc.slides.iter_mut() {
                    if sl.background.is_some() {
                        Arc::make_mut(sl).background = None;
                    }
                }
            }
            Target::Slides => {
                if let Some(sl) = doc.slides.get_mut(index.unwrap_or(sel.slide)) {
                    Arc::make_mut(sl).background = bg.clone();
                }
            }
        }
        Ok(())
    })?;
    ok()
}

fn hide_bg_graphics(s: &mut Session, p: &Value) -> Result<Value> {
    s.edit(|doc, sel| {
        if let Some(sl) = doc.slides.get_mut(sel.slide) {
            let sl = Arc::make_mut(sl);
            sl.show_master_shapes = !bool_param(p, "hide").unwrap_or(sl.show_master_shapes);
        }
        Ok(())
    })?;
    ok()
}

fn scale_shapes(shapes: &mut [Shape], sx: f64, sy: f64, k: f64, ox: f64, oy: f64) {
    for sh in shapes {
        if let Some(x) = sh.xfrm.as_mut() {
            x.x = x.x * sx + ox;
            x.y = x.y * sy + oy;
            x.w *= sx;
            x.h *= sy;
        }
        if let Some(t) = sh.text.as_mut() {
            kition_slide_model::edit::format_all(t, &|r| {
                if let Some(sz) = r.size.as_mut() {
                    *sz = (*sz * k).max(1.0);
                }
            });
        }
    }
}

fn slide_size(s: &mut Session, p: &Value) -> Result<Value> {
    let size = if let Some(pre) = str_param(p, "preset") {
        let k = pre.to_ascii_lowercase();
        let found = defaults::SLIDE_SIZES.iter().find(|(l, _, _)| l.to_ascii_lowercase().starts_with(&k) || l.to_ascii_lowercase().contains(&k));
        let (_, w, h) = found.ok_or_else(|| bad("slide.size", format!("unknown preset `{pre}`")))?;
        Size::new(*w, *h)
    } else {
        Size::new(
            f64_param(p, "w").ok_or_else(|| bad("slide.size", "missing `w`"))?.clamp(72.0, 4032.0),
            f64_param(p, "h").ok_or_else(|| bad("slide.size", "missing `h`"))?.clamp(72.0, 4032.0),
        )
    };
    let mode = str_param(p, "scale").unwrap_or("ensureFit").to_string();
    s.edit(|doc, _| {
        let old = doc.slide_size;
        let (sx, sy) = (size.width / old.width.max(1.0), size.height / old.height.max(1.0));
        let (sx, sy, k, ox, oy) = match mode.as_str() {
            "none" => (1.0, 1.0, 1.0, 0.0, 0.0),
            "maximize" => {
                let k = sx.max(sy);
                (k, k, k, (size.width - old.width * k) / 2.0, (size.height - old.height * k) / 2.0)
            }
            _ => {
                let k = sx.min(sy);
                (k, k, k, (size.width - old.width * k) / 2.0, (size.height - old.height * k) / 2.0)
            }
        };
        // Masters and layouts stretch to the new size; slides scale by mode.
        let (msx, msy) = (size.width / old.width.max(1.0), size.height / old.height.max(1.0));
        for m in &mut doc.masters {
            let m = Arc::make_mut(m);
            scale_shapes(&mut m.shapes, msx, msy, 1.0, 0.0, 0.0);
            for l in &mut m.layouts {
                scale_shapes(&mut l.shapes, msx, msy, 1.0, 0.0, 0.0);
            }
        }
        for sl in &mut doc.slides {
            scale_shapes(&mut Arc::make_mut(sl).shapes, sx, sy, k, ox, oy);
        }
        doc.slide_size = size;
        Ok(())
    })?;
    Ok(json!({"w": size.width, "h": size.height}))
}

fn header_footer(s: &mut Session, p: &Value) -> Result<Value> {
    s.edit(|doc, sel| {
        let hf = &mut doc.header_footer;
        if let Some(v) = bool_param(p, "date") {
            hf.date = v;
        }
        if let Some(v) = str_param(p, "dateText") {
            hf.date_text = v.into();
        }
        if let Some(v) = bool_param(p, "slideNumber") {
            hf.slide_number = v;
        }
        if let Some(v) = bool_param(p, "footer") {
            hf.footer = v;
        }
        if let Some(v) = str_param(p, "footerText") {
            hf.footer_text = v.into();
            hf.footer = hf.footer || !v.is_empty();
        }
        if let Some(v) = bool_param(p, "hideOnTitle") {
            hf.hide_on_title = v;
        }
        let hf = hf.clone();
        let date_given = str_param(p, "dateText").is_some();
        let all = bool_or(p, "all", true);
        let list: Vec<usize> = if all { (0..doc.slides.len()).collect() } else { vec![sel.slide] };
        for i in list {
            let Some(layout) = doc.slides.get(i).map(|x| x.layout) else { continue };
            let lay = doc.layout(layout).map(|(_, l)| l.clone());
            let is_title = lay.as_ref().is_some_and(|l| l.kind == kition_slide_model::LayoutType::Title);
            let want = |k: PhType| {
                !(hf.hide_on_title && is_title)
                    && match k {
                        PhType::Date => hf.date,
                        PhType::SlideNum => hf.slide_number,
                        PhType::Footer => hf.footer,
                        _ => false,
                    }
            };
            let mut new_ids = vec![];
            for _ in 0..3 {
                new_ids.push(ShapeId(doc.alloc_id()));
            }
            let Some(sl) = doc.slides.get_mut(i) else { continue };
            let sl = Arc::make_mut(sl);
            for (n, k) in [PhType::Date, PhType::Footer, PhType::SlideNum].into_iter().enumerate() {
                let has = sl.shapes.iter().position(|x| x.ph_type() == Some(k));
                match (has, want(k)) {
                    (Some(pos), false) => {
                        sl.shapes.remove(pos);
                    }
                    (None, true) => {
                        let lp = lay.as_ref().and_then(|l| l.shapes.iter().find(|x| x.ph_type() == Some(k)).cloned());
                        let Some(lp) = lp else { continue };
                        let body = match k {
                            PhType::SlideNum => TextBody {
                                paragraphs: vec![kition_slide_model::Paragraph {
                                    runs: vec![Run {
                                        text: "‹#›".into(),
                                        props: Default::default(),
                                        kind: RunKind::Field { field: "slidenum".into() },
                                    }],
                                    ..Default::default()
                                }],
                                ..Default::default()
                            },
                            PhType::Date if hf.date_text.is_empty() => TextBody {
                                paragraphs: vec![kition_slide_model::Paragraph {
                                    runs: vec![Run { text: today(), props: Default::default(), kind: RunKind::Field { field: "datetime1".into() } }],
                                    ..Default::default()
                                }],
                                ..Default::default()
                            },
                            PhType::Date => TextBody::from_text(&hf.date_text),
                            _ => TextBody::from_text(&hf.footer_text),
                        };
                        sl.shapes.push(Shape {
                            id: new_ids.get(n).copied().unwrap_or_default(),
                            name: lp.name.clone(),
                            ph: lp.ph.clone(),
                            text: Some(body),
                            ..Default::default()
                        });
                    }
                    (Some(pos), true) if k == PhType::Footer => {
                        if let Some(sh) = sl.shapes.get_mut(pos) {
                            sh.text = Some(TextBody::from_text(&hf.footer_text));
                        }
                    }
                    (Some(pos), true) if k == PhType::Date && date_given => {
                        if let Some(sh) = sl.shapes.get_mut(pos) {
                            set_date(sh, &hf.date_text);
                        }
                    }
                    _ => {}
                }
            }
        }
        Ok(())
    })?;
    ok()
}

/// Point a slide's date placeholder at a fixed date (its own plain text) or, for an empty
/// `fixed`, at an automatic date field (one it already has keeps its format).
fn set_date(sh: &mut Shape, fixed: &str) {
    let runs = || sh.text.iter().flat_map(|t| &t.paragraphs).flat_map(|p| &p.runs);
    let has_field = runs().any(|r| matches!(&r.kind, RunKind::Field { field } if kition_slide_text::datetime::is_date_field(field)));
    let props = runs().next().map(|r| r.props.clone()).unwrap_or_default();
    let (text, kind) = match fixed {
        "" if has_field => return,
        "" => (today(), RunKind::Field { field: "datetime1".into() }),
        _ if !has_field && sh.text.as_ref().is_some_and(|t| t.text() == fixed) => return,
        _ => (fixed.to_string(), RunKind::Text),
    };
    let para = kition_slide_model::Paragraph { runs: vec![Run { text, props, kind }], ..Default::default() };
    sh.text = Some(TextBody { paragraphs: vec![para], ..sh.text.clone().unwrap_or_default() });
}

/// Today's local date as a `datetime1` field shows it (M/D/YYYY): the saved text of a new date
/// field. Empty where there is no clock (wasm).
pub fn today() -> String {
    date_field_text("datetime1")
}

/// What date field `kind` shows now (empty if unknown or there is no clock).
pub fn date_field_text(kind: &str) -> String {
    use kition_slide_text::datetime::{DateTime, field_text};
    DateTime::now_local().and_then(|now| field_text(kind, None, now)).unwrap_or_default()
}

/// Days since 1970-01-01 → (year, month, day) (Howard Hinnant's algorithm).
pub fn civil(z: i64) -> (i64, u32, u32) {
    let z = z + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = (doy - (153 * mp + 2) / 5 + 1) as u32;
    let m = if mp < 10 { mp + 3 } else { mp - 9 } as u32;
    (if m <= 2 { y + 1 } else { y }, m, d)
}

fn master_view(s: &mut Session, p: &Value) -> Result<Value> {
    let st = s.doc()?;
    let mi = usize_param(p, "master").unwrap_or_else(|| master_index(&st.doc, &st.selection));
    let layout = if p.get("layout").is_some_and(Value::is_null) {
        None
    } else {
        usize_param(p, "layout")
            .or_else(|| st.current_slide().and_then(|sl| st.doc.masters.get(mi).and_then(|m| m.layouts.iter().position(|l| l.id == sl.layout))))
    };
    let target = match layout {
        Some(l) => Target::Layout { master: mi, layout: l },
        None => Target::Master { master: mi },
    };
    s.select(|_, sel| {
        sel.target = target;
        sel.shapes.clear();
        sel.text = None;
    })?;
    Ok(serde_json::to_value(target).unwrap_or_default())
}

fn close_master(s: &mut Session, _p: &Value) -> Result<Value> {
    s.select(|_, sel| {
        sel.target = Target::Slides;
        sel.shapes.clear();
        sel.text = None;
    })?;
    ok()
}

fn insert_layout(s: &mut Session, p: &Value) -> Result<Value> {
    let name = str_param(p, "name").unwrap_or("Custom Layout").to_string();
    s.edit(|doc, sel| {
        let mi = master_index(doc, sel);
        let id = kition_slide_model::LayoutId(doc.alloc_id());
        let tid = ShapeId(doc.alloc_id());
        let size = doc.slide_size;
        let m = Arc::make_mut(doc.masters.get_mut(mi).ok_or_else(|| bad("master.insertLayout", "no master"))?);
        let title = m.shapes.iter().find(|x| x.ph_type() == Some(PhType::Title)).cloned().map(|mut t| {
            t.id = tid;
            t.text = Some(TextBody::default());
            t
        });
        let _ = size;
        let at = match sel.target {
            Target::Layout { layout, .. } => layout + 1,
            _ => m.layouts.len(),
        };
        m.layouts.insert(
            at.min(m.layouts.len()),
            kition_slide_model::Layout {
                id,
                identity: kition_slide_model::identity::fresh(),
                name,
                kind: kition_slide_model::LayoutType::Custom,
                shapes: title.into_iter().collect(),
                background: None,
                show_master_shapes: true,
                preserve: true,
                // Made by a person, as PowerPoint marks layouts made in Slide Master view.
                user_drawn: true,
                tags: vec![],
                raw_ext: None,
                hf: None,
            },
        );
        sel.target = Target::Layout { master: mi, layout: at.min(m.layouts.len().saturating_sub(1)) };
        Ok(json!({"id": id}))
    })
}

fn layout_target(s: &Session, p: &Value) -> Result<(usize, usize)> {
    let st = s.doc()?;
    let (m, l) = match st.selection.target {
        Target::Layout { master, layout } => (master, layout),
        _ => (master_index(&st.doc, &st.selection), 0),
    };
    Ok((usize_param(p, "master").unwrap_or(m), usize_param(p, "layout").unwrap_or(l)))
}

fn rename_layout(s: &mut Session, p: &Value) -> Result<Value> {
    let (mi, li) = layout_target(s, p)?;
    let name = str_param(p, "name").ok_or_else(|| bad("master.renameLayout", "missing `name`"))?.to_string();
    s.edit(|doc, _| {
        let l =
            doc.masters.get_mut(mi).and_then(|m| Arc::make_mut(m).layouts.get_mut(li)).ok_or_else(|| bad("master.renameLayout", "no such layout"))?;
        l.name = name;
        Ok(())
    })?;
    ok()
}

fn delete_layout(s: &mut Session, p: &Value) -> Result<Value> {
    let (mi, li) = layout_target(s, p)?;
    s.edit(|doc, sel| {
        let id = doc.masters.get(mi).and_then(|m| m.layouts.get(li)).map(|l| l.id).ok_or_else(|| bad("master.deleteLayout", "no such layout"))?;
        if doc.slides.iter().any(|x| x.layout == id) {
            return Err(bad("master.deleteLayout", "slides use this layout"));
        }
        let m = Arc::make_mut(doc.masters.get_mut(mi).ok_or_else(|| bad("master.deleteLayout", "no master"))?);
        if m.layouts.len() <= 1 {
            return Err(bad("master.deleteLayout", "a master needs at least one layout"));
        }
        m.layouts.remove(li);
        sel.target = Target::Layout { master: mi, layout: li.min(m.layouts.len().saturating_sub(1)) };
        Ok(())
    })?;
    ok()
}

fn insert_placeholder(s: &mut Session, p: &Value) -> Result<Value> {
    let kind = match str_param(p, "kind").unwrap_or("content") {
        "text" => PhType::Body,
        "picture" => PhType::Picture,
        "chart" => PhType::Chart,
        "table" => PhType::Table,
        "media" => PhType::Media,
        _ => PhType::Obj,
    };
    if !matches!(s.doc()?.selection.target, Target::Layout { .. }) {
        return Err(bad("master.insertPlaceholder", "select a layout in Slide Master view"));
    }
    let size = s.doc()?.doc.slide_size;
    let rect =
        rect_param(p, "rect").unwrap_or(kition_slide_geom::Xfrm::new(size.width * 0.25, size.height * 0.3, size.width * 0.5, size.height * 0.4));
    let idx = s.doc()?.shapes().iter().filter_map(|x| x.ph.as_ref().map(|p| p.idx)).max().unwrap_or(0) + 1;
    let shape = Shape {
        xfrm: Some(rect),
        ph: Some(kition_slide_model::Placeholder { kind, idx, ..Default::default() }),
        text: Some(TextBody::default()),
        ..Default::default()
    };
    let id = super::insert::add_shape(s, shape, true)?;
    Ok(json!({"id": id}))
}

#[cfg(test)]
mod tests {
    #[test]
    fn civil_dates() {
        assert_eq!(super::civil(0), (1970, 1, 1));
        assert_eq!(super::civil(20_734), (2026, 10, 8));
    }
}
