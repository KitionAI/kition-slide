//! Every command declares the editor state it reads (`KitCommand::reads`): what the engine's code
//! reads, found by reading it. Each command's `enabled` predicate and `run` function (or closure)
//! are followed through the engine's functions and the commands it runs (`execute("…")`) to the
//! accessors of the selection and of the current slide; a command that reaches one and does not
//! declare it, or declares what it never reads, fails here (task card C4-slide). A name bound to a
//! local closure (`let set = |…|`) calls that closure, not an engine function of the same name; a
//! `crate::<module>::f` call is that module's `f`; a command run with explicit `ids` (or `id`), and
//! a function that refuses to run without them, do not read the selection that the targets would
//! fall back to without them. To see how the scan reached a reading, set
//! `ACTS_DEBUG=<command id>` or `ACTS_FN=<engine function>`.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

use kition_kit::{EditorState, KitDocument};
use kition_slide_kit::SlideDocument;

/// Code that reads the selection: the selected shapes or slides, the text being edited, the table
/// cells, and the helpers that hand them out.
const SELECTION: &[&str] = &[
    "selection.shapes",
    "selection.text",
    "selection.cells",
    "selection.slides",
    "sel.shapes",
    "sel.text",
    "sel.cells",
    "sel.slides",
    ".selected_shapes(",
];
/// Code that reads the current part: the current slide (or the master or layout in Slide Master
/// view) and the shapes of the tree being edited.
const PART: &[&str] = &[
    "selection.slide",
    "sel.slide",
    "selection.target",
    "sel.target",
    ".current_slide(",
    "current_shapes(",
    "shapes_mut(",
    ".shapes()",
    ".shape(",
    ".selected_shapes(",
];
/// `enabled` predicates that gate on the deck alone (open, has slides, has history), not on what
/// is current or selected.
const DOCUMENT_GATES: &[&str] = &["always", "has_doc", "has_slide", "can_undo", "can_redo"];

fn engine_src() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../crates/engine/engine/src")
}

fn rust_files(dir: &Path, out: &mut Vec<PathBuf>) {
    for entry in std::fs::read_dir(dir).unwrap() {
        let path = entry.unwrap().path();
        if path.is_dir() {
            rust_files(&path, out);
        } else if path.extension().is_some_and(|e| e == "rs")
            && !path.file_name().is_some_and(|n| n == "tests.rs" || n.to_string_lossy().ends_with("_tests.rs"))
        {
            out.push(path);
        }
    }
}

/// `source` with every comment, string and character literal blanked (to spaces), so brackets and
/// names are only the code's; same length, byte for byte.
fn code_only(source: &str) -> Vec<u8> {
    let b = source.as_bytes();
    let mut out = b.to_vec();
    let mut i = 0;
    let blank = |out: &mut Vec<u8>, from: usize, to: usize| {
        for byte in out.iter_mut().take(to).skip(from) {
            *byte = b' ';
        }
    };
    while i < b.len() {
        let ident_before = i > 0 && (b[i - 1].is_ascii_alphanumeric() || b[i - 1] == b'_');
        if b[i..].starts_with(b"//") {
            let end = b[i..].iter().position(|&c| c == b'\n').map_or(b.len(), |n| i + n);
            blank(&mut out, i, end);
            i = end;
        } else if b[i..].starts_with(b"/*") {
            let end = source[i + 2..].find("*/").map_or(b.len(), |n| i + 2 + n + 2);
            blank(&mut out, i, end);
            i = end;
        } else if !ident_before && (b[i] == b'r' || b[i..].starts_with(b"br")) && {
            let start = if b[i] == b'b' { i + 2 } else { i + 1 };
            let hashes = b[start..].iter().take_while(|&&c| c == b'#').count();
            b.get(start + hashes) == Some(&b'"')
        } {
            let start = if b[i] == b'b' { i + 2 } else { i + 1 };
            let hashes = b[start..].iter().take_while(|&&c| c == b'#').count();
            let close = format!("\"{}", "#".repeat(hashes));
            let body = start + hashes + 1;
            let end = source[body..].find(&close).map_or(b.len(), |n| body + n + close.len());
            blank(&mut out, i, end);
            i = end;
        } else if b[i] == b'"' {
            let mut j = i + 1;
            while j < b.len() && b[j] != b'"' {
                j += if b[j] == b'\\' { 2 } else { 1 };
            }
            blank(&mut out, i, (j + 1).min(b.len()));
            i = j + 1;
        } else if b[i] == b'\'' {
            // A character literal ('a', '\n', '{', 'é'), not a lifetime ('a).
            let rest = &source[i + 1..];
            let len = if let Some(escaped) = rest.strip_prefix('\\') {
                escaped.find('\'').map(|n| n + 2)
            } else {
                rest.chars().next().map(char::len_utf8).filter(|&n| rest[n..].starts_with('\''))
            };
            match len {
                Some(len) => {
                    blank(&mut out, i, i + len + 2);
                    i += len + 2;
                }
                None => i += 1,
            }
        } else {
            i += 1;
        }
    }
    out
}

