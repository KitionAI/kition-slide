//! Keyboard: text-editing keys while editing text, nudges and shortcuts otherwise. Shared by the
//! UI, the control channel and MCP so a key does the same thing everywhere.

use std::sync::Arc;

use kition_slide_model::SlideFile;
use serde_json::{Value, json};

use crate::tools::Mods;
use crate::{DocState, Result, Selection, Session};

/// Cmd+arrow's nudge, in points (WPS W-D4).
pub const CMD_NUDGE_PT: f64 = 10.0;
/// An arrow key's nudge with snap to grid off, in points (WPS W-D2: a pixel at 96 dpi).
pub const FINE_NUDGE_PT: f64 = 0.75;

/// What a run of keystrokes is (plan stage 2; WPS W-H1, W-H2): characters typed one after
/// another, Backspace or Delete pressed one after another, arrow-key nudges of one selection.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum RunKind {
    Typing,
    Deleting,
    Nudging,
}

/// A run under way: the person's next keystroke of the same kind joins its undo step (one step
/// however long the run, a pause too), as long as the document, the selection and the history are
/// exactly as its last keystroke left them. Anything else in between ends it: moving the caret or
/// changing the selection (keys or the pointer), another command (formatting too), undo, an
/// agent's edit, a collaborator's change, a merged draft or a restored version.
#[derive(Clone, Debug)]
pub(crate) struct EditRun {
    kind: RunKind,
    doc: Arc<SlideFile>,
    selection: Selection,
    pushed: u64,
    undo: usize,
    redo: usize,
}

impl EditRun {
    fn after(kind: RunKind, st: &DocState) -> Self {
        EditRun {
            kind,
            doc: st.doc.clone(),
            selection: st.selection.clone(),
            pushed: st.history.pushed,
            undo: st.history.undo.len(),
            redo: st.history.redo.len(),
        }
    }

    fn goes_on(&self, kind: RunKind, st: &DocState) -> bool {
        self.kind == kind
            && Arc::ptr_eq(&self.doc, &st.doc)
            && self.selection == st.selection
            && self.pushed == st.history.pushed
            && self.undo == st.history.undo.len()
            && self.redo == st.history.redo.len()
            && st.history.undo.last().is_some_and(|e| !e.agent)
    }
}

/// Does a shortcut like `Cmd+Shift+]` match `key` + `mods`?
pub fn shortcut_matches(shortcut: &str, key: &str, mods: Mods) -> bool {
    let parts: Vec<&str> = shortcut.split('+').collect();
    let (k, ms) = match parts.as_slice() {
        [.., "", ""] => ("+", &parts[..parts.len().saturating_sub(2)]),
        _ => match parts.split_last() {
            Some((k, ms)) => (*k, ms),
            None => return false,
        },
    };
    let has = |m: &str| ms.iter().any(|x| x.eq_ignore_ascii_case(m));
    k.eq_ignore_ascii_case(key) && (has("cmd") || has("ctrl")) == mods.cmd && has("shift") == mods.shift && (has("alt") || has("option")) == mods.alt
}

impl Session {
    /// Handle one key press. `key` is a name (`Enter`, `Escape`, `Backspace`, `Delete`, `Tab`,
    /// `ArrowLeft`…, `Home`, `End`, `PageUp`, `PageDown`, `F5`) or a character (`b`, `]`).
    /// Returns `{handled: bool, …}`.
    pub fn key(&mut self, key: &str, mods: Mods) -> Result<Value> {
        let r = match self.run_kind(key, mods) {
            Some(kind) => self.in_run(kind, |s| s.key_press(key, mods)),
            None => {
                self.end_run();
                self.key_press(key, mods)
            }
        };
        self.sync_all_kdm();
        self.drop_left_box();
        r
    }

    /// The run a key press goes on with: Backspace and Delete (Alt too) while text is edited, an
    /// arrow key that nudges the selected shapes. Any other key ends the run.
    fn run_kind(&self, key: &str, mods: Mods) -> Option<RunKind> {
        let st = self.active()?;
        if st.interaction.is_some() {
            return None;
        }
        let k = key.to_ascii_lowercase();
        let k = k.trim_start_matches("arrow");
        if st.selection.text.is_some() {
            return (!mods.cmd && matches!(k, "backspace" | "delete")).then_some(RunKind::Deleting);
        }
        // Cmd+arrow nudges too (by 10 pt), on the same run.
        let nudges = !st.selection.shapes.is_empty() && st.selection.cells.is_none();
        (nudges && matches!(k, "left" | "right" | "up" | "down")).then_some(RunKind::Nudging)
    }

