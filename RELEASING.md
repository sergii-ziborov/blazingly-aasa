# Releasing

Nothing here is automated to the point of running by itself. Publishing to crates.io is
**permanent** — a version can be yanked but never replaced — so every step is deliberate.

## Order matters

`blazingly-aasa-mcp` depends on this crate. Its manifest carries both a `version` and a pinned
`git` revision: local builds use the revision, and `cargo publish` drops the git source and depends
on the crates.io release. That means **the library must be on crates.io before the MCP crate can be
published at all** — `cargo package` refuses otherwise, which is the intended guard rather than an
obstacle.

```
blazingly-aasa (crates.io)  ->  blazingly-aasa-mcp (crates.io)
       |
       +-> @blazingly/aasa (npm)
```

## Before releasing anything

```bash
cargo fmt --all --check
cargo test --workspace
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo +1.78 check -p blazingly-aasa --lib
./scripts/verify_fixtures.sh
./bindings/wasm/build.sh && node bindings/wasm/tests/conformance.mjs
```

And, on a Mac, with the run committed to `conformance/oracle`:

```bash
sudo ./scripts/oracle_swcutil.sh
```

Check that `docs/parity.md` still describes reality. A row that says **oracle** must have a run
behind it in `conformance/oracle`.

## 1. The library, to crates.io

One-time: add a crates.io API token as the repository secret `CARGO_REGISTRY_TOKEN`.

```bash
cargo publish -p blazingly-aasa --dry-run
git tag v0.1.0 && git push origin v0.1.0
```

The tag fires `.github/workflows/publish.yml`, which re-runs the quality gate, checks that the tag
matches the crate version, and publishes. To publish by hand instead:

```bash
cargo publish -p blazingly-aasa
```

## 2. The npm package

One-time: the `@blazingly` scope must exist on npm and the account must own it. Add an npm
automation token as the repository secret `NPM_TOKEN`.

```bash
./bindings/wasm/build.sh
node bindings/wasm/tests/conformance.mjs   # the same corpus the Rust suite runs
cd bindings/wasm/pkg && npm publish --access public
```

`wasm-pack` writes `pkg/package.json` from `bindings/wasm/Cargo.toml`, so the name and version come
from there. Keep the npm version in step with the crate version.

## 3. The MCP server, to crates.io

Only after step 1 has landed and the index has caught up.

In `blazingly-aasa-mcp`:

```bash
cargo update -p blazingly-aasa
cargo package            # now resolves the published version
cargo publish
git tag v0.1.0 && git push origin v0.1.0
```

Once the library is on crates.io the `git` and `rev` keys can be dropped from that manifest
entirely, leaving a plain version dependency. Keep them only while tracking an unreleased change.

## Versioning

Semantic versioning, with one addition: **a diagnostic code is a public contract.** Codes may be
added in a minor release. A code is never renumbered and never changes meaning — `AASA191` was
removed before the first release when `swcutil` disproved it, and its number stays retired rather
than being reused.

A change to matching behaviour is a breaking change even when the API is untouched, because a CI
check built on this crate will start reporting something different. Say so in the changelog and in
`docs/parity.md`.
