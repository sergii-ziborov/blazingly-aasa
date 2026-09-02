#!/usr/bin/env bash
# Builds the npm package for every target wasm-pack supports.
#
#   ./build.sh          -> pkg/ (bundler), pkg-node/ (Node ESM), pkg-web/ (<script type=module>)
#
# Requires wasm-pack: https://rustwasm.github.io/wasm-pack/installer/
set -euo pipefail
cd "$(dirname "$0")"

for target in bundler nodejs web; do
  case "$target" in
    bundler) out=pkg ;;
    nodejs)  out=pkg-node ;;
    web)     out=pkg-web ;;
  esac
  echo "==> wasm-pack build --target $target --out-dir $out"
  wasm-pack build --release --target "$target" --out-dir "$out" --out-name blazingly_aasa
done

# wasm-pack writes pkg/package.json from the Rust crate name, which would publish this as
# `blazingly-aasa-wasm`. The intended name is the scoped one, and the rest of the npm metadata --
# keywords, homepage, bugs -- has nowhere to live in Cargo.toml either. Merge it in from
# package.json, which is the source of truth for everything npm-facing.
for out in pkg pkg-node pkg-web; do
  [ -d "$out" ] || continue
  python3 - "$out" <<'PY'
import json, pathlib, sys

out = pathlib.Path(sys.argv[1])
intended = json.loads(pathlib.Path("package.json").read_text())
generated = json.loads((out / "package.json").read_text())

for key in ("name", "description", "keywords", "homepage", "bugs", "license", "repository"):
    if key in intended:
        generated[key] = intended[key]
(out / "package.json").write_text(json.dumps(generated, indent=2) + "\n")
print(f"  {out}: {generated['name']}@{generated['version']}")
PY
done

echo
echo "wasm payload sizes:"
for out in pkg pkg-node pkg-web; do
  [ -d "$out" ] || continue
  find "$out" -name '*.wasm' -exec ls -lh {} \; | awk -v o="$out" '{printf "  %-10s %s\n", o, $5}'
done
