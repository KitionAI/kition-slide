//! The visual-fidelity run with slides drawn by the WebAssembly engine, as the web editor draws
//! them: `renderSlideClean` under node (`web/scripts/visual-render.mjs`), with only the fonts the
//! editor has. Everything else is the native run's: the corpus, the slide mapping, the comparator
//! and metric, region attribution, and the scorecard.
//!
//! ```text
//! (cd web && pnpm engine)   # or KITION_VISUAL_WASM_ENGINE=<dir of kition_slide_kit.js>
//! KITION_SLIDE_REFERENCE_ROOT=<references> KITION_VISUAL_SHARD=0/10 \
//!   cargo run --release -p kition-slide-visual --example wasm
//! ```
//!
//! Takes the native run's variables (`runner::Config::from_env`), except that results go to
//! `KITION_VISUAL_OUT` (default `$HOME/.cache/vis/slide-wasm`) and the scorecard to
//! `KITION_VISUAL_SCORECARD` (default `<out>/scorecard.json`), never the native baseline.
//! `KITION_VISUAL_NODE` names the node binary (default `node`). A slide on which the engine traps
//! (a panic, which would end the editor's worker) is recorded as `wasm-aborted`; the rest of the
//! slide renders in a fresh process.

use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Instant;

use kition_slide_model::SlideFile;
use kition_slide_visual::compare::{self, DiffImage, Rgb, round};
use kition_slide_visual::corpus::{self, Case};
use kition_slide_visual::regions;
use kition_slide_visual::runner::{self, Config};
use serde_json::{Value, json};

/// What node said about one slide.
enum Drawn {
    Ok(u64),
    Error(String),
    Aborted(String),
}

struct Job {
    index: usize,
    width: u32,
    height: u32,
    out: PathBuf,
}

fn main() {
    let Some(mut cfg) = Config::from_env() else {
        eprintln!("set KITION_SLIDE_REFERENCE_ROOT to the reference renders");
        std::process::exit(2);
    };
    let var = |k: &str| std::env::var(k).ok().filter(|v| !v.is_empty());
    let home = PathBuf::from(var("HOME").unwrap_or_else(|| ".".into()));
    cfg.out = var("KITION_VISUAL_OUT").map(PathBuf::from).unwrap_or_else(|| home.join(".cache/vis/slide-wasm"));
    cfg.scorecard = var("KITION_VISUAL_SCORECARD").map(PathBuf::from).unwrap_or_else(|| cfg.out.join("scorecard.json"));
    let node = var("KITION_VISUAL_NODE").unwrap_or_else(|| "node".into());
    let script = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../../web/scripts/visual-render.mjs");

    let cases: Vec<Case> = corpus::discover(&cfg.root, cfg.suffix.as_deref()).into_iter().filter(|c| cfg.derived || !c.derived).collect();
    let selected: Vec<Case> = cases
        .into_iter()
        .enumerate()
        .filter(|(i, c)| i % cfg.shard.1 == cfg.shard.0 && cfg.filter.as_ref().is_none_or(|f| c.rel.contains(f.as_str())))
        .map(|(_, c)| c)
        .collect();
    let _ = std::fs::create_dir_all(cfg.out.join("cases"));
    let done = |c: &Case| cfg.out.join("cases").join(format!("{}.json", c.id));
    let todo: Vec<&Case> = selected.iter().filter(|c| cfg.rerun || !done(c).exists()).collect();
    eprintln!("{} cases, {} to run", selected.len(), todo.len());

    let start = Instant::now();
    let next = AtomicUsize::new(0);
    std::thread::scope(|scope| {
        for _ in 0..cfg.threads {
            scope.spawn(|| {
                loop {
                    if start.elapsed() > cfg.budget {
                        break;
                    }
                    let Some(case) = todo.get(next.fetch_add(1, Ordering::SeqCst)) else { break };
                    let v = run_case(case, &cfg, &node, &script);
                    let _ = std::fs::write(done(case), serde_json::to_vec_pretty(&v).unwrap_or_default());
                    let slides = v["slides"].as_array().map(Vec::len).unwrap_or(0);
                    let pass = v["slides"].as_array().map(|s| s.iter().filter(|x| x["pass"] == json!(true)).count()).unwrap_or(0);
                    eprintln!("[{:>4}s] {} {} {pass}/{slides} pass, {} ms", start.elapsed().as_secs(), case.id, v["status"], v["elapsed_ms"]);
                }
            });
        }
    });

    let mut card = runner::scorecard(&cfg, &selected);
    card["renderer"]["mode"] = json!("clean show-mode render, WebAssembly engine (renderSlideClean under node)");
    card["renderer"]["fonts"] = json!("the engine's built-in faces plus Droid Sans Fallback, as the web editor (no system fonts)");
    card["renderer"]["platform"] = json!("wasm32");
    if let Some(parent) = cfg.scorecard.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    if let Err(e) = std::fs::write(&cfg.scorecard, serde_json::to_vec_pretty(&card).unwrap_or_default()) {
        eprintln!("scorecard: {e}");
        std::process::exit(1);
    }
    let set = &card["sets"]["original"];
    eprintln!("scorecard {}: {set}", cfg.scorecard.display());
}

