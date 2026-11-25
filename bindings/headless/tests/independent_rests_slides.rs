//! C8-slide-read-3: an independent writer changes what a deck-level write rests on (history, the
//! slide and its shapes, threads, the deck's slide list and properties, a slide's place) or
//! something it does not read; the write with `basis` refuses the first and applies over the
//! second.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use kition_kit::KitDocument;
use kition_kits::tools::{self, CallOptions};
use kition_kits::{KitDefinition, KitManager};
use kition_slide_kit::SlideDocument;
use kition_slide_model::{Comment, LayoutType, SlideFile};
use serde_json::{Value, json};

const NAME: &str = "Rests.pptx";

/// Three slides, each with a text box: a title slide and a Title and Content slide with a thread
/// each, and a blank slide. The kit opens it on the first slide with nothing selected.
struct Fixture {
    kits: KitManager,
    workspace: tempfile::TempDir,
}

impl Fixture {
    fn new() -> Self {
        let mut doc = SlideDocument::create(NAME, 960, 540).unwrap();
        doc.execute("textBox.insert", json!({ "text": "First", "rect": [60, 400, 240, 80] })).unwrap();
        doc.execute("comment.add", json!({ "slide": 0, "text": "Opening", "author": "reader", "name": "Reader", "created": "2026-11-13T09:00:00Z" }))
            .unwrap();
        doc.execute("slide.new", json!({ "layout": "titleAndContent", "title": "Results", "body": "One" })).unwrap();
        doc.execute("slide.go", json!({ "index": 1 })).unwrap();
        doc.execute("textBox.insert", json!({ "text": "Box", "rect": [60, 400, 240, 80] })).unwrap();
        doc.execute("comment.add", json!({ "slide": 1, "text": "Check", "author": "reader", "name": "Reader", "created": "2026-11-13T10:00:00Z" }))
            .unwrap();
        doc.execute("slide.new", json!({ "layout": "blank" })).unwrap();
        doc.execute("slide.go", json!({ "index": 2 })).unwrap();
        doc.execute("textBox.insert", json!({ "text": "Last", "rect": [60, 60, 240, 80] })).unwrap();
        doc.execute("slide.go", json!({ "index": 0 })).unwrap();
        let workspace = tempfile::tempdir().unwrap();
        std::fs::write(workspace.path().join(NAME), doc.save(Some(NAME)).unwrap()).unwrap();
        let definition = KitDefinition::from_manifest_text(include_str!("../../../kit.json")).unwrap();
        let kits = KitManager::new(Some(workspace.path().to_path_buf()), vec![(definition, Some(PathBuf::from(env!("CARGO_BIN_EXE_kition-slide"))))]);
        Self { kits, workspace }
    }

    fn file(&self) -> PathBuf {
        self.workspace.path().join(NAME)
    }

    fn deck(&self) -> SlideFile {
        kition_slide_engine::cmd::file::open_slide_file(NAME, &std::fs::read(self.file()).unwrap()).unwrap()
    }

    fn rewrite(&self, change: impl FnOnce(&mut SlideFile)) {
        let mut deck = self.deck();
        change(&mut deck);
        let modified = std::fs::metadata(self.file()).unwrap().modified().unwrap() - Duration::from_secs(3600);
        std::fs::write(self.file(), kition_slide_engine::cmd::file::save_bytes(&deck, "pptx").unwrap()).unwrap();
        std::fs::File::options().write(true).open(self.file()).unwrap().set_modified(modified).unwrap();
    }

    async fn call(&self, action: &str, mut params: Value) -> Result<Value, Value> {
        params["path"] = json!(NAME);
        match tools::call_as(&self.kits, &format!("slide_{action}"), &params, CallOptions::default(), "rests-agent").await {
            Some(Ok(output)) => Ok(output.output),
            Some(Err(error)) => Err(error.structured()),
            None => panic!("slide_{action} is missing"),
        }
    }

