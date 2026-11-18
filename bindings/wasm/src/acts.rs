//! What each command acts on and the editor state it reads besides its parameters
//! (`KitCommand::targets`, `KitCommand::reads`), for kition-sdk's explicit call context: a host
//! sets the selection and the current slide a command reads from a run step's `on` and `at.part`,
//! and puts them back after the call (task card C4-slide).
//!
//! `reads` is what the engine's code reads, not what descriptions say: `tests/acts.rs` follows
//! each command's `enabled` predicate and `run` code through the engine to the accessors of the
//! selection and of the current slide, and fails when this table says otherwise.
use kition_kit::EditorState::{self, Part, Selection};
use serde_json::Value;

const SELECTION_AND_PART: &[EditorState] = &[Selection, Part];
const PART: &[EditorState] = &[Part];
const SELECTION: &[EditorState] = &[Selection];

/// The editor state command `id` reads besides its parameters: the selection (the selected shapes
/// or slides, the text being edited, table cells) and the current slide (or the master or layout
/// in Slide Master view). Slides have no time.
pub fn reads(id: &str) -> &'static [EditorState] {
    match id {
        "edit.undo"
        | "edit.redo"
        | "edit.cut"
        | "edit.copy"
        | "edit.paste"
        | "edit.pasteText"
        | "edit.duplicate"
        | "edit.delete"
        | "edit.selectAll"
        | "edit.select"
        | "slide.duplicate"
        | "slide.delete"
        | "slide.hide"
        | "slide.layout"
        | "insert.picture"
        | "insert.slideNumber"
        | "insert.dateTime"
        | "insert.symbol"
        | "insert.hyperlink"
        | "chart.type"
        | "chart.data"
        | "chart.options"
        | "shape.move"
        | "shape.resize"
        | "shape.setBounds"
        | "shape.rotate"
        | "shape.flip"
        | "shape.adjust"
        | "shape.change"
        | "shape.fill"
        | "shape.line"
        | "shape.effects"
        | "shape.quickStyle"
        | "shape.rename"
        | "shape.altText"
        | "shape.visible"
        | "shape.lock"
        | "shape.setDefault"
        | "picture.crop"
        | "picture.adjust"
        | "picture.reset"
        | "picture.change"
        | "media.options"
        | "drawing.describe"
        | "arrange.bringToFront"
        | "arrange.bringForward"
        | "arrange.sendBackward"
        | "arrange.sendToBack"
        | "arrange.group"
        | "arrange.ungroup"
        | "arrange.regroup"
        | "arrange.align"
        | "arrange.distribute"
        | "arrange.rotateLeft"
        | "arrange.rotateRight"
        | "arrange.flipHorizontal"
        | "arrange.flipVertical"
        | "arrange.nudge"
        | "shape.merge"
        | "text.edit"
        | "text.insert"
        | "text.delete"
        | "text.move"
        | "text.selectAll"
        | "text.selectWord"
        | "text.selectParagraph"
        | "text.set"
        | "text.get"
        | "format.bold"
        | "format.italic"
        | "format.underline"
        | "format.strikethrough"
        | "format.superscript"
        | "format.subscript"
        | "format.font"
        | "format.size"
        | "format.grow"
        | "format.shrink"
        | "format.color"
        | "format.highlight"
        | "format.case"
        | "format.caps"
        | "format.spacing"
        | "format.clear"
        | "format.textShadow"
        | "format.textOutline"
        | "format.transform"
        | "format.align"
        | "format.rtl"
        | "format.alignLeft"
        | "format.alignCenter"
        | "format.alignRight"
        | "format.justify"
        | "format.bullets"
        | "format.numbering"
        | "format.indent"
        | "format.outdent"
        | "format.lineSpacing"
        | "format.paragraph"
        | "format.anchor"
        | "format.direction"
        | "format.columns"
        | "format.autofit"
        | "format.wrap"
        | "format.margins"
        | "format.painter"
        | "format.painterApply"
        | "format.state"
        | "transition.set"
        | "transition.options"
        | "transition.timing"
        | "animation.set"
        | "animation.add"
        | "animation.remove"
        | "animation.timing"
        | "animation.options"
        | "animation.painter"
        | "animation.painterApply"
        | "table.state"
        | "table.cellFormat"
        | "table.border"
        | "table.clearCells"
        | "table.insertRowAbove"
        | "table.insertRowBelow"
        | "table.insertColumnLeft"
        | "table.insertColumnRight"
        | "table.deleteRow"
        | "table.deleteColumn"
        | "table.merge"
        | "table.split"
        | "table.style"
        | "table.options"
        | "table.cellFill"
        | "table.distributeRows"
        | "table.distributeColumns"
        | "table.columnWidth"
        | "table.rowHeight"
        | "table.selectCells"
        | "comment.add"
        | "shape.inspect"
        | "media.play"
        | "media.pause"
        | "media.toggle"
        | "media.stop"
        | "media.seek"
        | "media.info"
        | "media.posterFrame"
        | "media.gifPlay"
        | "comment.anchor" => SELECTION_AND_PART,
        "file.export"
        | "file.exportBytes"
        | "slide.new"
        | "slide.move"
        | "slide.go"
        | "slide.next"
        | "slide.previous"
        | "slide.first"
        | "slide.last"
        | "slide.reset"
        | "slide.notes"
        | "slide.rename"
        | "section.add"
        | "section.remove"
        | "slide.fromOutline"
        | "shape.freeform"
        | "shape.delete"
        | "insert.smartGraphic"
        | "drawing.delete"
        | "drawing.transform"
        | "drawing.format"
        | "drawing.text"
        | "drawing.reconnect"
        | "drawing.group"
        | "drawing.ungroup"
        | "drawing.chart"
        | "design.fonts"
        | "shape.insert"
        | "insert.textBox"
        | "insert.table"
        | "insert.chart"
        | "insert.audio"
        | "insert.video"
        | "insert.artText"
        | "insert.actionButton"
        | "shape.connect"
        | "shape.sites"
        | "drawing.insert"
        | "drawing.arrange"
        | "drawing.connect"
        | "drawing.ink"
        | "drawing.erase"
        | "arrange.reorder"
        | "design.theme"
        | "design.colors"
        | "design.background"
        | "design.hideBackgroundGraphics"
        | "design.headerFooter"
        | "view.slideMaster"
        | "master.insertLayout"
        | "master.renameLayout"
        | "master.deleteLayout"
        | "master.insertPlaceholder"
        | "transition.applyAll"
        | "animation.move"
        | "animation.clear"
        | "animation.get"
        | "comment.reply"
        | "comment.delete"
        | "comment.resolve"
        | "comment.list"
        | "comment.threads"
        | "show.fromCurrent"
        | "slide.inspect"
        | "presence.selection"
        | "presence.locate"
        | "comment.place" => PART,
        "edit.deselect" | "slide.selectSlides" | "text.exit" | "text.select" => SELECTION,
        _ => &[],
    }
}

