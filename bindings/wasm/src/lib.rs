//! # kition-slide-kit
//!
//! The Slide engine for Kition's in-page editor, on the Kition SDK. [`SlideDocument`]
//! implements [`KitDocument`] for slides in Kition Slide's native format (`.kslide`, and PowerPoint
//! `.pptx` through Kition Slide's own import and export) and [`KitTools`] for Kition Slide's pointer tools:
//! Select, Text Box, the shapes, freeforms, and ink, running in the engine as they do in the app.
//!
//! Document pixels are points, with the slides stacked top to bottom on one canvas (a margin
//! around them and a gap between), so the host scrolls through the slides. A click on another
//! slide makes it the current one; the tools act on the current slide. The canvas around the
//! slides is a light neutral pasteboard.
#![forbid(unsafe_code)]
#![deny(clippy::unwrap_used, clippy::expect_used, clippy::panic, clippy::unimplemented, clippy::todo, clippy::unreachable)]

pub mod acts;
pub mod check;
mod collab;
pub mod described;
mod files;
mod rests_deck;
mod rests_drawing;
mod rests_media;
mod rests_other;
mod rests_shapes;
mod rests_slides;
mod rests_tables;
mod rests_text;

use std::cell::{Cell, RefCell};
use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::sync::{Arc, Mutex, Weak};

use kition_kit::raster::{self, Frame};
use kition_kit::tools::{KitTools, PointerAction, ToolInfo, ToolMods, ToolOverlay, ToolPointer, ToolRequest, ToolResponse};
use kition_kit::{CallContext, KitCommand, KitDocument, MAX_RENDER_SIDE};
use kition_slide_engine::cmd::{self, xfrm_of};
use kition_slide_engine::tools::{Hit, Mods, PointerEvent, PointerKind, ToolKind, box_handles};
use kition_slide_engine::{CallSlide, DocState, Session, Target, UiRequest};
use kition_slide_geom::{Point, Xfrm};
use kition_slide_model::SlideFile;
use kition_slide_render::{Image, RenderOpts};
use serde_json::{Value, json};

/// Canvas around the slides, in points.
pub const MARGIN: f64 = 40.0;
/// Space between two slides, in points.
pub const GAP: f64 = 40.0;
/// Hit tolerance in screen pixels, as in the app.
const HIT_PX: f64 = 4.0;
/// Memory the 1:1 slide images kept for zoomed-out views may take (about 90 slides of 960 × 540
/// points). Past it the least recently drawn images go, never one the frame being drawn uses, so a
/// long slide file scrolls without repainting every slide.
const CACHE_BYTES: usize = 192 << 20;

/// The canvas around and between the slides: a light neutral, opaque.
const PASTEBOARD: [u8; 4] = [238, 238, 242, 255];
/// The slide kit's brand colour (`ui.brand.light` in `kit.json`); the slides are always drawn light.
const ACCENT: [u8; 4] = [192, 85, 13, 255];
const HANDLE: [u8; 4] = [255, 255, 255, 255];
const ADJUST: [u8; 4] = [255, 210, 63, 255];
const TEXT_SELECTION: [u8; 4] = [192, 85, 13, 64];
const CARET: [u8; 4] = [20, 20, 20, 255];
const GUIDE: [u8; 4] = [234, 67, 53, 255];

/// The tools on the rail: (id, label, shortcut, icon, group).
const TOOLS: &[(&str, &str, Option<&str>, &str, usize)] = &[
    ("select", "Select", Some("Escape"), "tool-selection", 0),
    ("textBox", "Text Box", Some("T"), "tool-type", 1),
    ("rect", "Rectangle", Some("R"), "tool-rect", 2),
    ("roundRect", "Rounded Rectangle", None, "tool-rounded-rect", 2),
    ("ellipse", "Oval", Some("O"), "tool-ellipse", 2),
    ("triangle", "Triangle", None, "tool-polygon", 2),
    ("rightArrow", "Arrow", None, "tool-arrow", 2),
    ("star5", "Star", None, "tool-star", 2),
    ("line", "Line", Some("L"), "tool-line", 3),
    ("straightConnector1", "Connector", None, "tool-connector", 3),
    ("freeform", "Freeform", None, "tool-pen", 4),
    ("scribble", "Scribble", None, "tool-pencil", 4),
    ("pen", "Pen", Some("P"), "tool-brush", 5),
    ("redPen", "Red Pen", None, "tool-brush", 5),
    ("bluePen", "Blue Pen", None, "tool-brush", 5),
    ("pencil", "Pencil", None, "tool-brush", 5),
    ("highlighter", "Highlighter", None, "tool-highlighter", 5),
    ("eraser", "Eraser", None, "tool-eraser", 5),
];

fn tool_kind(id: &str) -> Option<ToolKind> {
    Some(match id {
        "select" => ToolKind::Select,
        "textBox" => ToolKind::TextBox,
        "pen" => ToolKind::Ink { mode: "pen".into(), color: kition_slide_color::Rgba::BLACK, width: 2.0 },
        "redPen" => ToolKind::Ink { mode: "pen".into(), color: kition_slide_color::Rgba::rgb(216, 63, 63), width: 1.5 },
        "bluePen" => ToolKind::Ink { mode: "pen".into(), color: kition_slide_color::Rgba::rgb(46, 111, 216), width: 1.5 },
        "pencil" => ToolKind::Ink { mode: "pencil".into(), color: kition_slide_color::Rgba::rgb(90, 90, 90), width: 1.0 },
        "highlighter" => ToolKind::Ink { mode: "highlighter".into(), color: kition_slide_color::Rgba { r: 255, g: 230, b: 0, a: 255 }, width: 12.0 },
        "eraser" => ToolKind::Ink { mode: "eraser".into(), color: kition_slide_color::Rgba::BLACK, width: 2.0 },
        other if TOOLS.iter().any(|(tool, ..)| *tool == other) => ToolKind::Shape { preset: other.to_owned() },
        _ => return None,
    })
}

fn tool_id(kind: &ToolKind) -> String {
    match kind {
        ToolKind::Select => "select".into(),
        ToolKind::TextBox => "textBox".into(),
        ToolKind::Shape { preset } => preset.clone(),
        ToolKind::Ink { mode, color, width } if mode == "pen" => {
            if *color == kition_slide_color::Rgba::rgb(216, 63, 63) {
                "redPen".into()
            } else if *color == kition_slide_color::Rgba::rgb(46, 111, 216) {
                "bluePen".into()
            } else {
                "pen".into()
            }
        }
        ToolKind::Ink { mode, .. } => mode.clone(),
    }
}

/// Where a slide lies on the canvas, in points.
#[derive(Clone, Copy, Debug, PartialEq)]
struct SlideArea {
    x: f64,
    y: f64,
    w: f64,
    h: f64,
}

/// One cached 1:1 slide image and what it was rendered from.
struct Cached {
    key: (usize, usize, usize),
    image: Image,
    /// The frame that last drew it ([`SlideDocument::frame`]).
    used: u64,
}

/// Drops the least recently drawn images until the cache holds at most `budget` bytes, keeping
/// every image the current `frame` drew (a frame may need more than the budget).
fn evict(cache: &mut HashMap<usize, Cached>, frame: u64, budget: usize) {
    let mut total: usize = cache.values().map(|cached| cached.image.pixels.len()).sum();
    while total > budget {
        let Some((&index, _)) = cache.iter().filter(|(_, cached)| cached.used < frame).min_by_key(|(_, cached)| cached.used) else { break };
        if let Some(cached) = cache.remove(&index) {
            total -= cached.image.pixels.len();
        }
    }
}

/// One open slides document.
pub struct SlideDocument {
    session: Session,
    /// An isolated command/model prefix may contain a creation not inserted into its owner yet.
    /// Such a copy is never saved or returned as a live document.
    premise_plan: bool,
    /// A live batch's available parts, refreshed between its command and model operations.
    batch_media: RefCell<Weak<Mutex<BatchMedia>>>,
    /// The slide a gesture started on; it stays the target until the gesture ends.
    gesture_slide: Option<usize>,
    /// Pointer position (canvas points) for the overlays' hover state.
    zoom: f64,
    cache: RefCell<HashMap<usize, Cached>>,
    /// Counts the frames drawn from the cache, so the least recently drawn images go first.
    frame: Cell<u64>,
    /// The tool the host was last told is active (see [`KitTools::state`]).
    reported: RefCell<String>,
    /// Requests the engine made through commands, for the next tool answer.
    requests: RefCell<Vec<ToolRequest>>,
    /// Only the current slide is on the canvas (an editor's normal view), not all the slides
    /// stacked (the default, which agents and overview editors use).
    single: bool,
    /// Around and between the slides (`setBackdrop`): the host's canvas color in an editor.
    pasteboard: [u8; 4],
    /// Where the pointer last was on the current slide (slide points), for its cursor.
    pointer_at: Option<Point>,
}

#[derive(Default)]
struct BatchMedia {
    blobs: BTreeMap<String, u64>,
    missing: BTreeMap<kition_kit::kdm::ObjectId, Value>,
    /// The stored objects the deck has before the batch's model operations
    /// ([`kdm_model::stored_objects`]): one they take out of every container leaves the deck.
    existing: BTreeSet<kition_kit::kdm::ObjectId>,
}

/// The editor's state in [`KitDocument::inspect`], as JSON Pointers: the current slide, the
/// selection, what is selected, Slide Master view, and text being edited. It describes the editing
/// session, not the deck, so a headless host leaves it out of what an agent reads
/// (`kition_headless::KitInfo::volatile`).
pub const EDITOR_STATE: &[&str] = &["/slide", "/selection", "/selectedKinds", "/target", "/editingText"];

/// `file` commands that only produce bytes or files from the slide, leaving it as it is.
const FILE_OUTPUTS: &[&str] =
    &["file.saveBytes", "file.render", "file.export", "file.exportBytes", "file.saveTemplate", "file.recovery.list", "file.inspect"];

/// Whether running a command changes the slide (the catalog's `mutates`): its edits (the
/// engine's undoable commands), undo and redo, and the `file` commands that replace, reload, close,
/// or save it. Entering text editing can materialize a missing stored text body.
fn changes_document(spec: &cmd::CommandSpec) -> bool {
    spec.undoable || matches!(spec.id, "edit.undo" | "edit.redo" | "text.edit") || (spec.id.starts_with("file.") && !FILE_OUTPUTS.contains(&spec.id))
}

fn engine_error(error: impl std::fmt::Display) -> String {
    error.to_string()
}

/// Why a command did not run, with the reason typed (`kition_kit::failure`) so a host answers
/// its error code without reading the wording: an unknown command, one that cannot run now (or
/// with no slide file open), and wrong parameters. The engine's other errors carry no reason of
/// their own and are passed on as they read.
fn command_failure(error: kition_slide_engine::EngineError) -> String {
    use kition_kit::failure::{CommandFailure, failure};
    use kition_slide_engine::EngineError;
    match error {
        EngineError::UnknownCommand(_) => failure(CommandFailure::NotFound, error),
        EngineError::Disabled(..) | EngineError::NoDocument => failure(CommandFailure::Disabled, error),
        EngineError::BadParams { .. } => failure(CommandFailure::InvalidParams, error),
        EngineError::Other(_) | EngineError::Internal(..) => error.to_string(),
    }
}

/// A command's parameters refused for a reason the engine's schema cannot state.
fn params_failure(message: impl std::fmt::Display) -> String {
    kition_kit::failure::failure(kition_kit::failure::CommandFailure::InvalidParams, message)
}

