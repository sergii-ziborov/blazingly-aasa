# Parity

What is implemented, and how confident you should be in it.

| Mark | Meaning |
| --- | --- |
| **oracle** | Checked against Apple's `swcutil` and recorded in [`conformance/oracle`](../conformance/oracle) |
| **documented** | Apple states the behaviour and a test asserts it, but no oracle run covers it |
| **decided** | Apple does not state it and the oracle cannot speak to it; this crate chose a reading |

139 of the 140 matching cases in `conformance/cases.json` are **oracle**-verified against
`swcutil` on macOS 27.0 (26A5388g), 2026-09-02. The remaining one is this crate's own API
convention, which `swcutil` has no way to express.

## What the oracle changed

The first differential run put 68 of 73 corpus cases in agreement. One disagreement was an
artifact of the harness. **The other four were this crate being wrong**, and all four are now
fixed. Two more findings came out of the 67 targeted probes that followed.

### A path pattern without a leading slash matches after all

Apple's `components` reference contains `{"/": "abc", "?": "def", "#": "*"}` and says
`https://www.example.com/abc?def` matches it, while every other example writes `/buy/*`. This crate
read the bare `abc` as unmatchable and raised `AASA191` for it — "a URL path always starts with
`/`", which sounded obviously true.

`swcutil` matches `abc` against `/abc`, and `buy/*` against `/buy/42`. **The documentation example
was right and the lint was wrong.** `AASA191` was removed and its number retired; the leading slash
of a path pattern is optional.

### Trailing slashes are insignificant

`/buy/*` matches `/buy`, `/buy` matches `/buy/`, `/buy/` matches `/buy`, and `/buy/*` matches
`/buy//`. A leading run of slashes in a pattern collapses to one, so `//abc` matches `/abc`.

This one has a trap. The obvious implementation — also try the path with a trailing slash added —
makes `/id/????` match `/id/481`, because `481/` is four characters. `swcutil` says that does not
match, and the conformance corpus caught it before the change shipped. The actual rule is narrower:
a trailing slash run is *dropped* from both sides, and a pattern ending in `/*` additionally
matches the path without that segment.

### A missing query item counts as empty

`{"b": "*"}` matches a URL with no `b` at all, and `{"b": ""}` does too. `{"b": "?*"}` does not,
because it needs at least one character. This crate previously failed any predicate whose item was
absent.

### Every occurrence of a repeated query name must match

`{"id": "42"}` does **not** match `?id=7&id=42`, in any position — not first, not last, not any.
But `{"id": "7"}` does match `?id=7&id=7`. The rule is that all occurrences must satisfy the
pattern. This crate previously accepted any single match, which was the most permissive of the
three plausible readings and the wrong one.

### A non-string predicate discards the whole query dictionary

`{"a": "1", "flag": true}` matches `?a=2`. Not because `a=2` satisfies `a: "1"` — it does not — but
because a single non-string predicate makes `swcutil` ignore **the entire `?` object**, taking
every constraint beside it with it.

This crate previously made such a predicate never match, on the principle of refusing rather than
guessing. That was the wrong direction: Apple is more permissive here, not less, so the safe-looking
choice produced false negatives. `AASA150` remains an error, and its documentation now says what it
actually costs.

## Structure

| Feature | Status |
| --- | --- |
| `applinks.details` array, `appID`, `appIDs` | oracle |
| both `appID` and `appIDs` on one entry — union, warned as `AASA111` | documented |
| `details` as a dictionary keyed by app ID | oracle |
| legacy `applinks.apps` must be empty | documented |
| `webcredentials` / `appclips` / `activitycontinuation` | oracle |
| unknown top-level keys ignored, noted as `AASA101` | documented |
| CMS-signed (iOS 9) files read, signature not verified | decided |

## Matching

| Feature | Status |
| --- | --- |
| ordered rules, first match wins, `exclude` stops the scan | oracle |
| every specified component must match | oracle |
| unspecified component defaults to `*` | oracle |
| absent component reads as the empty string | oracle |
| `*`, `?`, `?*` | oracle |
| leading slash of a path pattern optional | oracle |
| trailing slash insignificant; `/*` matches the parent path | oracle |
| leading slash run in a pattern collapses | oracle |
| `?` counts characters, not bytes | decided |
| query as a whole string | oracle |
| query as a dictionary | oracle |
| missing query item counts as empty | oracle |
| all occurrences of a repeated query name must match | oracle |
| non-string predicate discards the whole dictionary | oracle |
| fragment | oracle |
| `caseSensitive` and its three-level hierarchy | oracle |
| non-ASCII case folding — simple 1:1, not full folding | decided |
| legacy `paths` with `NOT ` | oracle |
| host must equal the served domain | oracle |
| empty domain means "skip the host check" | decided — this crate's API; `swcutil` requires `-d` |
| non-`https` scheme still matched, reported as a note | decided |

## Percent encoding

Previously the least certain area of the crate. **Every behaviour is now oracle-confirmed**, and
none of them changed:

| Behaviour | Status |
| --- | --- |
| `percentEncoded: true` compares against the URL as written | oracle |
| `percentEncoded: false` decodes the URL, leaving the pattern alone | oracle |
| `%2F` is not a path separator under the default | oracle |
| escape hex case is significant when comparing encoded text | oracle |
| a pattern that is itself encoded stops matching once decoding is on | oracle |

That last row is the one that settles it. Two readings of Apple's one-sentence description were
possible — decode the URL, or encode the pattern — and they agree on ordinary input. They disagree
on `{"/": "/a%20b", "percentEncoded": false}` against `/a%20b`: decoding the URL yields `/a b`,
which the still-encoded pattern does not match, while encoding the pattern would match. `swcutil`
does not match. This crate decodes the URL, and that is now confirmed rather than argued.

## Substitution variables

| Feature | Status |
| --- | --- |
| custom variables, values with wildcards | oracle |
| names case-sensitive, may not contain `$ ( )` | documented |
| values may not reference other variables — `AASA141` | documented |
| `$(alpha)` `$(upper)` `$(lower)` `$(alnum)` `$(digit)` `$(xdigit)` | oracle |
| each predefined entry matches one alternative | oracle |
| `$(region)` from `Locale.isoRegionCodes` | oracle |
| `$(lang)` from `Locale.isoLanguageCodes` | oracle |
| `$(region)` does not match `UK` | oracle |
| folding applies to predefined variables under `caseSensitive: false` | oracle |
| custom variable shadowing a predefined name — `AASA144` | decided |
| an undefined `$(name)` never matches — `AASA142` | decided |

All twenty substitution cases agreed with `swcutil` on the first run, including the `UK` finding:
Apple's prose lists it as an example region, `Locale.isoRegionCodes` does not contain it, and
`swcutil` does not match it.

## Reproducing

```bash
sudo ./scripts/oracle_swcutil.sh
```

macOS only, and root only, because `swcutil` refuses to run any subcommand otherwise — which is why
this is not part of ordinary CI. `conformance/oracle` holds the raw output so the conclusions can
be audited without a Mac.

The useful subcommand is `swcutil match -u <url> -j <dict>`, which tests one pattern dictionary
against one URL with no document structure in the way. `swcutil verify -d <domain> -j <file> -u
<url>` exercises a whole document.

## Deliberately not implemented

Network fetching, `.well-known` lookup, Apple CDN behaviour, HTTP redirects and caching, `.ipa` or
`.app` inspection, Mach-O parsing, code signature verification, entitlement extraction, device
state, and the App Store Connect API. Those belong to
[`blazingly-aasa-mcp`](https://github.com/sergii-ziborov/blazingly-aasa-mcp) and to tools like it.
