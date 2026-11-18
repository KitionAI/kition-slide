# Slide show and presenter view: parity with PowerPoint

> **Last reviewed:** 2026-11-17 · **Last updated:** 2026-11-17 · **Target:** Microsoft PowerPoint for Mac 16.113 (Microsoft 365)

Presenting is the job PowerPoint is hired for. This doc covers the slide show, presenter view,
rehearsal and recording in the kit's editor. Its slide show (`web/src/SlideShow.tsx`) shows one
slide at a time, full screen, each rendered by the engine at the screen's resolution; it plays no
transitions or animations yet. Transition and animation playback are in
[animation-parity.md](animation-parity.md); display hardware in
[hardware-parity.md](hardware-parity.md).

## Running the show

| Feature | PowerPoint | Kition Slide | Notes |
|---|---|---|---|
| Play from Start / from Current Slide (F5, Shift+F5, Cmd+Enter) | yes | yes | Slide Show tab, or `show.fromStart` / `show.fromCurrent` |
| Next/previous by click, arrows, Space, Page Up/Down, Enter, Backspace | yes | yes | Click, Right/Down, Page Down, Space and Enter go on; Left/Up, Page Up and Backspace go back |
| Go to slide (number + Enter), first/last | yes | first/last only (Home, End) | |
| Black / white screen (B / W) | yes | no | |
| End screen, Esc to leave | yes | Esc and a Close button; the show ends after the last slide, with no end screen | |
| Step clock: a click after a long pause plays its step | yes | no (no animations play) | |
| Hidden slides skipped | yes | no | |
| Hyperlinks and action settings followed (slides, web, mail, End Show, Last Viewed) | yes | no | |
| Media playback with controls, play across slides, full-screen video | yes | no | Media plays in the editor's Media pane, not in the show |
| Animated GIFs play; pause from their corner button | yes | no | |
| See All Slides grid during the show | yes | no | |
| Zoom into part of a slide during the show | yes | no | |
| Pen and highlighter ink | yes | no | |
| Laser pointer | yes | **no** | 1–2 h |
| Subtitles and live captions | yes (cloud speech) | no | AI dimension |
| Custom shows | yes | defined with `show.customShow`; the show does not play them | |
| Set Up Show: loop until Esc, kiosk (browsed at a kiosk), show without animation/narration, pen colour, slides range | yes | stored with `show.setup`; the show does not apply them | |
| Use rehearsed timings | yes | stored; the show does not advance by them | |

## Presenter view

| Feature | PowerPoint | Kition Slide | Notes |
|---|---|---|---|
| Separate presenter window with current slide, next slide, notes | yes | a panel beside the slide in the same full-screen page, with the next slide and the notes (Slide Show › Presenter View) | |
| Elapsed timer | yes | no | |
| Pause / reset timer, clock | yes | no | |
| Slide counter, previous/next/end buttons | yes | slide counter, Next and Close buttons | |
| Notes font size, scrolling | yes | no font size control | |
| Placed on the presenter's display automatically, show on the projector | yes | **no**: the presenter panel shares the show's screen | Beta blocker, 3–5 h |
| Swap displays | yes | no | |
| Pen, laser, black screen, zoom from presenter view | yes | no | |
| See all slides from presenter view | yes | no | |
| Rehearse with Coach (Speaker Coach) | yes (cloud) | no | AI dimension |

## Rehearse and record

| Feature | PowerPoint | Kition Slide |
|---|---|---|
| Rehearse Timings, keep timings | yes | yes: Rehearse Timings records each slide's time (`transition.timing`) as the show goes on |
| Record Slide Show (narration, ink, laser, camera; Cameo) | yes | no (the Record buttons are disabled) |
| Clear timing / narration | yes | timings only (`transition.timing` with `after: null`) |
| Export the recorded show to video | yes | no (see [file-format-parity.md](file-format-parity.md)) |

## Reading View

View › Reading View shows the same slide view without going full screen; the Transitions and
Animations tabs' Preview opens it too.

## Estimates

| Work | Hours |
|---|---|
| Presenter view on the right display, swap displays, pause/reset timer, clock, notes size | 3–5 |
| Laser pointer, highlighter, eraser, keep-annotations prompt | 2–4 |
| See All Slides and zoom during the show | 2–3 |
| Record slide show with narration (no camera) | 5–8 |
| **Total** | **~12–20** |

## Revision history

| Date | Change | Summary |
|---|---|---|
| 2026-11-17 | major | First public version |
