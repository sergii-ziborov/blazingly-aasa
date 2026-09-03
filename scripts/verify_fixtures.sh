#!/usr/bin/env bash
# Every fixture must be valid JSON and must be referenced by at least one test, so a fixture
# cannot rot unnoticed.
set -euo pipefail
cd "$(dirname "$0")/.."

status=0
while IFS= read -r fixture; do
  relative="${fixture#tests/fixtures/}"

  if command -v python3 >/dev/null; then
    if ! python3 -c "import json,sys; json.load(open(sys.argv[1]))" "$fixture" 2>/dev/null; then
      echo "invalid JSON: $fixture" >&2
      status=1
      continue
    fi
  fi

  if ! grep -rq -- "$relative" tests/*.rs; then
    echo "unreferenced fixture: $fixture" >&2
    status=1
  fi
done < <(find tests/fixtures -name '*.json' | sort)

if command -v python3 >/dev/null; then
  python3 scripts/check_corpus.py || status=1
  python3 scripts/check_workflows.py >/dev/null || status=1
fi

if [ "$status" -eq 0 ]; then
  echo "all fixtures are valid JSON and referenced by tests"
fi
exit "$status"
