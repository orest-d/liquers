# Phase 1: High-Level Design — value-interface-capability-split

## Design Readiness

- **Readiness:** needs-decision
- **Automatic fixing:** not-eligible — adds a `pub` trait (`ValueType`) to `liquers-core` and
  removes and renames `pub` trait methods used by `liquers-lib`, `liquers-web` and `liquers-py`
  (rules 1, 3, 4, 6).
- **Leading issue:** **Open design - what "the split" is.** At HEAD the only capability that is
  duplicated, drifts and is implemented by integrations is the *type identity and description*
  surface; this design carves out that one trait and states why serialization stays a supertrait.
- **Explanation:** the slice is mechanical and checked against every implementor at HEAD
  (3 `ValueInterface`, 1 production and 4 test `ValueExtension`, 2 production and 4 test
  `ForeignValue` implementors). Phases 3-4 assume the recommended answers below.
- **Open questions:**
  1. **Open design - scope:** extract only `ValueType` (identity + description) and share it
     between `ValueInterface`, `ValueExtension` and `ForeignValue`; do not split the scalar,
     core-payload or JSON methods further. Recommended: yes — every one of them is used by the
     engine for every value type, so a sub-trait would gain no implementor an opt-out.
  2. **Open design - serialization:** keep `DefaultValueSerializer` as a supertrait of
     `ValueInterface` and delete the TODO with a doc comment saying why. Recommended: yes — every
     environment value is persisted by the asset layer, and a type with no byte form already opts
     out by refusing (`ForeignValue::as_bytes`). Turning it into a configurable serializer object
     is `DATA-FORMAT-CONSTANTS-AND-TOOLING` territory, not this design's.
  3. **Open design - names:** rename `identifier` to `type_identifier` (it then matches
     `MetadataRecord::type_identifier`); keep `type_name` (not `detailed_type_identifier`), which is
     already the field name in `MetadataRecord`, `AssetInfo`, Python and the reference.
     Recommended: as stated; no backward compatibility is kept (maintainer preference).
  4. **Open design - JavaScript getter:** rename `liquers-web`'s `Value.identifier` getter to
     `typeIdentifier` in the same change. Recommended: yes, for one name everywhere; it changes the
     `.d.ts` declarations.
  5. **Proposed resolution - required methods:** `type_identifier` and `type_info` are required;
     `type_name`, the four `default_*` and `supports_data_format` are derived from `type_info`.

## Purpose

A value type's facts — identifier, type name, default format, extension, media type, filename —
are written twice today: once in its `TypeInfo` and again as five hand-written match methods,
in three parallel traits (`ValueInterface`, `ValueExtension`, `ForeignValue`). Give them one
trait, `ValueType`, whose only required content is the identifier and the description, so the
description is the single source and new capabilities (conversion, value summary) attach beside
it (VISION: "extensibility through traits"; Strategy: core defines the traits).

## Problem Example

`liquers-lib/src/value/mod.rs` declares `polars.DataFrame` as
`TypeInfo::new("polars.DataFrame").with_defaults("csv", "csv", "text/csv", "data.csv")` and then
repeats `"csv"`, `"data.csv"`, `"text/csv"` and `"polars_dataframe"` in four separate `match`
arms (`type_name`, `default_extension`, `default_filename`, `default_media_type`), plus four more
delegations in `CombinedValue` (`value/extended.rs`). Core `Value` does the same for 15 variants
(75 arms, `liquers-core/src/value.rs`); `PyObjectValue` repeats `py_type_info`'s defaults in three
`ForeignValue` methods (`liquers-py/src/value.rs`). Changing `.with_defaults("csv", …)` to
`"parquet"` today leaves `default_extension()` returning `"csv"` — the class of bug that was
`COMBINED-VALUE-DEFAULT-EXTENSION-NOT-DELEGATED`. Should be: the arm does not exist; every default
is read from the one description, and `value.type_identifier()` names the method the metadata field
already names.

## Scope and Acceptance Criteria

