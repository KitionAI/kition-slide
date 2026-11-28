use super::*;
use kition_kit::testing::check_contract;

#[test]
fn master_view_draws_and_edits_the_layout_instead_of_the_current_slide() {
    let mut doc = SlideDocument::create("Master.pptx", 960, 540).unwrap();
    doc.set_single(true);
    doc.execute("shape.insert", json!({ "preset": "rect", "rect": [100, 100, 80, 80] })).unwrap();
    doc.execute("shape.fill", json!({ "color": "#0000ff" })).unwrap();
    let before = pixel(&doc, 180.0, 180.0);
    doc.execute("view.slideMaster", json!({})).unwrap();
    assert_eq!(doc.inspect().unwrap()["target"]["kind"], "layout");
    doc.execute("shape.insert", json!({ "preset": "rect", "rect": [100, 100, 80, 80] })).unwrap();
    doc.execute("shape.fill", json!({ "color": "#ff0000" })).unwrap();
    let master_pixel = pixel(&doc, 180.0, 180.0);
    assert!(master_pixel[0] > 200 && master_pixel[1] < 50, "the master's rectangle is visible");
    assert_eq!(doc.inspect().unwrap()["selectedKinds"][0], "shape");
    assert!(!doc.inspect().unwrap()["masters"].as_array().unwrap().is_empty());
    doc.execute("view.closeMaster", json!({})).unwrap();
    assert_eq!(doc.inspect().unwrap()["target"]["kind"], "slides");
    assert_eq!(pixel(&doc, 180.0, 180.0), before);
    doc.execute("view.slideMaster", json!({ "master": 0, "layout": null })).unwrap();
    assert_eq!(doc.inspect().unwrap()["target"]["kind"], "master");
    doc.execute("shape.insert", json!({ "preset": "rect", "rect": [100, 100, 80, 80] })).unwrap();
    doc.execute("shape.fill", json!({ "color": "#00ff00" })).unwrap();
    let master_pixel = pixel(&doc, 180.0, 180.0);
    assert!(master_pixel[1] > 200 && master_pixel[0] < 50);
}

#[test]
fn theme_previews_never_change_the_open_document_or_its_history() {
    let doc = SlideDocument::create("Themes.pptx", 960, 540).unwrap();
    let before = doc.save(Some("Themes.pptx")).unwrap();
    let info = doc.inspect().unwrap();
    let theme = kition_slide_model::theme::builtin_themes().first().unwrap().name.clone();
    assert_eq!(doc.render_theme(&theme, 0, 160).unwrap().len(), 160 * 90 * 4);
    assert_eq!(doc.save(Some("Themes.pptx")).unwrap(), before);
    assert_eq!(doc.inspect().unwrap(), info);
    assert!(doc.render_theme("missing-theme", 0, 160).is_err());
    assert!(doc.media_data(99999).is_err());
}

#[test]
fn the_native_pen_variants_keep_their_color_and_tool_identity() {
    let mut doc = SlideDocument::create("Pens.pptx", 960, 540).unwrap();
    for id in ["redPen", "bluePen", "pencil"] {
        doc.select_tool(id, 1.0).unwrap();
        assert_eq!(doc.tool(), id);
        assert!(doc.tools().iter().any(|tool| tool.id == id));
    }
}

/// Two slides: a title slide and a blank slide with a red rectangle.
fn fixture() -> Vec<u8> {
    let mut doc = SlideDocument::create("Slide.kslide", 0, 0).unwrap();
    doc.execute("slide.new", json!({})).unwrap();
    doc.execute("shape.insert", json!({ "preset": "rect", "rect": [380, 170, 200, 200] })).unwrap();
    doc.save(Some("Slide.kslide")).unwrap()
}

#[test]
fn the_summary_is_the_decks_slide_size_not_the_canvas() {
    let mut doc = SlideDocument::create("Deck.pptx", 960, 540).unwrap();
    assert_eq!((doc.width(), doc.height()), (1040, 620), "the canvas adds the margins");
    assert_eq!(doc.summary(), "1 slide, 960 × 540 pt");
    doc.execute("slide.new", json!({})).unwrap();
    assert_eq!(doc.summary(), "2 slides, 960 × 540 pt");
}

#[test]
fn a_missing_font_without_a_metric_twin_is_a_warning() {
    let mut doc = SlideDocument::create("Deck.pptx", 960, 540).unwrap();
    doc.execute("theme.fonts", json!({"major": "No Such Family 123", "minor": "No Such Family 123"})).unwrap();
    let found = doc.check();
    let font = found.iter().find(|d| d.code == "slide.font-substituted").expect("the missing font is said");
    assert_eq!(font.severity, kition_kit::diagnostics::Severity::Warning, "{:?}", font.message);
    assert!(font.message.contains("use an installed family"), "{}", font.message);
}