impl SlideDocument {
    /// Guard references use the journal's canonical tool ids, including list-order anchors.
    /// The replica's UUID is still used internally to look up layout-owned shape/body objects.
    fn name_layout_premises(&self, under: &mut kition_kit::Under) {
        let Some(state) = self.session.active() else { return };
        let Some(session) = state.kdm() else { return };
        let names: HashMap<_, _> = state
            .doc
            .masters
            .iter()
            .flat_map(|master| &master.layouts)
            .filter_map(|layout| Some((session.model().ids().layouts.object(layout.id)?.as_str(), format!("layout:{}", layout.identity))))
            .collect();
        // A Shape type can have a tool-id definition while layout shapes have no key.
        // SDK rereads omit their private Shape edits. Hold the same consumed fields on the
        // named layout too, where the public journal records them, including stored identity.
        fn paths(
            ids: &kdm_model::Ids,
            owner: &kition_kit::kdm::ObjectId,
            shapes: &[kition_slide_model::Shape],
            base: &str,
            holder: &str,
            mapped: &mut HashMap<String, (String, String, String)>,
            depth: usize,
        ) {
            if depth > 64 {
                return;
            }
            for (at, shape) in shapes.iter().enumerate() {
                let path = format!("{base}.{at}");
                let Some(object) = ids.shape(owner, shape.id) else { continue };
                let identity = format!("{path}.identity");
                mapped.insert(object.to_string(), (holder.into(), path.clone(), identity.clone()));
                if let Some(body) = ids.body(&kdm_model::BodyOwner::Shape(object.clone())) {
                    mapped.insert(body.body.to_string(), (holder.into(), format!("{path}.body"), identity));
                }
                if !shape.children().is_empty() {
                    paths(ids, owner, shape.children(), &format!("{path}.children"), holder, mapped, depth + 1);
                }
            }
        }
        let mut mapped = HashMap::new();
        for master in &state.doc.masters {
            for layout in &master.layouts {
                if let Some(owner) = session.model().ids().layouts.object(layout.id) {
                    paths(session.model().ids(), owner, &layout.shapes, "shapes", &format!("layout:{}", layout.identity), &mut mapped, 0);
                }
            }
        }
        let mut extra: BTreeMap<String, std::collections::BTreeSet<String>> = BTreeMap::new();
        for premise in &under.premises {
            if let Some((holder, prefix, identity)) = premise.id.as_str().and_then(|id| mapped.get(id)) {
                let held = extra.entry(holder.clone()).or_default();
                held.insert(identity.clone());
                match &premise.fields {
                    Some(fields) => held.extend(fields.iter().map(|field| format!("{prefix}.{field}"))),
                    None => {
                        held.insert(prefix.clone());
                    }
                }
            }
        }
        let name = |value: &mut Value| {
            if let Some(id) = value.as_str().and_then(|id| names.get(id)) {
                *value = json!(id);
            }
        };
        for value in &mut under.objects {
            name(value);
        }
        for premise in &mut under.premises {
            name(&mut premise.id);
        }
        for window in &mut under.within {
            name(&mut window.parent);
        }
        for order in &mut under.orders {
            name(&mut order.parent);
            for object in &mut order.objects {
                name(object);
            }
        }
        for content in &mut under.content {
            name(&mut content.id);
        }
        for (id, fields) in extra {
            if let Some(premise) = under.premises.iter_mut().find(|premise| premise.id == id) {
                if let Some(held) = &mut premise.fields {
                    held.extend(fields);
                    held.sort();
                    held.dedup();
                }
            } else {
                under.premises.push(kition_kit::Premise::on(id).fields(fields).positions(Vec::<String>::new()));
            }
        }
    }

    /// Refuses a key the command's schema does not name instead of running without it, and names
    /// the parameter it resembles (platform rule E7a). The schemas are the ones the catalogue
    /// offers agents, read once, since a command's parameters do not change with the deck.
    fn refuse_unknown(&self, command: &str, params: &Value) -> Result<(), String> {
        static SCHEMAS: std::sync::OnceLock<std::collections::HashMap<String, Value>> = std::sync::OnceLock::new();
        let schemas = SCHEMAS.get_or_init(|| self.commands().into_iter().filter_map(|command| Some((command.id, command.schema?))).collect());
        match schemas.get(command) {
            Some(schema) => kition_kit::schema::refuse_unknown(command, schema, params),
            None => Ok(()),
        }
    }

    /// A document of `session`, which reaches no file by path when a host owns every file of
    /// this process ([`served_by_host`]): it reads the pictures and media a command names by
    /// path through the host ([`lent_file`]).
    fn from_session(mut session: Session) -> Self {
        session.set_host_files_only(served_by_host());
        session.set_host_reader(Some(lent_file));
        Self {
            session,
            premise_plan: false,
            batch_media: RefCell::new(Weak::new()),
            gesture_slide: None,
            zoom: 1.0,
            cache: RefCell::new(HashMap::new()),
            frame: Cell::new(0),
            reported: RefCell::new("select".into()),
            requests: RefCell::new(Vec::new()),
            single: false,
            pasteboard: PASTEBOARD,
            pointer_at: None,
        }
    }

    /// Lets the person only comment (a commenter of a shared slide, the Kit UI protocol's
    /// `document.comments`): `me` is their principal id, the author of what they write. Commands
    /// that edit the slides, the engine tools' edits, and saving are refused; the `comment.*`
    /// commands run, and the replica takes only comment changes. Empty `me`: no principal known.
    pub fn set_comment_only(&mut self, me: &str) {
        self.session.set_comment_only(Some(me.to_owned()));
    }

    fn refuse_comment_only(&self, what: &str) -> Result<(), String> {
        if self.session.comment_only().is_some() {
            return Err(format!("the engine tools' {what} is not allowed: this person may only comment (read-only)"));
        }
        Ok(())
    }

    /// Paints the canvas around the slides in `#rrggbb`, so it matches the editor's own canvas.
    pub fn set_backdrop(&mut self, color: &str) -> Result<(), String> {
        let hex = color.trim().trim_start_matches('#');
        let channel = |at: usize| hex.get(at..at + 2).and_then(|part| u8::from_str_radix(part, 16).ok());
        match (hex.len(), channel(0), channel(2), channel(4)) {
            (6, Some(r), Some(g), Some(b)) => {
                self.pasteboard = [r, g, b, 255];
                self.cache.borrow_mut().clear();
                Ok(())
            }
            _ => Err(format!("{color} is not a #rrggbb color")),
        }
    }

    /// Shows only the current slide (`true`, an editor's normal view) or all the slides stacked.
    pub fn set_single(&mut self, single: bool) {
        self.single = single;
    }

    /// A theme preview rendered from a copy; it never edits the open slides.
    pub fn render_theme(&self, name: &str, index: usize, width: u32) -> Result<Vec<u8>, String> {
        let mut pres = self.pres().ok_or("no slides are open")?.clone();
        let theme = kition_slide_model::theme::builtin_themes().into_iter().find(|theme| theme.name == name).ok_or("unknown theme")?;
        if index >= pres.slides.len() {
            return Err("no such slide".into());
        }
        if let Some(master) = pres.masters.first_mut() {
            let master = std::sync::Arc::make_mut(master);
            master.color_map = kition_slide_model::theme::builtin_color_map(&theme.name);
            master.theme = theme;
        }
        let width = width.clamp(1, MAX_RENDER_SIDE);
        let scale = f64::from(width) / pres.slide_size.width.max(1.0);
        let height = ((pres.slide_size.height * scale).round() as u32).clamp(1, MAX_RENDER_SIDE);
        Ok(kition_slide_render::render_slide(&pres, index, &RenderOpts { scale, size: Some((width, height)), ..Default::default() }).to_straight())
    }

    /// Media stored in the selected shape, for the embedded editor's own player.
    pub fn media_data(&self, id: u32) -> Result<Vec<u8>, String> {
        let state = self.state()?;
        let shape = state.shape(kition_slide_model::ShapeId(id)).ok_or("no such shape")?;
        let kition_slide_model::ShapeKind::Media(clip) = &shape.kind else { return Err("not a media shape".into()) };
        state.doc.media(clip.media).map(|item| item.data.to_vec()).ok_or_else(|| "media data is missing".into())
    }

    /// Straight RGBA8 of slide `index` at `width` pixels wide (its height keeps the slide's
    /// proportions), for thumbnails; drawn directly at that scale, so text stays sharp.
    pub fn render_slide(&self, index: usize, width: u32) -> Result<Vec<u8>, String> {
        let pres = self.pres().ok_or("no slides are open")?;
        if index >= pres.slides.len() {
            return Err(format!("there is no slide {}", index + 1));
        }
        let (w, h) = self.slide_size();
        let width = width.clamp(1, MAX_RENDER_SIDE);
        let scale = f64::from(width) / w.max(1.0);
        let height = ((h * scale).round() as u32).clamp(1, MAX_RENDER_SIDE);
        let image =
            kition_slide_render::render_slide(pres, index, &RenderOpts { scale, edit: true, size: Some((width, height)), ..RenderOpts::default() });
        let mut out = self.pasteboard.repeat(width as usize * height as usize);
        over(&mut out, &image.pixels);
        straight(&mut out);
        Ok(out)
    }

    /// Straight RGBA8 of slide `index` drawn as a slide show shows it, at exactly `width` x
    /// `height` pixels (scaled from the slide's width), for measuring against reference renders:
    /// no placeholder prompts or outlines, no editor marks, hidden shapes hidden, and date fields
    /// showing `date` (`None`: the clock).
    pub fn render_slide_clean(
        &self,
        index: usize,
        width: u32,
        height: u32,
        date: Option<kition_slide_text::datetime::DateTime>,
    ) -> Result<Vec<u8>, String> {
        let pres = self.pres().ok_or("no slides are open")?;
        if index >= pres.slides.len() {
            return Err(format!("there is no slide {}", index + 1));
        }
        let (width, height) = (width.clamp(1, MAX_RENDER_SIDE), height.clamp(1, MAX_RENDER_SIDE));
        let scale = f64::from(width) / self.slide_size().0.max(1.0);
        let opts = RenderOpts { scale, edit: false, size: Some((width, height)), now: date, ..RenderOpts::default() };
        Ok(kition_slide_render::render_slide(pres, index, &opts).to_straight())
    }

    /// The slides on the canvas: the current one, or all of them.
    fn shown(&self) -> Vec<usize> {
        if self.single { vec![self.current_slide()] } else { (0..self.slide_count()).collect() }
    }

    fn state(&self) -> Result<&DocState, String> {
        self.session.doc().map_err(engine_error)
    }

    fn pres(&self) -> Option<&SlideFile> {
        self.session.active().map(|state| &*state.doc)
    }

