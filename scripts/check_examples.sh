#!/usr/bin/env bash
# Every example prints what the README says it prints.
#
#   ./scripts/check_examples.sh          verify
#   BLESS=1 ./scripts/check_examples.sh  re-record after an intentional change
#
# The README quotes these files. Without this check, an output change is invisible until a reader
# runs the code and finds the documentation lying to them.
set -euo pipefail
cd "$(dirname "$0")/.."

status=0
for example in examples/*.rs; do
  name="$(basename "$example" .rs)"
  expected="examples/expected/$name.txt"
  actual="$(cargo run --quiet --example "$name")"
  if [ "${BLESS:-}" = "1" ]; then
    printf '%s\n' "$actual" > "$expected"
    echo "recorded $expected"
    continue
  fi
  if [ ! -f "$expected" ]; then
    echo "MISSING  $expected (run with BLESS=1 to record)" >&2
    status=1
  elif ! diff -u "$expected" <(printf '%s\n' "$actual") > /dev/null; then
    echo "CHANGED  $name" >&2
    diff -u "$expected" <(printf '%s\n' "$actual") | sed 's/^/  /' >&2
    status=1
  else
    echo "ok       $name"
  fi
done
exit $status
