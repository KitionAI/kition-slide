//! Picture, media options, drawing, arrangement and merge dependencies (C8-slide-read-3).
//!
//! For these commands this module states every premise of the call itself: it drops what the
//! preamble and the earlier family modules pushed and holds what the handler reads (the
//! independent readings in `tests/declarations_independent/rests_c.rs`): the targets' fields the
//! write reads or replaces, where they are when the call depends on their place in the stack or
//! names them by number, a placeholder's inherited frame, the connectors that follow a moved,
//! deleted or grouped shape and their other ends, a group member's group and siblings (the group
//! refit), the neighbour a one-step reorder swaps with, the slide's fields (animations, shape
//! count, layout) and the slide size.

use std::collections::{BTreeMap, BTreeSet};

use super::*;
use kition_slide_model::{Shape, ShapeId, ShapeKind, resolve};

const MINE: &[&str] = &[
    "image.crop",
    "image.adjust",
    "image.reset",
    "image.change",
    "media.options",
    "drawing.delete",
    "drawing.transform",
    "drawing.format",
    "drawing.text",
    "drawing.arrange",
    "drawing.reconnect",
    "drawing.connect",
    "drawing.group",
    "drawing.ungroup",
    "drawing.chart",
    "drawing.ink",
    "arrange.bringToFront",
    "arrange.bringForward",
    "arrange.sendBackward",
    "arrange.sendToBack",
    "arrange.reorder",
    "arrange.group",
    "arrange.ungroup",
    "arrange.regroup",
    "arrange.align",
    "arrange.distribute",
    "arrange.rotateLeft",
    "arrange.rotateRight",
    "arrange.flipHorizontal",
    "arrange.flipVertical",
    "arrange.nudge",
    "shape.merge",
];

const CONNECTOR: [&str; 3] = ["look.geom", "look.kind", "xfrm"];

/// What the call holds of one object: `None` every field (or position), else these.
#[derive(Default)]
struct Hold {
    fields: Option<BTreeSet<String>>,
    positions: Option<BTreeSet<String>>,
}

#[derive(Default)]
struct Holds {
    order: Vec<String>,
    by_id: BTreeMap<String, Hold>,
}

impl Holds {
    fn entry(&mut self, id: &str) -> &mut Hold {
        if !self.by_id.contains_key(id) {
            self.order.push(id.to_owned());
        }
        self.by_id.entry(id.to_owned()).or_insert_with(|| Hold { fields: Some(BTreeSet::new()), positions: Some(BTreeSet::new()) })
    }

    fn fields<'a>(&mut self, id: &str, fields: impl IntoIterator<Item = &'a str>) {
        if let Some(all) = &mut self.entry(id).fields {
            all.extend(fields.into_iter().map(str::to_owned));
        }
    }

    fn every_field(&mut self, id: &str) {
        self.entry(id).fields = None;
    }

    fn every_position(&mut self, id: &str) {
        self.entry(id).positions = None;
    }

    fn position(&mut self, id: &str, position: &str) {
        if let Some(all) = &mut self.entry(id).positions {
            all.insert(position.to_owned());
        }
    }

    fn into_premises(mut self) -> Vec<kition_kit::Premise> {
        let mut out = Vec::new();
        for id in self.order {
            let Some(hold) = self.by_id.remove(&id) else { continue };
            let mut premise = kition_kit::Premise::on(id);
            if let Some(fields) = hold.fields {
                premise = premise.fields(fields);
            }
            if let Some(positions) = hold.positions {
                premise = premise.positions(positions);
            }
            out.push(premise);
        }
        out
    }
}

/// The slide (or layout, master) being edited and how its shapes are named.
struct Part<'a> {
    deck: &'a SlideFile,
    selection: kition_slide_engine::Selection,
    shapes: &'a [Shape],
    ids: &'a kdm_model::Ids,
    owner: String,
    owner_object: &'a kdm_model::kition_kit::kdm::ObjectId,
    placed: bool,
}

