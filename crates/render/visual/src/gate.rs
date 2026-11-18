//! Non-regression gate: this run's per-case results against an accepted baseline scorecard.
//!
//! Only cases finished in this run's selection (shard and filter) are compared, so a sharded or
//! filtered run gates its own part. A slide that passes the strict gate in the baseline and does
//! not pass now is a **lost pass** and fails the gate (a slide that is missing, aborted or failed
//! to render counts as not passing). A slide whose SSIM dropped by more than the drop threshold
//! is reported as a **drop**; drops fail the gate only when asked to. Findings name the case id
//! and the slide number only, never the slide file's path or text, so the output can be pasted into
//! commits and reports.

use std::collections::BTreeMap;
use std::path::PathBuf;

use serde_json::Value;

/// Gate settings (`KITION_VISUAL_GATE=1` and friends; see `runner::Config::from_env`).
#[derive(Clone, Debug)]
pub struct GateConfig {
    /// The accepted scorecard (`KITION_VISUAL_BASELINE`).
    pub baseline: PathBuf,
    /// SSIM drop reported as a drop (`KITION_VISUAL_GATE_DROP`, default [`DEFAULT_DROP`]).
    pub drop: f64,
    /// Drops fail the gate too (`KITION_VISUAL_GATE_DROP_FAILS=1`); by default they only warn.
    pub drop_fails: bool,
}

/// Default SSIM drop that is reported.
pub const DEFAULT_DROP: f64 = 0.05;

/// One slide that regressed.
#[derive(Clone, Debug, PartialEq)]
pub struct Finding {
    pub id: String,
    pub slide: u64,
    pub ssim_before: Option<f64>,
    pub ssim_after: Option<f64>,
}

/// What the gate found.
#[derive(Clone, Debug, Default)]
pub struct Report {
    pub cases: usize,
    pub slides: usize,
    /// Cases of this run the baseline does not have (not gated).
    pub new_cases: usize,
    pub gained: usize,
    pub lost: Vec<Finding>,
    pub drops: Vec<Finding>,
    pub drop_fails: bool,
    /// The baseline's metric version when it differs from this run's.
    pub metric_mismatch: Option<(String, String)>,
}

impl Report {
    /// The gate fails on any lost pass, and on drops when `drop_fails`.
    pub fn failed(&self) -> bool {
        !self.lost.is_empty() || (self.drop_fails && !self.drops.is_empty())
    }

    /// Human-readable lines (ids and slide numbers only).
    pub fn lines(&self) -> Vec<String> {
        let mut out = vec![format!(
            "gate: {} cases, {} slides compared ({} new cases not gated); {} passes gained, {} lost, {} drops{}",
            self.cases,
            self.slides,
            self.new_cases,
            self.gained,
            self.lost.len(),
            self.drops.len(),
            if self.drop_fails { " (drops fail)" } else { " (drops warn)" }
        )];
        if let Some((before, now)) = &self.metric_mismatch {
            out.push(format!("gate: warning: baseline metric {before}, this run {now}"));
        }
        let fmt = |f: &Finding| {
            let s = |v: Option<f64>| v.map_or("-".to_string(), |v| format!("{v:.4}"));
            format!("{} slide {}: ssim {} -> {}", f.id, f.slide, s(f.ssim_before), s(f.ssim_after))
        };
        out.extend(self.lost.iter().map(|f| format!("gate: LOST {}", fmt(f))));
        let tag = if self.drop_fails { "DROP" } else { "warning: drop" };
        out.extend(self.drops.iter().map(|f| format!("gate: {tag} {}", fmt(f))));
        out
    }
}

/// Compare `current` case results (the per-case JSON this run's selection finished) with the
/// baseline scorecard's cases.
pub fn compare(baseline: &Value, current: &[Value], cfg: &GateConfig) -> Report {
    let mut report = Report { drop_fails: cfg.drop_fails, ..Default::default() };
    let before: BTreeMap<&str, &Value> = baseline["cases"].as_array().into_iter().flatten().filter_map(|c| Some((c["id"].as_str()?, c))).collect();
    for case in current {
        let Some(id) = case["id"].as_str() else { continue };
        let Some(old) = before.get(id) else {
            report.new_cases += 1;
            continue;
        };
        report.cases += 1;
        let now: BTreeMap<u64, &Value> = case["slides"].as_array().into_iter().flatten().filter_map(|s| Some((s["n"].as_u64()?, s))).collect();
        // A sampled case compared only some slides: the others are not missing, just not run.
        let sampled = case["sampled"] == Value::Bool(true);
        for s in old["slides"].as_array().into_iter().flatten() {
            let Some(n) = s["n"].as_u64() else { continue };
            if sampled && !now.contains_key(&n) {
                continue;
            }
            report.slides += 1;
            let was = s["pass"] == Value::Bool(true);
            let new = now.get(&n);
            let is = new.is_some_and(|v| v["pass"] == Value::Bool(true));
            let finding =
                || Finding { id: id.to_string(), slide: n, ssim_before: s["ssim"].as_f64(), ssim_after: new.and_then(|v| v["ssim"].as_f64()) };
            if was && !is {
                report.lost.push(finding());
            } else if is && !was {
                report.gained += 1;
            }
            if let (Some(a), Some(b)) = (s["ssim"].as_f64(), new.and_then(|v| v["ssim"].as_f64()))
                && a - b > cfg.drop
            {
                report.drops.push(finding());
            }
        }
    }
    report
}