/// Commands whose parameter `id` is not what they act on, with what is.
pub const NOT_TARGETS: &[(&str, &str)] = &[("comment.add", "`id` anchors the comment; the comment is on slide `slide`")];

/// The parameter that names what command `id` acts on (shapes or slides, by the ids slide_inspect
/// lists): `ids`, else `id`. `None` for a command without one.
pub fn names_objects(id: &str, schema: &Value) -> Option<&'static str> {
    if NOT_TARGETS.iter().any(|(command, _)| *command == id) {
        return None;
    }
    ["ids", "id"].into_iter().find(|name| schema.get(*name).is_some())
}

/// [`names_objects`] as `KitCommand::targets` declares it: only where kition-sdk's
/// `schema::check_targets` takes the parameter's schema for ids, including this kit's number or
/// string (`oneOf`) forms. A command without a target parameter takes `on` through its selection.
pub fn targets(id: &str, schema: Option<&Value>) -> Option<&'static str> {
    let schema = schema?;
    names_objects(id, schema).filter(|name| kition_kit::schema::check_targets(schema, name).is_ok())
}

/// Mark the completed schema: notation conversion and `described::type_keys` may add
/// the single-id form beside a list. Numbers in all of these parameters name objects.
pub fn mark_objects(command: &mut kition_kit::KitCommand) {
    const SHAPE: &[&str] = &["Shape"];
    const SLIDE: &[&str] = &["Slide"];
    const LAYOUT: &[&str] = &["Layout"];
    let has = |name: &str| command.schema.as_ref().is_some_and(|schema| schema.get(name).is_some());
    command.objects = match command.id.as_str() {
        "shape.move" => &["toSlide"],
        "slide.new" | "slide.layout" => &["layout"],
        "shape.connect" => &["from", "to"],
        "drawing.connect" => &["from", "to"],
        "drawing.reconnect" => &["to"],
        "animation.timing" => &["trigger"],
        "comment.add" => &["id"],
        "comment.reply" | "comment.resolve" => &["thread"],
        "comment.delete" => &["thread", "comment"],
        "file.render" => &["target.slideId"],
        _ if has("id") && command.targets != Some("id") => &["id"],
        _ => &[],
    };
    command.object_types = match command.id.as_str() {
        "shape.move" => &[("ids", SHAPE), ("toSlide", SLIDE)],
        "slide.new" => &[("layout", LAYOUT)],
        "slide.layout" => &[("layout", LAYOUT)],
        "shape.connect" => &[("from", SHAPE), ("to", SHAPE), ("id", SHAPE)],
        "drawing.connect" => &[("from", SHAPE), ("to", SHAPE)],
        "drawing.reconnect" => &[("id", SHAPE), ("to", SHAPE)],
        "animation.timing" => &[("trigger", SHAPE)],
        "comment.add" => &[("id", SHAPE)],
        "comment.reply" | "comment.resolve" => &[("thread", &["CommentThread"])],
        "comment.delete" => &[("thread", &["CommentThread"]), ("comment", &["Comment", "CommentThread"])],
        "file.render" => &[("target.slideId", SLIDE)],
        id if id.starts_with("slide.") => match command.targets {
            Some("id") => &[("id", SLIDE)],
            Some("ids") => &[("ids", SLIDE)],
            _ => &[],
        },
        _ if has("id") && has("ids") => &[("id", SHAPE), ("ids", SHAPE)],
        _ if has("id") => &[("id", SHAPE)],
        _ if has("ids") => &[("ids", SHAPE)],
        _ => &[],
    };
    command.name_params = match command.id.as_str() {
        "slide.new" | "slide.layout" => &[("layout", "props.name")],
        _ => &[],
    };
    if has(crate::ON_SLIDE) {
        command.places = match command.id.as_str() {
            "comment.add" => &["slide", "onSlide"],
            "insert.hyperlink" => &["slide", "onSlide"],
            "animation.remove" => &["index", "onSlide"],
            "arrange.reorder" => &["index", "onSlide"],
            "insert.picture" | "insert.audio" | "insert.video" => &["data", "path", "onSlide"],
            "shape.fill" => &["picture", "onSlide"],
            "table.insertRowAbove" | "table.insertRowBelow" => &["row", "onSlide"],
            "table.insertColumnLeft" | "table.insertColumnRight" => &["col", "onSlide"],
            _ => &["onSlide"],
        };
    }
}

