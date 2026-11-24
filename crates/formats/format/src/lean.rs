//! `slide.json` without the members equal to their defaults.
//!
//! The slide's serde form is walked by type: each object of a model type (a shape, a text body,
//! a paragraph's properties…) loses the members equal to that type's default's, and keeps that
//! only if it reads back as the very same value (every model type reads a missing member as its
//! default, so it does; one that does not keeps its members). The objects inside it are leaned
//! first, each checked the same way, so a mistake here can only make a file larger, never change
//! what it holds.
//!
//! What a default holds of the built-in content (a new deck's master, layouts, slide and default
//! text style, a theme) is always written: that content is made by code, so a file leaning on it
//! would change appearance whenever that code does. Only members whose default is a plain value
//! (none, false, zero, empty, a fixed setting) are left out.

use std::any::TypeId;
use std::collections::HashMap;

use kition_slide_model::anim::{Animation, Transition};
use kition_slide_model::style::{Effects, Line};
use kition_slide_model::table::{Cell, Row};
use kition_slide_model::text::{BodyProps, LevelStyle, ListStyle, ParaProps, Paragraph, RunProps, TextBody};
use kition_slide_model::{
    Comment, CustomShow, HeaderFooter, Layout, Master, MediaItem, Placeholder, Properties, Section, Shape, ShowSettings, Slide, SlideFile,
};
use serde::Serialize;
use serde::de::DeserializeOwned;
use serde_json::Value;

/// How deep groups may nest before their members are written in full.
const MAX_DEPTH: usize = 64;

/// `p`'s serde form without the members equal to their defaults.
pub(crate) fn slide_file(p: &SlideFile) -> serde_json::Result<Value> {
    let mut v = serde_json::to_value(p)?;
    // A PPTX's view properties (the last view, zoom) are a view of the presentation, not part of it.
    if let Some(map) = v.as_object_mut() {
        map.remove("viewProps");
    }
    let full = v.clone();
    let mut lean = Lean::default();
    if let Some(map) = v.as_object_mut() {
        for (key, item) in map.iter_mut() {
            match key.as_str() {
                "masters" => lean.each(item, Lean::master),
                "notesMaster" | "handoutMaster" => lean.master(item),
                "slides" => lean.each(item, Lean::slide),
                "defaultTextStyle" => lean.list_style(item),
                "sections" => lean.each(item, |lean, v| lean.plain::<Section>(v)),
                "customShows" => lean.each(item, |lean, v| lean.plain::<CustomShow>(v)),
                "show" => lean.plain::<ShowSettings>(item),
                "headerFooter" => lean.plain::<HeaderFooter>(item),
                "props" => lean.plain::<Properties>(item),
                "media" => lean.each(item, |lean, v| lean.plain::<MediaItem>(v)),
                _ => {}
            }
        }
    }
    lean.members_keeping::<SlideFile>(&mut v, &full, BUILT_IN);
    Ok(v)
}

/// The slide file's members whose default is built-in content (`SlideFile::default()` is a new
/// deck): always written.
const BUILT_IN: &[&str] = &["slideSize", "notesSize", "masters", "slides", "notesMaster", "handoutMaster", "defaultTextStyle", "nextId"];

/// The default of each model type met so far, in its serde form.
#[derive(Default)]
struct Lean {
    defaults: HashMap<TypeId, Option<Value>>,
}

