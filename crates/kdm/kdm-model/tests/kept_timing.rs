//! A slide's kept timing tree (phase 7 fidelity: keyframes and the rest of `p:timing` the
//! animation list does not model) replicates in the slide's `props` register, with the shapes it
//! names as object ids, so a replica whose reader minted other shape ids still writes the tree back
//! naming the right shapes. Additive: a 0.4.x reader ignores the key and builds the timing from the
//! animations.

mod common;

use std::sync::Arc;

use common::{SharedBlobs, canonical, edit, first, new_replica, peer, slide_file, sync};
use kdm_model::read_document;
use kition_slide_model::SlideFile;
use kition_slide_model::anim::{KeptTiming, TimingTarget, timing_digest};
use serde::Deserialize;

const FORMULA: &str = "1+#ppt_h/2+0.0123";

fn timing_xml(spid: u32) -> String {
    format!(
        r##"<p:timing xmlns:p="http://schemas.openxmlformats.org/presentationml/2006/main"><p:tnLst><p:par><p:cTn id="1" dur="indefinite" restart="never" nodeType="tmRoot"><p:childTnLst><p:seq concurrent="1" nextAc="seek"><p:cTn id="2" dur="indefinite" nodeType="mainSeq"><p:childTnLst><p:par><p:cTn id="3" fill="hold"><p:stCondLst><p:cond delay="indefinite"/></p:stCondLst><p:childTnLst><p:par><p:cTn id="4" fill="hold"><p:stCondLst><p:cond delay="0"/></p:stCondLst><p:childTnLst><p:par><p:cTn id="5" presetID="2" presetClass="entr" presetSubtype="4" fill="hold" nodeType="clickEffect"><p:stCondLst><p:cond delay="0"/></p:stCondLst><p:childTnLst><p:anim calcmode="lin" valueType="num"><p:cBhvr additive="base"><p:cTn id="6" dur="500" fill="hold"/><p:tgtEl><p:spTgt spid="{spid}"/></p:tgtEl><p:attrNameLst><p:attrName>ppt_y</p:attrName></p:attrNameLst></p:cBhvr><p:tavLst><p:tav tm="0"><p:val><p:strVal val="{FORMULA}"/></p:val></p:tav><p:tav tm="100000"><p:val><p:strVal val="#ppt_y"/></p:val></p:tav></p:tavLst></p:anim></p:childTnLst></p:cTn></p:par></p:childTnLst></p:cTn></p:par></p:childTnLst></p:cTn></p:par></p:childTnLst></p:cTn></p:seq></p:childTnLst></p:cTn></p:par></p:tnLst></p:timing>"##
    )
}

/// The common slide file, its slide-0 animation (shape A, triggered by B) kept with a timing tree.
fn timed() -> SlideFile {
    let mut p = slide_file();
    let s = Arc::make_mut(&mut p.slides[0]);
    let id = |name: &str| s.shapes.iter().find(|x| x.name == name).map(|x| x.id).unwrap_or_else(|| panic!("shape {name}"));
    let targets = vec![TimingTarget { spid: 9, shape: id("A") }, TimingTarget { spid: 10, shape: id("B") }];
    let digest = timing_digest(&s.animations, &targets).unwrap_or_else(|| panic!("digest"));
    s.timing = Some(KeptTiming { xml: timing_xml(9), targets, digest });
    p
}

/// The written slide's kept formula and the id its tree names, if the tree was written back.
fn written(p: &SlideFile) -> Option<(u32, u32)> {
    let bytes = kition_slide_pptx::export(p).unwrap_or_else(|error| panic!("export: {error}"));
    let q = kition_slide_pptx::import(&bytes).unwrap_or_else(|error| panic!("import: {error}"));
    let kept = q.slides[0].timing.as_ref()?;
    if !kept.xml.contains(FORMULA) {
        return None;
    }
    let a = q.slides[0].shapes.iter().find(|x| x.name == "A").map(|x| x.id)?;
    let spid = kept.targets.iter().find(|t| t.shape == a).map(|t| t.spid)?;
    Some((spid, a.0))
}

#[test]
fn a_kept_timing_tree_reads_back_with_the_readers_shape_ids() {
    let p = timed();
    let blobs = SharedBlobs::default();
    let (replica, _) = new_replica(&p, &blobs);
    let (read, _) = read_document(&replica, &blobs).unwrap_or_else(|error| panic!("read: {error}"));
    assert_eq!(canonical(&read), canonical(&p), "the same slide, ids aside (the tree's shapes renumbered with the rest)");
    let kept = read.slides[0].timing.as_ref().unwrap_or_else(|| panic!("kept"));
    let a = read.slides[0].shapes.iter().find(|x| x.name == "A").map(|x| x.id).unwrap_or_else(|| panic!("A"));
    assert!(kept.targets.iter().any(|t| t.spid == 9 && t.shape == a), "{:?}", kept.targets);
    assert_eq!(kept.digest, p.slides[0].timing.as_ref().map(|t| t.digest.clone()).unwrap_or_default());
    // Saved from the replica's view: the tree, naming shape A by the id it is written with.
    let (spid, a_written) = written(&read).unwrap_or_else(|| panic!("the kept tree is written"));
    assert_eq!(spid, a_written);
}

#[test]
fn a_kept_timing_tree_replicates_and_an_edit_rebuilds_it() {
    let blobs = SharedBlobs::default();
    let mut a = first(&timed(), &blobs);
    let mut b = peer(&a, 2, &blobs);
    assert!(written(&b.model().slide_file).is_some(), "a peer writes the kept tree");
    edit(&mut a, &blobs, |p| Arc::make_mut(&mut p.slides[0]).animations[0].duration_ms = 1500);
    sync(&mut a, &mut b);
    assert_eq!(b.model().slide_file.slides[0].animations[0].duration_ms, 1500);
    assert!(written(&b.model().slide_file).is_none(), "an edited animation is written from the list");
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct OldSlideProps {
    name: String,
    hidden: bool,
}

#[test]
fn a_0_4_reader_reads_slide_props_with_a_kept_timing_tree() {
    let p = timed();
    let blobs = SharedBlobs::default();
    let (replica, ids) = new_replica(&p, &blobs);
    let slide = ids.slides.object(p.slides[0].id).cloned().unwrap_or_else(|| panic!("slide"));
    let props = replica.get(&slide, "props").unwrap_or_else(|error| panic!("props: {error}"));
    let shape = props.pointer("/timing/targets/0/shape").cloned().unwrap_or_default();
    assert!(shape.is_string(), "the shape is named by its object: {props}");
    let old: OldSlideProps = serde_json::from_value(props).unwrap_or_else(|error| panic!("old slide props: {error}"));
    assert_eq!((old.name.as_str(), old.hidden), (p.slides[0].name.as_str(), p.slides[0].hidden));
}
