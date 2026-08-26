---
id: FOREIGN-VALUE-TYPE-REGISTRATION-PHASE2
kind: design
title: "Phase 2: Architecture — foreign and Python value types in the type registry"
status: in_review
phase: architecture
area: [core/value, lib/value, web, py]
created: 2026-08-26
---
# Phase 2: Solution & Architecture — Foreign Value Type Registration

## Overview

Three additive changes and one repair. **(1)** Every environment gains a constructor that accepts a
finished `TypeRegistry`, so an integration extends the base registry and hands it over; the registry
is still written only before construction and needs no lock. **(2)** `ForeignValue` gains an instance
`type_info()` with a working default, routed through `ValueExtension` and `CombinedValue` so a
foreign value describes itself instead of falling back to a generic derivation; `liquers-web`
registers `js.Value` inside `new_environment()`, which every rebuild path already funnels through.
**(3)** `liquers-py`'s `Value` gains `type_descriptions()` and one identifier per variant, which
first requires repairing four compile errors in a file that has never been part of the crate.

No new struct, no new enum, no new `ExtValue` variant, no new dependency, no new command.

## Known-Issue Preflight

Searched: issues linked from `DESIGN.md` and Phase 1; every open (`draft`/`accepted`/`in_progress`)
record in `specs/index.csv` whose `area` includes `core/value`, `lib/value`, `web`, `py`,
`core/assets` or `core/store`; and the integration points themselves — `TypeRegistry`
construction, `validate_metadata_hard`, `ForeignValue`, `liquers-web`'s environment rebuild, and
`liquers-py`'s module declarations. 39 open records matched by area; the eight below are relevant.

| Issue | Status | Priority | Relevance and solution impact | First? | Blocking? | Required action | Priority action |
|---|---|---|---|---|---|---|---|
| `FOREIGN-VALUE-TYPES-NOT-REGISTERED` | in_progress | P1 | The subject. | — | no | Close in Phase 5 | Keep P1 |
| `PY-VALUE-TYPE-DESCRIPTIONS-MISSING` | in_progress | P2 | The subject's Python half. | — | no | Close in Phase 5 | Keep P2 |
| `PY-MODULES-NOT-DECLARED-IN-LIB` | draft | P2 | `liquers-py/src/value.rs` is not in the crate, so the Python half cannot be *verified* without declaring it, and declaring it exposes four compile errors. | **yes, in part** | no — scope absorbs it | Declare `value` and `context` and repair `value.rs` only; leave `commands.rs`, `store.rs`, `interpreter.rs`, `cache.rs`, `state.rs` undeclared | Keep P2 — the remainder is untouched |
| `POST-INIT-COMMAND-REGISTRATION` | accepted | P3 | Documents the rebuild-and-replay lifecycle the type registry must join. Its resolution ("build registry, then environment, then share — and simply do it again") is the same shape this design adopts, which is corroboration rather than conflict. | no | no | Register inside `new_environment()`, the funnel both rebuild paths already use, so there is nothing to retain or replay | Keep P3 |
| `WEB-VALUE04-BYTES-IDENTIFIER-CASE-MISMATCH` | draft | P2 | The `liquers-web` wasm suite is **red at HEAD** on one assertion (`Bytes` vs `bytes`), and this design must run that suite to verify `js.Value`. The issue asks which spelling is intended; the one-identifier-per-variant rule answers it — `Bytes` is the registered identifier, the lowercase spellings are read-side accommodations the write path refuses. | no | no | **In scope — confirmed by the user 2026-08-26**: fix the stale assertion (one line) so the suite is green and a real regression is visible | Keep P2 |
| `TYPE-REGISTRY-NOT-REALM-AWARE` | draft | P2 | The realm behaviour Phase 1 recorded as a forward constraint. | no | no | Do not obstruct: registrations are realm-nameable, `TypeInfo` stays serializable, the constructor accepts a registry assembled from anywhere | Keep P2 |
| `VALUE-TYPE-DEFINITION-MACRO` | draft | P2 | Would generate `identifier`/`type_descriptions` arms. Hand-written constants must not become an obstacle. | no | no | Keep every addition builder-constructed and generator-shaped, per `value-type-system`'s generator-alignment commitments | Keep P2 |
| `CORE-VALUE-INTERFACE-CAPABILITY-SPLIT` | accepted | P2 | Owns the eventual `identifier` → `type_identifier` rename. This design edits that method's **doc comment**, not its name. | no | no | Correct the doc comment; leave the name alone, as `value-type-system` also did | Keep P2 |

