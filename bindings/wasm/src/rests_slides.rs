//! Dependencies established by the independent slide-handler reading.

use super::*;

pub(super) fn derived(doc: &SlideDocument, requested: &[Value], names: &[String]) -> Result<Vec<kition_kit::DerivedValues>, String> {
    let state = doc.state()?;
    let mut out = Vec::new();
    let wanted = |name: &str| names.is_empty() || names.iter().any(|asked| asked == name);
    if requested.is_empty() || requested.contains(&json!("slide-file")) {
        let mut values = serde_json::Map::new();
        if wanted("commentOrder") {
            let order: serde_json::Map<String, Value> = state
                .doc
                .slides
                .iter()
                .map(|slide| (format!("slide:{}", slide.id.0), json!(slide.comments.iter().map(|thread| &thread.id).collect::<Vec<_>>())))
                .collect();
            values.insert("commentOrder".into(), Value::Object(order));
        }
        // Which slides the deck has, in order, without their content: what a write over every
        // slide walks (the content it reads of each is held on the slide).
        if wanted("slideOrder") {
            values.insert("slideOrder".into(), json!(state.doc.slides.iter().map(|slide| format!("slide:{}", slide.id.0)).collect::<Vec<_>>()));
        }
        if !values.is_empty() {
            out.push(kition_kit::DerivedValues { id: json!("slide-file"), values });
        }
    }
    if !names.is_empty() && !names.iter().any(|name| name == "shapeCount" || name == "layoutCount") {
        return Ok(out);
    }
    let mut add = |id: Value, count: usize, layouts: Option<usize>| {
        if requested.is_empty() || requested.contains(&id) {
            let mut values = serde_json::Map::new();
            if wanted("shapeCount") {
                values.insert("shapeCount".into(), json!(count));
            }
            if wanted("layoutCount")
                && let Some(count) = layouts
            {
                values.insert("layoutCount".into(), json!(count));
            }
            if !values.is_empty() {
                out.push(kition_kit::DerivedValues { id, values });
            }
        }
    };
    for slide in &state.doc.slides {
        add(json!(format!("slide:{}", slide.id.0)), slide.shapes.len(), None);
    }
    if let Some(session) = state.kdm() {
        let ids = session.model().ids();
        for master in &state.doc.masters {
            if let Some(id) = ids.masters.object(master.id) {
                add(json!(id.as_str()), master.shapes.len(), Some(master.layouts.len()));
            }
            for layout in &master.layouts {
                add(json!(format!("layout:{}", layout.identity)), layout.shapes.len(), None);
            }
        }
    }
    Ok(out)
}

/// The places a slide-family command names, which the guard follows (the objects there at `basis`
/// and the windows of the lists holding them). Their premises are [`super::rests_deck::settle`]'s.
pub(super) fn add(doc: &SlideDocument, command: &str, params: &Value, found: &mut kition_kit::Under) -> Result<(), String> {
    let Some(deck) = doc.pres() else { return Ok(()) };
    match command {
        "slide.new" => new_slide(doc, params, found)?,
        "comment.reply" | "comment.delete" | "comment.resolve" => comment_place(doc, command, params, found)?,
        "show.customShow" if params.get("delete") != Some(&json!(true)) => {
            for at in
                params.get("slides").and_then(Value::as_array).into_iter().flatten().filter_map(Value::as_u64).filter_map(|n| usize::try_from(n).ok())
            {
                if let Some(slide) = deck.slides.get(at) {
                    found.objects.push(json!(format!("slide:{}", slide.id.0)));
                    found.within.push(kition_kit::Within {
                        parent: json!("slide-file"),
                        field: "slides".into(),
                        from: at,
                        to: at.saturating_add(1),
                        ..Default::default()
                    });
                }
            }
        }
        "slide.layout" | "slide.reset" => layout_change(doc, command, params, found)?,
        _ => {}
    }
    Ok(())
}

