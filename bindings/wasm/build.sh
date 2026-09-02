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
# `blazingly-aasa-wasm`. The intended name and the rest of the npm metadata live in package.json.
#
# The three builds are also assembled into one publishable package rather than shipping only one of
# them. Publishing the bundler build alone looks fine until someone runs it under Node, where
# importing a .wasm file is not something the runtime can do -- which is exactly what happened to
# 0.1.0.
echo
echo "==> assembling npm/ from all three targets"
rm -rf npm && mkdir -p npm/dist
cp -R pkg      npm/dist/bundler
cp -R pkg-node npm/dist/node
cp -R pkg-web  npm/dist/web
rm -f npm/dist/*/package.json npm/dist/*/.gitignore npm/dist/*/README.md

# wasm-pack's nodejs target emits CommonJS. The package is "type": "module", so a .js file there
# would be read as ESM and fail on `module.exports`. Renaming to .cjs states the format instead of
# fighting it; the file loads its wasm with fs.readFileSync and requires nothing else, so nothing
# internal has to change.
mv npm/dist/node/blazingly_aasa.js npm/dist/node/blazingly_aasa.cjs
mv npm/dist/node/blazingly_aasa.d.ts npm/dist/node/blazingly_aasa.d.cts
cp ../../LICENSE npm/LICENSE 2>/dev/null || true
cp ../../README.md npm/README.md 2>/dev/null || true

python3 - <<'PY'
import json, pathlib

intended = json.loads(pathlib.Path("package.json").read_text())
generated = json.loads(pathlib.Path("pkg/package.json").read_text())

package = {
    "name": intended["name"],
    "version": generated["version"],
    "description": intended["description"],
    "license": intended["license"],
    "keywords": intended.get("keywords", []),
    "repository": intended.get("repository"),
    "homepage": intended.get("homepage"),
    "bugs": intended.get("bugs"),
    "type": "module",
    "types": "./dist/bundler/blazingly_aasa.d.ts",
    # Node gets the build that loads the module itself; bundlers get the one they know how to
    # process; `./web` is there for a browser with no bundler, which must await the default export.
    "exports": {
        ".": {
            "types": "./dist/bundler/blazingly_aasa.d.ts",
            "node": {
                "types": "./dist/node/blazingly_aasa.d.cts",
                "default": "./dist/node/blazingly_aasa.cjs",
            },
            "default": "./dist/bundler/blazingly_aasa.js",
        },
        "./web": {
            "types": "./dist/web/blazingly_aasa.d.ts",
            "default": "./dist/web/blazingly_aasa.js",
        },
        "./package.json": "./package.json",
    },
    "files": ["dist", "README.md", "LICENSE"],
    "sideEffects": ["./dist/bundler/blazingly_aasa.js", "./dist/web/blazingly_aasa.js"],
}
package = {k: v for k, v in package.items() if v is not None}
pathlib.Path("npm/package.json").write_text(json.dumps(package, indent=2) + "\n")
print(f"  npm/: {package['name']}@{package['version']} (bundler + node + web)")
PY

echo
echo "wasm payload sizes:"
for out in pkg pkg-node pkg-web; do
  [ -d "$out" ] || continue
  find "$out" -name '*.wasm' -exec ls -lh {} \; | awk -v o="$out" '{printf "  %-10s %s\n", o, $5}'
done
