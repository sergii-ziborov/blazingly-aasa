// WebAssembly against the pure-JavaScript implementation a web developer would actually write:
// JSON.parse plus a RegExp per pattern.
//
//   node bindings/wasm/bench/bench.mjs
//   bun  bindings/wasm/bench/bench.mjs

import { fileURLToPath } from "node:url";
import { dirname, join } from "node:path";

const here = dirname(fileURLToPath(import.meta.url));
const { Aasa } = await import(join(here, "..", "pkg-node", "blazingly_aasa.js"));

// ---------------------------------------------------------------- pure JS baseline

const toRegExp = (pattern, caseSensitive) => {
  let source = "^";
  for (const character of pattern) {
    if (character === "*") source += "[\\s\\S]*";
    else if (character === "?") source += "[\\s\\S]";
    else source += character.replace(/[.*+?^${}()|[\]\\]/g, "\\$&");
  }
  return new RegExp(source + "$", caseSensitive ? "" : "i");
};

class JsAasa {
  constructor(bytes) {
    const root = JSON.parse(new TextDecoder().decode(bytes));
    this.details = (root.applinks?.details ?? []).map((detail) => ({
      appIds: [detail.appID, ...(detail.appIDs ?? [])].filter(Boolean),
      rules: (detail.components ?? []).map((component) => {
        const caseSensitive = component.caseSensitive ?? true;
        const query = component["?"];
        return {
          path: component["/"] === undefined ? null : toRegExp(component["/"], caseSensitive),
          queryWhole: typeof query === "string" ? toRegExp(query, caseSensitive) : null,
          queryItems:
            query && typeof query === "object"
              ? Object.entries(query).map(([k, v]) => [k, toRegExp(v, caseSensitive)])
              : [],
          fragment: component["#"] === undefined ? null : toRegExp(component["#"], caseSensitive),
          exclude: component.exclude ?? false,
        };
      }),
    }));
  }

  decide(appId, url) {
    const parsed = new URL(url);
    const path = parsed.pathname;
    const query = parsed.search.slice(1);
    const fragment = parsed.hash.slice(1);
    const items = [...parsed.searchParams.entries()];

    for (const detail of this.details) {
      if (!detail.appIds.includes(appId)) continue;
      for (const rule of detail.rules) {
        if (rule.path && !rule.path.test(path)) continue;
        if (rule.queryWhole && !rule.queryWhole.test(query)) continue;
        let ok = true;
        for (const [name, pattern] of rule.queryItems) {
          if (!items.some(([k, v]) => k === name && pattern.test(v))) { ok = false; break; }
        }
        if (!ok) continue;
        if (rule.fragment && !rule.fragment.test(fragment)) continue;
        return rule.exclude ? "exclude" : "match";
      }
    }
    return "no_match";
  }
}

// ---------------------------------------------------------------- corpus

const corpus = (details, rules) => {
  const entries = [];
  for (let d = 0; d < details; d += 1) {
    const components = [];
    for (let r = 0; r < rules; r += 1) {
      if (r % 5 === 0) components.push({ "/": `/section${r}/private/*`, exclude: true });
      else if (r % 5 === 1) components.push({ "/": `/section${r}/*` });
      else if (r % 5 === 2) components.push({ "/": `/help${r}/*`, "?": { articleNumber: "????" } });
      else if (r % 5 === 3) components.push({ "/": `/catalog${r}/*`, caseSensitive: false });
      else components.push({ "/": `/item${r}/?*`, "?": "ref=*" });
    }
    entries.push({ appIDs: [`ABCDE12345.com.example.app${d}`], components });
  }
  return new TextEncoder().encode(JSON.stringify({ applinks: { details: entries } }));
};

const urls = [
  "https://example.com/section1/product/42",
  "https://example.com/section0/private/secret",
  "https://example.com/help2/topic?articleNumber=4815",
  "https://example.com/help2/topic?articleNumber=481",
  "https://example.com/catalog3/pizza/margherita",
  "https://example.com/item4/x?ref=email",
  "https://example.com/nothing/here",
  "https://example.com/",
];

// ---------------------------------------------------------------- harness

