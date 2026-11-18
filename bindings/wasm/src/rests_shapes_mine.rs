//! What the shape commands (`shape.*`, the shape inserts, `chart.*`) and the clipboard and delete
//! commands rest on, as the independent reading of their handlers says (C8-slide-read-3): each
//! object they read with the fields they read and the positions they depend on, nothing more.
//!
//! The reads, by handler path:
//! - `edit_shapes` (cmd/mod.rs) acts on `ids`/`id` else the selection; a target without its own
//!   box gets the box it inherits written in (its placeholder kind, the layout and master
//!   placeholders it matches, `resolve::xfrm`).
//! - `Session::edit` reroutes the connectors glued to a shape whose box or geometry changed (the
//!   connector's glue, geometry and box, its other end's geometry and box), refits a group to its
//!   members' union, and refits shrink-to-fit text whose box, geometry or text changed
//!   (`autofit::invalidate`: the body, the layout and master placeholders' bodies and styles, the
//!   master's list style and theme fonts, the deck's default text style).
//! - `add_shape` names a new shape after its tree's shape count; media are deduplicated by bytes.

use std::collections::BTreeSet;

use super::super::*;
use kition_slide_engine::Selection;
use kition_slide_model::resolve::{self, Ctx};
use kition_slide_model::text::AutoFit;
use kition_slide_model::{PhType, Shape, ShapeId, ShapeKind, Slide};

/// The commands this module answers for.
pub(in super::super) fn owns(command: &str) -> bool {
    (command.starts_with("shape.") && command != "shape.merge")
        || command.starts_with("chart.")
        || matches!(
            command,
            "insert.textBox"
                | "insert.picture"
                | "insert.table"
                | "insert.chart"
                | "insert.audio"
                | "insert.video"
                | "insert.artText"
                | "insert.smartGraphic"
                | "insert.actionButton"
        )
}

/// What one call holds, by object: its fields (`None`: every one) and positions (`None`: every
/// one), in the order first named.
#[derive(Default)]
pub(in super::super) struct Holds {
    list: Vec<(String, Option<BTreeSet<String>>, Option<BTreeSet<String>>)>,
}

impl Holds {
    fn entry(&mut self, id: &str) -> Option<&mut (String, Option<BTreeSet<String>>, Option<BTreeSet<String>>)> {
        if !self.has(id) {
            self.list.push((id.to_owned(), Some(BTreeSet::new()), Some(BTreeSet::new())));
        }
        self.list.iter_mut().find(|(known, _, _)| known == id)
    }

    /// The object's `fields` (none: only that it is there).
    pub(in super::super) fn read(&mut self, id: &str, fields: &[&str]) {
        if let Some(held) = self.entry(id).and_then(|entry| entry.1.as_mut()) {
            held.extend(fields.iter().map(|field| (*field).to_owned()));
        }
    }

    /// Every field of the object.
    pub(in super::super) fn all(&mut self, id: &str) {
        if let Some(entry) = self.entry(id) {
            entry.1 = None;
        }
    }

    /// The object's position `name` (`address`).
    pub(in super::super) fn at(&mut self, id: &str, name: &str) {
        if let Some(held) = self.entry(id).and_then(|entry| entry.2.as_mut()) {
            held.insert(name.to_owned());
        }
    }

    /// Every position of the object (where it is in its list matters).
    pub(in super::super) fn everywhere(&mut self, id: &str) {
        if let Some(entry) = self.entry(id) {
            entry.2 = None;
        }
    }

    pub(in super::super) fn has(&self, id: &str) -> bool {
        self.list.iter().any(|(known, _, _)| known == id)
    }

    /// The holds as premises, after (and so in place of) any earlier premise of the same object.
    pub(in super::super) fn emit(self, found: &mut kition_kit::Under) {
        for (id, fields, positions) in self.list {
            let mut premise = kition_kit::Premise::on(id);
            if let Some(fields) = fields {
                premise = premise.fields(fields);
            }
            if let Some(positions) = positions {
                premise = premise.positions(positions);
            }
            found.premises.push(premise);
        }
    }
}

/// The tree a call edits (the current slide, the slide `onSlide` or a tool id names, or the
/// master or layout in Slide Master view) and how to name its objects.
pub(in super::super) struct Scope<'a> {
    pub state: &'a DocState,
    pub selection: Selection,
    pub shapes: &'a [Shape],
    pub ids: &'a kdm_model::Ids,
    /// The tree's own object (`slide:N`, the layout's or master's object id).
    pub tree: String,
    /// The slide, when the tree is one.
    pub slide: Option<&'a Slide>,
    /// `onSlide` named the slide by its number: where it is matters.
    pub numbered: bool,
    /// The parameters with tool ids taken out (local shape numbers).
    pub local: Value,
    owner: Option<kition_kit::kdm::ObjectId>,
}

