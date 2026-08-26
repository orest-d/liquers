---
id: FOREIGN-VALUE-TYPE-REGISTRATION-PHASE1
kind: design
title: "Phase 1: High-level design — foreign and Python value types in the type registry"
status: in_review
phase: high-level
area: [core/value, lib/value, web, py]
created: 2026-08-26
---
# Phase 1: High-Level Design — Foreign Value Type Registration

## Feature Name

Foreign value type registration (fixes `FOREIGN-VALUE-TYPES-NOT-REGISTERED` and
`PY-VALUE-TYPE-DESCRIPTIONS-MISSING`)

## Purpose

A value whose type identifier is only known at *runtime* — a JavaScript, Python or Starlark handle
held in `ExtValue::Foreign` — cannot appear in the `TypeRegistry`, which is seeded from the
**static** `ValueInterface::type_descriptions()`. Since `value-type-system` step 6 the write path
refuses any identifier the registry does not contain, so such a value cannot be stored at all. This
design gives an integration a way to declare its type, and applies the same treatment to
`liquers-py`, whose `Value` describes no types at all.

**Confirmed against a build** (2026-08-26, native, mock `ForeignValue` returning `js.Value`):
`AssetManager::set_state` fails with `[General] Type identifier 'js.Value' is not registered in
this build`. The issue's "not verified" caveat is now settled.

## The governing rule: one identifier, one variant

**A type identifier corresponds one-to-one with a value variant.** `ExtValue::Foreign` is a *single*
variant, so it has exactly *one* `TypeInfo` — in a `liquers-web` build, `js.Value`. This settles the
issue's open choice: **there is no provider *family* entry** covering `js.*`, and no per-class
runtime typing such as `py.numpy.ndarray`. What varies per instance is `type_name`, which is
informational and never dispatched on — `JsOpaque` already does exactly this, reporting a constant
`js.Value` identifier and the JavaScript `constructor.name` as `type_name`.

Three consequences, all of which Phase 2 must honour:

1. **The identifier is a property of the `ForeignValue` implementation, not of an instance.** It
   must therefore be obtainable *without a value*, because registration happens at environment
   construction when no foreign value exists yet. `ForeignValue::identifier(&self)` is today an
   instance method; it needs a static counterpart, and the two must be proven to agree.
2. **One build carries one foreign implementation.** Two integrated languages in one build would be
   two identifiers for one variant, which the rule forbids; that case needs a second variant and is
   out of scope here.
3. **`liquers-py` is the last place the old many-to-one model survives.** Its `Value` maps `None`,
   `Bool`, `I32`, `I64`, `F64` and `Array` all onto `"generic"` — precisely the collapse
   `value-type-system` step 3 removed from `liquers-core::Value`. Its `Py` variant is its foreign
   variant and gets one identifier, `py.Object`.

### The rule is nowhere stated, and one formulation contradicts it

The rule is *enforced* — `type_descriptions_match_identifier` (`liquers-core/src/value.rs:1155`)
asserts "one description per variant, no more and no less" — but never *written down*:

| Where | Says | Needs |
|---|---|---|
| `liquers-core/src/value.rs:230` | "Several types can be linked to the same identifier." | **Wrong.** The doc comment on the very trait method now contradicts the test in the same file. Replace it. |
| `specs/reference/VALUE_TYPE_SYSTEM.md` | Cardinality "exactly one"; local name "normally the value variant's name" | Implies the rule; does not state it. State it, and add `Foreign`/`py.*` to the registered-identifier list. |
| `specs/guides/TYPE_SYSTEM_GUIDE.md` §2 | How to choose bare vs prefixed | Add: one identifier per variant, and how a runtime-typed variant declares itself. |
| `specs/guides/LANGUAGE-INTEGRATION_GUIDE.md` §VALUE | "**Registration is an open problem**" | Becomes the procedure. |

Clarifying the formulation is part of this design's deliverable, not a side effect.

## Core Interactions

### Query System
None. No syntax, parsing or plan change.

### Store System
Indirect only: the refusal happens above the store, in asset-write validation. After the fix a
foreign value persists as metadata only, exactly as a UI element does.

### Command System
No new command required. A diagnostic command listing registered types (the `value-type-system`
Phase 2 open question) remains a candidate, not a commitment.

### Asset System
`AssetManager::set_state` / `set_binary` → `validate_metadata_hard` (`liquers-core/src/assets.rs:584`)
is the failing check. The read path already degrades gracefully on an unknown identifier
(`deserialize_stored_value`); that behaviour is deliberately left alone.

### Value Types
No new `ExtValue` variant. `ForeignValue` gains a `type_info()` derived from its identifier and
`default_*` methods, declaring no data formats, so the format check exempts it as it exempts
`UIElement`. `liquers-py`'s `Value` gains `type_descriptions()` with one entry per variant and
identifiers reconciled with `liquers-core`'s, so a store written from Python is readable from Rust.

