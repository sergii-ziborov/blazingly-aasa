#!/usr/bin/env node
// Reference adapter: the corpus protocol, driven by this project's own WebAssembly package.
//
// Nine lines of actual work. Any implementation in any language can do the same; see PROTOCOL.md.

import { createInterface } from "node:readline";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";

const here = dirname(fileURLToPath(import.meta.url));
const { Aasa } = await import(join(here, "..", "..", "bindings", "wasm", "pkg-node", "blazingly_aasa.js"));

for await (const line of createInterface({ input: process.stdin })) {
  if (!line.trim()) continue;
  const c = JSON.parse(line);
  let decision;
  const aasa = Aasa.compile(new TextEncoder().encode(JSON.stringify(c.aasa)), c.domain);
  try {
    decision = aasa.decide(c.appId, c.url);
  } finally {
    aasa.free();
  }
  process.stdout.write(JSON.stringify({ id: c.id, decision }) + "\n");
}