Discarded as not relevant despite an `area` match: `COMBINED-VALUE-DISCRIMINATION`,
`CORE-VALUE-ENUM-OVERSIZED`, `VALUE-CONVERSION-CAPABILITY`, `VALUE-DESCRIPTION`,
`DATA-FORMAT-CONSTANTS-AND-TOOLING` (data-format constants, not type-identifier constants),
`WORKSPACE-SERDE-DERIVE-UNDECLARED` (no serde derive is added here), and every `core/assets`
record — this design changes no asset-manager behaviour, only which identifiers pass a check that
already exists.

### Blocking and Priority Decision

**No unresolved blocker.** `PY-MODULES-NOT-DECLARED-IN-LIB` is the only prerequisite, and it is not
blocking because the part this design depends on is absorbed into its own scope (declare two
modules, repair one file) rather than waited on. That absorption is deliberate: the Python half is
otherwise unverifiable, and shipping an unverified `type_descriptions()` into a file nothing
compiles would be a change nobody could trust. No priority change is recommended for any issue.

## Data Structures

### No new structs, no new enums

`TypeRegistry`, `TypeKey` and `TypeInfo` (`liquers-core/src/type_system.rs`) are used exactly as
they are. `TypeRegistry` keeps `Debug + Clone + Default` and its `BTreeMap<TypeKey, TypeInfo>`; it
is written only before an environment is constructed, so it still needs no lock and no concurrent
map.

### One new enum variant, in `liquers-py` only

```rust
// liquers-py/src/value.rs
pub enum Value {
    // … existing variants …
    AssetInfo { value: Vec<crate::metadata::AssetInfo> },   // NEW
}
```

**Rationale:** forced by the trait, not by this design. `ValueInterface::from_asset_info` takes
`Vec<AssetInfo>` and returns `Self` — it cannot fail — and `liquers-py`'s implementation is
`todo!()`, which is a panic in library code. Mirroring `liquers-core`'s `Value::AssetInfo(Vec<_>)`
is the only shape that satisfies the signature and the one-identifier-per-variant rule.
`crate::metadata::AssetInfo` is the existing `#[pyclass]` wrapper (`liquers-py/src/metadata.rs:527`),
so the variant follows the established `Metadata { value: MetadataRecord }` pattern.

**No default match arm** on it: every `match self` in `liquers-py/src/value.rs` gains an explicit
arm. Several currently end in `_ =>`, which is how the missing variant went unnoticed; those are
narrowed where the addition makes them ambiguous, but converting every one of them is *not* in
scope — only the arms the new variant and the new identifiers require.

## Trait Implementations

### `ForeignValue` — one new method, with a working default

```rust
// liquers-lib/src/value/foreign.rs
pub trait ForeignValue: Debug + MaybeSend + MaybeSync + 'static {
    // … existing methods unchanged …

    /// This value's type description, for the type registry and the write path.
    ///
    /// The default derives everything from the methods above and declares **no** data formats,
    /// which is correct for a handle with no byte form: the write path exempts such a type from
    /// the format check exactly as it exempts a UI element. An implementation whose `as_bytes`
    /// does produce bytes overrides this and adds `.with_data_formats([…])`.
    fn type_info(&self) -> liquers_core::type_system::TypeInfo {
        liquers_core::type_system::TypeInfo::new(self.identifier())
            .with_type_name(self.type_name())
            .with_defaults(
                self.default_extension(),      // default_data_format derives from the extension
                self.default_extension(),
                self.default_media_type(),
                self.default_filename(),
            )
    }
}
```