/// The position after the bracket matching the one at `open`.
fn matching(code: &[u8], open: usize) -> usize {
    let (left, right) = match code[open] {
        b'(' => (b'(', b')'),
        b'{' => (b'{', b'}'),
        _ => (b'[', b']'),
    };
    let mut depth = 0usize;
    for (at, &c) in code.iter().enumerate().skip(open) {
        if c == left {
            depth += 1;
        } else if c == right {
            depth -= 1;
            if depth == 0 {
                return at + 1;
            }
        }
    }
    code.len()
}

/// The source cut before its `#[cfg(test)]` module, the code of what is left, and its file.
struct File {
    source: String,
    code: Vec<u8>,
    name: String,
}

fn files() -> Vec<File> {
    let mut paths = Vec::new();
    rust_files(&engine_src(), &mut paths);
    paths.sort();
    paths
        .into_iter()
        .map(|path| {
            let mut source = std::fs::read_to_string(&path).unwrap();
            if let Some(at) = source.find("#[cfg(test)]\nmod ") {
                source.truncate(at);
            }
            let code = code_only(&source);
            File { source, code, name: path.to_string_lossy().into_owned() }
        })
        .collect()
}

fn ident_at(code: &[u8], at: usize) -> &str {
    let end = code[at..].iter().position(|c| !(c.is_ascii_alphanumeric() || *c == b'_')).map_or(code.len(), |n| at + n);
    std::str::from_utf8(&code[at..end]).unwrap_or("")
}

/// What a piece of code reads directly, the free functions it calls (`crate::<module>::f` keeps its
/// path), the commands it runs (with whether it names their shapes by explicit `ids` or `id`), and
/// whether it refuses to run without `ids` or `id` (so what it calls never falls back to the
/// selection).
#[derive(Default, Clone)]
struct Reach {
    reads: BTreeSet<&'static str>,
    calls: BTreeSet<String>,
    runs: BTreeSet<(String, bool)>,
    named: bool,
}

/// Whether `code` returns an error when it is given neither `ids` nor `id`: an
/// `if <only conditions on p.get("ids") and p.get("id").is_none()> { return Err(…) }`.
fn requires_ids(code: &[u8], source: &str) -> bool {
    let flat = |from: usize, to: usize| -> String { source[from..to].chars().filter(|c| !c.is_whitespace()).collect() };
    let text = std::str::from_utf8(code).unwrap_or("");
    text.match_indices("if ").any(|(at, _)| {
        if at > 0 && (code[at - 1].is_ascii_alphanumeric() || code[at - 1] == b'_') {
            return false;
        }
        let Some(brace) = code[at..].iter().position(|&c| c == b'{' || c == b';').map(|n| at + n) else { return false };
        if code[brace] != b'{' {
            return false;
        }
        let condition = flat(at + 3, brace);
        let terms: Vec<&str> = condition.split("&&").collect();
        let only_ids = !condition.contains("||") && terms.iter().all(|t| t.contains("get(\"ids\")") || t.contains("get(\"id\")"));
        only_ids
            && condition.contains("get(\"ids\")")
            && condition.contains("get(\"id\").is_none()")
            && flat(brace, matching(code, brace)).starts_with("{returnErr(")
    })
}

