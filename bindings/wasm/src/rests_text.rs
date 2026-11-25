//! What text edits and text formatting read (C8-slide-read-3): the edited body (a shape's
//! paragraphs and body properties, a table's cells, the slide's notes), a group's members, and
//! only when the handler resolves inherited values (a toggle without `on`, grow/shrink, a box that
//! fits its text, a placeholder whose shrink result is refitted) the placeholders, master styles,
//! theme fonts and default text style they come from.

use std::collections::BTreeMap;

use super::*;
use kition_slide_engine::{Selection, Target, TextSel};
use kition_slide_model::resolve::{self, Ctx};
use kition_slide_model::text::AutoFit;
use kition_slide_model::{PhType, Shape, ShapeId, ShapeKind};

/// The handler families: run formatting (`format_run`), paragraph formatting (`format_para`),
/// body properties (`format_body`), text replacement and typing, case change, the format
/// painter and entering text edit.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Class {
    Run,
    Para,
    Body,
    Text,
    Case,
    Painter,
    Edit,
}

fn class(command: &str) -> Option<Class> {
    Some(match command {
        "text.set" | "text.insert" | "text.delete" => Class::Text,
        "text.edit" => Class::Edit,
        "format.case" => Class::Case,
        "format.painterApply" => Class::Painter,
        "format.bold"
        | "format.italic"
        | "format.underline"
        | "format.strikethrough"
        | "format.superscript"
        | "format.subscript"
        | "format.fontName"
        | "format.fontSize"
        | "format.increaseFontSize"
        | "format.decreaseFontSize"
        | "format.color"
        | "format.highlight"
        | "format.caps"
        | "format.spacing"
        | "format.clear"
        | "format.textShadow"
        | "format.textOutline" => Class::Run,
        "format.align" | "format.rtl" | "format.alignLeft" | "format.alignCenter" | "format.alignRight" | "format.justify" | "format.bullets"
        | "format.numbering" | "format.indent" | "format.outdent" | "format.lineSpacing" | "format.paragraph" => Class::Para,
        "format.transform" | "format.anchor" | "format.direction" | "format.columns" | "format.autofit" | "format.wrap" | "format.margins" => {
            Class::Body
        }
        _ => return None,
    })
}

/// Whether the handler resolves the effective run or paragraph value at the selection (the
/// first selected shape's first run or paragraph) before it writes.
fn toggles(command: &str, params: &Value) -> bool {
    match command {
        "format.increaseFontSize" | "format.decreaseFontSize" => true,
        "format.bold"
        | "format.italic"
        | "format.underline"
        | "format.strikethrough"
        | "format.superscript"
        | "format.subscript"
        | "format.textShadow"
        | "format.rtl"
        | "format.bullets"
        | "format.numbering" => params.get("on").and_then(Value::as_bool).is_none(),
        _ => false,
    }
}

const LOOK: [&str; 4] = ["look.fill", "look.line", "look.effects", "look.style"];
const ROOT: &str = "slide-file";

/// The objects a call reads and the fields of each, in the order found.
struct Reads<'a> {
    state: &'a DocState,
    ctx: Ctx<'a>,
    shapes: &'a [Shape],
    target: Target,
    owner: Option<kition_kit::kdm::ObjectId>,
    slide: Option<String>,
    needs: BTreeMap<String, Vec<String>>,
    order: Vec<String>,
}

impl<'a> Reads<'a> {
    fn need(&mut self, id: impl Into<String>, fields: &[&str]) {
        let id = id.into();
        let held = self.needs.entry(id.clone()).or_insert_with(|| {
            self.order.push(id);
            Vec::new()
        });
        for field in fields {
            if !held.iter().any(|have| have == field) {
                held.push((*field).to_owned());
            }
        }
    }

    fn public(&self, shape: &Shape) -> Option<String> {
        match self.target {
            Target::Slides => Some(cmd::inspect::shape_tool_id(shape)),
            _ => self.owner.as_ref().and_then(|owner| self.state.kdm()?.model().ids().shape(owner, shape.id)).map(ToString::to_string),
        }
    }