impl<'a> Scope<'a> {
    pub(in super::super) fn of(doc: &'a SlideDocument, params: &Value) -> Result<Option<Self>, String> {
        let state = doc.state()?;
        let mut local = params.clone();
        let implied = doc.session.take_tool_ids(&mut local).map_err(command_failure)?;
        let explicit = local.get(ON_SLIDE).and_then(Value::as_u64).and_then(|n| usize::try_from(n).ok()).and_then(|n| n.checked_sub(1));
        let mut selection = state.selection.clone();
        if let Some(on) = implied.or(explicit) {
            selection.slide = on;
            selection.target = Target::Slides;
        }
        let Some(session) = state.kdm() else { return Ok(None) };
        let ids = session.model().ids();
        let deck = &state.doc;
        let (shapes, tree, owner, slide) = match selection.target {
            Target::Slides => {
                let Some(slide) = deck.slides.get(selection.slide) else { return Ok(None) };
                (slide.shapes.as_slice(), format!("slide:{}", slide.id.0), None, Some(&**slide))
            }
            Target::Master { master } => {
                let Some(master) = deck.masters.get(master) else { return Ok(None) };
                let Some(object) = ids.masters.object(master.id) else { return Ok(None) };
                (master.shapes.as_slice(), object.as_str().to_owned(), Some(object.clone()), None)
            }
            Target::Layout { master, layout } => {
                let Some(layout) = deck.masters.get(master).and_then(|master| master.layouts.get(layout)) else { return Ok(None) };
                let Some(object) = ids.layouts.object(layout.id) else { return Ok(None) };
                (layout.shapes.as_slice(), object.as_str().to_owned(), Some(object.clone()), None)
            }
        };
        Ok(Some(Self { state, selection, shapes, ids, tree, slide, numbered: explicit.is_some() && implied.is_none(), local, owner }))
    }

