# The existing tooling, and what it covers

Surveyed 2026-09-01 by reading the published source of every AASA tool I could find with real
usage. Versions and line references are from that date.

| Tool | Language | Stars | What it is |
| --- | --- | --- | --- |
| [`chayev/yurl`](https://github.com/chayev/yurl) | Go | 48 | The validator behind yurl.chayev.com |
| [`shortstuffsushi/Universal-Link-Validator`](https://github.com/shortstuffsushi/Universal-Link-Validator) | JavaScript | 125 | Hosted validator, node app |
| [`st-tech/universal-links-test`](https://github.com/st-tech/universal-links-test) | TypeScript | 36 | npm package, simulates `swcutil` |
| [`@linkforty/aasa-core`](https://www.npmjs.com/package/@linkforty/aasa-core) | TypeScript | — | Validation engine + widget/worker packages |
| Branch.io AASA validator | closed | — | Hosted, no source |

## The matrix

| | yurl | ULV | universal-links-test | linkforty | **blazingly-aasa** |
| --- | :-: | :-: | :-: | :-: | :-: |
| **Hosting / network** | | | | | |
| fetch `.well-known`, redirects, content-type | ✅ | ✅ | ✗ | ✅ | ✗ *by design* |
| Apple CDN debug headers | ✅ | ✗ | ✗ | ✗ | ✗ *by design* |
| **Structure** | | | | | |
| JSON + shape validation | basic | basic | type guards | ✅ | ✅ |
| stable machine-readable codes | ✗ | ✗ | ✗ | ✅ | ✅ 27 codes |
| team / bundle identifier presence | ✅ | ✅ | ✗ | ✅ | ✅ |
| unreachable / catch-all rule lint | ✗ | ✗ | ✗ | ✗ | ✅ |
| **Matching** | | | | | |
| evaluates a URL at all | ✗ | ✗ | ✅ | ✗ | ✅ |
| ordered rules, first match wins | — | — | ✅ | — | ✅ |
| `exclude` | — | — | ✅ | — | ✅ |
| defaults hierarchy | — | — | ✅ | — | ✅ |
| query as string **and** dictionary | — | — | ✅ | — | ✅ |
| **`$(...)` substitution variables** | ✗ | ✗ | ✗ | ✗ | ✅ |
| **`$(region)` / `$(lang)` ISO tables** | ✗ | ✗ | ✗ | ✗ | ✅ |
| **`percentEncoded`** | ✗ | ✗ | ✗ *TODO* | ✗ | ✅ |
| **legacy `paths` / `NOT`** | ✗ | ✗ | ✗ | ✗ | ✅ |
| legacy `details` dictionary | ✗ | ✗ | ✗ | ✗ | ✅ |
| repeated query keys | — | — | first only | — | any occurrence |
| **CMS-signed (iOS 9) files** | ✅ | ✗ | ✗ | ✗ | ✅ *reads, does not verify* |
| **Beyond validation** | | | | | |
| explains *why* a URL failed | ✗ | ✗ | ✗ | ✗ | ✅ |
| which apps a URL reaches | ✗ | ✗ | ✅ | ✗ | ✅ |
| semantic diff of two files | ✗ | ✗ | ✗ | ✗ | ✅ |
| canonical normalized output | ✗ | ✗ | ✗ | ✗ | ✅ |

## The three gaps that were real, and are now closed

Reading the source turned up three things a competitor did that this crate did not. All three are
now implemented, with tests in `tests/coverage.rs` naming the tool they came from.

**Which apps does this URL reach?** `universal-links-test` returns a `Map<appID, "match" | "block">`
for a URL. Asking that of a per-app API means rescanning the rules once per app.
`apps_for_url` answers it in one pass, and a property test asserts it never disagrees with
`decide`.

**Team prefix and bundle identifier as separate inputs.** `yurl` and `@linkforty/aasa-core` both
take them apart, because that is how Xcode shows them. `services_for_bundle(team, bundle)`,
`app_ids_for_bundle(bundle)` — the second answers "which team prefix does this file still name for
my app", which is the symptom when an app moves between teams.

**CMS-signed files.** Before iOS 10 the file had to be a PKCS#7 `SignedData` blob with the JSON
inside. `yurl` handles those; every JavaScript tool reports them as invalid JSON at byte 0. This
crate now detects the DER, extracts the payload, and reports `AASA200` saying in as many words that
the signature was **not** verified — reading is not checking, and a semantics crate has no business
carrying a trust store.

## The gap nobody else has closed

**No surveyed tool evaluates `$(...)` at all.** All of them declare `substitutionVariables` in
their type definitions and then ignore it when matching.

`universal-links-test` is the instructive case, because it does simulate matching. Its pattern
translation escapes `$`, `(` and `)` before handling wildcards
([`sim/regex.js`](https://github.com/st-tech/universal-links-test/blob/main/src/sim/regex.ts)), so
a rule of `/order/$(food)/*` compiles to a regex matching the literal text `/order/$(food)/`. A
file using substitution variables does not fail loudly there — it silently matches nothing, which
is the worst way for a link check to be wrong.

`$(region)` and `$(lang)` need Foundation's `isoRegionCodes` and `isoLanguageCodes`, 257 and 631
entries. This crate generates them from Foundation itself (`scripts/generate_iso_tables.swift`)
rather than transcribing them, and `ISO_TABLE_SOURCE` reports which OS release the snapshot came
from.

## Measured, not asserted

The matrix above is a reading of source. The corpus in `conformance/` turns it into a number.
`universal-links-test` is the only surveyed tool with a matching simulator, so it is the only one
that can be scored:

```bash
npm pack universal-links-test && tar xzf universal-links-test-*.tgz
node conformance/run.mjs --exec "node your-adapter.mjs ./package/dist/sim/index.js" --skip host
```

Result on 2026-09-03 against v0.1.0 of that package, on the oracle-verified corpus. The three
`host` cases are skipped: it takes a path relative to a fixed origin and does not claim to check
hosts.

| Feature | universal-links-test | blazingly-aasa |
| --- | --- | --- |
| rule order, `exclude` | 11/11 | 11/11 |
| defaults hierarchy | 3/3 | 3/3 |
| component defaults | 5/5 | 5/5 |
| `appID` / `appIDs` | 3/3 | 3/3 |
| wildcards | 7/8 | 8/8 |
| query | 22/38 | 38/38 |
| percent encoding | 8/15 | 15/15 |
| path slashes | 13/25 | 25/25 |
| legacy `paths` and `details` | 1/4 | 4/4 |
| **substitution variables** | **10/20** | 20/20 |
| **total** | **88/137** | 137/137 |

It is genuinely solid on the core: rule ordering, `exclude`, the defaults hierarchy, and component
defaults are all correct, and that is the part most implementations get wrong first.

The substitution row needs reading carefully, and it is the reason this crate exists. Exactly 10 of
those 20 cases expect `no_match` and exactly 10 expect `match`. It passes all ten of the first kind
and none of the second — because a pattern it cannot interpret never matches anything. Its score
there is not "half right"; it is **zero right, with half the cases passing by accident.** That is
what makes the failure mode dangerous: a file using `$(lang)` does not blow up, it quietly stops
matching, and a green check mark says nothing is wrong.

The runner prints the trivial-pass count for exactly this reason. Any comparison that does not is
overstating the loser.

The path-slashes and percent-encoding rows are mostly behaviours neither implementation knew about
until `swcutil` settled them — this crate was wrong on four of them too, and
[findings.md](findings.md) says which.

## Where the others are ahead, deliberately

`yurl`, `Universal-Link-Validator`, and `@linkforty/aasa-core` all fetch the file, follow
redirects, check `Content-Type`, and read Apple's CDN debug headers. That is genuinely valuable and
genuinely not this crate's job — it would drag in an HTTP stack, opinions about proxies and
timeouts, and a model of Apple CDN behaviour that changes without notice. See
`aasadiff-integration.md` for where the line sits and how to build that layer on top.

The honest summary: **they are validators, this is an engine.** A validator built on this engine
would cover everything in the matrix.

## Reproducing this survey

```bash
npm pack universal-links-test @linkforty/aasa-core
git clone --depth 1 https://github.com/chayev/yurl
git clone --depth 1 https://github.com/shortstuffsushi/Universal-Link-Validator
```

The matching logic is in `dist/sim/{regex,match,verify}.js`, `yurllib/aasa.go`, and
`dist/index.js` respectively. Corrections welcome — if a tool has grown a capability this table
denies it, that is a bug in the table.
