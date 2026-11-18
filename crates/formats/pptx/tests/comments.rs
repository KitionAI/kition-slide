//! Comments in PowerPoint packages: legacy comment parts (`p:cmLst`, replies threaded through
//! `p15:threadingInfo`) and modern ones (`p188:cmLst` with `authors.xml`), read and written back
//! with what each form keeps.

use std::io::{Read, Write};
use std::sync::Arc;

use kition_slide_model::{Comment, CommentKept, SlideFile, defaults};

fn round(p: &SlideFile) -> SlideFile {
    let bytes = kition_slide_pptx::export(p).expect("export");
    kition_slide_pptx::import(&bytes).expect("import")
}

fn parts(bytes: &[u8]) -> Vec<(String, Vec<u8>)> {
    let mut z = zip::ZipArchive::new(std::io::Cursor::new(bytes)).expect("zip");
    (0..z.len())
        .map(|i| {
            let mut e = z.by_index(i).expect("entry");
            let mut d = Vec::new();
            e.read_to_end(&mut d).expect("read");
            (e.name().to_string(), d)
        })
        .collect()
}

fn part_text(bytes: &[u8], prefix: &str) -> String {
    parts(bytes).into_iter().find(|(n, _)| n.starts_with(prefix)).map(|(_, d)| String::from_utf8(d).expect("utf-8")).unwrap_or_default()
}

/// `bytes` with the first part whose name starts with `prefix` replaced by `xml`.
fn replace_part(bytes: &[u8], prefix: &str, xml: &str) -> Vec<u8> {
    let mut zout = zip::ZipWriter::new(std::io::Cursor::new(Vec::new()));
    let mut done = false;
    for (name, mut data) in parts(bytes) {
        if !done && name.starts_with(prefix) {
            data = xml.as_bytes().to_vec();
            done = true;
        }
        zout.start_file(name, zip::write::SimpleFileOptions::default()).expect("start");
        zout.write_all(&data).expect("write");
    }
    assert!(done, "no part {prefix}");
    zout.finish().expect("finish").into_inner()
}

fn said(author: &str, text: &str, date: &str) -> Comment {
    Comment { author: author.into(), text: text.into(), date: date.into(), ..Default::default() }
}

fn slide_file_with(comments: Vec<Comment>) -> SlideFile {
    let mut p = defaults::new_slide_file(None);
    Arc::make_mut(&mut p.slides[0]).comments = comments;
    p
}

