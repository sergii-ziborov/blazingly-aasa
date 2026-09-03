// Runs the conformance corpus against a third-party AASA implementation exposing
// `verify(json, url)`.
//
// For anything else -- another language, a command line, a different signature -- use the general
// runner instead, which speaks a line protocol and does not care what is on the other end:
//
//   node conformance/run.mjs --exec "<command>"
//
// See PROTOCOL.md. This file remains because that one shape is common enough to be worth a
// ready-made adapter.
//
//   npm pack universal-links-test && tar xzf universal-links-test-*.tgz
//   node conformance/run-third-party.mjs ./package/dist/sim/index.js
//
// Adapt `decide` for a library with a different shape. The point is that the corpus is
// implementation-neutral: any library claiming to evaluate AASA can be held to it.
//
// Fair-play rules this runner follows, and any comparison should:
//  - skip cases a library does not claim to cover (host checking, when it takes a relative path);
//  - count a thrown error as a failure of that case, not of the whole run;
//  - report *why* a case passed, since a library that never matches anything passes every
//    "expect: no_match" case by accident.

import { readFileSync } from "node:fs";
import { resolve } from "node:path";

const [, , modulePath, corpusPath = "conformance/cases.json"] = process.argv;
if (!modulePath) {
  console.error("usage: node conformance/run-third-party.mjs <module> [corpus.json]");
  process.exit(2);
}

const { verify } = await import(resolve(modulePath));
const corpus = JSON.parse(readFileSync(corpusPath, "utf8"));

/** Adapt this for a library with a different API. */
const decide = async (aasa, domain, appId, url) => {
  const result = await verify(structuredClone(aasa), url);
  const value = result.get(appId);
  return value === "match" ? "match" : value === "block" ? "exclude" : "no_match";
};

const stats = new Map();
let skipped = 0;
const divergences = [];

for (const c of corpus.matching) {
  // A library taking a path relative to a fixed origin cannot answer host questions.
  if (c.feature === "host" || c.domain === "") { skipped += 1; continue; }

  let actual;
  try {
    actual = await decide(c.aasa, c.domain, c.appId, c.url);
  } catch (error) {
    actual = `threw: ${error.message}`;
  }

  const s = stats.get(c.feature) ?? { pass: 0, fail: 0, trivialPass: 0 };
  if (actual === c.expect) {
    s.pass += 1;
    // Passing an "expect: no_match" case proves nothing on its own -- an implementation that
    // silently matches nothing passes all of them.
    if (c.expect === "no_match") s.trivialPass += 1;
  } else {
    s.fail += 1;
    if (divergences.length < 25) {
      divergences.push(`${c.name} [${c.feature}]: expected ${c.expect}, got ${actual}`);
    }
  }
  stats.set(c.feature, s);
}

let pass = 0, fail = 0;
console.log(`${modulePath} vs ${corpus.matching.length} cases (${skipped} not applicable)\n`);
console.log(`  ${"feature".padEnd(20)} ${"score".padStart(7)}   of which trivial`);
for (const [feature, s] of [...stats].sort()) {
  pass += s.pass;
  fail += s.fail;
  const mark = s.fail === 0 ? "ok  " : "FAIL";
  const trivial = s.trivialPass ? `${s.trivialPass} expect no_match` : "";
  console.log(`  ${mark} ${feature.padEnd(20)} ${`${s.pass}/${s.pass + s.fail}`.padStart(7)}   ${trivial}`);
}
console.log(`\n  total: ${pass} passed, ${fail} failed`);
if (divergences.length) console.log(`\ndivergences:\n  ${divergences.join("\n  ")}`);
