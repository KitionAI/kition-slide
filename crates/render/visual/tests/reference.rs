//! Visual fidelity against reference renders (env-gated).
//!
//! `KITION_SLIDE_REFERENCE_ROOT=<dir>`: every slide under that directory with a folder of
//! reference renders next to it (see `corpus`) is imported, each slide rendered clean at its
//! reference's pixel size and compared (see `compare::manifest`). Per-case results and diff
//! images go to `KITION_VISUAL_OUT` (default `$HOME/.cache/vis/slide`), the scorecard to
//! `KITION_VISUAL_SCORECARD` (default `$HOME/.cache/kition-fidelity/slide-visual.json`). Runs are
//! resumable and time-boxed (`KITION_VISUAL_BUDGET` seconds); see `runner::Config::from_env` for
//! the other variables. `KITION_VISUAL_GATE=1` fails the test when a slide that passes in the
//! accepted baseline (`KITION_VISUAL_BASELINE`) no longer passes. Skipped when the root is unset.
//! Run it optimized:
//!
//! ```text
//! KITION_SLIDE_REFERENCE_ROOT=<reference root> \
//!   cargo test --release -p kition-slide-visual --test reference -- --nocapture
//! ```

use kition_slide_visual::runner::{Config, run};

#[test]
fn reference_corpus() {
    let Some(cfg) = Config::from_env() else {
        eprintln!("KITION_SLIDE_REFERENCE_ROOT unset: skipped");
        return;
    };
    let outcome = run(&cfg).expect("corpus run");
    let s = &outcome.scorecard;
    if cfg.manifest.is_some() {
        assert_eq!(outcome.pending, 0, "the fixed feature suite must finish every selected slide file");
        assert_eq!(s["complete"], true, "incomplete feature suite");
        for case in s["cases"].as_array().into_iter().flatten() {
            assert_eq!(case["status"], "ok", "feature case failed: {}", case["id"]);
            for slide in case["slides"].as_array().into_iter().flatten() {
                assert_eq!(slide["status"], "compared", "missing feature page: {} / {}", case["id"], slide["n"]);
                assert_ne!(slide["render_panicked"], true, "feature render panicked");
            }
        }
    }
    eprintln!("ran {} cases this time, {} left in this selection; scorecard {}", outcome.ran, outcome.pending, cfg.scorecard.display());
    for (set, t) in s["sets"].as_object().into_iter().flatten() {
        eprintln!(
            "{set}: {} cases, {} slides, {} pass ({}), ssim median {}, min {}; layout gate {} pass, ssim median {}",
            t["cases"], t["slides"], t["pass"], t["pass_rate"], t["ssim_median"], t["ssim_min"], t["pass_layout"], t["layout_ssim_median"]
        );
    }
    let mut cats: Vec<_> = s["categories"].as_object().into_iter().flatten().collect();
    cats.sort_by(|a, b| a.1["pass_rate"].as_f64().unwrap_or(0.0).total_cmp(&b.1["pass_rate"].as_f64().unwrap_or(0.0)));
    for (cat, t) in cats {
        eprintln!(
            "  {cat}: {}/{} pass, ssim median {}; layout {} pass, ssim median {}",
            t["pass"], t["slides"], t["ssim_median"], t["pass_layout"], t["layout_ssim_median"]
        );
    }
    if let Some(report) = &outcome.gate {
        for line in report.lines() {
            eprintln!("{line}");
        }
        assert!(!report.failed(), "the non-regression gate failed: {} lost passes, {} drops", report.lost.len(), report.drops.len());
    }
}
