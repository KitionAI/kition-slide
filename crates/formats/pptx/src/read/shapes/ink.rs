//! InkML round pens become editable paths; unsupported brushes keep the MC fallback.
//!
//! W3C InkML defines trace channels and physical brush units. Office's content-part
//! profile supplies the relationship and object frame. Resizing that frame fits the
//! sample coordinates, independently of the pen width.

use kition_office_core::ink::inkml::{decode_xy, fitted_center_line, length_in_points};
use kition_slide_color::Rgba;
use kition_slide_geom::preset::FillMode;
use kition_slide_model::style::{ColorRef, Fill, Line, LineCap, LineJoin};
use kition_slide_model::{CustomPath, Geom, Shape};

use super::{IdCtx, Imp, Part, base, dml};
use crate::xml::El;

const INKML_NS: &str = "http://www.w3.org/2003/InkML";
const MAX_PART_BYTES: usize = 1_000_000;

pub(super) fn read(imp: &mut Imp, part: &Part, ctx: &mut IdCtx, content: &El) -> Option<Shape> {
    let rel = part.rels.get(content.attr("r:id")?)?;
    if rel.external || rel.kind != "customXml" || imp.pkg.get(&rel.target)?.len() > MAX_PART_BYTES {
        return None;
    }
    let doc = imp.pkg.xml(&rel.target)?;
    let ink = &doc.root;
    let namespace = if ink.prefix().is_empty() { "xmlns".to_string() } else { format!("xmlns:{}", ink.prefix()) };
    if !ink.is("ink")
        || !ink.attrs.iter().any(|(key, value)| key == &namespace && value == INKML_NS)
        || !ink_namespaces(ink, &std::collections::HashMap::new(), 0)
    {
        return None;
    }
    // Context inheritance, trace groups and variable-pressure brushes require a richer
    // adapter. Reject them atomically so the producer's fallback remains available.
    if ink.elements().any(|e| !matches!(e.local(), "definitions" | "trace" | "annotation" | "annotationXML")) {
        return None;
    }
    let mut traces = ink.children_named("trace");
    let trace = traces.next()?;
    if traces.next().is_some()
        || trace.attr("continuation").is_some()
        || trace.attr("priorRef").is_some()
        || trace.attr("type").is_some_and(|value| value != "penDown")
        || trace.elements().next().is_some()
    {
        return None;
    }
    let definitions = one_child(ink, "definitions")?;
    let context = referenced(definitions, "context", trace.attr("contextRef")?)?;
    if ["contextRef", "canvasTransformRef", "canvasRef", "traceFormatRef", "inkSourceRef"].iter().any(|name| context.attr(name).is_some())
        || context.elements().any(|e| !matches!(e.local(), "inkSource" | "timestamp"))
    {
        return None;
    }
    let source = one_child(context, "inkSource")?;
    let format = one_child(source, "traceFormat")?;
    let channels: Vec<_> = format.elements().collect();
    let [first, second] = channels.as_slice() else { return None };
    for channel in &channels {
        if !channel.is("channel")
            || !matches!(channel.attr("type"), None | Some("integer" | "decimal"))
            || !matches!(channel.attr("units"), Some("cm" | "mm" | "in" | "pt" | "pc"))
            || !matches!(channel.attr("orientation"), None | Some("+ve"))
            || channel.elements().next().is_some()
        {
            return None;
        }
    }
    let y_first = match (first.attr("name"), second.attr("name")) {
        (Some("X"), Some("Y")) => false,
        (Some("Y"), Some("X")) => true,
        _ => return None,
    };
    if let Some(properties) = source.child("channelProperties") {
        for property in properties.elements() {
            if property.attr("name") == Some("resolution") && !property.f64("value").is_some_and(|v| v.is_finite() && v > 0.0) {
                return None;
            }
        }
    }
    let brush = referenced(definitions, "brush", trace.attr("brushRef")?)?;
    let (width, color) = round_brush(brush)?;
    let points = decode_xy(&trace.text(), y_first).ok()?;
    let frame = one_child(content, "xfrm")?;
    for (child, attrs, signed) in [("off", ["x", "y"], true), ("ext", ["cx", "cy"], false)] {
        let values = one_child(frame, child)?;
        for attr in attrs {
            let value = values.f64(attr)? / 12700.0;
            if !value.is_finite() || value.abs() > kition_office_core::ops::MAX_LENGTH || (!signed && value < 0.0) {
                return None;
            }
        }
    }
    if frame.attr("rot").is_some() && !frame.f64("rot").is_some_and(f64::is_finite) {
        return None;
    }
    let xfrm = dml::xfrm(frame).0;
    let path = fitted_center_line(&points, xfrm.w, xfrm.h)?;
    // ID allocation is last: an unsupported choice must not consume its fallback's ID.
    let mut shape = base(imp, part, ctx, content.child("nvContentPartPr"));
    shape.xfrm = Some(xfrm);
    shape.geom = Geom::Custom {
        paths: vec![CustomPath {
            w: xfrm.w,
            h: xfrm.h, // The model's path syntax separates command letters from numbers.
            d: path.to_svg().replace('M', "M ").replace('L', "L "),
            fill: FillMode::None,
            stroke: true,
        }],
        source: None,
    };
    shape.fill = Some(Fill::None);
    shape.line = Some(Line { cap: Some(LineCap::Round), join: Some(LineJoin::Round), ..Line::solid(ColorRef::rgb(color), width) });
    Some(shape)
}

