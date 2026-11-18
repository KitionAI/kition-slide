//! Preserve comment identities and native metadata while the standard part stays unchanged.

use kition_slide_model::Comment;
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, HashMap};

use crate::PptxError;
use crate::xml::{El, Node};

const URI: &str = "https://kition.ai/ns/slide/comments/1";
const MAX_STATE: usize = 16 * 1024 * 1024;

fn digest(root: &El, authors: &HashMap<String, [String; 4]>) -> String {
    fn visit(root: &El, authors: &HashMap<String, [String; 4]>, used: &mut BTreeMap<String, [String; 4]>, depth: usize) {
        if depth > crate::xml::MAX_DEPTH {
            return;
        }
        if let Some(id) = root.attr("authorId") {
            used.insert(id.to_owned(), authors.get(id).cloned().unwrap_or_default());
        }
        for child in root.elements() {
            visit(child, authors, used, depth + 1);
        }
    }
    let mut used = BTreeMap::new();
    visit(root, authors, &mut used, 0);
    let mut hash = Sha256::new();
    hash.update(root.to_xml().as_bytes());
    hash.update(serde_json::to_vec(&used).unwrap_or_default());
    format!("{:x}", hash.finalize())
}

pub(crate) fn write(bytes: Vec<u8>, comments: &[Comment], authors: &HashMap<String, [String; 4]>) -> Result<Vec<u8>, PptxError> {
    if comments.iter().all(|c| c.id.is_empty()) {
        return Ok(bytes);
    }
    let state = serde_json::to_string(comments).map_err(|error| PptxError::Write(error.to_string()))?;
    if state.len() > MAX_STATE {
        return Err(PptxError::Write("comment state exceeds 16 MiB".into()));
    }
    let mut doc = crate::xml::parse(&bytes)?;
    let fingerprint = digest(&doc.root, authors);
    let Some(first) = doc.root.elements_mut().find(|e| e.is("cm")) else { return Ok(bytes) };
    let prefix = first.prefix().to_owned();
    if first.child("extLst").is_none() {
        first.children.push(Node::El(El { name: format!("{prefix}:extLst"), ..Default::default() }));
    }
    if let Some(list) = first.elements_mut().find(|e| e.is("extLst")) {
        list.children.push(Node::El(El {
            name: format!("{prefix}:ext"),
            attrs: vec![("uri".into(), URI.into())],
            children: vec![Node::El(El {
                name: "k:comments".into(),
                attrs: vec![("xmlns:k".into(), URI.into()), ("fingerprint".into(), fingerprint)],
                children: vec![Node::Text(state)],
            })],
        }));
    }
    Ok(doc.root.to_xml().into_bytes())
}

/// A changed standard part wins over the extension. Never restore stale text or anchors.
pub(crate) fn read(root: &El, authors: &HashMap<String, [String; 4]>) -> Option<Vec<Comment>> {
    let first = root.children_named("cm").next()?;
    let state = first.child("extLst")?.children_named("ext").find(|e| e.attr("uri") == Some(URI))?.child("comments")?;
    let fingerprint = state.attr("fingerprint")?;
    let text = state.text();
    if text.len() > MAX_STATE {
        return None;
    }
    let mut plain = root.clone();
    strip(&mut plain);
    if digest(&plain, authors) != fingerprint {
        return None;
    }
    let comments: Vec<Comment> = serde_json::from_str(&text).ok()?;
    (comments.len() <= 10_000 && comments.iter().all(|c| c.replies.len() <= 1000)).then_some(comments)
}

pub(crate) fn strip(root: &mut El) {
    let Some(first) = root.elements_mut().find(|e| e.is("cm")) else { return };
    let empty = {
        let Some(list) = first.elements_mut().find(|e| e.is("extLst")) else { return };
        list.children.retain(|n| !matches!(n, Node::El(e) if e.is("ext") && e.attr("uri") == Some(URI)));
        list.children.is_empty()
    };
    if empty {
        first.children.retain(|n| !matches!(n, Node::El(e) if e.is("extLst")));
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    #[test]
    fn another_programs_text_edit_invalidates_the_native_state() {
        let comment = Comment { id: "thread".into(), head: "head".into(), text: "Before".into(), ..Default::default() };
        let authors = HashMap::new();
        let bytes =
            write(b"<p:cmLst xmlns:p=\"urn:test\"><p:cm><p:text>Before</p:text></p:cm></p:cmLst>".to_vec(), std::slice::from_ref(&comment), &authors)
                .unwrap();
        let mut doc = crate::xml::parse(&bytes).unwrap();
        assert_eq!(read(&doc.root, &authors), Some(vec![comment]));
        let body = doc.root.elements_mut().next().unwrap().elements_mut().find(|e| e.is("text")).unwrap();
        body.children = vec![Node::Text("After".into())];
        assert!(read(&doc.root, &authors).is_none());
    }

    #[test]
    fn another_programs_author_edit_invalidates_the_native_state() {
        let mut authors = HashMap::from([("0".to_owned(), ["Before".to_owned(), String::new(), String::new(), String::new()])]);
        let comment = Comment { id: "thread".into(), head: "head".into(), author: "Before".into(), ..Default::default() };
        let bytes = write(b"<p:cmLst xmlns:p=\"urn:test\"><p:cm authorId=\"0\"/></p:cmLst>".to_vec(), &[comment], &authors).unwrap();
        let doc = crate::xml::parse(&bytes).unwrap();
        assert!(read(&doc.root, &authors).is_some());
        authors.insert("0".into(), ["After".into(), String::new(), String::new(), String::new()]);
        assert!(read(&doc.root, &authors).is_none());
    }
}
