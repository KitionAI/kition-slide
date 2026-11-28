# UI parity with PowerPoint

> **Last reviewed:** 2026-11-17 · **Last updated:** 2026-11-28 · **Target:** Microsoft PowerPoint for Mac 16.113 (Microsoft 365)

How Kition Slide looks and feels next to PowerPoint: the shell, the ribbon, menus, panes, dialogs,
direct manipulation, keyboard and precision. Observations of PowerPoint are kept locally in
`plan/powerpoint/01-observed-ui.md` and `menu-tree.txt` (gitignored, clean-room rules in
AGENTS.md §2). Dimension score: **~60% ready** (estimated), **30–50 h** to parity.

## Table editing

The web editor has dedicated Table Design and Table Layout controls for style options, cell
shading, Draw Borders (pen color, pen weight, and every edge set including diagonals), vertical
alignment, margin presets, row/column insertion and removal, dimensions, distribution, and
merge/split. Click a cell to edit text; drag across cells or Shift-click to select a range.
Interior row and column lines show a resize cursor and drag in one undo step (Escape cancels).
Tab, Shift+Tab and arrow keys at a cell's edge move between cells and skip covered cells;
Shift+Arrow at the edge selects cells; Tab at the last visible cell inserts a row; Delete on a
range clears its text. Cell caret and range highlights come from engine overlays.

The flow is covered by `web/tests/table.mjs` in Chromium and WebKit and by native engine and SDK
PPTX round-trip tests. Draw-table tools and full imported table-style fidelity remain open, and
table editing has not been compared with PowerPoint at 99.9% visual parity.

## Shell

| Element | PowerPoint for Mac | Kition Slide | Gap |
|---|---|---|---|
| Menu-bar items with a Kition Slide equivalent | 217 app-specific items | 128 matched by name (59%, heuristic lower bound) | One parameterized command (`insert.chart {type}`) stands for many menu items, which a name match cannot see |
| Ribbon tabs: Home, Insert, Draw, Design, Transitions, Animations, Slide Show, Record, Review, View | 10 | 10 | Record tab is a placeholder |
| Contextual tabs: Shape Format, Picture Format, Table Design, Table Layout, Chart Design, Chart Format, Video/Audio Format and Playback | yes | most; checklist P0 row partial | Shape Format and Picture Format are the editor's own: fill, outline and effects menus, align, order, group, rotate, typed height, width and rotation, no parameter forms, in Chinese too; selecting an object brings its tab forward and clearing the selection goes back, as WPS does (`web/tests/hand/format.mjs`). Chart Format, media tabs thinner |
| Status bar: slide counter, language, notes, comments, views, zoom slider | yes | yes | Zoom goes 10% at a time to multiples of 10 within 10%–400% (as WPS), the percentage takes a typed value, Fit fits the slide as it opens (`web/tests/hand/zoom.mjs`) |
| Command search (Tell Me) | yes | no | |
| Light / dark appearance | follows the system | follows the host's theme | |
| Quick Access Toolbar customization | yes | no | P1 partial |

## Panes and dialogs

| Pane / dialog | PowerPoint | Kition Slide |
|---|---|---|
| Format Shape / Picture / Text pane (Fill & Line, Effects, Size & Properties, Picture, Text Box) | full | partial: fill, gradient editor, line, some effects, size; no 3-D, thin picture/texture options |
| Format Background pane | yes | yes |
| Selection Pane | yes | yes |
| Animation Pane | list + advanced timeline | list, reorder, timing; no timeline |
| Comments pane (threads) | yes | yes |
| Design Ideas (Designer) | yes (cloud) | no |
| Dialogs | ~60 (Font, Paragraph, Bullets and Numbering, Find, Replace, Replace Fonts, Hyperlink, Action Settings, Header and Footer, Slide Size, Set Up Show, Custom Shows, Print, Page Setup, Insert Object, Symbol, Equation, Chart data sheet, Smart graphic chooser, Preferences…) | The Export dialog (`web/src/ExportDialog.tsx`), the Link dialog (address or slide, Cmd+K; `web/src/LinkDialog.tsx`), the Spelling and Accessibility panes, and one generic parameters form (`editor.ask`, `web/src/slides.tsx`) for any command that needs values |

## Direct manipulation

