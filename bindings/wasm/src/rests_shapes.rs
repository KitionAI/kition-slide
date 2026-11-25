//! Shape dependencies established by the independent handler reading.

use super::*;
use kition_slide_model::{Shape, ShapeId, ShapeKind, resolve};

#[path = "rests_shapes_mine.rs"]
mod mine;
pub(super) use mine::{Holds, Scope};

pub(super) fn add(doc: &SlideDocument, command: &str, params: &Value, found: &mut kition_kit::Under) -> Result<(), String> {
    if mine::owns(command) {
        return mine::add(doc, command, params, found);
    }
    // rests_other answers for it.
    if command == "media.posterFrame" {
        return Ok(());
    }
    legacy(doc, command, params, found)
}

fn hold(found: &mut kition_kit::Under, id: impl Into<Value>, fields: impl IntoIterator<Item = impl Into<String>>) {
    let id = id.into();
    let fields: Vec<String> = fields.into_iter().map(Into::into).collect();
    if let Some(previous) = found.premises.iter_mut().rev().find(|premise| premise.id == id) {
        let all = previous.fields.get_or_insert_with(Vec::new);
        for field in fields {
            if !all.contains(&field) {
                all.push(field)
            }
        }
        if previous.positions.is_none() {
            previous.positions = Some(Vec::new())
        }
    } else {
        found.premises.push(kition_kit::Premise::on(id).fields(fields).positions(Vec::<String>::new()));
    }
}