    fn find(&self, id: ShapeId) -> Option<&'a Shape> {
        kition_slide_model::find_shape(self.shapes, id)
    }

    fn master(&self) -> Option<String> {
        self.state.kdm()?.model().ids().masters.object(self.ctx.master.id).map(ToString::to_string)
    }

    /// The slide's layout, through which the master (its styles and theme) is found.
    fn through_layout(&mut self) {
        if let Some(slide) = self.slide.clone() {
            self.need(slide, &["layout"]);
        }
    }

    /// The placeholders a parent lookup compares (resolve.rs `find_ph`, `master_ph`): each up to
    /// the parent it finds, every one when it finds none, on their placeholder kind and index.
    fn lookup(&mut self, shape: &Shape) {
        let Some(ph) = shape.ph.as_ref() else { return };
        let Some(session) = self.state.kdm() else { return };
        let ids = session.model().ids();
        let (lp, mp) = resolve::parents(&self.ctx, shape);
        let mut compared = Vec::new();
        if let Some(layout) = self.ctx.layout
            && let Some(owner) = ids.layouts.object(layout.id)
        {
            let whole = lp.is_none_or(|lp| ph.idx != 0 && lp.ph.as_ref().is_some_and(|p| p.idx != ph.idx));
            for parent in &layout.shapes {
                if parent.ph.is_some()
                    && let Some(id) = ids.shape(owner, parent.id)
                {
                    compared.push(id.to_string());
                }
                if !whole && lp.is_some_and(|lp| lp.id == parent.id) {
                    break;
                }
            }
        }
        if let Some(owner) = ids.masters.object(self.ctx.master.id) {
            for parent in &self.ctx.master.shapes {
                if parent.ph.is_some()
                    && let Some(id) = ids.shape(owner, parent.id)
                {
                    compared.push(id.to_string());
                }
                if mp.is_some_and(|mp| mp.id == parent.id) {
                    break;
                }
            }
        }
        for id in compared {
            self.need(id, &["look.ph"]);
        }
    }

    /// What resolving a placeholder's text reads: its layout and master placeholders' body
    /// properties (list styles included), the layout one's shape style, and the master's text
    /// style group for its kind.
    fn placeholder_text(&mut self, shape: &Shape, frame: bool) {
        if shape.ph.is_none() {
            return;
        }
        self.through_layout();
        self.lookup(shape);
        let Some(session) = self.state.kdm() else { return };
        let ids = session.model().ids();
        let (lp, mp) = resolve::parents(&self.ctx, shape);
        let layout = self.ctx.layout.and_then(|layout| ids.layouts.object(layout.id));
        let master = ids.masters.object(self.ctx.master.id);
        let bare = frame && shape.xfrm.is_none();
        if let (Some(owner), Some(parent)) = (layout, lp)
            && let Some(id) = ids.shape(owner, parent.id)
        {
            let id = id.to_string();
            self.need(id.as_str(), &["body.props", "look.ph", "look.style"]);
            if bare {
                self.need(id, &["xfrm"]);
            }
        }
        if let (Some(owner), Some(parent)) = (master, mp)
            && let Some(id) = ids.shape(owner, parent.id)
        {
            let id = id.to_string();
            self.need(id.as_str(), &["body.props", "look.ph"]);
            if bare && lp.and_then(|lp| lp.xfrm).is_none() {
                self.need(id, &["xfrm"]);
            }
        }
        let style = match shape.ph_type() {
            Some(PhType::Title | PhType::CtrTitle) => "listStyles.title",
            Some(
                PhType::Body
                | PhType::Obj
                | PhType::SubTitle
                | PhType::Table
                | PhType::Chart
                | PhType::Diagram
                | PhType::Media
                | PhType::Picture
                | PhType::ClipArt,
            ) => "listStyles.body",
            Some(_) => "listStyles.other",
            None => return,
        };
        if let Some(master) = self.master() {
            self.need(master, &[style]);
        }
    }

    /// What a text layout of `shape` reads besides its own body: the theme's fonts, the default
    /// text style, the slide size for a box nobody sized, and the frames of the groups it is in.
    fn layout_inputs(&mut self, shape: &Shape) {
        self.through_layout();
        if let Some(master) = self.master() {
            self.need(master, &["theme.fonts"]);
        }
        self.need(ROOT, &["defaultTextStyle"]);
        let (lp, mp) = resolve::parents(&self.ctx, shape);
        if shape.xfrm.is_none() && lp.and_then(|parent| parent.xfrm).or_else(|| mp.and_then(|parent| parent.xfrm)).is_none() {
            self.need(ROOT, &["props.slideSize"]);
        }
        let mut groups = Vec::new();
        ancestors(self.shapes, shape.id, &mut groups, 0);
        for group in groups {
            if let Some(group) = self.find(group)
                && let Some(id) = self.public(group)
            {
                self.need(id, &["xfrm", "look.kind.child"]);
            }
        }
    }

    /// What resolving the first selected run or paragraph reads beyond the shape's own body.
    fn toggle(&mut self, shape: Option<&Shape>) {
        if let Some(shape) = shape {
            self.placeholder_text(shape, false);
        }
        self.need(ROOT, &["defaultTextStyle"]);
    }

    /// One shape the call writes: its own fields by kind and family, a group's members, and a
    /// refit (`fit_text_box` when `fits`, the placeholder shrink refit) with what it reads.
    fn shape(&mut self, shape: &Shape, class: Class, fits: bool, autofit: Option<AutoFit>, depth: usize) {
        let Some(id) = self.public(shape) else { return };
        if class == Class::Edit {
            let creates = shape.text.is_none() && matches!(shape.kind, ShapeKind::Shape | ShapeKind::Connector { .. });
            self.need(id, if creates { &["body"] } else { &[] });
            return;
        }
        if shape.is_group() {
            match class {
                Class::Case => self.need(id, &["body"]),
                Class::Painter => {
                    self.need(id.as_str(), &LOOK);
                    self.need(id, &["body"]);
                }
                _ => {
                    self.need(id, &["children"]);
                    if depth < 32 {
                        for child in shape.children() {
                            self.shape(child, class, false, autofit, depth + 1);
                        }
                    }
                }
            }
            return;
        }
        let own: &[&str] = match (&shape.kind, class) {
            (ShapeKind::Table(_), Class::Run | Class::Para | Class::Text) => &["table.cells"],
            (ShapeKind::Chart(_), Class::Run) => &["chart"],
            (ShapeKind::Shape, Class::Body) => &["body.props"],
            (ShapeKind::Shape, _) => &["body.paragraphs", "body.props"],
            _ => &["body"],
        };
        self.need(id.as_str(), own);
        if class == Class::Painter {
            self.need(id.as_str(), &LOOK);
        }
        if !matches!(shape.kind, ShapeKind::Shape) {
            return;
        }
        let now = resolve::body(&self.ctx, shape).autofit;
        let after = autofit.or(now);
        let shrink = |fit: Option<AutoFit>| matches!(fit, Some(AutoFit::Shrink { .. }));
        let refit = shrink(now) || shrink(after);
        let resize = fits && after == Some(AutoFit::Shape);
        if refit || resize {
            self.need(id.as_str(), &["xfrm", "body.paragraphs", "body.props", "look.geom", "look.style"]);
            self.layout_inputs(shape);
        }
        if refit && shape.ph.is_some() {
            self.need(id, &["look.ph"]);
            self.placeholder_text(shape, true);
        }
    }

    /// The text being edited: the notes, a table cell or a shape's body.
    fn text(&mut self, text: &TextSel, class: Class, toggle: bool) {
        if text.notes {
            if let Some(slide) = self.slide.clone() {
                self.need(slide, &["notesBody"]);
            }
            if toggle {
                self.toggle(None);
            }
            return;
        }
        let Some(shape) = self.find(text.shape) else { return };
        if text.cell.is_some() {
            if let Some(id) = self.public(shape) {
                self.need(id, &["table.cells"]);
            }
            if toggle {
                self.toggle(None);
            }
            return;
        }
        self.shape(shape, class, true, None, 0);
        if toggle {
            self.toggle(Some(shape));
        }
    }

    /// Whole shapes: each written, the first one's first run or paragraph resolved by a toggle.
    fn whole(&mut self, ids: &[ShapeId], class: Class, toggle: bool, autofit: Option<AutoFit>) {
        let fits = matches!(class, Class::Run | Class::Para | Class::Body | Class::Text);
        for (at, id) in ids.iter().enumerate() {
            let Some(shape) = self.find(*id) else { continue };
            self.shape(shape, class, fits, autofit, 0);
            if toggle && at == 0 {
                let placeholder = (matches!(shape.kind, ShapeKind::Shape) && shape.ph.is_some()).then_some(shape);
                self.toggle(placeholder);
            }
        }
    }
}