    pub(in super::super) fn deck(&self) -> &'a SlideFile {
        &self.state.doc
    }

    /// A shape of the tree, by the id the guard knows it by.
    pub(in super::super) fn public(&self, shape: &Shape) -> Option<String> {
        match &self.owner {
            None => Some(cmd::inspect::shape_tool_id(shape)),
            Some(owner) => self.ids.shape(owner, shape.id).map(|id| id.as_str().to_owned()),
        }
    }

    pub(in super::super) fn ctx(&self) -> Option<Ctx<'a>> {
        let deck = self.deck();
        match self.selection.target {
            Target::Slides => Ctx::for_slide(deck, self.slide?),
            Target::Master { master } => Some(Ctx::for_master(deck, deck.masters.get(master)?)),
            Target::Layout { master, layout } => {
                let master = deck.masters.get(master)?;
                Some(Ctx::for_layout(deck, master, master.layouts.get(layout)?))
            }
        }
    }

    fn master(&self, ctx: &Ctx<'_>) -> Option<String> {
        self.ids.masters.object(ctx.master.id).map(|id| id.as_str().to_owned())
    }

    fn layout_object(&self, ctx: &Ctx<'_>) -> Option<kition_kit::kdm::ObjectId> {
        ctx.layout.and_then(|layout| self.ids.layouts.object(layout.id)).cloned()
    }

    /// The shapes a call names (`ids`, else `id`, else the selection), each with whether it was
    /// named by its number on the slide (an address, whose place matters).
    pub(in super::super) fn targets(&self, params: &Value) -> Vec<(ShapeId, bool)> {
        let given = |params: &Value| -> Option<Vec<Value>> {
            match (params.get("ids"), params.get("id")) {
                (Some(Value::Array(list)), _) if !list.is_empty() => Some(list.clone()),
                (_, Some(value)) if !value.is_null() => Some(vec![value.clone()]),
                _ => None,
            }
        };
        match (given(&self.local), given(params)) {
            (Some(local), Some(original)) => local
                .iter()
                .enumerate()
                .filter_map(|(at, value)| {
                    let addressed =
                        original.get(at).is_some_and(|value| value.is_u64() || value.as_str().is_some_and(|text| text.parse::<u32>().is_ok()));
                    number(value).map(|id| (id, addressed))
                })
                .collect(),
            _ => self.selection.shapes.iter().map(|id| (*id, false)).collect(),
        }
    }

    pub(in super::super) fn find(&self, id: ShapeId) -> Option<&'a Shape> {
        kition_slide_model::find_shape(self.shapes, id)
    }

    /// The target `id` (when the tree has it): its fields, and its address when named by number.
    fn target(&self, holds: &mut Holds, id: ShapeId, addressed: bool, fields: &[&str]) -> Option<(&'a Shape, String)> {
        let shape = self.find(id)?;
        let object = self.public(shape)?;
        holds.read(&object, fields);
        if addressed {
            holds.at(&object, "address");
        }
        Some((shape, object))
    }

    /// The box a shape without its own inherits (`resolve::xfrm`): its placeholder kind, the
    /// placeholders the lookup compares (`resolve::find_ph`, `master_ph`), the matched ones'
    /// boxes, else the slide size.
    pub(in super::super) fn inherited_box(&self, holds: &mut Holds, shape: &Shape) {
        if let Some(object) = self.public(shape) {
            holds.read(&object, &["look.ph"]);
        }
        let Some(ctx) = self.ctx() else { return };
        if shape.ph.is_none() {
            holds.read("slide-file", &["props.slideSize"]);
            return;
        }
        if let Some(slide) = self.slide.filter(|_| matches!(self.selection.target, Target::Slides)) {
            holds.read(&format!("slide:{}", slide.id.0), &["layout"]);
        }
        let (layout, master) = self.parents_in(holds, &ctx, shape, matches!(self.selection.target, Target::Slides), false);
        for parent in [layout, master].into_iter().flatten() {
            holds.read(&parent.1, &["look.ph", "xfrm"]);
            if parent.0.xfrm.is_some() {
                return;
            }
        }
        holds.read("slide-file", &["props.slideSize"]);
    }

    /// The layout and master placeholders `shape` inherits from, with their objects, holding
    /// what the lookup reads: the slide's layout, and the placeholder kind of each placeholder
    /// compared before the match (all of them when none matches).
    fn parents(&self, holds: &mut Holds, ctx: &Ctx<'a>, shape: &Shape) -> (Option<(&'a Shape, String)>, Option<(&'a Shape, String)>) {
        if shape.ph.is_none() {
            return (None, None);
        }
        if let Some(slide) = self.slide.filter(|_| matches!(self.selection.target, Target::Slides)) {
            holds.read(&format!("slide:{}", slide.id.0), &["layout"]);
        }
        self.parents_in(holds, ctx, shape, matches!(self.selection.target, Target::Slides), true)
    }

    /// [`Self::parents`] in `ctx`, for a shape on a slide (`on_slide`) or a layout.
    /// The master placeholder only `with_master`: what reads it when the layout's has no box.
    fn parents_in(
        &self,
        holds: &mut Holds,
        ctx: &Ctx<'a>,
        shape: &Shape,
        on_slide: bool,
        with_master: bool,
    ) -> (Option<(&'a Shape, String)>, Option<(&'a Shape, String)>) {
        let Some(ph) = shape.ph.as_ref() else { return (None, None) };
        let (lp, mp) = resolve::parents(ctx, shape);
        let layout_owner = self.layout_object(ctx);
        let mut out = (None, None);
        if let (Some(layout), Some(owner)) = (ctx.layout.filter(|_| on_slide), layout_owner.as_ref()) {
            let matched = lp.and_then(|parent| layout.shapes.iter().position(|candidate| std::ptr::eq(candidate, parent)));
            // `find_ph`: by index first (when the shape has one), then by kind; what failed
            // compared every placeholder.
            let compared = match (matched, lp) {
                (Some(at), Some(parent)) => {
                    let found_early = match parent.ph.as_ref() {
                        Some(p) if ph.idx != 0 => p.idx == ph.idx,
                        Some(p) => p.kind == ph.kind,
                        None => false,
                    };
                    if found_early { at.saturating_add(1) } else { layout.shapes.len() }
                }
                _ => layout.shapes.len(),
            };
            for candidate in layout.shapes.iter().take(compared) {
                if let Some(object) = self.ids.shape(owner, candidate.id) {
                    holds.read(object.as_str(), &["look.ph"]);
                }
            }
            out.0 = lp.and_then(|parent| Some((parent, self.ids.shape(owner, parent.id)?.as_str().to_owned())));
        }
        let with_master = with_master || out.0.as_ref().is_none_or(|(parent, _): &(&Shape, String)| parent.xfrm.is_none());
        if with_master && let Some(owner) = self.ids.masters.object(ctx.master.id) {
            let matched = mp.and_then(|parent| ctx.master.shapes.iter().position(|candidate| std::ptr::eq(candidate, parent)));
            let compared = matched.map_or(ctx.master.shapes.len(), |at| at.saturating_add(1));
            for candidate in ctx.master.shapes.iter().take(compared) {
                if let Some(object) = self.ids.shape(owner, candidate.id) {
                    holds.read(object.as_str(), &["look.ph"]);
                }
            }
            out.1 = mp.and_then(|parent| Some((parent, self.ids.shape(owner, parent.id)?.as_str().to_owned())));
        }
        out
    }

    /// The text fit after the box, geometry or text of `shape` changed: shrink-to-fit text
    /// (`autofit::invalidate`) and, unless `shrink_only`, a text box fitted to its text.
    /// `inherited_body` selects the reviewed ancestor text scope; own text remains whole.
    pub(in super::super) fn refit(&self, holds: &mut Holds, shape: &Shape, shrink_only: bool, inherited_body: &str) {
        let Some(ctx) = self.ctx() else { return };
        let Some(object) = self.public(shape) else { return };
        match resolve::body(&ctx, shape).autofit {
            Some(AutoFit::Shrink { .. }) => {
                holds.read(&object, &["body", "look.geom"]);
                if shape.xfrm.is_some() {
                    holds.read(&object, &["xfrm"]);
                }
                if let Some(slide) = self.slide.filter(|_| matches!(self.selection.target, Target::Slides)) {
                    holds.read(&format!("slide:{}", slide.id.0), &["layout"]);
                }
                self.shrink_inputs(holds, &ctx, shape, matches!(self.selection.target, Target::Slides), inherited_body);
            }
            Some(AutoFit::Shape) if !shrink_only => {
                holds.read(&object, &["body", "xfrm"]);
                self.text_fit(holds, &ctx);
            }
            _ => {}
        }
    }

    /// What shrink-to-fit text inherits (`autofit::invalidate`): the layout placeholder's body properties/list styles,
    /// kind, style (and box, for a shape without one), the master placeholder's body properties/list styles and kind,
    /// the master's list style and theme fonts, the deck's default text style.
    fn shrink_inputs(&self, holds: &mut Holds, ctx: &Ctx<'a>, shape: &Shape, on_slide: bool, inherited_body: &str) {
        let (layout, master_parent) = self.parents_in(holds, ctx, shape, on_slide, true);
        if let Some((_, layout)) = layout {
            holds.read(&layout, &[inherited_body, "look.ph", "look.style"]);
            if shape.xfrm.is_none() {
                holds.read(&layout, &["xfrm"]);
            }
        }
        if let Some((_, parent)) = master_parent {
            holds.read(&parent, &[inherited_body, "look.ph"]);
        }
        if let Some(master) = self.master(ctx) {
            holds.read(&master, &["theme.fonts"]);
            if let Some(style) = list_style(shape) {
                holds.read(&master, &[style]);
            }
        }
        holds.read("slide-file", &["defaultTextStyle"]);
    }

    /// Slides pasted from the clipboard: the layout each names (kept when it is there), and the
    /// inherited text of their shrink-to-fit placeholders, refit in their new place.
    pub(in super::super) fn pasted_slides(&self, holds: &mut Holds, slides: &[Slide]) {
        let deck = self.deck();
        for slide in slides {
            if let Some(layout) = self.ids.layouts.object(slide.layout) {
                holds.read(layout.as_str(), &[]);
            }
            let Some(ctx) = Ctx::for_slide(deck, slide) else { continue };
            kition_slide_model::walk(&slide.shapes, &mut |shape, _| {
                if shape.ph.is_some() && matches!(resolve::body(&ctx, shape).autofit, Some(AutoFit::Shrink { .. })) {
                    self.shrink_inputs(holds, &ctx, shape, true, "body.props");
                }
            });
        }
    }

    /// A text box fitted to its text (`fit_text_box`): the master the slide's layout names (its
    /// theme fonts) and the deck's default text style.
    fn text_fit(&self, holds: &mut Holds, ctx: &Ctx<'_>) {
        if let Some(slide) = self.slide.filter(|_| matches!(self.selection.target, Target::Slides)) {
            holds.read(&format!("slide:{}", slide.id.0), &["layout"]);
        }
        if let Some(master) = self.master(ctx) {
            holds.read(&master, &["theme.fonts"]);
        }
        holds.read("slide-file", &["defaultTextStyle"]);
    }

    /// The connectors glued to `moved` (none of them in `gone`) rerouted: the connector's glue,
    /// geometry and box, and each end that stays its geometry and box; with no end left, the glue
    /// is dropped (the glue and box only).
    pub(in super::super) fn reroute(&self, holds: &mut Holds, moved: &[ShapeId], gone: &[ShapeId]) {
        kition_slide_model::walk(self.shapes, &mut |connector, _| {
            let ShapeKind::Connector { start, end } = &connector.kind else { return };
            if gone.contains(&connector.id) {
                return;
            }
            let ends: Vec<ShapeId> = [start, end].into_iter().flatten().map(|(id, _)| *id).collect();
            if !ends.iter().any(|id| moved.contains(id)) {
                return;
            }
            let Some(object) = self.public(connector) else { return };
            let staying: Vec<ShapeId> = ends.iter().copied().filter(|id| !gone.contains(id)).collect();
            if staying.is_empty() {
                holds.read(&object, &["look.kind", "xfrm"]);
                return;
            }
            holds.read(&object, &["look.geom", "look.kind", "xfrm"]);
            for id in staying {
                if let Some(shape) = self.find(id)
                    && let Some(end) = self.public(shape)
                {
                    holds.read(&end, &["look.geom", "xfrm"]);
                    if shape.xfrm.is_none() {
                        self.inherited_box(holds, shape);
                    }
                }
            }
        });
    }

    /// A group member's box changed: each group holding it is refit to its members' union.
    pub(in super::super) fn group_refit(&self, holds: &mut Holds, target: ShapeId) {
        for (group, inside) in ancestors(self.shapes, target) {
            let Some(object) = self.public(group) else { continue };
            holds.read(&object, &["children", "look.kind", "xfrm"]);
            for member in group.children().iter().filter(|member| member.id != inside) {
                if let Some(member) = self.public(member) {
                    holds.read(&member, &["xfrm"]);
                }
            }
        }
    }

    /// Shapes removed (`edit.delete`'s shape path): each whole with what it holds, the groups it
    /// leaves empty (removed) or not (refit from the members left), the slide's animations
    /// (filtered), the connectors glued to them.
    pub(in super::super) fn remove(&self, holds: &mut Holds, targets: &[(ShapeId, bool)]) {
        let mut gone: Vec<ShapeId> = Vec::new();
        for (id, addressed) in targets {
            let Some((shape, _)) = self.target(holds, *id, *addressed, &[]) else { continue };
            if let Some(object) = self.public(shape) {
                holds.all(&object);
            }
            gone.push(shape.id);
            kition_slide_model::walk(shape.children(), &mut |inner, _| {
                gone.push(inner.id);
                if let Some(object) = self.public(inner) {
                    holds.all(&object);
                }
            });
        }
        // Groups left empty go too (innermost first).
        loop {
            let mut more = Vec::new();
            kition_slide_model::walk(self.shapes, &mut |shape, _| {
                if shape.is_group() && !gone.contains(&shape.id) && shape.children().iter().all(|child| gone.contains(&child.id)) {
                    more.push(shape.id);
                }
            });
            if more.is_empty() {
                break;
            }
            gone.extend(more);
        }
        for (id, _) in targets {
            for (group, _) in ancestors(self.shapes, *id) {
                let Some(object) = self.public(group) else { continue };
                if gone.contains(&group.id) {
                    holds.all(&object);
                    continue;
                }
                holds.read(&object, &["children", "look.kind", "xfrm"]);
                for member in group.children().iter().filter(|member| !gone.contains(&member.id)) {
                    if let Some(member) = self.public(member) {
                        holds.read(&member, &["xfrm"]);
                    }
                }
            }
        }
        if let Some(slide) = self.slide.filter(|_| matches!(self.selection.target, Target::Slides)) {
            holds.read(&format!("slide:{}", slide.id.0), &["animations"]);
        }
        self.reroute(holds, &gone, &gone);
    }

    /// Media added with `bytes` reuse the media holding the same bytes (`add_media`).
    pub(in super::super) fn same_media(&self, holds: &mut Holds, bytes: &[u8]) {
        if let Some(media) = self.deck().media.iter().find(|media| media.data.as_slice() == bytes)
            && let Some(object) = self.ids.media.object(media.id)
        {
            holds.read(object.as_str(), &["blob"]);
        }
    }

    /// The tree a new shape goes into: its shape count names it; where `onSlide` names the slide
    /// by number, also where it is.
    pub(in super::super) fn create(&self, holds: &mut Holds) {
        holds.read(&self.tree, &["~shapeCount"]);
    }

    /// The slide the call named by number keeps every position held.
    pub(in super::super) fn finish(&self, mut holds: Holds, found: &mut kition_kit::Under) {
        if self.numbered && holds.has(&self.tree) {
            holds.everywhere(&self.tree);
        }
        holds.emit(found);
    }

    pub(in super::super) fn fit_new_text(&self, holds: &mut Holds) {
        if let Some(ctx) = self.ctx() {
            self.text_fit(holds, &ctx);
        }
    }
}

