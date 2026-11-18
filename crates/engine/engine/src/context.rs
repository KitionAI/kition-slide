//! The editor's state a command may read besides its parameters (the current slide, the selected
//! shapes, Slide Master view, the text being edited, the tool), taken, set for one call, and put
//! back without an edit: the revision stays, the slide file stays, nothing joins the undo history.
//! A host saves it before an agent's call and restores it afterwards, so no call depends on or
//! leaves behind what the person had current or selected (kition-sdk `KitDocument::editor_state`,
//! `set_context`; task card C4-slide).

use kition_slide_model::{ShapeId, SlideId};
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::{EngineError, Result, Selection, Session, Target, ToolKind};

/// A snapshot of the editor's state ([`Session::editor_state`]).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct EditorState {
    pub selection: Selection,
    pub tool: ToolKind,
}

/// The slide a call acts in, as a run's `target` or a step's `at.part` names it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CallSlide {
    /// Counted from 1, as `slide_inspect` and diagnostics count slides.
    Number(u64),
    /// A slide's id, as `slide_inspect` lists it.
    Id(u64),
}

fn refused(message: String) -> EngineError {
    EngineError::Other(message)
}

/// A persistent tool id, or a scoped numeric shape address accepted on command input.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ToolId {
    Slide(SlideId),
    Identity(String),
    Shape(SlideId, ShapeId),
}

impl ToolId {
    /// The tool id `text` writes, if it is one.
    pub fn parse(text: &str) -> Option<Self> {
        let number = |text: &str| text.parse::<u32>().ok();
        if let Some(rest) = text.strip_prefix("slide:") {
            return number(rest).map(|id| Self::Slide(SlideId(id)));
        }
        let value = text.strip_prefix("shape:")?;
        if let Some(identity) = kition_slide_model::identity::canonical(value) {
            return Some(Self::Identity(identity));
        }
        let (slide, shape) = value.split_once('/')?;
        Some(Self::Shape(SlideId(number(slide)?), ShapeId(number(shape)?)))
    }
}

/// The keys whose values name what a command acts on, by number or by tool id.
const ID_KEYS: [&str; 5] = ["ids", "id", "from", "to", "trigger"];

impl Session {
    /// The editor's state now; `None` without an open slide file.
    pub fn editor_state(&self) -> Option<EditorState> {
        self.active().map(|st| EditorState { selection: st.selection.clone(), tool: self.tool.kind.clone() })
    }

    /// Puts back a state [`Session::editor_state`] answered. It is no edit; ids the slide file no
    /// longer has are dropped, as after any command.
    pub fn restore_editor_state(&mut self, state: &EditorState) -> Result<()> {
        let st = self.doc_mut()?;
        st.selection = state.selection.clone();
        crate::clamp_selection(st);
        if self.tool.kind != state.tool {
            self.set_tool(state.tool.clone());
        }
        Ok(())
    }

    /// Writes tool ids in target, endpoint and trigger parameters as the numbers commands take, and answers
    /// the slide (its index, from 0) the shapes' tool ids name: a shape's tool id names its slide,
    /// so it needs no `onSlide`. Shapes of two slides in one call, and a slide the deck lacks, are
    /// refused.
    pub fn take_tool_ids(&self, params: &mut Value) -> Result<Option<usize>> {
        let st = self.doc()?;
        let mut slide: Option<(SlideId, String)> = None;
        for key in ID_KEYS {
            let Some(value) = params.get_mut(key) else { continue };
            let items: Vec<&mut Value> = match value {
                Value::Array(items) => items.iter_mut().collect(),
                other => vec![other],
            };
            for item in items {
                let Some(id) = item.as_str().and_then(ToolId::parse) else { continue };
                let text = item.as_str().unwrap_or_default().to_owned();
                let id = if let ToolId::Identity(identity) = id {
                    let found = st.doc.slides.iter().find_map(|slide| {
                        let mut found = None;
                        kition_slide_model::walk(&slide.shapes, &mut |shape, _| {
                            if shape.identity == identity {
                                found = Some(ToolId::Shape(slide.id, shape.id));
                            }
                        });
                        found
                    });
                    found.ok_or_else(|| refused(format!("no shape has identity `{text}`: inspect the deck for its shapes' tool ids")))?
                } else {
                    id
                };
                match id {
                    ToolId::Slide(id) => *item = Value::from(id.0),
                    ToolId::Identity(_) => {}
                    ToolId::Shape(on, shape) => {
                        // The scoped spelling is a positional address: shape number N on slide S.
                        if st.doc.slides.iter().any(|s| s.id == on) && !st.doc.slides.iter().any(|s| s.id == on && s.shape(shape).is_some()) {
                            return Err(refused(format!(
                                "`{text}` is a positional address (shape number {} on the slide with id {}) and no shape is there now: the shape may have moved or been renumbered; name it by its persistent tool id (`shape:<identity>`) from slide_inspect",
                                shape.0, on.0
                            )));
                        }
                        match &slide {
                            Some((first, named)) if *first != on => {
                                return Err(refused(format!(
                                    "`{named}` and `{text}` are shapes of two slides (slide ids {} and {}): one call acts on one slide's shapes; give them in separate steps",
                                    first.0, on.0
                                )));
                            }
                            Some(_) => {}
                            None => slide = Some((on, text.clone())),
                        }
                        *item = Value::from(shape.0);
                    }
                }
            }
        }
        let Some((id, named)) = slide else { return Ok(None) };
        st.doc.slide_index(id).map(Some).ok_or_else(|| {
            refused(format!("`{named}` names the slide with id {}, and no slide has id {}: inspect the deck for its slides' ids", id.0, id.0))
        })
    }

