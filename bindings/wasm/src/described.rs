//! What each command does and what each read command answers, for the model choosing a command
//! (`KitCommand.description`, `output_schema`; kition-sdk `testing::check_commands`).
use serde_json::Value;

/// Read commands that answer nothing a model reads.
pub const NO_OUTPUT: &[(&str, &str)] = &[
    ("window.next", "changes the editor's selection, mode or view and answers nothing to read"),
    ("edit.deselectAll", "changes the editor's selection, mode or view and answers nothing to read"),
    ("text.exit", "changes the editor's selection, mode or view and answers nothing to read"),
    ("text.move", "changes the editor's selection, mode or view and answers nothing to read"),
    ("text.select", "changes the editor's selection, mode or view and answers nothing to read"),
    ("text.selectAll", "changes the editor's selection, mode or view and answers nothing to read"),
    ("text.selectWord", "changes the editor's selection, mode or view and answers nothing to read"),
    ("text.selectParagraph", "changes the editor's selection, mode or view and answers nothing to read"),
    ("view.closeMaster", "changes the editor's selection, mode or view and answers nothing to read"),
    ("animation.painter", "changes the editor's selection, mode or view and answers nothing to read"),
    ("table.selectCells", "changes the editor's selection, mode or view and answers nothing to read"),
    ("media.play", "changes the editor's selection, mode or view and answers nothing to read"),
    ("media.pause", "changes the editor's selection, mode or view and answers nothing to read"),
    ("media.toggle", "changes the editor's selection, mode or view and answers nothing to read"),
    ("media.stop", "changes the editor's selection, mode or view and answers nothing to read"),
    ("media.seek", "changes the editor's selection, mode or view and answers nothing to read"),
    ("file.export", "writes the file it names and answers nothing to read"),
];