const measure = (name, iterations, body) => {
  for (let i = 0; i < Math.max(1, iterations / 10); i += 1) body(); // warm up
  const started = process.hrtime.bigint();
  for (let i = 0; i < iterations; i += 1) body();
  const nanos = Number(process.hrtime.bigint() - started) / iterations;
  const label = nanos > 1e6 ? `${(nanos / 1e6).toFixed(3)} ms` : nanos > 1e3 ? `${(nanos / 1e3).toFixed(2)} us` : `${nanos.toFixed(1)} ns`;
  console.log(`  ${name.padEnd(34)} ${label.padStart(12)}`);
  return nanos;
};

const runtime = typeof Bun === "undefined" ? `node ${process.version}` : `bun ${Bun.version}`;
console.log(`blazingly-aasa wasm bench (${runtime})\n`);

for (const [details, rules] of [[1, 8], [8, 16], [32, 32]]) {
  const bytes = corpus(details, rules);
  console.log(`corpus ${details} details x ${rules} rules (${(bytes.length / 1024).toFixed(1)} KiB)`);

  const iterations = details * rules > 256 ? 200 : 2000;
  const wasmCompile = measure("compile  wasm", iterations, () => Aasa.compile(bytes, "example.com").free());
  const jsCompile = measure("compile  pure JS", iterations, () => new JsAasa(bytes));
  console.log(`  ${"->".padEnd(34)} ${(jsCompile / wasmCompile).toFixed(2)}x`.padStart(12));

  const app = `ABCDE12345.com.example.app${details - 1}`;
  const wasmAasa = Aasa.compile(bytes, "example.com");
  const jsAasa = new JsAasa(bytes);

  const batch = details * rules > 256 ? 2000 : 20000;
  const wasmMatch = measure("match    wasm (8 urls)", batch, () => {
    for (const url of urls) wasmAasa.decide(app, url);
  });
  const wasmBatch = measure("match    wasm batched (8 urls)", batch, () => wasmAasa.decideMany(app, urls));
  const wasmCodes = measure("match    wasm codes (8 urls)", batch, () => wasmAasa.decideManyCodes(app, urls));
  const jsMatch = measure("match    pure JS (8 urls)", batch, () => {
    for (const url of urls) jsAasa.decide(app, url);
  });
  console.log(`  ${"-> per-call".padEnd(34)} ${(jsMatch / wasmMatch).toFixed(2)}x`.padStart(12));
  console.log(`  ${"-> batched".padEnd(34)} ${(jsMatch / wasmBatch).toFixed(2)}x`.padStart(12));
  console.log(`  ${"-> codes".padEnd(34)} ${(jsMatch / wasmCodes).toFixed(2)}x`.padStart(12));

  wasmAasa.free();
  console.log("");
}

// How large does a batch have to be before crossing the boundary once beats staying in JS?
{
  const bytes = corpus(8, 16);
  const wasmAasa = Aasa.compile(bytes, "example.com");
  const jsAasa = new JsAasa(bytes);
  const app = "ABCDE12345.com.example.app7";
  console.log("batch size sweep (8 details x 16 rules)");
  console.log(`  ${"urls".padEnd(8)} ${"wasm array".padStart(11)} ${"wasm lines".padStart(11)} ${"pure JS".padStart(11)}   speedup`);
  for (const size of [1, 8, 64, 512, 4096]) {
    const many = Array.from({ length: size }, (_, i) => urls[i % urls.length]);
    const iterations = Math.max(20, Math.round(200000 / size));
    const warm = (body) => {
      for (let i = 0; i < Math.max(1, iterations / 10); i += 1) body();
      const t0 = process.hrtime.bigint();
      for (let i = 0; i < iterations; i += 1) body();
      return Number(process.hrtime.bigint() - t0) / iterations / size;
    };
    const joined = many.join("\n");
    const w = warm(() => wasmAasa.decideManyCodes(app, many));
    const l = warm(() => wasmAasa.decideLines(app, joined));
    const j = warm(() => { for (const url of many) jsAasa.decide(app, url); });
    const fmt = (n) => (n > 1e3 ? `${(n / 1e3).toFixed(2)} us` : `${n.toFixed(0)} ns`);
    console.log(`  ${String(size).padEnd(8)} ${fmt(w).padStart(11)} ${fmt(l).padStart(11)} ${fmt(j).padStart(11)}   ${(j / l).toFixed(2) + "x"}`);
  }
  wasmAasa.free();
}