fn list_style(shape: &Shape) -> Option<&'static str> {
    match shape.ph_type()? {
        PhType::Title | PhType::CtrTitle => Some("listStyles.title"),
        PhType::Body
        | PhType::Obj
        | PhType::SubTitle
        | PhType::Table
        | PhType::Chart
        | PhType::Diagram
        | PhType::Media
        | PhType::Picture
        | PhType::ClipArt => Some("listStyles.body"),
        _ => Some("listStyles.other"),
    }
}

pub(in super::super) fn number(value: &Value) -> Option<ShapeId> {
    value.as_u64().or_else(|| value.as_str().and_then(|value| value.parse().ok())).and_then(|id| u32::try_from(id).ok()).map(ShapeId)
}

/// The groups holding `target`, innermost first, each with its member on the way to it.
fn ancestors(shapes: &[Shape], target: ShapeId) -> Vec<(&Shape, ShapeId)> {
    fn rec<'s>(shapes: &'s [Shape], target: ShapeId, out: &mut Vec<(&'s Shape, ShapeId)>, depth: usize) -> bool {
        if depth > 64 {
            return false;
        }
        for shape in shapes {
            if shape.id == target {
                return true;
            }
            if rec(shape.children(), target, out, depth + 1) {
                let inside = out.last().map_or(target, |(group, _)| group.id);
                out.push((shape, inside));
                return true;
            }
        }
        false
    }
    let mut out = Vec::new();
    rec(shapes, target, &mut out, 0);
    out
}

