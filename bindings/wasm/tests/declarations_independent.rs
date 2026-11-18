//! C8-slide-read: expected rows written from handlers and their engine operations.
//! Each family's initial readings were frozen before its reader opened the binding declarations.
//! Later reviewed metadata is marked on each row; these expectations are not all blind.
//! `paths` records separate branches, including reads not expressible in KitCommand.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
use kition_kit::KitDocument;
use kition_slide_kit::SlideDocument;

#[path = "declarations_independent/comments.rs"]
mod comments;
#[path = "declarations_independent/fixture.rs"]
mod fixture;
#[path = "declarations_independent/other.rs"]
mod other;
#[path = "declarations_independent/rests.rs"]
mod rests;
#[path = "declarations_independent/shapes.rs"]
mod shapes;
#[path = "declarations_independent/slides.rs"]
mod slides;
#[path = "declarations_independent/text.rs"]
mod text;

#[derive(Debug)]
struct Row {
    id: &'static str,
    targets: Option<&'static str>,
    objects: &'static [&'static str],
    places: &'static [&'static str],
    reads: &'static [&'static str],
    position_independent: bool,
    paths: &'static [&'static str],
    blind: bool,
}

fn rows() -> impl Iterator<Item = &'static Row> {
    other::ROWS.iter().chain(shapes::ROWS).chain(slides::ROWS).chain(text::ROWS)
}

fn sorted<T: Ord>(items: impl IntoIterator<Item = T>) -> Vec<T> {
    let mut items: Vec<T> = items.into_iter().collect();
    items.sort();
    items
}

fn commands() -> Vec<kition_kit::KitCommand> {
    SlideDocument::create("Reading.kslide", 960, 540).unwrap().commands()
}

#[test]
fn every_write_has_a_handler_reading() {
    let commands = commands();
    let mut seen = std::collections::BTreeSet::new();
    for row in rows() {
        assert!(seen.insert(row.id), "duplicate reading: {}", row.id);
        assert!(!row.paths.is_empty(), "no code paths: {}", row.id);
        assert!(row.paths.iter().all(|path| !path.is_empty()));
        assert!(
            row.blind || row.paths.iter().any(|path| path.to_ascii_lowercase().contains("pinned from the declaration after review")),
            "missing provenance: {}",
            row.id
        );
    }
    let missing: Vec<_> =
        commands.iter().filter(|command| command.mutates && !seen.contains(command.id.as_str())).map(|command| command.id.as_str()).collect();
    assert!(missing.is_empty(), "unread write handlers: {missing:?}");
}

#[test]
fn declarations_match_independent_handler_readings() {
    let commands = commands();
    let mut differences = Vec::new();
    for row in rows() {
        let Some(command) = commands.iter().find(|command| command.id == row.id && command.mutates) else { continue };
        let schema = command.schema.as_ref().unwrap();
        // Project handler aliases onto the published interface. `onSlide` belongs to the
        // call wrapper, whose resolution is recorded once in the shape family's preamble.
        let has = |name: &str| schema.get(name).is_some();
        let names: Vec<_> = row.targets.into_iter().chain(row.objects.iter().copied()).filter(|name| has(name)).collect();
        let target = if row.targets.is_some() { ["ids", "id"].into_iter().find(|name| names.contains(name)) } else { None };
        let objects = sorted(names.into_iter().filter(|name| Some(*name) != target));
        let mut places: Vec<_> = row.places.iter().copied().filter(|name| has(name)).collect();
        if has("onSlide") && !places.contains(&"onSlide") {
            places.push("onSlide");
        }
        let actual = (
            command.targets,
            sorted(command.objects.iter().copied()),
            sorted(command.places.iter().copied()),
            sorted(command.reads.iter().map(|state| state.as_str())),
            command.targets.is_some() && command.position_independent,
        );
        let expected = (target, objects, sorted(places), sorted(row.reads.iter().copied()), target.is_some() && row.position_independent);
        if actual != expected {
            differences.push(format!("{}: declared {actual:?}; reading {expected:?}", row.id));
        }
    }
    assert!(differences.is_empty(), "{} differing commands:\n{}", differences.len(), differences.join("\n"));
}

/// The commands whose holds differ from their reading (C8-slide-read-3). Independent review may
/// raise this count when the reading, rather than the declaration, was wrong.
/// The 802 dynamic paths include 106 reviewed paths (48 commands); SCANNED also supplies 51
/// originally reviewed editor-state facts (38 commands). That original union was 81 commands,
/// leaving the 112-command population used for the fresh blind sample. SCANNED now retains 38
/// facts on 34 commands. Matching these rows does not close SDK journal or plan-context gaps.
// C8-slide-read-4: 16 -> 3 after unused editor-state holds, -> 2 after anchor content,
// -> 1 after show members, -> 0 after ancestor body -> body.props. Each intermediate gate passed.
// A shared ancestor narrowing grew LESS to 4 and was reverted; only clipboard paths narrow.
// Static metadata is checked separately; SDK journal/plan-context probes remain strict and red.
const DIFFERING_FLOOR: usize = 0;

#[test]
fn every_write_rests_on_what_its_handler_reads() {
    let (lines, differing, less, more, writes) = rests::compare();
    if let Ok(path) = std::env::var("SLIDE_RESTS_REPORT") {
        let report = format!(
            "{}\ncommands differing {} of {writes}: holding less {}, holding more {}, both {}\nLESS {less:?}\nMORE {more:?}\n",
            lines.join("\n"),
            differing.len(),
            less.len(),
            more.len(),
            less.intersection(&more).count()
        );
        std::fs::write(path, report).unwrap();
    }
    assert_eq!(writes, 193, "the write commands read");
    assert!(lines.iter().all(|line| !line.ends_with("a write command with no reading")), "{}", lines.join("\n"));
    assert_eq!(differing.len(), DIFFERING_FLOOR, "{} of {writes} commands differ (floor {DIFFERING_FLOOR}):\n{}", differing.len(), lines.join("\n"));
}