    fn addressed_shapes(&self, command: &str, params: &Value) -> Vec<(u32, u32)> {
        let Some(deck) = self.pres() else { return Vec::new() };
        let mut local = params.clone();
        let implied = self.session.take_tool_ids(&mut local).ok().flatten();
        let index = |key: &str| params.get(key).and_then(Value::as_u64).and_then(|n| usize::try_from(n).ok());
        let on = (command == "comment.add")
            .then(|| index("slide"))
            .flatten()
            .or(implied)
            .or_else(|| index(ON_SLIDE).and_then(|n| n.checked_sub(1)))
            .unwrap_or_else(|| self.current_slide());
        let mut addresses = Vec::new();
        for key in ["id", "ids", "from", "to", "trigger"] {
            let shape = (!command.starts_with("slide.") && matches!(key, "id" | "ids"))
                || (matches!(command, "shape.connect" | "drawing.connect") && matches!(key, "from" | "to"))
                || (command == "drawing.reconnect" && key == "to")
                || (command == "animation.timing" && key == "trigger");
            if !shape {
                continue;
            }
            let Some(value) = params.get(key) else { continue };
            for value in value.as_array().map(Vec::as_slice).unwrap_or_else(|| std::slice::from_ref(value)) {
                if let Some(kition_slide_engine::ToolId::Shape(slide, number)) = value.as_str().and_then(kition_slide_engine::ToolId::parse) {
                    addresses.push((slide.0, number.0));
                } else if let Some(number) = value.as_u64().or_else(|| value.as_str()?.parse().ok()).and_then(|n| u32::try_from(n).ok())
                    && let Some(slide) = deck.slides.get(on)
                {
                    addresses.push((slide.id.0, number));
                }
            }
        }
        addresses
    }

    /// A body register reached through an unnamed layout/master shape is journalled on the
    /// body itself. Hold that exact register as well as its owning path. These are the same
    /// consumed fields; no paragraph content or unrelated placeholder is added.
    fn inherited_body_premises(&self, found: &mut kition_kit::Under) {
        let Some(session) = self.session.kdm() else { return };
        let ids = session.model().ids();
        let additions: Vec<_> = found
            .premises
            .iter()
            .filter_map(|premise| {
                let owner = kition_kit::kdm::ObjectId::from_string(premise.id.as_str()?.to_owned());
                let body = ids.body(&kdm_model::BodyOwner::Shape(owner))?;
                let fields: Vec<_> = premise
                    .fields
                    .as_ref()?
                    .iter()
                    .filter_map(|field| {
                        field.strip_prefix("body.props").filter(|rest| rest.is_empty() || rest.starts_with('.')).map(|rest| format!("props{rest}"))
                    })
                    .collect();
                (!fields.is_empty()).then(|| (json!(body.body.as_str()), fields))
            })
            .collect();
        for (id, fields) in additions {
            if let Some(existing) = found.premises.iter_mut().find(|premise| premise.id == id) {
                if let Some(held) = &mut existing.fields {
                    for field in fields {
                        if !held.contains(&field) {
                            held.push(field);
                        }
                    }
                }
            } else {
                found.premises.push(kition_kit::Premise::on(id).fields(fields).positions(Vec::<String>::new()));
            }
        }
    }

    fn slide_size(&self) -> (f64, f64) {
        self.pres().map_or((960.0, 540.0), |pres| (pres.slide_size.width, pres.slide_size.height))
    }

    /// Runs `command` (through `run`, which is the engine's `execute` or `execute_as`) as the call
    /// names it. `onSlide` (counted from 1, as diagnostics name slides) is the slide the command's
    /// `ids` are on: it is current for this command only and is taken off before the engine sees
    /// the parameters. A command given what it acts on (`ids`, `id`, `onSlide`) leaves the current
    /// slide and the selection as they were, whatever it selected on the way, so no call depends
    /// on or leaves behind where the editor was; only the commands whose work is moving the editor
    /// ([`acts::moves_the_editor`]: `edit.select`, `slide.go`, `text.edit`, …) keep their effect.
    pub(crate) fn run_named(
        &mut self,
        command: &str,
        mut params: Value,
        run: impl FnOnce(&mut Session, &Value) -> kition_slide_engine::Result<Value>,
    ) -> Result<Value, String> {
        let number = match params.as_object_mut().and_then(|object| object.remove(ON_SLIDE)) {
            None => None,
            Some(slide) => {
                let count = self.slide_count();
                Some(
                    slide
                        .as_u64()
                        .filter(|n| usize::try_from(*n).is_ok_and(|n| (1..=count).contains(&n)))
                        .ok_or_else(|| params_failure(format!("{command}: `{ON_SLIDE}` is a slide number from 1 to {count}, not {slide}")))?,
                )
            }
        };
        let explicit_slide = number.is_some();
        // Tool ids (`shape:<slideId>/<n>`, `slide:<id>`) become numbers; a shape's names its slide.
        let implied = if self.session.active().is_some() {
            self.session.take_tool_ids(&mut params).map_err(|e| params_failure(format!("{command}: {e}")))?
        } else {
            None
        };
        let number = match (number, implied) {
            (Some(number), Some(index)) if usize::try_from(number).ok() != index.checked_add(1) => {
                return Err(params_failure(format!(
                    "{command}: the shapes' tool ids are on slide {} (counted from 1), and `{ON_SLIDE}` names slide {number}: a tool id names its slide, so leave `{ON_SLIDE}` out",
                    index + 1
                )));
            }
            (None, Some(index)) => Some(u64::try_from(index).unwrap_or(u64::MAX).saturating_add(1)),
            (number, _) => number,
        };
        let named = number.is_some() || ["ids", "id"].iter().any(|key| params.get(*key).is_some_and(|value| !value.is_null()));
        let kept = if named && !acts::moves_the_editor(command) { self.session.editor_state() } else { None };
        let current = self
            .session
            .active()
            .filter(|state| matches!(state.selection.target, Target::Slides))
            .and_then(|state| state.selection.slide.checked_add(1));
        if let Some(number) = number.filter(|number| explicit_slide || usize::try_from(*number).ok() != current) {
            self.session.set_call_context(Some(CallSlide::Number(number)), &[]).map_err(command_failure)?;
        }
        let result = run(&mut self.session, &params).map_err(command_failure);
        if let Some(kept) = kept {
            // Fails only when the command closed the slide file, which leaves nothing to put back.
            let _ = self.session.restore_editor_state(&kept);
        }
        self.refresh_batch_media()?;
        result
    }

    fn refresh_batch_media(&self) -> Result<(), String> {
        if let Some(media) = self.batch_media.borrow().upgrade() {
            *media.lock().map_err(|error| format!("batch media store unavailable: {error}"))? = self.current_batch_media();
        }
        Ok(())
    }

    fn current_batch_media(&self) -> BatchMedia {
        let mut media = BatchMedia { blobs: self.blob_sizes(), ..Default::default() };
        if let Some(session) = self.session.active().and_then(|state| state.kdm()) {
            let replica = session.replica();
            media.existing = kdm_model::stored_objects(replica);
            for id in replica.objects_of_type("Media") {
                if replica.get(&id, "blob").is_ok_and(|blob| blob.is_null())
                    && let Ok(props) = replica.get(&id, "props")
                    && props.get("link").and_then(Value::as_str).is_none_or(str::is_empty)
                {
                    media.missing.insert(id, props);
                }
            }
        }
        media
    }

    fn blob_sizes(&self) -> BTreeMap<String, u64> {
        let Some(state) = self.session.active() else { return BTreeMap::new() };
        match state.kdm() {
            Some(session) => session.blobs().0.iter().map(|(hash, bytes)| (hash.clone(), bytes.len() as u64)).collect(),
            None => state
                .doc
                .media
                .iter()
                .filter(|media| !media.data.is_empty())
                .map(|media| (kition_kit::session::sha256_hex(&media.data), media.data.len() as u64))
                .collect(),
        }
    }

    fn slide_count(&self) -> usize {
        self.pres().map_or(0, |pres| pres.slides.len())
    }

    fn area(&self, index: usize) -> SlideArea {
        let (w, h) = self.slide_size();
        let row = if self.single { 0.0 } else { index as f64 };
        SlideArea { x: MARGIN, y: MARGIN + row * (h + GAP), w, h }
    }

    /// The slide nearest canvas height `y`.
    fn slide_at(&self, y: f64) -> usize {
        if self.single {
            return self.current_slide();
        }
        let (_, h) = self.slide_size();
        let index = ((y - MARGIN + GAP / 2.0) / (h + GAP)).floor().max(0.0) as usize;
        index.min(self.slide_count().saturating_sub(1))
    }

    fn current_slide(&self) -> usize {
        self.session.active().map_or(0, |state| state.selection.slide)
    }

    /// Premultiplied RGBA of the canvas area at (`x`, `y`) at `scale` pixels per point: each slide
    /// it touches drawn into place on the pasteboard.
    fn compose(&self, x: f64, y: f64, scale: f64, width: u32, height: u32) -> Vec<u8> {
        let mut out = self.pasteboard.repeat(width as usize * height as usize);
        let Some(pres) = self.pres() else { return out };
        let (x1, y1) = (x + f64::from(width) / scale, y + f64::from(height) / scale);
        for index in self.shown() {
            let area = self.area(index);
            if area.y > y1 || area.y + area.h < y || area.x > x1 || area.x + area.w < x {
                continue;
            }
            let opts = RenderOpts {
                scale,
                edit: true,
                offset: ((area.x - x) * scale, (area.y - y) * scale),
                size: Some((width, height)),
                ..RenderOpts::default()
            };
            let target = self.session.active().map(|st| st.selection.target).unwrap_or_default();
            let image = match target {
                Target::Slides => kition_slide_render::render_slide(pres, index, &opts),
                Target::Master { master } => kition_slide_render::render_layout(pres, master, None, &opts),
                Target::Layout { master, layout } => kition_slide_render::render_layout(pres, master, Some(layout), &opts),
            };
            over(&mut out, &image.pixels);
        }
        out
    }