pub(in super::super) fn add(doc: &SlideDocument, command: &str, params: &Value, found: &mut kition_kit::Under) -> Result<(), String> {
    let Some(scope) = Scope::of(doc, params)? else { return Ok(()) };
    let mut holds = Holds::default();
    let given = |name: &str| params.get(name).is_some_and(|value| !value.is_null());
    match command {
        "shape.setDefault" => {}
        "shape.move" if given("toSlide") => move_to_slide(&scope, &mut holds, params),
        "shape.move" | "shape.resize" | "shape.setBounds" | "shape.rotate" | "shape.flip" | "shape.adjust" | "shape.change" => {
            let targets = scope.targets(params);
            let boxed = !matches!(command, "shape.adjust" | "shape.change");
            let mut moved = Vec::new();
            for (id, addressed) in &targets {
                let fields: &[&str] = if boxed { &["xfrm"] } else { &["look.geom"] };
                let Some((shape, object)) = scope.target(&mut holds, *id, *addressed, fields) else { continue };
                moved.push(shape.id);
                if shape.xfrm.is_none() {
                    holds.read(&object, &["xfrm"]);
                    scope.inherited_box(&mut holds, shape);
                }
                if boxed {
                    scope.group_refit(&mut holds, shape.id);
                }
                if !matches!(command, "shape.move" | "shape.rotate" | "shape.flip") {
                    // Preserve the geometry matrix's existing conservative ancestor bodies.
                    // Clipboard paths have separately reviewed body.props-only expectations.
                    scope.refit(&mut holds, shape, true, "body");
                }
            }
            scope.reroute(&mut holds, &moved, &[]);
        }
        "shape.delete" => scope.remove(&mut holds, &scope.targets(params)),
        "shape.fill" | "shape.line" | "shape.effects" | "shape.quickStyle" => {
            let fields: &[&str] = match command {
                "shape.fill" => &["look.fill"],
                "shape.line" => &["look.line"],
                "shape.effects" => &["look.effects"],
                _ => &["body", "look.effects", "look.fill", "look.line", "look.style"],
            };
            let alpha_only = command == "shape.fill"
                && given("transparency")
                && !params.get("none").and_then(Value::as_bool).unwrap_or(false)
                && !params.get("reset").and_then(Value::as_bool).unwrap_or(false)
                && !["color", "gradient", "pattern", "picture"].iter().any(|name| given(name));
            for (id, addressed) in scope.targets(params) {
                let Some((shape, _)) = scope.target(&mut holds, id, addressed, &[]) else { continue };
                members(&scope, &mut holds, shape, fields, alpha_only, 0);
            }
            if command == "shape.fill"
                && let Some(bytes) = params.get("picture").and_then(Value::as_str).and_then(cmd::base64_decode)
            {
                scope.same_media(&mut holds, &bytes);
            }
        }
        "shape.rename" | "shape.altText" | "shape.visible" | "shape.lock" => {
            let fields: &[&str] = match command {
                "shape.rename" => &["name"],
                "shape.altText" if given("decorative") => &["look.descr", "look.decorative"],
                "shape.altText" => &["look.descr"],
                "shape.visible" => &["look.hidden"],
                _ => &["look.locked"],
            };
            for (id, addressed) in scope.targets(params) {
                let Some((shape, object)) = scope.target(&mut holds, id, addressed, fields) else { continue };
                if shape.xfrm.is_none() {
                    holds.read(&object, &["xfrm"]);
                    scope.inherited_box(&mut holds, shape);
                }
            }
        }
        "shape.connect" => {
            for name in ["from", "to"] {
                let Some(value) = scope.local.get(name) else { continue };
                let addressed =
                    params.get(name).is_some_and(|value| value.is_u64() || value.as_str().is_some_and(|text| text.parse::<u32>().is_ok()));
                let Some(id) = number(value) else { continue };
                let mut fields = vec!["look.geom", "xfrm"];
                if scope.find(id).is_some_and(|shape| matches!(shape.kind, ShapeKind::Table(_))) {
                    fields.push("table");
                }
                let Some((shape, _)) = scope.target(&mut holds, id, addressed, &fields) else { continue };
                if shape.xfrm.is_none() {
                    scope.inherited_box(&mut holds, shape);
                }
            }
            match scope.targets(params).first().filter(|_| given("id")) {
                Some((id, addressed)) => {
                    scope.target(&mut holds, *id, *addressed, &["look.geom", "look.kind", "xfrm"]);
                }
                None => scope.create(&mut holds),
            }
        }
        "shape.insert" | "shape.freeform" | "shape.sites" => {
            if command != "shape.sites" {
                scope.create(&mut holds);
            }
            if command == "shape.insert" && !given("rect") {
                holds.read("slide-file", &["props.slideSize"]);
            }
        }
        "chart.type" | "chart.data" | "chart.options" => {
            let mut fields: Vec<&str> = Vec::new();
            let series_kind = ["chart.parts.kind", "chart.parts.series", "chart.parts.varyColors"];
            match command {
                "chart.type" => fields.extend(series_kind),
                "chart.data" => {
                    if given("categories") {
                        fields.push("chart.parts.categories");
                    }
                    if params.get("series").is_some() {
                        fields.push("chart.parts.series");
                    }
                }
                _ => {
                    for (name, member) in [
                        ("title", "chart.parts.title"),
                        ("legend", "chart.parts.legend"),
                        ("gapWidth", "chart.parts.gapWidth"),
                        ("overlap", "chart.parts.overlap"),
                        ("dataLabels", "chart.parts.dataLabels"),
                        ("gridlines", "chart.parts.gridlines"),
                        ("palette", "chart.parts.palette"),
                    ] {
                        if params.get(name).is_some() {
                            fields.push(member);
                        }
                    }
                    if given("colors") {
                        fields.extend(series_kind);
                    }
                }
            }
            if let Some((id, addressed)) =
                scope.targets(params).into_iter().find(|(id, _)| scope.find(*id).is_some_and(|shape| matches!(shape.kind, ShapeKind::Chart(_))))
            {
                scope.target(&mut holds, id, addressed, &fields);
            }
        }
        _ => insert(&scope, &mut holds, command, params),
    }
    scope.finish(holds, found);
    Ok(())
}

