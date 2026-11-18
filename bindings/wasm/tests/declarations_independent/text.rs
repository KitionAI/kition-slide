//! Blind handler readings for C8-slide-read: edit, text, format, table and anchor.
//! Frozen before opening the binding's declarations or premise tests. Anchor has no writes.
use super::Row;

// The engine wrapper resolves persistent shape identities globally, scoped shape:S/N on S,
// and numeric/string local ids in the current part. onSlide and call context select that part.
// Formatting's named-selection wrapper accepts ids/id, temporarily selects them, and restores.
const ADDRESS: &str = "explicit ids/id: persistent identity -> owning slide/local id, scoped shape:S/N -> local id on S, or number in current part/onSlide; absent -> selected shapes; view chooses slide/master/layout tree";
const EDIT: &str = "edit_shapes: read target content, materialize missing transform from layout/master placeholder or slide-size fallback, read containing group transforms, apply in slide coordinates; Session::edit refits changed group bounds from all children, reroutes affected attached connectors from endpoint geometry, settles stored units and invalidates dependent autofit";
const FIT: &str = "autofit path: text/body/preset geometry and box dimensions; inherited layout/master placeholders, master/default text styles and theme fonts; fit_text_box writes dimensions for Shape autofit, Session::edit updates Shrink results; East Asian vertical text also reads containing group transforms";
const RUN: &str = "text selection: range or containing word at caret (else paragraph end props), table cell/notes supported; no text selection: all selected shape descendants, selected table cells or chart text; read existing runs/paragraphs and normalize, apply run properties, then fit boxes";
const PARA: &str = "text selection: paragraphs touched by caret/range, table cell/notes supported; else all paragraphs of selected shape descendants or selected table cells; read existing paragraphs and normalize; apply paragraph properties, then fit boxes";
const BODY: &str = "edited non-notes shape or selected shape descendants: read existing body, apply body properties and fit boxes; text.cell does not redirect body formatting into the table cell";
const CURRENT_RUN: &str = "toggle/default path: first selected shape's first run or first selected text run, resolved through placeholder/list/master/default text styles; one effective value supplies all targets (explicit on still evaluates current_run)";
const CURRENT_PARA: &str = "toggle path: paragraph at text caret, or first paragraph of first selected shape, resolved through placeholder/master/default paragraph styles; one value supplies all targets (explicit on still evaluates current_para)";
const TABLE: &str = "id or edited-cell table / selected-cells table / first selected shape in current part; local row/column/cell indices, merged-cell ownership; with_table reads grid, normalizes every cell span/row length, writes table and transform width/height, clamps edited/selected cells; edit_shapes inheritance/group path also runs";

macro_rules! row {
    ($id:literal, $target:expr, $objects:expr, $places:expr, $reads:expr, $independent:expr, [$($path:expr),+ $(,)?]) => {
        Row { id: $id, targets: $target, objects: $objects, places: $places,
            reads: $reads, position_independent: $independent,
            paths: &[$($path),+], blind: true }
    };
}

macro_rules! reviewed {
    ($($args:tt)*) => { Row { blind: false, ..row!($($args)*) } };
}

