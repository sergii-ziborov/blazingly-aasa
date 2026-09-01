# Semantics

What this crate implements, and where each rule comes from. Every claim below is either quoted
from Apple's reference pages (linked at the bottom) or marked as a decision this crate made in the
absence of documentation. `parity.md` tracks which of those decisions have been checked against
Apple's own tooling.

## The three questions

Associated Domains tooling tends to collapse three separate questions into one boolean. This crate
keeps them apart, because the answers have different failure modes.

| Question | API | Fails when |
| --- | --- | --- |
| Is this parseable? | `AasaDocument::parse` | invalid JSON, non-object root, over the size limit |
| Is this sane? | `CompiledAasa::validate` | never — it returns diagnostics, not a verdict |
| Does this URL match? | `CompiledAasa::match_url` | only when the *URL* is unusable |

A URL that does not match is `MatchDecision::NoMatch`. A URL the file explicitly blocks is
`MatchDecision::Exclude`. Neither is an error, and a CLI built on this crate should not treat them
as one.

## What a match does and does not mean

A match means: **this document considers this URL eligible for this app identifier.**

It does not mean the link will open the app. That additionally depends on the app being installed,
its Associated Domains entitlement listing the domain, what Apple's CDN is currently serving for
the domain, and how the user arrived at the link. This crate has no way to know any of that and
does not pretend to.

## Rule order

`components` is an array, and the order is load-bearing:

> The order that you use to specify the patterns in the array determines the order the system
> follows when looking for a match. The first match wins.

So the first rule whose specified components all match decides the outcome — and if that rule has
`"exclude": true`, matching stops there. It does **not** fall through to a later rule that would
have accepted the URL. Reversing two rules can therefore change the answer, which is why
`semantic_diff` reports a reorder as a change.

`details` is also an array. Apple does not document how several entries listing the same app
interact, so this crate scans entries in source order and takes the first matching rule found;
`AASA160` warns when an app appears in more than one entry.

## Which components must match

> A match occurs when a URL matches all the components that a `components` object specifies.

An unspecified component defaults to `*`, which matches everything — including an absent
component, which reads as the empty string. Apple's own example relies on this: `"#": "*"` matches
a URL with no fragment at all.

| Key | URL component | Default |
| --- | --- | --- |
| `/` | path | `*` |
| `?` | query | `*` |
| `#` | fragment | `*` |

`comment` is ignored by the system. This crate preserves it and surfaces it in traces, because it
is usually the most useful thing in the file when something breaks.

## Patterns

Three wildcards, and nothing else:

| Token | Meaning |
| --- | --- |
| `*` | zero or more characters, greedy |
| `?` | exactly one character |
| `?*` | one or more characters — just `?` followed by `*`, not a separate token |

There is no escape syntax. `*` and `?` are always wildcards, and `$(` always begins a substitution
reference. A pattern must match the **whole** component, not a prefix of it.

`?` counts characters, not bytes: `/?/ ` matches `/é/` and not `/éé/`.

### How they are matched

Patterns are compiled, not translated into a regular expression. Three engines, chosen at compile
time:

* **Literal, prefix, suffix, contains** — a pattern like `/buy/*` or `/help/website/faq` becomes a
  direct string test with no allocation. This covers the overwhelming majority of real patterns.
* **Glob** — anything built from literals, `?`, `*`, and single-character classes runs the
  classical greedy algorithm, which uses no heap and only ever reconsiders the most recent `*`.
* **Bitset NFA** — patterns containing a multi-character substitution set (`$(region)`,
  `$(lang)`, or a custom variable) need real alternation, so they run as an NFA over a bitset of
  reachable positions.

None of the three backtracks exponentially. The classic `*a*a*a…*b` blow-up is bounded by
`O(positions x tokens)`.

Input that is entirely ASCII — which URL components almost always are — is matched directly
against the string's bytes. Only non-ASCII input is expanded into a `Vec<char>`.

## Substitution variables

Apple describes custom and predefined variables the same way: a named list of alternative strings.

