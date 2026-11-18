//! The Kition Slide document model.
//!
//! A [`SlideFile`] has slide masters (each with a theme and layouts) and slides. Each slide
//! uses a layout; placeholders on a slide inherit position, formatting and text styles from the
//! matching placeholder on its layout and master ([`resolve`]). Slides and masters sit behind
//! `Arc`s so an undo snapshot of the whole slide file costs one clone of the slide list.
#![deny(clippy::unwrap_used, clippy::expect_used, clippy::panic, clippy::unimplemented, clippy::todo, clippy::unreachable)]
#![forbid(unsafe_code)]

pub mod anim;
pub mod chart;
pub mod chart_spec;
pub mod defaults;
pub mod edit;
pub mod identity;
pub mod resolve;
pub mod style;
pub mod table;
pub mod text;
pub mod theme;

use std::sync::Arc;

pub use kition_slide_color::{ColorScheme, ColorTransform, Rgba, SchemeSlot};
pub use kition_slide_geom::{Rect, Size, Xfrm};
use serde::{Deserialize, Serialize};

pub use anim::{AnimClass, AnimStart, Animation, Transition};
pub use chart::Chart;
pub use style::{ColorBase, ColorRef, Effects, Fill, Line, ShapeStyle};
pub use table::{Cell, Table};
pub use text::{Paragraph, Run, RunProps, TextBody};
pub use theme::Theme;

pub(crate) fn yes() -> bool {
    true
}

macro_rules! id_type {
    ($name:ident) => {
        #[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
        #[serde(transparent)]
        pub struct $name(pub u32);
        impl std::fmt::Display for $name {
            fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                write!(f, "{}", self.0)
            }
        }
    };
}
id_type!(SlideId);
id_type!(ShapeId);
id_type!(LayoutId);
id_type!(MasterId);
id_type!(MediaId);

#[derive(Debug, thiserror::Error)]
pub enum ModelError {
    #[error("no slide {0}")]
    NoSlide(String),
    #[error("no shape {0}")]
    NoShape(ShapeId),
    #[error("no layout {0}")]
    NoLayout(LayoutId),
    #[error("{0}")]
    Invalid(String),
}

pub type Result<T> = std::result::Result<T, ModelError>;

/// Placeholder types.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum PhType {
    Title,
    CtrTitle,
    SubTitle,
    #[default]
    Body,
    Obj,
    Chart,
    Table,
    ClipArt,
    Diagram,
    Media,
    Picture,
    Date,
    Footer,
    SlideNum,
    Header,
    SlideImage,
}

impl PhType {
    pub fn xml(self) -> &'static str {
        match self {
            PhType::Title => "title",
            PhType::CtrTitle => "ctrTitle",
            PhType::SubTitle => "subTitle",
            PhType::Body => "body",
            PhType::Obj => "obj",
            PhType::Chart => "chart",
            PhType::Table => "tbl",
            PhType::ClipArt => "clipArt",
            PhType::Diagram => "dgm",
            PhType::Media => "media",
            PhType::Picture => "pic",
            PhType::Date => "dt",
            PhType::Footer => "ftr",
            PhType::SlideNum => "sldNum",
            PhType::Header => "hdr",
            PhType::SlideImage => "sldImg",
        }
    }
    pub fn from_xml(s: &str) -> Self {
        match s {
            "title" => PhType::Title,
            "ctrTitle" => PhType::CtrTitle,
            "subTitle" => PhType::SubTitle,
            "obj" => PhType::Obj,
            "chart" => PhType::Chart,
            "tbl" => PhType::Table,
            "clipArt" => PhType::ClipArt,
            "dgm" => PhType::Diagram,
            "media" => PhType::Media,
            "pic" => PhType::Picture,
            "dt" => PhType::Date,
            "ftr" => PhType::Footer,
            "sldNum" => PhType::SlideNum,
            "hdr" => PhType::Header,
            "sldImg" => PhType::SlideImage,
            _ => PhType::Body,
        }
    }
    pub fn is_title(self) -> bool {
        matches!(self, PhType::Title | PhType::CtrTitle)
    }
    /// The prompt shown in an empty placeholder in Normal view.
    pub fn prompt(self) -> &'static str {
        match self {
            PhType::Title | PhType::CtrTitle => "Click to add title",
            PhType::SubTitle => "Click to add subtitle",
            PhType::Body | PhType::Obj => "Click to add text",
            PhType::Chart => "Click icon to add chart",
            PhType::Table => "Click icon to add table",
            PhType::Diagram => "Click icon to add smart graphic",
            PhType::Media => "Click icon to add media",
            PhType::Picture | PhType::ClipArt => "Click icon to add picture",
            PhType::Date => "Date",
            PhType::Footer => "Footer",
            PhType::SlideNum => "‹#›",
            PhType::Header => "Header",
            PhType::SlideImage => "",
        }
    }
    /// Footer-row placeholders are not shown on slides unless the slide file turns them on.
    pub fn is_footer_kind(self) -> bool {
        matches!(self, PhType::Date | PhType::Footer | PhType::SlideNum | PhType::Header)
    }
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct Placeholder {
    pub kind: PhType,
    pub idx: u32,
    /// `vert` orientation flag.
    pub vertical: bool,
    /// `quarter`, `half`, `full` size hint.
    pub size: Option<String>,
    pub has_custom_prompt: bool,
}

