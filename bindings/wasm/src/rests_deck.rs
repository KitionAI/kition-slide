//! What the deck-level writes rest on (C8-slide-read-3): file and history, slides, sections,
//! design, masters, transitions, animations, comments and the show, as the independent reading of
//! their handlers found it (`tests/declarations_independent/rests_{a,e}.rs`).
//!
//! [`settle`] runs last in `under` and states each object's premise whole: the fields the handler
//! reads or replaces, and its position only when the call names it by place (`index`, `at`,
//! `onSlide`, `slide`) or the handler reads where it is. An object it names only to act on (an
//! animation's or a comment's shape) is held to exist; an object the guard would otherwise hold
//! whole only because the call mentions it is narrowed to what is read.

use std::collections::BTreeSet;

use super::*;
use kition_slide_model::resolve::{self, Ctx};
use kition_slide_model::text::AutoFit;
use kition_slide_model::{Layout, LayoutType, Master, Shape, ShapeId, ShapeKind, Slide};

/// One object's premise as it is being gathered: fields and positions, `None` for every one.
struct Hold {
    id: String,
    fields: Option<BTreeSet<String>>,
    positions: Option<BTreeSet<String>>,
    identities: BTreeMap<String, String>,
}

#[derive(Default)]
struct Holds {
    items: Vec<Hold>,
    orders: Vec<kition_kit::RelativeOrder>,
}

impl Holds {
    fn entry(&mut self, id: &str, change: impl FnOnce(&mut Hold)) {
        if !self.items.iter().any(|hold| hold.id == id) {
            self.items.push(Hold { id: id.to_owned(), fields: Some(BTreeSet::new()), positions: Some(BTreeSet::new()), identities: BTreeMap::new() });
        }
        if let Some(hold) = self.items.iter_mut().find(|hold| hold.id == id) {
            change(hold);
        }
    }

    /// The object exists and the call reads `paths` of it (none: only that it is there).
    fn read(&mut self, id: &str, paths: &[&str]) {
        self.read_owned(id, paths.iter().map(|path| (*path).to_owned()));
    }

    fn read_owned(&mut self, id: &str, paths: impl IntoIterator<Item = String>) {
        self.entry(id, |hold| {
            if let Some(fields) = hold.fields.as_mut() {
                fields.extend(paths);
            }
        });
    }

    /// Every field of the object.
    fn whole(&mut self, id: &str) {
        self.entry(id, |hold| hold.fields = None);
    }

    /// Where the object is matters (it is named by place, or its place is read).
    fn placed(&mut self, id: &str) {
        self.entry(id, |hold| hold.positions = None);
    }

    fn position(&mut self, id: &str, name: &str) {
        self.entry(id, |hold| {
            if let Some(positions) = hold.positions.as_mut() {
                positions.insert(name.to_owned());
            }
        });
    }

    /// List membership and order, without holding each member's descendant content.
    fn order(&mut self, parent: &str, field: &str, objects: Vec<Value>) {
        if !objects.is_empty() {
            self.orders.push(kition_kit::RelativeOrder { parent: json!(parent), field: field.into(), objects });
        }
    }

    /// The premises, in place of every one gathered before for the command.
    fn apply(self, found: &mut kition_kit::Under) {
        found.premises.clear();
        found.orders.extend(self.orders);
        for hold in self.items {
            let mut premise = kition_kit::Premise::on(hold.id.as_str());
            if let Some(fields) = hold.fields {
                premise = premise.fields(fields);
            }
            if let Some(positions) = hold.positions {
                premise = premise.positions(positions);
            }
            premise.identities = hold.identities;
            found.premises.push(premise);
        }
    }
}

const ROOT: &str = "slide-file";

fn slide_id(slide: &Slide) -> String {
    format!("slide:{}", slide.id.0)
}

fn index(params: &Value, key: &str) -> Option<usize> {
    params.get(key).and_then(Value::as_u64).and_then(|n| usize::try_from(n).ok())
}

fn given(params: &Value, key: &str) -> bool {
    params.get(key).is_some_and(|value| !value.is_null())
}

/// The slide `onSlide` names (from 1), if the call names one.
fn on_slide(params: &Value) -> Option<usize> {
    index(params, ON_SLIDE).and_then(|n| n.checked_sub(1))
}

/// The slides a slide command acts on: `index`; else (when `selected`) the selected slides; else
/// the current slide, which `onSlide` sets. Whether they are named by place.
fn slides_of(state: &DocState, params: &Value, selected: bool) -> (Vec<usize>, bool) {
    if let Some(at) = index(params, "index") {
        return (vec![at], true);
    }
    if let Some(at) = on_slide(params) {
        return (vec![at], true);
    }
    let mut chosen: Vec<usize> =
        if selected { state.selection.slides.iter().filter_map(|id| state.doc.slide_index(*id)).collect() } else { Vec::new() };
    chosen.sort_unstable();
    if chosen.is_empty() {
        chosen.push(state.selection.slide);
    }
    (chosen, false)
}

/// The section at `at` by its tool id (`section:<identity>`); `None` without an identity.
fn section_id(deck: &SlideFile, at: usize) -> Option<String> {
    deck.sections.get(at).filter(|section| !section.identity.is_empty()).map(|section| format!("section:{}", section.identity))
}

/// `fields` of the section at `at` (its `name`, its member `slides`); the slide file's section
/// list when it has no identity.
fn section(deck: &SlideFile, at: usize, fields: &[&str], holds: &mut Holds) {
    match section_id(deck, at) {
        Some(id) => holds.read(&id, fields),
        None => holds.read(ROOT, &["sections"]),
    }
}

