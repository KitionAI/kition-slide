//! Review tab: comments, spelling, accessibility; Slide Show requests.

use std::sync::Arc;

use kition_slide_model::Comment;
use serde_json::{Value, json};

use super::*;
use crate::{Result, Session, UiRequest};

pub fn specs() -> Vec<CommandSpec> {
    vec![
        cmd!(
            "comment.add",
            "New Comment",
            ["Review", "Comments"],
            Some("Cmd+Alt+M"),
            "{text, slide?: index, id?: shape, x?, y?, author?: principal, name?, created?} → {index, thread}",
            has_slide,
            add
        ),
        cmd!("comment.reply", "Reply", ["Comments"], None, "{thread | index, text, author?, name?, created?} → {comment}", has_slide, reply),
        cmd!("comment.delete", "Delete Comment", ["Review", "Comments"], None, "{thread, comment?} | {index?} | {all: true}", has_slide, delete),
        cmd!("comment.resolve", "Resolve Thread", ["Comments"], None, "{thread | index, resolved?: bool}", has_slide, resolve),
        cmd!(query "comment.list", "Comments", [], None, "{slide?} → comments", has_slide, list),
        cmd!(
            query "comment.threads",
            "Comment Threads",
            [],
            None,
            "{} → [{id, slide, anchor: {object, slide, x, y, shape?, box?}, resolved, comments: [{id, author, name, body, created}]}]",
            has_doc,
            threads
        ),
        cmd!(query "accessibility.check", "Check Accessibility", ["Review", "Accessibility"], None, "{} → [{slide, shape, issue}]", has_doc, accessibility),
        cmd!(query "spelling.check", "Spelling", ["Review", "Proofing"], Some("F7"), "{} → [{slide, shape, word}] (words not in the built-in list)", has_doc, spelling),
        cmd!(noundo "show.fromStart", "Play from Start", ["Slide Show", "Start Slide Show"], Some("Cmd+Shift+Enter"), "{}", has_slide, |s, _| start(s, 0)),
        cmd!(noundo "show.fromCurrent", "Play from Current Slide", ["Slide Show", "Start Slide Show"], Some("Cmd+Enter"), "{}", has_slide, |s, _| {
            let i = s.doc()?.selection.slide;
            start(s, i)
        }),
        cmd!(
            "show.setup",
            "Set Up Slide Show…",
            ["Slide Show", "Set Up"],
            None,
            "{type?: speaker|browsed|kiosk, loop?: bool, noNarration?, noAnimation?, useTimings?, from?, to?, all?: bool (every slide, instead of from and to)}",
            has_doc,
            setup
        ),
        cmd!(
            "show.customShow",
            "Custom Slide Show",
            ["Slide Show", "Start Slide Show"],
            None,
            "{name, slides: [index]} | {name, delete: true}",
            has_doc,
            custom_show
        ),
    ]
}

/// A fresh comment id: a KDM object id (UUID v7), unique without coordination.
pub fn new_comment_id() -> String {
    kdm_model::kition_kit::kdm::ObjectId::new().as_str().to_owned()
}

/// Now, as ISO 8601 UTC. In WebAssembly (no system clock) the time a fresh UUID v7 carries.
pub fn now() -> String {
    #[cfg(not(target_arch = "wasm32"))]
    let millis = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_millis() as i64).unwrap_or(0);
    #[cfg(target_arch = "wasm32")]
    let millis = new_comment_id().chars().filter(char::is_ascii_hexdigit).take(12).collect::<String>();
    #[cfg(target_arch = "wasm32")]
    let millis = i64::from_str_radix(&millis, 16).unwrap_or(0);
    let secs = millis.div_euclid(1000);
    let (y, m, d) = super::design::civil(secs.div_euclid(86_400));
    let t = secs.rem_euclid(86_400);
    format!("{y:04}-{m:02}-{d:02}T{:02}:{:02}:{:02}.{:03}Z", t / 3600, t / 60 % 60, t % 60, millis.rem_euclid(1000))
}

/// Whether a person who may only comment can run `spec`: the comment commands, undo and redo
/// (their own steps), and whatever leaves the slide as it is (queries, views, the
/// selection, file outputs).
pub(crate) fn comment_only_allows(spec: &CommandSpec) -> bool {
    const OUTPUTS: &[&str] = &["file.saveBytes", "file.render", "file.export", "file.exportBytes", "file.recovery.list", "file.inspect"];
    if spec.id.starts_with("comment.") || matches!(spec.id, "edit.undo" | "edit.redo") {
        return true;
    }
    if spec.id.starts_with("file.") {
        return OUTPUTS.contains(&spec.id);
    }
    !spec.undoable
}

