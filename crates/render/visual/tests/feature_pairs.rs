//! Fixed-suite integration using only temporary, synthetic decks and reference pixels.

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use kition_slide_model::style::ColorRef;
use kition_slide_model::{Background, Fill, Rgba, Slide, SlideFile, SlideId};
use kition_slide_visual::compare::DiffImage;
use kition_slide_visual::corpus::{self, Case};
use kition_slide_visual::gate::GateConfig;
use kition_slide_visual::runner::{Config, FROZEN_DATE, run};
use serde_json::{Value, json};

static NEXT: AtomicU64 = AtomicU64::new(0);

struct Fixture {
    directory: PathBuf,
    cfg: Config,
    cases: Vec<Case>,
}

impl Fixture {
    fn new(names: &[(&str, u32)]) -> Self {
        let nonce = SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_nanos();
        let directory =
            std::env::temp_dir().join(format!("kition-feature-pairs-{}-{nonce}-{}", std::process::id(), NEXT.fetch_add(1, Ordering::Relaxed)));
        let root = directory.join("corpus");
        std::fs::create_dir_all(&root).unwrap();
        for &(name, count) in names {
            let slide_file = SlideFile {
                slides: (1..=count)
                    .map(|n| {
                        Arc::new(Slide {
                            id: SlideId(100 + n),
                            show_master_shapes: false,
                            background: Some(Background::Fill { fill: Fill::Solid { color: ColorRef::rgb(Rgba::WHITE) } }),
                            ..Default::default()
                        })
                    })
                    .collect(),
                ..Default::default()
            };
            std::fs::write(root.join(format!("{name}.pptx")), kition_slide_pptx::export(&slide_file).unwrap()).unwrap();
            let references = root.join(format!("{name}_reference"));
            std::fs::create_dir_all(&references).unwrap();
            for n in 1..=count {
                image::RgbImage::from_pixel(96, 54, image::Rgb([255, 255, 255])).save(references.join(format!("slide{n}.png"))).unwrap();
            }
        }
        let cases = corpus::discover(&root, None);
        assert_eq!(cases.len(), names.len());
        let manifest = directory.join("manifest.json");
        write_json(
            &manifest,
            &json!({"version":1,"cases":cases.iter().map(|case| {
                let pages = if case.references.len() == 1 { vec![1] } else { vec![2, case.references.len() as u32] };
                json!({"id":case.id,"pages":pages})
            }).collect::<Vec<_>>()}),
        );
        let baseline = directory.join("baseline.json");
        write_json(
            &baseline,
            &json!({"cases":cases.iter().map(|case| json!({
                "id":case.id,"slides":case.references.iter().map(|(n,_)| json!({"n":n,"pass":true,"ssim":1.0})).collect::<Vec<_>>()
            })).collect::<Vec<_>>()}),
        );
        let out = directory.join("output");
        let cfg = Config {
            root,
            scorecard: out.join("scorecard.json"),
            out,
            suffix: None,
            threads: 1,
            filter: None,
            shard: (0, 1),
            budget: Duration::from_secs(60),
            diff: DiffImage::None,
            derived: false,
            rerun: false,
            system_fonts: true,
            date: FROZEN_DATE,
            gate: Some(GateConfig { baseline, drop: 0.05, drop_fails: true }),
            sample: None,
            manifest: Some(manifest),
            pairs: false,
            cache: None,
        };
        Self { directory, cfg, cases }
    }

    fn result(&self, index: usize) -> PathBuf {
        self.cfg.out.join("cases").join(format!("{}.json", self.cases[index].id))
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.directory);
    }
}

fn write_json(path: &Path, value: &Value) {
    std::fs::write(path, serde_json::to_vec_pretty(value).unwrap()).unwrap();
}

fn read_json(path: &Path) -> Value {
    serde_json::from_slice(&std::fs::read(path).unwrap()).unwrap()
}

fn numbers(case: &Value) -> Vec<u64> {
    case["slides"].as_array().unwrap().iter().map(|slide| slide["n"].as_u64().unwrap()).collect()
}

#[test]
fn fixed_pair_never_adds_full_baseline_passes_and_resumes_exactly() {
    let fixture = Fixture::new(&[("alpha", 4)]);
    let first = run(&fixture.cfg).unwrap();
    assert_eq!((first.ran, first.pending), (1, 0));
    assert_eq!(numbers(&first.scorecard["cases"][0]), vec![2, 4]);
    assert_eq!(first.scorecard["complete"], true);
    let gate = first.gate.unwrap();
    assert_eq!(gate.slides, 2);
    assert!(!gate.failed(), "{gate:?}");
    let resumed = run(&fixture.cfg).unwrap();
    assert_eq!((resumed.ran, resumed.pending), (0, 0));
    assert_eq!(numbers(&resumed.scorecard["cases"][0]), vec![2, 4]);
}

