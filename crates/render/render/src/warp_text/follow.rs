//! Rigid shaped-cluster placement on the SDK's single-path text arcs.
//! Single lines anchored top, middle or bottom follow the actual guide with their unwarped ink
//! center, at their own size. Justified and distributed anchors remain deferred.

use kition_office_core::warp::follow::FollowPath;
use kition_slide_model::text::{Align, Anchor};
use kition_slide_text::{LineInfo, TextLayout};
use kurbo::{Affine, BezPath, Rect, Shape as _};

/// Cluster bounds include all ligature caret subdivisions. Build once per line, including
/// whitespace, so long text does not rescan all character edges for every glyph.
fn cluster_positions(line: &LineInfo) -> Option<std::collections::BTreeMap<usize, (f64, f64)>> {
    if line.cluster_heads.len() != line.edges.len() {
        return None;
    }
    let mut positions = std::collections::BTreeMap::new();
    for (&head, &(a, b)) in line.cluster_heads.iter().zip(&line.edges) {
        if !a.is_finite() || !b.is_finite() || b < a {
            return None;
        }
        let entry = positions.entry(head).or_insert((a, b));
        entry.0 = entry.0.min(a);
        entry.1 = entry.1.max(b);
    }
    Some(positions)
}

fn cluster_transform(path: &FollowPath, line: &LineInfo, (left, right): (f64, f64)) -> Option<Affine> {
    let center = (left + right) / 2.0;
    let (start, end) = line.extent;
    if ![center, start, end, line.baseline].into_iter().all(f64::is_finite) || end < start {
        return None;
    }
    let slack = path.length() - (end - start);
    let offset = match line.alignment? {
        Align::Center => slack / 2.0,
        Align::Right => slack,
        Align::Left => 0.0,
        Align::Distributed if line.end.saturating_sub(line.start) <= 1 => slack / 2.0,
        Align::Justify | Align::JustLow | Align::Distributed if line.rtl => slack,
        Align::Justify | Align::JustLow | Align::Distributed => 0.0,
    };
    let sample = path.sample(offset + center - start)?;
    let t = sample.tangent;
    Some(Affine::translate(sample.point.to_vec2()) * Affine::new([t.x, t.y, -t.y, t.x, 0.0, 0.0]) * Affine::translate((-center, -line.baseline)))
}

fn bounds(paths: &[Vec<BezPath>]) -> Option<Rect> {
    paths.iter().flatten().filter(|p| !p.is_empty()).map(|p| p.bounding_box()).reduce(|a, b| a.union(b)).filter(|r| r.is_finite())
}

pub(super) fn prepare(l: &TextLayout, tr: Rect, preset: &str, adjust: &[(String, f64)], anchor: Anchor) -> Option<Vec<Vec<BezPath>>> {
    let path = FollowPath::from_preset(preset, adjust, tr.width(), tr.height())?;
    // Decorations, multiline, vertical text, bullets and formula runs need separate placement contracts.
    // Leave them on the existing renderer until their source-backed policy is established.
    // A single line follows the guide whatever its vertical anchor (PowerPoint's Follow Path
    // warps place the line on the path; top, middle and bottom alike). Justified and
    // distributed anchors stay on the existing renderer.
    if !matches!(anchor, Anchor::Middle | Anchor::Top | Anchor::Bottom)
        || !l.decos.is_empty()
        || l.lines.len() != 1
        || l.rotation != 0.0
        || !tr.is_finite()
        || l.runs.iter().any(|r| r.upright || r.cells.len() != r.glyphs.len() || (r.fake_italic && (r.fake_bold || r.outline.is_some())))
    {
        return None;
    }
    let line = l.lines.first()?;
    let transforms: std::collections::BTreeMap<usize, Affine> =
        cluster_positions(line)?.into_iter().map(|(head, span)| Some((head, cluster_transform(&path, line, span)?))).collect::<Option<_>>()?;
    // Preserve glyph/cell correspondence, including spaces, while outlining only once.
    let mut flat = Vec::with_capacity(l.runs.len());
    for run in &l.runs {
        if run.para != line.para {
            return None;
        }
        let paths: Vec<_> = run.glyphs.iter().map(|&(gid, x, y)| super::glyph_outline(run, gid, x, y)).collect();
        if paths.iter().any(|p| !p.is_finite()) {
            return None;
        }
        flat.push(paths);
    }
    // All-whitespace text has no foreground; it is still handled without a fake glyph.
    let Some(before) = bounds(&flat) else { return Some(flat) };
    // The guide carries the ink center, not a flattened text-box baseline. This translation
    // happens before each cluster rotation, so its offset follows the local path normal.
    let center_to_baseline = Affine::translate((0.0, line.baseline - before.center().y));
    let back = Affine::translate(tr.origin().to_vec2());
    let mut bent = Vec::with_capacity(l.runs.len());
    for (run, paths) in l.runs.iter().zip(flat) {
        let mut placed = Vec::with_capacity(paths.len());
        for (p, &(head, _)) in paths.into_iter().zip(&run.cells) {
            if p.is_empty() {
                continue;
            }
            let at = *transforms.get(&head)?;
            let p = back * at * center_to_baseline * p;
            if !p.is_finite() {
                return None;
            }
            placed.push(p);
        }
        bent.push(placed);
    }
    Some(bent)
}

#[cfg(test)]
mod tests;