/// Shape geometry: a preset name with adjust values, or a custom path.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", tag = "kind")]
pub enum Geom {
    Preset {
        name: String,
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        adj: Vec<f64>,
    },
    /// Custom geometry: sub-paths in their own coordinate space (w, h) scaled to the shape.
    Custom {
        paths: Vec<CustomPath>,
        /// The file's `a:custGeom` element (guides, adjust handles, connection sites, text box),
        /// written back as it was while `paths` are still what it evaluates to.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        source: Option<String>,
    },
}

impl Default for Geom {
    fn default() -> Self {
        Geom::Preset { name: "rect".into(), adj: vec![] }
    }
}

impl Geom {
    pub fn preset(name: &str) -> Self {
        Geom::Preset { name: name.into(), adj: vec![] }
    }
    pub fn preset_name(&self) -> Option<&str> {
        match self {
            Geom::Preset { name, .. } => Some(name),
            Geom::Custom { .. } => None,
        }
    }
}

/// One sub-path of a custom geometry: SVG-like commands in path units.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct CustomPath {
    pub w: f64,
    pub h: f64,
    /// `M x y`, `L x y`, `C x1 y1 x2 y2 x y`, `Q x1 y1 x y`, `Z`.
    pub d: String,
    pub fill: kition_slide_geom::preset::FillMode,
    pub stroke: bool,
}

impl Default for CustomPath {
    fn default() -> Self {
        CustomPath { w: 0.0, h: 0.0, d: String::new(), fill: kition_slide_geom::preset::FillMode::Norm, stroke: true }
    }
}

/// A relationship named by kept XML, and the part it targets.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct KeptRel {
    /// The id the kept XML uses for it.
    pub id: String,
    /// The full relationship type URI.
    pub rel_type: String,
    /// The target: an embedded part, or an external link.
    pub media: MediaId,
    /// The target part's own relationships, by the ids its bytes use (a diagram data part's
    /// pictures). Their targets have none of their own.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub rels: Vec<KeptRel>,
}

/// A smart graphic's diagram as its file had it, kept on the group it is drawn as: written back in
/// place of the group while the group's shapes are as read (see [`shapes_fingerprint`]).
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct KeptDiagram {
    /// The `p:graphicFrame`, its `dgm:relIds` naming `parts` by their ids.
    pub frame: String,
    /// The data, layout, quick style and colours parts, by the ids `frame` uses.
    pub parts: Vec<KeptRel>,
    /// The drawing part (the diagram's shapes as last laid out), named from the data part.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub drawing: Option<KeptRel>,
    /// The relationship id the data part names the drawing by (`dsp:dataModelExt/@relId`).
    pub drawing_ref: String,
    /// [`shapes_fingerprint`] of the group's shapes as read.
    pub fingerprint: u64,
}

/// A fingerprint of shapes' content, ids left out (ids are renumbered when a file is read
/// again). Media references use the image content, so local media numbering does not affect it.
/// Equal while nothing in the shapes or their images has been edited.
pub fn shapes_fingerprint(shapes: &[Shape], media: &[MediaItem]) -> u64 {
    fn hash(bytes: impl IntoIterator<Item = u8>) -> u64 {
        bytes.into_iter().fold(0xcbf2_9ce4_8422_2325_u64, |h, b| (h ^ u64::from(b)).wrapping_mul(0x0100_0000_01b3))
    }
    fn strip(v: &mut serde_json::Value, media: &[MediaItem], cache: &mut std::collections::HashMap<u64, String>, depth: usize) {
        if depth > 64 {
            return;
        }
        match v {
            serde_json::Value::Object(map) => {
                map.remove("id");
                map.remove("identity");
                for (key, c) in map.iter_mut() {
                    if matches!(key.as_str(), "media" | "poster" | "preview" | "sound")
                        && let Some(id) = c.as_u64()
                        && let Some(item) = media.iter().find(|item| u64::from(item.id.0) == id)
                    {
                        let content = cache.entry(id).or_insert_with(|| {
                            let bytes =
                                item.content_type.bytes().chain(item.link.as_deref().unwrap_or_default().bytes()).chain(item.data.iter().copied());
                            format!("media:{:016x}", hash(bytes))
                        });
                        *c = serde_json::Value::String(content.clone());
                    } else {
                        strip(c, media, cache, depth + 1);
                    }
                }
            }
            serde_json::Value::Array(items) => {
                for c in items {
                    strip(c, media, cache, depth + 1);
                }
            }
            _ => {}
        }
    }
    let mut v = serde_json::to_value(shapes).unwrap_or_default();
    strip(&mut v, media, &mut Default::default(), 0);
    // FNV-1a: stable across builds, as the fingerprint is stored in files.
    hash(v.to_string().bytes())
}

