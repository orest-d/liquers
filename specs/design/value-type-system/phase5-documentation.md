# Phase 5: Documentation - Liquers value type system

## Completion Preconditions

- Steps 0–10 of the Phase 4 plan are implemented and committed.
- `cargo test -p liquers-core --lib --tests` — 19 suites green, 592 unit tests.
- `cargo test -p liquers-lib --lib --tests` — 16 suites green.
- `cargo check -p liquers-axum -p liquers-py` — clean.
- `python3 scripts/docs_index.py --check` — 0 errors.
- Scope not delivered is filed as an issue rather than described only here.

**One precondition is not met and is stated rather than glossed:** `liquers-web` is
`wasm32`-only and the `wasm32-unknown-unknown` target is not installed in this environment, so it is
**compile-unverified**. Its source was updated (`js.Value`, the `Bytes` comparison, the fetch
override) but not built. See "Omitted".

## Implementation Summary

### What was requested

An answer to `CORE-METADATA-FORMAT-TYPE-CONSISTENCY`: metadata's `data_format` and type identifier
could disagree with the value they describe, so a value serialized under one format could
deserialize as another type, silently.

### What was implemented

**A type model on two axes** — type (`type_identifier`, refined by the informational `type_name`)
and encoding (`data_format` inward, `media_type` outward) — plus a registry that owns the facts.

- `liquers-core/src/type_system.rs`: `TypeInfo` (builder-constructed), `TypeKey` (realm-keyed),
  `TypeRegistry`, `TypeIdentifiedIn<V>` and `to_type_identifier`.
- `Value` gained honest identifiers. Five variants used to report `"generic"` while the
  deserializer branched on `"i32"`/`"i64"`/`"f64"`/`"bool"`; they are now bare CamelCase names
  matching what the deserializer dispatches on.
- `Environment::get_type_registry`, mirroring `get_command_metadata_registry`, on six implementors.
- `MetadataRecord::media_type` became `Option<String>` — `None` derives, `Some` is an override kept
  verbatim — with `declared_*`/`effective_*` resolution methods and container-level
  `#[serde(default)]`.
- Two enforcement tiers at `set_binary`/`set_state`: hard rejections for what makes a value
  unreadable, soft `LogEntry` warnings for legitimate divergence.
- Degrade-on-read for a type this build does not know.
- `liquers-lib` adopted the naming: `Image` and `UIElement` bare, `polars.DataFrame` and `egui.*`
  prefixed.

### Deviations from the approved plan

| Deviation | Why |
|---|---|
| The base-format comparison (`csv:comma` vs `csv`) landed in step 1, not 6b | The hoisted function is where it lives; separating them meant writing the comparison twice and deleting one |
| `ValueInterface::type_descriptions` was added in step 2, not step 3 | `TypeRegistry::from_value_type` needs it to compile, and the default is empty, so it breaks no implementor |
| `TypeRegistry::from_value_type` is infallible | An `Environment` constructor is; a duplicate there is a value-type bug, not a runtime condition a caller can act on |
| Six `Environment` implementors, not four | The payload variants of both core environments are separate impls |
| A new exemption: a type declaring **no** formats skips the format check | Found by a failing test. A UI element has no byte form and the asset layer persists it as metadata only, so requiring it to name a format it cannot produce is contradictory |

### Findings the tests produced

Each of these was found by a test failing, not by reading:

1. **`Bytes` never survived a JSON round trip.** `Value` is `#[serde(untagged)]`, so
   `Value::Bytes(vec![1,2,3])` serialized to `[1,2,3]` and read back as `Value::Array`. The declared
   identifier is exactly the discriminator serde lacks, and the JSON branch was ignoring it. Fixed;
   a chunk of `COMBINED-VALUE-DISCRIMINATION` is resolved as a side effect.
2. **Two live identifier comparisons broke silently.** `app_state.rs:340` tested `== "query"` and
   `bridge.rs:385` tested `== "bytes"`. Both stopped matching under the new names, and the first
   hung a UI integration test rather than failing it.
3. **`Query` and `Key` could be written as text but not read back.** Missing deserialization arms.
4. **Supported formats are wider than round-tripping formats.** `None` as text writes `none`, which
   the text branch cannot read; `Text` as bytes returns `Bytes`. Both are legitimate writes, so the
   declaration is what `as_bytes` accepts, and the asymmetry is documented where it is declared.

### Issues closed

`CORE-METADATA-FORMAT-TYPE-CONSISTENCY` (P0), `CORE-LEGACY-METADATA-ACCESSORS-RETURN-JSON` (raised
to P1 first, because reject-on-write turned a quoting bug into a refusal of every partial document),
`COMBINED-VALUE-DEFAULT-EXTENSION-NOT-DELEGATED`.

