For [`issues/COMMAND-DECLARATION-FORMAT.md`](../../issues/COMMAND-DECLARATION-FORMAT.md). Nothing here is implemented.

# Phase 1 — High-level design

> Rewritten 2026-08-29 together with Phase 2. The measurements below are the original ones and
> reproduce exactly at `HEAD`; the conclusion drawn from them changed. The superseded reading — that
> `CommandMetadata` cannot serve as the declaration format and needs a parallel type — is recorded
> in [`phase2-architecture.md`](./phase2-architecture.md) §Rejected alternatives.

## Problem and evidence

There is no serde-able, language-neutral description of a command *as an author declares it*.
`liquers-web` hand-rolls one: `JsCommandSpec::parse` (`liquers-web/src/command/spec.rs:106-190`)
reads eleven fields out of a `JsValue` with `js_sys::Reflect::get` and bespoke `get`/`get_string`/
`get_bool` helpers (`:91-103`), plus `parse_arguments` (`:196`), `parse_argument_type` (`:236`) and
`js_default_to_json` (`:253`). A Python binding wanting the same ergonomics rewrites the serde-able
part of that against `PyObject`, and the two then drift in what they accept, what they infer, and
what they say when a declaration is wrong. **The drift is the cost, not the line count** — of
`spec.rs`'s 389 lines only ~136 are shareable; the rest parses JavaScript source, which Python
replaces with `inspect.signature` regardless.

`CommandMetadata` (`command_metadata.rs:772`) is the natural format — it derives `Serialize` and
`Deserialize`, and `specs/command_registry.yaml` is a serialized `CommandMetadataRegistry` — but its
deserializer is not author-facing. Measured against `HEAD`:

| Input | Result |
|---|---|
| `{"name":"greet"}` | `missing field 'label'` |
| `{"name":"greet","label":"Greet"}` | `missing field 'cache'` |
| `{"name":"greet","label":"Greet","cache":true,"volatile":false}` | `missing field 'definition'` |
| `{"name":…,"label":…,"cache":…,"volatile":…,"definition":"Registered"}` | accepted |

The minimum accepted object names five fields, three of which an author should never write. But the
strictness is an oversight, not a property of the format: **14 of `CommandMetadata`'s 20 fields
already carry `#[serde(default)]`**, and nothing separates the four that do not — `is_async: bool`
has one, `volatile: bool` three fields away does not. `ArgumentInfo::label` is a fifth
(`{"name":"count"}` fails the same way). **The gap is five serde attributes, not a new type.**

