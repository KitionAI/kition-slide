//! The corpus run: discover → import → clean render at the reference size → compare → per-case
//! results (resumable) → scorecard.
//!
//! Each finished case is written to `<out>/cases/<id>.json` at once, so a run can be split into
//! time-boxed chunks: a later run skips finished cases (`KITION_VISUAL_RERUN=1` redoes them). A
//! case left with only its `<id>.running` marker was in progress when an earlier run died (a crash
//! or a time limit) and is recorded as `aborted`, never retried silently.
//!
//! With `KITION_VISUAL_GATE=1` the run ends with the non-regression gate ([`crate::gate`]): the
//! cases this selection finished are compared with the accepted baseline scorecard, and a slide
//! that passed there and does not pass now fails the gate.
//!
//! With `KITION_VISUAL_SAMPLE=<n>` each case compares only `n` slides spread evenly over its
//! references (and, with the gate on, every slide the baseline passes, so the quick run still
//! catches a lost pass): the fast feature loop. A sampled run keeps its own output folder and
//! scorecard, so it never mixes with a full run's results.

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::{Duration, Instant};

use serde_json::{Value, json};

use kition_slide_model::SlideFile;
use kition_slide_render::{RenderOpts, render_slide};
use kition_slide_text::datetime::DateTime;

use crate::cache::{self, MetricCache};
use crate::compare::{self, DiffImage, Rgb, round};
use crate::corpus::{self, Case};
use crate::feature_pairs::FeaturePairs;
use crate::gate::{self, GateConfig};
use crate::regions;

/// Run settings (from the environment: see [`Config::from_env`]).
#[derive(Clone, Debug)]
pub struct Config {
    pub root: PathBuf,
    pub out: PathBuf,
    pub scorecard: PathBuf,
    pub suffix: Option<String>,
    pub threads: usize,
    pub filter: Option<String>,
    pub shard: (usize, usize),
    pub budget: Duration,
    pub diff: DiffImage,
    pub derived: bool,
    pub rerun: bool,
    pub system_fonts: bool,
    /// The date every date field shows (`KITION_VISUAL_DATE=YYYY-MM-DD`; default [`FROZEN_DATE`]):
    /// set it to the day the references were rendered.
    pub date: DateTime,
    /// The non-regression gate, when enabled (`KITION_VISUAL_GATE=1`).
    pub gate: Option<GateConfig>,
    /// Slides compared per case (`KITION_VISUAL_SAMPLE`); `None` compares them all.
    pub sample: Option<usize>,
    /// Fixed, reviewed page pairs (`KITION_VISUAL_MANIFEST`); exclusive with sampling.
    pub manifest: Option<PathBuf>,
    /// Also save each reference and its render side by side (`KITION_VISUAL_PAIRS=1`).
    pub pairs: bool,
    /// Explicit directory for exact RGB metric reuse; disabled by default.
    pub cache: Option<PathBuf>,
}

