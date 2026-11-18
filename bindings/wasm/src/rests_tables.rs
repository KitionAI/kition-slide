//! What table commands read of the table they edit (C8-slide-read-3): the table named by `id`,
//! else the one the selection holds, and of it only the parts the command computes from: the
//! grid (cells, columns, rows), the style in its props, the frame a grid change resizes.

use super::*;
use kition_slide_engine::Target;
use kition_slide_model::resolve::{self, Ctx};
use kition_slide_model::{PhType, Shape, ShapeId, ShapeKind};

fn reads(command: &str) -> Option<&'static [&'static str]> {
    Some(match command {
        "table.cellFormat" | "table.border" | "table.clearCells" | "table.merge" | "table.split" | "table.cellFill" => {
            &["table.cells", "table.cols", "table.rows"]
        }
        "table.insertRowAbove"
        | "table.insertRowBelow"
        | "table.insertColumnLeft"
        | "table.insertColumnRight"
        | "table.deleteRow"
        | "table.deleteColumn" => &["table.cells", "table.cols", "table.rows", "xfrm"],
        "table.style" | "table.options" => &["table.props"],
        "table.distributeRows" => &["table.rows"],
        "table.distributeColumns" => &["table.cols"],
        "table.columnWidth" => &["table.cols", "xfrm"],
        "table.rowHeight" => &["table.rows", "xfrm"],
        _ => return None,
    })
}

fn local_id(value: &Value) -> Option<ShapeId> {
    value.as_u64().or_else(|| value.as_str().and_then(|text| text.parse().ok())).and_then(|n| u32::try_from(n).ok()).map(ShapeId)
}

pub(super) fn add(doc: &SlideDocument, command: &str, params: &Value, found: &mut kition_kit::Under) -> Result<(), String> {
    if matches!(command, "insert.slideNumber" | "insert.dateTime" | "insert.symbol" | "insert.hyperlink") {
        return fields_and_links(doc, command, params, found);
    }
    let Some(fields) = reads(command) else { return Ok(()) };
    let state = doc.state()?;
    let mut local = params.clone();
    let implied = doc.session.take_tool_ids(&mut local).map_err(command_failure)?;
    let explicit = params.get(ON_SLIDE).and_then(Value::as_u64).and_then(|n| usize::try_from(n).ok()).and_then(|n| n.checked_sub(1));
    let mut selection = state.selection.clone();
    if let Some(on) = implied.or(explicit) {
        selection.slide = on;
        selection.target = Target::Slides;
    }
    let ids = state.kdm().map(|session| session.model().ids());
    let (shapes, owner) = match selection.target {
        Target::Slides => {
            let Some(slide) = state.doc.slides.get(selection.slide) else { return Ok(()) };
            (slide.shapes.as_slice(), None)
        }
        Target::Master { master } => {
            let Some(master) = state.doc.masters.get(master) else { return Ok(()) };
            (master.shapes.as_slice(), ids.and_then(|ids| ids.masters.object(master.id)))
        }
        Target::Layout { master, layout } => {
            let Some(layout) = state.doc.masters.get(master).and_then(|master| master.layouts.get(layout)) else { return Ok(()) };
            (layout.shapes.as_slice(), ids.and_then(|ids| ids.layouts.object(layout.id)))
        }
    };
    let public = |shape: &Shape| match selection.target {
        Target::Slides => Some(cmd::inspect::shape_tool_id(shape)),
        _ => owner.and_then(|owner| ids?.shape(owner, shape.id)).map(ToString::to_string),
    };
    // `with_table` (cmd/table.rs): `id`, else the edited cell's table, the selected cells' table,
    // the first selected shape. The cell commands also find the table by `id` or the first
    // selected shape (cmd/table/selection.rs `table_id`) to compute the cells they change.
    let named = local.get("id").filter(|id| !id.is_null()).and_then(local_id);
    let edited = selection
        .text
        .as_ref()
        .filter(|text| text.cell.is_some())
        .map(|text| text.shape)
        .or(selection.cells.map(|cells| cells.0))
        .or_else(|| selection.shapes.first().copied());
    let mut tables = vec![named.or(edited)];
    if matches!(command, "table.cellFormat" | "table.border" | "table.clearCells" | "table.cellFill") {
        tables.push(named.or_else(|| selection.shapes.first().copied()));
    }
    let mut held: Vec<String> = Vec::new();
    for table in tables.into_iter().flatten() {
        let Some(shape) = kition_slide_model::find_shape(shapes, table) else { continue };
        if !matches!(shape.kind, ShapeKind::Table(_)) {
            continue;
        }
        let Some(id) = public(shape) else { continue };
        if held.contains(&id) {
            continue;
        }
        let key = json!(id);
        let positions = found
            .premises
            .iter()
            .rev()
            .find(|premise| premise.id == key)
            .and_then(|premise| premise.positions.clone())
            .filter(|positions| positions.iter().any(|at| at == "address"))
            .unwrap_or_default();
        found.premises.retain(|premise| premise.id != key);
        found.premises.push(kition_kit::Premise::on(id.as_str()).fields(fields.iter().copied()).positions(positions));
        held.push(id);
    }
    Ok(())
}

