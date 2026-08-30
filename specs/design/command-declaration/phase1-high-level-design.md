For [`issues/COMMAND-DECLARATION-FORMAT.md`](../../issues/COMMAND-DECLARATION-FORMAT.md). Nothing here is implemented.

# Phase 1 — High-level design

> Rewritten 2026-08-29, twice. The measurements below are the original ones and reproduce exactly at
> `HEAD`. **[`purpose-and-semantics.md`](./purpose-and-semantics.md) is now the authoritative
> statement of what this feature is**; this document records the evidence that motivated it and the
> acceptance criteria, restated against that purpose. Two superseded readings are in
> [`phase2-architecture.md`](./phase2-architecture.md) §Rejected alternatives.

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

What `CommandMetadata` genuinely cannot express is **how a function becomes a command** — what
`register_command!` decides at compile time. The concept exists twice already and is shared nowhere:
`CommandSignature` (`liquers-macro/src/registration.rs:1104`) is consumed by codegen and gone after
expansion; `CallableSpec` (`liquers-web/src/command/adapter.rs:130`), whose doc comment reads *"The
retained callable and how to call it"*, is wasm32-only. A document cannot express it at all.

Concretely, the call specification needs three things metadata does not carry:

- **`state`** — `none | value | text | state`, which form of the input the callable receives.
  `state_argument` records *whether*, never *which*; the macro's `StateParameter` (`:785`) records
  both.
- **variadic passing** — spread across the call, or collected as one list. Rust has only one mode:
  `get_multiple` (`liquers-core/src/commands.rs:151`) always collects into `Vec<T>`.
- **`async` when undeclared** — the macro knows from `async fn`; a host may need to decide per call
  (`IsAsync::Auto`, `spec.rs:69`).

And it needs something metadata's *representation* cannot do: **compose**. A declaration adds to what
the host already discovered by introspection, field by field and inside nested structures, so an
author can attach a widget hint to one argument without restating its type and default. That requires
distinguishing *absent* from *default-valued*, which `#[serde(default)]` collapses — the reason
merging happens on the serialized form.

Two wrapping decisions stay out. The **result form** (`ResultType{Value,Result}`, `:792`) is
Rust-only, since JavaScript throws and Python raises. **Keyword argument passing** is deferred by
decision (`purpose-and-semantics.md` §Decisions, C3): meaningful in Python, meaningless in
JavaScript. A third, **context injection** (`CommandParameter::Context`, `:489`), is portable in
principle but has no JavaScript implementation at all — `register_js_command` receives a context and
discards it (`_context`, `adapter.rs:165`). Filed as
[`JS-COMMAND-CANNOT-ACCESS-CONTEXT`](../../issues/JS-COMMAND-CANNOT-ACCESS-CONTEXT.md).

A further candidate, **`run`** (naming the implementation, from the issue's JSON sketch), belongs to
neither half: it answers *which* implementation, which is `CommandDefinition`'s question. Withdrawn;
see Phase 2 open question 1.

## Expected behaviour and acceptance criteria

1. `CommandMetadata` deserializes from an author-written document whose minimal form is
   `{"name":"greet"}`, applying the defaults `CommandMetadata::from_key` already applies.
2. `liquers-core` owns stages 2-4 of the pipeline — merge, derive defaults, convert-and-validate —
   so Python, JavaScript and a plain document share them and only their introspection differs.
3. Field names agree with `specs/command_registry.yaml` because the pipeline's output *is*
   `CommandMetadata`; the declaration never re-enumerates its fields.
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
