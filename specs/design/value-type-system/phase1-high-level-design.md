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
(later) driving automatic conversion.

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
No new variants.

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

**Other documents to create:** `prior-art.md` (done — research into UTI, Arrow, media types, MLflow
flavors, structural typing) and `type-conversion-draft.md` (Phase 2) in this design folder. The
conversion draft states which types convert to which and by what mechanism; it is explicitly a
draft for a **later** project, not part of this one.

**Specific documents to update:** `specs/reference/PROJECT_OVERVIEW.md` (value/state/metadata
section), `specs/reference/ASSET_SET_OPERATION.md` (the mandatory-field rules it already asserts
but code does not enforce), `specs/reference/api/DOC_01_ARCHITECTURE_REFERENCE.md`, `specs/README.md`
(capability map), `CLAUDE.md` ("Adding a Value Type").

## Open Questions

1. Is `type_identifier` allowed to change meaning? Today `Value::I32.identifier()` is `"generic"`
   while the deserializer branches on `"i32"` — they cannot both be right. Fixing it changes bytes
   already written into stores; is a migration or a compatibility alias table required?
2. Do purposes need to be a closed vocabulary in `liquers-core`, an open namespaced string space,
   or a registry that third parties extend at startup? Prior art favours the third.
3. Should the carrier (`native`, `json`, `javascript`, `python`, `polars`) be a separate field or a
   namespace prefix inside the identifier (`py:int`, `js:number`, `core:i32`)?
4. How strict is the write path — reject inconsistent metadata, or normalise and warn? The
   predecessor plan recommends a hybrid; this project must settle it.
5. Where does the registry live at runtime — in `Environment`, or a process-global static?
6. Does the guide need `docs/` (user-facing) coverage as well, or is `specs/guides/` enough?

## References

- `specs/issues/CORE-METADATA-FORMAT-TYPE-CONSISTENCY.md` — the P0 this project resolves
- `specs/design/metadata-consistency/{FINDINGS,PROPOSED_PLAN}.md` — predecessor investigation;
  this design supersedes its scope
- `specs/issues/CORE-VALUE-INTERFACE-CAPABILITY-SPLIT.md` — naming TODOs and the trait split that
  should follow this work
- `specs/issues/COMBINED-VALUE-DISCRIMINATION.md` — deserialization dispatch, directly enabled here
- `specs/issues/VALUE-DESCRIPTION.md` — the "describe a value" job, adjacent and overlapping
- `./prior-art.md` — Apple UTI, Apache Arrow, IANA media types, MLflow flavors, Jupyter MIME
  bundles, structural typing, clipboard format negotiation