Custom variables come from `applinks.substitutionVariables`:

```json
"substitutionVariables": { "food": ["burrito", "pizza", "sushi", "samosa"] }
```

> The names you use for substitution variables are always case-sensitive and can contain any
> character except `$`, `(`, and `)`. The values you use with substitution variables are
> case-sensitive by default and can contain the `?` and `*` wildcard characters, but not other
> substitution variables.

All four constraints are validated (`AASA140`, `AASA141`, `AASA143`).

The predefined variables are lists Apple ships:

| Variable | Contents |
| --- | --- |
| `$(alpha)` | `A`–`Z`, `a`–`z` |
| `$(upper)` | `A`–`Z` |
| `$(lower)` | `a`–`z` |
| `$(alnum)` | `A`–`Z`, `a`–`z`, `0`–`9` |
| `$(digit)` | `0`–`9` |
| `$(xdigit)` | `0`–`9`, `A`–`F`, `a`–`f` |
| `$(region)` | `Locale.isoRegionCodes` |
| `$(lang)` | `Locale.isoLanguageCodes` |

Each entry in those lists is one alternative, so the ASCII classes match exactly one character and
`$(region)` matches exactly one two-letter code.

Because the lists *are* alternatives, case-insensitivity applies to them like anything else: under
`"caseSensitive": false`, `$(upper)` also accepts `a`.

### Where the region and language tables come from

`src/iso_tables.rs` is generated by `scripts/generate_iso_tables.swift` from Foundation itself, not
transcribed by hand. `blazingly_aasa::ISO_TABLE_SOURCE` records which OS release produced the
snapshot, because these lists change between releases.

One consequence worth knowing: Apple's prose gives `CA`, `UK`, and `US` as example regions, but
`UK` is not an ISO 3166-1 alpha-2 code and does not appear in `isoRegionCodes` — the United Kingdom
is `GB`. The generated table wins over the prose, so `$(region)` does not match `UK`. There is a
test pinning this.

## The defaults hierarchy

> You can specify pattern-matching values at three levels: domain, app, and URL. … The more
> specific definition overrides the less specific.

```
built-in defaults        caseSensitive = true, percentEncoded = true
  applinks.defaults      domain level
    details[].defaults   app level
      components[]       URL level — wins
```

`EffectiveDefaults` is what a rule actually runs under, and every match trace reports it. It is
also what makes `semantic_diff` able to see that hoisting `caseSensitive` out of ten components
into one `defaults` object changed nothing.

Apple documents `defaults` as "a subclass of `components`", implying it may also carry `/`, `?`,
and `#`. What those would mean there is not specified, so this crate applies only `caseSensitive`
and `percentEncoded` from a `defaults` object and reports anything else as `AASA192`.

## Case sensitivity

`caseSensitive` defaults to `true`. When false, comparison folds ASCII exactly and applies simple
1:1 lowercase folding to non-ASCII. Full Unicode case folding — where one scalar expands to several,
as with `İ` — is out of scope, because it would break the guarantee that `?` matches one character.

Query item *names* are compared under the same setting as their values. Apple does not say whether
names are case-sensitive; this crate treats them consistently with the rest of the rule.

## Query matching

`?` takes either form, and they are not equivalent.

**A string** matches against the whole query, exactly as written after `?`:

```json
{ "?": "a=1" }
```

matches `?a=1` and not `?a=1&b=2` — the pattern has to cover the entire query string.

**A dictionary** constrains only the items it names:

```json
{ "?": { "articleNumber": "????" } }
```

Every named predicate must hold; unnamed items are ignored. So this matches
`?articleNumber=4815&utm_source=x` but not `?articleNumber=481`.

Details this crate settled, none of which Apple documents:

* An item written without `=` (`?flag`) has an empty value, so `{"flag": ""}` matches it.
* If a name repeats (`?id=7&id=42`), the predicate holds when **any** occurrence matches.
* A predicate that is not a string — `{"flag": true}` — is reported as `AASA150` and never
  matches, rather than being guessed at.