/// The names `code` binds to closures (`let set = |…|`, `let mut at = move |…|`): calling one
/// runs the closure, whose code is scanned where it stands, not a function of the same name.
fn closures(code: &[u8]) -> BTreeSet<String> {
    let text = std::str::from_utf8(code).unwrap_or("");
    let mut out = BTreeSet::new();
    for (at, _) in text.match_indices("let ") {
        if at > 0 && (code[at - 1].is_ascii_alphanumeric() || code[at - 1] == b'_') {
            continue;
        }
        let rest = text[at + 4..].trim_start();
        let rest = rest.strip_prefix("mut ").map_or(rest, str::trim_start);
        let name = ident_at(rest.as_bytes(), 0);
        if name.is_empty() {
            continue;
        }
        let rest = rest[name.len()..].trim_start();
        // A type annotation (`let at: fn(..) = …`) is no closure literal; only `= |` or `= move |`.
        let Some(value) = rest.strip_prefix('=').filter(|_| !rest.starts_with("==")) else { continue };
        let value = value.trim_start();
        let value = value.strip_prefix("move").filter(|v| v.starts_with(|c: char| c.is_whitespace() || c == '|')).map_or(value, str::trim_start);
        if value.starts_with('|') {
            out.insert(name.to_owned());
        }
    }
    out
}

/// Whether the `execute` arguments `args` (source text) name the shapes by a non-empty `ids` list
/// or an `id`, so the command run acts on them and never on the selection.
fn names_ids(args: &str) -> bool {
    let flat: String = args.chars().filter(|c| !c.is_whitespace()).collect();
    let given = |key: &str, none: &str| flat.match_indices(key).any(|(at, key)| !flat[at + key.len()..].starts_with(none));
    given("\"ids\":", "[]") || given("\"id\":", "null")
}

/// The `crate::a::b::` path before the name at `at`, as `crate::a::b`, if the call is through `crate`.
fn crate_path(code: &[u8], at: usize) -> Option<String> {
    let mut start = at;
    while start >= 2 && &code[start - 2..start] == b"::" {
        let mut from = start - 2;
        while from > 0 && (code[from - 1].is_ascii_alphanumeric() || code[from - 1] == b'_') {
            from -= 1;
        }
        start = from;
    }
    let path = std::str::from_utf8(&code[start..at]).ok()?.strip_suffix("::")?;
    (path == "crate" || path.starts_with("crate::")).then(|| path.to_owned())
}

fn reach(code: &[u8], source: &str) -> Reach {
    let text = std::str::from_utf8(code).unwrap_or("");
    let mut found = Reach { named: requires_ids(code, source), ..Reach::default() };
    let local = closures(code);
    let has = |token: &str| {
        text.match_indices(token).any(|(at, _)| {
            let end = at + token.len();
            // `selection.slide` is not `selection.slides`.
            let next = code.get(end).copied().unwrap_or(b' ');
            let whole = token.ends_with('(') || token.ends_with(')') || !(next.is_ascii_alphanumeric() || next == b'_');
            // Setting it (`sel.shapes = vec![id]`, `sel.shapes.clear()`) is no read.
            let rest = text[end..].trim_start();
            let written = (rest.starts_with('=') && !rest.starts_with("=="))
                || [".clear()", ".push(", ".retain(", ".extend(", ".truncate(", ".dedup("].iter().any(|write| rest.starts_with(write));
            whole && !written
        })
    };
    if SELECTION.iter().any(|token| has(token)) {
        found.reads.insert("selection");
    }
    if PART.iter().any(|token| has(token)) {
        found.reads.insert("part");
    }
    for at in 0..code.len() {
        let starts =
            (code[at].is_ascii_alphabetic() || code[at] == b'_') && (at == 0 || !(code[at - 1].is_ascii_alphanumeric() || code[at - 1] == b'_'));
        if !starts {
            continue;
        }
        let name = ident_at(code, at);
        let after = at + name.len();
        let method = at > 0 && code[at - 1] == b'.';
        if code.get(after) == Some(&b'(') && !method && in_engine(code, at) {
            match crate_path(code, at) {
                Some(path) => {
                    found.calls.insert(format!("{path}::{name}"));
                }
                // An unqualified call to a local closure's name is the closure.
                None if local.contains(name) && !(at >= 2 && &code[at - 2..at] == b"::") => {}
                None => {
                    found.calls.insert(name.to_owned());
                }
            }
        }
        // `s.execute("shape.rotate", …)`: the command runs another, and reads what it reads.
        if (name == "execute" || name == "execute_unguarded") && code.get(after) == Some(&b'(') {
            let rest = &source[after + 1..];
            let rest = rest.trim_start();
            if let Some(stripped) = rest.strip_prefix('"')
                && let Some(end) = stripped.find('"')
            {
                let close = matching(code, after);
                let named = names_ids(&source[after + 1..close.saturating_sub(1).max(after + 1)]);
                found.runs.insert((stripped[..end].to_owned(), named));
            }
        }
    }
    found
}