**Object safety is preserved.** `type_info(&self)` takes `&self` and uses only `&self` methods, so
it lives in the vtable and `Arc<dyn ForeignValue>` still works. It is **not** an associated
function: `fn type_info() -> TypeInfo where Self: Sized` would also keep the trait object-safe, but
it could carry no useful default (a default body cannot reach `&self`), so it would force every
implementor to write one and would still not be callable through the trait object. The static side
is a free function in the integration crate instead — see below.

**Additive, so no implementor breaks.** This follows `CLAUDE.md`'s "add new methods with default
implementations when possible".

### `ValueExtension` — the same method, same default

```rust
// liquers-lib/src/value/extended.rs
pub trait ValueExtension: /* … */ {
    // … existing methods unchanged …
    fn type_info(&self) -> TypeInfo {
        // Same derivation as ValueInterface::type_info: search this extension's own descriptions
        // by identifier, else build one from this value's defaults.
    }
}
```

```rust
// liquers-lib/src/value/mod.rs — ExtValue overrides it for exactly one variant
impl ValueExtension for ExtValue {
    fn type_info(&self) -> TypeInfo {
        match self {
            ExtValue::Foreign { value } => value.type_info(),
            ExtValue::Image { .. } | ExtValue::UIElement { .. } => default_lookup(self),
            #[cfg(feature = "polars")]
            ExtValue::PolarsDataFrame { .. } => default_lookup(self),
            #[cfg(feature = "egui")]
            ExtValue::UiCommand { .. } | ExtValue::Widget { .. } => default_lookup(self),
        }
    }
}
```

Every arm explicit, `#[cfg]`-gated arms for `#[cfg]`-gated variants, no `_ =>`.

### `CombinedValue` — route `ValueInterface::type_info` to whichever side holds the value

```rust
// liquers-lib/src/value/extended.rs
impl<BaseValue: ValueInterface + Default, Ext: ValueExtension> ValueInterface
    for CombinedValue<BaseValue, Ext>
{
    fn type_info(&self) -> TypeInfo {
        match self {
            CombinedValue::Base(base) => base.type_info(),
            CombinedValue::Extended(ext) => ext.type_info(),
        }
    }
}
```

**Why this chain is needed and not merely tidy.** `ValueInterface::type_info`'s default searches
`Self::type_descriptions()` and, on a miss, builds a description from the value's own defaults with
`supported_data_formats` empty. For today's `JsOpaque` that produces the right answer by accident,
because `JsOpaque` genuinely supports no formats. For a foreign value that *can* serialize — a
Python object with a JSON form — the fallback would report no supported formats while the registry
reported some, so `value.supports_data_format("json")` would answer `false` against a registry that
says `true`. The routing is what makes `ForeignValue::type_info` reachable; without it the new
method would be dead API. **Behaviour for every existing type is unchanged**: delegating the lookup
to `ExtValue::type_descriptions()` finds `Image`, `polars.DataFrame` and the rest exactly as the
flat concatenation does.

### `liquers-py`'s `ValueInterface` impl — repair, then describe

Four compile errors and one gap, all in `liquers-py/src/value.rs` (verified 2026-08-26 by declaring
the module):

| Item | Now | Becomes |
|---|---|---|
| `try_into_query` | returns `crate::parse::Query` (the `#[pyclass]` wrapper) | returns `liquers_core::query::Query`; the `Value::Query` arm yields `value.0.clone()` |
| `from_asset_info` | `fn(AssetInfo) -> Self`, body `todo!()` | `fn(Vec<AssetInfo>) -> Self`, building `Value::AssetInfo` |
| `from_command_metadata` | missing | `Value::CommandMetadata { value: … }` |
| `try_into_bytes` | missing | `Value::Bytes` yields a clone; every other variant is a `conversion_error` |
| `try_into_key` | missing | `Value::Key` yields the inner key; `Value::Text` parses; others error |
| `try_into_command_metadata` | missing | `Value::CommandMetadata` yields the inner; others error |