| Behaviour | PowerPoint | Kition Slide |
|---|---|---|
| Selection handles, rotation handle, yellow adjustment handles | yes | yes: white round handles with a dark ring, a grey rotation handle 18 px above the top edge at any zoom, yellow diamonds, visible on any slide; a handle takes a press 7 px from its centre, the nearest wins, and a press just past one keeps the selection (`web/tests/hand/handles.mjs`, `pointer.mjs`) |
| Pointer shape: move over objects, a double arrow on each resize handle turned with the object, rotate, adjust, line end, the text cursor only where a press puts the caret, copy with Format Painter loaded | yes | yes (`web/tests/hand/pointer.mjs`) |
| Marquee, Shift/Cmd multi-select, Tab to cycle | yes | yes |
| Members of a group, several objects resized together, locked objects | a click on a member of the selected group selects it; one handle scales every selected object; a locked object's handles are grey | as WPS: a click selects the group, the next on a member that member (its own handles, a thin grey frame on the group), dragged, resized and formatted alone; a double-click edits a member's words; Escape goes back to the group; a handle of one of several scales each by the same ratio about its own opposite handle, one undo step; a locked object's handles are grey and take no press (`web/tests/hand/groups.mjs`) |
| Smart guides (edges, centres, equal spacing) | yes | yes: edges and centres, and equal spacing with the neighbours on a move (the gaps marked), drawn while the shape snaps, off from Grid and Guides (`web/tests/hand/snap.mjs`) |
| Snap to grid, grid options, drawing guides | yes | yes: View › Grid and Guides… and the slide's context menu set snap to grid, the spacing, the gridlines (drawn at that spacing), the drawing guides and the smart guides, kept on this machine; with snap to grid off a drag follows the pointer; Alt on a handle snaps nothing (`web/tests/hand/snap.mjs`) |
| Nudge (arrows), fine nudge (Option+arrows) | yes | as WPS: one grid step with snap to grid on, 0.75 pt with it off, Cmd+arrow 10 pt, Shift and Alt the same as the plain key (`web/tests/hand/snap.mjs`); an arrow key held on a selection is one undo step (`web/tests/hand/undo.mjs`) |
| Undo of typing | a run of typing is one step | the same, as WPS: a run of typing is one step whatever its pauses, Backspaces in a row are one, moving the caret, Enter, a click or another command start a new one, and each commit of an input method is one of its own (`web/tests/hand/undo.mjs`, `ime.mjs`) |
| Shift-constrained move, Option-drag to duplicate | yes | yes; a Shift drag stays on its axis against the grid and guides; the Alt copy lands at the pointer, off the grid; Cmd on a handle grows about the centre and Cmd draws a shape from its centre (WPS's keys, `web/tests/hand/snap.mjs`) |
| Connector glue and rerouting | yes | yes |
| Edit points on freeforms | yes | **no** |
| Crop handles, crop to shape, aspect ratio | yes | partial |
| Text caret, selection, double/triple click, drag-select | yes | yes (three clicks select the paragraph, `web/tests/hand/pointer.mjs`); Tab in text boxes ends editing; the caret blinks (`web/tests/hand/text.mjs`) |
| Chinese input (input methods) | composition shows at the caret, underlined; candidate window by the caret | the same, as WPS: the composition shows in the text at the caret, underlined and laid out with it, without changing the document; the candidate window opens under the caret's line where the composition starts; the input method's keys stay its own; each commit is one undo step (`web/tests/hand/ime.mjs`; a real input method not yet checked by hand) |
| Entering text by hand | a click on text edits it; Enter or F2 on a selected shape edits it | as WPS: one click on a shape's words puts the caret there, a press elsewhere on the shape selects it and a drag there moves it, a drag on words selects them, the band along an edited box's frame moves the box; Enter and F2 select all the text; typing on a selected shape goes on where its caret last was; an empty text box left goes (`web/tests/hand/text.mjs`) |
| Exact size/position, lock aspect | yes | yes: Shape Format's height, width and rotation fields take a typed value (centimetres, or with its unit), with the aspect ratio locked on demand (`web/tests/hand/format.mjs`) |
| Align and distribute, order, group, rotate and flip from the ribbon | yes | yes: Home › Arrange and Shape Format run them on the selection in one click, one object to the slide and several to their selection (`web/tests/hand/format.mjs`) |
| Rulers with indent markers and tab stops | yes | rulers yes; tab stops partial |
| Context menus on shapes, slides, text, thumbnails | yes | yes: on the slide a right-click (Ctrl+click on the Mac) selects what it is on first and opens WPS's rows for a shape, a picture, text, a table cell or the empty slide; Shift+F10 opens it for the selection (`web/tests/hand/context-menu.mjs`); the thumbnails' menu has Copy, Cut, Paste, New, Duplicate, Delete, Hide, Add and Rename Section, Layout and Reset (`web/tests/hand/thumbnails.mjs`) |
| Floating toolbar on selected text, align bar on several objects | yes (Mini Toolbar) | yes: words selected with the mouse bring font, size, bold, italic, underline, color, alignment and bullets up above them without taking the keyboard; several objects bring the six aligns, two distributes and Group (`web/tests/hand/context-menu.mjs`) |
| Drag and drop of files and between thumbnails | yes | partial: picture files dropped on the slide go where they are dropped (`web/tests/hand/clipboard.mjs`); a dropped slide file is not opened (the host opens files); thumbnails reorder by dragging, several at once, the others making room live with no insertion line (`web/tests/hand/thumbnails.mjs`) |
| Copy, cut and paste with other apps | yes | yes, plain text: what Cmd+C copies reaches other apps as text; their text pastes at the caret or as a new text box; Cmd+Alt+Shift+V pastes text only (`web/tests/hand/clipboard.mjs`) |
| Image paste from the clipboard | yes | yes: 0.75 pt a pixel, in the middle of the slide (`web/tests/hand/clipboard.mjs`), or into the slide's empty content placeholder, whole and as large as fits (`web/tests/hand/insert.mjs`) |
| Drawing shapes and text boxes | the Shapes gallery: drag out any shape, click for a default size, lock drawing mode; Text Box: click or drag | as WPS: every preset in the gallery as an icon drawn from its geometry, in the nine groups; a drag draws it as large as the drag, a click makes 72 × 72 pt, then Select with the shape selected; a right-click on a gallery shape locks it for drawing until Escape; the Text Box tool's click makes a 320 pt wrapping box, its drag sets the width; a double-click on the empty slide makes a text box (`web/tests/hand/insert.mjs`) |
| Insert Table grid | sweep the grid, or Insert Table… | yes: a 10 × 8 grid to sweep (or walk with the arrows), and Insert Table… for rows and columns (`web/tests/hand/insert.mjs`) |
| Font menu: every installed family with its faces | yes | families yes; only Regular/Bold/Italic faces |

## Keyboard shortcuts

Every common PowerPoint for Mac shortcut is bound (checklist row done): new slide, duplicate,
group/ungroup (Cmd+G, Cmd+Shift+G, as WPS), bold/italic/underline, alignment, font size, list levels, Format Painter, show from
start/current, view switching. Not yet: Cmd+F Find opening a find bar on the slide, Option+drag
precision without snapping, F6 pane cycling, ribbon KeyTips (Windows). With nothing selected Home and
End go to the first and last slide, the arrows change slides, Tab selects the first object and Delete
deletes nothing (as WPS); Cmd+Shift+Z redoes as Cmd+Y does; F5 and Shift+F5 start the show, and
Cmd+Enter does with an object selected too; Cmd+1 is 100% (`web/tests/hand/keys.mjs`). In text,
Cmd+Left and Cmd+Right go to the line's start and end, Cmd+Up and Cmd+Down (and Cmd+Home, Cmd+End) to
the text's, with Shift extending; Cmd+Backspace deletes to the line's start; Ctrl+A and Ctrl+E as on
the Mac; on Windows and Linux Ctrl+Left and Ctrl+Right go by word, Ctrl+Backspace deletes a word
(`web/tests/hand/text.mjs`). Not yet: Ctrl+Enter to the next placeholder.

## Visual fidelity

Colours come from the host's theme tokens and the ribbon, panes and dialogs from kit-editor's
components (AGENTS.md "Kition kit rules"); icons come from Kition Tessera's `@kition/tessera/icons`
and must not imitate Microsoft's. The editor has not been compared with PowerPoint screenshots
pixel by pixel. Icon consistency across Kition's kits is open.

## Estimates

| Work | Hours |
|---|---|
| Format Shape pane depth | 6–10 |
| Missing dialogs (Font, Bullets and Numbering, Find/Replace, Replace Fonts, Action Settings, Print, Insert Object) | 8–14 |
| Edit points UI, crop depth | 3–6 |
| A screenshot comparison pass on the most-used tabs | 2–4 |
| **Total** | **~30–52** |

## Revision history

| Date | Change | Summary |
|---|---|---|
| 2026-11-17 | major | First public version |