What `CommandMetadata` genuinely cannot express is **the wrapping model** — how a callable is
adapted to serve as a command. Every binding has this concept and none of them shares it: Rust
decides it at compile time in `register_command!` (`CommandSignature`, `registration.rs:1104`,
consumed by codegen and gone after expansion); `liquers-web` re-decides it at runtime
(`CallableSpec`, `adapter.rs:130`, whose doc comment reads *"The retained callable and how to call
it"*); a document cannot express it at all. `CommandMetadata` describes *the command* and says
nothing about *the call*, which is correct — but leaves a document-driven host nowhere to put it.

Most of that model is already portable or already recorded. Only two decisions are neither:

- **`state`** — `none | value | text | state`, which form of the input the callable receives.
  `state_argument` records *whether*, never *which*; the macro's `StateParameter` (`:785`) records
  both.
- **`async` when undeclared** — the macro knows from `async fn`; a host may need to decide per call
  (`IsAsync::Auto`, `spec.rs:69`). `is_async` is a `bool` and has no undeclared state.

Two more are wrapping decisions that stay out: the **result form** (`ResultType{Value,Result}`,
`:792`) is Rust-only, since a JavaScript callable throws and a Python one raises; and **context
injection** (`CommandParameter::Context`, `:489`) is portable in principle but has no JavaScript
implementation at all — `register_js_command` receives a context and discards it
(`_context`, `adapter.rs:165`). Filed as
[`JS-COMMAND-CANNOT-ACCESS-CONTEXT`](../../issues/JS-COMMAND-CANNOT-ACCESS-CONTEXT.md).

Separately, **`arguments` inferred versus declared** is a parse artifact rather than part of the
model — kept in a thread-local (`adapter.rs:26-37`) because `Vec<ArgumentInfo>` cannot distinguish
"absent" from "empty".

A further candidate, **`run`** (naming the implementation, from the issue's JSON sketch), belongs to
neither: it answers *which* implementation, which is `CommandDefinition`'s question, not *how to
call it*. Withdrawn from the design; see Phase 2 open question 1.

Phase 2 also records two constructor/serde default disagreements found while re-measuring, both able
to change `metadata_version` silently; one is filed as
[`STATE-ARGUMENT-CONSTRUCTOR-SERDE-DEFAULT-DISAGREE`](../../issues/STATE-ARGUMENT-CONSTRUCTOR-SERDE-DEFAULT-DISAGREE.md).

## Expected behaviour and acceptance criteria

1. `CommandMetadata` deserializes from an author-written document whose minimal form is
   `{"name":"greet"}`, applying the defaults `CommandMetadata::from_key` already applies.
2. `liquers-core` names the portable part of the wrapping model — the state form and the undeclared
   async state — in a type distinct from metadata, so the macro, the browser binding and a document
   describe one model instead of three.
3. Field names agree with `specs/command_registry.yaml` **because they are the same struct**.
4. `specs/command_registry.yaml` parses and re-serializes **byte-identically** — possible only
   because nothing is dropped, so `presets`, `next`, `hints`, `CommandDefinition::Alias` and
   query-valued defaults all keep working from a document.
5. A declaration parsed from JSON produces the same `CommandMetadata` as the equivalent
   `register_command!` invocation, `metadata_version` included. (It is computed from stored content
   by `add_command_metadata` — `command_metadata.rs:1036,1064` — so the test asserts equality rather
   than assuming it.)
6. `JsCommandSpec::parse` is reimplemented over those two types, keeping `js_sys::Function`
   resolution in `liquers-web`. All command conformance suites pass unchanged, with error wording
   preserved where a test asserts on it.
7. A Python binding deserializes the same document with no new parsing code.

## Affected users, workflows and systems

Commands (`core/commands`), the browser binding (`web`) and the future Python binding (`py`). No
change to Query, Store, Assets or UI. Every existing Rust command keeps being registered by
`register_command!`; the declaration format is that macro's *runtime* counterpart, not its
replacement.

## Scope and non-goals

In scope: the serde fixes to `CommandMetadata`, the binding type, their validation, the
`liquers-web` re-implementation, and tests including the registry round-trip.

Explicitly **not** in this issue: the Python binding itself; `POST-INIT-COMMAND-REGISTRATION`
(registering after `to_ref`); the *environment* document (`STORE-CONFIG-IN-CORE`, document #1 of
two); any change to `register_command!`; any change to what `export-command-registry` writes.

## Compatibility constraints

- `specs/command_registry.yaml` must keep deserializing and keep exporting byte-compatibly:
  `cargo test -p liquers-lib --test registry_export` compares signatures, and `liquers-validate`
  reads the file.
- `liquers.registerCommand({…})` must accept every declaration it accepts today, and reject with the
  same wording where a test asserts it.

## Known questions and assumptions

- **Q1 — `run`.** Sharpened rather than answered: it duplicates `CommandDefinition`'s role. Phase 2
  open question 1 offers three resolutions and leaves the choice to the gate.
- **Q2 — argument type aliases.** The JavaScript parser accepts `str`, `text`, `integer`, `number`,
  `boolean`, which `ArgumentType`'s serde names do not. **Resolved** in Phase 2 §Part A by
  `#[serde(alias)]`, which is deserialize-only and leaves the exported file untouched.
- **Q3 — `P0`.** With the fix at five attributes plus a small module, the issue's own note about the
  tension with `DOCS_STRUCTURE_GUIDE.md` §4.4 sharpens: **P1** looks right. For the gate.
- Assumption: preserving the current bespoke error wording matters only where a test asserts it.
  Verified — `commands_COMMAND.rs` asserts on `"kaboom"` (`:66`), `"reserved"` (`:509`), `"not a
  plain identifier"` and `"Function.length" || "parameter list"` (`:409,:422`), and nothing else.

## Documentation assessment

Potentially substantive, to revisit at Phase 5: reusing `CommandMetadata` rather than mirroring it
makes the *field-list* documentation unnecessary — the fields are already documented on the struct.
What is left is a genuine concept with no home: **how a callable becomes a command**, with
`register_command!`, `registerCommand` and a document as three front-ends to one model.
`specs/guides/LANGUAGE-INTEGRATION_GUIDE.md` §COMMAND is where an integration author would look, and
`specs/reference/REGISTER_COMMAND_FSD.md` documents only the Rust front-end today. Small maintenance in scope: a pointer
from `specs/reference/REGISTER_COMMAND_FSD.md` noting the runtime counterpart. A *new* reference
document is a Phase 5 proposal, not in-scope work.