- **AC-1** Defaults come from the description
  WHEN `type_name`, `default_data_format`, `default_extension`, `default_filename`,
  `default_media_type` and `supports_data_format` are asked of every core `Value`, `SimpleValue`
  and `ExtValue` variant
  THEN each equals the corresponding field of that value's `type_info()`, and `type_info()` equals
  the registry entry for its identifier
- **AC-2** A foreign value states its facts once
  WHEN a `ForeignValue` implementor provides only `type_identifier`, `type_info` and `type_name`
  THEN its defaults equal its registered description, and the registry and the instance agree
- **AC-3** One method name for the identifier
  WHEN any value, extension or foreign value is asked its identifier
  THEN the method is `type_identifier()`, and `State` writes exactly that string into
  `MetadataRecord::type_identifier`
- **AC-4** No behaviour change
  WHEN the existing value, registry, round-trip, foreign-registration and binding tests run
  THEN they pass with only the renames applied, and stored metadata is byte-identical
- **AC-5** A changed default cannot diverge
  WHEN a variant's `TypeInfo` defaults are changed
  THEN `default_*` of that variant, and of the combined value holding it, follow with no other edit

Non-goals: splitting the scalar, core-payload or JSON methods into sub-traits (question 1);
removing serialization from `ValueInterface` (question 2); generating the arms that remain
(`value-type-definition-macro`); new capabilities (`value-conversion-registry`, `value-summary`).

## Core Interactions

Value types (`ValueInterface`, `ValueExtension`, `ForeignValue`, `TypeInfo`); Assets and State
(read identifiers and defaults); bindings (`liquers-py`, `liquers-web` implement `ForeignValue`).

## Crate Placement

`ValueType` lives in `liquers-core/src/type_system.rs` beside `TypeInfo`, which it returns;
`liquers-lib` and both bindings implement it. No new dependency.

## Documentation Intent

- Reference: extend `specs/reference/VALUE_TYPE_SYSTEM.md` — the `ValueType` trait, derived defaults.
- Guide: update `specs/guides/TYPE_SYSTEM_GUIDE.md` §3 (two methods, not seven) and
  `specs/guides/LANGUAGE-INTEGRATION_GUIDE.md` §VALUE (`ForeignValue` listing).
- Other documents: none.
- Documents to update: `specs/reference/PYTHON_INTEGRATION.md` (method name), the `liquers-web`
  `.d.ts` (getter name).

## Design Dependencies

- `value-type-definition-macro` — **required-by**: generates `ValueType` impls; must target this shape.
- `value-conversion-registry`, `value-summary` — **required-by** (soft): each adds a capability
  beside `ValueType`; landing this first avoids editing the same trait methods twice.
- `core-value-enum-oversized` (ready) — **overlaps**: edits the same core `Value` impl
  (`liquers-core/src/value.rs`); either order works, land one and rebase the other.
- `data-format-vocabulary` — **overlaps**: rewrites the same `type_descriptions()` bodies with
  format constants; independent contracts.

## Consolidated Findings

- The issue's claim that the split breaks "both bindings" is narrower at HEAD: since commit
  `0d0a4c1` `liquers-py` implements only `ForeignValue`; `liquers-web` implements `ForeignValue`
  and, in a test, `ValueExtension`. Both change by renames and by deleting three methods.
- "The trait cannot grow a capability without breaking every implementor" is not true at HEAD:
  `data_node` and `type_descriptions` were added with defaults. The real cost is duplication.
- `TypeRegistry::from_value_type` and the `default_*` derivation must read the *same* description;
  caching the descriptions per concrete type keeps the derived methods cheap (Phase 2, Risks).
- Order: land before `value-type-definition-macro`; independent of the rest of the cluster.
- Validation is the existing suites plus three new tests (Phase 3) and the build matrix, because
  cfg-gated arms are deleted in `ExtValue`.

## References

- `specs/issues/CORE-VALUE-INTERFACE-CAPABILITY-SPLIT.md`; `specs/reference/VALUE_TYPE_SYSTEM.md`;
  `specs/design/value-type-system/` (Generator alignment); `specs/design/foreign-value-type-registration/`.
