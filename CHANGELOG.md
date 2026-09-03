# Changelog

All notable changes to this project are documented here. This project adheres to
[Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## npm 0.1.1 - 2026-09-03

Packaging only; the Rust crate is unchanged.

- The published npm package contained the **bundler** build alone, so `npm install` followed by
  `node` failed with `ERR_UNKNOWN_FILE_EXTENSION` on the `.wasm` import. Found by installing 0.1.0
  from the registry and running it, which is the only way this class of bug shows up.
- The package now carries all three builds behind conditional exports: Node and Bun resolve the
  `nodejs` build, bundlers the `bundler` build, and `blazingly-aasa/web` the browser build.
- The Node build ships as `.cjs`, because wasm-pack's `nodejs` target emits CommonJS and the
  package is `"type": "module"`.

## [Unreleased]

### Added

- **The conformance corpus is now usable by any implementation, in any language.**
  `conformance/PROTOCOL.md` defines a line-oriented contract — read one JSON case per line, write
  one decision per line — and `conformance/run.mjs --exec "<command>"` scores anything that speaks
  it. Two reference adapters ship with it, one binding a library in JavaScript and one shelling out
  to a command line from Python; both score 140/140, and CI runs the protocol so the contract
  cannot drift. The corpus was already implementation-neutral data; it was only reachable through a
  JavaScript module of one particular shape.
- `docs/findings.md`: what this project actually caught, in production files and in its own code,
  with every observation dated and reproducible.

### Changed

- `AASA180` now quotes the rule's own `comment` when it has one. A catch-all is frequently
  deliberate — GitHub's file ends with `{"/": "*", "comment": "Matches all remaining routes"}` —
  and quoting the author back turns a warning the reader has to investigate into one they can
  dismiss at a glance.

- `semantic_diff`, `semantic_equal`, and `AasaDiff::is_equivalent` documented what they actually
  compute. They compare **normalised effective policy**, which is sound — equivalent means
  identical decisions for every URL — but not complete: reordering two rules whose patterns can
  never both match is reported as a difference, and so is changing a substitution variable no
  pattern uses. The previous wording ("the same decisions for every app", "only differences that
  change behaviour") promised more than the algorithm proves. No behaviour changed; the contract is
  now honest, and `tests/diffing.rs` pins both directions.
- The README no longer claims to be "the only implementation" checked against Apple's tooling. It
  is an independent matcher differential-tested against `swcutil` — a claim that is both stronger
  and defensible, since `universal-links-test` also invokes the real `swcutil` on macOS for its
  non-simulated path.

## [0.1.0] - 2026-09-02

First release. A semantic engine for Apple Associated Domains, usable from Rust and WebAssembly,
with its matching behaviour verified against Apple's own `swcutil`.

### Verified against Apple's swcutil

`swcutil` requires root for every subcommand, so it had never been run. It has now been, and the
raw output is committed in `conformance/oracle` so the conclusions are auditable without a Mac.

**139 of 140 matching cases are oracle-verified.** The remaining one is this crate's own API
convention — an empty domain skips the host check — which `swcutil` cannot express.

The first run agreed on 68 of 73 cases. One disagreement was a harness artifact. **The other four
were this crate being wrong**, and 67 targeted probes against `swcutil match` pinned down exactly
what Apple does.

### Fixed as a result

- **A path pattern without a leading slash matches.** `abc` matches `/abc`; `buy/*` matches
  `/buy/42`. Apple's reference uses a bare `abc` in one example and this crate had read it as a
  documentation slip, going as far as shipping `AASA191` to warn about it. The lint was wrong.
  It is removed and its number retired — a code never changes meaning — and the Apple example it
  contradicted is now asserted in full, positive case included.
- **Trailing slashes are insignificant.** `/buy/*` matches `/buy`, `/buy` matches `/buy/`, and a
  leading run of slashes in a pattern collapses. The obvious implementation of this is wrong: also
  trying the path with a slash appended makes `/id/????` match `/id/481`, since `481/` is four
  characters. `swcutil` says no, and the conformance corpus caught it before it shipped.
- **A missing query item counts as present with an empty value.** `{"b": "*"}` matches a URL with
  no `b`; `{"b": "?*"}` does not.
- **Every occurrence of a repeated query name must match**, not any one of them. `{"id": "42"}`
  does not match `?id=7&id=42` in any position, while `{"id": "7"}` matches `?id=7&id=7`. The
  previous behaviour was the most permissive of three plausible readings and the wrong one.
- **A non-string query predicate discards the whole dictionary.** `{"a": "1", "flag": true}`
  matches `?a=2`. This crate previously made such a predicate never match, on the principle of
  refusing rather than guessing — the wrong direction, since Apple is more permissive here, so the
  cautious-looking choice produced false negatives. `AASA150` stays an error and now documents what
  it actually costs.

Every `percentEncoded` behaviour was confirmed unchanged, including the one case that distinguishes
the two possible readings of Apple's single sentence about it. That was the least certain area of
the crate; it is now the best evidenced.

### Added

Read the published source of every AASA tool with real usage (`chayev/yurl`,
`shortstuffsushi/Universal-Link-Validator`, `st-tech/universal-links-test`,
`@linkforty/aasa-core`) and closed the three gaps where a competitor did something this crate did
not. `docs/competitors.md` is the full matrix; `tests/coverage.rs` is its executable half.

- `apps_for_url` / `apps_for_url_parts`: which apps a URL reaches, in one pass over the rules
  rather than one scan per app. `universal-links-test` answers this and this crate did not. A
  property test asserts it never disagrees with `decide`.
- `services_for_bundle(team, bundle)`, `app_ids_for_bundle(bundle)`, and `split_app_id`, because
  `yurl` and `@linkforty/aasa-core` both take the two halves apart, as Xcode does.
  `app_ids_for_bundle` answers "which team prefix does this file still name for my app", the
  symptom when an app moves between teams.
- CMS-signed (iOS 9 era) association files are now recognised and read. `yurl` handles them; every
  JavaScript tool reports them as invalid JSON at byte 0. The DER is walked with no dependencies,
  the payload extracted, and `AASA200` reports in as many words that the signature was **not**
  verified — reading is not checking.
- `conformance/cases.json`: 140 matching and 13 validation cases, each tagged with its feature, a
  source link, and whether Apple documents the behaviour or this crate decided it. Run by the Rust
  suite and the WebAssembly suite, and published so other implementations can be held to it —
  `conformance/run-third-party.mjs` points it at anyone else's library.
- WebAssembly: `appsForUrl`, `servicesForBundle`, `appIdsForBundle`, `splitAppId`.
- `docs/roadmap.md` records why there is no hand-written JavaScript port and no MCP server.

### Fixed

- A JSON document beginning with the digit `0` was reported as a CMS signing problem. The DER
  SEQUENCE tag is `0x30`, which is also ASCII `0`, so sniffing the leading byte misread `0`, `0.5`,
  and any invalid JSON starting with a zero. JSON is now attempted first, and the signed-file path
  requires an actual `id-signedData` OID rather than a byte guess. A signed file whose payload is
  not JSON now blames the payload rather than the envelope.
- `caseSensitive: false` folded non-ASCII in pattern values but only ASCII in query item *names*.
  Both now use the same folding, so the setting means one thing across a rule.
- `tests/properties.rs` hard-coded `with_cases(400)`, which silently overrides `PROPTEST_CASES`.
  A deep run therefore proved nothing. The count now reads the environment, and the reason is
  written down next to it.

### Verified

- The bitset NFA — the engine used whenever a pattern carries a multi-character substitution set —
  had no cross-check against a reference. It is now compared against expanding the alternatives by
  hand and matching each expansion, which is exact because substitution values cannot nest.
- 50,000-case property run across all twelve properties: matcher against the naive reference, NFA
  against expansion, `decide` against `match_url`, `apps_for_url` against `decide`, and no panic on
  arbitrary bytes or arbitrary URLs.

### Measured

Scoring `universal-links-test` against the corpus: 52 of 70 applicable cases. It is solid on rule
ordering, `exclude`, wildcards, and the defaults hierarchy. It scores 10/20 on substitution
variables — and all ten passes are cases expecting `no_match`, which any implementation that
silently matches nothing passes for free. **No surveyed tool expands `$(...)` at all.**

### Parsing

- `AasaDocument::parse` for `apple-app-site-association` bytes, built on `blazingly-json` with no
  runtime `serde_json` dependency.
- Lenient by design: only invalid JSON, a non-object root, or an oversized payload fail. A bad
  field type or an unfamiliar key becomes a diagnostic, so one broken entry never hides the rest of
  the file.
- Configurable size limit via `ParseOptions`, defaulting to 128 KiB.

### Validation

- `ValidationReport` with stable `AASA###` diagnostic codes, severities, dotted document paths, and
  help text. Codes are a public contract from this release forward.
- 26 codes covering structure, app identifiers, substitution variables, rule reachability, and
  legacy formats.

### Matching

- `CompiledAasa::decide` for the answer, `match_url` for the answer plus a full trace naming the
  detail entry, rule index, effective settings, and the exact component that failed.
- Modern `components` with `/`, `?` (as pattern or dictionary), `#`, `exclude`, `caseSensitive`,
  and `percentEncoded`; ordered first-match-wins evaluation.
- The three-level defaults hierarchy: domain, app, and URL.
- Legacy `paths` with `NOT ` exclusions, and the oldest `details`-as-dictionary form.
- `webcredentials`, `appclips`, and `activitycontinuation` membership.
- Wildcards compiled rather than translated to a regular expression: literal, prefix, suffix, and
  contains patterns take allocation-free fast paths, general patterns run a greedy glob matcher,
  and patterns with multi-character substitution sets run a bitset NFA. None backtracks
  exponentially.
- `WildcardPattern` for checking one pattern against one string, outside any document.
- `$(region)` and `$(lang)` generated from Foundation's `isoRegionCodes` and `isoLanguageCodes`
  rather than hand-written; `ISO_TABLE_SOURCE` reports the snapshot.

### Comparison

- `semantic_diff` comparing behaviour rather than bytes: hoisting `caseSensitive` into `defaults`
  reports no change, reordering two rules reports a move.
- `structural_equal` and `semantic_equal` as separate questions.
- `to_normalized_json` for a canonical rendering with every default resolved.

### WebAssembly

- `blazingly-aasa` with a persistent handle, so the compiled document stays inside WebAssembly and
  only small values cross the boundary.
- Batch entry points, including `decideLines` for one string encode per batch.
- Bundler, Node, and browser packages from one Rust module; tested on Node and Bun.

### Known limits

- `AASA142`, `AASA144`, simple non-ASCII case folding, and reading a CMS signature without
  verifying it remain **decided** rather than **oracle**: `swcutil` has no way to answer them.
- `$(region)` does not match `UK`, which Apple's prose gives as an example. `Locale.isoRegionCodes`
  contains `GB` and not `UK`, and `swcutil` agrees. Pinned by a test.
- The `$(region)` and `$(lang)` tables are a snapshot of one Foundation release; `ISO_TABLE_SOURCE`
  reports which.

[0.1.0]: https://github.com/sergii-ziborov/blazingly-aasa/releases/tag/v0.1.0
