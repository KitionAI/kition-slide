//! The Kition Slide engine façade.
//!
//! Every user-visible action is a command with a stable id (`slide.new`, `shape.insert`,
//! `text.insert`, `format.bold`…) and JSON parameters. The egui UI, the CLI, the control channel
//! and MCP all go through [`Session::execute`]. Pointer tools reduce to commands too, so every
//! gesture is journaled, undoable and replayable.
#![deny(clippy::unwrap_used, clippy::expect_used, clippy::panic, clippy::unimplemented, clippy::todo, clippy::unreachable)]
#![forbid(unsafe_code)]

mod autofit;
pub mod cmd;
pub mod connect;
pub mod context;
pub mod design;
pub mod disk;
pub mod guard;
pub mod kdm_bridge;
pub mod keys;
pub mod recovery;
pub mod sample;
mod stored;
pub mod tools;

use std::sync::Arc;

use kition_slide_model::{Shape, ShapeId, Slide, SlideFile, SlideId};
use serde::{Deserialize, Serialize};
use serde_json::Value;

pub use cmd::{CommandInfo, CommandSpec, command_specs, find_command};
pub use context::{CallSlide, EditorState, ToolId};
use kdm_model::kition_kit::kdm::OriginKind;
pub use kdm_model::kition_kit::kdm::{Origin, Version};
pub use kdm_model::kition_kit::session::{Damage, MemoryBlobs};
pub use kition_slide_geom as geom;
pub use kition_slide_media as media;
pub use kition_slide_model as model;
pub use kition_slide_render as render;
pub use tools::{Mods, PointerEvent, PointerKind, ToolKind};

#[derive(Debug, thiserror::Error)]
pub enum EngineError {
    #[error("unknown command `{0}`")]
    UnknownCommand(String),
    #[error("command `{0}` is not available right now: {1}")]
    Disabled(String, String),
    #[error("invalid parameters for `{cmd}`: {msg}")]
    BadParams { cmd: String, msg: String },
    #[error("no active slide")]
    NoDocument,
    #[error("{0}")]
    Other(String),
    #[error("internal error in `{0}` (the slide was kept as it was): {1}")]
    Internal(String, String),
}

impl From<kition_slide_model::ModelError> for EngineError {
    fn from(e: kition_slide_model::ModelError) -> Self {
        EngineError::Other(e.to_string())
    }
}

pub type Result<T> = std::result::Result<T, EngineError>;

/// Text being edited in place: a shape's body (or a table cell, or the slide's notes).
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TextSel {
    pub shape: ShapeId,
    /// Selection anchor and caret: (paragraph, char).
    pub anchor: (usize, usize),
    pub caret: (usize, usize),
    /// Editing a table cell (row, col).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cell: Option<(usize, usize)>,
    /// Editing the slide's speaker notes (shape is ignored).
    #[serde(default)]
    pub notes: bool,
}

impl TextSel {
    pub fn is_range(&self) -> bool {
        self.anchor != self.caret
    }
    pub fn ordered(&self) -> ((usize, usize), (usize, usize)) {
        if self.anchor <= self.caret { (self.anchor, self.caret) } else { (self.caret, self.anchor) }
    }
}

