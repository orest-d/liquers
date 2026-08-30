---
id: COMMAND-DECLARATION-FORMAT
kind: feature
title: No language-neutral command declaration format, so every binding hand-parses its own
status: closed
priority: P0
complexity: L
area: [core/commands, web, py]
design: command-declaration
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

## Scope revision (2026-08-29)

Re-scoped **M → L** by maintainer decision. The design work established that a declaration is the
runtime equivalent of `register_command!`, not a serialization of `CommandMetadata`, and that its
substance is a *merge*: a partial declaration composed over what the host discovered by
introspection. That brings a merge algebra with absence-tracking and name-keyed argument merging, a
defaults-derivation rule set, and a call specification (state form, variadic passing, asynchrony) —
none of which fits an `M`. Under `DOCS_STRUCTURE_GUIDE.md` §4.5 the design folder is now required
rather than optional, and `design/command-declaration/` adopts the `liquers-project` workflow.

See `design/command-declaration/purpose-and-semantics.md` for the purpose statement and the recorded
decisions.

## Priority rationale

Recorded **P0** by maintainer decision (2026-08-27): this is a prerequisite for the document-driven
JavaScript and Python setup path, and that work cannot start until it lands.

Note the tension with `DOCS_STRUCTURE_GUIDE.md` §4.4, which defines P1 as "something blocking
planned work" and reserves P0 for incorrect results, data loss, a panic on a supported path, or a
documented feature that does not work. This issue is none of those; it is scheduling weight, applied
deliberately. Either §4.4 should gain a clause for hard prerequisites, or this should settle at P1.

**Confirmed 2026-08-30, and the `P0` stands.** Supporting Python *and* JavaScript is real and likely
the next major development goal, so the prerequisite claim is now a fact rather than a projection.
The §4.4 tension is unchanged — this is still not a wrong result, data loss, a panic or a broken
documented feature — so the observation above stands as filed: §4.4 has no clause for a hard
prerequisite, and one would be worth adding rather than leaving each such issue to argue the point.
The design's evaluation of the same question is in
`design/command-declaration/purpose-and-semantics.md` §The test this design has to pass.

## Verification

1. Round-trip: every command in `specs/command_registry.yaml` parses into equivalent metadata.
2. A declaration registered from JSON produces the same `CommandMetadata` as the equivalent
   `register_command!` invocation, including `metadata_version`.
3. `liquers-web`'s command conformance suites pass unchanged (error wording preserved).
4. A malformed declaration reports the same diagnostics as today.

## Resolution (2026-08-30)

**Closed — implemented.** `liquers-core::command_declaration` owns the shared pipeline: merge over
introspection, apply conventions, derive defaults, build and validate. `liquers-web` parses its
declarations through it, and about 150 lines of hand-written `JsValue` parsing are gone.

- Format reference: [`reference/COMMAND_DECLARATION.md`](../reference/COMMAND_DECLARATION.md)
- Design and reasoning: [`design/command-declaration/`](../design/command-declaration/)
- Tests: 51 unit and 5 integration in `liquers-core`, 3 conversion tests in `liquers-web`, with the
  20-test COMMAND conformance suite unchanged and passing.

All four verification criteria hold. The registry round-trips byte-identically; a declaration and
`register_command!` agree including `metadata_version`; the `liquers-web` suites pass with their
error wording preserved; and the diagnostics name the command and argument.

Four defects were found by doing the work and are filed separately, none of them introduced by it:
`MACRO-LEAVES-STALE-METADATA-VERSION` (P1), `ARGUMENT-GUI-INFO-HAS-THREE-DEFAULTS`,
`STATE-ARGUMENT-CONSTRUCTOR-SERDE-DEFAULT-DISAGREE` and `COMMAND-METADATA-HAS-NO-COMMAND-LEVEL-HINTS`.