No `todo!()`, no `unwrap()`, no `expect()` survives in the repaired file: `todo!()` is a panic on a
supported path once the module is compiled.

## Function Signatures

### `liquers-core` — five constructors, one shape

```rust
// liquers-core/src/context.rs
impl<V: ValueInterface> SimpleEnvironment<V> {
    /// Creates an environment whose registry describes exactly the types `V` describes.
    pub fn new() -> Self {
        Self::new_with_type_registry(TypeRegistry::from_value_type::<V>())
    }

    /// Creates an environment with a caller-supplied registry.
    ///
    /// For an integration that adds a type `V` cannot describe statically — a foreign language
    /// handle whose identifier belongs to the integration crate. Extend
    /// `TypeRegistry::from_value_type::<V>()` rather than starting from `TypeRegistry::new()`,
    /// or the build loses every type it already had, including `error`.
    pub fn new_with_type_registry(type_registry: TypeRegistry) -> Self { /* … */ }
}
```

Identically on `ImmediateEnvironment`, `SimpleEnvironmentWithPayload`,
`ImmediateEnvironmentWithPayload` (`liquers-core/src/context.rs`) and `DefaultEnvironment`
(`liquers-lib/src/environment.rs`). `new()` delegates in each case, so the field initialisation
stays in one place per type and `Default` is untouched.

**Naming.** `new_with_type_registry`, not `with_type_registry`: these types already use
`with_*(&mut self) -> &mut Self` *mutators* (`with_async_store`, `with_recipe_provider`,
`with_default_recipe_provider`), and a `with_`-named associated function among them would read as
another mutator — inviting exactly the post-construction mutation Phase 1 ruled out.

**`liquers-py` gets no such constructor.** Its `Environment::new` is a PyO3 `#[new]` and, more to
the point, it needs none: `Value::Py` is a variant of a value type `liquers-py` owns, so `py.Object`
goes in its **static** `type_descriptions()`. Only a type whose implementation lives in a
*different* crate from its value type needs the constructor — today, `js.Value` alone.

### `liquers-web` — a constant, a free function, and one line in `new_environment`

```rust
// liquers-web/src/value.rs, beside the existing ORIGIN_JAVASCRIPT
pub const JS_VALUE_TYPE_IDENTIFIER: &str = "js.Value";

/// The registry entry for a retained JavaScript value. The single construction site.
pub fn js_value_type_info() -> TypeInfo {
    TypeInfo::new(JS_VALUE_TYPE_IDENTIFIER)
        .with_type_name("JsValue")
        .with_defaults("json", "json", "application/json", "value.json")
    // No .with_data_formats: JsOpaque::as_bytes refuses, so the type has no byte form and the
    // write path's format check exempts it. Adding a format here would make set_binary accept
    // bytes that can never be materialized.
}

impl ForeignValue for JsOpaque {
    fn identifier(&self) -> Cow<'static, str> { JS_VALUE_TYPE_IDENTIFIER.into() }
    /// Refines the registered description with this instance's constructor name.
    fn type_info(&self) -> TypeInfo { js_value_type_info().with_type_name(self.type_name()) }
}
```

```rust
// liquers-web/src/environment.rs
pub fn new_environment() -> Result<WebEnvironment, Error> {
    let mut types = TypeRegistry::from_value_type::<Value>();
    types.register(crate::value::js_value_type_info())?;
    let mut env = WebEnvironment::new_with_type_registry(types);
    crate::builtins::register_builtin_commands(&mut env)?;
    env.with_default_recipe_provider();
    Ok(env)
}
```

**The replay problem solves itself.** `rebuild_with` and `rebuild_without`
(`liquers-web/src/environment.rs:254`, `:433`) both start from `new_environment()`, so a
registration made there is reconstructed on every rebuild with nothing retained and nothing to
drift. This is why the registration goes in `new_environment()` and **not** alongside
`REGISTERED_SPECS` or `STORE_CONFIG`: those retain *runtime-varying* declarations, and a type
identifier fixed by the build is not one.

