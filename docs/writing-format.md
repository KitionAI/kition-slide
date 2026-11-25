# Slide writing format

How a model writes Kition Slide documents: the steps it sends, how it names what it changes,
what a new document starts with, the named values commands accept, and worked examples.
The package ships the short version for models as [agent/guide.md](../agent/guide.md); this file is
the complete reference it is drawn from. Everything below comes from the engine itself
(its 255 commands and their parameter schemas, and what `inspect` reports) and from the guide.

## What a model writes

A model never writes the file bytes. It calls the kit's agent tools (prefix `slide`), and the
engine applies each change, keeps it in undo history and saves the file:

- `slide_inspect`: read the document; `slide_commands`: find commands by keyword;
- `slide_run`: run steps `{command, params}` in order;
- `slide_check`: list problems; `slide_query` / `slide_batch`: read or set objects of the object model.

## Syntax

A step is one engine command with its parameters, as JSON:

```json
{"command": "<command id>", "params": {"<name>": <value>}}
```

`slide_run` takes a list of steps and runs them in order, stopping at the first failure. Parameter
names, types and required fields come from each command's schema; `slide_commands` returns them.
Commands by namespace (the part of the id before the first dot):

| Namespace | Commands | Change the document | Open to the agent | Examples |
| --- | ---: | ---: | ---: | --- |
| `accessibility` | 1 | 0 | 1 | `accessibility.check` |
| `actionButton` | 1 | 1 | 1 | `actionButton.insert` |
| `animation` | 11 | 8 | 11 | `animation.set`, `animation.add`, `animation.remove` |
| `arrange` | 15 | 15 | 15 | `arrange.bringToFront`, `arrange.bringForward`, `arrange.sendBackward` |
| `artText` | 1 | 1 | 1 | `artText.insert` |
| `audio` | 1 | 1 | 1 | `audio.insert` |
| `chart` | 4 | 4 | 4 | `chart.insert`, `chart.type`, `chart.data` |
| `command` | 1 | 0 | 1 | `command.list` |
| `comment` | 8 | 4 | 8 | `comment.add`, `comment.reply`, `comment.delete` |
| `drawing` | 14 | 13 | 14 | `drawing.describe`, `drawing.insert`, `drawing.delete` |
| `edit` | 11 | 7 | 11 | `edit.undo`, `edit.redo`, `edit.cut` |
| `field` | 2 | 2 | 2 | `field.insertSlideNumber`, `field.insertDateTime` |
| `file` | 18 | 11 | 18 | `file.new`, `file.open`, `file.openBytes` |
| `format` | 40 | 38 | 40 | `format.bold`, `format.italic`, `format.underline` |
| `image` | 5 | 5 | 5 | `image.insert`, `image.crop`, `image.adjust` |
| `ink` | 6 | 0 | 6 | `ink.pen`, `ink.pencil`, `ink.highlighter` |
| `link` | 1 | 1 | 1 | `link.insert` |
| `master` | 4 | 4 | 4 | `master.insertLayout`, `master.renameLayout`, `master.deleteLayout` |
| `media` | 9 | 2 | 9 | `media.options`, `media.play`, `media.pause` |
| `presence` | 2 | 0 | 2 | `presence.selection`, `presence.locate` |
| `section` | 5 | 5 | 5 | `section.add`, `section.rename`, `section.remove` |
| `shape` | 24 | 21 | 24 | `shape.insert`, `shape.presets`, `shape.connect` |
| `show` | 4 | 2 | 4 | `show.fromStart`, `show.fromCurrent`, `show.setup` |
| `slide` | 21 | 14 | 21 | `slide.new`, `slide.duplicate`, `slide.delete` |
| `smartGraphic` | 1 | 1 | 1 | `smartGraphic.insert` |
| `spelling` | 1 | 0 | 1 | `spelling.check` |
| `symbol` | 1 | 1 | 1 | `symbol.insert` |
| `table` | 21 | 19 | 21 | `table.insert`, `table.state`, `table.cellFormat` |
| `text` | 11 | 4 | 11 | `text.edit`, `text.exit`, `text.insert` |
| `textBox` | 1 | 1 | 1 | `textBox.insert` |
| `theme` | 4 | 3 | 4 | `theme.set`, `theme.list`, `theme.colors` |
| `transition` | 5 | 4 | 5 | `transition.set`, `transition.options`, `transition.timing` |
| `video` | 1 | 1 | 1 | `video.insert` |
| `view` | 2 | 0 | 2 | `view.slideMaster`, `view.closeMaster` |
| `window` | 1 | 0 | 1 | `window.next` |