    async fn basis(&self, slide: usize) -> String {
        let seen = self.call("inspect", json!({ "target": { "slide": slide } })).await.unwrap();
        seen["revision"].as_str().unwrap().to_owned()
    }

    /// The step with `basis`: refused without a write when `must_refuse`, else applied.
    async fn run(&self, basis: &str, step: Value, must_refuse: bool) -> Value {
        let bytes = std::fs::read(self.file()).unwrap();
        let answer = self.call("run", json!({ "basis": basis, "steps": [step] })).await;
        if must_refuse {
            let error = answer.expect_err(&format!("{step} applied over a change it rests on"));
            assert_eq!(error["error"]["code"], "conflict", "{step}: {error}");
            assert_eq!(std::fs::read(self.file()).unwrap(), bytes, "refusal wrote the file");
            error
        } else {
            answer.unwrap_or_else(|error| panic!("{step}: unrelated change refused: {error}"))
        }
    }
}

fn text_of(deck: &SlideFile, slide: usize, text: &str) -> usize {
    deck.slides[slide].shapes.iter().position(|shape| shape.text.as_ref().is_some_and(|body| body.text() == text)).unwrap()
}

fn set_text(deck: &mut SlideFile, slide: usize, from: &str, to: &str) {
    let at = text_of(deck, slide, from);
    Arc::make_mut(&mut deck.slides[slide]).shapes[at].text = Some(kition_slide_model::text::TextBody::from_text(to));
}

/// Retaining by target examines every animation's target. In particular the
/// last old match can disappear from the premises the guard asks about after the file rewrite.
#[tokio::test(flavor = "multi_thread")]
async fn review_animation_remove_holds_membership_before_and_after_the_rewrite() {
    for was_matched in [false, true] {
        let f = Fixture::new();
        let deck = f.deck();
        let first = text_of(&deck, 0, "First");
        let id = deck.slides[0].shapes[first].id;
        let other = deck.slides[0].shapes.iter().find(|shape| shape.id != id).unwrap().id;
        let tool = kition_slide_engine::cmd::inspect::shape_tool_id(&deck.slides[0].shapes[first]);
        f.rewrite(|deck| {
            Arc::make_mut(&mut deck.slides[0]).animations =
                vec![kition_slide_model::Animation { shape: if was_matched { id } else { other }, ..Default::default() }];
        });
        let basis = f.basis(1).await;
        f.rewrite(|deck| Arc::make_mut(&mut deck.slides[0]).animations[0].shape = if was_matched { other } else { id });
        f.run(&basis, json!({ "command": "animation.remove", "params": { "ids": [tool] } }), true).await;
        f.kits.shutdown().await;
    }
}

/// The kit holds the edited placeholder's own font style, but SDK guard preflight asks under() for both steps
/// before executing text.edit. Paste is checked as a new text box, not the edited placeholder.
/// This needs SDK projection/execution of preceding plan context; holding the whole deck in
/// the kit would wrongly refuse unrelated edits. Keep the conflict and unchanged-file assertions.
#[tokio::test(flavor = "multi_thread")]
async fn review_paste_holds_its_own_placeholder_font_style() {
    let f = Fixture::new();
    let title_id = f.deck().slides[0].shapes.iter().find(|shape| shape.ph_type().is_some_and(|kind| kind.is_title())).unwrap().id;
    let title = f.deck().slides[0].shapes.iter().find(|shape| shape.id == title_id).unwrap().clone();
    let tool = kition_slide_engine::cmd::inspect::shape_tool_id(&title);
    let basis = f.basis(1).await;
    f.rewrite(|deck| {
        let shape = Arc::make_mut(&mut deck.slides[0]).shapes.iter_mut().find(|shape| shape.id == title_id).unwrap();
        let mut style = shape.style.clone().unwrap_or_else(|| kition_slide_model::ShapeStyle::accent(kition_slide_model::SchemeSlot::Accent1));
        style.font_ref.0 = "minor".into();
        shape.style = Some(style);
    });
    let bytes = std::fs::read(f.file()).unwrap();
    let refusal = f
        .call(
            "run",
            json!({ "basis": basis, "steps": [
        { "command": "text.edit", "params": { "id": tool } },
        { "command": "edit.paste", "params": { "text": "Hi" } },
    ] }),
        )
        .await
        .expect_err("paste applied over its placeholder's changed font style");
    assert_eq!(refusal["error"]["code"], "conflict", "{refusal}");
    assert_eq!(std::fs::read(f.file()).unwrap(), bytes, "refusal wrote the file");
    f.kits.shutdown().await;
}