#[test]
fn filter_scores_only_selected_decks_even_with_other_cached_results() {
    let mut fixture = Fixture::new(&[("alpha", 4), ("beta", 3)]);
    assert_eq!(run(&fixture.cfg).unwrap().ran, 2);
    fixture.cfg.filter = Some("alpha".into());
    let filtered = run(&fixture.cfg).unwrap();
    assert_eq!((filtered.ran, filtered.pending), (0, 0));
    assert_eq!(filtered.scorecard["complete"], true);
    assert_eq!(filtered.scorecard["cases"].as_array().unwrap().len(), 1);
    assert_eq!(filtered.scorecard["selection"]["decks"], 1);
    assert_eq!(filtered.scorecard["selection"]["pages"], 2);
    assert_eq!(filtered.gate.unwrap().cases, 1);
    fixture.cfg.filter = Some("absent".into());
    assert!(run(&fixture.cfg).is_err());
}

#[test]
fn copied_receipt_from_another_deck_with_the_same_pages_is_rejected() {
    let fixture = Fixture::new(&[("alpha", 4), ("beta", 4)]);
    assert_eq!(run(&fixture.cfg).unwrap().ran, 2);
    std::fs::copy(fixture.result(1), fixture.result(0)).unwrap();
    assert!(run(&fixture.cfg).is_err());
}

#[test]
fn missing_selected_result_cannot_be_hidden_by_sampled_gate() {
    let fixture = Fixture::new(&[("alpha", 4)]);
    run(&fixture.cfg).unwrap();
    let path = fixture.result(0);
    let mut result = read_json(&path);
    result["slides"].as_array_mut().unwrap().pop();
    write_json(&path, &result);
    assert!(run(&fixture.cfg).is_err());
}

#[test]
fn existing_full_run_is_not_reused_as_a_pair_receipt() {
    let fixture = Fixture::new(&[("alpha", 4)]);
    let mut full = fixture.cfg.clone();
    full.manifest = None;
    assert_eq!(numbers(&run(&full).unwrap().scorecard["cases"][0]), vec![1, 2, 3, 4]);
    assert!(run(&fixture.cfg).is_err());
}

#[test]
fn crashed_case_records_only_selected_pages_and_fails_the_gate() {
    let fixture = Fixture::new(&[("alpha", 4)]);
    let cases = fixture.cfg.out.join("cases");
    std::fs::create_dir_all(&cases).unwrap();
    std::fs::write(cases.join(format!("{}.running", fixture.cases[0].id)), b"").unwrap();
    let outcome = run(&fixture.cfg).unwrap();
    assert_eq!((outcome.ran, outcome.pending), (0, 0));
    assert_eq!(outcome.scorecard["cases"][0]["status"], "aborted");
    assert_eq!(numbers(&outcome.scorecard["cases"][0]), vec![2, 4]);
    let gate = outcome.gate.unwrap();
    assert_eq!(gate.lost.iter().map(|finding| finding.slide).collect::<Vec<_>>(), vec![2, 4]);
    assert!(gate.failed());
}

#[test]
fn budget_exhaustion_stays_incomplete_and_single_page_exception_is_retained() {
    let mut fixture = Fixture::new(&[("alpha", 4), ("beta", 1)]);
    fixture.cfg.budget = Duration::ZERO;
    let empty = run(&fixture.cfg).unwrap();
    assert_eq!((empty.ran, empty.pending), (0, 2));
    assert_eq!(empty.scorecard["complete"], false);
    assert_eq!(empty.scorecard["pending_cases"], 2);
    fixture.cfg.budget = Duration::from_secs(60);
    let finished = run(&fixture.cfg).unwrap();
    assert_eq!((finished.ran, finished.pending), (2, 0));
    assert_eq!(finished.scorecard["selection"]["pages"], 3);
    assert_eq!(numbers(&finished.scorecard["cases"][1]), vec![1]);
}

#[test]
fn manifest_and_legacy_sampling_are_mutually_exclusive() {
    let mut fixture = Fixture::new(&[("alpha", 4)]);
    fixture.cfg.sample = Some(2);
    assert!(run(&fixture.cfg).is_err());
    assert!(!fixture.cfg.out.exists());
}
