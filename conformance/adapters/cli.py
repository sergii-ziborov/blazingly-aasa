#!/usr/bin/env python3
"""The corpus protocol, driven by a command-line matcher instead of a library binding.

Deliberately in a different language and through a different mechanism than `wasm.mjs`: the point
is that the contract needs neither JavaScript nor an in-process binding. Any program that can read
a line and print a line can be scored.

This one shells out per case, which is slow and perfectly correct. Usage:

    node conformance/run.mjs --exec "python3 conformance/adapters/cli.py ./blazingly-aasa"
"""
import json
import subprocess
import sys

binary = sys.argv[1] if len(sys.argv) > 1 else "blazingly-aasa"

for line in sys.stdin:
    line = line.strip()
    if not line:
        continue
    case = json.loads(line)
    result = subprocess.run(
        [binary, "explain", "-", case["domain"], case["url"], "--app", case["appId"], "--json"],
        input=json.dumps(case["aasa"]),
        capture_output=True,
        text=True,
    )
    try:
        decision = json.loads(result.stdout)["decision"]
    except (json.JSONDecodeError, KeyError):
        # A case this matcher cannot answer fails that case, not the run.
        decision = "error"
    print(json.dumps({"id": case["id"], "decision": decision}), flush=True)