    /// A slide at 1:1, from the cache when the slide, its layout and master, and its number are
    /// unchanged.
    fn slide_image(&self, index: usize) -> Option<std::cell::Ref<'_, Image>> {
        let pres = self.pres()?;
        let slide = pres.slides.get(index)?;
        let masters = pres.masters.iter().fold(0usize, |hash, master| hash.rotate_left(5) ^ (std::sync::Arc::as_ptr(master) as usize));
        let key = (std::sync::Arc::as_ptr(slide) as usize, masters, pres.slides.len());
        let frame = self.frame.get();
        let fresh = self.cache.borrow_mut().get_mut(&index).filter(|cached| cached.key == key).map(|cached| cached.used = frame).is_some();
        if !fresh {
            let image = kition_slide_render::render_slide(pres, index, &RenderOpts { scale: 1.0, edit: true, ..RenderOpts::default() });
            let mut cache = self.cache.borrow_mut();
            cache.insert(index, Cached { key, image, used: frame });
            evict(&mut cache, frame, CACHE_BYTES);
        }
        std::cell::Ref::filter_map(self.cache.borrow(), |cache| cache.get(&index).map(|cached| &cached.image)).ok()
    }

    /// Straight-alpha 1:1 pixels of the integer area (`x0`, `y0`) `w` × `h`, from the cached slides.
    fn full(&self, x0: i32, y0: i32, w: u32, h: u32) -> Vec<u8> {
        if self.session.active().is_some_and(|st| st.selection.target != Target::Slides) {
            let mut out = self.compose(f64::from(x0), f64::from(y0), 1.0, w, h);
            straight(&mut out);
            return out;
        }
        let mut out = self.pasteboard.repeat(w as usize * h as usize);
        self.frame.set(self.frame.get() + 1);
        for index in self.shown() {
            let area = self.area(index);
            let (sx, sy) = (area.x.round() as i32, area.y.round() as i32);
            // Only the slides the area shows are drawn (and kept): a slide's 1:1 image is at most
            // its size rounded up.
            let (aw, ah) = (area.w.ceil() as i32 + 1, area.h.ceil() as i32 + 1);
            if sx >= x0 + w as i32 || sy >= y0 + h as i32 || sx + aw <= x0 || sy + ah <= y0 {
                continue;
            }
            let Some(image) = self.slide_image(index) else { continue };
            let (iw, ih) = (image.width as i32, image.height as i32);
            let (ox0, oy0) = (sx.max(x0), sy.max(y0));
            let (ox1, oy1) = ((sx + iw).min(x0 + w as i32), (sy + ih).min(y0 + h as i32));
            if ox0 >= ox1 || oy0 >= oy1 {
                continue;
            }
            for row in oy0..oy1 {
                let from = (((row - sy) * iw + (ox0 - sx)) * 4) as usize;
                let to = (((row - y0) * w as i32 + (ox0 - x0)) * 4) as usize;
                let len = ((ox1 - ox0) * 4) as usize;
                if let (Some(source), Some(target)) = (image.pixels.get(from..from + len), out.get_mut(to..to + len)) {
                    over(target, source);
                }
            }
        }
        straight(&mut out);
        out
    }

    /// Takes the engine's requests to the host: dialogs and messages.
    fn take_requests(&mut self) {
        for request in self.session.ui_requests.drain(..) {
            let request = match request {
                UiRequest::Dialog { id, params } => ToolRequest::Dialog { name: id, params },
                UiRequest::Message { text } => ToolRequest::Status { text },
                _ => continue,
            };
            self.requests.borrow_mut().push(request);
        }
    }

    fn mods(mods: ToolMods) -> Mods {
        Mods { shift: mods.shift, alt: mods.alt, cmd: mods.command || mods.control }
    }

    /// Canvas points of a point on the current slide.
    fn to_canvas(&self, p: Point) -> (f64, f64) {
        let area = self.area(self.current_slide());
        (area.x + p.x, area.y + p.y)
    }

    fn quad(&self, xfrm: &Xfrm, corners: [(f64, f64); 4]) -> String {
        let a = xfrm.affine();
        let mut d = String::new();
        for (index, (px, py)) in corners.iter().enumerate() {
            let (x, y) = self.to_canvas(a * Point::new(*px, *py));
            d.push_str(&format!("{}{x:.2},{y:.2} ", if index == 0 { "M" } else { "L" }));
        }
        d.push('Z');
        d
    }

    fn path_of(&self, points: &[Point]) -> String {
        let mut d = String::new();
        for (index, point) in points.iter().enumerate() {
            let (x, y) = self.to_canvas(*point);
            d.push_str(&format!("{}{x:.2},{y:.2} ", if index == 0 { "M" } else { "L" }));
        }
        d
    }

    /// The selection, text editing, and tool previews, ported from upstream's desktop UI canvas
    /// (`crates/ui-egui/src/canvas.rs` `overlays`, before that UI was removed).
    fn overlays(&self) -> Vec<ToolOverlay> {
        let mut out = Vec::new();
        let Some(st) = self.session.active() else { return out };
        let editing = st.selection.text.clone();
        let anchor = |(x, y): (f64, f64), color: [u8; 4], size: f32| ToolOverlay::Anchor { x, y, color, filled: true, size };
        for id in &st.selection.shapes {
            let Some(sh) = st.shape(*id) else { continue };
            let x = xfrm_of(&st.doc, &st.selection, sh);
            let a = x.affine();
            if sh.is_line() {
                for end in [Point::new(0.0, 0.0), Point::new(x.w, x.h)] {
                    out.push(anchor(self.to_canvas(a * end), HANDLE, 9.0));
                }
                continue;
            }
            let editing_this = editing.as_ref().is_some_and(|e| e.shape == *id && !e.notes);
            out.push(ToolOverlay::Path {
                d: self.quad(&x, [(0.0, 0.0), (x.w, 0.0), (x.w, x.h), (0.0, x.h)]),
                color: ACCENT,
                width: 1.0,
                dashed: editing_this,
            });
            if st.selection.shapes.len() == 1 || !editing_this {
                let (handles, rotate) = box_handles(&x);
                if !editing_this {
                    let (tx, ty) = self.to_canvas(handles[1]);
                    let (rx, ry) = self.to_canvas(rotate);
                    out.push(ToolOverlay::Line { ax: tx, ay: ty, bx: rx, by: ry, color: ACCENT, dashed: false });
                    out.push(ToolOverlay::Anchor { x: rx, y: ry, color: ACCENT, filled: false, size: 11.0 });
                }
                for handle in handles {
                    out.push(anchor(self.to_canvas(handle), HANDLE, 9.0));
                }
                for adjust in &kition_slide_render::shape_geometry(sh, x.w, x.h).handles {
                    out.push(anchor(self.to_canvas(a * adjust.pos), ADJUST, 9.0));
                }
            }
        }
        // Cell ranges are editor overlays, never part of exported slide pixels.
        if let Some((id, a, b)) = st.selection.cells
            && let Some(sh) = st.shape(id)
            && let kition_slide_model::ShapeKind::Table(table) = &sh.kind
        {
            let m = xfrm_of(&st.doc, &st.selection, sh).affine();
            for (r, c, rect) in kition_slide_render::table_cell_rects(table, &kition_slide_engine::tools::table_rows(st, table)) {
                let Some(cell) = table.cell(r, c) else { continue };
                if r <= a.0.max(b.0)
                    && r.saturating_add(cell.row_span.max(1) as usize) > a.0.min(b.0)
                    && c <= a.1.max(b.1)
                    && c.saturating_add(cell.grid_span.max(1) as usize) > a.1.min(b.1)
                {
                    let quad = [(rect.x0, rect.y0), (rect.x1, rect.y0), (rect.x1, rect.y1), (rect.x0, rect.y1)].map(|(x, y)| {
                        let (x, y) = self.to_canvas(m * Point::new(x, y));
                        [x, y]
                    });
                    out.push(ToolOverlay::Highlight { quad, color: TEXT_SELECTION });
                }
            }
        }
        // The caret and the text selection.
        if let Some(ts) = &editing
            && !ts.notes
            && let Some(sh) = st.shape(ts.shape)
            && let Some(layout) = cmd::text::layout_for(st, ts)
        {
            let x = xfrm_of(&st.doc, &st.selection, sh);
            let rect = ts
                .cell
                .and_then(|cell| cmd::table::cell_rect(st, ts.shape, cell))
                .unwrap_or_else(|| kition_slide_render::shape_geometry(sh, x.w, x.h).text_rect);
            let m = kition_slide_render::text_transform(x.affine(), rect, layout.rotation);
            let (s0, s1) = ts.ordered();
            for r in layout.selection_rects(kition_slide_text::Pos { para: s0.0, ch: s0.1 }, kition_slide_text::Pos { para: s1.0, ch: s1.1 }) {
                let quad = [(r.x0, r.y0), (r.x1, r.y0), (r.x1, r.y1), (r.x0, r.y1)].map(|(px, py)| {
                    let (cx, cy) = self.to_canvas(m * Point::new(px, py));
                    [cx, cy]
                });
                out.push(ToolOverlay::Highlight { quad, color: TEXT_SELECTION });
            }
            if !ts.is_range()
                && let Some((cx, top, bottom)) = layout.caret(kition_slide_text::Pos { para: ts.caret.0, ch: ts.caret.1 })
            {
                let (ax, ay) = self.to_canvas(m * Point::new(cx, top));
                let (bx, by) = self.to_canvas(m * Point::new(cx, bottom));
                out.push(ToolOverlay::Line { ax, ay, bx, by, color: CARET, dashed: false });
            }
        }
        let tool = &self.session.tool;
        for guide in &tool.guides {
            let (a, b) = if guide.vertical {
                (Point::new(guide.pos, guide.from), Point::new(guide.pos, guide.to))
            } else {
                (Point::new(guide.from, guide.pos), Point::new(guide.to, guide.pos))
            };
            let ((ax, ay), (bx, by)) = (self.to_canvas(a), self.to_canvas(b));
            out.push(ToolOverlay::Line { ax, ay, bx, by, color: GUIDE, dashed: true });
        }
        if let Some(m) = tool.marquee {
            let ((x0, y0), (x1, y1)) = (self.to_canvas(Point::new(m.x0, m.y0)), self.to_canvas(Point::new(m.x1, m.y1)));
            out.push(ToolOverlay::Marquee { x0, y0, x1, y1 });
        }
        if let Some(preview) = tool.preview {
            let line_like = matches!(&tool.kind, ToolKind::Shape { preset } if kition_slide_geom::preset::is_line_like(preset));
            if line_like {
                let a = preview.affine();
                let ((ax, ay), (bx, by)) = (self.to_canvas(a * Point::new(0.0, 0.0)), self.to_canvas(a * Point::new(preview.w, preview.h)));
                out.push(ToolOverlay::Line { ax, ay, bx, by, color: ACCENT, dashed: false });
            } else {
                let d = self.quad(&preview, [(0.0, 0.0), (preview.w, 0.0), (preview.w, preview.h), (0.0, preview.h)]);
                out.push(ToolOverlay::Path { d, color: ACCENT, width: 1.5, dashed: true });
            }
        }
        if tool.path_preview.len() > 1 {
            out.push(ToolOverlay::Path { d: self.path_of(&tool.path_preview), color: ACCENT, width: 1.5, dashed: false });
            if tool.path_closing
                && let Some(start) = tool.path_preview.first()
            {
                out.push(anchor(self.to_canvas(*start), ACCENT, 10.0));
            }
        }
        for site in &tool.sites {
            out.push(anchor(self.to_canvas(*site), HANDLE, 7.0));
        }
        if let Some(glue) = tool.glue {
            out.push(anchor(self.to_canvas(glue), ACCENT, 10.0));
        }
        if tool.ink_preview.len() > 1 {
            let points: Vec<Point> = tool.ink_preview.iter().map(|(x, y, _)| Point::new(*x, *y)).collect();
            let (color, width) = match &tool.kind {
                ToolKind::Ink { color, width, mode } => {
                    ([color.r, color.g, color.b, if mode == "highlighter" { 115 } else { 255 }], (*width * self.zoom).max(1.0) as f32)
                }
                _ => (ACCENT, 1.5),
            };
            out.push(ToolOverlay::Path { d: self.path_of(&points), color, width, dashed: false });
        }
        out
    }

    fn respond(&mut self) -> ToolResponse {
        self.take_requests();
        let mut requests: Vec<ToolRequest> = self.requests.borrow_mut().drain(..).collect();
        // The rail follows the tool: a shape tool returns to Select after one shape.
        let tool = self.tool();
        if *self.reported.borrow() != tool {
            requests.push(ToolRequest::SwitchTool { id: tool.clone() });
            *self.reported.borrow_mut() = tool;
        }
        let state = self.session.active();
        let editing = state.is_some_and(|st| st.selection.text.is_some());
        let selected = state.is_some_and(|st| !st.selection.shapes.is_empty());
        // Typing on one selected shape replaces its text, as in PowerPoint.
        let one = state.is_some_and(|st| st.selection.shapes.len() == 1);
        let drawing = !matches!(self.session.tool.kind, ToolKind::Select);
        ToolResponse {
            requests,
            overlays: self.overlays(),
            cursor: if let Some(Hit::TableDivider { vertical, .. }) = &self.session.tool.hover
                && !drawing
            {
                if *vertical { "ew-resize" } else { "ns-resize" }
            } else if self.pointer_at.is_some_and(|p| self.session.over_text(p, HIT_PX / self.zoom)) {
                // Over the text being edited, or text a click puts the caret in: the I-beam.
                "text"
            } else if drawing {
                "crosshair"
            } else {
                "default"
            }
            .to_owned(),
            // Editing keys come here while text is edited, shapes are selected (Delete, arrows,
            // Escape), or a gesture or freeform is under way.
            busy: editing || selected || self.session.tool.dragging(),
            wants_text: editing || one,
        }
    }
}

