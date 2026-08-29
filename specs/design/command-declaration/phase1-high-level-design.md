For [`issues/COMMAND-DECLARATION-FORMAT.md`](../../issues/COMMAND-DECLARATION-FORMAT.md). Nothing here is implemented.

# Phase 1 — High-level design

## Problem and evidence

There is no serde-able, language-neutral description of a command *as an author declares it*.
`liquers-web` therefore hand-rolls one: `JsCommandSpec::parse`
(`liquers-web/src/command/spec.rs:106-190`) reads eleven fields out of a `JsValue` with
`js_sys::Reflect::get`, through bespoke `get` / `get_string` / `get_bool` helpers (`:91-103`), with
hand-written error messages for every missing or malformed field, plus `parse_arguments` (`:196`),
`parse_argument_type` (`:236`) and `js_default_to_json` (`:253`). A Python binding wanting the same
ergonomics must write all of it again against `PyObject`.

The obvious candidate for the format already exists and does **not** work. `CommandMetadata`
(`liquers-core/src/command_metadata.rs:772`) derives `Serialize` and `Deserialize`, and
`specs/command_registry.yaml` is a serialized `CommandMetadataRegistry`. But its deserializer is not
an author-facing format — measured directly against `HEAD`:

| Input | Result |
|---|---|
| `{"name":"greet"}` | `missing field 'label'` |
| `{"name":"greet","label":"Greet"}` | `missing field 'cache'` |
| `{"name":"greet","label":"Greet","cache":true,"volatile":false}` | `missing field 'definition'` |
| `{"name":…,"label":…,"cache":…,"volatile":…,"definition":"Registered"}` | accepted |

So the minimum accepted object names five fields, three of which (`label`, `cache`, `definition`)
an author should never have to write. `CommandMetadata` round-trips its own output faithfully — it
is a good *export* format and a poor *declaration* format.

Three concepts in a JavaScript declaration have no `CommandMetadata` home at all:

- **`state`** — `none | value | text | state`, i.e. which form of the input the callable receives.
  `StateMode` (`spec.rs:32`) is a `liquers-web` type; `CommandMetadata.state_argument` is
  `Option<ArgumentInfo>` and is set to `Some(any_argument("state"))` unconditionally by
  `CommandMetadata::from_key` (`command_metadata.rs:929`), so it does not carry the distinction.
- **`async`** — tri-state (`IsAsync::{Async, Sync, Auto}`, `spec.rs:69`). `CommandMetadata.is_async`
  is a `bool` and cannot express `Auto` ("decide per call by testing whether the result is
  thenable").
- **`arguments` inferred versus declared** — `adapter.rs:26-37` keeps this in a thread-local
  `INFERRED_ARGUMENTS` set precisely because "inventing a field in `liquers-core` for a
  JavaScript-only concern would be wrong".

## Expected behaviour and acceptance criteria

1. `liquers-core` owns a `CommandDeclaration` deriving `Serialize` + `Deserialize`, whose minimal
   form is `{"name": "greet"}`, converting fallibly into `CommandMetadata`.
2. Field names agree with `specs/command_registry.yaml` where they already fit, so exporter and
   parser describe one format rather than two.
3. Every command in `specs/command_registry.yaml` can be expressed as a `CommandDeclaration` whose
   `to_metadata()` equals the exported `CommandMetadata` — a round-trip test.
4. A declaration parsed from JSON produces the same `CommandMetadata` as the equivalent
   `register_command!` invocation, `metadata_version` included. (`metadata_version` is computed by
   `CommandMetadataRegistry::add_command_metadata` from the stored content —
   `command_metadata.rs:1036,1064` — so equal content gives an equal version automatically; the
   test asserts it rather than assuming it.)
5. `JsCommandSpec::parse` is reimplemented over `CommandDeclaration`, keeping the `js_sys::Function`
   resolution in `liquers-web`. All `liquers-web` command conformance suites pass unchanged, with
   error wording preserved where a test asserts on it.
6. A Python binding can deserialize the same document with no new parsing code.

## Affected users, workflows and systems

Commands (`core/commands`), the browser binding (`web`) and the future Python binding (`py`). No
change to Query, Store, Assets or UI. Every existing Rust command keeps being registered by
`register_command!`; the declaration format is that macro's *runtime* counterpart, not its
replacement.

## Scope and non-goals

In scope: the core type, its conversion and validation, the `liquers-web` re-implementation, and
tests including the registry round-trip.

Explicitly **not** in this issue:

- the Python binding itself;
- `POST-INIT-COMMAND-REGISTRATION` (registering after `to_ref`), which is a separate half of the
  ergonomics;
- the *environment* document (`STORE-CONFIG-IN-CORE`) — this is document #2 of two;
- any change to `register_command!`;
- any change to what `export-command-registry` writes.

## Compatibility constraints

- `specs/command_registry.yaml` must keep deserializing and keep exporting byte-compatibly:
  `cargo test -p liquers-lib --test registry_export` compares signatures, and `liquers-validate`
  reads the file.
- `liquers-web`'s public JavaScript surface (`liquers.registerCommand({…})`) must accept every
  declaration it accepts today, and reject with the same wording where a test asserts it.

## Known questions and assumptions

- **Q1** — does `CommandDeclaration` carry a `run: Option<String>` (the host-resolved
  implementation name from the issue's JSON sketch), or is that a wrapper type? See Phase 2.
- **Q2** — the JavaScript parser accepts type *aliases* (`str`, `text`, `integer`, `number`,
  `boolean`) that `ArgumentType`'s serde names do not. Serde-based parsing changes what is accepted
  in both directions. See Phase 2 §Argument types.
- Assumption: preserving the current bespoke error wording matters only where a test asserts it.
  Verified — `commands_COMMAND.rs` asserts on `"kaboom"` (`:66`), `"reserved"` (`:509`),
  `"not a plain identifier"` and `"Function.length" || "parameter list"` (`:409,:422`), and nothing
  else.

## Documentation assessment

Potentially substantive, to revisit at Phase 5: a declaration format that two bindings share wants
a reference document describing its fields, and
`specs/guides/LANGUAGE-INTEGRATION_GUIDE.md` §COMMAND is where an integration author would look.
Small maintenance in scope: a pointer from `specs/reference/REGISTER_COMMAND_FSD.md` noting the
runtime counterpart. A *new* reference document is a Phase 5 proposal, not in-scope work.