## Percent encoding

`percentEncoded` defaults to `true`. Apple describes the key as indicating "whether URLs are
percent-encoded" without spelling out the comparison, so this crate implements the reading that
keeps patterns usable:

* **`true`** — the pattern is compared against the URL component exactly as it appears, still
  encoded. `/a%20b` matches a pattern of `/a%20b`, not one of `/a b`.
* **`false`** — the URL component is percent-decoded first, so a pattern may contain literal
  spaces and non-ASCII text. `/café/*` matches `/caf%C3%A9/menu`.

This matters for more than convenience. Under the default, `%2F` is not a path separator, so a rule
written as `/a/b` does not accept `/a%2Fb`. Turning decoding on erases that distinction — worth
knowing before you set `"percentEncoded": false` on a rule that guards something.

Invalid escapes are left as written rather than dropped, so `/100%zz` still matches itself.

This is the least certain area of the crate. See `parity.md`.

## Legacy formats

Two older shapes still appear in the wild, and both are supported.

**`paths`** — an array of path patterns where an entry prefixed with `NOT ` is an exclusion:

```json
{ "appID": "ABCDE12345.com.example.app", "paths": ["/test/*", "NOT /path/1/*"] }
```

Same ordering rule, same wildcards, matched against the path only.

**`details` as a dictionary** keyed by application identifier — the oldest form. It is supported and
reported as `AASA121`. A JSON object has no defined order, so the keys are evaluated in sorted
order; if order matters to you, migrate to the array form.

A single entry using both `components` and `paths` gets `AASA120`. Apple recommends against mixing
them and does not define the combined behaviour; this crate evaluates `components` first, then
`paths`, and says so rather than pretending the question is settled.

## The other services

`webcredentials`, `appclips`, and `activitycontinuation` are flat app lists with no URL matching:

```rust
compiled.has_webcredential_app(app_id);
compiled.services_for_app(app_id); // -> [AppLinks, WebCredentials]
```

## URL handling

URLs are split by a small RFC 3986 splitter rather than a full URL library, because matching
compares against the URL *as written*. Normalising or re-encoding first would change what the
patterns see. The only normalisation applied is lowercasing the scheme and host, which RFC 3986
defines as case-insensitive.

Consequences worth knowing:

* An empty path reads as `/`, matching how browsers and Foundation present it.
* Internationalised hosts are not punycode-converted. Compare hosts in the form the file is served
  under.
* A non-`https` scheme still matches, but the result carries a note. Apple serves and matches
  universal links over `https` only.
* A port is preserved and noted. Whether a port is allowed is decided by the app's entitlement, not
  by this file.
* Passing an empty `domain` skips the host check, which is useful when testing a file in isolation.

## Limits

`ParseOptions::DEFAULT_MAX_BYTES` is 128 KiB. That is a defensive default this crate chose for
handling remote, attacker-controlled input — not a limit stated on the Apple pages cited below.
Override it freely.

Matching cost is bounded by `O(path length x pattern tokens)`. Both factors are bounded by the size
limit, so leaving it in place is what keeps a hostile file from turning into a slow one.

## Sources

- [applinks](https://developer.apple.com/documentation/bundleresources/applinks)
- [applinks.Details](https://developer.apple.com/documentation/bundleresources/applinks/details-swift.dictionary)
- [applinks.Details.Components](https://developer.apple.com/documentation/bundleresources/applinks/details-swift.dictionary/components-swift.dictionary)
- [applinks.Defaults](https://developer.apple.com/documentation/bundleresources/applinks/defaults-swift.dictionary)
- [applinks.SubstitutionVariables](https://developer.apple.com/documentation/bundleresources/applinks/substitutionvariables-swift.dictionary)
- [TN3155: Debugging universal links](https://developer.apple.com/documentation/technotes/tn3155-debugging-universal-links)
- [Supporting associated domains](https://developer.apple.com/documentation/xcode/supporting-associated-domains)
