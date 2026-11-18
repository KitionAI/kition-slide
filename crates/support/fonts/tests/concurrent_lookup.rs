//! Lookups by family name on several threads at once all find an installed family, whichever
//! thread loads it: the font a text resolves to never depends on what other threads are doing.

use std::path::PathBuf;
use std::sync::{Arc, Barrier};

use kition_slide_fonts::FontDb;

#[test]
fn concurrent_lookups_all_find_an_installed_family() {
    let dir = PathBuf::from(env!("CARGO_TARGET_TMPDIR")).join("kition-slide-fonts-concurrent");
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join("Hack-Regular.ttf"), epaint_default_fonts::HACK_REGULAR).unwrap();

    const THREADS: usize = 8;
    for round in 0..20 {
        let db = Arc::new(FontDb::with_font_dirs(vec![dir.clone()]));
        db.set_system_fallback(false);
        // Catalog first, so every thread reaches the family's loading together.
        assert!(db.has_family("Hack"));
        let barrier = Arc::new(Barrier::new(THREADS));
        let families: Vec<String> = (0..THREADS)
            .map(|_| {
                let (db, barrier) = (db.clone(), barrier.clone());
                std::thread::spawn(move || {
                    barrier.wait();
                    db.face("Hack", "Regular").family.clone()
                })
            })
            .collect::<Vec<_>>()
            .into_iter()
            .map(|thread| thread.join().unwrap())
            .collect();
        assert!(families.iter().all(|family| family == "Hack"), "round {round}: {families:?}");
        assert_eq!(db.styles("Hack"), ["Regular"], "round {round}: the family's faces are added once");
    }
}