fn legacy(doc: &SlideDocument, command: &str, params: &Value, found: &mut kition_kit::Under) -> Result<(), String> {
    let shape_command = command.starts_with("shape.")
        || command.starts_with("picture.")
        || command.starts_with("drawing.")
        || command.starts_with("arrange.")
        || command.starts_with("chart.")
        || command.starts_with("insert.")
        || matches!(command, "media.options" | "media.posterFrame");
    if !shape_command {
        return Ok(());
    }
    let state = doc.state()?;
    let mut local = params.clone();
    let implied = doc.session.take_tool_ids(&mut local).map_err(command_failure)?;
    let explicit = local.get(ON_SLIDE).and_then(Value::as_u64).and_then(|n| usize::try_from(n).ok()).and_then(|n| n.checked_sub(1));
    let mut selection = state.selection.clone();
    if let Some(on) = implied.or(explicit) {
        selection.slide = on;
        selection.target = Target::Slides;
    }
    let shapes = match selection.target {
        Target::Slides => state.doc.slides.get(selection.slide).map(|slide| slide.shapes.as_slice()),
        Target::Master { master } => state.doc.masters.get(master).map(|master| master.shapes.as_slice()),
        Target::Layout { master, layout } => {
            state.doc.masters.get(master).and_then(|master| master.layouts.get(layout)).map(|layout| layout.shapes.as_slice())
        }
    };
    let Some(shapes) = shapes else { return Ok(()) };
    let Some(session) = state.kdm() else { return Ok(()) };
    let ids = session.model().ids();
    let owner = match selection.target {
        Target::Slides => state.doc.slides.get(selection.slide).and_then(|slide| ids.slides.object(slide.id)),
        Target::Master { master } => state.doc.masters.get(master).and_then(|master| ids.masters.object(master.id)),
        Target::Layout { master, layout } => {
            state.doc.masters.get(master).and_then(|master| master.layouts.get(layout)).and_then(|layout| ids.layouts.object(layout.id))
        }
    };
    let Some(owner) = owner else { return Ok(()) };
    let public = |shape: &Shape| match selection.target {
        Target::Slides => Some(cmd::inspect::shape_tool_id(shape)),
        _ => ids.shape(owner, shape.id).map(|id| id.as_str().to_owned()),
    };
    let number = |value: &Value| {
        value.as_u64().or_else(|| value.as_str().and_then(|value| value.parse().ok())).and_then(|id| u32::try_from(id).ok()).map(ShapeId)
    };
    let mut targets: Vec<ShapeId> = match local.get("ids").and_then(Value::as_array).filter(|list| !list.is_empty()) {
        Some(list) => list.iter().filter_map(number).collect(),
        None => local.get("id").and_then(number).map(|id| vec![id]).unwrap_or_else(|| selection.shapes.clone()),
    };
    if command == "media.posterFrame" && targets.is_empty() {
        let media: Vec<_> = shapes.iter().filter(|shape| matches!(shape.kind, ShapeKind::Media(_))).map(|shape| shape.id).collect();
        if media.len() == 1 {
            targets = media
        }
    }
    let existing_targets = (command.starts_with("shape.") && !matches!(command, "shape.insert" | "shape.freeform" | "shape.sites"))
        || command.starts_with("picture.")
        || command.starts_with("chart.")
        || command.starts_with("arrange.")
        || matches!(
            command,
            "media.options"
                | "media.posterFrame"
                | "link.insert"
                | "drawing.delete"
                | "drawing.transform"
                | "drawing.format"
                | "drawing.text"
                | "drawing.arrange"
                | "drawing.reconnect"
                | "drawing.group"
                | "drawing.ungroup"
                | "drawing.chart"
        );
    if !existing_targets || (command == "shape.connect" && local.get("id").is_none()) {
        targets.clear();
    }
    let mut replacing_picture = false;
    if command == "image.insert" {
        for selected in &selection.shapes {
            if let Some(shape) = kition_slide_model::find_shape(shapes, *selected) {
                if let Some(id) = public(shape) {
                    hold(found, id, ["look.ph", "body"])
                }
                if shape.ph_type().is_some_and(|kind| {
                    matches!(kind, kition_slide_model::PhType::Picture | kition_slide_model::PhType::Obj | kition_slide_model::PhType::Body)
                }) && shape.text.as_ref().is_none_or(|body| body.is_empty())
                {
                    targets = vec![shape.id];
                    replacing_picture = true;
                    break;
                }
            }
        }
    }
    let geometric = matches!(
        command,
        "shape.move"
            | "shape.resize"
            | "shape.setBounds"
            | "shape.rotate"
            | "shape.flip"
            | "arrange.align"
            | "arrange.distribute"
            | "arrange.nudge"
            | "arrange.rotateLeft"
            | "arrange.rotateRight"
            | "arrange.flipHorizontal"
            | "arrange.flipVertical"
            | "drawing.transform"
            | "drawing.reconnect"
            | "shape.connect"
            | "drawing.connect"
    );
    let member_fields: Option<&[&str]> = match command {
        "shape.fill" => Some(&["look.fill"]),
        "shape.line" => Some(&["look.line"]),
        "shape.effects" => Some(&["look.effects"]),
        "shape.quickStyle" => Some(&["look.style", "look.fill", "look.line", "look.effects", "body"]),
        "shape.adjust" | "shape.change" => Some(&["look.geom"]),
        "shape.rename" => Some(&["name"]),
        "shape.altText" => Some(&["look.descr", "look.decorative"]),
        "shape.visible" => Some(&["look.hidden"]),
        "shape.lock" => Some(&["look.locked"]),
        "shape.setDefault" => Some(&["body", "look.fill", "look.line", "look.effects", "look.style"]),
        "image.crop" | "image.adjust" | "image.change" | "media.options" => Some(&["kind", "look.kind"]),
        "image.reset" => Some(&["kind", "look.kind", "look.line", "look.effects"]),
        "chart.type" | "chart.data" | "chart.options" | "drawing.chart" => Some(&["kind", "chart"]),
        "link.insert" if local.get("id").is_some() || local.get("ids").is_some() => Some(&["look.click"]),
        _ => None,
    };
    for target in &targets {
        let Some(shape) = kition_slide_model::find_shape(shapes, *target) else { continue };
        if let (Some(fields), Some(id)) = (member_fields, public(shape)) {
            let mut fields: Vec<String> = fields.iter().map(|field| (*field).to_owned()).collect();
            if shape.xfrm.is_none() {
                fields.extend(["xfrm".to_owned(), "look.ph".to_owned()]);
            }
            if shape.children().is_empty() || !matches!(command, "shape.fill" | "shape.line" | "shape.effects" | "shape.quickStyle") {
                hold(found, id, fields);
            } else {
                fields.extend(["kind".to_owned(), "children".to_owned()]);
                hold(found, id, fields.clone());
                kition_slide_model::walk(shape.children(), &mut |child, _| {
                    if let Some(id) = public(child) {
                        hold(found, id, fields.clone());
                    }
                });
            }
        } else if let Some(id) = public(shape) {
            let mut premise = kition_kit::Premise::on(id);
            if acts::position_independent(command) {
                premise = premise.positions(Vec::<String>::new())
            }
            found.premises.push(premise);
        }
        if geometric {
            ancestor_geometry(shapes, *target, &public, found, 0);
        }
        if shape.xfrm.is_none() {
            inherited_geometry(&state.doc, &selection, shape, ids, found);
        }
        if command == "shape.fill"
            && params.get("transparency").is_some()
            && !params.get("none").and_then(Value::as_bool).unwrap_or(false)
            && !params.get("reset").and_then(Value::as_bool).unwrap_or(false)
            && !["color", "gradient", "pattern", "picture"].iter().any(|name| params.get(*name).is_some_and(|value| !value.is_null()))
        {
            inherited_fill(&state.doc, &selection, shape, ids, &public, found);
        }
    }
    if matches!(command, "shape.connect" | "drawing.connect" | "drawing.reconnect") {
        for name in ["from", "to"] {
            if let Some(shape) = local.get(name).and_then(number).and_then(|id| kition_slide_model::find_shape(shapes, id)) {
                if let Some(id) = public(shape) {
                    let mut fields = vec!["kind", "xfrm", "look.geom", "look.ph"];
                    if matches!(shape.kind, ShapeKind::Table(_)) {
                        fields.push("table")
                    }
                    hold(found, id, fields);
                }
                ancestor_geometry(shapes, shape.id, &public, found, 0);
                if shape.xfrm.is_none() {
                    inherited_geometry(&state.doc, &selection, shape, ids, found)
                }
            }
        }
    }
    // Moving one glued end reroutes the connector using its retained end and the other shape's
    // sites. The colleague can change an unconnected shape independently.
    if geometric {
        for connector in shapes {
            let ShapeKind::Connector { start, end } = &connector.kind else { continue };
            if !targets.contains(&connector.id) && ![start, end].into_iter().flatten().any(|(id, _)| targets.contains(id)) {
                continue;
            }
            if let Some(id) = public(connector) {
                hold(found, id, ["xfrm", "look.kind", "look.geom"])
            }
            for (endpoint, _) in [start, end].into_iter().flatten() {
                if let Some(shape) = kition_slide_model::find_shape(shapes, *endpoint) {
                    if let Some(id) = public(shape) {
                        hold(found, id, ["kind", "xfrm", "look.geom", "look.ph"])
                    }
                    if shape.xfrm.is_none() {
                        inherited_geometry(&state.doc, &selection, shape, ids, found)
                    }
                }
            }
        }
    }
    hyperlink(doc, command, params, &selection, found);
    let creates = matches!(
        command,
        "shape.insert"
            | "shape.freeform"
            | "textBox.insert"
            | "image.insert"
            | "table.insert"
            | "chart.insert"
            | "audio.insert"
            | "video.insert"
            | "artText.insert"
            | "smartGraphic.insert"
            | "actionButton.insert"
            | "drawing.insert"
            | "drawing.connect"
    ) || (command == "shape.connect" && params.get("id").is_none());
    let creates = creates && !replacing_picture;
    if creates {
        let owner_id = match selection.target {
            Target::Slides => state.doc.slides.get(selection.slide).map(|slide| format!("slide:{}", slide.id.0)),
            _ => Some(owner.as_str().to_owned()),
        };
        if let Some(owner_id) = owner_id {
            hold(found, owner_id, ["~shapeCount"]);
        }
        let frame_missing = if command == "drawing.insert" { params.get("frame").is_none() } else { params.get("rect").is_none() };
        if frame_missing && !matches!(command, "shape.freeform" | "shape.connect" | "drawing.connect") {
            hold(found, "slide-file", ["props.slideSize"]);
        }
    }
    Ok(())
}

