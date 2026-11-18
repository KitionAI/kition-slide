//! Animations tab and Animation Pane.

use std::sync::Arc;

use kition_slide_model::anim::{ANIMATIONS, AnimClass, AnimStart, Animation, TextBuild, animation_info};
use serde_json::{Value, json};

use super::*;
use crate::{Result, Session};

pub fn specs() -> Vec<CommandSpec> {
    vec![
        cmd!(
            "animation.set",
            "Animation Styles",
            ["Animations", "Animation"],
            None,
            "{effect: fade|fly|zoom|wipe|appear|spin|pulse|fadeOut|lines|… (animation.list), class?: entrance|emphasis|exit|path, option?, start?: onClick|withPrevious|afterPrevious, duration?: ms, delay?: ms, points?: [[dx, dy]] (customPath: where the path goes, in points from the shape's place), smooth?: bool (customPath: a curve through the points), closed?: bool (customPath: back to the start), ids?, onSlide?: int (the slide the ids are on, from 1)} — replaces the shapes' animations",
            has_selection,
            set
        ),
        cmd!(
            "animation.add",
            "Add Animation",
            ["Animations", "Advanced Animation"],
            None,
            "{effect, class?, option?, start?: onClick|withPrevious|afterPrevious, duration?: ms, delay?: ms, points?: [[dx, dy]] (customPath: where the path goes, in points from the shape's place), smooth?: bool, closed?: bool, ids?}",
            has_selection,
            add
        ),
        cmd!("animation.remove", "Remove Animation", ["Animation Pane"], None, "{index} | {ids?} (all of the shapes' animations)", has_slide, remove),
        cmd!("animation.move", "Reorder Animation", ["Animations", "Timing"], None, "{index, to}", has_slide, move_anim),
        cmd!(
            "animation.timing",
            "Timing",
            ["Animations", "Timing"],
            None,
            "{index? (else selected shapes), start?: onClick|withPrevious|afterPrevious, duration?: ms, delay?: ms, repeat?: n, rewind?: bool, trigger?: shape id | null}",
            has_slide,
            timing
        ),
        cmd!(
            "animation.options",
            "Effect Options",
            ["Animations", "Animation"],
            None,
            "{option?, textBuild?: asOne|byParagraph|byWord|byLetter, index?}",
            has_slide,
            options
        ),
        cmd!("animation.clear", "Remove All Animations", [], None, "{index?: slide}", has_slide, clear),
        cmd!(
            noundo "animation.painter",
            "Animation Painter",
            ["Animations", "Advanced Animation"],
            Some("Alt+Shift+C"),
            "{sticky?: bool, id?}",
            has_selection,
            painter_pick
        ),
        cmd!("animation.painterApply", "Apply Animation", [], Some("Alt+Shift+V"), "{ids?}", has_selection, painter_apply),
        cmd!(query "animation.list", "Animation Gallery", [], None, "{} → [{id, label, class, options}]", always, list),
        cmd!(query "animation.get", "Animations on Slide", [], None, "{slide?} → [animation]", has_slide, get),
    ]
}

fn class_of(s: Option<&str>) -> Option<AnimClass> {
    s.map(|c| match c {
        "emphasis" | "emph" => AnimClass::Emphasis,
        "exit" => AnimClass::Exit,
        "path" | "motion" | "motionPath" => AnimClass::Path,
        "media" => AnimClass::Media,
        _ => AnimClass::Entrance,
    })
}

fn start_of(s: &str) -> AnimStart {
    match s {
        "withPrevious" | "with" => AnimStart::WithPrevious,
        "afterPrevious" | "after" => AnimStart::AfterPrevious,
        _ => AnimStart::OnClick,
    }
}