### The document

- A file has **slides**; each slide uses a **layout** of a **master** (`slide_inspect` lists
  them: "Title Slide", "Title and Content", "Blank", …). A slide holds **shapes**: text boxes,
  placeholders (title, body, …), preset shapes, pictures, tables, charts, media, groups.
- Coordinates are **points** on the slide: `x` grows right, `y` grows **down**, `(0, 0)` is the
  slide's top-left. A 16:9 slide is 960 × 540 pt. A shape's `box` is `[x, y, width, height]`.
- Colours are `"#RRGGBB"` or theme slots (`accent1`…`accent6`, `tx1`, `bg1`, …).

### Addressing: read this first

Most failed calls come from addressing a shape on the wrong slide. The rules:

1. **Shape ids are numbers, and only mean something on their own slide.** Read them from
   `slide_inspect {path, target: {slide: N}}` → `part.shapes[].id`, or from the result of the
   command that made the shape (`textBox.insert` → `{id}`). The same number on another slide is
   another shape, or none.
2. **Commands edit the current slide.** Three ways to make an id hit its slide; use one every
   time you edit off the first slide:
   - pass **`onSlide: N`** (counted from 1) in the command's params, as in
     `text.set {id: 263, text: "…", onSlide: 2}`; every command that takes shape `ids` or `id`
     accepts it, and it is the safest;
   - or pass the same `target: {slide: N}` to `slide_run` that you gave `slide_inspect`;
   - or run `slide.go {index: N-1}` as the first step.
3. **Two counting bases.** `target.slide`, `onSlide` and diagnostics' `target.slide` count
   from **1**. Command parameters named `index`, `at`, `from`, `to` count from **0** (the
   schema says "a position counted from 0"). Slide 2 is `index: 1`.
4. **Slides have ids too** (`part.id` in `slide_inspect`, `{id}` from `slide.new`). To delete
   several slides, pass their ids at once, `slide.delete {ids: [262, 270]}`: indexes shift
   after each deletion, so deleting by index one after another removes the wrong slides.
5. **Object-model ids** (`slide_query`, `slide_batch`) are strings; the tools also accept the
   shape numbers. Don't mix them into command params that the schema types as numbers.
6. `slide_run` steps can't refer to an earlier step's result. Make the shapes in one call, read
   their ids from `completed[].result`, then use them in the next call.

## Text writing format

kition-sdk `docs/platform/data-model.md` (section 9.3) plans a Marp-like format that the kit compiles
into document operations. The engine has none yet, so a model writes through the commands above.

## Defaults

What a new, empty document starts with:

- File name `Untitled slide.kslide`, size 960 × 540 (`kit.json` `creates`).

What `slide_inspect` reports for a new document made at that size (generated from the engine; lists shortened to 20 entries):

```json
{
  "canRedo": false,
  "canUndo": false,
  "editingText": false,
  "masters": [
    {
      "index": 0,
      "layouts": [
        {
          "index": 0,
          "name": "Title Slide"
        },
        {
          "index": 1,
          "name": "Title and Content"
        },
        {
          "index": 2,
          "name": "Section Header"
        },
        {
          "index": 3,
          "name": "Two Content"
        },
        {
          "index": 4,
          "name": "Comparison"
        },
        {
          "index": 5,
          "name": "Title Only"
        },
        {
          "index": 6,
          "name": "Blank"
        },
        {
          "index": 7,
          "name": "Content with Caption"
        },
        {
          "index": 8,
          "name": "Picture with Caption"
        },
        {
          "index": 9,
          "name": "Title and Vertical Text"
        },
        {
          "index": 10,
          "name": "Vertical Title and Text"
        }
      ],
      "name": "Kition Slide"
    }
  ],
  "selectedKinds": [],
  "selection": 0,
  "slide": 0,
  "slideSize": {
    "height": 540,
    "width": 960
  },
  "slides": [
    {
      "area": {
        "height": 540,
        "width": 960,
        "x": 40,
        "y": 40
      },
      "index": 0
    }
  ],
  "target": {
    "kind": "slides"
  },
  "title": "SlideFile1"
}
```

