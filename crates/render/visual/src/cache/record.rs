//! Small strict records: raw floating-point bits, identity and corruption detection.
use super::*;

fn bits(value: f64) -> Value {
    json!(format!("{:016x}", value.to_bits()))
}

fn float(value: &Value) -> Option<f64> {
    let text = value.as_str()?;
    if text.len() != 16 || !text.bytes().all(|c| c.is_ascii_hexdigit()) {
        return None;
    }
    let value = f64::from_bits(u64::from_str_radix(text, 16).ok()?);
    value.is_finite().then_some(value)
}

pub(super) fn encode(key: &Key, metrics: CachedMetrics) -> Option<Vec<u8>> {
    let payload = json!({
        "schema": SCHEMA, "key": key.value(), "ssim": bits(metrics.ssim),
        "worst_tile": metrics.worst_tile.map(|(rect, ssim)| json!({ "rect": rect, "ssim": bits(ssim) })),
        "layout": { "ssim": bits(metrics.layout.ssim), "diff_ratio": bits(metrics.layout.diff_ratio) },
    });
    let checksum = digest(b"metric-record", &canonical(&payload));
    let bytes = canonical(&json!({ "payload": payload, "checksum": checksum }));
    if bytes.len() as u64 > MAX_RECORD || decode(&bytes, key).is_none() {
        return None;
    }
    Some(bytes)
}

pub(super) fn decode(bytes: &[u8], key: &Key) -> Option<CachedMetrics> {
    if bytes.len() as u64 > MAX_RECORD {
        return None;
    }
    let record: Value = serde_json::from_slice(bytes).ok()?;
    if record.as_object()?.len() != 2 {
        return None;
    }
    let payload = record.get("payload")?;
    if payload.as_object()?.len() != 5
        || payload.get("schema")?.as_str()? != SCHEMA
        || payload.get("key")? != &key.value()
        || record.get("checksum")?.as_str()? != digest(b"metric-record", &canonical(payload))
    {
        return None;
    }
    let ssim = float(payload.get("ssim")?)?;
    let layout = payload.get("layout")?;
    if layout.as_object()?.len() != 2 {
        return None;
    }
    let layout = LayoutMetrics { ssim: float(layout.get("ssim")?)?, diff_ratio: float(layout.get("diff_ratio")?)? };
    if !(0.0..=1.0).contains(&layout.diff_ratio) {
        return None;
    }
    let tile = payload.get("worst_tile")?;
    let small = key.width < compare::WINDOW as u32 || key.height < compare::WINDOW as u32;
    let worst_tile = if tile.is_null() {
        if !small {
            return None;
        }
        None
    } else {
        if small || tile.as_object()?.len() != 2 {
            return None;
        }
        let values = tile.get("rect")?.as_array()?;
        if values.len() != 4 {
            return None;
        }
        let mut rect = [0u32; 4];
        for (dst, value) in rect.iter_mut().zip(values) {
            *dst = u32::try_from(value.as_u64()?).ok()?;
        }
        let half = (compare::WINDOW / 2) as u32;
        let side = compare::TILE as u32;
        let (ow, oh) = (key.width - compare::WINDOW as u32 + 1, key.height - compare::WINDOW as u32 + 1);
        let (x, y) = (rect[0].checked_sub(half)?, rect[1].checked_sub(half)?);
        if x >= ow
            || y >= oh
            || x % side != 0
            || y % side != 0
            || rect[2] != x.saturating_add(side).min(ow) + half
            || rect[3] != y.saturating_add(side).min(oh) + half
        {
            return None;
        }
        Some((rect, float(tile.get("ssim")?)?))
    };
    Some(CachedMetrics { ssim, worst_tile, layout })
}
