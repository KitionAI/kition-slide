//! Files named by path, and the session a host owns every file of (KHP A29, A58, section 10.6).
//!
//! In the app a command may read a picture, open a slide file, save, export, or keep AutoRecover
//! copies by path. A kit served by a host (Kition's headless kit) reaches files only through that
//! host, which opens, saves and exports documents and lends the files a document refers to.
//! [`Session::set_host_files_only`] says so. Then every by-path write and every other by-path
//! access of the engine refuses ([`need`]), and a picture or media file named by path is read
//! through the host's lent files alone ([`read`], [`Session::set_host_reader`]), whoever
//! dispatched the command: an agent directly, a `model.batch` step, or a command run by another
//! command. Bytes given as `data` work as ever.

use crate::{EngineError, Result, Session};

/// What a refused by-path access answers when a host owns every file.
const HOSTED: &str = "this kit reads and writes no file by path here: its host opens, saves and exports documents";

/// Reads a file the host lends by the path a command names (a path in the workspace, relative to
/// the deck's folder): `kition_kit::host::read` in the headless kit, which refuses any path outside
/// the workspace the same way whether or not it exists.
pub type HostRead = fn(&str) -> std::result::Result<Vec<u8>, String>;

impl Session {
    /// Says a host owns every file of this session (or that it does not): with `true` the
    /// engine writes no file by path, opens no slide file by path, keeps no AutoRecover copies
    /// ([`need`]), and reads pictures and media named by path only through
    /// [`Session::set_host_reader`]'s reader. The headless kit sets it before an agent runs
    /// anything.
    pub fn set_host_files_only(&mut self, on: bool) {
        self.host_files_only = on;
        if on {
            self.recovery_dir = None;
        }
    }

    /// Whether a host owns every file of this session ([`Session::set_host_files_only`]).
    pub fn host_files_only(&self) -> bool {
        self.host_files_only
    }

    /// What reads a picture or media file named by path while a host owns every file; `None`
    /// refuses such reads.
    pub fn set_host_reader(&mut self, read: Option<HostRead>) {
        self.host_read = read;
    }
}

/// Refuses `command`'s by-path access when a host owns every file of the session. The path is not
/// echoed: the refusal reads the same whether or not the file exists.
pub(crate) fn need(s: &Session, command: &str) -> Result<()> {
    if s.host_files_only {
        return Err(EngineError::Other(format!("`{command}`: {HOSTED}")));
    }
    Ok(())
}

/// The bytes of the file `path` names, for `command`: when a host owns every file, through its
/// reader alone ([`Session::set_host_reader`]; refused without one), else from the disk.
pub(crate) fn read(s: &Session, command: &str, path: &str) -> Result<Vec<u8>> {
    if s.host_files_only {
        let read = s.host_read.ok_or_else(|| EngineError::Other(format!("`{command}`: {HOSTED}")))?;
        return read(path).map_err(|error| EngineError::Other(format!("`{command}`: {error}")));
    }
    from_disk(path)
}

#[cfg(not(target_arch = "wasm32"))]
fn from_disk(path: &str) -> Result<Vec<u8>> {
    std::fs::read(path).map_err(|e| EngineError::Other(format!("{path}: {e}")))
}

