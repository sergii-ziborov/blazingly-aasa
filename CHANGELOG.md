# Changelog

All notable changes to this project are documented here. This project adheres to
[Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [0.1.0] - 2026-09-01

First release. A semantic engine for Apple Associated Domains, usable from Rust and WebAssembly.

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

- `@blazingly/aasa` with a persistent handle, so the compiled document stays inside WebAssembly and
  only small values cross the boundary.
- Batch entry points, including `decideLines` for one string encode per batch.
- Bundler, Node, and browser packages from one Rust module; tested on Node and Bun.

### Known limits

- No behaviour has been verified against Apple's `swcutil` yet. `docs/parity.md` marks every
  behaviour as either documented by Apple or decided by this crate, and
  `scripts/oracle_swcutil.sh` runs the differential check on macOS.
- `percentEncoded` is the least settled area; the chosen reading is documented and tested but not
  oracle-checked.
- `$(region)` does not match `UK`, which Apple's prose gives as an example — `Locale.isoRegionCodes`
  contains `GB` and not `UK`. Pinned by a test.

[0.1.0]: https://github.com/sergii-ziborov/blazingly-aasa/releases/tag/v0.1.0
