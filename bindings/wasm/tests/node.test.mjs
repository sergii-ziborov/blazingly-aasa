// Runs the same expectations as the Rust suite, but through the WebAssembly boundary, so a
// binding bug cannot hide behind passing Rust tests.
//
//   node bindings/wasm/tests/node.test.mjs
//   bun  bindings/wasm/tests/node.test.mjs
//
// Build first with bindings/wasm/build.sh.

import { readFileSync } from "node:fs";
import { fileURLToPath } from "node:url";
import { dirname, join } from "node:path";
import assert from "node:assert/strict";

const here = dirname(fileURLToPath(import.meta.url));
const root = join(here, "..", "..", "..");
const wasm = await import(join(here, "..", "pkg-node", "blazingly_aasa.js"));
const { Aasa, diffAasa, matchPattern, validateAasa, isoTableSource } = wasm;

const fixture = (name) => readFileSync(join(root, "tests", "fixtures", name));

const APP = "ABCDE12345.com.example.app";
const DOMAIN = "example.com";

let passed = 0;
const test = (name, body) => {
  try {
    body();
    passed += 1;
    console.log(`  ok   ${name}`);
  } catch (error) {
    console.error(`  FAIL ${name}\n       ${error.message}`);
    process.exitCode = 1;
  }
};

console.log(`blazingly-aasa wasm tests (${typeof Bun === "undefined" ? "node " + process.version : "bun " + Bun.version})`);

const overview = Aasa.compile(fixture("apple/applinks-overview.json"), DOMAIN);

test("decides the documented Apple example", () => {
  assert.equal(overview.decide(APP, "https://example.com/buy/42"), "match");
  assert.equal(overview.decide(APP, "https://example.com/buy/42#no_universal_links"), "exclude");
  assert.equal(overview.decide(APP, "https://example.com/help/1?articleNumber=4815"), "match");
  assert.equal(overview.decide(APP, "https://example.com/help/1?articleNumber=481"), "no_match");
  assert.equal(overview.decide(APP, "https://example.com/elsewhere"), "no_match");
});

test("rejects a URL on another host", () => {
  assert.equal(overview.decide(APP, "https://evil.test/buy/42"), "no_match");
  assert.equal(overview.domain, DOMAIN);
});

test("returns a structured trace", () => {
  const result = overview.match(APP, "https://example.com/help/1?articleNumber=4815");
  assert.equal(result.decision, "match");
  assert.equal(result.trace.selected_detail, 0);
  assert.equal(result.trace.selected_rule, 3);
  assert.equal(result.trace.stop_reason.stop, "matched");
});

test("matchJson agrees with match", () => {
  const url = "https://example.com/help/website/faq";
  const viaObject = overview.match(APP, url);
  const viaJson = JSON.parse(overview.matchJson(APP, url));
  assert.equal(viaObject.decision, "exclude");
  assert.equal(viaJson.decision, viaObject.decision);
});

test("explains a miss in words", () => {
  const text = overview.explain(APP, "https://example.com/help/1?articleNumber=481");
  assert.match(text, /NO_MATCH/);
  assert.match(text, /articleNumber/);
});

test("reports service membership", () => {
  const all = Aasa.compile(fixture("apple/all-services.json"), DOMAIN);
  assert.deepEqual(all.servicesForApp(APP), ["applinks", "webcredentials", "activitycontinuation"]);
  assert.deepEqual(all.servicesForApp("ZZZZZ99999.com.other.app"), []);
  all.free();
});

test("surfaces diagnostics with stable codes", () => {
  const diagnostics = validateAasa(Buffer.from('{"applinks":{"details":[{"components":[{"/":"buy/*"}]}]}}'));
  const codes = diagnostics.map((d) => d.code);
  assert.ok(codes.includes("AASA110"), `expected AASA110 in ${codes}`);
  assert.ok(codes.includes("AASA191"), `expected AASA191 in ${codes}`);
  assert.equal(diagnostics.find((d) => d.code === "AASA110").severity, "error");
});

test("throws on an unusable document", () => {
  assert.throws(() => Aasa.compile(Buffer.from("{ nope"), DOMAIN));
  assert.throws(() => Aasa.compile(Buffer.from("[]"), DOMAIN));
});

test("throws on an unusable URL", () => {
  assert.throws(() => overview.decide(APP, "not a url"));
});

test("honours a custom size limit", () => {
  const bytes = fixture("apple/applinks-overview.json");
  assert.throws(() => Aasa.compile(bytes, DOMAIN, 16));
  const ok = Aasa.compile(bytes, DOMAIN, bytes.length);
  ok.free();
});

test("diffs two documents semantically", () => {
  const inline = Buffer.from(
    '{"applinks":{"details":[{"appIDs":["A.b"],"components":[{"/":"/a/*","caseSensitive":false}]}]}}',
  );
  const hoisted = Buffer.from(
    '{"applinks":{"details":[{"appIDs":["A.b"],"defaults":{"caseSensitive":false},"components":[{"/":"/a/*"}]}]}}',
  );
  assert.deepEqual(diffAasa(inline, hoisted), []);

  const changed = Buffer.from(
    '{"applinks":{"details":[{"appIDs":["A.b"],"components":[{"/":"/a/*"}]}]}}',
  );
  const changes = diffAasa(inline, changed);
  assert.equal(changes.length, 1);
  assert.equal(changes[0].change, "rule_changed");
});

test("compares handles directly", () => {
  const left = Aasa.compile(fixture("apple/applinks-overview.json"), DOMAIN);
  const right = Aasa.compile(fixture("apple/applinks-overview.json"), DOMAIN);
  assert.equal(left.semanticEqual(right), true);
  assert.deepEqual(left.semanticDiff(right), []);
  left.free();
  right.free();
});

test("matches a standalone pattern", () => {
  assert.equal(matchPattern("/help/*", "/help/website", true), true);
  assert.equal(matchPattern("/Help/*", "/help/website", true), false);
  assert.equal(matchPattern("/Help/*", "/help/website", false), true);
  assert.equal(matchPattern("/id/$(digit)$(digit)", "/id/42", true), true);
  assert.throws(() => matchPattern("/$(nope)", "/x", true));
});

test("reports where the ISO tables came from", () => {
  assert.match(isoTableSource(), /Foundation/);
});

test("normalized output resolves defaults", () => {
  const aasa = Aasa.compile(
    Buffer.from('{"applinks":{"defaults":{"caseSensitive":false},"details":[{"appIDs":["A.b"],"components":[{"/":"/a/*"}]}]}}'),
    DOMAIN,
  );
  assert.match(aasa.normalizedJson(), /"case_sensitive": false/);
  aasa.free();
});

overview.free();

console.log(`\n${passed} passed${process.exitCode ? ", some FAILED" : ""}`);