pub fn describe(id: &str) -> &'static str {
    match id {
        "file.new" => "Starts a new presentation, from a theme and slide size or blank, replacing the open one in this session.",
        "file.open" => "Opens a .kslide or .pptx file from the workspace as the presentation in this session.",
        "file.openBytes" => "Opens a presentation from base64 `data` named `name` (.kslide or .pptx) instead of a file path.",
        "file.save" => "Saves the presentation to its file, or to `path` in `format` (kslide or pptx).",
        "file.saveAs" => "Saves the presentation to a new file `path` (kslide or pptx by extension) and makes it the file being edited.",
        "file.saveTemplate" => "Saves the presentation's theme, masters and layouts as a template file at `path`.",
        "file.saveBytes" => "Answers the presentation encoded as kslide or pptx (base64) without writing a file.",
        "file.export" => {
            "Writes the slides as PNG/JPEG pictures, a PDF, an outline, or a pptx/kslide copy to `path`, without changing the presentation."
        }
        "file.exportBytes" => {
            "Answers the slides as a PDF, or one slide (or with `all` every slide) as a PNG/JPEG picture, in base64 `data`, without writing a file or changing the presentation."
        }
        "file.render" => {
            "Answers one slide as a PNG (base64) with its size, the agent's preview: `target.slide` (from 1) or `target.slideId`, or `slide` (an index from 0), else the first slide, never the current one; `scale` pixels per point (default 1, at most 16), brought down so the longest side is at most `maxSize` pixels; date fields show 2000-01-01, so the same call answers the same bytes. `edit: true` draws placeholder prompts as the editor does."
        }
        "file.close" => "Closes the presentation in this session.",
        "file.properties" => "Sets the presentation's document properties (title, author, keywords, …) when given, and answers them.",
        "file.recovery.save" => "Writes an AutoRecover copy of the presentation to the host's recovery folder now.",
        "file.recovery.list" => "Lists the AutoRecover copies in the host's recovery folder (uid, path, title, saved time).",
        "file.recovery.open" => "Opens every AutoRecover copy as a recovered presentation.",
        "file.recovery.discard" => {
            "Deletes the AutoRecover copy `uid` names (the number `file.recovery.list` answers), or every copy when `uid` is left out; a `uid` that names no copy is refused and deletes nothing."
        }
        "file.revert" => "Throws away unsaved changes and reloads the presentation from its file.",
        "window.next" => "Switches the editor to the next open presentation window (or window `index`).",
        "edit.undo" => {
            "Takes back the caller's last edit (the person's, or the agent's with `scope: \"agent\"`), leaving the other's edits in place."
        }
        "edit.redo" => "Applies again the edit the caller's last undo took back, in the same scope as `edit.undo`.",
        "edit.cut" => "Removes the selected shapes (or with `scope: \"slides\"` the selected slides) and puts them on the clipboard.",
        "edit.copy" => "Copies the selected shapes or slides to the clipboard and answers their plain text.",
        "edit.paste" => "Pastes the clipboard (or `text` as plain text) onto the current slide, or as slides with `scope: \"slides\"`.",
        "edit.pasteText" => "Pastes `text` (or the clipboard) taking the destination's formatting instead of the source's.",
        "edit.duplicate" => {
            "Duplicates shapes in place, slightly offset. Applies to the shapes in `ids` (on slide `onSlide`, from 1) or else the selection."
        }
        "edit.delete" => {
            "Deletes shapes, or the selected slides with `scope: \"slides\"`. Applies to the shapes in `ids` (on slide `onSlide`, from 1) or else the selection; with nothing selected it deletes nothing."
        }
        "edit.selectAll" => "Selects every shape on the current slide and answers their ids.",
        "edit.deselectAll" => "Clears the selection on the current slide.",
        "edit.select" => {
            "Selects the shapes in `ids` (on slide `onSlide`, from 1) for selection-based commands; `add`/`toggle` change the selection instead of replacing it. Answers the selection."
        }
        "slide.new" => {
            "Adds a slide after the current one with a layout (by name or kind such as title, titleAndContent, sectionHeader) and makes it current."
        }
        "slide.duplicate" => "Duplicates slide `index` (from 0, else the current one) right after it.",
        "slide.delete" => {
            "Deletes slide `index` (from 0, else the current one), or the slides in `ids`. To delete several slides use `ids`: indexes shift after each deletion."
        }
        "slide.move" => "Moves slide `from` (from 0, else the current one) to position `to`.",
        "slide.go" => "Makes slide `index` (from 0) or slide id `id` the current slide, which selection-based commands act on.",
        "slide.next" => "Makes the next slide current and answers its index.",
        "slide.previous" => "Makes the previous slide current and answers its index.",
        "slide.first" => "Makes the first slide current and answers its index (0).",
        "slide.last" => "Makes the last slide current and answers its index.",
        "slide.selectSlides" => {
            "Selects slides by id or index (from 0) in the slide panel, for slide-scope commands such as `edit.cut {scope: \"slides\"}`. Answers the selection."
        }
        "slide.hide" => "Hides slide `index` (from 0, else the current one) from the slide show, or shows it again with `hidden: false`.",
        "slide.layout" => "Applies a layout (name or kind) to slide `index` or the current one, keeping placeholder content.",
        "slide.reset" => "Resets the slide's placeholders to their layout's position, size and formatting.",
        "slide.notes" => "Sets the speaker notes of slide `index` (from 0, else the current one) to `text`.",
        "slide.rename" => "Sets the name of slide `index` (from 0, else the current one).",
        "section.add" => "Adds a section starting at slide `at` (from 0, else the current slide), optionally named.",
        "section.rename" => "Renames section `index` (from 0).",
        "section.remove" => "Removes section `index`, keeping its slides unless `slides: true`.",
        "section.removeAll" => "Removes every section; the slides stay.",
        "section.move" => "Moves section `index` with its slides to section position `to`.",
        "slide.fromOutline" => "Adds slides from an outline `text`: each top-level line becomes a slide title and indented lines its bullets.",
        "shape.insert" => {
            "Inserts a preset shape (rect, roundRect, ellipse, rightArrow, star5, …; see `shape.presets`) on the current slide at `rect` [x, y, w, h] in points, with optional text, and selects it."
        }
        "shape.presets" => "Lists the preset shapes `shape.insert` and `shape.change` take, with label and category.",
        "textBox.insert" => "Inserts a text box at `rect` [x, y, w, h] points on the current slide with optional `text`, and selects it.",
        "image.insert" => "Inserts a picture from a workspace file or base64 `data` on the current slide, at `rect` or fitted to the slide.",
        "table.insert" => "Inserts a table of `rows` × `cols` on the current slide, at `rect` or centered.",
        "chart.insert" => {
            "Inserts a chart (column, bar, line, pie, waterfall, funnel, treemap, combo, …) with categories and series (each with an optional color, combo type and waterfall totals) on the current slide; format.fontSize, format.color, format.bold and format.fontName on it format its text."
        }
        "audio.insert" => "Inserts an audio clip from a workspace file or base64 `data` on the current slide.",
        "video.insert" => "Inserts a video from a workspace file or base64 `data` on the current slide.",
        "artText.insert" => "Inserts art text in one of the gallery styles on the current slide.",
        "field.insertSlideNumber" => {
            "Turns on the slide number placeholder on the current slide, or inserts a slide-number field in the text being edited."
        }
        "field.insertDateTime" => "Inserts the date and time as a field in the text being edited (or the date placeholder), in a `format`.",
        "symbol.insert" => "Inserts `text` (special characters) at the text insertion point.",
        "link.insert" => {
            "Adds a link to a URL or to slide `slide` (from 0) on the selected text or on the shapes in `ids`; with nothing selected inserts `text` as a link."
        }
        "chart.type" => "Changes the chart `id` (else the selected chart) to another chart type, keeping its data; an unknown type is refused.",
        "chart.data" => {
            "Replaces the categories and series values (and any series color, type or waterfall totals given) of chart `id` (else the selected chart)."
        }
        "chart.options" => {
            "Sets which chart elements show (title, legend position, data labels, gridlines), the colour palette (colorful1–4, monochrome1–13; others are refused), series colours, gap width and overlap of the selected or named chart."
        }
        "smartGraphic.insert" => {
            "Inserts a smart graphic (list, process, cycle with arrows, hierarchy with connecting lines, pyramid, matrix) built from `items` in `rect` (else the lower slide) on the current slide, as a group: shape.fill, shape.line, shape.effects and format.* on the group format all its shapes."
        }
        "actionButton.insert" => "Inserts an action button (back, forward, home, …) that moves through the slide show when clicked.",
        "shape.connect" => {
            "Draws a connector between shapes `from` and `to` (at given connection sites or the closest pair) that follows them when they move."
        }
        "shape.freeform" => "Draws a freeform line or closed filled shape through `points` (slide points), optionally smoothed into curves.",
        "shape.sites" => "Answers the connection sites of shape `id` as [x, y] slide points, numbered as `shape.connect` takes them.",
        "shape.move" => {
            "Moves shapes by `dx`/`dy` or to `x`/`y` in slide points (a group member too; its group's box follows). Applies to the shapes in `ids` (on slide `onSlide`, from 1) or else the selection."
        }
        "shape.resize" => {
            "Sets the width and/or height in points, optionally keeping the aspect ratio. Applies to the shapes in `ids` (on slide `onSlide`, from 1) or else the selection."
        }
        "shape.delete" => "Deletes the shapes in `ids` (on slide `onSlide`, from 1).",
        "shape.setBounds" => {
            "Sets position and size exactly (`x`, `y`, `w`, `h` in slide points, as slide.inspect lists boxes, a group member's too). Applies to the shapes in `ids` (on slide `onSlide`, from 1) or else the selection."
        }
        "shape.rotate" => {
            "Rotates to `deg` degrees, or by `by` degrees. Applies to the shapes in `ids` (on slide `onSlide`, from 1) or else the selection."
        }
        "shape.flip" => "Flips horizontally or vertically. Applies to the shapes in `ids` (on slide `onSlide`, from 1) or else the selection.",
        "shape.adjust" => {
            "Sets a preset shape's adjustment handle `index` (corner radius, arrow head size) to `value`. Applies to the shapes in `ids` (on slide `onSlide`, from 1) or else the selection."
        }
        "shape.change" => {
            "Changes the preset geometry (keeping text, fill and size). Applies to the shapes in `ids` (on slide `onSlide`, from 1) or else the selection."
        }
        "shape.fill" => {
            "Sets the fill: a color, none, a gradient, a picture or a pattern. Applies to the shapes in `ids` (on slide `onSlide`, from 1) or else the selection."
        }
        "shape.line" => {
            "Sets the outline: color or none, width in points, dash, caps, joins and arrowheads. Applies to the shapes in `ids` (on slide `onSlide`, from 1) or else the selection."
        }
        "shape.effects" => {
            "Sets shadow, glow, soft edges, reflection, bevel or 3-D rotation effects. The shadow presets are PowerPoint's: `outer` is Offset: Bottom Right (black at 40 %, blurred 4 pt, 3 pt down and right; subtle by design), `bottom` is Offset: Bottom, `center` is Offset: Center; give {color, blur, dist, dir} (points, degrees) for a stronger one. Applies to the shapes in `ids` (on slide `onSlide`, from 1) or else the selection."
        }
        "shape.quickStyle" => {
            "Applies a theme quick style (0..41: style row × accent color). Applies to the shapes in `ids` (on slide `onSlide`, from 1) or else the selection."
        }
        "shape.rename" => "Sets the name of shape `id` (else the selected shape), as shown in the selection pane.",
        "shape.altText" => {
            "Sets alternative text, or marks the shapes decorative. Applies to the shapes in `ids` (on slide `onSlide`, from 1) or else the selection."
        }
        "shape.visible" => "Shows or hides shapes on the slide. Applies to the shapes in `ids` (on slide `onSlide`, from 1) or else the selection.",
        "shape.lock" => {
            "Locks shapes against moving and editing, or unlocks them. Applies to the shapes in `ids` (on slide `onSlide`, from 1) or else the selection."
        }
        "shape.setDefault" => "Makes the selected shape's formatting the default for new shapes.",
        "image.crop" => {
            "Crops pictures by fractions (0..1) of each edge. Applies to the shapes in `ids` (on slide `onSlide`, from 1) or else the selection."
        }
        "image.adjust" => {
            "Sets picture brightness, contrast, saturation, grayscale, transparency or recolor. Applies to the shapes in `ids` (on slide `onSlide`, from 1) or else the selection."
        }
        "image.reset" => {
            "Removes the crop and corrections from pictures. Applies to the shapes in `ids` (on slide `onSlide`, from 1) or else the selection."
        }
        "image.change" => {
            "Replaces the picture with another file or base64 data, keeping size and position. Applies to the shapes in `ids` (on slide `onSlide`, from 1) or else the selection."
        }
        "media.options" => {
            "Sets audio/video playback options (autoplay, loop, rewind, volume, trim, fade). Applies to the shapes in `ids` (on slide `onSlide`, from 1) or else the selection."
        }
        "drawing.describe" => {
            "Answers the drawing objects (shapes, text boxes, connectors, groups, charts) in KitDrawing form: geometry, frame, fill, stroke, text. Defaults to the selection, else every object on the current slide."
        }
        "drawing.insert" => {
            "Inserts a KitDrawing object (shape, text box or chart) from geometry, frame, fill, stroke and text on the current slide and answers its id."
        }
        "drawing.delete" => "Deletes the drawing objects in `ids` (on slide `onSlide`, from 1).",
        "drawing.transform" => "Sets a drawing object's frame, or moves, rotates or flips it, by KitDrawing ids.",
        "drawing.format" => "Sets fill, stroke or geometry of drawing objects by KitDrawing ids.",
        "drawing.text" => "Sets a drawing object's text (plain or as paragraphs with runs), body properties and warp.",
        "drawing.arrange" => "Moves drawing objects to the front, back, forward or backward in the stacking order.",
        "drawing.reconnect" => "Moves one end of connector `id` to another object's site or to a point.",
        "drawing.connect" => "Draws a connector of a given geometry between drawing objects `from` and `to` and answers its id.",
        "drawing.group" => "Groups two or more drawing objects on the slide into one group and answers its id.",
        "drawing.ungroup" => "Ungroups the groups in `ids` into their members.",
        "drawing.chart" => "Changes a chart drawing object's kind, title, legend, data labels or data.",
        "drawing.ink" => "Draws ink strokes (pen, pencil or highlighter) through `points` on the current slide.",
        "drawing.erase" => "Deletes the ink strokes a path, or a circle at `x`, `y`, touches.",
        "ink.pen" => "Selects the pen ink tool in the editor with a color and width; answers the tool state.",
        "ink.pencil" => "Selects the pencil ink tool in the editor; answers the tool state.",
        "ink.highlighter" => "Selects the highlighter ink tool in the editor; answers the tool state.",
        "ink.eraser" => "Selects the ink eraser in the editor; answers the tool state.",
        "ink.select" => "Leaves inking in the editor and goes back to selecting; answers the tool state.",
        "ink.state" => "Answers the editor's current ink tool, color and width.",
        "arrange.bringToFront" => {
            "Brings shapes to the front of the stacking order. Applies to the shapes in `ids` (on slide `onSlide`, from 1) or else the selection."
        }
        "arrange.bringForward" => {
            "Brings shapes one step forward in the stacking order. Applies to the shapes in `ids` (on slide `onSlide`, from 1) or else the selection."
        }
        "arrange.sendBackward" => {
            "Sends shapes one step backward in the stacking order. Applies to the shapes in `ids` (on slide `onSlide`, from 1) or else the selection."
        }
        "arrange.sendToBack" => {
            "Sends shapes to the back of the stacking order. Applies to the shapes in `ids` (on slide `onSlide`, from 1) or else the selection."
        }
        "arrange.reorder" => "Puts shape `id` at position `index` in the slide's stacking order (0 is the back).",
        "arrange.group" => "Groups the shapes into one group. Applies to the shapes in `ids` (on slide `onSlide`, from 1) or else the selection.",
        "arrange.ungroup" => {
            "Ungroups the selected groups into their members. Applies to the shapes in `ids` (on slide `onSlide`, from 1) or else the selection."
        }
        "arrange.regroup" => "Groups again the shapes that were last ungrouped.",
        "arrange.align" => {
            "Aligns shapes' left, center, right, top, middle or bottom edges to the slide or to the selection. Applies to the shapes in `ids` (on slide `onSlide`, from 1) or else the selection."
        }
        "arrange.distribute" => {
            "Spaces shapes evenly across or down the slide or the selection. Applies to the shapes in `ids` (on slide `onSlide`, from 1) or else the selection."
        }
        "arrange.rotateLeft" => {
            "Rotates shapes 90° counter-clockwise. Applies to the shapes in `ids` (on slide `onSlide`, from 1) or else the selection."
        }
        "arrange.rotateRight" => "Rotates shapes 90° clockwise. Applies to the shapes in `ids` (on slide `onSlide`, from 1) or else the selection.",
        "arrange.flipHorizontal" => "Flips shapes horizontally. Applies to the shapes in `ids` (on slide `onSlide`, from 1) or else the selection.",
        "arrange.flipVertical" => "Flips shapes vertically. Applies to the shapes in `ids` (on slide `onSlide`, from 1) or else the selection.",
        "arrange.nudge" => {
            "Moves shapes by `dx`/`dy` points, as the arrow keys do. Applies to the shapes in `ids` (on slide `onSlide`, from 1) or else the selection."
        }
        "shape.merge" => "Merges shapes (union, combine, fragment, intersect, subtract) into new shapes; the first shape gives the formatting.",
        "text.edit" => {
            "Starts editing the text of shape `id` (or a table cell, or the notes) with the insertion point at a paragraph and character, for the `text.*` typing commands."
        }
        "text.exit" => "Stops editing text and selects the shape that was edited.",
        "text.insert" => {
            "Types `text` at the insertion point of the text being edited (`\\n` starts a paragraph). Needs `text.edit` first; to replace a shape's whole text use `text.set`."
        }
        "text.delete" => "Deletes a character or word backward or forward from the insertion point of the text being edited.",
        "text.move" => {
            "Moves the insertion point of the text being edited (left, right, word, line, paragraph, start, end), extending the selection with `extend`."
        }
        "text.select" => "Selects text from `anchor` to `caret` ([paragraph, character]) in the text being edited.",
        "text.selectAll" => "Selects all the text of the shape being edited.",
        "text.selectWord" => "Selects the word at the insertion point (or at `at`) in the text being edited.",
        "text.selectParagraph" => "Selects the paragraph at the insertion point (or at `at`) in the text being edited.",
        "text.set" => {
            "Replaces all the text of shape `id` (or a table cell) with `text`, keeping the first run's formatting. The way to write a shape's text without `text.edit`."
        }
        "text.get" => "Answers the text of shape `id` (else the selected shape) as plain text and as paragraphs with runs.",
        "format.bold" => {
            "Turns bold on or off (toggles without `on`) for the selected text or the whole text of shapes in `ids` (on slide `onSlide`, from 1)."
        }
        "format.italic" => "Turns italic on or off for the selected text or the whole text of shapes in `ids`.",
        "format.underline" => "Turns underline on or off, in a style, for the selected text or the whole text of shapes in `ids`.",
        "format.strikethrough" => "Turns single or double strikethrough on or off for the selected text or shapes in `ids`.",
        "format.superscript" => "Turns superscript on or off for the selected text or shapes in `ids`.",
        "format.subscript" => "Turns subscript on or off for the selected text or shapes in `ids`.",
        "format.fontName" => {
            "Sets the font family of the selected text or the whole text of shapes in `ids`; the answer says when this machine lacks the font and what draws it."
        }
        "format.fontSize" => "Sets the font size in points of the selected text or shapes in `ids`.",
        "format.increaseFontSize" => "Increases the font size one step of the size list for the selected text or shapes in `ids`.",
        "format.decreaseFontSize" => "Decreases the font size one step of the size list for the selected text or shapes in `ids`.",
        "format.color" => {
            "Sets the font color (hex, a theme color such as accent1, or a scheme color with modifiers) of the selected text or the whole text of shapes in `ids`."
        }
        "format.highlight" => "Sets or removes the text highlight color of the selected text or shapes in `ids`.",
        "format.case" => "Changes the case (sentence, lower, upper, title, toggle) of the selected text or shapes in `ids`.",
        "format.caps" => "Sets small caps or all caps (or none) for the selected text or shapes in `ids`.",
        "format.spacing" => "Sets character spacing in points for the selected text or shapes in `ids`.",
        "format.clear" => "Removes character formatting from the selected text or shapes in `ids`, back to the placeholder's style.",
        "format.textShadow" => "Turns a text shadow on or off for the selected text or shapes in `ids`.",
        "format.textOutline" => "Sets a text outline color and width (or none) for the selected text or shapes in `ids`.",
        "format.transform" => "Applies an art text warp (arch, circle, wave, inflate, …) to shapes in `ids` or the selection.",
        "format.align" => "Sets paragraph alignment (left, center, right, justify, distributed) of the selected paragraphs or the shapes in `ids`.",
        "format.rtl" => "Sets right-to-left (or left-to-right) paragraph direction for the selected paragraphs or shapes in `ids`.",
        "format.alignLeft" => "Aligns the selected paragraphs, or shapes in `ids`, left.",
        "format.alignCenter" => "Centers the selected paragraphs, or shapes in `ids`.",
        "format.alignRight" => "Aligns the selected paragraphs, or shapes in `ids`, right.",
        "format.justify" => "Justifies the selected paragraphs, or shapes in `ids`.",
        "format.bullets" => "Turns bullets on or off, with a character, color and size, for the selected paragraphs or shapes in `ids`.",
        "format.numbering" => "Turns numbering on or off, with a scheme and start number, for the selected paragraphs or shapes in `ids`.",
        "format.indent" => "Raises the list level of the selected paragraphs or shapes in `ids` by one.",
        "format.outdent" => "Lowers the list level of the selected paragraphs or shapes in `ids` by one.",
        "format.lineSpacing" => "Sets line spacing as a multiple or exact points for the selected paragraphs or shapes in `ids`.",
        "format.paragraph" => "Sets paragraph alignment, indents and spacing before/after for the selected paragraphs or shapes in `ids`.",
        "format.anchor" => "Sets the vertical alignment of text (top, middle, bottom) in shapes in `ids` or the selection.",
        "format.direction" => "Sets text direction (horizontal, rotated 90/270, stacked) in shapes in `ids` or the selection.",
        "format.columns" => "Sets the number of text columns and their spacing in shapes in `ids` or the selection.",
        "format.autofit" => "Sets autofit: none, shrink text on overflow, or resize the shape to fit the text, for shapes in `ids` or the selection.",
        "format.wrap" => "Turns text wrapping inside the shape on or off for shapes in `ids` or the selection.",
        "format.margins" => "Sets the text box margins in points for shapes in `ids` or the selection.",
        "format.painter" => {
            "Picks up the formatting of the selected text or shape for `format.painterApply` (`sticky` keeps it for several applies)."
        }
        "format.painterApply" => "Applies the formatting picked up by `format.painter` to the shapes in `ids` or the selection.",
        "format.state" => {
            "Answers the effective formatting at the selection (font, size, bold, italic, alignment, bullets, …), as the ribbon shows it."
        }
        "theme.set" => "Applies a theme by name (see `theme.list`) to the whole presentation.",
        "theme.list" => "Lists the themes, color schemes and font schemes the design commands take.",
        "theme.colors" => {
            "Applies a color scheme by name, or sets theme colors (accent1..6, dk1, lt1, …) directly, and answers the theme's scheme as {name, colors}; with no parameters it only reads it."
        }
        "theme.fonts" => "Applies a font scheme by name, or sets the major (heading) and minor (body) fonts; the answer says when a font is missing.",
        "slide.background" => {
            "Sets the background of the current slide to a color, gradient, picture or style, or resets it to the layout's; with `all` it is applied to all, as PowerPoint's Apply to All: the slide masters and layouts take it and the slides drop their own, so slides added later have it too."
        }
        "slide.hideBackgroundGraphics" => "Hides or shows the master's background graphics on the current slide.",
        "slide.size" => "Sets the slide size by preset or width/height in points, scaling content to maximize or ensure fit.",
        "slide.headerFooter" => "Turns date, slide number and footer placeholders on or off (and sets their text) on the current or all slides.",
        "view.slideMaster" => "Opens slide master view on a master or layout, so edits apply to it.",
        "view.closeMaster" => "Leaves slide master view and goes back to the slides.",
        "master.insertLayout" => "Adds a new layout to the current master in slide master view.",
        "master.renameLayout" => "Renames a layout of a master.",
        "master.deleteLayout" => "Deletes a layout from a master; slides using it move to another layout.",
        "master.insertPlaceholder" => {
            "Inserts a placeholder (content, text, picture, chart, table, media) on the layout being edited in slide master view."
        }
        "transition.set" => "Sets the transition into slide `index` (from 0, else the current slide): none, morph, fade, push, wipe, …",
        "transition.options" => "Sets the transition's effect option (such as the direction) on slide `index` or the current slide.",
        "transition.timing" => "Sets transition duration, advance on click and automatic advance after a time on slide `index` or the current slide.",
        "transition.applyAll" => "Applies the current slide's transition and timing to every slide.",
        "transition.list" => "Lists the transitions `transition.set` takes, with category, default duration and effect options.",
        "animation.set" => {
            "Replaces the animations of shapes in `ids` or the selection with one effect (see `animation.list`); a `customPath` motion path goes through `points` ([[dx, dy]] in points from the shape's place), straight or `smooth`, open or `closed`."
        }
        "animation.add" => {
            "Adds an animation effect to shapes in `ids` or the selection, after their existing ones, with start, duration and delay; a `customPath` motion path goes through `points` ([[dx, dy]] in points from the shape's place)."
        }
        "animation.remove" => "Removes animation `index` on the slide, or all animations of shapes in `ids`.",
        "animation.move" => "Moves animation `index` to position `to` in the slide's animation order.",
        "animation.timing" => {
            "Sets start (on click, with or after previous), duration and delay of animation `index` or the selected shapes' animations. Appear and Disappear happen at once and take no duration, as a .pptx stores none for them: a duration given to them stays 0."
        }
        "animation.options" => "Sets an animation's effect option and text build (as one, by paragraph, word or letter).",
        "animation.clear" => "Removes every animation from slide `index` or the current slide.",
        "animation.painter" => "Picks up the animations of a shape for `animation.painterApply`.",
        "animation.painterApply" => "Applies the animations picked up by `animation.painter` to shapes in `ids` or the selection.",
        "animation.list" => "Lists the animation effects the animation commands take, with class (entrance, emphasis, exit, path) and options.",
        "animation.get" => "Answers the animations on slide `slide` (from 0, else the current slide) in playing order.",
        "table.state" => "Answers the selected or named table's selected cells, rows, columns and style options, or null when no table is selected.",
        "table.cellFormat" => "Sets cell anchor, margins and borders of the selected (or given) cells of a table.",
        "table.border" => "Sets borders on edges (all, outside, inside, top, …) of the selected or given cells of a table.",
        "table.clearCells" => "Clears the text of the given (else selected) cells of a table.",
        "table.insertRowAbove" => "Inserts a row above row `row` (from 0, else the selected row) of the table `id` or the selected table.",
        "table.insertRowBelow" => "Inserts a row below row `row` (from 0, else the selected row) of the table.",
        "table.insertColumnLeft" => "Inserts a column left of column `col` (from 0, else the selected column) of the table.",
        "table.insertColumnRight" => "Inserts a column right of column `col` (from 0, else the selected column) of the table.",
        "table.deleteRow" => "Deletes row `row` (from 0, else the selected rows) of the table.",
        "table.deleteColumn" => "Deletes column `col` (from 0, else the selected columns) of the table.",
        "table.merge" => "Merges the cells from [row, col] `from` to `to` (from 0) into one cell.",
        "table.split" => "Splits a merged cell at [row, col] back into its cells.",
        "table.style" => "Applies a table style (medium2-accent1, light1-accent2, grid, none, …) to the table.",
        "table.options" => "Turns table style options on or off: header row, total row, banded rows, first/last column, banded columns.",
        "table.cellFill" => "Sets the shading color (or none) of the given, selected or all cells of a table.",
        "table.distributeRows" => "Makes the table's rows the same height.",
        "table.distributeColumns" => "Makes the table's columns the same width.",
        "table.columnWidth" => "Sets the width in points of column `col` (from 0) of the table.",
        "table.rowHeight" => "Sets the height in points of row `row` (from 0) of the table.",
        "table.selectCells" => "Selects table cells from [row, col] `from` to `to` (from 0) for cell commands.",
        "comment.add" => {
            "Adds a comment with `text` on slide `slide` (from 0, else the current one), anchored to shape `id` or a point, and answers the thread."
        }
        "comment.reply" => "Adds a reply to comment thread `thread` (or the `index`-th thread) and answers the comment.",
        "comment.delete" => "Deletes a comment, a whole thread, or all comments with `all: true`.",
        "comment.resolve" => "Marks comment thread `thread` resolved, or open again with `resolved: false`.",
        "comment.list" => "Lists the comments on slide `slide` (from 0) or on every slide.",
        "comment.threads" => "Lists the comment threads with their slide, anchor object, comments and resolved state.",
        "comment.anchor" => {
            "Answers the selected text, shapes, or point (`x`, `y`), else the current slide, as a comment anchor of object ids for a host that keeps comment threads."
        }
        "comment.place" => {
            "Answers where each comment anchor in `anchors` is now: whether its object is found, its slide, and its shape's box or point (in points)."
        }
        "accessibility.check" => "Lists accessibility issues (missing slide titles, missing alt text, reading order) by slide and shape.",
        "spelling.check" => "Lists the words not in the built-in word list, by slide and shape.",
        "show.fromStart" => "Starts the slide show from the first slide.",
        "show.fromCurrent" => "Starts the slide show from the current slide.",
        "show.setup" => "Sets slide show options: presenter or kiosk mode, looping, narration, animations, timings and slide range.",
        "show.customShow" => "Creates or replaces a custom show named `name` from slide indexes (from 0), or deletes it.",
        "file.inspect" => {
            "Answers the presentation: slides with ids, titles, layouts and shape counts, the current slide and selection, slide size, theme and fonts. Start here."
        }
        "slide.inspect" => {
            "Answers one slide (`index` from 0, else the current one): its shapes with ids, kinds, boxes, text and fills (a group's children with their boxes on the slide), plus animations, transition, notes and comments. An agent's read leaves the current slide and the selection as they were: name the slide in later calls (`onSlide`, or the run's `target` or a step's `at.part`)."
        }
        "shape.inspect" => "Answers the full properties of shape `id` (on slide `onSlide`, from 1) with its effective formatting.",
        "command.list" => "Lists the kit's commands with labels, parameters and whether they are enabled, filtered by a substring.",
        "media.play" => "Plays the audio or video `id` (else the one on the slide) in the editor, from `from` milliseconds.",
        "media.pause" => "Pauses the audio or video `id` (else the one on the slide) in the editor.",
        "media.toggle" => "Plays or pauses the audio or video `id` (else the one on the slide) in the editor.",
        "media.stop" => "Stops the audio or video `id` (else the one on the slide) and rewinds it.",
        "media.seek" => "Moves the playback position of the audio or video `id` to `ms` milliseconds.",
        "media.info" => "Answers the audio or video `id` (else the one on the slide): name, content type, size, duration and playback options.",
        "media.posterFrame" => "Sets the video's poster frame to the frame at `ms` (else the current position).",
        "media.gifPlay" => "Plays or pauses the animated GIF `id` (else the one on the slide) in the editor.",
        "presence.selection" => "Answers the selection in the form collaborators' presence shows it (slide and objects).",
        "presence.locate" => "Answers where the objects in `ids` are drawn (x, y, width, height), for showing collaborators' cursors.",
        _ => "",
    }
}

