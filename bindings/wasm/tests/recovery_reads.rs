//! The AutoRecover commands work on the host's recovery folder, not on the deck being edited: they
//! read neither the selection nor the current slide, so a host sets no `on` or `at.part` for them
//! (case regressions/issue-recovery-list-reads).
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use kition_kit::KitDocument;
use kition_slide_kit::SlideDocument;

#[test]
fn recovery_commands_read_no_editor_state() {
    let commands = SlideDocument::create("Deck.kslide", 960, 540).unwrap().commands();
    for id in ["file.recovery.list", "file.recovery.discard"] {
        let command = commands.iter().find(|c| c.id == id).unwrap_or_else(|| panic!("no command {id}"));
        assert!(command.reads.is_empty(), "{id} reads {:?}; it lists the recovery folder only", command.reads);
    }
}