pub(super) fn ancestor_geometry(
    shapes: &[Shape],
    target: ShapeId,
    public: &impl Fn(&Shape) -> Option<String>,
    found: &mut kition_kit::Under,
    depth: usize,
) -> bool {
    if depth > 64 {
        return false;
    }
    for shape in shapes {
        if shape.id == target {
            return true;
        }
        if ancestor_geometry(shape.children(), target, public, found, depth + 1) {
            if let Some(id) = public(shape) {
                hold(found, id, ["xfrm", "look.kind.child"]);
            }
            return true;
        }
    }
    false
}

fn inherited_geometry(
    deck: &SlideFile,
    selection: &kition_slide_engine::Selection,
    shape: &Shape,
    ids: &kdm_model::Ids,
    found: &mut kition_kit::Under,
) {
    let Some(ctx) = context(deck, selection) else { return };
    if matches!(selection.target, Target::Slides)
        && let Some(slide) = deck.slides.get(selection.slide)
    {
        hold(found, format!("slide:{}", slide.id.0), ["layout"]);
    }
    let (layout, master) = resolve::parents(&ctx, shape);
    let layout_owner = ctx.layout.and_then(|layout| ids.layouts.object(layout.id));
    for (parent, owner) in [(layout, layout_owner), (master, ids.masters.object(ctx.master.id))] {
        if let (Some(parent), Some(owner)) = (parent, owner)
            && let Some(object) = ids.shape(owner, parent.id)
        {
            hold(found, object.as_str(), ["xfrm", "look.ph"]);
            if parent.xfrm.is_some() {
                return;
            }
        }
    }
    hold(found, "slide-file", ["props.slideSize"]);
}

