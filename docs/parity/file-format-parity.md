# File-format parity with PowerPoint

> **Last reviewed:** 2026-11-17 · **Last updated:** 2026-11-17 · **Target:** Microsoft PowerPoint for Mac 16.113 (Microsoft 365)

Every format PowerPoint reads or writes, Kition Slide's support, fidelity and how it is tested.
The features and their gaps are in [gaps.md](../gaps.md). Dimension score: **~58% ready**
(estimated), **60–110 h** to parity.

Weights inside this dimension: PPTX 70%, PDF export 10%, image export 5%, legacy binary formats
5%, everything else (templates, shows, macros, ODP, video, GIF, outline) 10%.

## Documents

From `/Applications/Microsoft PowerPoint.app/Contents/Info.plist` `CFBundleDocumentTypes` (Editor =
PowerPoint opens and saves; Viewer = opens only).

| Format | PowerPoint | Kition Slide read | Kition Slide write | Fidelity / notes | Tested by |
|---|---|---|---|---|---|
| `.pptx` slide | Editor | yes | yes | Broad (see below); never measured on real slides | `crates/formats/pptx/tests/roundtrip.rs`, `sample.rs`, `malformed.rs`, `sniff.rs`; generated files open in PowerPoint without repair (checked by hand) |
| `.potx` template | Editor | yes (as a slide) | no: written with the `.pptx` content type | `file.saveTemplate` is Save As under another name | none |
| `.ppsx` slide show | Editor | yes (as a slide file) | no: written as `.pptx` content | Doesn't start in show mode | none |
| `.pptm` / `.potm` / `.ppsm` macro-enabled | Editor | no | no | Should open and keep `vbaProject.bin` without running it | none |
| `.ppt` / `.pot` / `.pps` 97–2003 binary | Editor | no | no | Maintainer decision; import only would be enough | none |
| `.odp` OpenDocument | Editor | no | no | | none |
| `.thmx` Office theme | Viewer | no | no | Browse for themes is a P2 row | none |
| `.kslide` native (zip + JSON) | n/a | yes (also `.kslide`) | yes | Lossless for the Kition Slide model; version 1 only, another version is refused | `crates/formats/format` tests |
| PDF | export | no | yes | Slides, notes pages, handouts (1–9); real-text layer with embedded subsets, links, bookmarks. Slide artwork is a raster image at print resolution, not vector | `crates/formats/pdf` tests |
| Outline `.rtf` / `.txt` | export (RTF), import (outline) | `.txt`/`.md` outline (`slide.fromOutline`) | `.txt` outline | No RTF | engine tests |
| PNG / JPEG slide images | export | n/a | yes (`file.export`; the editor's File › Export) | Matches the editor's renderer | render tests |
| TIFF / BMP / GIF slide images | export | n/a | no | Easy (the `image` crate writes them) | none |
| Animated GIF of the slide | export | n/a | no | | none |
| MP4 / MOV video of the slide | export | n/a | no | Needs timeline render + H.264 encode | none |
| Print | yes | n/a | no | Print the exported PDF | none |

## Pictures and media inside slides

| Kind | PowerPoint | Kition Slide | Notes |
|---|---|---|---|
| PNG, JPEG, GIF (animated), BMP, TIFF, WebP | yes | yes | `image` 0.25; animated GIFs play |
| SVG | yes (with PNG fallback) | **no** | The PNG fallback shows; SVG files can't be inserted |
| EMF / WMF | yes | partial | Common vector/text/bitmap records render; complete state/comment-only WMFs can yield transparent previews. Unknown drawing records, embedded EMF/EMF+ and full validation remain incomplete |
| HEIC | yes (macOS) | no | |
| Audio: MP3, AAC/M4A, ALAC, WAV, AIFF, CAF, FLAC, Ogg Vorbis, Opus | yes (Ogg/Opus partly) | yes | Pure-Rust decoders in `crates/media/media`, `crates/media/opus` |
| WMA | yes (Windows) | embedded, not playable | |
| Video: H.264, HEVC in MP4/MOV | yes | yes | Pure-Rust, software decode |
| Video: VP9, AV1 in WebM/MKV | partly (OS codecs) | yes | Ahead of PowerPoint for Mac |
| WMV | yes (Windows) | embedded, not playable | |
| Online video (YouTube embeds) | yes | no | Cloud feature |
| Embedded OLE objects | yes | kept on round trip (opaque), not insertable or editable | `ShapeKind::Opaque` |
| 3-D models (`.glb`) | yes | no | |
| Embedded fonts | yes | no | `p:embeddedFontLst` ignored |

## PPTX coverage (PresentationML / DrawingML)

What `crates/formats/pptx` reads and writes, from ECMA-376. "Kept" means the XML is preserved untouched for
the round trip even though Kition Slide doesn't model it.

| Part / feature | Read | Write | Notes |
|---|---|---|---|
| OPC package, content types, relationships | yes | yes | Content types found anywhere in the zip; zip bombs capped; relationship cycles bounded |
| Presentation, slide size, sections (p14), custom shows | yes | yes | |
| Slide masters, layouts, placeholders and inheritance | yes | yes | |
| Themes: colour, font (Latin/EA/CS), effect schemes, background styles | yes | yes | Unknown theme parts kept |
| Notes slides and notes master | yes | yes | No notes-master editing view |
| Handout master | no | no | |
| Shapes: preset geometry (168 catalog entries), custom geometry, groups, connectors | yes | yes | Re-derived arrows, equation symbols, line callouts, curved ribbons, scrolls, transport buttons and crescents; four- and five-segment elbow connectors honor all bend adjustments; 20 standard presets remain absent or incomplete |
| Fills: solid, gradient, picture (stretch/tile/crop), pattern, group | yes | yes | Scaled linear directions, centered elliptical shape gradients and endpoint brightness interpolation are rendered; exact duplicate stops are paint-neutral while coincident different colors retain hard transitions; arbitrary path focus, pattern anchoring and complete transparent-gradient fidelity remain incomplete |
| Lines: dash, compound, caps, joins, arrowheads | yes | yes | Gradient shape outlines render all stops; compound chart outlines and symmetric double/triple shape outlines are drawn; straight asymmetric shape outlines retain unequal bands and transparent gaps; curved and joined asymmetric outlines remain incomplete |
| Effects: shadow, glow, soft edges, reflection | yes | yes | Run shadows and glow render; explicit run effect lists replace inherited lists as a whole, including empty resets, and retain that presence after regenerated output; solid text alpha is resolved once, and invisible outlines/fill do not enlarge effect silhouettes; exact glow coverage, split-run compositing and spatial alpha masks for gradient text effects remain incomplete |
| 3-D: `scene3d`, `sp3d` | kept | kept | Not rendered |
| Text: runs, paragraphs, bullets, numbering, levels, autofit, columns, vertical, RTL, script fonts, fields, hyperlinks | yes | yes | Paragraph default run properties survive import; horizontal body anchoring centers the occupied aligned text bounds. Imported shape autofit scales survive display; editing recalculates and saves the new scale. Wrapped horizontal visible bounds exclude trailing ASCII spaces without removing caret advances; non-wrapping text keeps its full width. Unrotated, unreflected East Asian vertical bodies use upright glyphs and font vertical metrics; transformed bodies keep the legacy path, and full vertical alternate forms remain incomplete. Run gradients resolve theme stops and share a continuous paint frame across compatible font fragments on each line. Tests also cover `justLow`, date fields and part-local shape IDs |
| Text warp (`prstTxWarp`) | yes | yes | Adjustments and explicit no-shape inheritance resets survive regeneration. Verified horizontal single-line middle-anchored ArchUp/ArchDown use rigid shaped-cluster placement; general preset fidelity and unsupported placement or decoration modes remain incomplete |
| Pictures with crop, recolour, duotone, alpha, luminance | yes | yes | Fixed picture fills preserve `rotWithShape=false` and use the transformed silhouette's bounds; parent transforms, independent flips, crop, tile and reflection remain active |
| Tables with styles | yes | yes | |
| Charts (`c:chartSpace`) | yes | yes | Original XML kept in `Chart.raw`; homogeneous stacks on a single normal-direction value axis use observed right-side vertical and horizontal-stack legend order, preserving original series names and key colors; script typefaces and explicit per-series smoothing are retained; line/scatter error-bar statistics, ranges and point formatting improved; percentage-stack standard errors use normalized plotted coordinates, while ordinary-stack cumulative statistics remain incomplete; series/point picture markers retain media and edited fill properties, and unset point outlines inherit the series. Authored data-label positions and extension sizes survive regeneration; factor/edge placement is rendered, except BestFit position offsets pending accurate automatic anchoring. Point overrides retain inherited display flags and number formats, including source-linked resets. Chart shadow/glow values and explicit effect-list resets are imported and regenerated for data labels; effect-only point overrides preserve the existing font fallback. Generated category-plus-percentage pie labels default to a newline; authored separators and separator-only point overrides (including empty) survive regeneration. Automatic 2-D pie inside-end labels use measured circle containment; chart text-effect painting and complete regenerated chart text formatting remain incomplete |
| Charts (`cx:` chartex: waterfall, funnel, treemap, sunburst, histogram, box) | no | no | |
| Smart graphics (`dgm`) | drawing part only, as a group | as shapes | Diagram data model lost on save |
| Equations (OMML, `a14:m`) | partial | preserved | Original OMML and authored font properties retained; bounded inline layout for fractions, scripts, radicals, delimiters and basic operator limits, explicit math run styles, upright ASCII digits and recognized operators, ordinary-text style preservation, grapheme-safe operator spacing, boundary-aware function application spacing, centered reduced upper/lower function limits, group characters with actual-face horizontal variants/assembly and baseline selection, justification for fitting standalone math paragraphs, serif fallback for missing DejaVu Math, basic matrix rows/columns, intrinsic equation-array rows with alternating alignment/column markers and authored baseline/row spacing, multiargument separators, upright delimiter growth/alignment, bars and combining accents, empty essential argument metrics, skewed-fraction separators sized from painted argument bounds, border-box edges and horizontal/vertical strikes, actual-face MATH delimiter variants/assembly; empty-text structural equations retain an atomic model character; width-dependent equation-array distribution and run/box alignment points, diagonal border strikes, unavailable math-font fallback and structural editing remain incomplete |
| Audio / video (`p14:media`, trim, fades, bookmarks) | yes | partly | Playback options lost on save |
| Ink (`contentPart`) | partial | freeform paths and preserved fallback shapes | Supported single-trace round XY pens become editable paths with physical widths and frame transforms; unsupported brushes, mappings, pressure, multiple traces, stationary strokes and ambiguous or malformed content retain their entire fallback. Regenerated output uses shapes, not InkML |
| Transitions incl. p14/p15 (Vortex, Morph…) | all 48 | all 48 | `every_transition_kind_round_trips` |
| Animations: timing tree, build paragraphs, triggers, motion paths | yes (flattened to the Animation Pane list) | yes (rebuilt) | `every_animation_effect_survives`; rewind lost |
| Comments: legacy | yes | yes | |
| Comments: modern threaded (`p188`) | yes | no (written as legacy) | Replies and resolved state lost |
| Unknown slide-level elements (`graphicFrame` OLE, extensions, `mc:AlternateContent`) | kept | kept | Rendered as a labelled placeholder with the preview picture when present |
| Tags, custom XML, VBA | no / no / no | | |
| Embedded fonts | no | no | |
| Encryption (password to open) | no | no | |

### What would move the PPTX number

1. An owned corpus of real-world-shaped slides (corporate templates, smart graphics, chartex charts, EMF,
   embedded fonts, every transition and animation) in a corpus repo, with import → render → export
   → re-import checks in CI and a PowerPoint reference render for each (a maintainer runs PowerPoint).
2. The round-trip losses above: threaded comments, media playback options, animation rewind.
3. Remaining EMF/WMF record coverage and SVG rendering.

## Revision history

| Date | Change | Summary |
|---|---|---|
| 2026-11-17 | major | First public version |
