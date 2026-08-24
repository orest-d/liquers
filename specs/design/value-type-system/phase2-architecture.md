# Phase 2: Solution & Architecture - Liquers value type system

## Overview

One new module, `liquers-core/src/type_system.rs`, holds `TypeInfo` (the facts about one type) and
`TypeRegistry` (identifier-keyed lookup). Type facts reach the system through two complementary
surfaces: **instance methods on `ValueInterface`**, used everywhere a value is in hand, and the
**registry**, used only on the deserialization path where bytes and an identifier arrive without a
value. The metadata invariants are enforced at `AssetManager::set`/`set_state`, splitting into a
hard tier that rejects and a soft tier that logs. `liquers-lib` gains `ExtValue::Scalar(ExtScalar)`
behind two features.

The type model that ships is **one type axis** — variant identity (`type_identifier`) — alongside
the **encoding axis** (`data_format` inward, `media_type` outward, extension as a seeding source).
See "Resolved: the `data_type` axis" below for why the second type axis does not ship.

## Known-Issue Preflight

Searched: `specs/index.csv` for open (`draft`/`accepted`/`in_progress`) issues and features whose
`area` intersects `core/value`, `core/commands`, `lib/value`, `core/assets`, `core/store`, `macro`,
`py`, `web`, `axum` — 44 records. Also inspected everything linked from `DESIGN.md` and Phase 1.

