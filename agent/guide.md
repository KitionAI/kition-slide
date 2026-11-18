# Slide kit

The Slide kit opens, edits and checks presentations (`.kslide`, and `.pptx` both ways). An agent can
add and delete slides, choose layouts and themes, write and format text, insert shapes, pictures,
tables and charts, move and size shapes, check the slides for missing media, overflowing text and
shapes off the slide, and export the presentation to PDF, PNG or JPEG.

## Tools

The kit's prefix is `slide`. Its tools are:

| Tool | Does |
|---|---|
| `slide_inspect` | The presentation as JSON (slides, layouts and size), or one slide's shapes, with a preview |
| `slide_query` | Reads objects of the presentation's object model by id, or by following fields from the root |
| `slide_commands` | Finds commands by keywords, each with its parameter schema |
| `slide_run` | Runs a list of command steps |
| `slide_batch` | Runs one batch of object-model operations |
| `slide_check` | What is wrong with the presentation, as diagnostics |
| `slide_create` | Makes a new `.kslide` presentation |
| `slide_export` | Writes the presentation to a new PDF, PNG or JPEG file |
| `slide_draft_ready`, `slide_draft_reopen` | Asks the person to review your draft, or takes it back to edit |

Every tool takes `path`, the presentation's path in the workspace. With `draft: true` your edits go
to your draft of the presentation and leave the file as it is; only the person merges or discards a
draft.

## Addressing and coordinates

Most failed calls come from addressing a shape on the wrong slide. The rules:

- Coordinates are **points** on the slide: `x` grows right, `y` grows **down**, and `(0, 0)` is the
  slide's top-left corner. A 16:9 slide is 960 × 540 pt. A shape's `box` is `[x, y, width, height]`.
- Colours are `"#RRGGBB"` or theme slots (`accent1`…`accent6`, `tx1`, `bg1`, …).
- **Two counting bases.** Slides named in the call count from **1**: a run's `target.slide`, a
  step's `at.part.slide`, `onSlide`, `slide_inspect`'s `target.slide`, `file.render`'s
  `target.slide`, and diagnostics' `target.slide`. Command parameters that are positions count
  from **0** (the schema says "a position counted from 0"): `index`, `slide` (as in
  [`comment.add`](/kits/slide/commands/comment.add/) and [`file.render`](/kits/slide/commands/file.render/)),
  `from` and `to` of `slide.move`, and `section.add`'s `at`. Slide 2 is `index: 1`. The one
  exception, [`show.setup`](/kits/slide/commands/show.setup/)'s `from` and `to`, counts from 1, as
  the slide show's dialog does.
- **One id space: use the `toolId`s slide_inspect lists.** `slide_inspect {path, target: {slide:
  N}}` lists each shape with `toolId` (`"shape:<identity>"`, a 32-digit lowercase creation GUID) beside its
  number on the slide (`id`, `263`), and the slide with `toolId` (`"slide:257"`) beside its `id`.
  A tool id names one shape in the whole deck and stays the same when the file is saved and opened
  again, across moves to another slide, grouping and ungrouping. Copies get fresh identities.
  Decks without identities get deterministic ones on open, persisted on their first save in
  DrawingML `a16:creationId`. The number means something only on its own
  slide (the same number on another slide is another shape, or none). Every tool takes tool ids:
  command params `id` and `ids`, a step's `on`, `slide_query` and `slide_batch`; results, receipts,
  `others` and `conflicts` name objects by them. A shape's tool id finds its current slide, so it needs no
  `onSlide` or `at.part`: [`text.set`](/kits/slide/commands/text.set/) `{id: "shape:6a4063de16985dbea293b1b9f1dfdf5a",
  text: "…"}`. Shapes of two slides in one step are refused. Commands that make a shape answer its
  number (`{id}`); read its tool id from the receipt's `changes.created` or from slide_inspect.
  Connector endpoints (`from`, `to`) and animation `trigger` also accept shape tool ids; a numeric
  endpoint beside a tool id is resolved on the slide that tool id names.
  The `shape:<slide>/<number>` form and bare numbers are also accepted command inputs, as
  positional addresses: shape number N on slide S. They do not follow a shape across moves or
  renumbering. With `basis`, a write by such an address is refused as `relocated` when the shape
  it named then was moved to another slide or renumbered, or another shape holds the address now;
  a `shape:<slide>/<number>` that names no shape at all now is refused by the command (its error
  says so). Outputs never use the positional form. Hold the persistent identities.