/// The groups holding shape `target`, outermost first.
fn ancestors(shapes: &[Shape], target: ShapeId, out: &mut Vec<ShapeId>, depth: usize) -> bool {
    if depth > 64 {
        return false;
    }
    for shape in shapes {
        if shape.id == target {
            return true;
        }
        if shape.is_group() {
            out.push(shape.id);
            if ancestors(shape.children(), target, out, depth + 1) {
                return true;
            }
            out.pop();
        }
    }
    false
}

fn local_id(value: &Value) -> Option<ShapeId> {
    value.as_u64().or_else(|| value.as_str().and_then(|text| text.parse().ok())).and_then(|n| u32::try_from(n).ok()).map(ShapeId)
}

fn autofit_param(command: &str, params: &Value) -> Option<AutoFit> {
    (command == "format.autofit").then(|| match params.get("mode").and_then(Value::as_str).unwrap_or("none") {
        "shrink" => AutoFit::Shrink { font_scale: 1.0, line_reduction: 0.0 },
        "resize" | "shape" => AutoFit::Shape,
        _ => AutoFit::None,
    })
}

pub(super) fn add(doc: &SlideDocument, command: &str, params: &Value, found: &mut kition_kit::Under) -> Result<(), String> {
    let Some(class) = class(command) else { return Ok(()) };
    let state = doc.state()?;
    let mut local = params.clone();
    let implied = doc.session.take_tool_ids(&mut local).map_err(command_failure)?;
    let explicit = params.get(ON_SLIDE).and_then(Value::as_u64).and_then(|n| usize::try_from(n).ok()).and_then(|n| n.checked_sub(1));
    let mut selection: Selection = state.selection.clone();
    if let Some(on) = implied.or(explicit) {
        selection.slide = on;
        selection.target = Target::Slides;
    }
    let ids = state.kdm().map(|session| session.model().ids());
    let (ctx, owner, shapes, slide) = match selection.target {
        Target::Slides => {
            let Some(slide) = state.doc.slides.get(selection.slide) else { return Ok(()) };
            let Some(ctx) = Ctx::for_slide(&state.doc, slide) else { return Ok(()) };
            (ctx, ids.and_then(|ids| ids.slides.object(slide.id)).cloned(), slide.shapes.as_slice(), Some(format!("slide:{}", slide.id.0)))
        }
        Target::Master { master } => {
            let Some(master) = state.doc.masters.get(master) else { return Ok(()) };
            (Ctx::for_master(&state.doc, master), ids.and_then(|ids| ids.masters.object(master.id)).cloned(), master.shapes.as_slice(), None)
        }
        Target::Layout { master, layout } => {
            let Some(master) = state.doc.masters.get(master) else { return Ok(()) };
            let Some(layout) = master.layouts.get(layout) else { return Ok(()) };
            (Ctx::for_layout(&state.doc, master, layout), ids.and_then(|ids| ids.layouts.object(layout.id)).cloned(), layout.shapes.as_slice(), None)
        }
    };
    let mut reads = Reads { state, ctx, shapes, target: selection.target, owner, slide, needs: BTreeMap::new(), order: Vec::new() };
    let named: Vec<ShapeId> = local
        .get("ids")
        .and_then(Value::as_array)
        .map(|ids| ids.iter().filter_map(local_id).collect())
        .or_else(|| local.get("id").filter(|id| !id.is_null()).and_then(local_id).map(|id| vec![id]))
        .unwrap_or_default();
    let toggle = toggles(command, params);
    let text = selection.text.clone();
    match class {
        Class::Edit => {
            if params.get("notes").and_then(Value::as_bool) != Some(true)
                && let Some(id) = named.first().copied().or_else(|| selection.shapes.first().copied())
                && let Some(shape) = reads.find(id)
            {
                reads.shape(shape, Class::Edit, false, None, 0);
            }
        }
        Class::Text if command == "text.set" => {
            if let Some(id) = named.first().copied().or_else(|| selection.shapes.first().copied())
                && let Some(shape) = reads.find(id)
            {
                reads.shape(shape, Class::Text, true, None, 0);
            }
        }
        Class::Text => {
            if let Some(text) = &text {
                reads.text(text, Class::Text, false);
            }
        }
        _ if !named.is_empty() => reads.whole(&named, class, toggle, autofit_param(command, params)),
        Class::Body => {
            let ids = match &text {
                Some(text) if !text.notes => vec![text.shape],
                _ => selection.shapes.clone(),
            };
            reads.whole(&ids, class, toggle, autofit_param(command, params));
        }
        Class::Case | Class::Painter if text.as_ref().is_some_and(|text| !text.is_range()) => {
            if class == Class::Painter {
                reads.whole(&selection.shapes, class, false, None);
            }
        }
        Class::Painter => {
            if let Some(text) = &text {
                reads.text(text, Class::Run, false);
            } else {
                reads.whole(&selection.shapes, class, false, None);
            }
        }
        _ => match &text {
            Some(text) => reads.text(text, class, toggle),
            None => reads.whole(&selection.shapes, class, toggle, None),
        },
    }
    let Reads { needs, order, .. } = reads;
    for id in order {
        let Some(fields) = needs.get(&id) else { continue };
        let key = json!(id);
        let before: Option<Vec<String>> = found.premises.iter().rev().find(|premise| premise.id == key).and_then(|premise| premise.positions.clone());
        let slide_premise = id.starts_with("slide:");
        let mut fields = fields.clone();
        if slide_premise
            && let Some(earlier) = found.premises.iter().rev().find(|premise| premise.id == key).and_then(|premise| premise.fields.clone())
        {
            for field in earlier {
                if !fields.contains(&field) {
                    fields.push(field);
                }
            }
        }
        let had = found.premises.iter().any(|premise| premise.id == key);
        found.premises.retain(|premise| premise.id != key);
        let mut premise = kition_kit::Premise::on(id.as_str()).fields(fields);
        // A shape keeps the address it was named by; a slide named by its number keeps its
        // place. Nothing else here depends on where an object is.
        premise = match before {
            Some(positions) if positions.iter().any(|at| at == "address") => premise.positions(positions),
            None if slide_premise && had => premise,
            _ => premise.positions(Vec::<String>::new()),
        };
        found.premises.push(premise);
    }
    Ok(())
}