/// Whether the call at `at` is to the engine's own code: unqualified, or through `crate`, `super`,
/// `self` or one of its modules, not `std::fs::rename` or another crate's function.
fn in_engine(code: &[u8], at: usize) -> bool {
    let mut start = at;
    while start >= 2 && &code[start - 2..start] == b"::" {
        let mut from = start - 2;
        while from > 0 && (code[from - 1].is_ascii_alphanumeric() || code[from - 1] == b'_') {
            from -= 1;
        }
        start = from;
    }
    if start == at {
        return true;
    }
    let first = ident_at(code, start);
    [
        "crate",
        "super",
        "self",
        "cmd",
        "tools",
        "text",
        "table",
        "drawing",
        "arrange",
        "format",
        "shape",
        "slide",
        "insert",
        "design",
        "review",
        "connect",
        "autofit",
        "kdm_bridge",
        "keys",
        "context",
        "selection",
        "animation",
        "transition",
        "media",
        "merge",
        "ink",
        "edit",
        "file",
        "inspect",
        "presence",
        "anchor",
    ]
    .contains(&first)
}

/// A `cmd!` argument: a closure or call, or a function given by name (`has_selection`, `order`).
fn given(code: &[u8], source: &str) -> Reach {
    let mut found = reach(code, source);
    let name = std::str::from_utf8(code).unwrap_or("").trim();
    if !name.is_empty() && name.bytes().all(|c| c.is_ascii_alphanumeric() || c == b'_') {
        found.calls.insert(name.to_owned());
    }
    found
}

/// Every `fn` body, by name (a name defined in several files keeps each, by file).
fn functions(files: &[File]) -> BTreeMap<String, Vec<(String, Reach)>> {
    let mut out: BTreeMap<String, Vec<(String, Reach)>> = BTreeMap::new();
    for file in files {
        let code = &file.code;
        let text = std::str::from_utf8(code).unwrap_or("");
        for (at, _) in text.match_indices("fn ") {
            if at > 0 && (code[at - 1].is_ascii_alphanumeric() || code[at - 1] == b'_') {
                continue;
            }
            let name = ident_at(code, at + 3).to_owned();
            if name.is_empty() {
                continue;
            }
            let Some(open) = code[at..].iter().position(|&c| c == b'(').map(|n| at + n) else { continue };
            let params_end = matching(code, open);
            let Some(brace) = code[params_end..].iter().position(|&c| c == b'{' || c == b';').map(|n| params_end + n) else { continue };
            if code[brace] == b';' {
                continue;
            }
            let end = matching(code, brace);
            let body = reach(&code[brace..end], &file.source[brace..end]);
            out.entry(name).or_default().push((file.name.clone(), body));
        }
    }
    out
}

/// One `cmd!(…)` entry: its id, its `enabled` and `run` code, and its file.
struct Spec {
    id: String,
    enabled: Reach,
    run: Reach,
    file: String,
}

fn split_args(code: &[u8], from: usize, to: usize) -> Vec<(usize, usize)> {
    let mut args = Vec::new();
    let (mut start, mut at) = (from, from);
    while at < to {
        match code[at] {
            b'(' | b'[' | b'{' => at = matching(code, at),
            b',' => {
                args.push((start, at));
                at += 1;
                start = at;
            }
            _ => at += 1,
        }
    }
    if code[start..to].iter().any(|c| !c.is_ascii_whitespace()) {
        args.push((start, to));
    }
    args
}

