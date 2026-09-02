// Runs conformance/cases.json through WebAssembly.
//
//   node bindings/wasm/tests/conformance.mjs
//   bun  bindings/wasm/tests/conformance.mjs
//
// The Rust suite runs the identical file. If these two ever disagree, the binding is wrong -- and
// that is exactly what this catches, since passing Rust tests say nothing about the boundary.
//
// The corpus is deliberately implementation-neutral: point a different AASA library at it by
// replacing `decide` below.

import { readFileSync } from "node:fs";
import { fileURLToPath } from "node:url";
import { dirname, join } from "node:path";

const here = dirname(fileURLToPath(import.meta.url));
const root = join(here, "..", "..", "..");
const { Aasa } = await import(join(here, "..", "pkg-node", "blazingly_aasa.js"));

const corpus = JSON.parse(readFileSync(join(root, "conformance", "cases.json"), "utf8"));
if (corpus.version !== 2) throw new Error(`unexpected corpus version ${corpus.version}`);

const runtime = typeof Bun === "undefined" ? `node ${process.version}` : `bun ${Bun.version}`;
console.log(`conformance: ${corpus.matching.length} matching + ${corpus.validation.length} validation cases (${runtime})`);

const encode = (value) => new TextEncoder().encode(JSON.stringify(value));
const failures = [];
const byFeature = new Map();

for (const c of corpus.matching) {
  let actual;
  const aasa = Aasa.compile(encode(c.aasa), c.domain);
  try {
    actual = aasa.decide(c.appId, c.url);
    // The tracing path must agree with the fast one.
    const traced = aasa.match(c.appId, c.url).decision;
    if (traced !== actual) {
      failures.push(`${c.name}: decide said ${actual}, match said ${traced}`);
    }
  } catch (error) {
    failures.push(`${c.name}: threw ${error.message}`);
    continue;
  } finally {
    aasa.free();
  }

  const stats = byFeature.get(c.feature) ?? { pass: 0, fail: 0 };
  if (actual === c.expect) stats.pass += 1;
  else {
    stats.fail += 1;
    failures.push(`${c.name} [${c.feature}]: expected ${c.expect}, got ${actual}`);
  }
  byFeature.set(c.feature, stats);
}

for (const c of corpus.validation) {
  const aasa = Aasa.compile(encode(c.aasa), "example.com");
  try {
    const codes = aasa.validate().map((d) => d.code);
    for (const expected of c.expectCodes) {
      if (!codes.includes(expected)) {
        failures.push(`${c.name}: expected ${expected}, got [${codes}]`);
      }
    }
    if (c.expectCodes.length === 0 && codes.length > 0) {
      failures.push(`${c.name}: expected a silent report, got [${codes}]`);
    }
  } finally {
    aasa.free();
  }
}

for (const [feature, { pass, fail }] of [...byFeature].sort()) {
  console.log(`  ${fail ? "FAIL" : "ok  "} ${feature.padEnd(20)} ${pass}/${pass + fail}`);
}

if (failures.length) {
  console.error(`\n${failures.length} failed:\n  ${failures.join("\n  ")}`);
  process.exit(1);
}
console.log(`\nall ${corpus.matching.length + corpus.validation.length} conformance cases passed`);