/// `out` = `src` over `out`, both premultiplied.
fn over(out: &mut [u8], src: &[u8]) {
    for (dst, src) in out.as_chunks_mut::<4>().0.iter_mut().zip(src.as_chunks::<4>().0) {
        let keep = 255 - u32::from(src[3]);
        if keep == 255 {
            continue;
        }
        for channel in 0..4 {
            dst[channel] = (u32::from(src[channel]) + (u32::from(dst[channel]) * keep + 127) / 255).min(255) as u8;
        }
    }
}

/// Premultiplied to straight alpha, in place.
fn straight(pixels: &mut [u8]) {
    for px in pixels.as_chunks_mut::<4>().0 {
        let a = u32::from(px[3]);
        if a != 0 && a != 255 {
            for channel in px.iter_mut().take(3) {
                *channel = ((u32::from(*channel) * 255 + a / 2) / a).min(255) as u8;
            }
        }
    }
}

/// Whether a host owns every file of this process (KHP A29, section 10.6; `kition_kit::host::hosting`,
/// which kition-headless sets before it opens or creates anything). Then the machine's font
/// folders are never read either ([`kition_slide_fonts::no_system_fonts`]): what the deck is drawn
/// with and what the kit answers depend on the kit's own fonts, never on this machine's. Asked
/// before a document is read, as reading one lays its text out.
fn served_by_host() -> bool {
    let files_only = kition_kit::host::hosting().files_only;
    if files_only {
        kition_slide_fonts::no_system_fonts();
    }
    files_only
}

/// The commands whose file parameters a headless kit may offer agents (`kition_headless`'s
/// `sandboxed`): they read the picture or media file they name through the host
/// ([`lent_file`]), a path in the workspace, and write nothing.
pub const SANDBOXED: &[&str] = &["image.insert", "image.change", "audio.insert", "video.insert"];

/// The largest picture or media file a command reads through the host, in bytes (1 GiB).
const MAX_LENT: u64 = 1 << 30;

/// A picture or media file a command names by path, as the host lends it (`kition_kit::host`): a
/// path in the workspace, relative to the deck's folder; the host refuses any other.
fn lent_file(path: &str) -> Result<Vec<u8>, String> {
    let size = kition_kit::host::size(path)?;
    if size > MAX_LENT {
        return Err(format!("{path} is larger than {} MiB, the most a picture or media file may be", MAX_LENT >> 20));
    }
    kition_kit::host::read(path)
}

impl KitDocument for SlideDocument {
    fn batch_settler(&self) -> Option<kition_kit::BatchSettler> {
        let state = self.state().ok()?;
        let floor = state.doc.next_id;
        let media = Arc::new(Mutex::new(self.current_batch_media()));
        self.batch_media.replace(Arc::downgrade(&media));
        let planning = self.premise_plan;
        Some(Box::new(move |tx| {
            let media = media.lock().map_err(|error| format!("batch media store unavailable: {error}"))?;
            let settle = if planning { kdm_model::settle_premise_batch } else { kdm_model::settle_batch };
            settle(tx, floor, &media.blobs, &media.missing, &media.existing).map_err(|error| error.to_string())
        }))
    }

    fn fork_for_premises(&self) -> Result<Self, String> {
        self.premise_fork()
    }
    /// Opens slides (`.kslide`) or a PowerPoint file.
    fn open(name: &str, bytes: &[u8]) -> Result<Self, String> {
        served_by_host();
        let pres = cmd::file::open_slide_file(name, bytes).map_err(engine_error)?;
        let mut session = Session::new();
        cmd::file::add_opened(&mut session, name, None, pres);
        Ok(Self::from_session(session))
    }

    /// New slides with a title slide, 16:9.
    fn create(name: &str, _width: u32, _height: u32) -> Result<Self, String> {
        served_by_host();
        let mut session = Session::with_new();
        let state = session.doc_mut().map_err(engine_error)?;
        // The new deck as its file will store it (C8-slide-8): what a format writes of a blank
        // deck (a master's colour map, saved fit results, view properties) is there before the
        // first save, so the first save changes nothing an agent was told.
        let format = kition_slide_engine::cmd::file::format_for_path(name);
        if matches!(format, "pptx" | "kslide") {
            let bytes = kition_slide_engine::cmd::file::save_bytes(&state.doc, format).map_err(engine_error)?;
            state.doc = kition_slide_engine::cmd::file::open_slide_file(name, &bytes).map_err(engine_error)?.into();
        }
        state.saved_doc = state.doc.clone();
        Ok(Self::from_session(session))
    }

    fn width(&self) -> u32 {
        (self.slide_size().0 + 2.0 * MARGIN).round() as u32
    }

    fn height(&self) -> u32 {
        let (_, h) = self.slide_size();
        let slides = if self.single { 1.0 } else { self.slide_count().max(1) as f64 };
        (2.0 * MARGIN + slides * h + (slides - 1.0) * GAP).round() as u32
    }

    /// The deck, not the canvas: `width` and `height` add the margins and stack every slide.
    fn summary(&self) -> String {
        let (w, h) = self.slide_size();
        let slides = self.slide_count();
        format!("{slides} slide{}, {} × {} pt", if slides == 1 { "" } else { "s" }, w.round(), h.round())
    }

    fn revision(&self) -> u64 {
        self.session.active().map_or(0, |state| state.revision)
    }

    /// Never for a person who may only comment: their comments travel through the replica, and
    /// the file is not theirs to save.
    fn dirty(&self) -> bool {
        self.session.comment_only().is_none() && self.session.active().is_some_and(DocState::is_dirty)
    }

    /// Vectors: drawn at the zoom at 1:1 and above, so text and shapes stay sharp; below, the
    /// exact reduction of 1:1.
    fn render(&self, x: f64, y: f64, scale: f64, width: u32, height: u32) -> Result<Vec<u8>, String> {
        let (width, height) = (width.clamp(1, MAX_RENDER_SIDE), height.clamp(1, MAX_RENDER_SIDE));
        let scale = if scale.is_finite() && scale > 0.0 { scale } else { 1.0 };
        if scale < 1.0 {
            let (x0, y0) = (x.floor() as i32, y.floor() as i32);
            let span_x = ((f64::from(width) / scale).ceil() as i32).max(1);
            let span_y = ((f64::from(height) / scale).ceil() as i32).max(1);
            return self.render_area(x0, y0, x0.saturating_add(span_x), y0.saturating_add(span_y), width, height);
        }
        if scale == 1.0 && x.fract() == 0.0 && y.fract() == 0.0 {
            return Ok(self.full(x as i32, y as i32, width, height));
        }
        let mut out = self.compose(x, y, scale, width, height);
        straight(&mut out);
        Ok(out)
    }

    fn render_area(&self, x0: i32, y0: i32, x1: i32, y1: i32, width: u32, height: u32) -> Result<Vec<u8>, String> {
        let w = u32::try_from(x1.saturating_sub(x0).max(1)).unwrap_or(1).min(MAX_RENDER_SIDE);
        let h = u32::try_from(y1.saturating_sub(y0).max(1)).unwrap_or(1).min(MAX_RENDER_SIDE);
        let full = self.full(x0, y0, w, h);
        Ok(raster::render_area(Frame { rgba: &full, width: w, height: h }, 0, 0, w as i32, h as i32, width, height))
    }

    fn execute(&mut self, command: &str, params: Value) -> Result<Value, String> {
        self.refuse_unknown(command, &params)?;
        let result = self.run_named(command, params, |session, params| session.execute(command, params));
        self.take_requests();
        result
    }

    /// A host owns every file (engine contract E22, KHP A29): the engine writes no file by path
    /// and opens no deck by path (`kition_slide_engine::Session::set_host_files_only`), reads the
    /// pictures and media a command names by path only through the host ([`lent_file`]), keeps no
    /// AutoRecover copies, and reads no font folder of the machine.
    fn hosted(&mut self, hosting: &kition_kit::host::Hosting) {
        if hosting.files_only {
            kition_slide_fonts::no_system_fonts();
        }
        self.session.set_host_files_only(hosting.files_only);
        self.session.set_host_reader(Some(lent_file));
    }

    /// The current slide, the selection (shapes, slides, text being edited, table cells), Slide
    /// Master view, and the tool, as an opaque snapshot ([`kition_slide_engine::EditorState`]).
    fn editor_state(&self) -> Value {
        self.session.editor_state().and_then(|state| serde_json::to_value(state).ok()).unwrap_or(Value::Null)
    }

    /// Puts back what [`KitDocument::editor_state`] answered; no edit.
    fn restore_editor_state(&mut self, saved: &Value) -> Result<(), String> {
        if saved.is_null() {
            return Ok(());
        }
        let state: kition_slide_engine::EditorState =
            serde_json::from_value(saved.clone()).map_err(|error| format!("this is not the editor state this kit answered: {error}"))?;
        self.session.restore_editor_state(&state).map_err(engine_error)
    }

    /// Takes the file as read again (`fresh`) into this session, keeping its undo history and
    /// editor state ([`kition_slide_engine::Session::reread_file`]).
    fn reconcile_reread(&mut self, fresh: &Self) -> Result<bool, String> {
        let Some(state) = fresh.session.active() else { return Ok(false) };
        self.session.reread_file(&state.doc, &state.selection).map_err(engine_error)
    }

    /// Sets the slide a call acts in (`part`: `{slide: n}` counted from 1, or `{slideId}` as
    /// slide_inspect lists it), then the shapes it acts on there as the selection (`on`, shape ids
    /// as slide_inspect lists them; they need the slide). Slides have no time. A slide or shape the
    /// deck lacks is refused, saying where valid ones come from, and nothing changes. No edit.
    fn set_context(&mut self, context: &CallContext) -> Result<(), String> {
        if context.time.is_some() {
            return Err("slides have no timeline: setting the current time (`at.time`) is not available in this kit".into());
        }
        let slide = context.part.as_ref().map(call_slide).transpose()?;
        self.session.set_call_context(slide, &context.on).map_err(engine_error)
    }

    fn derived(&self, ids: &[Value], names: &[String]) -> Result<Vec<kition_kit::DerivedValues>, String> {
        rests_slides::derived(self, ids, names)
    }