fn specs(files: &[File]) -> Vec<Spec> {
    let mut out = Vec::new();
    for file in files {
        let code = &file.code;
        let text = std::str::from_utf8(code).unwrap_or("");
        for (at, _) in text.match_indices("cmd!(") {
            let open = at + 4;
            let end = matching(code, open);
            let args = split_args(code, open + 1, end - 1);
            if args.len() < 7 {
                continue;
            }
            let (id_from, id_to) = args[0];
            let id_source = &file.source[id_from..id_to];
            let Some(id) = id_source.split('"').nth(1) else { continue };
            let (enabled_from, enabled_to) = args[5];
            let enabled_name = std::str::from_utf8(&code[enabled_from..enabled_to]).unwrap_or("").trim().to_owned();
            let enabled = if DOCUMENT_GATES.contains(&enabled_name.as_str()) {
                Reach::default()
            } else {
                given(&code[enabled_from..enabled_to], &file.source[enabled_from..enabled_to])
            };
            // A closure's `|s, p|` holds a comma: the run is everything after `enabled`.
            let (run_from, run_to) = (args[6].0, args[args.len() - 1].1);
            let run = given(&code[run_from..run_to], &file.source[run_from..run_to]);
            out.push(Spec { id: id.to_owned(), enabled, run, file: file.name.clone() });
        }
    }
    out
}

/// What every command reads, through the functions and commands it reaches.
fn derived() -> BTreeMap<String, BTreeSet<&'static str>> {
    let files = files();
    let functions = functions(&files);
    let specs = specs(&files);
    // A function's reads, to a fixed point over the calls (a name defined in several files is the
    // caller's own file's when it has one, else every one of them).
    let mut reads: BTreeMap<(String, String), BTreeSet<&'static str>> = BTreeMap::new();
    for (name, defs) in &functions {
        for (file, body) in defs {
            reads.insert((name.clone(), file.clone()), body.reads.clone());
        }
    }
    let resolve = |call: &str, file: &str, reads: &BTreeMap<(String, String), BTreeSet<&'static str>>| -> BTreeSet<&'static str> {
        let name = base(call);
        chosen(&functions, call, file).iter().flat_map(|(f, _)| reads.get(&(name.to_owned(), f.clone())).cloned().unwrap_or_default()).collect()
    };
    loop {
        let mut changed = false;
        for (name, defs) in &functions {
            for (file, body) in defs {
                let mut now = reads.get(&(name.clone(), file.clone())).cloned().unwrap_or_default();
                for call in &body.calls {
                    if base(call) != name {
                        now.extend(unnamed(resolve(call, file, &reads), body.named));
                    }
                }
                if reads.get(&(name.clone(), file.clone())) != Some(&now) {
                    reads.insert((name.clone(), file.clone()), now);
                    changed = true;
                }
            }
        }
        if !changed {
            break;
        }
    }
    if let Ok(wanted) = std::env::var("ACTS_FN") {
        for (file, body) in functions.get(&wanted).into_iter().flatten() {
            eprintln!("fn {wanted} in {file}: direct {:?}", body.reads);
            for call in &body.calls {
                eprintln!("  calls {call}: {:?}", resolve(call, file, &reads));
            }
        }
    }
    let mut commands: BTreeMap<String, BTreeSet<&'static str>> = BTreeMap::new();
    let mut command_runs: BTreeMap<String, BTreeSet<(String, bool)>> = BTreeMap::new();
    for spec in &specs {
        let mut now: BTreeSet<&'static str> = spec.enabled.reads.iter().chain(&spec.run.reads).copied().collect();
        for call in spec.enabled.calls.iter().filter(|call| !DOCUMENT_GATES.contains(&call.as_str())) {
            now.extend(unnamed(resolve(call, &spec.file, &reads), spec.enabled.named));
        }
        for call in &spec.run.calls {
            now.extend(unnamed(resolve(call, &spec.file, &reads), spec.run.named));
        }
        if std::env::var("ACTS_DEBUG").is_ok_and(|id| id == spec.id) {
            for call in spec.enabled.calls.iter().chain(&spec.run.calls) {
                eprintln!("{} calls {call}: {:?}", spec.id, resolve(call, &spec.file, &reads));
            }
        }
        commands.insert(spec.id.clone(), now);
        // Follow commands invoked inside named handlers and their helpers, as well as inline
        // registry closures. `regroup` delegates to `arrange.group` through this path.
        let mut runs = spec.run.runs.clone();
        let mut pending: Vec<_> = spec.enabled.calls.iter().chain(&spec.run.calls).map(|name| (name.clone(), spec.file.clone())).collect();
        let mut seen = BTreeSet::new();
        while let Some((name, caller)) = pending.pop() {
            if !seen.insert((name.clone(), caller.clone())) {
                continue;
            }
            for (file, body) in chosen(&functions, &name, &caller) {
                runs.extend(body.runs.iter().cloned());
                pending.extend(body.calls.iter().map(|name| (name.clone(), file.clone())));
            }
        }
        command_runs.insert(spec.id.clone(), runs);
    }
    // Commands that run other commands read what those read; run with explicit `ids`, not the
    // selection they would act on without them.
    loop {
        let mut changed = false;
        for spec in &specs {
            let mut now = commands[&spec.id].clone();
            for (other, named) in &command_runs[&spec.id] {
                now.extend(unnamed(commands.get(other).cloned().unwrap_or_default(), *named));
            }
            if commands[&spec.id] != now {
                commands.insert(spec.id.clone(), now);
                changed = true;
            }
        }
        if !changed {
            break;
        }
    }
    commands
}

