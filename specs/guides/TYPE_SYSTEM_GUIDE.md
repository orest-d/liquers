---
title: Adding and Typing Value Types
kind: guide
audience: both
area: [core/value, lib/value]
reviewed: 2026-10-07
---

# Adding and Typing Value Types

How to add a value type so the system can describe it, store it and read it back. For *why* the
model looks like this, see `specs/reference/VALUE_TYPE_SYSTEM.md`.

## The four steps

Adding a value type used to be step 1 alone. It is now four, and skipping the last two means the
value cannot be stored — the write path refuses an identifier it does not know.

### 1. Add the variant

`liquers-lib/src/value/mod.rs`. Payload behind `Arc<T>` unless it is small and inline: a `Value` is
cloned freely, so the payload must be O(1) to clone.

```rust
pub enum ExtValue {
    // …
    Sketch { value: Arc<crate::sketch::Sketch> },
}
```

Feature-gate it if it brings a dependency, and remember that a `#[cfg]`-gated variant needs a
`#[cfg]`-gated arm in **every** exhaustive `match` — the no-`_ =>` rule means the compiler will tell
you, in the configuration where the feature is off.

### 2. Choose an identifier

**Bare, or `provider.LocalName`?** Ask: *does Liquers own this concept?*

- **Bare** — Liquers commits to this as *the* canonical type for the concept, and converts others
  into it. `Image` is bare even though its payload is `image::DynamicImage`, because Liquers owns
  the meaning of an image.
- **Prefixed** — Liquers is exposing somebody else's type, and there are, or will be, siblings.
  `polars.DataFrame` is prefixed because pandas, arrow and lazy frames are coming.

Bare names are reserved for `liquers-core` and `liquers-lib` and the set is enumerated in the
reference. **When unsure, prefix**: baring a name later is free, un-baring one is a breaking rename
of stored data.

Local names are CamelCase and name the **Liquers concept**, not the backing Rust struct —
`Image`, not `DynamicImage`. A Python reader should recognise it.

**One identifier per variant, one variant per identifier.** Anything that varies from instance to
instance goes in `type_name`, which is informational and never dispatched on. If you find yourself
wanting two identifiers for one variant — or one identifier covering a family — the variant is
probably wrong, not the naming rule.

There is no `error` identifier, and a new type needs nothing for failure: an errored state holds
`V::none()` and is typed accordingly, with the failure recorded in the metadata. See
`specs/reference/VALUE_TYPE_SYSTEM.md`, "How a failure is typed".

### 3. Add the arms

Each of `identifier`, `type_name`, `default_extension`, `default_filename`, `default_media_type`,
`as_bytes` and `deserialize_from_bytes`. They must agree with each other — `default_filename` ends
in `default_extension`, and `default_data_format` derives from it.

> Nothing enforces that agreement today, which is why one of the five once returned a constant for
> every extended value while its siblings delegated
> (`COMBINED-VALUE-DEFAULT-EXTENSION-NOT-DELEGATED`). Generating these arms from one declaration is
> `specs/issues/VALUE-TYPE-DEFINITION-MACRO.md`.

### 4. Describe it

Add a `TypeInfo` to `ExtValue::type_descriptions()`. **This is the step that makes the type
storable**, and the one most easily forgotten:

```rust
TypeInfo::new("Sketch")
    .with_type_name("sketch")
    .with_defaults("svg", "svg", "image/svg+xml", "sketch.svg")
    .with_data_formats(["svg", "png"]),
```

`supported_data_formats` lists what the type can be **written** in — that is what the write path
checks. It is legitimately wider than what round-trips: `Text` can be written as bytes and reads
back as `Bytes`. Declare what `as_bytes` accepts.

A type with **no** byte form declares no formats and omits `with_data_formats`. It is then stored
as metadata only, which is what a UI element or a foreign handle needs.

