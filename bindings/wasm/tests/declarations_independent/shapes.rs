use super::Row;

// These rows were written from the handlers and their engine operations before opening the
// binding's declarations, under(), or declaration tests. `paths` records conditional reads;
// position independence concerns the target, not inherited geometry or an ancestor's transform.
macro_rules! row {
    (pinned $id:literal, $targets:expr, $objects:expr, $places:expr, $reads:expr, $independent:expr, $paths:expr) => {
        Row {
            id: $id,
            targets: $targets,
            objects: $objects,
            places: $places,
            reads: $reads,
            position_independent: $independent,
            paths: $paths,
            blind: false,
        }
    };
    ($id:literal, $targets:expr, $objects:expr, $places:expr, $reads:expr, $independent:expr, $paths:expr) => {
        Row {
            id: $id,
            targets: $targets,
            objects: $objects,
            places: $places,
            reads: $reads,
            position_independent: $independent,
            paths: $paths,
            blind: true,
        }
    };
}

// Shared shape resolution: explicit ids/id (ids wins), otherwise selection; local numbers and
// replica ids resolve in the current slide/layout/master tree. The call wrapper first resolves
// persistent identity globally or the old slide/number address, and chooses the implied slide.
// edit_shapes materializes missing xfrm from placeholder inheritance, reads table text metrics,
// and maps grouped members through every group's xfrm/child space before applying the mutation.
// Session::edit then refits affected groups, reroutes affected connectors from BOTH endpoints,
// settles stored precision, and refreshes shrink-to-fit results from theme/layout/text inputs.
pub(super) const ROWS: &[Row] = &[
    row!(pinned
        "shape.connect",
        Some("id"),
        &["from", "to"],
        &[],
        &["part"],
        true,
        &[
            "Editor-state metadata pinned from the declaration after review: Part includes the edited view; session defaults and helper reads remain documented in the paths",
            "from/to: shape ids; fromSite/toSite: site indexes, else nearest pair; reads endpoint kind, preset geometry and effective xfrm (placeholder layout/master); writes connector geometry and endpoint refs",
            "id given: existing line/connector by id, never selection; reads its line kind and retained style; rewrites its xfrm and glue",
            "id absent: inserts preset connector; reads current shape count for name and default_look only if preset is not line-like; appends to current shape list",
        ]
    ),
    row!(pinned
        "shape.freeform",
        None,
        &[],
        &[],
        &["part"],
        true,
        &[
            "Metadata pinned from the declaration after review; the edited tree is Part; explicit or newly generated ids do not use the previous shape/text selection",
            "points/closed/smooth: values; delegates shape.insert with explicit bounds, using default_look for closed paths; reads current shape count for name; appends custom geometry",
        ]
    ),
    row!(
        "shape.move",
        Some("ids"),
        &["id", "toSlide"],
        &[],
        &["selection", "part"],
        false,
        &[
            "toSlide absent: targets helper; absolute x/y plus relative dx/dy use effective world xfrm and each group transform; writes local xfrm, refits groups and attached connectors",
            "toSlide present: destination by slide id (not index); source current slide; reads moved descendants, source animations/comments anchored to them, destination occupied local shape numbers; moves shapes and owned animation/comment entries, remaps numeric refs on collision, appends to destination; relative/absolute coords use stored local xfrm",
        ]
    ),
    row!(
        "shape.resize",
        Some("ids"),
        &["id"],
        &[],
        &["selection", "part"],
        false,
        &[
            "ids/id else selection; reads effective xfrm, optional aspect ratio and rotation center plus ancestor transforms; writes width/height and rotated center position; reroutes connectors",
        ]
    ),
    row!(pinned
        "shape.delete",
        Some("ids"),
        &["id"],
        &[],
        &["part"],
        true,
        &[
            "Metadata pinned from the declaration after review; the edited tree is Part; explicit or newly generated ids do not use the previous shape/text selection",
            "ids/id required (no selection fallback); delegates edit.delete with resolved numeric ids; deletes descendants/empty ancestor groups, animations targeting explicit ids and dangling connector refs; comment anchors and descendant-only animation targets remain",
        ]
    ),
    row!(
        "shape.setBounds",
        Some("ids"),
        &["id"],
        &[],
        &["selection", "part"],
        false,
        &[
            "ids/id else selection; omitted x/y/w/h retain effective world xfrm components; reads ancestor transforms and placeholder inheritance; writes mapped local xfrm",
        ]
    ),
    row!(
        "shape.rotate",
        Some("ids"),
        &["id"],
        &[],
        &["selection", "part"],
        false,
        &[
            "ids/id else selection; deg absolute or existing rotation plus by; uses effective xfrm and group transforms; writes normalized rotation and reroutes connectors",
        ]
    ),
    row!(
        "shape.flip",
        Some("ids"),
        &["id"],
        &[],
        &["selection", "part"],
        false,
        &["ids/id else selection; axis value selects flip bit to toggle; reads effective xfrm and ancestor transforms; writes local xfrm",]
    ),
    row!(
        "shape.adjust",
        Some("ids"),
        &["id"],
        &[],
        &["selection", "part"],
        true,
        &[
            "index is preset adjustment slot (0..16), not another object; ids/id else selection; reads preset name and existing adjustments, fills missing slots with catalogue defaults; writes geom adjustments; edit_shapes may materialize inherited xfrm",
        ]
    ),
    row!(
        "shape.change",
        Some("ids"),
        &["id"],
        &[],
        &["selection", "part"],
        true,
        &[
            "preset is catalogue value; ids/id else selection; replaces geometry only, preserving effective xfrm through edit_shapes; shrink-to-fit refresh reads text, bounds and inherited text style",
        ]
    ),
    // C8-slide-pin r2: cmd/shape.rs add_media and cmd/insert.rs reuse media by the bytes supplied in these parameters.
    row!(
        "shape.fill",
        Some("ids"),
        &["id"],
        &["picture"],
        &["selection", "part"],
        true,
        &[
            "color/none/gradient/pattern/reset: values, scheme references remain references; ids/id else selection; recursively changes group members; materializes missing effective xfrm",
            "transparency: reads effective fill from shape/style/placeholder/layout/master/theme and slide color map, including group members; resolves inherited solid color to RGB then writes alpha",
            "picture: adds media from supplied bytes before edit_shapes validates targets; writes media ref and optional transparency",
        ]
    ),
    row!(
        "shape.line",
        Some("ids"),
        &["id"],
        &[],
        &["selection", "part"],
        true,
        &[
            "ids/id else selection; recursively edits group members; reads existing line fields to retain unspecified components and choose default fill when setting width; writes/reset line; edit_shapes may materialize inherited xfrm",
        ]
    ),
    row!(
        "shape.effects",
        Some("ids"),
        &["id"],
        &[],
        &["selection", "part"],
        true,
        &[
            "ids/id else selection; recursively edits group members; reads existing effect fields retained by partial update; writes/reset shadow/glow/soft edge/reflection/bevel; edit_shapes may materialize inherited xfrm",
        ]
    ),
    row!(
        "shape.quickStyle",
        Some("ids"),
        &["id"],
        &[],
        &["selection", "part"],
        true,
        &[
            "index is style catalogue value, not object index; ids/id else selection; recursively replaces style/fill/line/effects and clears run fills; scheme values stay references; materializes missing effective xfrm",
        ]
    ),
    row!(
        "shape.rename",
        Some("id"),
        &["ids"],
        &[],
        &["selection", "part"],
        true,
        &[
            "id/ids else selection through edit_shapes (despite singular params description); name is assigned value, never lookup; writes name and possibly materializes inherited xfrm",
        ]
    ),
    row!(
        "shape.altText",
        Some("ids"),
        &["id"],
        &[],
        &["selection", "part"],
        true,
        &[
            "ids/id else selection; descr set to text (empty default), decorative optionally updated; writes shape's own fields, not members; materializes missing effective xfrm",
        ]
    ),
    row!(
        "shape.visible",
        Some("ids"),
        &["id"],
        &[],
        &["selection", "part"],
        true,
        &[
            "ids/id else selection; visible supplied sets hidden inverse, otherwise reads current hidden to toggle; writes own hidden and may materialize inherited xfrm",
        ]
    ),
    row!(
        "shape.lock",
        Some("ids"),
        &["id"],
        &[],
        &["selection", "part"],
        true,
        &[
            "ids/id else selection; locked supplied assigns, otherwise reads current locked to toggle; writes own locked and may materialize inherited xfrm",
        ]
    ),
    row!(pinned
        // C8-slide-read-4 RIGHT: cmd/shape.rs:739-749 reads selected_shapes in the edited part; no target parameter.
        "shape.setDefault",
        None,
        &[],
        &[],
        &["selection", "part"],
        true,
        &[
            "Editor-state metadata pinned from the declaration after review: Part includes the edited view; session defaults and helper reads remain documented in the paths",
            "no target params; first selected shape on current edited part supplies first run props/fill/line/effects/style; writes session default_look only",
        ]
    ),
    row!(
        "picture.crop",
        Some("ids"),
        &["id"],
        &[],
        &["selection", "part"],
        true,
        &[
            "ids/id else selection; picture kind gate; partial crop retains omitted fractions; writes picture fill crop and may materialize inherited xfrm",
        ]
    ),
    row!(
        "picture.adjust",
        Some("ids"),
        &["id"],
        &[],
        &["selection", "part"],
        true,
        &[
            "ids/id else selection; picture kind gate; reset or partial brightness/contrast/saturation/grayscale/alpha retains others; writes picture fill adjustment and may materialize inherited xfrm",
        ]
    ),
    row!(
        "picture.reset",
        Some("ids"),
        &["id"],
        &[],
        &["selection", "part"],
        true,
        &[
            "ids/id else selection; resets picture fill crop/adjustment/alpha if picture, but effects and line reset for every target kind; materializes inherited xfrm",
        ]
    ),
    row!(
        "picture.change",
        Some("ids"),
        &["id"],
        &[],
        &["selection", "part"],
        true,
        &[
            "ids/id else selection; path/data values supply new media, added before target resolution; picture kind receives new media and zero crop while retaining other fill data; materializes inherited xfrm",
        ]
    ),
    row!(
        "media.options",
        Some("ids"),
        &["id"],
        &[],
        &["selection", "part"],
        true,
        &[
            "ids/id else selection; media kind gate; partial playback option values retain existing fields; writes clip playback fields and may materialize inherited xfrm",
        ]
    ),
    row!(pinned
        // C8-slide-read-4 RIGHT: cmd/insert.rs:115-138,147-187 append in the edited part using supplied values/defaults.
        "shape.insert",
        None,
        &[],
        &[],
        &["part"],
        true,
        &[
            "Editor-state metadata pinned from the declaration after review: Part includes the edited view; session defaults and helper reads remain documented in the paths",
            "explicit rect: preset/adj/flip/text are values; non-line shape copies session default_look; appends to current edited shape tree, naming by top-level shape count",
            "rect absent: centered 72pt box (line height zero) reads document slide size; then same insertion",
        ]
    ),
    row!(
        "insert.textBox",
        None,
        &[],
        &[],
        &["part"],
        true,
        &[
            "rect absent: centered 216x36 box reads document slide size; explicit rect bypasses that default; appends named text box by shape count",
            "text layout: autofit Shape and wrap from new body's defaults; fit_text_box reads owning slide layout/master/theme/fonts and geometry/insets; writes fitted bounds; empty text starts editor text selection",
        ]
    ),
    row!(
        "insert.picture",
        None,
        &[],
        &["data", "path"],
        &["selection", "part"],
        true,
        &[
            "selected empty picture/content/body placeholder: reads selected shapes' ph type and empty text, effective placeholder box, natural media aspect; replaces placeholder kind/text/descr/xfrm and ph type even with explicit rect; preserves ph identity",
            "no selected empty placeholder: explicit rect used, else image fitted to document slide size and centered; adds media and appends named picture by shape count",
        ]
    ),
    row!(
        "insert.table",
        None,
        &[],
        &[],
        &["part"],
        true,
        &[
            "rows/cols/data are values (cell indexes internal to newly built table); absent rect reads document slide size; appends table named by shape count; new cells have default text style",
        ]
    ),
    row!(
        "insert.chart",
        None,
        &[],
        &[],
        &["part"],
        true,
        &[
            "type/categories/series/title are values; single supplied series provides default title, pie-like varies colors, combo defaults from supplied series; absent rect reads document slide size; appends chart named by shape count",
        ]
    ),
    row!(
        "insert.audio",
        None,
        &[],
        &["data", "path"],
        &["part"],
        true,
        &[
            "path/data supplies media bytes; probe determines audio/video metadata; absent rect reads document slide size to center 48pt object; adds media and appends named clip by shape count",
        ]
    ),
    row!(
        "insert.video",
        None,
        &[],
        &["data", "path"],
        &["part"],
        true,
        &[
            "path/data supplies media bytes; probe and decoded poster determine clip metadata; absent rect fits natural dimensions into document slide size; adds media/poster and appends named clip by shape count",
        ]
    ),
    row!(
        "insert.artText",
        None,
        &[],
        &[],
        &["part"],
        true,
        &[
            "text/style values build own run properties and scheme refs; frame reads document slide size; appends named box then fits text using layout/master/theme fonts and body metrics",
        ]
    ),
    row!(
        "insert.slideNumber",
        None,
        &[],
        &[],
        &["selection", "part"],
        true,
        &[
            "text selection present: uses selected shape/cell/notes body and caret/range, props_at insertion point; replaces range with field run, fits text box and refreshes autofit",
            "not editing text: delegates design.headerFooter slideNumber=true on current slide; reads inherited footer placeholders, theme/layout/master and slide's header/footer setting",
        ]
    ),
    row!(
        "insert.dateTime",
        None,
        &[],
        &[],
        &["selection", "part"],
        true,
        &[
            "text selection present: format selects date field/time value; reads selected shape/cell/notes text and caret/range/run props; inserts field and fits text box",
            "not editing text: delegates design.headerFooter date=true on current slide; reads existing date settings/inherited footer placeholders; input format is not forwarded",
        ]
    ),
    row!(pinned
        "insert.symbol",
        None,
        &[],
        &[],
        &["selection", "part"],
        true,
        &[
            "Editor-state metadata pinned from the declaration after review: Part includes the edited view; session defaults and helper reads remain documented in the paths",
            "requires text selection; reads selected shape/cell/notes text range and run props, smart_quotes/autocorrect prefs and text-fit inheritance; replaces range with text and moves caret",
        ]
    ),
    row!(
        "insert.hyperlink",
        Some("ids"),
        &["id"],
        &["slide"],
        &["selection", "part"],
        true,
        &[
            "url present: URL value wins and slide param is not resolved; otherwise slide is ZERO-based destination slide index, reads destination id and title for default display text (title unused when explicit text given)",
            "original reading: active text used caret/range and selected body even with explicit ids, so it could write another shape; C8-slide-read regression/fix now gives explicit id/ids priority",
            "without explicit targets, active text inserts display text at an empty caret and formats runs with link, fitting the box; otherwise ids/id else shape selection sets click through edit_shapes, materializing inherited xfrm; no destination geometry is read",
        ]
    ),
    row!(
        "chart.type",
        Some("id"),
        &["ids"],
        &[],
        &["selection", "part"],
        true,
        &[
            "id/ids else selection through edit_shapes; reads chart kind, series kinds/count and vary_colors to reset series kinds/combo defaults; writes chart kind/series/vary_colors and materializes inherited xfrm",
        ]
    ),
    row!(
        "chart.data",
        Some("id"),
        &["ids"],
        &[],
        &["selection", "part"],
        true,
        &[
            "id/ids else selection; series list entries are new values indexed by position into old series to keep omitted fill/type/totals and all unmodeled formatting; writes categories and series, materializes inherited xfrm",
        ]
    ),
    row!(
        "chart.options",
        Some("id"),
        &["ids"],
        &[],
        &["selection", "part"],
        true,
        &[
            "id/ids else selection; partial options retain fields; colors index points when vary_colors and single series, otherwise series, with line defaults depending on existing series/chart kinds; writes chart options and materializes inherited xfrm",
        ]
    ),
    row!(pinned
        "insert.smartGraphic",
        None,
        &[],
        &[],
        &["part"],
        true,
        &[
            "Metadata pinned from the declaration after review; the edited tree is Part; explicit or newly generated ids do not use the previous shape/text selection",
            "kind/items and rect values build nodes/link shapes with own scheme refs; missing/invalid rect reads document slide size; appends shapes named by count then groups own generated ids using their geometry/order, sets group frame/name",
        ]
    ),
    row!(
        "insert.actionButton",
        None,
        &[],
        &[],
        &["part"],
        true,
        &[
            "kind chooses catalogue preset and relative slide action; no target slide resolved; absent rect centers 48pt box from document slide size; appends named shape by shape count",
        ]
    ),
    row!(
        "drawing.insert",
        None,
        &[],
        &[],
        &["part"],
        true,
        &[
            "kind/geometry/fill/stroke/text/body/chart are values; placement must slide; absent frame reads document slide size; creates chart from supplied spec or shape with own accent defaults, appends by shape count; session default_look is not consulted",
            "answer advice reads resulting shape and slide-wide design context after write; font notes consult installed fonts; these do not change written document",
        ]
    ),
    row!(pinned
        "drawing.delete",
        Some("ids"),
        &["id"],
        &[],
        &["part"],
        true,
        &[
            "Metadata pinned from the declaration after review; the edited tree is Part; explicit or newly generated ids do not use the previous shape/text selection",
            "ids or id required, no selection fallback; local numbers/replica/persistent ids in current tree; reads drawing kind then delegates edit.delete with exact resolved ids; deletes descendants/empty ancestor groups, explicit-id animations and dangling glue; comment anchors remain",
        ]
    ),
    row!(pinned
        // C8-slide-read-4 REPAIR: cmd/drawing.rs:291-349 changes resolved frames, never stack position; independent false -> true.
        "drawing.transform",
        Some("ids"),
        &["id"],
        &[],
        &["part"],
        true,
        &[
            "Metadata pinned from the declaration after review; the edited tree is Part; explicit or newly generated ids do not use the previous shape/text selection",
            "ids or id required, no selection fallback; frame sets single object's absolute frame or delta transform reads effective world frame including group ancestors; rejects changing a member's frame; writes changed frame through write_back",
            "reviewed after the absolute-frame fix (6ef28c0): frame Set uses the resolved object's persistent identity, matching to_drawing for numeric, persistent and scoped input ids",
        ]
    ),
    row!(pinned
        // C8-slide-read-4 RIGHT: cmd/drawing.rs:291-349 resolves required ids in the edited part; no old selection or stack read.
        "drawing.format",
        Some("ids"),
        &["id"],
        &[],
        &["part"],
        true,
        &[
            "Metadata pinned from the declaration after review; the edited tree is Part; explicit or newly generated ids do not use the previous shape/text selection",
            "ids or id required; reads kind/effective frame and existing own fill/stroke/geometry/effects; merges partial stroke/effects and writes changed members; missing xfrm materializes effective geometry; rejects ink edits and non-frame group edits",
        ]
    ),
    row!(pinned
        // C8-slide-read-4 REPAIR: cmd/drawing.rs:291-354 replaces resolved text and fits its frame, never stack position; independent false -> true.
        "drawing.text",
        Some("id"),
        &[],
        &[],
        &["part"],
        true,
        &[
            "Metadata pinned from the declaration after review; apply resolves required ids in the edited Part and does not use the old shape/text selection",
            "id required; text/paragraphs replace content, body replaces body layout while preserving omitted paragraphs, warp separately; reads current text/body and effective xfrm; writes changed text/body and materializes missing xfrm; fits Shape-autofit box with inherited layout/master/theme/fonts",
            "answer advice reads resulting shape and slide-wide design context after write without changing document",
        ]
    ),
    // Pinned from the declaration after review: blind paths read stack order, so PI true was a transcription error.
    row!(pinned
        // C8-slide-read-4 RIGHT: cmd/drawing.rs:361-380 resolves required ids in the part and reads their stack positions.
        "drawing.arrange",
        Some("ids"),
        &[],
        &[],
        &["part"],
        false,
        &[
            "Metadata pinned from the declaration after review: position independence false; the stack-order read below was written blind",
            "ids required, order value; resolves ids and rejects members not top-level; reads current shape stack/order (not sibling content); computes front/back/forward/backward order and rewrites stack",
        ]
    ),
    row!(pinned
        // C8-slide-read-4 REPAIR: cmd/drawing.rs:323-346 resolves connector/end ids and geometry, never stack position; independent false -> true.
        "drawing.reconnect",
        Some("id"),
        &["to"],
        &[],
        &["part"],
        true,
        &[
            "Metadata pinned from the declaration after review; apply resolves required ids in the edited Part and does not use the old shape/text selection",
            "id required existing connector; end value chooses endpoint; to shape id/site index optional or free point value; reads retained endpoint ref/geometry and endpoint shapes' effective geometry/sites; writes glue and possibly xfrm then reroutes both ends",
        ]
    ),
    row!(pinned
        // C8-slide-read-4 RIGHT: cmd/drawing.rs:389-405 resolves from/to in the part; neither endpoint is the new connector target.
        "drawing.connect",
        None,
        &["from", "to"],
        &[],
        &["part"],
        true,
        &[
            "Editor-state metadata pinned from the declaration after review: Part includes the edited view; session defaults and helper reads remain documented in the paths",
            "from/to required drawing ids; fromSite/toSite indexes else closest pair from endpoint effective boxes/preset sites; delegates shape.connect without id, appends connector named by shape count; default_look only if supplied preset not line-like",
        ]
    ),
    row!(pinned
        // C8-slide-read-4 RIGHT: cmd/drawing.rs:410-426 requires ids and delegates group at their original stack positions.
        "drawing.group",
        Some("ids"),
        &[],
        &[],
        &["part"],
        false,
        &[
            "Metadata pinned from the declaration after review; the edited tree is Part; explicit or newly generated ids do not use the previous shape/text selection",
            "ids required, >=2; resolves only top-level drawing kinds; delegates arrange.group; reads selected member content/effective bounds and stack order, appends group at topmost member and clears ph links",
        ]
    ),
    row!(pinned
        "drawing.ungroup",
        Some("ids"),
        &[],
        &[],
        &["part"],
        false,
        &[
            "Metadata pinned from the declaration after review; the edited tree is Part; explicit or newly generated ids do not use the previous shape/text selection",
            "ids required; top-level group kind check; reads groups' child lists, group xfrm and child coordinate space; replaces each group by transformed children at same stack location",
        ]
    ),
    row!(pinned
        // C8-slide-read-4 RIGHT: cmd/drawing.rs:291-349 resolves required id(s) in the part and changes chart fields, not stack.
        "drawing.chart",
        Some("id"),
        &["ids"],
        &[],
        &["part"],
        true,
        &[
            "Metadata pinned from the declaration after review; apply resolves required ids in the edited Part and does not use the old shape/text selection",
            "id or one-element ids required; reads full current chart spec including kind/combo/categories/series/axes and retained raw content; seriesFormat.index indexes existing series, never shape objects; partial spec update retains unspecified formatting; write_back may materialize effective xfrm",
        ]
    ),
    row!(pinned
        // C8-slide-read-4 RIGHT: cmd/ink.rs:119-149 appends to the last ink layer in the part; no target parameter.
        "drawing.ink",
        None,
        &[],
        &[],
        &["part"],
        true,
        &[
            "Editor-state metadata pinned from the declaration after review: Part includes the edited view; session defaults and helper reads remain documented in the paths",
            "stroke tool/color/width default to current ToolKind; scans top-level current shapes in reverse for last ink layer; appends strokes to existing layer (reads its kind/order/stroke list)",
            "no top-level ink layer: creates layer using document slide size; appends new ink shape then strokes; other non-ink content is not a dependency",
        ]
    ),
    row!(
        "drawing.erase",
        None,
        &[],
        &["path", "x", "y"],
        &["part"],
        true,
        &[
            "gesture is point/path plus radius; scans EVERY top-level ink shape and stroke polyline/width to choose hit stroke indexes; deletes exactly those strokes; non-ink content irrelevant and geometry xfrm unused because ink points are slide coordinates",
        ]
    ),
    // Pinned from the declaration after review: position_independent true -> false.
    row!(pinned
        // C8-slide-read-4 RIGHT: cmd/arrange.rs:51-83 reads targets/selection and top-level stack order in the part.
        "arrange.bringToFront",
        Some("ids"),
        &["id"],
        &[],
        &["selection", "part"],
        false,
        &[
            "Metadata pinned from the declaration after review: position independence false; the stack-order read below was written blind",
            "ids/id else selection; reads top-level stack membership/order only, partitions named shapes behind/in front of rest; writes stack order, leaves grouped members untouched",
        ]
    ),
    // Pinned from the declaration after review: position_independent true -> false.
    row!(pinned
        // C8-slide-read-4 RIGHT: cmd/arrange.rs:51-83 reads targets/selection and adjacent stack positions in the part.
        "arrange.bringForward",
        Some("ids"),
        &["id"],
        &[],
        &["selection", "part"],
        false,
        &[
            "Metadata pinned from the declaration after review: position independence false; the stack-order read below was written blind",
            "ids/id else selection; reads top-level stack order and adjacent nonselected siblings, swaps one step forward preserving selected order; writes stack only",
        ]
    ),
    // Pinned from the declaration after review: position_independent true -> false.
    row!(pinned
        // C8-slide-read-4 RIGHT: cmd/arrange.rs:51-83 reads targets/selection and adjacent stack positions in the part.
        "arrange.sendBackward",
        Some("ids"),
        &["id"],
        &[],
        &["selection", "part"],
        false,
        &[
            "Metadata pinned from the declaration after review: position independence false; the stack-order read below was written blind",
            "ids/id else selection; reads top-level stack order and adjacent nonselected siblings, swaps one step backward preserving selected order; writes stack only",
        ]
    ),
    // Pinned from the declaration after review: position_independent true -> false.
    row!(pinned
        "arrange.sendToBack",
        Some("ids"),
        &["id"],
        &[],
        &["selection", "part"],
        false,
        &[
            "Metadata pinned from the declaration after review: position independence false; the stack-order read below was written blind",
            "ids/id else selection; reads top-level stack membership/order only, partitions named shapes to back preserving order; writes stack only",
        ]
    ),
    // Pinned from the declaration after review: position_independent true -> false.
    row!(pinned
        "arrange.reorder",
        Some("id"),
        &[],
        &["index"],
        &["part"],
        false,
        &[
            "Metadata pinned from the declaration after review: position independence false; the stack-order read below was written blind",
            "id numeric target required (wrapper accepts tool id); index is destination gap in top-level current shape list after removing source, clamped to list length; reads target's stack index/membership and destination list order; writes stack",
        ]
    ),
    row!(
        "arrange.group",
        Some("ids"),
        &["id"],
        &[],
        &["selection", "part"],
        false,
        &[
            "ids/id else selection; takes top-level shapes only; reads members' full retained content/effective boxes and current stack order/count; inherits missing xfrm then removes ph links; creates group at topmost selected stack position",
        ]
    ),
    row!(
        "arrange.ungroup",
        Some("ids"),
        &["id"],
        &[],
        &["selection", "part"],
        false,
        &[
            "ids/id else selection; only top-level groups with xfrm are expanded; reads group xfrm/child coordinate system and retained child content; replaces group by transformed children and selects them",
        ]
    ),
    row!(pinned
        "arrange.regroup",
        None,
        &[],
        &[],
        &["selection", "part"],
        false,
        &[
            "Editor-state metadata pinned from the declaration after review: Part includes the edited view; session defaults and helper reads remain documented in the paths",
            "no target params; reads undo history for an Ungroup label but ignores that entry's selected ids; CURRENT selection is grouped via arrange.group, so current selected shapes/order/geometry are dependencies; writes new group",
        ]
    ),
    row!(
        "arrange.align",
        Some("ids"),
        &["id"],
        &[],
        &["selection", "part"],
        false,
        &[
            "ids/id else selection; to slide (default for <2 found) reads document slide size; to selection reads union of effective target bounds; edit_shapes reads group transforms before applying world-space delta; writes local positions",
            "reference boxes use xfrm_of (local coordinates for group children), while mutation receives slide-space boxes; same fact follows different coordinate paths for grouped children",
        ]
    ),
    row!(
        "arrange.distribute",
        Some("ids"),
        &["id"],
        &[],
        &["selection", "part"],
        false,
        &[
            "ids/id else selection; dir axis value; to slide or <3 targets reads document slide size, otherwise target bounds/extreme edges; sorts by effective bounds and derives gaps; edit_shapes uses ancestor transforms; writes positions",
            "initial reference boxes are group-local via xfrm_of but mutations are slide-space through edit_shapes, so grouped-member placement computes mixed coordinate spaces",
        ]
    ),
    row!(
        "arrange.rotateLeft",
        Some("ids"),
        &["id"],
        &[],
        &["selection", "part"],
        false,
        &["ids/id else selection; delegates shape.rotate by=-90; reads effective target xfrm and ancestor transforms; writes rotation",]
    ),
    row!(
        "arrange.rotateRight",
        Some("ids"),
        &["id"],
        &[],
        &["selection", "part"],
        false,
        &["ids/id else selection; delegates shape.rotate by=90; reads effective target xfrm and ancestor transforms; writes rotation",]
    ),
    row!(
        "arrange.flipHorizontal",
        Some("ids"),
        &["id"],
        &[],
        &["selection", "part"],
        false,
        &["ids/id else selection; delegates shape.flip horizontal; reads effective target xfrm and ancestor transforms; writes flip",]
    ),
    row!(
        "arrange.flipVertical",
        Some("ids"),
        &["id"],
        &[],
        &["selection", "part"],
        false,
        &["ids/id else selection; delegates shape.flip vertical; reads effective target xfrm and ancestor transforms; writes flip",]
    ),
    row!(
        "arrange.nudge",
        Some("ids"),
        &["id"],
        &[],
        &["selection", "part"],
        false,
        &[
            "ids/id else selection; delegates shape.move including any toSlide path; reads effective xfrm/ancestors or destination slide/number collisions/anchored animations/comments; writes moved geometry/tree",
        ]
    ),
    row!(
        "shape.merge",
        Some("ids"),
        &["id"],
        &[],
        &["selection", "part"],
        false,
        &[
            "ids/id else selection IN CALLER ORDER; op value; reads targets' kind/preset/custom geometry/effective xfrm and first target's retained formatting; computes polygon boolean result and reads first target's stack position; removes top-level originals and inserts result shapes retaining first target's kind/style, first piece text, no placeholder; animation/comment refs remain",
        ]
    ),
];
