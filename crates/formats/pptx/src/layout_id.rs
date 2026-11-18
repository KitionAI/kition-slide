//! Layout identity in the layout's own extension; it follows the part through reorder/rename.
use crate::xml::{A, El, W};
pub(crate) const URI: &str = "https://kition.ai/ns/slide/layout-identity/1";

pub(crate) fn write(w: &mut W, identity: &str) {
    let Some(identity) = kition_slide_model::identity::canonical(identity) else { return };
    w.open("p:ext", A::new().a("uri", URI));
    w.empty("k:layout", A::new().a("xmlns:k", URI).a("id", identity));
    w.close("p:ext");
}

pub(crate) fn read(root: &El) -> String {
    root.child("extLst")
        .and_then(|list| list.children_named("ext").find(|ext| ext.attr("uri") == Some(URI)))
        .and_then(|ext| ext.child("layout"))
        .and_then(|layout| layout.attr("id"))
        .and_then(kition_slide_model::identity::canonical)
        .unwrap_or_default()
}
