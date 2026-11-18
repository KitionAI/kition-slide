//! Optional metric reuse keyed exclusively by current opaque RGB pixels.
mod record;

use std::fs::{self, File, OpenOptions};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::sync::OnceLock;
use std::sync::atomic::{AtomicU64, Ordering};

use serde_json::{Value, json};
use sha2::{Digest, Sha256};

use crate::compare::{self, Comparison, LayoutMetrics, Rgb};

const SCHEMA: &str = "kition-visual-pixel-metrics/1";
const MAX_RECORD: u64 = 16 * 1024;

pub(crate) fn canonical(value: &Value) -> Vec<u8> {
    fn write(value: &Value, out: &mut String) {
        match value {
            Value::Array(values) => {
                out.push('[');
                for (i, value) in values.iter().enumerate() {
                    if i != 0 {
                        out.push(',');
                    }
                    write(value, out);
                }
                out.push(']');
            }
            Value::Object(values) => {
                out.push('{');
                let mut keys: Vec<_> = values.keys().collect();
                keys.sort();
                for (i, key) in keys.into_iter().enumerate() {
                    if i != 0 {
                        out.push(',');
                    }
                    out.push_str(&json!(key).to_string());
                    out.push(':');
                    write(&values[key], out);
                }
                out.push('}');
            }
            _ => out.push_str(&value.to_string()),
        }
    }
    let mut out = String::new();
    write(value, &mut out);
    out.into_bytes()
}

fn digest(domain: &[u8], bytes: &[u8]) -> String {
    let mut hash = Sha256::new();
    hash.update((domain.len() as u64).to_le_bytes());
    hash.update(domain);
    hash.update((bytes.len() as u64).to_le_bytes());
    hash.update(bytes);
    format!("{:x}", hash.finalize())
}

fn identity() -> &'static str {
    static ID: OnceLock<String> = OnceLock::new();
    ID.get_or_init(|| digest(b"metric-identity", &canonical(&identity_config())))
}

fn identity_config() -> Value {
    let bits = |x: f64| format!("{:016x}", x.to_bits());
    let float_bits = [
        compare::PASS_SSIM,
        compare::PASS_DIFF_RATIO,
        compare::PASS_TILE_SSIM,
        compare::SIGMA,
        compare::K1,
        compare::K2,
        compare::LAYOUT_BLUR_SIGMA,
        0.299,
        0.587,
        0.114,
    ]
    .map(bits);
    let kernel = compare::kernel().map(bits);
    let blur_kernel = compare::blur_kernel().map(bits);
    json!({
        "schema": SCHEMA, "build": env!("KITION_VISUAL_NUMERICAL_BUILD"),
        "manifest": compare::manifest(), "float_bits": float_bits,
        "integers": [compare::WINDOW, compare::LAYOUT_BLUR_RADIUS, compare::TILE, compare::DIFF_THRESHOLD as usize],
        "kernel": kernel, "blur_kernel": blur_kernel,
    })
}

pub(crate) fn directory(value: Option<String>) -> Option<PathBuf> {
    value.filter(|v| !v.is_empty() && !v.eq_ignore_ascii_case("off")).map(PathBuf::from)
}

#[derive(Clone, Debug)]
struct Key {
    identity: String,
    reference: String,
    actual: String,
    width: u32,
    height: u32,
}

