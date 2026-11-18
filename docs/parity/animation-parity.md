# Transitions and animations: parity with PowerPoint

> **Last reviewed:** 2026-11-17 · **Last updated:** 2026-11-17 · **Target:** Microsoft PowerPoint for Mac 16.113 (Microsoft 365)

Transitions and animations are what make a slide a slide, and the part of PowerPoint people
notice first when it's missing. Estimated readiness: Transitions ~80%, Animations ~65%. Each
feature's status is in [target-app-parity.md](target-app-parity.md); gaps are in [gaps.md](../gaps.md).

The scores below are the data model's breadth (every kind the model carries and round-trips
through PPTX). The web editor does not play transitions or animations yet
(`web/src/SlideShow.tsx`). The renderer can still draw a Morph frame between two slides
(`render_blend`, with `morph_path.rs` and `morph_text.rs`, in `crates/render/render`), but only
its tests call it; nothing in this repository plays a transition or an animation.

## Transitions: 48 of 48

`crates/engine/model/src/anim.rs` `TRANSITIONS` lists every transition in PowerPoint's gallery, in its
three groups, with its Effect Options. All 48 round-trip through PPTX
(`every_transition_kind_round_trips`), including the PowerPoint 2010+ `p14`/`p15` ones.

| Group | PowerPoint | Kition Slide | Fidelity |
|---|---|---|---|
| Subtle (Morph, Fade, Push, Wipe, Split, Reveal, Cut, Random Bars, Shape, Uncover, Cover, Flash) | 12 | 12 | Modeled and round-trips through PPTX; not rendered by anything in this repository today (see note above) |
| Exciting (Fall Over, Drape, Curtains, Wind, Prestige, Fracture, Crush, Peel Off, Page Curl, Airplane, Origami, Dissolve, Checkerboard, Blinds, Clock, Ripple, Honeycomb, Glitter, Vortex, Shred, Switch, Flip, Gallery, Cube, Doors, Box, Comb, Zoom, Random) | 29 | 29 | Modeled and round-trips through PPTX; not rendered by anything in this repository today (see note above) |
| Dynamic Content (Pan, Ferris Wheel, Conveyor, Rotate, Window, Orbit, Fly Through) | 7 | 7 | Same: modeled, not rendered today |

| Transition feature | PowerPoint | Kition Slide |
|---|---|---|
| Effect Options per transition | yes | yes |
| Duration, advance on click / after time, Apply To All | yes | yes |
| Transition sounds | yes | **no** (the model has the field; no picker, no playback) |
| Preview in the editor | yes | **no** (not rendered by the web editor yet) |
| Morph: objects move, resize, recolour | yes | yes |
| Morph: Words and Characters | yes | yes |
| Morph: geometry (circle to star) | yes | yes |
| Morph: `!!` name forcing, pictures, 3-D models, ink | yes | names partly; no 3-D or ink |
| Frame-by-frame comparison with PowerPoint | | **not done** |

## Animations: 64 of ~190 effects

| Class | PowerPoint (gallery + More Effects) | Kition Slide | Missing (examples) |
|---|---:|---:|---|
| Entrance | ~50 | 24 | Plus, Wedge, Basic Zoom, Center Revolve, Compress, Stretch, Boomerang, Credits, Curve Up, Drop, Flip, Float, Pinwheel, Spiral In, Whip, Ascend, Descend, Unfold |
| Emphasis | ~25 | 17 | Brush Color, Bold Reveal, Contrasting Color, Flicker, Grow With Color, Shimmer, Blink |
| Exit | ~50 | 14 | Mirrors of the missing entrance effects, plus Blinds, Box, Checkerboard, Circle, Diamond, Peek Out, Strips as exits |
| Motion paths | 64 presets + custom | 6 (Lines, Arcs, Turns, Loops, Shapes-style presets, Custom Path) | Most of Basic (shapes), Lines & Curves and Special presets |
| Media (Play, Pause, Stop) | 3 (+ Seek) | 3 | Seek, bookmarks as triggers |

By how often people use them, the effects present cover nearly all real slides (Appear, Fade, Fly
In, Wipe, Zoom, Float In, Grow/Shrink, Pulse, Spin, motion lines): weighted, ~85% of use; by count,
~34%.

| Animation feature | PowerPoint | Kition Slide |
|---|---|---|
| Start On Click / With Previous / After Previous, duration, delay | yes | yes |
| Effect Options (direction, sequence by paragraph) | yes | yes |
| Effect dialog: sound, after-animation dim/hide, animate text by word/letter, smooth start/end, bounce end, repeat, rewind | yes | partly; rewind lost on PPTX save |
| By-paragraph text builds, sub-bullets follow parents | yes | yes |
| Triggers (on click of a shape) | yes | yes |
| Triggers on a media bookmark | yes | no |
| Animation Painter | yes | yes |
| Animation Pane: list, reorder, remove | yes | yes |
| Animation Pane: advanced timeline (bars you drag) | yes | **no** |
| Preview in the editor (current effect, from selected) | yes | **no**: Preview opens the slide in Reading View without playing the effect |
| Motion path editing (edit points, reverse, lock) | yes | partly (no edit points) |
| Multiple animations on one object | yes | yes |
| Round trip of the timing tree | yes | flattened to the pane list on read, rebuilt on write (`every_animation_effect_survives`) |

## Estimates

| Work | Hours |
|---|---|
| Animation Pane timeline, preview from selected, effect dialog depth | 5–8 |
| The remaining ~126 effects (most are parameter variations of existing ones) | 3–6 |
| Rewind and other effect options on round trip | 1–2 |
| Transition sounds | 1–2 |
| Morph across pictures, `!!` names, align once per transition | 2–4 |
| Reference captures from PowerPoint for the 3-D transitions, then tuning | 2–4 |
| **Total** | **~14–26** |

## Revision history

| Date | Change | Summary |
|---|---|---|
| 2026-11-17 | major | First public version |