impl Part<'_> {
    fn public(&self, shape: &Shape) -> Option<String> {
        match self.selection.target {
            Target::Slides => Some(cmd::inspect::shape_tool_id(shape)),
            _ => self.ids.shape(self.owner_object, shape.id).map(|id| id.as_str().to_owned()),
        }
    }

    fn find(&self, id: ShapeId) -> Option<&Shape> {
        kition_slide_model::find_shape(self.shapes, id)
    }

    fn top(&self, id: ShapeId) -> bool {
        self.shapes.iter().any(|shape| shape.id == id)
    }

    fn slide(&self) -> Option<String> {
        matches!(self.selection.target, Target::Slides).then(|| self.owner.clone())
    }

    fn master(&self) -> Option<String> {
        let ctx = context(self.deck, &self.selection)?;
        self.ids.masters.object(ctx.master.id).map(|id| id.as_str().to_owned())
    }

    /// The groups holding `id`, outermost first.
    fn groups(&self, id: ShapeId) -> Vec<&Shape> {
        fn find<'s>(list: &'s [Shape], id: ShapeId, path: &mut Vec<&'s Shape>, depth: usize) -> bool {
            if depth > 64 {
                return false;
            }
            for shape in list {
                if shape.id == id {
                    return true;
                }
                if matches!(shape.kind, ShapeKind::Group { .. }) {
                    path.push(shape);
                    if find(shape.children(), id, path, depth + 1) {
                        return true;
                    }
                    path.pop();
                }
            }
            false
        }
        let mut path = Vec::new();
        find(self.shapes, id, &mut path, 0);
        path
    }

    /// A shape's frame as the call reads it: its own, or (a placeholder without one) its layout's
    /// or master's placeholder's, which the write then stores on the shape.
    fn frame(&self, shape: &Shape, holds: &mut Holds) {
        let Some(id) = self.public(shape) else { return };
        holds.fields(&id, ["xfrm"]);
        if shape.xfrm.is_some() {
            return;
        }
        holds.fields(&id, ["look.ph"]);
        if let Some(slide) = self.slide() {
            holds.fields(&slide, ["layout"]);
        }
        let Some(ctx) = context(self.deck, &self.selection) else { return };
        let (layout, master) = resolve::parents(&ctx, shape);
        let layout_owner = ctx.layout.and_then(|layout| self.ids.layouts.object(layout.id));
        for (parent, owner) in [(layout, layout_owner), (master, self.ids.masters.object(ctx.master.id))] {
            if let (Some(parent), Some(owner)) = (parent, owner)
                && let Some(object) = self.ids.shape(owner, parent.id)
            {
                holds.fields(object.as_str(), ["xfrm", "look.ph"]);
                if parent.xfrm.is_some() {
                    return;
                }
            }
        }
        holds.fields("slide-file", ["props.slideSize"]);
    }

    /// A shape's connection sites: its frame and geometry (a table's drawn rows too).
    fn sites(&self, shape: &Shape, holds: &mut Holds) {
        self.frame(shape, holds);
        if let Some(id) = self.public(shape) {
            holds.fields(&id, ["look.geom"]);
            if matches!(shape.kind, ShapeKind::Table(_)) {
                holds.fields(&id, ["table"]);
            }
        }
    }

    /// The connectors the edit reroutes: those glued to a shape in `changed` (or one of them),
    /// and the sites of their other ends. Ends in `gone` are not read (removed or grouped).
    fn connectors(&self, changed: &[ShapeId], gone: &[ShapeId], holds: &mut Holds) {
        for connector in self.shapes {
            let ShapeKind::Connector { start, end } = &connector.kind else { continue };
            let glued: Vec<ShapeId> = [start, end].into_iter().flatten().map(|(id, _)| *id).collect();
            if glued.is_empty() || (!changed.contains(&connector.id) && !glued.iter().any(|id| changed.contains(id))) {
                continue;
            }
            if let Some(id) = self.public(connector) {
                holds.fields(&id, CONNECTOR);
            }
            for end in glued.iter().filter(|id| !gone.contains(id)) {
                if let Some(shape) = self.shapes.iter().find(|shape| shape.id == *end) {
                    self.sites(shape, holds);
                }
            }
        }
    }

    /// A group member's groups and their other members: the group refit reads them.
    fn refit(&self, id: ShapeId, holds: &mut Holds) {
        for group in self.groups(id) {
            if let Some(group_id) = self.public(group) {
                holds.fields(&group_id, ["look.kind", "xfrm"]);
            }
            for member in group.children().iter().filter(|member| member.id != id) {
                if let Some(member_id) = self.public(member) {
                    holds.fields(&member_id, ["xfrm"]);
                }
            }
        }
    }
}

