---
title: Value Type System
kind: reference
audience: internal
area: [core/value, lib/value]
reviewed: 2026-08-18
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

`type_name` refines the type axis. It is informational — a runtime-oriented detail such as `i64` or
a Python class name — and is **never** a dispatch key.

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
`Recipe`, `CommandMetadata`, `Query`, `Key`, and `error`.

Library (`liquers-lib::value::ExtValue`): `Image`, `UIElement`, `polars.DataFrame` (feature
`polars`), `egui.Command` and `egui.Widget` (feature `egui`).

`error` is the identifier of an errored value. It is registered because the write path requires
every stored identifier to be registered, and an errored asset is still a stored, typed thing.

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
  an effective format of `csv` against an `error` identifier — and its bytes are not a
  serialization of the declared type at all.
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
