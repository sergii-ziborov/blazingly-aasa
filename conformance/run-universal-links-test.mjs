// Points the blazingly-aasa conformance corpus at `universal-links-test`, to see how much of
// Apple's documented semantics it actually implements. Fair-play notes:
//  - it takes a path relative to https://www.example.com, so host cases are marked N/A;
//  - it has no concept of validation diagnostics, so only matching cases are run;
//  - absence from its result Map means "no match".
import { readFileSync } from "node:fs";
import { verify } from "../universal-links-test/package/dist/sim/index.js";

const corpus = JSON.parse(readFileSync(process.argv[2], "utf8"));
const byFeature = new Map();
let pass = 0, fail = 0, na = 0, err = 0;
const examples = [];

for (const c of corpus.matching) {
  if (c.feature === "host" || c.domain === "") { na += 1; continue; }
  let actual;
  try {
    const res = await verify(structuredClone(c.aasa), c.url);
    const v = res.get(c.appId);
    actual = v === "match" ? "match" : v === "block" ? "exclude" : "no_match";
  } catch (e) { actual = `threw:${e.message.slice(0, 40)}`; err += 1; }

  const s = byFeature.get(c.feature) ?? { pass: 0, fail: 0 };
  if (actual === c.expect) { s.pass += 1; pass += 1; }
  else {
    s.fail += 1; fail += 1;
    if (examples.length < 12) examples.push(`${c.name}: expected ${c.expect}, got ${actual}`);
  }
  byFeature.set(c.feature, s);
}

console.log(`universal-links-test vs the corpus: ${pass} passed, ${fail} failed, ${na} not applicable\n`);
for (const [f, { pass: p, fail: q }] of [...byFeature].sort()) {
  const mark = q === 0 ? "ok  " : "FAIL";
  console.log(`  ${mark} ${f.padEnd(20)} ${p}/${p + q}`);
}
console.log(`\nfirst divergences:\n  ${examples.join("\n  ")}`);