**`type_name` differs between the two spellings, deliberately.** The registered entry says
`JsValue`; an instance says `Uint8Array` or whatever `constructor.name` gave. That is the
type-axis/`type_name` split the reference already states — `type_name` is informational and never
dispatched on, and `validate_required_fields` (`assets.rs:522`) requires only that it be non-empty.
`constructor_name` (`liquers-web/src/value.rs:130`) already filters the empty string.

### `liquers-py` — describe every variant

```rust
// liquers-py/src/value.rs
impl ValueInterface for Value {
    fn type_descriptions() -> Vec<TypeInfo> { /* one entry per variant, 16 entries */ }
    fn identifier(&self) -> Cow<'static, str> { /* one identifier per variant */ }
}

pub const PY_OBJECT_TYPE_IDENTIFIER: &str = "py.Object";
```

Identifier realignment, driven by the one-identifier-per-variant rule and by matching
`liquers-core::Value` so a store written from Python is readable from Rust:

| Variant(s) | Now | Becomes |
|---|---|---|
| `None`, `Bool`, `I32`, `I64`, `F64`, `Array` | all `generic` | `None`, `Bool`, `I32`, `I64`, `F64`, `Array` |
| `Text` | `text` | `Text` |
| `Object` | `dictionary` | `Object` |
| `Bytes` | `bytes` | `Bytes` |
| `Metadata`, `Recipe`, `CommandMetadata`, `Query`, `Key` | lowercase / `command_metadata` | `Metadata`, `Recipe`, `CommandMetadata`, `Query`, `Key` |
| `AssetInfo` (new) | — | `AssetInfo` |
| `Py` | `python_value` | `py.Object` |

`python_value` could not have been registered even if it had been described: `_` is a reserved
character and `identifier_naming_rule_holds` (`liquers-core/src/type_system.rs:438`) rejects it.

**No migration is needed**, and this is a fact rather than an assumption: `liquers-py/src/value.rs`
has never been part of the crate, so nothing was ever written to a store with the old identifiers.

### `liquers-core` — one doc comment

```rust
// liquers-core/src/value.rs:229
/// The type identifier of this value.
///
/// **Exactly one identifier per value variant** — the correspondence is one-to-one, and
/// `type_descriptions_match_identifier` enforces it ("one description per variant, no more and no
/// less"). Detail that varies per instance belongs in `type_name`, which is informational and is
/// never dispatched on. The identifier must be cross-platform.
fn identifier(&self) -> Cow<'static, str>;
```

Replaces "Several types can be linked to the same identifier", which contradicts the test 900 lines
below it in the same file. No rename — that belongs to `CORE-VALUE-INTERFACE-CAPABILITY-SPLIT`.

## Generic Parameters & Bounds

No new generic parameter and no new bound anywhere. `new_with_type_registry` is generic only in the
`V: ValueInterface` (and `P: PayloadType`) the type already carries; the registry argument is
concrete. `ForeignValue::type_info` adds no bound and no `where` clause, which is what keeps the
trait object-safe.

## Sync vs Async Decisions

| Function | Async? | Rationale |
|---|---|---|
| `TypeRegistry::register` | No | In-memory map insert; already sync |
| `*::new_with_type_registry` | No | Construction; mirrors `new()` |
| `ForeignValue::type_info` | No | Pure derivation from `&self` |
| `ValueExtension::type_info`, `CombinedValue::type_info` | No | Pure lookup |
| `liquers-py` `ValueInterface` repairs | No | Pure conversions |

Everything here is synchronous, and nothing touches an async path. The write-path check this design
unblocks (`validate_metadata_hard`) is itself sync and called from async `set_state`/`set_binary`;
that arrangement is unchanged.

## Integration Points