- **Name every call's slide and objects.** Commands with omitted targets can read the current
  slide or selection. The host restores the person's editor state after the call. Name what each
  edit acts on:
  - by tool ids, which name their slide (above);
  - or with numbers, in the command's params, **`onSlide: N`** (counted from 1), as in
    [`text.set`](/kits/slide/commands/text.set/) `{id: 263, text: "…", onSlide: 2}`; every command
    that takes shape `ids` or `id` accepts it;
  - or for the whole run, `slide_run`'s **`target: {slide: N}`** (or `{slideId}`);
  - or for one step, the step's **`at: {part: {slide: N}}`** (or `{slideId}`), which wins over the
    run's `target`.

  A step is `{command, params?, on?, at?}`; any other key is refused. **`on`** lists the ids of the
  shapes the step acts on, as `slide_inspect` lists them: it fills the command's `id` or `ids`
  (give them once, in `on` or in `params`), or, for a command that acts on the selected shapes,
  selects exactly those shapes for that step. Numbers in `on` need a slide (`at.part` or the run's
  `target`), since they repeat across slides; an id the slide lacks is an error that names the
  slide. [`slide.go`](/kits/slide/commands/slide.go/) still moves the editor, but only until the
  call ends. A call that names no slide acts on the current one, the person's (in a file no editor
  has open, the slide it opened on), so name it every time. While the file is open in its editor,
  `target`, `on` and `at` are refused: use tool ids or `onSlide`.
- **Slides have ids too** (`part.id` and `part.toolId` in `slide_inspect`, `{id}` from
  [`slide.new`](/kits/slide/commands/slide.new/)). To delete several slides, pass their ids at
  once, [`slide.delete`](/kits/slide/commands/slide.delete/) `{ids: ["slide:262", "slide:270"]}`:
  indexes shift after each deletion, so deleting by index one after another removes the wrong
  slides.