#[test]
fn keeps_the_kit_contract() {
    check_contract::<SlideDocument>(&fixture(), "Slide.kslide", |doc| {
        doc.execute("textBox.insert", json!({ "rect": [100, 100, 300, 80], "text": "Hello" })).map(|_| ())
    })
    .unwrap();
}

fn event(action: PointerAction, x: f64, y: f64) -> ToolPointer {
    ToolPointer { action, x, y, mods: ToolMods::default(), pressure: 1.0, zoom: 1.0 }
}

fn drag(doc: &mut SlideDocument, from: (f64, f64), to: (f64, f64)) -> ToolResponse {
    doc.pointer(&event(PointerAction::Down, from.0, from.1)).unwrap();
    for step in 1..=4 {
        let t = f64::from(step) / 4.0;
        doc.pointer(&event(PointerAction::Drag, from.0 + (to.0 - from.0) * t, from.1 + (to.1 - from.1) * t)).unwrap();
    }
    doc.pointer(&event(PointerAction::Up, to.0, to.1)).unwrap()
}

fn click(doc: &mut SlideDocument, x: f64, y: f64) -> ToolResponse {
    doc.pointer(&event(PointerAction::Down, x, y)).unwrap();
    doc.pointer(&event(PointerAction::Up, x, y)).unwrap()
}

fn pixel(doc: &SlideDocument, x: f64, y: f64) -> [u8; 4] {
    let rgba = doc.render(x, y, 1.0, 1, 1).unwrap();
    [rgba[0], rgba[1], rgba[2], rgba[3]]
}

#[test]
fn stacks_the_slides_and_draws_them_in_place() {
    let doc = SlideDocument::open("Slide.kslide", &fixture()).unwrap();
    assert_eq!((doc.width(), doc.height()), (1040, 40 + 540 + 40 + 540 + 40));
    // The second slide's red square, and the gap between the slides.
    let square = pixel(&doc, 40.0 + 480.0, 620.0 + 270.0);
    let paper = pixel(&doc, 40.0 + 20.0, 620.0 + 20.0);
    assert_eq!((square[3], paper[3]), (255, 255), "slides are opaque");
    assert_ne!(square, paper, "the square is drawn on the second slide");
    assert_eq!(pixel(&doc, 500.0, 600.0), PASTEBOARD, "the gap is the pasteboard");
    // Zoomed in, the same place is drawn at that scale, not enlarged.
    let sharp = doc.render(519.0, 889.0, 4.0, 8, 8).unwrap();
    assert_eq!(sharp.len(), 8 * 8 * 4);
}

#[test]
fn a_click_on_another_slide_makes_it_current_and_selects_there() {
    let mut doc = SlideDocument::open("Slide.kslide", &fixture()).unwrap();
    assert_eq!(doc.inspect().unwrap()["slide"], 0);
    let response = click(&mut doc, 40.0 + 480.0, 620.0 + 270.0);
    assert_eq!(doc.inspect().unwrap()["slide"], 1);
    assert_eq!(doc.inspect().unwrap()["selection"], 1);
    assert!(response.busy, "editing keys go to the engine while a shape is selected");
    assert!(response.overlays.iter().any(|overlay| matches!(overlay, ToolOverlay::Anchor { .. })), "the selection shows its handles");
    // Delete removes it; undo brings it back.
    doc.key("Delete", ToolMods::default(), 1.0).unwrap();
    assert_eq!(doc.inspect().unwrap()["selection"], 0);
    assert!(doc.dirty());
    doc.execute("edit.undo", json!({})).unwrap();
    assert!(!doc.dirty());
}