    /// Sets what one call acts in and on: `slide` becomes the current slide (out of Slide Master
    /// view, with nothing selected there), then the shapes `on` (numbers as `slide_inspect` lists
    /// them, or the object ids `drawing.insert` answers) become the selection on it. Shape ids
    /// repeat across slides, so `on` needs `slide`. A slide or shape the file lacks is refused,
    /// with where valid ones come from, and nothing is changed. It is no edit.
    pub fn set_call_context(&mut self, slide: Option<CallSlide>, on: &[Value]) -> Result<()> {
        let st = self.doc()?;
        let count = st.doc.slides.len();
        // Shapes by tool id name their slide: it is the part, and a part the call names must agree.
        let mut ids = serde_json::json!({ "ids": on });
        let implied = self.take_tool_ids(&mut ids)?;
        let on: Vec<Value> = ids["ids"].as_array().cloned().unwrap_or_default();
        let on = on.as_slice();
        let slide = match (slide, implied) {
            (None, Some(index)) => Some(CallSlide::Number(u64::try_from(index).unwrap_or(u64::MAX).saturating_add(1))),
            (given, _) => given,
        };
        let index = match slide {
            None => None,
            Some(CallSlide::Number(n)) => Some(usize::try_from(n).ok().filter(|n| (1..=count).contains(n)).map(|n| n - 1).ok_or_else(|| {
                refused(format!("there is no slide {n}: the deck has {}, counted from 1 (slide_inspect lists them)", plural(count)))
            })?),
            Some(CallSlide::Id(id)) => Some(u32::try_from(id).ok().and_then(|id| st.doc.slide_index(SlideId(id))).ok_or_else(|| {
                refused(format!(
                    "no slide has id {id}: slide ids are the `id`s slide_inspect lists for the slides (or `{{slide: n}}`, counted from 1)"
                ))
            })?),
        };
        if let (Some(index), Some(implied)) = (index, implied)
            && index != implied
        {
            return Err(refused(format!(
                "`on` names shapes of slide {}, and the call's part is slide {} (counted from 1): a shape's tool id names its slide, so leave the part out or name that slide",
                implied + 1,
                index + 1
            )));
        }
        if !on.is_empty() && index.is_none() {
            return Err(refused(
                "`on` names shapes, and a shape id means something only on its own slide (ids repeat across slides): give the slide as well, as the run's `target` or the step's `at.part` (`{slide: n}` counted from 1, or `{slideId}`)"
                    .into(),
            ));
        }
        let st = self.doc_mut()?;
        let before = st.selection.clone();
        if let Some(index) = index {
            st.selection = Selection {
                slide: index,
                slides: st.doc.slides.get(index).map(|slide| vec![slide.id]).unwrap_or_default(),
                target: Target::Slides,
                ..Selection::default()
            };
        }
        if on.is_empty() {
            return Ok(());
        }
        let number = st.selection.slide + 1;
        let mut shapes: Vec<ShapeId> = Vec::new();
        for id in on {
            let text = match id {
                Value::String(text) => text.clone(),
                Value::Number(n) => n.to_string(),
                other => {
                    st.selection = before;
                    return Err(refused(format!("`on` holds {other}: a shape id is a number (or an object id string), as slide_inspect lists it")));
                }
            };
            match crate::cmd::drawing::local_id(st, &text) {
                Some(shape) if !shapes.contains(&shape) => shapes.push(shape),
                Some(_) => {}
                None => {
                    st.selection = before;
                    return Err(refused(format!(
                        "slide {number} has no shape `{text}`: its shapes' ids are the `id`s slide_inspect lists for it (`target: {{slide: {number}}}`, `part.shapes[].id`)"
                    )));
                }
            }
        }
        st.selection.shapes = shapes;
        st.selection.text = None;
        st.selection.cells = None;
        Ok(())
    }
}

fn plural(count: usize) -> String {
    format!("{count} slide{}", if count == 1 { "" } else { "s" })
}
