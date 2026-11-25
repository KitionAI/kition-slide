use super::{Rest, pinned, r, rs};

#[allow(unused_imports)]
use pinned as _;

pub const ROWS: &[Rest] = &[
    // field.insertSlideNumber: editing text (selection.text) inserts a slidenum field at the caret of the edited body; otherwise
    // slide.headerFooter {slideNumber: true} on every slide: headerFooter edited from its value, each slide's layout and shapes
    // (placeholder kinds) read, the layout's Slide Number placeholder (name, ph) copied into a new shape on the slide.
    r(
        "field.insertSlideNumber",
        r#"{}"#,
        "selection",
        &[
            ("root", "headerFooter", "-"),
            ("$S1", "layout,shapes", "-"),
            ("$S2", "layout,shapes", "-"),
            ("$S3", "layout,shapes", "-"),
            ("$Layout:Title Slide", "shapes", "-"),
            ("$Layout:Title and Content", "shapes", "-"),
            ("$Layout:Blank", "shapes", "-"),
            ("$Layout:Title Slide/Slide Number Placeholder", "look.ph,name", "-"),
            ("$Layout:Title and Content/Slide Number Placeholder", "look.ph,name", "-"),
            ("$Layout:Blank/Slide Number Placeholder", "look.ph,name", "-"),
        ],
    ),
    // normalized after review: the edit goes through edit_text, whose fit_text_box resolves the body properties for the autofit mode (crates/engine/engine/src/cmd/text.rs:83-92) and autofit::invalidate compares them (crates/engine/engine/src/autofit.rs:10-21); the rows of text.insert read the same path
    pinned(rs(
        r#"[["text.edit", {"id": "$Rect"}]]"#,
        "field.insertSlideNumber",
        r#"{}"#,
        "selection",
        &[("$Rect", "body.paragraphs,body.props", "-")],
    )),
    rs(
        r#"[["text.edit", {"id": "$Table", "cell": [1, 1]}]]"#,
        "field.insertSlideNumber",
        r#"{}"#,
        "selection",
        &[("$Table", "table.cells,table.cols,table.rows", "-")],
    ),
    // field.insertDateTime: as field.insertSlideNumber; editing text inserts a date field (format param) at the caret, otherwise
    // slide.headerFooter {date: true} (the format is not used) adds each layout's Date placeholder to every slide.
    r(
        "field.insertDateTime",
        r#"{"format": "datetime2"}"#,
        "selection",
        &[
            ("root", "headerFooter", "-"),
            ("$S1", "layout,shapes", "-"),
            ("$S2", "layout,shapes", "-"),
            ("$S3", "layout,shapes", "-"),
            ("$Layout:Title Slide", "shapes", "-"),
            ("$Layout:Title and Content", "shapes", "-"),
            ("$Layout:Blank", "shapes", "-"),
            ("$Layout:Title Slide/Date Placeholder", "look.ph,name", "-"),
            ("$Layout:Title and Content/Date Placeholder", "look.ph,name", "-"),
            ("$Layout:Blank/Date Placeholder", "look.ph,name", "-"),
        ],
    ),
    // normalized after review: fit_text_box lays the text out to fit (crates/engine/engine/src/cmd/text.rs:83-125): `body` spelled as its two parts, plus the geometry (shape_geometry), the shape style and default text style (level_style, crates/engine/model/src/resolve.rs:264-275), the theme fonts (font_family, resolve.rs:309-313) and the slide's layout the context is found through (text.rs:138-141), as the text.insert rows read
    pinned(rs(
        r#"[["text.edit", {"id": "$Text"}]]"#,
        "field.insertDateTime",
        r#"{"format": "datetime2"}"#,
        "selection",
        &[
            ("$Text", "xfrm,body.paragraphs,body.props,look.geom,look.style", "-"),
            ("$Master", "theme.fonts", "-"),
            ("root", "defaultTextStyle", "-"),
            ("$S2", "layout", "-"),
        ],
    )),
    // normalized after review: the edit goes through edit_text, whose fit_text_box resolves the body properties for the autofit mode (crates/engine/engine/src/cmd/text.rs:83-92) and autofit::invalidate compares them (crates/engine/engine/src/autofit.rs:10-21); the rows of text.insert read the same path
    pinned(rs(r#"[["text.edit", {"id": "$Rect"}]]"#, "field.insertDateTime", r#"{}"#, "selection", &[("$Rect", "body.paragraphs,body.props", "-")])),
    // symbol.insert: only while editing text; inserts the characters at the caret (selection replaced) of the edited body (shape,
    // table cell, notes). A Shrink placeholder ($Title) is refitted: body.props from its text laid out in the inherited frame,
    // body properties and list styles; an auto-size text box ($Text) is refitted (its xfrm).
    // normalized after review: the edit goes through edit_text, whose fit_text_box resolves the body properties for the autofit mode (crates/engine/engine/src/cmd/text.rs:83-92) and autofit::invalidate compares them (crates/engine/engine/src/autofit.rs:10-21); the rows of text.insert read the same path
    pinned(rs(
        r#"[["text.edit", {"id": "$Rect"}]]"#,
        "symbol.insert",
        r#"{"text": "©"}"#,
        "selection",
        &[("$Rect", "body.paragraphs,body.props", "-")],
    )),
    // normalized after review: the shrink refit lays the placeholder out (crates/engine/engine/src/autofit.rs:37-100): `body` spelled as its two parts, plus its frame, placeholder, geometry and style, the layout placeholder's style and the default text style (resolve.rs:119-139, 264-275), as the text.insert row on $Title reads
    pinned(rs(
        r#"[["text.edit", {"id": "$Title"}]]"#,
        "symbol.insert",
        r#"{"text": "©"}"#,
        "selection",
        &[
            ("$Title", "xfrm,body.paragraphs,body.props,look.ph,look.geom,look.style", "-"),
            ("$S2", "layout", "-"),
            ("$Layout:Title and Content/Title 1", "body.props,look.ph,look.style,xfrm", "-"),
            ("$Master/Title 1", "body.props,look.ph", "-"),
            ("$Master", "listStyles.title,theme.fonts", "-"),
            ("root", "defaultTextStyle", "-"),
        ],
    )),
    // normalized after review: fit_text_box lays the text out to fit (crates/engine/engine/src/cmd/text.rs:83-125): `body` spelled as its two parts, plus the geometry (shape_geometry), the shape style and default text style (level_style, crates/engine/model/src/resolve.rs:264-275), the theme fonts (font_family, resolve.rs:309-313) and the slide's layout the context is found through (text.rs:138-141), as the text.insert rows read
    pinned(rs(
        r#"[["text.edit", {"id": "$Text"}]]"#,
        "symbol.insert",
        r#"{"text": "©"}"#,
        "selection",
        &[
            ("$Text", "xfrm,body.paragraphs,body.props,look.geom,look.style", "-"),
            ("$Master", "theme.fonts", "-"),
            ("root", "defaultTextStyle", "-"),
            ("$S2", "layout", "-"),
        ],
    )),
    rs(
        r#"[["text.edit", {"id": "$Table", "cell": [1, 1]}]]"#,
        "symbol.insert",
        r#"{"text": "©"}"#,
        "selection",
        &[("$Table", "table.cells,table.cols,table.rows", "-")],
    ),
    rs(r#"[["text.edit", {"notes": true}]]"#, "symbol.insert", r#"{"text": "©"}"#, "part,selection", &[("$S2", "notesBody", "-")]),
    // link.insert: with id/ids (or no text being edited: the selected shapes) sets look.click of each target, giving a
    // placeholder without its own frame the inherited one; `slide` links to the slide at that index (its id; its title is the
    // text inserted at a bare caret). Editing text with no id: links the selection or the word at the caret, else inserts the text.
    r("link.insert", r#"{"url": "https://example.com", "ids": ["$Oval"]}"#, "", &[("$Oval", "look.click", "-")]),
    r("link.insert", r#"{"url": "https://example.com"}"#, "selection", &[("$Rect", "look.click", "-")]),
    r("link.insert", r#"{"slide": 2, "ids": ["$Group"]}"#, "", &[("$Group", "look.click", "-"), ("$S3", "-", "*")]),
    // normalized after review: giving the placeholder its inherited frame runs the parent lookup, which reads the target's own placeholder and the master's placeholder kinds up to its Title (crates/engine/model/src/resolve.rs:76-92)
    pinned(r(
        "link.insert",
        r#"{"url": "https://example.com", "ids": ["$Title"]}"#,
        "",
        &[
            ("$Title", "look.click,look.ph,xfrm", "-"),
            ("$S2", "layout", "-"),
            ("$Layout:Title and Content/Title 1", "look.ph,xfrm", "-"),
            ("$Master/Title 1", "look.ph", "-"),
        ],
    )),
    // normalized after review: the edit goes through edit_text, whose fit_text_box resolves the body properties for the autofit mode (crates/engine/engine/src/cmd/text.rs:83-92) and autofit::invalidate compares them (crates/engine/engine/src/autofit.rs:10-21); the rows of text.insert read the same path
    pinned(rs(
        r#"[["text.edit", {"id": "$Rect", "at": [0, 1]}]]"#,
        "link.insert",
        r#"{"url": "https://example.com"}"#,
        "selection",
        &[("$Rect", "body.paragraphs,body.props", "-")],
    )),
    // normalized after review: the edit goes through edit_text, whose fit_text_box resolves the body properties for the autofit mode (crates/engine/engine/src/cmd/text.rs:83-92) and autofit::invalidate compares them (crates/engine/engine/src/autofit.rs:10-21); the rows of text.insert read the same path
    pinned(rs(
        r#"[["text.edit", {"id": "$Rect"}]]"#,
        "link.insert",
        r#"{"slide": 0}"#,
        "selection",
        &[("$Rect", "body.paragraphs,body.props", "-"), ("$S1", "shapes", "*"), ("$S1Title", "body.paragraphs,look.ph", "-")],
    )),
    // table.cellFormat: the table by id or the first selected shape; cells (and the border range) from the selected cells or
    // the edited cell, else the whole table (spans via cell owners); sets anchor/margins/borders in those cells.
    r("table.cellFormat", r#"{"id": "$Table", "anchor": "center"}"#, "selection", &[("$Table", "table.cells,table.cols,table.rows", "-")]),
    r(
        "table.cellFormat",
        r#"{"id": 269, "onSlide": 2, "margins": [1, 1, 1, 1], "border": "all"}"#,
        "selection",
        &[("$Table", "table.cells,table.cols,table.rows", "address"), ("$S2", "-", "*")],
    ),
    rs(
        r#"[["table.selectCells", {"id": "$Table", "from": [0, 0], "to": [0, 1]}]]"#,
        "table.cellFormat",
        r#"{"anchor": "bottom"}"#,
        "selection",
        &[("$Table", "table.cells,table.cols,table.rows", "-")],
    ),
    // table.border: the table by id or the first selected shape; range from/to, else selected cells, else the whole table;
    // sets the edges' lines in the cells of the range (and their neighbours' shared edges).
    r("table.border", r#"{"id": "$Table", "edges": "all"}"#, "selection", &[("$Table", "table.cells,table.cols,table.rows", "-")]),
    r(
        "table.border",
        r#"{"id": "$Table", "edges": "outside", "from": [0, 0], "to": [1, 0]}"#,
        "",
        &[("$Table", "table.cells,table.cols,table.rows", "-")],
    ),
    rs(
        r#"[["table.selectCells", {"id": "$Table", "from": [1, 0], "to": [1, 1]}]]"#,
        "table.border",
        r#"{"edges": "bottom", "none": true}"#,
        "selection",
        &[("$Table", "table.cells,table.cols,table.rows", "-")],
    ),
    // table.clearCells: the table by id or the first selected shape; the listed cells, else the selected cells, else every cell
    // (cell owners); empties their text.
    r("table.clearCells", r#"{"id": "$Table", "cells": [[0, 1]]}"#, "", &[("$Table", "table.cells,table.cols,table.rows", "-")]),
    r("table.clearCells", r#"{"id": "$Table"}"#, "selection", &[("$Table", "table.cells,table.cols,table.rows", "-")]),
    rs(
        r#"[["table.selectCells", {"id": "$Table", "from": [0, 0], "to": [1, 0]}]]"#,
        "table.clearCells",
        r#"{}"#,
        "selection",
        &[("$Table", "table.cells,table.cols,table.rows", "-")],
    ),
    // table.insertRowAbove: the table by id, else the edited cell's table, the selected cells' table or the first selected shape;
    // the row from `row`, else the edited or selected cell, else 0; inserts a row (height copied from the row before), the
    // frame's height grows to the rows' sum.
    r("table.insertRowAbove", r#"{"id": "$Table", "row": 1}"#, "", &[("$Table", "table.cells,table.cols,table.rows,xfrm", "-")]),
    r("table.insertRowAbove", r#"{"id": "$Table"}"#, "selection", &[("$Table", "table.cells,table.cols,table.rows,xfrm", "-")]),
    rs(
        r#"[["edit.select", {"ids": ["$Table"]}]]"#,
        "table.insertRowAbove",
        r#"{"row": 0}"#,
        "selection",
        &[("$Table", "table.cells,table.cols,table.rows,xfrm", "-")],
    ),
    // table.insertRowBelow: as insertRowAbove, after the row.
    r("table.insertRowBelow", r#"{"id": "$Table", "row": 0}"#, "", &[("$Table", "table.cells,table.cols,table.rows,xfrm", "-")]),
    r(
        "table.insertRowBelow",
        r#"{"id": 269, "onSlide": 2, "row": 1}"#,
        "",
        &[("$Table", "table.cells,table.cols,table.rows,xfrm", "address"), ("$S2", "-", "*")],
    ),
    rs(
        r#"[["text.edit", {"id": "$Table", "cell": [1, 1]}]]"#,
        "table.insertRowBelow",
        r#"{}"#,
        "selection",
        &[("$Table", "table.cells,table.cols,table.rows,xfrm", "-")],
    ),
    // table.insertColumnLeft: as insertRowAbove for columns (`col`; width copied from the column before; the frame widens).
    r("table.insertColumnLeft", r#"{"id": "$Table", "col": 1}"#, "", &[("$Table", "table.cells,table.cols,table.rows,xfrm", "-")]),
    rs(
        r#"[["table.selectCells", {"id": "$Table", "from": [0, 1], "to": [1, 1]}]]"#,
        "table.insertColumnLeft",
        r#"{}"#,
        "selection",
        &[("$Table", "table.cells,table.cols,table.rows,xfrm", "-")],
    ),
    // table.insertColumnRight: as insertColumnLeft, after the column.
    r("table.insertColumnRight", r#"{"id": "$Table", "col": 0}"#, "", &[("$Table", "table.cells,table.cols,table.rows,xfrm", "-")]),
    rs(
        r#"[["edit.select", {"ids": ["$Table"]}]]"#,
        "table.insertColumnRight",
        r#"{}"#,
        "selection",
        &[("$Table", "table.cells,table.cols,table.rows,xfrm", "-")],
    ),
    // table.deleteRow: the table and row as insertRowAbove; removes the row (merges adjusted), the frame shrinks.
    r("table.deleteRow", r#"{"id": "$Table", "row": 1}"#, "", &[("$Table", "table.cells,table.cols,table.rows,xfrm", "-")]),
    rs(
        r#"[["text.edit", {"id": "$Table", "cell": [0, 1]}]]"#,
        "table.deleteRow",
        r#"{}"#,
        "selection",
        &[("$Table", "table.cells,table.cols,table.rows,xfrm", "-")],
    ),
    // table.deleteColumn: as deleteRow for columns.
    r("table.deleteColumn", r#"{"id": "$Table", "col": 0}"#, "", &[("$Table", "table.cells,table.cols,table.rows,xfrm", "-")]),
    r(
        "table.deleteColumn",
        r#"{"id": 269, "onSlide": 2}"#,
        "selection",
        &[("$Table", "table.cells,table.cols,table.rows,xfrm", "address"), ("$S2", "-", "*")],
    ),
    // table.merge: from/to, else the selected cells of that table; merges the range (texts joined into the first cell).
    r("table.merge", r#"{"id": "$Table", "from": [0, 0], "to": [0, 1]}"#, "", &[("$Table", "table.cells,table.cols,table.rows", "-")]),
    rs(
        r#"[["table.selectCells", {"id": "$Table", "from": [0, 0], "to": [1, 0]}]]"#,
        "table.merge",
        r#"{}"#,
        "selection",
        &[("$Table", "table.cells,table.cols,table.rows", "-")],
    ),
    // table.split: `cell`, else the edited or selected cell, else (0,0); unmerges the merge that cell belongs to.
    rs(
        r#"[["table.merge", {"id": "$Table", "from": [0, 0], "to": [0, 1]}]]"#,
        "table.split",
        r#"{"id": "$Table", "cell": [0, 0]}"#,
        "",
        &[("$Table", "table.cells,table.cols,table.rows", "-")],
    ),
    rs(
        r#"[["table.merge", {"id": "$Table", "from": [0, 0], "to": [1, 0]}], ["edit.select", {"ids": ["$Table"]}]]"#,
        "table.split",
        r#"{}"#,
        "selection",
        &[("$Table", "table.cells,table.cols,table.rows", "-")],
    ),
    // table.style: replaces the style in the table's props (with the options).
    r("table.style", r#"{"id": "$Table", "style": "grid"}"#, "", &[("$Table", "table.props", "-")]),
    rs(r#"[["edit.select", {"ids": ["$Table"]}]]"#, "table.style", r#"{"style": "light1-accent2"}"#, "selection", &[("$Table", "table.props", "-")]),
    // table.options: sets the given style options in the table's props, keeping the others.
    r("table.options", r#"{"id": "$Table", "headerRow": false, "bandedColumns": true}"#, "", &[("$Table", "table.props", "-")]),
    r("table.options", r#"{"id": 269, "onSlide": 2, "totalRow": true}"#, "", &[("$Table", "table.props", "address"), ("$S2", "-", "*")]),
    // table.cellFill: the table by id or the first selected shape; `cells`, else selected cells, else all; sets their fill.
    r("table.cellFill", r##"{"id": "$Table", "color": "#FF0000"}"##, "selection", &[("$Table", "table.cells,table.cols,table.rows", "-")]),
    r("table.cellFill", r#"{"id": "$Table", "none": true, "cells": [[1, 0], [1, 1]]}"#, "", &[("$Table", "table.cells,table.cols,table.rows", "-")]),
    rs(
        r#"[["table.selectCells", {"id": "$Table", "from": [1, 1], "to": [1, 1]}]]"#,
        "table.cellFill",
        r#"{"color": "accent2"}"#,
        "selection",
        &[("$Table", "table.cells,table.cols,table.rows", "-")],
    ),
    // table.distributeRows: every row's height := the rows' total / count (the frame keeps its height).
    r("table.distributeRows", r#"{"id": "$Table"}"#, "", &[("$Table", "table.rows", "-")]),
    rs(r#"[["edit.select", {"ids": ["$Table"]}]]"#, "table.distributeRows", r#"{}"#, "selection", &[("$Table", "table.rows", "-")]),
    // table.distributeColumns: every column's width := the columns' total / count.
    r("table.distributeColumns", r#"{"id": "$Table"}"#, "", &[("$Table", "table.cols", "-")]),
    r("table.distributeColumns", r#"{"id": 269, "onSlide": 2}"#, "", &[("$Table", "table.cols", "address"), ("$S2", "-", "*")]),
    // table.columnWidth: column `col`, else the edited/selected cell's, else 0; sets its width, the frame's width := the columns' sum.
    r("table.columnWidth", r#"{"id": "$Table", "col": 1, "width": 120}"#, "", &[("$Table", "table.cols,xfrm", "-")]),
    rs(
        r#"[["text.edit", {"id": "$Table", "cell": [0, 1]}]]"#,
        "table.columnWidth",
        r#"{"width": 120}"#,
        "selection",
        &[("$Table", "table.cols,xfrm", "-")],
    ),
    // table.rowHeight: row `row`, else the edited/selected cell's, else 0; sets its height, the frame's height := the rows' sum.
    r("table.rowHeight", r#"{"id": "$Table", "row": 1, "height": 60}"#, "", &[("$Table", "table.rows,xfrm", "-")]),
    r("table.rowHeight", r#"{"id": "$Table", "height": 60}"#, "selection", &[("$Table", "table.rows,xfrm", "-")]),
    rs(
        r#"[["table.selectCells", {"id": "$Table", "from": [1, 0], "to": [1, 1]}]]"#,
        "table.rowHeight",
        r#"{"height": 60}"#,
        "selection",
        &[("$Table", "table.rows,xfrm", "-")],
    ),
];