### Issues filed

| Issue | Priority | Why |
|---|---|---|
| `VALUE-CONVERSION-CAPABILITY` | P2 | The purpose axis and conversion, moved out of scope |
| `TYPE-REGISTRY-NOT-REALM-AWARE` | P2 | Cross-realm type support; the key ships, the behaviour does not |
| `VALUE-TYPE-DEFINITION-MACRO` | P2/XL | Generating value types; now owns the scalar widening |
| `DATA-FORMAT-CONSTANTS-AND-TOOLING` | P2/L | Format constants, validation, a generic serde path, a format command, a binary format |
| `CORE-VALUE-ENUM-OVERSIZED` | P2 | `size_of::<Value>()` is 704 bytes, measured |
| `FOREIGN-VALUE-TYPES-NOT-REGISTERED` | **P1** | A runtime-known identifier cannot be registered, so a foreign value cannot be stored |
| `LIB-INTEGRATION-TESTS-NOT-FEATURE-GATED` | P2 | Pre-existing; the feature matrix cannot be run |

## Conformance and Remaining Work

The request was to design type handling for `CORE-METADATA-FORMAT-TYPE-CONSISTENCY`, research
existing type systems, produce a conversion draft, and create a guide. All four were delivered;
the P0 is fixed and closed. The scope grew during design — scalars, a naming rule, realm keying —
and two parts were deliberately handed on rather than delivered, listed under "Omitted".

## Validation

```
cargo test -p liquers-core --lib --tests     # 19 suites, 592 unit tests, green
cargo test -p liquers-lib  --lib --tests     # 16 suites, green
cargo test -p liquers-lib --no-default-features --lib   # 215 tests, green
cargo check -p liquers-axum -p liquers-py    # clean
cargo test -p liquers-lib --test registry_export        # unaffected, as predicted
python3 scripts/docs_index.py --check        # 0 errors
```

`liquers-web` is not in this list, and that is the one gap: see "Omitted".

## Omitted

- **The scalar widening** — fifteen scalars — moved to `VALUE-TYPE-DEFINITION-MACRO` by the user's
  decision, because hand-writing them meant ~120 match arms a generator would delete.
- **`ArgumentType` typing** moved to `COMMAND-METADATA-ENHANCEMENTS`; 101 references across five
  crates makes it a project of its own.
- **`liquers-web` verification.** Source updated, not compiled. `FOREIGN-VALUE-TYPES-NOT-REGISTERED`
  is P1 precisely because it most likely breaks there and could not be observed.
- **The diagnostic command** listing a build's registered types — asked at the Phase 2 gate, never
  answered, and not needed for the P0.

## Documentation Delivered

| Path | Change |
|---|---|
| `specs/reference/VALUE_TYPE_SYSTEM.md` | **New.** Both axes, the naming rule and its enumerated bare set, the seeding cascade, the registry contract, the orphan-rule constraint, the payload discipline, both enforcement tiers |
| `specs/guides/TYPE_SYSTEM_GUIDE.md` | **New.** The four steps of adding a value type, how to choose an identifier, common errors and what they mean |
| `specs/reference/PROJECT_OVERVIEW.md` | Updated: the value/metadata section points at the new reference |
| `specs/reference/ASSET_SET_OPERATION.md` | Updated: the rules it asserted are now enforced, and in which tier |
| `CLAUDE.md` | "Adding a Value Type" is now four steps, not three |
| `specs/README.md` | Capability map |

## Important Learning

**A design that removes machinery beats one that adds it.** The command-argument question went
through a data file, then a generated alias module, then inference — and landed on
`to_type_identifier::<V, T>()`, which makes the mismatch *unrepresentable* rather than checked. Two
runtime checks and a whole file format were deleted rather than refined.

**The orphan rule is invisible in a signature.** `TypeIdentifiedIn<V>` looks like over-engineering
until you try `impl TypeIdentified for polars::frame::DataFrame` and get E0117. It is written into
the trait's doc comment for exactly that reason.

**Reject and warn are not alternatives.** Rejecting everything inconsistent would have broken the
remote-fetch media type; warning about everything would have kept the P0. The tier split is what
made both work, and the two exemptions from the format check — an error state, and a type with no
byte form — were both found by tests rather than anticipated.

**Measure before asserting.** `size_of::<Value>()` being 704 bytes turned a plausible concern into a
filed issue with a number. The `Bytes` JSON failure and the ungated feature matrix were likewise
found by running things, not by reading them.