fn import(path: &Path) -> Result<SlideFile, String> {
    let bytes = std::fs::read(path).map_err(|e| format!("read: {e}"))?;
    match std::panic::catch_unwind(|| kition_slide_pptx::import(&bytes)) {
        Ok(Ok(p)) => Ok(p),
        Ok(Err(e)) => Err(format!("import error: {e}")),
        Err(_) => Err("import panicked".into()),
    }
}

/// Draws `jobs` with node, restarting after a trap; one answer per job.
fn draw(jobs: &[Job], slide_file: &Path, cfg: &Config, node: &str, script: &Path, work: &Path) -> Vec<Drawn> {
    let mut drawn: Vec<Option<Drawn>> = jobs.iter().map(|_| None).collect();
    let mut from = 0;
    while from < jobs.len() {
        let rest = &jobs[from..];
        let spec = json!({
            "slideFile": slide_file,
            "date": [cfg.date.year, cfg.date.month, cfg.date.day],
            "slides": rest.iter().map(|j| json!({ "index": j.index, "width": j.width, "height": j.height, "out": j.out })).collect::<Vec<_>>(),
        });
        let file = work.join("job.json");
        let _ = std::fs::write(&file, spec.to_string());
        let output = Command::new(node).arg(script).arg(&file).output();
        let stdout = match &output {
            Ok(o) => String::from_utf8_lossy(&o.stdout).into_owned(),
            Err(e) => {
                for d in drawn.iter_mut().skip(from) {
                    *d = Some(Drawn::Error(format!("node: {e}")));
                }
                break;
            }
        };
        let mut answered = 0;
        let mut aborted = false;
        for line in stdout.lines() {
            let mut parts = line.splitn(3, ' ');
            let (kind, index, rest_text) = (parts.next().unwrap_or(""), parts.next().unwrap_or(""), parts.next().unwrap_or("").to_string());
            if index == "-1" {
                // The slide file did not open: every slide gets the same answer.
                for d in drawn.iter_mut().skip(from) {
                    *d = Some(if kind == "abort" { Drawn::Aborted(rest_text.clone()) } else { Drawn::Error(rest_text.clone()) });
                }
                return drawn.into_iter().map(|d| d.unwrap_or(Drawn::Error("no answer".into()))).collect();
            }
            let Some(slot) = drawn.get_mut(from + answered) else { break };
            *slot = Some(match kind {
                "ok" => Drawn::Ok(rest_text.trim().parse().unwrap_or(0)),
                "abort" => Drawn::Aborted(rest_text),
                _ => Drawn::Error(rest_text),
            });
            answered += 1;
            aborted |= kind == "abort";
        }
        if !aborted && answered < rest.len() {
            // node died without saying why (out of memory, a crash outside the engine).
            let why = output.as_ref().map(|o| String::from_utf8_lossy(&o.stderr).lines().last().unwrap_or("").to_string()).unwrap_or_default();
            drawn[from + answered] = Some(Drawn::Aborted(format!("node exited: {why}")));
            answered += 1;
        }
        from += answered.max(1);
    }
    drawn.into_iter().map(|d| d.unwrap_or(Drawn::Error("no answer".into()))).collect()
}