| Crate | File | Change |
|---|---|---|
| liquers-core | `src/context.rs` | Four `new_with_type_registry` constructors; `new()` delegates |
| liquers-core | `src/value.rs` | `identifier` doc comment |
| liquers-lib | `src/value/foreign.rs` | `ForeignValue::type_info` with default body |
| liquers-lib | `src/value/extended.rs` | `ValueExtension::type_info` default; `CombinedValue::type_info` routing |
| liquers-lib | `src/value/mod.rs` | `ExtValue::type_info` — delegate the `Foreign` arm |
| liquers-lib | `src/environment.rs` | `DefaultEnvironment::new_with_type_registry` |
| liquers-web | `src/value.rs` | `JS_VALUE_TYPE_IDENTIFIER`, `js_value_type_info`, two `impl` methods |
| liquers-web | `src/environment.rs` | Register inside `new_environment()` |
| liquers-web | `tests/second_value_type.rs` | *Proposed*: the stale `bytes`/`Bytes` assertion |
| liquers-py | `src/lib.rs` | `pub mod value; pub mod context;` |
| liquers-py | `src/value.rs` | Four repairs, `AssetInfo` variant, identifiers, `type_descriptions` |

**Dependency flow is respected**: `liquers-core` gains nothing pointing outward, `liquers-lib` names
no language, and `js.Value` is named only in `liquers-web`. No new dependency in any `Cargo.toml`.

## Error Handling

No new error type; `liquers_core::error::Error` throughout, with typed constructors only.

| Scenario | Constructor | Where |
|---|---|---|
| Registry already holds the identifier | `Error::general_error` (existing message in `TypeRegistry::register`) | `new_environment()` propagates with `?` |
| Identifier not registered on write | `Error::general_error` (existing) | `validate_metadata_hard` — unchanged, and still the behaviour for an unregistered type |
| A `liquers-py` conversion has no meaning for a variant | `Error::conversion_error(self.identifier(), "<target>")` | the repaired `try_into_*` methods |
| Parsing a `Query`/`Key` out of `Value::Text` fails | `Error::from_error(ErrorType::ParseError, e)` | as the existing `try_into_query` already does |

`new_environment()` returns `Result<_, Error>` already, so the registration's `?` needs no signature
change — a duplicate identifier surfaces as an environment-construction failure, which is where a
build-level mistake belongs.

## Serialization Strategy

Nothing new is serialized. `TypeInfo` and `TypeKey` keep the `Serialize`/`Deserialize` they already
derive; `TypeRegistry` deliberately gains **no** derive here — sharing a registry across realms
belongs to `TYPE-REGISTRY-NOT-REALM-AWARE`, and the shape it will want (a list of `TypeInfo`, each
carrying its own realm, rather than a map with a struct key) is a decision for that design.
`ExtValue` still derives only `Debug + Clone`.

## Concurrency Considerations

Unchanged, and that is the point of freezing the registry at construction: `TypeRegistry` is
immutable once the environment exists, so `get_type_registry(&self) -> &TypeRegistry` hands out a
shared reference with no lock, from any thread, exactly as today. `Arc<dyn ForeignValue>` keeps its
`MaybeSend`/`MaybeSync` bounds — `type_info(&self)` adds no bound that would disturb the wasm
relaxation that lets a never-`Send` `JsValue` live in the variant at all.

## Documentation Architecture

### Reference Plan

**Extend `specs/reference/VALUE_TYPE_SYSTEM.md`** (kind `reference`, audience `internal`, area
`core/value, lib/value`). Three changes:

1. Under **Two axes**, state the one-to-one rule explicitly — the table's "exactly one" cardinality
   currently implies it and the prose never says it.
2. A new subsection, **Registering a type known only to an integration**, after "Registered
   identifiers": how a registry is extended and passed to a constructor, and why it is frozen after.
3. Extend **Registered identifiers** with the foreign variant (`js.Value`, `liquers-web`) and
   `liquers-py`'s set including `py.Object`.

Plus a `## History` row and a `reviewed:` bump in the same commit (§9.2).

### Guide Plan

