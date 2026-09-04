# Diagnostics

Every finding carries a stable code. Codes are a public contract: new ones are added in minor
releases, and an existing one is never repurposed. Build CI checks against `AASA###`, not against
message text — messages are meant to be readable and will be reworded.

```rust
let report = blazingly_aasa::validate(bytes)?;
if report.has_errors() {
    for diagnostic in report.errors() {
        eprintln!("{diagnostic}");
    }
    std::process::exit(1);
}
```

Each diagnostic has a `code`, a `severity`, a dotted `path` into the document
(`applinks.details[0].components[2]./`), a `message`, and often a `help` line with the fix.

Reports are sorted most severe first, then by path.

| Severity | Meaning |
| --- | --- |
| `error` | malformed or self-contradictory; something here cannot work |
| `warning` | legal, but almost certainly not what was meant |
| `info` | worth knowing; no action implied |

## Codes

### Structure

| Code | Severity | Meaning |
| --- | --- | --- |
| `AASA001` | error | payload is not valid JSON |
| `AASA002` | error | root value is not a JSON object |
| `AASA004` | error | a field has an unexpected JSON type |
| `AASA100` | warning | no recognized Associated Domains service section |
| `AASA101` | info | unrecognized top-level key — ignored, as Apple ignores it |

`AASA001` and `AASA002` surface as a `ParseError` rather than in a report, because there is no
document to report against.

### Details and app identifiers

| Code | Severity | Meaning |
| --- | --- | --- |
| `AASA110` | error | a details entry has neither `appID` nor `appIDs` |
| `AASA111` | warning | a details entry sets both `appID` and `appIDs` |
| `AASA120` | warning | an entry mixes modern `components` with legacy `paths` |
| `AASA121` | warning | `details` uses the legacy dictionary form |
| `AASA122` | warning | the legacy `applinks.apps` array is not empty |
| `AASA130` | error | an application identifier is empty |
| `AASA131` | warning | an identifier is not `<TeamID>.<BundleID>` shaped |
| `AASA160` | warning | an identifier appears more than once |
| `AASA193` | warning | `applinks` declares no details, so no app can open this domain |

### Substitution variables

| Code | Severity | Meaning |
| --- | --- | --- |
| `AASA140` | error | a variable name contains `$`, `(` or `)` |
| `AASA141` | error | a value references another substitution variable |
| `AASA142` | error | a pattern references an undefined variable |
| `AASA143` | warning | a variable has no values and can never match |
| `AASA144` | warning | a variable shadows a predefined Apple variable |
| `AASA151` | error | a pattern contains an unterminated `$(` |
| `AASA194` | warning | a variable contains an empty alternative |

A pattern that triggers `AASA142` or `AASA151` compiles to something that never matches. A broken
pattern must not accidentally open a domain.

### Rules

| Code | Severity | Meaning |
| --- | --- | --- |
| `AASA150` | error | a query predicate is not a string pattern |
| `AASA180` | warning | a rule constrains nothing and matches every URL |
| `AASA190` | warning | a rule is unreachable because an earlier rule always matches |
| `AASA192` | warning | a `defaults` object carries pattern keys this crate does not yet apply |

`AASA191` was removed before the first release. It warned that a path pattern without a leading
slash could never match, which Apple's `swcutil` disproves — a pattern of `abc` matches `/abc`.
The number is retired rather than reused: a code never changes meaning.

`AASA180` and `AASA190` travel together: the catch-all is reported once, and every rule it shadows
is reported as unreachable. Both are worth failing CI on — a rule that never runs is either a typo
or dead configuration.

`AASA150` deserves more alarm than its name suggests. A non-string predicate such as `"flag": true`
does not disable that one entry — `swcutil` discards the **entire** `?` dictionary, so every
constraint beside it stops applying and the rule matches URLs it was never meant to. Fail on it.

### Limits

| Code | Severity | Meaning |
| --- | --- | --- |
| `AASA170` | error | the payload exceeds the configured size limit |

Also surfaced as `ParseErrorKind::TooLarge`, since nothing is parsed.

## Suggested CI policy

A reasonable starting point, in rough order of how much they hurt in production:

* **Fail** on any `error`.
* **Fail** on `AASA190` and `AASA180` — a rule that never runs is a bug either way.
* **Warn** on `AASA120`, `AASA121`, `AASA122` — legacy shapes worth migrating on your own schedule.
* **Ignore** `info`.

## Enumerating the codes

`DiagnosticCode::all()` returns every code this release knows, in ascending order, with
`as_str()`, `title()`, and `default_severity()`. Useful for generating your own documentation or
an allow-list, and a test asserts the codes are unique and ordered.
