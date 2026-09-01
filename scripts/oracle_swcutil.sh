#!/usr/bin/env bash
# Differential harness: this crate against Apple's swcutil.
#
#   sudo ./scripts/oracle_swcutil.sh
#
# swcutil requires root and exists only on macOS, which is why no ordinary `cargo test` depends on
# it. Everything it settles should be promoted into tests/fixtures with a line in docs/parity.md,
# so Linux and Windows CI can replay the conclusion without the tool.
#
# Reference: Apple TN3155, "Debugging universal links" — `swcutil verify -d <domain> -j <file>`.
set -uo pipefail
cd "$(dirname "$0")/.."

if [ "$(uname -s)" != "Darwin" ]; then
  echo "swcutil exists only on macOS; skipping" >&2
  exit 0
fi
if [ "$(id -u)" -ne 0 ]; then
  echo "swcutil must run as root; re-run with sudo" >&2
  exit 1
fi

DOMAIN="${DOMAIN:-example.com}"
work="$(mktemp -d)"
trap 'rm -rf "$work"' EXIT

cargo build --release --quiet

echo "swcutil: $(swcutil --version 2>&1 | head -1)"
echo "domain:  $DOMAIN"
echo

for fixture in tests/fixtures/apple/*.json tests/fixtures/real-world/*.json; do
  [ -e "$fixture" ] || continue
  echo "=== $fixture"
  cp "$fixture" "$work/apple-app-site-association"
  swcutil verify -d "$DOMAIN" -j "$work/apple-app-site-association" 2>&1 | sed 's/^/  /'
  echo
done

cat <<'NOTE'
Compare the output above against `docs/parity.md`. When swcutil settles a question the
documentation leaves open — percent-encoding, duplicate query keys, a path pattern without a
leading slash — add a fixture and a test for it, and move the row in the parity table from
"documented only" to "oracle-checked".
NOTE
