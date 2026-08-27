---
id: COMMAND-DECLARATION-FORMAT
kind: feature
title: No language-neutral command declaration format, so every binding hand-parses its own
status: draft
priority: P0
complexity: M
area: [core/commands, web, py]
design: environment-builder
created: 2026-08-27
github:
---
## Problem

There is no serde-able, language-neutral description of a command. `liquers-web` therefore
hand-rolls one: `JsCommandSpec` (`liquers-web/src/command/spec.rs`) reads a declaration out of a
`JsValue` field by field with `js_sys::Reflect::get`, plus bespoke `get_string` / `get_bool`
helpers and hand-written error messages for every missing or malformed field.

That parser is perfectly good, and entirely unreusable. A Python binding wanting the same
"declare commands from a document" ergonomics must write the whole thing again against
`PyObject`, and the two will drift in what they accept, what they infer, and what they say when a
declaration is wrong.

## Why it matters now

The intended host setup is **two documents**: one configuring the environment (see
`STORE-CONFIG-IN-CORE`), one declaring commands. The second document has no home today.

The obstacle is that a command declaration is not uniformly serializable: `JsCommandSpec.run` is a
`js_sys::Function`. The fix is to split the declaration rather than to abandon it:

- **Declarative half** — name, namespace, realm, arguments and their types and defaults, state
  mode, async-ness, label, doc. Pure data. Serde-able, and identical in every language.
- **Implementation half** — the callable. Referenced from the document *by name* and resolved
  against a host-supplied table.

```json
{ "commands": [
    { "name": "greet",
      "arguments": [ { "name": "greeting", "type": "string", "default": "Hello" } ],
      "state": "value",
      "run": "greet" } ] }
```

`run` is a key into a JavaScript object of functions, not a function. The host resolves it; the
core parses everything else.

## Expected behavior

`liquers-core` owns a `CommandDeclaration` (serde `Serialize` + `Deserialize`) that converts into
`CommandMetadata` with the existing validation and error messages. `JsCommandSpec` becomes a thin
binder — deserialize the declarative half, resolve `run` — and a Python binding reuses the same
type.

## Relationship to existing work

- `specs/command_registry.yaml` is already a serialized view of command metadata, produced by
  `export-command-registry`. It is a strong starting point for the schema, and a natural
  round-trip test: export, re-parse, compare.
- The `register_command!` macro builds the same metadata from Rust syntax. The declaration format
  is its runtime counterpart; they should agree on argument types and defaults, and a test should
  enforce that.
- `POST-INIT-COMMAND-REGISTRATION` is the other half of the ergonomics: for a document-driven host,
  registering commands after the environment is built is the normal case, not the exception.

## Fix direction

1. Define `CommandDeclaration` in `liquers-core/src/command_metadata.rs` (or a sibling module),
   deriving `Serialize`/`Deserialize`, converting to `CommandMetadata` fallibly.
2. Reuse `specs/command_registry.yaml`'s field names where they already fit, so the exporter and
   the parser describe one format rather than two.
3. Reimplement `JsCommandSpec::parse` over it, keeping the current error wording where the tests
   assert on it.

## Priority rationale

Recorded **P0** by maintainer decision (2026-08-27): this is a prerequisite for the document-driven
JavaScript and Python setup path, and that work cannot start until it lands.

Note the tension with `DOCS_STRUCTURE_GUIDE.md` §4.4, which defines P1 as "something blocking
planned work" and reserves P0 for incorrect results, data loss, a panic on a supported path, or a
documented feature that does not work. This issue is none of those; it is scheduling weight, applied
deliberately. Either §4.4 should gain a clause for hard prerequisites, or this should settle at P1.

## Verification

1. Round-trip: every command in `specs/command_registry.yaml` parses into equivalent metadata.
2. A declaration registered from JSON produces the same `CommandMetadata` as the equivalent
   `register_command!` invocation, including `metadata_version`.
3. `liquers-web`'s command conformance suites pass unchanged (error wording preserved).
4. A malformed declaration reports the same diagnostics as today.