    /// Runs `edit`, one keystroke of a run of `kind`: the person's keystroke that goes on with the
    /// run under way pushes no undo step and joins the run's one (and its replica step); one that
    /// changed the document starts or goes on with the run. An agent's keystroke is a step of its
    /// own and ends the run.
    fn in_run(&mut self, kind: RunKind, edit: impl FnOnce(&mut Self) -> Result<Value>) -> Result<Value> {
        let person = !self.acts_as_agent();
        let Some(st) = self.active_mut() else { return edit(self) };
        let run = st.run.take();
        let joins = person && run.is_some_and(|run| run.goes_on(kind, st));
        st.link.join = joins;
        let (uid, before) = (st.uid, st.doc.clone());
        let r = edit(self);
        self.sync_all_kdm();
        if let Some(st) = self.active_mut()
            && st.uid == uid
        {
            st.link.join = false;
            if person && r.is_ok() && (joins || !Arc::ptr_eq(&before, &st.doc)) {
                st.run = Some(EditRun::after(kind, st));
            }
        }
        r
    }

    /// How far an arrow key moves the selected shapes, in points (WPS W-D1–W-D4): one grid step
    /// with snap to grid on, 0.75 pt (a pixel at 96 dpi) with it off, 10 pt with Cmd either way.
    /// Shift and Alt change nothing.
    pub fn nudge_step(&self, mods: Mods) -> f64 {
        let grid = self.prefs.grid_spacing;
        if mods.cmd {
            CMD_NUDGE_PT
        } else if self.prefs.snap_to_grid && grid.is_finite() && grid > 0.0 {
            grid
        } else {
            FINE_NUDGE_PT
        }
    }

    /// Ends the run of typing or nudges under way: the next keystroke is a step of its own.
    pub(crate) fn end_run(&mut self) {
        for st in &mut self.docs {
            st.run = None;
        }
    }