pub(super) fn add(doc: &SlideDocument, command: &str, params: &Value, found: &mut kition_kit::Under) -> Result<(), String> {
    if !MINE.contains(&command) {
        return Ok(());
    }
    let state = doc.state()?;
    let Some(session) = state.kdm() else { return Ok(()) };
    let ids = session.model().ids();
    let mut local = params.clone();
    let implied = doc.session.take_tool_ids(&mut local).map_err(command_failure)?;
    let explicit = local.get(ON_SLIDE).and_then(Value::as_u64).and_then(|n| usize::try_from(n).ok()).and_then(|n| n.checked_sub(1));
    let mut selection = state.selection.clone();
    if let Some(on) = implied.or(explicit) {
        selection.slide = on;
        selection.target = Target::Slides;
    }
    let deck = &state.doc;
    let (shapes, owner_object) = match selection.target {
        Target::Slides => deck.slides.get(selection.slide).map(|slide| (slide.shapes.as_slice(), ids.slides.object(slide.id))),
        Target::Master { master } => deck.masters.get(master).map(|master| (master.shapes.as_slice(), ids.masters.object(master.id))),
        Target::Layout { master, layout } => deck
            .masters
            .get(master)
            .and_then(|master| master.layouts.get(layout))
            .map(|layout| (layout.shapes.as_slice(), ids.layouts.object(layout.id))),
    }
    .unwrap_or_default();
    let Some(owner_object) = owner_object else { return Ok(()) };
    let owner = match selection.target {
        Target::Slides => deck.slides.get(selection.slide).map(|slide| format!("slide:{}", slide.id.0)).unwrap_or_default(),
        _ => owner_object.as_str().to_owned(),
    };
    let part = Part { deck, selection, shapes, ids, owner, owner_object, placed: explicit.is_some() };
    let mut holds = Holds::default();
    // The slide a call names by place: where it is in the deck, nothing of its own.
    if part.placed
        && let Some(slide) = part.slide()
    {
        holds.every_position(&slide);
    }
    let number = |value: &Value| {
        value.as_u64().or_else(|| value.as_str().and_then(|value| value.parse().ok())).and_then(|id| u32::try_from(id).ok()).map(ShapeId)
    };
    let named: Option<Vec<ShapeId>> = match (local.get("ids").and_then(Value::as_array).filter(|list| !list.is_empty()), local.get("id")) {
        (Some(list), _) => Some(list.iter().filter_map(number).collect()),
        (None, Some(id)) if !id.is_null() => Some(number(id).into_iter().collect()),
        _ => None,
    };
    let drawing = command.starts_with("drawing.");
    let targets: Vec<ShapeId> = named.unwrap_or_else(|| if drawing { Vec::new() } else { part.selection.shapes.clone() });
    let targets: Vec<&Shape> = targets.iter().filter_map(|id| part.find(*id)).collect();
    let target_ids: Vec<ShapeId> = targets.iter().map(|shape| shape.id).collect();
    let flag = |name: &str| local.get(name).is_some_and(|value| !value.is_null());
    match command {
        "image.crop" | "image.adjust" | "image.reset" | "image.change" | "media.options" => {
            for shape in &targets {
                let Some(id) = part.public(shape) else { continue };
                holds.entry(&id);
                if shape.xfrm.is_none() {
                    part.frame(shape, &mut holds);
                }
                let picture = matches!(shape.kind, ShapeKind::Picture { .. });
                match command {
                    "media.options" if matches!(shape.kind, ShapeKind::Media(_)) => holds.fields(&id, ["look.kind"]),
                    "image.reset" if picture => holds.fields(&id, ["look.kind", "look.line", "look.effects"]),
                    "image.change" if picture => holds.fields(&id, ["look.kind", "media"]),
                    "image.crop" | "image.adjust" if picture => holds.fields(&id, ["look.kind"]),
                    _ => {}
                }
            }
            // The bytes are stored as a media item unless one already holds the same bytes.
            if command == "image.change"
                && let Some(bytes) = local.get("data").and_then(Value::as_str).and_then(cmd::base64_decode)
                && let Some(item) = deck.media.iter().find(|item| *item.data == bytes)
                && let Some(media) = ids.media.object(item.id)
            {
                holds.fields(media.as_str(), ["blob"]);
            }
        }
        "drawing.chart" => {
            for shape in &targets {
                if let Some(id) = part.public(shape) {
                    holds.fields(&id, ["chart"]);
                }
                if shape.xfrm.is_none() {
                    part.frame(shape, &mut holds);
                }
            }
        }
        "drawing.delete" => {
            for shape in &targets {
                // Removing a shape replaces it whole: a colleague's edit of it would be lost.
                if let Some(id) = part.public(shape) {
                    holds.every_field(&id);
                }
                part.refit(shape.id, &mut holds);
                // An emptied group goes: its member list is read.
                for group in part.groups(shape.id) {
                    if let Some(group_id) = part.public(group) {
                        holds.fields(&group_id, ["children"]);
                    }
                }
            }
            let mut gone = target_ids.clone();
            for shape in &targets {
                kition_slide_model::walk(shape.children(), &mut |child, _| gone.push(child.id));
            }
            part.connectors(&gone, &gone, &mut holds);
            if let Some(slide) = part.slide() {
                holds.fields(&slide, ["animations"]);
            }
        }
        "drawing.format" => {
            let geometry = flag("geometry");
            for shape in &targets {
                let Some(id) = part.public(shape) else { continue };
                holds.entry(&id);
                if shape.xfrm.is_none() {
                    part.frame(shape, &mut holds);
                }
                for (name, field) in [("fill", "look.fill"), ("stroke", "look.line"), ("effects", "look.effects")] {
                    if flag(name) {
                        holds.fields(&id, [field]);
                    }
                }
                if geometry {
                    part.sites(shape, &mut holds);
                }
            }
            if geometry {
                part.connectors(&target_ids, &[], &mut holds);
            }
        }
        "drawing.text" => {
            let ctx = context(deck, &part.selection);
            for shape in &targets {
                let Some(id) = part.public(shape) else { continue };
                holds.fields(&id, ["body.paragraphs", "body.props"]);
                if shape.xfrm.is_none() {
                    part.frame(shape, &mut holds);
                }
                let Some(ctx) = &ctx else { continue };
                // Body properties (autofit) inherit from the layout's and master's placeholders.
                let (layout, master) = resolve::parents(ctx, shape);
                let layout_owner = ctx.layout.and_then(|layout| ids.layouts.object(layout.id));
                for (parent, parent_owner) in [(layout, layout_owner), (master, ids.masters.object(ctx.master.id))] {
                    if let (Some(parent), Some(parent_owner)) = (parent, parent_owner)
                        && let Some(object) = ids.shape(parent_owner, parent.id)
                    {
                        holds.fields(object.as_str(), ["body.props"]);
                    }
                }
                // A body that fits its shape re-fits the frame from the laid-out text.
                if resolve::body(ctx, shape).autofit == Some(kition_slide_model::text::AutoFit::Shape) {
                    holds.fields(&id, ["body", "look.geom", "look.textBox", "xfrm"]);
                    holds.fields("slide-file", ["defaultTextStyle"]);
                    if let Some(master_id) = part.master() {
                        holds.fields(&master_id, ["theme.fonts"]);
                        if shape.ph.is_some() {
                            holds.fields(&master_id, ["listStyles"]);
                        }
                    }
                    if shape.ph.is_some() {
                        for (parent, parent_owner) in [(layout, layout_owner), (master, ids.masters.object(ctx.master.id))] {
                            if let (Some(parent), Some(parent_owner)) = (parent, parent_owner)
                                && let Some(object) = ids.shape(parent_owner, parent.id)
                            {
                                holds.fields(object.as_str(), ["body"]);
                            }
                        }
                    }
                    part.connectors(&[shape.id], &[], &mut holds);
                }
            }
        }
        "drawing.transform" | "arrange.rotateLeft" | "arrange.rotateRight" | "arrange.flipHorizontal" | "arrange.flipVertical" | "arrange.nudge" => {
            for shape in &targets {
                part.frame(shape, &mut holds);
                if command != "drawing.transform" {
                    part.refit(shape.id, &mut holds);
                }
            }
            moved(&part, &target_ids, &mut holds);
        }
        "arrange.align" | "arrange.distribute" => {
            let moving = aligned(&part, command, &local, &targets, &mut holds);
            for shape in &targets {
                part.frame(shape, &mut holds);
                if matches!(shape.kind, ShapeKind::Table(_))
                    && let Some(id) = part.public(shape)
                {
                    holds.fields(&id, ["table"]);
                }
                for group in part.groups(shape.id) {
                    if let Some(group_id) = part.public(group) {
                        holds.fields(&group_id, ["look.kind", "xfrm"]);
                    }
                }
            }
            for id in &moving {
                part.refit(*id, &mut holds);
            }
            moved(&part, &moving, &mut holds);
        }
        "arrange.bringToFront" | "arrange.bringForward" | "arrange.sendBackward" | "arrange.sendToBack" | "drawing.arrange" => {
            let order = match command {
                "arrange.bringToFront" => "front",
                "arrange.sendToBack" => "back",
                "arrange.bringForward" => "forward",
                "arrange.sendBackward" => "backward",
                _ => local.get("order").and_then(Value::as_str).unwrap_or_default(),
            };
            for shape in &targets {
                let Some(id) = part.public(shape) else { continue };
                holds.entry(&id);
                if !part.top(shape.id) {
                    continue;
                }
                holds.every_position(&id);
                // A one-step move swaps with the nearest other shape that way.
                let Some(at) = part.shapes.iter().position(|other| other.id == shape.id) else { continue };
                let neighbour = match order {
                    "forward" => part.shapes.iter().skip(at.saturating_add(1)).find(|other| !target_ids.contains(&other.id)),
                    "backward" => part.shapes.iter().take(at).rev().find(|other| !target_ids.contains(&other.id)),
                    _ => None,
                };
                if let Some(neighbour) = neighbour.and_then(|other| part.public(other)) {
                    holds.every_position(&neighbour);
                }
            }
        }
        "arrange.reorder" => {
            for shape in &targets {
                let Some(id) = part.public(shape) else { continue };
                holds.every_position(&id);
                let to = local.get("index").and_then(Value::as_u64).and_then(|n| usize::try_from(n).ok());
                let rest: Vec<&Shape> = part.shapes.iter().filter(|other| other.id != shape.id).collect();
                // The shape it lands above.
                if let Some(below) = to.map(|to| to.min(rest.len())).and_then(|to| to.checked_sub(1)).and_then(|at| rest.get(at))
                    && let Some(below) = part.public(below)
                {
                    holds.every_position(&below);
                }
            }
        }
        "arrange.group" | "drawing.group" | "arrange.regroup" => {
            let members: Vec<&&Shape> = targets.iter().filter(|shape| part.top(shape.id)).collect();
            for shape in &members {
                let Some(id) = part.public(shape) else { continue };
                holds.fields(&id, ["look.ph"]);
                holds.every_position(&id);
                part.frame(shape, &mut holds);
            }
            let grouped: Vec<ShapeId> = members.iter().map(|shape| shape.id).collect();
            part.connectors(&grouped, &grouped, &mut holds);
            holds.fields(&part.owner, ["~shapeCount"]);
        }
        "arrange.ungroup" | "drawing.ungroup" => {
            let mut released = Vec::new();
            for shape in &targets {
                let Some(id) = part.public(shape) else { continue };
                holds.entry(&id);
                if !part.top(shape.id) || !matches!(shape.kind, ShapeKind::Group { .. }) {
                    continue;
                }
                holds.fields(&id, ["children", "look.kind", "xfrm"]);
                holds.every_position(&id);
                for member in shape.children() {
                    if let Some(member_id) = part.public(member) {
                        holds.fields(&member_id, ["xfrm"]);
                        holds.every_position(&member_id);
                    }
                    released.push(member.id);
                }
            }
            part.connectors(&released, &[], &mut holds);
        }
        "shape.merge" => {
            for (index, shape) in targets.iter().enumerate() {
                let Some(id) = part.public(shape) else { continue };
                if index == 0 {
                    // The results copy it and take its place in the stack.
                    holds.every_field(&id);
                    holds.every_position(&id);
                }
                part.sites(shape, &mut holds);
            }
            part.connectors(&target_ids, &target_ids, &mut holds);
        }
        "drawing.reconnect" => {
            let end = local.get("end").and_then(Value::as_str).unwrap_or("end");
            let to = local.get("to").and_then(number).and_then(|id| part.find(id));
            for shape in &targets {
                if let Some(id) = part.public(shape) {
                    holds.fields(&id, CONNECTOR);
                }
                if shape.xfrm.is_none() {
                    part.frame(shape, &mut holds);
                }
                // The end kept is routed from its shape's sites.
                if let ShapeKind::Connector { start, end: finish } = &shape.kind {
                    let kept = if end == "start" { finish } else { start };
                    if let Some(kept) = kept.and_then(|(id, _)| part.shapes.iter().find(|other| other.id == id)) {
                        part.sites(kept, &mut holds);
                    }
                }
            }
            if let Some(to) = to {
                part.sites(to, &mut holds);
            }
        }
        "drawing.connect" => {
            for name in ["from", "to"] {
                if let Some(shape) = local.get(name).and_then(number).and_then(|id| part.find(id)) {
                    part.sites(shape, &mut holds);
                }
            }
            holds.fields(&part.owner, ["~shapeCount"]);
        }
        "drawing.ink" => {
            holds.entry(&part.owner);
            match part.shapes.iter().rev().find(|shape| matches!(shape.kind, ShapeKind::Ink { .. })) {
                Some(layer) => {
                    if let Some(id) = part.public(layer) {
                        holds.fields(&id, ["look.kind"]);
                    }
                }
                None => holds.fields("slide-file", ["props.slideSize"]),
            }
        }
        _ => {}
    }
    // A shape named by its number is held where that number is.
    for (slide, number) in doc.addressed_shapes(command, params) {
        if let Some(shape) = deck.slides.iter().find(|s| s.id.0 == slide).and_then(|s| s.shape(ShapeId(number))) {
            holds.position(&cmd::inspect::shape_tool_id(shape), "address");
        }
    }
    found.premises = holds.into_premises();
    Ok(())
}