/// Another shape the write reads: the current slide deleted takes its shapes; an edit of one is
/// lost, an edit on another slide is not.
#[tokio::test(flavor = "multi_thread")]
async fn slide_delete_holds_the_current_slides_shapes_but_not_another_slides() {
    for related in [true, false] {
        let f = Fixture::new();
        let basis = f.basis(1).await;
        f.rewrite(|deck| if related { set_text(deck, 0, "First", "Colleague's first") } else { set_text(deck, 2, "Last", "Colleague's last") });
        f.run(&basis, json!({ "command": "slide.delete" }), related).await;
        if !related {
            let deck = f.deck();
            assert_eq!(deck.slides.len(), 2);
            text_of(&deck, 1, "Colleague's last");
        }
        f.kits.shutdown().await;
    }
}

/// Comment thread: a slide deleted takes its threads; a reply added to one is lost. The pinned
/// SDK's outside-file journal names no change for the reply (its `others` is empty).
#[tokio::test(flavor = "multi_thread")]
async fn slide_delete_holds_its_threads() {
    let f = Fixture::new();
    let basis = f.basis(2).await;
    f.rewrite(|deck| {
        let thread = &mut Arc::make_mut(&mut deck.slides[1]).comments[0];
        thread.replies.push(Comment { author: "colleague".into(), text: "Reply".into(), date: "2026-11-13T11:00:00Z".into(), ..Default::default() });
    });
    f.run(&basis, json!({ "command": "slide.delete", "params": { "index": 1 } }), true).await;
    f.kits.shutdown().await;
}

/// Deck-level read: Apply to All copies onto every slide the deck has; a slide added is missed.
#[tokio::test(flavor = "multi_thread")]
async fn apply_all_holds_the_slide_list_but_not_slide_names() {
    for related in [true, false] {
        let f = Fixture::new();
        let basis = f.basis(1).await;
        f.rewrite(|deck| {
            if related {
                let layout = deck.slides[2].layout;
                let slide = kition_slide_model::defaults::new_slide(deck, layout);
                deck.slides.push(Arc::new(slide));
            } else {
                Arc::make_mut(&mut deck.slides[2]).name = "Colleague's name".into();
            }
        });
        f.run(&basis, json!({ "command": "transition.applyAll", "at": { "part": { "slide": 1 } } }), related).await;
        if !related {
            assert_eq!(f.deck().slides[2].name, "Colleague's name");
        }
        f.kits.shutdown().await;
    }
}

/// Deck-level read: a document property replaced; another property is not read.
#[tokio::test(flavor = "multi_thread")]
async fn properties_hold_the_replaced_property_only() {
    for related in [true, false] {
        let f = Fixture::new();
        let basis = f.basis(1).await;
        f.rewrite(|deck| if related { deck.props.title = "Colleague's title".into() } else { deck.props.author = "Colleague".into() });
        f.run(&basis, json!({ "command": "file.properties", "params": { "title": "Mine" } }), related).await;
        if !related {
            let deck = f.deck();
            assert_eq!((deck.props.title.as_str(), deck.props.author.as_str()), ("Mine", "Colleague"));
        }
        f.kits.shutdown().await;
    }
}

