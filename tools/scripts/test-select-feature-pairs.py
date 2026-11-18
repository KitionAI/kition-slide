#!/usr/bin/env python3
"""Synthetic selection tests; no corpus documents or visual scores are fixtures."""

import copy
import importlib.util
from pathlib import Path
import unittest


spec = importlib.util.spec_from_file_location("select_feature_pairs", Path(__file__).with_name("select-feature-pairs.py"))
selector = importlib.util.module_from_spec(spec)
spec.loader.exec_module(selector)
CID = "c0000000000000001"


def case(features, cid=CID):
    return {
        "id": cid, "status": "ok", "set": "original", "mapping": "physical",
        "references": len(features), "model_slides": len(features), "hidden_slides": 0,
        "slides": [{"n": n, "slide": n, "status": "compared", "features": f,
                    "ssim": 0.9, "diff_ratio": 0.1}
                   for n, f in enumerate(features, 1)],
    }


def scorecard(*cases):
    return {"complete": True, "pending_cases": 0, "cases": list(cases),
            "sets": {"original": {"cases": len(cases), "slides": sum(len(c["slides"]) for c in cases)}}}


def selected(value):
    return selector.generate(value)["cases"][0]["pages"]


class SelectionTests(unittest.TestCase):
    def test_diversity_precedes_distance_or_object_counts(self):
        value = scorecard(case([{"shape": 1, "gradient": 1}, {"picture": 1},
                                {"shape": 100, "gradient": 50}]))
        self.assertEqual(selected(value), [1, 2])

    def test_scores_and_pass_flags_cannot_change_selection(self):
        value = scorecard(case([{"shape": 1}, {"shape": 1}, {"shape": 1}]))
        expected = selector.generate(value)
        for i, page in enumerate(value["cases"][0]["slides"]):
            page.update(ssim=1.0 if i == 1 else 0.0, diff_ratio=0.0 if i == 1 else 1.0, pass_local=i == 1)
        self.assertEqual(selector.generate(value), expected)
        self.assertEqual(selected(value), [1, 3])

    def test_title_and_empty_pages_yield_to_substantive_content(self):
        value = scorecard(case([{"text": 1}, {}, {"shape": 1}, {"text": 3}]))
        self.assertEqual(selected(value), [3, 4])

    def test_hidden_pages_excluded_with_two_visible_pages(self):
        value = scorecard(case([{"shape": 1}, {"picture": 1, "gradient": 1, "hidden-slide": 1}, {"text": 2}]))
        self.assertEqual(selected(value), [1, 3])

    def test_one_visible_page_retained_when_hidden_fallback_needed(self):
        value = scorecard(case([{"shape": 1, "hidden-slide": 1}, {"text": 1},
                                {"picture": 1, "gradient": 1, "hidden-slide": 1}]))
        self.assertEqual(selected(value), [2, 3])

    def test_single_page_is_not_duplicated_even_if_empty_or_hidden(self):
        for features in ({}, {"text": 1}, {"hidden-slide": 1}):
            self.assertEqual(selected(scorecard(case([features]))), [1])

    def test_reviewed_overrides_and_invalid_override(self):
        for cid, (pages, _) in selector.OVERRIDES.items():
            self.assertEqual(selected(scorecard(case([{"text": 2}] * max(pages), cid))), pages)
            with self.assertRaisesRegex(ValueError, "override"):
                selected(scorecard(case([{"text": 2}], cid)))

    def test_determinism_under_case_page_and_feature_reordering(self):
        a = case([{"shape": 1, "gradient": 1}, {"picture": 1}, {"shape": 2}])
        b = case([{"text": 2}], "c0000000000000002")
        value = scorecard(a, b)
        expected = selector.generate(value)
        value["cases"].reverse()
        for deck in value["cases"]:
            deck["slides"].reverse()
            for page in deck["slides"]:
                page["features"] = dict(reversed(list(page["features"].items())))
        self.assertEqual(selector.generate(value), expected)

    def test_rejects_partial_failed_and_unscored_receipts(self):
        base = scorecard(case([{"shape": 1}, {"picture": 1}]))
        mutations = [
            lambda s: s.update(complete=False),
            lambda s: s.update(pending_cases=1),
            lambda s: s["cases"][0].update(status="open-failed"),
            lambda s: s["cases"][0].update(sampled=True),
            lambda s: s["cases"][0].update(mapping="physical-mismatch"),
            lambda s: s["cases"][0]["slides"].pop(),
            lambda s: s["cases"][0]["slides"][0].update(status="aborted"),
            lambda s: s["cases"][0]["slides"][0].pop("ssim"),
            lambda s: s["cases"][0]["slides"][0].update(ssim=float("nan")),
            lambda s: s["cases"][0]["slides"][0].update(features={"shape": -1}),
        ]
        for mutate in mutations:
            with self.subTest(mutation=mutate):
                value = copy.deepcopy(base)
                mutate(value)
                with self.assertRaises(ValueError):
                    selector.generate(value)

    def test_rejects_duplicate_ids_pages_and_missing_decks(self):
        a = case([{"shape": 1}, {"picture": 1}])
        with self.assertRaisesRegex(ValueError, "duplicate case"):
            selector.generate(scorecard(a, copy.deepcopy(a)))
        a["slides"][1]["n"] = 1
        with self.assertRaisesRegex(ValueError, "duplicate page"):
            selector.generate(scorecard(a))
        value = scorecard(case([{"shape": 1}]), case([{"text": 2}], "c0000000000000002"))
        value["cases"].pop()
        with self.assertRaisesRegex(ValueError, "coverage totals"):
            selector.generate(value)


if __name__ == "__main__":
    unittest.main()
