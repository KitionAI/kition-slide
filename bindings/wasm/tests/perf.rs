//! The convergence suite's performance mode for Kition Slide (Kition `docs/kits.md` section 7):
//! `cargo test --release -p kition-slide-kit --test perf -- --ignored --nocapture`. Each test
//! fails when the kit is over its budget.

mod common;

use std::time::Instant;

use common::SlideKit;
use kition_convergence::perf::{self, PerfOptions};
use kition_convergence::{CountingAlloc, KitAdapter, Rng};

#[global_allocator]
static ALLOC: CountingAlloc = CountingAlloc::new();

#[test]
#[ignore = "a measurement; run in release builds"]
fn budgets() {
    let report = perf::measure(&SlideKit, &PerfOptions { allocator: Some(&ALLOC), ..PerfOptions::default() }).unwrap();
    println!("{}", report.render());
    // Kit Standard 2.0 collaboration budgets (Kition `docs/kits.md` section 7).
    assert!(report.open_ms < 50.0, "open from a snapshot: {:.1} ms, budget 50 ms", report.open_ms);
    let per_change = report.remote_ms / report.remote_changes.max(1) as f64;
    assert!(per_change < 10.0, "a remote change: {per_change:.2} ms, budget 10 ms");
    assert!(report.keystroke_p95_ms < 50.0, "keystroke to paint p95: {:.1} ms, budget 50 ms", report.keystroke_p95_ms);
    assert!(report.memory_bytes.is_some_and(|bytes| bytes < 16 << 20), "memory per replica over 16 MB");
}

/// A remote edit of one object (a shape moved, resized or filled, text typed or set) patches only
/// its slide of the view: on the suite's working document it applies in under 4 ms (median).
#[test]
#[ignore = "a measurement; run in release builds"]
fn single_object_remote_edit() {
    const SINGLE: &[&str] = &["shape.move", "shape.resize", "shape.fill", "text.insert", "text.set"];
    let kit = SlideKit;
    let mut rng = Rng::new(7);
    let mut doc = kit.create().unwrap();
    for _ in 0..200 {
        kit.random_edit(&mut doc, &mut rng).unwrap();
    }
    let snapshot = kit.snapshot(&mut doc).unwrap();
    let blobs = kit.blobs(&mut doc).unwrap();
    let mut author = kit.join(&snapshot, &blobs).unwrap();
    let mut reader = kit.join(&snapshot, &blobs).unwrap();
    let mut times = Vec::new();
    let mut tries = 0;
    while times.len() < 200 && tries < 5000 {
        tries += 1;
        let since = kit.version(&mut author).unwrap();
        let Some(label) = kit.random_edit(&mut author, &mut rng).unwrap() else { continue };
        let change = kit.export_changes(&mut author, &since).unwrap();
        let start = Instant::now();
        kit.import_changes(&mut reader, &change).unwrap();
        let ms = start.elapsed().as_secs_f64() * 1000.0;
        if SINGLE.contains(&label.as_str()) && kit.version(&mut author).unwrap() != since {
            times.push(ms);
        }
    }
    times.sort_by(f64::total_cmp);
    let at = |q: f64| times[((times.len() as f64 - 1.0) * q).round() as usize];
    println!("kit.slide: {} single-object remote edits, p50 {:.2} ms, p95 {:.2} ms", times.len(), at(0.5), at(0.95));
    assert!(at(0.5) < 4.0, "a single-object remote edit: p50 {:.2} ms, budget 4 ms", at(0.5));
}