    fn key_press(&mut self, key: &str, mods: Mods) -> Result<Value> {
        let editing = self.active().is_some_and(|d| d.selection.text.is_some());
        let has_shapes = self.active().is_some_and(|d| !d.selection.shapes.is_empty());
        let k = key.to_ascii_lowercase();
        let k = k.trim_start_matches("arrow");
        if self.tool.drawing_freeform() && matches!(k, "enter" | "return" | "escape") {
            return self.finish_freeform(false);
        }
        if matches!(k, "escape" | "esc") && self.cancel_gesture() {
            return Ok(json!({"handled": true}));
        }
        // Escape puts a loaded Format Painter down (one kept loaded by a double click too, WPS
        // W-P2) before it cancels anything else.
        if matches!(k, "escape" | "esc") && (self.painter.is_some() || self.anim_painter.is_some()) {
            self.painter = None;
            self.anim_painter = None;
            return Ok(json!({"handled": true}));
        }
        // Escape puts a drawing tool down (the Text Box tool, a shape locked for drawing one after
        // another; WPS W-E4, W-E10) before it leaves the selection.
        if matches!(k, "escape" | "esc") && !matches!(self.tool.kind, crate::tools::ToolKind::Select) {
            self.set_tool(crate::tools::ToolKind::Select);
            return Ok(json!({"handled": true}));
        }
        if !mods.cmd && !mods.alt && matches!(k, "left" | "right" | "up" | "down") && self.table_arrow(k, mods.shift)? {
            return Ok(json!({"handled": true}));
        }
        if !editing && !mods.cmd && matches!(k, "backspace" | "delete") && self.active().is_some_and(|d| d.selection.cells.is_some()) {
            self.execute("table.clearCells", &json!({}))?;
            return Ok(json!({"handled": true}));
        }
        if editing && !mods.cmd {
            let r = match k {
                "left" => Some(self.execute("text.move", &json!({"to": if mods.alt { "wordLeft" } else { "left" }, "extend": mods.shift}))),
                "right" => Some(self.execute("text.move", &json!({"to": if mods.alt { "wordRight" } else { "right" }, "extend": mods.shift}))),
                "up" => Some(self.execute("text.move", &json!({"to": if mods.alt { "paraStart" } else { "up" }, "extend": mods.shift}))),
                "down" => Some(self.execute("text.move", &json!({"to": if mods.alt { "paraEnd" } else { "down" }, "extend": mods.shift}))),
                "home" => Some(self.execute("text.move", &json!({"to": "lineStart", "extend": mods.shift}))),
                "end" => Some(self.execute("text.move", &json!({"to": "lineEnd", "extend": mods.shift}))),
                "backspace" => Some(self.execute("text.delete", &json!({"dir": if mods.alt { "wordBackward" } else { "backward" }}))),
                "delete" => Some(self.execute("text.delete", &json!({"dir": if mods.alt { "wordForward" } else { "forward" }}))),
                "enter" | "return" => Some(self.execute("text.insert", &json!({"text": if mods.shift { "\u{b}" } else { "\n" }}))),
                "escape" | "esc" => Some(self.execute("text.exit", &json!({}))),
                "tab" => {
                    // Tab at the start of a paragraph changes the list level; elsewhere it types a tab.
                    let at_start = self.active().and_then(|d| d.selection.text.as_ref()).is_some_and(|t| t.ordered().0.1 == 0 || t.is_range());
                    let in_cell = self.active().and_then(|d| d.selection.text.as_ref()).is_some_and(|t| t.cell.is_some());
                    if in_cell {
                        Some(self.next_cell(mods.shift))
                    } else if at_start {
                        Some(self.execute(if mods.shift { "format.outdent" } else { "format.indent" }, &json!({})))
                    } else {
                        Some(self.execute("text.insert", &json!({"text": "\t"})))
                    }
                }
                _ => None,
            };
            if let Some(r) = r {
                r?;
                return Ok(json!({"handled": true}));
            }
        }
        // The Mac's text keys (walkthrough E3): Cmd+Left / Right to the line's start and end,
        // Cmd+Up / Down (and Cmd+Home / End) to the text's start and end, with Shift extending the
        // selection; Cmd+Backspace deletes to the line's start, Cmd+Delete to its end. (Elsewhere
        // the editor sends Ctrl+Left / Right as Alt+Left / Right: by word.)
        if editing && mods.cmd && !mods.alt {
            let to = match k {
                "left" => Some("lineStart"),
                "right" => Some("lineEnd"),
                "up" | "home" => Some("start"),
                "down" | "end" => Some("end"),
                _ => None,
            };
            let r = match (to, k) {
                (Some(to), _) => Some(self.execute("text.move", &json!({"to": to, "extend": mods.shift}))),
                (None, "backspace") if !mods.shift => Some(self.execute("text.delete", &json!({"dir": "lineBackward"}))),
                (None, "delete") if !mods.shift => Some(self.execute("text.delete", &json!({"dir": "lineForward"}))),
                _ => None,
            };
            if let Some(r) = r {
                r?;
                return Ok(json!({"handled": true}));
            }
        }
        let cells = self.active().is_some_and(|d| d.selection.cells.is_some());
        if !editing && has_shapes && !cells {
            let step = self.nudge_step(mods);
            let d = match k {
                "left" => Some((-step, 0.0)),
                "right" => Some((step, 0.0)),
                "up" => Some((0.0, -step)),
                "down" => Some((0.0, step)),
                _ => None,
            };
            if let Some((dx, dy)) = d {
                self.execute("shape.move", &json!({"dx": dx, "dy": dy}))?;
                return Ok(json!({"handled": true}));
            }
        }
        if !editing && has_shapes && !mods.cmd {
            match k {
                "backspace" | "delete" => {
                    self.execute("edit.delete", &json!({}))?;
                    return Ok(json!({"handled": true}));
                }
                "enter" | "return" | "f2" => {
                    // Into the text with all of it selected (WPS W-F4): what is typed next replaces it.
                    self.execute("text.edit", &json!({"all": true}))?;
                    return Ok(json!({"handled": true}));
                }
                "escape" | "esc" => {
                    // Out of a group's members to the group (WPS W-A12), then to nothing.
                    match self.group_of_selection() {
                        Some(group) => self.select(|_, sel| {
                            sel.shapes = vec![group];
                            sel.text = None;
                            sel.cells = None;
                        })?,
                        None => {
                            self.execute("edit.deselectAll", &json!({}))?;
                        }
                    }
                    return Ok(json!({"handled": true}));
                }
                "tab" => {
                    self.cycle_selection(!mods.shift)?;
                    return Ok(json!({"handled": true}));
                }
                _ => {}
            }
        }
        if !editing && !has_shapes && !mods.cmd {
            let r = match k {
                "pagedown" | "down" | "right" => Some("slide.next"),
                "pageup" | "up" | "left" => Some("slide.previous"),
                "home" => Some("slide.first"),
                "end" => Some("slide.last"),
                "enter" | "return" => Some("slide.new"),
                "tab" => {
                    self.cycle_selection(!mods.shift)?;
                    return Ok(json!({"handled": true}));
                }
                _ => None,
            };
            if let Some(id) = r {
                self.execute(id, &json!({}))?;
                return Ok(json!({"handled": true}));
            }
        }
        // Shortcuts from the registry.
        let key_name = match k {
            "enter" | "return" => "Enter",
            "escape" | "esc" => "Escape",
            "delete" => "Delete",
            "backspace" => "Backspace",
            "pageup" => "PageUp",
            "pagedown" => "PageDown",
            "home" => "Home",
            "end" => "End",
            "tab" => "Tab",
            "space" | " " => "Space",
            _ => key,
        };
        let found = crate::command_specs().iter().find(|c| c.shortcut.is_some_and(|s| shortcut_matches(s, key_name, mods))).map(|c| c.id);
        if let Some(id) = found {
            let r = self.execute(id, &json!({}))?;
            return Ok(json!({"handled": true, "command": id, "result": r}));
        }
        Ok(json!({"handled": false}))
    }

