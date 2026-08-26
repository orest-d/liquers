---
title: Value Type System
kind: reference
audience: internal
area: [core/value, lib/value]
reviewed: 2026-08-26
---

# Value Type System

How Liquers says what a value *is*, and how that is kept true.

## Two axes

A value is described on two independent axes. Collapsing them is what produced
`CORE-METADATA-FORMAT-TYPE-CONSISTENCY`.

| Axis | Field | Cardinality | Answers |
|---|---|---|---|
| **Type** | `type_identifier` | exactly one | Which value variant is this? |
| **Encoding** | `data_format` (inward), `media_type` (outward) | one per serialized copy | How are these bytes written, and what is the world told they are? |

**One identifier, one variant.** The correspondence is one-to-one in both directions: a variant has
exactly one identifier, and an identifier names exactly one variant. `type_descriptions_match_identifier`
(`liquers-core/src/value.rs`) enforces it — "one description per variant, no more and no less" — and
it is what makes "which variant is this?" answerable from a stored string.

`type_name` refines the type axis and is where everything that varies *within* a variant goes. It is
informational — a runtime-oriented detail such as `i64`, a Python class name, or the JavaScript
`constructor.name` of a retained object — and is **never** a dispatch key. A `js.Value` reports that
one identifier and a different `type_name` per instance; that asymmetry is the split working, not a
leak.

Two further axes were considered and deliberately do not exist:

- **Carrier** (`native`, `python`, `javascript`) is not an axis. A carrier always brings its own
  variant with its own identifier, so it is derivable from the identifier as a namespace prefix.
  No prior art carries a separate origin field either — `com.adobe.pdf`, `arrow.json`, a Kubernetes
  group all put the producer inside the name.
- **Purpose** ("can this be used as a table?") belongs with conversion, because a purpose vocabulary
  without conversion has no consumer: a caller could ask whether a value is a `table` and still have
  no way to obtain one. See `specs/issues/VALUE-CONVERSION-CAPABILITY.md`.

## Type identifiers

**Form: `provider.LocalName`, or a bare `LocalName`.**

| Rule | |
|---|---|
| Separator | exactly one `.`, both parts alphanumeric |
| Provider | lowercase, naming the system the type belongs to: `polars`, `js`, `py`, `egui` |
| Local name | the **Liquers concept name** in CamelCase — normally the value variant's name, not the backing Rust struct's |
| Reserved | every other non-alphanumeric character. `:` already means a data-format refinement; the rest are kept free for future structure such as generics |
| Bare names | reserved permanently for `liquers-core` and `liquers-lib` |

**A bare name asserts that Liquers owns the concept** — that this is the canonical type Liquers
commits to and converts others into. The test is semantic, not locational: `Image`'s payload is
`image::DynamicImage`, from a third-party crate, and it is still bare, because Liquers owns the
*meaning* of an image and the crate is an implementation detail. A provider prefix says the
opposite — Liquers is exposing somebody else's type, and the provider is part of what identifies it.

- Liquers commits to a canonical raster image → **`Image`**.
- Liquers explicitly refuses a canonical dataframe — polars and pandas, eager and lazy, arrow — so
  there is no bare `DataFrame`, and **`polars.DataFrame`** is right.

**The bare set is closed and enumerated below**, and `identifier_naming_rule_holds`
(`liquers-core/src/type_system.rs`) asserts the syntax mechanically. Adding a bare name is a
reviewed change to this document, not a decision made while adding a type. The asymmetry justifies
the ceremony: baring a name later is free — add a shorthand — while un-baring one is a breaking
rename of stored data. **When unsure, prefix.**

### Registered identifiers

