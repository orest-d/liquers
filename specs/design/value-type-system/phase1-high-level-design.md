# Phase 1: High-Level Design - value-type-system

## Feature Name

Liquers value type system

## Purpose

Liquers describes a value with two independent metadata fields (`type_identifier`, `data_format`)
that nothing keeps consistent with the value or with each other, which is the silent-corruption bug
reported as `CORE-METADATA-FORMAT-TYPE-CONSISTENCY`. This project replaces the ad-hoc pair with an
explicit type model on four independent axes — **variant identity**, **carrier**, **principal data
type**, and **purposes** — plus a registry that owns the facts about each type, so that type
information can serve its three real jobs at once: describing a value, driving serialization, and
(later) driving automatic conversion. It also widens the scalar set, which is narrower than every
ecosystem Liquers must exchange values with.

## Core Interactions

### Query System
None. No query syntax changes; type identifiers are metadata, not query tokens.

### Store System
Read-back correctness: the stored `data_format` — never the filename extension — selects the
deserializer, and the stored variant identity selects the value to reconstruct.

### Command System
`ArgumentInfo`/`CommandMetadata` gain the ability to state an argument's required *purpose* instead
of only `ArgumentType::Any` (the `// TODO: add support for value with type_identifier` markers at
`liquers-core/src/command_metadata.rs:73` and `:152`). Declaration and validation only; automatic
coercion of arguments stays out of scope.

### Asset System
`AssetManager::set`/`set_state` validate the type/format pair before persisting; `AssetInfo` carries
the new fields so a client can tell what an asset is without fetching it.

### Value Types
`ValueInterface` grows the type-describing methods and loses none; `ExtValue`, `SimpleValue`,
`CombinedValue`, `ForeignValue`, the Python and JavaScript values all report into one registry.

**The scalar set widens.** `Value` carries `None, Bool, I32, I64, F64, Text, Bytes`; measured
against the nine ecosystems Liquers exchanges with (`./prior-art.md` §9) it lacks `i8, i16, i128,
u8, u16, u32, u64, u128, f32, decimal, date, time, datetime, duration, uuid`. Scalars are defined
**by their Rust type** — a Liquers scalar exists only if Rust has it (or a canonical crate does,
for `decimal`/temporal/`uuid`) and at least five of the nine systems represent it distinctly. That
rule excludes `f16`, `complex`, `char`, `isize`/`usize`, and GlueSQL's `Inet`/`Point`.

### Web/API (if applicable)
Asset-info responses expose the new fields; content negotiation gains a defensible basis. No new
endpoints.

### UI (if applicable)
UI widgets can select a renderer by purpose (`table`, `image`) rather than by concrete variant.
No widget work in this project.

## Crate Placement

**liquers-core** (`value.rs`, `metadata.rs`, `state.rs`, new `type_system.rs`) — the model, the
registry trait, the invariants. **liquers-lib** (`value/`) — registration of the rich types.
**liquers-py**, **liquers-web** — register their foreign types; both are breaking-change surfaces
and are checked in Phase 2. No `liquers-store` or `liquers-axum` structural change.

## Documentation Intent

**Reference:** Create `specs/reference/VALUE_TYPE_SYSTEM.md` — the four axes, the field
invariants, the registry contract, and the naming rules for identifiers. Audience: internal. It is
new rather than an extension because no current reference owns value typing; `PROJECT_OVERVIEW.md`
covers it in a paragraph and `ASSET_SET_OPERATION.md` only states requirements on `set()`.

**Guide:** Create `specs/guides/TYPE_SYSTEM_GUIDE.md` — how to add a value type, choose its
identifier, declare its purposes, and make it serializable. Audience: both. Written in Phase 5
against the implementation, since a guide claims present behaviour (`DOCS_STRUCTURE_GUIDE.md` §9).

**Other documents to create:** `prior-art.md` (done) and `type-conversion-draft.md` (Phase 2) in
this design folder. `prior-art.md` §1–8 researches UTI, Arrow, media types, MLflow flavors, Jupyter
MIME bundles, structural typing and clipboard negotiation; §9 inventories the scalar type systems
of Rust, JSON, Python, JavaScript, NumPy, Polars, Pandas, Arrow/Parquet and GlueSQL, read from each
project's defining source, and carries the **correspondence table** across all nine. The conversion
draft builds the conversion rules and mechanism on that table; it is explicitly a draft for a
**later** project, not part of this one.

**Specific documents to update:** `specs/reference/PROJECT_OVERVIEW.md` (value/state/metadata
section), `specs/reference/ASSET_SET_OPERATION.md` (the mandatory-field rules it already asserts
but code does not enforce), `specs/reference/api/DOC_01_ARCHITECTURE_REFERENCE.md`, `specs/README.md`
(capability map), `CLAUDE.md` ("Adding a Value Type").

## Settled by the user (2026-08-18)

- **Backward compatibility is not a constraint.** Type identifiers are changed outright;
  no migration of stored data and no compatibility alias table. `Value::I32.identifier()` becomes
  `i32`, not `generic`.
- **The write path rejects.** Inconsistent metadata is a typed `Error` at `set`/`set_state`, not a
  normalise-and-warn. The predecessor plan's hybrid option is dropped.
- **Scalars are grounded in Rust**, and the correspondence table across the nine ecosystems is a
  required artefact.

## Open Questions

1. Do purposes need to be a closed vocabulary in `liquers-core`, an open namespaced string space,
   or a registry that third parties extend at startup? Prior art favours the third.
2. Should the carrier (`native`, `json`, `javascript`, `python`, `polars`) be a separate field or a
   namespace prefix inside the identifier (`py:int`, `js:number`, `core:i32`)?
3. Where does the registry live at runtime — in `Environment`, or a process-global static?
4. Do the fifteen new scalars go in as flat `Value` variants, or behind a `Value::Scalar(Scalar)`
   sub-enum? The sub-enum keeps `Value` small and gives the scalar set its own exhaustive `match`.
5. `Value` is `#[serde(untagged)]`; ten more numeric variants make shape inference ambiguous —
   `7` matches `I8`, `I16`, `I32`, `U8`… Does serialization move to the declared type identifier
   (which this project argues for anyway), or does the enum become tagged?
6. `decimal`, temporal and `uuid` need `rust_decimal`, `chrono`/`time` and `uuid` in
   `liquers-core`, which is meant to stay minimal. Mandatory dependencies or optional features?
   Feature-gating a *scalar* means a value that exists in one build and not another.
7. Does the guide need `docs/` (user-facing) coverage as well, or is `specs/guides/` enough?

## References

- `specs/issues/CORE-METADATA-FORMAT-TYPE-CONSISTENCY.md` — the P0 this project resolves
- `specs/design/metadata-consistency/{FINDINGS,PROPOSED_PLAN}.md` — predecessor investigation;
  this design supersedes its scope
- `specs/issues/CORE-VALUE-INTERFACE-CAPABILITY-SPLIT.md` — naming TODOs and the trait split that
  should follow this work
- `specs/issues/COMBINED-VALUE-DISCRIMINATION.md` — deserialization dispatch, directly enabled here
- `specs/issues/VALUE-DESCRIPTION.md` — the "describe a value" job, adjacent and overlapping
- `./prior-art.md` §1–8 — Apple UTI, Apache Arrow, IANA media types, MLflow flavors, Jupyter MIME
  bundles, structural typing, clipboard format negotiation
- `./prior-art.md` §9 — scalar type systems of Rust, JSON, Python, JavaScript, NumPy, Polars,
  Pandas, Arrow, Parquet and GlueSQL, with the nine-way correspondence table