pub fn output(id: &str) -> Option<Value> {
    let schema = match id {
        "file.saveTemplate" => r#"{"type":"object","additionalProperties":true,"properties":{"needsPath":{"type":"boolean"}}}"#,
        "file.saveBytes" => r#"{"type":"object","additionalProperties":true,"properties":{"bytes":{"type":"integer"},"data":{"type":"string"}}}"#,
        "file.exportBytes" => {
            r#"{"type":"object","additionalProperties":true,"properties":{"bytes":{"type":"integer"},"data":{"type":"string"},"files":{"type":"array"},"format":{"type":"string"}}}"#
        }
        "file.render" => {
            r#"{"type":"object","additionalProperties":true,"properties":{"height":{"type":"integer"},"png":{"type":"string"},"scale":{"type":"number"},"slide":{"type":"integer"},"slideId":{"type":"integer"},"width":{"type":"integer"}}}"#
        }
        "edit.copy" => r#"{"type":"object","additionalProperties":true,"properties":{"text":{"type":"string"}}}"#,
        "edit.selectAll" => r#"{"type":"object","additionalProperties":true,"properties":{"ids":{"type":"array","items":{"type":"integer"}}}}"#,
        "edit.select" => r#"{"type":"object","additionalProperties":true,"properties":{"selected":{"type":"array"}}}"#,
        "slide.go" => r#"{"type":"object","additionalProperties":true,"properties":{"index":{"type":"integer"}}}"#,
        "slide.next" => r#"{"type":"object","additionalProperties":true,"properties":{"index":{"type":"integer"}}}"#,
        "slide.previous" => r#"{"type":"object","additionalProperties":true,"properties":{"index":{"type":"integer"}}}"#,
        "slide.first" => r#"{"type":"object","additionalProperties":true,"properties":{"index":{"type":"integer"}}}"#,
        "slide.last" => r#"{"type":"object","additionalProperties":true,"properties":{"index":{"type":"integer"}}}"#,
        "slide.selectSlides" => r#"{"type":"object","additionalProperties":true,"properties":{"selected":{"type":"array"}}}"#,
        "shape.presets" => {
            r#"{"type":"array","items":{"type":"object","additionalProperties":true,"properties":{"category":{"type":"string"},"label":{"type":"string"},"name":{"type":"string"}}}}"#
        }
        "shape.sites" => r#"{"type":"array","items":{"type":"array","items":{"type":"number"}}}"#,
        "drawing.describe" => {
            r#"{"type":"object","additionalProperties":true,"properties":{"objects":{"type":"array","items":{"type":"object","additionalProperties":true,"properties":{"frame":{"type":"json"},"geometry":{"type":"json"},"id":{"type":"string"},"kind":{"type":"string"},"placement":{"type":"json"},"text":{"type":"json"}}}},"selection":{"type":"object","additionalProperties":true,"properties":{"frame":{"type":"object","additionalProperties":true,"properties":{"mixed":{"type":"boolean"}}},"geometry":{"type":"object","additionalProperties":true,"properties":{"adjust":{"type":"json"},"preset":{"type":"string"}}},"ids":{"type":"array","items":{"type":"string"}},"kind":{"type":"object","additionalProperties":true,"properties":{"mixed":{"type":"boolean"}}},"placement":{"type":"object","additionalProperties":true,"properties":{"kind":{"type":"string"}}},"text":{"type":"object","additionalProperties":true,"properties":{"mixed":{"type":"boolean"}}}}}}}"#
        }
        "ink.pen" => {
            r#"{"type":"object","additionalProperties":true,"properties":{"color":{"type":"string"},"tool":{"type":"string"},"width":{"type":"number"}}}"#
        }
        "ink.pencil" => {
            r#"{"type":"object","additionalProperties":true,"properties":{"color":{"type":"string"},"tool":{"type":"string"},"width":{"type":"number"}}}"#
        }
        "ink.highlighter" => {
            r#"{"type":"object","additionalProperties":true,"properties":{"color":{"type":"string"},"tool":{"type":"string"},"width":{"type":"number"}}}"#
        }
        "ink.eraser" => {
            r#"{"type":"object","additionalProperties":true,"properties":{"color":{"type":"string"},"tool":{"type":"string"},"width":{"type":"number"}}}"#
        }
        "ink.select" => {
            r#"{"type":"object","additionalProperties":true,"properties":{"color":{"type":"string"},"tool":{"type":"string"},"width":{"type":"number"}}}"#
        }
        "ink.state" => {
            r#"{"type":"object","additionalProperties":true,"properties":{"color":{"type":"string"},"tool":{"type":"string"},"width":{"type":"number"}}}"#
        }
        "text.edit" => {
            r#"{"type":"object","additionalProperties":true,"properties":{"at":{"type":"array","items":{"type":"integer"}},"id":{"type":"integer"}}}"#
        }
        "text.get" => {
            r#"{"type":"object","additionalProperties":true,"properties":{"paragraphs":{"type":"array","items":{"type":"object","additionalProperties":true,"properties":{"level":{"type":"integer"},"text":{"type":"string"}}}},"text":{"type":"string"}}}"#
        }
        "format.painter" => r#"{"type":"object","additionalProperties":true,"properties":{"armed":{"type":"boolean"}}}"#,
        "format.state" => {
            r#"{"type":"object","additionalProperties":true,"properties":{"align":{"type":"string"},"bold":{"type":"boolean"},"bullets":{"type":"boolean"},"font":{"type":"string"},"fontRef":{"type":"string"},"italic":{"type":"boolean"},"lineSpacing":{"type":"json"},"numbering":{"type":"boolean"},"painter":{"type":"boolean"},"size":{"type":"number"},"strike":{"type":"boolean"},"subscript":{"type":"boolean"},"superscript":{"type":"boolean"},"underline":{"type":"boolean"}}}"#
        }
        "theme.list" => {
            r#"{"type":"object","additionalProperties":true,"properties":{"colorSchemes":{"type":"array","items":{"type":"string"}},"fontSchemes":{"type":"array","items":{"type":"object","additionalProperties":true,"properties":{"major":{"type":"string"},"minor":{"type":"string"},"name":{"type":"string"}}}},"themes":{"type":"array","items":{"type":"object","additionalProperties":true,"properties":{"colors":{"type":"json"},"major":{"type":"string"},"minor":{"type":"string"},"name":{"type":"string"}}}}}}"#
        }
        "view.slideMaster" => {
            r#"{"type":"object","additionalProperties":true,"properties":{"kind":{"type":"string"},"layout":{"type":"integer"},"master":{"type":"integer"}}}"#
        }
        "transition.list" => {
            r#"{"type":"array","items":{"type":"object","additionalProperties":true,"properties":{"category":{"type":"string"},"duration":{"type":"integer"},"id":{"type":"string"},"label":{"type":"string"},"options":{"type":"array","items":{"type":"string"}}}}}"#
        }
        "animation.list" => {
            r#"{"type":"array","items":{"type":"object","additionalProperties":true,"properties":{"class":{"type":"string"},"id":{"type":"string"},"label":{"type":"string"},"options":{"type":"array","items":{"type":"string"}}}}}"#
        }
        "animation.get" => r#"{"type":"array"}"#,
        "comment.list" => r#"{"type":"array"}"#,
        "comment.threads" => r#"{"type":"array"}"#,
        "comment.anchor" => r#"{"type":"json"}"#,
        "comment.place" => r#"{"type":"array"}"#,
        "accessibility.check" => {
            r#"{"type":"array","items":{"type":"object","additionalProperties":true,"properties":{"issue":{"type":"string"},"slide":{"type":"integer"}}}}"#
        }
        "spelling.check" => r#"{"type":"array"}"#,
        "show.fromStart" => r#"{"type":"object","additionalProperties":true,"properties":{"from":{"type":"integer"}}}"#,
        "show.fromCurrent" => r#"{"type":"object","additionalProperties":true,"properties":{"from":{"type":"integer"}}}"#,
        "file.inspect" => {
            r#"{"type":"object","additionalProperties":true,"properties":{"dirty":{"type":"boolean"},"fonts":{"type":"object","additionalProperties":true,"properties":{"major":{"type":"string"},"minor":{"type":"string"}}},"layouts":{"type":"array","items":{"type":"object","additionalProperties":true,"properties":{"id":{"type":"integer"},"name":{"type":"string"}}}},"media":{"type":"array"},"path":{"type":"json"},"redo":{"type":"json"},"sections":{"type":"array"},"selection":{"type":"object","additionalProperties":true,"properties":{"cells":{"type":"json"},"shapes":{"type":"array"},"slide":{"type":"integer"},"slides":{"type":"array"},"target":{"type":"object","additionalProperties":true,"properties":{"kind":{"type":"string"}}},"text":{"type":"json"}}},"slideSize":{"type":"array","items":{"type":"number"}},"slides":{"type":"array","items":{"type":"object","additionalProperties":true,"properties":{"animations":{"type":"integer"},"hidden":{"type":"boolean"},"id":{"type":"integer"},"index":{"type":"integer"},"layout":{"type":"string"},"notes":{"type":"string"},"shapes":{"type":"integer"},"title":{"type":"string"},"transition":{"type":"json"}}}},"theme":{"type":"string"},"title":{"type":"string"},"undo":{"type":"json"}}}"#
        }
        "slide.inspect" => {
            r#"{"type":"object","additionalProperties":true,"properties":{"animations":{"type":"array"},"background":{"type":"string"},"comments":{"type":"integer"},"hidden":{"type":"boolean"},"id":{"type":"integer"},"index":{"type":"integer"},"layout":{"type":"string"},"notes":{"type":"string"},"shapes":{"type":"array","items":{"type":"object","additionalProperties":true,"properties":{"box":{"type":"json"},"fill":{"type":"string"},"id":{"type":"integer"},"kind":{"type":"string"},"name":{"type":"string"},"preset":{"type":"string"},"rotation":{"type":"number"},"text":{"type":"string"}}}},"title":{"type":"string"},"transition":{"type":"json"}}}"#
        }
        "shape.inspect" => {
            r#"{"type":"object","additionalProperties":true,"properties":{"box":{"type":"object","additionalProperties":true,"properties":{"h":{"type":"number"},"w":{"type":"number"},"x":{"type":"number"},"y":{"type":"number"}}},"shape":{"type":"object","additionalProperties":true,"properties":{"click":{"type":"json"},"decorative":{"type":"boolean"},"descr":{"type":"string"},"effects":{"type":"json"},"fill":{"type":"json"},"geom":{"type":"object","additionalProperties":true,"properties":{"kind":{"type":"string"},"name":{"type":"string"}}},"hidden":{"type":"boolean"},"hover":{"type":"json"},"id":{"type":"integer"},"kind":{"type":"object","additionalProperties":true,"properties":{"type":{"type":"string"}}},"line":{"type":"json"},"locked":{"type":"boolean"},"name":{"type":"string"},"ph":{"type":"json"},"style":{"type":"json"},"text":{"type":"object","additionalProperties":true,"properties":{"body":{"type":"json"},"paragraphs":{"type":"json"}}},"textBox":{"type":"boolean"},"title":{"type":"string"},"xfrm":{"type":"object","additionalProperties":true,"properties":{"h":{"type":"number"},"w":{"type":"number"},"x":{"type":"number"},"y":{"type":"number"}}}}}}}"#
        }
        "command.list" => {
            r#"{"type":"array","items":{"type":"object","additionalProperties":true,"properties":{"enabled":{"type":"boolean"},"id":{"type":"string"},"label":{"type":"string"},"menu":{"type":"array","items":{"type":"string"}},"params":{"type":"string"},"shortcut":{"type":"json"},"disabled_reason":{"type":"string"}}}}"#
        }
        "presence.selection" => r#"{"type":"object","additionalProperties":true,"properties":{"objects":{"type":"array"},"slide":{"type":"json"}}}"#,
        "presence.locate" => r#"{"type":"array"}"#,
        "media.info" => {
            r#"{"type":"object","additionalProperties":true,"properties":{"id":{"type":"integer"},"name":{"type":"string"},"contentType":{"type":"string"},"bytes":{"type":"integer"},"durationMs":{"type":"number"},"video":{"type":"boolean"},"width":{"type":"integer"},"height":{"type":"integer"},"probe":{"type":"json"},"options":{"type":"json"},"status":{"type":"json"}}}"#
        }
        "file.recovery.list" => {
            r#"{"type":"array","items":{"type":"object","additionalProperties":true,"properties":{"uid":{"type":"integer","description":"the entry's uid, as file.recovery.discard takes it"},"path":{"type":"string"},"title":{"type":"string"},"saved":{"type":"json"}}}}"#
        }
        "table.state" => {
            r#"{"type":"json","description":"the table selection's cells, rows, columns and style options; null when no table is selected"}"#
        }
        "media.gifPlay" => r#"{"type":"object","additionalProperties":true,"properties":{"id":{"type":"integer"},"playing":{"type":"boolean"}}}"#,
        _ => return None,
    };
    serde_json::from_str(schema).ok()
}