/// Where the section at `at` starts: its first member (its entry `slides.<slide tool id>`) and
/// with `placed`, where the section is (an index locates it); all its members when it has none.
fn section_start(deck: &SlideFile, at: usize, placed: bool, holds: &mut Holds) {
    let Some(id) = section_id(deck, at) else { return holds.read(ROOT, &["sections"]) };
    match deck.sections.get(at).and_then(|section| section.slides.first()) {
        Some(first) => holds.read_owned(&id, [format!("slides.slide:{}", first.0)]),
        None => holds.read(&id, &["slides"]),
    }
    if placed {
        holds.placed(&id);
    }
}

/// The section at `at`, named by its place: every field of it and where it is.
fn section_at(deck: &SlideFile, at: usize, holds: &mut Holds) {
    match section_id(deck, at) {
        Some(id) => {
            holds.whole(&id);
            holds.placed(&id);
        }
        None => holds.read(ROOT, &["sections"]),
    }
}

/// Every section's members, or the section list when there are none: where slides are placed
/// and taken (the names are not read).
fn memberships(deck: &SlideFile, holds: &mut Holds) {
    if deck.sections.is_empty() {
        holds.read(ROOT, &["sections"]);
    } else {
        for at in 0..deck.sections.len() {
            section(deck, at, &["slides"], holds);
        }
    }
}

/// A slide that goes or is copied whole: every field of it, its shapes and its comment threads.
fn whole_slide(slide: &Slide, placed: bool, holds: &mut Holds) {
    let id = slide_id(slide);
    holds.whole(&id);
    if placed {
        holds.placed(&id);
    }
    kition_slide_model::walk(&slide.shapes, &mut |shape, _| holds.whole(&cmd::inspect::shape_tool_id(shape)));
    for thread in &slide.comments {
        holds.whole(&thread.id);
    }
}

/// The media a slide's shapes draw from (a picture's image, a clip and its poster).
fn media_of(state: &DocState, slide: &Slide, holds: &mut Holds) {
    let Some(session) = state.kdm() else { return };
    let ids = session.model().ids();
    let mut media = Vec::new();
    kition_slide_model::walk(&slide.shapes, &mut |shape, _| match &shape.kind {
        ShapeKind::Picture { fill } => media.push(fill.media),
        ShapeKind::Media(clip) => {
            media.push(clip.media);
            media.extend(clip.poster);
        }
        _ => {}
    });
    for item in media {
        if let Some(object) = ids.media.object(item) {
            holds.read(object.as_str(), &[]);
        }
    }
}

type Object = kition_kit::kdm::ObjectId;

fn master_object(state: &DocState, master: &Master) -> Option<Object> {
    state.kdm().and_then(|session| session.model().ids().masters.object(master.id).cloned())
}

fn layout_object(state: &DocState, layout: &Layout) -> Option<Object> {
    state.kdm().and_then(|session| session.model().ids().layouts.object(layout.id).cloned())
}

/// A layout's or master's shape, by the object that holds it.
fn part_shape(state: &DocState, owner: Option<&Object>, shape: &Shape) -> Option<String> {
    state.kdm()?.model().ids().shape(owner?, shape.id).map(|id| id.as_str().to_owned())
}

/// The master placeholder a layout placeholder takes its text from, and the master's text styles:
/// what a placeholder copied from `layout` is fitted with.
fn master_parents(state: &DocState, master: &Master, layout: &Layout, holds: &mut Holds) {
    let master_id = master_object(state, master);
    let ctx = Ctx::for_layout(&state.doc, master, layout);
    let mut text = false;
    for shape in layout.shapes.iter().filter(|shape| shape.ph.as_ref().is_some_and(|ph| !ph.kind.is_footer_kind())) {
        text = true;
        if let (_, Some(parent)) = resolve::parents(&ctx, shape)
            && let Some(id) = part_shape(state, master_id.as_ref(), parent)
        {
            holds.read(&id, &["body.props"]);
        }
    }
    if text {
        if let Some(id) = &master_id {
            holds.read(id.as_str(), &["listStyles", "theme.fonts"]);
        }
        holds.read(ROOT, &["defaultTextStyle"]);
    }
}

/// A layout a slide is made from or put on: its shape list, its placeholders (copied: name, kind,
/// and what they are fitted with), their master placeholders and the master's text styles.
fn made_from(state: &DocState, master: &Master, layout: &Layout, holds: &mut Holds) {
    let Some(object) = layout_object(state, layout) else { return };
    holds.read(object.as_str(), &["~shapeCount"]);
    holds.order(
        object.as_str(),
        "shapes",
        layout.shapes.iter().filter_map(|shape| part_shape(state, Some(&object), shape).map(Value::String)).collect(),
    );
    for shape in &layout.shapes {
        let Some(id) = part_shape(state, Some(&object), shape) else { continue };
        // The predicate is read even when the shape is no longer a placeholder.
        holds.read(&id, &["look.ph"]);
        let Some(ph) = shape.ph.as_ref() else { continue };
        if ph.kind.is_footer_kind() {
            holds.read(&id, &["look.ph"]);
        } else {
            holds.read(&id, &["body.props", "look.ph", "look.style", "name", "xfrm"]);
        }
    }
    master_parents(state, master, layout, holds);
}

fn normalized(name: &str) -> String {
    name.chars().filter(|c| c.is_alphanumeric()).collect::<String>().to_ascii_lowercase()
}

