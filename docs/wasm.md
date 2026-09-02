# WebAssembly

`bindings/wasm` compiles the engine to WebAssembly and publishes it as `@sergii-ziborov/aasa`. The core
crate knows nothing about JavaScript; the binding is a separate crate.

## Building

```bash
./bindings/wasm/build.sh
```

Produces three packages from one Rust module, all published under the same name:

| Directory | wasm-pack target | For |
| --- | --- | --- |
| `pkg` | `bundler` | Vite, webpack, Rollup |
| `pkg-node` | `nodejs` | Node ESM, Bun |
| `pkg-web` | `web` | `<script type="module">`, no bundler |

Requires [wasm-pack](https://rustwasm.github.io/wasm-pack/installer/).

`wasm-pack` generates each `pkg/package.json` from the **Rust crate name**, which would publish
this as `blazingly-aasa-wasm`. `build.sh` merges the intended npm metadata — the scoped name,
keywords, homepage — from `bindings/wasm/package.json`, which is the source of truth for anything
npm-facing. Without that step `npm install @sergii-ziborov/aasa` would 404.

## Using it

```js
import { Aasa } from "@sergii-ziborov/aasa";

const response = await fetch("https://example.com/.well-known/apple-app-site-association");
const aasa = Aasa.compile(new Uint8Array(await response.arrayBuffer()), "example.com");

try {
  const diagnostics = aasa.validate();
  for (const d of diagnostics) console.log(`${d.severity} ${d.code} ${d.path}: ${d.message}`);

  console.log(aasa.decide(appId, url));   // "match" | "exclude" | "no_match"
  console.log(aasa.explain(appId, url));  // the same decision, in words
} finally {
  aasa.free();
}
```

`Aasa` owns memory inside the WebAssembly instance. Call `free()` when you are done — a `try/finally`
is the reliable shape.

In a browser with the `pkg-web` build, call the default export first:

```js
import init, { Aasa } from "@sergii-ziborov/aasa";
await init();
```

## The design decision that matters

The compiled document stays inside WebAssembly. JavaScript holds a handle and crosses the boundary
only with small arguments and small results. Handing a parsed association file back as a JavaScript
object tree would cost more than the parse.

That decision has a limit worth stating plainly, because it is the thing most WebAssembly libraries
gloss over: **for matching, this module is not faster than a competent pure-JavaScript
implementation.** Matching itself takes nanoseconds; moving a URL string across the boundary takes
hundreds of them, and that cost is per string — it does not amortise over a batch. Measured against
a `JSON.parse` + `RegExp` implementation on the same corpus, per-URL matching is roughly at parity.

Where WebAssembly does win is compilation: parsing and compiling a document is about twice as fast,
and the gap grows with file size.

The real reason to use this module is not speed. It is that the semantics are the same ones the
Rust crate implements — the same diagnostics, the same traces, the same diff — rather than a second
implementation that will drift.

## API

### `Aasa.compile(bytes, domain, maxBytes?)`

Parses and compiles. `domain` is the host the file was served for; pass `""` to skip the host check.
`maxBytes` overrides the 128 KiB default. Throws for unusable input.

### Deciding

| Method | Returns | Notes |
| --- | --- | --- |
| `decide(appId, url)` | `"match" \| "exclude" \| "no_match"` | cheapest single call; throws on a bad URL |
| `decideMany(appId, urls)` | `string[]` | one call, still one string encode per URL |
| `decideManyCodes(appId, urls)` | `Uint8Array` | `0` no match, `1` match, `2` exclude, `3` bad URL |
| `decideLines(appId, joined)` | `Uint8Array` | URLs separated by `\n`; one encode for the whole batch |

`decideLines` is the fastest shape, because the per-string encode is what costs. It never throws on
a bad URL — that line comes back as `3`, so one bad entry does not discard the batch.

### Explaining

| Method | Returns |
| --- | --- |
| `match(appId, url)` | the decision and the full trace, as a JavaScript object |
| `matchJson(appId, url)` | the same, as a JSON string for `JSON.parse` |
| `explain(appId, url)` | a human-readable trace |

`match` and `matchJson` are roughly an order of magnitude more expensive than `decide`, because the
trace records every component comparison. Use `decide` in a loop and `match` when a person needs to
read the answer. Which of `match` and `matchJson` is faster depends on the engine and the trace
size — measure rather than assume.

### Validating and comparing

| Function | Returns |
| --- | --- |
| `aasa.validate()` | array of `{ code, severity, path, message, help? }` |
| `aasa.hasErrors()` | boolean |
| `aasa.servicesForApp(appId)` | `["applinks", "webcredentials", …]` |
| `aasa.applinkApps()` | every app identifier under `applinks.details` |
| `aasa.normalizedJson()` | canonical rendering with defaults resolved |
| `aasa.semanticDiff(other)` | behavioural changes between two handles |
| `aasa.semanticEqual(other)` | boolean |

Handle-free convenience functions — `validateAasa`, `matchAasa`, `diffAasa`, `matchPattern`,
`isoTableSource` — reparse on every call. Fine for one-shot use, wasteful in a loop.

## Payload size

Roughly 358 KB raw, 144 KB gzipped, 115 KB brotli. Most of it is the JSON engine and
`serde-wasm-bindgen`; the ISO region and language tables are about 15 KB.

`panic = "abort"` was measured and made the module *larger*, so it is not used — `wasm-opt` already
removes the unwinding paths.

## Testing

```bash
node bindings/wasm/tests/node.test.mjs
bun  bindings/wasm/tests/node.test.mjs
```

The suite runs the same Apple expectations as the Rust tests, through the boundary, so a binding
bug cannot hide behind passing Rust tests. CI runs both runtimes.

```bash
node bindings/wasm/bench/bench.mjs
```

compares the module against a pure-JavaScript implementation, including a batch-size sweep.