#[cfg(target_arch = "wasm32")]
fn from_disk(_path: &str) -> Result<Vec<u8>> {
    Err(EngineError::Other("reading a file by path is not available in the browser: give its bytes as base64 `data`".into()))
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use crate::Session;

    /// A folder of its own under the system's temporary folder, removed afterwards.
    struct Scratch(std::path::PathBuf);

    impl Scratch {
        fn new(name: &str) -> Self {
            let dir = std::env::temp_dir().join(format!("kition-slide-hosted-{name}-{}", std::process::id()));
            let _ = std::fs::remove_dir_all(&dir);
            std::fs::create_dir_all(&dir).unwrap();
            Self(dir)
        }

        fn at(&self, name: &str) -> String {
            self.0.join(name).to_string_lossy().into_owned()
        }

        fn names(&self) -> Vec<String> {
            let mut names = Vec::new();
            for entry in std::fs::read_dir(&self.0).unwrap().flatten() {
                names.push(entry.file_name().to_string_lossy().into_owned());
            }
            names.sort();
            names
        }
    }

    impl Drop for Scratch {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    /// One red pixel, as PNG.
    const RED: &str = "iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAIAAACQd1PeAAAADElEQVR4nGM4wcAAAAJcAMlDyBJGAAAAAElFTkSuQmCC";

    /// A host that lends `inside.png` (one red pixel) and refuses every other path.
    fn lend(path: &str) -> Result<Vec<u8>, String> {
        if path != "inside.png" {
            return Err(format!("{path}: not lent"));
        }
        crate::cmd::base64_decode(RED).ok_or_else(|| "not base64".to_owned())
    }

    // regressions/issue-writes-outside-workspace: a session a host owns every file of opens,
    // saves, exports, inserts and keeps AutoRecover copies by no path, whatever the parameters
    // say; the same commands with bytes still run, and nothing is read or written.
    #[test]
    fn a_hosted_session_reaches_no_file_by_path() {
        let dir = Scratch::new("refuse");
        let mut maker = Session::with_new();
        let saved = maker.execute("file.saveBytes", &json!({})).unwrap();
        let deck = crate::cmd::base64_decode(saved["data"].as_str().unwrap()).unwrap();
        std::fs::write(dir.at("planted.kslide"), &deck).unwrap();
        std::fs::write(dir.at("planted.png"), crate::cmd::base64_decode(RED).unwrap()).unwrap();
        let planted = dir.names();

        let mut s = Session::with_new();
        s.set_host_files_only(true);
        assert!(s.host_files_only());
        // A deck saved to a path, as a replica opened headless is (its file's name).
        s.doc_mut().unwrap().path = Some(dir.at("replica.kslide"));
        let documents = s.documents().len();
        let refused = [
            ("file.open", json!({ "path": dir.at("planted.kslide") })),
            ("file.save", json!({})),
            ("file.save", json!({ "path": dir.at("saved.kslide") })),
            ("file.saveAs", json!({ "path": dir.at("saved-as.pptx") })),
            ("file.saveTemplate", json!({ "path": dir.at("template.potx") })),
            ("file.export", json!({ "path": dir.at("export.pdf") })),
            ("file.export", json!({ "path": dir.at("export.png"), "all": true })),
            ("file.export", json!({ "path": dir.at("export.kslide"), "format": "kslide" })),
            ("image.insert", json!({ "path": dir.at("planted.png") })),
            ("image.change", json!({ "path": dir.at("planted.png") })),
            ("audio.insert", json!({ "path": dir.at("planted.png") })),
            ("video.insert", json!({ "path": dir.at("planted.png") })),
            ("file.recovery.save", json!({})),
            ("file.recovery.list", json!({})),
            ("file.recovery.open", json!({})),
            ("file.recovery.discard", json!({})),
        ];
        for (command, params) in refused {
            let answer = s.execute(command, &params);
            assert!(answer.is_err(), "{command} {params} ran in a hosted session: {answer:?}");
        }
        assert_eq!(dir.names(), planted, "a hosted session wrote or deleted a file by path");
        assert_eq!(s.documents().len(), documents, "a hosted session opened a deck by path");

        // Bytes still work.
        s.execute("image.insert", &json!({ "name": "red.png", "data": RED })).unwrap();
        s.execute("file.exportBytes", &json!({ "format": "pdf" })).unwrap();
        // With the host's reader a picture named by path is what the host lends, and nothing else.
        s.set_host_reader(Some(lend));
        s.execute("image.insert", &json!({ "path": "inside.png" })).unwrap();
        assert!(s.execute("image.insert", &json!({ "path": dir.at("planted.png") })).is_err(), "the host lent a file it does not lend");
        assert!(s.execute("file.open", &json!({ "path": dir.at("planted.kslide") })).is_err(), "a hosted session opened a deck by path");
        assert_eq!(dir.names(), planted);
        // And the same session not hosted reads the picture by path, as the app does.
        s.set_host_files_only(false);
        s.execute("image.insert", &json!({ "path": dir.at("planted.png") })).unwrap();
    }
}