/// What [`cmd::slide::find_layout`] reads of the master's layouts: the names up to the one whose
/// name matches, else every name and the kinds up to the one of the kind.
fn looked_up(state: &DocState, key: &Value, master: usize, found: Option<kition_slide_model::LayoutId>, holds: &mut Holds) {
    if key.as_u64().is_some_and(|n| u32::try_from(n).ok().is_some_and(|n| state.doc.layout(kition_slide_model::LayoutId(n)).is_some())) {
        return;
    }
    let Some(text) = key.as_str() else { return };
    if text.starts_with("layout:") && found.is_some() {
        return;
    }
    let Some(m) = state.doc.masters.get(master).or(state.doc.masters.first()) else { return };
    let wanted = normalized(text);
    let by_name = m.layouts.iter().position(|layout| normalized(&layout.name) == wanted);
    let by_kind = m.layouts.iter().position(|layout| Some(layout.id) == found);
    for (at, layout) in m.layouts.iter().enumerate() {
        let Some(object) = layout_object(state, layout) else { continue };
        match by_name {
            Some(matched) if at <= matched => holds.read(object.as_str(), &["props.name"]),
            Some(_) => {}
            None => {
                holds.read(object.as_str(), &["props.name"]);
                if by_kind.is_none_or(|matched| at <= matched) {
                    holds.read(object.as_str(), &["props.kind"]);
                }
            }
        }
    }
}

/// What [`kition_slide_model::defaults::layout_of_kind`] reads: the first master's layout kinds up
/// to the first of `kind`.
fn by_kind(state: &DocState, kind: LayoutType, holds: &mut Holds) {
    let Some(m) = state.doc.masters.first() else { return };
    for layout in &m.layouts {
        if let Some(object) = layout_object(state, layout) {
            holds.read(object.as_str(), &["props.kind"]);
        }
        if layout.kind == kind {
            break;
        }
    }
}

fn master_of(deck: &SlideFile, slide: Option<&std::sync::Arc<Slide>>) -> usize {
    slide.and_then(|slide| deck.masters.iter().position(|master| master.layout(slide.layout).is_some())).unwrap_or(0)
}

/// The master a design command edits: the one Slide Master view shows, else the current slide's
/// (whose layout says which).
fn design_master(state: &DocState, holds: &mut Holds) -> Option<usize> {
    match state.selection.target {
        Target::Master { master } | Target::Layout { master, .. } => Some(master),
        Target::Slides => {
            let slide = state.doc.slides.get(state.selection.slide);
            if let Some(slide) = slide {
                holds.read(&slide_id(slide), &["layout"]);
            }
            Some(master_of(&state.doc, slide))
        }
    }
}

/// Whether autofit refits the placeholder from its text and frame (shrink on overflow).
fn shrinks(ctx: &Ctx<'_>, shape: &Shape) -> bool {
    shape.ph.is_some() && matches!(resolve::body(ctx, shape).autofit, Some(AutoFit::Shrink { .. }))
}

/// The placeholders a refit after `master` changed recomputes: their text, and their own frame.
fn refits(state: &DocState, master: usize, holds: &mut Holds) {
    let deck = &state.doc;
    let Some(m) = deck.masters.get(master) else { return };
    for slide in deck.slides.iter().filter(|slide| m.layout(slide.layout).is_some()) {
        let Some(ctx) = Ctx::for_slide(deck, slide) else { continue };
        for shape in slide.shapes.iter().filter(|shape| shrinks(&ctx, shape)) {
            let id = cmd::inspect::shape_tool_id(shape);
            holds.read(&id, &["body"]);
            if shape.xfrm.is_some() {
                holds.read(&id, &["xfrm"]);
            }
        }
    }
    let master_id = master_object(state, m);
    let ctx = Ctx::for_master(deck, m);
    for shape in m.shapes.iter().filter(|shape| shrinks(&ctx, shape)) {
        if let Some(id) = part_shape(state, master_id.as_ref(), shape) {
            holds.read(&id, if shape.xfrm.is_some() { &["body", "xfrm"] } else { &["body"] });
        }
    }
    for layout in &m.layouts {
        let object = layout_object(state, layout);
        let ctx = Ctx::for_layout(deck, m, layout);
        for shape in layout.shapes.iter().filter(|shape| shrinks(&ctx, shape)) {
            if let Some(id) = part_shape(state, object.as_ref(), shape) {
                holds.read(&id, if shape.xfrm.is_some() { &["body", "xfrm"] } else { &["body"] });
            }
        }
    }
}

/// The shapes a command acts on, as animation and comment commands name them: `ids`/`id` (tool
/// ids make their slide current; numbers are on the current slide or `onSlide`), else the
/// selection. The slide, whether it is named by place, and whether the shapes were named.
fn named_shapes(doc: &SlideDocument, state: &DocState, params: &Value, key: &[&str]) -> Result<(usize, bool, Vec<ShapeId>, bool), String> {
    let mut local = params.clone();
    let implied = doc.session.take_tool_ids(&mut local).map_err(command_failure)?;
    let explicit = on_slide(params);
    let slide = implied.or(explicit).unwrap_or(state.selection.slide);
    let number = |value: &Value| value.as_u64().or_else(|| value.as_str().and_then(|text| text.parse().ok())).and_then(|n| u32::try_from(n).ok());
    for key in key {
        if let Some(value) = local.get(*key).filter(|value| !value.is_null()) {
            let values = value.as_array().cloned().unwrap_or_else(|| vec![value.clone()]);
            return Ok((slide, explicit.is_some(), values.iter().filter_map(number).map(ShapeId).collect(), true));
        }
    }
    Ok((slide, explicit.is_some(), state.selection.shapes.clone(), false))
}

/// A shape named only to act on: it is there (and, named by its number on a slide, still that
/// number there).
fn acted_on(shape: &Shape, addressed: bool, holds: &mut Holds) {
    let id = cmd::inspect::shape_tool_id(shape);
    holds.read(&id, &[]);
    if addressed {
        holds.position(&id, "address");
    }
}

