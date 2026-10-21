# Phase 1: High-Level Design — a type registry that describes every realm

## Purpose

Let one side of a client/server deployment (`VISION.md`, "Client/server") know which value types
the *other* side holds, and which of them can cross the boundary at all, so a boundary problem is
visible before a value is sent rather than when the receiver fails to decode it.

## Problem Example

A page's environment (`liquers-web`) and a server's (`liquers-lib` with `polars`) each build a
`TypeRegistry` with `TypeRegistry::from_value_type::<V>()` (`liquers-core/src/type_system.rs`).

```rust
let server: TypeRegistry = /* the server's registry */;
let json = serde_json::to_string(&server)?;              // today: does not compile —
                                                          // TypeRegistry derives no Serialize
let mut page = TypeRegistry::from_value_type::<Value>();
page.import_realm("server", &serde_json::from_str(&json)?)?;   // today: no such method
page.get_in_realm("server", "polars.DataFrame");          // wanted: Some(..)
page.get_in_realm("", "js.Value").map(|t| t.transfer_mode());
                                                          // wanted: Untransferable
```

Today a planner on the page has no way to learn that `polars.DataFrame` exists only on the server,
and nothing marks `js.Value` (a JavaScript closure, among others) as a value no encoding can carry.

## Scope and Acceptance Criteria

- **AC-1** The registry round-trips as data
  WHEN a `TypeRegistry` is serialized to JSON or YAML and deserialized
  THEN the result holds the same `TypeInfo` entries, and the serialized form is a list of `TypeInfo`
- **AC-2** A duplicate in serialized input is refused
  WHEN a serialized registry lists the same realm and identifier twice
  THEN deserialization fails with an error naming the identifier, as `register` does
- **AC-3** Another side's registry is imported under a realm name
  WHEN `import_realm("server", &other)` is called
  THEN every default-realm entry of `other` is reachable with `get_in_realm("server", id)`, and the local default realm is unchanged
- **AC-4** Each type declares its boundary behaviour
  WHEN a `TypeInfo` is built with `with_transfer(..)`, or without it
  THEN `transfer_mode()` returns the declared mode, defaulting to serialization in its formats, and a type with no formats reports `Untransferable`
- **AC-5** Types missing from a realm are listed
  WHEN a registry holds two or more realms
  THEN `types_not_in(realm)` lists each identifier present in some other realm and absent from `realm`
- **AC-6** Older serialized entries still read
  WHEN a `TypeInfo` serialized before this change (no transfer field) is deserialized
  THEN it reads with the default transfer mode
- **AC-7** `js.Value` is untransferable
  WHEN the web environment's registry is asked for `js.Value`'s `transfer_mode()`
  THEN it answers `Untransferable`

Non-goals: publishing or fetching a registry over HTTP; any planner or interpreter change;
performing a conversion (`VALUE-CONVERSION-CAPABILITY`); a realm for commands that differs from
the type realm name.

## Core Interactions

Value types (`type_system.rs`); the environment builder (`with_type_registry` already accepts a
finished registry); `liquers-web` (one registry entry). Query, store, assets and commands untouched.

## Crate Placement

`liquers-core` (the registry is core's), plus one line in `liquers-web/src/value.rs`. No new
dependency (`serde` is already used).

## Documentation Intent

- Reference: extend `specs/reference/VALUE_TYPE_SYSTEM.md` (the registry's serialized form, realms,
  transfer modes) and `specs/reference/REALMS.md` ("Types per realm" open question).
- Guide: none — `TYPE_SYSTEM_GUIDE.md` gains one line on `with_transfer` for a new type.
- Other documents: none.
- Documents to update: the two references above, `guides/TYPE_SYSTEM_GUIDE.md`.

## Design Readiness

- **Readiness:** needs-decision
- **Automatic fixing:** not-eligible — new `pub` enum and `pub` methods in `liquers-core`, and a
  serialized format (rules 3, 4); crosses two crates (rule 6).
- **Leading issue:** the realm name an import uses (question 1).
- **Explanation:** a self-contained slice: every change is additive, read-only once built, and
  testable without a second process. Three proposals remain visible; none changes the plan.
- **Open questions:**
  1. **Open design - realm of an import:** imported entries take the realm name the importing side
     uses for that host — the same name a query header uses for its commands (`-server/…`).
     Recommended, because a step's command realm and its result's type realm then coincide.
     Alternative: the exporter names its own realm, which breaks when two clients name it
     differently.
  2. **Proposed resolution - what is imported:** only the other side's default-realm entries;
     entries it imported itself are skipped (no transitive relaying), so each realm has one source.
  3. **Proposed resolution - transfer vocabulary:** `Serialize` (default: crosses in a supported
     format), `ConvertTo { type_identifier, data_format }`, `Untransferable`. The issue's
     *degrade to bytes* is not a declared mode: it is the receiver's fallback in
     `value-type-system`, which stays.

## Design Dependencies

- `multi-realm-dispatch` (`CORE-MULTI-REALM-INTERPRETER`) — **required-by** in reverse of the
  issue's wording: this slice does not need dispatch; acting on transfer modes does, which is
  `CROSS-REALM-TRANSFER-NOT-PLANNED` (filed, requires both).
- `VALUE-CONVERSION-CAPABILITY` (issue) — **overlaps**: `ConvertTo` names a target; performing it
  is that issue's conversion registry.
- `data-format-vocabulary` — **overlaps**, weak (its own Phase 1 says so): per-realm formats later.
- `value-type-system`, `foreign-value-type-registration` (complete, E4) — origin; not extended.

## Consolidated Findings

- Purely additive in `liquers-core`: `TypeInfo` gains one `#[serde(default)]` field set only by a
  builder (no struct literal exists outside `type_system.rs`, checked), so no construction site
  breaks and stored descriptions still read (AC-6).
- Lookups used by the write path (`get`, `supports_data_format`) read only the default realm, so
  imported realms cannot change what a local build stores.
- `TypeRegistry` serializes as a list via `#[serde(into, try_from)]`, reusing `register` for the
  duplicate check (AC-2) rather than a second rule.
- `js.Value` already has no formats, so `transfer_mode()` would infer `Untransferable`; the
  explicit declaration documents the closure case the issue names.
- No Liquers query is involved (nothing for `liquers-validate`).
- Decisions: questions 1-3 above; implementing with the recommendations is safe, and changing
  question 1 later changes only the caller's argument.