fn build(p: &Value, slide: (f64, f64)) -> Result<Animation> {
    let effect = str_param(p, "effect").ok_or_else(|| bad("animation", "missing `effect`"))?;
    let class = class_of(str_param(p, "class"));
    let info = match class {
        Some(c) => animation_info(effect, c),
        None => ANIMATIONS.iter().find(|a| a.0 == effect),
    }
    .ok_or_else(|| bad("animation", format!("unknown effect `{effect}`")))?;
    let mut a = Animation {
        class: info.2,
        effect: info.0.to_string(),
        option: str_param(p, "option").map(String::from).or_else(|| info.5.first().map(|o| o.to_string())).unwrap_or_default(),
        duration_ms: p.get("duration").and_then(Value::as_u64).map(|v| v.min(600_000) as u32).unwrap_or(info.4),
        delay_ms: p.get("delay").and_then(Value::as_u64).map(|v| v.min(600_000) as u32).unwrap_or(0),
        preset_id: Some(info.3),
        ..Default::default()
    };
    if let Some(st) = str_param(p, "start") {
        a.start = start_of(st);
    }
    let points = p.get("points").filter(|v| !v.is_null());
    if a.effect == "customPath" {
        let points = points.ok_or_else(|| bad("animation", "customPath needs `points`: [[dx, dy], …] in points from the shape's place"))?;
        a.path = Some(custom_path(points, slide, bool_param(p, "smooth").unwrap_or(false), bool_param(p, "closed").unwrap_or(false))?);
    } else if points.is_some() {
        return Err(bad("animation", "`points` is for the customPath effect"));
    } else if a.class == AnimClass::Path {
        a.path = Some(default_path(&a.effect, &a.option));
    }
    if a.effect == "spin" {
        a.amount = Some(360.0);
    }
    if a.effect == "growShrink" {
        a.amount = Some(1.5);
    }
    Ok(a)
}

/// A custom motion path through `points` ([[dx, dy]] in points from the shape's place; the path
/// starts there), in slide-fraction units: straight segments, or a smooth curve through them.
fn custom_path(points: &Value, (w, h): (f64, f64), smooth: bool, closed: bool) -> Result<String> {
    const WHAT: &str = "`points` is [[dx, dy], …]: numbers in points";
    let list = points.as_array().ok_or_else(|| bad("animation", WHAT))?;
    if list.is_empty() || list.len() > 1000 {
        return Err(bad("animation", "`points` takes 1 to 1000 points"));
    }
    let (w, h) = (w.max(1.0), h.max(1.0));
    let mut pts = vec![kition_slide_geom::Point::new(0.0, 0.0)];
    for q in list {
        let xy = q.as_array().filter(|xy| xy.len() == 2).ok_or_else(|| bad("animation", WHAT))?;
        let (Some(x), Some(y)) = (xy[0].as_f64(), xy[1].as_f64()) else { return Err(bad("animation", WHAT)) };
        if !x.is_finite() || !y.is_finite() || x.abs() > 1e5 || y.abs() > 1e5 {
            return Err(bad("animation", WHAT));
        }
        pts.push(kition_slide_geom::Point::new(x / w, y / h));
    }
    if pts.get(1).is_some_and(|q| q.x == 0.0 && q.y == 0.0) {
        pts.remove(0);
    }
    if pts.len() < 2 {
        return Err(bad("animation", "`points` needs a point away from the shape's place"));
    }
    let d = crate::cmd::shape::freeform_path(&pts, closed, smooth, 4);
    Ok(format!("{d} E"))
}

/// Default motion paths, in slide-fraction units relative to the shape's position.
pub fn default_path(effect: &str, option: &str) -> String {
    let (dx, dy) = match option {
        "up" => (0.0, -0.25),
        "right" => (0.25, 0.0),
        "left" => (-0.25, 0.0),
        _ => (0.0, 0.25),
    };
    match effect {
        "arcs" => format!("M 0 0 C {} {} {} {} {dx} {dy} E", dx * 0.1 + dy * 0.5, dy * 0.1 - dx * 0.5, dx * 0.9 + dy * 0.5, dy * 0.9 - dx * 0.5),
        "turns" => {
            // Across first, then a rounded quarter turn into the option's direction: down and up
            // set off to the right, right and left set off downward.
            let (fx, fy) = if dx == 0.0 { (0.25, 0.0) } else { (0.0, 0.25) };
            let k = 0.5 + 0.5 * 0.5523;
            format!(
                "M 0 0 L {} {} C {} {} {} {} {} {} L {} {} E",
                fx * 0.5,
                fy * 0.5,
                fx * k,
                fy * k,
                fx + dx * (1.0 - k),
                fy + dy * (1.0 - k),
                fx + dx * 0.5,
                fy + dy * 0.5,
                fx + dx,
                fy + dy
            )
        }
        "shapes" => "M 0 0 C 0.1 0 0.1 0.15 0 0.15 C -0.1 0.15 -0.1 0 0 0 Z E".into(),
        "loops" => "M 0 0 C 0.1 -0.1 0.2 0.1 0.1 0.1 C 0 0.1 0.1 -0.1 0.25 0 E".into(),
        _ => format!("M 0 0 L {dx} {dy} E"),
    }
}

