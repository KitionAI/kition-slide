//! `kition-slide`: the Slide kit for Kition's agent. The same document the in-page
//! editor runs (`kition-slide-kit`), served headless over the Kit Protocol by
//! `kition-headless`, as the kit's `kit.json` declares it, with the object model
//! (`model.describe`, `model.batch`) over the document's change exchange.

/// The commands with file parameters the agent is offered: they read the picture or media they
/// name through the host, in the workspace (`kition_slide_kit::SANDBOXED`).
const SANDBOXED: &[&str] = kition_slide_kit::SANDBOXED;
/// `inspect()` keys about the editing session rather than the file.
const VOLATILE: &[&str] = kition_slide_kit::EDITOR_STATE;

fn main() -> std::process::ExitCode {
    kition_headless::run_manifest_model::<kition_slide_kit::SlideDocument>(include_str!("../../../kit.json"), SANDBOXED, VOLATILE)
}
