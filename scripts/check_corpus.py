#!/usr/bin/env python3
"""Structural checks on the published conformance corpus.

The corpus is a cross-implementation contract, so its shape is worth enforcing outside the Rust
tests that consume it.
"""
import json
import sys

corpus = json.load(open("conformance/cases.json", encoding="utf-8"))

names = [c["name"] for c in corpus["matching"]] + [c["name"] for c in corpus["validation"]]
if len(names) != len(set(names)):
    sys.exit("conformance: duplicate case names")

for case in corpus["matching"]:
    if case["expect"] not in ("match", "exclude", "no_match"):
        sys.exit(f"conformance: {case['name']} has unknown expectation {case['expect']}")
    if case["status"] not in ("documented", "decided", "oracle"):
        sys.exit(f"conformance: {case['name']} has unknown status {case['status']}")
    if case["status"] == "oracle" and not case.get("oracle"):
        sys.exit(f"conformance: {case['name']} is 'oracle' but does not say which run confirmed it")
    if case["status"] == "decided" and "note" not in case:
        sys.exit(f"conformance: {case['name']} is 'decided' but has no note explaining the choice")
    if not case.get("source"):
        sys.exit(f"conformance: {case['name']} has no source link")

by_status = {}
for case in corpus["matching"]:
    by_status[case["status"]] = by_status.get(case["status"], 0) + 1
print(
    f"conformance corpus: {len(corpus['matching'])} matching "
    f"+ {len(corpus['validation'])} validation cases "
    f"({', '.join(f'{n} {s}' for s, n in sorted(by_status.items()))})"
)