fn slide_points(s: &Session) -> Result<(f64, f64)> {
    let size = s.doc()?.doc.slide_size;
    Ok((size.width, size.height))
}

fn set(s: &mut Session, p: &Value) -> Result<Value> {
    let ids = targets(s, p)?;
    let a = build(p, slide_points(s)?)?;
    s.edit(|doc, sel| {
        let sl = doc.slides.get_mut(sel.slide).ok_or_else(|| bad("animation.set", "no slide"))?;
        let sl = Arc::make_mut(sl);
        // Replace each shape's animations, keeping their place in the sequence.
        let mut out = vec![];
        let mut placed = vec![];
        for x in sl.animations.drain(..) {
            if ids.contains(&x.shape) {
                if !placed.contains(&x.shape) {
                    placed.push(x.shape);
                    out.push(Animation { shape: x.shape, start: x.start, ..a.clone() });
                }
            } else {
                out.push(x);
            }
        }
        for id in &ids {
            if !placed.contains(id) {
                out.push(Animation { shape: *id, ..a.clone() });
            }
        }
        if a.effect == "none" {
            out.retain(|x| !ids.contains(&x.shape));
        }
        sl.animations = out;
        Ok(json!({"count": sl.animations.len()}))
    })
}

fn add(s: &mut Session, p: &Value) -> Result<Value> {
    let ids = targets(s, p)?;
    let a = build(p, slide_points(s)?)?;
    s.edit(|doc, sel| {
        let sl = Arc::make_mut(doc.slides.get_mut(sel.slide).ok_or_else(|| bad("animation.add", "no slide"))?);
        for (k, id) in ids.iter().enumerate() {
            let mut x = Animation { shape: *id, ..a.clone() };
            if k > 0 {
                x.start = AnimStart::WithPrevious;
            }
            sl.animations.push(x);
        }
        Ok(json!({"count": sl.animations.len()}))
    })
}

fn remove(s: &mut Session, p: &Value) -> Result<Value> {
    let index = usize_param(p, "index");
    let ids = if index.is_none() { targets(s, p)? } else { vec![] };
    s.edit(|doc, sel| {
        let sl = Arc::make_mut(doc.slides.get_mut(sel.slide).ok_or_else(|| bad("animation.remove", "no slide"))?);
        match index {
            Some(i) if i < sl.animations.len() => {
                sl.animations.remove(i);
            }
            Some(_) => return Err(bad("animation.remove", "no such animation")),
            None => sl.animations.retain(|a| !ids.contains(&a.shape)),
        }
        Ok(())
    })?;
    ok()
}

fn move_anim(s: &mut Session, p: &Value) -> Result<Value> {
    let i = usize_param(p, "index").ok_or_else(|| bad("animation.move", "missing `index`"))?;
    let to = usize_param(p, "to").ok_or_else(|| bad("animation.move", "missing `to`"))?;
    s.edit(|doc, sel| {
        let sl = Arc::make_mut(doc.slides.get_mut(sel.slide).ok_or_else(|| bad("animation.move", "no slide"))?);
        if i >= sl.animations.len() {
            return Err(bad("animation.move", "no such animation"));
        }
        let a = sl.animations.remove(i);
        let to = to.min(sl.animations.len());
        sl.animations.insert(to, a);
        Ok(())
    })?;
    ok()
}

fn each_target(s: &mut Session, p: &Value, f: impl Fn(&mut Animation)) -> Result<Value> {
    let index = usize_param(p, "index");
    let ids = if index.is_none() { targets(s, p)? } else { vec![] };
    s.edit(|doc, sel| {
        let sl = Arc::make_mut(doc.slides.get_mut(sel.slide).ok_or_else(|| bad("animation", "no slide"))?);
        let mut n = 0;
        for (k, a) in sl.animations.iter_mut().enumerate() {
            if index == Some(k) || (index.is_none() && ids.contains(&a.shape)) {
                f(a);
                n += 1;
            }
        }
        if n == 0 {
            return Err(bad("animation", "no animation to change (select an animated object or give `index`)"));
        }
        Ok(json!({"changed": n}))
    })
}