/// Who writes: `author` (a principal) and `name`, else the person the preferences name. A
/// person who may only comment always writes as themselves.
fn writer(s: &Session, p: &Value) -> (String, String) {
    let name = str_param(p, "name").filter(|n| !n.is_empty()).map(str::to_owned).unwrap_or_else(|| s.prefs.author.clone());
    let principal = match &s.comment_only {
        Some(me) if !me.is_empty() => me.clone(),
        _ => str_param(p, "author").filter(|a| !a.is_empty()).map(str::to_owned).unwrap_or_default(),
    };
    let principal = if principal == name { String::new() } else { principal };
    (name, principal)
}

fn created(p: &Value) -> String {
    str_param(p, "created").filter(|c| !c.is_empty()).map(str::to_owned).unwrap_or_else(now)
}

fn initials_of(name: &str) -> String {
    name.split_whitespace().filter_map(|w| w.chars().next()).collect()
}

/// The slide and index of thread `id` (a thread's id, or `@<slide>:<index>` for one that has none
/// yet); else the current slide's thread `index`.
fn find_thread(s: &Session, p: &Value, cmd: &str) -> Result<(usize, usize)> {
    let st = s.doc()?;
    if let Some(id) = str_param(p, "thread") {
        for (si, slide) in st.doc.slides.iter().enumerate() {
            if let Some(ti) = slide.comments.iter().position(|c| c.id == id) {
                return Ok((si, ti));
            }
        }
        if let Some((si, ti)) = id.strip_prefix('@').and_then(|rest| rest.split_once(':'))
            && let (Ok(si), Ok(ti)) = (si.parse::<usize>(), ti.parse::<usize>())
            && st.doc.slides.get(si).is_some_and(|slide| ti < slide.comments.len())
        {
            return Ok((si, ti));
        }
        return Err(bad(cmd, "no such comment thread"));
    }
    let ti = usize_param(p, "index").ok_or_else(|| bad(cmd, "missing `thread`"))?;
    let si = st.selection.slide;
    if st.doc.slides.get(si).is_some_and(|slide| ti < slide.comments.len()) { Ok((si, ti)) } else { Err(bad(cmd, "no such comment")) }
}

/// A person who may only comment cannot change a thread the shared replica does not hold yet (one
/// an older build kept on its slide, until an editor's edit brings it over).
fn check_shared(s: &Session, si: usize, ti: usize, cmd: &str) -> Result<()> {
    if s.comment_only.is_none() {
        return Ok(());
    }
    let st = s.doc()?;
    let Some(kdm) = st.kdm() else { return Ok(()) };
    let id = st.doc.slides.get(si).and_then(|slide| slide.comments.get(ti)).map(|c| c.id.clone()).unwrap_or_default();
    if kdm.replica().exists(&kdm_model::kition_kit::kdm::ObjectId::from_string(id)) {
        Ok(())
    } else {
        Err(bad(cmd, "this comment thread is read-only until an editor opens the slide"))
    }
}

fn add(s: &mut Session, p: &Value) -> Result<Value> {
    let text = str_param(p, "text").ok_or_else(|| bad("comment.add", "missing `text`"))?.to_string();
    let (author, author_id) = writer(s, p);
    let date = created(p);
    let target = usize_param(p, "slide");
    let named = id_param(p, "id");
    let shape = named.or_else(|| if target.is_some() { None } else { s.active().and_then(|d| d.selection.shapes.first().copied()) });
    let (x, y) = (f64_or(p, "x", 10.0), f64_or(p, "y", 10.0));
    let (thread, head) = (new_comment_id(), new_comment_id());
    let id = thread.clone();
    s.edit(|doc, sel| {
        let index = target.unwrap_or(sel.slide);
        let sl = Arc::make_mut(doc.slides.get_mut(index).ok_or_else(|| bad("comment.add", "no slide"))?);
        // A shape the call names is on this slide, or the call is refused: dropping the anchor
        // silently would put the comment somewhere the caller did not say (C8-slide-read-3).
        if let Some(id) = named.filter(|id| sl.shape(*id).is_none()) {
            return Err(bad("comment.add", format!("slide {index} (counted from 0) has no shape {}", id.0)));
        }
        let shape = shape.filter(|id| sl.shape(*id).is_some());
        let initials = initials_of(&author);
        sl.comments.push(Comment { author, author_id, initials, text, date, x, y, shape, id, head, ..Default::default() });
        Ok(json!({"index": sl.comments.len() - 1, "thread": thread}))
    })
}