/// One case: like the native run's `run_case`, with the slides drawn by the wasm engine.
fn run_case(case: &Case, cfg: &Config, node: &str, script: &Path) -> Value {
    let started = Instant::now();
    let mut result = json!({
        "id": case.id, "path": case.rel, "category": case.category, "set": if case.derived { "derived" } else { "original" },
        "references": case.references.len(),
    });
    let pres = match import(&case.slide_file) {
        Ok(p) => p,
        Err(e) => {
            result["status"] = json!("open-failed");
            result["error"] = json!(e);
            result["slides"] = json!(case.references.iter().map(|(n, _)| json!({ "n": n, "status": "unmapped", "pass": false })).collect::<Vec<_>>());
            return result;
        }
    };
    let visible: Vec<usize> = (0..pres.slides.len()).filter(|&i| !pres.slides[i].hidden).collect();
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
    let dir = cfg.out.join("cases").join(&case.id);
    let _ = std::fs::create_dir_all(&dir);

    // References first (their size is the render size), then one node run for the whole slide.
    let mut refs = Vec::new();
    let mut jobs = Vec::new();
    for (n, ref_path) in &case.references {
        let index = (*n as usize).checked_sub(1).and_then(|i| mapping.get(i).copied());
        let reference = index.map(|_| Rgb::load(ref_path));
        if let (Some(index), Some(Ok(r))) = (index, &reference) {
            jobs.push(Job { index, width: r.width, height: r.height, out: dir.join(format!("slide-{n:04}.rgba")) });
        }
        refs.push((n, index, reference));
    }
    let mut drawn = draw(&jobs, &case.slide_file, cfg, node, script, &dir).into_iter();

    let mut slides = Vec::new();
    let mut render_ms = 0u64;
    for (n, index, reference) in refs {
        let mut s = json!({ "n": n });
        let Some(index) = index else {
            s["status"] = json!("unmapped");
            s["pass"] = json!(false);
            slides.push(s);
            continue;
        };
        s["slide"] = json!(index + 1);
        let reference = match reference {
            Some(Ok(r)) => r,
            Some(Err(e)) => {
                s["status"] = json!("reference-unreadable");
                s["error"] = json!(e);
                s["pass"] = json!(false);
                slides.push(s);
                continue;
            }
            None => unreachable!("mapped slides load their reference"),
        };
        let (w, h) = (reference.width, reference.height);
        let scale = w as f64 / pres.slide_size.width.max(1.0);
        s["size"] = json!([w, h]);
        s["aspect_px"] = json!(round(pres.slide_size.height * scale - h as f64, 2));
        let file = dir.join(format!("slide-{n:04}.rgba"));
        let rgba = match drawn.next() {
            Some(Drawn::Ok(ms)) => {
                render_ms += ms;
                s["render_ms"] = json!(ms);
                std::fs::read(&file).ok()
            }
            Some(Drawn::Aborted(e)) => {
                s["status"] = json!("wasm-aborted");
                s["error"] = json!(e);
                s["render_panicked"] = json!(true);
                s["pass"] = json!(false);
                slides.push(s);
                continue;
            }
            Some(Drawn::Error(e)) => {
                s["status"] = json!("render-failed");
                s["error"] = json!(e);
                s["pass"] = json!(false);
                slides.push(s);
                continue;
            }
            None => None,
        };
        let _ = std::fs::remove_file(&file);
        let Some(rgba) = rgba.filter(|b| b.len() == (w * h * 4) as usize) else {
            s["status"] = json!("render-failed");
            s["error"] = json!("the engine returned no image of the reference's size");
            s["pass"] = json!(false);
            slides.push(s);
            continue;
        };
        let actual = Rgb::from_straight_rgba(w, h, &rgba);
        match compare::compare(&reference, &actual) {
            Ok(c) => {
                s["status"] = json!("compared");
                if let Value::Object(m) = c.metrics.to_json() {
                    for (k, v) in m {
                        s[k] = v;
                    }
                }
                match compare::compare_layout(&reference, &actual) {
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
            }
            Err(e) => {
                s["status"] = json!("compare-failed");
                s["error"] = json!(e);
                s["pass"] = json!(false);
            }
        }
        slides.push(s);
    }
    let _ = std::fs::remove_file(dir.join("job.json"));
    if cfg.diff == DiffImage::None {
        let _ = std::fs::remove_dir(&dir);
    }
    result["status"] = json!(if mode == "physical-mismatch" { "count-mismatch" } else { "ok" });
    result["render_ms"] = json!(render_ms);
    result["elapsed_ms"] = json!(started.elapsed().as_millis() as u64);
    result["slides"] = json!(slides);
    result
}