- **Text, tables and charts are parts of their shape** (a slide's notes, of its slide): in the
  object model a shape's text body, table and chart, and their paragraphs, rows and columns, keep
  object ids (strings) that last only while the server has the file open; layouts, masters and
  media too. Receipts, `others` and `conflicts` name a change inside a shape by the shape's tool id.

## What a call changed, and what others changed

- **Receipts.** A successful `slide_run` or `slide_batch` answers `changes`: `created`, `changed`
  (with `edits`, each field `before` and `after`), `moved` (`from`, `to`) and `deleted` (with `at`,
  where it was), by tool ids, and `diagnostics: {new, resolved}`. Equal edits of several objects
  may be compressed as `footprint: [{ids: [...], edits: [{field, by}]}]`; `by` is the numerical
  change. Request `changes: "all"` for each object's full before and after values. You do not need to inspect again
  to see what a run changed. A change inside a shape is listed under the shape, at the path to
  what changed: a text edit is `{"id": "shape:6a4063de16985dbea293b1b9f1dfdf5a", "type": "Shape", "fields":
  ["body.paragraphs.0.text"], "edits": [{"field": "body.paragraphs.0.text", "before": "Title",
  "after": "Results"}]}`; a slide's notes are listed under the slide (`notesBody.paragraphs.0`).
- **Pass the latest `revision` as `basis`.** Every answer of inspect, query, run and batch has a
  short `revision`. Pass the last one you were given as `basis` on your next call: the answer then
  says what others (people, other agents, changes made in the file) changed since, in `others`
  (with `by`).
- **A `conflict`** (retryable) means others changed what the write acts on since your `basis`: a
  target or its slide was `deleted`, `relocated` (moved, or other slides inserted or deleted before
  it, so the slide number you gave now names another slide; the conflict names that slide's tool
  id and `now` its number now), or `changed` (another edit of a held field),
  or `unknown` (a change in the file the server cannot describe object by object). Nothing was
  run. Read `others` and `conflicts`, inspect again if you need to, decide again, and send the call
  with the `revision` the refusal gives as `basis`. Do not undo others' changes. Changes outside the held dependencies allow your write; another shape can be a dependency
  through a connector, group, layout, theme or copied value. Some holds remain broad (below).
  Your own earlier edits do not count as changes by others.
- **After a change made in the file** (another program saved it), your target and its slide are
  followed through the change: a write on a shape the other program edited (its text included) is
  refused as `changed`, a slide number that now names another slide as `relocated`, a shape or
  slide it deleted as `deleted`, and a write on a shape or slide it did not touch applies.
  `others` names what the other program changed and nothing else, as receipts do: a text edit by
  the shape and the path to its text, a slide it added or duplicated under `created`, a slide's
  new layout under the slide, a table cell's text as one edit under the table's shape
  (`table.cells.<row>:<col>.body.paragraphs.0.text`; the key is not an id you can use), and a
  replaced picture image counted in `unnamed` (`{"Media": {"changed": 1}}`). A file saved again
  unchanged is no change. If another program numbers shapes again while preserving their stored
  identities, writes through those identities still reach the same shapes. A stale positional
  address is refused; inspect the slide to obtain its current address or persistent tool id.
  Decks of any size are compared object by object (on a deck of a thousand slides a write after a
  change made in the file takes some seconds longer). What your own writes leave in the deck is
  what its saved file holds (sizes, rotations and shrink-to-fit scales as the file rounds them), so
  your own edits never come back as changes by others. Known exceptions, which name shapes you did
  not touch in `others` after a change made in the file: some imported chart/media relationships,
  animation targets, transition XML and font metadata do not survive save/reopen unchanged.
  Text-run ids are paired by the SDK when comparing a reread. A cross-slide shape move is a move
  in both its receipt and the saved file (its `address`, slide and number, changes; nothing is
  deleted or created), and its animations and the comments anchored on it go with it; continue
  writing through the held persistent identity.
- **Name a step's target by its id.** The server checks every object a step names, in `on`, in
  the command's targets parameter and other declared object parameters (a shape given as `id`
  to a command whose targets parameter is `ids`, a connector's ends). Integer parameters are
  object ids only when the completed schema marks them; a scanner checks every command.
  Numbers local to a slide are mapped to its shape tool ids. A parameter the command does not
  take is refused as `invalid-params`, naming it, before anything else. A step acting on the slide
  named by `at.part` checks its own fields, including its layout. `onSlide` holds
  the slide's location; a named shape holds the fields the operation consumes. A slide command
  that names its slide by its place (`index`, a move's `from` and `to`, a new slide's `at`) has as
  premise that the slide there is still the one that was: after others inserted, deleted or moved
  slides before it, it is refused as `relocated`, the conflict giving `place` (the place as you
  named it, with `under`, the slide it names now). When `slide.delete` gives `ids`, an unused
  `index` is not a premise. `comment.add` scopes its numeric anchor to its own `slide` index
  (counted from 0), which is also a place premise. Animation timing, options and removal by
  `index` hold that animation; reordering holds the animation list. Another animation's timing
  can change independently, but moving an animation into the addressed index is refused.
  Animation operations selecting shapes by id hold the whole animation list, including newly
  added animations. This can also refuse an edit after another shape's animation changed.
  Retry with the refusal's `revision` after inspecting.

### Handler Readings And Guard Limits

The handlers and engine operations behind all **193** advertised writes have been read, including
`text.edit`'s stored-body creation path; static metadata rows also cover 29 editor/query
commands. The readings live in `bindings/wasm/tests/declarations_independent.rs` and its family
files (`bindings/wasm/tests/declarations_independent/`). Its dynamic comparison covers **802 call
paths over the 193 write commands** and finds no command whose declared premises differ from
what its handler reads (`DIFFERING_FLOOR` is zero); static metadata equality passes too. Some
paths and editor-state facts are pinned by review (their row comments say why); they are
not counted as blind readings. This comparison does not prove every conceivable call path.

What writes hold depends on the call path:

- File replacement and history commands hold the deck whole; properties hold only replaced
  properties. Slide creation/copy/deletion holds the shapes, threads, layouts and section
  membership it consumes; indexed slide operations hold their addressed places. Slide-list
  traversal uses `~slideOrder`, independently of slide names/content.
- Text/format writes hold the relevant body paragraphs/properties, table cells or chart member.
  Toggles and fit/refit paths add inherited defaults, placeholder search inputs, theme fonts,
  geometry and frames when read. Table grid operations hold rows, columns, cells and frame;
  style/options paths hold the properties they replace. Selection fallbacks are represented.
- Geometry/group/order writes hold consumed target fields, ancestor frames, affected siblings,
  rerouted connectors and their other endpoints, order neighbours, animations, counts or slide
  size as needed. Picture operations hold consumed picture fields and reused media bytes;
  chart operations hold the affected `chart.parts` members. Clipboard paths hold the source
  facts they copy, while the session buffer itself remains a limit below.
- Themes/design/masters hold changed properties and refit inputs. Transition propagation holds
  its source and destination slide list; animation paths hold the addressed animation or list.
  Comment ordinal paths hold thread identity and addressed slide position; anchored additions
  hold anchor identity/existence and its address when positional, allowing content-only edits.
  Fields and hyperlinks add footer/layout inputs, edited text inputs, destination place and
  destination title only on paths that use it. Poster extraction holds
  the referenced clip bytes.

At the public SDK (`f5ea478`, 0.1.0) the kit reconciles a reread of its saved file: a document
with no history takes the file as read; one with history records the file's difference outside
both undo histories, so an agent's undo takes back only its own step. When another program
renumbered shapes in a session with history, the reread is refused rather than pair shapes by
number. When persisted layout identities and their order are unchanged, history rereads retain
the local part numbers; a changed layout set or order is refused safely.
Raw `slide_batch` operations declare their premise contract (`batchOps`), including positional
paragraph and table creates and list edits. Without `basis` they apply under their ordinary
validation. With `basis`, nested paragraph and table gaps refuse `relocated` when another writer
shifted the position, and later changes apply. Table rows and columns have immutable identities
within their table; they survive saves, edits and copies in their owning table. Sections use their
standard saved GUID. Layout references use `layout:<identity>`, persisted in the layout's own
extension, independently of part numbers and names. Missing or duplicated import metadata is
repaired deterministically; continuity through an editor that strips it cannot be proven.
A paragraph named by its id after an outside insertion still names that paragraph.

Picture insertion holds the historical media selected by its bytes, even when another writer
changed that media so it no longer matches. Inherited layout text style is held directly. A new
slide holds layout membership and placeholder inputs without holding ordinary layout decoration
text. Guard-only derived facts stay private in receipts while retaining their comparisons.
An isolated premise plan can follow a raw creation before its later insertion into a slide or
shape list; the incomplete plan cannot save, and actual batches still require stored owners.

A raw text offset refuses `relocated` when another writer shifted the text before it, and a
later text edit applies. A section move whose position shifted refuses `relocated`. A chart write
applies after another writer changed only an unrelated chart part (its title). A raw `list.move`
takes its `to` in pre-removal coordinates, as its guard and the platform describe. A raw
positional `list.remove` of a paragraph without `basis` applies: an object a batch takes out of
every container (and inserts nowhere else) leaves the deck with what it owns, while an object a
batch creates and places nowhere is still refused.

Still open: command table and animation operations can refuse unrelated later edits. A history
reread keeps the local number of media whose part and bytes are unchanged. Slide creation with an
owned notes body replays. Closing returns `closed:true`, stops guarded and ordinary runs, marks
later steps `notRun`, and does not save.

Clipboard/painter buffers, last-used styling, preferences, history and media clock are not all
in the editor snapshot. Group reparenting can change ancestry without changing a held ancestor
frame. Default styles, inherited style JSON, group children, animation lists and section
membership still have broad holds that can refuse independent edits. Within the shared
ancestor-body helper, the narrowing to ancestor `body.props` is limited to reviewed clipboard paths. `shape.resize`, `shape.setBounds`, `shape.adjust`
and `shape.change` retain their matrix's conservative whole ancestor-body holds. Deletion
protects whole removed objects. The 802 structural paths do not provide complete
outside-file behavior coverage or prove every conceivable call path. The mixed table
(seed `0x9e3779b97f4a7c15`, rules unchanged) has no false refusals and no wrong writes; its
narrow scenario set does not close these gaps.

## Workflow

1. `slide_inspect {path}`: the slides, layouts and size. Then `slide_inspect {path, target:
   {slide: N}}` lists that slide's shapes with `toolId`, `id`, `kind`, `box`, `text` and `fill`; with
   `preview: true` it draws that slide (without a `target`, the first slide). Reading moves
   nothing: name slide N again in the calls that edit it.
2. `slide_commands {path, query}`: find commands by keywords (`"set text"`, `"delete slide"`,
   `"font color"`, `"new slide"`). Use the exact `id` it returns and follow its parameter schema;
   never guess a command id. An unknown id fails and names the nearest commands (`slide.goto` →
   "did you mean `slide.go`").
3. `slide_run {path, steps: [{command, params}]}` a few steps at a time. Steps run in order; a
   failing step stops the call and the kit undoes the steps before it, so the file is as it was
   before the call and nothing is saved: read `error.index` for the failing step, fix it, and send
   the whole run again. A successful call saves the file. Pass the last `revision` you were given
   as `basis`, and read the answer's `changes` (what it changed) and `others` (what others changed).
4. `slide_check {path}`: fix what it reports, then check again until it is clean.
5. `slide_inspect {path, target: {slide: N}}` again to confirm the text and boxes.

## Common tasks

| Need | Command and parameters |
|---|---|
| New slide with title and body | [`slide.new`](/kits/slide/commands/slide.new/) `{layout: titleAndContent, title, body}` (`\n` in `body` starts a new paragraph) |
| Replace a shape's text | [`text.set`](/kits/slide/commands/text.set/) `{id, text, onSlide}` (keeps the first run's formatting) |
| Text box | [`insert.textBox`](/kits/slide/commands/insert.textBox/) `{rect: [x, y, w, h], text}` |
| Shape | [`shape.insert`](/kits/slide/commands/shape.insert/) `{preset: rect\|ellipse\|…, rect: [x, y, w, h]}` |
| Move shapes | [`shape.move`](/kits/slide/commands/shape.move/) `{ids, x, y, onSlide}` (absolute), `{ids, dx, dy}`, or `{ids, toSlide: "slide:262"}` across slides |
| Size shapes | [`shape.setBounds`](/kits/slide/commands/shape.setBounds/) `{ids, x, y, w, h}` |
| Font | [`format.font`](/kits/slide/commands/format.font/) `{ids, family}` |
| Size | [`format.size`](/kits/slide/commands/format.size/) `{ids, size}` |
| Colour | [`format.color`](/kits/slide/commands/format.color/) `{ids, color}` |
| Bold | [`format.bold`](/kits/slide/commands/format.bold/) `{ids, on}` |
| Fill | [`shape.fill`](/kits/slide/commands/shape.fill/) `{ids, color}` |
| Delete shapes | [`shape.delete`](/kits/slide/commands/shape.delete/) `{ids, onSlide}` |
| Delete slides | [`slide.delete`](/kits/slide/commands/slide.delete/) `{ids: [slide id, …]}` |
| Picture | [`insert.picture`](/kits/slide/commands/insert.picture/) `{path}` (in the workspace) or `{data: base64, name, rect}` |
| Theme | [`design.theme`](/kits/slide/commands/design.theme/) `{name}` (names from [`design.themes`](/kits/slide/commands/design.themes/)) |