fn reply(s: &mut Session, p: &Value) -> Result<Value> {
    let (si, ti) = find_thread(s, p, "comment.reply")?;
    check_shared(s, si, ti, "comment.reply")?;
    let text = str_param(p, "text").unwrap_or("").to_string();
    let (author, author_id) = writer(s, p);
    let date = created(p);
    let id = new_comment_id();
    let answer = id.clone();
    s.edit(|doc, _| {
        let sl = Arc::make_mut(doc.slides.get_mut(si).ok_or_else(|| bad("comment.reply", "no slide"))?);
        let c = sl.comments.get_mut(ti).ok_or_else(|| bad("comment.reply", "no such comment"))?;
        let initials = initials_of(&author);
        c.replies.push(Comment { author, author_id, initials, text, date, id, ..Default::default() });
        Ok(())
    })?;
    Ok(json!({"comment": answer}))
}

/// Whether `c` (a thread's first comment or a reply) was written by `me`.
fn written_by(c: &Comment, me: &str) -> bool {
    c.author_id == me || (c.author_id.is_empty() && c.author == me)
}

fn delete(s: &mut Session, p: &Value) -> Result<Value> {
    if str_param(p, "thread").is_some() {
        let (si, ti) = find_thread(s, p, "comment.delete")?;
        check_shared(s, si, ti, "comment.delete")?;
        let comment = str_param(p, "comment").map(str::to_owned);
        let me = s.comment_only.clone();
        return s
            .edit(|doc, _| {
                let sl = Arc::make_mut(doc.slides.get_mut(si).ok_or_else(|| bad("comment.delete", "no slide"))?);
                let thread = sl.comments.get_mut(ti).ok_or_else(|| bad("comment.delete", "no such comment"))?;
                let reply = comment.as_deref().and_then(|id| thread.replies.iter().position(|r| r.id == id));
                let first = comment.as_deref().is_none_or(|id| id == thread.head || id == thread.id);
                if reply.is_none() && !first {
                    return Err(bad("comment.delete", "no such comment"));
                }
                let mine = |c: &Comment| me.as_deref().is_none_or(|me| written_by(c, me));
                match reply {
                    Some(r) => {
                        if !thread.replies.get(r).is_some_and(mine) {
                            return Err(bad("comment.delete", "only its author may delete a comment"));
                        }
                        thread.replies.remove(r);
                    }
                    // The thread's first comment: its first reply opens the thread from now on, or the
                    // thread goes with its last comment.
                    None if comment.is_some() && !thread.replies.is_empty() => {
                        if !mine(thread) {
                            return Err(bad("comment.delete", "only its author may delete a comment"));
                        }
                        let next = thread.replies.remove(0);
                        thread.head = next.id;
                        thread.author = next.author;
                        thread.author_id = next.author_id;
                        thread.initials = next.initials;
                        thread.text = next.text;
                        thread.date = next.date;
                        thread.kept = next.kept;
                    }
                    None => {
                        if !mine(thread) || !thread.replies.iter().all(mine) {
                            return Err(bad("comment.delete", "only its author may delete a comment"));
                        }
                        sl.comments.remove(ti);
                    }
                }
                Ok(())
            })
            .and_then(|()| ok());
    }
    if s.comment_only.is_some() {
        return Err(bad("comment.delete", "name the comment to delete"));
    }
    let all = bool_or(p, "all", false);
    let i = usize_param(p, "index");
    s.edit(|doc, sel| {
        if all {
            for sl in &mut doc.slides {
                Arc::make_mut(sl).comments.clear();
            }
            return Ok(());
        }
        let sl = Arc::make_mut(doc.slides.get_mut(sel.slide).ok_or_else(|| bad("comment.delete", "no slide"))?);
        match i {
            Some(i) if i < sl.comments.len() => {
                sl.comments.remove(i);
            }
            None => sl.comments.clear(),
            _ => return Err(bad("comment.delete", "no such comment")),
        }
        Ok(())
    })?;
    ok()
}