impl Config {
    /// `KITION_SLIDE_REFERENCE_ROOT` (required), `KITION_VISUAL_OUT` (per-case results and diff
    /// images; default `$HOME/.cache/vis/slide`), `KITION_VISUAL_SCORECARD` (default
    /// `$HOME/.cache/kition-fidelity/slide-visual.json`), `KITION_SLIDE_REFERENCE_SUFFIX`,
    /// `KITION_VISUAL_THREADS` (default 2), `KITION_VISUAL_FILTER` (substring of the relative
    /// path), `KITION_VISUAL_SHARD` (`i/n`), `KITION_VISUAL_BUDGET` (seconds after which no new
    /// case starts; default 480), `KITION_VISUAL_DIFF` (`half` default, `full`, `none`),
    /// `KITION_VISUAL_DERIVED=1` (also the derived set), `KITION_VISUAL_RERUN=1`,
    /// `KITION_VISUAL_FONTS=bundled` (no system font fallback, closer to the web build),
    /// `KITION_VISUAL_DATE=YYYY-MM-DD` (what date fields show), `KITION_VISUAL_SAMPLE=<n>` (slides
    /// per case; output then defaults to `$HOME/.cache/vis/slide-sample<n>` and the scorecard to
    /// `<out>/scorecard.json`), `KITION_VISUAL_PAIRS=1` (also `slide-<n>-pair.png`: the reference
    /// left, the render right). `KITION_VISUAL_CACHE=<directory>` enables exact RGB metric
    /// reuse; unset or `off` disables it. Use a fresh output directory or rerun for each new
    /// candidate: case resumability still skips completed cases before rendering.
    /// `KITION_VISUAL_MANIFEST=<file>` uses reviewed fixed feature pairs, defaults output to
    /// `$HOME/.cache/vis/slide-feature-pairs`, and cannot be combined with sampling. Empty or
    /// invalid manifest paths fail rather than silently starting a full run.
    ///
    /// Gate: `KITION_VISUAL_GATE=1` compares this run with `KITION_VISUAL_BASELINE` (default
    /// `$HOME/.cache/kition-fidelity/slide-visual.json`) and fails on a lost pass;
    /// `KITION_VISUAL_GATE_DROP` (default 0.05) is the SSIM drop reported as a warning, and
    /// `KITION_VISUAL_GATE_DROP_FAILS=1` makes those drops fail too. With the gate on, the
    /// scorecard defaults to `<out>/scorecard.json` so a gated run never overwrites its baseline
    /// (a scorecard path equal to the baseline is refused).
    pub fn from_env() -> Option<Config> {
        let var = |k: &str| std::env::var(k).ok().filter(|v| !v.is_empty());
        let root = PathBuf::from(var("KITION_SLIDE_REFERENCE_ROOT")?);
        let home = PathBuf::from(var("HOME").unwrap_or_else(|| ".".into()));
        let shard = var("KITION_VISUAL_SHARD")
            .and_then(|s| {
                let (i, n) = s.split_once('/')?;
                Some((i.parse().ok()?, n.parse().ok()?))
            })
            .filter(|&(i, n): &(usize, usize)| n > 0 && i < n)
            .unwrap_or((0, 1));
        let accepted = home.join(".cache/kition-fidelity/slide-visual.json");
        let sample = var("KITION_VISUAL_SAMPLE").and_then(|v| v.parse::<usize>().ok()).filter(|&n| n > 0);
        let manifest = std::env::var_os("KITION_VISUAL_MANIFEST").map(PathBuf::from);
        let out = var("KITION_VISUAL_OUT").map(PathBuf::from).unwrap_or_else(|| {
            if manifest.is_some() {
                home.join(".cache/vis/slide-feature-pairs")
            } else {
                match sample {
                    Some(n) => home.join(format!(".cache/vis/slide-sample{n}")),
                    None => home.join(".cache/vis/slide"),
                }
            }
        });
        let gate = var("KITION_VISUAL_GATE").is_some_and(|v| v == "1").then(|| GateConfig {
            baseline: var("KITION_VISUAL_BASELINE").map(PathBuf::from).unwrap_or_else(|| accepted.clone()),
            drop: var("KITION_VISUAL_GATE_DROP").and_then(|v| v.parse().ok()).unwrap_or(gate::DEFAULT_DROP),
            drop_fails: var("KITION_VISUAL_GATE_DROP_FAILS").is_some_and(|v| v == "1"),
        });
        let scorecard = var("KITION_VISUAL_SCORECARD")
            .map(PathBuf::from)
            .unwrap_or_else(|| if gate.is_some() || sample.is_some() || manifest.is_some() { out.join("scorecard.json") } else { accepted });
        Some(Config {
            root,
            out,
            scorecard,
            suffix: var("KITION_SLIDE_REFERENCE_SUFFIX"),
            threads: var("KITION_VISUAL_THREADS").and_then(|v| v.parse().ok()).unwrap_or(2).max(1),
            filter: var("KITION_VISUAL_FILTER"),
            shard,
            budget: Duration::from_secs(var("KITION_VISUAL_BUDGET").and_then(|v| v.parse().ok()).unwrap_or(480)),
            diff: match var("KITION_VISUAL_DIFF").as_deref() {
                Some("none") => DiffImage::None,
                Some("full") => DiffImage::Full,
                _ => DiffImage::Half,
            },
            derived: var("KITION_VISUAL_DERIVED").is_some_and(|v| v == "1"),
            rerun: var("KITION_VISUAL_RERUN").is_some_and(|v| v == "1"),
            system_fonts: var("KITION_VISUAL_FONTS").as_deref() != Some("bundled"),
            date: var("KITION_VISUAL_DATE").and_then(|d| parse_date(&d)).unwrap_or(FROZEN_DATE),
            gate,
            sample,
            manifest,
            pairs: var("KITION_VISUAL_PAIRS").is_some_and(|v| v == "1"),
            cache: cache::directory(var("KITION_VISUAL_CACHE")),
        })
    }
}