    /// What a command names by a slide's place (kition-sdk `KitDocument::under`, C8-slide-5): the
    /// slide at its `index` (or a move's `from`), by the tool id slide_inspect lists, and the window
    /// of the deck's `slides` list the place covers; for where a slide goes (a move's `to`, a new
    /// slide's `at`), the empty window there. Local shape numbers are mapped on that slide.
    /// `None` without an open deck. A slide the deck lacks is refused, as the command would.
    fn under(&self, command: &str, params: &Value) -> Result<Option<kition_kit::Under>, String> {
        let places = acts::places(command);
        let Some(deck) = self.pres() else { return Ok(None) };
        let count = deck.slides.len();
        let index = |name: &str| params.get(name).and_then(Value::as_u64).and_then(|n| usize::try_from(n).ok());
        let window =
            |from: usize, to: usize| kition_kit::Within { parent: json!("slide-file"), field: "slides".to_owned(), from, to, ..Default::default() };
        let mut found = kition_kit::Under::default();
        let addresses = self.addressed_shapes(command, params);
        if !addresses.is_empty() {
            found.covers = Some(kition_kit::Covers { fields: vec!["address".to_owned()] });
            for (slide, number) in &addresses {
                if let Some(shape) = deck.slides.iter().find(|s| s.id.0 == *slide).and_then(|s| s.shape(kition_slide_model::ShapeId(*number))) {
                    let id = cmd::inspect::shape_tool_id(shape);
                    found.objects.push(json!(id));
                    found.premises.push(kition_kit::Premise::on(id).positions(["address"]));
                }
            }
        }
        // Shape numbers are local to a slide, whereas the guard's ids are composite.
        // State that mapping even for a deleted shape: its old id is in the journal.
        let comment_slide = (command == "comment.add").then(|| index("slide")).flatten();
        let mut local = params.clone();
        let implied = self.session.take_tool_ids(&mut local).map_err(command_failure)?;
        let on = comment_slide.or(implied).or_else(|| index(ON_SLIDE).and_then(|n| n.checked_sub(1))).unwrap_or_else(|| self.current_slide());
        for key in ["id", "ids", "from", "to", "trigger"] {
            let is_slide = command.starts_with("slide.") && matches!(key, "id" | "ids");
            let is_shape = (!command.starts_with("slide.") && matches!(key, "id" | "ids"))
                || (matches!(command, "shape.connect" | "drawing.connect") && matches!(key, "from" | "to"))
                || (command == "drawing.reconnect" && key == "to")
                || (command == "animation.timing" && key == "trigger");
            if !is_slide && !is_shape {
                continue;
            }
            let Some(value) = params.get(key) else { continue };
            let values = value.as_array().map(Vec::as_slice).unwrap_or_else(|| std::slice::from_ref(value));
            for value in values {
                if is_shape {
                    continue;
                }
                let number = value.as_u64().or_else(|| value.as_str().and_then(|text| text.parse::<u64>().ok()));
                let Some(number) = number else { continue };
                let id = if is_slide {
                    format!("slide:{number}")
                } else if let Some(slide) = deck.slides.get(on) {
                    match u32::try_from(number).ok().and_then(|id| slide.shape(kition_slide_model::ShapeId(id))) {
                        Some(shape) => cmd::inspect::shape_tool_id(shape),
                        None => continue,
                    }
                } else {
                    continue;
                };
                let mut premise = kition_kit::Premise::on(id);
                if acts::position_independent(command) {
                    premise = premise.positions(Vec::<String>::new());
                }
                found.premises.push(premise);
            }
        }
        let slide_at = |at: usize, found: &mut kition_kit::Under| -> Result<(), String> {
            let slide =
                deck.slides.get(at).ok_or_else(|| params_failure(format!("{command}: there is no slide {at} (the deck has {count}, from 0)")))?;
            found.objects.push(json!(kition_slide_engine::cmd::inspect::slide_tool_id(slide.id)));
            found.within.push(window(at, at + 1));
            Ok(())
        };
        if comment_slide.is_some() || params.get(ON_SLIDE).is_some_and(|value| !value.is_null()) {
            slide_at(on, &mut found)?;
            // The slide locates a shape. Its other shapes and own content are not read.
            if let Some(slide) = deck.slides.get(on) {
                found.premises.push(kition_kit::Premise::on(format!("slide:{}", slide.id.0)).fields(Vec::<String>::new()));
            }
        }
        // Animations belong to the slide, not the shape whose id selects them. Without an
        // index, hold the list too: a colleague may have added another animation to that shape.
        if index("index").is_none()
            && matches!(
                command,
                "animation.set" | "animation.remove" | "animation.timing" | "animation.options" | "animation.painter" | "animation.painterApply"
            )
            && let Some(slide) = deck.slides.get(on)
        {
            found.premises.push(kition_kit::Premise::on(format!("slide:{}", slide.id.0)).fields(["animations"]));
        }
        rests_slides::add(self, command, params, &mut found)?;
        rests_shapes::add(self, command, params, &mut found)?;
        rests_other::add(self, command, params, &mut found)?;
        rests_text::add(self, command, params, &mut found)?;
        rests_drawing::add(self, command, params, &mut found)?;
        rests_tables::add(self, command, params, &mut found)?;
        // The deck-level commands' premises are restated whole last (rests_deck).
        if places.is_empty() {
            rests_deck::settle(self, command, params, &mut found)?;
            rests_media::add(self, command, params, &mut found)?;
            self.inherited_body_premises(&mut found);
            self.name_layout_premises(&mut found);
            return Ok(Some(found));
        }
        match command {
            "comment.reply"
            | "comment.delete"
            | "comment.resolve"
            | "master.renameLayout"
            | "master.deleteLayout"
            | "show.customShow"
            | "drawing.erase" => {}
            "section.add" => {
                let at = index("at").unwrap_or_else(|| self.current_slide()).min(count.saturating_sub(1));
                // A section starts at the boundary before this slide. A source-item window
                // also held its right neighbour and falsely refused deletion after the boundary.
                if deck.slides.get(at).is_none() {
                    return Err(params_failure(format!("{command}: no slide at {at}")));
                }
                found.within.push(window(at, at));
                if let Some(slide) = deck.slides.get(at) {
                    found.objects.push(json!(format!("slide:{}", slide.id.0)));
                    found.premises.push(kition_kit::Premise::on(format!("slide:{}", slide.id.0)).fields(Vec::<String>::new()));
                }
            }
            "section.move" | "section.rename" | "section.remove" => {
                // The section list; `rests_deck` restates the premises by section (its tool id
                // `section:<identity>`) when the call names one.
                found.premises.push(kition_kit::Premise::on("slide-file").fields(["sections"]));
            }
            "arrange.reorder" => {
                if let (Some(slide), Some(to)) = (deck.slides.get(on), index("index")) {
                    let number = local.get("id").and_then(Value::as_u64);
                    if let Some(from) = slide.shapes.iter().position(|shape| Some(u64::from(shape.id.0)) == number) {
                        let to = to.min(slide.shapes.len().saturating_sub(1));
                        let at = if to >= from { to.saturating_add(1) } else { to };
                        found.within.push(kition_kit::Within {
                            parent: json!(format!("slide:{}", slide.id.0)),
                            field: "shapes".into(),
                            from: at,
                            to: at,
                            ..Default::default()
                        });
                    }
                }
            }
            "table.insertRowAbove" | "table.insertRowBelow" | "table.insertColumnLeft" | "table.insertColumnRight" => {
                if let Some(slide) = deck.slides.get(on) {
                    let selection = self.state().ok().map(|state| &state.selection);
                    let explicit = local.get("id").and_then(Value::as_u64).and_then(|id| u32::try_from(id).ok()).map(kition_slide_model::ShapeId);
                    let selected = selection.and_then(|sel| {
                        sel.text
                            .as_ref()
                            .filter(|t| t.cell.is_some())
                            .map(|t| t.shape)
                            .or(sel.cells.map(|c| c.0))
                            .or_else(|| sel.shapes.first().copied())
                    });
                    if let Some(shape) = explicit.or(selected).and_then(|id| kition_slide_model::find_shape(&slide.shapes, id)) {
                        let row = command.contains("Row");
                        let cell = selection.and_then(|sel| {
                            sel.text
                                .as_ref()
                                .filter(|t| explicit.is_none_or(|id| t.shape == id))
                                .and_then(|t| t.cell)
                                .or_else(|| sel.cells.filter(|c| explicit.is_none_or(|id| c.0 == id)).map(|c| c.1))
                        });
                        let at = index(if row { "row" } else { "col" })
                            .or(cell.map(|cell| if row { cell.0 } else { cell.1 }))
                            .unwrap_or(0)
                            .saturating_add(usize::from(command.ends_with("Below") || command.ends_with("Right")));
                        let state = self.state()?;
                        let model = state.kdm().ok_or_else(|| "table insertion window needs an initialized model".to_owned())?.model();
                        let owner = model
                            .ids()
                            .slides
                            .object(slide.id)
                            .and_then(|owner| model.ids().shape(owner, shape.id))
                            .and_then(|object| model.ids().table(object))
                            .ok_or_else(|| "table insertion window has no owning table".to_owned())?;
                        found.within.push(kition_kit::Within {
                            parent: json!(owner.table.as_str()),
                            field: if row { "rows" } else { "cols" }.into(),
                            from: at,
                            to: at,
                            ..Default::default()
                        });
                    }
                }
            }
            "animation.timing" | "animation.options" | "animation.remove" | "animation.move" => {
                if let Some(at) = index("index") {
                    let slide = deck.slides.get(on).ok_or_else(|| params_failure(format!("{command}: no slide")))?;
                    if slide.animations.get(at).is_none() {
                        return Err(params_failure(format!("{command}: no such animation")));
                    }
                    let id = format!("slide:{}", slide.id.0);
                    found.objects.push(json!(id));
                    // A move reads no animation's content: only the two windows below (rests_deck).
                    let fields = if command == "animation.move" { Vec::new() } else { vec![format!("animations.{at}")] };
                    found.premises.push(kition_kit::Premise::on(id.as_str()).fields(fields));
                    found.within.push(kition_kit::Within {
                        parent: json!(id),
                        field: "animations".into(),
                        from: at,
                        to: at + 1,
                        ..Default::default()
                    });
                    if command == "animation.move"
                        && let Some(to) = index("to")
                    {
                        let to = to.min(slide.animations.len().saturating_sub(1));
                        let gap = if to >= at { to.saturating_add(1) } else { to };
                        found.within.push(kition_kit::Within {
                            parent: json!(id),
                            field: "animations".into(),
                            from: gap,
                            to: gap,
                            ..Default::default()
                        });
                    }
                }
            }
            "slide.move" => {
                let from = index("from").unwrap_or_else(|| self.current_slide());
                slide_at(from, &mut found)?;
                if let Some(to) = index("to") {
                    // `to` is the index after the slide left `from`: before the slide now at `to`
                    // (or `to + 1` past `from`).
                    let at = if to >= from { to.saturating_add(1) } else { to }.min(count);
                    found.within.push(window(at, at));
                }
            }
            "slide.new" => {
                if let Some(at) = index("at") {
                    let at = at.min(count);
                    found.within.push(window(at, at));
                }
            }
            "slide.delete" if params.get("ids").and_then(Value::as_array).is_some_and(|ids| !ids.is_empty()) => {}
            _ => {
                if let Some(at) = index("index") {
                    slide_at(at, &mut found)?;
                }
            }
        }
        rests_deck::settle(self, command, params, &mut found)?;
        rests_media::add(self, command, params, &mut found)?;
        self.inherited_body_premises(&mut found);
        self.name_layout_premises(&mut found);
        Ok(Some(found))
    }

    fn blob_source(&self) -> Option<&dyn kition_kit::session::BlobSource> {
        self.session.kdm().map(|session| session.blobs() as &dyn kition_kit::session::BlobSource)
    }

    fn covered_at(
        &self,
        command: &str,
        params: &Value,
        objects: &[kition_kit::CoveredAsk],
        history: &dyn kition_kit::HistoricalBlobs,
    ) -> Result<Option<kition_kit::CoveredAt>, String> {
        rests_media::covered(self, command, params, objects, history)
    }