impl Lean {
    /// `v` without the members equal to `T`'s default's, if it reads back as `full` (what it was
    /// before anything inside it was leaned); otherwise as it is.
    fn members<T: Serialize + DeserializeOwned + Default + 'static>(&mut self, v: &mut Value, full: &Value) {
        self.members_keeping::<T>(v, full, &[]);
    }

    /// [`Lean::members`], always writing the members `keep` names.
    fn members_keeping<T: Serialize + DeserializeOwned + Default + 'static>(&mut self, v: &mut Value, full: &Value, keep: &[&str]) {
        let defaults = self.defaults.entry(TypeId::of::<T>()).or_insert_with(|| serde_json::to_value(T::default()).ok());
        let (Some(Value::Object(defaults)), Some(map)) = (defaults.as_ref(), v.as_object()) else { return };
        let lean: serde_json::Map<String, Value> = map
            .iter()
            .filter(|(key, item)| keep.contains(&key.as_str()) || defaults.get(*key) != Some(*item))
            .map(|(k, x)| (k.clone(), x.clone()))
            .collect();
        if lean.len() == map.len() {
            return;
        }
        let lean = Value::Object(lean);
        let back = serde_json::from_value::<T>(lean.clone()).ok().and_then(|t| serde_json::to_value(t).ok());
        if back.as_ref() == Some(full) {
            *v = lean;
        }
    }

    /// A `T` holding no other model type worth leaning.
    fn plain<T: Serialize + DeserializeOwned + Default + 'static>(&mut self, v: &mut Value) {
        if v.is_object() {
            let full = v.clone();
            self.members::<T>(v, &full);
        }
    }

    /// Every item of the array `v` through `f`.
    fn each(&mut self, v: &mut Value, f: impl Fn(&mut Self, &mut Value)) {
        if let Value::Array(items) = v {
            for item in items {
                f(self, item);
            }
        }
    }

    fn master(&mut self, v: &mut Value) {
        if !v.is_object() {
            return;
        }
        let full = v.clone();
        if let Some(map) = v.as_object_mut() {
            for (key, item) in map.iter_mut() {
                match key.as_str() {
                    "shapes" => self.each(item, |lean, v| lean.shape(v, 0)),
                    "layouts" => self.each(item, Lean::layout),
                    "titleStyle" | "bodyStyle" | "otherStyle" => self.list_style(item),
                    _ => {}
                }
            }
        }
        // The theme's default is the built-in theme.
        self.members_keeping::<Master>(v, &full, &["theme"]);
    }

    fn layout(&mut self, v: &mut Value) {
        if !v.is_object() {
            return;
        }
        let full = v.clone();
        if let Some(shapes) = v.get_mut("shapes") {
            self.each(shapes, |lean, v| lean.shape(v, 0));
        }
        self.members::<Layout>(v, &full);
    }

    fn slide(&mut self, v: &mut Value) {
        if !v.is_object() {
            return;
        }
        let full = v.clone();
        if let Some(map) = v.as_object_mut() {
            for (key, item) in map.iter_mut() {
                match key.as_str() {
                    "shapes" => self.each(item, |lean, v| lean.shape(v, 0)),
                    "notes" => self.text_body(item),
                    "animations" => self.each(item, |lean, v| lean.plain::<Animation>(v)),
                    "transition" => self.plain::<Transition>(item),
                    "comments" => self.each(item, |lean, v| lean.plain::<Comment>(v)),
                    _ => {}
                }
            }
        }
        self.members::<Slide>(v, &full);
    }

    fn shape(&mut self, v: &mut Value, depth: usize) {
        if !v.is_object() || depth > MAX_DEPTH {
            return;
        }
        let full = v.clone();
        if let Some(map) = v.as_object_mut() {
            for (key, item) in map.iter_mut() {
                match key.as_str() {
                    "text" => self.text_body(item),
                    "ph" => self.plain::<Placeholder>(item),
                    "line" => self.plain::<Line>(item),
                    "effects" => self.plain::<Effects>(item),
                    "kind" => self.kind(item, depth),
                    _ => {}
                }
            }
        }
        self.members::<Shape>(v, &full);
    }

    /// A shape's kind: a group's members and a table's rows.
    fn kind(&mut self, v: &mut Value, depth: usize) {
        let Some(map) = v.as_object_mut() else { return };
        match map.get("type").and_then(Value::as_str) {
            Some("group") => {
                if let Some(children) = map.get_mut("children") {
                    self.each(children, |lean, v| lean.shape(v, depth + 1));
                }
            }
            Some("table") => {
                if let Some(rows) = map.get_mut("rows") {
                    self.each(rows, Lean::row);
                }
            }
            _ => {}
        }
    }

    fn row(&mut self, v: &mut Value) {
        if !v.is_object() {
            return;
        }
        let full = v.clone();
        if let Some(cells) = v.get_mut("cells") {
            self.each(cells, Lean::cell);
        }
        self.members::<Row>(v, &full);
    }

    fn cell(&mut self, v: &mut Value) {
        if !v.is_object() {
            return;
        }
        let full = v.clone();
        if let Some(text) = v.get_mut("text") {
            self.text_body(text);
        }
        if let Some(Value::Array(borders)) = v.get_mut("borders") {
            for border in borders {
                self.plain::<Line>(border);
            }
        }
        for key in ["diagDown", "diagUp"] {
            if let Some(line) = v.get_mut(key) {
                self.plain::<Line>(line);
            }
        }
        self.members::<Cell>(v, &full);
    }

    fn text_body(&mut self, v: &mut Value) {
        if !v.is_object() {
            return;
        }
        let full = v.clone();
        if let Some(map) = v.as_object_mut() {
            for (key, item) in map.iter_mut() {
                match key.as_str() {
                    "body" => self.plain::<BodyProps>(item),
                    "listStyle" => self.list_style(item),
                    "paragraphs" => self.each(item, Lean::paragraph),
                    _ => {}
                }
            }
        }
        self.members::<TextBody>(v, &full);
    }

    fn paragraph(&mut self, v: &mut Value) {
        if !v.is_object() {
            return;
        }
        let full = v.clone();
        if let Some(map) = v.as_object_mut() {
            for (key, item) in map.iter_mut() {
                match key.as_str() {
                    "props" => self.plain::<ParaProps>(item),
                    "endProps" => self.plain::<RunProps>(item),
                    "runs" => self.each(item, |lean, run| {
                        if let Some(props) = run.get_mut("props") {
                            lean.plain::<RunProps>(props);
                        }
                    }),
                    _ => {}
                }
            }
        }
        self.members::<Paragraph>(v, &full);
    }

    fn list_style(&mut self, v: &mut Value) {
        if !v.is_object() {
            return;
        }
        let full = v.clone();
        if let Some(map) = v.as_object_mut() {
            for (key, item) in map.iter_mut() {
                match key.as_str() {
                    "levels" => self.each(item, Lean::level),
                    "defaultLevel" => self.level(item),
                    _ => {}
                }
            }
        }
        self.members::<ListStyle>(v, &full);
    }

    fn level(&mut self, v: &mut Value) {
        if !v.is_object() {
            return;
        }
        let full = v.clone();
        if let Some(para) = v.get_mut("para") {
            self.plain::<ParaProps>(para);
        }
        if let Some(run) = v.get_mut("run") {
            self.plain::<RunProps>(run);
        }
        self.members::<LevelStyle>(v, &full);
    }
}