fn parse_date(s: &str) -> Option<DateTime> {
    let mut it = s.split('-').map(|p| p.parse::<u32>().ok());
    let (y, m, d) = (it.next()??, it.next()??, it.next()??);
    ((1..=12).contains(&m) && (1..=31).contains(&d)).then_some(DateTime { year: y as i32, month: m, day: d, hour: 0, minute: 0, second: 0 })
}

/// The default date every date field shows, so runs are reproducible.
pub const FROZEN_DATE: DateTime = DateTime { year: 2026, month: 1, day: 1, hour: 0, minute: 0, second: 0 };

thread_local! {
    static PANICS: std::cell::Cell<u32> = const { std::cell::Cell::new(0) };
}

fn install_panic_counter() {
    static ONCE: std::sync::Once = std::sync::Once::new();
    ONCE.call_once(|| {
        let previous = std::panic::take_hook();
        std::panic::set_hook(Box::new(move |info| {
            PANICS.with(|p| p.set(p.get() + 1));
            if std::env::var_os("KITION_VISUAL_PANIC_TRACE").is_some() {
                previous(info);
            } else {
                eprintln!("panic: {}", info.to_string().lines().next().unwrap_or(""));
            }
        }));
    });
}

fn panics() -> u32 {
    PANICS.with(|p| p.get())
}

/// Clean (show-mode) render of slide `index` at `width` x `height`: no placeholder prompts or
/// outlines, no editor overlays, hidden shapes hidden, date fields frozen.
pub fn render_clean(pres: &SlideFile, index: usize, width: u32, height: u32, date: DateTime) -> Rgb {
    let scale = width as f64 / pres.slide_size.width.max(1.0);
    let opts = RenderOpts { scale, edit: false, size: Some((width, height)), now: Some(date), ..Default::default() };
    let img = render_slide(pres, index, &opts);
    if (img.width, img.height) != (width, height) || img.pixels.len() != (width * height * 4) as usize {
        return Rgb::new(width, height, [255; 3]);
    }
    Rgb::from_premultiplied_rgba(img.width, img.height, &img.pixels)
}

fn import(path: &Path) -> Result<SlideFile, String> {
    let bytes = std::fs::read(path).map_err(|e| format!("read: {e}"))?;
    match std::panic::catch_unwind(|| kition_slide_pptx::import(&bytes)) {
        Ok(Ok(p)) => Ok(p),
        Ok(Err(e)) => Err(format!("import error: {e}")),
        Err(_) => Err("import panicked".into()),
    }
}

/// Run one case: every reference slide compared with its render.
pub fn run_case(case: &Case, cfg: &Config) -> Value {
    run_case_slides(case, cfg, None)
}