/// A format edited on a shape, on each member of a group instead (`edit_shapes` on the members):
/// the group's members list, each member's `fields`.
fn members(scope: &Scope<'_>, holds: &mut Holds, shape: &Shape, fields: &[&str], alpha_only: bool, depth: usize) {
    let Some(object) = scope.public(shape) else { return };
    if shape.is_group() && depth < 32 {
        holds.read(&object, &["children"]);
        for member in shape.children() {
            members(scope, holds, member, fields, alpha_only, depth + 1);
        }
        return;
    }
    holds.read(&object, fields);
    if shape.xfrm.is_none() {
        holds.read(&object, &["xfrm"]);
        scope.inherited_box(holds, shape);
    }
    if alpha_only {
        inherited_fill(scope, holds, shape, &object);
    }
}

/// Transparency on the fill a shape shows: its own, else its placeholders', else its style's
/// theme fill, resolved through the theme colours and colour map.
fn inherited_fill(scope: &Scope<'_>, holds: &mut Holds, shape: &Shape, object: &str) {
    holds.read(object, &["look.style"]);
    if shape.fill.is_some() {
        return;
    }
    let Some(ctx) = scope.ctx() else { return };
    let (layout, master) = scope.parents(holds, &ctx, shape);
    let mut colors = false;
    let mut sources: Vec<(&Shape, String)> = vec![(shape, object.to_owned())];
    sources.extend(layout);
    sources.extend(master);
    let master_object = scope.master(&ctx);
    if let Some(slide) = scope.slide.filter(|_| matches!(scope.selection.target, Target::Slides)) {
        holds.read(&format!("slide:{}", slide.id.0), &["layout"]);
    }
    for (source, id) in sources {
        holds.read(&id, &["look.fill", "look.style"]);
        if let Some(fill) = &source.fill {
            colors = matches!(fill, kition_slide_model::Fill::Solid { color } if matches!(color.base, kition_slide_model::ColorBase::Scheme { .. }));
            break;
        }
        if source.style.is_some() {
            colors = true;
            if let Some(master) = &master_object {
                holds.read(master, &["theme.format"]);
            }
            break;
        }
    }
    if colors && let Some(master) = &master_object {
        holds.read(master, &["theme.colors", "colorMap"]);
    }
}