/// Sets the premise on `id` to `fields` and `positions` (`None`: every position), replacing
/// what earlier families held of it.
fn put(found: &mut kition_kit::Under, id: &str, fields: &[&str], positions: Option<Vec<String>>) {
    let key = json!(id);
    let mut fields: Vec<String> = fields.iter().map(|field| (*field).to_owned()).collect();
    if let Some(earlier) = found.premises.iter().find(|premise| premise.id == key).and_then(|premise| premise.fields.clone()) {
        for field in earlier {
            if !fields.contains(&field) {
                fields.push(field);
            }
        }
    }
    found.premises.retain(|premise| premise.id != key);
    let premise = kition_kit::Premise::on(id).fields(fields);
    found.premises.push(match positions {
        Some(positions) => premise.positions(positions),
        None => premise,
    });
}

/// The insert commands that write into the text being edited, else (slide number, date) turn on
/// a footer placeholder on every slide, or (hyperlink) set the click action of shapes. What
/// earlier families held for them is replaced: only an address a shape was named by is kept.
fn fields_and_links(doc: &SlideDocument, command: &str, params: &Value, found: &mut kition_kit::Under) -> Result<(), String> {
    let state = doc.state()?;
    let addressed: Vec<kition_kit::Premise> = found
        .premises
        .iter()
        .filter(|premise| premise.positions.as_ref().is_some_and(|positions| positions.iter().any(|at| at == "address")))
        .map(|premise| kition_kit::Premise { fields: Some(Vec::new()), ..premise.clone() })
        .collect();
    found.premises = addressed;
    let mut local = params.clone();
    doc.session.take_tool_ids(&mut local).map_err(command_failure)?;
    let named: Vec<ShapeId> = local
        .get("ids")
        .and_then(Value::as_array)
        .map(|ids| ids.iter().filter_map(local_id).collect())
        .or_else(|| local.get("id").filter(|id| !id.is_null()).and_then(local_id).map(|id| vec![id]))
        .unwrap_or_default();
    let editing = state.selection.text.clone();
    let caret = editing.is_some() && (command != "insert.hyperlink" || named.is_empty());
    if command == "insert.hyperlink"
        && let Some(at) = params.get("slide").and_then(Value::as_u64).and_then(|n| usize::try_from(n).ok())
        && let Some(slide) = state.doc.slides.get(at)
    {
        // The slide at that index is linked by its id: where it is decides which slide it is.
        // Its title is the text a bare caret inserts: held without `ids` even when nothing is
        // edited yet, since an earlier step of the same run may enter text edit.
        let id = format!("slide:{}", slide.id.0);
        if named.is_empty() {
            put(found, &id, &["shapes"], None);
            if let Some(title) = slide.shapes.iter().find(|shape| shape.ph_type().is_some_and(PhType::is_title)) {
                put(found, &cmd::inspect::shape_tool_id(title), &["body.paragraphs", "look.ph"], Some(Vec::new()));
            }
        } else {
            put(found, &id, &[], None);
        }
    }
    if caret {
        super::rests_text::add(doc, "text.insert", &json!({}), found)?;
        // A field or symbol typed into a table cell locates the cell in the grid.
        if command != "insert.hyperlink"
            && let Some(text) = editing.as_ref().filter(|text| text.cell.is_some())
            && let Some(shape) = state.shape(text.shape)
        {
            put(found, &cmd::inspect::shape_tool_id(shape), &["table.cells", "table.cols", "table.rows"], Some(Vec::new()));
        }
        return Ok(());
    }
    match command {
        "insert.slideNumber" => footers(state, PhType::SlideNum, found),
        "insert.dateTime" => footers(state, PhType::Date, found),
        "insert.hyperlink" => {
            let targets = if named.is_empty() { state.selection.shapes.clone() } else { named };
            links(state, &targets, found);
        }
        _ => {}
    }
    Ok(())
}