/// Run one case on the reference slides numbered in `only` (all of them when `None`); a case run
/// on a subset is marked `sampled`, so the gate checks only the slides it compared.
pub fn run_case_slides(case: &Case, cfg: &Config, only: Option<&BTreeSet<u32>>) -> Value {
    let started = Instant::now();
    let mut result = json!({
        "id": case.id, "path": case.rel, "category": case.category, "set": if case.derived { "derived" } else { "original" },
        "references": case.references.len(),
    });
    if only.is_some() {
        result["sampled"] = json!(true);
    }
    let references: Vec<&(u32, PathBuf)> = case.references.iter().filter(|(n, _)| only.is_none_or(|o| o.contains(n))).collect();
    let pres = match import(&case.slide_file) {
        Ok(p) => p,
        Err(e) => {
            result["status"] = json!("open-failed");
            result["error"] = json!(e);
            result["slides"] = json!(references.iter().map(|(n, _)| json!({ "n": n, "status": "unmapped", "pass": false })).collect::<Vec<_>>());
            return result;
        }
    };
    let visible: Vec<usize> = (0..pres.slides.len()).filter(|&i| !pres.slides[i].hidden).collect();
    // Reference n is slide n, unless the references match the visible slides only.
    let (mapping, mode) = if case.references.len() == pres.slides.len() {
        ((0..pres.slides.len()).collect::<Vec<_>>(), "physical")
    } else if case.references.len() == visible.len() {
        (visible.clone(), "visible")
    } else {
        ((0..pres.slides.len()).collect::<Vec<_>>(), "physical-mismatch")
    };
    result["model_slides"] = json!(pres.slides.len());
    result["hidden_slides"] = json!(pres.slides.len() - visible.len());
    result["mapping"] = json!(mode);
    result["slide_size_pt"] = json!([pres.slide_size.width, pres.slide_size.height]);
    result["missing_fonts"] = json!(regions::missing_fonts(&pres));
    result["font_substitutions"] = json!(regions::substitutions(&pres));
    let dir = cfg.out.join("cases").join(&case.id);
    if cfg.diff != DiffImage::None {
        let _ = std::fs::create_dir_all(&dir);
    }
    let mut slides = Vec::new();
    let mut render_ms = 0u128;
    let metric_cache = cfg.cache.as_deref().map(MetricCache::new);
    let mut cache_stats = cache::Stats::default();
    for (n, ref_path) in references {
        let mut s = json!({ "n": n });
        let index = (*n as usize).checked_sub(1).and_then(|i| mapping.get(i).copied());
        let Some(index) = index else {
            s["status"] = json!("unmapped");
            s["pass"] = json!(false);
            slides.push(s);
            continue;
        };
        s["slide"] = json!(index + 1);
        let reference = match Rgb::load(ref_path) {
            Ok(r) => r,
            Err(e) => {
                s["status"] = json!("reference-unreadable");
                s["error"] = json!(e);
                s["pass"] = json!(false);
                slides.push(s);
                continue;
            }
        };
        let (w, h) = (reference.width, reference.height);
        let scale = w as f64 / pres.slide_size.width.max(1.0);
        s["size"] = json!([w, h]);
        s["aspect_px"] = json!(round(pres.slide_size.height * scale - h as f64, 2));
        let before = panics();
        let t = Instant::now();
        let actual = render_clean(&pres, index, w, h, cfg.date);
        render_ms += t.elapsed().as_millis();
        s["render_ms"] = json!(t.elapsed().as_millis() as u64);
        if panics() != before {
            s["render_panicked"] = json!(true);
        }
        match cache::compare_pair(&reference, &actual, metric_cache.as_ref()) {
            Ok(pair) => {
                cache_stats.add(pair.event);
                let c = pair.strict;
                s["status"] = json!("compared");
                if let Value::Object(m) = c.metrics.to_json() {
                    for (k, v) in m {
                        s[k] = v;
                    }
                }
                match pair.layout {
                    Ok(l) => {
                        s["layout"] = l.to_json();
                        s["pass_layout"] = json!(l.pass());
                    }
                    Err(e) => s["layout_error"] = json!(e),
                }
                let slide = &pres.slides[index];
                s["regions"] = regions::map_json(&regions::attribute(&pres, slide, scale, scale, w, h, &c.max_diff, compare::DIFF_THRESHOLD));
                s["features"] = regions::map_json(&regions::features(&pres, slide));
                if let Some(img) = compare::diff_image(&reference, &actual, &c.max_diff, cfg.diff) {
                    let file = dir.join(format!("slide-{n:04}-diff.png"));
                    if img.save_png(&file).is_ok() {
                        s["diff_image"] = json!(file.strip_prefix(&cfg.out).unwrap_or(&file).to_string_lossy());
                    }
                }
                if cfg.pairs {
                    let _ = std::fs::create_dir_all(&dir);
                    let file = dir.join(format!("slide-{n:04}-pair.png"));
                    if Rgb::side_by_side(&reference, &actual).save_png(&file).is_ok() {
                        s["pair_image"] = json!(file.strip_prefix(&cfg.out).unwrap_or(&file).to_string_lossy());
                    }
                }
            }
            Err(e) => {
                s["status"] = json!("compare-failed");
                s["error"] = json!(e);
                s["pass"] = json!(false);
            }
        }
        slides.push(s);
    }
    if metric_cache.is_some() {
        result["metric_cache"] = cache_stats.json();
    }
    result["status"] = json!(if mode == "physical-mismatch" { "count-mismatch" } else { "ok" });
    result["render_ms"] = json!(render_ms as u64);
    result["elapsed_ms"] = json!(started.elapsed().as_millis() as u64);
    result["slides"] = json!(slides);
    result
}