fn resolve(s: &mut Session, p: &Value) -> Result<Value> {
    let (si, ti) = find_thread(s, p, "comment.resolve")?;
    check_shared(s, si, ti, "comment.resolve")?;
    let v = bool_or(p, "resolved", true);
    s.edit(|doc, _| {
        let sl = Arc::make_mut(doc.slides.get_mut(si).ok_or_else(|| bad("comment.resolve", "no slide"))?);
        sl.comments.get_mut(ti).ok_or_else(|| bad("comment.resolve", "no such comment"))?.resolved = v;
        Ok(())
    })?;
    ok()
}

fn list(s: &mut Session, p: &Value) -> Result<Value> {
    let st = s.doc()?;
    let i = usize_param(p, "slide").unwrap_or(st.selection.slide);
    Ok(serde_json::to_value(st.doc.slides.get(i).map(|x| x.comments.clone()).unwrap_or_default()).unwrap_or_default())
}

/// One comment as the M23 convention reads it: `author` the principal (the file's author name for
/// a comment from a file), `name` the display name.
fn comment_json(c: &Comment, id: &str) -> Value {
    let author = if c.author_id.is_empty() { c.author.clone() } else { c.author_id.clone() };
    json!({"id": id, "author": author, "name": c.author, "body": c.text, "created": c.date})
}

/// Every slide's comment threads, oldest first per slide: where each is (its slide's index, the
/// point on the slide in points, and its shape's box), whether it is resolved, and its comments.
fn threads(s: &mut Session, _p: &Value) -> Result<Value> {
    let st = s.doc()?;
    let mut out = Vec::new();
    for (si, slide) in st.doc.slides.iter().enumerate() {
        for (ti, c) in slide.comments.iter().enumerate() {
            let id = if c.id.is_empty() { format!("@{si}:{ti}") } else { c.id.clone() };
            let shape = c.shape.and_then(|id| slide.shape(id));
            let mut anchor = json!({"object": match shape { Some(sh) => super::inspect::shape_tool_id(sh), None => format!("slide:{}", slide.id.0) }, "slide": si, "x": c.x, "y": c.y});
            if let (Some(sh), Some(map)) = (shape, anchor.as_object_mut()) {
                map.insert("shape".into(), json!(sh.id.0));
                if let Some(x) = &sh.xfrm {
                    map.insert("box".into(), json!({"x": x.x, "y": x.y, "width": x.w, "height": x.h}));
                }
            }
            let head = if c.head.is_empty() { format!("{id}:0") } else { c.head.clone() };
            let mut comments = vec![comment_json(c, &head)];
            for (ri, r) in c.replies.iter().enumerate() {
                comments.push(comment_json(r, &if r.id.is_empty() { format!("{id}:{}", ri + 1) } else { r.id.clone() }));
            }
            out.push(json!({"id": id, "slide": si, "anchor": anchor, "resolved": c.resolved, "comments": comments}));
        }
    }
    Ok(Value::Array(out))
}

fn accessibility(s: &mut Session, _p: &Value) -> Result<Value> {
    let st = s.doc()?;
    let mut out = vec![];
    for (i, sl) in st.doc.slides.iter().enumerate() {
        if sl.title().trim().is_empty() {
            out.push(json!({"slide": i, "issue": "Missing slide title"}));
        }
        kition_slide_model::walk(&sl.shapes, &mut |sh, _| {
            let needs_alt = matches!(
                sh.kind,
                kition_slide_model::ShapeKind::Picture { .. }
                    | kition_slide_model::ShapeKind::Chart(_)
                    | kition_slide_model::ShapeKind::Media(_)
                    | kition_slide_model::ShapeKind::Group { .. }
            );
            if needs_alt && sh.descr.trim().is_empty() && !sh.decorative {
                out.push(json!({"slide": i, "shape": sh.id, "name": sh.name, "issue": "Missing alternative text"}));
            }
        });
        // Low-contrast text against the slide background.
        if let Some(ctx) = kition_slide_model::resolve::Ctx::for_slide(&st.doc, sl) {
            let bg = match kition_slide_model::resolve::background(&ctx, Some(sl)) {
                (kition_slide_model::Fill::Solid { color }, ph) => Some(ctx.color(&color, ph)),
                _ => None,
            };
            if let Some(bg) = bg {
                for sh in &sl.shapes {
                    let Some(t) = &sh.text else { continue };
                    let Some(para) = t.paragraphs.iter().find(|p| !p.is_empty()) else { continue };
                    let rp = kition_slide_model::resolve::run(&ctx, sh, para, &para.runs.first().map(|r| r.props.clone()).unwrap_or_default());
                    let (fill, _) = kition_slide_model::resolve::fill(&ctx, sh);
                    if fill.as_ref().is_some_and(|f| !f.is_none()) {
                        continue;
                    }
                    let fg = kition_slide_model::resolve::text_color(&ctx, &rp);
                    if fg.contrast(bg) < 3.0 {
                        out.push(json!({"slide": i, "shape": sh.id, "name": sh.name, "issue": format!("Hard-to-read text contrast ({:.1}:1)", fg.contrast(bg))}));
                    }
                }
            }
        }
    }
    Ok(Value::Array(out))
}