#[test]
fn draws_a_shape_then_returns_to_select_and_types_into_it() {
    let mut doc = SlideDocument::open("Slide.kslide", &fixture()).unwrap();
    doc.select_tool("ellipse", 1.0).unwrap();
    assert_eq!(doc.tool(), "ellipse");
    let drawn = drag(&mut doc, (100.0, 100.0), (300.0, 250.0));
    assert_eq!(doc.tool(), "select");
    assert!(drawn.requests.iter().any(|request| matches!(request, ToolRequest::SwitchTool { id } if id == "select")));
    assert_eq!(doc.inspect().unwrap()["selection"], 1);
    assert!(drawn.wants_text, "typing goes to the one selected shape");
    // Typing on a selected shape gives it that text.
    let typed = KitTools::text(&mut doc, "Hi", 1.0).unwrap();
    assert!(typed.wants_text);
    assert!(typed.overlays.iter().any(|overlay| matches!(overlay, ToolOverlay::Line { .. })), "the caret");
    doc.key("Escape", ToolMods::default(), 1.0).unwrap();
    assert_eq!(doc.inspect().unwrap()["editingText"], false);
    // PowerPoint round trip.
    let pptx = doc.save(Some("Slide.pptx")).unwrap();
    assert_eq!(&pptx[..2], b"PK");
    let reopened = SlideDocument::open("Slide.pptx", &pptx).unwrap();
    assert_eq!(reopened.inspect().unwrap()["slides"].as_array().map(Vec::len), Some(2));
}

#[test]
fn creates_a_document_with_one_slide() {
    let doc = SlideDocument::create("New.kslide", 0, 0).unwrap();
    assert!(!doc.dirty());
    assert_eq!(doc.inspect().unwrap()["slides"].as_array().map(Vec::len), Some(1));
    assert!(doc.disabled_commands().contains(&"edit.undo".to_owned()));
    assert!(doc.tools().iter().any(|tool| tool.id == "textBox"));
}

#[test]
fn the_single_slide_view_shows_the_current_slide_and_thumbnails_render() {
    let mut doc = SlideDocument::open("Slide.kslide", &fixture()).unwrap();
    let stacked = doc.height();
    doc.set_single(true);
    assert_eq!(doc.height(), 40 + 540 + 40, "one slide");
    assert!(stacked > doc.height());
    let first = doc.render(40.0 + 480.0, 40.0 + 270.0, 1.0, 1, 1).unwrap();
    // The second slide (the red square) becomes the canvas when it is current.
    doc.execute("slide.go", json!({ "index": 1 })).unwrap();
    let second = doc.render(40.0 + 480.0, 40.0 + 270.0, 1.0, 1, 1).unwrap();
    assert_ne!(first, second);
    assert_eq!(doc.inspect().unwrap()["slides"][0]["area"], Value::Null);
    assert!(doc.inspect().unwrap()["slides"][1]["area"].is_object());
    // A click lands on the current slide.
    click(&mut doc, 40.0 + 480.0, 40.0 + 270.0);
    assert_eq!(doc.inspect().unwrap()["selection"], 1);
    doc.set_backdrop("#f6f5f4").unwrap();
    assert_eq!(doc.render(5.0, 5.0, 1.0, 1, 1).unwrap(), [0xf6, 0xf5, 0xf4, 255], "the editor's canvas color");
    assert!(doc.set_backdrop("purple").is_err());
    let thumbnail = doc.render_slide(1, 192).unwrap();
    assert_eq!(thumbnail.len(), 192 * 108 * 4);
    assert!(doc.render_slide(5, 192).is_err());
    // The clean render for measurement: the requested size exactly, straight RGBA, no editor
    // marks (an empty placeholder's prompt and outline are absent, so blank slides are white).
    let clean = doc.render_slide_clean(1, 200, 100, None).unwrap();
    assert_eq!(clean.len(), 200 * 100 * 4);
    let centre = &clean[(50 * 200 + 100) * 4..][..4];
    assert!(centre[3] == 255 && centre[..3] != [255, 255, 255], "the square: {centre:?}");
    assert!(doc.render_slide_clean(5, 200, 100, None).is_err());
}

#[test]
fn takes_a_font_for_text_its_own_faces_do_not_cover() {
    assert_eq!(add_font(b"not a font"), 0);
    // Hack Regular: a dependency of crates/support/fonts (docs/attribution.md) that crates/support/fonts never loads
    // as a bundled face, unlike kition-fonts' families, so this is deterministically new
    // regardless of what KITION_FONTS_DIR points at.
    let bytes = epaint_default_fonts::HACK_REGULAR;
    assert!(add_font(bytes) > 0);
    // A face it already has is not added twice.
    assert_eq!(add_font(bytes), 0);
}

#[test]
fn the_catalog_flags_as_mutating_exactly_what_changes_the_slide_file() {
    let doc = SlideDocument::create("Flags.kslide", 0, 0).unwrap();
    let mutates = |id: &str| doc.commands().into_iter().find(|command| command.id == id).unwrap_or_else(|| panic!("no {id}")).mutates;
    for id in ["slide.new", "shape.insert", "edit.undo", "edit.redo", "file.new", "file.open", "file.revert", "file.save"] {
        assert!(mutates(id), "{id} changes the slide");
    }
    for id in ["slide.next", "edit.selectAll", "edit.copy", "view.slideMaster", "file.export", "file.exportBytes", "file.saveBytes", "show.fromStart"]
    {
        assert!(!mutates(id), "{id} does not change the slide");
    }
}