pub(super) const ROWS: &[Row] = &[
    reviewed!(
        // C8-slide-read-4 RIGHT: cmd/edit.rs:42-84 stores/restores/follows selection and part; no target parameters.
        "edit.undo",
        None,
        &[],
        &[],
        &["selection", "part"],
        false,
        [
            "Metadata pinned from the declaration after review: undo/redo follow and clamp selection; text.set holds positional addresses in under and ignores sibling shifts for persistent ids while holding geometry as content",
            "scope or current actor: restore latest matching history entry/replica transactions; read history and replica, whole current doc for redo and identity floor, saved snapshot; writes document/selection/history/revision; no target params or caller part determines restored document"
        ]
    ),
    reviewed!(
        "edit.redo",
        None,
        &[],
        &[],
        &["selection", "part"],
        false,
        [
            "Metadata pinned from the declaration after review: undo/redo follow and clamp selection; text.set holds positional addresses in under and ignores sibling shifts for persistent ids while holding geometry as content",
            "scope or current actor: restore latest matching redo entry/replica transactions; read history/current document/identity floor/saved snapshot; writes document/selection/history/revision"
        ]
    ),
    row!(
        "edit.cut",
        Some("ids"),
        &["id"],
        &[],
        &["selection", "part"],
        false,
        [
            ADDRESS,
            "text selection without explicit targets: copy selected text/body formatting, then forward-delete selection; explicit id/ids temporarily replaces text and slide selection so the named shapes are copied and removed (C8 fixed path); writes clipboard and text/autofit",
            "scope slides or implicit selected-slides scope without explicit shape targets: copy selected/current slide and its referenced media, then delete slides",
            "shape path: copy selected shapes and inherited boxes/referenced media; delete explicit ids/id or selection plus animations/empty groups; reads slide/layout/master/media and selection",
            EDIT,
            FIT
        ]
    ),
    row!(
        "edit.copy",
        None,
        &[],
        &[],
        &["selection", "part"],
        false,
        [
            "text range: read selected shape/table cell/notes text and formatting; write clipboard body/plain text",
            "slides scope explicit or implicit: read selected/current slides, shapes and their referenced media; write clipboard slides/media/plain text",
            "shapes scope: read selected shapes, inherited placeholder bounds and referenced media; write clipboard shapes/media/plain text; ignores ids/id params"
        ]
    ),
    row!(
        "edit.paste",
        None,
        &[],
        &[],
        &["selection", "part"],
        false,
        [
            "text differs from clipboard plain: insert into selected text or create centered textBox using slide size",
            "editing text: insert clipboard body/plain into selected shape/cell/notes, replacing range with surrounding formatting; read clipboard and target text; write text/caret/autofit",
            "clipboard slides: insert after current slide; read slide list, referenced media ids/bytes, layouts and fallback first master/layout; assign fresh ids and connector refs, write slides/media/selection",
            "clipboard shapes: read clipboard/paste count/preference offset, existence of clipboard local ids on destination (same-slide offset test), destination view/shape tree/media; append fresh ids, clear placeholder status on slides; write shapes/media/selection",
            "clipboard plain only: text insert or centered new text box; empty clipboard -> no write",
            FIT
        ]
    ),
    row!(
        "edit.pasteText",
        None,
        &[],
        &[],
        &["selection", "part"],
        false,
        [
            "explicit text else clipboard.plain: selected text/cell/notes -> insert preserving destination props; else new centered text box reads slide size; write text/autofit or append shape",
            FIT
        ]
    ),
    row!(
        "edit.duplicate",
        Some("ids"),
        &["id"],
        &[],
        &["selection", "part"],
        false,
        [
            ADDRESS,
            "selected/explicit local ids: copies only matching top-level shapes from current tree (nested ids produce no copies); read target content, inherited bounds, sibling order, duplicate_offset preference; fresh ids including connector endpoints; clear ph on slides, offset boxes and name copies, append/select copies",
            EDIT
        ]
    ),
    row!(
        "edit.delete",
        Some("ids"),
        &["id"],
        &[],
        &["selection", "part"],
        false,
        [
            ADDRESS,
            "text selection and ids/id absent: delete forward from edited text; explicit id/ids choose the shape path (C8 fixed path); read range/text/paragraph level; write text/caret/autofit",
            "selected table cells and ids/id absent: table.clearCells reads merged-cell ownership/text, clears selected cells preserving formatting",
            "slides scope and ids absent: slide.delete uses selected/current slide; reads/writes sections/shows and slide order",
            "otherwise ids/id/selection: recursively remove targets and empty groups, remove their animations; reads shape/group tree and animation targets, writes tree/animations/selection",
            FIT
        ]
    ),
    row!(
        "edit.selectAll",
        None,
        &[],
        &[],
        &["selection", "part"],
        false,
        [
            "editing text -> select entire edited body (notes/cell supported); else top-level visible/unlocked current-part shapes -> selection; read text length or hidden/locked flags"
        ]
    ),
    row!(
        "edit.deselect",
        None,
        &[],
        &[],
        &["selection", "part"],
        false,
        [
            "editing text -> exit and select its shape except notes; else clear shapes/cells; clear format/animation painter; selection clamp resolves remaining ids in current part"
        ]
    ),
    row!(
        "edit.select",
        Some("ids"),
        &["id"],
        &[],
        &["selection", "part"],
        false,
        [
            ADDRESS,
            "explicit ids/id else empty: validate ids in current part; add/toggle reads existing selection, replace clears it; writes selection/text/cells only"
        ]
    ),
    row!(
        "text.edit",
        Some("id"),
        &[],
        &[],
        &["selection", "part"],
        false,
        [
            ADDRESS,
            "notes true: current slide notes length -> caret at end, clear shape selection; id ignored",
            "table: cell param else selected range for that table else (0,0); resolve merged owner and text; at else body end -> caret; shape/connector: read text or create missing TextBody (stored write); writes selection/body"
        ]
    ),
    row!(
        "text.exit",
        None,
        &[],
        &[],
        &["selection", "part"],
        false,
        ["edited text: take selection.text and select its shape except notes; selection clamp resolves ids in current part; writes selection only"]
    ),
    row!(
        "text.insert",
        None,
        &[],
        &[],
        &["selection", "part"],
        false,
        [
            "selected text body/cell/notes and ordered range; read previous character for smart quotes, surrounding run/link props, paragraph levels/styles for new lines, autocorrect word and preference flags; replace range, insert and update caret",
            FIT
        ]
    ),
    row!(
        "text.delete",
        None,
        &[],
        &[],
        &["selection", "part"],
        false,
        [
            "selected text body/cell/notes: delete range else character/word by dir; backward at indented paragraph start outdents; read character bounds and paragraph/run formatting, preserve joined text and set caret",
            FIT
        ]
    ),
    row!(
        "text.move",
        None,
        &[],
        &[],
        &["selection", "part"],
        false,
        [
            "edited body/cell/notes and caret/range: char/word/paragraph endpoints, up/down/line bounds use laid-out text; layout is evaluated for all directions; read box/geometry/theme/fonts/inheritance and containing group transform (table row heights and cell metrics); writes caret/range"
        ]
    ),
    row!(
        "text.select",
        None,
        &[],
        &[],
        &["selection", "part"],
        false,
        [
            "existing text selection: supplied paragraph/character anchor/caret replace range; enabled checks editing; clamp resolves shape in current part; writes selection only"
        ]
    ),
    row!(
        "text.selectAll",
        None,
        &[],
        &[],
        &["selection", "part"],
        false,
        ["edited body/cell/notes -> read paragraph and character length, set range start..end"]
    ),
    row!(
        "text.selectWord",
        None,
        &[],
        &[],
        &["selection", "part"],
        false,
        ["at paragraph/character else caret in edited body/cell/notes -> read word character classes, set word range"]
    ),
    row!(
        "text.selectParagraph",
        None,
        &[],
        &[],
        &["selection", "part"],
        false,
        ["at paragraph else caret -> read edited body/cell/notes paragraph length, set whole paragraph range"]
    ),
    reviewed!(
        // C8-slide-read-4 RIGHT: cmd/text.rs:559-565 resolves id or selection in the part; target stack position is irrelevant.
        "text.set",
        Some("id"),
        &[],
        &[],
        &["selection", "part"],
        true,
        [
            "Metadata pinned from the declaration after review: undo/redo follow and clamp selection; text.set holds positional addresses in under and ignores sibling shifts for persistent ids while holding geometry as content",
            ADDRESS,
            "shape or table cell by local row/column -> merged owner; replace paragraphs preserving first run props and each old paragraph's level/props unless leading tabs override level; read existing body and table ownership; group/picture/connector/chart rejected",
            EDIT,
            FIT
        ]
    ),
    row!(
        "format.bold",
        Some("ids"),
        &["id"],
        &[],
        &["selection", "part"],
        false,
        [ADDRESS, CURRENT_RUN, RUN, EDIT, FIT, "set bold from explicit on or inverted effective bold"]
    ),
    row!(
        "format.italic",
        Some("ids"),
        &["id"],
        &[],
        &["selection", "part"],
        false,
        [ADDRESS, CURRENT_RUN, RUN, EDIT, FIT, "set italic from explicit on or inverted effective italic"]
    ),
    row!(
        "format.underline",
        Some("ids"),
        &["id"],
        &[],
        &["selection", "part"],
        false,
        [ADDRESS, CURRENT_RUN, RUN, EDIT, FIT, "set underline style or none from explicit on/effective underline"]
    ),
    row!(
        "format.strikethrough",
        Some("ids"),
        &["id"],
        &[],
        &["selection", "part"],
        false,
        [ADDRESS, CURRENT_RUN, RUN, EDIT, FIT, "set strike none/single/double from explicit on/effective strike"]
    ),
    row!(
        "format.superscript",
        Some("ids"),
        &["id"],
        &[],
        &["selection", "part"],
        false,
        [ADDRESS, CURRENT_RUN, RUN, EDIT, FIT, "set baseline 0.3 or 0 from explicit on/effective baseline"]
    ),
    row!(
        "format.subscript",
        Some("ids"),
        &["id"],
        &[],
        &["selection", "part"],
        false,
        [ADDRESS, CURRENT_RUN, RUN, EDIT, FIT, "set baseline -0.25 or 0 from explicit on/effective baseline"]
    ),
    row!(
        "format.font",
        Some("ids"),
        &["id"],
        &[],
        &["selection", "part"],
        false,
        [
            ADDRESS,
            RUN,
            EDIT,
            FIT,
            "family string; read installed font DB and substitution lists for result note, infer East Asian family and set font/font_ea"
        ]
    ),
    row!(
        "format.size",
        Some("ids"),
        &["id"],
        &[],
        &["selection", "part"],
        false,
        [ADDRESS, RUN, EDIT, FIT, "size scalar clamps 1..4000 -> run size"]
    ),
    row!(
        "format.grow",
        Some("ids"),
        &["id"],
        &[],
        &["selection", "part"],
        false,
        [ADDRESS, CURRENT_RUN, RUN, EDIT, FIT, "each run size steps up; missing size uses effective first selected run size (18 fallback)"]
    ),
    row!(
        "format.shrink",
        Some("ids"),
        &["id"],
        &[],
        &["selection", "part"],
        false,
        [ADDRESS, CURRENT_RUN, RUN, EDIT, FIT, "each run size steps down; missing size uses effective first selected run size (18 fallback)"]
    ),
    row!(
        "format.color",
        Some("ids"),
        &["id"],
        &[],
        &["selection", "part"],
        false,
        [ADDRESS, RUN, EDIT, FIT, "store parsed RGB/scheme reference as solid run fill; theme color is symbolic, not eagerly sampled"]
    ),
    row!(
        "format.highlight",
        Some("ids"),
        &["id"],
        &[],
        &["selection", "part"],
        false,
        [ADDRESS, RUN, EDIT, FIT, "store parsed highlight color reference or remove"]
    ),
    row!(
        "format.case",
        Some("ids"),
        &["id"],
        &[],
        &["selection", "part"],
        false,
        [
            ADDRESS,
            "text range: change case in selected run portions, reads existing text/RunKind; collapsed caret no-op, then fit; whole shapes: edit_shapes changes ordinary text runs only, no group recursion or explicit fit_text_box",
            EDIT,
            FIT
        ]
    ),
    row!("format.caps", Some("ids"), &["id"], &[], &["selection", "part"], false, [ADDRESS, RUN, EDIT, FIT, "set run caps enum"]),
    row!("format.spacing", Some("ids"), &["id"], &[], &["selection", "part"], false, [ADDRESS, RUN, EDIT, FIT, "set run spacing scalar"]),
    row!(
        "format.clear",
        Some("ids"),
        &["id"],
        &[],
        &["selection", "part"],
        false,
        [ADDRESS, RUN, EDIT, FIT, "replace run props preserving each run's language and link"]
    ),
    row!(
        "format.textShadow",
        Some("ids"),
        &["id"],
        &[],
        &["selection", "part"],
        false,
        [ADDRESS, CURRENT_RUN, RUN, EDIT, FIT, "explicit on or toggle effective first-run shadow -> fixed shadow or remove"]
    ),
    row!(
        "format.textOutline",
        Some("ids"),
        &["id"],
        &[],
        &["selection", "part"],
        false,
        [ADDRESS, RUN, EDIT, FIT, "set outline from supplied color/width or remove"]
    ),
    row!(
        "format.transform",
        Some("ids"),
        &["id"],
        &[],
        &["selection", "part"],
        false,
        [ADDRESS, BODY, EDIT, FIT, "warp preset string or none; clear prior adjustments iff preset changes"]
    ),
    row!("format.align", Some("ids"), &["id"], &[], &["selection", "part"], false, [ADDRESS, PARA, EDIT, FIT, "set supplied paragraph align enum"]),
    row!(
        "format.rtl",
        Some("ids"),
        &["id"],
        &[],
        &["selection", "part"],
        false,
        [
            ADDRESS,
            CURRENT_PARA,
            PARA,
            EDIT,
            FIT,
            "explicit on else invert effective first paragraph rtl; for each paragraph mirror stored left/right alignment when its stored rtl differs"
        ]
    ),
    row!("format.alignLeft", Some("ids"), &["id"], &[], &["selection", "part"], false, [ADDRESS, PARA, EDIT, FIT, "set left alignment"]),
    row!("format.alignCenter", Some("ids"), &["id"], &[], &["selection", "part"], false, [ADDRESS, PARA, EDIT, FIT, "set center alignment"]),
    row!("format.alignRight", Some("ids"), &["id"], &[], &["selection", "part"], false, [ADDRESS, PARA, EDIT, FIT, "set right alignment"]),
    row!("format.justify", Some("ids"), &["id"], &[], &["selection", "part"], false, [ADDRESS, PARA, EDIT, FIT, "set justified alignment"]),
    row!(
        "format.bullets",
        Some("ids"),
        &["id"],
        &[],
        &["selection", "part"],
        false,
        [
            ADDRESS,
            CURRENT_PARA,
            PARA,
            EDIT,
            FIT,
            "explicit on else invert effective first paragraph bullet; read each paragraph level/indents/bullet_font; set char/color/size and default indents conditionally"
        ]
    ),
    row!(
        "format.numbering",
        Some("ids"),
        &["id"],
        &[],
        &["selection", "part"],
        false,
        [
            ADDRESS,
            CURRENT_PARA,
            PARA,
            EDIT,
            FIT,
            "explicit on else invert effective first paragraph autonum; set scheme/start and indents from each paragraph level"
        ]
    ),
    row!(
        "format.indent",
        Some("ids"),
        &["id"],
        &[],
        &["selection", "part"],
        false,
        [ADDRESS, PARA, EDIT, FIT, "increment each paragraph level, shift existing explicit left margin"]
    ),
    row!(
        "format.outdent",
        Some("ids"),
        &["id"],
        &[],
        &["selection", "part"],
        false,
        [ADDRESS, PARA, EDIT, FIT, "decrement each paragraph level, shift existing explicit left margin"]
    ),
    row!(
        "format.lineSpacing",
        Some("ids"),
        &["id"],
        &[],
        &["selection", "part"],
        false,
        [ADDRESS, PARA, EDIT, FIT, "set paragraph line spacing from explicit pt else line multiplier"]
    ),
    row!(
        "format.paragraph",
        Some("ids"),
        &["id"],
        &[],
        &["selection", "part"],
        false,
        [ADDRESS, PARA, EDIT, FIT, "set supplied alignment/indents/space before-after/line spacing; unspecified props retained"]
    ),
    row!("format.anchor", Some("ids"), &["id"], &[], &["selection", "part"], false, [ADDRESS, BODY, EDIT, FIT, "set body vertical anchor"]),
    row!("format.direction", Some("ids"), &["id"], &[], &["selection", "part"], false, [ADDRESS, BODY, EDIT, FIT, "set text direction enum"]),
    row!(
        "format.columns",
        Some("ids"),
        &["id"],
        &[],
        &["selection", "part"],
        false,
        [ADDRESS, BODY, EDIT, FIT, "set column count and spacing scalars"]
    ),
    row!(
        "format.autofit",
        Some("ids"),
        &["id"],
        &[],
        &["selection", "part"],
        false,
        [ADDRESS, BODY, EDIT, FIT, "set body autofit none/shrink/shape; then calculate applicable fit"]
    ),
    row!("format.wrap", Some("ids"), &["id"], &[], &["selection", "part"], false, [ADDRESS, BODY, EDIT, FIT, "set wrap boolean, then fit"]),
    row!(
        "format.margins",
        Some("ids"),
        &["id"],
        &[],
        &["selection", "part"],
        false,
        [ADDRESS, BODY, EDIT, FIT, "set supplied inset scalars, retaining others, then fit"]
    ),
    row!(
        "format.painter",
        None,
        &[],
        &[],
        &["selection", "part"],
        false,
        [
            "selected text: copy explicit run props at first range character and paragraph props; else first selected shape fill/line/effects/style/first run; no inherited resolution; writes session painter plus sticky flag, ignores ids/id"
        ]
    ),
    row!(
        "format.painterApply",
        Some("ids"),
        &["id"],
        &[],
        &["selection", "part"],
        false,
        [
            ADDRESS,
            "session painter required; clear if not sticky; selected text range and ids/id absent -> apply stored run/paragraph props through format_run/format_para; explicit id/ids override that range (C8 fixed path); else edit explicit/selected shapes' fill/line/effects/style and all existing text props; read painter state and target contents",
            EDIT,
            FIT
        ]
    ),
    row!(
        "table.cellFormat",
        Some("id"),
        &[],
        &[],
        &["selection", "part"],
        false,
        [
            TABLE,
            "cells list else selected cells/edited cell else whole table -> merged owners; set anchor/margins; border range uses from/to else matching selected range else whole; expand across merges and mirror shared edges into neighbours"
        ]
    ),
    row!(
        "table.border",
        Some("id"),
        &[],
        &[],
        &["selection", "part"],
        false,
        [
            TABLE,
            "from/to local indices else matching selected range/edited cell else whole table; expand through merged cells, read spans/owners and mirror edges into neighbours; write selected border slots (scheme Tx1 default remains symbolic)"
        ]
    ),
    row!(
        "table.clearCells",
        Some("id"),
        &[],
        &[],
        &["selection", "part"],
        false,
        [
            TABLE,
            "cells list else matching selected range/edited cell else whole table; merged owner resolution, delete cell text preserving formatting"
        ]
    ),
    reviewed!(
        // C8-slide-read-4 REPAIR: cmd/table.rs:105-155,159-171 uses table-local row and edited part/selection; independent false -> true.
        "table.insertRowAbove",
        Some("id"),
        &[],
        &["row"],
        &["selection", "part"],
        true,
        [
            TABLE,
            "Place metadata pinned from the declaration after review: row is a table-local insertion place, not an object id",
            "row param else matching edited/selected cell row else 0; col likewise validated although unused; insert at row, copy preceding row height (row 0 copies old first height), remap merged regions/promote owner content"
        ]
    ),
    reviewed!(
        // C8-slide-read-4 REPAIR: cmd/table.rs:105-155,159-171 uses table-local row and edited part/selection; independent false -> true.
        "table.insertRowBelow",
        Some("id"),
        &[],
        &["row"],
        &["selection", "part"],
        true,
        [
            TABLE,
            "Place metadata pinned from the declaration after review: row is a table-local insertion place, not an object id",
            "row param else matching edited/selected cell row else 0; col likewise validated although unused; insert at row+1, copy prior row height, remap merged regions/promote owner content"
        ]
    ),
    reviewed!(
        "table.insertColumnLeft",
        Some("id"),
        &[],
        &["col"],
        &["selection", "part"],
        true,
        [
            TABLE,
            "Place metadata pinned from the declaration after review: col is a table-local insertion place, not an object id",
            "col param else matching edited/selected cell col else 0; row likewise validated although unused; insert at col, copy preceding width (col 0 copies old first), remap merged regions"
        ]
    ),
    reviewed!(
        // C8-slide-read-4 RIGHT: cmd/table.rs:105-155,159-173 uses table-local col and edited part/selection, not target stack.
        "table.insertColumnRight",
        Some("id"),
        &[],
        &["col"],
        &["selection", "part"],
        true,
        [
            TABLE,
            "Place metadata pinned from the declaration after review: col is a table-local insertion place, not an object id",
            "col param else matching edited/selected cell col else 0; row likewise validated although unused; insert at col+1, copy prior width, remap merged regions"
        ]
    ),
    row!(
        "table.deleteRow",
        Some("id"),
        &[],
        &[],
        &["selection", "part"],
        false,
        [
            TABLE,
            "row param else matching edited/selected cell row else 0; col likewise validated although unused; reject last row; delete row, remap all merged regions, promote deleted owner content/formatting"
        ]
    ),
    row!(
        "table.deleteColumn",
        Some("id"),
        &[],
        &[],
        &["selection", "part"],
        false,
        [
            TABLE,
            "col param else matching edited/selected cell col else 0; row likewise validated although unused; reject last column; delete column, remap all merged regions, promote owner content"
        ]
    ),
    row!(
        "table.merge",
        Some("id"),
        &[],
        &[],
        &["selection", "part"],
        false,
        [
            TABLE,
            "from/to else selection.cells coordinates only when its table matches explicit id (C8 fixed path); read overlapping merged regions and all cell paragraphs in range; reject partial overlap; move text to top-left and rewrite spans/covered flags"
        ]
    ),
    row!(
        "table.split",
        Some("id"),
        &[],
        &[],
        &["selection", "part"],
        false,
        [TABLE, "cell param else matching edited/selected row/column else (0,0); resolve owner, read spans, clear merge metadata across region"]
    ),
    row!(
        "table.style",
        Some("id"),
        &[],
        &[],
        &["selection", "part"],
        false,
        [TABLE, "style is stored string value; no style-object/name resolution; set table.style"]
    ),
    row!(
        "table.options",
        Some("id"),
        &[],
        &[],
        &["selection", "part"],
        false,
        [TABLE, "set supplied table style-option booleans; retain unspecified flags"]
    ),
    row!(
        "table.cellFill",
        Some("id"),
        &[],
        &[],
        &["selection", "part"],
        false,
        [
            TABLE,
            "cells list else matching selected range/edited cell else whole table -> merged owners; set fill reference/none (no eager theme color sampling)"
        ]
    ),
    row!(
        "table.distributeRows",
        Some("id"),
        &[],
        &[],
        &["selection", "part"],
        false,
        [TABLE, "read all stored row heights/count, replace each with average"]
    ),
    row!(
        "table.distributeColumns",
        Some("id"),
        &[],
        &[],
        &["selection", "part"],
        false,
        [TABLE, "read all stored column widths/count, replace each with average"]
    ),
    row!(
        "table.columnWidth",
        Some("id"),
        &[],
        &[],
        &["selection", "part"],
        false,
        [TABLE, "col param else matching edited/selected column else 0; replace stored column width, aggregate table box"]
    ),
    row!(
        "table.rowHeight",
        Some("id"),
        &[],
        &[],
        &["selection", "part"],
        false,
        [TABLE, "row param else matching edited/selected row else 0; replace stored row height, aggregate table box"]
    ),
    row!(
        "table.selectCells",
        Some("id"),
        &[],
        &[],
        &["selection", "part"],
        false,
        [
            "id else first selected shape; current part table; from defaults (0,0), to defaults from; read grid bounds, then write cell/shape selection and exit text; no table edit"
        ]
    ),
];