fn timing(s: &mut Session, p: &Value) -> Result<Value> {
    let start = str_param(p, "start").map(start_of);
    let dur = p.get("duration").and_then(Value::as_u64).map(|v| v.min(600_000) as u32);
    let delay = p.get("delay").and_then(Value::as_u64).map(|v| v.min(600_000) as u32);
    let repeat = p.get("repeat").and_then(Value::as_u64).map(|v| v.min(u32::MAX as u64) as u32);
    let rewind = bool_param(p, "rewind");
    let trigger = p.get("trigger").map(|_| id_param(p, "trigger"));
    each_target(s, p, |a| {
        if let Some(x) = start {
            a.start = x;
        }
        if let Some(x) = dur {
            a.duration_ms = x;
        }
        if let Some(x) = delay {
            a.delay_ms = x;
        }
        if let Some(x) = repeat {
            a.repeat = x;
        }
        if let Some(x) = rewind {
            a.rewind = x;
        }
        if let Some(t) = trigger {
            a.trigger = t;
        }
    })
}

fn options(s: &mut Session, p: &Value) -> Result<Value> {
    let option = str_param(p, "option").map(String::from);
    let build_mode = str_param(p, "textBuild").map(|b| match b {
        "byParagraph" => TextBuild::ByParagraph,
        "byWord" => TextBuild::ByWord,
        "byLetter" => TextBuild::ByLetter,
        _ => TextBuild::AsOne,
    });
    each_target(s, p, |a| {
        if let Some(o) = &option {
            a.option = o.clone();
            if a.class == AnimClass::Path && a.effect != "customPath" {
                a.path = Some(default_path(&a.effect, o));
            }
        }
        if let Some(b) = build_mode {
            a.text_build = b;
        }
    })
}

fn clear(s: &mut Session, p: &Value) -> Result<Value> {
    let i = usize_param(p, "index").unwrap_or(s.doc()?.selection.slide);
    s.edit(|doc, _| {
        if let Some(sl) = doc.slides.get_mut(i) {
            Arc::make_mut(sl).animations.clear();
        }
        Ok(())
    })?;
    ok()
}

fn list(_s: &mut Session, _p: &Value) -> Result<Value> {
    Ok(Value::Array(ANIMATIONS.iter().map(|a| json!({"id": a.0, "label": a.1, "class": a.2.xml(), "options": a.5})).collect()))
}

fn get(s: &mut Session, p: &Value) -> Result<Value> {
    let st = s.doc()?;
    let i = usize_param(p, "slide").unwrap_or(st.selection.slide);
    let sl = st.doc.slides.get(i).ok_or_else(|| bad("animation.get", "no such slide"))?;
    Ok(serde_json::to_value(&sl.animations).unwrap_or_default())
}

fn painter_pick(s: &mut Session, p: &Value) -> Result<Value> {
    let st = s.doc()?;
    let id = id_param(p, "id")
        .or_else(|| st.selection.shapes.first().copied())
        .ok_or_else(|| bad("animation.painter", "select an animated shape first"))?;
    let sl = st.current_slide().ok_or_else(|| bad("animation.painter", "no slide"))?;
    let anims: Vec<Animation> = sl.animations.iter().filter(|a| a.shape == id).cloned().collect();
    if anims.is_empty() {
        return Err(bad("animation.painter", format!("shape {id} has no animations to copy")));
    }
    let sticky = bool_or(p, "sticky", false);
    let count = anims.len();
    s.anim_painter = Some((anims, sticky));
    Ok(json!({"armed": true, "count": count}))
}