/// Read the baseline and compare; `metric` is this run's metric version.
pub fn check(cfg: &GateConfig, current: &[Value], metric: &str) -> Result<Report, String> {
    let bytes = std::fs::read(&cfg.baseline).map_err(|e| format!("gate baseline {}: {e}", cfg.baseline.display()))?;
    let baseline: Value = serde_json::from_slice(&bytes).map_err(|e| format!("gate baseline {}: {e}", cfg.baseline.display()))?;
    Ok(check_value(&baseline, current, metric, cfg))
}

/// [`compare`], plus a warning when the baseline was measured with another metric version.
pub fn check_value(baseline: &Value, current: &[Value], metric: &str, cfg: &GateConfig) -> Report {
    let mut report = compare(baseline, current, cfg);
    let before = baseline["metric"]["version"].as_str().unwrap_or("unknown");
    if before != metric {
        report.metric_mismatch = Some((before.to_string(), metric.to_string()));
    }
    report
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn cfg(drop_fails: bool) -> GateConfig {
        GateConfig { baseline: PathBuf::new(), drop: DEFAULT_DROP, drop_fails }
    }

    fn slide(n: u64, ssim: f64, pass: bool) -> Value {
        json!({ "n": n, "ssim": ssim, "pass": pass })
    }

    fn baseline() -> Value {
        json!({ "cases": [
            { "id": "c1", "slides": [slide(1, 0.9995, true), slide(2, 0.95, false), slide(3, 0.9, false)] },
            { "id": "c2", "slides": [slide(1, 1.0, true)] },
        ]})
    }

    #[test]
    fn unchanged_run_passes() {
        let cur = baseline()["cases"].as_array().unwrap().clone();
        let r = compare(&baseline(), &cur, &cfg(true));
        assert_eq!((r.cases, r.slides, r.gained), (2, 4, 0));
        assert!(r.lost.is_empty() && r.drops.is_empty() && !r.failed());
    }

    #[test]
    fn lost_pass_fails_and_names_id_and_slide_only() {
        let cur = vec![
            json!({ "id": "c1", "path": "private/slide.pptx", "slides": [slide(1, 0.998, false), slide(2, 0.95, false), slide(3, 0.9, false)] }),
        ];
        let r = compare(&baseline(), &cur, &cfg(false));
        assert_eq!(r.lost, vec![Finding { id: "c1".into(), slide: 1, ssim_before: Some(0.9995), ssim_after: Some(0.998) }]);
        assert!(r.failed());
        let text = r.lines().join("\n");
        assert!(text.contains("LOST c1 slide 1") && !text.contains("slideFile"));
    }

    #[test]
    fn missing_or_aborted_slide_counts_as_lost() {
        let cur = vec![
            json!({ "id": "c2", "status": "aborted", "slides": [{ "n": 1, "status": "aborted", "pass": false }] }),
            json!({ "id": "c1", "slides": [] }),
        ];
        let r = compare(&baseline(), &cur, &cfg(false));
        assert_eq!(r.lost.len(), 2);
        assert_eq!(r.lost[0].ssim_after, None);
    }

    #[test]
    fn sampled_case_gates_only_its_slides() {
        let cur = vec![json!({ "id": "c1", "sampled": true, "slides": [slide(2, 0.95, false)] })];
        let r = compare(&baseline(), &cur, &cfg(true));
        assert_eq!((r.cases, r.slides), (1, 1));
        assert!(!r.failed());
        let lost = vec![json!({ "id": "c1", "sampled": true, "slides": [slide(1, 0.99, false)] })];
        assert_eq!(compare(&baseline(), &lost, &cfg(false)).lost.len(), 1);
    }

    #[test]
    fn drops_warn_unless_asked_to_fail() {
        let cur = vec![json!({ "id": "c1", "slides": [slide(1, 0.9995, true), slide(2, 0.89, false), slide(3, 0.88, false)] })];
        let warn = compare(&baseline(), &cur, &cfg(false));
        assert_eq!(warn.drops.len(), 1);
        assert_eq!(warn.drops[0].slide, 2);
        assert!(!warn.failed());
        assert!(compare(&baseline(), &cur, &cfg(true)).failed());
    }

    #[test]
    fn only_cases_of_this_run_are_gated() {
        let cur = vec![json!({ "id": "c2", "slides": [slide(1, 1.0, true)] }), json!({ "id": "c9", "slides": [slide(1, 0.5, false)] })];
        let r = compare(&baseline(), &cur, &cfg(true));
        assert_eq!((r.cases, r.slides, r.new_cases), (1, 1, 1));
        assert!(!r.failed());
    }

    #[test]
    fn gained_passes_are_counted() {
        let cur = vec![json!({ "id": "c1", "slides": [slide(1, 0.9995, true), slide(2, 0.9999, true), slide(3, 0.9, false)] })];
        assert_eq!(compare(&baseline(), &cur, &cfg(true)).gained, 1);
    }

    #[test]
    fn metric_mismatch_is_reported() {
        let mut b = baseline();
        b["metric"] = json!({ "version": "slide-visual-metric/1" });
        let r = check_value(&b, &[], "slide-visual-metric/2", &cfg(false));
        assert_eq!(r.metric_mismatch, Some(("slide-visual-metric/1".into(), "slide-visual-metric/2".into())));
        b["metric"] = json!({ "version": "slide-visual-metric/2" });
        assert_eq!(check_value(&b, &[], "slide-visual-metric/2", &cfg(false)).metric_mismatch, None);
    }
}