/// Types for parameter keys the command notation leaves untyped (`json`), from the engine's
/// parsing of each.
fn key_type(id: &str, key: &str) -> Option<Value> {
    let node = match (id, key) {
        ("shape.move", "toSlide") => {
            r#"{"oneOf":[{"type":"string"},{"type":"integer"}],"description":"destination slide id; shape identities are retained"}"#
        }
        ("file.new", "size") => r#"{"type":"array","items":{"type":"number"},"description":"[w, h] in points"}"#,
        ("file.openBytes", "data") => r#"{"type":"string","description":"the file as base64"}"#,
        ("file.saveAs", "format") => r#"{"type":"string","description":"kslide or pptx (else from the extension)","enum":["kslide","pptx"]}"#,
        ("file.export", "dpi") => r#"{"type":"number","description":"dots per inch for PDF pictures"}"#,
        ("file.export", "scale") => r#"{"type":"number","description":"pixels per point for PNG/JPEG"}"#,
        ("file.exportBytes", "dpi") => r#"{"type":"number","description":"dots per inch for PDF pictures"}"#,
        ("file.exportBytes", "scale") => r#"{"type":"number","description":"pixels per point for PNG/JPEG"}"#,
        ("file.exportBytes", "textLayer") => r#"{"type":"boolean","description":"PDF: keep the text selectable"}"#,
        ("file.export", "textLayer") => r#"{"type":"boolean","description":"PDF: keep the text selectable"}"#,
        ("file.render", "target") => {
            r#"{"type":"object","description":"the slide to draw: `slide` counted from 1, or `slideId` as slide_inspect lists it","properties":{"slide":{"type":"integer","description":"the slide, counted from 1"},"slideId":{"type":"integer","description":"the slide's id"}},"additionalProperties":false}"#
        }
        ("file.render", "slide") => r#"{"type":"integer","description":"the slide, an index counted from 0 (target.slide counts from 1)"}"#,
        ("file.render", "scale") => r#"{"type":"number","description":"pixels per point, more than 0 and at most 16 (default 1)"}"#,
        ("file.render", "maxSize") => {
            r#"{"type":"integer","description":"the longest side in pixels, 16 to 8192; a larger render is brought down to it"}"#
        }
        ("file.properties", "author") => r#"{"type":"string"}"#,
        ("file.properties", "category") => r#"{"type":"string"}"#,
        ("file.properties", "comments") => r#"{"type":"string"}"#,
        ("file.properties", "company") => r#"{"type":"string"}"#,
        ("file.properties", "keywords") => r#"{"type":"string"}"#,
        ("file.properties", "subject") => r#"{"type":"string"}"#,
        ("edit.cut", "scope") => r#"{"type":"string","description":"slides: act on the selected slides","enum":["slides"]}"#,
        ("edit.copy", "scope") => r#"{"type":"string","description":"slides: act on the selected slides","enum":["slides"]}"#,
        ("edit.paste", "scope") => r#"{"type":"string","description":"slides: paste as slides","enum":["slides"]}"#,
        ("edit.delete", "scope") => r#"{"type":"string","description":"slides: delete the selected slides","enum":["slides"]}"#,
        ("edit.paste", "text") => r#"{"type":"string","description":"plain text to paste instead of the clipboard"}"#,
        ("slide.new", "layout") => {
            r#"{"type":"string","description":"a layout name or kind: title, titleAndContent, sectionHeader, twoContent, comparison, titleOnly, blank, …"}"#
        }
        ("slide.layout", "layout") => r#"{"type":"string","description":"a layout name or kind"}"#,
        ("section.add", "at") => r#"{"type":"integer","description":"the slide the section starts at (from 0)"}"#,
        ("section.move", "to") => r#"{"type":"integer","description":"the section position (from 0)"}"#,
        ("image.insert", "data") => r#"{"type":"string","description":"the picture as base64, instead of path"}"#,
        ("audio.insert", "data") => r#"{"type":"string","description":"the audio as base64, instead of path"}"#,
        ("video.insert", "data") => r#"{"type":"string","description":"the video as base64, instead of path"}"#,
        ("image.change", "data") => r#"{"type":"string","description":"the picture as base64, instead of path"}"#,
        ("table.insert", "cols") => r#"{"type":"integer","description":"number of columns"}"#,
        ("table.insert", "rect") => r#"{"type":"array","items":{"type":"number"},"description":"[x, y, w, h] in points"}"#,
        ("chart.insert", "rect") => r#"{"type":"array","items":{"type":"number"},"description":"[x, y, w, h] in points"}"#,
        ("audio.insert", "rect") => r#"{"type":"array","items":{"type":"number"},"description":"[x, y, w, h] in points"}"#,
        ("video.insert", "rect") => r#"{"type":"array","items":{"type":"number"},"description":"[x, y, w, h] in points"}"#,
        ("actionButton.insert", "rect") => r#"{"type":"array","items":{"type":"number"},"description":"[x, y, w, h] in points"}"#,
        ("chart.insert", "type") => {
            r#"{"type":"string","description":"a chart type; an unknown one is refused","enum":["column","stackedColumn","percentColumn","bar","stackedBar","percentBar","line","lineMarkers","stackedLine","area","stackedArea","pie","doughnut","scatter","bubble","radar","filledRadar","waterfall","funnel","histogram","pareto","treemap","sunburst","boxWhisker","stock","surface","combo"]}"#
        }
        ("artText.insert", "style") => r#"{"type":"integer","description":"the art text gallery style, from 0"}"#,
        ("field.insertDateTime", "format") => r#"{"type":"string","description":"datetime1 to datetime13, as PowerPoint numbers them"}"#,
        ("symbol.insert", "text") => r#"{"type":"string","description":"the character(s) to insert"}"#,
        ("link.insert", "text") => r#"{"type":"string","description":"the link text, used when nothing is selected"}"#,
        ("link.insert", "tooltip") => r#"{"type":"string","description":"the tooltip shown on hover"}"#,
        ("chart.type", "type") => r#"{"type":"string","description":"the chart type: column, bar, line, pie, doughnut, area, scatter, …"}"#,
        ("chart.options", "legend") => {
            r#"{"oneOf":[{"type":"string","enum":["b","t","l","r"]},{"type":"null"}],"description":"legend position, null for none"}"#
        }
        ("chart.options", "palette") => r#"{"type":"string","description":"a palette name"}"#,
        ("chart.options", "title") => r#"{"oneOf":[{"type":"string"},{"type":"null"}],"description":"chart title, null for none"}"#,
        ("shape.connect", "from") => {
            r#"{"oneOf":[{"type":"integer"},{"type":"string"}],"description":"a shape id (number, number string or KitDrawing id)"}"#
        }
        ("shape.connect", "to") => {
            r#"{"oneOf":[{"type":"integer"},{"type":"string"}],"description":"a shape id (number, number string or KitDrawing id)"}"#
        }
        ("shape.connect", "fromSite") => r#"{"type":"integer","description":"connection site index (shape.sites)"}"#,
        ("shape.connect", "toSite") => r#"{"type":"integer","description":"connection site index (shape.sites); default: the closest pair"}"#,
        ("shape.connect", "id") => {
            r#"{"oneOf":[{"type":"integer"},{"type":"string"}],"description":"an existing line to glue; else a new connector"}"#
        }
        ("shape.freeform", "points") => {
            r#"{"type":"array","items":{"type":"array","items":{"type":"number"}},"description":"[[x, y], …] in slide points"}"#
        }
        ("shape.rotate", "by") => r#"{"type":"number","description":"degrees to rotate by"}"#,
        ("shape.rotate", "deg") => r#"{"type":"number","description":"absolute rotation in degrees"}"#,
        ("shape.adjust", "value") => r#"{"type":"number","description":"the adjustment value in file units"}"#,
        ("shape.line", "cap") => r#"{"type":"string","description":"line cap","enum":["flat","round","square"]}"#,
        ("shape.line", "join") => r#"{"type":"string","description":"line join","enum":["round","bevel","miter"]}"#,
        ("shape.line", "none") => r#"{"type":"boolean","description":"remove the outline"}"#,
        ("shape.line", "reset") => r#"{"type":"boolean","description":"back to the theme outline"}"#,
        ("shape.line", "tail") => r#"{"type":"string","description":"arrowhead at the line's end: none, triangle, stealth, diamond, oval, arrow"}"#,
        ("shape.effects", "reset") => r#"{"type":"boolean","description":"remove every effect"}"#,
        ("shape.effects", "shadow") => {
            r#"{"oneOf":[{"type":"string"},{"type":"object","additionalProperties":false,"properties":{"color":{"type":"json"},"blur":{"type":"number","minimum":0,"maximum":200},"dist":{"type":"number","minimum":0,"maximum":200},"dir":{"type":"number"},"inner":{"type":"boolean"}}}],"description":"shadow preset or custom shadow"}"#
        }
        ("shape.effects", "glow") => {
            r#"{"oneOf":[{"type":"number","minimum":0,"maximum":150},{"type":"object","additionalProperties":false,"properties":{"color":{"type":"json"},"radius":{"type":"number","minimum":0,"maximum":150}}},{"type":"null"},{"type":"string","enum":["none"]}],"description":"glow radius in points or custom glow; null or none removes it"}"#
        }
        ("shape.effects", "softEdges") => {
            r#"{"type":"number","minimum":0,"maximum":100,"description":"soft edge radius in points; zero removes it"}"#
        }
        ("image.crop", "bottom") => r#"{"type":"number","description":"fraction 0..1 cropped from the bottom"}"#,
        ("media.options", "acrossSlides") => r#"{"type":"boolean"}"#,
        ("media.options", "autoplay") => r#"{"type":"boolean"}"#,
        ("media.options", "fullScreen") => r#"{"type":"boolean"}"#,
        ("media.options", "hide") => r#"{"type":"boolean","description":"hide while not playing"}"#,
        ("media.options", "loop") => r#"{"type":"boolean"}"#,
        ("media.options", "rewind") => r#"{"type":"boolean","description":"rewind after playing"}"#,
        ("drawing.transform", "flipH") => r#"{"type":"boolean"}"#,
        ("drawing.transform", "flipV") => r#"{"type":"boolean"}"#,
        ("drawing.transform", "rotateBy") => r#"{"type":"number","description":"degrees"}"#,
        ("drawing.text", "warp") => r#"{"oneOf":[{"type":"string"},{"type":"null"}],"description":"a text warp preset, or null for none"}"#,
        ("drawing.connect", "from") => {
            r#"{"oneOf":[{"type":"integer"},{"type":"string"}],"description":"a shape id (number, number string or KitDrawing id)"}"#
        }
        ("drawing.connect", "to") => {
            r#"{"oneOf":[{"type":"integer"},{"type":"string"}],"description":"a shape id (number, number string or KitDrawing id)"}"#
        }
        ("drawing.connect", "fromSite") => r#"{"type":"integer"}"#,
        ("drawing.connect", "toSite") => r#"{"type":"integer"}"#,
        ("drawing.chart", "categories") => r#"{"type":"array","items":{"type":"string"}}"#,
        ("drawing.chart", "dataLabels") => r#"{"type":"boolean"}"#,
        ("drawing.chart", "gridlines") => r#"{"type":"boolean"}"#,
        ("drawing.chart", "kind") => r#"{"type":"string","description":"the chart kind: column, bar, line, pie, …"}"#,
        ("drawing.chart", "legend") => {
            r#"{"oneOf":[{"type":"string","enum":["top","bottom","left","right","topRight"]},{"type":"null"}],"description":"legend position, null for none"}"#
        }
        ("drawing.chart", "title") => r#"{"oneOf":[{"type":"string"},{"type":"null"}],"description":"chart title, null for none"}"#,
        ("drawing.chart", "xTitle") => r#"{"oneOf":[{"type":"string"},{"type":"null"}],"description":"x axis title"}"#,
        ("drawing.chart", "yTitle") => r#"{"oneOf":[{"type":"string"},{"type":"null"}],"description":"y axis title"}"#,
        ("arrange.reorder", "index") => r#"{"type":"integer","description":"position in the stack, 0 is the back"}"#,
        ("shape.merge", "ids") => {
            r#"{"type":"array","items":{"oneOf":[{"type":"integer"},{"type":"string"}]},"description":"shapes in selection order; the first one's formatting is kept"}"#
        }
        ("format.spacing", "pt") => {
            r#"{"type":"number","description":"character spacing in points (veryTight -3, tight -1.5, normal 0, loose 3, veryLoose 6)"}"#
        }
        ("format.transform", "preset") => {
            r#"{"type":"string","description":"a text warp preset: textArchUp, textArchDown, textCircle, textWave1, textInflate, textDeflate, …"}"#
        }
        ("format.bullets", "char") => r#"{"type":"string","description":"the bullet character: •, ○, ■, □, ◆, ➢, ✓, –"}"#,
        ("format.bullets", "size") => r#"{"type":"number","description":"bullet size in % of the text"}"#,
        ("format.lineSpacing", "lines") => r#"{"type":"number","description":"line spacing in lines (1.0, 1.5, 2.0, …)"}"#,
        ("format.lineSpacing", "pt") => r#"{"type":"number","description":"exact line spacing in points"}"#,
        ("format.paragraph", "align") => {
            r#"{"type":"string","description":"left, center, right, justify, justifyLow or distributed","enum":["left","center","right","justify","justifyLow","distributed"]}"#
        }
        ("format.paragraph", "lineSpacing") => r#"{"type":"number","description":"line spacing in lines"}"#,
        ("slide.size", "preset") => r#"{"type":"string","description":"widescreen, standard, a4, letter, …"}"#,
        ("slide.headerFooter", "dateText") => r#"{"type":"string","description":"fixed date text"}"#,
        ("view.slideMaster", "layout") => {
            r#"{"oneOf":[{"type":"integer"},{"type":"null"}],"description":"the layout index from 0, or null for the master itself"}"#
        }
        ("master.renameLayout", "layout") => r#"{"type":"integer","description":"layout (from 0)"}"#,
        ("master.renameLayout", "master") => r#"{"type":"integer","description":"master (from 0)"}"#,
        ("master.deleteLayout", "layout") => r#"{"type":"integer","description":"layout (from 0)"}"#,
        ("master.deleteLayout", "master") => r#"{"type":"integer","description":"master (from 0)"}"#,
        ("master.insertPlaceholder", "kind") => {
            r#"{"type":"string","description":"placeholder kind","enum":["content","text","picture","chart","table","media"]}"#
        }
        ("transition.set", "kind") => {
            r#"{"type":"string","description":"the transition: none, morph, fade, push, wipe, split, reveal, cut, randomBar, shape, uncover, cover, flash, … (transition.list)"}"#
        }
        ("transition.set", "option") => r#"{"type":"string","description":"an effect option the transition offers (transition.list)"}"#,
        ("transition.options", "option") => r#"{"type":"string","description":"an effect option the transition offers (transition.list)"}"#,
        ("transition.timing", "after") => {
            r#"{"oneOf":[{"type":"integer","minimum":0,"maximum":86400000},{"type":"null"}],"description":"advance automatically after this many milliseconds; null for never"}"#
        }
        ("animation.set", "class") => r#"{"type":"string","description":"animation class","enum":["entrance","emphasis","exit","path"]}"#,
        ("animation.add", "class") => r#"{"type":"string","description":"animation class","enum":["entrance","emphasis","exit","path"]}"#,
        ("animation.set", "effect") => {
            r#"{"type":"string","description":"an effect id from animation.list: fade, fly, zoom, wipe, appear, spin, pulse, …"}"#
        }
        ("animation.add", "effect") => r#"{"type":"string","description":"an effect id from animation.list"}"#,
        ("animation.set", "option") => r#"{"type":"string","description":"an effect option from animation.list"}"#,
        ("animation.add", "option") => r#"{"type":"string","description":"an effect option from animation.list"}"#,
        ("animation.options", "option") => r#"{"type":"string","description":"an effect option from animation.list"}"#,
        ("animation.move", "to") => r#"{"type":"integer","description":"the animation position (from 0)"}"#,
        ("animation.timing", "trigger") => {
            r#"{"oneOf":[{"type":"integer"},{"type":"string"},{"type":"null"}],"description":"start on a click on this shape; null for the slide's click order"}"#
        }
        ("animation.clear", "index") => r#"{"type":"integer","description":"the slide (from 0)"}"#,
        ("animation.get", "slide") => r#"{"type":"integer","description":"the slide (from 0)"}"#,
        ("table.style", "style") => r#"{"type":"string","description":"a table style: medium2-accent1, light1-accent2, dark1-tx1, grid, none, …"}"#,
        ("table.insertColumnLeft", "col") => r#"{"type":"integer","description":"column (from 0)"}"#,
        ("table.insertColumnRight", "col") => r#"{"type":"integer","description":"column (from 0)"}"#,
        ("table.deleteColumn", "col") => r#"{"type":"integer","description":"column (from 0)"}"#,
        ("table.columnWidth", "col") => r#"{"type":"integer","description":"column (from 0)"}"#,
        ("table.options", "bandedRows") => r#"{"type":"boolean"}"#,
        ("table.options", "firstColumn") => r#"{"type":"boolean"}"#,
        ("table.options", "headerRow") => r#"{"type":"boolean"}"#,
        ("table.options", "lastColumn") => r#"{"type":"boolean"}"#,
        ("table.options", "totalRow") => r#"{"type":"boolean"}"#,
        ("comment.add", "author") => r#"{"type":"string","description":"the author's principal id"}"#,
        ("comment.add", "created") => r#"{"type":"string","description":"ISO 8601 time"}"#,
        ("comment.add", "id") => {
            r#"{"oneOf":[{"type":"integer"},{"type":"string"}],"description":"a shape id (number, number string or KitDrawing id)"}"#
        }
        ("comment.reply", "author") => r#"{"type":"string","description":"the author's principal id"}"#,
        ("comment.reply", "created") => r#"{"type":"string","description":"ISO 8601 time"}"#,
        ("comment.delete", "comment") => r#"{"type":"string","description":"one comment's id; without it the whole thread"}"#,
        ("comment.delete", "thread") => r#"{"type":"string","description":"the thread id"}"#,
        ("comment.list", "slide") => r#"{"type":"integer","description":"the slide (from 0)"}"#,
        ("comment.anchor", "x") => r#"{"type":"number","description":"a point on the slide, in points from its left"}"#,
        ("comment.anchor", "y") => r#"{"type":"number","description":"a point on the slide, in points from its top"}"#,
        ("comment.place", "anchors") => r#"{"type":"array","description":"comment anchors: {ids, range?}"}"#,
        ("show.setup", "from") => r#"{"type":"integer","description":"first slide (from 1)"}"#,
        ("show.setup", "to") => r#"{"type":"integer","description":"last slide (from 1)"}"#,
        ("show.setup", "noAnimation") => r#"{"type":"boolean"}"#,
        ("show.setup", "noNarration") => r#"{"type":"boolean"}"#,
        ("command.list", "filter") => r#"{"type":"string","description":"a substring of the ids and labels"}"#,
        ("media.seek", "ms") => r#"{"type":"number","description":"milliseconds from the start"}"#,
        ("media.posterFrame", "ms") => r#"{"type":"number","description":"frame time in milliseconds; default: the current position"}"#,
        _ => return None,
    };
    serde_json::from_str(node).ok()
}