fn painter_apply(s: &mut Session, p: &Value) -> Result<Value> {
    let Some((painted, sticky)) = s.anim_painter.clone() else {
        return Err(bad("animation.painterApply", "pick animations with the Animation Painter first"));
    };
    let ids = targets(s, p)?;
    if ids.is_empty() {
        return Err(bad("animation.painterApply", "no target shapes selected"));
    }
    let result = s.edit(|doc, sel| {
        let sl = Arc::make_mut(doc.slides.get_mut(sel.slide).ok_or_else(|| bad("animation.painterApply", "no slide"))?);
        // As in PowerPoint, painting replaces the target's own animations (painting a shape onto
        // itself doesn't double them). A paragraph effect keeps its paragraph only if the target
        // has that paragraph; otherwise it animates the whole shape.
        sl.animations.retain(|a| !ids.contains(&a.shape));
        let mut added = 0;
        for id in &ids {
            let paras = kition_slide_model::find_shape(&sl.shapes, *id).and_then(|sh| sh.text.as_ref()).map_or(0, |t| t.paragraphs.len());
            for a in &painted {
                let mut cloned = a.clone();
                cloned.shape = *id;
                if cloned.paragraph.is_some_and(|i| i as usize >= paras) {
                    cloned.paragraph = None;
                }
                sl.animations.push(cloned);
                added += 1;
            }
        }
        Ok(json!({"applied": added}))
    })?;
    if !sticky {
        s.anim_painter = None;
    }
    Ok(result)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The numbers of a motion path as (x, y) pairs.
    fn path_points(d: &str) -> Vec<(f64, f64)> {
        let n: Vec<f64> = d.split_whitespace().filter_map(|t| t.parse().ok()).collect();
        n.chunks(2).map(|c| (c[0], c[1])).collect()
    }

    fn path_of(s: &Session, id: u64) -> String {
        let st = s.doc().unwrap();
        st.doc.slides[0].animations.iter().find(|a| a.shape.0 as u64 == id).unwrap().path.clone().unwrap()
    }

    #[test]
    fn turns_turns_and_custom_paths_take_points() {
        // The Turns motion path was a straight line, and customPath had
        // no way to say where it goes.
        let mut s = Session::with_new();
        let id = s.execute("shape.insert", &json!({"preset": "rect", "rect": [100, 100, 100, 100]})).unwrap()["id"].as_u64().unwrap();
        for (option, end) in [("down", (0.25, 0.25)), ("up", (0.25, -0.25)), ("right", (0.25, 0.25)), ("left", (-0.25, 0.25))] {
            s.execute("animation.set", &json!({"effect": "turns", "option": option, "ids": [id]})).unwrap();
            let pts = path_points(&path_of(&s, id));
            let last = *pts.last().unwrap();
            assert!((last.0 - end.0).abs() < 1e-9 && (last.1 - end.1).abs() < 1e-9, "{option}: {pts:?}");
            // Not a straight line: some point is well off the line from the start to the end.
            let off = pts.iter().map(|p| (p.0 * end.1 - p.1 * end.0).abs() / (end.0.hypot(end.1))).fold(0.0, f64::max);
            assert!(off > 0.05, "{option} is a straight line: {pts:?}");
        }
        // customPath goes through the given points (points from the shape's place; 960 × 540 slide).
        s.execute("animation.set", &json!({"effect": "customPath", "points": [[96, 0], [96, 54], [192, 108]], "ids": [id]})).unwrap();
        assert_eq!(path_of(&s, id), "M 0 0 L 0.1 0 L 0.1 0.1 L 0.2 0.2 E");
        s.execute("animation.add", &json!({"effect": "customPath", "points": [[96, 0], [0, 54]], "closed": true, "smooth": true, "ids": [id]}))
            .unwrap();
        let d = s.doc().unwrap().doc.slides[0].animations.last().unwrap().path.clone().unwrap();
        assert!(d.starts_with("M 0 0 C ") && d.ends_with(" Z E"), "{d}");
        // Without points, or points for another effect, the call is refused.
        assert!(s.execute("animation.set", &json!({"effect": "customPath", "ids": [id]})).is_err());
        assert!(s.execute("animation.set", &json!({"effect": "fade", "points": [[1, 2]], "ids": [id]})).is_err());
        assert!(s.execute("animation.set", &json!({"effect": "customPath", "points": [[1]], "ids": [id]})).is_err());
    }

    #[test]
    fn animation_painter_copies_effects_between_shapes() {
        let mut s = Session::with_new();
        // Insert two shapes
        let s1 = s.execute("shape.insert", &json!({"preset": "rect", "rect": [10.0, 10.0, 100.0, 100.0]})).unwrap();
        let id1 = s1.get("id").unwrap().as_u64().unwrap();
        let s2 = s.execute("shape.insert", &json!({"preset": "ellipse", "rect": [150.0, 10.0, 100.0, 100.0]})).unwrap();
        let id2 = s2.get("id").unwrap().as_u64().unwrap();

        // Add 2 animations to shape 1: fade (entrance) and spin (emphasis)
        s.execute("animation.add", &json!({"effect": "fade", "class": "entrance", "ids": [id1]})).unwrap();
        s.execute("animation.add", &json!({"effect": "spin", "class": "emphasis", "ids": [id1]})).unwrap();

        // Shape 2 initially has no animations
        let anims_before = s.execute("animation.get", &json!({})).unwrap();
        let arr_before = anims_before.as_array().unwrap();
        assert_eq!(arr_before.len(), 2);
        assert!(arr_before.iter().all(|a| a.get("shape").unwrap().as_u64().unwrap() == id1));

        // Pick animations from shape 1
        s.select(|_, sel| {
            sel.shapes = vec![kition_slide_model::ShapeId(id1 as u32)];
        })
        .unwrap();
        let res = s.execute("animation.painter", &json!({})).unwrap();
        assert_eq!(res.get("armed").unwrap().as_bool(), Some(true));
        assert_eq!(res.get("count").unwrap().as_u64(), Some(2));
        assert!(s.anim_painter.is_some());

        // Apply animations to shape 2
        s.select(|_, sel| {
            sel.shapes = vec![kition_slide_model::ShapeId(id2 as u32)];
        })
        .unwrap();
        let applied = s.execute("animation.painterApply", &json!({})).unwrap();
        assert_eq!(applied.get("applied").unwrap().as_u64(), Some(2));

        // Single-click should disarm the painter
        assert!(s.anim_painter.is_none());

        // Painting again replaces shape 2's animations instead of adding to them; undo restores.
        s.execute("animation.painter", &json!({"id": id1})).unwrap();
        s.execute("animation.painterApply", &json!({"ids": [id2]})).unwrap();
        let count_on = |s: &mut Session, id: u64| {
            let all = s.execute("animation.get", &json!({})).unwrap();
            all.as_array().unwrap().iter().filter(|a| a.get("shape").unwrap().as_u64().unwrap() == id).count()
        };
        assert_eq!(count_on(&mut s, id2), 2);
        s.execute("edit.undo", &json!({})).unwrap();
        assert_eq!(count_on(&mut s, id2), 2, "undo restores the first painting");
        s.execute("edit.undo", &json!({})).unwrap();
        assert_eq!(count_on(&mut s, id2), 0, "undo removes the painted animations");
        s.execute("edit.redo", &json!({})).unwrap();

        // Verify shape 2 now has the 2 animations cloned
        let anims_after = s.execute("animation.get", &json!({})).unwrap();
        let arr_after = anims_after.as_array().unwrap();
        assert_eq!(arr_after.len(), 4);
        let s2_anims: Vec<_> = arr_after.iter().filter(|a| a.get("shape").unwrap().as_u64().unwrap() == id2).collect();
        assert_eq!(s2_anims.len(), 2);
        assert_eq!(s2_anims[0].get("effect").unwrap().as_str(), Some("fade"));
        assert_eq!(s2_anims[1].get("effect").unwrap().as_str(), Some("spin"));

        // Picking from shape with no animations returns an error
        s.execute("animation.clear", &json!({})).unwrap();
        let err = s.execute("animation.painter", &json!({"id": id1}));
        assert!(err.is_err());
    }

    #[test]
    fn animation_painter_sticky_mode() {
        let mut s = Session::with_new();
        let s1 = s.execute("shape.insert", &json!({"preset": "rect", "rect": [10.0, 10.0, 100.0, 100.0]})).unwrap();
        let id1 = s1.get("id").unwrap().as_u64().unwrap();
        let s2 = s.execute("shape.insert", &json!({"preset": "ellipse", "rect": [150.0, 10.0, 100.0, 100.0]})).unwrap();
        let id2 = s2.get("id").unwrap().as_u64().unwrap();
        let s3 = s.execute("shape.insert", &json!({"preset": "roundRect", "rect": [300.0, 10.0, 100.0, 100.0]})).unwrap();
        let id3 = s3.get("id").unwrap().as_u64().unwrap();

        s.execute("animation.add", &json!({"effect": "zoom", "class": "entrance", "ids": [id1]})).unwrap();

        // Pick with sticky = true
        s.execute("animation.painter", &json!({"id": id1, "sticky": true})).unwrap();
        assert!(s.anim_painter.is_some());

        // Apply to shape 2 - painter remains armed
        s.execute("animation.painterApply", &json!({"ids": [id2]})).unwrap();
        assert!(s.anim_painter.is_some());

        // Apply to shape 3 - painter still armed
        s.execute("animation.painterApply", &json!({"ids": [id3]})).unwrap();
        assert!(s.anim_painter.is_some());
    }
}
