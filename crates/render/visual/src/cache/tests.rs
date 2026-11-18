use super::*;
use crate::compare::{DiffImage, Metrics};

struct Temp(PathBuf);
impl Temp {
    fn new() -> Self {
        static NEXT: AtomicU64 = AtomicU64::new(0);
        let now = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos();
        let path = std::env::temp_dir().join(format!("kition-metric-cache-{}-{now}-{}", std::process::id(), NEXT.fetch_add(1, Ordering::Relaxed)));
        fs::create_dir(&path).unwrap();
        Self(path)
    }
    fn cache(&self) -> MetricCache {
        MetricCache::new(&self.0)
    }
}
impl Drop for Temp {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

fn images(w: u32, h: u32) -> (Rgb, Rgb) {
    let mut reference = Rgb::new(w, h, [0; 3]);
    for (i, byte) in reference.data.iter_mut().enumerate() {
        *byte = ((i * 97 + i / 3 * 11) % 256) as u8;
    }
    let mut actual = reference.clone();
    for (i, byte) in actual.data.iter_mut().enumerate() {
        if i % 17 == 0 {
            *byte = byte.wrapping_add(13);
        }
    }
    (reference, actual)
}

fn assert_metrics(a: &Metrics, b: &Metrics) {
    assert_eq!(a, b);
    for (x, y) in [(a.ssim, b.ssim), (a.exact_equal, b.exact_equal), (a.diff_ratio, b.diff_ratio), (a.mean_abs_diff, b.mean_abs_diff)] {
        assert_eq!(x.to_bits(), y.to_bits());
    }
    assert_eq!(a.worst_tile.map(|(r, v)| (r, v.to_bits())), b.worst_tile.map(|(r, v)| (r, v.to_bits())));
    assert_eq!(a.to_json(), b.to_json());
}

fn assert_pair(a: &PairComparison, b: &PairComparison) {
    assert_metrics(&a.strict.metrics, &b.strict.metrics);
    assert_eq!(a.strict.max_diff, b.strict.max_diff);
    let (a, b) = (a.layout.as_ref().unwrap(), b.layout.as_ref().unwrap());
    assert_eq!(a.ssim.to_bits(), b.ssim.to_bits());
    assert_eq!(a.diff_ratio.to_bits(), b.diff_ratio.to_bits());
    assert_eq!(a.to_json(), b.to_json());
}

#[test]
fn cold_warm_and_disabled_preserve_raw_metrics_masks_and_images() {
    let temp = Temp::new();
    let cache = temp.cache();
    for (w, h) in [(1, 1), (10, 9), (11, 11), (32, 17), (65, 39)] {
        let (reference, actual) = images(w, h);
        let disabled = compare_pair(&reference, &actual, None).unwrap();
        assert_eq!(disabled.event.lookup, Lookup::Disabled);
        assert_metrics(&disabled.strict.metrics, &compare::compare(&reference, &actual).unwrap().metrics);
        let cold = compare_pair(&reference, &actual, Some(&cache)).unwrap();
        let warm = compare_pair(&reference, &actual, Some(&cache)).unwrap();
        assert_eq!(cold.event.lookup, Lookup::Miss);
        assert!(cold.event.written);
        assert_eq!(warm.event.lookup, Lookup::Hit);
        assert_pair(&disabled, &cold);
        assert_pair(&disabled, &warm);
        for mode in [DiffImage::None, DiffImage::Half, DiffImage::Full] {
            assert_eq!(
                compare::diff_image(&reference, &actual, &disabled.strict.max_diff, mode),
                compare::diff_image(&reference, &actual, &warm.strict.max_diff, mode)
            );
        }
    }
}

#[test]
fn identity_uses_ordered_rgb_dimensions_build_and_complete_configuration() {
    let (reference, actual) = images(4, 3);
    let key = Key::new(&reference, &actual, identity()).unwrap();
    let mut changed = actual.clone();
    changed.data[1] ^= 1;
    let mut resized = actual.clone();
    resized.width = 6;
    resized.height = 2;
    let mut resized_ref = reference.clone();
    resized_ref.width = 6;
    resized_ref.height = 2;
    for other in [
        Key::new(&reference, &changed, identity()).unwrap(),
        Key::new(&changed, &actual, identity()).unwrap(),
        Key::new(&actual, &reference, identity()).unwrap(),
        Key::new(&resized_ref, &resized, identity()).unwrap(),
        Key::new(&reference, &actual, "different-numerical-build-or-configuration").unwrap(),
    ] {
        assert_ne!(key.digest(), other.digest());
    }
    let a = json!({"z": [1, 2], "a": {"y": true, "b": null}});
    let b = json!({"a": {"b": null, "y": true}, "z": [1, 2]});
    assert_eq!(canonical(&a), canonical(&b));
    assert_eq!(Key::new(&reference.clone(), &actual.clone(), identity()).unwrap().digest(), key.digest());
}

fn resign(record: &mut Value) {
    record["checksum"] = json!(digest(b"metric-record", &canonical(&record["payload"])));
}

#[test]
fn corrupt_incomplete_incompatible_and_out_of_bounds_records_fail_closed() {
    let temp = Temp::new();
    let cache = temp.cache();
    let (reference, actual) = images(33, 33);
    let original = compare_pair(&reference, &actual, Some(&cache)).unwrap();
    let key = Key::new(&reference, &actual, identity()).unwrap();
    let path = cache.path(&key);
    let bytes = fs::read(&path).unwrap();
    let valid: Value = serde_json::from_slice(&bytes).unwrap();
    let mut mutations = vec![Vec::new(), bytes[..bytes.len() / 2].to_vec(), vec![b' '; MAX_RECORD as usize + 1]];
    for field in ["schema", "key", "ssim", "worst_tile", "layout"] {
        let mut value = valid.clone();
        value["payload"].as_object_mut().unwrap().remove(field);
        resign(&mut value);
        mutations.push(canonical(&value));
    }
    for (path, value) in [
        (vec!["schema"], json!("old-schema")),
        (vec!["key", "identity"], json!("old-build")),
        (vec!["ssim"], json!("7ff8000000000000")),
        (vec!["layout", "diff_ratio"], json!("4000000000000000")),
        (vec!["worst_tile"], Value::Null),
        (vec!["worst_tile", "rect"], json!([0, 0, 1, 1])),
    ] {
        let mut record = valid.clone();
        let mut at = &mut record["payload"];
        for key in path {
            at = &mut at[key];
        }
        *at = value;
        resign(&mut record);
        mutations.push(canonical(&record));
    }
    let mut checksum = valid.clone();
    checksum["checksum"] = json!("invalid-checksum");
    mutations.push(canonical(&checksum));
    for bytes in mutations {
        fs::write(&path, bytes).unwrap();
        let result = compare_pair(&reference, &actual, Some(&cache)).unwrap();
        assert_eq!(result.event.lookup, Lookup::Invalid);
        assert_pair(&original, &result);
        // Platforms supporting atomic replacement repair the bad entry. Others keep
        // missing conservatively rather than allowing an unchecked cache hit.
        if result.event.written {
            assert_eq!(cache.load(&key).0, Lookup::Hit);
        }
    }
}

#[test]
fn float_bits_preserve_threshold_decisions_and_required_null_tiles() {
    let (reference, actual) = images(1, 1);
    let key = Key::new(&reference, &actual, identity()).unwrap();
    let base = compare::compare(&reference, &reference).unwrap().metrics;
    let below = f64::from_bits(compare::PASS_SSIM.to_bits() - 1);
    let above = f64::from_bits(compare::PASS_SSIM.to_bits() + 1);
    assert_eq!(compare::round(below, 6), compare::round(above, 6));
    for value in [below, above, -0.0, 1.0000000000000002] {
        let metrics = CachedMetrics { ssim: value, worst_tile: None, layout: LayoutMetrics { ssim: value, diff_ratio: -0.0 } };
        let bytes = record::encode(&key, metrics).unwrap();
        let decoded = record::decode(&bytes, &key).unwrap();
        assert_eq!(decoded.ssim.to_bits(), value.to_bits());
        assert_eq!(decoded.layout.diff_ratio.to_bits(), (-0.0f64).to_bits());
        let current = Metrics { ssim: decoded.ssim, ..base.clone() };
        assert_eq!(current.pass(), value >= compare::PASS_SSIM);
        assert_eq!(current.pass_local(), value >= compare::PASS_SSIM);
        assert_eq!(decoded.layout.pass(), value >= compare::PASS_SSIM);
    }
}

#[test]
fn five_concurrent_writers_publish_one_complete_record() {
    let temp = Temp::new();
    let cache = temp.cache();
    let (reference, actual) = images(65, 39);
    let oracle = compare_pair(&reference, &actual, None).unwrap();
    let barrier = std::sync::Barrier::new(5);
    std::thread::scope(|scope| {
        let mut workers = Vec::new();
        for _ in 0..5 {
            workers.push(scope.spawn(|| {
                barrier.wait();
                compare_pair(&reference, &actual, Some(&cache)).unwrap()
            }));
        }
        let results: Vec<_> = workers.into_iter().map(|worker| worker.join().unwrap()).collect();
        assert_eq!(results.iter().filter(|r| r.event.written).count(), 1);
        for result in results {
            assert!(!result.event.write_error);
            assert_pair(&oracle, &result);
        }
    });
    let warm = compare_pair(&reference, &actual, Some(&cache)).unwrap();
    assert_eq!(warm.event.lookup, Lookup::Hit);
    assert_pair(&oracle, &warm);
    for shard in fs::read_dir(&temp.0).unwrap() {
        for file in fs::read_dir(shard.unwrap().path()).unwrap() {
            assert!(!file.unwrap().file_name().to_string_lossy().ends_with(".tmp"));
        }
    }
}

#[test]
fn invalid_images_and_unwritable_cache_do_not_hide_comparison_failures() {
    let temp = Temp::new();
    let cache_file = temp.0.join("regular-file");
    fs::write(&cache_file, b"not a directory").unwrap();
    let cache = MetricCache::new(&cache_file);
    let (reference, actual) = images(12, 12);
    let oracle = compare_pair(&reference, &actual, None).unwrap();
    let result = compare_pair(&reference, &actual, Some(&cache)).unwrap();
    assert!(result.event.write_error);
    assert_pair(&oracle, &result);
    for bad in [Rgb::new(0, 0, [0; 3]), Rgb::new(1, 2, [0; 3]), Rgb { data: vec![0; 7], ..actual.clone() }] {
        assert!(compare_pair(&reference, &bad, Some(&cache)).is_err());
        assert!(compare_pair(&reference, &bad, None).is_err());
    }
}

#[test]
fn numerical_identity_covers_real_kernel_bits_and_canonical_configuration() {
    let config = identity_config();
    assert_eq!(config["kernel"], json!(compare::kernel().map(|x| format!("{:016x}", x.to_bits()))));
    assert_eq!(config["blur_kernel"], json!(compare::blur_kernel().map(|x| format!("{:016x}", x.to_bits()))));
    let original = digest(b"metric-identity", &canonical(&config));
    assert_eq!(original, identity());
    for field in ["schema", "build", "manifest", "float_bits", "integers", "kernel", "blur_kernel"] {
        let mut changed = config.clone();
        changed[field] = json!("different");
        assert_ne!(digest(b"metric-identity", &canonical(&changed)), original);
    }
    let mut changed = config.clone();
    changed["kernel"][0] = json!(format!("{:016x}", compare::kernel()[0].to_bits() + 1));
    assert_ne!(digest(b"metric-identity", &canonical(&changed)), original);
    assert!(directory(None).is_none());
    assert!(directory(Some(String::new())).is_none());
    assert!(directory(Some("OFF".into())).is_none());
    assert_eq!(directory(Some("metrics".into())), Some(PathBuf::from("metrics")));
}

#[test]
fn cached_image_metrics_do_not_reuse_model_features_or_region_ownership() {
    use crate::{regions, runner};
    use kition_slide_model::{Shape, ShapeKind, SlideFile, Xfrm};
    use std::sync::Arc;

    // Both models draw the same white image, but the second has a transparent group
    // covering it. Diagnostics intentionally attribute differing pixels to that group.
    let first = SlideFile::default();
    let mut second = first.clone();
    Arc::make_mut(&mut second.slides[0]).shapes.push(Shape {
        kind: ShapeKind::Group { children: Vec::new(), child: Xfrm::new(0.0, 0.0, 960.0, 540.0) },
        xfrm: Some(Xfrm::new(0.0, 0.0, 960.0, 540.0)),
        ..Default::default()
    });
    let a = runner::render_clean(&first, 0, 32, 18, runner::FROZEN_DATE);
    let b = runner::render_clean(&second, 0, 32, 18, runner::FROZEN_DATE);
    assert_eq!(a, b);
    let reference = Rgb::new(32, 18, [0; 3]);
    let temp = Temp::new();
    let cache = temp.cache();
    let cold = compare_pair(&reference, &a, Some(&cache)).unwrap();
    let warm = compare_pair(&reference, &b, Some(&cache)).unwrap();
    assert_eq!(warm.event.lookup, Lookup::Hit);
    assert_pair(&cold, &warm);
    let attrib = |pres: &SlideFile, diff: &[u8]| {
        regions::attribute(pres, &pres.slides[0], 32.0 / pres.slide_size.width, 18.0 / pres.slide_size.height, 32, 18, diff, compare::DIFF_THRESHOLD)
    };
    let before = attrib(&first, &cold.strict.max_diff);
    let after = attrib(&second, &warm.strict.max_diff);
    assert_ne!(before, after);
    assert!(after.get("group").is_some_and(|n| *n > 0));
    assert_ne!(regions::features(&first, &first.slides[0]), regions::features(&second, &second.slides[0]));
}