pub(super) fn settle(doc: &SlideDocument, command: &str, params: &Value, found: &mut kition_kit::Under) -> Result<(), String> {
    let Some(_) = doc.pres() else { return Ok(()) };
    let state = doc.state()?;
    let deck = &state.doc;
    let mut holds = Holds::default();
    match command {
        "file.new" | "file.open" | "file.openBytes" | "file.recovery.open" | "file.revert" => {
            holds.whole(ROOT);
            holds.placed(ROOT);
        }
        // Closing discards the session; it reads or replaces no saved content.
        "file.close" => {}
        // A step back or forward puts the deck as the step had it: everything since it is read.
        "edit.undo" | "edit.redo" => {
            let steps = if command == "edit.undo" { &state.history.undo } else { &state.history.redo };
            if steps.is_empty() {
                return Ok(());
            }
            holds.whole(ROOT);
            holds.placed(ROOT);
        }
        "file.properties" => {
            for key in ["title", "subject", "author", "keywords", "comments", "category", "company"] {
                if params.get(key).is_some_and(Value::is_string) {
                    holds.read_owned(ROOT, [format!("props.doc.{key}")]);
                }
            }
            if holds.items.is_empty() {
                return Ok(());
            }
        }
        "slide.new" => {
            let current = deck.slides.get(state.selection.slide);
            if let Some(slide) = current {
                holds.read(&slide_id(slide), &["layout"]);
            }
            let master = master_of(deck, current);
            let layout = match params.get("layout") {
                Some(key) => {
                    let chosen = cmd::slide::find_layout(deck, key, master);
                    looked_up(state, key, master, chosen, &mut holds);
                    chosen
                }
                None => {
                    let layout = current.and_then(|slide| deck.layout(slide.layout));
                    if let Some(object) = layout.and_then(|(_, layout)| layout_object(state, layout)) {
                        holds.read(object.as_str(), &["props.kind"]);
                    }
                    match layout {
                        Some((_, layout)) if layout.kind != LayoutType::Title => Some(layout.id),
                        _ => {
                            by_kind(state, LayoutType::TitleAndContent, &mut holds);
                            kition_slide_model::defaults::layout_of_kind(deck, LayoutType::TitleAndContent).or(current.map(|slide| slide.layout))
                        }
                    }
                }
            };
            if let Some((m, layout)) = layout.and_then(|layout| deck.layout(layout)) {
                if let Some(object) = master_object(state, m) {
                    holds.read(object.as_str(), &["~layoutCount"]);
                    holds.order(
                        object.as_str(),
                        "layouts",
                        m.layouts.iter().filter_map(|layout| layout_object(state, layout).map(|id| json!(id.as_str()))).collect(),
                    );
                }
                made_from(state, m, layout, &mut holds);
            }
            // The slide before an insertion place: its section is the new slide's.
            if let Some(at) = index(params, "at").map(|at| at.min(deck.slides.len()))
                && let Some(before) = at.checked_sub(1).and_then(|at| deck.slides.get(at))
            {
                holds.placed(&slide_id(before));
            }
            memberships(deck, &mut holds);
        }
        "slide.fromOutline" => {
            by_kind(state, LayoutType::TitleAndContent, &mut holds);
            if let Some((m, layout)) =
                kition_slide_model::defaults::layout_of_kind(deck, LayoutType::TitleAndContent).and_then(|layout| deck.layout(layout))
            {
                if let Some(object) = master_object(state, m) {
                    holds.read(object.as_str(), &["layouts"]);
                }
                made_from(state, m, layout, &mut holds);
            }
        }
        "slide.duplicate" => {
            let (at, placed) = slides_of(state, params, true);
            let mut copied = false;
            for slide in at.iter().filter_map(|at| deck.slides.get(*at)) {
                whole_slide(slide, placed && index(params, "index").is_some(), &mut holds);
                media_of(state, slide, &mut holds);
                // The copies' placeholders are fitted against what they inherit.
                let Some(ctx) = Ctx::for_slide(deck, slide) else { continue };
                let layout = ctx.layout.and_then(|layout| layout_object(state, layout));
                let master = master_object(state, ctx.master);
                for shape in slide.shapes.iter().filter(|shape| shape.ph.is_some()) {
                    copied = true;
                    let (lp, mp) = resolve::parents(&ctx, shape);
                    if let Some(id) = lp.and_then(|parent| part_shape(state, layout.as_ref(), parent)) {
                        holds.read(&id, &["body.props", "look.style", "xfrm"]);
                    }
                    if let Some(id) = mp.and_then(|parent| part_shape(state, master.as_ref(), parent)) {
                        holds.read(&id, &["body.props"]);
                    }
                }
                if copied {
                    if let Some(master) = &master {
                        holds.read(master.as_str(), &["listStyles", "theme.fonts"]);
                    }
                    holds.read(ROOT, &["defaultTextStyle"]);
                }
            }
            memberships(deck, &mut holds);
        }
        "slide.delete" => {
            let named: Vec<usize> = params
                .get("ids")
                .and_then(Value::as_array)
                .map(|ids| {
                    ids.iter()
                        .filter_map(|id| {
                            id.as_u64().or_else(|| id.as_str().and_then(|text| text.strip_prefix("slide:").unwrap_or(text).parse().ok()))
                        })
                        .filter_map(|id| deck.slides.iter().position(|slide| u64::from(slide.id.0) == id))
                        .collect()
                })
                .unwrap_or_default();
            // Slides named by id are those slides wherever they are.
            let (at, placed) = if params.get("ids").is_some() { (named, false) } else { slides_of(state, params, true) };
            for slide in at.iter().filter_map(|at| deck.slides.get(*at)) {
                whole_slide(slide, placed, &mut holds);
            }
            memberships(deck, &mut holds);
        }
        "slide.move" => {
            let from = index(params, "from").unwrap_or(state.selection.slide);
            if let Some(slide) = deck.slides.get(from) {
                holds.read(&slide_id(slide), &[]);
                if given(params, "from") {
                    holds.placed(&slide_id(slide));
                }
            }
            // The slide now at `to`: the moved one goes beside it, into its neighbour's section.
            if let Some(to) = index(params, "to").filter(|to| *to != from)
                && let Some(slide) = deck.slides.get(to)
            {
                holds.read(&slide_id(slide), &[]);
                holds.placed(&slide_id(slide));
            }
            memberships(deck, &mut holds);
        }
        "slide.hide" | "slide.notes" | "slide.rename" | "transition.set" | "transition.options" | "transition.timing" | "animation.clear" => {
            let path = match command {
                "slide.hide" => "props.hidden",
                "slide.notes" => "notesBody",
                "slide.rename" => "props.name",
                "animation.clear" => "animations",
                _ => "transition",
            };
            let (at, placed) =
                slides_of(state, params, matches!(command, "slide.hide" | "transition.set" | "transition.options" | "transition.timing"));
            for slide in at.iter().filter_map(|at| deck.slides.get(*at)) {
                holds.read(&slide_id(slide), &[path]);
                if placed {
                    holds.placed(&slide_id(slide));
                }
            }
        }
        "transition.applyAll" => {
            for slide in deck.slides.iter() {
                holds.read(&slide_id(slide), &["transition"]);
            }
            holds.read(ROOT, &["~slideOrder"]);
        }
        "slide.layout" | "slide.reset" => {
            let current = deck.slides.get(state.selection.slide);
            let master = master_of(deck, current);
            let (at, placed) = slides_of(state, params, command == "slide.layout");
            let mut layout = None;
            if command == "slide.layout" {
                if let Some(slide) = current {
                    holds.read(&slide_id(slide), &["layout"]);
                }
                if let Some(key) = params.get("layout") {
                    layout = cmd::slide::find_layout(deck, key, master);
                    looked_up(state, key, master, layout, &mut holds);
                }
            }
            for slide in at.iter().filter_map(|at| deck.slides.get(*at)) {
                let id = slide_id(slide);
                if command == "slide.reset" {
                    holds.read(&id, &["background", "layout", "shapes"]);
                    layout = Some(slide.layout);
                } else {
                    holds.read(&id, &["layout", "shapes"]);
                }
                if placed {
                    holds.placed(&id);
                }
                let paths: &[&str] = if command == "slide.reset" {
                    &["body", "look.effects", "look.fill", "look.geom", "look.line", "look.ph", "xfrm"]
                } else {
                    &["body", "look.ph", "xfrm"]
                };
                for shape in slide.shapes.iter().filter(|shape| shape.ph.is_some()) {
                    holds.read(&cmd::inspect::shape_tool_id(shape), paths);
                }
                if let Some((m, chosen)) = layout.and_then(|layout| deck.layout(layout)) {
                    if command == "slide.layout"
                        && let Some(object) = master_object(state, m)
                    {
                        holds.read(object.as_str(), &["layouts"]);
                    }
                    made_from(state, m, chosen, &mut holds);
                }
            }
        }
        "section.add" => {
            let at = index(params, "at").unwrap_or(state.selection.slide).min(deck.slides.len().saturating_sub(1));
            if deck.sections.is_empty() {
                holds.read(ROOT, &["sections"]);
            } else if let Some(slide) = deck.slides.get(at) {
                section_start(deck, deck.section_of(slide.id).unwrap_or(0), false, &mut holds);
            }
            // The boundary and its anchor locate the split; later slides keep their edits.
            if let Some(slide) = deck.slides.get(at) {
                holds.read(&slide_id(slide), &[]);
                if given(params, "at") {
                    holds.placed(&slide_id(slide));
                }
            }
        }
        "section.rename" => {
            if let Some(at) = index(params, "index") {
                section(deck, at, &["name"], &mut holds);
                if let Some(id) = section_id(deck, at) {
                    holds.placed(&id);
                }
            }
        }
        "section.remove" => {
            let Some(at) = index(params, "index") else { return Ok(()) };
            section_at(deck, at, &mut holds);
            if params.get("slides") == Some(&json!(true)) {
                if let Some(section) = deck.sections.get(at) {
                    for slide in deck.slides.iter().filter(|slide| section.slides.contains(&slide.id)) {
                        whole_slide(slide, false, &mut holds);
                    }
                }
            } else if let Some(before) = at.checked_sub(1) {
                section(deck, before, &["slides"], &mut holds);
            } else if deck.sections.len() > 1 {
                section(deck, at.saturating_add(1), &["slides"], &mut holds);
            }
        }
        "section.removeAll" => {
            holds.read(ROOT, &["sections"]);
            for at in 0..deck.sections.len() {
                if let Some(id) = section_id(deck, at) {
                    holds.whole(&id);
                }
            }
        }
        "section.move" => {
            let from = index(params, "index").unwrap_or(0);
            let to = index(params, "to").unwrap_or(0).min(deck.sections.len().saturating_sub(1));
            let gap = if to >= from { to.saturating_add(1) } else { to };
            if gap >= deck.sections.len() {
                holds.read(ROOT, &["sections"]);
            } else {
                section_at(deck, from, &mut holds);
                for at in gap.checked_sub(1).into_iter().chain(Some(gap)) {
                    section_start(deck, at, true, &mut holds);
                }
            }
        }
        "theme.set" | "theme.fonts" | "theme.colors" => {
            if command == "theme.colors" && !given(params, "name") && !given(params, "colors") {
                return Ok(());
            }
            let Some(master) = design_master(state, &mut holds) else { return Ok(()) };
            if let Some(object) = deck.masters.get(master).and_then(|m| master_object(state, m)) {
                match command {
                    "theme.set" => holds.read(object.as_str(), &["colorMap", "listStyles", "props.name", "theme"]),
                    "theme.fonts" => holds.read(object.as_str(), &["listStyles", "theme.fonts"]),
                    _ => holds.read(object.as_str(), &["theme.colors"]),
                }
            }
            if command != "theme.colors" {
                refits(state, master, &mut holds);
            }
        }
        "slide.background" => background(state, params, &mut holds),
        "slide.hideBackgroundGraphics" => {
            let at = on_slide(params).unwrap_or(state.selection.slide);
            if let Some(slide) = deck.slides.get(at) {
                holds.read(&slide_id(slide), &["props.showMasterShapes"]);
                if on_slide(params).is_some() {
                    holds.placed(&slide_id(slide));
                }
            }
        }
        "slide.size" => slide_size(state, params, &mut holds),
        "slide.headerFooter" => header_footer(state, params, &mut holds),
        "master.insertLayout" => {
            let Some(master) = design_master(state, &mut holds) else { return Ok(()) };
            let Some(m) = deck.masters.get(master) else { return Ok(()) };
            if let Target::Layout { layout, .. } = state.selection.target
                && let Some(object) = m.layouts.get(layout).and_then(|layout| layout_object(state, layout))
            {
                holds.read(object.as_str(), &[]);
                holds.placed(object.as_str());
            }
            let object = master_object(state, m);
            if let Some(title) = m.shapes.iter().find(|shape| shape.ph_type() == Some(kition_slide_model::PhType::Title))
                && let Some(id) = part_shape(state, object.as_ref(), title)
            {
                holds.read(&id, &["look", "name", "xfrm"]);
            }
        }
        "master.renameLayout" | "master.deleteLayout" => {
            let (master, layout) = match state.selection.target {
                Target::Layout { master, layout } => (master, layout),
                _ => (master_of(deck, deck.slides.get(state.selection.slide)), 0),
            };
            if let Some(chosen) = index(params, "master") {
                if let Some(object) = deck.masters.get(chosen).and_then(|m| master_object(state, m)) {
                    holds.read(object.as_str(), &[]);
                    holds.placed(object.as_str());
                }
            } else if matches!(state.selection.target, Target::Slides)
                && let Some(slide) = deck.slides.get(state.selection.slide)
            {
                holds.read(&slide_id(slide), &["layout"]);
            }
            let master = index(params, "master").unwrap_or(master);
            let layout = index(params, "layout").unwrap_or(layout);
            if let Some(object) = deck.masters.get(master).and_then(|m| m.layouts.get(layout)).and_then(|layout| layout_object(state, layout)) {
                if command == "master.renameLayout" {
                    holds.read(object.as_str(), &["props.name"]);
                } else {
                    holds.whole(object.as_str());
                }
                holds.placed(object.as_str());
            }
        }
        "master.insertPlaceholder" => {
            let Target::Layout { master, layout } = state.selection.target else { return Ok(()) };
            let Some(layout) = deck.masters.get(master).and_then(|m| m.layouts.get(layout)) else { return Ok(()) };
            let Some(object) = layout_object(state, layout) else { return Ok(()) };
            // Its number comes from the largest placeholder index, its name from the count.
            holds.read(object.as_str(), &["~shapeCount"]);
            for shape in &layout.shapes {
                if let Some(id) = part_shape(state, Some(&object), shape) {
                    holds.read(&id, &["look.ph"]);
                }
            }
            if !given(params, "rect") {
                holds.read(ROOT, &["props.slideSize"]);
            }
        }
        "animation.set"
        | "animation.add"
        | "animation.remove"
        | "animation.timing"
        | "animation.options"
        | "animation.painterApply"
        | "animation.move" => animations(doc, state, command, params, &mut holds)?,
        "comment.add" => {
            let slide_at = index(params, "slide");
            let (on, addressed, shapes, named) = named_shapes(doc, state, params, &["id"])?;
            let at = slide_at.unwrap_or(on);
            if let Some(slide) = deck.slides.get(at) {
                holds.read(&slide_id(slide), &[]);
                if slide_at.is_some() || addressed {
                    holds.placed(&slide_id(slide));
                }
                // The anchor: `id`, else (no `slide`) the first selected shape.
                let anchor = if named || slide_at.is_none() { shapes.first().copied() } else { None };
                // cmd/review.rs:155-175 stores only the anchor identity. Its content and stack
                // position do not determine the comment; an addressed slide/shape still holds place.
                if let Some(shape) = anchor.and_then(|id| kition_slide_model::find_shape(&slide.shapes, id)) {
                    acted_on(shape, addressed, &mut holds);
                }
            }
        }
        "comment.reply" | "comment.delete" | "comment.resolve" => comments(state, command, params, &mut holds),
        "show.setup" => {
            // cmd/review.rs:425-450 replaces these members separately. The returned show
            // object is receipt data; unrelated settings do not determine the stored result.
            if let Some(kind) = params.get("type").and_then(Value::as_str) {
                holds.read(ROOT, &["show.showType"]);
                if kind == "kiosk" {
                    holds.read(ROOT, &["show.loopUntilEsc"]);
                }
            }
            for (param, field) in [
                ("loop", "show.loopUntilEsc"),
                ("noNarration", "show.withoutNarration"),
                ("noAnimation", "show.withoutAnimation"),
                ("useTimings", "show.useTimings"),
            ] {
                if params.get(param).and_then(Value::as_bool).is_some() {
                    holds.read(ROOT, &[field]);
                }
            }
            match (params.get("from").and_then(Value::as_u64), params.get("to").and_then(Value::as_u64)) {
                (Some(_), Some(_)) => holds.read(ROOT, &["show.range"]),
                (None, None) if params.get("all").is_some() => holds.read(ROOT, &["show.range"]),
                _ => {}
            }
        }
        "show.customShow" => {
            holds.read(ROOT, &["customShows"]);
            if params.get("delete") != Some(&json!(true)) {
                for at in params.get("slides").and_then(Value::as_array).into_iter().flatten().filter_map(Value::as_u64) {
                    if let Some(slide) = usize::try_from(at).ok().and_then(|at| deck.slides.get(at)) {
                        holds.read(&slide_id(slide), &[]);
                        holds.placed(&slide_id(slide));
                    }
                }
            }
        }
        _ => return Ok(()),
    }
    holds.apply(found);
    Ok(())
}