fn inherited_fill(
    deck: &SlideFile,
    selection: &kition_slide_engine::Selection,
    shape: &Shape,
    ids: &kdm_model::Ids,
    public: &impl Fn(&Shape) -> Option<String>,
    found: &mut kition_kit::Under,
) {
    if !shape.children().is_empty() {
        for child in shape.children() {
            inherited_fill(deck, selection, child, ids, public, found);
        }
        return;
    }
    if shape.fill.is_some() {
        return;
    }
    let Some(ctx) = context(deck, selection) else { return };
    if matches!(selection.target, Target::Slides)
        && let Some(slide) = deck.slides.get(selection.slide)
    {
        hold(found, format!("slide:{}", slide.id.0), ["layout"]);
    }
    let (layout, master) = resolve::parents(&ctx, shape);
    let layout_owner = ctx.layout.and_then(|layout| ids.layouts.object(layout.id));
    let mut colors = false;
    for (source, object) in std::iter::once((shape, public(shape))).chain(
        [(layout, layout_owner), (master, ids.masters.object(ctx.master.id))]
            .into_iter()
            .filter_map(|(shape, owner)| Some((shape?, owner?)))
            .map(|(shape, owner)| (shape, ids.shape(owner, shape.id).map(|id| id.as_str().to_owned()))),
    ) {
        if let Some(object) = object {
            hold(found, object, ["look.fill", "look.style", "look.ph"]);
        }
        if let Some(fill) = &source.fill {
            colors = matches!(fill, kition_slide_model::Fill::Solid { color } if matches!(color.base, kition_slide_model::ColorBase::Scheme { .. }));
            break;
        }
        if source.style.is_some() {
            colors = true;
            if let Some(master) = ids.masters.object(ctx.master.id) {
                hold(found, master.as_str(), ["theme.format"]);
            }
            break;
        }
    }
    if colors && let Some(master) = ids.masters.object(ctx.master.id) {
        hold(found, master.as_str(), ["theme.colors", "colorMap"]);
    }
}

fn hyperlink(doc: &SlideDocument, command: &str, params: &Value, selection: &kition_slide_engine::Selection, found: &mut kition_kit::Under) {
    if command != "link.insert" || params.get("url").is_some() {
        return;
    }
    let Some(index) = params.get("slide").and_then(Value::as_u64).and_then(|index| usize::try_from(index).ok()) else { return };
    let Some(slide) = doc.pres().and_then(|deck| deck.slides.get(index)) else { return };
    let id = format!("slide:{}", slide.id.0);
    found.objects.push(json!(id));
    hold(found, id, Vec::<String>::new());
    found.within.push(kition_kit::Within {
        parent: json!("slide-file"),
        field: "slides".to_owned(),
        from: index,
        to: index + 1,
        ..Default::default()
    });
    // Earlier steps can enter text editing before this step; no explicit shape target then
    // means the empty caret may consume the destination title as its display text.
    if params.get("text").is_none() && (selection.text.is_some() || (params.get("id").is_none() && params.get("ids").is_none())) {
        for shape in &slide.shapes {
            let mut fields = vec!["look.ph"];
            if shape.ph_type().is_some_and(kition_slide_model::PhType::is_title) {
                fields.push("body");
            }
            hold(found, cmd::inspect::shape_tool_id(shape), fields);
        }
    }
}

fn context<'a>(deck: &'a SlideFile, selection: &kition_slide_engine::Selection) -> Option<resolve::Ctx<'a>> {
    match selection.target {
        Target::Slides => resolve::Ctx::for_slide(deck, deck.slides.get(selection.slide)?),
        Target::Master { master } => Some(resolve::Ctx::for_master(deck, deck.masters.get(master)?)),
        Target::Layout { master, layout } => {
            let master = deck.masters.get(master)?;
            Some(resolve::Ctx::for_layout(deck, master, master.layouts.get(layout)?))
        }
    }
}
