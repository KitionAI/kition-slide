# Localization parity with PowerPoint

> **Last reviewed:** 2026-11-17 · **Last updated:** 2026-11-17 · **Target:** Microsoft PowerPoint for Mac 16.113 (Microsoft 365)

The engine's own strings (command labels, model enum display names) are English-only literals,
with no catalog and no language switch. Kition's own web editor, separately, ships a small
`en-US`/`zh-CN` catalog (`web/src/strings.ts`, `web/src/i18n/locales/`; AGENTS.md "Kition kit
rules") and takes its language from the host (`host.init.locale`); the table below counts only
English for the engine's strings and does not reflect the editor's `zh-CN` catalog.
Dimension score: **~8% ready** (estimated: English complete, slide-text script support partly
there), **55–90 h** to parity plus native-speaker review.

## How the numbers are measured

- **UI strings:** ~1,000 user-visible strings: about 690 distinct capitalized literals in
  `crates/engine/engine/src/cmd` and `crates/engine/model/src` (including the command labels) and
  about 320 keys in the editor's `en-US` catalog (`web/src/strings.ts`,
  `web/src/i18n/locales/en-US/ribbon.json`). A catalog for the engine will refine the count; until
  then percentages are against this figure.
- **Script support** is about slide text (what users type and import), separately from the UI:
  bidi and shaping live in `crates/support/text` (UAX #9, HarfRust, UAX #14 line breaking).
- **PowerPoint's languages:** `Contents/Resources/*.lproj` in PowerPoint 16.113.4: ar, cs, da, de,
  el, en, en_GB, es, es_MX, fi, fr, fr_CA, he, hu, id, it, ja, ko, nl, no, pl, pt (Brazil), pt_PT,
  ru, sk, sv, th, tr, zh_CN, zh_TW (26 languages). PowerPoint for **Mac** has no Hindi or Vietnamese
  UI; PowerPoint for Windows has both.

## The twelve key languages

| Language | Code | UI strings translated | Dialogs / tooltips / help | Script support (slide text) | Native review | Status | PowerPoint for Mac UI | Hours to full |
|---|---|---|---|---|---|---|---|---|
| English | en | ~1,000 / ~1,000 (100%) | yes / yes / no help | full | yes | **full** | yes | 0 |
| Simplified Chinese (Mandarin) | zh-Hans | 0 (0%) | no | renders with Droid Sans Fallback, which the editor gives the engine at run time (asset policy excludes Noto CJK from builds); UAX #14 breaking; input methods compose in kit-editor's text field (`TextSink`); no true vertical layout | no | none | yes | 6–10 (incl. IME) |
| Spanish | es | 0 (0%) | no | full (Latin) | no | none | yes (es, es-MX) | 3–5 |
| Hindi | hi | 0 (0%) | no | Devanagari shaping through HarfRust, untested; no bundled Devanagari font | no | none | **no** (Windows only) | 4–7 |
| Arabic | ar | 0 (0%) | no | bidi, contextual shaping, kashida justification, RTL bullets and caret; **UI not mirrored**; browser export shows boxes | no | none | yes | 10–16 (incl. UI mirroring) |
| French | fr | 0 (0%) | no | full (Latin) | no | none | yes (fr, fr-CA) | 3–5 |
| Portuguese | pt | 0 (0%) | no | full (Latin) | no | none | yes (pt-BR, pt-PT) | 3–5 |
| Indonesian | id | 0 (0%) | no | full (Latin) | no | none | yes | 3–5 |
| Japanese | ja | 0 (0%) | no | renders with Droid Sans Fallback from kition-fonts (it covers Japanese); input methods compose in kit-editor's text field; vertical text is rotated, not laid out vertically | no | none | yes | 4–7 |
| German | de | 0 (0%) | no | full (Latin) | no | none | yes | 3–5 |
| Korean | ko | 0 (0%) | no | no kition-fonts face covers Hangul, and the web build has no system fonts; input methods compose in kit-editor's text field | no | none | yes | 4–7 |
| Vietnamese | vi | 0 (0%) | no | stacked diacritics through HarfRust | no | none | **no** (Windows only) | 3–5 |

Other languages shipped: **0**. PowerPoint for Mac ships 16 more
than the twelve above: Czech, Danish, Greek, Finnish, Hebrew, Hungarian, Italian, Dutch, Norwegian,
Polish, Russian, Slovak, Swedish, Thai, Turkish and Traditional Chinese.

Per-language hours assume the infrastructure exists: **8–14 h** once for a string catalog (copy a
sibling app's approach, e.g. Kition Motion's menu and panel catalogs), a language setting that follows
the system, plural and number formatting, and an `xtask` check that every key has a
translation. IME composition (3–6 h) is counted once, under Simplified Chinese.

## Proofing languages

PowerPoint checks spelling in dozens of languages and lets each run carry a proofing language.
Kition Slide keeps `a:rPr/@lang` on round trip but its spelling check is a small English list of
common misspellings (`crates/engine/engine/src/cmd/review.rs`); its feature is `slide.review.proofing` in [gaps.md](../gaps.md).

## Revision history

| Date | Change | Summary |
|---|---|---|
| 2026-11-17 | major | First public version |
