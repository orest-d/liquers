# Phase 1: High-Level Design — value-type-definition-macro

## Design Readiness

- **Readiness:** needs-decision
- **Automatic fixing:** not-eligible — `XL`; a new `pub` proc macro in `liquers-macro`, a rewritten
  `ExtValue` definition and changed accessor signatures in `liquers-lib` (rules 1, 3, 4, 6).
- **Leading issue:** **Open design - scope split.** The feature asks for the generator *and* the
  fifteen extended scalars. This design delivers the generator, proved on the existing `ExtValue`
  variants with no behaviour change, and moves the scalars to the follow-up
  `VALUE-EXTENDED-SCALAR-TYPES-MISSING`, which then declares them through it.
- **Explanation:** a concrete generator is specified against HEAD's `ExtValue`; every hand-written
  behaviour becomes a named per-type function the declaration points at. It requires
  `value-interface-capability-split` (the trait shape it emits). Phases 3-4 assume the answers below.
- **Open questions:**
  1. **Open design - scope split:** generator first, scalars as a follow-up. Recommended: yes. The
     maintainer's 2026-08-18 decision ("the scalars ship with the generator") is kept in spirit —
     they are declared through it, never hand-written — but they need decisions this generator does
     not: JSON form of `u64`/`i128`/`decimal`, feature gating (`ext-scalars` / `ext-temporal`), and
     narrowing classes from `value-conversion-registry`. Bundling would make one unbounded design.
  2. **Open design - accessor signatures:** generate `from_NAME(Arc<T>)` / `as_NAME() ->
     Result<Arc<T>, Error>` for every accessor-bearing variant, changing
     `from_polars_dataframe(DataFrame)` to take `Arc<DataFrame>` (29 call sites). Recommended: yes,
     one rule; no backward compatibility is kept.
  3. **Proposed resolution - target:** `ExtValue` only. Core `Value` and `SimpleValue` keep
     hand-written impls (bespoke untagged serde and `json_f64` handling); the macro is usable by any
     downstream extension enum through a `crate_path` option.
  4. **Proposed resolution - proc macro, not `macro_rules!`:** optional keys with defaults, cfg
     attributes carried onto every generated arm, and per-key validation are impractical in
     `macro_rules!`.
  5. **Implementation detail:** consumer matches outside the value module (`ui/web/html.rs`,
     `egui/mod.rs`, `liquers-web/src/default_value.rs`) stay hand-written: they render, they do not
     describe.

## Purpose

Adding a value variant means editing ~10 matches in `liquers-lib/src/value/mod.rs` (136 variant
arms, 128 `#[cfg]` lines at HEAD), each a place for a silently divergent arm. Generate the enum,
its identity, description, serializer dispatch, extension hooks, accessors and `TypeIdentifiedIn`
impls from one declaration per variant (VISION: "Extensibility through … declarative registration";
priority area 6, extended value library).

## Problem Example

Adding `Sketch { value: Arc<Sketch> }` to `ExtValue` today requires arms in `type_identifier`,
`type_info`, nine `ValueExtension` hooks, `data_node`, `as_bytes`, `deserialize_from_bytes`, and an
`as_*` arm in each of five `ExtValueInterface` accessors, plus a new accessor pair in the trait and
both of its impls. The `Foreign` arms of `try_into_string` / `try_into_json_value` show the cost: they
refuse instead of delegating, so `ForeignValue`'s hooks are dead
(`FOREIGN-VALUE-TEXT-AND-JSON-HOOKS-NEVER-CALLED`). Should be:

```rust
define_value_types! {
    pub enum ExtValue (base = SimpleValue) {
        Sketch(Arc<crate::sketch::Sketch>) {
            identifier: "Sketch", type_name: "sketch",
            defaults: ("svg", "svg", "image/svg+xml", "sketch.svg"),
            formats: ["svg", "png"],
            accessor: sketch,
            encode: crate::sketch::encode, decode: crate::sketch::decode,
        },
    }
}
```

and nothing else; every unlisted hook refuses with the standard error.

## Scope and Acceptance Criteria