fn comment_place(doc: &SlideDocument, command: &str, params: &Value, found: &mut kition_kit::Under) -> Result<(), String> {
    let state = doc.state()?;
    // A stable id wins over the positional spelling, just as find_thread does.
    let thread = params.get("thread").and_then(Value::as_str);
    if thread.is_some_and(|id| state.doc.slides.iter().any(|slide| slide.comments.iter().any(|comment| comment.id == id))) {
        return Ok(());
    }
    let place = match thread {
        Some(id) => id
            .strip_prefix('@')
            .and_then(|rest| rest.split_once(':'))
            .and_then(|(slide, at)| Some((slide.parse::<usize>().ok()?, at.parse::<usize>().ok()?))),
        None if command == "comment.delete" && params.get("all") == Some(&json!(true)) => None,
        None => index(params, "index").map(|at| (state.selection.slide, at)),
    };
    let Some((si, at)) = place else { return Ok(()) };
    let Some(slide) = state.doc.slides.get(si) else { return Ok(()) };
    // The slot (`~commentOrder`, held by settle) and the thread there at `basis`.
    if let Some(comment) = slide.comments.get(at) {
        found.objects.push(json!(comment.id));
    }
    if thread.is_some() {
        found.within.push(kition_kit::Within {
            parent: json!("slide-file"),
            field: "slides".into(),
            from: si,
            to: si.saturating_add(1),
            ..Default::default()
        });
    }
    Ok(())
}

fn index(params: &Value, key: &str) -> Option<usize> {
    params.get(key).and_then(Value::as_u64).and_then(|n| usize::try_from(n).ok())
}

fn slide_indexes(doc: &SlideDocument, params: &Value, selected: bool) -> Result<Vec<usize>, String> {
    if let Some(index) = index(params, "index") {
        return Ok(vec![index]);
    }
    let state = doc.state()?;
    let selected: Vec<_> = if selected { state.selection.slides.iter().filter_map(|id| state.doc.slide_index(*id)).collect() } else { Vec::new() };
    Ok(if selected.is_empty() { vec![state.selection.slide] } else { selected })
}

/// The window of a layout's shape list a slide made from it copies.
fn layout_window(state: &DocState, layout: kition_slide_model::LayoutId, found: &mut kition_kit::Under) {
    let Some((_, chosen)) = state.doc.layout(layout) else { return };
    let Some(session) = state.kdm() else { return };
    let Some(object) = session.model().ids().layouts.object(layout) else { return };
    found.within.push(kition_kit::Within {
        parent: json!(object.as_str()),
        field: "shapes".into(),
        from: 0,
        to: chosen.shapes.len(),
        ..Default::default()
    });
    // Include the end boundary so deleting the old last candidate is still a place change.
    found.within.push(kition_kit::Within {
        parent: json!(object.as_str()),
        field: "shapes".into(),
        from: chosen.shapes.len(),
        to: chosen.shapes.len(),
        ..Default::default()
    });
}

fn layout_change(doc: &SlideDocument, command: &str, params: &Value, found: &mut kition_kit::Under) -> Result<(), String> {
    let state = doc.state()?;
    let deck = &state.doc;
    let current = deck.slides.get(state.selection.slide);
    let master = current.and_then(|slide| deck.masters.iter().position(|master| master.layout(slide.layout).is_some())).unwrap_or(0);
    for index in slide_indexes(doc, params, command == "slide.layout")? {
        let Some(slide) = deck.slides.get(index) else { continue };
        let chosen = if command == "slide.reset" {
            Some(slide.layout)
        } else {
            params.get("layout").and_then(|key| cmd::slide::find_layout(deck, key, master))
        };
        if let Some(chosen) = chosen {
            layout_window(state, chosen, found);
        }
    }
    Ok(())
}

fn new_slide(doc: &SlideDocument, params: &Value, found: &mut kition_kit::Under) -> Result<(), String> {
    use kition_slide_model::{LayoutType, defaults};

    let state = doc.state()?;
    let deck = &state.doc;
    let current = deck.slides.get(state.selection.slide);
    let master = current.and_then(|slide| deck.masters.iter().position(|master| master.layout(slide.layout).is_some())).unwrap_or(0);
    let layout = match params.get("layout") {
        Some(key) => cmd::slide::find_layout(deck, key, master),
        None => {
            let layout = current.map(|slide| slide.layout);
            let title = layout.and_then(|layout| deck.layout(layout)).is_some_and(|(_, layout)| layout.kind == LayoutType::Title);
            match layout {
                Some(layout) if !title => Some(layout),
                _ => defaults::layout_of_kind(deck, LayoutType::TitleAndContent).or(layout),
            }
        }
    };
    if let Some(layout) = layout {
        layout_window(state, layout, found);
    }
    let at = index(params, "at")
        .unwrap_or_else(|| if deck.slides.is_empty() { 0 } else { state.selection.slide.saturating_add(1) })
        .min(deck.slides.len());
    if params.get("at").is_none_or(Value::is_null) {
        found.within.push(kition_kit::Within { parent: json!("slide-file"), field: "slides".to_owned(), from: at, to: at, ..Default::default() });
    }
    Ok(())
}