    /// Type text: into the text being edited, or (a shape selected) start editing it.
    ///
    /// One character typed goes on with the run of typing under way (one undo step, WPS W-H1);
    /// more at once or a line break is a step of its own and ends the run. So is what an input
    /// method commits while composing ([`crate::ime`], W-R2), however long: it ends the
    /// composition, and the typing after it is a step of its own.
    pub fn type_text(&mut self, text: &str) -> Result<Value> {
        let committed = self.end_preedit();
        let mut chars = text.chars();
        let one = chars.next().is_some_and(|c| !matches!(c, '\n' | '\r' | '\u{b}')) && chars.next().is_none();
        let r = if committed {
            self.end_run();
            let r = self.type_into(text);
            self.end_run();
            r
        } else if one {
            self.in_run(RunKind::Typing, |s| s.type_into(text))
        } else {
            self.end_run();
            self.type_into(text)
        };
        self.sync_all_kdm();
        r
    }

    fn type_into(&mut self, text: &str) -> Result<Value> {
        let st = self.doc()?;
        if st.selection.text.is_none() {
            let Some(id) = st.typing_target() else { return Ok(json!({"handled": false})) };
            // Typing on a selected shape goes into its text where its caret last was, keeping the
            // text (WPS W-F5); at its end when it was never edited.
            let at = st.last_caret(id);
            self.execute(
                "text.edit",
                &match at {
                    Some(at) => json!({"id": id, "at": [at.0, at.1]}),
                    None => json!({"id": id}),
                },
            )?;
        }
        self.execute("text.insert", &json!({"text": text}))
    }

    fn cycle_selection(&mut self, forward: bool) -> Result<()> {
        let st = self.doc()?;
        let ids: Vec<_> = st.shapes().iter().filter(|s| !s.hidden).map(|s| s.id).collect();
        if ids.is_empty() {
            return Ok(());
        }
        let cur = st.selection.shapes.first().and_then(|c| ids.iter().position(|x| x == c));
        let n = ids.len();
        let next = match cur {
            Some(i) if forward => (i + 1) % n,
            Some(i) => (i + n - 1) % n,
            None if forward => 0,
            None => n - 1,
        };
        let id = ids.get(next).copied();
        self.select(|_, sel| {
            sel.shapes = id.into_iter().collect();
            sel.text = None;
        })
    }