Formatting commands with `ids` format the whole text of those shapes; no text selection is needed.

The kit reads and writes files only in the workspace, through the host: `insert.picture`,
`picture.change`, `insert.audio` and `insert.video` take a `path` in the workspace (relative to the
deck's folder) or base64 `data`; a path outside the workspace is refused. The host opens, saves and
exports decks; `file.save`, `file.export` and the other commands that name a file are not offered.
A picture a deck links (rather than holds) is never read.

The kit draws text only with its own fonts, whatever this machine has installed: a family it lacks
is drawn with a substitute, which `slide_check` reports as `slide.font-substituted`.

## Example 1: add a results slide and colour its title

Start from a one-slide file `Deck.kslide`.

```json
slide_run {"path": "Deck.kslide", "steps": [
  {"command": "slide.new", "params": {"layout": "titleAndContent", "title": "Results", "body": "Revenue up 12%\nCosts down 3%"}}]}
```

→ `completed[0].result` is `{"id": 262, "index": 1}`: the slide's id and its index, counted from 0.

```json
slide_inspect {"path": "Deck.kslide", "target": {"slide": 2}}
```

→ `part.toolId` is `"slide:262"`; `part.shapes`: `{"toolId": "shape:6a4063de16985dbea293b1b9f1dfdf5a", "id": 263, "name":
"Title 1", "placeholder": "title", "text": "Results"}` and `{"toolId": "shape:528612fa19b55569b1c6c6ec1858ca8d", "id": 264,
"placeholder": "obj", "text": "Revenue up 12%\nCosts down 3%"}`; and a `revision`, say `"qx7"`.

The tool id names the shape on slide 2 whatever slide is current by now:

```json
slide_run {"path": "Deck.kslide", "basis": "qx7", "steps": [
  {"command": "format.color", "params": {"ids": ["shape:6a4063de16985dbea293b1b9f1dfdf5a"], "color": "#C00000"}}]}
```

→ `changes` lists what the run changed, with each field before and after, and `revision` is the
`basis` to pass next.
With the number instead, `onSlide` puts the edit on slide 2:

```json
slide_run {"path": "Deck.kslide", "steps": [
  {"command": "format.color", "params": {"ids": [263], "onSlide": 2, "color": "#C00000"}}]}
slide_check {"path": "Deck.kslide"}
```

→ `{"diagnostics": []}`.

## Example 2: format three shapes on slide 5 with `on`

`slide_inspect {"path": "Deck.kslide", "target": {"slide": 5}}` listed the three text boxes
`{"id": 4}`, `{"id": 5}` and `{"id": 6}` on slide 5. [`format.color`](/kits/slide/commands/format.color/)
colours the selected shapes' text, so `on` selects them for the step:

```json
slide_run {"path": "Deck.kslide", "steps": [
  {"command": "format.color", "on": [4, 5, 6], "at": {"part": {"slide": 5}}, "params": {"color": "#C00000"}}]}
```

→ the three shapes on slide 5 are red; shapes 4, 5 and 6 of other slides, and the person's
current slide and selection, are as they were. Without `at` (or the run's `target`) the step is
refused: `on` needs the slide its ids are on.

## Example 3: edit shapes on two slides in one run

In `Deck.pptx`, saved by another program, slides 2 and 3 both have a title with id `263`. Each
step names its own slide:

```json
slide_run {"path": "Deck.pptx", "steps": [
  {"command": "text.set", "at": {"part": {"slide": 2}}, "params": {"id": 263, "text": "Results"}},
  {"command": "text.set", "at": {"part": {"slide": 3}}, "params": {"id": 263, "text": "Next steps"}},
  {"command": "format.bold", "on": [263], "at": {"part": {"slide": 3}}, "params": {"on": true}}]}
```

→ slide 2's title reads "Results", slide 3's "Next steps" in bold. (`params.on` is
`format.bold`'s own switch; the step's `on` lists shapes.)

## Example 4: fix what the check reports

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

## Pitfalls

- **The wrong slide:** if a call answers ``no shape `999` where this edits`` or
  ``slide 2 has no shape `999` ``, the id is not on the slide the call named (or, naming none, the
  current one). Inspect the right slide (`target: {slide: N}`) and name it: `onSlide: N`, the run's
  `target`, or the step's `at.part`.
- **Results of earlier steps:** `slide_run` steps can't refer to an earlier step's result. Make the
  shapes in one call, read their ids from `completed[].result`, then use them in the next call.
- **Part of a text:** there is no text selection without the editor, so to give part of a text
  another look, put that part in its own shape.
- **Run colours:** a run `fill` set through the object model's text marks is accepted but neither
  drawn nor saved. Use [`format.color`](/kits/slide/commands/format.color/) `{ids}` on the shape.

## Checks

`slide_check` lists what is wrong with the presentation. Each diagnostic has a stable `code`, a
`severity` (`error`, `warning` or `info`), a `message`, a `target` and, for some, a `fix`: a
command step to run as it is. Act on the code, not on the message. A `fix` is a suggestion: run it
only when it is what the person asked for. Every code the kit reports is on the
[checks page](/kits/slide/checks/).

| Code | Severity | What to do |
|---|---|---|
| `slide.missing-media` | error | The picture or clip has no bytes: insert it again or delete the shape |
| `slide.unreadable-picture` | error | The bytes are not a picture: insert a PNG, JPEG, GIF, WebP, BMP, TIFF or SVG |
| `slide.text-overflow` | warning | `fix`: [`format.autofit`](/kits/slide/commands/format.autofit/) `{ids, onSlide, mode: shrink}`; or shorten the text, or enlarge the box |
| `slide.effects-dropped` | warning | Reduce the shadow, glow, soft edge or reflection size |
| `slide.font-substituted` | warning or info | Choose an installed family with [`format.font`](/kits/slide/commands/format.font/) `{ids, family}` (info: a stand-in with the same metrics) |
| `slide.off-slide` | warning | `fix`: [`shape.move`](/kits/slide/commands/shape.move/) `{ids, onSlide, x, y}` puts it back on the slide; or delete it |
| `slide.empty-placeholder` | info | Fill it with [`text.set`](/kits/slide/commands/text.set/), or `fix`: [`edit.delete`](/kits/slide/commands/edit.delete/) `{ids, onSlide}` |
| `slide.missing-alt-text` | info | Describe the picture: [`shape.altText`](/kits/slide/commands/shape.altText/) `{ids, onSlide, text}` (or `decorative: true`) |

`slide.render-failed` (error) means the renderer could not draw a slide: simplify what was last
changed on it.