/// How a file describes a typeface where it names it in one slot (`latin`, `ea`, `cs`, `sym` or
/// `buFont`): its PANOSE number, character set and pitch and family, so a saved file names the same
/// face. Attributes are kept verbatim.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct FontFace {
    pub slot: String,
    pub typeface: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub panose: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub charset: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub pitch_family: Option<String>,
}

/// One customer data tag list (`p:tags`, a `ppt/tags/tag#.xml` part): name/value pairs in order.
pub type TagList = Vec<(String, String)>;

/// Embedded audio or video on a slide.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct MediaClip {
    pub media: MediaId,
    pub video: bool,
    /// Poster frame image.
    pub poster: Option<MediaId>,
    pub trim_start_ms: u32,
    pub trim_end_ms: u32,
    pub fade_in_ms: u32,
    pub fade_out_ms: u32,
    /// 0–1.
    pub volume: f64,
    pub autoplay: bool,
    pub loop_play: bool,
    pub rewind: bool,
    pub play_across_slides: bool,
    pub hide_while_not_playing: bool,
    pub full_screen: bool,
    /// Bookmarks (name, ms).
    pub bookmarks: Vec<(String, u32)>,
    /// Probed length in ms (0 = unknown).
    pub duration_ms: u32,
    /// Probed picture size of a video in pixels (0 = unknown).
    pub width: u32,
    pub height: u32,
}

/// One ink stroke (Draw tab): points in slide coordinates with pressure.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct InkStroke {
    pub points: Vec<(f64, f64, f32)>,
    pub color: Rgba,
    pub width: f64,
    pub highlighter: bool,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", tag = "type")]
pub enum ShapeKind {
    /// An autoshape or text box.
    Shape,
    Picture {
        fill: style::PictureFill,
    },
    Group {
        children: Vec<Shape>,
        /// Child coordinate space.
        child: Xfrm,
    },
    Connector {
        start: Option<(ShapeId, u32)>,
        end: Option<(ShapeId, u32)>,
    },
    Table(Table),
    Chart(Box<Chart>),
    Media(MediaClip),
    Ink {
        strokes: Vec<InkStroke>,
    },
    /// Something we read but don't model (a diagram without its drawing, OLE…): its XML is kept and its
    /// fallback picture, if any, is shown.
    Opaque {
        xml: String,
        preview: Option<MediaId>,
        /// Pixel corrections for the preserved object's preview image.
        #[serde(default, rename = "previewAdjust", skip_serializing_if = "Option::is_none")]
        preview_adjust: Option<style::PictureAdjust>,
        label: String,
        /// The relationships `xml` names (`r:id`, `r:embed`…), with the parts they target carried
        /// as media items (an OLE object's embedding, its preview picture, tag lists).
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        rels: Vec<KeptRel>,
    },
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct Shape {
    pub id: ShapeId,
    /// Persistent DrawingML creation GUID, independent of the part-local numeric id.
    #[serde(skip_serializing_if = "String::is_empty")]
    pub identity: String,
    pub name: String,
    /// `None`: placeholder inheriting its box from the layout/master.
    pub xfrm: Option<Xfrm>,
    pub kind: ShapeKind,
    pub geom: Geom,
    pub ph: Option<Placeholder>,
    /// `None`: inherited (placeholder chain or style); `Some(Fill::None)`: no fill.
    pub fill: Option<Fill>,
    pub line: Option<Line>,
    pub effects: Option<Effects>,
    pub style: Option<ShapeStyle>,
    pub text: Option<TextBody>,
    pub hidden: bool,
    pub locked: bool,
    /// Text box (`txBox="1"`): no fill, autosize, wrap off by default.
    pub text_box: bool,
    /// Alt text.
    pub descr: String,
    pub title: String,
    pub decorative: bool,
    pub click: Option<text::Hyperlink>,
    pub hover: Option<text::Hyperlink>,
    /// Customer data tag lists (`p:custDataLst/p:tags`), kept for add-ins that wrote them.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub tags: Vec<TagList>,
    /// Unmodelled XML kept for round-trip (extension lists etc.).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub raw_ext: Option<String>,
    /// The diagram (a smart graphic) a group was read from (see [`KeptDiagram`]).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub diagram: Option<Box<KeptDiagram>>,
    /// A smart graphic node's placeholder prompts, by paragraph (the data model's `phldrT`, "[Text]"):
    /// shown while editing in the paragraphs that are still empty.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub prompts: Vec<Option<String>>,
}

impl Default for Shape {
    fn default() -> Self {
        Shape {
            id: ShapeId(0),
            identity: String::new(),
            name: String::new(),
            xfrm: None,
            kind: ShapeKind::Shape,
            geom: Geom::default(),
            ph: None,
            fill: None,
            line: None,
            effects: None,
            style: None,
            text: None,
            hidden: false,
            locked: false,
            text_box: false,
            descr: String::new(),
            title: String::new(),
            decorative: false,
            click: None,
            hover: None,
            tags: vec![],
            raw_ext: None,
            diagram: None,
            prompts: vec![],
        }
    }
}

impl Shape {
    pub fn is_placeholder(&self) -> bool {
        self.ph.is_some()
    }
    pub fn ph_type(&self) -> Option<PhType> {
        self.ph.as_ref().map(|p| p.kind)
    }
    pub fn is_group(&self) -> bool {
        matches!(self.kind, ShapeKind::Group { .. })
    }
    pub fn children(&self) -> &[Shape] {
        match &self.kind {
            ShapeKind::Group { children, .. } => children,
            _ => &[],
        }
    }
    pub fn children_mut(&mut self) -> Option<&mut Vec<Shape>> {
        match &mut self.kind {
            ShapeKind::Group { children, .. } => Some(children),
            _ => None,
        }
    }
    /// Is this a line-like shape (no area)?
    pub fn is_line(&self) -> bool {
        matches!(self.kind, ShapeKind::Connector { .. }) || self.geom.preset_name().is_some_and(kition_slide_geom::preset::is_line_like)
    }
    pub fn kind_name(&self) -> &'static str {
        match &self.kind {
            ShapeKind::Shape if self.ph.is_some() => "placeholder",
            ShapeKind::Shape if self.text_box => "textBox",
            ShapeKind::Shape => "shape",
            ShapeKind::Picture { .. } => "picture",
            ShapeKind::Group { .. } => "group",
            ShapeKind::Connector { .. } => "connector",
            ShapeKind::Table(_) => "table",
            ShapeKind::Chart(_) => "chart",
            ShapeKind::Media(m) if m.video => "video",
            ShapeKind::Media(_) => "audio",
            ShapeKind::Ink { .. } => "ink",
            ShapeKind::Opaque { .. } => "object",
        }
    }
}

