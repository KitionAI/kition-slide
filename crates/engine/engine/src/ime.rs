//! An input method's composition in the text being edited (plan stage 4, walkthrough E10; WPS
//! W-R1, W-R2).
//!
//! What an input method (Chinese, Japanese, Korean) is composing shows in the text at the caret,
//! replacing the selected text, laid out with the text around it (an auto-sizing box grows with it):
//! the editor draws a copy of the slide file with the composition typed in and underlines it. The
//! composition is session state: the document, its undo history and its saved state stay as they
//! were until the input method commits, and the committed text is typed as one undo step of its
//! own ([`Session::type_text`]). The composition shows only while the document and the text
//! selection are as they were when it was last set: a click elsewhere, an undo or another's edit
//! hides it until the input method sends it again.

use std::sync::Arc;

use kition_slide_model::SlideFile;
use kition_slide_model::edit::{self as ed, Pos};

use crate::{Result, Session, TextSel, cmd};

/// The longest composition shown, in characters.
pub const MOST: usize = 1000;

/// A composition under way.
#[derive(Clone, Debug)]
pub struct Preedit {
    /// The composed text (no line breaks).
    pub text: String,
    /// The slide file it was typed into and the text selection it replaces: it shows while both
    /// are the document's and the selection's.
    base: Arc<SlideFile>,
    at: TextSel,
    /// The slide file with the composition typed in, as the editor draws it.
    pub shown: Arc<SlideFile>,
    /// Where the composition is in the shown text: from `start` to `end`, the input method's cursor
    /// at `cursor`.
    pub start: Pos,
    pub end: Pos,
    pub cursor: Pos,
}

/// What [`Session::show_preedit`] replaced, and the composition it shows.
#[derive(Debug)]
pub struct Shown {
    doc: Arc<SlideFile>,
    text: Option<TextSel>,
    pub preedit: Preedit,
}

impl Preedit {
    /// The text selection as the editor shows it while composing: the caret at the input
    /// method's cursor, in the shown text.
    pub fn caret(&self) -> TextSel {
        TextSel { anchor: self.cursor, caret: self.cursor, ..self.at.clone() }
    }
}

impl Session {
    /// Sets what an input method composes (`text`, its cursor `cursor` characters into it) in the
    /// text being edited; empty text ends the composition. With a shape selected that typing goes
    /// into (WPS W-F5), its text is edited first, where its caret last was. Line breaks are left
    /// out (Enter is a key of its own), and at most [`MOST`] characters show. Answers whether a
    /// composition shows now; with no text to type into there is none (WPS W-R4).
    pub fn set_preedit(&mut self, text: &str, cursor: usize) -> Result<bool> {
        let text: String = text.chars().filter(|c| !matches!(c, '\n' | '\r' | '\u{b}')).take(MOST).collect();
        if text.is_empty() {
            if let Some(st) = self.active_mut()
                && st.preedit.take().is_some()
            {
                st.revision += 1;
            }
            return Ok(false);
        }
        let Some(st) = self.active() else { return Ok(false) };
        if st.selection.text.is_none() {
            let Some(id) = st.typing_target() else { return Ok(false) };
            let at = st.last_caret(id);
            self.execute(
                "text.edit",
                &match at {
                    Some(at) => serde_json::json!({"id": id, "at": [at.0, at.1]}),
                    None => serde_json::json!({"id": id}),
                },
            )?;
        }
        let st = self.doc_mut()?;
        let Some(at) = st.selection.text.clone().filter(|t| !t.notes) else { return Ok(false) };
        let base = st.doc.clone();
        let mut doc = (*base).clone();
        let Some(body) = cmd::text::body_mut(&mut doc, &st.selection, &at) else { return Ok(false) };
        let (a, b) = at.ordered();
        let start = ed::delete(body, a, b);
        let props = if a != b { Some(ed::props_at(body, start)) } else { cmd::text::unlinked_at_link_end(body, start) };
        let end = ed::insert(body, start, &text, props);
        let length = text.chars().count();
        let cursor = (start.0, start.1.saturating_add(cursor.min(length)));
        if at.cell.is_none()
            && let Some(fitted) = cmd::text::fitted_box(&doc, &st.selection, at.shape)
            && let Some(list) = crate::shapes_mut(&mut doc, &st.selection)
            && let Some(shape) = kition_slide_model::find_shape_mut(list, at.shape)
        {
            shape.xfrm = Some(fitted);
        }
        st.preedit = Some(Preedit { text, base, at, shown: Arc::new(doc), start, end, cursor });
        st.revision += 1;
        Ok(true)
    }

    /// The composition the editor shows now, if any (see the module's notes).
    pub fn preedit(&self) -> Option<&Preedit> {
        let st = self.active()?;
        let p = st.preedit.as_ref()?;
        (Arc::ptr_eq(&p.base, &st.doc) && st.selection.text.as_ref() == Some(&p.at)).then_some(p)
    }

    /// Puts the document as the editor shows it while composing in the session's place: the
    /// composition typed in and the caret at the input method's cursor, for drawing it (the
    /// overlays, the caret's place). `None` with no composition showing. Give the answer to
    /// [`Session::unshow_preedit`] straight after, which puts the document and the selection back.
    pub fn show_preedit(&mut self) -> Option<Shown> {
        let preedit = self.preedit()?.clone();
        let st = self.active_mut()?;
        let doc = std::mem::replace(&mut st.doc, preedit.shown.clone());
        let text = st.selection.text.replace(preedit.caret());
        Some(Shown { doc, text, preedit })
    }

    /// Puts back what [`Session::show_preedit`] replaced.
    pub fn unshow_preedit(&mut self, shown: Shown) {
        if let Some(st) = self.active_mut() {
            st.doc = shown.doc;
            st.selection.text = shown.text;
        }
    }

    /// Ends the composition under way, if there is one; answers whether there was.
    pub(crate) fn end_preedit(&mut self) -> bool {
        let Some(st) = self.active_mut() else { return false };
        let had = st.preedit.take().is_some();
        if had {
            st.revision += 1;
        }
        had
    }
}
