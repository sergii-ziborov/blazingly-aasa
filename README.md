# blazingly-aasa

**Apple Associated Domains semantics for Rust and WebAssembly.** Parse, validate, match, explain,
and diff `apple-app-site-association` policy.

[![CI](https://github.com/sergii-ziborov/blazingly-aasa/actions/workflows/ci.yml/badge.svg)](https://github.com/sergii-ziborov/blazingly-aasa/actions/workflows/ci.yml)
[![WebAssembly](https://github.com/sergii-ziborov/blazingly-aasa/actions/workflows/wasm.yml/badge.svg)](https://github.com/sergii-ziborov/blazingly-aasa/actions/workflows/wasm.yml)
![MSRV 1.78](https://img.shields.io/badge/MSRV-1.78-blue)
![license MIT](https://img.shields.io/badge/license-MIT-blue)

---

An `apple-app-site-association` file is the JSON document a website serves at
`/.well-known/apple-app-site-association` to say which apps may open which of its URLs — universal
links, App Clips, shared web credentials, Handoff. It is a small file with surprisingly sharp
semantics: rules are ordered and the first match wins, `exclude` stops the scan rather than falling
through, and three levels of defaults override each other.

Most tooling reduces all of that to a green checkmark. When a universal link silently stops
working, a checkmark tells you nothing.

This crate gives you the answer **and the reason**:

```
NO_MATCH

application: ABCDE12345.com.example.app
domain:      example.com
url:         https://example.com/help/1?articleNumber=481

reason:
  the entries that apply to ABCDE12345.com.example.app have no rule matching this URL

closest failure:
  detail #0, rule #3
  [ok  ] path
         url:     /help/1
         pattern: /help/*
         wildcard match
  [FAIL] query[articleNumber]
         url:     481
         pattern: ????
         pattern did not match
```

## What it does

- **Parses** every shape in the wild — modern `components`, legacy `paths` with `NOT ` exclusions,
  and the oldest `details`-as-a-dictionary form — leniently, so one broken entry never hides the
  rest of the file.
- **Validates** with stable, machine-readable `AASA###` codes: unreachable rules, catch-alls that
  open a whole domain by accident, path patterns that can never match, recursive substitution
  variables, mixed legacy and modern formats.
- **Matches** a URL for an app, with full trace: which detail entry, which rule index, what the
  effective `caseSensitive` and `percentEncoded` were, and exactly which component failed.
- **Compares** two files semantically — behaviour, not bytes. Hoisting `caseSensitive` into
  `defaults` reports no change; reordering two rules reports a move.
- **Answers in both directions**: does *this app* get *this URL*, and which apps does a URL reach.
- **Reads CMS-signed files** from the iOS 9 era, which every JavaScript tool rejects as invalid
  JSON — extracting the payload, and saying plainly that the signature was not verified.
- **Runs everywhere**: Rust, and a WebAssembly package for browsers, Node, and Bun.

## What it does not do

It never touches the network, never opens an `.ipa`, and never claims to know what a device will
do. A match means *this document considers this URL eligible for this app* — not that the link will
open the app, which also depends on install state, entitlements, and what Apple's CDN is currently
serving. Those belong to the tools built on this crate; see
[docs/aasadiff-integration.md](docs/aasadiff-integration.md).

## Rust

Not on crates.io yet — the parity table is honest about what is and is not verified, and that
should settle before a version number becomes permanent. Until then:

```toml
[dependencies]
blazingly-aasa = { git = "https://github.com/sergii-ziborov/blazingly-aasa" }
```

```rust
use blazingly_aasa::{CompiledAasa, MatchDecision};

let bytes = br#"{
  "applinks": {
    "details": [{
      "appIDs": ["ABCDE12345.com.example.app"],
      "components": [
        { "/": "/help/website/*", "exclude": true },
        { "/": "/help/*", "?": { "articleNumber": "????" } }
      ]
    }]
  }
}"#;

let aasa = CompiledAasa::parse(bytes)?;
let app = "ABCDE12345.com.example.app";

// The document blocks this one, and says so rather than just declining.
let blocked = aasa.match_url("example.com", app, "https://example.com/help/website/faq")?;
assert_eq!(blocked.decision, MatchDecision::Exclude);

// Four characters, as the pattern demands.
let hit = aasa.match_url("example.com", app, "https://example.com/help/1?articleNumber=4815")?;
assert_eq!(hit.decision, MatchDecision::Match);

// Three characters. Not an error — an answer.
let miss = aasa.match_url("example.com", app, "https://example.com/help/1?articleNumber=481")?;
assert_eq!(miss.decision, MatchDecision::NoMatch);
println!("{miss}"); // the trace above
```

The other direction — which apps does a URL reach?

```rust
for (app_id, decision) in aasa.apps_for_url("example.com", "https://example.com/help/1?articleNumber=4815")? {
    println!("{app_id}: {decision}");
}
// ABCDE12345.com.example.app: MATCH
```

Linting, with codes you can build CI on:

```rust
let report = blazingly_aasa::validate(bytes)?;
for diagnostic in report.errors() {
    eprintln!("{diagnostic}");
    // error [AASA110] applinks.details[1]: this entry names no application identifier
    //   help: add `appID` or `appIDs`
}
```

Comparing what you serve against what Apple's CDN serves:

```rust
let diff = origin.semantic_diff(&cdn);
if !diff.is_equivalent() {
    for change in diff.changes() {
        println!("{change}");
        // RULE_CHANGED    ABCDE12345.com.example.app #2
        //   before: / = /help/*, caseSensitive=false, percentEncoded=true
        //   after:  / = /help/*, caseSensitive=true, percentEncoded=true
    }
}
```

## JavaScript

Not on npm yet; build it from the repository with `./bindings/wasm/build.sh`.

```js
import { Aasa } from "@blazingly/aasa";

const response = await fetch("https://example.com/.well-known/apple-app-site-association");
const aasa = Aasa.compile(new Uint8Array(await response.arrayBuffer()), "example.com");

try {
  for (const d of aasa.validate()) {
    console.log(`${d.severity} ${d.code} ${d.path}: ${d.message}`);
  }

  console.log(aasa.decide(appId, url));   // "match" | "exclude" | "no_match"
  console.log(aasa.explain(appId, url));  // the same decision, in words

  // One boundary crossing for a whole batch: 0 no match, 1 match, 2 exclude, 3 bad URL.
  const codes = aasa.decideLines(appId, urls.join("\n"));
} finally {
  aasa.free();
}
```

Works in browsers, Node, and Bun. Details in [docs/wasm.md](docs/wasm.md).

## How this compares

There are four AASA tools with real usage. [docs/competitors.md](docs/competitors.md) reads each
one's source and maps what it covers. The short version: **they are validators, this is an engine.**

`yurl`, `Universal-Link-Validator`, and `@linkforty/aasa-core` fetch the file and check how it is
hosted — genuinely valuable, and deliberately not this crate's job. None of them evaluates a URL
against the rules at all.

`st-tech/universal-links-test` does, and it is well built: rule ordering, `exclude`, wildcards, and
the defaults hierarchy are all correct. So it can be scored against the same corpus this crate runs:

| Feature | universal-links-test | blazingly-aasa |
| --- | --- | --- |
| rule order, `exclude`, wildcards, defaults | 24/24 | 24/24 |
| query | 6/8 | 8/8 |
| percent encoding | 3/6 | 6/6 |
| **substitution variables** | **10/20** | 20/20 |
| legacy `paths`, legacy `details` | 1/4 | 4/4 |
| **total** | **52/70** | 70/70 |

That substitution row is the reason this crate exists, and it needs reading carefully. Exactly ten
of those twenty cases expect `no_match`; it passes all ten of those and none of the other ten —
because **no surveyed tool expands `$(...)` at all.** They declare `substitutionVariables` in their
types and ignore it when matching. Its score there is not "half right", it is zero right with half
the cases passing by accident.

That is the dangerous failure mode: a file using `$(lang)` does not error, it silently matches
nothing, and the check stays green.

## The conformance corpus

`conformance/cases.json` is 73 matching and 14 validation cases, each tagged with the feature it
covers, a link to the Apple page that documents it, and whether the behaviour is **documented** by
Apple or **decided** by this crate.

The Rust suite and the WebAssembly suite both run it, so a binding bug cannot hide behind passing
Rust tests. It is published rather than kept internal, because a shared corpus is how the whole
ecosystem gets more correct rather than just this crate:

```bash
node conformance/run-third-party.mjs ./path/to/some-other-implementation.js
```

The runner reports how many passes are *trivial* — an implementation that silently matches nothing
passes every `expect: no_match` case, and a comparison that hides this overstates the loser.

## Performance

Measured on an Apple M4, macOS 27.0, rustc 1.96.1, criterion with 3 s warm-up and 8 s measurement;
JavaScript on Node 22.13. Reproduce with `cargo bench` and `node bindings/wasm/bench/bench.mjs`.

There is no established Rust crate implementing AASA semantics, so the baseline is what a competent
engineer would actually build: **`serde_json` for parsing plus the `regex` crate for wildcards**
(`*` → `.*`, `?` → `.`, anchored) — which is how nearly every AASA checker in the wild works. Both
sides use the same URL splitter, so the numbers reflect AASA work rather than URL parsing. The
benchmark corpus deliberately contains no `$(...)` references, because the regex baseline does not
implement substitution variables and would otherwise be credited for skipping work.

**Where this crate wins, and where it doesn't:**

### Matching one pattern

| Pattern | blazingly-aasa | serde_json + regex | |
| --- | --- | --- | --- |
| `/help/website/faq` (literal) | **2.61 ns** | 15.3 ns | **5.9x** |
| `/buy/*` (prefix) | **3.55 ns** | 60.8 ns | **17x** |
| `*/checkout` (suffix) | **3.34 ns** | 69.1 ns | **21x** |
| `/id/????` | 10.5 ns | 10.2 ns | — |
| `/id/$(digit)$(digit)$(digit)$(digit)` | 15.1 ns | 9.88 ns | 0.65x |
| `/a/*/b/?*/c` | 20.7 ns | 13.0 ns | 0.63x |
| `*a*a…*b` on 512 `a`s (adversarial) | 942 ns | 605 ns | 0.64x |

The first three shapes cover almost every pattern in a real association file, and they take
allocation-free string tests. On genuinely general patterns a mature DFA beats a glob matcher by
about 1.6x — that is the honest cost of not shipping a regex engine, and the adversarial row shows
neither engine degrades catastrophically.

### Compiling patterns

| Pattern | blazingly-aasa | serde_json + regex | |
| --- | --- | --- | --- |
| `/help/website/faq` | **207 ns** | 5.57 µs | **27x** |
| `/buy/*` | **152 ns** | 17.2 µs | **113x** |
| `/id/????` | **128 ns** | 22.7 µs | **177x** |
| `*a*a…*b` | **1.35 µs** | 50.5 µs | **37x** |

An association file compiles every pattern it contains, once, before matching anything. This is
where the regex dependency actually costs.

### Parsing and compiling a document

| Document | blazingly-aasa | serde_json + regex | |
| --- | --- | --- | --- |
| 0.4 KiB, 8 rules | **7.52 µs** | 216 µs | **29x** |
| 5 KiB, 128 rules | **93.1 µs** | 3.49 ms | **37x** |
| 38 KiB, 1024 rules | **695 µs** | 28.7 ms | **41x** |

Read that with the next table, not on its own — most of the gap is the regex compiler, not the JSON
parser:

| JSON parse only | blazingly-json | serde_json | |
| --- | --- | --- | --- |
| 0.4 KiB | **1.75 µs** | 2.00 µs | 1.14x |
| 5 KiB | **19.5 µs** | 23.4 µs | 1.20x |
| 38 KiB | **143 µs** | 178 µs | 1.24x |

### Matching a real document

8 URLs against 8 apps x 16 rules:

| | |
| --- | --- |
| `decide`, pre-split URLs | 861 ns |
| same, `serde_json` + `regex` baseline | 759 ns |
| `decide`, splitting the URL too | 1.37 µs |
| `match_url` with a full trace | 11.8 µs |

**Matching a document is a wash** — this crate and the regex baseline land within ~13% of each
other, and on a scan-heavy miss they are identical to within noise (165 ns vs 166 ns across 32 app
entries). The advantage is in getting there, not in the loop.

The trace costs about 14x the bare decision, which is why `decide` and `match_url` are separate
calls rather than one function with a flag.

### Reusing the compiled handle

| | |
| --- | --- |
| `compiled.decide(...)` | **134 ns** |
| `blazingly_aasa::match_url(bytes, ...)`, reparsing each call | 94.6 µs |

**706x.** Parse once, match many — the one-shot helpers exist for convenience, not for loops.

### Validation

Full lint of the same documents: 517 ns (0.4 KiB), 6.68 µs (5 KiB), 48.6 µs (38 KiB). Cheap enough
to run on every request if you want to.

### WebAssembly against pure JavaScript

Against a `JSON.parse` + `RegExp` implementation — the JavaScript equivalent of the Rust baseline:

| | WebAssembly | pure JS | |
| --- | --- | --- | --- |
| compile 5 KiB | **76.3 µs** | 161 µs | **2.1x** |
| compile 38 KiB | **581 µs** | 1.26 ms | **2.2x** |
| match, 1 URL per call | 442 ns | **291 ns** | 0.79x |
| match, `decideLines` batch of 64+ | **349 ns** | 435 ns | **1.25x** |

This is the part most WebAssembly libraries do not tell you: **matching one URL at a time is
_slower_ than pure JavaScript**, because moving a string across the boundary costs more than
matching it, and that cost is per string — it does not amortise. `decideLines` takes the whole
batch as one string, which is as far as that goes: 1.25x, not 10x.

Compilation is genuinely ~2x faster. But the real reason to use the WebAssembly build is not speed —
it is that the semantics are the same ones the Rust crate implements, with the same diagnostics,
traces, and diff, rather than a second implementation that will drift.

Payload: 345 KB raw, 141 KB gzip, 113 KB brotli.

## Correctness

Apple's reference pages leave real questions open. Rather than guessing and presenting the guess as
fact, every behaviour is classified:

- **documented** — Apple states it, and a test asserts it.
- **decided** — Apple does not state it; this crate chose a reading and pinned it with a test.

[docs/parity.md](docs/parity.md) is that table, feature by feature. Nothing is yet marked
**oracle-checked** against Apple's `swcutil`, so this crate does not claim bit-exact parity with
iOS — it claims to implement what Apple documents and to be explicit about the rest.
`scripts/oracle_swcutil.sh` runs the differential check on macOS.

Two examples of what that discipline turns up:

**`$(region)` does not match `UK`.** Apple's prose gives "`CA`, `UK`, and `US`" as example regions,
but `UK` is not an ISO 3166-1 alpha-2 code and does not appear in `Locale.isoRegionCodes` — the
United Kingdom is `GB`. The `$(region)` and `$(lang)` tables are *generated* from Foundation by
`scripts/generate_iso_tables.swift` rather than transcribed, so the list Apple points at wins over
the prose. `ISO_TABLE_SOURCE` records which OS release the snapshot came from.

**Path patterns without a leading slash.** One Apple example writes `"/": "abc"` and says
`https://www.example.com/abc` matches, while every other example writes `/buy/*` — and a URL path
always starts with `/`. Instead of guessing, this crate matches the full path and reports the
suspicious pattern as `AASA191` with a suggested fix. An ambiguity became a useful lint.

The test suite is 90-odd tests across Apple's documented examples, parsing, validation, matching,
percent-encoding, and semantic diff — plus property tests that check the pattern matcher against a
deliberately naive exponential reference implementation, that parsing arbitrary bytes never panics,
and that the fast decision path never disagrees with the tracing one.

## How the pattern engine works

Apple's wildcard language is `*` (zero or more), `?` (exactly one), and therefore `?*` (one or
more), plus `$(name)` substitution references. The obvious implementation translates it to a
regular expression. This crate does not, for three reasons: you would have to prove the translation
equivalent, ship a regex engine to every WebAssembly consumer, and pay regex compilation for every
rule in the file.

Instead, patterns compile to one of three engines, chosen at compile time:

| Shape | Engine |
| --- | --- |
| `/help/website/faq`, `/buy/*`, `*/checkout`, `*sale*` | direct string test, no allocation |
| anything from literals, `?`, `*`, and single-character classes | greedy glob, no heap |
| contains `$(region)`, `$(lang)`, or a custom variable | bitset NFA over reachable positions |

None backtracks exponentially — the classic `*a*a*a…*b` blow-up is bounded by
`O(positions x tokens)`. Input that is entirely ASCII, which URL components almost always are, is
matched directly against the string's bytes.

You can use the matcher on its own:

```rust
use blazingly_aasa::WildcardPattern;

let pattern = WildcardPattern::compile("/id/$(digit)$(digit)", true)?;
assert!(pattern.matches("/id/42"));
assert!(!pattern.matches("/id/4x"));
```

## Dependencies

`blazingly-json` and `serde`. That is the whole runtime dependency list — no HTTP client, no regex
engine, no async runtime, no URL crate. URLs are split by a small RFC 3986 splitter that preserves
each component exactly as written, because matching compares against the URL *as the system saw
it*; normalising first would change what the patterns see.

`serde_json` and `regex` appear only as dev-dependencies, as benchmark baselines.

## Using it as a tool

This crate is an engine, not a program. If you want the program:

**[`blazingly-aasa-mcp`](https://github.com/sergii-ziborov/blazingly-aasa-mcp)** — an MCP server
and CLI built on it. It fetches a domain's file, matches a URL, explains the decision, and compares
what a site serves against what Apple's CDN is handing to devices:

```bash
cargo install --git https://github.com/sergii-ziborov/blazingly-aasa-mcp
blazingly-aasa check example.com "https://example.com/buy/42" --app ABCDE12345.com.example.app
```

The split is deliberate: everything network-shaped lives there, and this crate keeps two
dependencies and compiles to WebAssembly. See
[docs/aasadiff-integration.md](docs/aasadiff-integration.md) for where the line sits.

## Documentation

| | |
| --- | --- |
| [docs/competitors.md](docs/competitors.md) | what the existing tools cover, measured against the corpus |
| [docs/roadmap.md](docs/roadmap.md) | why there is no hand-written JS port, and no MCP server yet |
| [docs/semantics.md](docs/semantics.md) | what is implemented and where each rule comes from |
| [docs/parity.md](docs/parity.md) | feature-by-feature: documented by Apple, or decided here |
| [docs/diagnostics.md](docs/diagnostics.md) | every `AASA###` code and a suggested CI policy |
| [docs/wasm.md](docs/wasm.md) | the WebAssembly design, its limits, and the API |
| [docs/aasadiff-integration.md](docs/aasadiff-integration.md) | where this crate ends and your tool begins |
| [AGENTS.md](AGENTS.md) | guardrails for contributors |

## Development

```bash
cargo test --workspace
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo +1.78 check -p blazingly-aasa --lib

./bindings/wasm/build.sh
node bindings/wasm/tests/node.test.mjs
node bindings/wasm/tests/conformance.mjs
bun  bindings/wasm/tests/conformance.mjs
```

Benchmarks:

```bash
cargo bench --bench pattern_engine
cargo bench --bench compile
cargo bench --bench matching
node bindings/wasm/bench/bench.mjs
```

## License

MIT. See [LICENSE](LICENSE).