#[test]
fn commands_flagged_as_not_mutating_never_commit_to_the_replica() {
    let bytes = fixture();
    let commands = SlideDocument::open("Slide.kslide", &bytes).unwrap().commands();
    // Commands that write files are left out: the test must not write any.
    for command in commands.iter().filter(|command| !command.mutates && (!command.id.starts_with("file.") || command.id == "file.inspect")) {
        let mut doc = SlideDocument::open("Slide.kslide", &bytes).unwrap();
        doc.execute("edit.selectAll", json!({})).unwrap();
        let before = doc.version().unwrap();
        let _ = doc.execute(&command.id, json!({}));
        assert_eq!(doc.version().unwrap(), before, "{} committed to the replica", command.id);
    }
}

#[test]
fn a_long_stacked_slide_file_draws_and_keeps_only_the_slides_in_view() {
    let mut doc = SlideDocument::create("Long.pptx", 960, 540).unwrap();
    while doc.slide_count() < 120 {
        doc.execute("slide.new", json!({})).unwrap();
    }
    let cached = |doc: &SlideDocument| {
        let mut indexes: Vec<usize> = doc.cache.borrow().keys().copied().collect();
        indexes.sort_unstable();
        indexes
    };
    let pixels_of = |doc: &SlideDocument, index: usize| doc.cache.borrow().get(&index).map(|cached| cached.image.pixels.as_ptr() as usize);
    // The top of the slide file at a quarter of its size: only the slides it shows are drawn.
    doc.render(0.0, 0.0, 0.25, 300, 600).unwrap();
    let top = cached(&doc);
    assert!(!top.is_empty() && top.len() < 10, "only the slides in view: {top:?}");
    assert_eq!(top[0], 0);
    let first = pixels_of(&doc, 0);
    // The end of the slide, then the top again: the top's images were kept, not drawn again.
    let (_, h) = doc.slide_size();
    let end = doc.area(117).y;
    doc.render(0.0, end, 0.25, 300, 600).unwrap();
    assert!(cached(&doc).contains(&118), "the end of the slide is drawn: {:?}", cached(&doc));
    assert!(cached(&doc).contains(&0), "the top stays cached");
    doc.render(0.0, 0.0, 0.25, 300, 600).unwrap();
    assert_eq!(pixels_of(&doc, 0), first, "slide 1 is not drawn again");
    assert!(h > 0.0);
}

#[test]
fn eviction_drops_the_least_recently_drawn_and_never_the_frame_being_drawn() {
    let image = |bytes: usize| Image { width: 1, height: 1, pixels: vec![0; bytes] };
    let mut cache: HashMap<usize, Cached> = HashMap::new();
    for (index, used) in [(0, 1), (1, 2), (2, 3), (3, 3)] {
        cache.insert(index, Cached { key: (index, 0, 0, None), image: image(100), used });
    }
    evict(&mut cache, 3, 250);
    let mut left: Vec<usize> = cache.keys().copied().collect();
    left.sort_unstable();
    assert_eq!(left, vec![2, 3], "the oldest go first, until the budget holds");
    // The current frame keeps what it drew even over the budget.
    evict(&mut cache, 3, 50);
    assert_eq!(cache.len(), 2);
}