fn background(state: &DocState, params: &Value, holds: &mut Holds) {
    let deck = &state.doc;
    // A picture is stored once: an existing media with its bytes is reused.
    if let Some(data) = params.get("picture").and_then(Value::as_str)
        && let Some(bytes) = cmd::base64_decode(data)
        && let Some(session) = state.kdm()
    {
        let ids = session.model().ids();
        for media in deck.media.iter().filter(|media| media.data.as_slice() == bytes.as_slice()) {
            if let Some(object) = ids.media.object(media.id) {
                holds.read(object.as_str(), &["blob"]);
            }
        }
    }
    match state.selection.target {
        Target::Master { master } => {
            if let Some(object) = deck.masters.get(master).and_then(|m| master_object(state, m)) {
                holds.read(object.as_str(), &["props.background"]);
            }
        }
        Target::Layout { master, layout } => {
            if let Some(object) = deck.masters.get(master).and_then(|m| m.layouts.get(layout)).and_then(|layout| layout_object(state, layout)) {
                holds.read(object.as_str(), &["props.background"]);
                holds.placed(object.as_str());
            }
        }
        Target::Slides if params.get("all") == Some(&json!(true)) => {
            if params.get("reset") != Some(&json!(true)) {
                for m in deck.masters.iter() {
                    if let Some(object) = master_object(state, m) {
                        holds.read(object.as_str(), &["props.background"]);
                    }
                    for layout in &m.layouts {
                        if let Some(object) = layout_object(state, layout) {
                            holds.read(object.as_str(), &["props.background"]);
                        }
                    }
                }
            }
            for slide in deck.slides.iter() {
                holds.read(&slide_id(slide), &["background"]);
            }
            holds.read(ROOT, &["~slideOrder"]);
        }
        Target::Slides => {
            let at = index(params, "index");
            if let Some(slide) = deck.slides.get(at.unwrap_or(state.selection.slide)) {
                holds.read(&slide_id(slide), &["background"]);
                if at.is_some() {
                    holds.placed(&slide_id(slide));
                }
            }
        }
    }
}

