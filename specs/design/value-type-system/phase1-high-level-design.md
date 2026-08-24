# Phase 1: High-Level Design - value-type-system

## Feature Name

Liquers value type system

## Purpose

Liquers describes a value with two independent metadata fields (`type_identifier`, `data_format`)
that nothing keeps consistent with the value or with each other, which is the silent-corruption bug
reported as `CORE-METADATA-FORMAT-TYPE-CONSISTENCY`. This project replaces the ad-hoc pair with an
explicit type model on two independent axes — **variant identity** and **principal data type** —
plus a registry that owns the facts about each type, so that type
information can serve its three real jobs at once: describing a value, driving serialization, and
(later) driving automatic conversion. It also settles the scalar model — the set is narrower than
every ecosystem Liquers exchanges values with — though the scalars themselves are implemented by
`VALUE-TYPE-DEFINITION-MACRO`, which generates them rather than hand-writing ~120 match arms.

## Core Interactions

### Query System
None. No query syntax changes; type identifiers are metadata, not query tokens.

### Store System
Read-back correctness: the effective `data_format` — never a read-time guess from the filename
extension — selects the deserializer, and the stored variant identity selects the value to
reconstruct. See "The encoding axis" below for how the effective format is arrived at.

### Command System
None. Declaring an argument by type identifier — the `// TODO: add support for value with
type_identifier` markers at `liquers-core/src/command_metadata.rs:73` and `:152` — was proposed
here and **moved out in Phase 2** to `COMMAND-METADATA-ENHANCEMENTS`, which already owns explicit
input/output typing. `ArgumentType` has 101 references across five crates, so a new variant is a
project of its own and is not what the P0 needs.

### Asset System
`AssetManager::set`/`set_state` validate the type/format pair before persisting; `AssetInfo` carries
the new fields so a client can tell what an asset is without fetching it.

### Value Types
`ValueInterface` grows the type-describing methods and loses none; `ExtValue`, `SimpleValue`,
`CombinedValue`, `ForeignValue`, the Python and JavaScript values all report into one registry.

**The scalar set widens — but in a later project.** The rule and the tiers below stand as the
agreed model; the *implementation* moved to `VALUE-TYPE-DEFINITION-MACRO` in Phase 2, because
fifteen scalars across eight exhaustive match sites is ~120 hand-written arms that a generator
would immediately delete. Scalars are defined **by their Rust type** — a Liquers scalar exists only
if Rust has it (or a canonical crate does) and at least five of the nine target systems represent
it distinctly (`./prior-art.md` §9). That rule excludes `f16`, `complex`, `char`, `isize`/`usize`,
and GlueSQL's `Inet`/`Point`. Where each scalar will live:

| Tier | Home | Contents |
|---|---|---|
| Core basics | `liquers-core::value::Value` | `none, bool, i32, i64, f64, text, bytes` — unchanged. These are what the query language, action parameters and command metadata actually need. |
| Extended scalars | `liquers-lib::value::ExtValue`, feature-gated — **deferred to `VALUE-TYPE-DEFINITION-MACRO`** | `i8, i16, i128, u8, u16, u32, u64, u128, f32`; `decimal, date, time, datetime, duration, uuid`. |
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
None. Selecting a renderer by purpose rather than by concrete variant is the motivating example for
the purpose axis, and moves with it into the conversion project.

## Why carrier is not an axis

The carrier (`native`, `json`, `javascript`, `python`, `polars`) was proposed as an axis and is
**not** one. A carrier always brings its own variant with its own identifier — `py:int`,
`js:number`, `i64` — so the carrier is *derivable from the identifier*. Two carriers sharing an
identifier would violate the uniqueness the identifier exists to provide, so the derivation can
never fail. That makes carrier a **projection** of the identifier space, expressed as a namespace
prefix, not an independent dimension.

Prior art agrees, and none of it carries a separate origin field: `com.adobe.pdf`, Arrow's
`arrow.json` extension name, a Kubernetes group/version/kind. The producer is inside the name.

**Variant placement carries no meaning either.** `CombinedValue::identifier()` delegates to
whichever side holds the value (`liquers-lib/src/value/extended.rs:136`), and
`ExtValue::Foreign` delegates onward to `ForeignValue::identifier()` (`value/mod.rs:135`). The
identifier space is therefore flat: whether a variant physically sits in `Value`, in `ExtValue`, or
behind a foreign handle is an implementation detail the type system neither sees nor should.

The one thing a carrier axis would have added is **recognising a native carrier** — "is this a
Liquers-native value or a foreign handle?". Every use for it reduces to a per-type capability the
registry answers better and more precisely: *can this serialize to format X?*, *can this leave the
process?*, *can this become JSON?* A value being foreign predicts none of those reliably — a
foreign value may serialize perfectly while a native one refuses. Dropped. If grouping by origin is
ever wanted, it is a derived query over the namespace prefix, not stored state.

## The encoding axis: two levels of seeding, then override

`data_format`, `media_type` and the filename extension are **one axis with two audiences**, not
separate type axes. The four type axes stand; these are its inward and outward faces.