**If the identifier belongs to another crate, step 4 moves.** A type whose implementation lives in
an integration crate — a JavaScript handle in `liquers-web` — cannot be in this static list, because
`liquers-lib` does not know the name. That crate instead extends the base registry and passes it to
the environment constructor:

```rust
let mut types = TypeRegistry::from_value_type::<Value>();
types.register(js_value_type_info())?;
let env = DefaultEnvironment::<Value>::new_with_type_registry(types);
```

The registry is frozen once the environment exists. Extend `from_value_type`; starting from
`TypeRegistry::new()` discards every type the build already had. Full procedure:
`specs/guides/LANGUAGE-INTEGRATION_GUIDE.md` §VALUE.

## A gated variant, worked: `RecordView` and `RecordSource`

The record variants are the first optional value types after `polars.DataFrame`, and the cleanest
example of what a feature gate costs. Both exist only with `liquers-lib`'s `records` feature.

**Step 1.** Each variant carries `#[cfg(feature = "records")]`, and its payload is a trait object
behind `Arc` — `RecordView { value: Arc<dyn RecordView> }`, `RecordSource { value: Arc<dyn
RecordSource> }` — so every implementation of the trait shares one variant and one identifier.

**Every exhaustive `match` on `ExtValue` gains a gated arm**, not only the ones in `value/mod.rs`:

```rust
#[cfg(feature = "records")]
ExtValue::RecordSource { .. } => Err(ext_scalar_refusal(self, "i64")),
#[cfg(feature = "records")]
ExtValue::RecordView { value } => record_view_cell_i64(&value.single_cell()?),
```

For the record variants that meant `liquers-lib/src/value/mod.rs` (the `ExtValueInterface`
accessors, the scalar hooks, `type_info`, `identifier`, `type_name`, the three `default_*` methods,
`as_bytes`), `liquers-lib/src/ui/web/html.rs`, `liquers-lib/src/egui/mod.rs`, and
`liquers-web/src/default_value.rs`, which is outside `liquers-lib` and is found only by building
`liquers-web`. `deserialize_from_bytes` matches the identifier string, so its arm
(`"RecordView" => …`) is gated too, ahead of the refusing catch-all.

**Step 2.** Both are bare: Liquers owns the concepts, whatever implements the traits.

**Step 4.** The `TypeInfo`s are pushed inside a `#[cfg(feature = "records")]` block, and a format
whose writer sits behind a narrower feature is declared only under that feature too:

```rust
let mut record_view_formats: Vec<&'static str> = vec![
    "csv", "csv:comma", "tsv", "csv:tab", "ndjson", "jsonl", "json", "md", "markdown", "html",
];
#[cfg(feature = "records-ipc")]
record_view_formats.extend(["ipc", "feather", "arrow_ipc", "arrow"]);
#[cfg(feature = "records-parquet")]
record_view_formats.push("parquet");
```

Declaring `parquet` in a build that cannot write it would let the asset layer accept a format the
codec refuses. Aliases are declared too (`jsonl`, `markdown`, `feather`…), for the same reason.

**The build matrix.** A gated arm that forgot its `#[cfg]` compiles with the default features and
breaks only when the feature is off, so the compiler catches it only in a configuration nobody
builds by accident. `scripts/check-build-matrix.sh` carries rows for exactly this —
`records`, `records,polars`, `webui,records`, `records-ipc`, `records-parquet`,
`records-parquet,polars`, and wasm32 with `webui,records` and `webui,records,records-ipc`. Run it
after adding a gated variant, and add rows for its interactions with the other optional features.
Tests that need the variant are gated the same way: `liquers-lib/tests/record_typeinfo.rs` opens
with `#![cfg(feature = "records")]`.

## Verifying it

```bash
cargo test -p liquers-lib --test value_type_system
```

