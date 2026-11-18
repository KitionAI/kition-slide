//! The clipboard, duplicate and delete commands' and the poster frame's dependencies, as the
//! independent reading of their handlers says (C8-slide-read-3; cmd/edit.rs, cmd/media.rs).

use super::rests_shapes::{Holds, Scope};
use super::*;
use kition_slide_model::{Shape, ShapeKind};

pub(super) fn add(doc: &SlideDocument, command: &str, params: &Value, found: &mut kition_kit::Under) -> Result<(), String> {
    if !matches!(command, "media.posterFrame" | "edit.cut" | "edit.delete" | "edit.duplicate" | "edit.paste" | "edit.pasteText") {
        return Ok(());
    }
    let Some(scope) = Scope::of(doc, params)? else { return Ok(()) };
    let mut holds = Holds::default();
    match command {
        "media.posterFrame" => poster_frame(doc, &scope, &mut holds, params),
        "edit.cut" | "edit.delete" => delete(&scope, &mut holds, params),
        "edit.duplicate" => duplicate(&scope, &mut holds, params),
        "edit.paste" => paste(doc, &scope, &mut holds, params),
        _ => {
            let text = params.get("text").and_then(Value::as_str).map(str::to_owned).unwrap_or_else(|| doc.session.clipboard.plain.clone());
            paste_plain(&scope, &mut holds, &text);
        }
    }
    scope.finish(holds, found);
    Ok(())
}

fn names_shapes(params: &Value) -> bool {
    params.get("ids").is_some() || params.get("id").is_some_and(|id| !id.is_null())
}

/// `edit.delete` (and `edit.cut`, which copies first: the clipboard is not the deck): text being
/// edited, else selected cells, else the slides scope, else the shapes.
fn delete(scope: &Scope<'_>, holds: &mut Holds, params: &Value) {
    let named = names_shapes(params);
    let selection = &scope.selection;
    if !named && selection.text.is_some() {
        edited_text(scope, holds);
        return;
    }
    if !named && let Some((table, _, _)) = selection.cells {
        if let Some(object) = scope.find(table).and_then(|shape| scope.public(shape)) {
            holds.read(&object, &["table", "xfrm"]);
        }
        return;
    }
    let slides_scope = params.get("scope").and_then(Value::as_str) == Some("slides")
        || (selection.shapes.is_empty() && selection.text.is_none() && !selection.slides.is_empty());
    if !named && slides_scope {
        // `slide.delete {}`: the selected slides (else the current one) whole, with their shapes
        // and comment threads; the sections around them.
        let deck = scope.deck();
        let slides: Vec<_> = if selection.slides.is_empty() {
            deck.slides.get(selection.slide).into_iter().collect()
        } else {
            deck.slides.iter().filter(|slide| selection.slides.contains(&slide.id)).collect()
        };
        for slide in slides {
            holds.all(&format!("slide:{}", slide.id.0));
            kition_slide_model::walk(&slide.shapes, &mut |shape, _| holds.all(&cmd::inspect::shape_tool_id(shape)));
            for object in slide.comments.iter().filter_map(|thread| scope.ids.comments_object(&thread.id)) {
                holds.all(object.as_str());
            }
        }
        holds.read("slide-file", &["sections"]);
        return;
    }
    scope.remove(holds, &scope.targets(params));
}

/// Text inserted into or deleted from the body being edited: the body (the slide's notes when
/// editing them), refit as its shape fits its text.
fn edited_text(scope: &Scope<'_>, holds: &mut Holds) {
    let Some(text) = &scope.selection.text else { return };
    if text.notes {
        if let Some(slide) = scope.slide {
            holds.read(&format!("slide:{}", slide.id.0), &["notesBody"]);
        }
        return;
    }
    let Some(shape) = scope.find(text.shape) else { return };
    let Some(object) = scope.public(shape) else { return };
    if text.cell.is_some() {
        holds.read(&object, &["table", "xfrm"]);
        return;
    }
    holds.read(&object, &["body"]);
    // Shrink-to-fit resolves the edited shape's own placeholder/style and
    // frame before inherited values. In particular an outside fontRef edit changes the fit.
    if scope
        .ctx()
        .is_some_and(|ctx| matches!(kition_slide_model::resolve::body(&ctx, shape).autofit, Some(kition_slide_model::text::AutoFit::Shrink { .. })))
    {
        holds.read(&object, &["look.ph", "look.style", "xfrm"]);
    }
    scope.refit(holds, shape, false, "body.props");
}