/// Which shape tree is being edited: slides (normal), or a master/layout (Slide Master view).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", tag = "kind")]
pub enum Target {
    #[default]
    Slides,
    Master {
        master: usize,
    },
    Layout {
        master: usize,
        layout: usize,
    },
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Selection {
    /// Current slide index.
    pub slide: usize,
    /// Slides selected in the thumbnail pane / sorter (includes the current one when non-empty).
    pub slides: Vec<SlideId>,
    /// Selected shapes on the current slide (top-level or inside groups).
    pub shapes: Vec<ShapeId>,
    pub text: Option<TextSel>,
    /// Selected table cells: (table, (r0, c0), (r1, c1)).
    pub cells: Option<(ShapeId, (usize, usize), (usize, usize))>,
    pub target: Target,
}

#[derive(Clone, Debug)]
pub struct HistoryEntry {
    pub label: String,
    pub doc: Arc<SlideFile>,
    pub selection: Selection,
    /// How many of the document's KDM replica undo steps this step took ([`kdm_bridge`]): 0 for
    /// a step that changed nothing the replica keeps, or that was taken before a reset.
    pub steps: u32,
    /// The step is an agent's (its undo scope, KitModel 1.1 M12a): the person's undo and redo
    /// leave it, the agent's take it (engine contract 2 E14a).
    pub agent: bool,
}

#[derive(Clone, Debug, Default)]
pub struct History {
    pub undo: Vec<HistoryEntry>,
    pub redo: Vec<HistoryEntry>,
    pub limit: usize,
    /// How many steps [`History::push`] ever pushed (only grows): the KDM bridge finds the steps
    /// a command pushed by it, since at the limit a push leaves the stack's length unchanged.
    pub pushed: u64,
    /// The commands running now are an agent's ([`Session::set_origin`]): the steps they push are
    /// the agent's.
    pub agent: bool,
}

impl History {
    /// Pushes a new undo step of the scope running now (clearing that scope's redo steps, as the
    /// replica does; dropping the oldest past the limit).
    pub fn push(&mut self, mut entry: HistoryEntry) {
        entry.agent = self.agent;
        let agent = self.agent;
        self.undo.push(entry);
        self.redo.retain(|e| e.agent != agent);
        self.pushed = self.pushed.saturating_add(1);
        let excess = self.undo.len().saturating_sub(self.limit.max(1));
        self.undo.drain(..excess);
    }

    /// The undo stack's indexes of the steps pushed since [`History::pushed`] was `mark` that are
    /// still on it (the newest ones).
    pub(crate) fn pushed_since(&self, mark: u64) -> std::ops::Range<usize> {
        let new = usize::try_from(self.pushed.saturating_sub(mark)).unwrap_or(usize::MAX);
        let len = self.undo.len();
        len.saturating_sub(new)..len
    }
}

/// A live pointer interaction: the document before it began (one undo step at the end).
#[derive(Clone, Debug)]
pub struct Interaction {
    pub label: String,
    pub doc: Arc<SlideFile>,
    pub selection: Selection,
    /// Who began it (the person at the pointer): what it changed so far is written as theirs
    /// when another's edit comes in before it ends ([`kdm_bridge`]).
    pub origin: Origin,
}

#[derive(Debug)]
pub struct DocState {
    pub doc: Arc<SlideFile>,
    pub selection: Selection,
    pub history: History,
    pub path: Option<String>,
    pub revision: u64,
    pub saved_doc: Arc<SlideFile>,
    pub interaction: Option<Interaction>,
    pub uid: u64,
    pub title: String,
    /// The document's KDM replica and what it last saw ([`kdm_bridge`]).
    pub(crate) link: kdm_bridge::Link,
    /// The run of typing, deleting or nudging under way ([`keys`]): the person's next keystroke
    /// of the same kind joins its undo step. Session state, never written.
    pub(crate) run: Option<keys::EditRun>,
}

static NEXT_UID: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(1);

impl DocState {
    pub fn new(mut doc: SlideFile, path: Option<String>, title: String) -> Self {
        doc.fill_comment_ids(&mut cmd::review::new_comment_id);
        let doc = Arc::new(doc);
        DocState {
            link: kdm_bridge::Link::new(&doc),
            saved_doc: doc.clone(),
            doc,
            selection: Selection::default(),
            history: History { limit: 500, ..Default::default() },
            path,
            revision: 1,
            interaction: None,
            run: None,
            uid: NEXT_UID.fetch_add(1, std::sync::atomic::Ordering::Relaxed),
            title,
        }
    }
    pub fn is_dirty(&self) -> bool {
        !Arc::ptr_eq(&self.doc, &self.saved_doc)
    }
    pub fn title(&self) -> String {
        self.path
            .as_deref()
            .and_then(|p| std::path::Path::new(p).file_stem())
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_else(|| self.title.clone())
    }
    pub fn current_slide(&self) -> Option<&Slide> {
        self.doc.slides.get(self.selection.slide).map(|s| s.as_ref())
    }
    /// Shapes of the edited tree (current slide, or the master/layout in Slide Master view).
    pub fn shapes(&self) -> &[Shape] {
        match self.selection.target {
            Target::Slides => self.current_slide().map(|s| s.shapes.as_slice()).unwrap_or(&[]),
            Target::Master { master } => self.doc.masters.get(master).map(|m| m.shapes.as_slice()).unwrap_or(&[]),
            Target::Layout { master, layout } => {
                self.doc.masters.get(master).and_then(|m| m.layouts.get(layout)).map(|l| l.shapes.as_slice()).unwrap_or(&[])
            }
        }
    }
    pub fn shape(&self, id: ShapeId) -> Option<&Shape> {
        kition_slide_model::find_shape(self.shapes(), id)
    }
    pub fn selected_shapes(&self) -> Vec<&Shape> {
        self.selection.shapes.iter().filter_map(|id| self.shape(*id)).collect()
    }
}

/// Mutable access to the edited shape tree of a slide.
pub fn shapes_mut<'a>(doc: &'a mut SlideFile, sel: &Selection) -> Option<&'a mut Vec<Shape>> {
    match sel.target {
        Target::Slides => doc.slides.get_mut(sel.slide).map(|s| &mut Arc::make_mut(s).shapes),
        Target::Master { master } => doc.masters.get_mut(master).map(|m| &mut Arc::make_mut(m).shapes),
        Target::Layout { master, layout } => {
            doc.masters.get_mut(master).and_then(|m| Arc::make_mut(m).layouts.get_mut(layout)).map(|l| &mut l.shapes)
        }
    }
}