/// Walk shapes depth-first (groups included).
pub fn walk<'a>(shapes: &'a [Shape], f: &mut dyn FnMut(&'a Shape, usize)) {
    fn rec<'a>(shapes: &'a [Shape], depth: usize, f: &mut dyn FnMut(&'a Shape, usize)) {
        if depth > 64 {
            return;
        }
        for s in shapes {
            f(s, depth);
            rec(s.children(), depth + 1, f);
        }
    }
    rec(shapes, 0, f);
}

pub fn find_shape(shapes: &[Shape], id: ShapeId) -> Option<&Shape> {
    fn rec(shapes: &[Shape], id: ShapeId, depth: usize) -> Option<&Shape> {
        if depth > 64 {
            return None;
        }
        for s in shapes {
            if s.id == id {
                return Some(s);
            }
            if let Some(f) = rec(s.children(), id, depth + 1) {
                return Some(f);
            }
        }
        None
    }
    rec(shapes, id, 0)
}

pub fn find_shape_mut(shapes: &mut [Shape], id: ShapeId) -> Option<&mut Shape> {
    fn rec(shapes: &mut [Shape], id: ShapeId, depth: usize) -> Option<&mut Shape> {
        if depth > 64 {
            return None;
        }
        for s in shapes.iter_mut() {
            if s.id == id {
                return Some(s);
            }
            if let ShapeKind::Group { children, .. } = &mut s.kind
                && let Some(f) = rec(children, id, depth + 1)
            {
                return Some(f);
            }
        }
        None
    }
    rec(shapes, id, 0)
}

/// Background of a slide, layout or master.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", tag = "kind")]
pub enum Background {
    Fill {
        fill: Fill,
    },
    /// Theme background style reference (1001–1003) with a colour.
    Ref {
        idx: u32,
        color: ColorRef,
    },
}

/// A comment on a slide: a slide's top-level comment opens a thread (its `replies` follow it).
///
/// In a replica a thread is a `CommentThread` object beside the slide and each comment a
/// `Comment` object (Kition Kit Standard 2.0 rule M23): `id` is the thread's object id for a
/// top-level comment (`head` its own comment's) and the comment's object id for a reply. Empty or
/// repeated ids (a file read outside an editor, a copied slide) are replaced by
/// [`SlideFile::fill_comment_ids`].
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct Comment {
    /// The author's display name (a file's author name for comments read from a file).
    pub author: String,
    pub initials: String,
    pub text: String,
    /// ISO 8601.
    pub date: String,
    pub x: f64,
    pub y: f64,
    pub resolved: bool,
    pub replies: Vec<Comment>,
    /// The shape the comment is attached to.
    pub shape: Option<ShapeId>,
    #[serde(skip_serializing_if = "String::is_empty")]
    pub id: String,
    /// A top-level comment's own comment id (its thread's is `id`).
    #[serde(skip_serializing_if = "String::is_empty")]
    pub head: String,
    /// The author's principal id when it differs from `author` (a person commenting in Kition).
    #[serde(skip_serializing_if = "String::is_empty")]
    pub author_id: String,
    /// What a file format kept of the comment beyond the fields above.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub kept: Option<Box<CommentKept>>,
}

