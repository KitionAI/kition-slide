//! `kition-slide`: the Slide kit for Kition's agent. The same document the in-page
//! editor runs (`kition-slide-kit`), served headless over the Kit Protocol by
//! `kition-headless`, as the kit's `kit.json` declares it, with the object model
//! (`model.describe`, `model.batch`) over the document's change exchange, and the identity a file
//! carries (`file.identity`, `file.fork`): a `.kslide` (its `slide.json` and media) or a
//! `.pptx` embeds no replica, which the kit makes anew each time it opens a file, so a file carries
//! no identity and its fork is the same bytes.

use kition_headless::{FileIdentity, Headless, KitInfo};

/// The commands with file parameters the agent is offered: they read the picture or media they
/// name through the host, in the workspace (`kition_slide_kit::SANDBOXED`).
const SANDBOXED: &[&str] = kition_slide_kit::SANDBOXED;
/// `inspect()` keys about the editing session rather than the file.
const VOLATILE: &[&str] = kition_slide_kit::EDITOR_STATE;

fn main() -> std::process::ExitCode {
    let kit = match KitInfo::from_manifest(include_str!("../../../kit.json"), SANDBOXED, VOLATILE) {
        Ok(kit) => kit,
        Err(error) => {
            eprintln!("kit: {error}");
            return std::process::ExitCode::from(2);
        }
    };
    let backend = Headless::<kition_slide_kit::SlideDocument>::with_replica_port(kit.clone()).with_file_identity(FileIdentity::NotEmbedded);
    kition_headless::serve_backend(&kit, backend)
}