/// A read of a slide's place: `@2:0` names the first thread of the slide second in the deck;
/// slides reordered, it names another thread. A colleague's rename moves nothing.
#[tokio::test(flavor = "multi_thread")]
async fn positional_reply_holds_the_slides_place() {
    for related in [true, false] {
        let f = Fixture::new();
        let basis = f.basis(2).await;
        f.rewrite(|deck| if related { deck.slides.swap(0, 1) } else { Arc::make_mut(&mut deck.slides[2]).name = "Colleague's name".into() });
        f.run(&basis, json!({ "command": "comment.reply", "params": { "thread": "@1:0", "text": "Mine", "name": "Reader" } }), related).await;
        if !related {
            let deck = f.deck();
            assert_eq!(deck.slides[2].name, "Colleague's name");
            assert_eq!(deck.slides[1].comments[0].replies.last().unwrap().text, "Mine");
        }
        f.kits.shutdown().await;
    }
}

/// The slide's own field: the current slide's background is replaced whole.
#[tokio::test(flavor = "multi_thread")]
async fn background_holds_the_current_slides_background_but_not_its_name() {
    for related in [true, false] {
        let f = Fixture::new();
        let basis = f.basis(1).await;
        f.rewrite(|deck| {
            let slide = Arc::make_mut(&mut deck.slides[0]);
            if related {
                slide.background = Some(kition_slide_model::Background::Fill {
                    fill: kition_slide_model::Fill::solid(kition_slide_model::ColorRef::scheme(kition_slide_model::SchemeSlot::Accent2)),
                });
            } else {
                slide.name = "Colleague's name".into();
            }
        });
        f.run(&basis, json!({ "command": "slide.background", "params": { "color": "#336699" } }), related).await;
        if !related {
            let deck = f.deck();
            assert_eq!(deck.slides[0].name, "Colleague's name");
            assert!(deck.slides[0].background.is_some());
        }
        f.kits.shutdown().await;
    }
}

/// Inherited read: a layout change copies the chosen layout's placeholders.
#[tokio::test(flavor = "multi_thread")]
async fn slide_layout_holds_the_chosen_layouts_placeholders() {
    let f = Fixture::new();
    let basis = f.basis(3).await;
    f.rewrite(|deck| {
        let master = Arc::make_mut(&mut deck.masters[0]);
        let layout = master.layouts.iter_mut().find(|layout| layout.kind == LayoutType::TwoContent).unwrap();
        let title = layout.shapes.iter_mut().find(|shape| shape.ph_type().is_some_and(|kind| kind.is_title())).unwrap();
        title.name = "Colleague's placeholder".into();
    });
    f.run(&basis, json!({ "command": "slide.layout", "params": { "index": 2, "layout": "twoContent" } }), true).await;
    f.kits.shutdown().await;
}

/// Held fields the call does not read: an animation set on a shape does not read its text.
#[tokio::test(flavor = "multi_thread")]
async fn animation_on_a_shape_allows_its_text_to_change() {
    let f = Fixture::new();
    let deck = f.deck();
    let target = format!("shape:{}", deck.slides[1].shapes[text_of(&deck, 1, "Box")].identity);
    let basis = f.basis(2).await;
    f.rewrite(|deck| set_text(deck, 1, "Box", "Colleague's box"));
    f.run(&basis, json!({ "command": "animation.set", "params": { "effect": "zoom", "ids": [target] } }), false).await;
    let deck = f.deck();
    text_of(&deck, 1, "Colleague's box");
    assert_eq!(deck.slides[1].animations.len(), 1);
    f.kits.shutdown().await;
}

/// Held an object the call does not read: a layout change re-maps placeholders; a text box on the
/// slide stays as it is.
#[tokio::test(flavor = "multi_thread")]
async fn slide_layout_allows_a_text_box_on_the_slide_to_change() {
    let f = Fixture::new();
    let basis = f.basis(2).await;
    f.rewrite(|deck| set_text(deck, 1, "Box", "Colleague's box"));
    f.run(&basis, json!({ "command": "slide.layout", "params": { "index": 1, "layout": "twoContent" } }), false).await;
    let deck = f.deck();
    text_of(&deck, 1, "Colleague's box");
    assert_eq!(deck.layout(deck.slides[1].layout).unwrap().1.kind, LayoutType::TwoContent);
    f.kits.shutdown().await;
}
