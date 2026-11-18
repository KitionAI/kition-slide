# Hardware parity with PowerPoint

> **Last reviewed:** 2026-11-17 · **Last updated:** 2026-11-17 · **Target:** Microsoft PowerPoint for Mac 16.113 (Microsoft 365); PowerPoint for Windows where noted

What hardware PowerPoint uses and what Kition Slide uses. Kition Slide's editor (`web/`) runs in
the page of its host, Kition, with the engine compiled to WebAssembly; the headless kit executable
is a native build. Dimension score: **~40% ready** (estimated), **25–45 h** to parity.
Performance (unmeasured, ~55%) is tracked here too because it mostly follows from hardware use.

## Graphics and rendering

| Feature | PowerPoint | Kition Slide (the editor in `web/`) |
|---|---|---|
| Slide rasterization | GPU | **CPU** (`vello_cpu`, in WebAssembly) |
| Transitions and Morph frames | GPU | not played: the editor's slide show has no transitions yet |
| High-DPI / Retina | yes | yes (kit-editor's canvas and the show draw at the device pixel ratio) |
| Wide gamut / HDR display | partly (P3 images) | no (sRGB) |

## Media

| Feature | PowerPoint | Kition Slide |
|---|---|---|
| Video decode | Hardware (AVFoundation/VideoToolbox, Media Foundation) | The editor plays media with the browser's `<video>` and `<audio>` elements (the Media pane, `web/src/MediaPlayer.tsx`); the engine's own decoders are pure-Rust software (`crates/media/h264`, `hevc`, `vp9`, `av1`) |
| 4K video in the show | smooth on hardware decode | no: the editor's slide show does not play media |
| Audio output | Core Audio / WASAPI | the browser's `<audio>` and `<video>` elements |
| Microphone (record audio, narration) | yes | no |
| Camera (Cameo, record with camera) | yes | no |
| Screen recording | yes | no |

## Displays

| Feature | PowerPoint | Kition Slide |
|---|---|---|
| Detect the projector, show on it, presenter view on the laptop | yes | **no**: the show goes full screen on the current display; presenter view is a panel beside the slide in the same full-screen page (next slide and notes) |
| Swap displays, choose the monitor in Set Up Show | yes | no |
| Mirroring-aware behaviour | yes | no |

## Input devices

| Feature | PowerPoint | Kition Slide |
|---|---|---|
| Keyboard and slide clickers (Page Up/Down, B, Esc) | yes | Page Up/Down, arrows, Space, Enter, Home/End and Esc in the show; no B (black screen) |
| Mouse and trackpad scroll | yes | yes |
| Pinch to zoom, two-finger gestures | yes | pinch (Ctrl or ⌘ with the wheel) zooms kit-editor's slide canvas; two-finger scrolling pages or pans (`web/src/useSlideWheel.ts`) |
| Pen pressure and tilt for ink (Apple Pencil via Sidecar, Windows Ink) | yes | no: ink strokes have constant width |
| Touch screens (Windows) | yes | basic pointer only |
| IME input methods for CJK text | yes | they compose in kit-editor's text field (see [localization-parity.md](localization-parity.md)) |
| Dictation | yes (macOS) | no |

## Performance

Nothing is measured against PowerPoint yet: no benchmark of open time, render time per slide,
show frame rate or memory exists in `xtask`. Known: slides rasterize on the CPU and the engine's
decoders run in software.

## Estimates

| Work | Hours |
|---|---|
| Presenter view on the right display, choose monitor, swap | 3–5 |
| GPU slide rasterization (vello's GPU/hybrid path) | 12–20 |
| Hardware video decode (copy Kition Film's VideoToolbox / Media Foundation / VA-API paths) | 8–14 |
| Pen pressure, pinch zoom | 3–6 |
| Benchmark suite (`cargo xtask bench`) and a first performance pass | 6–12 (Performance dimension) |
| **Hardware total** | **~26–45** |

## Revision history

| Date | Change | Summary |
|---|---|---|
| 2026-11-17 | major | First public version |