/// `onSlide` (from 1) names the slide a command's `ids` are on: it goes there first, and a number
/// past the deck is an error, never another slide; commands that take `ids` say so in their schema.
#[test]
fn on_slide_names_the_slide_the_ids_are_on() {
    let mut doc = SlideDocument::create("Deck.kslide", 960, 540).unwrap();
    doc.execute("slide.new", json!({})).unwrap();
    let id = doc.execute("shape.insert", json!({ "preset": "rect", "rect": [100, 100, 80, 80] })).unwrap()["id"].as_u64().unwrap();
    doc.execute("slide.go", json!({ "index": 0 })).unwrap();
    assert!(doc.execute("edit.delete", json!({ "ids": [id] })).is_err(), "slide 1 has no such shape");
    let error = doc.execute("edit.delete", json!({ "ids": [id], "onSlide": 9 })).unwrap_err();
    assert!(error.contains("onSlide"), "{error}");
    doc.execute("edit.delete", json!({ "ids": [id], "onSlide": 2 })).unwrap();
    let delete = doc.commands().into_iter().find(|command| command.id == "edit.delete").unwrap();
    assert!(delete.schema.as_ref().is_some_and(|schema| schema.get("onSlide").is_some()), "{:?}", delete.schema);
    let commands = doc.commands();
    let has = |id: &str| {
        commands
            .iter()
            .find(|command| command.id == id)
            .and_then(|command| command.schema.as_ref())
            .is_some_and(|schema| schema.get("onSlide").is_some())
    };
    assert!(has("text.set"), "a single shape `id` too");
    assert!(!has("slide.delete"), "slide ids are not on a slide");
    let comment = commands.iter().find(|command| command.id == "comment.add").and_then(|command| command.params.clone()).unwrap();
    let arrow = comment.find('→').unwrap_or(comment.len());
    assert!(comment[..arrow].contains("onSlide"), "onSlide is a parameter, not part of the result: {comment}");
}

/// The selection frame, handles' line and text highlight are drawn in the slide kit's own brand
/// (`ui.brand.light` in `kit.json`), not the host's purple; the highlight keeps its alpha.
#[test]
fn editor_chrome_is_drawn_in_the_kits_brand() {
    let manifest: Value = serde_json::from_str(include_str!("../../../kit.json")).unwrap();
    let brand = manifest["ui"]["brand"]["light"].as_str().unwrap().trim_start_matches('#');
    let rgb: Vec<u8> = (0..3).map(|i| u8::from_str_radix(&brand[i * 2..i * 2 + 2], 16).unwrap()).collect();
    assert_eq!(ACCENT, [rgb[0], rgb[1], rgb[2], 255]);
    assert_eq!(TEXT_SELECTION, [rgb[0], rgb[1], rgb[2], 64]);
    let mut doc = SlideDocument::create("Brand.kslide", 0, 0).unwrap();
    doc.set_single(true);
    doc.execute("shape.insert", json!({ "preset": "rect", "rect": [100, 100, 200, 100] })).unwrap();
    let shown = KitTools::state(&mut doc, 0.0, 0.0, ToolMods::default(), 1.0);
    assert!(shown.overlays.iter().any(|overlay| matches!(overlay, ToolOverlay::Path { color, .. } if *color == ACCENT)), "the frame");
    assert!(!shown.overlays.iter().any(|overlay| matches!(overlay,
        ToolOverlay::Path { color, .. } | ToolOverlay::Line { color, .. } | ToolOverlay::Anchor { color, .. } | ToolOverlay::Highlight { color, .. }
            if color[..3] == [86, 69, 212])));
}

/// A text box placed in a single-slide view at (100, 100) slide points, 300 × 80, deselected:
/// the canvas point of its first characters.
fn text_box_doc() -> (SlideDocument, (f64, f64)) {
    let mut doc = SlideDocument::create("Cursor.kslide", 0, 0).unwrap();
    doc.set_single(true);
    doc.execute("textBox.insert", json!({ "rect": [100, 100, 300, 80], "text": "alpha beta gamma delta" })).unwrap();
    doc.key("Escape", ToolMods::default(), 1.0).unwrap();
    click(&mut doc, MARGIN + 700.0, MARGIN + 450.0);
    assert_eq!(doc.inspect().unwrap()["selection"], 0);
    (doc, (MARGIN + 112.0, MARGIN + 112.0))
}