- **`data_format` is inward.** It selects the codec (`State::as_bytes`, and every deserialization
  path — `assets.rs:492`, `:684`, `:3681`). Liquers-local vocabulary, refinable (`csv:comma` vs
  `csv:tab`, which `text/csv` cannot distinguish).
- **`media_type` is outward.** Across the whole workspace it drives exactly one decision:
  the HTTP `Content-Type` header at `liquers-axum/src/axum_integration.rs:52`. Its role is web
  communication, and **a user overriding it to influence the response is an intended capability**,
  not a mistake to be normalised away.

### Seeding cascade (write time)

Each level overwrites the previous one:

| Level | Source | Sets | Matters for |
|---|---|---|---|
| 1 | The value's own defaults — `default_data_format`, `default_extension`, `default_media_type` | all three | programmatically created assets, and queries with no filename |
| 2 | The filename extension | `data_format`, `media_type` | the common case: the extension decides the format |
| 3 | Explicit user override in metadata | any of them | deliberate control; in future through the context and through dedicated commands |

### Absent `data_format` is meaningful

`None` means **"no format was specified, so the value's own default (level 1) applies"** — it is a
distinguishable state, useful when reasoning about how a format came to be chosen, not a missing
value to be patched. Read-time resolution is therefore just: `Some(f)` → `f`; `None` → the value's
`default_data_format()`. That stays simple *because* seeding is guaranteed at write time — where a
filename exists, level 2 has already written the extension into `data_format`, so `None` can only
mean no filename was ever involved.

Today the guarantee does not hold, so `get_data_format()` (`metadata.rs:1239`) patches around it by
falling back to the extension and then to the constant `"bin"`. That `"bin"` is the level-1 slot
filled with a guess, because `MetadataRecord` cannot see the value. Under this design the fallback
chain disappears: level 1 is seeded from the value where both are in hand — the natural home is
`State::sync_metadata_with_value` (`state.rs:25`), which already syncs the type identifiers.

### Consequences

1. **`media_type` gains the same optionality `data_format` already has.** It is
   `media_type: String` with an empty-string sentinel (`metadata.rs:687`), while the setters
   (`set_filename`, `set_extension`) write it and `get_media_type()` re-derives it when empty. Make
   it `Option<String>`: `None` = derive from the effective `data_format`, `Some` = a deliberate
   level-3 override that must survive untouched.
2. **Rejection and soft warnings are two tiers, and both are kept.** "Reject" applies to the
   invariants whose violation makes a value unreadable — the P0 class. Divergences that are
   legitimate but worth surfacing stay as log entries and do not fail the write.

   | Tier | Check | Why this tier |
   |---|---|---|
   | **Hard — typed `Error`** | `type_identifier` empty | already enforced (`assets.rs:3159`) |
   | | `type_name` empty | already enforced (`assets.rs:3165`) |
   | | effective `data_format` unsupported for that `type_identifier` | **the P0 itself**: the bytes cannot be read back |
   | | malformed `media_type` (CR/LF, not a media type) | it reaches an HTTP response header |
   | **Soft — `LogEntry`** | filename extension ≠ effective `data_format` | legitimate; the declaration is authoritative and the filename may lag |
   | | declared `media_type` ≠ the one derived from `data_format` | *expected* whenever a level-3 override or a remote `Content-Type` is active |
   | | which seeding level supplied the effective format | provenance, at `Info` — pairs with absent-`data_format`-is-meaningful when reasoning about how a format got chosen |

   Soft warnings are the diagnostic layer this design most wants to keep: they are how a developer
   sees *that* an override is in play and *where* the format came from, which no amount of
   rejection reveals. Note `MetadataRecord::error()` (`metadata.rs:1180`) sets `Status::Error`, so
   advisory entries stay at `Warning` or below.

   Two refinements to the existing `add_soft_consistency_warnings` (`assets.rs:3173`):
   it compares extension against format with a plain `!=`, so a legitimate refinement
   (extension `csv`, format `csv:comma`) warns spuriously — the comparison belongs on the *base*
   format. And its media-type check must not become an error, or it breaks both intended
   overrides: `liquers-web/src/store/fetch.rs:91-100`, which substitutes the origin server's
   declared `Content-Type`, and any user shaping a web response. A *declared* level-3 value is
   consistent by definition; only an undeclared mismatch is worth a word.
3. **An override reaching an HTTP header needs validating, not restricting.** Since user control is
   intended, the guard is on the string's shape — a well-formed media type, no CR/LF — so the
   freedom cannot become header injection.

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

**Reference:** Create `specs/reference/VALUE_TYPE_SYSTEM.md` — the axes, the field
invariants, the registry contract, and the naming rules for identifiers. Audience: internal. It is
new rather than an extension because no current reference owns value typing; `PROJECT_OVERVIEW.md`
covers it in a paragraph and `ASSET_SET_OPERATION.md` only states requirements on `set()`.