/// Objects whose members the command notation leaves untyped, typed whole by `key_type`.
const TYPED_OBJECTS: &[(&str, &str)] =
    &[("file.render", "target"), ("shape.effects", "shadow"), ("shape.effects", "glow"), ("shape.effects", "softEdges")];

/// Gives the untyped keys of a command's schema their types from `key_type`, keeping `required`.
pub fn type_keys(id: &str, schema: &mut Value) {
    let Some(properties) = schema.as_object_mut() else { return };
    // A list of shape or slide ids (`[shape id]`, `[slide id]`): each a number, or a string such
    // as the tool id `shape:<slideId>/<n>` slide_inspect lists.
    if let Some(items) = properties.get_mut("ids").and_then(|node| node.get_mut("items"))
        && items.get("type").and_then(Value::as_str) == Some("json")
    {
        *items = serde_json::json!({ "oneOf": [{ "type": "integer" }, { "type": "string" }] });
    }
    for (key, node) in properties.iter_mut() {
        if node.get("type").and_then(Value::as_str) != Some("json") && !TYPED_OBJECTS.contains(&(id, key.as_str())) {
            continue;
        }
        let Some(mut typed) = key_type(id, key) else { continue };
        if node.get("required") == Some(&Value::Bool(true)) {
            typed["required"] = Value::Bool(true);
        }
        *node = typed;
    }
    // These are the command handlers' clamps, not limits inferred from example decks.
    let limits: &[(&str, f64, f64)] = match id {
        "format.fontSize" => &[("size", 1.0, 4000.0)],
        "format.spacing" => &[("pt", -100.0, 400.0)],
        "format.lineSpacing" => &[("pt", 0.0, 1584.0), ("lines", 0.0, 9.99)],
        "format.columns" => &[("count", 1.0, 16.0)],
        "format.numbering" => &[("start", 1.0, 32767.0)],
        "format.paragraph" => &[("lineSpacing", 0.0, 9.99)],
        "ink.pen" | "ink.pencil" | "ink.highlighter" => &[("width", 0.25, 72.0)],
        "slide.background" => &[("style", 1.0, 12.0)],
        "file.export" | "file.exportBytes" => &[("dpi", 36.0, 600.0), ("scale", 0.01, 16.0)],
        "table.columnWidth" => &[("width", 4.0, 4000.0)],
        "table.rowHeight" => &[("height", 4.0, 4000.0)],
        "table.insert" => &[("rows", 1.0, 75.0), ("cols", 1.0, 75.0)],
        "shape.line" => &[("width", 0.0, 1584.0)],
        "image.adjust" => &[("brightness", -1.0, 1.0), ("contrast", -1.0, 1.0), ("saturation", 0.0, 4.0), ("transparency", 0.0, 1.0)],
        "image.crop" => &[("left", -10.0, 0.95), ("top", -10.0, 0.95), ("right", -10.0, 0.95), ("bottom", -10.0, 0.95)],
        "media.options" => &[("volume", 0.0, 1.0)],
        "animation.set" | "animation.add" | "animation.timing" => {
            &[("duration", 0.0, 600_000.0), ("delay", 0.0, 600_000.0), ("repeat", 0.0, u32::MAX as f64)]
        }
        "transition.set" => &[("duration", 0.0, 59_000.0)],
        "transition.timing" => &[("duration", 0.0, 59_000.0)],
        "slide.size" => &[("w", 72.0, 4032.0), ("h", 72.0, 4032.0)],
        _ => &[],
    };
    for (field, minimum, maximum) in limits {
        if let Some(node) = properties.get_mut(*field).and_then(Value::as_object_mut) {
            node.insert("minimum".into(), serde_json::json!(minimum));
            node.insert("maximum".into(), serde_json::json!(maximum));
        }
    }
}