/// A shape moved to another slide (`move_to_slide`): taken whole, with the slide's animations
/// and the comments anchored to it; renumbered where the destination's numbers collide.
fn move_to_slide(scope: &Scope<'_>, holds: &mut Holds, params: &Value) {
    let targets = scope.targets(params);
    for (id, addressed) in &targets {
        if let Some((_, object)) = scope.target(holds, *id, *addressed, &[]) {
            holds.all(&object);
            holds.at(&object, "address");
        }
    }
    if let Some(slide) = scope.slide {
        holds.read(&format!("slide:{}", slide.id.0), &["animations"]);
        for thread in slide.comments.iter().filter(|thread| thread.shape.is_some_and(|shape| targets.iter().any(|(id, _)| *id == shape))) {
            if let Some(object) = scope.ids.comments_object(&thread.id) {
                holds.read(object.as_str(), &["anchor"]);
            }
        }
    }
    let destination = params.get("toSlide").and_then(|value| {
        value
            .as_str()
            .and_then(|text| text.strip_prefix("slide:"))
            .and_then(|n| n.parse::<u32>().ok())
            .or_else(|| value.as_u64().and_then(|n| u32::try_from(n).ok()))
    });
    if let Some(slide) = destination.and_then(|id| scope.deck().slides.iter().find(|slide| slide.id.0 == id)) {
        holds.read(&format!("slide:{}", slide.id.0), &[]);
        kition_slide_model::walk(&slide.shapes, &mut |shape, _| {
            let object = cmd::inspect::shape_tool_id(shape);
            holds.read(&object, &[]);
            holds.at(&object, "address");
        });
    }
}