/// Engine preferences (the UI keeps its own).
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct Prefs {
    pub snap_to_grid: bool,
    pub grid_spacing: f64,
    pub smart_guides: bool,
    /// Ctrl+D / Paste offset.
    pub duplicate_offset: f64,
    pub autocorrect: bool,
    pub smart_quotes: bool,
    pub author: String,
    pub undo_limit: usize,
    /// Minutes between AutoRecover saves of unsaved slides.
    pub recovery_minutes: f64,
}

impl Default for Prefs {
    fn default() -> Self {
        Prefs {
            snap_to_grid: true,
            grid_spacing: 6.0,
            smart_guides: true,
            duplicate_offset: 12.0,
            autocorrect: true,
            smart_quotes: true,
            author: "Presenter".into(),
            undo_limit: 500,
            recovery_minutes: 1.0,
        }
    }
}

/// Things a command or tool asks the UI to do (open a dialog, show a pane, start the show…).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", tag = "kind")]
pub enum UiRequest {
    Dialog {
        id: String,
        params: Value,
    },
    Pane {
        id: String,
    },
    StartShow {
        from: usize,
    },
    Message {
        text: String,
    },
    PickFile {
        purpose: String,
    },
    /// Media playback for the UI host (`media.play` / `pause` / `stop` / `seek`): `action` is
    /// `play`, `pause`, `toggle`, `stop` or `seek`; `ms` the media time to start from / seek to.
    Media {
        action: String,
        shape: ShapeId,
        ms: Option<u64>,
    },
}

/// Playback state of a media shape as the UI host reports it (see [`Session::media_status`]).
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MediaStatus {
    /// `loading`, `playing`, `paused` or `ended`.
    pub state: String,
    pub position_ms: u64,
    pub duration_ms: u64,
    /// Why the media is silent / blank, when it can't be decoded.
    pub error: Option<String>,
}

/// Clipboard contents copied within the app.
#[derive(Clone, Debug, Default)]
pub struct Clipboard {
    pub shapes: Vec<Shape>,
    pub slides: Vec<Slide>,
    /// Media the copied shapes/slides reference.
    pub media: Vec<kition_slide_model::MediaItem>,
    pub text: Option<kition_slide_model::TextBody>,
    pub plain: String,
    /// How many times the shapes were pasted (each paste offsets further).
    pub pastes: u32,
}