/// PowerPoint's own data about a comment, kept so a saved file says what the read one said.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct CommentKept {
    /// Read from a modern (threaded, `p188`) comment part: written back as one.
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    pub modern: bool,
    /// The modern comment's id (`{GUID}`).
    #[serde(skip_serializing_if = "String::is_empty")]
    pub guid: String,
    /// The modern author's id (`{GUID}`), user id and identity provider.
    #[serde(skip_serializing_if = "String::is_empty")]
    pub author_guid: String,
    #[serde(skip_serializing_if = "String::is_empty")]
    pub user_id: String,
    #[serde(skip_serializing_if = "String::is_empty")]
    pub provider_id: String,
    /// A modern comment's status when it is neither active nor resolved (`closed`).
    #[serde(skip_serializing_if = "String::is_empty")]
    pub status: String,
    /// A modern comment's anchor (`pc:sldMkLst`, `ac:deMkLst`, `ac:txMkLst`…) as read.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub anchor_xml: Option<String>,
    /// A modern comment's `p188:txBody` as read: written back while `text` still matches
    /// `body_text`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub body_xml: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub body_text: Option<String>,
    /// The comment's own `extLst` as read (a legacy comment's without its threading).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ext_xml: Option<String>,
    /// A legacy reply's time zone bias (`p15:threadingInfo/@timeZoneBias`).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tz: Option<i32>,
}