/// Commands whose work is moving the editor (the selection, the current slide, the text being
/// edited, Slide Master view, the tool): given ids or `onSlide`, they keep what they set, where
/// every other command leaves the editor as it was ([`crate::SlideDocument::run_named`]). A host
/// still puts an agent's run back afterwards.
pub fn moves_the_editor(id: &str) -> bool {
    matches!(
        id,
        "edit.select"
            | "edit.selectAll"
            | "edit.deselect"
            | "slide.go"
            | "slide.next"
            | "slide.previous"
            | "slide.first"
            | "slide.last"
            | "slide.selectSlides"
            | "text.edit"
            | "text.exit"
            | "text.move"
            | "text.select"
            | "text.selectAll"
            | "text.selectWord"
            | "text.selectParagraph"
            | "table.selectCells"
            | "view.slideMaster"
            | "view.closeMaster"
    )
}

/// The parameters of command `id` that name a slide or animation by its place rather than by its id
/// (kition-sdk `KitCommand::places`, task card C8-slide-5): a slide's index (`index`, from 0), the
/// slide a move takes and where it puts it (`from`, `to`), where a new slide goes (`at`). A step
/// that gives one and no slide ids acts on whatever slide is there; with `basis`, its premise is
/// that the slide there is the one that was ([`crate::SlideDocument`]'s `under`). A slide named by
/// `at.part` (`{slide: n}`) is the part premise's, and a shape's number on its slide (`id: 13`)
/// is its id there, not a place. Animation commands' `index` names an item in the slide's animation
/// list; its `under` premise holds that item (the whole list for a reorder).
pub fn places(id: &str) -> &'static [&'static str] {
    match id {
        "slide.duplicate" | "slide.delete" | "slide.hide" | "slide.layout" | "slide.reset" | "slide.notes" | "slide.rename" | "transition.set"
        | "transition.options" | "transition.timing" | "animation.clear" => &["index"],
        "slide.move" => &["from", "to"],
        "slide.new" => &["at"],
        "section.add" => &["at"],
        "section.move" => &["index", "to"],
        "section.rename" | "section.remove" => &["index"],
        "comment.add" => &["slide"],
        "comment.reply" | "comment.delete" | "comment.resolve" => &["index"],
        "design.background" => &["index"],
        "master.renameLayout" | "master.deleteLayout" => &["master", "layout"],
        "show.customShow" => &["slides"],
        "insert.hyperlink" => &["slide"],
        "drawing.erase" => &["path", "y"],
        "animation.timing" | "animation.options" | "animation.remove" => &["index"],
        "animation.move" => &["index", "to"],
        "arrange.reorder" => &["index"],
        "insert.picture" | "insert.audio" | "insert.video" => &["data", "path"],
        "shape.fill" => &["picture"],
        "table.insertRowAbove" | "table.insertRowBelow" => &["row"],
        "table.insertColumnLeft" | "table.insertColumnRight" => &["col"],
        _ => &[],
    }
}

