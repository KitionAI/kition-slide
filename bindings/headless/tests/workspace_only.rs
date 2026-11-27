//! `kition-slide` reads and writes only inside its workspace, and only through its host (KHP A29,
//! A58, section 10.6): whatever an agent runs, nothing appears in the process's current folder,
//! its home folder or its temporary folder, nothing outside the workspace is read into an answer
//! or a picture, changed or deleted, and the workspace holds only what the host saved. An audit
//! (slide section) found that the engine opens, saves, exports and inserts pictures
//! and media by any path it is given (`file.save`, `file.saveAs`, `file.saveTemplate`,
//! `file.export`, `file.open`, `image.insert`, `audio.insert`, `video.insert`), that a deck
//! opened from its replica is saved to its bare name, so `file.save {}` writes into the current
//! folder, and that the kit scans the machine's font folders and tells the agent which fonts are
//! installed. The pictures and media a command names by path are read through the host now, so a
//! picture in the workspace is placed and one outside it is not.
//!
//! The test runs itself again as a child process whose current folder, `HOME` and `TMPDIR` are
//! fresh folders, so every by-path write the engine makes lands where it can be seen. `HOME` holds
//! a font in the folders a font scan reads; the deck asks for that font, which a kit that reads no
//! font folder of the machine does not have. A folder beside them (`outside/`) holds this test's
//! sentinel files, which the agent names by path (directly, inside `model.batch` as a script, as
//! the agent and as a person it claims to be, and as links of a deck, which is the agent's data):
//! the kit must neither read, change nor delete them.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::path::{Path, PathBuf};
use std::sync::Arc;

use kition_headless::{Headless, HostFolder, KitInfo};
use kition_kit::KitDocument;
use kition_kit_server::protocol::{Origin, ScreenshotScope, base64};
use kition_kit_server::{KitBackend, OpenRequest, ScreenshotRequest};
use kition_slide_kit::SlideDocument;
use serde_json::{Map, Value, json};

/// Set in the child: the folder holding `workspace/`, `outside/`, `cwd/`, `home/` and `tmp/`.
const CHILD: &str = "KITION_SLIDE_TEST_SANDBOX";
const DECK: &str = "deck.kslide";
/// The picture outside the workspace, and one inside it.
const OUTSIDE_RGB: [u8; 3] = [17, 233, 71];
const INSIDE_RGB: [u8; 3] = [201, 40, 160];
/// Text only files outside the workspace hold: it must reach no answer.
const SECRET: &str = "SENTINEL-5c2e";
/// A font family no kit build bundles (crates/support/fonts embeds kition-fonts' families and
/// Ubuntu Light): installed only in the child's home font folders.
const HOME_FONT: &str = "Hack";
/// The decks that link a picture outside the workspace, and what each links.
const LINKED: [(&str, &str); 3] = [("linked-absolute.pptx", "absolute"), ("linked-up.pptx", "../outside/secret.png"), ("linked-url.pptx", "file")];

/// Every file and folder under `dir`, relative to it, with a file's bytes.
fn tree(dir: &Path) -> Vec<(String, Option<Vec<u8>>)> {
    let mut out = Vec::new();
    let mut stack = vec![dir.to_path_buf()];
    while let Some(next) = stack.pop() {
        for entry in std::fs::read_dir(&next).into_iter().flatten().flatten() {
            let path = entry.path();
            let name = path.strip_prefix(dir).unwrap_or(&path).to_string_lossy().into_owned();
            if path.is_dir() {
                stack.push(path.clone());
                out.push((name, None));
            } else {
                out.push((name, std::fs::read(&path).ok()));
            }
        }
    }
    out.sort();
    out
}

fn names(dir: &Path) -> Vec<String> {
    tree(dir).into_iter().map(|(name, _)| name).collect()
}

/// A 64 x 64 PNG of one colour.
fn png(rgb: [u8; 3]) -> Vec<u8> {
    let mut out = std::io::Cursor::new(Vec::new());
    image::RgbImage::from_pixel(64, 64, image::Rgb(rgb)).write_to(&mut out, image::ImageFormat::Png).unwrap();
    out.into_inner()
}

/// Whether some pixel of `rgba` is within 8 of `rgb` in every channel.
fn shows(rgba: &[u8], rgb: [u8; 3]) -> bool {
    rgba.as_chunks::<4>().0.iter().any(|px| (0..3).all(|c| px[c].abs_diff(rgb[c]) <= 8))
}

fn params(value: Value) -> Map<String, Value> {
    value.as_object().cloned().unwrap_or_default()
}

/// The agent's session: the headless kit over the workspace, and every answer it gave.
struct Agent {
    kit: Headless<SlideDocument>,
    folder: HostFolder,
    answers: Vec<String>,
}

