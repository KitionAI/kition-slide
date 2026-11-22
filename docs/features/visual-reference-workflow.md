# Native reference comparison

The strict target is **SSIM >= 0.999 and diff_ratio <= 0.001** for each slide.
The separate blurred layout metric and aggregate means do not replace that target.
Use the original references, pixel sizes, font configuration and frozen date of the
accepted baseline when comparing a rendering change.

## Fixed feature pairs: the default development loop

Use `crates/render/visual/tests/feature-pairs.json` for everyday compatibility work.
It freezes two original reference pages per feature deck, or the sole page of a one-page
sample: currently 315 decks and 526 pages instead of 15,547. Formula pages 2 and 20 and
art text pages 18 and 22 were selected by visual review. Other pairs prioritize substantive
content, feature-type diversity and page spread. Similarity scores never rank selection.
The unchanged original reference images remain the standard every comparison measures
against; current renderer output is only a regression baseline, never a replacement reference.

```sh
KITION_SLIDE_REFERENCE_ROOT="<reference-root>" \
KITION_VISUAL_MANIFEST=crates/render/visual/tests/feature-pairs.json \
KITION_VISUAL_OUT="<new-output-directory>" \
KITION_VISUAL_BASELINE="<accepted-scorecard.json>" \
KITION_VISUAL_GATE=1 \
KITION_VISUAL_GATE_DROP_FAILS=1 \
KITION_VISUAL_DATE=2026-09-20 \
KITION_VISUAL_THREADS=5 \
KITION_VISUAL_BUDGET=7200 \
KITION_VISUAL_PAIRS=1 \
KITION_VISUAL_DIFF=full \
KITION_VISUAL_CACHE="$HOME/.cache/kition-slide/pixel-metrics" \
cargo test --release -p kition-slide-visual --test reference -- --nocapture
```

Do not combine the manifest with `KITION_VISUAL_SAMPLE`. A manifest run never adds extra
pages from a full baseline's passing set. It rejects missing or unknown decks, invalid page
numbers, duplicate selections and receipts from a different selection. A filtered run reports
completion for that filter; every selected page must be compared without import/render errors.
Use a fresh output directory for each changed renderer, or set `KITION_VISUAL_RERUN=1`:
matching selection receipts alone do not prove that the renderer binary is unchanged.

A pair is a quick feature check, not proof that every page is correct. Prioritize missing text,
formula structure, effects, objects and major layout errors before small pixel differences.
The full suite remains available for periodic coverage and wider regression investigations;
it is not the inner loop for each small fix. Keep the strict 99.9% score
visible and report unresolved pages without changing thresholds.

When the corpus changes, regenerate and review the selection explicitly:

```sh
python3 tools/scripts/select-feature-pairs.py \
  --scorecard "<complete-full-scorecard.json>" \
  --output crates/render/visual/tests/feature-pairs.json
PYTHONDONTWRITEBYTECODE=1 python3 tools/scripts/test-select-feature-pairs.py
```

The generator requires a complete, unsampled, successfully compared scorecard. The checked-in
manifest stores ASCII case IDs, page numbers and English reasons, with no corpus assets or
machine paths. Review changed pairs before adopting the newly selected baseline.

## Run a new candidate

Use a new output directory for every candidate. Completed case JSON files are resumable
results: reusing their output directory without `KITION_VISUAL_RERUN=1` skips import and
rendering. This is independent of the pixel metric cache.

```sh
KITION_SLIDE_REFERENCE_ROOT="<reference-root>" \
KITION_VISUAL_OUT="<new-output-directory>" \
KITION_VISUAL_SCORECARD="<new-output-directory>/scorecard.json" \
KITION_VISUAL_BASELINE="<accepted-scorecard.json>" \
KITION_VISUAL_GATE=1 \
KITION_VISUAL_DATE=2026-09-20 \
KITION_VISUAL_THREADS=5 \
KITION_VISUAL_BUDGET=7200 \
KITION_VISUAL_DIFF=none \
KITION_VISUAL_CACHE="$HOME/.cache/kition-slide/pixel-metrics" \
cargo test --release -p kition-slide-visual --test reference -- --nocapture
```

The date above belongs to the current accepted corpus. Use the accepted baseline's
recorded date when measuring a different corpus. Never overwrite the accepted baseline
with an unreviewed candidate. Inspect the completed scorecard, status/count changes,
strict passes gained and lost, per-slide decreases, and font/mapping diagnostics.
A non-regression gate pass alone does not mean every slide meets the strict target.

For a quick all-deck check, add `KITION_VISUAL_SAMPLE=8` and use a separate output
directory. With the gate enabled, sampling also includes every slide that passes in the baseline.
For visual review, set `KITION_VISUAL_PAIRS=1` and `KITION_VISUAL_DIFF=full` on a small
selection. Use the fixed pairs for routine acceptance, with focused regression tests and repository CI.
Run the full corpus periodically or when a change warrants broader investigation.

## Exact pixel metric reuse

`KITION_VISUAL_CACHE=<directory>` opts into local metric reuse. Unset it or set it to
`off` for a direct comparison. Each selected slide is still imported and rendered;
its reference is decoded. Reuse requires the ordered SHA-256 hashes of both current
RGB buffers, dimensions, and the complete metric/build identity to match.

Records preserve raw floating-point bits. Cache hits still recompute pixel differences,
region attribution, model features, font diagnostics, diagnostic images and pass
flags from the current run. Missing, invalid or incompatible entries are recomputed;
cache I/O failures do not suppress normal comparison. The `metric_cache` counters report
hits, misses, invalid entries, writes and write errors separately from scores.

A cold cache can add hashing and I/O overhead. Repeated unchanged pixels avoid the
expensive SSIM and blurred-layout convolutions; changed pixels are measured normally.
Do not seed this cache from rounded scorecard values, timestamps or document IDs.
When auditing cache behavior, compare fresh output directories with cache disabled,
then cold, then warm. Ignore only elapsed/render timings and cache counters when
checking report equality; diagnostic image bytes must also match.

This workflow measures native import/render. WebAssembly compilation, a sampled run,
or native results alone do not establish full browser or edited export/re-import parity.