**Guide:** Create `specs/guides/TYPE_SYSTEM_GUIDE.md` — how to add a value type, choose its
identifier, declare its purposes, and make it serializable. Audience: both. Written in Phase 5
against the implementation, since a guide claims present behaviour (`DOCS_STRUCTURE_GUIDE.md` §9).

**Other documents to create:** `prior-art.md` and `type-conversion-draft.md`, both done, in this
design folder. `prior-art.md` §1–8 researches UTI, Arrow, media types, MLflow flavors, Jupyter
MIME bundles, structural typing and clipboard negotiation; §9 inventories the scalar type systems
of Rust, JSON, Python, JavaScript, NumPy, Polars, Pandas, Arrow/Parquet and GlueSQL, read from each
project's defining source, and carries the **correspondence table** across all nine. The conversion
draft builds the conversion rules and mechanism on that table, and carries the purpose-axis
proposal; it is explicitly a draft for a **later** project, tracked by
`specs/issues/VALUE-CONVERSION-CAPABILITY.md`, not part of this one.

**Specific documents to update:** `specs/reference/PROJECT_OVERVIEW.md` (value/state/metadata
section), `specs/reference/ASSET_SET_OPERATION.md` (the mandatory-field rules it already asserts
but code does not enforce), `specs/reference/api/DOC_01_ARCHITECTURE_REFERENCE.md`, `specs/README.md`
(capability map), `CLAUDE.md` ("Adding a Value Type").

## Settled by the user (2026-08-18)

- **Backward compatibility is not a constraint.** Type identifiers are changed outright;
  no migration of stored data and no compatibility alias table. `Value::I32.identifier()` becomes
  `i32`, not `generic`.
- **The write path rejects — for the invariants that make a value unreadable.** Those are a typed
  `Error` at `set`/`set_state`, not a normalise-and-warn. **Soft warnings are kept alongside** for
  legitimate divergences: they are the diagnostic layer that shows an override is active and which
  seeding level chose the format. The two tiers are enumerated under "The encoding axis" below.
- **Scalars are grounded in Rust**, and the correspondence table across the nine ecosystems is a
  required artefact.
- **Three-tier placement.** `liquers-core` keeps only the important basic types; `liquers-lib`
  carries the fuller set behind one or two features; carrier-specific types belong to the package
  that supports that carrier (Polars types under the `polars` feature, Python types in
  `liquers-py`, JavaScript types in `liquers-web`).
- **The encoding axis has two seeding levels and an override**, `data_format` is inward and
  `media_type` outward, absent `data_format` means "use the value default", and user control of
  `media_type` to influence a web response is deliberate. See "The encoding axis" above.
- **The purpose axis is out of scope.** Purposes exist to answer "can this value be used as a
  table / an image / JSON?", which is the conversion and negotiation question. Carrying them here
  would define a vocabulary with no consumer. The proposal is written up in
  `./type-conversion-draft.md` and tracked by `specs/issues/VALUE-CONVERSION-CAPABILITY.md`; this
  project ships the three axes the P0 needs.
- **Carrier is not an axis.** A carrier always has its own variant, so it is a namespace prefix
  inside the identifier rather than an independent dimension; variant *placement* across `Value`,
  `ExtValue` and foreign handles carries no type-system meaning either. See "Why carrier is not an
  axis" below.
- **`media_type` derivation is already trusted.** It comes from
  `liquers-core::media_type::file_extension_to_media_type` (`media_type.rs:3`), a static 134-entry
  extension table in core. The earlier worry about a client-supplied media type reaching an HTTP
  header therefore only applies to an explicit level-3 override, and is handled by validating the
  string's shape rather than by restricting who may set it.

## Open Questions

1. ~~Does `principal data type` survive on its own?~~ **Resolved in Phase 2: no.** `type_name`
   already occupies that niche — it is the documented informational counterpart to `identifier`
   (`value.rs:194-197`), already a metadata field, and already synced from the value. A separate
   `data_type` would be a second answer to the same question, which is the failure mode this
   project exists to fix. The shipped model is one type axis plus the encoding axis.
2. Where does the registry live at runtime — in `Environment`, or a process-global static?
3. ~~Flat `ExtValue` variants or an `ExtValue::Scalar` sub-enum?~~ **Moot: the scalars moved to
   `VALUE-TYPE-DEFINITION-MACRO`**, where they are declared rather than written, so the question
   becomes the generator's.
4. ~~Is `ext-scalars` / `ext-temporal` the right cut?~~ Moot for the same reason.
5. ~~Hard error or degrade when a stored identifier is unknown to this build?~~ **Resolved in
   Phase 2: degrade**, keeping the bytes and metadata verbatim with a warning, so a minimal build
   can still move data it cannot interpret.
6. Where does level-3 override live once the context carries it — a metadata field the context
   writes, or a resolution the context performs at serialization time? Phase 1 records the
   intent; the mechanism is deliberately deferred.
7. Does an extension that disagrees with an explicitly declared `data_format` (`data.json` +
   `data_format: csv`) reject, or is the declaration simply authoritative and the filename
   cosmetic?
8. Does the guide need `docs/` (user-facing) coverage as well, or is `specs/guides/` enough?

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
