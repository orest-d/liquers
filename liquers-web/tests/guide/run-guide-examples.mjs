// Runs every JavaScript example in `specs/guides/JAVASCRIPT_GUIDE.md` (`DOCS03`) and in the user
// guide *Liquers for the Web*, `docs/guides/web/*.md`.
//
// Both guides say each example runs as written; this keeps that true. Usage:
//
//   node run-guide-examples.mjs <dir> [name…]   # <dir> holds liquers_web.js from `wasm-bindgen --target web`
//
// A `name` limits the run to the files whose name contains it (`03-queries`), which is handy while
// writing a chapter; the minimum counts are then not checked.
//
// `scripts/test-guide-examples.sh` builds that directory and runs this. Each ```js block runs in a
// fresh instance of the module — its own wasm instance and so its own environment — so one
// example's registrations cannot make another pass. The block's `import … from "…liquers_web.js"`
// line is what a page writes: `./liquers_web.js` for a copy beside the page, or the address the
// project site publishes the module at, which the user guide's examples use. Here it is replaced
// by the same bindings: `init` (loading the wasm from bytes, since Node cannot fetch a file URL)
// and the module namespace `liquers`. `assert` is Node's `node:assert`, which the guides say their
// examples use.
//
// A block preceded by `<!-- example: not run — <reason> -->` is skipped, and must give a reason.
// A block preceded by `<!-- example: needs records -->` runs only against a `records` build.

import assert from "node:assert";
import { readFileSync, readdirSync } from "node:fs";
import { resolve, dirname } from "node:path";
import { fileURLToPath, pathToFileURL } from "node:url";

const here = dirname(fileURLToPath(import.meta.url));
const root = resolve(here, "../../..");
const webGuide = resolve(root, "docs/guides/web");
// Each source: the file, and the fewest examples it must hold (a guard against a fence that
// stopped matching, which would otherwise pass by running nothing).
const sources = [
  { path: resolve(root, "specs/guides/JAVASCRIPT_GUIDE.md"), name: "JAVASCRIPT_GUIDE.md", least: 10 },
  ...readdirSync(webGuide)
    .filter((name) => name.endsWith(".md") && name !== "HISTORY.md")
    .sort()
    .map((name) => ({ path: resolve(webGuide, name), name: `web/${name}`, least: 0 })),
];
const WEB_GUIDE_LEAST = 40;

const pkg = process.argv[2];
if (!pkg) {
  console.error("usage: node run-guide-examples.mjs <wasm-bindgen output directory> [name…]");
  process.exit(2);
}
const only = process.argv.slice(3);
const moduleUrl = pathToFileURL(resolve(pkg, "liquers_web.js")).href;
const wasmBytes = readFileSync(resolve(pkg, "liquers_web_bg.wasm"));

const fence = /(?:<!-- example: (not run|needs records)(?:\s*—\s*(.*?))?\s*-->\s*\n)?```js\n([\s\S]*?)```/g;
const blocks = [];
const counts = new Map();
for (const source of sources) {
  if (only.length > 0 && !only.some((part) => source.name.includes(part))) continue;
  const text = readFileSync(source.path, "utf8");
  let count = 0;
  for (const match of text.matchAll(fence)) {
    const line = text.slice(0, match.index).split("\n").length;
    blocks.push({ file: source.name, line, marker: match[1], reason: match[2], code: match[3] });
    count++;
  }
  counts.set(source.name, count);
}

// The module's address: a copy beside the page, or the one the project site publishes.
const importLine =
  /^\s*import\s+init,\s*\*\s*as\s+liquers\s+from\s+["'](?:\.\/|https:\/\/orest-d\.github\.io\/liquers\/pkg\/liquers-web\/)liquers_web\.js["'];?\s*$/m;
const AsyncFunction = Object.getPrototypeOf(async () => {}).constructor;

let failures = 0;
let ran = 0;
let notRun = 0;
for (const [index, block] of blocks.entries()) {
  const label = `${block.file}:${block.line}`;
  if (block.marker === "not run") {
    notRun++;
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
  if (block.marker === "needs records" && liquers.RecordBatch === undefined) {
    notRun++;
    console.log(`skip ${label}: this build has no record streams; build with --features records`);
    continue;
  }
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

if (only.length === 0) {
  for (const source of sources) {
    if (counts.get(source.name) < source.least) {
      console.log(`FAIL ${source.name} has only ${counts.get(source.name)} examples; expected at least ${source.least}`);
      failures++;
    }
  }
  const web = [...counts].filter(([name]) => name.startsWith("web/")).reduce((sum, [, n]) => sum + n, 0);
  if (web < WEB_GUIDE_LEAST) {
    console.log(`FAIL the user guide has only ${web} examples; expected at least ${WEB_GUIDE_LEAST}`);
    failures++;
  }
}
console.log(`\n${ran} passed, ${failures} failed, ${notRun} not run`);
process.exit(failures === 0 ? 0 : 1);