fn spelling(s: &mut Session, _p: &Value) -> Result<Value> {
    // A small built-in list of frequent misspellings (full dictionaries come later).
    const COMMON: &[&str] = &[
        "teh",
        "recieve",
        "seperate",
        "occured",
        "untill",
        "wich",
        "becuase",
        "definately",
        "thier",
        "alot",
        "accomodate",
        "acheive",
        "adress",
        "begining",
        "beleive",
        "calender",
        "commited",
        "enviroment",
        "goverment",
        "independant",
        "neccessary",
        "occassion",
        "publically",
        "reccomend",
        "tommorow",
        "wierd",
    ];
    let st = s.doc()?;
    let mut out = vec![];
    for (i, sl) in st.doc.slides.iter().enumerate() {
        kition_slide_model::walk(&sl.shapes, &mut |sh, _| {
            if let Some(t) = &sh.text {
                for w in t.text().split(|c: char| !c.is_alphanumeric() && c != '\'') {
                    if COMMON.contains(&w.to_lowercase().as_str()) {
                        out.push(json!({"slide": i, "shape": sh.id, "word": w}));
                    }
                }
            }
        });
    }
    Ok(Value::Array(out))
}

fn start(s: &mut Session, from: usize) -> Result<Value> {
    s.ui_requests.push(UiRequest::StartShow { from });
    Ok(json!({"from": from}))
}

fn setup(s: &mut Session, p: &Value) -> Result<Value> {
    s.edit(|doc, _| {
        let sh = &mut doc.show;
        if let Some(t) = str_param(p, "type") {
            sh.show_type = t.to_string();
            if t == "kiosk" {
                sh.loop_until_esc = true;
            }
        }
        if let Some(v) = bool_param(p, "loop") {
            sh.loop_until_esc = v;
        }
        if let Some(v) = bool_param(p, "noNarration") {
            sh.without_narration = v;
        }
        if let Some(v) = bool_param(p, "noAnimation") {
            sh.without_animation = v;
        }
        if let Some(v) = bool_param(p, "useTimings") {
            sh.use_timings = v;
        }
        match (p.get("from").and_then(Value::as_u64), p.get("to").and_then(Value::as_u64)) {
            (Some(a), Some(b)) => sh.range = Some((a.min(u32::MAX as u64) as u32, b.min(u32::MAX as u64) as u32)),
            (None, None) if p.get("all").is_some() => sh.range = None,
            _ => {}
        }
        Ok(serde_json::to_value(&*sh).unwrap_or_default())
    })
}

fn custom_show(s: &mut Session, p: &Value) -> Result<Value> {
    let name = str_param(p, "name").ok_or_else(|| bad("show.customShow", "missing `name`"))?.to_string();
    let delete = bool_or(p, "delete", false);
    let idx: Vec<usize> =
        p.get("slides").and_then(Value::as_array).map(|a| a.iter().filter_map(Value::as_u64).map(|v| v as usize).collect()).unwrap_or_default();
    s.edit(|doc, _| {
        doc.custom_shows.retain(|c| c.name != name);
        if !delete {
            let slides = idx.iter().filter_map(|i| doc.slides.get(*i).map(|x| x.id)).collect();
            doc.custom_shows.push(kition_slide_model::CustomShow { name: name.clone(), slides });
        }
        Ok(())
    })?;
    ok()
}