/// The pointer is the I-beam over text a click puts the caret in (a text box, the text being
/// edited, the words of a selected shape) and not elsewhere: a shape shows move, selected or not
/// when it has no text (plan 0B, walkthrough C3), and the empty slide keeps the arrow.
#[test]
fn the_pointer_is_the_text_cursor_over_editable_text_only() {
    let (mut doc, at) = text_box_doc();
    let moved = |doc: &mut SlideDocument, (x, y): (f64, f64)| doc.pointer(&event(PointerAction::Move, x, y)).unwrap().cursor;
    assert_eq!(moved(&mut doc, at), "text", "a text box takes the caret on a click");
    assert_eq!(moved(&mut doc, (MARGIN + 700.0, MARGIN + 450.0)), "default", "the empty slide");
    doc.execute("shape.insert", json!({ "preset": "rect", "rect": [500, 300, 100, 100] })).unwrap();
    doc.key("Escape", ToolMods::default(), 1.0).unwrap();
    click(&mut doc, MARGIN + 700.0, MARGIN + 450.0);
    assert_eq!(moved(&mut doc, (MARGIN + 550.0, MARGIN + 350.0)), "move", "an unselected shape is selected and moved, not typed in");
    click(&mut doc, MARGIN + 550.0, MARGIN + 350.0);
    assert_eq!(moved(&mut doc, (MARGIN + 550.0, MARGIN + 350.0)), "move", "a selected shape without text has no text to point at");
    doc.execute("shape.insert", json!({ "preset": "rect", "rect": [500, 100, 200, 100], "text": "Words" })).unwrap();
    click(&mut doc, MARGIN + 700.0, MARGIN + 450.0);
    click(&mut doc, MARGIN + 520.0, MARGIN + 110.0);
    assert_eq!(moved(&mut doc, (MARGIN + 600.0, MARGIN + 150.0)), "text", "the words of the selected shape take the caret");
    doc.key("Escape", ToolMods::default(), 1.0).unwrap();
    // While its text is edited: the I-beam inside the box, the arrow far from it.
    click(&mut doc, at.0, at.1);
    click(&mut doc, at.0, at.1);
    assert_eq!(doc.inspect().unwrap()["editingText"], true);
    assert_eq!(moved(&mut doc, at), "text");
    assert_eq!(moved(&mut doc, (MARGIN + 700.0, MARGIN + 450.0)), "default", "away from the text being edited");
    doc.select_tool("rect", 1.0).unwrap();
    assert_eq!(moved(&mut doc, at), "crosshair", "a drawing tool keeps its cross");
}

/// A drag that selects characters changes the selection (and so repaints the view) only when
/// the caret moves to another character.
#[test]
fn a_text_selecting_drag_repaints_only_when_the_caret_moves() {
    let (mut doc, (x, y)) = text_box_doc();
    click(&mut doc, x, y);
    assert_eq!(doc.inspect().unwrap()["editingText"], true);
    doc.pointer(&event(PointerAction::Down, x, y)).unwrap();
    let held = doc.pointer(&event(PointerAction::Drag, x + 30.0, y)).unwrap();
    assert_eq!(held.cursor, "text", "selecting keeps the text cursor");
    let before = doc.revision();
    doc.pointer(&event(PointerAction::Drag, x + 30.2, y)).unwrap();
    assert_eq!(doc.revision(), before, "the caret stayed put");
    let far = doc.pointer(&event(PointerAction::Drag, x + 120.0, y)).unwrap();
    assert!(doc.revision() > before, "the caret moved");
    assert!(far.overlays.iter().any(|overlay| matches!(overlay, ToolOverlay::Highlight { .. })), "the selection is shown");
    doc.pointer(&event(PointerAction::Up, x + 120.0, y)).unwrap();
}

/// The caret spans the line of text it is in, wherever the box anchors its text (top, middle,
/// bottom): the drawn letters lie within the caret's height.
#[test]
fn the_caret_lines_up_with_the_text_at_every_vertical_anchor() {
    for anchor in ["top", "middle", "bottom"] {
        let mut doc = SlideDocument::create("Caret.kslide", 0, 0).unwrap();
        doc.set_single(true);
        doc.execute("shape.insert", json!({ "preset": "rect", "rect": [100, 100, 300, 200] })).unwrap();
        KitTools::text(&mut doc, "HHHH", 1.0).unwrap();
        doc.execute("format.anchor", json!({ "anchor": anchor })).unwrap();
        let shown = KitTools::state(&mut doc, 0.0, 0.0, ToolMods::default(), 1.0);
        let (top, bottom) = shown
            .overlays
            .iter()
            .find_map(|overlay| match overlay {
                ToolOverlay::Line { ay, by, color, .. } if *color == CARET => Some((ay.min(*by), ay.max(*by))),
                _ => None,
            })
            .unwrap();
        // The rows where the left of the box (where the text is) differs from its right (bare).
        let (x0, y0, w, h) = (MARGIN + 100.0, MARGIN + 100.0, 300u32, 200u32);
        let px = doc.render(x0, y0, 1.0, w, h).unwrap();
        let inked: Vec<f64> = (5..h - 5)
            .filter(|&row| {
                let bare = ((row * w + w - 20) * 4) as usize;
                (8..120).any(|col| {
                    let i = ((row * w + col) * 4) as usize;
                    (0..3).any(|c| (i32::from(px[i + c]) - i32::from(px[bare + c])).abs() > 60)
                })
            })
            .map(|row| y0 + f64::from(row))
            .collect();
        let (first, last) = (inked[0], inked[inked.len() - 1]);
        assert!(top <= first && last <= bottom, "{anchor}: caret {top}..{bottom}, text {first}..{last}");
        assert!((first - top) < 8.0 && (bottom - last) < 8.0, "{anchor}: the caret hugs the line");
    }
}