pub struct Session {
    docs: Vec<DocState>,
    active: Option<usize>,
    pub prefs: Prefs,
    pub journal: Vec<(String, Value)>,
    pub clipboard: Clipboard,
    pub ui_requests: Vec<UiRequest>,
    pub tool: tools::ToolState,
    pub(crate) untitled: u32,
    /// Format Painter: copied shape/text formatting and whether it stays on (double-click).
    pub painter: Option<(cmd::format::Painted, bool)>,
    /// Animation Painter: copied shape animations and whether it stays on (double-click).
    pub anim_painter: Option<(Vec<kition_slide_model::anim::Animation>, bool)>,
    /// Set as Default Shape: the look new shapes get.
    pub default_look: Option<cmd::format::Painted>,
    /// Nesting of commands run by commands: only the outermost one is an undo step and a journal
    /// entry.
    depth: u32,
    /// Crash-recovery folder (the app sets it; saving or closing a slide removes its entry).
    pub recovery_dir: Option<std::path::PathBuf>,
    /// Media playback state, written by the UI host every frame (empty without a host).
    pub media_status: std::collections::HashMap<ShapeId, MediaStatus>,
    /// Animated GIF pictures paused with `media.gifPlay` (the others play).
    pub gif_paused: std::collections::HashSet<ShapeId>,
    /// Who the commands act for on the replicas ([`kdm_bridge`]); `None`: the local person.
    origin: Option<Origin>,
    /// Set when the person may only comment (the Kit UI protocol's `document.comments` for a
    /// commenter): their principal id. Every command that edits the slide is refused but
    /// the `comment.*` ones (and undo and redo of their own steps), which write as that person,
    /// and the replica takes only comment changes. Set it with [`Session::set_comment_only`].
    pub(crate) comment_only: Option<String>,
    /// A host owns every file (Kition's headless kit): no file is read or written by path
    /// ([`Session::set_host_files_only`]).
    host_files_only: bool,
    /// What reads a picture or media file named by path then ([`Session::set_host_reader`]).
    host_read: Option<disk::HostRead>,
}

impl Default for Session {
    fn default() -> Self {
        Self::new()
    }
}

impl Session {
    pub fn new() -> Self {
        guard::install_panic_hook();
        Session {
            docs: vec![],
            active: None,
            prefs: Prefs::default(),
            journal: vec![],
            clipboard: Clipboard::default(),
            ui_requests: vec![],
            tool: tools::ToolState::default(),
            untitled: 0,
            default_look: None,
            painter: None,
            anim_painter: None,
            depth: 0,
            recovery_dir: None,
            media_status: Default::default(),
            gif_paused: Default::default(),
            origin: None,
            comment_only: None,
            host_files_only: false,
            host_read: None,
        }
    }
    /// A session with one new slide open.
    pub fn with_new() -> Self {
        let mut s = Self::new();
        let _ = s.execute("file.new", &Value::Null);
        s
    }
    pub fn documents(&self) -> &[DocState] {
        &self.docs
    }
    pub fn active_index(&self) -> Option<usize> {
        self.active
    }
    pub fn active(&self) -> Option<&DocState> {
        self.active.and_then(|i| self.docs.get(i))
    }
    pub fn active_mut(&mut self) -> Option<&mut DocState> {
        self.active.and_then(|i| self.docs.get_mut(i))
    }
    pub fn doc(&self) -> Result<&DocState> {
        self.active().ok_or(EngineError::NoDocument)
    }
    pub fn doc_mut(&mut self) -> Result<&mut DocState> {
        self.active_mut().ok_or(EngineError::NoDocument)
    }
    pub fn set_active(&mut self, i: usize) {
        if i < self.docs.len() {
            self.active = Some(i);
        }
    }
    pub fn add_document(&mut self, mut d: DocState) -> usize {
        d.link.comment_only = self.comment_only.is_some();
        self.docs.push(d);
        self.active = Some(self.docs.len() - 1);
        self.docs.len() - 1
    }
    pub fn close_document(&mut self, i: usize) {
        if i < self.docs.len() {
            let d = self.docs.remove(i);
            if let Some(dir) = &self.recovery_dir {
                recovery::discard(dir, d.uid);
            }
            self.active = if self.docs.is_empty() { None } else { Some(i.min(self.docs.len() - 1)) };
        }
    }
    pub fn next_untitled(&mut self) -> String {
        self.untitled += 1;
        format!("SlideFile{}", self.untitled)
    }

    /// Run command `id`. A panic inside it becomes [`EngineError::Internal`] and leaves the
    /// slide as it was.
    /// Afterwards (at the top level) what it changed is written to the document's replica.
    pub fn execute(&mut self, id: &str, params: &Value) -> Result<Value> {
        let r = self.guarded(id, |s| s.execute_unguarded(id, params));
        self.sync_all_kdm();
        // A new drawing object's KitDrawing id is its replica object, made by the write above.
        if id == "drawing.insert"
            && self.depth == 0
            && let (Ok(v), Some(st)) = (&r, self.active())
        {
            return Ok(cmd::drawing::insert_result(st, v.clone()));
        }
        r
    }

