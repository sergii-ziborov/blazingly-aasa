# Working on this repository

Guardrails that exist because the failure modes they prevent are easy to walk into.

## Scope

This crate is a semantic engine for `apple-app-site-association` documents. Bytes and explicit
context in; parse, validation, matching, explanation, and comparison out.

**Do not add** to the core crate: HTTP or DNS, `.well-known` lookup, Apple CDN behaviour, `.ipa`
or `.app` inspection, Mach-O parsing, code signature or entitlement extraction, device-state
modelling, or an MCP server. Those belong to tools built on top — see
`docs/aasadiff-integration.md`.

**Do not add** Apple-specific knowledge to `blazingly-json`. The dependency points one way:
`blazingly-aasa -> blazingly-json`, never back.

## Correctness

**Do not implement Apple semantics from memory.** If the reference pages do not state it, either
derive it from something Apple does state, or make a decision, pin it with a test, and add a row to
`docs/parity.md` marked **decided**. A guess that looks like documented behaviour is worse than an
explicit choice.

**Under-claim rather than over-claim.** `semantic_diff` must never report equivalence it cannot
prove. A broken pattern must never match. When a reading is ambiguous, prefer the one that refuses.

**Rule order is semantics.** Never reorder `components` or `details` while normalising. The first
matching rule decides, and `exclude` stops the scan rather than falling through.

**`exclude` and `NoMatch` are decisions, not errors.** `Result::Err` is only for input that cannot
be interpreted.

**Never claim device behaviour.** The crate knows what a document says. It does not know what an
iPhone will do.

## Diagnostics

Codes are a public contract. Add new ones; never repurpose an existing one, never renumber, never
change a code's meaning. Messages may be reworded freely — that is why consumers are told to match
on `DiagnosticCode`.

Every code needs a document that triggers it in `tests/validation.rs` and a row in
`docs/diagnostics.md`.

## Patterns

**No regex.** Apple's wildcard language is small enough to compile directly, and translating it to
a regular expression means proving the translation equivalent, shipping a regex engine to every
WebAssembly consumer, and paying regex compilation on every rule. `src/pattern.rs` has three
engines — literal fast paths, a greedy glob matcher, and a bitset NFA — chosen at compile time.

Any change to the matcher must keep `matcher_agrees_with_the_naive_reference` passing. That
property test compares against an obviously-correct exponential implementation, and it is the main
thing standing between a clever optimisation and a silent behaviour change.

Matching must stay bounded. No unbounded recursion, no exponential backtracking.

## Performance

**Measure before optimising, and measure honestly.** `benches/` compares against real baselines:
`serde_json` + `regex` for Rust, a `JSON.parse` + `RegExp` implementation for JavaScript.

Do not publish a comparison that measures different amounts of work on each side. An earlier
revision of `benches/pattern_engine.rs` compared a full `decide()` call against a bare
`Regex::is_match` and made this crate look 8x slower than it is; the fix was to expose
`WildcardPattern` so both sides do the same work.

Report the places this crate loses too. `docs/wasm.md` says outright that WebAssembly matching is
not faster than pure JavaScript, because it is not.

## Compatibility

MSRV is 1.78, checked in CI. `Option::is_none_or` (1.82), `let`-chains, and similar newer APIs are
not available — CI will catch it, but `cargo +1.78 check --lib` catches it sooner.

The core crate must keep building for `wasm32-unknown-unknown` on its own. The WebAssembly binding
stays a separate crate under `bindings/wasm`.

Runtime dependencies are `blazingly-json` and `serde`, and adding a third needs a capability
argument, not a convenience one. `serde_json` and `regex` are dev-dependencies for benchmarking
baselines only — never runtime dependencies.

## Before opening a pull request

```bash
cargo fmt --all --check
cargo test --workspace
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo +1.78 check -p blazingly-aasa --lib
./scripts/verify_fixtures.sh
```

If you touched matching semantics, also:

```bash
./bindings/wasm/build.sh && node bindings/wasm/tests/node.test.mjs
sudo ./scripts/oracle_swcutil.sh   # macOS only; settles what the docs leave open
```

Add a fixture for every semantic bug you fix. That is how `tests/fixtures/apple` grew, and it is
the only thing that keeps a subtle regression from coming back.