#[test]
fn legacy_replies_thread_under_their_comment_and_round_trip() {
    let mut first = said("Ann", "Is this right?", "2026-09-09T14:21:45.983");
    first.initials = "AN".into();
    first.x = 127.0;
    first.y = 95.25;
    let mut reply = said("Bob", "Yes.", "2026-09-09T14:26:13.575");
    reply.x = 1.25;
    reply.y = 1.25;
    reply.kept = Some(Box::new(CommentKept { tz: Some(-480), ..Default::default() }));
    first.replies = vec![reply, said("Ann", "Thanks", "2026-09-09T14:30:00.000")];
    let p = slide_file_with(vec![first, said("Bob", "Another", "2026-09-09T15:00:00.000")]);
    let bytes = kition_slide_pptx::export(&p).expect("export");
    let xml = part_text(&bytes, "ppt/comments/comment");
    assert!(xml.contains("<p:cmLst"), "legacy part: {xml}");
    assert!(xml.contains(r#"<p15:threadingInfo xmlns:p15="http://schemas.microsoft.com/office/powerpoint/2012/main" timeZoneBias="-480"><p15:parentCm authorId="0" idx="1"/>"#), "{xml}");
    assert!(!parts(&bytes).iter().any(|(n, _)| n == "ppt/authors.xml"), "no modern authors part");

    let q = kition_slide_pptx::import(&bytes).expect("import");
    let c = &q.slides[0].comments;
    assert_eq!(c.len(), 2);
    assert_eq!((c[0].author.as_str(), c[0].initials.as_str(), c[0].text.as_str(), c[0].x, c[0].y), ("Ann", "AN", "Is this right?", 127.0, 95.25));
    assert_eq!(c[0].replies.iter().map(|r| (r.author.as_str(), r.text.as_str())).collect::<Vec<_>>(), vec![("Bob", "Yes."), ("Ann", "Thanks")]);
    assert_eq!(c[0].replies[0].kept.as_ref().and_then(|k| k.tz), Some(-480));
    assert_eq!((c[0].replies[0].x, c[0].replies[0].y), (1.25, 1.25));
    assert_eq!(c[1].text, "Another");
    assert!(c[1].replies.is_empty());
    assert_eq!(round(&q).slides[0].comments, q.slides[0].comments);
}

#[test]
fn a_legacy_reply_to_a_reply_joins_its_thread_and_unknown_extensions_stay() {
    // As PowerPoint 2013+ writes them (the corpus' comments slide has this shape), with an extra
    // extension of a kind Kition Slide does not know.
    let xml = r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<p:cmLst xmlns:a="http://schemas.openxmlformats.org/drawingml/2006/main" xmlns:r="http://schemas.openxmlformats.org/officeDocument/2006/relationships" xmlns:p="http://schemas.openxmlformats.org/presentationml/2006/main"><p:cm authorId="0" dt="2026-09-09T14:21:45.983" idx="1"><p:pos x="1016" y="762"/><p:text>One</p:text></p:cm><p:cm authorId="0" dt="2026-09-09T14:26:13.575" idx="2"><p:pos x="10" y="10"/><p:text>Two</p:text><p:extLst><p:ext uri="{C676402C-5697-4E1C-873F-D02D1690AC5C}"><p15:threadingInfo xmlns:p15="http://schemas.microsoft.com/office/powerpoint/2012/main" timeZoneBias="-480"><p15:parentCm authorId="0" idx="1"/></p15:threadingInfo></p:ext></p:extLst></p:cm><p:cm authorId="0" dt="2026-09-09T14:27:00.000" idx="3"><p:pos x="10" y="10"/><p:text>Three</p:text><p:extLst><p:ext uri="{C676402C-5697-4E1C-873F-D02D1690AC5C}"><p15:threadingInfo xmlns:p15="http://schemas.microsoft.com/office/powerpoint/2012/main" timeZoneBias="-480"><p15:parentCm authorId="0" idx="2"/></p15:threadingInfo></p:ext><p:ext uri="{FFFFFFFF-0000-0000-0000-000000000001}"><x:mine xmlns:x="urn:example:kept" v="1"/></p:ext></p:extLst></p:cm></p:cmLst>"#;
    let p = slide_file_with(vec![said("Ann", "placeholder", "2026-09-09T14:21:45.983")]);
    let bytes = replace_part(&kition_slide_pptx::export(&p).expect("export"), "ppt/comments/comment", xml);
    let q = kition_slide_pptx::import(&bytes).expect("import");
    let c = &q.slides[0].comments;
    assert_eq!(c.len(), 1);
    assert_eq!(c[0].replies.iter().map(|r| r.text.as_str()).collect::<Vec<_>>(), vec!["Two", "Three"]);
    let again = kition_slide_pptx::export(&q).expect("export");
    let xml = part_text(&again, "ppt/comments/comment");
    assert!(xml.contains(r#"<x:mine xmlns:x="urn:example:kept" v="1"/>"#), "{xml}");
    assert_eq!(xml.matches("p15:threadingInfo xmlns").count(), 2, "{xml}");
    assert_eq!(kition_slide_pptx::import(&again).expect("import").slides[0].comments, q.slides[0].comments);
}

#[test]
fn a_resolved_thread_is_written_as_modern_comments_and_round_trips() {
    let mut first = said("Ann", "Fix the title", "2026-10-19T10:00:00Z");
    first.resolved = true;
    first.x = 100.0;
    first.y = 50.0;
    first.replies = vec![said("Bob", "Done", "2026-10-19T10:05:00Z")];
    let p = slide_file_with(vec![first]);
    let bytes = kition_slide_pptx::export(&p).expect("export");
    let authors = part_text(&bytes, "ppt/authors.xml");
    assert!(authors.contains(r#"name="Ann""#) && authors.contains(r#"name="Bob""#), "{authors}");
    let xml = part_text(&bytes, "ppt/comments/modernComment_100_");
    assert!(xml.contains(r#"status="resolved""#) && xml.contains("<p188:replyLst>") && xml.contains(r#"sldId="256""#), "{xml}");
    assert!(!parts(&bytes).iter().any(|(n, _)| n.starts_with("ppt/comments/comment")), "no legacy parts");
    let rels = part_text(&bytes, "ppt/_rels/presentation.xml.rels");
    assert!(rels.contains("http://schemas.microsoft.com/office/2018/10/relationships/authors"), "{rels}");

    let q = kition_slide_pptx::import(&bytes).expect("import");
    let c = &q.slides[0].comments[0];
    assert!(c.resolved && c.is_modern());
    assert_eq!((c.text.as_str(), c.x, c.y), ("Fix the title", 100.0, 50.0));
    assert_eq!(c.replies.iter().map(|r| (r.author.as_str(), r.text.as_str())).collect::<Vec<_>>(), vec![("Bob", "Done")]);
    // The ids it was written with are kept, so writing it again names everything the same way.
    let twice = round(&q);
    assert_eq!(twice.slides[0].comments, q.slides[0].comments);
    let written = part_text(&kition_slide_pptx::export(&q).expect("export"), "ppt/comments/modernComment_100_");
    assert!(written.contains("Fix the title") && written.contains(r#"status="resolved""#), "{written}");
    assert_eq!(
        part_text(&kition_slide_pptx::export(&twice).expect("export"), "ppt/comments/modernComment_100_"),
        written,
        "a second save writes the same part"
    );
}

#[test]
fn a_modern_comment_keeps_its_anchor_body_status_and_people() {
    let p = slide_file_with(vec![Comment {
        kept: Some(Box::new(CommentKept { modern: true, ..Default::default() })),
        ..said("Ann", "x", "2026-10-19T10:00:00Z")
    }]);
    let shape = p.slides[0].shapes[0].id.0;
    let bytes = kition_slide_pptx::export(&p).expect("export");
    let authors = r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<p188:authorLst xmlns:a="http://schemas.openxmlformats.org/drawingml/2006/main" xmlns:r="http://schemas.openxmlformats.org/officeDocument/2006/relationships" xmlns:p188="http://schemas.microsoft.com/office/powerpoint/2018/8/main"><p188:author id="{11111111-2222-3333-4444-555555555555}" name="Ann Lee" initials="AL" userId="ann@example.com" providerId="AD"/><p188:author id="{66666666-7777-8888-9999-000000000000}" name="Bob" initials="B" userId="Bob" providerId="None"/></p188:authorLst>"#;
    let comments = format!(
        r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<p188:cmLst xmlns:a="http://schemas.openxmlformats.org/drawingml/2006/main" xmlns:r="http://schemas.openxmlformats.org/officeDocument/2006/relationships" xmlns:p188="http://schemas.microsoft.com/office/powerpoint/2018/8/main"><p188:cm id="{{AAAAAAAA-0000-0000-0000-000000000001}}" authorId="{{11111111-2222-3333-4444-555555555555}}" created="2026-10-19T09:00:00.000" status="closed"><ac:deMkLst xmlns:ac="http://schemas.microsoft.com/office/drawing/2013/main/command"><pc:docMk xmlns:pc="http://schemas.microsoft.com/office/powerpoint/2013/main/command"/><pc:sldMk xmlns:pc="http://schemas.microsoft.com/office/powerpoint/2013/main/command" cId="0" sldId="999"/><ac:spMk id="{shape}" creationId="{{BBBBBBBB-0000-0000-0000-000000000002}}"/></ac:deMkLst><p188:pos x="1270000" y="635000"/><p188:replyLst><p188:reply id="{{AAAAAAAA-0000-0000-0000-000000000003}}" authorId="{{66666666-7777-8888-9999-000000000000}}" created="2026-10-19T09:30:00.000"><p188:txBody><a:bodyPr/><a:lstStyle/><a:p><a:r><a:rPr lang="en-US"/><a:t>Agreed</a:t></a:r></a:p></p188:txBody></p188:reply></p188:replyLst><p188:txBody><a:bodyPr/><a:lstStyle/><a:p><a:r><a:rPr lang="en-US" b="1"/><a:t>Line one</a:t></a:r></a:p><a:p><a:r><a:rPr lang="en-US"/><a:t>Line two</a:t></a:r></a:p></p188:txBody><p188:extLst><p:ext xmlns:p="http://schemas.openxmlformats.org/presentationml/2006/main" uri="{{C1D0A4AA-0000-0000-0000-000000000004}}"><x:mine xmlns:x="urn:example:kept"/></p:ext></p188:extLst></p188:cm></p188:cmLst>"#
    );
    let bytes = replace_part(&replace_part(&bytes, "ppt/authors.xml", authors), "ppt/comments/modernComment_", &comments);
    let q = kition_slide_pptx::import(&bytes).expect("import");
    let c = &q.slides[0].comments[0];
    assert_eq!((c.author.as_str(), c.initials.as_str(), c.text.as_str()), ("Ann Lee", "AL", "Line one\nLine two"));
    assert_eq!(c.shape.map(|s| s.0), Some(shape));
    assert_eq!((c.x, c.y), (100.0, 50.0));
    assert!(!c.resolved);
    let kept = c.kept.as_deref().expect("kept");
    assert_eq!(
        (kept.guid.as_str(), kept.status.as_str(), kept.user_id.as_str(), kept.provider_id.as_str()),
        ("{AAAAAAAA-0000-0000-0000-000000000001}", "closed", "ann@example.com", "AD")
    );
    assert_eq!(c.replies[0].author, "Bob");

    let again = kition_slide_pptx::export(&q).expect("export");
    let xml = part_text(&again, "ppt/comments/modernComment_100_");
    for kept in [
        r#"id="{AAAAAAAA-0000-0000-0000-000000000001}" authorId="{11111111-2222-3333-4444-555555555555}" created="2026-10-19T09:00:00.000" status="closed""#,
        r#"creationId="{BBBBBBBB-0000-0000-0000-000000000002}""#,
        r#"sldId="256""#,
        r#"<a:rPr lang="en-US" b="1"/><a:t>Line one</a:t>"#,
        r#"<x:mine xmlns:x="urn:example:kept"/>"#,
        r#"<p188:pos x="1270000" y="635000"/>"#,
    ] {
        assert!(xml.contains(kept), "{kept} in {xml}");
    }
    let authors = part_text(&again, "ppt/authors.xml");
    assert!(
        authors.contains(r#"id="{11111111-2222-3333-4444-555555555555}" name="Ann Lee" initials="AL" userId="ann@example.com" providerId="AD""#),
        "{authors}"
    );
    assert_eq!(kition_slide_pptx::import(&again).expect("import").slides[0].comments, q.slides[0].comments);

    // Edited text is written from the text; a comment whose shape is gone anchors on its slide.
    let mut edited = q.clone();
    let c = &mut Arc::make_mut(&mut edited.slides[0]).comments[0];
    c.text = "New".into();
    c.shape = None;
    let xml = part_text(&kition_slide_pptx::export(&edited).expect("export"), "ppt/comments/modernComment_100_");
    assert!(xml.contains("<a:t>New</a:t>") && !xml.contains("Line one") && !xml.contains("spMk") && xml.contains("<pc:sldMkLst"), "{xml}");
}
