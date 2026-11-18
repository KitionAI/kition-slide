//! A reviewed, fixed pair of reference pages from every feature slide file.

use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

use serde_json::{Value, json};
use sha2::{Digest, Sha256};

use crate::corpus::Case;

#[derive(Clone, Debug)]
pub struct FeaturePairs {
    pub pages: BTreeMap<String, BTreeSet<u32>>,
    pub fingerprint: String,
}

impl FeaturePairs {
    pub fn load(path: &Path, cases: &[Case]) -> Result<Self, String> {
        if std::fs::metadata(path).map_err(|e| format!("feature manifest: {e}"))?.len() > 4 * 1024 * 1024 {
            return Err("feature manifest exceeds 4 MiB".into());
        }
        let bytes = std::fs::read(path).map_err(|e| format!("feature manifest: {e}"))?;
        let value: Value = serde_json::from_slice(&bytes).map_err(|e| format!("feature manifest: {e}"))?;
        Self::parse(&value, cases)
    }

    fn parse(value: &Value, cases: &[Case]) -> Result<Self, String> {
        if value["version"] != 1 {
            return Err("feature manifest requires version 1".into());
        }
        let entries = value["cases"].as_array().ok_or("feature manifest requires cases")?;
        let discovered: BTreeMap<_, _> = cases.iter().map(|c| (c.id.as_str(), c)).collect();
        let mut pages = BTreeMap::new();
        for entry in entries {
            let id = entry["id"].as_str().ok_or("feature manifest case requires an id")?;
            let case = discovered.get(id).ok_or_else(|| format!("feature manifest has unknown case {id}"))?;
            let numbers = entry["pages"].as_array().ok_or_else(|| format!("feature manifest {id} requires pages"))?;
            let mut selected = BTreeSet::new();
            for number in numbers {
                let n = number.as_u64().and_then(|n| u32::try_from(n).ok()).ok_or_else(|| format!("invalid page in {id}"))?;
                if !case.references.iter().any(|r| r.0 == n) || !selected.insert(n) {
                    return Err(format!("missing or duplicate reference {id} page {n}"));
                }
            }
            if selected.len() != case.references.len().min(2) || selected.is_empty() {
                return Err(format!("{id} requires two distinct pages, or its only reference page"));
            }
            if pages.insert(id.to_owned(), selected).is_some() {
                return Err(format!("duplicate feature manifest case {id}"));
            }
        }
        if pages.len() != discovered.len() {
            return Err("feature manifest does not cover every discovered slide file; regenerate and review it".into());
        }
        // Page membership, not comments or JSON formatting, defines the frozen selection.
        let encoded = serde_json::to_vec(&pages).map_err(|e| e.to_string())?;
        let fingerprint = format!("{:x}", Sha256::digest(encoded));
        Ok(Self { pages, fingerprint })
    }

    pub fn receipt(&self, id: &str) -> Value {
        json!({"kind": "feature-pairs-v1", "fingerprint": self.fingerprint, "pages": self.pages.get(id)})
    }

    /// Never resume a full run, a different pair, or a receipt with missing selected pages.
    pub fn validate_result(&self, id: &str, result: &Value) -> Result<(), String> {
        if result["id"].as_str() != Some(id) {
            return Err(format!("{id} contains another slide file's result; use a fresh output directory"));
        }
        if result["selection"] != self.receipt(id) || result["sampled"] != true {
            return Err(format!("{id} has a different selection; use a fresh output directory"));
        }
        let slides = result["slides"].as_array().ok_or_else(|| format!("{id} has no page results"))?;
        let numbers: BTreeSet<u32> = slides.iter().filter_map(|s| s["n"].as_u64().and_then(|n| u32::try_from(n).ok())).collect();
        if Some(&numbers) != self.pages.get(id) || slides.len() != numbers.len() {
            return Err(format!("{id} is missing or duplicating selected page results"));
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cases() -> Vec<Case> {
        [("c1", vec![1, 2, 3, 4]), ("c2", vec![7])]
            .into_iter()
            .map(|(id, numbers)| Case {
                id: id.into(),
                rel: format!("{id}.pptx"),
                category: String::new(),
                slide_file: Default::default(),
                references: numbers.into_iter().map(|n| (n, Default::default())).collect(),
                derived: false,
            })
            .collect()
    }

    fn manifest() -> Value {
        json!({"version":1,"cases":[{"id":"c1","pages":[2,4]},{"id":"c2","pages":[7]}]})
    }

    #[test]
    fn enforces_complete_deck_coverage_and_real_unique_pairs() {
        assert!(FeaturePairs::parse(&manifest(), &cases()).is_ok());
        for bad in [json!([]), json!([2]), json!([2, 2]), json!([2, 9]), json!([1, 2, 3]), json!([0, 2]), json!([2, 4.5])] {
            let mut value = manifest();
            value["cases"][0]["pages"] = bad;
            assert!(FeaturePairs::parse(&value, &cases()).is_err());
        }
        let mut value = manifest();
        value["cases"].as_array_mut().unwrap().pop();
        assert!(FeaturePairs::parse(&value, &cases()).is_err());
        let mut value = manifest();
        value["cases"][1]["id"] = json!("unknown");
        assert!(FeaturePairs::parse(&value, &cases()).is_err());
    }

    #[test]
    fn resume_requires_the_exact_selection_and_every_selected_result() {
        let pairs = FeaturePairs::parse(&manifest(), &cases()).unwrap();
        let mut result = json!({"id":"c1","sampled":true,"selection":pairs.receipt("c1"),"slides":[{"n":2},{"n":4}]});
        assert!(pairs.validate_result("c1", &result).is_ok());
        result["slides"] = json!([{"n":2}]);
        assert!(pairs.validate_result("c1", &result).is_err());
        result["slides"] = json!([{"n":2},{"n":2},{"n":4}]);
        assert!(pairs.validate_result("c1", &result).is_err());
        result["slides"] = json!([{"n":2},{"n":4}]);
        result["selection"] = Value::Null;
        assert!(pairs.validate_result("c1", &result).is_err());
    }

    #[test]
    fn notes_do_not_change_identity_but_page_changes_do() {
        let first = FeaturePairs::parse(&manifest(), &cases()).unwrap();
        let mut value = manifest();
        value["notes"] = json!("Reviewed feature coverage");
        value["cases"][0]["pages"] = json!([4, 2]);
        assert_eq!(first.fingerprint, FeaturePairs::parse(&value, &cases()).unwrap().fingerprint);
        value["cases"][0]["pages"] = json!([1, 2]);
        assert_ne!(first.fingerprint, FeaturePairs::parse(&value, &cases()).unwrap().fingerprint);
    }
}