/// `k` of the reference numbers `refs` (sorted), spread evenly: the middle of each of `k` equal
/// runs, so the pick is stable and covers the slide file from start to end. All of them when `k` covers
/// the slide file.
pub fn spread(refs: &[u32], k: usize) -> BTreeSet<u32> {
    if k >= refs.len() {
        return refs.iter().copied().collect();
    }
    (0..k).filter_map(|i| refs.get((2 * i + 1) * refs.len() / (2 * k)).copied()).collect()
}

/// Per case id, the slide numbers the baseline scorecard passes.
fn baseline_passes(path: &Path) -> std::collections::HashMap<String, BTreeSet<u32>> {
    let Some(baseline) = std::fs::read(path).ok().and_then(|b| serde_json::from_slice::<Value>(&b).ok()) else { return Default::default() };
    let mut out = std::collections::HashMap::new();
    for case in baseline["cases"].as_array().into_iter().flatten() {
        let Some(id) = case["id"].as_str() else { continue };
        let passes: BTreeSet<u32> = case["slides"]
            .as_array()
            .into_iter()
            .flatten()
            .filter(|s| s["pass"] == Value::Bool(true))
            .filter_map(|s| s["n"].as_u64().and_then(|n| u32::try_from(n).ok()))
            .collect();
        if !passes.is_empty() {
            out.insert(id.to_string(), passes);
        }
    }
    out
}

fn case_paths(cfg: &Config, case: &Case) -> (PathBuf, PathBuf) {
    let d = cfg.out.join("cases");
    (d.join(format!("{}.json", case.id)), d.join(format!("{}.running", case.id)))
}

/// Outcome of [`run`].
pub struct Outcome {
    pub ran: usize,
    pub pending: usize,
    pub scorecard: Value,
    /// The gate's report, when the gate is on.
    pub gate: Option<gate::Report>,
}