## Themes, layouts and other named values

Parameters that take one of a fixed list of names (themes, layouts, presets, styles, kinds, modes,
units and similar), from the command schemas. A parameter not listed here takes free values; its
schema says which.

| Parameter | Values | Used by |
| --- | --- | --- |
| `align` | `left`, `center`, `right`, `justify`, `justifyLow`, `distributed` | `format.align`, `format.paragraph` |
| `anchor` | `top`, `middle`, `bottom` | `format.anchor` |
| `anchor` | `top`, `center`, `bottom` | `table.cellFormat` |
| `cap` | `flat`, `round`, `square` | `shape.line` |
| `caps` | `none`, `small`, `all` | `format.caps` |
| `format` | `kslide`, `pptx` | `file.save`, `file.saveAs`, `file.saveBytes` |
| `format` | `png`, `jpeg`, `pptx`, `kslide`, `outline`, `pdf` | `file.export` |
| `join` | `round`, `bevel`, `miter` | `shape.line` |
| `kind` | `list`, `process`, `cycle`, `hierarchy`, `pyramid`, `matrix` | `smartGraphic.insert` |
| `kind` | `back`, `forward`, `beginning`, `end`, `home`, `information`, `return`, `movie`, `document`, `sound`, `help`, `custom` | `actionButton.insert` |
| `kind` | `shape`, `textBox`, `chart` | `drawing.insert` |
| `kind` | `content`, `text`, `picture`, `chart`, `table`, `media` | `master.insertPlaceholder` |
| `mode` | `sentence`, `lower`, `upper`, `title`, `toggle` | `format.case` |
| `mode` | `none`, `shrink`, `resize` | `format.autofit` |
| `preset` | `straightConnector1`, `bentConnector3`, `curvedConnector3` | `drawing.connect`, `shape.connect` |
| `style` | `sng`, `dbl`, `heavy`, `dotted`, `dash`, `wavy` | `format.underline` |
| `type` | `speaker`, `browsed`, `kiosk` | `show.setup` |

## Examples

### Example 1: add a results slide and colour its title

Start from a one-slide file `Deck.kslide`.

```json
slide_run {"path": "Deck.kslide", "steps": [
  {"command": "slide.new", "params": {"layout": "titleAndContent", "title": "Results", "body": "Revenue up 12%\nCosts down 3%"}}]}
```

→ `completed[0].result` is `{"id": 262, "index": 1}`: the slide's id and its 0-based index.

```json
slide_inspect {"path": "Deck.kslide", "target": {"slide": 2}}
```

→ `part.shapes`: `{"id": 263, "name": "Title 1", "placeholder": "title", "text": "Results"}`
and `{"id": 264, "placeholder": "obj", "text": "Revenue up 12%\nCosts down 3%"}`.

Even if another slide is current by now, `onSlide` puts the edit on slide 2:

```json
slide_run {"path": "Deck.kslide", "steps": [
  {"command": "format.color", "params": {"ids": [263], "onSlide": 2, "color": "#C00000"}}]}
slide_check {"path": "Deck.kslide"}
```

→ `{"diagnostics": []}`.

### Example 2: fix what the check reports

`Pitch.kslide` has a rectangle someone dragged off the slide.

```json
slide_check {"path": "Pitch.kslide"}
```

→

```json
{"diagnostics": [{"code": "slide.off-slide", "severity": "warning",
  "message": "\"Rectangle 3\" lies entirely outside the slide, so the show never displays it.",
  "target": {"slide": 1, "path": "shape:262"},
  "fix": {"command": "shape.move", "params": {"ids": [262], "onSlide": 1, "x": 880.0, "y": 60.0}}}]}
```

Run the fix as it is (the person asked to clean up the slide), then check again:

