# Changelog

Changes to Kition Slide, newest first. A version is the kit package version in `kit.json`;
each package is published with a `package-v<version>` tag.

## Unreleased

- The engine runs on kition-sdk's plain document model (no CRDT): Kition Desktop is for one person, so decks are no longer merged from concurrent edits; a deck's changes form one line, and a replica takes another's changes only in order. A file changed outside Kition is still recorded onto the stored replica as one import change, and the editor no longer declares `replicaMergesFile` (the host hands over the stored replica whatever the file's stamp). Loro is no longer bundled or linked.
- The engine answers the object model (`modelDescribe`, `modelBatch`) and receipts (`changesSince`) itself, as kition-sdk's kit editor now asks its engine for them.

## 0.1.3

- A host asks a slide file for its document id (`document.identity`) and forks it (`document.fork`), Kit Process Protocol section 5.6 (kition-sdk 6672663): a `.kslide` or `.pptx` embeds no replica (the kit makes a new one each time it opens a file), so it carries no id and its fork is the same bytes.
- The headless program speaks Kit Process Protocol version 1 (`headless.protocol` is `kition-kit/1`), as hosts on kition-sdk from bb1b07b require.

## 0.1.2

- `kit.json` declares the engine's editor state as `engine.editorState`, so a host that does not run the engine's Rust code reads `inspect()` without it.
- A lint fix in the HEVC decoder's SAO neighbour check (no change in behaviour).

## 0.1.1

- The pasteboard around the slide follows the host's theme, live: in a dark host it is dark, and it repaints when the host's theme changes (before, it kept its built-in light grey).

## 0.1.0

First public release of the source, under the Apache License 2.0. It contains:

- The Rust engine: presentations with masters, layouts, slides, sections and speaker notes; text
  with fonts, bullets, numbering, columns and autofit; preset and custom shapes, pictures, tables,
  charts, comments, transitions and animations; `.pptx` and `.kslide` read and written, PDF, PNG
  and JPEG export, and pure-Rust audio and video decoders for media.
- The kit engine (`kition-slide-kit`), native and WebAssembly: the deck is a KDM replica, replicas
  merge concurrent edits, and undo is per person and per agent. A check catalogue reports what a
  deck cannot carry, and design hints name the commands that fix what they find.
- The headless kit (`kition-slide`) for agents over the Kit Protocol: it reads a slide's shapes by
  id, edits them with the editor's own commands, renders a slide, and works in drafts. A write
  names the revision it read and is refused when someone else changed what it rests on. It reads
  nothing outside its workspace.
- The kit's editor (`web/`): the ribbon, the slides panel, the slide canvas, speaker notes and the
  slide show with its presenter panel, in kit-editor's frame.
- NOTICE and docs/attribution.md list the upstream work, the fonts and every asset, and Loro
  (MIT), bundled by the editor and linked by the engine.