impl Agent {
    fn open(&mut self, name: &str) -> String {
        self.kit.open(OpenRequest { file: &self.folder.file(name).unwrap(), read_only: false, create: None }).unwrap().id
    }

    fn keep(&mut self, what: String, answer: Result<Value, String>) -> Result<Value, String> {
        self.answers.push(format!("{what}: {answer:?}"));
        answer
    }

    /// `command` as the agent calls it directly.
    fn run(&mut self, id: &str, command: &str, value: Value) -> Result<Value, String> {
        let done = self.kit.execute(id, command, params(value), Origin::Agent);
        let answer = done.map(|done| done.result.unwrap_or(Value::Null)).map_err(|e| format!("{e:?}"));
        self.keep(command.to_owned(), answer)
    }

    /// `command` as one `command` operation of `model.batch`, with the batch's `origin` (none: the
    /// object model's default, a script; or one the caller claims).
    fn batch(&mut self, id: &str, command: &str, value: Value, origin: &Value) -> Result<Value, String> {
        let mut request = json!({ "documentId": id, "ops": [{ "op": "command", "id": command, "params": value }] });
        if !origin.is_null() {
            request["origin"] = origin.clone();
        }
        let answer = self.kit.model("model.batch", request).map_err(|e| format!("{e:?}"));
        self.keep(format!("model.batch {command} as {origin}"), answer)
    }

    /// The canvas of document `id` (every slide), as RGBA.
    fn picture(&mut self, id: &str) -> Vec<u8> {
        let shot = self.kit.screenshot(id, &ScreenshotRequest { scope: ScreenshotScope::Canvas, max_size: 512, scale: 1.0, target: None }).unwrap();
        image::load_from_memory(&shot.png).unwrap().to_rgba8().into_raw()
    }
}

