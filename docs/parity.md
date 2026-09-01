# Parity

What is implemented, and how confident you should be in it.

Three levels of evidence, and the difference between them matters:

| Mark | Meaning |
| --- | --- |
| **documented** | Apple states the behaviour, and a test asserts it |
| **decided** | Apple does not state it; this crate chose a reading and pinned it with a test |
| **oracle** | Checked against Apple's `swcutil` and promoted into a fixture |

Nothing is marked **oracle** yet. `swcutil` requires root and exists only on macOS, so it is not
part of ordinary CI; `scripts/oracle_swcutil.sh` runs it on demand. Until a row moves to **oracle**,
this crate does not claim bit-exact parity with iOS. It claims to implement what Apple documents,
and to be explicit about everything Apple does not.

## Structure

| Feature | Status | Evidence |
| --- | --- | --- |
| `applinks.details` array | documented | `tests/apple_examples.rs` |
| `appID` | documented | `tests/apple_examples.rs` |
| `appIDs` | documented | `tests/apple_examples.rs` |
| both `appID` and `appIDs` on one entry | decided — union of the two, warned as `AASA111` | `tests/parsing.rs` |
| `details` as a dictionary keyed by app ID | decided — supported, sorted-key order, warned as `AASA121` | `tests/apple_examples.rs` |
| legacy `applinks.apps` | documented — must be empty, warned as `AASA122` | `tests/validation.rs` |
| `webcredentials` / `appclips` / `activitycontinuation` | documented | `tests/apple_examples.rs` |
| unknown top-level keys | decided — ignored, noted as `AASA101` | `tests/parsing.rs` |
| several entries listing the same app | decided — source order, warned as `AASA160` | `tests/matching.rs` |

## Matching

| Feature | Status | Evidence |
| --- | --- | --- |
| ordered rules, first match wins | documented | `tests/apple_examples.rs` |
| `exclude` stops the scan | documented | `tests/matching.rs` |
| every specified component must match | documented | `tests/apple_examples.rs` |
| unspecified component defaults to `*` | documented | `tests/matching.rs` |
| absent component reads as empty string | documented (implied by Apple's `"#": "*"` example) | `tests/matching.rs` |
| `*`, `?`, `?*` | documented | `src/pattern.rs`, `tests/properties.rs` |
| `?` counts characters, not bytes | decided | `src/pattern.rs` tests |
| no escape syntax | decided — Apple documents none | `docs/semantics.md` |
| query as a whole string | documented | `tests/matching.rs` |
| query as a dictionary | documented | `tests/apple_examples.rs` |
| fragment | documented | `tests/apple_examples.rs` |
| `caseSensitive` and its hierarchy | documented | `tests/apple_examples.rs`, `tests/matching.rs` |
| non-ASCII case folding | decided — simple 1:1 folding; full folding out of scope | `docs/semantics.md` |
| query item names follow `caseSensitive` | decided | `tests/matching.rs` |
| item without `=` has an empty value | decided | `tests/matching.rs` |
| repeated query name matches if any occurrence does | decided | `tests/matching.rs` |
| non-string query predicate | decided — never matches, error `AASA150` | `tests/validation.rs` |
| host must equal the served domain | decided — this crate's check, not part of the file | `tests/matching.rs` |
| non-`https` scheme | decided — still matched, reported as a note | `tests/matching.rs` |
| explicit port | decided — preserved, reported as a note | `tests/matching.rs` |

## Percent encoding

The least settled area, and the one most worth running the oracle against.

| Feature | Status | Evidence |
| --- | --- | --- |
| `percentEncoded: true` compares against the URL as written | decided | `tests/encoding.rs` |
| `percentEncoded: false` decodes the URL first | decided | `tests/encoding.rs` |
| `%2F` is not a path separator under the default | decided | `tests/encoding.rs` |
| escape hex case is significant when comparing encoded text | decided | `tests/encoding.rs` |
| invalid escapes are left as written | decided | `tests/encoding.rs` |

Apple documents the key as "whether URLs are percent-encoded" and nothing more. Two readings are
compatible with that sentence — encode the pattern, or decode the URL — and they agree on ordinary
input while diverging on encoded separators. This crate decodes the URL, because encoding a pattern
that contains `*` and `?` is not well defined.

## Substitution variables

| Feature | Status | Evidence |
| --- | --- | --- |
| custom variables | documented | `tests/apple_examples.rs` |
| names are case-sensitive, may not contain `$ ( )` | documented | `tests/validation.rs` |
| values may contain `?` and `*` | documented | `src/pattern.rs` |
| values may not reference other variables | documented — error `AASA141` | `tests/validation.rs` |
| `$(alpha)` `$(upper)` `$(lower)` `$(alnum)` `$(digit)` `$(xdigit)` | documented | `src/substitution.rs` |
| each predefined entry matches one alternative | decided — follows from Apple describing them as lists | `tests/apple_examples.rs` |
| `$(region)` from `Locale.isoRegionCodes` | documented — table generated from Foundation | `tests/apple_examples.rs` |
| `$(lang)` from `Locale.isoLanguageCodes` | documented — table generated from Foundation | `tests/apple_examples.rs` |
| `$(region)` does not match `UK` | decided — see below | `tests/apple_examples.rs` |
| a custom variable shadowing a predefined name | decided — the document wins, warned as `AASA144` | `tests/validation.rs` |
| an undefined `$(name)` | decided — never matches, error `AASA142` | `tests/validation.rs` |

### The `UK` divergence

Apple's reference describes `$(region)` as "All ISO regions in isoRegionCodes, such as `CA`, `UK`,
and `US`". `UK` is not an ISO 3166-1 alpha-2 code and does not appear in `Locale.isoRegionCodes` —
the United Kingdom is `GB`. The generated table follows the list Apple points at rather than the
prose example, so a pattern of `$(region)` does not match `UK`.

This is worth confirming against `swcutil` if you depend on it. It is pinned by
`region_table_follows_foundation_not_apples_prose`, so a future decision to change it has to be
deliberate.

### Table drift

`$(region)` and `$(lang)` change between OS releases. `blazingly_aasa::ISO_TABLE_SOURCE` reports
which snapshot is compiled in, and the scheduled `apple-oracle` workflow warns when a macOS runner
disagrees with the committed tables.

## Path patterns without a leading slash

Apple's `components` reference contains this example:

```json
{ "/": "abc", "?": "def", "#": "*" }
```

and says `https://www.example.com/abc?def` matches it. Every other Apple example writes the path
with a leading slash (`/buy/*`), and a URL path always begins with `/`.

The two negatives in that passage are unambiguous and are asserted as tests. The positive is not,
so this crate matches the full path — including the leading `/` — and reports a pattern that cannot
match one as `AASA191` with a suggested fix. That turns an ambiguity into a useful lint instead of
a guess. `swcutil` can settle it.

## Deliberately not implemented

Not gaps — boundaries. These belong to the tools that consume this crate:

network fetching, `.well-known` lookup, Apple CDN behaviour, HTTP redirects and caching, `.ipa`
or `.app` inspection, Mach-O parsing, code signature verification, entitlement extraction, device
state, Safari behaviour, and the App Store Connect API.

See `aasadiff-integration.md` for where the line sits.