    fn execute_unguarded(&mut self, id: &str, params: &Value) -> Result<Value> {
        let spec = find_command(id).ok_or_else(|| EngineError::UnknownCommand(id.into()))?;
        if self.comment_only.is_some() && self.depth == 0 && !cmd::review::comment_only_allows(spec) {
            return Err(EngineError::Other(format!("`{id}` is not allowed: this person may only comment (read-only)")));
        }
        // Formatting acts on the selection; named shapes (`ids`/`id`) are selected for the command
        // only, so an agent formats a shape's text in one step, and the selection is as it was
        // afterwards, whether the command ran or failed.
        if self.depth == 0 && takes_named_selection(id) && (params.get("ids").is_some() || params.get("id").is_some()) {
            let kept = self.active().map(|d| d.selection.clone());
            let r = self.select_named(id, params).and_then(|()| self.run_spec(spec, id, params));
            if let (Some(kept), Some(st)) = (kept, self.active_mut()) {
                st.selection = kept;
                clamp_selection(st);
            }
            return r;
        }
        self.run_spec(spec, id, params)
    }

    /// Selects the shapes a formatting command names (`ids` or `id`) on the slide being edited.
    fn select_named(&mut self, id: &str, params: &Value) -> Result<()> {
        let ids = params.get("ids").cloned().unwrap_or_else(|| Value::Array(vec![params["id"].clone()]));
        // The ids as other shape commands take them (numbers, number strings, the KitDrawing
        // ids drawing.insert answers; cmd::targets_for): edit.select reads numbers only, so
        // "24" and a KitDrawing id selected nothing and were said to be on another slide.
        let elsewhere = |s: &Self| {
            let slide = s.active().map_or(0, |d| d.selection.slide) + 1;
            EngineError::Other(format!(
                "`{id}`: none of the shapes {ids} is on the slide being edited (slide {slide}, counted from 1); go to their slide first: name it with `onSlide` (from 1), or with the run step's `at.part`"
            ))
        };
        let numbers: Vec<u32> = match cmd::targets_for(self, params, id) {
            Ok(found) => found.into_iter().map(|s| s.0).collect(),
            Err(_) => return Err(elsewhere(self)),
        };
        self.execute_unguarded("edit.select", &serde_json::json!({ "ids": numbers }))?;
        if self.active().is_some_and(|d| d.selection.shapes.is_empty()) {
            return Err(elsewhere(self));
        }
        Ok(())
    }

    fn run_spec(&mut self, spec: &'static CommandSpec, id: &str, params: &Value) -> Result<Value> {
        if let Err(e) = (spec.enabled)(self) {
            let named = (e == cmd::NOTHING_SELECTED || e == cmd::SELECT_TWO) && (params.get("ids").is_some() || params.get("id").is_some());
            if !named {
                return Err(EngineError::Disabled(id.into(), with_remedy(e)));
            }
        }
        let outer = self.depth == 0;
        // An edit of the other undo scope while a gesture is under way (an agent's command while
        // the person drags): the gesture so far is written as its own first, and stays its own;
        // the edit is a step of its own scope. (One of the gesture's own scope joins its step.)
        let beside = outer
            && spec.undoable
            && self.active().and_then(|d| d.interaction.as_ref()).is_some_and(|i| (i.origin.kind == OriginKind::Agent) != self.acts_as_agent());
        if beside
            && let Some(st) = self.active_mut()
            && let Some(origin) = st.interaction.as_ref().map(|i| i.origin.clone())
        {
            st.sync_kdm(&origin, true);
        }
        let before = self.active().map(|d| (d.uid, d.doc.clone(), d.selection.clone()));
        self.depth += 1;
        let r = (spec.run)(self, params);
        self.depth -= 1;
        let r = r?;
        if let Some(st) = self.active_mut() {
            clamp_selection(st);
        }
        // A command that changed the document or the selection ends a run of typing or nudges
        // (`keys::EditRun`), unless it is the run's own next keystroke; one that only reads leaves
        // it.
        if outer
            && let (Some((uid, old, sel)), Some(st)) = (&before, self.active_mut())
            && st.uid == *uid
            && !st.link.join
            && (!Arc::ptr_eq(old, &st.doc) || *sel != st.selection)
        {
            st.run = None;
        }
        if let (Some((uid, old, sel)), Some(st)) = (before, self.active_mut())
            && st.uid == uid
            && !Arc::ptr_eq(&old, &st.doc)
            && (st.interaction.is_none() || beside)
            && spec.undoable
            && outer
            && !st.link.join
        {
            push_undo(st, HistoryEntry { label: spec.label.to_string(), doc: old, selection: sel, steps: 0, agent: false });
        }
        if spec.journal && outer {
            self.journal.push((id.to_string(), params.clone()));
            if self.journal.len() > 10_000 {
                self.journal.drain(..1000);
            }
        }
        Ok(r)
    }