### Web/API
`liquers-web` is the first consumer: `JsOpaque` declares `js.Value` where the environment is
constructed. Registrations must survive the environment rebuild that `PENDING_ENV` / store-config
replay performs, or they are silently lost — the trap store configuration already documents.

### UI
Not applicable.

## Crate Placement

- **liquers-core** — `type_system.rs` (how a runtime type is registered), `context.rs` (where an
  `Environment` accepts one), `value.rs` (correct the `identifier` doc comment). The registry stays
  lock-free and read-only *once shared*: an environment is mutable until `to_ref()` consumes it into
  an `Arc`, so registration is confined to that window.
- **liquers-lib** — `value/foreign.rs` (`ForeignValue::type_info`), `environment.rs` (registration
  entry point on `DefaultEnvironment`).
- **liquers-web** — registers `js.Value` at environment construction; no new abstraction.
- **liquers-py** — `value.rs`: `type_descriptions()`, identifiers split out of `"generic"`, and
  `python_value` → `py.Object`. Note `python_value` fails `identifier_naming_rule_holds` today
  (underscore is a reserved character), so it could not be registered even if it were described.

## Documentation Intent

**Reference:** Extend `specs/reference/VALUE_TYPE_SYSTEM.md` — state the one-identifier-per-variant
rule, add runtime registration, and extend the registered-identifier list with the foreign variant
and `liquers-py`'s set. Plus a `## History` row and a `reviewed:` bump. No new reference: this is
one mechanism inside an existing model.

**Guide:** Extend `specs/guides/LANGUAGE-INTEGRATION_GUIDE.md` §VALUE, "Typing an integrated value",
whose closing paragraph currently points at this issue as an open problem. Extend
`specs/guides/TYPE_SYSTEM_GUIDE.md` §2 with the cardinality rule and the runtime-typed variant.

**Other documents to create:** None. The fix is M-sized and its reasoning fits the Phase 5 summary.

**Specific documents to update:** `specs/issues/FOREIGN-VALUE-TYPES-NOT-REGISTERED.md` and
`specs/issues/PY-VALUE-TYPE-DESCRIPTIONS-MISSING.md` (status), `specs/README.md` (design link),
`CLAUDE.md` "Adding a Value Type" if the guide's step count changes.

Audience: an integration author bridging a language into Liquers, who must be able to type their
values without reading this design folder.

## Open Questions

1. **Where does an integration register?** A method on the `Environment` trait, an inherent method
   per implementor, or a `TypeRegistry` handed to the constructor. Affects four implementors. The
   intent is already recorded in `value-type-system` phase2-architecture.md — "builds its registry
   once at construction … then extends it with any foreign registrations" — but the shape is not.
2. **What happens to an unregistered foreign type after the fix** — still a hard refusal, or a
   logged degrade to metadata-only? A refusal is the current rule and is honest; a degrade restores
   pre-`value-type-system` behaviour, which the issue calls a regression.
3. **How is the static/instance agreement proven?** A `debug_assert`, a test helper every
   integration calls, or a conformance test in `LANGUAGE-INTEGRATION_GUIDE.md`'s suite.

## Prerequisite (not an open question)

`liquers-py/src/value.rs` and `context.rs` are among the eight files `lib.rs` never declares as
modules (`PY-MODULES-NOT-DECLARED-IN-LIB`, P2). Declaring `value` and `context` produces **four
compile errors in `value.rs`** alone (verified 2026-08-26): `try_into_query` returns the wrong
`Query` type, `from_asset_info` takes one `AssetInfo` where the trait wants a `Vec`, a `match` with
incompatible arms, and four unimplemented trait items (`from_command_metadata`, `try_into_bytes`,
`try_into_key`, `try_into_command_metadata`). The Python half of this design cannot be *verified*
until `value.rs` compiles, so repairing that file is inside this design's scope; the rest of
`PY-MODULES-NOT-DECLARED-IN-LIB` (`commands.rs`, `store.rs`, `interpreter.rs`, …) is not.

## References

- `specs/issues/FOREIGN-VALUE-TYPES-NOT-REGISTERED.md`, `specs/issues/PY-VALUE-TYPE-DESCRIPTIONS-MISSING.md`
- `specs/issues/PY-MODULES-NOT-DECLARED-IN-LIB.md` (prerequisite, partially in scope)
- `specs/design/value-type-system/phase2-architecture.md` ("Environment — one new method"),
  `phase4-implementation.md` step 3 (where `"generic"` was split in core)
- `specs/reference/VALUE_TYPE_SYSTEM.md`, `specs/guides/TYPE_SYSTEM_GUIDE.md`,
  `specs/guides/LANGUAGE-INTEGRATION_GUIDE.md` §VALUE
- Code: `liquers-core/src/type_system.rs`, `liquers-core/src/value.rs:230` and `:1155`,
  `liquers-core/src/assets.rs:584`, `liquers-lib/src/value/foreign.rs`,
  `liquers-web/src/value.rs:67`, `liquers-py/src/value.rs:282`
