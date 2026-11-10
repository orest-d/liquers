---
title: Command library
status: ready
sources: [command_registry.yaml]
reviewed: 2026-11-10
---
# B. Command library

PURPOSE: Every command of the standard library, by namespace, generated from the command registry.

The table below lists the commands of the whole standard library, as a native program has it.
The browser module has a part of it:

| Namespace | In the browser module | What it is |
|---|---|---|
| the root namespace | `to_text`, `to_metadata`, `commands_doc` | conversions and the list of commands |
| `dep` | yes | the versions of commands, used to track dependencies |
| `rec` | yes, in a build with `--features records` | tables (chapter 9) |
| `template` | yes | rendering templates (chapter 10) |
| `lui` | yes | user-interface elements (chapter 11) |
| `pl` | no | Polars data frames |
| `img` | no | images |
| `search`, `md`, `mem` | no | search, Markdown documents, agent memory |

A command in a namespace is called with `ns-<namespace>` before it: `ns-rec/head-10`.

The exact set of a module is what the module itself reports. The query `commands_doc` returns
the commands of the page's environment as Markdown, yours included, and
`liquers.Environment.global().commandNames()` returns their names:

<!-- example: needs records -->
```js
import init, * as liquers from "https://orest-d.github.io/liquers/pkg/liquers-web/liquers_web.js";

await init();
await liquers.init();
const names = liquers.Environment.global().commandNames();

for (const name of ["to_text", "commands_doc", "to_record", "sort", "render", "expand", "markdown"]) {
  assert(names.includes(name), name);
}
assert(!names.includes("from_csv"));          // a Polars command: not in the browser
```

{{$ -R/specs/command_registry.yaml/-/ns-docs/command_table}}