impl Comment {
    /// Whether this comment came from a modern comment part.
    pub fn is_modern(&self) -> bool {
        self.kept.as_ref().is_some_and(|k| k.modern)
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct Slide {
    pub id: SlideId,
    pub layout: LayoutId,
    pub name: String,
    pub shapes: Vec<Shape>,
    pub background: Option<Background>,
    pub hidden: bool,
    pub show_master_shapes: bool,
    pub transition: Option<Transition>,
    pub animations: Vec<Animation>,
    /// The timing tree the slide was read with, written back while `animations` are unchanged.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub timing: Option<crate::anim::KeptTiming>,
    /// Speaker notes.
    pub notes: TextBody,
    /// The slide's notes page as read when it held no notes (its slide image, date, number…
    /// placeholders as the file placed them): written back while `notes` stay empty.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub notes_page: Option<String>,
    pub comments: Vec<Comment>,
    /// Customer data tag lists (`p:custDataLst/p:tags`), kept for add-ins that wrote them.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub tags: Vec<TagList>,
    /// Unmodelled XML kept for round-trip (custom data, tags).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub raw_ext: Option<String>,
    /// The slide's id in the file it was read from (`p:sldId/@id`), written back so add-ins and
    /// links that name slides by it keep working. A copied slide carries it too; the writer gives
    /// every repeat a fresh id.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub file_id: Option<u32>,
}

impl Default for Slide {
    fn default() -> Self {
        Slide {
            id: SlideId(0),
            layout: LayoutId(0),
            name: String::new(),
            shapes: vec![],
            background: None,
            hidden: false,
            show_master_shapes: true,
            transition: None,
            animations: vec![],
            timing: None,
            notes: TextBody::default(),
            notes_page: None,
            comments: vec![],
            tags: vec![],
            raw_ext: None,
            file_id: None,
        }
    }
}

impl Slide {
    pub fn shape(&self, id: ShapeId) -> Option<&Shape> {
        find_shape(&self.shapes, id)
    }
    pub fn shape_mut(&mut self, id: ShapeId) -> Option<&mut Shape> {
        find_shape_mut(&mut self.shapes, id)
    }
    /// The slide's title text (first title placeholder), for thumbnails, outline and navigation.
    pub fn title(&self) -> String {
        self.shapes
            .iter()
            .find(|s| s.ph_type().is_some_and(PhType::is_title))
            .and_then(|s| s.text.as_ref())
            .map(|t| t.text().replace('\u{b}', " "))
            .unwrap_or_default()
    }
    pub fn notes_text(&self) -> String {
        self.notes.text()
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum LayoutType {
    Title,
    #[default]
    TitleAndContent,
    SectionHeader,
    TwoContent,
    Comparison,
    TitleOnly,
    Blank,
    ContentWithCaption,
    PictureWithCaption,
    TitleAndVerticalText,
    VerticalTitleAndText,
    Custom,
}

impl LayoutType {
    pub fn xml(self) -> &'static str {
        match self {
            LayoutType::Title => "title",
            LayoutType::TitleAndContent => "obj",
            LayoutType::SectionHeader => "secHead",
            LayoutType::TwoContent => "twoObj",
            LayoutType::Comparison => "twoTxTwoObj",
            LayoutType::TitleOnly => "titleOnly",
            LayoutType::Blank => "blank",
            LayoutType::ContentWithCaption => "objTx",
            LayoutType::PictureWithCaption => "picTx",
            LayoutType::TitleAndVerticalText => "vertTx",
            LayoutType::VerticalTitleAndText => "vertTitleAndTx",
            LayoutType::Custom => "cust",
        }
    }
    pub fn from_xml(s: &str) -> Self {
        match s {
            "title" => LayoutType::Title,
            "obj" | "tx" => LayoutType::TitleAndContent,
            "secHead" => LayoutType::SectionHeader,
            "twoObj" | "twoColTx" => LayoutType::TwoContent,
            "twoTxTwoObj" => LayoutType::Comparison,
            "titleOnly" => LayoutType::TitleOnly,
            "blank" => LayoutType::Blank,
            "objTx" => LayoutType::ContentWithCaption,
            "picTx" => LayoutType::PictureWithCaption,
            "vertTx" => LayoutType::TitleAndVerticalText,
            "vertTitleAndTx" => LayoutType::VerticalTitleAndText,
            _ => LayoutType::Custom,
        }
    }
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct Layout {
    pub id: LayoutId,
    /// Persistent layout identity, independent of its package part and local numeric id.
    #[serde(skip_serializing_if = "String::is_empty")]
    pub identity: String,
    pub name: String,
    pub kind: LayoutType,
    pub shapes: Vec<Shape>,
    pub background: Option<Background>,
    pub show_master_shapes: bool,
    pub preserve: bool,
    /// The layout was made or changed by a person (`userDrawn`): PowerPoint keeps it when the
    /// design is replaced. Kept for round trip.
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    pub user_drawn: bool,
    /// Customer data tag lists (`p:custDataLst/p:tags`), kept for add-ins that wrote them.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub tags: Vec<TagList>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub raw_ext: Option<String>,
    /// The layout's header and footer switches (`p:hf`), kept as written.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub hf: Option<String>,
}

/// Which placeholders slides show in the footer row (Header & Footer dialog).
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct HeaderFooter {
    pub date: bool,
    pub slide_number: bool,
    pub footer: bool,
    pub header: bool,
    pub footer_text: String,
    /// Fixed date text; empty = automatic date.
    pub date_text: String,
    /// `datetime1`…`datetime13` format for automatic dates.
    pub date_format: String,
    pub hide_on_title: bool,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct Master {
    pub id: MasterId,
    pub name: String,
    pub theme: Theme,
    pub shapes: Vec<Shape>,
    pub background: Option<Background>,
    pub title_style: text::ListStyle,
    pub body_style: text::ListStyle,
    pub other_style: text::ListStyle,
    pub layouts: Vec<Layout>,
    /// `bg1`→`lt1` etc. Default mapping when empty.
    pub color_map: Vec<(String, String)>,
    pub preserve: bool,
    /// Customer data tag lists (`p:custDataLst/p:tags`), kept for add-ins that wrote them.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub tags: Vec<TagList>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub raw_ext: Option<String>,
    /// The master's header and footer switches (`p:hf`), kept as written.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub hf: Option<String>,
}

impl Master {
    pub fn layout(&self, id: LayoutId) -> Option<&Layout> {
        self.layouts.iter().find(|l| l.id == id)
    }
    /// Resolve `bg1`/`tx1`/`bg2`/`tx2` through the colour map.
    pub fn map_slot(&self, slot: SchemeSlot) -> SchemeSlot {
        let key = match slot {
            SchemeSlot::Bg1 => "bg1",
            SchemeSlot::Tx1 => "tx1",
            SchemeSlot::Bg2 => "bg2",
            SchemeSlot::Tx2 => "tx2",
            other => return other,
        };
        self.color_map.iter().find(|(k, _)| k == key).and_then(|(_, v)| SchemeSlot::from_xml(v)).unwrap_or(match slot {
            SchemeSlot::Bg1 => SchemeSlot::Lt1,
            SchemeSlot::Tx1 => SchemeSlot::Dk1,
            SchemeSlot::Bg2 => SchemeSlot::Lt2,
            _ => SchemeSlot::Dk2,
        })
    }
    /// The theme's colour scheme with the colour map applied (`bg1`… resolve to mapped colours).
    pub fn scheme(&self) -> ColorScheme {
        let mut s = self.theme.colors.clone();
        if self.color_map.is_empty() {
            return s;
        }
        let base = self.theme.colors.clone();
        for (alias, slot) in [
            (SchemeSlot::Bg1, SchemeSlot::Lt1),
            (SchemeSlot::Tx1, SchemeSlot::Dk1),
            (SchemeSlot::Bg2, SchemeSlot::Lt2),
            (SchemeSlot::Tx2, SchemeSlot::Dk2),
        ] {
            s.set(slot, base.get(self.map_slot(alias)));
        }
        s
    }
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct Section {
    /// Persistent section GUID (the standard PresentationML section id).
    #[serde(skip_serializing_if = "String::is_empty")]
    pub identity: String,
    pub name: String,
    pub slides: Vec<SlideId>,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct CustomShow {
    pub name: String,
    pub slides: Vec<SlideId>,
}

/// Slide Show ▸ Set Up Slide Show.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct ShowSettings {
    /// `speaker`, `browsed`, `kiosk`.
    pub show_type: String,
    pub loop_until_esc: bool,
    pub without_narration: bool,
    pub without_animation: bool,
    pub use_timings: bool,
    /// 1-based inclusive range; `None` = all.
    pub range: Option<(u32, u32)>,
    pub custom_show: Option<String>,
    pub pen_color: Rgba,
    pub laser_color: Rgba,
    pub show_media_controls: bool,
}

impl Default for ShowSettings {
    fn default() -> Self {
        ShowSettings {
            show_type: "speaker".into(),
            loop_until_esc: false,
            without_narration: false,
            without_animation: false,
            use_timings: true,
            range: None,
            custom_show: None,
            pen_color: Rgba::rgb(0xFF, 0x00, 0x00),
            laser_color: Rgba::rgb(0xFF, 0x00, 0x00),
            show_media_controls: true,
        }
    }
}

/// Document properties (File ▸ Properties).
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct Properties {
    pub title: String,
    pub subject: String,
    pub author: String,
    pub keywords: String,
    pub comments: String,
    pub category: String,
    pub company: String,
    pub created: String,
    pub modified: String,
    pub last_modified_by: String,
    pub revision: u32,
    /// The custom document properties part (`docProps/custom.xml`) as the file had it, written
    /// back verbatim (Kition Slide does not edit custom properties).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub custom_xml: Option<String>,
}

/// An embedded file (image, audio, video, font, workbook).
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct MediaItem {
    pub id: MediaId,
    /// Original file name, e.g. `image1.png`.
    pub name: String,
    pub content_type: String,
    /// Bytes; kept out of the JSON manifest and stored as a separate file in native files.
    #[serde(skip)]
    pub data: Arc<Vec<u8>>,
    /// External link instead of embedded bytes.
    pub link: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct SlideFile {
    /// Slide size in points (default 16:9, 960 × 540).
    pub slide_size: Size,
    pub notes_size: Size,
    pub slides: Vec<Arc<Slide>>,
    pub masters: Vec<Arc<Master>>,
    pub notes_master: Option<Arc<Master>>,
    pub handout_master: Option<Arc<Master>>,
    pub sections: Vec<Section>,
    pub custom_shows: Vec<CustomShow>,
    pub show: ShowSettings,
    pub header_footer: HeaderFooter,
    pub props: Properties,
    /// SlideFile-wide default text style (shapes that aren't placeholders).
    pub default_text_style: text::ListStyle,
    pub media: Vec<MediaItem>,
    pub first_slide_number: u32,
    /// Next free shape/slide/layout/media id.
    pub next_id: u32,
    /// Embedded fonts (family names; bytes in `media`).
    pub embedded_fonts: Vec<(String, MediaId)>,
    /// The file's embedded font list as read (`p:embeddedFontLst`: every face of every font),
    /// written back as it was with the font parts in `embedded_font_rels`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub embedded_font_list: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub embedded_font_rels: Vec<KeptRel>,
    /// The face attributes the file gave each typeface it names (see [`FontFace`]).
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub font_faces: Vec<FontFace>,
    /// Customer data tag lists (`p:custDataLst/p:tags`), kept for add-ins that wrote them.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub tags: Vec<TagList>,
    /// Unmodelled slide-level XML kept for round-trip.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub raw_ext: Option<String>,
    /// The slide properties Kition does not model (`p:presentationPr` without its show
    /// settings: print settings, recent colours, picture compression and other extensions), kept
    /// as written; the show settings are written from [`SlideFile::show`].
    #[serde(skip_serializing_if = "Option::is_none")]
    pub pres_props: Option<String>,
    /// The view properties (`p:viewPr`: the last view, zoom, drawing guides), kept as written
    /// without the parts that name slides.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub view_props: Option<String>,
}

impl Default for SlideFile {
    fn default() -> Self {
        defaults::new_slide_file(None)
    }
}

impl SlideFile {
    /// A fresh id, unique across slides, shapes, layouts, masters and media in this slide file.
    pub fn alloc_id(&mut self) -> u32 {
        self.next_id = self.next_id.max(1).saturating_add(1);
        self.next_id
    }
    /// Gives each slide made since the file was read (no `file_id`) its own id as its file id, so
    /// a file written from this one names it by the id slide_inspect lists (and its tool id
    /// `slide:<id>`) when it is read again. For the copy a file is written from.
    pub fn keep_own_slide_ids(&mut self) {
        for slide in &mut self.slides {
            if slide.file_id.is_none() {
                std::sync::Arc::make_mut(slide).file_id = Some(slide.id.0);
            }
        }
    }
    /// Make sure `next_id` is above every id in use (after loading a file).
    pub fn fix_next_id(&mut self) {
        let mut m = self.next_id;
        for s in &self.slides {
            m = m.max(s.id.0);
            walk(&s.shapes, &mut |sh, _| m = m.max(sh.id.0));
        }
        for ma in &self.masters {
            m = m.max(ma.id.0);
            walk(&ma.shapes, &mut |sh, _| m = m.max(sh.id.0));
            for l in &ma.layouts {
                m = m.max(l.id.0);
                walk(&l.shapes, &mut |sh, _| m = m.max(sh.id.0));
            }
        }
        for md in &self.media {
            m = m.max(md.id.0);
        }
        self.next_id = m;
    }
    pub fn slide_index(&self, id: SlideId) -> Option<usize> {
        self.slides.iter().position(|s| s.id == id)
    }
    pub fn slide(&self, id: SlideId) -> Option<&Slide> {
        self.slides.iter().find(|s| s.id == id).map(|s| s.as_ref())
    }
    pub fn slide_mut(&mut self, id: SlideId) -> Option<&mut Slide> {
        self.slides.iter_mut().find(|s| s.id == id).map(Arc::make_mut)
    }
    pub fn slide_at(&self, i: usize) -> Option<&Slide> {
        self.slides.get(i).map(|s| s.as_ref())
    }
    /// Master and layout for a layout id.
    pub fn layout(&self, id: LayoutId) -> Option<(&Master, &Layout)> {
        self.masters.iter().find_map(|m| m.layout(id).map(|l| (m.as_ref(), l)))
    }
    /// The master (and layout) a slide uses, falling back to the first.
    pub fn master_for(&self, slide: &Slide) -> Option<(&Master, Option<&Layout>)> {
        if let Some((m, l)) = self.layout(slide.layout) {
            return Some((m, Some(l)));
        }
        self.masters.first().map(|m| (m.as_ref(), m.layouts.first()))
    }
    pub fn first_master(&self) -> Option<&Master> {
        self.masters.first().map(|m| m.as_ref())
    }
    pub fn media(&self, id: MediaId) -> Option<&MediaItem> {
        self.media.iter().find(|m| m.id == id)
    }
    /// Add media bytes, reusing an identical existing item.
    pub fn add_media(&mut self, name: &str, content_type: &str, data: Vec<u8>) -> MediaId {
        if let Some(m) = self.media.iter().find(|m| m.data.len() == data.len() && *m.data == data) {
            return m.id;
        }
        let id = MediaId(self.alloc_id());
        self.media.push(MediaItem { id, name: name.into(), content_type: content_type.into(), data: Arc::new(data), link: None });
        id
    }
    /// Whether every comment has an id no other comment holds (see
    /// [`SlideFile::fill_comment_ids`]).
    pub fn comment_ids_complete(&self) -> bool {
        let mut seen = std::collections::HashSet::new();
        self.slides.iter().all(|s| s.comments.iter().all(|c| comment_ids_fine(c, &mut seen)))
    }
    /// Gives every comment an id (and every top-level comment a `head`) unless it has one no
    /// other comment of the slide holds, minting them with `mint`. Answers whether it
    /// changed anything (only the slides it changed are copied).
    pub fn fill_comment_ids(&mut self, mint: &mut dyn FnMut() -> String) -> bool {
        let mut seen = std::collections::HashSet::new();
        let mut changed = false;
        for slide in &mut self.slides {
            let mut here = seen.clone();
            if slide.comments.iter().all(|c| comment_ids_fine(c, &mut here)) {
                seen = here;
                continue;
            }
            changed = true;
            for c in &mut Arc::make_mut(slide).comments {
                fill_one(c, true, &mut seen, mint);
            }
        }
        changed
    }
    /// Section index containing a slide (by position), when sections exist.
    pub fn section_of(&self, slide: SlideId) -> Option<usize> {
        self.sections.iter().position(|s| s.slides.contains(&slide))
    }
    /// 1-based slide number as displayed (respects `first_slide_number`).
    pub fn slide_number(&self, index: usize) -> u32 {
        self.first_slide_number.saturating_add(u32::try_from(index).unwrap_or(u32::MAX))
    }
    /// Basic consistency check: unique slide/shape ids, layouts exist. Returns problems found.
    pub fn validate(&self) -> Vec<String> {
        let mut out = vec![];
        let mut seen = std::collections::HashSet::new();
        for s in &self.slides {
            if !seen.insert(s.id) {
                out.push(format!("duplicate slide id {}", s.id));
            }
            if self.layout(s.layout).is_none() {
                out.push(format!("slide {} uses missing layout {}", s.id, s.layout));
            }
            let mut ids = std::collections::HashSet::new();
            walk(&s.shapes, &mut |sh, _| {
                if !ids.insert(sh.id) {
                    out.push(format!("slide {}: duplicate shape id {}", s.id, sh.id));
                }
            });
        }
        out
    }
}

#[cfg(test)]
mod tests;

fn each_comment_id(c: &Comment, top: bool, f: &mut dyn FnMut(&str)) {
    f(&c.id);
    if top {
        f(&c.head);
    }
    for r in &c.replies {
        f(&r.id);
    }
}

fn comment_ids_fine(c: &Comment, seen: &mut std::collections::HashSet<String>) -> bool {
    let mut ok = true;
    each_comment_id(c, true, &mut |id| ok &= !id.is_empty() && seen.insert(id.to_owned()));
    ok
}

fn fill_one(c: &mut Comment, top: bool, seen: &mut std::collections::HashSet<String>, mint: &mut dyn FnMut() -> String) {
    let mut fix = |id: &mut String| {
        if id.is_empty() || !seen.insert(id.clone()) {
            *id = mint();
            seen.insert(id.clone());
        }
    };
    fix(&mut c.id);
    if top {
        fix(&mut c.head);
    }
    for r in &mut c.replies {
        fix(&mut r.id);
    }
}
