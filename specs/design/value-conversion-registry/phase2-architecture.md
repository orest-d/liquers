# Phase 2: Solution & Architecture — value-conversion-registry

## Overview

A `ConversionRegistry<V>` in `liquers-core`, keyed by (source identifier, target identifier), each
edge carrying a `ConversionClass` and a function `value → value`. It is seeded from the value type
(`ValueInterface::conversions()`, mirroring `type_descriptions()`), may be extended by an
integration before construction, and is frozen in the environment beside the type registry. A
root-namespace command `convert` exposes explicit conversion; `ConversionPolicy::Automatic` is
implemented and tested now so the follow-up only has to call it.

Rejected: a command per target (`to_record_view`, …) — closed set, integrations cannot add targets;
value → Rust-type functions (`FromValue<T>`) — needs the argument typing that does not exist yet and
duplicates `TryFrom<V>`; async conversions — none of the edges does I/O; edges as `fn` pointers —
base edges must be lifted into `CombinedValue` by a closure.

## Known-Issue Preflight

| Issue | Status | Priority | Impact on this design | Fix first? | Blocking? | Action |
|---|---|---|---|---|---|---|
| `COMMAND-INPUT-OUTPUT-TYPES-ARE-UNDECLARED` | draft | P2 | Needed by automatic binding only | no | no | Follow-up depends on it |
| `CORE-VALUE-INTERFACE-CAPABILITY-SPLIT` (`value-interface-capability-split`) | accepted | P2 | Same trait; `conversions()` is added beside `ValueType` | no | no | Prefer after |
| `SERIALIZATION-CANNOT-REPORT-A-LOSSY-WRITE` | draft | P2 | Analogous lossy notion for bytes | no | no | None |
| `TYPE-REGISTRY-NOT-REALM-AWARE` | draft | P2 | Keys are identifiers in the default realm, as types are | no | no | Realm added with types later |
| `POLARS-BRIDGE-VECTOR-COLUMNS-REFUSED` | draft | P3 | `RecordView → polars.DataFrame` fails on vector columns | no | no | Typed error; class `Lossy` already non-automatic |

## Interfaces

```rust
// liquers-core/src/conversion.rs (new)
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ConversionClass { Identical, Exact, Widening, Structural, Lossy, Fallible }
impl ConversionClass { pub fn is_automatic(self) -> bool } // false for Lossy, Fallible

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConversionPolicy { Explicit, Automatic }

pub struct ConversionInput<'a, V> { pub value: &'a V, pub data_format: Option<&'a str> }

pub type ConvertFn<V> =
    Arc<dyn Fn(&ConversionInput<'_, V>) -> Result<V, Error> + MaybeSend + MaybeSync>;

pub struct Conversion<V> {
    pub source: Cow<'static, str>, pub target: Cow<'static, str>,
    pub class: ConversionClass, pub convert: ConvertFn<V>,
}
impl<V> Conversion<V> {
    pub fn new(source: impl Into<Cow<'static, str>>, target: impl Into<Cow<'static, str>>,
               class: ConversionClass, convert: impl Fn(&ConversionInput<'_, V>) -> Result<V, Error>
               + MaybeSend + MaybeSync + 'static) -> Self;
    /// Lift an edge of a base value into a value that wraps it (CombinedValue).
    pub fn map<W>(self, into: fn(V) -> W, from: fn(&W) -> Option<&V>) -> Conversion<W>;
}

pub struct ConversionRegistry<V> { edges: BTreeMap<(String, String), Conversion<V>> }
impl<V: ValueInterface> ConversionRegistry<V> {
    pub fn new() -> Self;
    pub fn from_value_type() -> Self;                       // V::conversions(), duplicates to stderr
    pub fn register(&mut self, c: Conversion<V>) -> Result<(), Error>; // refuses duplicate / self-edge
    pub fn can_convert(&self, from: &str, to: &str) -> Option<ConversionClass>; // Identical when equal
    pub fn targets_from(&self, from: &str) -> impl Iterator<Item = (&str, ConversionClass)>;
    pub fn convert(&self, input: &ConversionInput<'_, V>, to: &str, policy: ConversionPolicy)
        -> Result<(V, ConversionClass), Error>;
}

/// Generic scalar edges over the core identifiers, valid for any value type using them.
pub fn scalar_conversions<V: ValueInterface>() -> Vec<Conversion<V>>;
```