**Extend `specs/guides/LANGUAGE-INTEGRATION_GUIDE.md`** §VALUE, "Typing an integrated value": the
bullet that reads "**Registration is an open problem**" and points at this issue becomes the
procedure — constant, `type_info`, extend the registry, pass it to the constructor, and the unit
test that keeps the two spellings honest.

**Extend `specs/guides/TYPE_SYSTEM_GUIDE.md`** §2 ("Choose an identifier") with the cardinality rule,
and §4 ("Describe it") with the runtime-typed variant. The four steps stay four.

Both get a `## History` row and a `reviewed:` bump.

### Other Documents to Create

None. Phase 1's rationale stands: the fix is `M`-sized and its reasoning fits the Phase 5 summary.

### New Reference or Guide Documents

None.

### Existing Documents to Review or Update

| Path | In `affects_docs`? | Change |
|---|---|---|
| `specs/reference/VALUE_TYPE_SYSTEM.md` | **yes** | As above |
| `specs/guides/LANGUAGE-INTEGRATION_GUIDE.md` | **yes** | As above |
| `specs/guides/TYPE_SYSTEM_GUIDE.md` | **yes** | As above |
| `CLAUDE.md` | **yes** | "Adding a Value Type" — step 4 mentions the registry; add that an integration-owned type is registered at the environment constructor. One sentence; the four steps stay four |
| `specs/issues/FOREIGN-VALUE-TYPES-NOT-REGISTERED.md` | yes | → `closed` with evidence, Phase 5 |
| `specs/issues/PY-VALUE-TYPE-DESCRIPTIONS-MISSING.md` | yes | → `closed` with evidence, Phase 5 |
| `specs/issues/PY-MODULES-NOT-DECLARED-IN-LIB.md` | yes | Body records that `value` and `context` are now declared and `value.rs` repaired; stays open for the remaining six files |
| `specs/issues/WEB-VALUE04-BYTES-IDENTIFIER-CASE-MISMATCH.md` | yes | → `closed` in Phase 5, with the assertion fix as evidence and the one-identifier-per-variant rule as the authority for `Bytes` |
| `specs/README.md` | yes | Design-folder link; capability map |