```json
slide_run {"path": "Pitch.kslide", "steps": [
  {"command": "shape.move", "params": {"ids": [262], "onSlide": 1, "x": 880.0, "y": 60.0}}]}
slide_check {"path": "Pitch.kslide"}
```

→ `{"diagnostics": []}`.

If a call answers `no shape `999` where this edits`, the id is not on the slide being edited:
inspect the right slide (`target: {slide: N}`) and pass `onSlide: N`.

## Workflow

1. `slide_inspect {path}`: the slides, layouts and size. Then `slide_inspect {path, target:
   {slide: N}}` lists that slide's shapes with `id`, `kind`, `box`, `text`, `fill`, and makes
   it the current slide.
2. `slide_commands {path, query}`: find commands by keywords (`"set text"`, `"delete slide"`,
   `"font color"`, `"new slide"`). Use the exact `id` returned; each comes with its parameter
   schema. An unknown id fails and names the nearest commands (`slide.goto` → "did you mean
   `slide.go`"). Never guess a command id.
3. `slide_run {path, steps: [{command, params}]}`: a few steps at a time. Steps run in order; a
   failing step stops the call and the kit undoes the steps before it, so the file is as it was
   before the call and nothing is saved: read `error.index` for the failing step, fix it, and send
   the whole run again. A successful call saves the file.
4. `slide_check {path}`: fix what it reports, then check again until it is clean.
5. `slide_inspect {path, target: {slide: N}}` again to confirm the text and boxes.

Useful commands:

| Need | Command |
|---|---|
| New slide with title and body | `slide.new {layout: titleAndContent, title, body}` (body `\n` = new paragraph) |
| Replace a shape's text | `text.set {id, text, onSlide}` (keeps the first run's formatting) |
| Text box | `textBox.insert {rect: [x, y, w, h], text}` |
| Shape | `shape.insert {preset: rect\|ellipse\|…, rect: [x, y, w, h]}` |
| Move or size shapes | `shape.move {ids, x, y, onSlide}` (absolute) or `{ids, dx, dy}`; `shape.setBounds {ids, x, y, w, h}` |
| Font, size, colour, bold | `format.fontName {ids, family}`, `format.fontSize {ids, size}`, `format.color {ids, color}`, `format.bold {ids, on}` |
| Fill | `shape.fill {ids, color}` |
| Delete shapes | `shape.delete {ids, onSlide}` |
| Delete slides | `slide.delete {ids: [slide id, …]}` |
| Picture | `image.insert {data: base64, name, rect}` |
| Theme | `theme.set {name}` (names from `theme.list`) |

Formatting commands with `ids` format the whole text of those shapes; no text selection is
needed. There is no headless text selection, so to give part of a text another look, put that
part in its own shape. A run `fill` set through the object model's text marks is accepted but
not drawn or saved: use `format.color {ids}` on a shape instead.

## What the check reports

| Code | Severity | What to do |
|---|---|---|
| `slide.missing-media` | error | The picture or clip has no bytes: insert it again or delete the shape |
| `slide.unreadable-picture` | error | The bytes are not a picture: insert a PNG, JPEG, GIF, WebP, BMP, TIFF or SVG |
| `slide.text-overflow` | warning | `fix`: `format.autofit {ids, onSlide, mode: shrink}`; or shorten the text, or enlarge the box |
| `slide.effects-dropped` | warning | Reduce the shadow, glow, soft edge or reflection size |
| `slide.font-substituted` | warning / info | Choose an installed family with `format.fontName {ids, family}` (info: a same-metrics stand-in) |
| `slide.off-slide` | warning | `fix`: `shape.move {ids, onSlide, x, y}` puts it back on the slide; or delete it |
| `slide.empty-placeholder` | info | Fill it with `text.set`, or `fix`: `edit.delete {ids, onSlide}` |
| `slide.missing-alt-text` | info | Describe the picture: `shape.altText {ids, onSlide, text}` (or `decorative: true`) |

`slide.render-failed` (error) means the renderer could not draw a slide: simplify what was last
changed on it. A `fix` is a suggestion: apply it only when it is what the person asked for.
