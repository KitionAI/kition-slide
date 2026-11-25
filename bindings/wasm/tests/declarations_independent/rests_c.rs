use super::{Rest, pinned, r, rs};

#[allow(unused_imports)]
use pinned as _;

pub const ROWS: &[Rest] = &[
    // image.crop: edit_shapes on ids/id (local_id on the slide) else the selection; edits the crop
    // entries of a picture's fill (look.kind) from their current values; a non-picture is left
    // alone, but a placeholder without a frame gets its layout placeholder's frame written first.
    r("image.crop", r##"{"ids": ["$Pic"], "left": 0.1}"##, "", &[("$Pic", "look.kind", "-")]),
    r("image.crop", r##"{"ids": [273], "onSlide": 2, "top": 0.2}"##, "", &[("$Pic", "look.kind", "address"), ("$S2", "-", "*")]),
    r("image.crop", r##"{"ids": [273], "right": 0.2}"##, "part", &[("$Pic", "look.kind", "address")]),
    rs(r##"[["edit.select", {"ids": ["$Pic"]}]]"##, "image.crop", r##"{"bottom": 0.1}"##, "part,selection", &[("$Pic", "look.kind", "-")]),
    r("image.crop", r##"{"left": 0.1}"##, "part,selection", &[("$Rect", "-", "-")]),
    r(
        "image.crop",
        r##"{"ids": ["$Title"], "left": 0.1}"##,
        "",
        &[("$Title", "look.ph,xfrm", "-"), ("$Layout:Title and Content/Title 1", "look.ph,xfrm", "-"), ("$S2", "layout", "-")],
    ),
    // image.adjust: as image.crop; edits fill.adjust and fill.alpha (look.kind) from their values.
    r("image.adjust", r##"{"ids": ["$Pic"], "brightness": 0.2, "transparency": 0.5}"##, "", &[("$Pic", "look.kind", "-")]),
    r("image.adjust", r##"{"ids": ["$Pic"], "reset": true, "grayscale": true}"##, "", &[("$Pic", "look.kind", "-")]),
    r("image.adjust", r##"{"ids": [273], "onSlide": 2, "contrast": 0.3}"##, "", &[("$Pic", "look.kind", "address"), ("$S2", "-", "*")]),
    rs(r##"[["edit.select", {"ids": ["$Pic"]}]]"##, "image.adjust", r##"{"saturation": 2}"##, "part,selection", &[("$Pic", "look.kind", "-")]),
    r("image.adjust", r##"{"brightness": 0.2}"##, "part,selection", &[("$Rect", "-", "-")]),
    // image.reset: as image.crop; resets a picture's adjust/crop/alpha (look.kind, fill.media
    // kept) and, for ANY target, replaces its effects and outline with none.
    r("image.reset", r##"{"ids": ["$Pic"]}"##, "", &[("$Pic", "look.effects,look.kind,look.line", "-")]),
    r("image.reset", r##"{"ids": [273], "onSlide": 2}"##, "", &[("$Pic", "look.effects,look.kind,look.line", "address"), ("$S2", "-", "*")]),
    // normalized after review: engine fix 1ad182d, image.reset writes a non-picture's outline
    // and effects no more (crates/engine/engine/src/cmd/shape.rs picture_reset).
    pinned(r("image.reset", r##"{}"##, "part,selection", &[("$Rect", "-", "-")])),
    // normalized after review: engine fix 1ad182d, as above; edit_shapes still stores the
    // placeholder's inherited frame (crates/engine/engine/src/cmd/mod.rs edit_shapes).
    pinned(r(
        "image.reset",
        r##"{"ids": ["$Title"]}"##,
        "",
        &[("$Title", "look.ph,xfrm", "-"), ("$Layout:Title and Content/Title 1", "look.ph,xfrm", "-"), ("$S2", "layout", "-")],
    )),
    // image.change: adds the bytes as a media item (reusing one with the same bytes), then on the
    // targets sets fill.media and clears the crop (look.kind, keeping adjust/alpha) and the media ref.
    r("image.change", r##"{"ids": ["$Pic"], "data": "AAAA", "name": "x.png"}"##, "", &[("$Pic", "look.kind,media", "-")]),
    r(
        "image.change",
        r##"{"ids": ["$Pic"], "data": "iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAIAAACQd1PeAAAADElEQVR4nGM4wcAAAAJcAMlDyBJGAAAAAElFTkSuQmCC", "name": "red.png"}"##,
        "",
        &[("$Pic", "look.kind,media", "-"), ("$Media:red.png", "blob", "-")],
    ),
    r("image.change", r##"{"ids": [273], "onSlide": 2, "data": "AAAA"}"##, "", &[("$Pic", "look.kind,media", "address"), ("$S2", "-", "*")]),
    rs(r##"[["edit.select", {"ids": ["$Pic"]}]]"##, "image.change", r##"{"data": "AAAA"}"##, "part,selection", &[("$Pic", "look.kind,media", "-")]),
    r("image.change", r##"{"data": "AAAA"}"##, "part,selection", &[("$Rect", "-", "-")]),
    // media.options: edit_shapes; edits the given playback members of a media clip (look.kind) from
    // its current options; other kinds are left alone.
    r("media.options", r##"{"ids": ["$Video"], "loop": true, "volume": 0.5}"##, "", &[("$Video", "look.kind", "-")]),
    r("media.options", r##"{"ids": [276], "onSlide": 2, "autoplay": true}"##, "", &[("$Video", "look.kind", "address"), ("$S2", "-", "*")]),
    rs(r##"[["edit.select", {"ids": ["$Video"]}]]"##, "media.options", r##"{"trimStart": 500}"##, "part,selection", &[("$Video", "look.kind", "-")]),
    r("media.options", r##"{"ids": ["$Pic"], "loop": true}"##, "", &[("$Pic", "-", "-")]),
    // drawing.insert: a new shape/text box/chart appended to the current slide; its name counts the
    // slide's top-level shapes; without a frame it is placed from the slide size.
    r(
        "drawing.insert",
        r##"{"geometry": {"preset": "rect"}, "frame": {"x": 10, "y": 10, "w": 50, "h": 50}}"##,
        "part",
        &[("$S2", "~shapeCount", "-")],
    ),
    r("drawing.insert", r##"{"kind": "textBox", "text": "Hi"}"##, "part", &[("$S2", "~shapeCount", "-"), ("root", "props.slideSize", "-")]),
    r(
        "drawing.insert",
        r##"{"kind": "chart", "chart": {"kind": "bar", "categories": ["A"], "series": [{"name": "S", "values": [1]}]}}"##,
        "part",
        &[("$S2", "~shapeCount", "-"), ("root", "props.slideSize", "-")],
    ),
    r(
        "drawing.insert",
        r##"{"onSlide": 3, "geometry": {"preset": "ellipse"}, "frame": {"x": 10, "y": 10, "w": 50, "h": 50}}"##,
        "",
        &[("$S3", "~shapeCount", "-")],
    ),
    // drawing.delete: ids required, drawing kinds only, then edit.delete: removes the shapes (from
    // their group too; an emptied group goes), the slide's animations of them (the animation list is
    // filtered); reroute unglues a connector from a removed end; refit re-fits a group losing a member.
    // normalized after review (C8-slide-read-3, orchestrator): the deleted shape is held on every
    // field, as edit.delete's rows hold it (drawing.delete delegates to edit.delete, drawing.rs:321)
    // and the earlier end-to-end tests outside.rs integer_single_ids_hold_the_shape_and_the_numbered_slide
    // and n_a_target_named_by_id_... require: deleting replaces the shape whole (a lost update).
    pinned(r(
        "drawing.delete",
        r##"{"ids": ["$Rect"]}"##,
        "",
        &[("$Rect", "*", "-"), ("$S2", "animations", "-"), ("$Line", "look.geom,look.kind,xfrm", "-"), ("$Oval", "look.geom,xfrm", "-")],
    )),
    pinned(r("drawing.delete", r##"{"id": "$Text"}"##, "", &[("$Text", "*", "-"), ("$S2", "animations", "-")])),
    pinned(r("drawing.delete", r##"{"ids": [271], "onSlide": 2}"##, "", &[("$Text", "*", "address"), ("$S2", "animations", "*")])),
    pinned(r(
        "drawing.delete",
        r##"{"ids": ["$GA"]}"##,
        "",
        &[("$GA", "*", "-"), ("$S2", "animations", "-"), ("$Group", "children,look.kind,xfrm", "-"), ("$GB", "xfrm", "-")],
    )),
    pinned(r("drawing.delete", r##"{"ids": ["$Star"]}"##, "", &[("$Star", "*", "-"), ("$S3", "animations", "-")])),
    // drawing.transform: ids required; each object's slide frame (a placeholder's from its layout)
    // set (frame) or changed (dx, dy, rotateBy, flips) and written as xfrm; group members refused;
    // a moved end reroutes its glued connector (a glued connector moved snaps back to its sites).
    r(
        "drawing.transform",
        r##"{"ids": ["$Rect"], "dx": 10}"##,
        "",
        &[("$Rect", "look.geom,xfrm", "-"), ("$Line", "look.geom,look.kind,xfrm", "-"), ("$Oval", "look.geom,xfrm", "-")],
    ),
    r(
        "drawing.transform",
        r##"{"id": "$Rect", "frame": {"x": 0, "y": 0, "w": 100, "h": 100}}"##,
        "",
        &[("$Rect", "look.geom,xfrm", "-"), ("$Line", "look.geom,look.kind,xfrm", "-"), ("$Oval", "look.geom,xfrm", "-")],
    ),
    r("drawing.transform", r##"{"ids": ["$Text"], "rotateBy": 90, "flipH": true}"##, "", &[("$Text", "xfrm", "-")]),
    r(
        "drawing.transform",
        r##"{"ids": ["$Title"], "dy": 5}"##,
        "",
        &[("$Title", "look.ph,xfrm", "-"), ("$Layout:Title and Content/Title 1", "look.ph,xfrm", "-"), ("$S2", "layout", "-")],
    ),
    r("drawing.transform", r##"{"ids": ["$Group"], "dx": 10}"##, "", &[("$Group", "xfrm", "-")]),
    r("drawing.transform", r##"{"ids": [278], "onSlide": 3, "dx": 10}"##, "", &[("$Star", "xfrm", "address"), ("$S3", "-", "*")]),
    // drawing.format: ids required; replaces the fill or geometry, merges the stroke into the outline
    // and the shadow/glow into the effects; a placeholder's frame is materialised; a new geometry
    // moves the sites a glued connector is routed to.
    r("drawing.format", r##"{"ids": ["$Rect"], "fill": "#FF0000"}"##, "", &[("$Rect", "look.fill", "-")]),
    r("drawing.format", r##"{"ids": ["$Rect"], "stroke": {"color": "#00FF00"}}"##, "", &[("$Rect", "look.line", "-")]),
    r("drawing.format", r##"{"id": "$Text", "effects": {"shadow": {"color": "#000000"}}}"##, "", &[("$Text", "look.effects", "-")]),
    r(
        "drawing.format",
        r##"{"ids": ["$Rect"], "geometry": {"preset": "ellipse"}}"##,
        "",
        &[("$Rect", "look.geom,xfrm", "-"), ("$Line", "look.geom,look.kind,xfrm", "-"), ("$Oval", "look.geom,xfrm", "-")],
    ),
    r(
        "drawing.format",
        r##"{"ids": ["$Title"], "fill": "#FF0000"}"##,
        "",
        &[("$Title", "look.fill,look.ph,xfrm", "-"), ("$Layout:Title and Content/Title 1", "look.ph,xfrm", "-"), ("$S2", "layout", "-")],
    ),
    r("drawing.format", r##"{"ids": [271], "onSlide": 2, "fill": "#FF0000"}"##, "", &[("$Text", "look.fill", "address"), ("$S2", "-", "*")]),
    // drawing.text: one object; new paragraphs over the old body (each takes the old paragraph's
    // props and first run's props), or body props (anchor, insets, wrap, autofit, warp); then a body
    // that autofits its shape (a text box by default) re-fits the frame from the text's layout.
    r("drawing.text", r##"{"id": "$Rect", "text": "Hi"}"##, "", &[("$Rect", "body.paragraphs,body.props", "-")]),
    r("drawing.text", r##"{"id": "$Rect", "body": {"anchor": "top"}}"##, "", &[("$Rect", "body.paragraphs,body.props", "-")]),
    r("drawing.text", r##"{"id": "$Rect", "warp": "textArchUp"}"##, "", &[("$Rect", "body.paragraphs,body.props", "-")]),
    r(
        "drawing.text",
        r##"{"id": "$Text", "text": "A longer note that wraps"}"##,
        "",
        &[("$Text", "body,look.geom,look.textBox,xfrm", "-"), ("root", "defaultTextStyle", "-"), ("$Master", "theme.fonts", "-")],
    ),
    r(
        "drawing.text",
        r##"{"id": "$Title", "text": "New"}"##,
        "",
        &[
            ("$Title", "body.paragraphs,body.props,look.ph,xfrm", "-"),
            ("$Layout:Title and Content/Title 1", "body.props,look.ph,xfrm", "-"),
            ("$Master/Title 1", "body.props", "-"),
            ("$S2", "layout", "-"),
        ],
    ),
    r("drawing.text", r##"{"id": 263, "onSlide": 2, "text": "Hi"}"##, "", &[("$Rect", "body.paragraphs,body.props", "address"), ("$S2", "-", "*")]),
    // drawing.arrange: ids required, top-level only; the slide's stack reordered; what moves past
    // whom depends on where the objects are.
    r("drawing.arrange", r##"{"ids": ["$Rect"], "order": "front"}"##, "", &[("$Rect", "-", "*")]),
    r("drawing.arrange", r##"{"ids": ["$Rect"], "order": "back"}"##, "", &[("$Rect", "-", "*")]),
    r("drawing.arrange", r##"{"ids": ["$Rect"], "order": "forward"}"##, "", &[("$Rect", "-", "*"), ("$Oval", "-", "*")]),
    r("drawing.arrange", r##"{"ids": ["$Rect"], "order": "backward"}"##, "", &[("$Rect", "-", "*"), ("$Body", "-", "*")]),
    r("drawing.arrange", r##"{"ids": [263], "onSlide": 2, "order": "front"}"##, "", &[("$Rect", "-", "*"), ("$S2", "-", "*")]),
    // drawing.reconnect: one connector's end glued to `to` at `site` (look.kind) or left free (moved
    // to `point`); reroute then routes the connector (xfrm) from its glued ends' sites.
    r(
        "drawing.reconnect",
        r##"{"id": "$Line", "end": "end", "to": "$Text", "site": 0}"##,
        "",
        &[("$Line", "look.geom,look.kind,xfrm", "-"), ("$Rect", "look.geom,xfrm", "-"), ("$Text", "look.geom,xfrm", "-")],
    ),
    r(
        "drawing.reconnect",
        r##"{"id": "$Line", "end": "end", "point": [500, 500]}"##,
        "",
        &[("$Line", "look.geom,look.kind,xfrm", "-"), ("$Rect", "look.geom,xfrm", "-")],
    ),
    r(
        "drawing.reconnect",
        r##"{"id": "$Line", "end": "start"}"##,
        "",
        &[("$Line", "look.geom,look.kind,xfrm", "-"), ("$Oval", "look.geom,xfrm", "-")],
    ),
    r(
        "drawing.reconnect",
        r##"{"id": 265, "onSlide": 2, "end": "start", "to": 271, "site": 1}"##,
        "",
        &[
            ("$Line", "look.geom,look.kind,xfrm", "address"),
            ("$Oval", "look.geom,xfrm", "-"),
            ("$Text", "look.geom,xfrm", "address"),
            ("$S2", "-", "*"),
        ],
    ),
    // drawing.connect: shape.connect between two shapes of the slide: their sites (slide frame,
    // geometry) pick and place the ends; a new connector named from the slide's shape count.
    r(
        "drawing.connect",
        r##"{"from": "$Rect", "to": "$Text"}"##,
        "",
        &[("$Rect", "look.geom,xfrm", "-"), ("$Text", "look.geom,xfrm", "-"), ("$S2", "~shapeCount", "-")],
    ),
    r(
        "drawing.connect",
        r##"{"from": "$Rect", "to": "$Oval", "fromSite": 3, "toSite": 1, "geometry": {"preset": "bentConnector3"}}"##,
        "",
        &[("$Rect", "look.geom,xfrm", "-"), ("$Oval", "look.geom,xfrm", "-"), ("$S2", "~shapeCount", "-")],
    ),
    r(
        "drawing.connect",
        r##"{"from": "$Title", "to": "$Text"}"##,
        "",
        &[
            ("$Title", "look.geom,look.ph,xfrm", "-"),
            ("$Layout:Title and Content/Title 1", "look.ph,xfrm", "-"),
            ("$S2", "layout,~shapeCount", "-"),
            ("$Text", "look.geom,xfrm", "-"),
        ],
    ),
    r(
        "drawing.connect",
        r##"{"from": 263, "to": 264}"##,
        "part",
        &[("$Rect", "look.geom,xfrm", "address"), ("$Oval", "look.geom,xfrm", "address"), ("$S2", "~shapeCount", "-")],
    ),
    // drawing.group: ids required, top-level drawing objects, then arrange.group: members (frames
    // materialised, placeholder link dropped) moved into a new group placed where the topmost was,
    // named from the shape count; reroute drops glue to the grouped shapes (top-level lookup).
    r(
        "drawing.group",
        r##"{"ids": ["$Rect", "$Oval"]}"##,
        "",
        &[("$Rect", "look.ph,xfrm", "*"), ("$Oval", "look.ph,xfrm", "*"), ("$S2", "~shapeCount", "-"), ("$Line", "look.geom,look.kind,xfrm", "-")],
    ),
    r(
        "drawing.group",
        r##"{"ids": ["$Text", "$Chart"]}"##,
        "",
        &[("$Text", "look.ph,xfrm", "*"), ("$Chart", "look.ph,xfrm", "*"), ("$S2", "~shapeCount", "-")],
    ),
    r(
        "drawing.group",
        r##"{"ids": ["$Title", "$Text"]}"##,
        "",
        &[
            ("$Title", "look.ph,xfrm", "*"),
            ("$Layout:Title and Content/Title 1", "look.ph,xfrm", "-"),
            ("$S2", "layout,~shapeCount", "-"),
            ("$Text", "look.ph,xfrm", "*"),
        ],
    ),
    r(
        "drawing.group",
        r##"{"ids": [271, 270], "onSlide": 2}"##,
        "",
        &[("$Text", "look.ph,xfrm", "*"), ("$Chart", "look.ph,xfrm", "*"), ("$S2", "~shapeCount", "*")],
    ),
    // drawing.ungroup: ids required, top-level groups, then arrange.ungroup: members back where the
    // group was, frames mapped from the group's child space to its frame.
    r("drawing.ungroup", r##"{"ids": ["$Group"]}"##, "", &[("$Group", "children,look.kind,xfrm", "*"), ("$GA", "xfrm", "*"), ("$GB", "xfrm", "*")]),
    r(
        "drawing.ungroup",
        r##"{"ids": [268], "onSlide": 2}"##,
        "",
        &[("$Group", "children,look.kind,xfrm", "*"), ("$GA", "xfrm", "*"), ("$GB", "xfrm", "*"), ("$S2", "-", "*")],
    ),
    // drawing.chart: one chart; its spec changed and applied onto the chart model (chart parts).
    r("drawing.chart", r##"{"id": "$Chart", "title": "Sales"}"##, "", &[("$Chart", "chart", "-")]),
    r("drawing.chart", r##"{"ids": ["$Chart"], "kind": "bar", "series": [{"name": "S", "values": [3, 4]}]}"##, "", &[("$Chart", "chart", "-")]),
    r("drawing.chart", r##"{"id": 270, "onSlide": 2, "legend": "top"}"##, "", &[("$Chart", "chart", "address"), ("$S2", "-", "*")]),
    // drawing.ink: strokes added to the slide's ink layer (the last ink shape on it), or a new
    // full-slide ink layer (slide size) when it has none (the fixture's slides have none).
    r("drawing.ink", r##"{"strokes": [{"points": [[10, 10], [50, 50]]}]}"##, "part", &[("$S2", "-", "-"), ("root", "props.slideSize", "-")]),
    r(
        "drawing.ink",
        r##"{"onSlide": 3, "strokes": [{"tool": "highlighter", "points": [[10, 10], [50, 50]]}]}"##,
        "",
        &[("$S3", "-", "*"), ("root", "props.slideSize", "-")],
    ),
    // drawing.erase: the strokes of the slide's ink layers the path touches are deleted; on the
    // fixture no slide has ink, so nothing is written.
    r("drawing.erase", r##"{"path": [[10, 10], [50, 50]]}"##, "part", &[]),
    r("drawing.erase", r##"{"x": 100, "y": 100, "radius": 10}"##, "part", &[]),
    // arrange.bringToFront: targets (ids, else selection); top-level shapes moved to the top of the
    // slide's list (group members are not in it: nothing moves).
    r("arrange.bringToFront", r##"{"ids": ["$Rect"]}"##, "", &[("$Rect", "-", "*")]),
    r("arrange.bringToFront", r##"{}"##, "part,selection", &[("$Rect", "-", "*")]),
    r("arrange.bringToFront", r##"{"ids": [278], "onSlide": 3}"##, "", &[("$Star", "-", "*"), ("$S3", "-", "*")]),
    r("arrange.bringToFront", r##"{"ids": ["$GA"]}"##, "", &[("$GA", "-", "-")]),
    // arrange.bringForward: each target swapped with the shape above it.
    r("arrange.bringForward", r##"{"ids": ["$Rect"]}"##, "", &[("$Rect", "-", "*"), ("$Oval", "-", "*")]),
    r("arrange.bringForward", r##"{}"##, "part,selection", &[("$Rect", "-", "*"), ("$Oval", "-", "*")]),
    r("arrange.bringForward", r##"{"ids": [263], "onSlide": 2}"##, "", &[("$Rect", "-", "*"), ("$Oval", "-", "*"), ("$S2", "-", "*")]),
    // arrange.sendBackward: each target swapped with the shape below it.
    r("arrange.sendBackward", r##"{"ids": ["$Rect"]}"##, "", &[("$Rect", "-", "*"), ("$Body", "-", "*")]),
    r("arrange.sendBackward", r##"{}"##, "part,selection", &[("$Rect", "-", "*"), ("$Body", "-", "*")]),
    r("arrange.sendBackward", r##"{"ids": [263]}"##, "part", &[("$Rect", "-", "*"), ("$Body", "-", "*")]),
    // arrange.sendToBack: targets moved to the bottom of the slide's list.
    r("arrange.sendToBack", r##"{"ids": ["$Rect"]}"##, "", &[("$Rect", "-", "*")]),
    r("arrange.sendToBack", r##"{}"##, "part,selection", &[("$Rect", "-", "*")]),
    r("arrange.sendToBack", r##"{"ids": [278], "onSlide": 3}"##, "", &[("$Star", "-", "*"), ("$S3", "-", "*")]),
    // arrange.reorder: one top-level shape (by number) moved to stack index `index` (0 = back): the
    // shapes below that index decide where it lands.
    r("arrange.reorder", r##"{"id": "$Rect", "index": 0}"##, "", &[("$Rect", "-", "*")]),
    r("arrange.reorder", r##"{"id": "$Rect", "index": 1}"##, "", &[("$Rect", "-", "*"), ("$Title", "-", "*")]),
    r("arrange.reorder", r##"{"id": 263, "onSlide": 2, "index": 0}"##, "", &[("$Rect", "-", "*"), ("$S2", "-", "*")]),
    r("arrange.reorder", r##"{"id": 263, "index": 0}"##, "part", &[("$Rect", "-", "*")]),
    // arrange.group: targets (ids, else selection; two or more top-level shapes) moved into a new
    // group (frames materialised, placeholder link dropped) where the topmost was, named from the
    // shape count; reroute drops a connector's glue to a grouped shape.
    r(
        "arrange.group",
        r##"{"ids": ["$Rect", "$Oval"]}"##,
        "",
        &[("$Rect", "look.ph,xfrm", "*"), ("$Oval", "look.ph,xfrm", "*"), ("$S2", "~shapeCount", "-"), ("$Line", "look.geom,look.kind,xfrm", "-")],
    ),
    rs(
        r##"[["edit.select", {"ids": ["$Text", "$Pic"]}]]"##,
        "arrange.group",
        r##"{}"##,
        "part,selection",
        &[("$Text", "look.ph,xfrm", "*"), ("$Pic", "look.ph,xfrm", "*"), ("$S2", "~shapeCount", "-")],
    ),
    r(
        "arrange.group",
        r##"{"ids": ["$Title", "$Text"]}"##,
        "",
        &[
            ("$Title", "look.ph,xfrm", "*"),
            ("$Layout:Title and Content/Title 1", "look.ph,xfrm", "-"),
            ("$S2", "layout,~shapeCount", "-"),
            ("$Text", "look.ph,xfrm", "*"),
        ],
    ),
    r(
        "arrange.group",
        r##"{"ids": [271, 273], "onSlide": 2}"##,
        "",
        &[("$Text", "look.ph,xfrm", "*"), ("$Pic", "look.ph,xfrm", "*"), ("$S2", "~shapeCount", "*")],
    ),
    // arrange.ungroup: targets (ids, else selection); each top-level group replaced by its members
    // where it was, their frames mapped through the group's frame and child space; others untouched.
    r("arrange.ungroup", r##"{"ids": ["$Group"]}"##, "", &[("$Group", "children,look.kind,xfrm", "*"), ("$GA", "xfrm", "*"), ("$GB", "xfrm", "*")]),
    rs(
        r##"[["edit.select", {"ids": ["$Group"]}]]"##,
        "arrange.ungroup",
        r##"{}"##,
        "part,selection",
        &[("$Group", "children,look.kind,xfrm", "*"), ("$GA", "xfrm", "*"), ("$GB", "xfrm", "*")],
    ),
    r("arrange.ungroup", r##"{}"##, "part,selection", &[("$Rect", "-", "-")]),
    r(
        "arrange.ungroup",
        r##"{"ids": [268], "onSlide": 2}"##,
        "",
        &[("$Group", "children,look.kind,xfrm", "*"), ("$GA", "xfrm", "*"), ("$GB", "xfrm", "*"), ("$S2", "-", "*")],
    ),
    // arrange.regroup: after an Ungroup in the undo history, arrange.group on the current selection.
    rs(
        r##"[["arrange.ungroup", {"ids": ["$Group"]}], ["edit.select", {"ids": ["$GA", "$GB"]}]]"##,
        "arrange.regroup",
        r##"{}"##,
        "part,selection",
        &[("$GA", "look.ph,xfrm", "*"), ("$GB", "look.ph,xfrm", "*"), ("$S2", "~shapeCount", "-")],
    ),
    // arrange.align: targets' slide boxes (through their groups) aligned to the slide (one object,
    // or to: slide) or to the targets' union; edit_shapes writes the moved frames (a group member's
    // back in its group's space, the group refit); a moved end reroutes its connector.
    r(
        "arrange.align",
        r##"{"edge": "left", "ids": ["$Rect"]}"##,
        "",
        &[
            ("$Rect", "look.geom,xfrm", "-"),
            ("root", "props.slideSize", "-"),
            ("$Line", "look.geom,look.kind,xfrm", "-"),
            ("$Oval", "look.geom,xfrm", "-"),
        ],
    ),
    r(
        "arrange.align",
        r##"{"edge": "right"}"##,
        "part,selection",
        &[
            ("$Rect", "look.geom,xfrm", "-"),
            ("root", "props.slideSize", "-"),
            ("$Line", "look.geom,look.kind,xfrm", "-"),
            ("$Oval", "look.geom,xfrm", "-"),
        ],
    ),
    r("arrange.align", r##"{"edge": "top", "ids": ["$Rect", "$Text"]}"##, "", &[("$Rect", "xfrm", "-"), ("$Text", "xfrm", "-")]),
    r(
        "arrange.align",
        r##"{"edge": "center", "ids": ["$Title"]}"##,
        "",
        &[
            ("$Title", "look.ph,xfrm", "-"),
            ("$Layout:Title and Content/Title 1", "look.ph,xfrm", "-"),
            ("$S2", "layout", "-"),
            ("root", "props.slideSize", "-"),
        ],
    ),
    r(
        "arrange.align",
        r##"{"edge": "left", "ids": ["$GA"]}"##,
        "",
        &[("$GA", "xfrm", "-"), ("$Group", "look.kind,xfrm", "-"), ("$GB", "xfrm", "-"), ("root", "props.slideSize", "-")],
    ),
    r(
        "arrange.align",
        r##"{"edge": "bottom", "ids": [278], "onSlide": 3}"##,
        "",
        &[("$Star", "xfrm", "address"), ("$S3", "-", "*"), ("root", "props.slideSize", "-")],
    ),
    // arrange.distribute: targets' slide boxes spread across the slide (to: slide, or fewer than
    // three) or between the outermost; frames written as arrange.align does.
    r(
        "arrange.distribute",
        r##"{"dir": "horizontal", "ids": ["$Rect", "$Oval"]}"##,
        "",
        &[
            ("$Rect", "look.geom,xfrm", "-"),
            ("$Oval", "look.geom,xfrm", "-"),
            ("root", "props.slideSize", "-"),
            ("$Line", "look.geom,look.kind,xfrm", "-"),
        ],
    ),
    r(
        "arrange.distribute",
        r##"{"dir": "vertical", "to": "selection", "ids": ["$Rect", "$Oval", "$Text"]}"##,
        "",
        &[("$Rect", "look.geom,xfrm", "-"), ("$Oval", "look.geom,xfrm", "-"), ("$Text", "xfrm", "-"), ("$Line", "look.geom,look.kind,xfrm", "-")],
    ),
    r(
        "arrange.distribute",
        r##"{"dir": "horizontal"}"##,
        "part,selection",
        &[
            ("$Rect", "look.geom,xfrm", "-"),
            ("root", "props.slideSize", "-"),
            ("$Line", "look.geom,look.kind,xfrm", "-"),
            ("$Oval", "look.geom,xfrm", "-"),
        ],
    ),
    r(
        "arrange.distribute",
        r##"{"dir": "vertical", "ids": [278], "onSlide": 3}"##,
        "",
        &[("$Star", "xfrm", "address"), ("$S3", "-", "*"), ("root", "props.slideSize", "-")],
    ),
    // arrange.rotateLeft: shape.rotate by -90 through edit_shapes: each target's rotation changed
    // (placeholder frame materialised; group member through its group, the group refit); a turned
    // end reroutes its connector.
    r(
        "arrange.rotateLeft",
        r##"{"ids": ["$Rect"]}"##,
        "",
        &[("$Rect", "look.geom,xfrm", "-"), ("$Line", "look.geom,look.kind,xfrm", "-"), ("$Oval", "look.geom,xfrm", "-")],
    ),
    r(
        "arrange.rotateLeft",
        r##"{}"##,
        "part,selection",
        &[("$Rect", "look.geom,xfrm", "-"), ("$Line", "look.geom,look.kind,xfrm", "-"), ("$Oval", "look.geom,xfrm", "-")],
    ),
    r(
        "arrange.rotateLeft",
        r##"{"ids": ["$Title"]}"##,
        "",
        &[("$Title", "look.ph,xfrm", "-"), ("$Layout:Title and Content/Title 1", "look.ph,xfrm", "-"), ("$S2", "layout", "-")],
    ),
    r("arrange.rotateLeft", r##"{"ids": ["$GA"]}"##, "", &[("$GA", "xfrm", "-"), ("$Group", "look.kind,xfrm", "-"), ("$GB", "xfrm", "-")]),
    // arrange.rotateRight: shape.rotate by 90, as arrange.rotateLeft.
    r(
        "arrange.rotateRight",
        r##"{"ids": ["$Rect"]}"##,
        "",
        &[("$Rect", "look.geom,xfrm", "-"), ("$Line", "look.geom,look.kind,xfrm", "-"), ("$Oval", "look.geom,xfrm", "-")],
    ),
    r(
        "arrange.rotateRight",
        r##"{}"##,
        "part,selection",
        &[("$Rect", "look.geom,xfrm", "-"), ("$Line", "look.geom,look.kind,xfrm", "-"), ("$Oval", "look.geom,xfrm", "-")],
    ),
    r("arrange.rotateRight", r##"{"ids": ["$GA"]}"##, "", &[("$GA", "xfrm", "-"), ("$Group", "look.kind,xfrm", "-"), ("$GB", "xfrm", "-")]),
    r("arrange.rotateRight", r##"{"ids": [278], "onSlide": 3}"##, "", &[("$Star", "xfrm", "address"), ("$S3", "-", "*")]),
    // arrange.flipHorizontal: shape.flip horizontal: each target's flipH toggled (as rotate).
    r(
        "arrange.flipHorizontal",
        r##"{"ids": ["$Rect"]}"##,
        "",
        &[("$Rect", "look.geom,xfrm", "-"), ("$Line", "look.geom,look.kind,xfrm", "-"), ("$Oval", "look.geom,xfrm", "-")],
    ),
    r(
        "arrange.flipHorizontal",
        r##"{}"##,
        "part,selection",
        &[("$Rect", "look.geom,xfrm", "-"), ("$Line", "look.geom,look.kind,xfrm", "-"), ("$Oval", "look.geom,xfrm", "-")],
    ),
    r(
        "arrange.flipHorizontal",
        r##"{"ids": ["$Title"]}"##,
        "",
        &[("$Title", "look.ph,xfrm", "-"), ("$Layout:Title and Content/Title 1", "look.ph,xfrm", "-"), ("$S2", "layout", "-")],
    ),
    r("arrange.flipHorizontal", r##"{"ids": ["$GA"]}"##, "", &[("$GA", "xfrm", "-"), ("$Group", "look.kind,xfrm", "-"), ("$GB", "xfrm", "-")]),
    // arrange.flipVertical: shape.flip vertical, as arrange.flipHorizontal.
    r(
        "arrange.flipVertical",
        r##"{"ids": ["$Oval"]}"##,
        "",
        &[("$Oval", "look.geom,xfrm", "-"), ("$Line", "look.geom,look.kind,xfrm", "-"), ("$Rect", "look.geom,xfrm", "-")],
    ),
    r(
        "arrange.flipVertical",
        r##"{}"##,
        "part,selection",
        &[("$Rect", "look.geom,xfrm", "-"), ("$Line", "look.geom,look.kind,xfrm", "-"), ("$Oval", "look.geom,xfrm", "-")],
    ),
    r("arrange.flipVertical", r##"{"ids": [271], "onSlide": 2}"##, "", &[("$Text", "xfrm", "address"), ("$S2", "-", "*")]),
    // arrange.nudge: shape.move by dx, dy through edit_shapes (as rotate: placeholder frame
    // materialised, group member through its group and the group refit, connector rerouted).
    r(
        "arrange.nudge",
        r##"{"dx": 6, "dy": 0, "ids": ["$Rect"]}"##,
        "",
        &[("$Rect", "look.geom,xfrm", "-"), ("$Line", "look.geom,look.kind,xfrm", "-"), ("$Oval", "look.geom,xfrm", "-")],
    ),
    r(
        "arrange.nudge",
        r##"{"dx": 0, "dy": 1}"##,
        "part,selection",
        &[("$Rect", "look.geom,xfrm", "-"), ("$Line", "look.geom,look.kind,xfrm", "-"), ("$Oval", "look.geom,xfrm", "-")],
    ),
    r(
        "arrange.nudge",
        r##"{"dx": 6, "dy": 0, "ids": ["$Body"]}"##,
        "",
        &[("$Body", "look.ph,xfrm", "-"), ("$Layout:Title and Content/Content Placeholder 2", "look.ph,xfrm", "-"), ("$S2", "layout", "-")],
    ),
    r(
        "arrange.nudge",
        r##"{"dx": 6, "dy": 0, "ids": ["$GB"]}"##,
        "",
        &[("$GB", "xfrm", "-"), ("$Group", "look.kind,xfrm", "-"), ("$GA", "xfrm", "-")],
    ),
    r("arrange.nudge", r##"{"dx": 6, "dy": 0, "ids": [278], "onSlide": 3}"##, "", &[("$Star", "xfrm", "address"), ("$S3", "-", "*")]),
    // shape.merge: targets (ids, else selection; shapes or pictures, no lines): their outlines
    // (slide frame, geometry) combined; the results copy the first one (every member, text on the
    // first piece), take its place in the stack and replace the targets; reroute unglues $Line.
    r(
        "shape.merge",
        r##"{"op": "union", "ids": ["$Rect", "$Oval"]}"##,
        "",
        &[("$Rect", "*", "*"), ("$Oval", "look.geom,xfrm", "-"), ("$Line", "look.geom,look.kind,xfrm", "-")],
    ),
    rs(
        r##"[["edit.select", {"ids": ["$Oval", "$Rect"]}]]"##,
        "shape.merge",
        r##"{"op": "subtract"}"##,
        "part,selection",
        &[("$Oval", "*", "*"), ("$Rect", "look.geom,xfrm", "-"), ("$Line", "look.geom,look.kind,xfrm", "-")],
    ),
    r("shape.merge", r##"{"op": "fragment", "ids": ["$Text", "$Pic"]}"##, "", &[("$Text", "*", "*"), ("$Pic", "look.geom,xfrm", "-")]),
    r(
        "shape.merge",
        r##"{"op": "intersect", "ids": ["$Title", "$Text"]}"##,
        "",
        &[("$Title", "*", "*"), ("$Layout:Title and Content/Title 1", "look.ph,xfrm", "-"), ("$S2", "layout", "-"), ("$Text", "look.geom,xfrm", "-")],
    ),
    r(
        "shape.merge",
        r##"{"op": "combine", "ids": [263, 264], "onSlide": 2}"##,
        "",
        &[("$Rect", "*", "*"), ("$Oval", "look.geom,xfrm", "address"), ("$Line", "look.geom,look.kind,xfrm", "-"), ("$S2", "-", "*")],
    ),
];