impl Key {
    fn new(reference: &Rgb, actual: &Rgb, identity: &str) -> Option<Self> {
        let pixels = (reference.width as usize).checked_mul(reference.height as usize)?;
        let len = pixels.checked_mul(3)?;
        if pixels == 0
            || (reference.width, reference.height) != (actual.width, actual.height)
            || reference.data.len() != len
            || actual.data.len() != len
        {
            return None;
        }
        let image_hash = |image: &Rgb| {
            let mut hash = Sha256::new();
            hash.update(b"kition-opaque-rgb8/1\0");
            hash.update(image.width.to_le_bytes());
            hash.update(image.height.to_le_bytes());
            hash.update((image.data.len() as u64).to_le_bytes());
            hash.update(&image.data);
            format!("{:x}", hash.finalize())
        };
        Some(Self {
            identity: identity.into(),
            reference: image_hash(reference),
            actual: image_hash(actual),
            width: reference.width,
            height: reference.height,
        })
    }
    fn value(&self) -> Value {
        json!({ "identity": self.identity, "reference": self.reference, "actual": self.actual, "width": self.width, "height": self.height })
    }
    fn digest(&self) -> String {
        digest(b"ordered-image-pair", &canonical(&self.value()))
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
struct CachedMetrics {
    ssim: f64,
    worst_tile: Option<([u32; 4], f64)>,
    layout: LayoutMetrics,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) enum Lookup {
    #[default]
    Disabled,
    Hit,
    Miss,
    Invalid,
}

#[derive(Clone, Copy, Debug, Default)]
pub(crate) struct Event {
    lookup: Lookup,
    written: bool,
    write_error: bool,
}

#[derive(Default)]
pub(crate) struct Stats {
    hits: u64,
    misses: u64,
    invalid: u64,
    writes: u64,
    write_errors: u64,
}

impl Stats {
    pub(crate) fn add(&mut self, event: Event) {
        match event.lookup {
            Lookup::Hit => self.hits += 1,
            Lookup::Miss => self.misses += 1,
            Lookup::Invalid => {
                self.misses += 1;
                self.invalid += 1;
            }
            Lookup::Disabled => {}
        }
        self.writes += u64::from(event.written);
        self.write_errors += u64::from(event.write_error);
    }
    pub(crate) fn json(&self) -> Value {
        json!({ "hits": self.hits, "misses": self.misses, "invalid": self.invalid, "writes": self.writes, "write_errors": self.write_errors })
    }
}

pub(crate) struct MetricCache {
    root: PathBuf,
}

impl MetricCache {
    pub(crate) fn new(root: &Path) -> Self {
        Self { root: root.to_owned() }
    }
    fn path(&self, key: &Key) -> PathBuf {
        let digest = key.digest();
        self.root.join(&digest[..2]).join(format!("{digest}.json"))
    }
    fn load(&self, key: &Key) -> (Lookup, Option<CachedMetrics>) {
        let file = match File::open(self.path(key)) {
            Ok(file) => file,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return (Lookup::Miss, None),
            Err(_) => return (Lookup::Invalid, None),
        };
        let mut bytes = Vec::new();
        if file.take(MAX_RECORD + 1).read_to_end(&mut bytes).is_err() || bytes.len() as u64 > MAX_RECORD {
            return (Lookup::Invalid, None);
        }
        match record::decode(&bytes, key) {
            Some(metrics) => (Lookup::Hit, Some(metrics)),
            None => (Lookup::Invalid, None),
        }
    }
    fn save(&self, key: &Key, metrics: CachedMetrics) -> std::io::Result<bool> {
        let bytes = record::encode(key, metrics).ok_or_else(|| std::io::Error::other("invalid metric cache record"))?;
        let path = self.path(key);
        let parent = path.parent().ok_or_else(|| std::io::Error::other("missing cache directory"))?;
        fs::create_dir_all(parent)?;
        static NEXT: AtomicU64 = AtomicU64::new(0);
        let temp = parent.join(format!(".{}-{}-{}.tmp", key.digest(), std::process::id(), NEXT.fetch_add(1, Ordering::Relaxed)));
        let mut file = OpenOptions::new().write(true).create_new(true).open(&temp)?;
        struct Cleanup(PathBuf);
        impl Drop for Cleanup {
            fn drop(&mut self) {
                let _ = fs::remove_file(&self.0);
            }
        }
        let _cleanup = Cleanup(temp.clone());
        file.write_all(&bytes)?;
        drop(file);
        // Atomic create-if-absent. A corrupt entry can be replaced atomically; a racing
        // valid replacement has the same complete key. Unsupported filesystems simply
        // run without cache writes.
        match fs::hard_link(&temp, &path) {
            Ok(()) => Ok(true),
            Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => {
                if self.load(key).0 == Lookup::Hit {
                    return Ok(false);
                }
                fs::rename(&temp, &path)?;
                Ok(true)
            }
            Err(e) => Err(e),
        }
    }
}

pub(crate) struct PairComparison {
    pub(crate) strict: Comparison,
    pub(crate) layout: Result<LayoutMetrics, String>,
    pub(crate) event: Event,
}

pub(crate) fn compare_pair(reference: &Rgb, actual: &Rgb, cache: Option<&MetricCache>) -> Result<PairComparison, String> {
    let key = cache.and_then(|_| Key::new(reference, actual, identity()));
    let (lookup, cached) = match (cache, &key) {
        (Some(cache), Some(key)) => cache.load(key),
        _ => (Lookup::Disabled, None),
    };
    let mut event = Event { lookup, ..Default::default() };
    let strict = compare::compare_with_ssim(reference, actual, cached.map(|m| (m.ssim, m.worst_tile)))?;
    let layout = cached.map(|m| Ok(m.layout)).unwrap_or_else(|| compare::compare_layout(reference, actual));
    if cached.is_none()
        && let (Some(cache), Some(key), Ok(layout)) = (cache, key.as_ref(), &layout)
    {
        let metrics = CachedMetrics { ssim: strict.metrics.ssim, worst_tile: strict.metrics.worst_tile, layout: *layout };
        match cache.save(key, metrics) {
            Ok(written) => event.written = written,
            Err(_) => event.write_error = true,
        }
    }
    Ok(PairComparison { strict, layout, event })
}

#[cfg(test)]
#[path = "cache/tests.rs"]
mod tests;