Discarded candidates, recorded per §9: `specs/reference/ASSETS.md` and `ASSET_LIFECYCLE.md` (the
write path's behaviour is unchanged — only which identifiers pass it), `WEB_API_SPECIFICATION.md`
(no endpoint changes), `PROJECT_OVERVIEW.md` (no core concept changes; Query/Key encoding untouched).

### Design and Capability Links

`specs/README.md` gains the design folder while in progress, and after Phase 5 the capability is
reachable through `VALUE_TYPE_SYSTEM.md` and `LANGUAGE-INTEGRATION_GUIDE.md` rather than through
this folder. `LANGUAGE-INTEGRATION_GUIDE.md` §VALUE loses its pointer to the open issue and gains a
pointer to the reference subsection.

### Evidence to Collect During Implementation

- Whether PyO3 accepts `Vec<crate::metadata::AssetInfo>` in a `#[pyclass]` complex-enum variant, and
  what the error says if not — the one genuinely unproven assumption in this document.
- Which `liquers-py` `match` arms the new variant forces open, and whether any `_ =>` was hiding a
  second gap.
- Whether the `liquers-web` wasm loop goes green, and whether `js.Value` round-trips through
  `set_state` in a real browser rather than only in the native mock.
- Any place the registered `type_name` and the instance `type_name` diverging causes surprise —
  that split is intended, and a reader hitting it is a documentation signal.

## Relevant Commands

### New Commands

**None.** This design introduces no command and changes no command signature, so
`specs/command_registry.yaml` does not change and `cargo test -p liquers-lib --test registry_export`
should stay green untouched. That is itself a check worth running.

### Relevant Existing Namespaces

| Namespace | Relevance |
|---|---|
| — | No namespace is involved. The change is below the command layer entirely: a value's *type* is not addressed by any query |

**Decided 2026-08-26: no diagnostic command.** Phase 1 left "list the type identifiers this build
knows" as a candidate; it stays a candidate. This design remains at zero commands, so
`specs/command_registry.yaml` does not change. The idea belongs with `value-type-system`'s own
deferred open question rather than being smuggled in here.

## Web Endpoints

None. No route, no handler, no content-type mapping changes.

## Compilation Validation

| Check | Command |
|---|---|
| Core and lib, default features | `cargo test -p liquers-lib --lib --tests` |
| Feature matrix, library and test targets, plus wasm32 | `bash scripts/check-build-matrix.sh` |
| Python crate, with the two modules declared | `cargo check -p liquers-py --lib` |
| liquers-web conformance, under Node | `cargo test -p liquers-web --target wasm32-unknown-unknown --features debug-handles` |
| Command registry unchanged | `cargo test -p liquers-lib --test registry_export` |

`check-build-matrix.sh` is not optional here: `ExtValue::type_info` gains a `match` over variants
that are `#[cfg]`-gated three different ways, and a missing gated arm compiles fine under the
default features while breaking the minimal or wasm build.

**Expected friction, in likelihood order:** the PyO3 complex-enum variant (above); a
`#[cfg(feature)]` arm missed in `ExtValue::type_info`; and `liquers-py` `match` arms that the new
`AssetInfo` variant forces open beyond the `ValueInterface` impl itself.

## Cross-check against `liquers-patterns.md`

- [x] One-way crate dependency flow — nothing points backward; `js.Value` is named only in `liquers-web`
- [x] `ExtValue` extensions in `liquers-lib` only; no new variant there at all
- [x] Commands via `register_command!` — not applicable, no commands
- [x] `AsyncStore` pattern — not touched
- [x] Error handling uses typed constructors; no `Error::new`
- [x] No `unwrap()`/`expect()`; the one `todo!()` in scope is removed
- [x] Async default — everything added is genuinely sync and off any async path
- [x] No default match arm on a Liquers-owned enum; `#[cfg]`-gated variants get `#[cfg]`-gated arms
- [x] New trait methods carry default implementations, so no implementor breaks
- [x] Object safety preserved for `dyn ForeignValue`

## Review record

The workflow specifies two parallel reviewer agents and a fixer. This host does not spawn
subagents for this session, so the same passes were run sequentially against the codebase, as the
skill's host-compatibility section provides for. Findings and their resolutions:

**Pass A — Phase 1 conformity.** No scope drift found. Every Phase 1 decision is carried:
one-identifier-per-variant, extend-and-freeze, hard refusal, realms as a non-obstruction
constraint, constant-plus-test. Two Phase 1 items were *narrowed* on evidence, both recorded above:
`liquers-py` needs no new constructor (its foreign variant is statically describable), and the
"six constructors" of Phase 1 are five, because of that. One item was *widened*: the `liquers-py`
repair includes an `AssetInfo` variant, forced by a trait signature Phase 1 had not inspected.

**Pass B — Codebase alignment.** Five findings, all resolved in this document:
1. `ForeignValue::type_info` as a `Self: Sized` associated function would be dead weight — no
   useful default, unreachable through the trait object. → Instance method with a default body;
   the static side is a free function in the integration crate.
2. Adding `ForeignValue::type_info` without routing it would create API nothing calls, since
   `ValueInterface::type_info`'s default never consults it. → `ValueExtension` and `CombinedValue`
   routing added, with the divergence it prevents documented.
3. The `liquers-web` replay machinery (`REGISTERED_SPECS`, `STORE_CONFIG`) looked like the place to
   retain a registration; it is not. `new_environment()` is the funnel both rebuild paths use. →
   Registration sited there, and the reasoning recorded so nobody "fixes" it into the replay list.
4. `liquers-py`'s `from_asset_info` is `todo!()` against a trait signature that takes a `Vec` —
   a panic on a supported path the moment the module compiles. → New variant, mirroring core.
5. The `liquers-web` wasm suite is red at HEAD on an assertion this design's rule answers. →
   Raised as a scoped proposal rather than silently absorbed or silently ignored.
