use super::{Rest, pinned, r, rs};

#[allow(unused_imports)]
use pinned as _;

// Group b: shape.*, the inserts of shapes, pictures, tables, charts and media, chart.*.
//
// Shared paths (read in the engine):
// - edit_shapes (cmd/mod.rs): `ids`/`id` else the selection, local numbers in the current tree;
//   a target without its own `xfrm` (the placeholders `$Title`, `$Body`) gets the box it inherits
//   from its layout placeholder (matched by `look.ph`) written into `xfrm`; a group member's box
//   goes through its group's `xfrm` and child space (`look.kind`).
// - Session::edit: groups whose members' boxes changed are refit to their members' union (reads
//   every member's `xfrm`, writes the group's `xfrm` and child space); connectors glued to a shape
//   whose box or geometry changed are rerouted from both ends' sites (reads the connector's
//   `look.kind` (glue), `look.geom` (elbow or not), `xfrm`, the ends' `look.geom` and `xfrm`;
//   rewrites the connector's `xfrm`, drops glue to a shape that is gone); shrink-to-fit bodies whose
//   box, geometry or text changed are refit (reads the layout and master placeholders' body and
//   list styles, the master's list styles and theme fonts, the deck's default text style; writes
//   the body's autofit).
// - add_shape (cmd/insert.rs): the new shape is named after the current tree's top-level shape
//   count ("Rectangle 13"), which the stored name keeps; its number only has to be unique.
// - Text boxes and art text are fitted to their text: the slide's layout (its master), the master's
//   theme fonts and the deck's default text style.
// - Media are deduplicated by their bytes: `RED` (the fixture's picture) reuses `$Media:red.png`.
// - A chart's members are entries of its Chart object, which the shape's `chart` names (and owns);
//   that object has no role, so its members are spelled `chart.<member>` on the chart shape.
pub const ROWS: &[Rest] = &[
    // shape.insert: appends to the current tree (or the slide onSlide names), named by its
    // shape count; no rect: a 72 pt box centred on the slide size. Default look is session state.
    r("shape.insert", r##"{"preset": "rect", "rect": [10, 10, 50, 50]}"##, "part", &[("$S2", "~shapeCount", "-")]),
    r("shape.insert", r##"{"preset": "ellipse"}"##, "part", &[("$S2", "~shapeCount", "-"), ("root", "props.slideSize", "-")]),
    r("shape.insert", r##"{"preset": "line", "rect": [10, 10, 50, 0]}"##, "part", &[("$S2", "~shapeCount", "-")]),
    r("shape.insert", r##"{"preset": "rect", "rect": [10, 10, 50, 50], "onSlide": 3}"##, "", &[("$S3", "~shapeCount", "*")]),
    // textBox.insert: appends a text box named by shape count, then fits its box to the text
    // (layout -> master theme fonts, default text style); no rect: centred on the slide size.
    r(
        "textBox.insert",
        r##"{"rect": [10, 10, 200, 40], "text": "Hi"}"##,
        "part",
        &[("$S2", "layout,~shapeCount", "-"), ("$Master", "theme.fonts", "-"), ("root", "defaultTextStyle", "-")],
    ),
    r(
        "textBox.insert",
        r##"{"text": "Hi"}"##,
        "part",
        &[("$S2", "layout,~shapeCount", "-"), ("$Master", "theme.fonts", "-"), ("root", "defaultTextStyle,props.slideSize", "-")],
    ),
    r(
        "textBox.insert",
        r##"{"rect": [10, 10, 200, 40], "text": "Hi", "onSlide": 3}"##,
        "",
        &[("$S3", "layout,~shapeCount", "*"), ("$Master", "theme.fonts", "-"), ("root", "defaultTextStyle", "-")],
    ),
    // image.insert: a selected empty picture/content/body placeholder takes the picture (reads
    // the selection's `look.ph` and text, the placeholder's inherited box; replaces its kind,
    // frame, placeholder kind, text, alt text, media); else a new picture named by shape count, no
    // rect: fitted to and centred on the slide size. RED reuses $Media:red.png.
    r(
        "image.insert",
        r##"{"name": "b.png", "data": "iVBORw0KGgoAAAANSUhEUgAAAAIAAAABCAIAAAB7QOjdAAAADUlEQVR4nGNgYPgPRAAFAgH/wSuWnwAAAABJRU5ErkJggg==", "rect": [10, 10, 50, 50]}"##,
        "part,selection",
        &[("$S2", "~shapeCount", "-"), ("$Rect", "look.ph", "-")],
    ),
    r(
        "image.insert",
        r##"{"name": "b.png", "data": "iVBORw0KGgoAAAANSUhEUgAAAAIAAAABCAIAAAB7QOjdAAAADUlEQVR4nGNgYPgPRAAFAgH/wSuWnwAAAABJRU5ErkJggg=="}"##,
        "part,selection",
        &[("$S2", "~shapeCount", "-"), ("$Rect", "look.ph", "-"), ("root", "props.slideSize", "-")],
    ),
    r(
        "image.insert",
        r##"{"name": "red.png", "data": "iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAIAAACQd1PeAAAADElEQVR4nGM4wcAAAAJcAMlDyBJGAAAAAElFTkSuQmCC", "rect": [10, 10, 50, 50]}"##,
        "part,selection",
        &[("$S2", "~shapeCount", "-"), ("$Rect", "look.ph", "-"), ("$Media:red.png", "blob", "-")],
    ),
    // normalized after review: the setup's text.set wrote the placeholder its own box (edit_shapes), so
    // `xfrm_of` reads only that (cmd/mod.rs:293); the layout is not read.
    pinned(rs(
        r##"[["text.set", {"id": "$Body", "text": ""}], ["edit.select", {"ids": ["$Body"]}]]"##,
        "image.insert",
        r##"{"name": "b.png", "data": "iVBORw0KGgoAAAANSUhEUgAAAAIAAAABCAIAAAB7QOjdAAAADUlEQVR4nGNgYPgPRAAFAgH/wSuWnwAAAABJRU5ErkJggg=="}"##,
        "part,selection",
        &[("$Body", "body,kind,look.descr,look.kind,look.ph,media,xfrm", "-")],
    )),
    r(
        "image.insert",
        r##"{"name": "b.png", "data": "iVBORw0KGgoAAAANSUhEUgAAAAIAAAABCAIAAAB7QOjdAAAADUlEQVR4nGNgYPgPRAAFAgH/wSuWnwAAAABJRU5ErkJggg==", "rect": [10, 10, 50, 50], "onSlide": 3}"##,
        "",
        &[("$S3", "~shapeCount", "*")],
    ),
    // table.insert: a new table named by shape count; no rect: 80 % of the slide width.
    r("table.insert", r##"{"rows": 2, "cols": 3, "rect": [10, 10, 300, 80], "data": [["x", "y"]]}"##, "part", &[("$S2", "~shapeCount", "-")]),
    r("table.insert", r##"{"rows": 2, "cols": 3}"##, "part", &[("$S2", "~shapeCount", "-"), ("root", "props.slideSize", "-")]),
    r("table.insert", r##"{"rows": 2, "cols": 2, "rect": [10, 10, 300, 80], "onSlide": 3}"##, "", &[("$S3", "~shapeCount", "*")]),
    // chart.insert: a new chart from the values, named by shape count; no rect: 70 % of the slide.
    r(
        "chart.insert",
        r##"{"type": "pie", "rect": [10, 10, 200, 150], "categories": ["A", "B"], "series": [{"name": "S", "values": [1, 2]}]}"##,
        "part",
        &[("$S2", "~shapeCount", "-")],
    ),
    r("chart.insert", r##"{"type": "line"}"##, "part", &[("$S2", "~shapeCount", "-"), ("root", "props.slideSize", "-")]),
    r("chart.insert", r##"{"type": "column", "rect": [10, 10, 200, 150], "onSlide": 3}"##, "", &[("$S3", "~shapeCount", "*")]),
    // audio.insert: new media and a clip named by shape count; no rect: a 48 pt box centred on
    // the slide size.
    r("audio.insert", r##"{"name": "a.mp3", "data": "SUQzAwAAAAAAAA==", "rect": [10, 10, 48, 48]}"##, "part", &[("$S2", "~shapeCount", "-")]),
    r("audio.insert", r##"{"name": "a.mp3", "data": "SUQzAwAAAAAAAA=="}"##, "part", &[("$S2", "~shapeCount", "-"), ("root", "props.slideSize", "-")]),
    r(
        "audio.insert",
        r##"{"name": "a.mp3", "data": "SUQzAwAAAAAAAA==", "rect": [10, 10, 48, 48], "onSlide": 3}"##,
        "",
        &[("$S3", "~shapeCount", "*")],
    ),
    // video.insert: new media (and poster) and a clip named by shape count; no rect: its natural
    // size fitted into the slide size.
    r(
        "video.insert",
        r##"{"name": "v.mp4", "data": "AAAAEGZ0eXBpc29tAAACAA==", "rect": [10, 10, 160, 90]}"##,
        "part",
        &[("$S2", "~shapeCount", "-")],
    ),
    r(
        "video.insert",
        r##"{"name": "v.mp4", "data": "AAAAEGZ0eXBpc29tAAACAA=="}"##,
        "part",
        &[("$S2", "~shapeCount", "-"), ("root", "props.slideSize", "-")],
    ),
    r(
        "video.insert",
        r##"{"name": "v.mp4", "data": "AAAAEGZ0eXBpc29tAAACAA==", "rect": [10, 10, 160, 90], "onSlide": 3}"##,
        "",
        &[("$S3", "~shapeCount", "*")],
    ),
    // artText.insert: its box always comes from the slide size; named by shape count; fitted to
    // its text (layout -> master theme fonts, default text style).
    r(
        "artText.insert",
        r##"{"text": "Wow", "style": 2}"##,
        "part",
        &[("$S2", "layout,~shapeCount", "-"), ("$Master", "theme.fonts", "-"), ("root", "defaultTextStyle,props.slideSize", "-")],
    ),
    r(
        "artText.insert",
        r##"{"text": "Wow", "onSlide": 3}"##,
        "",
        &[("$S3", "layout,~shapeCount", "*"), ("$Master", "theme.fonts", "-"), ("root", "defaultTextStyle,props.slideSize", "-")],
    ),
    // chart.type: id else the selection; reads the chart's type, series (their own types, their
    // count) and varyColors; writes type, series types, varyColors.
    // normalized after review: a chart member is an entry of its Chart object's `parts` map, which the
    // change sets name `chart.parts.<member>` on the shape (kdm-model src/charts.rs:56).
    pinned(r(
        "chart.type",
        r##"{"id": "$Chart", "type": "line"}"##,
        "",
        &[("$Chart", "chart.parts.kind,chart.parts.series,chart.parts.varyColors", "-")],
    )),
    // normalized after review: a chart member is an entry of its Chart object's `parts` map, which the
    // change sets name `chart.parts.<member>` on the shape (kdm-model src/charts.rs:56).
    pinned(rs(
        r##"[["edit.select", {"ids": ["$Chart"]}]]"##,
        "chart.type",
        r##"{"type": "combo"}"##,
        "part,selection",
        &[("$Chart", "chart.parts.kind,chart.parts.series,chart.parts.varyColors", "-")],
    )),
    // normalized after review: a chart member is an entry of its Chart object's `parts` map, which the
    // change sets name `chart.parts.<member>` on the shape (kdm-model src/charts.rs:56).
    pinned(r(
        "chart.type",
        r##"{"id": 270, "type": "bar"}"##,
        "part",
        &[("$Chart", "chart.parts.kind,chart.parts.series,chart.parts.varyColors", "address")],
    )),
    // normalized after review: a chart member is an entry of its Chart object's `parts` map, which the
    // change sets name `chart.parts.<member>` on the shape (kdm-model src/charts.rs:56).
    pinned(r(
        "chart.type",
        r##"{"id": 270, "onSlide": 2, "type": "bar"}"##,
        "",
        &[("$S2", "-", "*"), ("$Chart", "chart.parts.kind,chart.parts.series,chart.parts.varyColors", "address")],
    )),
    // chart.data: categories replaced; series replaced, each keeping the formatting of the old
    // series at its position (an edit of the series from their current value).
    // normalized after review: a chart member is an entry of its Chart object's `parts` map, which the
    // change sets name `chart.parts.<member>` on the shape (kdm-model src/charts.rs:56).
    pinned(r(
        "chart.data",
        r##"{"id": "$Chart", "categories": ["A", "B"], "series": [{"name": "S", "values": [3, 4]}]}"##,
        "",
        &[("$Chart", "chart.parts.categories,chart.parts.series", "-")],
    )),
    // normalized after review: a chart member is an entry of its Chart object's `parts` map, which the
    // change sets name `chart.parts.<member>` on the shape (kdm-model src/charts.rs:56).
    pinned(r("chart.data", r##"{"id": "$Chart", "categories": ["A", "B", "C"]}"##, "", &[("$Chart", "chart.parts.categories", "-")])),
    // normalized after review: a chart member is an entry of its Chart object's `parts` map, which the
    // change sets name `chart.parts.<member>` on the shape (kdm-model src/charts.rs:56).
    pinned(rs(
        r##"[["edit.select", {"ids": ["$Chart"]}]]"##,
        "chart.data",
        r##"{"series": [{"name": "S", "values": [3, 4], "color": "#00ff00"}]}"##,
        "part,selection",
        &[("$Chart", "chart.parts.series", "-")],
    )),
    // normalized after review: a chart member is an entry of its Chart object's `parts` map, which the
    // change sets name `chart.parts.<member>` on the shape (kdm-model src/charts.rs:56).
    pinned(r(
        "chart.data",
        r##"{"id": 270, "onSlide": 2, "categories": ["A"]}"##,
        "",
        &[("$S2", "-", "*"), ("$Chart", "chart.parts.categories", "address")],
    )),
    // chart.options: each given option replaces its member; colors go per point when the chart
    // varies colours with one series, else per series (line types also colour the line).
    // normalized after review: a chart member is an entry of its Chart object's `parts` map, which the
    // change sets name `chart.parts.<member>` on the shape (kdm-model src/charts.rs:56).
    pinned(r("chart.options", r##"{"id": "$Chart", "title": "T", "legend": "b"}"##, "", &[("$Chart", "chart.parts.legend,chart.parts.title", "-")])),
    // normalized after review: a chart member is an entry of its Chart object's `parts` map, which the
    // change sets name `chart.parts.<member>` on the shape (kdm-model src/charts.rs:56).
    pinned(r(
        "chart.options",
        r##"{"id": "$Chart", "gapWidth": 50, "overlap": 10}"##,
        "",
        &[("$Chart", "chart.parts.gapWidth,chart.parts.overlap", "-")],
    )),
    // normalized after review: a chart member is an entry of its Chart object's `parts` map, which the
    // change sets name `chart.parts.<member>` on the shape (kdm-model src/charts.rs:56).
    pinned(r(
        "chart.options",
        r##"{"id": "$Chart", "colors": ["#ff0000"]}"##,
        "",
        &[("$Chart", "chart.parts.kind,chart.parts.series,chart.parts.varyColors", "-")],
    )),
    // normalized after review: a chart member is an entry of its Chart object's `parts` map, which the
    // change sets name `chart.parts.<member>` on the shape (kdm-model src/charts.rs:56).
    pinned(rs(
        r##"[["edit.select", {"ids": ["$Chart"]}]]"##,
        "chart.options",
        r##"{"dataLabels": true, "gridlines": false, "palette": "colorful2"}"##,
        "part,selection",
        &[("$Chart", "chart.parts.dataLabels,chart.parts.gridlines,chart.parts.palette", "-")],
    )),
    // normalized after review: a chart member is an entry of its Chart object's `parts` map, which the
    // change sets name `chart.parts.<member>` on the shape (kdm-model src/charts.rs:56).
    pinned(r("chart.options", r##"{"id": 270, "title": null}"##, "part", &[("$Chart", "chart.parts.title", "address")])),
    // smartGraphic.insert: appends link and node shapes named by shape count, groups them (a new
    // group named by count, then renamed) and sets the group's frame to the area; no rect: from
    // the slide size.
    r(
        "smartGraphic.insert",
        r##"{"kind": "process", "items": ["A", "B", "C"], "rect": [10, 10, 400, 200]}"##,
        "part",
        &[("$S2", "~shapeCount", "-")],
    ),
    r("smartGraphic.insert", r##"{"kind": "cycle", "items": ["A", "B"]}"##, "part", &[("$S2", "~shapeCount", "-"), ("root", "props.slideSize", "-")]),
    r(
        "smartGraphic.insert",
        r##"{"kind": "list", "items": ["A", "B"], "rect": [10, 10, 400, 200], "onSlide": 3}"##,
        "",
        &[("$S3", "~shapeCount", "*")],
    ),
    // actionButton.insert: a preset button with its relative-slide action, named by shape count;
    // no rect: a 48 pt box centred on the slide size.
    r("actionButton.insert", r##"{"kind": "forward", "rect": [10, 10, 48, 48]}"##, "part", &[("$S2", "~shapeCount", "-")]),
    r("actionButton.insert", r##"{"kind": "home"}"##, "part", &[("$S2", "~shapeCount", "-"), ("root", "props.slideSize", "-")]),
    // shape.connect: the ends' sites (their geometry and box; a placeholder's inherited box);
    // without id a new connector named by shape count; with id it rewrites that line's box and
    // glue, then reroutes it.
    r(
        "shape.connect",
        r##"{"from": "$Rect", "to": "$Oval"}"##,
        "",
        &[("$S2", "~shapeCount", "-"), ("$Rect", "look.geom,xfrm", "-"), ("$Oval", "look.geom,xfrm", "-")],
    ),
    r(
        "shape.connect",
        r##"{"from": "$Rect", "to": "$Oval", "fromSite": 0, "toSite": 2, "preset": "bentConnector3"}"##,
        "",
        &[("$S2", "~shapeCount", "-"), ("$Rect", "look.geom,xfrm", "-"), ("$Oval", "look.geom,xfrm", "-")],
    ),
    r(
        "shape.connect",
        r##"{"from": 263, "to": 264}"##,
        "part",
        &[("$S2", "~shapeCount", "-"), ("$Rect", "look.geom,xfrm", "address"), ("$Oval", "look.geom,xfrm", "address")],
    ),
    r(
        "shape.connect",
        r##"{"id": "$Line", "from": "$Oval", "to": "$Rect"}"##,
        "",
        &[("$Line", "look.geom,look.kind,xfrm", "-"), ("$Rect", "look.geom,xfrm", "-"), ("$Oval", "look.geom,xfrm", "-")],
    ),
    r(
        "shape.connect",
        r##"{"from": "$Title", "to": "$Oval"}"##,
        "",
        &[
            ("$S2", "layout,~shapeCount", "-"),
            ("$Title", "look.geom,look.ph,xfrm", "-"),
            ("$Layout:Title and Content/Title 1", "look.ph,xfrm", "-"),
            ("$Oval", "look.geom,xfrm", "-"),
        ],
    ),
    // shape.freeform: inserts a rect (closed) or line (open) on the current tree, then sets its
    // custom geometry and box and names it "Freeform <shape count>".
    r("shape.freeform", r##"{"points": [[10, 10], [60, 10], [30, 50]], "closed": true}"##, "part", &[("$S2", "~shapeCount", "-")]),
    r("shape.freeform", r##"{"points": [[10, 10], [60, 10], [30, 50]], "smooth": true}"##, "part", &[("$S2", "~shapeCount", "-")]),
    r("shape.freeform", r##"{"points": [[10, 10], [60, 40]], "onSlide": 3}"##, "", &[("$S3", "~shapeCount", "*")]),
    // shape.move: without toSlide, edit_shapes on the box (absolute or relative), rerouting the
    // connectors glued to it, refitting its group. With toSlide: takes the shapes (and their
    // animations and comment anchors) off the current slide, renumbers them where the
    // destination's numbers collide, appends them there.
    r(
        "shape.move",
        r##"{"ids": ["$Rect"], "dx": 10, "dy": 5}"##,
        "",
        &[("$Rect", "look.geom,xfrm", "-"), ("$Line", "look.geom,look.kind,xfrm", "-"), ("$Oval", "look.geom,xfrm", "-")],
    ),
    r(
        "shape.move",
        r##"{"x": 120}"##,
        "part,selection",
        &[("$Rect", "look.geom,xfrm", "-"), ("$Line", "look.geom,look.kind,xfrm", "-"), ("$Oval", "look.geom,xfrm", "-")],
    ),
    r(
        "shape.move",
        r##"{"ids": [263], "onSlide": 2, "dx": 10}"##,
        "",
        &[("$S2", "-", "*"), ("$Rect", "look.geom,xfrm", "address"), ("$Line", "look.geom,look.kind,xfrm", "-"), ("$Oval", "look.geom,xfrm", "-")],
    ),
    r("shape.move", r##"{"ids": ["$GA"], "x": 610}"##, "", &[("$GA", "xfrm", "-"), ("$Group", "children,look.kind,xfrm", "-"), ("$GB", "xfrm", "-")]),
    r("shape.move", r##"{"ids": ["$Group"], "dy": 20}"##, "", &[("$Group", "xfrm", "-")]),
    r(
        "shape.move",
        r##"{"ids": ["$Title"], "dy": 5}"##,
        "",
        &[("$S2", "layout", "-"), ("$Title", "look.ph,xfrm", "-"), ("$Layout:Title and Content/Title 1", "look.ph,xfrm", "-")],
    ),
    r("shape.move", r##"{"ids": ["$Star"], "dx": 5}"##, "", &[("$Star", "xfrm", "-")]),
    r(
        "shape.move",
        r##"{"ids": ["$Rect"], "toSlide": "$S3"}"##,
        "",
        &[("$Rect", "*", "address"), ("$S2", "animations", "-"), ("$Thread", "anchor", "-"), ("$S3", "-", "-"), ("$Star", "-", "address")],
    ),
    // shape.resize: edit_shapes; reads the box (aspect, centre when rotated); reroutes, refits
    // the group; a shrink-to-fit placeholder's text is refit to the new size.
    r(
        "shape.resize",
        r##"{"ids": ["$Rect"], "w": 150, "lockAspect": true}"##,
        "",
        &[("$Rect", "look.geom,xfrm", "-"), ("$Line", "look.geom,look.kind,xfrm", "-"), ("$Oval", "look.geom,xfrm", "-")],
    ),
    r(
        "shape.resize",
        r##"{"h": 90}"##,
        "part,selection",
        &[("$Rect", "look.geom,xfrm", "-"), ("$Line", "look.geom,look.kind,xfrm", "-"), ("$Oval", "look.geom,xfrm", "-")],
    ),
    r(
        "shape.resize",
        r##"{"ids": [264], "w": 120}"##,
        "part",
        &[("$Oval", "look.geom,xfrm", "address"), ("$Line", "look.geom,look.kind,xfrm", "-"), ("$Rect", "look.geom,xfrm", "-")],
    ),
    r(
        "shape.resize",
        r##"{"ids": ["$GB"], "w": 100}"##,
        "",
        &[("$GB", "xfrm", "-"), ("$Group", "children,look.kind,xfrm", "-"), ("$GA", "xfrm", "-")],
    ),
    r(
        "shape.resize",
        r##"{"ids": ["$Title"], "w": 300, "h": 60}"##,
        "",
        &[
            ("$S2", "layout", "-"),
            ("$Title", "body,look.geom,look.ph,xfrm", "-"),
            ("$Layout:Title and Content/Title 1", "body,look.ph,look.style,xfrm", "-"),
            ("$Master/Title 1", "body,look.ph", "-"),
            ("$Master", "listStyles.title,theme.fonts", "-"),
            ("root", "defaultTextStyle", "-"),
        ],
    ),
    // normalized after review: the placeholder lookup compares the placeholders before the match: the
    // layout's by index, then kind (`resolve::find_ph`, resolve.rs:64), the master's by kind
    // (`master_ph`, resolve.rs:82); the title placeholders come first.
    pinned(r(
        "shape.resize",
        r##"{"ids": ["$Body"], "h": 100}"##,
        "",
        &[
            ("$S2", "layout", "-"),
            ("$Body", "body,look.geom,look.ph,xfrm", "-"),
            ("$Layout:Title and Content/Content Placeholder 2", "body,look.ph,look.style,xfrm", "-"),
            ("$Master/Text Placeholder 2", "body,look.ph", "-"),
            ("$Layout:Title and Content/Title 1", "look.ph", "-"),
            ("$Master/Title 1", "look.ph", "-"),
            ("$Master", "listStyles.body,theme.fonts", "-"),
            ("root", "defaultTextStyle", "-"),
        ],
    )),
    // shape.delete: ids or id required (no selection); edit.delete removes them (and groups left
    // empty), the slide's animations of them (reads every animation's target); glue to them is
    // dropped and those connectors rerouted; a group losing a member is refit.
    r(
        "shape.delete",
        r##"{"ids": ["$Rect"]}"##,
        "",
        &[("$Rect", "*", "-"), ("$S2", "animations", "-"), ("$Line", "look.geom,look.kind,xfrm", "-"), ("$Oval", "look.geom,xfrm", "-")],
    ),
    r("shape.delete", r##"{"id": "$Line"}"##, "", &[("$Line", "*", "-"), ("$S2", "animations", "-")]),
    r(
        "shape.delete",
        r##"{"ids": ["$GA"]}"##,
        "",
        &[("$GA", "*", "-"), ("$Group", "children,look.kind,xfrm", "-"), ("$GB", "xfrm", "-"), ("$S2", "animations", "-")],
    ),
    r("shape.delete", r##"{"ids": ["$GA", "$GB"]}"##, "", &[("$GA", "*", "-"), ("$GB", "*", "-"), ("$Group", "*", "-"), ("$S2", "animations", "-")]),
    r(
        "shape.delete",
        r##"{"ids": [264], "onSlide": 2}"##,
        "",
        &[("$S2", "animations", "*"), ("$Oval", "*", "address"), ("$Line", "look.geom,look.kind,xfrm", "-"), ("$Rect", "look.geom,xfrm", "-")],
    ),
    r("shape.delete", r##"{"id": "$Star"}"##, "", &[("$Star", "*", "-"), ("$S3", "animations", "-")]),
    // shape.setBounds: edit_shapes; omitted x/y/w/h keep the box's; reroutes, refits, refits a
    // shrink-to-fit placeholder's text.
    r(
        "shape.setBounds",
        r##"{"ids": ["$Rect"], "x": 50, "y": 60, "w": 100, "h": 70}"##,
        "",
        &[("$Rect", "look.geom,xfrm", "-"), ("$Line", "look.geom,look.kind,xfrm", "-"), ("$Oval", "look.geom,xfrm", "-")],
    ),
    r(
        "shape.setBounds",
        r##"{"x": 50}"##,
        "part,selection",
        &[("$Rect", "look.geom,xfrm", "-"), ("$Line", "look.geom,look.kind,xfrm", "-"), ("$Oval", "look.geom,xfrm", "-")],
    ),
    r(
        "shape.setBounds",
        r##"{"ids": ["$GA"], "x": 590, "y": 90, "w": 80, "h": 60}"##,
        "",
        &[("$GA", "xfrm", "-"), ("$Group", "children,look.kind,xfrm", "-"), ("$GB", "xfrm", "-")],
    ),
    r(
        "shape.setBounds",
        r##"{"ids": ["$Title"], "x": 60, "y": 20, "w": 400, "h": 80}"##,
        "",
        &[
            ("$S2", "layout", "-"),
            ("$Title", "body,look.geom,look.ph,xfrm", "-"),
            ("$Layout:Title and Content/Title 1", "body,look.ph,look.style,xfrm", "-"),
            ("$Master/Title 1", "body,look.ph", "-"),
            ("$Master", "listStyles.title,theme.fonts", "-"),
            ("root", "defaultTextStyle", "-"),
        ],
    ),
    // shape.rotate: edit_shapes; deg, or the box's rotation plus by; reroutes, refits.
    r(
        "shape.rotate",
        r##"{"ids": ["$Rect"], "by": 15}"##,
        "",
        &[("$Rect", "look.geom,xfrm", "-"), ("$Line", "look.geom,look.kind,xfrm", "-"), ("$Oval", "look.geom,xfrm", "-")],
    ),
    r(
        "shape.rotate",
        r##"{"deg": 90}"##,
        "part,selection",
        &[("$Rect", "look.geom,xfrm", "-"), ("$Line", "look.geom,look.kind,xfrm", "-"), ("$Oval", "look.geom,xfrm", "-")],
    ),
    r(
        "shape.rotate",
        r##"{"ids": ["$GA"], "deg": 45}"##,
        "",
        &[("$GA", "xfrm", "-"), ("$Group", "children,look.kind,xfrm", "-"), ("$GB", "xfrm", "-")],
    ),
    r(
        "shape.rotate",
        r##"{"ids": ["$Title"], "by": 10}"##,
        "",
        &[("$S2", "layout", "-"), ("$Title", "look.ph,xfrm", "-"), ("$Layout:Title and Content/Title 1", "look.ph,xfrm", "-")],
    ),
    r("shape.rotate", r##"{"ids": ["$Star"], "by": 10}"##, "", &[("$Star", "xfrm", "-")]),
    // shape.flip: edit_shapes; toggles the box's flip; the glued ends' sites move with it.
    r(
        "shape.flip",
        r##"{"ids": ["$Rect"], "axis": "horizontal"}"##,
        "",
        &[("$Rect", "look.geom,xfrm", "-"), ("$Line", "look.geom,look.kind,xfrm", "-"), ("$Oval", "look.geom,xfrm", "-")],
    ),
    r(
        "shape.flip",
        r##"{"axis": "vertical"}"##,
        "part,selection",
        &[("$Rect", "look.geom,xfrm", "-"), ("$Line", "look.geom,look.kind,xfrm", "-"), ("$Oval", "look.geom,xfrm", "-")],
    ),
    r("shape.flip", r##"{"ids": ["$Group"], "axis": "horizontal"}"##, "", &[("$Group", "xfrm", "-")]),
    r(
        "shape.flip",
        r##"{"ids": ["$Title"], "axis": "vertical"}"##,
        "",
        &[("$S2", "layout", "-"), ("$Title", "look.ph,xfrm", "-"), ("$Layout:Title and Content/Title 1", "look.ph,xfrm", "-")],
    ),
    // shape.adjust: edit_shapes; a preset's adjust values, missing ones filled with the preset's
    // defaults (an edit of the geometry); the glued connectors are rerouted; a shrink-to-fit
    // placeholder's text is refit to the new geometry.
    r("shape.adjust", r##"{"ids": ["$Star"], "index": 0, "value": 19000}"##, "", &[("$Star", "look.geom", "-")]),
    r(
        "shape.adjust",
        r##"{"index": 0, "value": 10000}"##,
        "part,selection",
        &[("$Rect", "look.geom,xfrm", "-"), ("$Line", "look.geom,look.kind,xfrm", "-"), ("$Oval", "look.geom,xfrm", "-")],
    ),
    r(
        "shape.adjust",
        r##"{"ids": ["$Title"], "index": 0, "value": 10000}"##,
        "",
        &[
            ("$S2", "layout", "-"),
            ("$Title", "body,look.geom,look.ph,xfrm", "-"),
            ("$Layout:Title and Content/Title 1", "body,look.ph,look.style,xfrm", "-"),
            ("$Master/Title 1", "body,look.ph", "-"),
            ("$Master", "listStyles.title,theme.fonts", "-"),
            ("root", "defaultTextStyle", "-"),
        ],
    ),
    // shape.change: edit_shapes; replaces the geometry with the preset; reroutes from the new
    // sites; a shrink-to-fit placeholder's text is refit.
    r(
        "shape.change",
        r##"{"ids": ["$Rect"], "preset": "roundRect"}"##,
        "",
        &[("$Rect", "look.geom,xfrm", "-"), ("$Line", "look.geom,look.kind,xfrm", "-"), ("$Oval", "look.geom,xfrm", "-")],
    ),
    r(
        "shape.change",
        r##"{"preset": "triangle"}"##,
        "part,selection",
        &[("$Rect", "look.geom,xfrm", "-"), ("$Line", "look.geom,look.kind,xfrm", "-"), ("$Oval", "look.geom,xfrm", "-")],
    ),
    r(
        "shape.change",
        r##"{"ids": ["$Title"], "preset": "ellipse"}"##,
        "",
        &[
            ("$S2", "layout", "-"),
            ("$Title", "body,look.geom,look.ph,xfrm", "-"),
            ("$Layout:Title and Content/Title 1", "body,look.ph,look.style,xfrm", "-"),
            ("$Master/Title 1", "body,look.ph", "-"),
            ("$Master", "listStyles.title,theme.fonts", "-"),
            ("root", "defaultTextStyle", "-"),
        ],
    ),
    r("shape.change", r##"{"ids": [278], "onSlide": 3, "preset": "star6"}"##, "", &[("$S3", "-", "*"), ("$Star", "look.geom", "address")]),
    // shape.fill: edit_shapes on each target, a group's members instead of the group; a value
    // replaces the fill; transparency puts alpha on the shape's fill or on its effective fill
    // (shape style -> theme format fills, theme colours, colour map) resolved to RGB; picture adds
    // media first (RED reuses $Media:red.png).
    r("shape.fill", r##"{"ids": ["$Rect"], "color": "#ff0000"}"##, "", &[("$Rect", "look.fill", "-")]),
    r("shape.fill", r##"{"none": true}"##, "part,selection", &[("$Rect", "look.fill", "-")]),
    r(
        "shape.fill",
        r##"{"ids": ["$Rect"], "transparency": 0.5}"##,
        "",
        &[("$S2", "layout", "-"), ("$Rect", "look.fill,look.style", "-"), ("$Master", "colorMap,theme.colors,theme.format", "-")],
    ),
    r(
        "shape.fill",
        r##"{"ids": ["$Group"], "gradient": {"stops": [[0, "#ffffff"], [1, "#000000"]]}}"##,
        "",
        &[("$Group", "children", "-"), ("$GA", "look.fill", "-"), ("$GB", "look.fill", "-")],
    ),
    r(
        "shape.fill",
        r##"{"ids": ["$Rect"], "picture": "iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAIAAACQd1PeAAAADElEQVR4nGM4wcAAAAJcAMlDyBJGAAAAAElFTkSuQmCC"}"##,
        "",
        &[("$Rect", "look.fill", "-"), ("$Media:red.png", "blob", "-")],
    ),
    r(
        "shape.fill",
        r##"{"ids": ["$Title"], "color": "accent2"}"##,
        "",
        &[("$S2", "layout", "-"), ("$Title", "look.fill,look.ph,xfrm", "-"), ("$Layout:Title and Content/Title 1", "look.ph,xfrm", "-")],
    ),
    r("shape.fill", r##"{"ids": [263], "reset": true}"##, "part", &[("$Rect", "look.fill", "address")]),
    // shape.line: edit_shapes on each target (a group's members); edits the outline from its
    // current value (a width gives a missing colour tx1); reset clears it.
    r("shape.line", r##"{"ids": ["$Rect"], "width": 3}"##, "", &[("$Rect", "look.line", "-")]),
    r("shape.line", r##"{"color": "#0000ff", "dash": "dash"}"##, "part,selection", &[("$Rect", "look.line", "-")]),
    r(
        "shape.line",
        r##"{"ids": ["$Group"], "none": true}"##,
        "",
        &[("$Group", "children", "-"), ("$GA", "look.line", "-"), ("$GB", "look.line", "-")],
    ),
    r("shape.line", r##"{"ids": ["$Line"], "head": "oval", "tail": "none"}"##, "", &[("$Line", "look.line", "-")]),
    // shape.effects: edit_shapes on each target (a group's members); edits the effects from their
    // current value; reset clears them.
    r(
        "shape.effects",
        r##"{"ids": ["$Rect"], "shadow": "outer", "glow": {"color": "#ff0000", "radius": 6}}"##,
        "",
        &[("$Rect", "look.effects", "-")],
    ),
    r("shape.effects", r##"{"reset": true}"##, "part,selection", &[("$Rect", "look.effects", "-")]),
    r(
        "shape.effects",
        r##"{"ids": ["$Group"], "softEdges": 4}"##,
        "",
        &[("$Group", "children", "-"), ("$GA", "look.effects", "-"), ("$GB", "look.effects", "-")],
    ),
    // shape.quickStyle: edit_shapes on each target (a group's members); replaces style, fill,
    // outline and effects; clears every text run's fill (an edit of the text).
    r("shape.quickStyle", r##"{"ids": ["$Rect"], "index": 9}"##, "", &[("$Rect", "body,look.effects,look.fill,look.line,look.style", "-")]),
    r("shape.quickStyle", r##"{"index": 3}"##, "part,selection", &[("$Rect", "body,look.effects,look.fill,look.line,look.style", "-")]),
    r(
        "shape.quickStyle",
        r##"{"ids": ["$Group"], "index": 20}"##,
        "",
        &[
            ("$Group", "children", "-"),
            ("$GA", "body,look.effects,look.fill,look.line,look.style", "-"),
            ("$GB", "body,look.effects,look.fill,look.line,look.style", "-"),
        ],
    ),
    // shape.rename: edit_shapes (id or ids, else the selection); writes the name.
    r("shape.rename", r##"{"id": "$Rect", "name": "Box"}"##, "", &[("$Rect", "name", "-")]),
    r("shape.rename", r##"{"name": "Box"}"##, "part,selection", &[("$Rect", "name", "-")]),
    r("shape.rename", r##"{"id": 264, "name": "Ring"}"##, "part", &[("$Oval", "name", "address")]),
    r(
        "shape.rename",
        r##"{"id": "$Title", "name": "Heading"}"##,
        "",
        &[("$S2", "layout", "-"), ("$Title", "look.ph,name,xfrm", "-"), ("$Layout:Title and Content/Title 1", "look.ph,xfrm", "-")],
    ),
    // shape.altText: edit_shapes; writes the alt text, and decorative when given.
    r("shape.altText", r##"{"ids": ["$Pic"], "text": "Red", "decorative": false}"##, "", &[("$Pic", "look.decorative,look.descr", "-")]),
    r("shape.altText", r##"{"text": "A box"}"##, "part,selection", &[("$Rect", "look.descr", "-")]),
    r("shape.altText", r##"{"ids": ["$Group"], "text": "Pair"}"##, "", &[("$Group", "look.descr", "-")]),
    // shape.visible: edit_shapes; visible given sets hidden, else toggles it (reads it).
    r("shape.visible", r##"{"ids": ["$Rect"], "visible": false}"##, "", &[("$Rect", "look.hidden", "-")]),
    r("shape.visible", r##"{}"##, "part,selection", &[("$Rect", "look.hidden", "-")]),
    r("shape.visible", r##"{"ids": [278], "onSlide": 3, "visible": true}"##, "", &[("$S3", "-", "*"), ("$Star", "look.hidden", "address")]),
    // shape.lock: edit_shapes; locked given sets it, else toggles it (reads it).
    r("shape.lock", r##"{"ids": ["$Oval"], "locked": true}"##, "", &[("$Oval", "look.locked", "-")]),
    r("shape.lock", r##"{}"##, "part,selection", &[("$Rect", "look.locked", "-")]),
    // shape.setDefault: copies the first selected shape's first run, fill, outline, effects and
    // style into the session's default look; the deck is not changed.
    r("shape.setDefault", r##"{}"##, "part,selection", &[]),
];