fn slide_size(state: &DocState, params: &Value, holds: &mut Holds) {
    let deck = &state.doc;
    let scaled = params.get("scale").and_then(Value::as_str) != Some("none");
    holds.read(ROOT, &["props.slideSize"]);
    if scaled {
        holds.read(ROOT, &["~slideOrder"]);
    }
    for m in deck.masters.iter() {
        let master = master_object(state, m);
        if let Some(object) = &master {
            holds.read(object.as_str(), &["listStyles"]);
        }
        // Masters and layouts stretch to the new size; their shrinking placeholders refit.
        let ctx = Ctx::for_master(deck, m);
        for shape in &m.shapes {
            if let Some(id) = part_shape(state, master.as_ref(), shape) {
                stretched(&ctx, shape, &id, holds);
            }
        }
        for layout in &m.layouts {
            let object = layout_object(state, layout);
            let ctx = Ctx::for_layout(deck, m, layout);
            for shape in &layout.shapes {
                if let Some(id) = part_shape(state, object.as_ref(), shape) {
                    stretched(&ctx, shape, &id, holds);
                }
            }
        }
    }
    // Slides scale their own frames and run sizes by the mode; their placeholders refit.
    for slide in deck.slides.iter() {
        let Some(ctx) = Ctx::for_slide(deck, slide) else { continue };
        for shape in &slide.shapes {
            let id = cmd::inspect::shape_tool_id(shape);
            if shrinks(&ctx, shape) {
                holds.read(&id, &["body"]);
            }
            if scaled {
                if shape.xfrm.is_some() {
                    holds.read(&id, &["xfrm"]);
                }
                if shape.text.is_some() {
                    holds.read(&id, &["body"]);
                }
            }
        }
    }
}