    /// Arrow keys in a table: from a cell range they move (Shift: grow the range) one cell; while
    /// a cell's text is edited they leave the cell only at its edge, as PowerPoint does (Shift:
    /// the range then covers both cells). Returns whether the key was taken.
    fn table_arrow(&mut self, dir: &str, shift: bool) -> Result<bool> {
        let st = self.doc()?;
        let (id, from, active) = match (&st.selection.text, st.selection.cells) {
            (Some(t), _) if !t.notes => match t.cell {
                Some(cell) => (t.shape, cell, cell),
                None => return Ok(false),
            },
            (None, Some((id, a, b))) => (id, a, b),
            _ => return Ok(false),
        };
        let Some(kition_slide_model::ShapeKind::Table(tb)) = st.shape(id).map(|s| s.kind.clone()) else { return Ok(false) };
        if let Some(t) = st.selection.text.clone() {
            // Inside the text first; only a key that can't move the caret leaves the cell.
            let to = match dir {
                "left" | "right" | "up" | "down" => dir,
                _ => return Ok(false),
            };
            self.execute("text.move", &json!({"to": to, "extend": shift}))?;
            let moved = self.active().and_then(|d| d.selection.text.as_ref()).is_none_or(|now| now.caret != t.caret || now.anchor != t.anchor);
            if moved {
                return Ok(true);
            }
        }
        let Some(next) = step_cell(&tb, active, dir) else { return Ok(true) };
        if shift {
            self.execute("table.selectCells", &json!({"id": id, "from": from, "to": next}))?;
            self.select(|_, sel| sel.text = None)?;
        } else {
            // Moving back (left, up) lands at the end of the cell's text, forward at its start.
            let back = matches!(dir, "left" | "up");
            self.execute("text.edit", &json!({"id": id, "cell": [next.0, next.1]}))?;
            if !back {
                self.execute("text.move", &json!({"to": "start"}))?;
            }
        }
        Ok(true)
    }

    fn next_cell(&mut self, back: bool) -> Result<Value> {
        let st = self.doc()?;
        let Some(t) = st.selection.text.clone() else { return Ok(Value::Null) };
        let Some((r, c)) = t.cell else { return Ok(Value::Null) };
        let Some(kition_slide_model::ShapeKind::Table(tb)) = st.shape(t.shape).map(|s| s.kind.clone()) else { return Ok(Value::Null) };
        let cells: Vec<_> = tb
            .rows
            .iter()
            .enumerate()
            .flat_map(|(r, row)| row.cells.iter().enumerate().filter(|(_, cell)| !cell.is_covered()).map(move |(c, _)| (r, c)))
            .collect();
        let current = tb.cell_owner(r, c).unwrap_or((r, c));
        let index = cells.iter().position(|cell| *cell == current).unwrap_or(0);
        let next = if back { cells.get(index.saturating_sub(1)).copied() } else { cells.get(index + 1).copied() };
        let next = match next {
            Some(cell) => cell,
            None if !back && tb.n_rows() > 0 => {
                self.execute("table.insertRowBelow", &json!({"id": t.shape, "row": tb.n_rows() - 1}))?;
                (tb.n_rows(), 0)
            }
            _ => return Ok(Value::Null),
        };
        self.execute("text.edit", &json!({"id": t.shape, "cell": [next.0, next.1]}))?;
        self.execute("text.selectAll", &json!({}))
    }
}

