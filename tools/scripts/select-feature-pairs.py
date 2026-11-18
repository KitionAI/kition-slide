#!/usr/bin/env python3
"""Freeze two feature-diverse pages per deck without consulting fidelity scores."""

import argparse
import json
import math
from pathlib import Path
import re
import sys


OVERRIDES = {
    "ca973cd4290e67cd8": ([2, 20], "Reviewed formula coverage: introductory and advanced structures."),
    "c931c5c093926d870": ([18, 22], "Reviewed art text coverage: distinct text treatments."),
}
CONTEXT_FEATURES = {"cjk-text", "hidden-slide", "no-layout"}


def positive_int(value):
    return type(value) is int and value > 0


def validate(scorecard):
    """Reject incomplete receipts before deriving a smaller, authoritative scope."""
    if not isinstance(scorecard, dict):
        raise ValueError("scorecard must be an object")
    if scorecard.get("complete") is not True or scorecard.get("pending_cases") != 0:
        raise ValueError("scorecard must be complete with zero pending cases")
    cases = scorecard.get("cases")
    if not isinstance(cases, list) or not cases:
        raise ValueError("scorecard must contain cases")
    sets = scorecard.get("sets")
    if not isinstance(sets, dict) or not sets:
        raise ValueError("scorecard must declare set coverage totals")
    ids = set()
    totals = {}
    for case in cases:
        if not isinstance(case, dict):
            raise ValueError("case must be an object")
        cid = case.get("id")
        if not isinstance(cid, str) or not re.fullmatch(r"c[0-9a-f]{16}", cid):
            raise ValueError("case ID must be a stable ASCII corpus ID")
        if cid in ids:
            raise ValueError(f"duplicate case ID: {cid}")
        ids.add(cid)
        if case.get("status") != "ok" or case.get("sampled", False):
            raise ValueError(f"{cid}: requires an unsampled, successful case")
        if case.get("mapping") not in ("physical", "visible"):
            raise ValueError(f"{cid}: ambiguous reference mapping")
        slides = case.get("slides")
        if not isinstance(slides, list) or not slides:
            raise ValueError(f"{cid}: missing pages")
        if not positive_int(case.get("references")) or case["references"] != len(slides):
            raise ValueError(f"{cid}: incomplete reference coverage")
        seen = set()
        physical = set()
        for page in slides:
            if not isinstance(page, dict):
                raise ValueError(f"{cid}: page must be an object")
            n, slide = page.get("n"), page.get("slide")
            if not positive_int(n) or n in seen or not positive_int(slide) or slide in physical:
                raise ValueError(f"{cid}: invalid or duplicate page number")
            seen.add(n)
            physical.add(slide)
            if page.get("status") != "compared":
                raise ValueError(f"{cid}: every page must have been compared")
            for key in ("ssim", "diff_ratio"):
                value = page.get(key)
                if type(value) not in (int, float) or not math.isfinite(value):
                    raise ValueError(f"{cid}: missing finite comparison result")
            features = page.get("features")
            if not isinstance(features, dict) or any(
                not isinstance(k, str) or not positive_int(v) for k, v in features.items()
            ):
                raise ValueError(f"{cid}: invalid feature metadata")
        name = case.get("set")
        if not isinstance(name, str) or name not in sets:
            raise ValueError(f"{cid}: missing set coverage")
        count, pages = totals.get(name, (0, 0))
        totals[name] = (count + 1, pages + len(slides))
    for name, summary in sets.items():
        if not isinstance(summary, dict) or totals.get(name, (0, 0)) != (
            summary.get("cases"), summary.get("slides")):
            raise ValueError("set coverage totals do not match case receipts")
    return cases


def feature_types(page):
    return set(page["features"]) - CONTEXT_FEATURES


def content_rank(page):
    features = feature_types(page)
    if not features:
        return 0
    if features <= {"text"} and page["features"].get("text", 0) <= 1:
        return 1
    return 2


def select_case(case):
    pages = sorted(case["slides"], key=lambda page: page["n"])
    count = min(2, len(pages))
    override = OVERRIDES.get(case["id"])
    if override:
        requested, reason = override
        if len(requested) != count or not set(requested) <= {page["n"] for page in pages}:
            raise ValueError(f"{case['id']}: reviewed override pages are unavailable")
        return {"id": case["id"], "pages": requested, "reasons": [reason]}
    if count == 1:
        return {"id": case["id"], "pages": [pages[0]["n"]],
                "reasons": ["Single-page deck; retain its only reference page."]}
    visible = [page for page in pages if "hidden-slide" not in page["features"]]
    pool = visible if len(visible) >= count else pages
    # If only one visible page exists, retain it before considering hidden pages.
    first_pool = visible if len(visible) == 1 else pool
    first = max(first_pool, key=lambda page: (
        content_rank(page), len(feature_types(page)), -page["n"]))
    remaining = [page for page in pool if page["n"] != first["n"]]
    second = max(remaining, key=lambda page: (
        content_rank(page), len(feature_types(page) - feature_types(first)),
        len(feature_types(page)), abs(page["n"] - first["n"]), -page["n"]))
    reasons = [
        "Prefer substantive content, then feature-type diversity and page spread; scores are not selection inputs."
    ]
    if len(visible) < count:
        reasons.append("Fewer than two visible reference pages; retain available hidden coverage.")
    if min(content_rank(first), content_rank(second)) < 2:
        reasons.append("Fewer than two richer content pages; retain available simple content.")
    return {"id": case["id"], "pages": sorted([first["n"], second["n"]]), "reasons": reasons}


def generate(scorecard):
    cases = validate(scorecard)
    return {
        "version": 1,
        "selection": "Frozen feature diversity and page spread, with reviewed formula and art text coverage; no fidelity-score ranking.",
        "cases": [select_case(case) for case in sorted(cases, key=lambda case: case["id"])],
    }


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--scorecard", type=Path, required=True)
    parser.add_argument("--output", type=Path, required=True)
    args = parser.parse_args()
    try:
        if args.scorecard.resolve() == args.output.resolve():
            raise ValueError("output must not overwrite the input scorecard")
        manifest = generate(json.loads(args.scorecard.read_text(encoding="utf-8")))
        args.output.parent.mkdir(parents=True, exist_ok=True)
        args.output.write_text(json.dumps(manifest, indent=2, ensure_ascii=True) + "\n", encoding="utf-8")
    except (OSError, ValueError) as error:
        print(f"Selection failed: {error}", file=sys.stderr)
        return 1
    print(f"Selected {sum(len(c['pages']) for c in manifest['cases'])} pages across {len(manifest['cases'])} decks.")
    return 0


if __name__ == "__main__":
    sys.exit(main())