/// Run the corpus (the cases this shard and filter select) within the budget and write the
/// scorecard over every finished case.
pub fn run(cfg: &Config) -> Result<Outcome, String> {
    install_panic_counter();
    if cfg.manifest.is_some() && cfg.sample.is_some() {
        return Err("KITION_VISUAL_MANIFEST and KITION_VISUAL_SAMPLE cannot be combined".into());
    }
    if let Some(g) = &cfg.gate
        && same_file(&g.baseline, &cfg.scorecard)
    {
        return Err(format!("the scorecard {} is the gate's baseline: set KITION_VISUAL_SCORECARD elsewhere", cfg.scorecard.display()));
    }
    if !cfg.system_fonts {
        kition_slide_fonts::FontDb::global().set_system_fallback(false);
    }
    let cases: Vec<Case> = corpus::discover(&cfg.root, cfg.suffix.as_deref()).into_iter().filter(|c| cfg.derived || !c.derived).collect();
    if cases.is_empty() {
        return Err(format!("no cases under {}", cfg.root.display()));
    }
    let pairs = cfg.manifest.as_deref().map(|path| FeaturePairs::load(path, &cases)).transpose()?;
    std::fs::create_dir_all(cfg.out.join("cases")).map_err(|e| e.to_string())?;
    let selected: Vec<&Case> = cases
        .iter()
        .enumerate()
        .filter(|(i, c)| i % cfg.shard.1 == cfg.shard.0 && cfg.filter.as_ref().is_none_or(|f| c.rel.contains(f.as_str())))
        .map(|(_, c)| c)
        .collect();
    if pairs.is_some() && selected.is_empty() {
        return Err("the feature manifest filter selects no decks".into());
    }
    // A frozen pair never grows to include all passes from a full-corpus baseline.
    let passes = match (&cfg.gate, cfg.sample) {
        (Some(g), Some(_)) => baseline_passes(&g.baseline),
        _ => Default::default(),
    };
    let pick = |case: &Case| -> Option<BTreeSet<u32>> {
        if let Some(pairs) = &pairs {
            return pairs.pages.get(&case.id).cloned();
        }
        let k = cfg.sample?;
        let refs: Vec<u32> = case.references.iter().map(|(n, _)| *n).collect();
        let mut only = spread(&refs, k);
        only.extend(passes.get(&case.id).into_iter().flatten().copied());
        Some(only)
    };
    // Cases an earlier run died in.
    for c in &selected {
        let (done, running) = case_paths(cfg, c);
        if let Some(pairs) = &pairs
            && done.exists()
            && !cfg.rerun
        {
            let bytes = std::fs::read(&done).map_err(|e| e.to_string())?;
            let result: Value = serde_json::from_slice(&bytes).map_err(|e| e.to_string())?;
            pairs.validate_result(&c.id, &result)?;
        }
        if running.exists() && (!done.exists() || cfg.rerun) {
            let only = pick(c);
            let mut v = json!({
                "id": c.id, "path": c.rel, "category": c.category, "set": if c.derived { "derived" } else { "original" },
                "references": c.references.len(), "status": "aborted",
                "error": "an earlier run stopped inside this case (crash or time limit)",
                "slides": c.references.iter().filter(|(n, _)| only.as_ref().is_none_or(|o| o.contains(n)))
                    .map(|(n, _)| json!({ "n": n, "status": "aborted", "pass": false })).collect::<Vec<_>>(),
            });
            if let Some(pairs) = &pairs {
                v["sampled"] = json!(true);
                v["selection"] = pairs.receipt(&c.id);
            }
            let _ = std::fs::write(&done, serde_json::to_vec_pretty(&v).unwrap_or_default());
            let _ = std::fs::remove_file(&running);
        }
    }
    let todo: Vec<&Case> = selected.iter().copied().filter(|c| cfg.rerun || !case_paths(cfg, c).0.exists()).collect();
    let start = Instant::now();
    let next = AtomicUsize::new(0);
    let ran = AtomicUsize::new(0);
    let log = Mutex::new(());
    std::thread::scope(|scope| {
        for _ in 0..cfg.threads {
            scope.spawn(|| {
                loop {
                    if start.elapsed() > cfg.budget {
                        break;
                    }
                    let i = next.fetch_add(1, Ordering::SeqCst);
                    let Some(case) = todo.get(i) else { break };
                    let (done, running) = case_paths(cfg, case);
                    let _ = std::fs::write(&running, b"");
                    let mut v = run_case_slides(case, cfg, pick(case).as_ref());
                    if let Some(pairs) = &pairs {
                        v["selection"] = pairs.receipt(&case.id);
                    }
                    let _ = std::fs::write(&done, serde_json::to_vec_pretty(&v).unwrap_or_default());
                    let _ = std::fs::remove_file(&running);
                    ran.fetch_add(1, Ordering::SeqCst);
                    let _g = log.lock();
                    let slides = v["slides"].as_array().map(Vec::len).unwrap_or(0);
                    let pass = v["slides"].as_array().map(|s| s.iter().filter(|x| x["pass"] == json!(true)).count()).unwrap_or(0);
                    eprintln!("[{:>4}s] {} {} {pass}/{slides} pass, {} ms", start.elapsed().as_secs(), case.id, v["status"], v["elapsed_ms"]);
                }
            });
        }
    });
    let ran = ran.load(Ordering::SeqCst);
    let pending = todo.len().saturating_sub(ran);
    let scope: Vec<Case> = if pairs.is_some() { selected.iter().map(|c| (*c).clone()).collect() } else { cases.clone() };
    let mut scorecard = scorecard(cfg, &scope);
    if let Some(pairs) = &pairs {
        scorecard["selection"] = json!({
            "kind": "feature-pairs-v1", "fingerprint": pairs.fingerprint,
            "decks": selected.len(), "pages": selected.iter().filter_map(|c| pairs.pages.get(&c.id)).map(BTreeSet::len).sum::<usize>()
        });
        for case in scorecard["cases"].as_array().into_iter().flatten() {
            pairs.validate_result(case["id"].as_str().unwrap_or(""), case)?;
        }
    }
    if let Some(parent) = cfg.scorecard.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    std::fs::write(&cfg.scorecard, serde_json::to_vec_pretty(&scorecard).unwrap_or_default()).map_err(|e| e.to_string())?;
    let gate = match &cfg.gate {
        Some(g) => {
            let current: Vec<Value> =
                selected.iter().filter_map(|c| std::fs::read(case_paths(cfg, c).0).ok().and_then(|b| serde_json::from_slice(&b).ok())).collect();
            Some(gate::check(g, &current, compare::METRIC_VERSION)?)
        }
        None => None,
    };
    Ok(Outcome { ran, pending, scorecard, gate })
}

