# Consuming this crate

`blazingly-aasa` is a semantic engine. It takes bytes and explicit context and tells you what the
document says. It has no idea where the bytes came from, and that is deliberate.

[`blazingly-aasa-mcp`](https://github.com/sergii-ziborov/blazingly-aasa-mcp) is the worked example
of everything below: it owns the HTTPS client, the MCP protocol, and the presentation, and depends
on this crate for every semantic answer.

## The boundary

| This crate owns | A tool built on it owns |
| --- | --- |
| parsing and validation | fetching `.well-known/apple-app-site-association` |
| the defaults hierarchy | Apple CDN behaviour and freshness |
| pattern matching and traces | HTTP status, redirects, TLS, caching headers |
| semantic comparison | reading entitlements out of a signed binary |
| stable diagnostic codes | device state, install state, Safari behaviour |
| service membership | UI, CI policy, reporting |

Everything in the right column would drag in a network stack, a Mach-O parser, or a model of iOS
behaviour that would be wrong within a release. Keeping them out is what lets this crate stay small
enough to compile to a WebAssembly module you would actually ship.

## Origin against CDN

The comparison most Associated Domains tooling exists to make: the file you serve, against the one
Apple's CDN is handing to devices.

```rust
use blazingly_aasa::CompiledAasa;

let origin = CompiledAasa::parse(&origin_bytes)?;
let cdn = CompiledAasa::parse(&cdn_bytes)?;

let diff = origin.semantic_diff(&cdn);
if !diff.is_equivalent() {
    for change in diff.changes() {
        println!("{change}");
    }
}
# Ok::<(), blazingly_aasa::ParseError>(())
```

The point of a *semantic* diff here is that a CDN copy is often reformatted, reordered by key, or
re-serialised. A textual diff of those two files is mostly noise. This one reports a change only
when a decision changed:

```
RULE_CHANGED    ABCDE12345.com.example.app #2
  before: / = /help/*, caseSensitive=false, percentEncoded=true
  after:  / = /help/*, caseSensitive=true, percentEncoded=true
```

Rule *order* is compared too, because order decides which rule wins. Hoisting `caseSensitive` out
of ten components into a `defaults` object is reported as no change; swapping two rules is reported
as a move.

## Cross-checking against a binary

If you have extracted an app's `application-identifier` and its Associated Domains entitlement —
with your own code, in your own crate — this side of the check is one call:

```rust
let services = compiled.services_for_app(&app_id);
if services.is_empty() {
    println!("{app_id} claims {domain}, but the file grants it nothing");
}
```

`has_applink_app`, `has_webcredential_app`, `has_appclip`, and `has_activitycontinuation_app` cover
the per-service form.

## Reporting a decision to a human

Do not reimplement the explanation. Every result formats itself:

```rust
let result = compiled.match_url(domain, &app_id, url)?;
println!("{result}");
# Ok::<(), blazingly_aasa::Error>(())
```

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

`MatchResult` is also `Serialize`, so the same trace goes into JSON for a machine consumer.

## Things not to do

**Do not add fetching to this crate.** A tool that fetches has opinions about timeouts, redirects,
proxies, and caching. Those belong to the tool.

**Do not treat `NoMatch` or `Exclude` as errors.** They are answers. `Result::Err` is reserved for
input that cannot be interpreted.

**Do not claim device behaviour.** "This file considers the URL eligible for this app" is what the
crate knows. "This link will open the app" additionally depends on install state, entitlements, CDN
freshness, and how the user got there.

**Do not depend on diagnostic message text.** Depend on `DiagnosticCode`. Messages are meant to
read well and will be reworded; codes are the contract.