Changed: `ValueInterface::conversions() -> Vec<Conversion<Self>> where Self: Sized` (default empty;
core `Value` and `SimpleValue` return `scalar_conversions()`); `ValueExtension::conversions<B:
ValueInterface + Default>() -> Vec<Conversion<CombinedValue<B, Self>>>` (default empty);
`CombinedValue::conversions()` = lifted base edges + extension edges. `Environment::
get_conversion_registry(&self) -> &ConversionRegistry<Self::Value>`; `EnvRef::get_conversion_registry`;
`GenericEnvironment` field and `EnvironmentBuilder::with_conversion_registry`, seeded with
`from_value_type`, mirroring `with_type_registry`. All synchronous.

Scalar edges (core identifiers): `I32→I64` Widening, `I32→F64` Widening, `I64→I32` Fallible,
`I64→F64` Lossy, `F64→I64` Fallible (non-integral or out of range refused), `Bool/I32/I64/F64→Text`
Exact, `Text→Bool/I32/I64/F64` Fallible (`from_*_str`), `Query/Key→Text` Exact, `Text→Query/Key` Fallible.
Library edges (`ExtValue::conversions`): `Text/Bytes→RecordView` Fallible (format from
`data_format`, default `csv`), `Array/Object→RecordView` Structural, `RecordView→polars.DataFrame`
Lossy (roles dropped, vector columns refused), `polars.DataFrame→RecordView` Fallible,
`Text/Bytes→polars.DataFrame` Fallible, `Bytes→Image` Fallible. Each body is a function extracted
from the existing helper it duplicates, which then calls it.

## Integration Points

- `liquers-core/src/conversion.rs` (new), `lib.rs` (`pub mod conversion`), `value.rs`
  (`conversions`, core `Value` impl), `context.rs` (`Environment`, `GenericEnvironment`, `EnvRef`),
  `environment_builder.rs` (field, `with_conversion_registry`, `build`), `liquers-core/tests/
  environment_builder.rs` `CustomEnvironment` (implements the new method).
- `liquers-lib/src/value/simple.rs`, `value/extended.rs` (`ValueExtension::conversions`,
  `CombinedValue`), `value/mod.rs` (`ExtValue::conversions`, gated per feature).
- `liquers-lib/src/records/convert.rs`: extract `records_from_text_or_bytes`, `records_from_json`;
  `to_record` calls them. `liquers-lib/src/polars/util.rs`: extract `dataframe_from_text_or_bytes`;
  `try_to_polars_dataframe` calls it.
- `liquers-lib/src/commands.rs` `register_core_commands!`: the `convert` command.

## Error Handling

No edge: `Error::conversion_error_with_message(from, to, "no conversion; reachable: …")`. Policy
refusal: same constructor, message naming the class. Duplicate or self edge at registration:
`Error::general_error`. A failing edge returns its own typed error (e.g. the CSV reader's).

## Relevant Commands

```rust
fn convert(state, target: String, context) -> result
    label: "Convert"
    doc: "Convert the value to another type by identifier (any registered edge, lossy included)"
```

Root namespace; body: input from `state.data` and `state.metadata.declared_data_format()`,
`ConversionPolicy::Explicit`, then `context.info("converted Text → I64 (Fallible)")`. A
`specs/command_registry.yaml` regeneration and CHANGELOG line follow.

## Documentation Architecture

| Path | Change |
|---|---|
| `specs/reference/VALUE_TYPE_SYSTEM.md` | new §Conversion; the Purpose bullet points to it; History, `reviewed:` |
| `specs/guides/TYPE_SYSTEM_GUIDE.md` | "Declaring conversions"; History, `reviewed:` |
| `specs/guides/LANGUAGE-INTEGRATION_GUIDE.md` | §VALUE conversion paragraph: register edges with `with_conversion_registry`; History, `reviewed:` |

Proposed `affects_docs`: those three.

## Risks

| Assessment | Finding |
|---|---|
| Files likely to change | listed above; `specs/command_registry.yaml` |
| Crates and workflows affected | core, lib; `ns-rec` and `ns-pl` input helpers refactored to shared functions |
| Existing tests likely to change | `CustomEnvironment` gains a method; records/polars helper tests unchanged |
| New validation | Phase 3 tests; registry export test; build matrix (gated edges) |
| Compatibility / data / concurrency / performance / security | Additive API apart from one required `Environment` method (one external implementor, a test). Registry frozen after build: no lock. Classes are a public promise — mis-classifying an edge as automatic would later lose data silently; each class is asserted by a test |
| Recovery | Revert; the helpers' extracted functions are behaviour-neutral on their own |
| Certainty and open questions | High for the registry; class assignments are proposals for review (question 3) |
