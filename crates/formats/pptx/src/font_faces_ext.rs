//! Preserve face metadata even when its original run was empty or redundant.

use kition_slide_model::FontFace;

use crate::xml::{A, El, W};

pub(crate) const URI: &str = "https://kition.ai/ns/slide/font-faces/1";

pub(crate) fn write(w: &mut W, faces: &[FontFace]) {
    if faces.is_empty() {
        return;
    }
    w.open("p:ext", A::new().a("uri", URI));
    w.open("k:fontFaces", A::new().a("xmlns:k", URI));
    for face in faces.iter().take(4096) {
        w.empty(
            "k:face",
            A::new()
                .a("slot", &face.slot)
                .a("typeface", &face.typeface)
                .o("panose", face.panose.as_deref())
                .o("charset", face.charset.as_deref())
                .o("pitchFamily", face.pitch_family.as_deref()),
        );
    }
    w.close("k:fontFaces");
    w.close("p:ext");
}

pub(crate) fn read(root: &El, faces: &mut Vec<FontFace>) {
    let Some(kept) = root
        .child("extLst")
        .and_then(|ext| ext.children_named("ext").find(|ext| ext.attr("uri") == Some(URI)))
        .and_then(|ext| ext.child("fontFaces"))
    else {
        return;
    };
    for face in kept.children_named("face").take(4096) {
        let (Some(slot), Some(typeface)) = (face.attr("slot"), face.attr("typeface")) else { continue };
        let value = FontFace {
            slot: slot.to_owned(),
            typeface: typeface.to_owned(),
            panose: face.attr("panose").map(str::to_owned),
            charset: face.attr("charset").map(str::to_owned),
            pitch_family: face.attr("pitchFamily").map(str::to_owned),
        };
        if let Some(existing) = faces.iter_mut().find(|f| f.slot == slot && f.typeface == typeface) {
            *existing = value;
        } else if faces.len() < 4096 {
            faces.push(value);
        }
    }
    faces.sort_by(|a, b| (&a.slot, &a.typeface).cmp(&(&b.slot, &b.typeface)));
}
