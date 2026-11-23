//! What a saved PPTX keeps beyond the drawn model (phase 7 fidelity, second round) replicates:
//! theme system colours, the handout master, header and footer switches, an empty notes page,
//! the slide properties, a chart's carried parts, default paragraph styles and the line spacing
//! compatibility switch. All of it lives in registers, so a peer whose model lacks these keys
//! still reads every register it knows. The view properties (last view, zoom, guides) are the
//! opening person's own: they stay in that person's view (and the file it saves), never in the
//! replica.

mod common;

use std::sync::Arc;

use common::{SharedBlobs, canonical, edit, first, new_replica, peer, slide_file, sync};
use kdm_model::read_document;

use kition_slide_model::chart::{Chart, ChartType};
use kition_slide_model::text::{LevelStyle, RunProps};
use kition_slide_model::theme::SystemColor;
use kition_slide_model::{KeptRel, Master, Rgba, ShapeKind, SlideFile};
use serde::Deserialize;
use serde_json::Value;

const HF: &str = r#"<p:hf xmlns:p="urn:p" hdr="0"/>"#;

fn rich() -> SlideFile {
    let mut p = slide_file();
    let lang = LevelStyle { run: RunProps { lang: Some("zh-CN".into()), ..Default::default() }, ..Default::default() };
    p.default_text_style.default_level = Some(Box::new(lang));
    p.pres_props = Some(r#"<p:presentationPr xmlns:p="urn:p"><p:prnPr frameSlides="1"/></p:presentationPr>"#.into());
    p.view_props = Some(r#"<p:viewPr xmlns:p="urn:p" lastView="sldSorterView"/>"#.into());
    let mut handout = Master { name: "Handout Master".into(), hf: Some(HF.into()), ..Default::default() };
    handout.theme.system_colors = vec![SystemColor { slot: "dk1".into(), name: "windowText".into(), last: Rgba::BLACK }];
    p.handout_master = Some(Arc::new(handout));
    let style = p.add_media("style1.xml", "application/vnd.ms-office.chartstyle+xml", b"<cs:chartStyle/>".to_vec());
    let m = Arc::make_mut(&mut p.masters[0]);
    m.theme.system_colors = vec![SystemColor { slot: "lt1".into(), name: "window".into(), last: Rgba::WHITE }];
    m.hf = Some(HF.into());
    m.layouts[0].hf = Some(HF.into());
    let s = Arc::make_mut(&mut p.slides[0]);
    s.notes_page = Some(r#"<p:notes xmlns:p="urn:p"/>"#.into());
    let a = s.shapes.iter_mut().find(|x| x.name == "A").unwrap_or_else(|| panic!("shape A"));
    if let Some(t) = &mut a.text {
        t.body.compat_line_spacing = Some(true);
    }
    let b = s.shapes.iter_mut().find(|x| x.name == "B").unwrap_or_else(|| panic!("shape B"));
    b.text = None;
    let mut chart = Chart::sample(ChartType::Column);
    chart.rels = vec![KeptRel { id: "rId1".into(), rel_type: "urn:rel/chartStyle".into(), media: style, rels: vec![] }];
    b.kind = ShapeKind::Chart(Box::new(chart));
    p
}

fn chart_rels(p: &SlideFile) -> Vec<KeptRel> {
    let b = p.slides[0].shapes.iter().find(|x| x.name == "B").unwrap_or_else(|| panic!("shape B"));
    match &b.kind {
        ShapeKind::Chart(c) => c.rels.clone(),
        _ => panic!("not a chart"),
    }
}

#[test]
fn kept_properties_read_back_from_a_replica() {
    let p = rich();
    let blobs = SharedBlobs::default();
    let (replica, _) = new_replica(&p, &blobs);
    let (read, _) = read_document(&replica, &blobs).unwrap_or_else(|error| panic!("read: {error}"));
    assert_eq!(canonical(&read), canonical(&p));
    assert_eq!(read.view_props, None, "the view properties are not replicated");
    let rels = chart_rels(&read);
    assert_eq!(read.media(rels[0].media).map(|m| m.data.as_slice()), Some(&b"<cs:chartStyle/>"[..]), "the carried part names its media item");
}

#[test]
fn kept_properties_replicate_to_a_peer() {
    let blobs = SharedBlobs::default();
    let mut a = first(&slide_file(), &blobs);
    let mut b = peer(&a, 2, &blobs);
    let r = rich();
    edit(&mut a, &blobs, |p| *p = r.clone());
    sync(&mut a, &mut b);
    assert_eq!(canonical(&b.model().slide_file), canonical(&a.model().slide_file));
    let q = &b.model().slide_file;
    assert_eq!((&q.pres_props, &q.slides[0].notes_page), (&r.pres_props, &r.slides[0].notes_page));
    assert_eq!(q.view_props, None, "the peer does not get the view properties");
    assert_eq!(a.model().slide_file.view_props, r.view_props, "the view that set them keeps them");
    assert_eq!((&q.masters[0].hf, &q.masters[0].layouts[0].hf), (&r.masters[0].hf, &r.masters[0].layouts[0].hf));
    assert_eq!(q.masters[0].theme.system_colors, r.masters[0].theme.system_colors);
    assert_eq!(q.handout_master, r.handout_master);
    assert_eq!(chart_rels(q).len(), 1);
    // A change of the view properties writes nothing; a change of another kept field does.
    let version = a.version();
    edit(&mut a, &blobs, |p| p.view_props = None);
    assert_eq!(a.version(), version, "the view properties are not replicated");
    edit(&mut b, &blobs, |p| Arc::make_mut(&mut p.masters[0]).layouts[0].hf = None);
    sync(&mut a, &mut b);
    for q in [&a.model().slide_file, &b.model().slide_file] {
        assert_eq!((q.view_props.as_deref(), q.masters[0].layouts[0].hf.as_deref()), (None, None));
        assert_eq!(q.pres_props, r.pres_props);
    }
}

/// The registers as a 0.4.x reader declares them (none of this round's keys).
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct OldRootProps {
    first_slide_number: u32,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct OldNamed {
    name: String,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct OldScheme {
    colors: Vec<Value>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct OldTheme {
    name: String,
    colors: OldScheme,
}

#[test]
fn a_0_4_reader_reads_registers_with_the_new_keys() {
    let p = rich();
    let blobs = SharedBlobs::default();
    let (replica, ids) = new_replica(&p, &blobs);
    let root = kition_kit::kdm::ObjectId::from_string("slide-file");
    let props = replica.get(&root, "props").unwrap_or_else(|error| panic!("props: {error}"));
    assert!(props.get("presProps").is_some(), "{props}");
    assert!(props.get("viewProps").is_none(), "the view properties are not replicated: {props}");
    let old: OldRootProps = serde_json::from_value(props).unwrap_or_else(|error| panic!("old root props: {error}"));
    assert_eq!(old.first_slide_number, p.first_slide_number);

    let slide = ids.slides.object(p.slides[0].id).cloned().unwrap_or_else(|| panic!("slide"));
    let props = replica.get(&slide, "props").unwrap_or_else(|error| panic!("props: {error}"));
    assert!(props.get("notesPage").is_some(), "{props}");
    let old: OldNamed = serde_json::from_value(props).unwrap_or_else(|error| panic!("old slide props: {error}"));
    assert_eq!(old.name, p.slides[0].name);

    let handout = replica.get(&root, "handoutMaster").unwrap_or_else(|error| panic!("handout: {error}"));
    assert!(handout.get("hf").is_some(), "{handout}");
    let old: OldNamed = serde_json::from_value(handout.clone()).unwrap_or_else(|error| panic!("old handout master: {error}"));
    assert_eq!(old.name, "Handout Master");
    let theme = handout.get("theme").cloned().unwrap_or_else(|| panic!("theme"));
    assert!(theme.get("systemColors").is_some(), "{theme}");
    let old: OldTheme = serde_json::from_value(theme).unwrap_or_else(|error| panic!("old theme: {error}"));
    assert_eq!(old.colors.colors.len(), 12, "{}", old.name);
}