/// design.headerFooter turning on `kind` on every slide (cmd/design.rs:349-440): the deck's
/// header and footer settings, each slide's layout and shapes (the placeholder kinds it has),
/// each layout's shapes and the placeholder of that kind it copies (name and placeholder).
fn footers(state: &DocState, kind: PhType, found: &mut kition_kit::Under) {
    put(found, "slide-file", &["headerFooter"], Some(Vec::new()));
    let Some(session) = state.kdm() else { return };
    let ids = session.model().ids();
    let mut layouts = Vec::new();
    for slide in &state.doc.slides {
        put(found, &format!("slide:{}", slide.id.0), &["layout", "shapes"], Some(Vec::new()));
        if !layouts.contains(&slide.layout) {
            layouts.push(slide.layout);
        }
    }
    for layout in layouts {
        let Some((_, layout)) = state.doc.layout(layout) else { continue };
        let Some(owner) = ids.layouts.object(layout.id) else { continue };
        put(found, &owner.to_string(), &["shapes"], Some(Vec::new()));
        if let Some(placeholder) = layout.shapes.iter().find(|shape| shape.ph_type() == Some(kind))
            && let Some(id) = ids.shape(owner, placeholder.id)
        {
            put(found, &id.to_string(), &["look.ph", "name"], Some(Vec::new()));
        }
    }
}

/// The click action set on each target (cmd/insert.rs hyperlink, edit_shapes): replaced whole;
/// a placeholder without its own frame is given the one it inherits (cmd/mod.rs:353-354), which
/// the parent lookup finds through the slide's layout (resolve.rs:60-97).
fn links(state: &DocState, targets: &[ShapeId], found: &mut kition_kit::Under) {
    let Some(slide) = state.doc.slides.get(state.selection.slide) else { return };
    let Some(ctx) = Ctx::for_slide(&state.doc, slide) else { return };
    let ids = state.kdm().map(|session| session.model().ids());
    for target in targets {
        let Some(shape) = kition_slide_model::find_shape(&slide.shapes, *target) else { continue };
        let id = cmd::inspect::shape_tool_id(shape);
        if shape.ph.is_none() || shape.xfrm.is_some() {
            put(found, &id, &["look.click"], Some(Vec::new()));
            continue;
        }
        put(found, &id, &["look.click", "look.ph", "xfrm"], Some(Vec::new()));
        put(found, &format!("slide:{}", slide.id.0), &["layout"], Some(Vec::new()));
        let Some(ids) = ids else { continue };
        let (lp, mp) = resolve::parents(&ctx, shape);
        let layout = ctx.layout.and_then(|layout| Some((ids.layouts.object(layout.id)?, layout.shapes.as_slice())));
        let master = ids.masters.object(ctx.master.id).map(|owner| (owner, ctx.master.shapes.as_slice()));
        // The frame comes from the layout's placeholder, else the master's.
        let framed_by_layout = lp.is_some_and(|lp| lp.xfrm.is_some());
        for ((owner, shapes), parent, frame) in
            [(layout, lp, true), (master, mp, !framed_by_layout)].into_iter().filter_map(|(holder, parent, frame)| Some((holder?, parent, frame)))
        {
            for candidate in shapes {
                let Some(object) = ids.shape(owner, candidate.id) else { continue };
                if parent.is_some_and(|parent| parent.id == candidate.id) {
                    put(found, &object.to_string(), if frame { &["look.ph", "xfrm"] } else { &["look.ph"] }, Some(Vec::new()));
                    break;
                }
                if candidate.ph.is_some() {
                    put(found, &object.to_string(), &["look.ph"], Some(Vec::new()));
                }
            }
        }
    }
}