    fn covered(&self, command: &str, params: &Value, objects: &[kition_kit::CoveredAsk]) -> Result<Option<kition_kit::CoveredAt>, String> {
        let addresses = self.addressed_shapes(command, params);
        if addresses.is_empty() {
            return Ok(None);
        }
        let current = self.under(command, params)?.unwrap_or_default();
        let covered = objects
            .iter()
            .map(|object| {
                let address = object.before.get("address").cloned().or_else(|| {
                    let identity = object.id.as_str()?.strip_prefix("shape:")?;
                    self.pres()?.slides.iter().find_map(|slide| {
                        let mut address = None;
                        kition_slide_model::walk(&slide.shapes, &mut |shape, _| {
                            if shape.identity == identity {
                                address = Some(json!({"slide": slide.id.0, "number": shape.id.0}));
                            }
                        });
                        address
                    })
                });
                let was = if object.created {
                    Some(false)
                } else if object.id.as_str().is_some_and(|id| !id.starts_with("shape:")) && !object.before.contains_key("address") {
                    // Every stored slide shape has a shape: tool id and an address. Private
                    // Paragraph/TextBody history has neither, including after deletion.
                    Some(false)
                } else if object.unknown.iter().any(|field| field == "address" || field.starts_with("address.")) {
                    None
                } else {
                    address.map(|address| addresses.iter().any(|(slide, number)| address["slide"] == *slide && address["number"] == *number))
                };
                kition_kit::Covered { id: object.id.clone(), covered: was, ..Default::default() }
            })
            .collect::<Vec<_>>();
        // What the addresses named at `basis`: what they name now, less the objects asked about
        // that were elsewhere then, plus those that were there then.
        let mut then: Vec<Value> =
            current.objects.into_iter().filter(|id| !covered.iter().any(|asked| asked.id == *id && asked.covered == Some(false))).collect();
        for asked in &covered {
            if asked.covered == Some(true) && !then.contains(&asked.id) {
                then.push(asked.id.clone());
            }
        }
        Ok(Some(kition_kit::CoveredAt { under: then, covered }))
    }

    /// The slides (with where each lies on the canvas), the current slide, the selection, and
    /// the history.
    /// What is wrong with the slide (KHP A21): see [`check`].
    fn check(&self) -> Vec<kition_kit::diagnostics::Diagnostic> {
        self.state().map(|st| check::check(&st.doc)).unwrap_or_default()
    }

    fn inspect(&self) -> Result<Value, String> {
        let st = self.state()?;
        let (w, h) = self.slide_size();
        let shown = self.shown();
        let slides: Vec<Value> = (0..st.doc.slides.len())
            .map(|index| {
                let area = self.area(index);
                // In the single-slide view only the current slide is on the canvas.
                let area = shown.contains(&index).then(|| json!({ "x": area.x, "y": area.y, "width": area.w, "height": area.h }));
                json!({ "index": index, "area": area })
            })
            .collect();
        Ok(json!({
            "title": st.title(),
            "slideSize": { "width": w, "height": h },
            "slides": slides,
            "slide": st.selection.slide,
            "selection": st.selection.shapes.len(),
            "selectedKinds": st.selected_shapes().iter().map(|shape| shape.kind_name()).collect::<Vec<_>>(),
            "target": st.selection.target,
            "masters": st.doc.masters.iter().enumerate().map(|(index, master)| json!({
                "index": index,
                "name": master.name,
                "layouts": master.layouts.iter().enumerate().map(|(index, layout)| json!({ "index": index, "name": layout.name })).collect::<Vec<_>>()
            })).collect::<Vec<_>>(),
            "editingText": st.selection.text.is_some(),
            "canUndo": !st.history.undo.is_empty(),
            "canRedo": !st.history.redo.is_empty(),
        }))
    }

    fn commands(&self) -> Vec<KitCommand> {
        let mut commands = cmd::command_specs()
            .iter()
            .map(|spec| {
                let mut command = KitCommand {
                    id: spec.id.to_owned(),
                    title: spec.label.trim_end_matches('…').to_owned(),
                    menu: (!spec.menu.is_empty()).then(|| spec.menu.iter().map(|part| (*part).to_owned()).collect()),
                    shortcut: spec.shortcut.map(str::to_owned),
                    // Formatting commands also take named shapes (`ids`, selected first).
                    params: Some(on_slide_param(
                        spec.id,
                        &if kition_slide_engine::takes_named_selection(spec.id) && !spec.params.contains("ids") {
                            match spec.params.strip_prefix('{').map(str::trim_start) {
                                Some(rest) if rest.starts_with('}') => format!("{{ids?: [shape id]{rest}"),
                                Some(rest) => format!("{{ids?: [shape id], {rest}"),
                                None => spec.params.to_owned(),
                            }
                        } else {
                            spec.params.to_owned()
                        },
                    )),
                    mutates: changes_document(spec),
                    enabled: (spec.enabled)(&self.session).is_ok(),
                    description: described::describe(spec.id).into(),
                    output_schema: described::output(spec.id),
                    reads: acts::reads(spec.id),
                    ..Default::default()
                }
                .with_kit_schema();
                if let Some(schema) = command.schema.as_mut() {
                    described::type_keys(&command.id, schema);
                }
                command.targets = acts::targets(&command.id, command.schema.as_ref());
                command.places = acts::places(&command.id);
                command.position_independent = acts::position_independent(&command.id);
                acts::mark_objects(&mut command);
                command
            })
            .collect::<Vec<_>>();
        kition_kit::files::declare(&mut commands, files::FILES);
        commands
    }

    fn disabled_commands(&self) -> Vec<String> {
        cmd::command_specs().iter().filter(|spec| (spec.enabled)(&self.session).is_err()).map(|spec| spec.id.to_owned()).collect()
    }

    /// The slides as `.kslide` (the engine's native format), or as PowerPoint when `name` ends
    /// in `.pptx`.
    fn save(&self, name: Option<&str>) -> Result<Vec<u8>, String> {
        if self.premise_plan {
            return Err("a premise planning copy cannot be saved".into());
        }
        let st = self.state()?;
        let extension = name
            .and_then(|name| std::path::Path::new(name).extension())
            .map(|extension| extension.to_string_lossy().to_ascii_lowercase())
            .unwrap_or_default();
        // `kslide` is the engine's id for its native format, which `.kslide` files hold.
        let format = if extension == "pptx" { "pptx" } else { "kslide" };
        cmd::file::save_bytes(&st.doc, format).map_err(engine_error)
    }

    fn mark_saved(&mut self) {
        if let Ok(state) = self.session.doc_mut() {
            state.saved_doc = state.doc.clone();
        }
    }
}

impl KitTools for SlideDocument {
    fn tools(&self) -> Vec<ToolInfo> {
        TOOLS
            .iter()
            .map(|(id, label, shortcut, icon, group)| ToolInfo {
                id: (*id).to_owned(),
                label: (*label).to_owned(),
                shortcut: shortcut.map(str::to_owned),
                icon: (*icon).to_owned(),
                group: *group,
            })
            .collect()
    }

    fn tool(&self) -> String {
        tool_id(&self.session.tool.kind)
    }

    fn select_tool(&mut self, id: &str, zoom: f64) -> Result<ToolResponse, String> {
        self.zoom = zoom;
        *self.reported.borrow_mut() = id.to_owned();
        if self.tool() != id {
            self.session.set_tool(tool_kind(id).ok_or_else(|| format!("no tool {id}"))?);
        }
        Ok(self.respond())
    }

    fn pointer(&mut self, event: &ToolPointer) -> Result<ToolResponse, String> {
        self.refuse_comment_only("pointer")?;
        self.zoom = if event.zoom.is_finite() && event.zoom > 0.0 { event.zoom } else { 1.0 };
        let kind = match event.action {
            PointerAction::Move => PointerKind::Move,
            PointerAction::Down => PointerKind::Down,
            PointerAction::Drag => PointerKind::Drag,
            PointerAction::Up => PointerKind::Up,
            PointerAction::DoubleClick => PointerKind::DoubleClick,
        };
        // A press on another slide makes it the current one, as clicking its thumbnail does.
        let slide = match kind {
            PointerKind::Down | PointerKind::DoubleClick if self.gesture_slide.is_none() && !self.session.tool.drawing_freeform() => {
                let target = self.slide_at(event.y);
                if target != self.current_slide() {
                    self.session
                        .execute("slide.go", &json!({ "index": target }))
                        .or_else(|_| {
                            self.session
                                .select(|_, selection| {
                                    selection.slide = target;
                                    selection.shapes.clear();
                                    selection.text = None;
                                })
                                .map(|()| Value::Null)
                        })
                        .map_err(engine_error)?;
                }
                self.gesture_slide = Some(target);
                target
            }
            _ => self.gesture_slide.unwrap_or_else(|| self.current_slide()),
        };
        if kind == PointerKind::Move && slide != self.current_slide() {
            self.pointer_at = None;
            return Ok(self.respond());
        }
        let area = self.area(slide);
        let ev = PointerEvent { kind, x: event.x - area.x, y: event.y - area.y, mods: Self::mods(event.mods), tol: HIT_PX / self.zoom };
        self.pointer_at = Some(Point::new(ev.x, ev.y));
        let result = self.session.pointer(ev);
        if matches!(kind, PointerKind::Up | PointerKind::DoubleClick) {
            self.gesture_slide = None;
        }
        result.map_err(engine_error)?;
        Ok(self.respond())
    }

    fn key(&mut self, key: &str, mods: ToolMods, zoom: f64) -> Result<ToolResponse, String> {
        self.refuse_comment_only("key")?;
        self.zoom = zoom;
        self.session.key(key, Self::mods(mods)).map_err(engine_error)?;
        Ok(self.respond())
    }

    fn text(&mut self, text: &str, zoom: f64) -> Result<ToolResponse, String> {
        self.refuse_comment_only("text")?;
        self.zoom = zoom;
        self.session.type_text(text).map_err(engine_error)?;
        Ok(self.respond())
    }

    fn state(&mut self, _x: f64, _y: f64, _mods: ToolMods, zoom: f64) -> ToolResponse {
        self.zoom = zoom;
        self.respond()
    }
}

kition_kit::export_kit_document!(SlideDocument as "SlideDocument");
kition_kit::export_kit_tools!(SlideDocument as "SlideDocument");

/// Adds a font file's faces (TTF, OTF, TTC) for text the embedded faces do not cover, such as
/// simplified Chinese beyond the bundled Japanese face in a web build, which has no system fonts
/// (standard E9). Returns the faces added.
pub fn add_font(bytes: &[u8]) -> usize {
    kition_slide_fonts::FontDb::global().add_font(bytes.to_vec())
}

/// Slide's own additions to the exported class, for its embedded editor.
#[cfg(target_arch = "wasm32")]
pub mod web {
    use kition_kit::wasm_bindgen::prelude::*;
    use serde_json::json;

    use crate::__kition_kit_export::WebDocument;

    /// Adds a font file's faces for text the embedded faces do not cover; returns how many.
    #[wasm_bindgen(js_name = addFont, wasm_bindgen = kition_kit::wasm_bindgen)]
    pub fn add_font(bytes: &[u8]) -> u32 {
        crate::add_font(bytes) as u32
    }

    fn error(message: String) -> JsError {
        JsError::new(&message)
    }

    /// An origin in the KDM's commit form, the JSON array `[kind, principal, label]`.
    fn origin_of(origin: &str) -> Result<kition_slide_engine::Origin, JsError> {
        kition_slide_engine::Origin::decode(origin).ok_or_else(|| error(format!("origin is not [kind, principal, label]: {origin}")))
    }

    #[wasm_bindgen(js_class = "SlideDocument", wasm_bindgen = kition_kit::wasm_bindgen)]
    impl WebDocument {
        #[wasm_bindgen(js_name = renderTheme)]
        pub fn render_theme(&self, name: &str, index: u32, width: u32) -> Result<Vec<u8>, JsError> {
            self.0.render_theme(name, index as usize, width).map_err(error)
        }