#[test]
fn text_replacement_holds_typography_and_geometry_without_holding_fill() {
    use kition_kit::KitDocument;
    use kition_slide_kit::SlideDocument;
    use serde_json::json;

    let mut doc = SlideDocument::create("Reading.pptx", 960, 540).unwrap();
    let inserted = doc.execute("insert.textBox", json!({"text": "Before", "rect": [30, 40, 220, 80]})).unwrap();
    let id = doc.execute("shape.inspect", json!({"id": inserted["id"]})).unwrap()["shape"]["toolId"].clone();
    doc.version().unwrap();
    let under = doc.under("text.set", &json!({"id": id, "text": "After"})).unwrap().unwrap();
    let target = under.premises.iter().find(|premise| premise.id == id).unwrap();
    let held = target.fields.as_ref().unwrap();
    for path in ["body.paragraphs", "body.props", "xfrm", "look.geom"] {
        assert!(held.iter().any(|held| held == path), "missing {path}: {target:?}");
    }
    assert!(
        !held.iter().any(|held| matches!(held.as_str(), "look" | "look.fill" | "name")),
        "text replacement does not read the target's fill or name: {target:?}"
    );
    assert!(under.premises.iter().any(
        |premise| premise.id == "slide-file" && premise.fields.as_ref().is_some_and(|paths| paths.iter().any(|path| path == "defaultTextStyle"))
    ));
    assert!(
        under.premises.iter().any(|premise| premise.fields.as_ref().is_some_and(|paths| paths.iter().any(|path| path == "theme.fonts"))),
        "shape autofit reads theme fonts: {under:?}"
    );
}

