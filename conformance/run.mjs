// Scores any AASA implementation against the corpus.
//
//   node conformance/run.mjs --exec "./your-matcher"
//   node conformance/run.mjs --exec "node conformance/adapters/wasm.mjs"
//   node conformance/run.mjs --exec "..." --skip host --json
//
// The implementation reads one JSON case per line on stdin and writes one decision per line to
// stdout. See PROTOCOL.md. Nothing here is specific to this project: the corpus is the artifact,
// and it is checked against Apple's swcutil rather than against this crate's opinion.

import { spawn } from "node:child_process";
import { readFileSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";
import { createInterface } from "node:readline";

const here = dirname(fileURLToPath(import.meta.url));

function parseArgs(argv) {
  const options = { exec: null, corpus: join(here, "cases.json"), skip: [], json: false };
  for (let i = 0; i < argv.length; i += 1) {
    switch (argv[i]) {
      case "--exec": options.exec = argv[++i]; break;
      case "--corpus": options.corpus = argv[++i]; break;
      case "--skip": options.skip.push(argv[++i]); break;
      case "--json": options.json = true; break;
      default: throw new Error(`unknown option ${argv[i]}`);
    }
  }
  if (!options.exec) {
    throw new Error('usage: node conformance/run.mjs --exec "<command>" [--skip <feature>] [--json]');
  }
  return options;
}

const options = parseArgs(process.argv.slice(2));
const corpus = JSON.parse(readFileSync(options.corpus, "utf8"));

const cases = corpus.matching
  .map((c, index) => ({ ...c, id: index }))
  .filter((c) => !options.skip.includes(c.feature));
const skipped = corpus.matching.length - cases.length;

// One process, one line per case: an implementation should not pay process startup 140 times.
const child = spawn(options.exec, { shell: true, stdio: ["pipe", "pipe", "inherit"] });

const decisions = new Map();
const reader = createInterface({ input: child.stdout });
reader.on("line", (line) => {
  if (!line.trim()) return;
  try {
    const { id, decision } = JSON.parse(line);
    decisions.set(id, decision);
  } catch {
    // A malformed line loses that case, not the run.
  }
});

for (const c of cases) {
  child.stdin.write(JSON.stringify({
    id: c.id, aasa: c.aasa, domain: c.domain, appId: c.appId, url: c.url,
  }) + "\n");
}
child.stdin.end();

await new Promise((resolve) => {
  let done = 0;
  const finish = () => { if (++done === 2) resolve(); };
  reader.on("close", finish);
  child.on("close", finish);
});

const stats = new Map();
const divergences = [];
for (const c of cases) {
  const actual = decisions.get(c.id) ?? "<no answer>";
  const entry = stats.get(c.feature) ?? { pass: 0, fail: 0, trivial: 0 };
  if (actual === c.expect) {
    entry.pass += 1;
    // Passing an "expect: no_match" case proves nothing on its own: an implementation that
    // silently matches nothing passes every one of them.
    if (c.expect === "no_match") entry.trivial += 1;
  } else {
    entry.fail += 1;
    if (divergences.length < 40) {
      divergences.push({ name: c.name, feature: c.feature, expected: c.expect, actual, url: c.url });
    }
  }
  stats.set(c.feature, entry);
}

let pass = 0;
let fail = 0;
for (const entry of stats.values()) { pass += entry.pass; fail += entry.fail; }

if (options.json) {
  console.log(JSON.stringify({
    corpus: { version: corpus.version, cases: corpus.matching.length, skipped },
    total: { pass, fail },
    features: Object.fromEntries(stats),
    divergences,
  }, null, 2));
} else {
  console.log(`${options.exec}\n`);
  console.log(`  ${"feature".padEnd(20)} ${"score".padStart(7)}   of which trivial`);
  for (const [feature, entry] of [...stats].sort()) {
    const mark = entry.fail === 0 ? "ok  " : "FAIL";
    const trivial = entry.trivial ? `${entry.trivial} expect no_match` : "";
    console.log(`  ${mark} ${feature.padEnd(20)} ${`${entry.pass}/${entry.pass + entry.fail}`.padStart(7)}   ${trivial}`);
  }
  console.log(`\n  total: ${pass}/${pass + fail}${skipped ? `  (${skipped} skipped)` : ""}`);
  if (divergences.length) {
    console.log("\ndivergences:");
    for (const d of divergences) {
      console.log(`  ${d.name} [${d.feature}]\n    expected ${d.expected}, got ${d.actual}\n    ${d.url}`);
    }
  }
}
process.exit(fail === 0 ? 0 : 1);
