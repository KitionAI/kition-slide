use super::{Rest, pinned, r, rs};

#[allow(unused_imports)]
use pinned as _;

pub const ROWS: &[Rest] = &[
    // file.new: makes a new deck the active document (Session::add_document): the deck is replaced whole.
    r("file.new", r#"{}"#, "", &[("root", "*", "*")]),
    r("file.new", r#"{"theme": "Office", "size": [720, 540], "blank": true}"#, "", &[("root", "*", "*")]),
    // file.open: reads the file at `path` and makes it the active document: the deck is replaced whole.
    r("file.open", r#"{"path": "/tmp/reading.pptx"}"#, "", &[("root", "*", "*")]),
    // file.openBytes: decodes `data` and makes it the active document: the deck is replaced whole.
    r("file.openBytes", r#"{"name": "reading.kslide", "data": "AAAA"}"#, "", &[("root", "*", "*")]),
    // file.save: encodes the deck to bytes and marks it saved (saved_doc, path): nothing in the deck model changes.
    r("file.save", r#"{}"#, "", &[]),
    r("file.save", r#"{"path": "/tmp/reading.pptx", "format": "pptx"}"#, "", &[]),
    // file.saveAs: without `path` only asks the host for one; with it, file.save: nothing in the deck model changes.
    r("file.saveAs", r#"{"path": "/tmp/reading.kslide"}"#, "", &[]),
    r("file.saveAs", r#"{}"#, "", &[]),
    // C8-slide-pin r2: file.rs:414-418 closes the session without reading or replacing saved content.
    r("file.close", r#"{}"#, "", &[]),
    // file.properties: each given field replaces root props.doc.<field> (RootProps.doc); with none it only answers.
    r("file.properties", r#"{"title": "T"}"#, "", &[("root", "props.doc.title", "-")]),
    r("file.properties", r#"{"author": "A", "company": "C"}"#, "", &[("root", "props.doc.author,props.doc.company", "-")]),
    r("file.properties", r#"{}"#, "", &[]),
    // file.recovery.save: writes the unsaved documents to the recovery folder: nothing in the deck model.
    r("file.recovery.save", r#"{}"#, "", &[]),
    // file.recovery.open: adds each recovered document and makes the last one active: the active deck is replaced whole.
    r("file.recovery.open", r#"{}"#, "", &[("root", "*", "*")]),
    // file.recovery.discard: removes recovery files on disk: nothing in the deck model.
    r("file.recovery.discard", r#"{}"#, "", &[]),
    r("file.recovery.discard", r#"{"uid": 1}"#, "", &[]),
    // file.revert: the deck becomes the saved snapshot and the history is cleared: the deck is replaced whole.
    r("file.revert", r#"{}"#, "", &[("root", "*", "*")]),
    // edit.undo: restore(): the scope's last step's snapshot replaces the deck (or the replica steps back): the whole deck.
    r("edit.undo", r#"{}"#, "", &[("root", "*", "*")]),
    r("edit.undo", r#"{"scope": "agent"}"#, "", &[("root", "*", "*")]),
    // edit.redo: as undo, forward; with nothing to redo it returns before changing anything.
    rs(r#"[["edit.undo", {}]]"#, "edit.redo", r#"{}"#, "", &[("root", "*", "*")]),
    r("edit.redo", r#"{}"#, "", &[]),
    // edit.cut: named shapes (ids/id) are selected and copied to the clipboard (not the deck), then edit.delete removes them: the shape whole, $S2's animations filtered, the glued connector re-routed (its own frame and the other end's site).
    r(
        "edit.cut",
        r#"{"id": "$Rect"}"#,
        "",
        &[("$Rect", "*", "-"), ("$S2", "animations", "-"), ("$Line", "look.geom,look.kind,xfrm", "-"), ("$Oval", "look.geom,xfrm", "-")],
    ),
    r(
        "edit.cut",
        r#"{}"#,
        "part,selection",
        &[("$Rect", "*", "-"), ("$S2", "animations", "-"), ("$Line", "look.geom,look.kind,xfrm", "-"), ("$Oval", "look.geom,xfrm", "-")],
    ),
    // normalized after review: `onSlide` names the slide by its number: it is current for the call
    // (lib.rs `run_named`), so where it is matters, as the other `onSlide` rows hold.
    pinned(r(
        "edit.cut",
        r#"{"id": 264, "onSlide": 2}"#,
        "",
        &[("$Oval", "*", "address"), ("$S2", "animations", "*"), ("$Line", "look.geom,look.kind,xfrm", "-"), ("$Rect", "look.geom,xfrm", "-")],
    )),
    r(
        "edit.cut",
        r#"{"ids": ["$GA"]}"#,
        "",
        &[("$GA", "*", "-"), ("$S2", "animations", "-"), ("$Group", "children,look.kind,xfrm", "-"), ("$GB", "xfrm", "-")],
    ),
    // edit.cut scope slides: copies selection.slides and deletes them (slide.delete {}): the slide whole with its shapes and threads, the sections' members.
    r(
        "edit.cut",
        r#"{"scope": "slides"}"#,
        "selection",
        &[
            ("$S2", "*", "-"),
            ("$Title", "*", "-"),
            ("$Body", "*", "-"),
            ("$Rect", "*", "-"),
            ("$Oval", "*", "-"),
            ("$Line", "*", "-"),
            ("$Group", "*", "-"),
            ("$GA", "*", "-"),
            ("$GB", "*", "-"),
            ("$Table", "*", "-"),
            ("$Chart", "*", "-"),
            ("$Text", "*", "-"),
            ("$Pic", "*", "-"),
            ("$Video", "*", "-"),
            ("$Thread", "*", "-"),
            ("$Section:Intro", "slides", "-"),
            ("$Section:Main", "slides", "-"),
        ],
    ),
    // edit.cut while text is selected: copies the range, deletes it from the edited body; the text box refits its height (fit_text_box).
    rs(
        r#"[["text.edit", {"id": "$Text"}], ["text.selectAll", {}]]"#,
        "edit.cut",
        r#"{}"#,
        "part,selection",
        &[("$Text", "body,xfrm", "-"), ("$S2", "layout", "-"), ("$Master", "theme.fonts", "-"), ("root", "defaultTextStyle", "-")],
    ),
    // edit.paste with `text` differing from the clipboard, no text editing: insert.textBox on the current slide at a rect from the slide size, named from ~shapeCount, fit to its text.
    r(
        "edit.paste",
        r#"{"text": "Hi"}"#,
        "part,selection",
        &[("root", "defaultTextStyle,props.slideSize", "-"), ("$S2", "layout,~shapeCount", "-"), ("$Master", "theme.fonts", "-")],
    ),
    // edit.paste of clipboard shapes: appended to the current slide, offset when a shape with the same number is on it.
    rs(r#"[["edit.copy", {}]]"#, "edit.paste", r#"{}"#, "part,selection", &[("$Rect", "-", "-")]),
    // edit.paste of clipboard slides: inserted after the current slide, layout kept if it exists, media compared by bytes; the copies' placeholders refit (inheritance).
    // normalized after review: shrink-to-fit text is refit (`autofit::invalidate`) from the placeholders
    // the lookup compares by kind (`resolve::parents`, resolve.rs:60-96; `body` spelled
    // props (including list style), the master's list style for the kind
    // only (`resolve::master_list_style`, resolve.rs:100) and the shape's own geometry
    // (autofit.rs:15, :48); pasted slides take the clipboard slide's layout, not the
    // current slide's (cmd/edit.rs `paste`).
    pinned(rs(
        r#"[["edit.copy", {"scope": "slides"}]]"#,
        "edit.paste",
        r#"{}"#,
        "part,selection",
        &[
            ("$Layout:Title and Content", "-", "-"),
            ("$Media:red.png", "blob", "-"),
            ("$Media:clip.mp4", "blob", "-"),
            ("$Media:clip.mp4-poster.png", "blob", "-"),
            ("$Layout:Title and Content/Title 1", "body.props,look.ph,look.style,xfrm", "-"),
            ("$Layout:Title and Content/Content Placeholder 2", "body.props,look.ph,look.style,xfrm", "-"),
            ("$Master/Title 1", "body.props,look.ph", "-"),
            ("$Master/Text Placeholder 2", "body.props,look.ph", "-"),
            ("$Master", "listStyles.body,listStyles.title,theme.fonts", "-"),
            ("root", "defaultTextStyle", "-"),
        ],
    )),
    // edit.paste while editing a placeholder: inserts into its body; the title refits (shrink-to-fit) from its layout and master placeholders.
    // normalized after review: shrink-to-fit text is refit (`autofit::invalidate`) from the placeholders
    // the lookup compares by kind (`resolve::parents`, resolve.rs:60-96; `body` spelled
    // props (including list style), the master's list style for the kind
    // only (`resolve::master_list_style`, resolve.rs:100) and the shape's own geometry
    // (autofit.rs:15, :48); pasted slides take the clipboard slide's layout, not the
    // current slide's (cmd/edit.rs `paste`).
    pinned(rs(
        r#"[["text.edit", {"id": "$Title"}]]"#,
        "edit.paste",
        r#"{"text": "Hi"}"#,
        "part,selection",
        &[
            ("$Title", "body,look.geom,look.ph,look.style,xfrm", "-"),
            ("$S2", "layout", "-"),
            ("$Layout:Title and Content/Title 1", "body.props,look.ph,look.style,xfrm", "-"),
            ("$Master/Title 1", "body.props,look.ph", "-"),
            ("$Master", "listStyles.title,theme.fonts", "-"),
            ("root", "defaultTextStyle", "-"),
        ],
    )),
    // edit.paste while editing the notes: inserts into the current slide's notes.
    rs(r#"[["text.edit", {"notes": true}]]"#, "edit.paste", r#"{"text": "Hi"}"#, "part,selection", &[("$S2", "notesBody", "-")]),
    // edit.paste with an empty clipboard and no text: changes nothing.
    r("edit.paste", r#"{}"#, "selection", &[]),
    // edit.pasteText: `text` or the clipboard's plain text; no editing: a new text box (as edit.paste); editing: inserted into the edited body.
    r(
        "edit.pasteText",
        r#"{"text": "Hi"}"#,
        "part,selection",
        &[("root", "defaultTextStyle,props.slideSize", "-"), ("$S2", "layout,~shapeCount", "-"), ("$Master", "theme.fonts", "-")],
    ),
    r(
        "edit.pasteText",
        r#"{}"#,
        "part,selection",
        &[("root", "defaultTextStyle,props.slideSize", "-"), ("$S2", "layout,~shapeCount", "-"), ("$Master", "theme.fonts", "-")],
    ),
    rs(
        r#"[["text.edit", {"id": "$Text"}]]"#,
        "edit.pasteText",
        r#"{"text": "Hi"}"#,
        "part,selection",
        &[("$Text", "body,xfrm", "-"), ("$S2", "layout", "-"), ("$Master", "theme.fonts", "-"), ("root", "defaultTextStyle", "-")],
    ),
    // edit.delete: named shapes (ids/id) or, without names, by the editor state: text being edited (delete forward), selected
    // table cells (table.clearCells), slides scope (slide.delete {}), else the selected shapes. Shapes go whole, $S2's
    // animations are filtered, a glued connector is re-routed, a group losing a member refits.
    r(
        "edit.delete",
        r#"{"id": "$Rect"}"#,
        "",
        &[("$Rect", "*", "-"), ("$S2", "animations", "-"), ("$Line", "look.geom,look.kind,xfrm", "-"), ("$Oval", "look.geom,xfrm", "-")],
    ),
    r(
        "edit.delete",
        r#"{}"#,
        "part,selection",
        &[("$Rect", "*", "-"), ("$S2", "animations", "-"), ("$Line", "look.geom,look.kind,xfrm", "-"), ("$Oval", "look.geom,xfrm", "-")],
    ),
    // normalized after review: `onSlide` names the slide by its number: it is current for the call
    // (lib.rs `run_named`), so where it is matters, as the other `onSlide` rows hold.
    pinned(r(
        "edit.delete",
        r#"{"id": 264, "onSlide": 2}"#,
        "",
        &[("$Oval", "*", "address"), ("$S2", "animations", "*"), ("$Line", "look.geom,look.kind,xfrm", "-"), ("$Rect", "look.geom,xfrm", "-")],
    )),
    r(
        "edit.delete",
        r#"{"ids": ["$Rect", "$Oval"]}"#,
        "",
        &[("$Rect", "*", "-"), ("$Oval", "*", "-"), ("$S2", "animations", "-"), ("$Line", "look.kind,xfrm", "-")],
    ),
    r(
        "edit.delete",
        r#"{"ids": ["$GA"]}"#,
        "",
        &[("$GA", "*", "-"), ("$S2", "animations", "-"), ("$Group", "children,look.kind,xfrm", "-"), ("$GB", "xfrm", "-")],
    ),
    r("edit.delete", r#"{"id": "$Group"}"#, "", &[("$Group", "*", "-"), ("$GA", "*", "-"), ("$GB", "*", "-"), ("$S2", "animations", "-")]),
    r(
        "edit.delete",
        r#"{"scope": "slides"}"#,
        "selection",
        &[
            ("$S2", "*", "-"),
            ("$Title", "*", "-"),
            ("$Body", "*", "-"),
            ("$Rect", "*", "-"),
            ("$Oval", "*", "-"),
            ("$Line", "*", "-"),
            ("$Group", "*", "-"),
            ("$GA", "*", "-"),
            ("$GB", "*", "-"),
            ("$Table", "*", "-"),
            ("$Chart", "*", "-"),
            ("$Text", "*", "-"),
            ("$Pic", "*", "-"),
            ("$Video", "*", "-"),
            ("$Thread", "*", "-"),
            ("$Section:Intro", "slides", "-"),
            ("$Section:Main", "slides", "-"),
        ],
    ),
    rs(
        r#"[["text.edit", {"id": "$Text", "at": [0, 0]}]]"#,
        "edit.delete",
        r#"{}"#,
        "part,selection",
        &[("$Text", "body,xfrm", "-"), ("$S2", "layout", "-"), ("$Master", "theme.fonts", "-"), ("root", "defaultTextStyle", "-")],
    ),
    rs(
        r#"[["table.selectCells", {"id": "$Table", "from": [0, 0], "to": [0, 1]}]]"#,
        "edit.delete",
        r#"{}"#,
        "part,selection",
        &[("$Table", "table,xfrm", "-")],
    ),
    // edit.duplicate: targets (ids or selection); copies the TOP-LEVEL ones in list order with their frame (inherited for a placeholder, which loses its ph) offset, appended to the current slide.
    r("edit.duplicate", r#"{"ids": ["$Rect"]}"#, "", &[("$Rect", "*", "-")]),
    r("edit.duplicate", r#"{}"#, "part,selection", &[("$Rect", "*", "-")]),
    r("edit.duplicate", r#"{"ids": ["$Rect", "$Oval"]}"#, "", &[("$Rect", "*", "*"), ("$Oval", "*", "*")]),
    // normalized after review: `onSlide` names the slide by its number: it is current for the call
    // (lib.rs `run_named`), so where it is matters, as the other `onSlide` rows hold.
    pinned(r("edit.duplicate", r#"{"ids": [263], "onSlide": 2}"#, "", &[("$Rect", "*", "address"), ("$S2", "-", "*")])),
    // edit.duplicate of a placeholder: its frame from the layout placeholder; the copy (no ph) refits from the deck's default text style.
    // normalized after review: the inherited box (`xfrm_of` -> `resolve::xfrm`) finds the layout
    // placeholder by comparing placeholder kinds (`resolve::find_ph`, model resolve.rs:60).
    pinned(r(
        "edit.duplicate",
        r#"{"ids": ["$Title"]}"#,
        "",
        &[
            ("$Title", "*", "-"),
            ("$S2", "layout", "-"),
            ("$Layout:Title and Content/Title 1", "look.ph,xfrm", "-"),
            ("$Master", "theme.fonts", "-"),
            ("root", "defaultTextStyle", "-"),
        ],
    )),
    // edit.duplicate of a group member: the top-level filter finds nothing; nothing is copied.
    r("edit.duplicate", r#"{"ids": ["$GA"]}"#, "", &[("$GA", "-", "-")]),
    // slide.new default: the current slide's layout (not a title layout), its placeholders copied (name, ph) and refit; after the current slide, in its section.
    // normalized after review: the handler reads the sections' member lists only (section_of, crates/engine/model/src/lib.rs:1145; crates/engine/engine/src/cmd/slide.rs:124-137), never their names
    // C8-slide-pin r2: defaults.rs:535-554 and slide.rs:95-110 consume membership/order, not descendant text.
    pinned(r(
        "slide.new",
        r#"{}"#,
        "part",
        &[
            ("$S2", "layout", "-"),
            ("$Layout:Title and Content", "props.kind,@order(shapes),~shapeCount", "-"),
            ("$Layout:Title and Content/Title 1", "body.props,look.ph,look.style,name,xfrm", "-"),
            ("$Layout:Title and Content/Content Placeholder 2", "body.props,look.ph,look.style,name,xfrm", "-"),
            ("$Layout:Title and Content/Date Placeholder", "look.ph", "-"),
            ("$Layout:Title and Content/Footer Placeholder", "look.ph", "-"),
            ("$Layout:Title and Content/Slide Number Placeholder", "look.ph", "-"),
            ("$Master/Title 1", "body.props", "-"),
            ("$Master/Text Placeholder 2", "body.props", "-"),
            ("$Master", "@order(layouts),~layoutCount,listStyles,theme.fonts", "-"),
            ("root", "defaultTextStyle", "-"),
            ("$Section:Intro", "slides", "-"),
            ("$Section:Main", "slides", "-"),
        ],
    )),
    // normalized after review: the handler reads the sections' member lists only (section_of, crates/engine/model/src/lib.rs:1145; crates/engine/engine/src/cmd/slide.rs:124-137), never their names
    // C8-slide-pin r2: defaults.rs:535-554 and slide.rs:95-110 consume membership/order, not descendant text.
    pinned(r(
        "slide.new",
        r#"{"at": 3}"#,
        "part",
        &[
            ("$S2", "layout", "-"),
            ("$S3", "-", "*"),
            ("$Layout:Title and Content", "props.kind,@order(shapes),~shapeCount", "-"),
            ("$Layout:Title and Content/Title 1", "body.props,look.ph,look.style,name,xfrm", "-"),
            ("$Layout:Title and Content/Content Placeholder 2", "body.props,look.ph,look.style,name,xfrm", "-"),
            ("$Layout:Title and Content/Date Placeholder", "look.ph", "-"),
            ("$Layout:Title and Content/Footer Placeholder", "look.ph", "-"),
            ("$Layout:Title and Content/Slide Number Placeholder", "look.ph", "-"),
            ("$Master/Title 1", "body.props", "-"),
            ("$Master/Text Placeholder 2", "body.props", "-"),
            ("$Master", "@order(layouts),~layoutCount,listStyles,theme.fonts", "-"),
            ("root", "defaultTextStyle", "-"),
            ("$Section:Intro", "slides", "-"),
            ("$Section:Main", "slides", "-"),
        ],
    )),
    // slide.new after a title slide: the first Title and Content layout by kind.
    // normalized after review: the handler reads the sections' member lists only (section_of, crates/engine/model/src/lib.rs:1145; crates/engine/engine/src/cmd/slide.rs:124-137), never their names
    // C8-slide-pin r2: defaults.rs:535-554 and slide.rs:95-110 consume membership/order, not descendant text.
    pinned(rs(
        r#"[["slide.go", {"index": 0}]]"#,
        "slide.new",
        r#"{}"#,
        "part",
        &[
            ("$S1", "layout", "-"),
            ("$Layout:Title Slide", "props.kind", "-"),
            ("$Layout:Title and Content", "props.kind,@order(shapes),~shapeCount", "-"),
            ("$Layout:Title and Content/Title 1", "body.props,look.ph,look.style,name,xfrm", "-"),
            ("$Layout:Title and Content/Content Placeholder 2", "body.props,look.ph,look.style,name,xfrm", "-"),
            ("$Layout:Title and Content/Date Placeholder", "look.ph", "-"),
            ("$Layout:Title and Content/Footer Placeholder", "look.ph", "-"),
            ("$Layout:Title and Content/Slide Number Placeholder", "look.ph", "-"),
            ("$Master/Title 1", "body.props", "-"),
            ("$Master/Text Placeholder 2", "body.props", "-"),
            ("$Master", "@order(layouts),~layoutCount,listStyles,theme.fonts", "-"),
            ("root", "defaultTextStyle", "-"),
            ("$Section:Intro", "slides", "-"),
            ("$Section:Main", "slides", "-"),
        ],
    )),
    // slide.new with `layout`: looked up by name over the current slide's master's layouts (then kind); title/body set on its title and first body placeholders.
    // normalized after review: find_layout matches the name before any kind (crates/engine/engine/src/cmd/slide.rs:88) and new_slide reads only the layout's shapes (crates/engine/model/src/defaults.rs:535): no kind is read; the handler reads the sections' member lists only (section_of, crates/engine/model/src/lib.rs:1145; crates/engine/engine/src/cmd/slide.rs:124-137), never their names
    // C8-slide-pin r2: defaults.rs:535-554 and slide.rs:95-110 consume membership/order, not descendant text.
    pinned(r(
        "slide.new",
        r#"{"layout": "titleAndContent", "title": "T", "body": "B"}"#,
        "part",
        &[
            ("$S2", "layout", "-"),
            ("$Layout:Title Slide", "props.name", "-"),
            ("$Layout:Title and Content", "props.name,@order(shapes),~shapeCount", "-"),
            ("$Layout:Title and Content/Title 1", "body.props,look.ph,look.style,name,xfrm", "-"),
            ("$Layout:Title and Content/Content Placeholder 2", "body.props,look.ph,look.style,name,xfrm", "-"),
            ("$Layout:Title and Content/Date Placeholder", "look.ph", "-"),
            ("$Layout:Title and Content/Footer Placeholder", "look.ph", "-"),
            ("$Layout:Title and Content/Slide Number Placeholder", "look.ph", "-"),
            ("$Master/Title 1", "body.props", "-"),
            ("$Master/Text Placeholder 2", "body.props", "-"),
            ("$Master", "@order(layouts),~layoutCount,listStyles,theme.fonts", "-"),
            ("root", "defaultTextStyle", "-"),
            ("$Section:Intro", "slides", "-"),
            ("$Section:Main", "slides", "-"),
        ],
    )),
    // normalized after review: the handler reads the sections' member lists only (section_of, crates/engine/model/src/lib.rs:1145; crates/engine/engine/src/cmd/slide.rs:124-137), never their names
    // C8-slide-pin r2: defaults.rs:535-554 and slide.rs:95-110 consume membership/order, not descendant text.
    pinned(r(
        "slide.new",
        r#"{"layout": "blank", "at": 0}"#,
        "part",
        &[
            ("$S2", "layout", "-"),
            ("$Layout:Title Slide", "props.name", "-"),
            ("$Layout:Title and Content", "props.name", "-"),
            ("$Layout:Section Header", "props.name", "-"),
            ("$Layout:Two Content", "props.name", "-"),
            ("$Layout:Comparison", "props.name", "-"),
            ("$Layout:Title Only", "props.name", "-"),
            ("$Layout:Blank", "props.name,@order(shapes),~shapeCount", "-"),
            ("$Layout:Blank/Date Placeholder", "look.ph", "-"),
            ("$Layout:Blank/Footer Placeholder", "look.ph", "-"),
            ("$Layout:Blank/Slide Number Placeholder", "look.ph", "-"),
            ("$Master", "@order(layouts),~layoutCount", "-"),
            ("$Section:Intro", "slides", "-"),
            ("$Section:Main", "slides", "-"),
        ],
    )),
    // slide.duplicate: index, else selection.slides, else the current slide; the slide copied whole (shapes, animations, threads remapped), after it, into its section; the copies' placeholders checked against their layout (refit).
    // normalized after review: the handler reads the sections' member lists only (section_of, crates/engine/model/src/lib.rs:1145; crates/engine/engine/src/cmd/slide.rs:201-210), never their names
    pinned(r(
        "slide.duplicate",
        r#"{"index": 1}"#,
        "",
        &[
            ("$S2", "*", "*"),
            ("$Title", "*", "-"),
            ("$Body", "*", "-"),
            ("$Rect", "*", "-"),
            ("$Oval", "*", "-"),
            ("$Line", "*", "-"),
            ("$Group", "*", "-"),
            ("$GA", "*", "-"),
            ("$GB", "*", "-"),
            ("$Table", "*", "-"),
            ("$Chart", "*", "-"),
            ("$Text", "*", "-"),
            ("$Pic", "*", "-"),
            ("$Video", "*", "-"),
            ("$Thread", "*", "-"),
            ("$Media:red.png", "-", "-"),
            ("$Media:clip.mp4", "-", "-"),
            ("$Media:clip.mp4-poster.png", "-", "-"),
            ("$Layout:Title and Content/Title 1", "body.props,look.style,xfrm", "-"),
            ("$Layout:Title and Content/Content Placeholder 2", "body.props,look.style,xfrm", "-"),
            ("$Master/Title 1", "body.props", "-"),
            ("$Master/Text Placeholder 2", "body.props", "-"),
            ("$Master", "listStyles,theme.fonts", "-"),
            ("root", "defaultTextStyle", "-"),
            ("$Section:Intro", "slides", "-"),
            ("$Section:Main", "slides", "-"),
        ],
    )),
    // normalized after review: the handler reads the sections' member lists only (section_of, crates/engine/model/src/lib.rs:1145; crates/engine/engine/src/cmd/slide.rs:201-210), never their names
    pinned(r(
        "slide.duplicate",
        r#"{}"#,
        "selection",
        &[
            ("$S2", "*", "-"),
            ("$Title", "*", "-"),
            ("$Body", "*", "-"),
            ("$Rect", "*", "-"),
            ("$Oval", "*", "-"),
            ("$Line", "*", "-"),
            ("$Group", "*", "-"),
            ("$GA", "*", "-"),
            ("$GB", "*", "-"),
            ("$Table", "*", "-"),
            ("$Chart", "*", "-"),
            ("$Text", "*", "-"),
            ("$Pic", "*", "-"),
            ("$Video", "*", "-"),
            ("$Thread", "*", "-"),
            ("$Media:red.png", "-", "-"),
            ("$Media:clip.mp4", "-", "-"),
            ("$Media:clip.mp4-poster.png", "-", "-"),
            ("$Layout:Title and Content/Title 1", "body.props,look.style,xfrm", "-"),
            ("$Layout:Title and Content/Content Placeholder 2", "body.props,look.style,xfrm", "-"),
            ("$Master/Title 1", "body.props", "-"),
            ("$Master/Text Placeholder 2", "body.props", "-"),
            ("$Master", "listStyles,theme.fonts", "-"),
            ("root", "defaultTextStyle", "-"),
            ("$Section:Intro", "slides", "-"),
            ("$Section:Main", "slides", "-"),
        ],
    )),
    // normalized after review: the handler reads the sections' member lists only (section_of, crates/engine/model/src/lib.rs:1145; crates/engine/engine/src/cmd/slide.rs:201-210), never their names
    pinned(rs(
        r#"[["slide.selectSlides", {"indices": []}]]"#,
        "slide.duplicate",
        r#"{}"#,
        "part",
        &[
            ("$S2", "*", "-"),
            ("$Title", "*", "-"),
            ("$Body", "*", "-"),
            ("$Rect", "*", "-"),
            ("$Oval", "*", "-"),
            ("$Line", "*", "-"),
            ("$Group", "*", "-"),
            ("$GA", "*", "-"),
            ("$GB", "*", "-"),
            ("$Table", "*", "-"),
            ("$Chart", "*", "-"),
            ("$Text", "*", "-"),
            ("$Pic", "*", "-"),
            ("$Video", "*", "-"),
            ("$Thread", "*", "-"),
            ("$Media:red.png", "-", "-"),
            ("$Media:clip.mp4", "-", "-"),
            ("$Media:clip.mp4-poster.png", "-", "-"),
            ("$Layout:Title and Content/Title 1", "body.props,look.style,xfrm", "-"),
            ("$Layout:Title and Content/Content Placeholder 2", "body.props,look.style,xfrm", "-"),
            ("$Master/Title 1", "body.props", "-"),
            ("$Master/Text Placeholder 2", "body.props", "-"),
            ("$Master", "listStyles,theme.fonts", "-"),
            ("root", "defaultTextStyle", "-"),
            ("$Section:Intro", "slides", "-"),
            ("$Section:Main", "slides", "-"),
        ],
    )),
    // slide.delete: ids, else index, else selection.slides, else the current slide; the slides go whole (shapes, threads) and leave their sections.
    // normalized after review: the handler reads the sections' member lists only (section_of, crates/engine/model/src/lib.rs:1145; crates/engine/engine/src/cmd/slide.rs:249-251), never their names
    pinned(r(
        "slide.delete",
        r#"{"index": 1}"#,
        "",
        &[
            ("$S2", "*", "*"),
            ("$Title", "*", "-"),
            ("$Body", "*", "-"),
            ("$Rect", "*", "-"),
            ("$Oval", "*", "-"),
            ("$Line", "*", "-"),
            ("$Group", "*", "-"),
            ("$GA", "*", "-"),
            ("$GB", "*", "-"),
            ("$Table", "*", "-"),
            ("$Chart", "*", "-"),
            ("$Text", "*", "-"),
            ("$Pic", "*", "-"),
            ("$Video", "*", "-"),
            ("$Thread", "*", "-"),
            ("$Section:Intro", "slides", "-"),
            ("$Section:Main", "slides", "-"),
        ],
    )),
    // normalized after review: the handler reads the sections' member lists only (section_of, crates/engine/model/src/lib.rs:1145; crates/engine/engine/src/cmd/slide.rs:249-251), never their names
    pinned(r(
        "slide.delete",
        r#"{"ids": ["$S2"]}"#,
        "",
        &[
            ("$S2", "*", "-"),
            ("$Title", "*", "-"),
            ("$Body", "*", "-"),
            ("$Rect", "*", "-"),
            ("$Oval", "*", "-"),
            ("$Line", "*", "-"),
            ("$Group", "*", "-"),
            ("$GA", "*", "-"),
            ("$GB", "*", "-"),
            ("$Table", "*", "-"),
            ("$Chart", "*", "-"),
            ("$Text", "*", "-"),
            ("$Pic", "*", "-"),
            ("$Video", "*", "-"),
            ("$Thread", "*", "-"),
            ("$Section:Intro", "slides", "-"),
            ("$Section:Main", "slides", "-"),
        ],
    )),
    // normalized after review: the handler reads the sections' member lists only (section_of, crates/engine/model/src/lib.rs:1145; crates/engine/engine/src/cmd/slide.rs:249-251), never their names
    pinned(r(
        "slide.delete",
        r#"{}"#,
        "selection",
        &[
            ("$S2", "*", "-"),
            ("$Title", "*", "-"),
            ("$Body", "*", "-"),
            ("$Rect", "*", "-"),
            ("$Oval", "*", "-"),
            ("$Line", "*", "-"),
            ("$Group", "*", "-"),
            ("$GA", "*", "-"),
            ("$GB", "*", "-"),
            ("$Table", "*", "-"),
            ("$Chart", "*", "-"),
            ("$Text", "*", "-"),
            ("$Pic", "*", "-"),
            ("$Video", "*", "-"),
            ("$Thread", "*", "-"),
            ("$Section:Intro", "slides", "-"),
            ("$Section:Main", "slides", "-"),
        ],
    )),
    // normalized after review: the handler reads the sections' member lists only (section_of, crates/engine/model/src/lib.rs:1145; crates/engine/engine/src/cmd/slide.rs:249-251), never their names
    pinned(rs(
        r#"[["slide.selectSlides", {"indices": []}]]"#,
        "slide.delete",
        r#"{}"#,
        "part",
        &[
            ("$S2", "*", "-"),
            ("$Title", "*", "-"),
            ("$Body", "*", "-"),
            ("$Rect", "*", "-"),
            ("$Oval", "*", "-"),
            ("$Line", "*", "-"),
            ("$Group", "*", "-"),
            ("$GA", "*", "-"),
            ("$GB", "*", "-"),
            ("$Table", "*", "-"),
            ("$Chart", "*", "-"),
            ("$Text", "*", "-"),
            ("$Pic", "*", "-"),
            ("$Video", "*", "-"),
            ("$Thread", "*", "-"),
            ("$Section:Intro", "slides", "-"),
            ("$Section:Main", "slides", "-"),
        ],
    )),
    // normalized after review: the handler reads the sections' member lists only (section_of, crates/engine/model/src/lib.rs:1145; crates/engine/engine/src/cmd/slide.rs:249-251), never their names
    pinned(r(
        "slide.delete",
        r#"{"index": 2}"#,
        "",
        &[("$S3", "*", "*"), ("$Star", "*", "-"), ("$Section:Intro", "slides", "-"), ("$Section:Main", "slides", "-")],
    )),
    // slide.move: the slide at `from` (else the current one) moves to `to`; it leaves every section and joins the section of the slide before it (else the first), which is resorted by deck order.
    // normalized after review: the handler reads the sections' member lists only (section_of, crates/engine/model/src/lib.rs:1145; crates/engine/engine/src/cmd/slide.rs:272-282), never their names
    pinned(r(
        "slide.move",
        r#"{"from": 1, "to": 0}"#,
        "",
        &[("$S2", "-", "*"), ("$S1", "-", "*"), ("$Section:Intro", "slides", "-"), ("$Section:Main", "slides", "-")],
    )),
    // normalized after review: the handler reads the sections' member lists only (section_of, crates/engine/model/src/lib.rs:1145; crates/engine/engine/src/cmd/slide.rs:272-282), never their names
    pinned(r(
        "slide.move",
        r#"{"to": 2}"#,
        "part",
        &[("$S2", "-", "-"), ("$S3", "-", "*"), ("$Section:Intro", "slides", "-"), ("$Section:Main", "slides", "-")],
    )),
    // slide.hide: index, else selection.slides, else the current slide; `hidden`, else the toggle of whether all are hidden.
    r("slide.hide", r#"{"index": 1}"#, "", &[("$S2", "props.hidden", "*")]),
    r("slide.hide", r#"{"index": 1, "hidden": true}"#, "", &[("$S2", "props.hidden", "*")]),
    r("slide.hide", r#"{}"#, "selection", &[("$S2", "props.hidden", "-")]),
    rs(
        r#"[["slide.selectSlides", {"indices": [0, 2]}]]"#,
        "slide.hide",
        r#"{}"#,
        "selection",
        &[("$S1", "props.hidden", "-"), ("$S3", "props.hidden", "-")],
    ),
    rs(r#"[["slide.selectSlides", {"indices": []}]]"#, "slide.hide", r#"{}"#, "part", &[("$S2", "props.hidden", "-")]),
    // slide.layout: the layout by name/kind in the CURRENT slide's master; on index, else selection.slides, else the current slide: placeholders matched by ph (frame cleared), missing ones made, empty leftovers dropped, the shape list rebuilt, layout set; the placeholders refit.
    r(
        "slide.layout",
        r#"{"layout": "twoContent", "index": 1}"#,
        "part",
        &[
            ("$S2", "layout,shapes", "*"),
            ("$Title", "body,look.ph,xfrm", "-"),
            ("$Body", "body,look.ph,xfrm", "-"),
            ("$Layout:Title Slide", "props.name", "-"),
            ("$Layout:Title and Content", "props.name", "-"),
            ("$Layout:Section Header", "props.name", "-"),
            ("$Layout:Two Content", "props.name,@order(shapes),~shapeCount", "-"),
            ("$Layout:Two Content/Title 1", "body.props,look.ph,look.style,name,xfrm", "-"),
            ("$Layout:Two Content/Content Placeholder 2", "body.props,look.ph,look.style,name,xfrm", "-"),
            ("$Layout:Two Content/Content Placeholder 3", "body.props,look.ph,look.style,name,xfrm", "-"),
            ("$Layout:Two Content/Date Placeholder", "look.ph", "-"),
            ("$Layout:Two Content/Footer Placeholder", "look.ph", "-"),
            ("$Layout:Two Content/Slide Number Placeholder", "look.ph", "-"),
            ("$Master/Title 1", "body.props", "-"),
            ("$Master/Text Placeholder 2", "body.props", "-"),
            ("$Master", "layouts,listStyles,theme.fonts", "-"),
            ("root", "defaultTextStyle", "-"),
        ],
    ),
    r(
        "slide.layout",
        r#"{"layout": "twoContent"}"#,
        "part,selection",
        &[
            ("$S2", "layout,shapes", "-"),
            ("$Title", "body,look.ph,xfrm", "-"),
            ("$Body", "body,look.ph,xfrm", "-"),
            ("$Layout:Title Slide", "props.name", "-"),
            ("$Layout:Title and Content", "props.name", "-"),
            ("$Layout:Section Header", "props.name", "-"),
            ("$Layout:Two Content", "props.name,@order(shapes),~shapeCount", "-"),
            ("$Layout:Two Content/Title 1", "body.props,look.ph,look.style,name,xfrm", "-"),
            ("$Layout:Two Content/Content Placeholder 2", "body.props,look.ph,look.style,name,xfrm", "-"),
            ("$Layout:Two Content/Content Placeholder 3", "body.props,look.ph,look.style,name,xfrm", "-"),
            ("$Layout:Two Content/Date Placeholder", "look.ph", "-"),
            ("$Layout:Two Content/Footer Placeholder", "look.ph", "-"),
            ("$Layout:Two Content/Slide Number Placeholder", "look.ph", "-"),
            ("$Master/Title 1", "body.props", "-"),
            ("$Master/Text Placeholder 2", "body.props", "-"),
            ("$Master", "layouts,listStyles,theme.fonts", "-"),
            ("root", "defaultTextStyle", "-"),
        ],
    ),
    r(
        "slide.layout",
        r#"{"layout": "titleOnly", "index": 2}"#,
        "part",
        &[
            ("$S3", "layout,shapes", "*"),
            ("$S2", "layout", "-"),
            ("$Layout:Title Slide", "props.name", "-"),
            ("$Layout:Title and Content", "props.name", "-"),
            ("$Layout:Section Header", "props.name", "-"),
            ("$Layout:Two Content", "props.name", "-"),
            ("$Layout:Comparison", "props.name", "-"),
            ("$Layout:Title Only", "props.name,@order(shapes),~shapeCount", "-"),
            ("$Layout:Title Only/Title 1", "body.props,look.ph,look.style,name,xfrm", "-"),
            ("$Layout:Title Only/Date Placeholder", "look.ph", "-"),
            ("$Layout:Title Only/Footer Placeholder", "look.ph", "-"),
            ("$Layout:Title Only/Slide Number Placeholder", "look.ph", "-"),
            ("$Master/Title 1", "body.props", "-"),
            ("$Master", "layouts,listStyles,theme.fonts", "-"),
            ("root", "defaultTextStyle", "-"),
        ],
    ),
    // slide.reset: index, else the current slide; its own layout reapplied, background cleared, placeholders' frame/look/body formatting cleared; they refit.
    r(
        "slide.reset",
        r#"{"index": 1}"#,
        "",
        &[
            ("$S2", "background,layout,shapes", "*"),
            ("$Title", "body,look.effects,look.fill,look.geom,look.line,look.ph,xfrm", "-"),
            ("$Body", "body,look.effects,look.fill,look.geom,look.line,look.ph,xfrm", "-"),
            ("$Layout:Title and Content", "@order(shapes),~shapeCount", "-"),
            ("$Layout:Title and Content/Title 1", "body.props,look.ph,look.style,name,xfrm", "-"),
            ("$Layout:Title and Content/Content Placeholder 2", "body.props,look.ph,look.style,name,xfrm", "-"),
            ("$Layout:Title and Content/Date Placeholder", "look.ph", "-"),
            ("$Layout:Title and Content/Footer Placeholder", "look.ph", "-"),
            ("$Layout:Title and Content/Slide Number Placeholder", "look.ph", "-"),
            ("$Master/Title 1", "body.props", "-"),
            ("$Master/Text Placeholder 2", "body.props", "-"),
            ("$Master", "listStyles,theme.fonts", "-"),
            ("root", "defaultTextStyle", "-"),
        ],
    ),
    r(
        "slide.reset",
        r#"{}"#,
        "part",
        &[
            ("$S2", "background,layout,shapes", "-"),
            ("$Title", "body,look.effects,look.fill,look.geom,look.line,look.ph,xfrm", "-"),
            ("$Body", "body,look.effects,look.fill,look.geom,look.line,look.ph,xfrm", "-"),
            ("$Layout:Title and Content", "@order(shapes),~shapeCount", "-"),
            ("$Layout:Title and Content/Title 1", "body.props,look.ph,look.style,name,xfrm", "-"),
            ("$Layout:Title and Content/Content Placeholder 2", "body.props,look.ph,look.style,name,xfrm", "-"),
            ("$Layout:Title and Content/Date Placeholder", "look.ph", "-"),
            ("$Layout:Title and Content/Footer Placeholder", "look.ph", "-"),
            ("$Layout:Title and Content/Slide Number Placeholder", "look.ph", "-"),
            ("$Master/Title 1", "body.props", "-"),
            ("$Master/Text Placeholder 2", "body.props", "-"),
            ("$Master", "listStyles,theme.fonts", "-"),
            ("root", "defaultTextStyle", "-"),
        ],
    ),
    // slide.notes: index, else the current slide; the notes replaced.
    r("slide.notes", r#"{"text": "Hi", "index": 1}"#, "", &[("$S2", "notesBody", "*")]),
    r("slide.notes", r#"{"text": "Hi"}"#, "part", &[("$S2", "notesBody", "-")]),
    // slide.rename: index, else the current slide; its name replaced.
    r("slide.rename", r#"{"name": "N", "index": 1}"#, "", &[("$S2", "props.name", "*")]),
    r("slide.rename", r#"{"name": "N"}"#, "part", &[("$S2", "props.name", "-")]),
    // C8-slide-pin r2: slide.rs:521-542 splits only the addressed section at its slide anchor.
    // section.add: the slide at `at` (else the current one) starts a new section split off the one holding it (its members); with no sections, the slides before it go to a Default Section.
    r("section.add", r#"{"name": "X", "at": 2}"#, "", &[("$S3", "-", "*"), ("$Section:Main", "slides.$S2", "-")]),
    r("section.add", r#"{"name": "X"}"#, "part", &[("$S2", "-", "-"), ("$Section:Main", "slides.$S2", "-")]),
    rs(r#"[["section.removeAll", {}]]"#, "section.add", r#"{"name": "X", "at": 1}"#, "", &[("$S2", "-", "*"), ("root", "sections", "-")]),
    // section.rename: the section at `index` (where it is): its name replaced.
    r("section.rename", r#"{"index": 1, "name": "X"}"#, "", &[("$Section:Main", "name", "*")]),
    // section.remove: the section at `index`; its slides join the section before (else the next), or with `slides` they are deleted whole.
    r("section.remove", r#"{"index": 1}"#, "", &[("$Section:Intro", "slides", "-"), ("$Section:Main", "*", "*")]),
    r("section.remove", r#"{"index": 0}"#, "", &[("$Section:Intro", "*", "*"), ("$Section:Main", "slides", "-")]),
    r(
        "section.remove",
        r#"{"index": 1, "slides": true}"#,
        "",
        &[
            ("$Section:Main", "*", "*"),
            ("$S2", "*", "-"),
            ("$S3", "*", "-"),
            ("$Star", "*", "-"),
            ("$Title", "*", "-"),
            ("$Body", "*", "-"),
            ("$Rect", "*", "-"),
            ("$Oval", "*", "-"),
            ("$Line", "*", "-"),
            ("$Group", "*", "-"),
            ("$GA", "*", "-"),
            ("$GB", "*", "-"),
            ("$Table", "*", "-"),
            ("$Chart", "*", "-"),
            ("$Text", "*", "-"),
            ("$Pic", "*", "-"),
            ("$Video", "*", "-"),
            ("$Thread", "*", "-"),
        ],
    ),
    // section.removeAll: the sections cleared: the section list and every section whole.
    r("section.removeAll", r#"{}"#, "", &[("root", "sections", "-"), ("$Section:Intro", "*", "-"), ("$Section:Main", "*", "-")]),
    // C8-slide-pin r2: slide.rs:588-600 moves the addressed section; the sections beside its destination locate it.
    // section.move: the section at `index` (where it is) moves to `to`; the slides are resorted by section order.
    r("section.move", r#"{"index": 1, "to": 0}"#, "", &[("$Section:Main", "*", "*"), ("$Section:Intro", "slides.$S1", "*")]),
    // slide.fromOutline: a slide per top-level line on the first master's Title and Content layout (by kind), title and body placeholders filled; inserted after the current slide; they refit.
    r(
        "slide.fromOutline",
        r#"{"text": "A\n\tB"}"#,
        "part",
        &[
            ("$Layout:Title Slide", "props.kind", "-"),
            ("$Layout:Title and Content", "props.kind,@order(shapes),~shapeCount", "-"),
            ("$Layout:Title and Content/Title 1", "body.props,look.ph,look.style,name,xfrm", "-"),
            ("$Layout:Title and Content/Content Placeholder 2", "body.props,look.ph,look.style,name,xfrm", "-"),
            ("$Layout:Title and Content/Date Placeholder", "look.ph", "-"),
            ("$Layout:Title and Content/Footer Placeholder", "look.ph", "-"),
            ("$Layout:Title and Content/Slide Number Placeholder", "look.ph", "-"),
            ("$Master/Title 1", "body.props", "-"),
            ("$Master/Text Placeholder 2", "body.props", "-"),
            ("$Master", "layouts,listStyles,theme.fonts", "-"),
            ("root", "defaultTextStyle", "-"),
        ],
    ),
];