- **AC-1** One declaration per variant
  WHEN `ExtValue` is defined through `define_value_types!`
  THEN no hand-written `match` over `ExtValue` remains in `liquers-lib/src/value/mod.rs`
- **AC-2** No behaviour change
  WHEN the existing `liquers-lib` value, record, polars, image and foreign suites run in every
  build-matrix configuration
  THEN they pass, with only accessor call sites edited (question 2)
- **AC-3** Descriptions follow the declaration
  WHEN a variant is declared with `cfg: feature = "polars"`
  THEN its arms and its `TypeInfo` exist exactly when the feature is on, and
  `ext_value_type_descriptions_complete` holds in each configuration
- **AC-4** Foreign values delegate everywhere
  WHEN a foreign payload implements `try_into_json_value`
  THEN `ExtValue::Foreign`'s `try_into_json_value` returns its result
- **AC-5** Rust types name identifiers
  WHEN `to_type_identifier::<ExtValue, image::DynamicImage>()` is evaluated
  THEN it is `"Image"`, for every variant declaring `rust_type`
- **AC-6** Bad declarations fail at compile time
  WHEN a declaration repeats an identifier, omits `identifier`, or names an unknown key
  THEN the macro emits a `compile_error!` naming the variant and the key

Non-goals: the extended scalars (question 1); core `Value` / `SimpleValue` (question 3); format
constants (`data-format-vocabulary`); `register_command!` changes.

## Core Interactions

Value types (`ExtValue`, `ValueType`, `ValueExtension`, `DefaultValueSerializer`,
`ExtValueInterface`, `TypeIdentifiedIn`); macro crate.

## Crate Placement

The macro in `liquers-macro` (beside `register_command!`; no new dependency); its use in
`liquers-lib/src/value/mod.rs`; per-type functions where the types live (`image/`, `polars/`,
`records/`).

## Documentation Intent

- Reference: extend `specs/reference/VALUE_TYPE_SYSTEM.md` (how `ExtValue` is defined);
  new `specs/reference/DEFINE_VALUE_TYPES.md` (DSL reference, like `REGISTER_COMMAND_FSD.md`).
- Guide: rewrite `specs/guides/TYPE_SYSTEM_GUIDE.md` §The four steps and §A gated variant.
- Other documents: none.
- Documents to update: `CLAUDE.md` §Adding a Value Type is maintainer-owned — propose the edit in
  the PR, do not make it silently.

## Design Dependencies

- `value-interface-capability-split` — **requires**: the macro emits `impl ValueType`.
- `data-format-vocabulary` — **overlaps**: `formats:` may name its constants; no ordering.
- `value-conversion-registry` — **required-by** for the scalar follow-up (narrowing classes); the
  macro may later emit conversion edges from a `conversions:` key (not in this slice).
- `value-summary` — **overlaps**: a `summary:` hook key is added by whichever lands second.
- Covers `TYPE-IDENTIFIED-IN-HAS-NO-IMPLEMENTATIONS` (for `ExtValue`) and makes
  `FOREIGN-VALUE-TEXT-AND-JSON-HOOKS-NEVER-CALLED` impossible to reintroduce; both stay separate
  issues because each has a smaller fix.

## Consolidated Findings

- After `value-interface-capability-split`, the generator's work is dispatch, not facts: identity
  is a match on literals and every behaviour is a per-type function the declaration names.
- `TypeIdentifiedIn` has no implementations at HEAD; this is the first producer.
- The `Foreign` variant is special (dynamic identifier, delegating everything): a `foreign` marker,
  not a set of hooks.
- Proof is behavioural equivalence: the existing suites in all build-matrix rows (AC-2), plus
  token-level unit tests of the parser in `liquers-macro`.
- Order: split → this → `VALUE-EXTENDED-SCALAR-TYPES-MISSING` (also needs conversion).

## References

- `specs/issues/VALUE-TYPE-DEFINITION-MACRO.md`; `specs/design/value-type-system/phase2-architecture.md`
  §Generator alignment, §Sequencing decision; `liquers-macro/src/registration.rs` (parser pattern).
