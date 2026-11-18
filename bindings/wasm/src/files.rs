//! The parameters of this kit's commands that name files or folders on disk (KHP A28), declared
//! as typed files in the commands' schemas (`kition_kit::files`): a host lends them, and a
//! headless kit keeps those commands from agents (the host opens, saves and exports documents).

use kition_kit::files::FileParam;

/// Every file or folder parameter of the kit's commands.
pub const FILES: &[FileParam] = &[
    FileParam::read("file.open", "path", &["kslide", "pptx", "potx", "ppsx"]),
    FileParam::write("file.save", "path", &["kslide", "pptx"]),
    FileParam::create("file.saveAs", "path", &["kslide", "pptx"]),
    FileParam::create("file.saveTemplate", "path", &["potx", "kslide"]),
    FileParam::create("file.export", "path", &["pdf", "png", "jpg", "jpeg", "pptx", "kslide", "txt", "md"]),
    FileParam::read("insert.picture", "path", PICTURES),
    FileParam::read("picture.change", "path", PICTURES),
    FileParam::read("insert.audio", "path", &["wav", "mp3", "m4a", "m4b", "aac", "flac", "ogg", "aiff", "aif", "mka", "weba", "wma"]),
    FileParam::read("insert.video", "path", &["mp4", "m4v", "mov", "ogv", "mkv", "webm", "wmv", "avi"]),
];

/// The pictures the engine recognizes by their bytes (`content_type`).
const PICTURES: &[&str] = &["png", "jpg", "jpeg", "gif", "webp", "bmp", "tif", "tiff", "svg", "emf", "wmf"];

#[cfg(test)]
mod tests {
    use kition_kit::KitDocument;

    use super::*;

    /// Parameters whose names read like files but are not: animation paths, directions, ink
    /// strokes, shape geometry.
    const NOT_FILES: &[(&str, &str)] =
        &[("drawing.erase", "path"), ("arrange.distribute", "dir"), ("text.delete", "dir"), ("format.direction", "dir")];

    #[test]
    fn every_file_parameter_is_declared() {
        let doc = crate::SlideDocument::create("Untitled.kslide", 64, 64).unwrap_or_else(|error| panic!("{error}"));
        let problems = kition_kit::files::check(&doc.commands(), FILES, NOT_FILES);
        assert!(problems.is_empty(), "{problems:#?}");
    }
}