fn stretched(ctx: &Ctx<'_>, shape: &Shape, id: &str, holds: &mut Holds) {
    if shape.xfrm.is_some() {
        holds.read(id, &["xfrm"]);
    }
    if shrinks(ctx, shape) {
        holds.read(id, &["body"]);
    }
}

fn header_footer(state: &DocState, params: &Value, holds: &mut Holds) {
    use kition_slide_model::PhType;
    let deck = &state.doc;
    let all = params.get("all").and_then(Value::as_bool).unwrap_or(true);
    holds.read(ROOT, &["headerFooter"]);
    if all {
        holds.read(ROOT, &["~slideOrder"]);
    }
    let flag = |key: &str, now: bool| params.get(key).and_then(Value::as_bool).unwrap_or(now);
    let hf = &deck.header_footer;
    let footer_text = params.get("footerText").and_then(Value::as_str).is_some_and(|text| !text.is_empty());
    let wanted = [
        (PhType::Date, flag("date", hf.date)),
        (PhType::Footer, flag("footer", hf.footer) || footer_text),
        (PhType::SlideNum, flag("slideNumber", hf.slide_number)),
    ];
    let hide_on_title = flag("hideOnTitle", hf.hide_on_title);
    let list: Vec<usize> = if all { (0..deck.slides.len()).collect() } else { vec![state.selection.slide] };
    for slide in list.iter().filter_map(|at| deck.slides.get(*at)) {
        holds.read(&slide_id(slide), &["layout", "shapes"]);
        let Some((_, layout)) = deck.layout(slide.layout) else { continue };
        let object = layout_object(state, layout);
        if hide_on_title {
            if let Some(object) = &object {
                holds.read(object.as_str(), &["props.kind"]);
            }
            if layout.kind == LayoutType::Title {
                continue;
            }
        }
        // A wanted placeholder the slide lacks is copied from its layout's.
        for (kind, _) in wanted.iter().filter(|(kind, want)| *want && !slide.shapes.iter().any(|shape| shape.ph_type() == Some(*kind))) {
            if let Some(shape) = layout.shapes.iter().find(|shape| shape.ph_type() == Some(*kind))
                && let Some(id) = part_shape(state, object.as_ref(), shape)
            {
                holds.read(&id, &["look.ph", "name"]);
            }
        }
    }
}

