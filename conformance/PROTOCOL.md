# Running the corpus against your implementation

The corpus is `cases.json` — 140 matching cases, 139 of them checked against Apple's `swcutil`.
It is deliberately implementation-neutral. Nothing in it assumes Rust, JavaScript, or this project.

To score your own AASA matcher, implement a nine-line filter and run one command:

```bash
node conformance/run.mjs --exec "./your-matcher"
```

## The contract

Your program reads **one JSON object per line** on stdin and writes **one JSON object per line**
to stdout. That is the whole interface: no arguments, no files, no ordering requirement.

**In**, one line per case:

```json
{"id":0,"aasa":{"applinks":{...}},"domain":"example.com","appId":"ABCDE12345.com.example.app","url":"https://example.com/buy/42"}
```

- `aasa` — the complete association file, as a JSON value.
- `domain` — the domain the file was served for. The URL's host always matches it, except in the
  three cases tagged `feature: "host"`.
- `appId` — the application identifier under test.
- `url` — the URL to decide.

**Out**, one line per case, in any order:

```json
{"id":0,"decision":"match"}
```

- `decision` — exactly one of `match`, `exclude`, `no_match`.
- Anything else, or a missing id, counts as a failure for that case rather than aborting the run.

Write to stdout only. Diagnostics go to stderr; the runner passes them through.

## A complete implementation, for reference

```js
#!/usr/bin/env node
import { createInterface } from "node:readline";
import { Aasa } from "blazingly-aasa";

for await (const line of createInterface({ input: process.stdin })) {
  if (!line.trim()) continue;
  const c = JSON.parse(line);
  const aasa = Aasa.compile(new TextEncoder().encode(JSON.stringify(c.aasa)), c.domain);
  try {
    console.log(JSON.stringify({ id: c.id, decision: aasa.decide(c.appId, c.url) }));
  } finally {
    aasa.free();
  }
}
```

`conformance/adapters/` holds this and a shell one-liner driving the CLI, both of which the test
suite runs so the protocol cannot rot.

## Reading the score

```
feature              score   of which trivial
ok   rule-order      11/11   4 expect no_match
FAIL substitutions   10/20   10 expect no_match
```

The **trivial** column is the one to read first. Ten of the twenty substitution cases expect
`no_match`, so an implementation that silently matches nothing passes all ten by accident. A score
that hides this flatters the loser; `10/20` there means *zero* right, not half.

## Fair play

- **Skip what you do not claim.** A library taking a path relative to a fixed origin cannot answer
  the three `host` cases. `--skip host` excludes them and the report says so.
- **A thrown error fails that case, not the run.** An implementation that crashes on one input
  should still be scored on the other 139.
- **Cases tagged `decided`, not `oracle`, are this project's reading.** There is one. Disagreeing
  with it is a legitimate position, not a failure; `docs/parity.md` explains it.

If a case is wrong, that is a bug in the corpus and worth an issue. It is checked against Apple's
own tool, not against this crate's opinion, and `conformance/oracle/` has the raw runs.
