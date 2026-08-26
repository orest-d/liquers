---
id: FOREIGN-VALUE-TYPE-REGISTRATION-PHASE1
kind: design
title: "Phase 1: High-level design — runtime registration of foreign value types"
status: in_review
phase: high-level
area: [core/value, lib/value, web, py]
created: 2026-08-26
---
# Phase 1: High-Level Design — Foreign Value Type Registration

## Feature Name

Foreign value type registration (fixes `FOREIGN-VALUE-TYPES-NOT-REGISTERED`)

## Purpose

A value whose type identifier is only known at *runtime* — a JavaScript, Python or Starlark handle
held in `ExtValue::Foreign` — cannot appear in the `TypeRegistry`, which is seeded from the
**static** `ValueInterface::type_descriptions()`. Since `value-type-system` step 6 the write path
refuses any identifier the registry does not contain, so such a value cannot be stored at all. This
design gives an integration a way to declare its types, restoring metadata-only persistence for
values that have no byte form.

**Confirmed against a build** (2026-08-26, native, mock `ForeignValue` returning `js.Value`):
`AssetManager::set_state` fails with `[General] Type identifier 'js.Value' is not registered in
this build`. The issue's "not verified" caveat is now settled.

## Core Interactions

### Query System
None. No syntax, parsing or plan change.

### Store System
Indirect only: the refusal happens above the store, in asset-write validation, so nothing reaches
a backend today. After the fix, a foreign value persists as metadata only, exactly as a UI element
does.

### Command System
No new command required for the fix. A diagnostic command listing the registered types (the
`value-type-system` Phase 2 open question) is a candidate, not a commitment.

### Asset System
`AssetManager::set_state` / `set_binary` → `validate_metadata_hard` (`liquers-core/src/assets.rs:584`)
is the failing check. The read path already degrades gracefully on an unknown identifier
(`deserialize_stored_value`), and that behaviour is deliberately left alone.

### Value Types
No new `ExtValue` variant. `ForeignValue` gains a `type_info()` with a default derived from its
existing `identifier` / `type_name` / `default_*` methods and no supported data formats, so the
format check exempts it the way it exempts `UIElement`.

### Web/API
`liquers-web` is the first consumer: `JsOpaque` (`js.Value`) declares itself where the environment
is constructed. Registrations must survive the environment rebuild that `PENDING_ENV` / store-config
replay performs, or they are silently lost — the same trap store configuration already documents.

### UI
Not applicable.

## Crate Placement

- **liquers-core** — `type_system.rs` (how a runtime type is registered) and `context.rs` (where an
  `Environment` accepts one). The registry must stay lock-free and read-only *once shared*: an
  environment is mutable until `to_ref()` consumes it into an `Arc`, so registration is confined to
  that window.
- **liquers-lib** — `value/foreign.rs` (`ForeignValue::type_info`), `environment.rs` (registration
  entry point on `DefaultEnvironment`).
- **liquers-web** — registers `js.Value` at environment construction; no new abstraction.
- **liquers-py** — out of scope; see Open Question 4.

## Documentation Intent

**Reference:** Extend `specs/reference/VALUE_TYPE_SYSTEM.md` — it states the registry is built once
from static descriptions, which stops being the whole truth. One subsection on runtime
registration plus a `## History` row. No new reference: this is one mechanism inside an existing
model, not a new model.

**Guide:** Extend `specs/guides/LANGUAGE-INTEGRATION_GUIDE.md` §VALUE, "Typing an integrated value",
which currently ends with "**Registration is an open problem**" and points at this issue. That
paragraph becomes the procedure. Extend `specs/guides/TYPE_SYSTEM_GUIDE.md` only if the four-step
"Adding a Value Type" procedure acquires a runtime variant.

**Other documents to create:** None. The fix is M-sized and its reasoning fits the Phase 5 summary.

**Specific documents to update:** `specs/issues/FOREIGN-VALUE-TYPES-NOT-REGISTERED.md` (status),
`specs/README.md` (design folder link), `CLAUDE.md` "Adding a Value Type" only if the guide changes.

Audience: an integration author bridging a language into Liquers, who must be able to type their
values without reading this design folder.

## Open Questions

1. **Registration point, family entry, or both?** A mutable-at-construction registration point
   (issue option 1) matches the intent already recorded in `value-type-system`
   phase2-architecture.md — "builds its registry once at construction … then extends it with any
   foreign registrations" — and is precise. A provider *family* entry (option 2, one `TypeInfo` for
   all of `js.*`) additionally covers identifiers not known even at startup, e.g. a Python
   integration typing per class (`py.numpy.ndarray`). Recommendation: build option 1, decide option
   2 in Phase 2 on whether any planned integration needs per-instance identifiers.
2. **Where does an integration register?** A method on the `Environment` trait, an inherent method
   per implementor, or a `TypeRegistry` handed to the constructor. Affects four implementors.
3. **What happens to an unregistered foreign type after the fix** — still a hard refusal, or a
   logged degrade to metadata-only? A refusal is the current rule and is honest; a degrade restores
   pre-`value-type-system` behaviour, which the issue calls a regression.
4. **`liquers-py`**: its `Value` overrides no `type_descriptions()`, so its registry holds only
   `error` and *every* write would be refused — dormant only because `context.rs` and `value.rs`
   are not declared modules (`PY-MODULES-NOT-DECLARED-IN-LIB`). Filed separately as
   `PY-VALUE-TYPE-DESCRIPTIONS-MISSING`; confirm it stays out of this design's scope.

## References

- `specs/issues/FOREIGN-VALUE-TYPES-NOT-REGISTERED.md`
- `specs/design/value-type-system/phase2-architecture.md` (registry model, "Environment — one new method")
- `specs/reference/VALUE_TYPE_SYSTEM.md`, `specs/guides/LANGUAGE-INTEGRATION_GUIDE.md` §VALUE
- `specs/issues/TYPE-REGISTRY-NOT-REALM-AWARE.md` (adjacent, not a prerequisite)
- Code: `liquers-core/src/type_system.rs`, `liquers-core/src/assets.rs:584`,
  `liquers-lib/src/value/foreign.rs`, `liquers-web/src/value.rs:67`
