use super::Row;

macro_rules! file {
    ($id:literal, $path:literal) => {
        file!($id, &[], $path)
    };
    ($id:literal, $reads:expr, $path:literal) => {
        Row {
            id: $id,
            targets: None,
            objects: &[],
            places: &[],
            reads: $reads,
            position_independent: false,
            blind: false,
            paths: &["Metadata pinned from the declaration after review; file handler and engine operation read before recording this row.", $path],
        }
    };
}

// Frozen from cmd/file.rs and cmd/media.rs before reading their binding declarations.
pub(super) const ROWS: &[Row] = &[
    file!(
        "file.new",
        "theme builtin name and supplied size (else WIDE) build fresh defaults; settle storage/autofit; add active document using next untitled counter. No old document read."
    ),
    file!(
        // C8-slide-read-4 RIGHT: cmd/file.rs:98-105 imports supplied bytes; no object/place/editor input.
        "file.open",
        "path reads external bytes, format sniff/import/repair; add active document remembering native path. Host-only file access rejects this command in the served kit."
    ),
    // C8-slide-read-4 RIGHT: cmd/file.rs:112-117 imports supplied data; no object/place/editor input.
    file!("file.openBytes", "data base64/name decode and import/repair; add active document. No old document read."),
    file!(
        // C8-slide-read-4 RIGHT: cmd/file.rs:204-214 exports the active document, independent of its current part/selection.
        "file.save",
        "path supplied else active document path, format supplied else suffix; save full current document, materialize autofit in export projection, atomically write and mark saved; absent path queues UI picker. Host-only file access rejects disk write."
    ),
    file!(
        // C8-slide-read-4 RIGHT: cmd/file.rs:217-222 forwards save or requests a path; no part/selection.
        "file.saveAs",
        "path absent queues picker, else file.save; reads full document and marks saved after atomic disk write. Host-only disk write is rejected."
    ),
    // C8-slide-read-4 RIGHT: cmd/file.rs:414-418 closes the active document; no part/selection.
    file!("file.close", "read active document index/uid; remove that document and discard its recovery entry; select remaining document."),
    file!(
        // C8-slide-read-4 RIGHT: cmd/file.rs:446-451 restores saved_doc; no part/selection chooses its result.
        "file.revert",
        "replace entire current document by session saved_doc; clear history retaining its limits/counters; bump revision. Reads session saved snapshot."
    ),
    file!(
        // C8-slide-read-4 RIGHT: cmd/file.rs:469-471 and recovery.rs:52-65 walk documents, not the editor part.
        "file.recovery.save",
        "read recovery directory, each open document dirty state/content/path/title/uid and clock; write native data and metadata or remove clean entries. Host-only file access rejects recovery."
    ),
    file!(
        "file.recovery.open",
        "list native recovery files and metadata by uid, load/repair files, add documents and remove consumed entries. Host-only file access rejects recovery."
    ),
    file!(
        "file.recovery.discard",
        "uid is recovery-file uid, not a KDM id; supplied uid removes that recovery pair, absent uid lists/removes all recovery pairs. Host-only file access rejects recovery."
    ),
    Row {
        id: "file.properties",
        targets: None,
        objects: &[],
        places: &[],
        reads: &[],
        position_independent: true,
        blind: true,
        paths: &[
            "Any property supplied: replace those document props; preserve all other props; return props. No object/place/editor state is resolved.",
            "No property supplied: return document props without a write.",
        ],
    },
    Row {
        id: "media.posterFrame",
        targets: Some("id"),
        objects: &["ids"],
        places: &[],
        reads: &["selection", "part"],
        position_independent: false,
        blind: true,
        paths: &[
            "id/ids: resolve shape tool ids to local ids and slide; take first media among targets; read clip.video, media reference and referenced Media.data bytes, plus group/inherited geometry through edit_shapes; add PNG media then set poster.",
            "No explicit target: selected first media, else scan current slide for exactly one top-level media shape; same reads/writes; insertion of another media changes this resolution.",
            "ms absent: read session media_status position_ms, default zero; playback state is not included in engine editor_state.",
            "edit_shapes materializes an absent xfrm from the matching layout/master placeholder, projects through every enclosing group's outer box and child space, runs the poster-only update, and restores an unchanged group-local box.",
        ],
    },
    Row {
        id: "media.play",
        targets: Some("id"),
        objects: &["ids"],
        places: &[],
        reads: &["selection", "part"],
        position_independent: true,
        blind: true,
        paths: &["id/ids/selection, else the only top-level media on current slide: read media kind and queue UI request; no stored document write."],
    },
    Row {
        id: "media.pause",
        targets: Some("id"),
        objects: &["ids"],
        places: &[],
        reads: &["selection", "part"],
        position_independent: true,
        blind: true,
        paths: &[
            "id/ids/selection, else the only top-level media on current slide: read media kind and queue pause request; no stored document write.",
        ],
    },
    Row {
        id: "media.toggle",
        targets: Some("id"),
        objects: &["ids"],
        places: &[],
        reads: &["selection", "part"],
        position_independent: true,
        blind: true,
        paths: &[
            "id/ids/selection, else the only top-level media on current slide: read media kind and queue toggle request; no stored document write.",
        ],
    },
    Row {
        id: "media.stop",
        targets: Some("id"),
        objects: &["ids"],
        places: &[],
        reads: &["selection", "part"],
        position_independent: true,
        blind: true,
        paths: &[
            "id/ids/selection, else the only top-level media on current slide: read media kind and queue stop request; no stored document write.",
        ],
    },
    Row {
        id: "media.seek",
        targets: Some("id"),
        objects: &["ids"],
        places: &[],
        reads: &["selection", "part"],
        position_independent: true,
        blind: true,
        paths: &[
            "id/ids/selection, else the only top-level media on current slide: read media kind and queue request with supplied ms; no stored document write.",
        ],
    },
    Row {
        id: "media.gifPlay",
        targets: Some("ids"),
        objects: &["id"],
        places: &[],
        reads: &["selection", "part"],
        position_independent: true,
        blind: true,
        paths: &[
            "id/ids/selection, else only top-level animated GIF: read picture media bytes/adjustments, and paused set when play absent; change session paused set only.",
        ],
    },
];
