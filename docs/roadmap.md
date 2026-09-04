# What gets built next, and what does not

Two questions came up once the engine was working. Both are recorded here with the reasoning, so
the answers can be argued with rather than rediscovered.

---

## 1. Should there be a hand-written JavaScript port alongside the WebAssembly build?

**No. One engine, compiled to WebAssembly. But publish the conformance corpus so the question stops
being dangerous.**

### The case for a pure-JavaScript port is real

It is about size, not speed:

| | payload |
| --- | --- |
| `blazingly-aasa` (WebAssembly) | 358 KB raw, **144 KB gzip** |
| `@linkforty/aasa-core` | ~17 KB |
| `universal-links-test` | a few KB |

For a Node script or a CI check, 141 KB is irrelevant. For a browser widget that validates an AASA
file on a marketing page, it is roughly 10x what a competitor ships, plus an `await init()` before
anything works.

### The case against is bigger

Two implementations of the same subtle semantics **will** drift. Not might — will. The entire
argument for this crate over the existing tools is that it gets `percentEncoded`, `$(region)`, rule
ordering, and the defaults hierarchy right. Maintaining that twice, in two languages, doubles the
surface on which it can quietly stop being true. `universal-links-test` shows exactly how this
fails: it is not carelessly written, it simply did not implement `$(...)`, and the result is a file
that silently matches nothing while the check stays green.

And measurement already says the port would not buy speed. Matching through WebAssembly is *at
parity* with a pure-JavaScript implementation, because the string boundary costs more than the
match. A JavaScript port would win on bundle size only.

### So

1. **WebAssembly stays the only engine.** If bundle size becomes a real complaint, the fix is to
   shrink the module — the JSON engine and `serde-wasm-bindgen` dominate it, and neither has been
   optimised for size yet — not to fork the semantics.
2. **`conformance/cases.json` is published** so the question is no longer dangerous. 140 matching
   and 13 validation cases, each tagged with the feature it covers and whether Apple documents the
   behaviour, this crate decided it, or `swcutil` confirmed it. Both the Rust suite and the
   WebAssembly suite run it, and `conformance/run.mjs --exec` points it at any implementation in
   any language over the line protocol in `conformance/PROTOCOL.md`.
3. If a pure-JavaScript build is ever genuinely needed, it is now a tractable project rather than a
   liability: it has to pass 153 cases in CI, and drift becomes a failing test instead of a
   support ticket.

### Same repository, or separate?

**Same repository for the engine and its binding. Separate repositories for tools built on them.**

The binding is not an independent product — it is this crate with a different calling convention. A
semantics change has to update the Rust tests, the WebAssembly tests, and the corpus in one commit,
and CI has to run all three together. Splitting them means coordinating an npm release against a
crates.io release for every fix, which is pure overhead at this size.

The split belongs one layer up. A CLI, a React widget, a hosted validator, a GitHub Action — those
are consumers with their own release cadence, their own dependencies, and their own reasons to
change. Those should be separate.

---

## 2. Is an MCP server needed?

