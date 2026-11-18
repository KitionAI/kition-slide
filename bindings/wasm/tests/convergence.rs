//! The Kit Standard 2.0 convergence suite (L6) against Kition Slide, driven through its commands.

mod common;

use std::path::Path;

use common::SlideKit;
use kition_convergence::wasm::{self, WasmEngine};
use kition_convergence::{Options, Outcome, run};

/// Known failures of this kit, each a real limitation (not a suite issue), excluded from the
/// assertion until fixed (none at present).
const KNOWN: &[&str] = &[];

#[test]
fn converges() {
    let report = run(&SlideKit, &Options::from_env());
    println!("{}", report.render());
    report.save_if_requested().unwrap();
    for check in &report.checks {
        if KNOWN.contains(&check.id) {
            continue;
        }
        let ok = match &check.outcome {
            Outcome::Passed => true,
            Outcome::Skipped(_) => !check.required,
            Outcome::Failed(_) => false,
        };
        assert!(ok, "{} is not as L6 needs it:\n{}", check.id, report.render());
    }
}

/// Native and WebAssembly engines reach one state from one change log; runs when
/// `KITION_WASM_ENGINE` names a built engine folder (`kition_slide_kit.js` and its wasm).
#[test]
fn wasm_parity() {
    let Some(engine) = WasmEngine::from_env("kition_slide_kit", "SlideDocument", common::NAME) else {
        println!("skipped: set KITION_WASM_ENGINE to a built engine folder");
        return;
    };
    wasm::check_parity(&SlideKit, &engine, Path::new(env!("CARGO_TARGET_TMPDIR")), 1).unwrap();
}