/// `edit.duplicate`: the top-level targets copied in list order with their boxes (inherited for a
/// placeholder, which the copy no longer is: its text is fitted again).
fn duplicate(scope: &Scope<'_>, holds: &mut Holds, params: &Value) {
    let targets = scope.targets(params);
    let top: Vec<&Shape> = scope.shapes.iter().filter(|shape| targets.iter().any(|(id, _)| *id == shape.id)).collect();
    for (id, addressed) in &targets {
        let Some(shape) = scope.find(*id) else { continue };
        let Some(object) = scope.public(shape) else { continue };
        holds.read(&object, &[]);
        if *addressed {
            holds.at(&object, "address");
        }
        if !top.iter().any(|copied| copied.id == shape.id) {
            continue;
        }
        holds.all(&object);
        if top.len() > 1 {
            holds.everywhere(&object);
        }
        if shape.xfrm.is_none() {
            scope.inherited_box(holds, shape);
        }
        if shape.ph.is_some() && matches!(scope.selection.target, Target::Slides) {
            scope.fit_new_text(holds);
        }
    }
}

/// `edit.paste`: other plain text as a text box (or into the edited body); the clipboard's
/// formatted text into the edited body; its slides after the current one; its shapes onto the
/// tree (offset when the tree still has a shape of the same number); else its plain text.
fn paste(doc: &SlideDocument, scope: &Scope<'_>, holds: &mut Holds, params: &Value) {
    let clipboard = &doc.session.clipboard;
    if let Some(text) = params.get("text").and_then(Value::as_str)
        && text != clipboard.plain
    {
        paste_plain(scope, holds, text);
        return;
    }
    if scope.selection.text.is_some() {
        edited_text(scope, holds);
        return;
    }
    let media = |holds: &mut Holds| {
        for item in &clipboard.media {
            if let Some(object) = scope.deck().media(item.id).and(scope.ids.media.object(item.id)) {
                holds.read(object.as_str(), &["blob"]);
            }
        }
    };
    if !clipboard.slides.is_empty() {
        media(holds);
        scope.pasted_slides(holds, &clipboard.slides);
        return;
    }
    if !clipboard.shapes.is_empty() {
        media(holds);
        for copied in &clipboard.shapes {
            if let Some(object) = scope.find(copied.id).and_then(|shape| scope.public(shape)) {
                holds.read(&object, &[]);
            }
        }
        return;
    }
    if !clipboard.plain.is_empty() {
        paste_plain(scope, holds, &clipboard.plain);
    }
}

/// Plain text pasted: into the edited body, else a new text box (`insert.textBox` at a box from
/// the slide size, named by the shape count, fitted to its text).
fn paste_plain(scope: &Scope<'_>, holds: &mut Holds, _text: &str) {
    if scope.selection.text.is_some() {
        edited_text(scope, holds);
        return;
    }
    scope.create(holds);
    holds.read("slide-file", &["props.slideSize"]);
    scope.fit_new_text(holds);
}

/// `media.posterFrame`: the video named (else the selected one, else the only media on the
/// slide): its clip (replaced whole) and media, its bytes, and the media holding the bytes of the
/// new poster frame (reused).
fn poster_frame(doc: &SlideDocument, scope: &Scope<'_>, holds: &mut Holds, params: &Value) {
    let targets = scope.targets(params);
    let media = |shape: &Shape| matches!(shape.kind, ShapeKind::Media(_));
    let mut target = targets.iter().find_map(|(id, addressed)| scope.find(*id).filter(|shape| media(shape)).map(|shape| (shape, *addressed)));
    if target.is_none() {
        if names_shapes(params) {
            return;
        }
        // The only media on the slide: every shape of it is compared.
        holds.read(&scope.tree, &["shapes"]);
        let mut all = scope.shapes.iter().filter(|shape| media(shape));
        target = match (all.next(), all.next()) {
            (Some(only), None) => Some((only, false)),
            _ => None,
        };
    }
    let Some((shape, addressed)) = target else { return };
    let ShapeKind::Media(clip) = &shape.kind else { return };
    let Some(object) = scope.public(shape) else { return };
    holds.read(&object, &["look.kind", "media"]);
    if addressed {
        holds.at(&object, "address");
    }
    if shape.xfrm.is_none() {
        holds.read(&object, &["xfrm"]);
        scope.inherited_box(holds, shape);
    }
    let Some(item) = scope.deck().media(clip.media) else { return };
    if let Some(bytes) = scope.ids.media.object(clip.media) {
        holds.read(bytes.as_str(), &["blob"]);
    }
    if !clip.video {
        return;
    }
    let ms = params
        .get("ms")
        .and_then(Value::as_f64)
        .filter(|ms| ms.is_finite() && *ms >= 0.0)
        .map(|ms| ms as u64)
        .or_else(|| doc.session.media_status.get(&shape.id).map(|status| status.position_ms))
        .unwrap_or(0);
    if let Ok(png) = kition_slide_engine::media::poster_png(&item.data, ms as f64 / 1000.0) {
        scope.same_media(holds, &png);
    }
}