Core (`liquers-core::value::Value`, mirrored by `liquers-lib`'s `SimpleValue`):

`None`, `Bool`, `I32`, `I64`, `F64`, `Text`, `Array`, `Object`, `Bytes`, `Metadata`, `AssetInfo`,
`Recipe`, `CommandMetadata`, `Query`, `Key`.

Library (`liquers-lib::value::ExtValue`): `Image`, `UIElement`, `polars.DataFrame` (feature
`polars`), `egui.Command` and `egui.Widget` (feature `egui`).

Integration-owned, registered at environment construction rather than described statically:
`js.Value` (`liquers-web`). `liquers-py`'s value type mirrors the core identifiers and adds
`py.Object`.

**There is no `error` identifier**, and deliberately so. The type axis says what a value *is*, and
"failed" is not something a value can be — an errored state holds `V::none()`, so it reports the
none type like any other state holding none. The failure is recorded on the metadata instead
(`is_error`, `Status::Error`, `error_data`), which is where every consumer already reads it from:
nothing dispatches on a type identifier to decide whether a state failed.

The consequence for a reader of stored metadata is worth stating plainly: **the type identifier
reports what is available, not what was intended.** A failed `report.csv` is typed `None` rather
than `Image` or `error`; what it was going to be survives in the query, the key and the filename.

### Registering a type an integration owns

The registry is seeded from `ValueInterface::type_descriptions()`, which is **static**. A type whose
identifier belongs to an *integration crate* rather than to the value type cannot appear there:
`liquers-lib` defines `ExtValue::Foreign` for any integrated language, but only `liquers-web` knows
that its handles are called `js.Value`.

Such a type is registered by **extending the base registry and handing it to the environment
constructor**:

```rust
let mut types = TypeRegistry::from_value_type::<Value>();
types.register(js_value_type_info())?;
let env = DefaultEnvironment::<Value>::new_with_type_registry(types);
```

Three properties follow, and they are the reason for this shape rather than a mutable registration
point:

- **The registry is written only before construction.** Once the environment exists it is
  immutable, so `Environment::get_type_registry` hands out a shared reference with no lock.
- **Extend, never replace.** `TypeRegistry::new()` is empty; `from_value_type` is what supplies the
  value type's own descriptions. Building on the wrong one produces a build that cannot store
  ordinary text.
- **A registry assembled from anywhere is still a registry** — which is what leaves the door open
  for descriptions received from another realm (`TYPE-REGISTRY-NOT-REALM-AWARE`).

The identifier is needed in two places that cannot see each other: the static description, and the
instance's `identifier()` reached through `Arc<dyn ForeignValue>`. Rust cannot tie them together —
`ForeignValue` must stay object-safe, so `type_info` takes `&self`, and a default body is
type-checked with `Self: ?Sized` and cannot call an associated function. **A shared `const` plus a
unit test asserting the two agree** is the guarantee instead, which is proportionate: there are a
few tens of types in the whole system and each is fixed once its variant is implemented.

Only a type implemented in a *different crate from its value type* needs this. `liquers-py` holds
Python objects in `Value::Py`, a variant of a value type it owns, so `py.Object` goes in its static
`type_descriptions()` and no constructor is involved.

See `specs/guides/LANGUAGE-INTEGRATION_GUIDE.md` §VALUE for the procedure.

## The encoding axis

`data_format` selects the codec; `media_type` is what the world is told. They are one axis with two
audiences, not two type axes.

### Seeding: two levels, then an override

| Level | Source | Sets |
|---|---|---|
| 1 | the value's own defaults (`ValueInterface::default_data_format` and friends) | resolved on demand, **not written** |
| 2 | the filename extension (`with_filename`, `set_filename`, `set_extension`) | `data_format`, when none was declared |
| 3 | an explicit declaration (`with_media_type`, or a caller setting `data_format`) | either, verbatim |

**Level 1 resolves rather than writes.** An absent `data_format` *means* "no format was chosen, so
the value's own default applies". Writing the default into the field would destroy that
distinction — nobody could then tell a deliberate choice from a fall-through. Resolution happens
where a value is in hand: `State::effective_data_format`.

Consequently `MetadataRecord::declared_data_format` returns `Option<&str>` and
`effective_data_format(value_default)` takes the level-1 answer as an argument.
`get_data_format()` remains, with `bin` standing in for callers that have no value.

### `media_type` is an override slot

`MetadataRecord::media_type` is `Option<String>`: `None` means derive from the effective format,
`Some` is a deliberate override kept verbatim and never re-derived.

**Overriding it is an intended capability**, not a mistake to normalize away. It is how a caller
shapes an HTTP response, and how a remotely fetched file keeps the origin server's declared
`Content-Type` — information no extension and no data format can supply
(`liquers-web/src/store/fetch.rs`). The guard is therefore on the *shape* of the string, applied
when it is stored, so the freedom cannot become header injection.

`AssetInfo::media_type` stays an unwrapped `String`. It is a resolved projection for clients, not a
place to record how a media type came about.

### Refinements

A data format may carry a refinement after a colon: `csv:comma`. A refinement narrows a format
without changing which parser reads it, so `csv` and `csv:comma` are *consistent* — comparisons are
made on the base. A refinement cannot currently be written in a query parameter; see
`specs/issues/DATA-FORMAT-CONSTANTS-AND-TOOLING.md`.

## The registry

`liquers-core::type_system` holds `TypeInfo` (the facts about one type) and `TypeRegistry`
(identifier-keyed lookup), reached through `Environment::get_type_registry`, mirroring
`get_command_metadata_registry`. Built once at construction from `ValueInterface::type_descriptions`,
read-only thereafter — so no lock, and a deterministic listing order.

`TypeInfo` is **builder-constructed**, never a struct literal, so a later field stays additive for
generated code. Keys are `TypeKey { realm, type_identifier }`, mirroring `CommandKey`, with the
default realm stored as the empty string; realm-aware *behaviour* is
`specs/issues/TYPE-REGISTRY-NOT-REALM-AWARE.md`.

`register` refuses a duplicate rather than resolving it by load order. `from_value_type` is
infallible because an `Environment` constructor is: a duplicate there is a bug in a value type, so
the first description wins and the collision goes to stderr.

## Rust types and identifiers

> **Rust types in Rust code and in command registration. Type identifiers in the registries**,
> which exist to integrate with other languages and other realms.

`to_type_identifier::<V, T>()` is the only crossing, and it goes one way, resolved by the compiler:

```rust
pub trait TypeIdentifiedIn<V> {
    const TYPE_IDENTIFIER: &'static str;
    fn type_info() -> TypeInfo;
}
pub const fn to_type_identifier<V, T: TypeIdentifiedIn<V>>() -> &'static str { T::TYPE_IDENTIFIER }
```

**`V` is not decoration.** A bare `TypeIdentified` would have to be implemented in `liquers-lib` for
`polars::frame::DataFrame` — a foreign trait for a foreign type, rejected with E0117 — and the same
applies to `image::DynamicImage`, `chrono::NaiveDate` and every other type that matters.
Parameterising by the value type puts a **local** type into the impl head, which RFC 2451 permits.
A welcome consequence: the mapping is relative to a value type, so two crates that each define their
own value type may both name `polars::frame::DataFrame` without a coherence conflict.

The reverse direction — identifier to Rust type — is never needed inside Rust, because Rust code
always has the type. Deserialization is not a counterexample: bytes plus an identifier produce a
`Value`, which is dispatch *within* the value type.

`Arc<T>` and `&T` resolve to the same identifier as `T`: a wrapper changes the cost of extraction,
never the identity.

## What a value variant may hold

A `Value` is `Clone` by contract and is cloned freely, so a variant's payload must be **O(1) to
clone**: a small inline type, or `Arc<T>`. `ExtValue` follows this; `Value` does not yet, and the
cost is measured — `size_of::<Value>()` is 704 bytes, set entirely by `Value::Metadata`, so every
`Value::None` costs 704 bytes where `Arc<MetadataRecord>` would cost 8
(`specs/issues/CORE-VALUE-ENUM-OVERSIZED.md`).

`Arc<Mutex<T>>` is an **exception requiring justification**, not an available pattern. It makes a
value a *handle*: two clones observe each other's mutations, and since the asset layer versions
values by content, a payload that mutates in place can leave a cached asset whose content no longer
matches its version. `ExtValue::Widget` does this deliberately, because live mutation is a widget's
purpose.

## Enforcement

Checks run at `AssetManager::set_binary` and `set_state`, in two tiers.

| Tier | Check | Outcome |
|---|---|---|
| **Hard** | `type_identifier` or `type_name` empty | `Error::general_error` |
| | identifier not registered in this build | `Error::general_error`, naming it |
| | effective `data_format` not supported by the type | `ErrorType::SerializationError`, naming type, format and the supported set |
| | `media_type` override containing CR/LF, or not `type/subtype` | `Error::general_error` |
| **Soft** | extension ≠ base of the effective format | `LogEntry::warning` |
| | declared `media_type` ≠ the derived one | `LogEntry::warning` — expected under an override |

Soft warnings are the diagnostic layer: they show that an override is active and where a format came
from, which no amount of rejection reveals. `MetadataRecord::error()` sets `Status::Error`, so
advisory entries stay at `Warning` or below.

**Two exemptions from the format check**, both because the pairing is meaningless rather than
because the rule is inconvenient:

- **An error state.** An errored asset keeps the intended output's filename, so `report.csv` gives
  an effective format of `csv` while the value is gone and the type has become `None` — and its
  bytes are not a serialization of the declared type at all.
- **A type that declares no formats.** A UI element, an egui widget or a foreign handle has no byte
  form; the asset layer persists it as metadata only, so requiring it to name a format it cannot
  produce would be contradictory.

The identifier check applies in both cases.

### Reading

`deserialize_stored_value` consults the registry before dispatching. A type this build does not
know **degrades**: the bytes and metadata are kept verbatim so a minimal build can copy, proxy and
re-store data it cannot interpret. Asking for a *value* then fails with an error naming the type.

The lowercase `bytes`/`binary`/`bin`/`b` identifiers are read but never produced — a read-side
accommodation for older stores, not an alias, and the write path refuses them because they are not
registered.

## Compatibility

Identifiers changed outright in this design — `Value::I32.identifier()` was `"generic"`, and five
variants shared that answer while the deserializer branched on `"i32"`. **No migration is
provided**: data written by an older build reports identifiers this build does not register, and
degrades on read.

## History

| Date | Change |
|---|---|
| 2026-08-18 | Created with the `value-type-system` design, resolving `CORE-METADATA-FORMAT-TYPE-CONSISTENCY`. |
| 2026-08-26 | Removed the `error` type identifier: an errored state is typed by the value it holds, which is none, and the failure lives in the metadata. Stated the one-identifier-per-variant rule. Added runtime registration for a type an integration owns (`foreign-value-type-registration`). |