fn same_file(a: &Path, b: &Path) -> bool {
    a == b || matches!((a.canonicalize(), b.canonicalize()), (Ok(x), Ok(y)) if x == y)
}

fn median(v: &mut [f64]) -> Option<f64> {
    if v.is_empty() {
        return None;
    }
    v.sort_by(f64::total_cmp);
    Some(v[v.len() / 2])
}

/// Totals of a group of slides.
#[derive(Default)]
struct Tally {
    cases: usize,
    cases_all_pass: usize,
    slides: usize,
    compared: usize,
    pass: usize,
    pass_local: usize,
    pass_layout: usize,
    ssim_pass: usize,
    diff_pass: usize,
    ssim: Vec<f64>,
    diff_ratio: Vec<f64>,
    layout_ssim: Vec<f64>,
    layout_diff_ratio: Vec<f64>,
    regions: std::collections::BTreeMap<String, u64>,
    failing_features: std::collections::BTreeMap<String, u64>,
    status: std::collections::BTreeMap<String, u64>,
    panics: usize,
}

const SSIM_BUCKETS: &[f64] = &[0.999, 0.99, 0.98, 0.95, 0.9, 0.8, 0.0];

impl Tally {
    fn add_case(&mut self, case: &Value) {
        self.cases += 1;
        *self.status.entry(case["status"].as_str().unwrap_or("?").to_string()).or_default() += 1;
        let slides = case["slides"].as_array().cloned().unwrap_or_default();
        if !slides.is_empty() && slides.iter().all(|s| s["pass"] == json!(true)) {
            self.cases_all_pass += 1;
        }
        for s in &slides {
            self.slides += 1;
            if s["render_panicked"] == json!(true) {
                self.panics += 1;
            }
            let Some(ssim) = s["ssim"].as_f64() else { continue };
            let diff = s["diff_ratio"].as_f64().unwrap_or(1.0);
            self.compared += 1;
            self.ssim.push(ssim);
            self.diff_ratio.push(diff);
            if ssim >= compare::PASS_SSIM {
                self.ssim_pass += 1;
            }
            if diff <= compare::PASS_DIFF_RATIO {
                self.diff_pass += 1;
            }
            if s["pass_local"] == json!(true) {
                self.pass_local += 1;
            }
            if let Some(l) = s["layout"]["ssim"].as_f64() {
                self.layout_ssim.push(l);
                self.layout_diff_ratio.push(s["layout"]["diff_ratio"].as_f64().unwrap_or(1.0));
            }
            if s["pass_layout"] == json!(true) {
                self.pass_layout += 1;
            }
            if s["pass"] == json!(true) {
                self.pass += 1;
                continue;
            }
            if let Some(m) = s["regions"].as_object() {
                for (k, v) in m {
                    *self.regions.entry(k.clone()).or_default() += v.as_u64().unwrap_or(0);
                }
            }
            if let Some(m) = s["features"].as_object() {
                for k in m.keys() {
                    *self.failing_features.entry(k.clone()).or_default() += 1;
                }
            }
        }
    }