    pub fn commands(&self) -> Vec<CommandInfo> {
        command_specs().iter().map(|c| c.info(self)).collect()
    }

    /// Mutate the active slide (copy-on-write) and bump the revision.
    pub fn edit<T>(&mut self, f: impl FnOnce(&mut SlideFile, &mut Selection) -> Result<T>) -> Result<T> {
        let st = self.doc_mut()?;
        let mut doc = (*st.doc).clone();
        let mut sel = st.selection.clone();
        let r = f(&mut doc, &mut sel)?;
        cmd::arrange::refit_groups(&st.doc, &mut doc, &sel);
        connect::reroute(&mut doc, &sel, &st.doc);
        // What the file stores (`stored`) before the layout reads it. A slide the refit changed
        // that this command did not (one inheriting from a changed layout) is settled after it,
        // and refit again when that changed what the refit read.
        stored::settle(&mut doc, Some(&st.doc));
        autofit::invalidate(&mut doc, &st.doc);
        if stored::settle(&mut doc, Some(&st.doc)) {
            autofit::invalidate(&mut doc, &st.doc);
            stored::settle(&mut doc, Some(&st.doc));
        }
        st.doc = Arc::new(doc);
        st.selection = sel;
        st.revision += 1;
        Ok(r)
    }

    /// Change only the selection (no undo step).
    pub fn select(&mut self, f: impl FnOnce(&SlideFile, &mut Selection)) -> Result<()> {
        let st = self.doc_mut()?;
        let doc = st.doc.clone();
        f(&doc, &mut st.selection);
        clamp_selection(st);
        st.revision += 1;
        Ok(())
    }
}

/// Keep the selection pointing at things that exist.
pub(crate) fn clamp_selection(st: &mut DocState) {
    let n = st.doc.slides.len();
    if n == 0 {
        st.selection.slide = 0;
    } else if st.selection.slide >= n {
        st.selection.slide = n - 1;
    }
    let ids: Vec<ShapeId> = st.selection.shapes.iter().copied().filter(|id| st.shape(*id).is_some()).collect();
    st.selection.shapes = ids;
    if let Some(t) = &st.selection.text
        && !t.notes
        && st.shape(t.shape).is_none()
    {
        st.selection.text = None;
    }
    let slides: Vec<SlideId> = st.selection.slides.iter().copied().filter(|id| st.doc.slide(*id).is_some()).collect();
    st.selection.slides = slides;
    match st.selection.target {
        Target::Master { master } if master >= st.doc.masters.len() => st.selection.target = Target::Slides,
        Target::Layout { master, layout } if st.doc.masters.get(master).is_none_or(|m| layout >= m.layouts.len()) => {
            st.selection.target = Target::Slides
        }
        _ => {}
    }
}

pub(crate) fn push_undo(st: &mut DocState, e: HistoryEntry) {
    st.history.push(e);
}

#[cfg(test)]
mod tests;

/// Formatting commands that act on the selection and accept named shapes instead (`ids`).
pub fn takes_named_selection(id: &str) -> bool {
    id.starts_with("format.") && !matches!(id, "format.painterApply" | "format.state" | "format.painter")
}

/// A disabled command's reason, with what the agent can do about it.
fn with_remedy(reason: String) -> String {
    let hint = match reason.as_str() {
        "no text insertion point" => {
            " (to format whole shapes, pass their `ids` to the format command, or select them with edit.select {ids}; text.edit {id} puts the insertion point in a text box)"
        }
        "select text or a shape" | cmd::NOTHING_SELECTED => " (pass the shapes' `ids` to the command, or select them first with edit.select {ids})",
        _ => "",
    };
    format!("{reason}{hint}")
}
