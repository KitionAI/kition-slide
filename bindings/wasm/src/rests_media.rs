//! Media reused by byte equality, including recipients that stopped matching after a reread.

use super::*;

fn bytes(command: &str, params: &Value) -> Result<Option<Vec<u8>>, String> {
    let key = match command {
        "image.insert" | "audio.insert" | "video.insert" => "data",
        "shape.fill" => "picture",
        _ => return Ok(None),
    };
    if let Some(data) = params.get(key).and_then(Value::as_str) {
        let limit = MAX_LENT.saturating_mul(4).div_ceil(3).saturating_add(4);
        if !u64::try_from(data.len()).is_ok_and(|size| size <= limit) {
            return Err(params_failure("media data exceeds the allowed size"));
        }
        return cmd::base64_decode(data).map(Some).ok_or_else(|| params_failure("invalid media data"));
    }
    if key == "data"
        && let Some(path) = params.get("path").and_then(Value::as_str)
    {
        return lent_file(path).map(Some);
    }
    Ok(None)
}

pub(super) fn add(doc: &SlideDocument, command: &str, params: &Value, under: &mut kition_kit::Under) -> Result<(), String> {
    let Some(bytes) = bytes(command, params)? else { return Ok(()) };
    let Some(state) = doc.session.active() else { return Ok(()) };
    let Some(session) = state.kdm() else { return Ok(()) };
    under.covers.get_or_insert_with(Default::default).fields.push("blob".into());
    for media in state.doc.media.iter().filter(|media| media.data.as_slice() == bytes) {
        if let Some(id) = session.model().ids().media.object(media.id) {
            let id = json!(id.as_str());
            if !under.objects.contains(&id) {
                under.objects.push(id.clone());
            }
            if !under.premises.iter().any(|premise| premise.id == id) {
                under.premises.push(kition_kit::Premise::on(id).fields(["blob"]).positions(Vec::<String>::new()));
            }
        }
    }
    Ok(())
}

/// Compare retained bytes in bounded chunks. Missing bytes cannot establish a negative match.
fn matched(blob: &Value, desired: &[u8], history: &dyn kition_kit::HistoricalBlobs) -> Option<bool> {
    let size = usize::try_from(blob.get("size")?.as_u64()?).ok()?;
    let hash = blob.get("hash")?.as_str()?;
    if size != desired.len() {
        return Some(false);
    }
    for (chunk, want) in desired.chunks(1 << 20).enumerate() {
        let offset = chunk.checked_mul(1 << 20)?;
        let read = history.read(hash, offset, want.len()).ok()?;
        if read.len() != want.len() {
            return None;
        }
        if read != want {
            return Some(false);
        }
    }
    Some(true)
}

pub(super) fn covered(
    doc: &SlideDocument,
    command: &str,
    params: &Value,
    objects: &[kition_kit::CoveredAsk],
    history: &dyn kition_kit::HistoricalBlobs,
) -> Result<Option<kition_kit::CoveredAt>, String> {
    let Some(desired) = bytes(command, params)? else { return doc.covered(command, params, objects) };
    let Some(state) = doc.session.active() else { return Ok(None) };
    let Some(session) = state.kdm() else { return Ok(None) };
    let current: BTreeMap<String, &[u8]> = state
        .doc
        .media
        .iter()
        .filter_map(|media| Some((session.model().ids().media.object(media.id)?.as_str().to_owned(), media.data.as_slice())))
        .collect();
    let mut answer = doc.covered(command, params, objects)?.unwrap_or_default();
    let mut under: Vec<Value> = current.iter().filter(|(_, bytes)| **bytes == desired).map(|(id, _)| json!(id)).collect();
    for object in objects {
        let present = object.id.as_str().and_then(|id| current.get(id));
        let media = present.is_some()
            || object.before.contains_key("blob")
            || object.unknown.iter().any(|field| field == "blob" || field.starts_with("blob."));
        if !media {
            if !answer.covered.iter().any(|covered| covered.id == object.id) {
                answer.covered.push(kition_kit::Covered { id: object.id.clone(), covered: Some(false), ..Default::default() });
            }
            continue;
        }
        // Address coverage starts from the current under-set, which also includes media.
        // Replace an asked media item's membership there with the reconstructed basis value.
        answer.under.retain(|id| *id != object.id);
        let was = if object.created {
            Some(false)
        } else if object.unknown.iter().any(|field| field == "blob" || field.starts_with("blob.")) {
            None
        } else if let Some(blob) = object.before.get("blob") {
            matched(blob, &desired, history)
        } else {
            present.map(|bytes| *bytes == desired)
        };
        under.retain(|id| *id != object.id);
        if was == Some(true) {
            under.push(object.id.clone());
        }
        answer.covered.retain(|covered| covered.id != object.id);
        answer.covered.push(kition_kit::Covered { id: object.id.clone(), covered: was, ..Default::default() });
    }
    for id in under {
        if !answer.under.contains(&id) {
            answer.under.push(id);
        }
    }
    Ok(Some(answer))
}