#[test]
fn a_right_click_chooses_the_menu_in_the_engine_and_opens_it_where_it_was_pressed() {
    let mut doc = SlideDocument::create("Menu.kslide", 960, 540).unwrap();
    doc.execute("slide.new", json!({ "layout": "blank" })).unwrap();
    let id = doc.execute("shape.insert", json!({ "preset": "rect", "rect": [100, 100, 200, 100] })).unwrap()["id"].as_u64().unwrap();
    doc.execute("edit.deselectAll", json!({})).unwrap();
    // The slides stacked: the second one's area, below the first.
    let area = doc.area(1);
    let answer = doc.context_menu(Some((area.x + 150.0, area.y + 120.0)), 1.0).unwrap();
    assert_eq!(answer["menu"], "objects", "{answer}");
    assert_eq!(answer["ids"], json!([id]));
    assert_eq!(answer["at"], json!([area.x + 150.0, area.y + 120.0]));
    // Shift+F10: the selection as it is, the menu in the middle of it.
    let answer = doc.context_menu(None, 1.0).unwrap();
    assert_eq!(answer["ids"], json!([id]));
    assert_eq!(answer["at"], json!([area.x + 200.0, area.y + 150.0]));
    // A right-click on the first slide makes it the current one, with nothing selected there.
    let first = doc.area(0);
    let answer = doc.context_menu(Some((first.x + 900.0, first.y + 500.0)), 1.0).unwrap();
    assert_eq!(answer["menu"], "slide", "{answer}");
    assert_eq!(doc.inspect().unwrap()["slide"], 0);
}

#[test]
fn the_floating_toolbar_follows_the_selection() {
    // Plan stage 6B (WPS W-J1, W-A13): selected words have the text toolbar, over their highlight;
    // two or more objects the align bar, over their bounds; one object, a caret, nothing.
    let mut doc = SlideDocument::create("Bar.kslide", 960, 540).unwrap();
    doc.execute("slide.new", json!({ "layout": "blank" })).unwrap();
    let area = doc.area(1);
    let a = doc.execute("shape.insert", json!({ "preset": "rect", "rect": [100, 100, 200, 100], "text": "Hello world" })).unwrap()["id"]
        .as_u64()
        .unwrap();
    assert_eq!(doc.floating_bar(), None, "one object has no bar beside it");
    doc.execute("text.edit", json!({ "id": a, "end": true })).unwrap();
    assert_eq!(doc.floating_bar(), None, "a caret has none");
    doc.execute("text.selectWord", json!({ "at": [0, 8] })).unwrap();
    let bar = doc.floating_bar().unwrap();
    assert_eq!(bar["kind"], "text", "{bar}");
    let b: Vec<f64> = bar["box"].as_array().unwrap().iter().map(|v| v.as_f64().unwrap()).collect();
    assert!(b[0] < b[2] && b[1] < b[3], "{bar}");
    assert!(b[0] >= area.x + 100.0 && b[2] <= area.x + 300.0 && b[1] >= area.y + 100.0 && b[3] <= area.y + 200.0, "inside the shape: {bar}");
    doc.execute("edit.deselectAll", json!({})).unwrap();
    let c = doc.execute("shape.insert", json!({ "preset": "ellipse", "rect": [400, 300, 100, 50] })).unwrap()["id"].as_u64().unwrap();
    doc.execute("edit.select", json!({ "ids": [a, c] })).unwrap();
    let bar = doc.floating_bar().unwrap();
    assert_eq!(bar["kind"], "objects", "{bar}");
    assert_eq!(bar["box"], json!([area.x + 100.0, area.y + 100.0, area.x + 500.0, area.y + 350.0]));
    assert_eq!(bar["ids"], json!([a, c]));
}

/// The stamp of each slide in `stamps` (`[id, stamp, …]`), in deck order.
fn stamps_of(stamps: &[u32]) -> Vec<u32> {
    stamps.chunks(2).map(|pair| pair[1]).collect()
}

