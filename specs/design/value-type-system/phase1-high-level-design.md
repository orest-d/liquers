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

**The scalar set widens, in three tiers.** Scalars are defined **by their Rust type** — a Liquers
scalar exists only if Rust has it (or a canonical crate does) and at least five of the nine target
systems represent it distinctly (`./prior-art.md` §9). That rule excludes `f16`, `complex`, `char`,
`isize`/`usize`, and GlueSQL's `Inet`/`Point`. Where each scalar lives:

| Tier | Home | Contents |
|---|---|---|
| Core basics | `liquers-core::value::Value` | `none, bool, i32, i64, f64, text, bytes` — unchanged. These are what the query language, action parameters and command metadata actually need. |
| Extended scalars | `liquers-lib::value::ExtValue`, feature-gated | `i8, i16, i128, u8, u16, u32, u64, u128, f32` behind `ext-scalars`; `decimal, date, time, datetime, duration, uuid` behind `ext-temporal`. |
| Carrier-specific | the package that owns the carrier | Polars dtypes (`Categorical`, `Enum`, `Struct`, parameterised `Decimal(p,s)`) stay behind the existing `polars` feature; Python-only types (`py:int` arbitrary precision, `py:complex`, `py:bytearray`) live in `liquers-py`; JavaScript-only types (`js:bigint`, `js:symbol`, typed arrays) live in `liquers-web`. |

The tiering is what makes the registry mandatory rather than merely convenient: a scalar can be
absent from a given build, so the set of known types is a runtime fact each package contributes to,
not a compile-time enum in `liquers-core`. Reading an asset whose type is not registered in this
build must be a clean typed error naming the missing type — which is precisely the failure the P0
currently produces silently.

### Web/API (if applicable)
Asset-info responses expose the new fields; content negotiation gains a defensible basis. No new
endpoints.

### UI (if applicable)
UI widgets can select a renderer by purpose (`table`, `image`) rather than by concrete variant.
No widget work in this project.

## Crate Placement

**liquers-core** (`value.rs`, `metadata.rs`, `state.rs`, new `type_system.rs`) — the model, the
registry trait, the invariants, and only the basic types. It gains no scalar variants and no new
dependency. **liquers-lib** (`value/`) — the extended scalars behind `ext-scalars` / `ext-temporal`,
and registration of the rich types. **liquers-py**, **liquers-web** — register their own
carrier-specific types; both are breaking-change surfaces and are checked in Phase 2. No
`liquers-store` or `liquers-axum` structural change.

Dependency cost is smaller than it looks: `chrono` is **already** a non-optional dependency of both
`liquers-core` (`Cargo.toml:55`) and `liquers-lib`, so the temporal scalars add nothing. Only
`rust_decimal` and `uuid` are new to the workspace, and both land in `liquers-lib` behind
`ext-temporal`. Keeping the wide scalars out of core is therefore a decision about conceptual
surface, not about build weight — worth stating so it is not re-argued on dependency grounds.

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
- **Three-tier placement.** `liquers-core` keeps only the important basic types; `liquers-lib`
  carries the fuller set behind one or two features; carrier-specific types belong to the package
  that supports that carrier (Polars types under the `polars` feature, Python types in
  `liquers-py`, JavaScript types in `liquers-web`).

## Open Questions

1. Do purposes need to be a closed vocabulary in `liquers-core`, an open namespaced string space,
   or a registry that third parties extend at startup? Prior art favours the third.
2. Should the carrier (`native`, `json`, `javascript`, `python`, `polars`) be a separate field or a
   namespace prefix inside the identifier (`py:int`, `js:number`, `core:i32`)?
3. Where does the registry live at runtime — in `Environment`, or a process-global static?
4. Do the extended scalars go in as flat `ExtValue` variants, or behind an
   `ExtValue::Scalar(ExtScalar)` sub-enum? The sub-enum keeps the variant count and the
   `#[cfg]`-heavy `match` arms in `value/mod.rs` manageable.
5. Is `ext-scalars` / `ext-temporal` the right cut, or is one feature enough? `ext-scalars` needs
   no dependency at all, so gating it buys only a smaller enum; `ext-temporal` costs `rust_decimal`
   and `uuid`. Should either be in `default`?
6. When a build encounters a stored type identifier its features do not include, is that a hard
   error, or does the value degrade to `bytes` with the declared identifier preserved? The second
   keeps a minimal build able to *move* data it cannot interpret.
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