| Issue | Status | Priority | Relevance and solution impact | Address first? | Blocking? | Required action | Priority action |
|---|---|---|---|---|---|---|---|
| `CORE-METADATA-FORMAT-TYPE-CONSISTENCY` | accepted | P0 | The issue this project resolves | — | no | Close in Phase 5 | Keep P0 |
| `CORE-LEGACY-METADATA-ACCESSORS-RETURN-JSON` | accepted | P2 | **`Metadata::get_data_format` legacy branch (`metadata.rs:1782`) returns `data_format.to_string()` on a `serde_json::Value`, so a legacy/partial document yields `"\"json\""` with quotes.** Our resolution rule reads that value, and reject-on-write would then refuse valid legacy assets with an unparseable format. The issue itself hands its deeper half — whether `MetadataRecord` should accept a partial document — to this project | **yes** | no (fixed inside this project) | Fold the accessor sweep in as step 0; decide the partial-document question here | **Recommend P1** — under reject-on-write it becomes user-visible breakage, not a cosmetic quoting bug |
| `COMBINED-VALUE-DEFAULT-EXTENSION-NOT-DELEGATED` | draft | P2 | `CombinedValue::default_extension` returns `"ext"` for every extended value (`extended.rs:150`), so level-1 seeding would seed a format no serializer implements | **yes** | no (fixed inside this project) | Fold the one-line delegation fix in; it is directly in the seeding path | Keep P2 |
| `COMBINED-VALUE-DISCRIMINATION` | accepted | P2 | Wants identifier-driven decode dispatch with deterministic fallback — exactly what `TypeRegistry` provides | no | no | This project supplies the mechanism; the issue closes or narrows to its test matrix | Keep P2 |
| `COMMAND-METADATA-ENHANCEMENTS` | accepted | P2 | Owns "explicit input/output type constraints in metadata" — the same ground as the `ArgumentType` change Phase 1 proposed. **`ArgumentType` has 101 references across 10 files including `liquers-py`, `liquers-web` and `liquers-macro`**; a new variant ripples through all of them | no | no | **Scope change: the `ArgumentType` work moves to this issue.** See "Scope removed" | Keep P2 |
| `CORE-VALUE-INTERFACE-CAPABILITY-SPLIT` | accepted | P2 | Owns renaming `identifier`→`type_identifier` and splitting the trait. We add methods (non-breaking) but do **not** rename (breaking, and it is that issue's job) | no | no | Add methods with defaults; leave naming alone and note the coordination | Keep P2 |
| `VALUE-DESCRIPTION` | accepted | P3 | `TypeInfo` is where a description hook would hang, and `type_name` already carries the runtime-detail role | no | no | Leave room in `TypeInfo`; do not implement | Keep P3 |
| `VALUE-CONVERSION-CAPABILITY` | draft | P2 | Downstream: owns purposes and conversion, filed by this design. Also owns automatic conversion at command-argument binding, which needs a compile-time Rust-type ↔ identifier correspondence | no | no | Define `TypeIdentified` now — see "Forward compatibility" | Keep P2 |
| `VALUE-TYPE-DEFINITION-MACRO` | draft | P2 | Would generate `ExtValue`, every trait impl and the registry entries from one declaration. **Bears directly on this project's scalar widening** — see "Sequencing question" | no | no | Keep `TypeInfo` builder-constructed and `TypeIdentified` derivable so a generator can emit them; put the scalar sequencing to the user | Keep P2 |
| `TYPE-REGISTRY-NOT-REALM-AWARE` | draft | P2 | A cross-realm query needs to know which types the *other* realm supports; a single-build registry cannot say. Filed during this phase | no | no | Key the registry by `TypeKey { realm, .. }` and give `TypeInfo` a builder — see "Forward compatibility" | Keep P2 |
| `CORE-MULTI-REALM-INTERPRETER` | accepted | P3 | Realm-aware *dispatch* (`plan.rs:1081`) must exist before realm-aware *typing* has anything to attach to | no | no | Nothing here; the realm-ready key costs nothing while dispatch is single-realm | Keep P3 |
| `WORKSPACE-SERDE-DERIVE-UNDECLARED` | accepted | P2 | `TypeInfo` will carry serde derives in `liquers-core`, which is one of the crates with an undeclared `derive` feature | no | no | Do not add a new undeclared use; monitor | Keep P2 |
| `CORE-STATE-LOCK-API-CLEANUP` | accepted | P3 | We extend `State::sync_metadata_with_value`; that issue may reshape `State` internals | no | no | Keep the change inside the existing helper so it moves with any refactor | Keep P3 |
| `CORE-METADATA-TRACEBACK-SUPPORT` | accepted | P2 | Adds a neighbouring metadata field; no interaction with type or format fields | no | no | Monitor | Keep P2 |

**No blockers.** The two "address first" items are small (`S`) and land inside this project as
preparatory steps rather than as external prerequisites, so Phase 2 approval is not held by an
unresolved blocker. Both are recorded in the Phase 4 step list.

## Resolved: the `data_type` axis does not ship

Phase 1 left this open. It is resolved as **drop**, on a stronger ground than "no consumer here":
**`type_name` already occupies the niche.** It is documented as the detailed, runtime-oriented,
informational counterpart to `identifier` (`liquers-core/src/value.rs:194-197`), it is already a
`MetadataRecord` and `AssetInfo` field, and it is already synced from the value
(`state.rs:25-28`). For a dynamically-typed carrier — a JSON document, a Python object registered
under one `python` identifier — `type_name` is precisely where "what this actually is at runtime"
lives. A separate `data_type` field would be a second, competing answer to the same question, and
this project exists because two fields answering overlapping questions drifted apart.

The information is not lost, only relocated: `py:int`, `i64` and `js:number` remain distinct
identifiers, and *the fact that all three are integers* is the correspondence table
(`prior-art.md` §9), which is the conversion project's input.

**Shipped model:** one type axis (`type_identifier`), with `type_name` as its informational
refinement, plus the encoding axis.

## Scope removed since Phase 1

- **`ArgumentType` / `ArgumentInfo` typing** moves to `COMMAND-METADATA-ENHANCEMENTS`, which
  already owns "explicit input/output type constraints in metadata". Measured cost: 101
  `ArgumentType` references across `liquers-core`, `liquers-macro`, `liquers-lib`, `liquers-py`
  and `liquers-web`; adding a Liquers-owned enum variant makes every one of those matches a
  compile error, and the no-`_ =>` rule means that is by design. That is a project of its own and
  it is not what the P0 needs. `area` drops `core/commands`.

## Data Structures

### `TypeInfo` — the facts about one type

```rust
// liquers-core/src/type_system.rs
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
pub struct TypeInfo {
    /// Unique, cross-platform variant identity. The serialization dispatch key.
    /// Namespaced by producer where the producer is not core: `py:int`, `js:number`, `pl:dataframe`.
    pub type_identifier: Cow<'static, str>,

    /// Detailed, runtime-oriented name. Informational; never a dispatch key.
    pub type_name: Cow<'static, str>,

    /// Level-1 seeding defaults. Mutually consistent by construction.
    pub default_data_format: Cow<'static, str>,
    pub default_extension: Cow<'static, str>,
    pub default_media_type: Cow<'static, str>,
    pub default_filename: Cow<'static, str>,

    /// Data formats this type can be written to and read from.
    /// `data_format` outside this set is the hard-tier rejection that closes the P0.
    pub supported_data_formats: Vec<Cow<'static, str>>,
}
```

**Ownership rationale.** `Cow<'static, str>` throughout, matching `ValueInterface`'s existing
return type, so a statically-known type costs no allocation while a foreign type registered at
runtime can still own its strings. `Vec` rather than `&'static [&str]` because a foreign value's
format list is a runtime fact.

**Construction is through a builder**, following the `MetadataRecord::with_*` /
`ArgumentInfo::with_*` convention already used across the codebase:

```rust
impl TypeInfo {
    pub fn new(type_identifier: impl Into<Cow<'static, str>>) -> Self;
    pub fn with_type_name(self, ..) -> Self;
    pub fn with_defaults(self, format: .., extension: .., media_type: .., filename: ..) -> Self;
    pub fn with_data_format(self, format: ..) -> Self;   // appends to supported_data_formats
    pub fn with_realm(self, realm: ..) -> Self;

    /// From a Rust type that names its own identifier.
    pub fn of<T: TypeIdentified>() -> Self { T::type_info() }
}
```

This is not ceremony: it is what lets a later field — the per-realm unsupported-type action of
`TYPE-REGISTRY-NOT-REALM-AWARE` — be added without breaking every construction site.

**Serialization.** Plain derives; `TypeInfo` is a description, not a value. It is exposed through
the web API so a client can discover what a build supports, and it is the shape a cross-realm
registry exchange would transfer.

### `TypeIdentified` — the Rust type ↔ identifier correspondence

```rust
pub trait TypeIdentified {
    const TYPE_IDENTIFIER: &'static str;
    fn type_info() -> TypeInfo;
}
```

**Immediate consumer:** `V::type_descriptions()` implementations are written as
`vec![TypeInfo::of::<PolarsFrame>(), TypeInfo::of::<DynamicImage>(), ..]` instead of hand-writing
each identifier string twice — once in `identifier()` and once in the description — which is a
duplication this project would otherwise create.

**Why it is defined now** (the forward-compatibility the user asked for). The eventual DSL form is

```
register_command!(cr, fn use_df(state, df: polars_dataframe) -> result)
```

where `polars_dataframe` is a **type identifier** defined in `liquers-lib` — or in a downstream
crate — while `liquers-macro` depends on neither. The apparent requirement is that the macro
resolve an identifier to a Rust type, which would need a data file both crates can read.

**It does not.** The Rust type comes from the command function's own signature, and the mechanism
already exists: `registration.rs:492` generates `let #var_name: #ty = arguments.get(#i, #name)?;`
where `arguments.get` is generic and `#ty` is a token the macro *forwards* without interpreting.
In the identifier form the annotation simply moves — the macro emits an unannotated binding and
the generated call to the user's function pins the type by inference, while the identifier travels
as ordinary data. The direction that must be resolvable is therefore **Rust type → identifier**,
which is what `TypeIdentified` provides, and which the compiler resolves at the definition site.

What this project must offer, and does: `TypeIdentified` for the resolvable direction, and a
registry that can be consulted **at registration time**, so an identifier no build registers
becomes a `CommandRegistryIssue` (`command_metadata.rs:427`, `:965`) rather than a runtime lookup
miss. A data export of the registry remains worth having for `liquers-validate` and non-Rust
clients — the `export-command-registry` pattern — but its consumer is tooling, not the macro.
Recorded in full on `VALUE-CONVERSION-CAPABILITY`.

### `TypeRegistry` — identifier-keyed lookup

```rust
/// Registry key. Mirrors `CommandKey { realm, namespace, name }` (`command_metadata.rs:561`),
/// including its `DEFAULT_REALM` → `""` normalization, so realms mean the same thing for types
/// as they already do for commands.
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct TypeKey {
    pub realm: String,
    pub type_identifier: String,
}

pub struct TypeRegistry {
    types: BTreeMap<TypeKey, TypeInfo>,
}

impl TypeRegistry {
    pub fn new() -> Self;

    /// Seed from a value type's static self-description, into the default realm.
    pub fn from_value_type<V: ValueInterface>() -> Self;

    /// Add one entry. A duplicate key is a typed error, never a silent overwrite.
    pub fn register(&mut self, info: TypeInfo) -> Result<(), Error>;

    /// Default-realm lookup — what every check in this project uses.
    pub fn get(&self, type_identifier: &str) -> Option<&TypeInfo>;
    pub fn contains(&self, type_identifier: &str) -> bool;

    /// Explicit-realm lookup. Single-realm today; the surface a cross-realm planner needs.
    pub fn get_in_realm(&self, realm: &str, type_identifier: &str) -> Option<&TypeInfo>;

    pub fn iter(&self) -> impl Iterator<Item = (&TypeKey, &TypeInfo)>;

    /// Is this format writable/readable for this type? The P0 check.
    pub fn supports_data_format(&self, type_identifier: &str, data_format: &str) -> bool;
}
```

`BTreeMap` rather than `scc`: the registry is built once and then read-only, so it needs no
concurrent-mutation support, and deterministic iteration order makes the web-API listing stable.
`register` returns `Result` so a duplicate identifier is a typed error — two crates claiming
`image` must fail, not resolve by load order.

### `ExtScalar` — the extended scalar set

```rust
// liquers-lib/src/value/mod.rs
pub enum ExtValue {
    // ... existing variants unchanged
    Scalar(ExtScalar),
}

#[derive(Debug, Clone, PartialEq)]
pub enum ExtScalar {
    #[cfg(feature = "ext-scalars")] I8(i8),
    #[cfg(feature = "ext-scalars")] I16(i16),
    #[cfg(feature = "ext-scalars")] I128(i128),
    #[cfg(feature = "ext-scalars")] U8(u8),
    #[cfg(feature = "ext-scalars")] U16(u16),
    #[cfg(feature = "ext-scalars")] U32(u32),
    #[cfg(feature = "ext-scalars")] U64(u64),
    #[cfg(feature = "ext-scalars")] U128(u128),
    #[cfg(feature = "ext-scalars")] F32(f32),
    #[cfg(feature = "ext-temporal")] Decimal(rust_decimal::Decimal),
    #[cfg(feature = "ext-temporal")] Date(chrono::NaiveDate),
    #[cfg(feature = "ext-temporal")] Time(chrono::NaiveTime),
    #[cfg(feature = "ext-temporal")] DateTime(chrono::DateTime<chrono::Utc>),
    #[cfg(feature = "ext-temporal")] Duration(chrono::TimeDelta),
    #[cfg(feature = "ext-temporal")] Uuid(uuid::Uuid),
}
```

**Rationale for the sub-enum** (Phase 1 open question 3): `ExtValue` is matched exhaustively in at
least eight places — `identifier`, `type_name`, `default_extension`, `default_filename`,
`default_media_type`, `as_bytes`, and the `ExtValueInterface` accessors — each already carrying
`#[cfg]` arms. Fifteen flat variants would add ~120 cfg-gated arms. One `Scalar(_)` arm per site
delegating to a single exhaustive `match` on `ExtScalar` keeps the cfg complexity in one file
section. **No default match arm** on either enum.

**Empty-enum hazard.** With both features off, `ExtScalar` has no variants. A variantless enum is
uninhabited and `ExtValue::Scalar(ExtScalar)` then cannot be constructed — which is correct, but
every `match` arm must still compile. The `ExtValue::Scalar` variant therefore carries the same
`#[cfg(any(feature = "ext-scalars", feature = "ext-temporal"))]` guard as its arms. This is the
build-matrix trap the rust-best-practices lens flags; Phase 4 tests the full matrix.

### Features

```toml
# liquers-lib/Cargo.toml
ext-scalars  = []                                        # widths only; no dependency
ext-temporal = ["dep:rust_decimal", "dep:uuid"]          # chrono is already non-optional
```

`ext-scalars` in `default`; `ext-temporal` not, because it adds two dependencies. `chrono` is
already a non-optional dependency of `liquers-core` (`Cargo.toml:55`) and `liquers-lib`
(`Cargo.toml:48`), so the temporal types cost nothing beyond `rust_decimal` and `uuid`.

## Trait Implementations

### `ValueInterface` — additive only

```rust
// liquers-core/src/value.rs
pub trait ValueInterface: /* ... unchanged bounds ... */ {
    // ... all existing methods unchanged ...

    /// Static self-description of every type this value type can hold.
    /// Default is empty: an implementor that does not describe itself registers nothing,
    /// which degrades to "unknown type" rather than failing to compile.
    fn type_descriptions() -> Vec<TypeInfo> { Vec::new() }

    /// Can *this* value be written in this format? Answered without a registry,
    /// so `State::as_bytes` and the State-level checks need no `Environment`.
    fn supports_data_format(&self, data_format: &str) -> bool;

    /// The effective type info for this value.
    fn type_info(&self) -> TypeInfo;
}
```

`type_descriptions` is an associated function (no `self`), so `CombinedValue` concatenates
`BaseValue::type_descriptions()` with `Ext::type_descriptions()`. Object safety is not a concern:
`ValueInterface` is used as an associated type bound (`Environment::Value`), never as `dyn`.

Defaults on all three would be ideal for non-breakage, but `supports_data_format` and `type_info`
cannot have honest defaults — a default `true` would silently defeat the P0 check. They are
required, and the three implementors outside core (`liquers-lib`, `liquers-py`, `liquers-web`)
implement them. **Names `identifier` and `type_name` are left alone**; renaming belongs to
`CORE-VALUE-INTERFACE-CAPABILITY-SPLIT`.

### `Environment` — one new method

```rust
// liquers-core/src/context.rs
pub trait Environment: /* ... */ {
    // ... existing ...
    fn get_type_registry(&self) -> &TypeRegistry;
}
```

Mirrors `get_command_metadata_registry` exactly. Four implementors: `SimpleEnvironment`
(`context.rs:1021`), `ImmediateEnvironment` (`context.rs:1141`), `DefaultEnvironment`
(`liquers-lib/src/environment.rs:94`), and `liquers-py`'s (`context.rs:82`). Each builds its
registry once at construction via `TypeRegistry::from_value_type::<Self::Value>()`, then extends it
with any foreign registrations.

**Why the registry is needed at all**, given the instance methods: deserialization has bytes and a
`type_identifier` but no value yet (`assets.rs:484-492`, `:3681`). That path is already generic over
`E: Environment`, so it can reach the registry. Every other check has a value in hand and uses the
instance methods.

## Metadata Changes

```rust
// liquers-core/src/metadata.rs — MetadataRecord and AssetInfo alike
pub media_type: Option<String>,   // was: String with an empty-string sentinel
```

`None` = derive from the effective `data_format`; `Some` = a deliberate level-3 override that is
preserved verbatim and never re-derived. `data_format: Option<String>` is unchanged — its `None`
already means "unspecified, use the value default", which is the level-1 fall-through.

```rust
impl MetadataRecord {
    /// `Some(f)` → f. `None` → the caller supplies the value default (level 1).
    /// No extension fallback and no `"bin"` constant.
    pub fn declared_data_format(&self) -> Option<&str>;

    /// Full resolution, given the value's own default.
    pub fn effective_data_format(&self, value_default: &str) -> String;

    /// `Some` override verbatim, else derived from the effective format.
    pub fn effective_media_type(&self, value_default_format: &str) -> String;
}
```

`get_data_format()` (`metadata.rs:1239`) and `get_media_type()` (`:1226`) keep their names and
signatures during migration but lose the extension-and-`"bin"` fallback chain; the level-1 answer
now arrives from the value rather than from a constant. The `Metadata::LegacyMetadata` branches of
both are rewritten to extract with `as_str()`, which is the
`CORE-LEGACY-METADATA-ACCESSORS-RETURN-JSON` sweep.

**Partial-document decision** (handed to this project by that issue): `MetadataRecord` gains
`#[serde(default)]` on every field that has a sensible default, so `{"media_type":"text/plain"}`
deserializes into a record instead of dropping to the legacy branch. This removes the trap rather
than the symptom. `Metadata::from_json`'s legacy fallback stays, but stops being the common path.

## Function Signatures

Collected for reference; each is specified in context in the section named after it.

| Signature | Home | Section |
|---|---|---|
| `TypeRegistry::from_value_type<V: ValueInterface>() -> Self` | `type_system.rs` | Data Structures |
| `TypeRegistry::register(&mut self, TypeInfo) -> Result<(), Error>` | `type_system.rs` | Data Structures |
| `TypeRegistry::get(&self, &str) -> Option<&TypeInfo>` | `type_system.rs` | Data Structures |
| `TypeRegistry::supports_data_format(&self, &str, &str) -> bool` | `type_system.rs` | Data Structures |
| `ValueInterface::type_descriptions() -> Vec<TypeInfo>` | `value.rs` | Trait Implementations |
| `ValueInterface::supports_data_format(&self, &str) -> bool` | `value.rs` | Trait Implementations |
| `ValueInterface::type_info(&self) -> TypeInfo` | `value.rs` | Trait Implementations |
| `Environment::get_type_registry(&self) -> &TypeRegistry` | `context.rs` | Trait Implementations |
| `MetadataRecord::declared_data_format(&self) -> Option<&str>` | `metadata.rs` | Metadata Changes |
| `MetadataRecord::effective_data_format(&self, &str) -> String` | `metadata.rs` | Metadata Changes |
| `MetadataRecord::effective_media_type(&self, &str) -> String` | `metadata.rs` | Metadata Changes |
| `validate_metadata_hard(&MetadataRecord, &TypeRegistry, &Key) -> Result<(), Error>` | `assets.rs` | Where the invariants are enforced |
| `add_soft_consistency_warnings(&mut MetadataRecord, &TypeRegistry)` | `assets.rs` | Where the invariants are enforced |
| `check_metadata(&mut Metadata, &TypeRegistry, &Key) -> Result<(), Error>` | `assets.rs` | Where the invariants are enforced |
| `deserialize_stored_value<E>(&[u8], &str, &str, &TypeRegistry) -> Result<DeserializedValue<E::Value>, Error>` | `assets.rs` | Where the invariants are enforced |

## Where the invariants are enforced

### Level-1 seeding — `State`

```rust
// liquers-core/src/state.rs — extends the existing private helper
fn sync_metadata_with_value(metadata: &mut Metadata, value: &V) {
    // existing: type_identifier, type_name
    // added:    seed data_format / extension / media_type from the value's TypeInfo
    //           **only where they are not already set**
}
```

Keeping this inside the existing helper means every constructor that already calls it —
`new`, `from_value_and_metadata`, `with_metadata`, `with_data`, `from_error` — gets seeding for
free, and it moves as one unit if `CORE-STATE-LOCK-API-CLEANUP` reshapes `State`.

### The two tiers — `AssetManager::set` / `set_state`

**Codebase-alignment finding: the existing check is duplicated four times.**
`add_soft_consistency_warnings` is a *nested local function* declared separately at
`assets.rs:3173`, `:3280`, `:4767` and `:4906` — once per `set`/`set_state` on each of the two
manager implementations — and in **two different signatures**
(`&mut MetadataRecord` versus `&mut Metadata -> Result<(), Error>`). Patching the tier logic in
place would mean making the same change four times, which is how it drifted in the first place.

**They are hoisted to one module-level pair before any behaviour change**, and all four sites call
them:

```rust
// liquers-core/src/assets.rs — module level, not nested
fn validate_metadata_hard(
    metadata: &MetadataRecord, registry: &TypeRegistry, key: &Key,
) -> Result<(), Error>;

fn add_soft_consistency_warnings(metadata: &mut MetadataRecord, registry: &TypeRegistry);

/// Adapter for the two sites that hold a `Metadata` enum rather than a record.
fn check_metadata(
    metadata: &mut Metadata, registry: &TypeRegistry, key: &Key,
) -> Result<(), Error>;
```

| Tier | Check | Error / entry |
|---|---|---|
| Hard | `type_identifier` empty | existing, `Error::general_error` |
| Hard | `type_name` empty | existing, `Error::general_error` |
| Hard | `type_identifier` not in the registry | `Error::general_error` naming the identifier and that this build does not know it |
| Hard | effective `data_format` not in `supported_data_formats` | **the P0**: `Error::from_error(ErrorType::SerializationError, ..)` naming type, format, and the supported set |
| Hard | `Some(media_type)` malformed — CR, LF, or not `type/subtype` | `Error::general_error`; it reaches an HTTP header |
| Soft | extension ≠ **base** of the effective `data_format` | `LogEntry::warning` |
| Soft | `Some(media_type)` ≠ the derived one | `LogEntry::warning` — expected under an override |
| Soft | which seeding level supplied the format | `LogEntry::info` |

The extension comparison is on the **base** format, so `data.csv` with `csv:comma` does not warn —
today's plain `!=` (`assets.rs:3176`) warns spuriously. Base extraction splits on the first `:`.

All constructors are typed (`Error::general_error`, `Error::from_error`); **`Error::new` is not
used**, and no new error type is introduced.

### Read path

**Codebase-alignment finding: the read path cannot reach the registry as designed.**
`deserialize_stored_value<E: Environment>(binary, type_identifier, data_format)`
(`assets.rs:481`) carries `E` only as a *type* parameter — it holds no `&E` and no `EnvRef`, so it
cannot call `env.get_type_registry()`. The registry is therefore passed in:

```rust
fn deserialize_stored_value<E: Environment>(
    binary: &[u8],
    type_identifier: &str,
    data_format: &str,
    registry: &TypeRegistry,          // added
) -> Result<DeserializedValue<E::Value>, Error>;

/// Distinguishes "materialized" from "kept as bytes because this build does not know the type".
pub enum DeserializedValue<V> {
    Value(V),
    Undeserialized { type_identifier: String },
}
```

Both call sites can supply it: `AssetData::try_fast_track` (`assets.rs:654`) holds
`envref: EnvRef<E>` as a field, and `AssetManager::get_any_status`'s store-fallback path is a
manager method with the same access.

With the registry in hand it dispatches: 

| Situation | Behaviour |
|---|---|
| identifier known, format supported | deserialize normally |
| identifier known, format unsupported | `Error::from_error(ErrorType::SerializationError, ..)` naming both |
| **identifier not registered in this build** | **degrade**: returns `Undeserialized`; the asset keeps its bytes and its metadata verbatim, with a `LogEntry::warning` naming the unregistered identifier. Asking for a *value* then fails with a named error |

The degrade rule (Phase 1 open question 5) lets a minimal build copy, proxy and re-store data it
cannot interpret. Re-persisting is safe because that path already takes the bytes from
`poll_binary` without re-serializing, so the untouched metadata is written back unchanged and the
hard tier never sees a value it would have to reject.

## Integration Points

| Crate | Change |
|---|---|
| `liquers-core` | new `type_system.rs`; `value.rs` (3 trait methods + `Value::type_descriptions`); `metadata.rs` (`media_type: Option`, resolution methods, legacy `as_str()` sweep, `#[serde(default)]`); `state.rs` (seeding in the existing helper); `assets.rs` (**hoist the four duplicated checks to one pair first**, then the two tiers; `deserialize_stored_value` gains a registry parameter and a `DeserializedValue` return); `context.rs` (`Environment::get_type_registry` + 2 impls) |
| `liquers-store` | none |
| `liquers-lib` | `value/mod.rs` (`ExtScalar`, `Scalar` variant, delegating arms); `value/extended.rs` (implement the 3 methods; **fix `default_extension` delegation**); `value/simple.rs`; `value/foreign.rs` (`ForeignValue` gains `type_info`); `environment.rs` (registry construction); `Cargo.toml` (2 features, 2 deps) |
| `liquers-axum` | `axum_integration.rs:52` reads `effective_media_type` instead of `get_media_type` |
| `liquers-py` | implement the 3 `ValueInterface` methods, `get_type_registry`; `metadata.rs` accessors follow the `Option<String>` media type |
| `liquers-web` | same; `store/fetch.rs:96-101` sets the level-3 override explicitly rather than relying on empty-string detection |

## Sync vs Async

Everything here is synchronous: the registry is an in-memory read-only map, and validation is pure
computation on metadata. No I/O is introduced, so no `async_trait` and no blocking-in-async risk.
The async call sites (`AssetManager::set`, `set_state`, the load path) call synchronous helpers,
which is what they already do for the existing checks.

## Relevant Commands

**No new commands.** The P0 is a library invariant, not a query-language capability, and every
check runs on paths that already execute. Relevant existing namespaces are unaffected: `pl`
(Polars), `img` (image), `lui`/`egui` (UI) — their values gain `TypeInfo` registrations but no
signature changes.

*Question for the user before this phase is finalized:* is a diagnostic command worth adding — one
that reports the registered types of the running build, so a query can answer "does this deployment
understand `datetime`?" It is cheap and it pairs with the degrade-on-read rule, but it is not
needed for the P0 and it is a `lib/commands` addition rather than a `core/value` one.

## Documentation Architecture

| Path | Kind | Audience | Area | Change |
|---|---|---|---|---|
| `specs/reference/VALUE_TYPE_SYSTEM.md` | reference | internal | `core/value` | **New** (Phase 5). The type axis and the encoding axis; `TypeInfo`/`TypeRegistry` contract; identifier naming and namespacing rules; the two-level seeding cascade; the hard/soft tier table; the degrade rule |
| `specs/guides/TYPE_SYSTEM_GUIDE.md` | guide | both | `core/value`, `lib/value` | **New** (Phase 5). How to add a value type: choose an identifier, write `TypeInfo`, declare supported formats, register it, verify with a round-trip test |
| `specs/reference/PROJECT_OVERVIEW.md` | reference | internal | multiple | Update the value/state/metadata section; link the new reference. `## History` row + `reviewed:` bump |
| `specs/reference/ASSET_SET_OPERATION.md` | reference | internal | `core/assets` | Update: it already asserts mandatory `data_format` and `type_identifier` on `set()`; state which tier each check is in. `## History` row + `reviewed:` bump |
| `specs/reference/api/DOC_01_ARCHITECTURE_REFERENCE.md` | reference | both | multiple | Update the value-type description |
| `specs/README.md` | capability map | — | `docs` | Add the new reference and guide; move this design to `complete` |
| `CLAUDE.md` | guide | both | `docs` | Rewrite "Adding a Value Type" — the three steps are now four, with registration |

**Proposed authoritative `affects_docs`:** `[reference/PROJECT_OVERVIEW.md,
reference/ASSET_SET_OPERATION.md, reference/api/DOC_01_ARCHITECTURE_REFERENCE.md,
reference/VALUE_TYPE_SYSTEM.md, guides/TYPE_SYSTEM_GUIDE.md]`.

Candidates generated by `area` overlap and **discarded**: `reference/ASSETS.md` and
`reference/ASSET_LIFECYCLE.md` (describe lifecycle and status, not typing — the `set()` rules they
would touch live in `ASSET_SET_OPERATION.md`); `reference/POLARS_COMMAND_LIBRARY.md` and
`reference/IMAGE_COMMAND_LIBRARY.md` (command catalogues; their values gain registrations but no
documented behaviour changes); `reference/WEB_API_SPECIFICATION.md` (kept under review — if the
type listing is exposed over HTTP it moves into the set in Phase 5).

## Error Handling

Every error uses `liquers_core::error::Error` with a typed constructor; **`Error::new` is not used
anywhere in this design**, and no new error type is added to `ErrorType`.

| Condition | Constructor | `ErrorType` |
|---|---|---|
| `type_identifier` / `type_name` empty | `Error::general_error(..)` | `General` |
| `type_identifier` not registered in this build | `Error::general_error(..)`, naming the identifier | `General` |
| `data_format` not supported for the type (**the P0**) | `Error::from_error(ErrorType::SerializationError, ..)`, naming type, format and the supported set | `SerializationError` |
| malformed `media_type` override | `Error::general_error(..)` | `General` |
| duplicate `TypeRegistry::register` | `Error::general_error(..)`, naming the identifier and both claimants | `General` |
| deserializing an unregistered identifier | not an error — degrade with a `LogEntry::warning`; a later value request fails with the "not registered" error above | — |

`.with_key(key)` is attached on the `set`/`set_state` paths, as the existing checks already do
(`assets.rs:3162`). `ErrorType::ConversionError` is deliberately **not** used: nothing here
converts a value, and the `foreign.rs` doc comment already assigns `ConversionError` to structural
conversion refusals and `SerializationError` to the byte boundary — this design keeps that split.

No `unwrap()` or `expect()` appears in any signature or described path; every fallible step returns
`Result<_, Error>`.

## Forward compatibility

Two future capabilities were raised during this phase. Neither is implemented here; both would be
expensive to retrofit, so the shapes they need are established now at near-zero cost.

| Future capability | Tracked by | What Phase 2 does about it | Cost now |
|---|---|---|---|
| `register_command!` declares an argument by type identifier — `fn use_df(state, df: polars_dataframe)` — and the framework converts the value to the Rust type the signature carries | `VALUE-CONVERSION-CAPABILITY` (declaration half in `COMMAND-METADATA-ENHANCEMENTS`) | Defines `TypeIdentified` for the resolvable direction (Rust type → identifier), `TypeInfo::of::<T>()`, and a registry consultable at registration time so an unknown identifier is a `CommandRegistryIssue`. The macro needs no data file and no `liquers-lib` dependency: it forwards the identifier as data and lets inference at the generated call site supply the Rust type | None — `TypeIdentified` has an immediate consumer in `type_descriptions()` |
| A query spanning a `wasm` frontend and a native backend, whose realms support different type sets, converting values transparently at the boundary | `TYPE-REGISTRY-NOT-REALM-AWARE` | Keys the registry by `TypeKey { realm, type_identifier }` mirroring `CommandKey`, with `get`/`contains` defaulting to the default realm; gives `TypeInfo` a builder so the per-realm unsupported-type action is an additive field | One extra struct and a default-realm convenience layer |

### Generation, not a data file

`VALUE-TYPE-DEFINITION-MACRO` supersedes the shared-data-file question entirely, and by a mechanism
worth recording because it is not obvious: proc-macros hold no reliable state between invocations,
so `register_command!` can never *read* what a type-defining macro declared. The channel is
**generated code** — the type-defining macro emits a module of aliases and constants named after
each identifier, and `register_command!` expands an identifier into a path into that module, which
ordinary name resolution resolves at the definition site. That covers a downstream crate's own
types, which no file shipped with Liquers can.

This project stays compatible with that future by construction: `TypeInfo` is builder-constructed
rather than a struct literal, and `TypeIdentified` is a plain trait — both are things a generator
can emit without this design changing.

**Deliberately not done now:** the unsupported-type *action* enum. An enum whose variants are not
implemented is worse than an absent field — it invites callers to match on behaviour that does not
exist. The builder is what makes adding it later non-breaking, and that is sufficient readiness.

Both extension points are single-realm and single-purpose in this project: `get` and `contains`
resolve in the default realm, and nothing consults `TypeIdentified` except description construction.
No behaviour is written that a later project would have to undo.

## Sequencing question: the scalars and the generator

`VALUE-TYPE-DEFINITION-MACRO` and this project's scalar widening collide, and the collision is
worth deciding rather than discovering.

The scalar tier is the generator's ideal first customer: fifteen scalars across roughly eight
exhaustive match sites is on the order of **120 mechanical, cfg-gated match arms** — precisely the
code the macro exists to remove, and precisely the code that produced
`COMBINED-VALUE-DEFAULT-EXTENSION-NOT-DELEGATED`.

| Option | What happens | Cost |
|---|---|---|
| **A — hand-write now** (as Phase 2 currently specifies) | `ExtScalar` and its arms are written by hand; the generator deletes them later | ~120 arms written, reviewed and then thrown away; one more chance for a silent divergent arm before the mechanism that prevents them exists |
| **B — split** | This project ships the P0 fix, `TypeInfo`/`TypeRegistry` and the metadata invariants only. The scalar widening moves to `VALUE-TYPE-DEFINITION-MACRO` and is declared through the generator | The P0 lands sooner and stays `M`-sized; the scalars wait on an `L` project |
| **C — generator first** | Build the macro inside this project, declare the scalars through it | This project becomes `L`/`XL` and the P0 — an accepted `P0` — waits on a code generator |

**Recommendation: B.** The P0 is the accepted priority and does not need a single new scalar to be
fixed; the scalar set is a *capability* addition that happens to share an area. Splitting keeps a
P0 fix small and reviewable, and lets the fifteen scalars arrive as fifteen declaration lines
instead of 120 arms. Option C inverts the priorities, and A knowingly writes code to delete it.

If B is chosen, this project's Phase 2 drops the `ExtScalar` section and the two features; nothing
else changes, since no other part of the design depends on the scalars.

## Open questions for the user

1. **Diagnostic command** — see "Commands" above.
2. **`ext-temporal` in `default`?** Two dependencies (`rust_decimal`, `uuid`) against a scalar set
   that half the target ecosystems represent natively. Recommendation: not in `default`, so a wasm
   build stays lean.
3. **Level-3 override mechanism** stays deferred (Phase 1 recorded the intent). The `Option<String>`
   media type is the storage; whether the context writes it or resolves it at serialization time is
   the conversion-adjacent question this project does not answer.