    fn json(&self) -> Value {
        let mut ssim = self.ssim.clone();
        let mut diff = self.diff_ratio.clone();
        let mut layout_ssim = self.layout_ssim.clone();
        let mut layout_diff = self.layout_diff_ratio.clone();
        let mut hist = serde_json::Map::new();
        let mut lower = f64::INFINITY;
        for &b in SSIM_BUCKETS {
            let n = self.ssim.iter().filter(|&&v| v >= b && v < lower).count();
            hist.insert(if lower.is_infinite() { format!(">={b}") } else { format!("[{b},{lower})") }, json!(n));
            lower = b;
        }
        let total_diff: u64 = self.regions.values().sum();
        json!({
            "cases": self.cases,
            "cases_all_slides_pass": self.cases_all_pass,
            "case_status": self.status,
            "slides": self.slides,
            "compared": self.compared,
            "pass": self.pass,
            "pass_rate": if self.slides > 0 { round(self.pass as f64 / self.slides as f64, 4) } else { 0.0 },
            "pass_local": self.pass_local,
            "pass_layout": self.pass_layout,
            "layout_ssim_median": median(&mut layout_ssim).map(|v| round(v, 5)),
            "layout_diff_ratio_median": median(&mut layout_diff).map(|v| round(v, 5)),
            "ssim_pass": self.ssim_pass,
            "diff_pass": self.diff_pass,
            "render_panics": self.panics,
            "ssim_mean": if ssim.is_empty() { Value::Null } else { json!(round(ssim.iter().sum::<f64>() / ssim.len() as f64, 5)) },
            "ssim_median": median(&mut ssim).map(|v| round(v, 5)),
            "ssim_min": ssim.first().map(|v| round(*v, 5)),
            "ssim_histogram": hist,
            "diff_ratio_median": median(&mut diff).map(|v| round(v, 5)),
            "failing_diff_pixels_by_region": self.regions.iter().map(|(k, v)| (k.clone(), json!({ "pixels": v, "share": if total_diff > 0 { round(*v as f64 / total_diff as f64, 4) } else { 0.0 } }))).collect::<serde_json::Map<_, _>>(),
            "failing_slides_by_feature": self.failing_features,
        })
    }
}

/// The scorecard over every finished case of `cases`.
pub fn scorecard(cfg: &Config, cases: &[Case]) -> Value {
    let mut sets: std::collections::BTreeMap<&str, Tally> = Default::default();
    let mut cats: std::collections::BTreeMap<String, Tally> = Default::default();
    let mut list = Vec::new();
    let mut pending = 0;
    // Requested family -> family drawn instead -> slides.
    let mut subs: std::collections::BTreeMap<String, std::collections::BTreeMap<String, u64>> = Default::default();
    for c in cases {
        let (done, _) = case_paths(cfg, c);
        let Some(v) = std::fs::read(&done).ok().and_then(|b| serde_json::from_slice::<Value>(&b).ok()) else {
            pending += 1;
            continue;
        };
        let set = if c.derived { "derived" } else { "original" };
        sets.entry(set).or_default().add_case(&v);
        if !c.derived {
            cats.entry(c.category.clone()).or_default().add_case(&v);
            for (requested, used) in v["font_substitutions"].as_object().into_iter().flatten() {
                *subs.entry(requested.clone()).or_default().entry(used.as_str().unwrap_or("?").to_string()).or_default() += 1;
            }
        }
        list.push(v);
    }
    let mut result = json!({
        "tool": "kition-slide-visual",
        "metric": compare::manifest(),
        "renderer": {
            "crate_version": env!("CARGO_PKG_VERSION"),
            "mode": "clean show-mode render (RenderOpts edit: false), native",
            "fonts": if cfg.system_fonts { "bundled fonts + installed system fonts (native default)" } else { "bundled fonts only for fallback (system fallback off)" },
            "date_fields": format!("frozen at {:04}-{:02}-{:02} 00:00", cfg.date.year, cfg.date.month, cfg.date.day),
            "platform": format!("{}-{}", std::env::consts::OS, std::env::consts::ARCH),
        },
        "complete": pending == 0,
        "pending_cases": pending,
        "sets": sets.iter().map(|(k, t)| (k.to_string(), t.json())).collect::<serde_json::Map<_, _>>(),
        "font_substitutions": subs,
        "categories": cats.iter().map(|(k, t)| (k.clone(), t.json())).collect::<serde_json::Map<_, _>>(),
        "cases": list,
    });
    if cfg.cache.is_some() {
        let mut totals = serde_json::Map::new();
        for key in ["hits", "misses", "invalid", "writes", "write_errors"] {
            let total: u64 = result["cases"].as_array().into_iter().flatten().filter_map(|case| case["metric_cache"][key].as_u64()).sum();
            totals.insert(key.into(), json!(total));
        }
        result["metric_cache"] = Value::Object(totals);
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn spread_picks_the_middle_of_equal_runs() {
        let refs: Vec<u32> = (1..=10).collect();
        assert_eq!(spread(&refs, 2).into_iter().collect::<Vec<_>>(), vec![3, 8]);
        assert_eq!(spread(&refs, 1).into_iter().collect::<Vec<_>>(), vec![6]);
        assert_eq!(spread(&refs, 20).len(), 10);
        assert!(spread(&[], 2).is_empty());
    }
}