/// What the agent does in the child. Errors are fine; reaching files outside the workspace is not.
/// Returns what went wrong that the child can see (answers and pictures); the parent checks the
/// folders.
fn act(root: &Path) -> Vec<String> {
    let workspace = root.join("workspace");
    let outside = root.join("outside");
    let at = |name: &str| outside.join(name).to_string_lossy().into_owned();
    let mut wrong = Vec::new();

    let info = KitInfo::from_manifest(include_str!("../../../kit.json"), kition_slide_kit::SANDBOXED, kition_slide_kit::EDITOR_STATE).unwrap();
    let folder = HostFolder::new(&workspace).unwrap();
    let mut agent = Agent { kit: Headless::<SlideDocument>::with_replica_port(info), folder, answers: Vec::new() };
    agent.kit.set_host(agent.folder.host());
    let id = agent.open(DECK);

    // The deck asks for a font installed only in this process's home font folders: a kit that
    // reads no font folder of the machine does not have it, and says so.
    let found = agent.kit.check(&id).unwrap().to_string();
    if !(found.contains("slide.font-substituted") && found.contains(HOME_FONT)) {
        wrong.push(format!("the check does not report {HOME_FONT}, a font only the machine's home font folder has, as substituted: {found}"));
    }
    let _ = agent.run(&id, "theme.fonts", json!({ "major": HOME_FONT, "minor": HOME_FONT }));

    // A picture in the workspace, named by its path, is read through the host, placed and shown;
    // so is one given as bytes.
    let placed = agent.run(&id, "image.insert", json!({ "path": "inside.png" }));
    if placed.is_err() || !shows(&agent.picture(&id), INSIDE_RGB) {
        wrong.push(format!("the picture in the workspace is not placed and shown: {placed:?}"));
    }
    let _ = agent.run(&id, "edit.undo", json!({}));
    let inside = base64::encode(&std::fs::read(workspace.join("inside.png")).unwrap());
    let placed = agent.run(&id, "image.insert", json!({ "name": "inside.png", "data": inside }));
    if placed.is_err() || !shows(&agent.picture(&id), INSIDE_RGB) {
        wrong.push(format!("a picture given as bytes is not placed and shown: {placed:?}"));
    }

    // Every command with a file parameter, naming files outside the workspace (absolute, a `..`
    // climb from the current folder, and a bare name the current folder would take), directly and
    // inside model.batch: as a script (no origin), as the agent, and as a person the caller claims
    // to be.
    let mut by_path: Vec<(&str, Value)> = Vec::new();
    for target in [at("made.kslide"), "../outside/made-up.kslide".to_owned(), "made-here.kslide".to_owned()] {
        by_path.push(("file.save", json!({ "path": target })));
        by_path.push(("file.saveAs", json!({ "path": target.replace(".kslide", ".pptx") })));
        by_path.push(("file.saveTemplate", json!({ "path": target.replace(".kslide", ".potx") })));
        by_path.push(("file.export", json!({ "path": target.replace(".kslide", ".pdf") })));
        by_path.push(("file.export", json!({ "path": target.replace(".kslide", ".png"), "all": true })));
        by_path.push(("file.export", json!({ "path": target.replace(".kslide", ".txt"), "format": "outline" })));
    }
    for source in [at("secret.png"), "../outside/secret.png".to_owned()] {
        by_path.push(("image.insert", json!({ "path": source })));
        by_path.push(("image.change", json!({ "path": source })));
        by_path.push(("audio.insert", json!({ "path": source })));
        by_path.push(("video.insert", json!({ "path": source })));
        by_path.push(("shape.fill", json!({ "picture": null, "path": source })));
        by_path.push(("slide.background", json!({ "picture": null, "path": source })));
    }
    for source in [at("secret.txt"), at("secret.md"), "../outside/secret.txt".to_owned(), at("planted.kslide")] {
        by_path.push(("file.open", json!({ "path": source })));
    }
    // The deck as the host opened it: saved to no path of the engine's.
    by_path.push(("file.save", json!({})));
    by_path.push(("file.recovery.save", json!({})));
    by_path.push(("file.recovery.list", json!({})));
    for (command, value) in &by_path {
        let _ = agent.run(&id, command, value.clone());
    }
    for origin in [Value::Null, json!({ "kind": "agent", "principal": "agent:test" }), json!({ "kind": "person", "principal": "person:local" })] {
        for (command, value) in &by_path {
            let _ = agent.batch(&id, command, value.clone(), &origin);
        }
    }
    if shows(&agent.picture(&id), OUTSIDE_RGB) {
        wrong.push("the picture outside the workspace was read into the deck".to_owned());
    }
    for (command, value) in [("file.inspect", json!({})), ("slide.inspect", json!({ "index": 0 })), ("file.saveBytes", json!({}))] {
        let _ = agent.run(&id, command, value);
    }
    let described = agent.kit.describe(&id).map(|describe| Value::Object(describe.state)).map_err(|e| format!("{e:?}"));
    let _ = agent.keep("view.describe".to_owned(), described);

    // Decks are the agent's data: one whose picture links a file outside the workspace (by
    // absolute path, by a `..` climb from the deck's folder, by a file: URL) shows no picture of it.
    for (name, _) in LINKED {
        let linked = agent.open(name);
        if shows(&agent.picture(&linked), OUTSIDE_RGB) {
            wrong.push(format!("{name}: the picture it links outside the workspace was read into the render"));
        }
        let found = agent.kit.check(&linked).map_err(|e| format!("{e:?}"));
        let _ = agent.keep(format!("file.check {name}"), found);
        for (command, value) in [("file.inspect", json!({})), ("file.saveBytes", json!({ "format": "pptx" })), ("file.save", json!({}))] {
            let _ = agent.run(&linked, command, value);
        }
    }

    // The host saves the deck into the workspace, as it always may.
    if let Err(error) = agent.kit.save(&id, None, None) {
        wrong.push(format!("the host could not save the deck: {error:?}"));
    }

    // Every command the kit offers the agent, with no parameters.
    let offered: Vec<String> = agent.kit.commands(Some(&id)).unwrap().into_iter().map(|command| command.id).collect();
    for command in offered {
        // These end or replace the deck the rest of the sweep runs in.
        if matches!(command.as_str(), "file.close" | "file.new" | "file.revert" | "window.next") {
            continue;
        }
        let _ = agent.run(&id, &command, json!({}));
    }

    // Nothing outside the workspace reached an answer: its files' text, a folder of this process.
    let home = root.join("home").to_string_lossy().into_owned();
    let tmp = root.join("tmp").to_string_lossy().into_owned();
    let cwd = root.join("cwd").to_string_lossy().into_owned();
    for answer in &agent.answers {
        for leak in [SECRET, home.as_str(), tmp.as_str(), cwd.as_str()] {
            if answer.contains(leak) {
                wrong.push(format!("an answer holds `{leak}` from outside the workspace: {}", answer.chars().take(400).collect::<String>()));
            }
        }
    }
    wrong
}

/// A deck with one picture, as `.pptx`, its picture linking `target` instead of holding bytes.
fn linked_deck(target: &str) -> Vec<u8> {
    let mut doc = SlideDocument::create("linked.pptx", 960, 540).unwrap();
    doc.execute("image.insert", json!({ "name": "linked.png", "data": base64::encode(&png(INSIDE_RGB)) })).unwrap();
    let mut file = kition_slide_engine::cmd::file::open_slide_file("linked.pptx", &doc.save(Some("linked.pptx")).unwrap()).unwrap();
    for item in file.media.iter_mut().filter(|item| item.content_type.starts_with("image/")) {
        item.data = Arc::new(Vec::new());
        item.link = Some(target.to_owned());
    }
    kition_slide_engine::cmd::file::save_bytes(&file, "pptx").unwrap()
}