/// Whether what command `id` does to the shapes it names does not depend on where they are
/// (kition-sdk `KitCommand::position_independent`, C8-slide-5): their place in their slide's list
/// (the stacking order) or their slide's place in the deck. Each was read: `text.set` replaces the
/// text of the shape (or table cell) its id names, `shape.rename` its name, `shape.altText` its
/// description; none reads a stack index. Drawing geometry and text edits hold their frames as
/// content premises, and table insertion indexes address the table grid, not its stack position.
/// Order/group commands still hold stack positions; positional addresses are held by under().
pub fn position_independent(id: &str) -> bool {
    matches!(
        id,
        "text.set"
            | "shape.rename"
            | "shape.altText"
            | "shape.delete"
            | "shape.adjust"
            | "shape.change"
            | "shape.fill"
            | "shape.line"
            | "shape.effects"
            | "shape.quickStyle"
            | "shape.visible"
            | "shape.lock"
            | "picture.crop"
            | "picture.adjust"
            | "picture.reset"
            | "picture.change"
            | "media.options"
            | "insert.hyperlink"
            | "chart.type"
            | "chart.data"
            | "chart.options"
            | "drawing.delete"
            | "drawing.format"
            | "drawing.chart"
            | "drawing.transform"
            | "drawing.text"
            | "drawing.reconnect"
            | "shape.connect"
            | "table.insertRowAbove"
            | "table.insertRowBelow"
            | "table.insertColumnLeft"
            | "table.insertColumnRight"
            | "slide.delete"
            | "animation.set"
            | "animation.add"
            | "animation.remove"
            | "animation.timing"
            | "animation.options"
            | "animation.painterApply"
    )
}
