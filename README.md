# blazingly-aasa

**Apple Associated Domains semantics for Rust and WebAssembly.** Parse, validate, match, explain,
and diff `apple-app-site-association` policy.

[![CI](https://github.com/sergii-ziborov/blazingly-aasa/actions/workflows/ci.yml/badge.svg)](https://github.com/sergii-ziborov/blazingly-aasa/actions/workflows/ci.yml)
[![WebAssembly](https://github.com/sergii-ziborov/blazingly-aasa/actions/workflows/wasm.yml/badge.svg)](https://github.com/sergii-ziborov/blazingly-aasa/actions/workflows/wasm.yml)
[![crates.io](https://img.shields.io/crates/v/blazingly-aasa.svg)](https://crates.io/crates/blazingly-aasa)
[![docs.rs](https://img.shields.io/docsrs/blazingly-aasa)](https://docs.rs/blazingly-aasa)
[![npm](https://img.shields.io/npm/v/blazingly-aasa)](https://www.npmjs.com/package/blazingly-aasa)
[![AASA conformance 139/140 oracle](https://img.shields.io/badge/AASA%20conformance-139%2F140%20oracle-brightgreen)](conformance/)
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

An independent, cross-platform AASA matcher **differential-tested against Apple's own `swcutil`:
139 of 140 conformance cases oracle-verified**, with the raw runs committed in
[`conformance/oracle`](conformance/oracle) so the conclusions can be audited without a Mac. That
check found four places where this crate was wrong, including one it had been confident enough
about to ship as a lint — [docs/parity.md](docs/parity.md) has each of them.

## What it does

- **Parses** every shape in the wild — modern `components`, legacy `paths` with `NOT ` exclusions,
  and the oldest `details`-as-a-dictionary form — leniently, so one broken entry never hides the
  rest of the file.
- **Validates** with 27 stable, machine-readable `AASA###` codes: unreachable rules, catch-alls
  that open a whole domain by accident, recursive substitution variables, a single non-string
  query predicate that silently voids every constraint beside it, mixed legacy and modern formats.
- **Matches** a URL for an app, with full trace: which detail entry, which rule index, what the
  effective `caseSensitive` and `percentEncoded` were, and exactly which component failed.
- **Compares** two files by effective policy, not bytes. Hoisting `caseSensitive` into `defaults`
  reports no change. The comparison is conservative: equivalent means identical decisions for
  every URL, while a reported difference means *potentially* different.
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

```toml
[dependencies]
blazingly-aasa = "0.1"
```

```rust
use blazingly_aasa::{CompiledAasa, MatchDecision};

let aasa = CompiledAasa::parse(document)?;
let app = "ABCDE12345.com.example.app";

// The fast path: a decision, nothing else.
let decision = aasa.decide("example.com", app, "https://example.com/help/1?articleNumber=4815")?;
assert_eq!(decision, MatchDecision::Match);

// Costs more, answers "why".
let miss = aasa.match_url("example.com", app, "https://example.com/help/1?articleNumber=481")?;
println!("{miss}");
```

Against a document whose first rule excludes `/help/website/*` and whose second requires a
four-character `articleNumber`, that prints — this is the real output of
[`examples/matching.rs`](examples/matching.rs), not a paraphrase:

```text
MATCH    https://example.com/help/1?articleNumber=4815
NO_MATCH https://example.com/help/1?articleNumber=481
BLOCK    https://example.com/help/website/faq
NO_MATCH https://example.com/store

NO_MATCH

application: ABCDE12345.com.example.app
domain:      example.com
url:         https://example.com/help/1?articleNumber=481

reason:
  the entries that apply to ABCDE12345.com.example.app have no rule matching this URL

closest failure:
  detail #0, rule #1
  [ok  ] path
         url:     /help/1
         pattern: /help/*
         wildcard match
  [FAIL] query[articleNumber]
         url:     481
         pattern: ????
         pattern did not match


ABCDE12345.com.example.app: MATCH
```

A near miss is an answer, not an error: the trace names the rule that came closest and the one
component that failed, with the pattern and the input side by side.

Linting, with stable codes you can gate CI on ([`examples/validate.rs`](examples/validate.rs)):

```text
error [AASA150] applinks.details[0].components[0].?.flag: query predicate is a boolean, but Apple documents only string patterns here
  help: Apple ignores the entire query dictionary when any predicate is not a string, so every query constraint in this rule stops applying and the rule matches more URLs, not fewer. Replace every predicate with a string pattern.
error [AASA110] applinks.details[1]: this entry names no application identifier
  help: add `appID` or `appIDs`
warning [AASA180] applinks.details[0].components[0]: this rule constrains no URL component, so it matches every URL
  help: it opens the whole domain for this app; add `/`, `?`, or `#` if that was not intended
warning [AASA190] applinks.details[0].components[1]: rule #0 already matches every URL, so this rule never runs
  help: the first matching rule wins; move this rule above the catch-all
warning [AASA180] applinks.details[0].components[2]: this rule constrains no URL component, so it matches every URL (comment: everything else)
  help: it opens the whole domain for this app; the comment suggests that is intended

errors: 2  warnings: 3
has_errors: true
AASA150 present: a query dictionary in this file is inert
```

Note the chain: the non-string `flag` predicate makes Apple discard the whole `?` dictionary, which
leaves rule #0 constraining nothing — so the catch-all warning fires on a rule the author thought
was narrow, and the rule after it becomes unreachable. One mistake, three diagnostics.

Comparing what you serve against what Apple's CDN serves, by effective policy rather than bytes
([`examples/diff.rs`](examples/diff.rs)):

```text
origin vs reformatted
  equivalent:          true
  structurally_equal:  false

origin vs stale CDN copy
  equivalent: false
  RULE_CHANGED    ABCDE12345.com.example.app #0
  before: / = /help/*, caseSensitive=false, percentEncoded=true
  after:  / = /help/*, caseSensitive=true, percentEncoded=true

https://example.com/HELP/1
  origin: MATCH
  stale:  NO_MATCH
```

### Every method

**Parsing.** `CompiledAasa::parse(bytes)` and `parse_with(bytes, &ParseOptions)`, which sets the
size limit and whether unknown keys are reported. Both accept a signed (CMS/PKCS#7) file and
extract the payload. `document()` returns the parsed `AasaDocument` behind it.

**Deciding.**

| Method | Answers |
| --- | --- |
| `decide(domain, app_id, url)` | `Match` / `Exclude` / `NoMatch`, allocation-free |
| `decide_parts(domain, app_id, &UrlParts)` | the same, when the URL is already split |
| `match_url(domain, app_id, url)` | the decision plus the full trace |
| `match_parts(domain, app_id, &UrlParts)` | the same, from split parts |
| `apps_for_url(domain, url)` | every app the URL reaches, with each decision |
| `apps_for_url_parts(domain, &UrlParts)` | the same, from split parts |

**Asking about the document.**

| Method | Answers |
| --- | --- |
| `has_applinks()` | whether an `applinks` section exists at all |
| `applink_apps()` | every app identifier under `applinks` |
| `has_applink_app(app_id)` | whether one app is listed |
| `has_webcredential_app(app_id)` | shared web credentials |
| `has_appclip(app_id)` | App Clips |
| `has_activitycontinuation_app(app_id)` | Handoff |
| `services_for_app(app_id)` | every service one app is enrolled in |
| `services_for_bundle(team_id, bundle_id)` | the same, addressed by team and bundle |
| `app_ids_for_bundle(bundle_id)` | every team that ships this bundle id |
| `apps_for_service(Service)` | the inverse: who is enrolled in one service |
| `effective_rules_for(app_id)` | the rules after the defaults hierarchy is applied |
| `substitution_variables()` | the `$(...)` tables the document defines |

**Comparing.** `semantic_diff(&other)` returns an `AasaDiff` with `is_equivalent()` and
`changes()`; `semantic_equal(&other)` is the boolean; `structural_equal(&other)` compares the
normalised documents instead; `to_normalized_json()` is that normal form.

`equivalent == true` guarantees the same decision for every URL. `false` means they *may* differ —
it does not prove they do, and no witness URL is produced. The `docs/roadmap.md` entry on
behavioural diffing is about closing that gap.

**Validating.** `validate()` returns a `ValidationReport`: `diagnostics()`, `errors()`,
`warnings()`, `infos()`, `has_errors()`, `is_empty()`, and `contains(DiagnosticCode)` for gating
CI on one specific finding. Every code is listed in [docs/diagnostics.md](docs/diagnostics.md);
`DiagnosticCode::all()` enumerates them at runtime.

**Free functions.** `validate(bytes)`, `match_url(bytes, domain, app_id, url)`, and
`diff(left, right)` do the whole job in one call when you will not reuse the document.
`split_app_id`, `trim_path`, `strip_leading_slash`, and `percent_decode` are the URL helpers the
matcher itself uses. `WildcardPattern` exposes the glob engine on its own — note that it is *only*
the glob engine: `WildcardPattern::compile("/buy/*", true)?.matches("/buy")` is `false`, while
`decide` on the same pattern answers `Match`, because a rule's `/` component also matches the
parent path. Reach for it to test a pattern, not to answer a question about a URL.

Three runnable examples live in [`examples/`](examples/). CI diffs their output against
[`examples/expected/`](examples/expected/), so the blocks above cannot drift from what the code
actually prints.

## JavaScript

```bash
npm install blazingly-aasa
```

```js
import { Aasa } from "blazingly-aasa";

const response = await fetch("https://example.com/.well-known/apple-app-site-association");
const aasa = Aasa.compile(new Uint8Array(await response.arrayBuffer()), "example.com");

try {
  for (const d of aasa.validate()) {
    console.log(`${d.severity} ${d.code} ${d.path}: ${d.message}`);
  }
  console.log(aasa.decide(appId, url));   // "match" | "exclude" | "no_match"
  console.log(aasa.explain(appId, url));  // the same decision, in words
} finally {
  aasa.free();
}
```

`Aasa` holds WebAssembly memory. Call `free()` when you are done with it — a `try`/`finally` is the
honest way to do that.

### Every method

| Method | Answers |
| --- | --- |
| `Aasa.compile(bytes, domain, maxBytes?)` | a handle to reuse; `maxBytes` caps the payload |
| `.domain` | the domain it was compiled for |
| `validate()` | the diagnostics, as objects |
| `hasErrors()` | whether any diagnostic is an error |
| `decide(appId, url)` | `"match"` / `"exclude"` / `"no_match"` |
| `decideMany(appId, urls[])` | one crossing for an array of URLs |
| `decideManyCodes(appId, urls[])` | the same as bytes: 0 no match, 1 match, 2 exclude, 3 bad URL |
| `decideLines(appId, newlineSeparated)` | the same again, without building a JS array |
| `match(appId, url)` | decision plus trace, as an object |
| `matchJson(appId, url)` | the same, as a JSON string |
| `explain(appId, url)` | the trace as human-readable text |
| `appsForUrl(url)` | every app the URL reaches |
| `applinkApps()` | every app under `applinks` |
| `servicesForApp(appId)` | which services one app is enrolled in |
| `servicesForBundle(teamId, bundleId)` | the same, by team and bundle |
| `appIdsForBundle(bundleId)` | every team shipping this bundle id |
| `normalizedJson()` | the normal form used for comparison |
| `semanticDiff(other)` | the changes between two documents |
| `semanticEqual(other)` | whether they decide every URL alike |

One-shot functions, when there is nothing to reuse: `validateAasa(bytes)`,
`matchAasa(bytes, domain, appId, url)`, `diffAasa(left, right)`, `matchPattern(pattern, input,
caseSensitive)`, `splitAppId(appId)`, `isoTableSource()`. `matchPattern` is the glob engine alone —
it answers `false` for `("/buy/*", "/buy")`, where `decide` answers `"match"` — so use it to test a
pattern, not to decide a URL. `setPanicHook()` routes Rust panics to
`console.error` while debugging.

Works in browsers, Node, and Bun. Packaging details in [docs/wasm.md](docs/wasm.md).

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
| rule order, `exclude`, defaults, `appIDs` | 22/22 | 22/22 |
| wildcards | 7/8 | 8/8 |
| query | 22/38 | 38/38 |
| percent encoding | 8/15 | 15/15 |
| path slashes | 13/25 | 25/25 |
| legacy `paths`, legacy `details` | 1/4 | 4/4 |
| **substitution variables** | **10/20** | 20/20 |
| **total** | **88/137** | 137/137 |

That substitution row is the reason this crate exists, and it needs reading carefully. Exactly ten
of those twenty cases expect `no_match`; it passes all ten of those and none of the other ten —
because **no surveyed tool expands `$(...)` at all.** They declare `substitutionVariables` in their
types and ignore it when matching. Its score there is not "half right", it is zero right with half
the cases passing by accident.

That is the dangerous failure mode: a file using `$(lang)` does not error, it silently matches
nothing, and the check stays green.

## The AASA conformance corpus

[`conformance/`](conformance/) is a test suite for the *format*, not for this crate.

140 matching cases and 13 validation cases, each tagged with the feature it covers, a link to the
Apple page that documents it, and how the expectation was established:

| | |
| --- | --- |
| **oracle** | checked against Apple's `swcutil`, with the raw run committed — **139 of 140** |
| **documented** | Apple states it and a test asserts it |
| **decided** | Apple does not state it and the oracle cannot answer it — **1 case**, this crate's own convention that an empty domain skips the host check |

It is deliberately implementation-neutral, and scoring your own matcher takes one command:

```bash
node conformance/run.mjs --exec "./your-matcher"
```

Your program reads one JSON case per line and writes one decision per line — nine lines of work in
any language. [`conformance/PROTOCOL.md`](conformance/PROTOCOL.md) is the contract;
[`conformance/adapters/`](conformance/adapters/) holds two reference implementations, one binding a
library in JavaScript and one shelling out to a command line from Python, both scoring 140/140 and
both run in CI so the contract cannot rot.

The report separates real passes from accidental ones:

```
feature              score   of which trivial
ok   rule-order      11/11   4 expect no_match
FAIL substitutions   10/20   10 expect no_match
```

Ten of the twenty substitution cases expect `no_match`, so an implementation that silently matches
nothing passes all ten by accident. `10/20` there is **zero right**, not half — and a comparison
that hides this flatters the loser. That column is why the tables in
[docs/competitors.md](docs/competitors.md) can be read at face value.

If a case is wrong, that is a bug worth an issue. It is checked against Apple's tool, not against
this crate's opinion, and [`conformance/oracle/`](conformance/oracle/) has the raw runs so the
conclusions can be audited without a Mac.

## Performance

Apple M4, macOS 27.0, rustc 1.96.1, criterion. **Every figure is a ratio against a baseline
measured in the same run**, because absolute numbers on a shared machine are not comparable across
runs — in one pair of runs here the untouched `regex` baseline itself moved by 2.8x. Reproduce with
`cargo bench` and `node bindings/wasm/bench/bench.mjs`.

The baseline is what a competent engineer would actually build: **`serde_json` for parsing plus the
`regex` crate for wildcards**, which is how nearly every AASA checker in the wild works. Both sides
use the same URL splitter. The corpus contains no `$(...)`, because the baseline does not implement
substitution variables and would otherwise be credited for skipping work.

### Matching one pattern

| Pattern | vs `regex` |
| --- | --- |
| `/help/website/faq` (literal) | **6.4x faster** |
| `/buy/*` (prefix) | **22x faster** |
| `*/checkout` (suffix) | **18x faster** |
| `/id/????` | 1.7x slower |
| `/id/$(digit)$(digit)$(digit)$(digit)` | 1.7x slower |
| `/a/*/b/?*/c` | 2.3x slower |
| `*a*a…*b` on 512 `a`s (adversarial) | 1.7x slower |

The first three shapes cover almost every pattern in a real association file and take
allocation-free string tests. On genuinely general patterns a mature DFA beats a glob matcher by
under 2.5x — the honest cost of not shipping a regex engine — and the adversarial row shows neither
engine degrades catastrophically.

### Compiling

| | vs `regex` |
| --- | --- |
| one pattern (literal / prefix / `????` / mixed) | **25x – 295x faster** |
| a 0.4 KiB document | **22x faster** |
| a 5 KiB document, 128 rules | **24x faster** |
| a 38 KiB document, 1024 rules | **28x faster** |

Read the document rows next to this one, because most of that gap is the regex compiler rather than
the JSON parser:

| JSON parse only | `blazingly-json` vs `serde_json` |
| --- | --- |
| 0.4 KiB / 5 KiB / 38 KiB | 1.13x / 1.18x / 1.33x faster |

### Matching a real document

**This is where the crate is slower than the baseline, and the reason is worth stating plainly.**

| | vs `serde_json` + `regex` |
| --- | --- |
| 8 URLs against 8 apps x 16 rules | 2.4x slower |
| a miss scanned across 1 / 8 / 32 app entries | 2.1x / 1.9x / 1.6x slower |

Before this crate was checked against Apple's `swcutil` it was at parity here — 0.99x, 1.00x, 1.00x
on those same rows. It got slower by getting correct. `swcutil` settled four behaviours the
baseline does not implement at all:

* a pattern ending in `/*` also matches the parent path, so `/buy/*` needs two comparisons;
* every occurrence of a repeated query name must match, so the predicate loop cannot stop at the
  first hit;
* a missing query item counts as present with an empty value, so absence is a comparison rather
  than an immediate reject;
* the leading slash of a pattern is optional.

The baseline is faster partly because it is wrong. A comparison that omitted that would be
measuring less work, not better work.

Two things did come back from the first, naive version of those rules: trimming the path once per
match instead of once per rule, and deciding at compile time that a `/`-rooted pattern can never
match a path without one. That took the regression from 5.9x down to the rows above — the cost
settles around 2x, and the more app entries a miss has to scan, the smaller it gets, because the
per-rule work the baseline skips is not what dominates at that point.

| | |
| --- | --- |
| `compiled.decide(...)` vs reparsing per call | **924x faster** |
| `decide` vs `match_url` with a full trace | trace costs ~7x |

Parse once, match many. The trace is why `decide` and `match_url` are separate calls rather than one
function with a flag.

### WebAssembly against pure JavaScript

Against a `JSON.parse` + `RegExp` implementation — the JavaScript equivalent of the Rust baseline:

| | WebAssembly vs pure JS |
| --- | --- |
| compile 0.4 KiB / 5 KiB / 38 KiB | 0.71x / 0.66x / 1.19x |
| match, `decideLines` batch | 0.82x – 1.20x |

**Roughly a wash, and sometimes worse.** Moving a string across the boundary costs more than
matching it, and that cost is per string, so it does not amortise over a batch. The earlier ~2x
compile advantage narrowed when compilation took on the parent-path form.

The reason to use the WebAssembly build is not speed. It is that the semantics are the ones
verified against `swcutil`, with the same diagnostics, traces, and diff — rather than a second
implementation that will drift, which is exactly what the pure-JS tools in the comparison above
turned out to be.

Payload: 358 KB raw, 144 KB gzip, 115 KB brotli.

## Correctness

Apple's reference pages leave real questions open. Rather than guessing and presenting the guess as
fact, every behaviour is classified:

- **oracle** — checked against Apple's `swcutil`, with the run committed.
- **documented** — Apple states it and a test asserts it, but no oracle run covers it.
- **decided** — Apple does not state it and the oracle cannot speak to it.

[docs/parity.md](docs/parity.md) is that table, feature by feature. **139 of the 140 matching cases
are now verified against Apple's own `swcutil`**, with the raw runs committed in
`conformance/oracle` so the conclusions are auditable without a Mac. The one exception is this
crate's own API convention that an empty domain skips the host check, which `swcutil` has no way to
express.

**`$(region)` does not match `UK`.** Apple's prose gives "`CA`, `UK`, and `US`" as example regions,
but `UK` is not an ISO 3166-1 alpha-2 code and does not appear in `Locale.isoRegionCodes` — the
United Kingdom is `GB`. The `$(region)` and `$(lang)` tables are *generated* from Foundation by
`scripts/generate_iso_tables.swift` rather than transcribed, so the list Apple points at wins over
the prose. `swcutil` agrees: it does not match `UK` either.

**And the discipline caught this crate being wrong four times.** The first differential run against
`swcutil` agreed on 68 of 73 cases. The other four were all this crate's fault, including one it had
been confident enough about to ship as a lint: `AASA191` warned that a path pattern without a
leading slash could never match, since URL paths start with `/`. Apple matches `abc` against `/abc`.
The lint was removed, its number retired, and the documentation example it contradicted now passes
as a test. The others were a missing query item, a repeated query name, and a non-string predicate —
see [docs/parity.md](docs/parity.md) for each.

The test suite is 114 tests across Apple's documented examples, parsing, validation, matching,
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
| [docs/findings.md](docs/findings.md) | what it actually caught — in production files, and in its own code |
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