#[test]
fn agent_commands_reach_no_file_outside_the_workspace() {
    if let Some(root) = std::env::var_os(CHILD) {
        let wrong = act(&PathBuf::from(root));
        assert!(wrong.is_empty(), "{wrong:#?}");
        return;
    }
    let root = std::env::temp_dir().join(format!("kition-slide-workspace-only-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    for dir in ["workspace", "cwd", "home", "tmp", "outside/sub"] {
        std::fs::create_dir_all(root.join(dir)).unwrap();
    }
    let root = std::fs::canonicalize(root).unwrap();
    let (workspace, outside, home) = (root.join("workspace"), root.join("outside"), root.join("home"));

    // The font only the child's home font folders hold (macOS, then other Unix systems).
    for folder in ["Library/Fonts", ".fonts", ".local/share/fonts"] {
        std::fs::create_dir_all(home.join(folder)).unwrap();
        std::fs::write(home.join(folder).join("Hack-Regular.ttf"), epaint_default_fonts::HACK_REGULAR).unwrap();
    }
    // The workspace: a deck that asks for that font, a picture, and decks linking outside.
    let mut deck = SlideDocument::create(DECK, 960, 540).unwrap();
    deck.execute("theme.fonts", json!({ "major": HOME_FONT, "minor": HOME_FONT })).unwrap();
    deck.execute("textBox.insert", json!({ "rect": [60, 60, 400, 80], "text": "Title" })).unwrap();
    std::fs::write(workspace.join(DECK), deck.save(None).unwrap()).unwrap();
    std::fs::write(workspace.join("inside.png"), png(INSIDE_RGB)).unwrap();
    for (name, link) in LINKED {
        let target = match link {
            "absolute" => outside.join("secret.png").to_string_lossy().into_owned(),
            "file" => format!("file://{}", outside.join("secret.png").to_string_lossy()),
            relative => relative.to_owned(),
        };
        std::fs::write(workspace.join(name), linked_deck(&target)).unwrap();
    }
    // Outside it: the sentinels.
    std::fs::write(outside.join("secret.png"), png(OUTSIDE_RGB)).unwrap();
    std::fs::write(outside.join("secret.txt"), format!("{SECRET}\n\n{SECRET} body\n")).unwrap();
    std::fs::write(outside.join("secret.md"), format!("# {SECRET}\n\n- {SECRET}\n")).unwrap();
    let mut planted = SlideDocument::create("planted.kslide", 960, 540).unwrap();
    planted.execute("textBox.insert", json!({ "rect": [60, 60, 400, 80], "text": SECRET })).unwrap();
    std::fs::write(outside.join("planted.kslide"), planted.save(None).unwrap()).unwrap();
    std::fs::write(outside.join("sub/kept.txt"), SECRET).unwrap();
    let planted_outside = tree(&outside);
    let planted_home = tree(&home);
    let workspace_before = names(&workspace);

    let status = std::process::Command::new(std::env::current_exe().unwrap())
        .args(["--exact", "agent_commands_reach_no_file_outside_the_workspace", "--nocapture", "--test-threads=1"])
        .current_dir(root.join("cwd"))
        .env(CHILD, &root)
        .env("HOME", &home)
        .env("TMPDIR", root.join("tmp"))
        .env("XDG_CONFIG_HOME", home.join(".config"))
        .env("XDG_DATA_HOME", home.join(".local/share"))
        .env("XDG_CACHE_HOME", home.join(".cache"))
        .env("XDG_RUNTIME_DIR", root.join("tmp"))
        .status()
        .unwrap();
    let mut wrong = Vec::new();
    if !status.success() {
        wrong.push(format!("the agent's session found something outside the workspace (see its output above): {status}"));
    }
    for dir in ["cwd", "tmp"] {
        let found = names(&root.join(dir));
        if !found.is_empty() {
            wrong.push(format!("the kit wrote into its {dir} folder: {found:?}"));
        }
    }
    if tree(&home) != planted_home {
        wrong.push(format!("the kit changed its home folder: {:?}", names(&home)));
    }
    let now = tree(&outside);
    if now != planted_outside {
        let before: Vec<&String> = planted_outside.iter().map(|(name, _)| name).collect();
        let after: Vec<&String> = now.iter().map(|(name, _)| name).collect();
        wrong.push(format!("the folder outside the workspace changed: before {before:?}, after {after:?}"));
    }
    let found = names(&workspace);
    if found != workspace_before {
        wrong.push(format!("the workspace holds {found:?}, not only what the host saved and the test wrote ({workspace_before:?})"));
    }
    assert!(wrong.is_empty(), "{wrong:#?}");
    let _ = std::fs::remove_dir_all(&root);
}