/// The visible cell next to `at` in direction `dir`, stepping over merged cells.
fn step_cell(t: &kition_slide_model::Table, at: (usize, usize), dir: &str) -> Option<(usize, usize)> {
    let (r, c) = t.cell_owner(at.0, at.1)?;
    let cell = t.cell(r, c)?;
    let (rows, cols) = (cell.row_span.max(1) as usize, cell.grid_span.max(1) as usize);
    // Keep the coordinate the caret came from on the other axis.
    let target = match dir {
        "left" => (at.0, c.checked_sub(1)?),
        "right" => (at.0, c.checked_add(cols)?),
        "up" => (r.checked_sub(1)?, at.1),
        "down" => (r.checked_add(rows)?, at.1),
        _ => return None,
    };
    t.cell_owner(target.0, target.1)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn shortcut_parsing() {
        let m = |shift, alt, cmd| Mods { shift, alt, cmd };
        assert!(shortcut_matches("Cmd+B", "b", m(false, false, true)));
        assert!(!shortcut_matches("Cmd+B", "b", m(true, false, true)));
        assert!(shortcut_matches("Cmd+Shift+]", "]", m(true, false, true)));
        assert!(shortcut_matches("Cmd+Shift+.", ".", m(true, false, true)));
        assert!(shortcut_matches("Delete", "Delete", m(false, false, false)));
    }

    /// One key, one command: the frame and the engine take a shortcut's first command, so a key
    /// two commands claim runs only one of them (walkthrough J2: Cmd+Shift+V was both Paste and
    /// Match Formatting and Paste Formatting, and the Format Painter never got it).
    #[test]
    fn no_two_commands_share_a_shortcut() {
        let mut seen = std::collections::BTreeMap::new();
        for spec in crate::command_specs() {
            let Some(shortcut) = spec.shortcut else { continue };
            let mut parts: Vec<String> = shortcut.split('+').map(str::to_ascii_lowercase).collect();
            parts.sort();
            if let Some(other) = seen.insert(parts.join("+"), spec.id) {
                panic!("{shortcut} is the shortcut of both {other} and {}", spec.id);
            }
        }
    }

    #[test]
    fn cmd_shift_v_paints_and_cmd_alt_shift_v_pastes_plain_text() {
        let mut s = Session::with_new();
        let title = s.doc().unwrap().current_slide().unwrap().shapes[0].id.0;
        s.execute("edit.select", &json!({"ids": [title]})).unwrap();
        s.execute("format.painter", &json!({})).unwrap();
        let painted = s.key("v", Mods { cmd: true, shift: true, ..Default::default() }).unwrap();
        assert_eq!(painted["command"], "format.painterApply");
        let pasted = s.key("v", Mods { cmd: true, shift: true, alt: true }).unwrap();
        assert_eq!(pasted["command"], "edit.pasteText");
    }

    /// Cut with only a caret in the text cuts nothing (it deleted the next character).
    #[test]
    fn cut_with_only_a_caret_leaves_the_text() {
        let mut s = Session::with_new();
        let title = s.doc().unwrap().current_slide().unwrap().shapes[0].id.0;
        s.execute("edit.select", &json!({"ids": [title]})).unwrap();
        s.type_text("Hi").unwrap();
        s.key("Home", Mods::default()).unwrap();
        let cut = s.execute("edit.cut", &json!({})).unwrap();
        assert_eq!(cut["text"], "");
        assert_eq!(s.execute("text.get", &json!({"id": title})).unwrap()["text"], "Hi");
        s.key("Right", Mods { shift: true, ..Default::default() }).unwrap();
        assert_eq!(s.execute("edit.cut", &json!({})).unwrap()["text"], "H");
        assert_eq!(s.execute("text.get", &json!({"id": title})).unwrap()["text"], "i");
    }

    #[test]
    fn keys_drive_editing() {
        let mut s = Session::with_new();
        let title = s.doc().unwrap().current_slide().unwrap().shapes[0].id.0;
        s.execute("edit.select", &json!({"ids": [title]})).unwrap();
        s.type_text("Hi").unwrap();
        s.key("Enter", Mods::default()).unwrap();
        s.type_text("there").unwrap();
        s.key("Backspace", Mods::default()).unwrap();
        s.key("Escape", Mods::default()).unwrap();
        let t = s.execute("text.get", &json!({"id": title})).unwrap();
        assert_eq!(t["text"], "Hi\nther");
        let r = s.key("b", Mods { cmd: true, ..Default::default() }).unwrap();
        assert_eq!(r["command"], "format.bold");
        s.key("Right", Mods::default()).unwrap();
        s.key("Delete", Mods::default()).unwrap();
        assert_eq!(s.doc().unwrap().current_slide().unwrap().shapes.len(), 1);
    }
}