**Not in this crate. As a separate binary, yes — and it now exists:
[`blazingly-aasa-mcp`](https://github.com/sergii-ziborov/blazingly-aasa-mcp).**

The reasoning below is what the answer was before it was built. It is kept because the conclusion
it reached is the shape the thing actually took, and because the boundary it argued for is the one
that has to keep holding.

### Why not now

Consider what an MCP server over *just* this crate could offer an agent:

- `validate_aasa(json)` — the agent already has the JSON, or it would not be able to pass it.
- `match_url(json, app_id, url)` — same problem.

Both tools require the agent to already hold the file. That is the hard part, and neither tool
helps with it. A wrapper that only marshals arguments an agent already has is a tool nobody reaches
for twice.

### What would actually be worth building

The tool an agent would use constantly looks like this:

```
check_universal_link(domain, app_id, url)
  -> fetch https://<domain>/.well-known/apple-app-site-association
  -> fetch what Apple's CDN currently serves for <domain>
  -> semantic_diff between them          <- this crate
  -> match the URL, explain the decision <- this crate
  -> report HTTP status, redirects, content-type, Apple-Failure-Reason
```

"Why doesn't this link open my app?" is a real, recurring, tedious question, and that tool answers
it in one call. But notice where the value sits: **fetching plus semantics.** The semantics alone
are half a tool.

That is exactly the boundary in `aasadiff-integration.md`. Fetching means an HTTP client, opinions
about redirects and timeouts, and a model of Apple CDN behaviour that changes without notice — none
of which belong in a crate whose selling point is that it has two dependencies and compiles to
WebAssembly.

### If it gets built

A separate crate, `blazingly-aasa-mcp`, depending on:

- `mcport` for the JSON-RPC and MCP plumbing,
- `blazingly-aasa` for the semantics,
- an HTTP client of its choosing.

It composes cleanly: `mcport` already reads its JSON-RPC envelopes with `blazingly-json`, and this
crate reads association files with the same parser. One JSON stack from the transport down to the
document, which is the point of the `blazingly-json` consumer contract in the first place.

Two things it must not do, which follow directly from what this crate refuses to claim:

- **Never report that a link "will open the app."** It can report that the served file considers the
  URL eligible. Install state, entitlements, and CDN freshness are outside what any of this can see.
- **Never let fetch logic leak downward.** If the MCP server wants a cache or a retry policy, that
  lives in the MCP server.

### What was built

`blazingly-aasa-mcp` — a separate repository, a separate release cadence, and a dependency arrow
that points one way. It exposes five tools, three of which reach the network and two of which do
not, with `compare_origin_and_cdn` as the one that finds the hard bug.

Two things it does that this crate could not have:

- It never follows a redirect, because Apple requires the file to be served without one, so
  following it would hide the misconfiguration the tool exists to find. That is a *transport*
  opinion, and it belongs where the transport is.
- The caller names a domain, never a URL, and IP literals, `localhost`, `.local`, and unqualified
  names are refused before a socket opens. The string check alone turned out to be insufficient —
  a public name can still resolve inward — so the resolver itself now rejects every address
  outside public unicast space, and the agent takes no proxy. A semantics crate has no business
  having a view on that, and a networked tool has no business not having one.

The boundary held. `blazingly-aasa` gained nothing network-shaped: the MCP crate depends on the
published crate by version, and the arrow has stayed one-directional. Both statements in "Two things it
must not do" above are still true of the built thing — it reports what the served file permits, and
its fetch logic did not leak downward.

---

---

## 3. Behavioural diff with a witness URL

`semantic_diff` compares normalised effective policy. It is sound — equivalent means identical
decisions — but not complete: reordering two rules that can never both match is reported, because
proving they never overlap is a different algorithm than comparing lists.

The stronger question is worth answering, and the pieces are already here: compiled patterns, the
wildcard language, substitutions, the bitset NFA, rule order, and an evaluator.

> Is there a URL that these two documents decide differently, and what is it?

A three-valued answer is far more useful than a fourth kind of diff entry:

```
ProvenEquivalent
ProvenDifferent { witness: "https://example.com/help/website/test", left: Exclude, right: Match }
Unknown          { potential: [...] }
```

Because this is what a person actually needs to see:

```
ORIGIN != APPLE CDN

This difference changes behaviour.
  witness:    https://example.com/help/website/test
  origin:     EXCLUDE
  apple cdn:  MATCH
```

`RULE_MOVED` makes a reader work out whether it matters. A witness URL does not. It would also
retire the completeness caveat above for the cases it can decide, and say `Unknown` honestly for
the rest.

Not started. It is the most interesting thing left in the core, and unlike another validation rule
it is something no other implementation offers.

## Things deliberately not on the roadmap

`no_std`, streaming parse, C bindings, and a CLI were all in the original plan as "0.3+, only if
consumers require them". No consumer requires them. They stay off the list until one does.

The next thing that would genuinely improve this crate is not a feature. It is running
`scripts/oracle_swcutil.sh` and moving rows in `docs/parity.md` from **decided** to **oracle**.