#[test]
fn selected_text_replacement_holds_the_selected_shape() {
    use kition_kit::KitDocument;
    use kition_slide_kit::SlideDocument;
    use serde_json::json;

    let mut doc = SlideDocument::create("Reading.pptx", 960, 540).unwrap();
    let inserted = doc.execute("insert.textBox", json!({"text": "Before"})).unwrap();
    let id = doc.execute("shape.inspect", json!({"id": inserted["id"]})).unwrap()["shape"]["toolId"].clone();
    doc.execute("edit.select", json!({"ids": [id]})).unwrap();
    doc.version().unwrap();
    let under = doc.under("text.set", &json!({"text": "After"})).unwrap().unwrap();
    assert!(
        under.premises.iter().any(|premise| premise.id == id
            && premise
                .fields
                .as_ref()
                .is_some_and(|paths| ["body.paragraphs", "body.props"].iter().all(|body| paths.iter().any(|path| path == body)))),
        "selection is a target-content premise: {under:?}"
    );
}

#[test]
fn nested_text_holds_the_ancestor_frame_without_the_ancestor_name() {
    use kition_kit::KitDocument;
    use kition_slide_kit::SlideDocument;
    use serde_json::json;

    let mut doc = SlideDocument::create("Reading.pptx", 960, 540).unwrap();
    let child = doc.execute("insert.textBox", json!({"text": "Before", "rect": [30, 40, 220, 80]})).unwrap()["id"].clone();
    let neighbour = doc.execute("shape.insert", json!({"preset": "rect", "rect": [320, 40, 80, 80]})).unwrap()["id"].clone();
    let group = doc.execute("arrange.group", json!({"ids": [child, neighbour]})).unwrap()["id"].clone();
    let group = doc.execute("shape.inspect", json!({"id": group})).unwrap()["shape"]["toolId"].clone();
    let child = doc.execute("shape.inspect", json!({"id": child})).unwrap()["shape"]["toolId"].clone();
    doc.version().unwrap();
    let under = doc.under("text.set", &json!({"id": child, "text": "After"})).unwrap().unwrap();
    let parent = under.premises.iter().find(|premise| premise.id == group).unwrap();
    let fields = parent.fields.as_ref().unwrap();
    assert!(fields.iter().any(|field| field == "xfrm"), "{parent:?}");
    assert!(fields.iter().any(|field| field == "look.kind.child"), "{parent:?}");
    assert!(!fields.iter().any(|field| field == "name"), "{parent:?}");
}

#[test]
fn persistent_text_target_switches_from_master_view_to_its_slide() {
    use kition_kit::KitDocument;
    use kition_slide_kit::SlideDocument;
    use serde_json::json;

    let mut doc = SlideDocument::create("Reading.pptx", 960, 540).unwrap();
    let inserted = doc.execute("insert.textBox", json!({"text": "Before"})).unwrap();
    let id = doc.execute("shape.inspect", json!({"id": inserted["id"]})).unwrap()["shape"]["toolId"].clone();
    doc.execute("view.slideMaster", json!({})).unwrap();
    doc.version().unwrap();
    let under = doc.under("text.set", &json!({"id": id, "text": "After"})).unwrap().unwrap();
    assert!(
        under.premises.iter().any(|premise| {
            premise.id == id
                && premise
                    .fields
                    .as_ref()
                    .is_some_and(|fields| ["body.paragraphs", "body.props"].iter().all(|body| fields.iter().any(|field| field == body)))
        }),
        "the persistent target belongs to the slide even while the editor shows its master: {under:?}"
    );
}