fn referenced<'a>(definitions: &'a El, kind: &'a str, reference: &str) -> Option<&'a El> {
    let id = reference.strip_prefix('#').filter(|id| !id.is_empty())?;
    let mut matches = definitions.children_named(kind).filter(|e| e.attrs.iter().any(|(key, value)| key == "xml:id" && value == id));
    let element = matches.next()?;
    matches.next().is_none().then_some(element)
}

fn round_brush(brush: &El) -> Option<(f64, Rgba)> {
    if brush.attr("brushRef").is_some() {
        return None;
    }
    let mut width = None;
    let mut height = None;
    let mut color = Rgba::BLACK;
    let mut names = std::collections::HashSet::new();
    for property in brush.elements() {
        if !property.is("brushProperty") || !names.insert(property.attr("name")?) {
            return None;
        }
        let value = property.attr("value")?;
        match property.attr("name")? {
            "width" => width = length_in_points(property.f64("value")?, property.attr("units")?),
            "height" => height = length_in_points(property.f64("value")?, property.attr("units")?),
            "color" => {
                let hex = value.strip_prefix('#').filter(|v| v.len() == 6 && v.bytes().all(|b| b.is_ascii_hexdigit()))?;
                let rgb = u32::from_str_radix(hex, 16).ok()?;
                color = Rgba::rgb((rgb >> 16) as u8, (rgb >> 8) as u8, rgb as u8);
            }
            "tip" if value == "ellipse" => {}
            "rasterOp" if value == "copyPen" => {}
            "transparency" if value == "0" => {}
            "antiAliased" if matches!(value, "1" | "true") => {}
            "fitToCurve" if matches!(value, "0" | "false") => {}
            // There are no pressure channels in the accepted two-channel profile.
            "ignorePressure" if matches!(value, "0" | "1" | "true" | "false") => {}
            _ => return None,
        }
    }
    let (width, height) = (width?, height?);
    ((width - height).abs() <= 1e-9).then_some((width, color))
}

fn ink_namespaces(el: &El, inherited: &std::collections::HashMap<String, String>, depth: usize) -> bool {
    if depth > 32 {
        return false;
    }
    let mut namespaces = inherited.clone();
    for (key, value) in &el.attrs {
        if key == "xmlns" {
            namespaces.insert(String::new(), value.clone());
        } else if let Some(prefix) = key.strip_prefix("xmlns:") {
            namespaces.insert(prefix.to_string(), value.clone());
        }
    }
    if namespaces.get(el.prefix()).map(String::as_str) != Some(INKML_NS) {
        return false;
    }
    el.is("annotationXML") || el.elements().all(|child| ink_namespaces(child, &namespaces, depth + 1))
}

fn one_child<'a>(parent: &'a El, name: &'a str) -> Option<&'a El> {
    let mut children = parent.children_named(name);
    let child = children.next()?;
    children.next().is_none().then_some(child)
}