fn animations(doc: &SlideDocument, state: &DocState, command: &str, params: &Value, holds: &mut Holds) -> Result<(), String> {
    let deck = &state.doc;
    let (at, addressed, shapes, _) = named_shapes(doc, state, params, &["ids", "id"])?;
    let Some(slide) = deck.slides.get(at) else { return Ok(()) };
    let id = slide_id(slide);
    let by_index = matches!(command, "animation.remove" | "animation.timing" | "animation.options" | "animation.move");
    if let (true, Some(first)) = (by_index, index(params, "index")) {
        if command == "animation.move" {
            // cmd/animation.rs move_anim reads no animation's content: it takes the item at
            // `index` out of the slide's list and puts it back at `to` (clamped to the length
            // left). `under` holds both places as windows of the slide's `animations` (lib.rs):
            // the item at `index` and the gap it goes into. The moved item is carried whole, so
            // an edit of any animation's content, the moved one's too, does not change the result.
            holds.read(&id, &[]);
        } else {
            holds.read_owned(&id, [format!("animations.{first}")]);
        }
    } else {
        let targets: Vec<&Shape> = shapes.iter().filter_map(|shape| kition_slide_model::find_shape(&slide.shapes, *shape)).collect();
        for shape in &targets {
            acted_on(shape, addressed, holds);
        }
        // retain() tests every target, including nonmatches. A colleague
        // can retarget the last matching animation away before the guard asks under() again;
        // holding only current matches then forgets the changed premise entirely.
        if command == "animation.remove" {
            holds.read_owned(
                &id,
                slide
                    .animations
                    .iter()
                    .enumerate()
                    .filter(|(_, animation)| !targets.iter().any(|shape| shape.id == animation.shape))
                    .map(|(at, _)| format!("animations.{at}.props.shape")),
            );
        }
        // The targets' own animations are replaced or edited; an added one goes at the end.
        if command != "animation.add" {
            let own: Vec<String> = slide
                .animations
                .iter()
                .enumerate()
                .filter(|(_, animation)| targets.iter().any(|shape| shape.id == animation.shape))
                .map(|(at, _)| format!("animations.{at}"))
                .collect();
            if !own.is_empty() {
                holds.read_owned(&id, own);
            }
        }
    }
    // `onSlide` names the slide by place; else it is held only for its animations.
    if addressed {
        holds.read(&id, &[]);
        holds.placed(&id);
    }
    // A trigger shape is named only to start the animation.
    if command == "animation.timing"
        && let Some(trigger) = params.get("trigger").filter(|value| !value.is_null())
    {
        let mut local = json!({ "id": trigger });
        if let Some(text) = trigger.as_str() {
            local = json!({ "ids": [text] });
        }
        let (_, _, named, _) = named_shapes(doc, state, &local, &["ids", "id"])?;
        if let Some(shape) = named.first().and_then(|shape| kition_slide_model::find_shape(&slide.shapes, *shape)) {
            acted_on(shape, trigger.is_number(), holds);
        }
    }
    let custom = params.get("customPath").is_some() || params.get("effect").and_then(Value::as_str) == Some("customPath");
    if custom && matches!(command, "animation.set" | "animation.add") {
        holds.read(ROOT, &["props.slideSize"]);
    }
    Ok(())
}

fn comments(state: &DocState, command: &str, params: &Value, holds: &mut Holds) {
    let deck = &state.doc;
    let field: &[&str] = match command {
        "comment.delete" => &["comments"],
        "comment.resolve" => &["resolved"],
        _ => &[],
    };
    let thread = params.get("thread").and_then(Value::as_str);
    if let Some(thread) = thread.filter(|id| deck.slides.iter().any(|slide| slide.comments.iter().any(|comment| comment.id == *id))) {
        holds.read(thread, field);
        return;
    }
    let place = match thread {
        Some(id) => id
            .strip_prefix('@')
            .and_then(|rest| rest.split_once(':'))
            .and_then(|(slide, at)| Some((slide.parse::<usize>().ok()?, at.parse::<usize>().ok()?))),
        None => index(params, "index").map(|at| (state.selection.slide, at)),
    };
    if let Some((si, at)) = place {
        let Some(slide) = deck.slides.get(si) else { return };
        // The ordinal names whichever thread is there: hold the slot, then the thread it names.
        holds.read_owned(ROOT, [format!("~commentOrder.slide:{}.{at}", slide.id.0)]);
        if thread.is_some() {
            holds.read(&slide_id(slide), &[]);
            holds.placed(&slide_id(slide));
        }
        if let Some(comment) = slide.comments.get(at) {
            holds.read(&comment.id, field);
        }
        return;
    }
    if command != "comment.delete" {
        return;
    }
    if params.get("all") == Some(&json!(true)) {
        holds.read(ROOT, &["~commentOrder", "~slideOrder"]);
        for slide in deck.slides.iter() {
            for comment in &slide.comments {
                holds.read(&comment.id, field);
            }
        }
    } else if let Some(slide) = deck.slides.get(state.selection.slide) {
        holds.read_owned(ROOT, [format!("~commentOrder.slide:{}", slide.id.0)]);
        for comment in &slide.comments {
            holds.read(&comment.id, field);
        }
    }
}