/// The frames a geometric edit changes: the connectors glued to them follow (every end's sites).
fn moved(part: &Part, changed: &[ShapeId], holds: &mut Holds) {
    part.connectors(changed, &[], holds);
}

/// The targets an alignment or distribution moves (holding the slide size when it aligns to the
/// slide). A group member is taken to move.
fn aligned(part: &Part, command: &str, params: &Value, targets: &[&Shape], holds: &mut Holds) -> Vec<ShapeId> {
    let size = part.deck.slide_size;
    let boxes: Vec<(ShapeId, kition_slide_geom::Rect, bool)> =
        targets.iter().map(|shape| (shape.id, xfrm_of(part.deck, &part.selection, shape).bounds(), !part.groups(shape.id).is_empty())).collect();
    let to = params.get("to").and_then(Value::as_str);
    let near = |a: f64, b: f64| (a - b).abs() < 1e-6;
    let mut out = Vec::new();
    if command == "arrange.align" {
        let to_slide = match to {
            Some("slide") => true,
            Some(_) => false,
            None => boxes.len() < 2,
        };
        if to_slide {
            holds.fields("slide-file", ["props.slideSize"]);
        }
        let reference = if to_slide {
            Some(kition_slide_geom::Rect::new(0.0, 0.0, size.width, size.height))
        } else {
            boxes.iter().map(|b| b.1).reduce(|a, b| a.union(b))
        };
        let edge = params.get("edge").and_then(Value::as_str).unwrap_or("left");
        for (id, b, member) in &boxes {
            let Some(r) = reference else { continue };
            let still = match edge {
                "left" => near(r.x0, b.x0),
                "center" => near(r.center().x, b.center().x),
                "right" => near(r.x1, b.x1),
                "top" => near(r.y0, b.y0),
                "middle" => near(r.center().y, b.center().y),
                "bottom" => near(r.y1, b.y1),
                _ => true,
            };
            if *member || !still {
                out.push(*id);
            }
        }
        return out;
    }
    let horizontal = !params.get("dir").and_then(Value::as_str).unwrap_or("horizontal").starts_with('v');
    let to_slide = to == Some("slide") || boxes.len() < 3;
    if to_slide {
        holds.fields("slide-file", ["props.slideSize"]);
    }
    let mut sorted: Vec<&(ShapeId, kition_slide_geom::Rect, bool)> = boxes.iter().collect();
    sorted.sort_by(|a, b| if horizontal { a.1.x0.total_cmp(&b.1.x0) } else { a.1.y0.total_cmp(&b.1.y0) });
    let extent = |r: &kition_slide_geom::Rect| if horizontal { r.width() } else { r.height() };
    let origin = |r: &kition_slide_geom::Rect| if horizontal { r.x0 } else { r.y0 };
    let total: f64 = sorted.iter().map(|b| extent(&b.1)).sum();
    let (start, end) = if to_slide {
        (0.0, if horizontal { size.width } else { size.height })
    } else {
        let first = sorted.first().map(|b| origin(&b.1)).unwrap_or_default();
        let last = sorted.last().map(|b| if horizontal { b.1.x1 } else { b.1.y1 }).unwrap_or_default();
        (first, last)
    };
    let n = sorted.len();
    let gap = if n > 1 { ((end - start) - total) / (n - 1) as f64 } else { 0.0 };
    let mut cur = start;
    if n == 1
        && let Some(only) = sorted.first()
    {
        cur = start + ((end - start) - extent(&only.1)) / 2.0;
    }
    for (id, b, member) in sorted {
        if *member || !near(cur, origin(b)) {
            out.push(*id);
        }
        cur += extent(b) + gap;
    }
    out
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