        #[wasm_bindgen(js_name = mediaData)]
        pub fn media_data(&self, id: u32) -> Result<Vec<u8>, JsError> {
            self.0.media_data(id).map_err(error)
        }

        /// Lets the person only comment, as `me` (their principal id): see
        /// `SlideDocument::set_comment_only`.
        #[wasm_bindgen(js_name = setCommentOnly)]
        pub fn set_comment_only(&mut self, me: &str) {
            self.0.set_comment_only(me);
        }

        /// Only the current slide on the canvas (`true`), or all the slides stacked.
        #[wasm_bindgen(js_name = setSingle)]
        pub fn set_single(&mut self, single: bool) {
            self.0.set_single(single);
        }

        /// The canvas around the slides in `#rrggbb` (the editor's canvas color).
        #[wasm_bindgen(js_name = setBackdrop)]
        pub fn set_backdrop(&mut self, color: &str) -> Result<(), JsError> {
            self.0.set_backdrop(color).map_err(error)
        }

        /// RGBA8 of one slide at `width` pixels wide, for thumbnails.
        #[wasm_bindgen(js_name = renderSlide)]
        pub fn render_slide(&self, index: u32, width: u32) -> Result<Vec<u8>, JsError> {
            self.0.render_slide(index as usize, width).map_err(error)
        }

        /// Straight RGBA8 of one slide as the slide show draws it, at `width` x `height` pixels,
        /// with date fields showing `year`-`month`-`day` (`year` 0: today), for measurement.
        #[wasm_bindgen(js_name = renderSlideClean)]
        pub fn render_slide_clean(&self, index: u32, width: u32, height: u32, year: i32, month: u32, day: u32) -> Result<Vec<u8>, JsError> {
            let date = (year != 0).then(|| kition_slide_text::datetime::DateTime::new(year, month, day, 0, 0, 0));
            self.0.render_slide_clean(index as usize, width, height, date).map_err(error)
        }

        // Kition phase 3d, Task 3: the collaboration members (engine contract 2). Shapes mirror
        // `kition-sdk`'s `session::web` (`export_kit_session!` does not apply here; see
        // `crate::collab`).

        pub fn version(&mut self) -> Result<Vec<u8>, JsError> {
            self.0.version().map_err(error)
        }

        #[wasm_bindgen(js_name = exportChanges)]
        pub fn export_changes(&mut self, since: &[u8]) -> Result<Vec<u8>, JsError> {
            self.0.export_changes(since).map_err(error)
        }

        #[wasm_bindgen(js_name = importChanges)]
        pub fn import_changes(&mut self, bytes: &[u8]) -> Result<String, JsError> {
            self.0.import_changes(bytes).map(|damage| serde_json::to_string(&damage).unwrap_or_else(|_| "null".to_owned())).map_err(error)
        }

        pub fn snapshot(&mut self) -> Result<Vec<u8>, JsError> {
            self.0.snapshot().map_err(error)
        }

        pub fn undo(&mut self) -> Result<String, JsError> {
            self.0.undo().map(|(done, damage)| json!({ "done": done, "damage": damage }).to_string()).map_err(error)
        }

        pub fn redo(&mut self) -> Result<String, JsError> {
            self.0.redo().map(|(done, damage)| json!({ "done": done, "damage": damage }).to_string()).map_err(error)
        }

        #[wasm_bindgen(js_name = canUndo)]
        pub fn can_undo(&self) -> bool {
            self.0.can_undo()
        }

        #[wasm_bindgen(js_name = canRedo)]
        pub fn can_redo(&self) -> bool {
            self.0.can_redo()
        }

        /// `origin` is the KDM's commit form, the JSON array `[kind, principal, label]`.
        #[wasm_bindgen(js_name = executeAs)]
        pub fn execute_as(&mut self, command: &str, params: &str, origin: &str) -> Result<String, JsError> {
            let params = kition_kit::parse_params(params).map_err(error)?;
            let origin =
                kition_slide_engine::Origin::decode(origin).ok_or_else(|| error(format!("origin is not [kind, principal, label]: {origin}")))?;
            self.0.execute_as(command, params, &origin).map(|value| value.to_string()).map_err(error)
        }

        /// Every blob the replica references or can still bring back, for a peer's `addBlobs`.
        pub fn blobs(&mut self) -> Result<Vec<u8>, JsError> {
            self.0.blobs().map_err(error)
        }

        /// Adds a peer's blobs (from their `blobs()`): kept until the next `importChanges`.
        #[wasm_bindgen(js_name = addBlobs)]
        pub fn add_blobs(&mut self, bytes: &[u8]) -> Result<(), JsError> {
            self.0.add_blobs(bytes).map_err(error)
        }

        /// The blobs newly needed since `since` (empty bytes: everything the current state
        /// references), encoded like `blobs()`: for live sync, so a change ships only the blobs
        /// it needs.
        #[wasm_bindgen(js_name = blobsSince)]
        pub fn blobs_since(&mut self, since: &[u8]) -> Result<Vec<u8>, JsError> {
            self.0.blobs_since(since).map_err(error)
        }

        #[wasm_bindgen(js_name = kdmResets)]
        pub fn kdm_resets(&self) -> u32 {
            self.0.kdm_resets()
        }

        /// `model/describe` (the object model, `KitObjectModel 1.0`) as JSON text, through the
        /// change exchange `kition-slide` serves it with headless ([`crate::collab`]). A failure's
        /// message is the error's JSON (`{"code", "index", "message"}`).
        #[wasm_bindgen(js_name = modelDescribe)]
        pub fn model_describe(&mut self) -> Result<String, JsError> {
            kition_kit::session::model::Exchange::new()
                .describe(&mut self.0)
                .map(|answer| answer.to_string())
                .map_err(|failure| error(failure.to_json().to_string()))
        }

        /// `model/batch`: the request and the answer as JSON text, one undo step; a failure's
        /// message is the error's JSON (`{"code", "index", "message"}`).
        #[wasm_bindgen(js_name = modelBatch)]
        pub fn model_batch(&mut self, request: &str) -> Result<String, JsError> {
            let request: serde_json::Value = serde_json::from_str(request).map_err(|failure| {
                error(json!({ "code": "invalid-op", "index": null, "message": format!("the batch is not JSON: {failure}") }).to_string())
            })?;
            kition_kit::session::model::Exchange::new()
                .batch(&mut self.0, &request, false)
                .map(|answer| answer.to_string())
                .map_err(|failure| error(failure.to_json().to_string()))
        }

        /// A draft's changes taken in as one undo step, recorded with `origin` (`[kind, principal,
        /// label]`): `{"damage", "summary"}`. A refusal fails with the KDM's message.
        #[wasm_bindgen(js_name = mergeChanges)]
        pub fn merge_changes(&mut self, changes: &[u8], origin: &str) -> Result<String, JsError> {
            let origin = origin_of(origin)?;
            self.0.merge_changes(changes, &origin).map(|answer| answer.to_string()).map_err(error)
        }

        /// What merging a draft's changes would change now, as `StateDiff` JSON; changes nothing.
        #[wasm_bindgen(js_name = compareChanges)]
        pub fn compare_changes(&self, changes: &[u8]) -> Result<String, JsError> {
            self.0.compare_changes(changes).map(|answer| answer.to_string()).map_err(error)
        }

        /// Writes the state a snapshot holds over the slide as one change of `origin`; answers
        /// the damage JSON.
        #[wasm_bindgen(js_name = restoreState)]
        pub fn restore_state(&mut self, snapshot: &[u8], origin: &str) -> Result<String, JsError> {
            let origin = origin_of(origin)?;
            self.0.restore_state(snapshot, &origin).map(|damage| serde_json::to_string(&damage).unwrap_or_else(|_| "null".to_owned())).map_err(error)
        }

        /// An agent's reply (JSON text) with its object ids named as the tool interface names them.
        #[wasm_bindgen(js_name = modelReply)]
        pub fn model_reply(&self, value: &str) -> Result<String, JsError> {
            self.0.model_reply(value).map_err(error)
        }

        /// What changed since the version `since` (a receipt): `{"changes": StateDiff}`, or
        /// `{"changes": null, "unavailable"}` when the replica's log does not hold it.
        #[wasm_bindgen(js_name = changesSince)]
        pub fn changes_since(&self, since: &[u8]) -> String {
            self.0.changes_since(since).to_string()
        }

        /// Opens a file with the replica a host stored for it (the Kit UI protocol's `replica`
        /// capability); a replica that does not open is ignored and the file opens plainly.
        #[wasm_bindgen(js_name = openWithReplica)]
        pub fn open_with_replica(name: &str, bytes: &[u8], snapshot: &[u8], blobs: &[u8]) -> Result<WebDocument, JsError> {
            crate::SlideDocument::open_with_replica(name, bytes, snapshot, blobs).map(Self).map_err(error)
        }

        /// Whether the slide has made its replica yet (it makes one on the first shared
        /// edit).
        #[wasm_bindgen(getter, js_name = hasReplica)]
        pub fn has_replica(&self) -> bool {
            self.0.has_replica()
        }

        /// The replica's own document id, for the Kit UI protocol's `replica/info` message.
        /// `undefined` before the slide has made a replica yet.
        #[wasm_bindgen(getter, js_name = docId)]
        pub fn doc_id(&self) -> Option<String> {
            self.0.doc_id()
        }
    }
}

#[cfg(test)]
mod table_tests;
/// A call's part as the slide it names: `{slide: n}` counted from 1, or `{slideId}`.
fn call_slide(part: &Value) -> Result<CallSlide, String> {
    let fields = part.as_object().filter(|fields| fields.len() == 1);
    match fields.and_then(|fields| fields.iter().next()) {
        Some((key, value)) if key == "slide" => {
            value.as_u64().map(CallSlide::Number).ok_or_else(|| format!("`part.slide` is {value}: a slide number, counted from 1"))
        }
        Some((key, value)) if key == "slideId" => {
            value.as_u64().map(CallSlide::Id).ok_or_else(|| format!("`part.slideId` is {value}: a slide's id, as slide_inspect lists it"))
        }
        _ => Err(format!("`part` is {part}: this kit's parts are slides, `{{slide: n}}` counted from 1 or `{{slideId}}` as slide_inspect lists it")),
    }
}

/// The parameter naming the slide a command's `ids` are on ([`SlideDocument::run_named`]).
pub(crate) const ON_SLIDE: &str = "onSlide";

/// A command's parameter notation with `onSlide` added when it names shapes by `ids` or `id`
/// (slide, master and file commands address slides, layouts and files, not shapes).
fn on_slide_param(command: &str, params: &str) -> String {
    let names_shapes = params.contains("ids") || ["{id", ", id"].iter().any(|start| params.contains(start));
    let other_kind = ["slide.", "master.", "file."].iter().any(|group| command.starts_with(group));
    if !names_shapes || other_kind || params.contains(ON_SLIDE) {
        return params.to_owned();
    }
    // The parameters' own closing brace: a `→ {…}` result description may follow it.
    let mut depth = 0usize;
    let end = params.char_indices().find_map(|(at, c)| match c {
        '{' => {
            depth += 1;
            None
        }
        '}' if depth == 1 => Some(at),
        '}' => {
            depth = depth.saturating_sub(1);
            None
        }
        _ => None,
    });
    match end {
        Some(end) => format!("{}, {ON_SLIDE}?: int (the slide the ids are on, from 1){}", params[..end].trim_end(), &params[end..]),
        None => params.to_owned(),
    }
}

#[cfg(test)]
mod conformance;
#[cfg(test)]
mod tests;