fn insert(scope: &Scope<'_>, holds: &mut Holds, command: &str, params: &Value) {
    let given = |name: &str| params.get(name).is_some_and(|value| !value.is_null());
    if let Some(bytes) = params.get("data").and_then(Value::as_str).and_then(cmd::base64_decode)
        && matches!(command, "insert.picture" | "insert.audio" | "insert.video")
    {
        scope.same_media(holds, &bytes);
    }
    if command == "insert.picture" {
        // A selected empty picture, content or body placeholder takes the picture.
        for selected in &scope.selection.shapes {
            let Some(shape) = scope.find(*selected) else { continue };
            let Some(object) = scope.public(shape) else { continue };
            holds.read(&object, &["look.ph"]);
            let takes = shape.ph_type().is_some_and(|kind| matches!(kind, PhType::Picture | PhType::Obj | PhType::Body));
            if !takes {
                continue;
            }
            holds.read(&object, &["body"]);
            if shape.text.as_ref().is_none_or(|body| body.is_empty()) {
                holds.read(&object, &["kind", "look.descr", "look.kind", "media", "xfrm"]);
                if shape.xfrm.is_none() {
                    scope.inherited_box(holds, shape);
                }
                return;
            }
        }
    }
    scope.create(holds);
    let framed = given("rect") && command != "insert.artText";
    if !framed {
        holds.read("slide-file", &["props.slideSize"]);
    }
    if matches!(command, "insert.textBox" | "insert.artText") {
        scope.fit_new_text(holds);
    }
}