#[test]
fn a_thumbnails_stamp_changes_only_when_its_slide_does() {
    // Plan stage 7 (walkthrough B11): the slide panel draws again only the thumbnails that changed.
    let mut doc = SlideDocument::create("Stamps.kslide", 960, 540).unwrap();
    doc.execute("slide.new", json!({ "layout": "blank" })).unwrap();
    doc.execute("slide.new", json!({ "layout": "blank" })).unwrap();
    let first = doc.slide_stamps();
    assert_eq!(first.len(), 6, "an id and a stamp per slide");
    assert_eq!(doc.slide_stamps(), first, "nothing changed, nothing to draw");
    // A shape on the current (third) slide: its stamp alone changes.
    doc.execute("shape.insert", json!({ "preset": "rect", "rect": [100, 100, 80, 80] })).unwrap();
    let edited = doc.slide_stamps();
    let (before, after) = (stamps_of(&first), stamps_of(&edited));
    assert_eq!((before[0], before[1]), (after[0], after[1]));
    assert_ne!(before[2], after[2]);
    // Selecting changes no picture.
    doc.execute("slide.go", json!({ "index": 0 })).unwrap();
    assert_eq!(doc.slide_stamps(), edited);
    // A theme changes every slide's picture.
    let theme = kition_slide_model::theme::builtin_themes().get(1).unwrap().name.clone();
    doc.execute("theme.set", json!({ "name": theme })).unwrap();
    let themed = stamps_of(&doc.slide_stamps());
    assert!(themed.iter().zip(&after).all(|(a, b)| a != b), "{themed:?} {after:?}");
}

#[test]
fn the_slide_panel_selection_is_in_the_editor_state_and_thumbnails_have_no_prompts() {
    let mut doc = SlideDocument::create("Panel.kslide", 960, 540).unwrap();
    doc.execute("slide.new", json!({})).unwrap();
    doc.execute("slide.new", json!({})).unwrap();
    assert_eq!(doc.inspect().unwrap()["selectedSlides"], json!([2]), "the current slide alone");
    doc.execute("slide.selectSlides", json!({ "indices": [2, 0] })).unwrap();
    assert_eq!(doc.inspect().unwrap()["selectedSlides"], json!([0, 2]), "in deck order");
    assert_eq!(doc.inspect().unwrap()["slide"], 0, "the last one chosen is current");
    // The new slides' placeholders are empty: their prompts and outlines are the canvas's marks,
    // not the thumbnail's (walkthrough B12), so the thumbnail is one color.
    let thumbnail = doc.render_slide(1, 192).unwrap();
    let first = &thumbnail[..4];
    assert!(thumbnail.chunks(4).all(|pixel| pixel == first), "an empty slide's thumbnail shows no prompt");
}

/// Plan stage 9 items 1–3 (walkthrough F4; WPS W-E6, W-E10; case
/// regressions/issue-hand-f4-preset-drag): every preset in the gallery and the freeform tools are
/// drawing tools, each preset has an icon drawn from its own geometry, and a shape tool locks.
#[test]
fn every_gallery_shape_is_a_tool_with_an_icon_and_shape_tools_lock() {
    let mut doc = SlideDocument::create("Gallery.kslide", 960, 540).unwrap();
    let icons = crate::gallery::preset_icons(20.0);
    for info in kition_slide_geom::preset::CATALOG {
        assert!(tool_kind(info.name).is_some(), "{} is a tool", info.name);
        let paths = icons[info.name]["paths"].as_array().unwrap_or_else(|| panic!("{} has an icon", info.name));
        let view = icons[info.name]["view"].as_array().unwrap();
        assert!(view[2].as_f64().unwrap() >= 20.0 && view[2] == view[3], "{} is seen whole, square", info.name);
        assert!(paths.iter().any(|path| path["d"].as_str().is_some_and(|d| d.starts_with('M'))), "{} is drawn", info.name);
    }
    assert!(icons["heart"]["paths"][0]["fill"].as_bool().unwrap(), "a closed shape is filled");
    assert!(!icons["line"]["paths"][0]["fill"].as_bool().unwrap(), "a line is not");
    for name in ["curve", "freeform", "scribble"] {
        assert!(matches!(tool_kind(name), Some(ToolKind::Shape { .. })), "{name} draws");
    }
    assert!(tool_kind("noSuchShape").is_none());
    // Lock drawing mode: the heart stays chosen; chosen again plainly, it is not locked.
    assert!(doc.set_tool_lock("heart", true).unwrap());
    assert_eq!(doc.tool(), "heart");
    assert!(doc.session.tool.sticky);
    assert!(!doc.set_tool_lock("heart", false).unwrap());
    assert!(!doc.session.tool.sticky);
    assert!(!doc.set_tool_lock("textBox", true).unwrap(), "only shapes lock");
    assert!(doc.set_tool_lock("noSuchShape", true).is_err());
}