/// `reads` without the selection when the shapes are `named` by explicit ids: the selection is
/// what the targets fall back to without them.
fn unnamed(mut reads: BTreeSet<&'static str>, named: bool) -> BTreeSet<&'static str> {
    if named {
        reads.remove("selection");
    }
    reads
}

/// The function name of a call (`list` of `crate::recovery::list`).
fn base(call: &str) -> &str {
    call.rsplit("::").next().unwrap_or(call)
}

/// The definitions a call reaches: for `crate::<module>::f`, that module's `f` (when it defines one
/// rather than re-exporting it); otherwise the caller's own file's when it has one, else every one.
fn chosen<'a>(functions: &'a BTreeMap<String, Vec<(String, Reach)>>, call: &str, caller: &str) -> Vec<&'a (String, Reach)> {
    let Some(defs) = functions.get(base(call)) else { return Vec::new() };
    if let Some(path) = call.strip_prefix("crate").and_then(|rest| rest.strip_suffix(base(call))) {
        let module: PathBuf = path.split("::").filter(|part| !part.is_empty()).collect();
        let files = if module.as_os_str().is_empty() {
            vec![engine_src().join("lib.rs")]
        } else {
            vec![engine_src().join(&module).with_extension("rs"), engine_src().join(&module).join("mod.rs")]
        };
        let there: Vec<_> = defs.iter().filter(|(file, _)| files.iter().any(|f| Path::new(file) == f)).collect();
        if !there.is_empty() {
            return there;
        }
    }
    let own: Vec<_> = defs.iter().filter(|(file, _)| file == caller).collect();
    if own.is_empty() { defs.iter().collect() } else { own }
}

fn name(state: EditorState) -> &'static str {
    state.as_str()
}

#[test]
fn every_command_declares_the_editor_state_its_code_reads() {
    let derived = derived();
    let doc = SlideDocument::create("Deck.kslide", 960, 540).unwrap();
    let commands = doc.commands();
    assert_eq!(derived.len(), commands.len(), "the scan found {} commands; the kit lists {}", derived.len(), commands.len());
    let mut wrong = Vec::new();
    for command in &commands {
        let declared: BTreeSet<&'static str> = command.reads.iter().copied().map(name).collect();
        let expected = derived.get(&command.id).cloned().unwrap_or_else(|| panic!("the scan did not find {}", command.id));
        if declared != expected {
            wrong.push(format!("{}: declares {declared:?}, its code reads {expected:?}", command.id));
        }
    }
    assert!(wrong.is_empty(), "{} commands declare other editor state than they read:\n{}", wrong.len(), wrong.join("\n"));
}
