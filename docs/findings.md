# What it found

Features are easy to list and hard to judge. These are things this project actually caught —
in real association files served by real companies, and in its own code.

Everything below is reproducible. The findings against this crate's own code are specific and
dated; the ones about live association files are described as classes of problem rather than as
named domains, because whose file is misconfigured on a given day is not this project's story to
tell.

---

## In its own code

The most useful findings were against this crate, not against anyone else's.

Before any of it ran against Apple's `swcutil`, the test suite was a **self-confirming
specification**: every test asserted what the implementation already did, and all of them passed.
The oracle run agreed on 68 of 73 corpus cases and disagreed on five. One was a harness artifact.
**The other four were this crate being wrong.**

### A lint that was itself the bug

Apple's `components` reference writes one path pattern as `abc`, with no leading slash, while every
other example writes `/buy/*`. A URL path always begins with `/`, so this crate read the bare `abc`
as unmatchable — confidently enough to ship `AASA191` warning about it, and to describe the lint in
its README as turning an ambiguity into something useful.

`swcutil` matches `abc` against `/abc`. The documentation example was correct and the lint was
turning it into a false positive. `AASA191` was removed and its number retired.

### Three more

- **A missing query item counts as present with an empty value.** `{"b": "*"}` matches a URL with
  no `b` at all. This crate failed the predicate instead.
- **Every occurrence of a repeated query name must match**, not any one of them. `{"id": "42"}`
  does not match `?id=7&id=42` in any position. This crate accepted the first hit — the most
  permissive of three plausible readings, and the wrong one.
- **A single non-string predicate discards the whole `?` dictionary.** `{"a": "1", "flag": true}`
  matches `?a=2`. This crate made such a predicate never match, on the principle of refusing rather
  than guessing. That was the wrong direction: Apple is *more* lenient here, so the cautious-looking
  choice produced false negatives.

The last one is the uncomfortable lesson. Refusing to guess felt like the safe default and was not.

### And then the corpus caught the fix

Implementing the trailing-slash rule the obvious way — also trying the path with a slash appended —
makes `/id/????` match `/id/481`, because `481/` is four characters. `swcutil` says it does not.
The conformance corpus failed before that shipped.

---

## In files served in production

These are the classes of problem the tool surfaces on real, live association files. No domain is
named: which sites are misconfigured today is not this project's story to tell, and it changes.
Point it at your own and see:

```bash
npx blazingly-aasa-mcp fetch your-domain.example
```

### A redirect where Apple forbids one

```
error: https://.../.well-known/apple-app-site-association: HTTP 301

status:       301
redirect:     https://www..../.well-known/apple-app-site-association
  ! the server replied 301 and tried to redirect; Apple requires the association file to be
    served with no redirects
```

Apple requires the file to be served over HTTPS with no redirects. A validator that follows
redirects reports such a domain as healthy — it found *a* file, just not at the address it asked
for. This one refuses to follow, which is both faithful to the requirement and the reason the
finding is visible at all. Apex-to-`www` redirects make this common.

### `Content-Type` almost nobody serves correctly

Apple documents `application/json`. In practice `application/octet-stream` is widespread, including
on files served by very large sites. Reported as a note rather than an error for exactly that
reason: a rule the ecosystem universally ignores is worth surfacing and not worth failing a build
over.

### A file served from the older path

`/apple-app-site-association` still works; only `/.well-known/apple-app-site-association` is
documented. Worth knowing, not worth alarm — and the tool falls back to the older location and says
it did.

### A catch-all that is *not* a bug

A file ending with

```json
{ "/": "*", "comment": "Matches all remaining routes" }
```

raises `AASA180` — the rule constrains nothing and matches every URL. It is also obviously
deliberate, and the author said so in the file.

That is worth including precisely because it is **not** a finding. Reporting it as one would be the
kind of overclaiming this project is trying to avoid — and examining a real case of it produced an
improvement instead: `AASA180` now quotes the author's own comment back, so a reader can dismiss it
at a glance rather than investigating.

## In other implementations

`docs/competitors.md` scores the strongest surveyed tool against the corpus: **88 of 137**
applicable cases. It is genuinely solid on the core — rule ordering, `exclude`, the defaults
hierarchy — which is the part most implementations get wrong first.

The row that matters is substitution variables: **10 of 20** — and all ten passes are cases
expecting `no_match`, which any implementation that silently matches nothing passes for free. Its
real score there is zero, because **no surveyed tool expands `$(...)` at all.** They declare
`substitutionVariables` in their type definitions and ignore it when matching.

That failure mode is the dangerous one. A file using `$(lang)` does not error. It quietly stops
matching, and the check stays green.
