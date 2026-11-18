//! Justification for a standalone displayed equation; inline math keeps host alignment.
//! https://learn.microsoft.com/en-us/dotnet/api/documentformat.openxml.math.justification

use super::*;

/// Element and non-blank text children of `e`.
fn contents(e: &Element) -> (Vec<&Element>, bool) {
    let mut els = Vec::new();
    let mut text = false;
    for n in &e.children {
        match n {
            Node::Element(c) => els.push(c),
            Node::Text(t) => text |= !t.trim().is_empty(),
            Node::Raw(_) => {}
        }
    }
    (els, text)
}

pub(super) fn alignment(xml: &str) -> Option<Align> {
    let root = parse(xml)?;
    let (top, text) = contents(&root);
    let [paragraph] = top.as_slice() else { return None };
    if text || paragraph.name != "m:oMathPara" {
        return None;
    }
    let (children, text) = contents(paragraph);
    if text {
        return None;
    }
    let mut equation = false;
    let mut properties = None;
    for node in children {
        match node.name.as_str() {
            "m:oMath" if !equation => equation = true,
            "m:oMathParaPr" if properties.is_none() => properties = Some(node),
            _ => return None,
        }
    }
    if !equation {
        return None;
    }
    let mut value = "centerGroup";
    if let Some(properties) = properties {
        let (children, text) = contents(properties);
        if text {
            return None;
        }
        match children.as_slice() {
            [] => {}
            [jc] if jc.name == "m:jc" && contents(jc) == (Vec::new(), false) => {
                value = jc.attr("m:val").or_else(|| jc.attr("val")).unwrap_or("centerGroup");
            }
            _ => return None,
        }
    }
    // With one equation, center and centerGroup have identical placement. Multiple
    // equations require alignment-point layout and deliberately stay unsupported.
    match value {
        "left" => Some(Align::Left),
        "right" => Some(Align::Right),
        "center" | "centerGroup" => Some(Align::Center),
        _ => None,
    }
}

#[cfg(test)]
mod tests;