`ext_value_type_descriptions_complete` fails if a variant has no description — that is the check
for step 4. It samples every `ExtValue` variant compiled into the build except `Foreign`, and the
test fails to compile when a variant is added without a sample, because its
`statically_described` helper matches every variant with no default arm. Its sibling
`ext_value_type_descriptions_have_no_stale_entries` checks the reverse: every description belongs
to a sampled variant. `liquers-lib/tests/record_typeinfo.rs` additionally pins the record
variants' declared formats. For an integration-owned type, the equivalent check is that its constant and its
instance agree; see `liquers-lib/tests/foreign_value_registration.rs` for a worked example that
runs natively. Then a round trip:

```rust
let bytes = value.as_bytes("svg")?;
let back = Value::deserialize_from_bytes(&bytes, "Sketch", "svg")?;
assert_eq!(back.identifier(), "Sketch");
```

Asserting the **identifier** and not just the payload is the point: an integer once round-tripped
into a string because the identifier written and the identifier dispatched on were different words.

## Common problems

**"Type identifier 'X' is not registered in this build."** Step 4 is missing, or the feature that
declares it is off. If the value came from a store written by another build, that build knows a
type this one does not — the read degrades and keeps the bytes.

**"Type 'X' cannot be serialized as 'Y'; supported formats: [...]"** Either `as_bytes` gained a
format the description does not list, or a caller declared a format the type cannot write. The two
must agree; the description is the declaration.

**A warning about the filename extension.** Advisory. It compares against the *base* format, so
`notes.csv` with `csv:comma` is silent; `notes.json` with `csv` is not.

**A warning about the media type.** Expected whenever an override is active — it is how you can
tell one is. To set an override deliberately, use `with_media_type`; leaving it unset means "derive
from the format", which is what you usually want.

## Choosing a data format at write time

Level 1 is the value's own default, level 2 is the filename extension, level 3 is an explicit
declaration. So:

```rust
// Level 1 — takes the value's default.
let state = State::new().with_data(value);

// Level 2 — the extension chooses.
record.with_filename("report.csv".to_owned());

// Level 3 — say it outright.
record.data_format = Some("csv:comma".to_owned());
```

Leaving `data_format` as `None` is meaningful and usually right: it says nobody chose, so the
value's default applies, and that remains visible to anyone reasoning about the result later.

The same extension can be valid on several types, each with its own meaning: `md` on a `Text`
writes the text as is (plain markdown), while `md` on a `RecordView` renders a markdown table.

## Naming a type in Rust code

Use Rust types in Rust code; identifiers belong in the registries, which exist for other languages
and other realms. When you need the identifier for a Rust type, derive it rather than writing the
string:

```rust
to_type_identifier::<ExtValue, polars::frame::DataFrame>()   // "polars.DataFrame"
```

This resolves at compile time and cannot drift from the registration.

## History

| Date | Change | Source |
|---|---|---|
| 2026-10-07 | §Verifying it: `ext_value_type_descriptions_complete` samples every variant but `Foreign`, fails to compile when a variant has no sample, and has a reverse (no stale description) sibling. | phase-5, `design/ext-value-description-completeness/` |
| 2026-10-07 | §Choosing a data format: `md` is declared on `Text` (plain markdown) as well as on `RecordView` (a markdown table). | phase-5, `design/text-value-markdown-format/` |
| 2026-09-27 | Reviewed against `design/record-streams/` Phase 5. Added §A gated variant, worked: `RecordView` and `RecordSource` — trait-object payloads, a `#[cfg(feature = "records")]` arm in every `ExtValue` match including `liquers-web`'s, `TypeInfo`s declared under the features that enable their writers, and the build-matrix rows. §Verifying it no longer claims `ext_value_type_descriptions_complete` covers every variant. | phase-5 |
| 2026-08-18 | Created with the `value-type-system` design. | `design/value-type-system/` |
| 2026-08-26 | §2 states the one-identifier-per-variant rule and that there is no `error` identifier; §4 records where step 4 moves for a type whose identifier belongs to an integration crate. | `design/foreign-value-type-registration/` |
