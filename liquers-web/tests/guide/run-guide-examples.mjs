// Runs every JavaScript example in `specs/guides/JAVASCRIPT_GUIDE.md` (`DOCS03`).
//
// The guide says each example runs as written; this keeps that true. Usage:
//
//   node run-guide-examples.mjs <dir>     # <dir> holds liquers_web.js from `wasm-bindgen --target web`
//
// `scripts/test-guide-examples.sh` builds that directory and runs this. Each ```js block runs in a
// fresh instance of the module — its own wasm instance and so its own environment — so one
// example's registrations cannot make another pass. The block's `import … from "./liquers_web.js"`
// line is what a page writes; here it is replaced by the same bindings: `init` (loading the wasm
// from bytes, since Node cannot fetch a file URL) and the module namespace `liquers`. `assert` is
// Node's `node:assert`, which the guide says its examples use.
//
// A block preceded by `<!-- example: not run — <reason> -->` is skipped, and must give a reason.

import assert from "node:assert";
import { readFileSync } from "node:fs";
import { resolve, dirname } from "node:path";
import { fileURLToPath, pathToFileURL } from "node:url";

const here = dirname(fileURLToPath(import.meta.url));
const guidePath = resolve(here, "../../../specs/guides/JAVASCRIPT_GUIDE.md");
const pkg = process.argv[2];
if (!pkg) {
  console.error("usage: node run-guide-examples.mjs <wasm-bindgen output directory>");
  process.exit(2);
}
const moduleUrl = pathToFileURL(resolve(pkg, "liquers_web.js")).href;
const wasmBytes = readFileSync(resolve(pkg, "liquers_web_bg.wasm"));

const guide = readFileSync(guidePath, "utf8");
const blocks = [];
const fence = /(?:<!-- example: not run(?:\s*—\s*(.*?))?\s*-->\s*\n)?```js\n([\s\S]*?)```/g;
for (const match of guide.matchAll(fence)) {
  const line = guide.slice(0, match.index).split("\n").length;
  const skipped = match[0].startsWith("<!--");
  blocks.push({ line, skipped, reason: match[1], code: match[2] });
}

const importLine = /^\s*import\s+init,\s*\*\s*as\s+liquers\s+from\s+["']\.\/liquers_web\.js["'];?\s*$/m;
const AsyncFunction = Object.getPrototypeOf(async () => {}).constructor;

let failures = 0;
let ran = 0;
for (const [index, block] of blocks.entries()) {
  const label = `example ${index} (line ${block.line})`;
  if (block.skipped) {
    if (!block.reason) {
      console.log(`FAIL ${label}: marked not run without a reason`);
      failures++;
    } else {
      console.log(`skip ${label}: ${block.reason}`);
    }
    continue;
  }
  if (/^\s*(import|export)\s/m.test(block.code.replace(importLine, ""))) {
    console.log(`FAIL ${label}: only the liquers import line can be supplied by the runner`);
    failures++;
    continue;
  }
  const liquers = await import(`${moduleUrl}?example=${index}`);
  const init = () => liquers.default({ module_or_path: wasmBytes });
  const body = block.code.replace(importLine, "");
  try {
    await new AsyncFunction("init", "liquers", "assert", body)(init, liquers, assert);
    console.log(`ok   ${label}`);
    ran++;
  } catch (e) {
    failures++;
    const detail = e && e.errorType ? `${e.errorType}: ${e.message}` : (e && e.stack) || String(e);
    console.log(`FAIL ${label}\n${block.code}\n${detail}`);
  }
}

if (blocks.length < 10) {
  console.log(`FAIL the guide has only ${blocks.length} examples; expected at least 10`);
  failures++;
}
console.log(`\n${ran} passed, ${failures} failed, ${blocks.filter((b) => b.skipped).length} not run`);
process.exit(failures === 0 ? 0 : 1);
