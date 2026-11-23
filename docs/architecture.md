# Kition Slide architecture

> **Last reviewed:** 2026-11-17 · **Last updated:** 2026-11-17 · **Target:** Microsoft PowerPoint for Mac 16.113 (Microsoft 365)

How Kition Slide is built today. The binding rules (layering, everything is a command, thin UI, never
break wasm, never crash) are in [AGENTS.md](../AGENTS.md) §3–§5; this document describes what
exists: 23 crates, 1 app (`kition-slide`) and `xtask` (`cargo xtask stats` counts the lines and
`#[test]` functions of each crate).

The repository holds the engine (reachable from `bindings/wasm`), the headless kit executable
(`bindings/headless`), `xtask`, and the kit's own web editor at `web/`. It has no desktop app, CLI
or MCP server of its own.

## Layers

`cargo xtask layers` enforces that a crate depends only on lower layers (the table in
`tools/xtask/src/layers.rs`, which is authoritative; AGENTS.md §4 gives the simplified picture), and that
nothing below L6 uses egui, eframe, winit or rfd.

| Layer | Crates | What they do |
|---|---|---|
| L0 | `geom`, `color`; standalone `bitstream`, `matroska`, `ogg`, `opus` | EMU/pt units, transforms, ~150 preset shape generators re-derived from ECMA-376, adjust handles, hit testing; RGB/HSL and DrawingML colour transforms; MKV/WebM and Ogg demuxers, the Opus decoder |
| L1 | `model`, `fonts`; `h264`, `hevc`, `vp9`, `av1`, `isobmff` | Presentation, masters, layouts, slides, shapes, text, themes, tables, charts, transitions, animations, placeholder inheritance (`resolve`); font database (kition-fonts + system) with metric-compatible substitutes for Office fonts (Carlito for Calibri, Caladea for Cambria, Liberation for Arial/Times); video decoders and the MP4/MOV demuxer |
| L2 | `text`, `format`, `media` | Shaping (HarfRust), UAX #9 bidi, UAX #14 line breaking, autofit, caret hit testing; `.kslide` (zip + JSON + media) and outline text; media probing, decoding, mixer and clock |
| L3 | `render`, `pptx` | `vello_cpu` raster of slides, masters and layouts (fills, outlines, effects, pictures, tables, charts); PPTX import/export from ECMA-376 |
| L4 | `pdf`, `kdm-model` | PDF export through krilla; the `kit.slide` KDM bridge (Kition's own: the slide file as a kition-sdk replica) |
| L5 | `engine` | Sessions, documents, selection, undo history on shared slide snapshots, the command registry (in `cmd/*`), pointer tools, keys, AutoRecover, the panic guard (`guard.rs`) |
| L6 | `kition-slide-kit` (TABLE key `kit`) | Kition's in-page editor engine (Apache-2.0): the slides as a `KitDocument`, the pointer tools as `KitTools`. The kit's web editor sits above it, outside this Cargo workspace (`web/`) |
| apps | `kition-slide` (the headless kit executable for Kition's agent), `xtask` | Exempt from layering |

The media codecs and containers are clean-room pure Rust, 59k lines, shared in spirit with
Kition Film.

## Data model

A `Presentation` holds a theme set, slide masters with their layouts, slides, sections, custom
shows, comments, media and document properties. Shapes are a tree (`ShapeKind`: preset or custom
geometry, picture, table, chart, group, connector, ink, media, smart-graphic-as-group, and `Opaque` for
anything read from PPTX that Kition Slide doesn't model: its XML is kept and written back). Text
bodies hold paragraphs of runs with DrawingML properties; formatting resolves slide → layout →
master → theme at render time. Transitions live on slides; animations are the flat ordered list the
Animation Pane shows, rebuilt into PowerPoint's timing tree on save.

## Commands and frontends

Every user-visible action is a `CommandSpec` (id, label, ribbon/menu place, shortcut, params doc,
`enabled`, `run`) in `crates/engine/engine/src/cmd/*`. Both of this engine's frontends — the headless
`kition-slide` executable Kition's agent drives, and `kition-slide-kit`'s `KitDocument::execute` the
web editor's WASM build calls — dispatch through the same `Session::execute`, so they behave
identically. Programmatic calls never open dialogs. Pointer gestures go through `Session::pointer`,
keys through `Session::key`.

## Rendering and the show

The renderer draws a slide to an RGBA pixmap on the CPU. The web editor shows the slide being
edited in kit-editor's document canvas (`web/src/SlideCanvas.tsx`) and runs the slide show itself
in `web/src/SlideShow.tsx`, drawing `kition-slide-kit`'s `renderSlide` images into canvases; the
show plays no transitions or animations yet (`pnpm --dir web test:flow` covers starting and ending
it). The engine holds no runtime show or transition-playback state. PDF export renders each slide at print
resolution and overlays an invisible real-text layer with embedded font subsets.

## File I/O

- **PPTX** (`crates/formats/pptx`): lenient reader (unknown parts skipped, malformed parts logged and
  ignored, sizes and depths bounded, zip bombs capped), writer that PowerPoint opens without
  repair. Unknown elements round-trip through `Opaque` and `raw_ext` fields.
- **Native** (`crates/formats/format`, `.kslide` format version 1): zip with `mimetype`, `slide.json`
  and `media/<id>`. `slide.json` leaves out every member equal to its default (the reader fills it
  back); the built-in master and layouts are written out like any other, as they are made by code
  from the slide size and theme, not kept as a fixed set under a versioned name. Readers ignore
  unknown fields.
- Formats and their gaps: [file-format-parity.md](parity/file-format-parity.md).

## Quality gates

`cargo xtask ci`: rustfmt check, clippy `-D warnings`, tests, layering, asset attribution
(`cargo xtask assets`), wasm32 check. It runs in the kit's CI run on GitHub: the CI workflow
(`.github/workflows/ci.yml`) calls Kition's reusable `kit-ci.yml`, which also runs clippy,
`cargo test --workspace` and every `test:*` script of `web/`. The repository's other workflow is
the Package workflow (`.github/workflows/package.yml`), which builds and publishes the kit's
package on GitHub with Kition's reusable `kit-package.yml` (see `docs/kits.md` there); only the
owner starts it.

## Revision history

| Date | Change | Summary |
|---|---|---|
| 2026-11-17 | major | First public version |
