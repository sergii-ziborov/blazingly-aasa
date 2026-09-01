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

echo
echo "wasm payload sizes:"
for out in pkg pkg-node pkg-web; do
  [ -d "$out" ] || continue
  find "$out" -name '*.wasm' -exec ls -lh {} \; | awk -v o="$out" '{printf "  %-10s %s\n", o, $5}'
done
