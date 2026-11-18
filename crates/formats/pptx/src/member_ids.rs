//! Kition-owned table member metadata. Other readers may strip it; absent ids are repaired
//! deterministically when the engine opens the file, without claiming continuity through stripping.
use crate::xml::{A, El, W};
use kition_slide_model::Table;

const URI: &str = "https://kition.ai/ns/slide/table-members/1";

pub(crate) fn write(w: &mut W, table: &Table) {
    if table.col_ids.is_empty() && table.rows.iter().all(|row| row.identity.is_empty()) {
        return;
    }
    w.open0("a:extLst");
    w.open("a:ext", A::new().a("uri", URI));
    w.open("k:members", A::new().a("xmlns:k", URI));
    for id in table.col_ids.iter().take(1000) {
        w.empty("k:col", A::new().a("id", id));
    }
    for row in table.rows.iter().take(1000) {
        w.empty("k:row", A::new().a("id", &row.identity));
    }
    w.close("k:members");
    w.close("a:ext");
    w.close("a:extLst");
}

pub(crate) fn read(root: &El, table: &mut Table) {
    let Some(members) = root
        .child("tblPr")
        .and_then(|props| props.child("extLst"))
        .and_then(|list| list.children_named("ext").find(|ext| ext.attr("uri") == Some(URI)))
        .and_then(|ext| ext.child("members"))
    else {
        return;
    };
    let cols: Vec<_> = members.children_named("col").take(1001).map(|col| col.attr("id").unwrap_or("").to_owned()).collect();
    if cols.len() == table.cols.len() {
        table.col_ids = cols;
    }
    let rows: Vec<_> = members.children_named("row").take(1001).collect();
    if rows.len() == table.rows.len() {
        for (row, el) in table.rows.iter_mut().zip(rows) {
            row.identity = el.attr("id").unwrap_or("").to_owned();
        }
    }
}
