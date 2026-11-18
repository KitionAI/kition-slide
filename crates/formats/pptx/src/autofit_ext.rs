//! Whether a text body's autofit result is a saved one (`BodyProps::autofit_saved`) when that is
//! not what a reader infers (task card C8-slide-5): a shrink result read from a file is saved, and
//! any other body says nothing, but Kition marks a body whose shrink result it cleared as not
//! saved (`false`, which also masks a result its placeholder inherits). The file has no place for
//! that, so it goes into the text body's extension list under Kition's own URI, which other
//! programs skip; a file without it reads as before. The deck a kit holds in memory is then the deck
//! its saved file opens as. The receipt of a result's inputs (`autofit_input`) is not written: a
//! deck read from a file has it recomputed (kition-slide-engine `stored`).

use kition_slide_model::text::{AutoFit, BodyProps};

use crate::xml::El;

/// The extension's URI (`a:ext/@uri`): Kition's own.
pub(crate) const URI: &str = "{5D0E7A4C-0B8E-4E4A-9C3B-6B1F2D8A7C51}";
/// The namespace of its element (`k:autofit`).
pub(crate) const NS: &str = "https://kition.ai/ns/slide/autofit";

/// The flag a file must hold for a reader to read `b`'s back, when the reader would infer another:
/// `1` or `0`. A shrink result without a flag is written as a saved one (a file cannot say
/// "unset"; Kition settles such a body to saved before it is written).
pub(crate) fn written(b: &BodyProps) -> Option<&'static str> {
    let inferred = matches!(b.autofit, Some(AutoFit::Shrink { .. })).then_some(true);
    match b.autofit_saved {
        saved if saved == inferred => None,
        Some(true) => Some("1"),
        Some(false) => Some("0"),
        None => None,
    }
}

fn element(ext_list: &El) -> Option<&El> {
    ext_list.elements().filter(|ext| ext.attr("uri") == Some(URI)).find_map(|ext| ext.elements().find(|e| e.is("autofit")))
}

/// Whether a `bodyPr` says itself whether its shrink result is saved.
pub(crate) fn flags_saved(body_pr: &El) -> bool {
    body_pr.child("extLst").and_then(element).is_some_and(|fit| matches!(fit.attr("saved"), Some("1" | "0")))
}

/// Reads the extension from a `bodyPr`'s `extLst` into `b`.
pub(crate) fn read(ext_list: &El, b: &mut BodyProps) {
    let Some(fit) = element(ext_list) else { return };
    match fit.attr("saved") {
        Some("1") => b.autofit_saved = Some(true),
        Some("0") => b.autofit_saved = Some(false),
        _ => {}
    }
}
