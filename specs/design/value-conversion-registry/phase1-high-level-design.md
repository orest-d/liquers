# Phase 1: High-Level Design — value-conversion-registry

## Design Readiness

- **Readiness:** needs-decision
- **Automatic fixing:** not-eligible — new `pub` module and trait methods in `liquers-core`, a new
  `Environment` method and a new command (rules 1, 3, 4, 6).
- **Leading issue:** **Open design - scope split.** The feature has an explicit half (a registry,
  `can_convert`, a `convert` command) and an automatic half (argument binding, state, UI, web
  negotiation, purposes). This design delivers the explicit half and the classification the
  automatic half will consult; the automatic half is the follow-up
  `VALUE-NOT-CONVERTED-AUTOMATICALLY`, which also needs `COMMAND-INPUT-OUTPUT-TYPES-ARE-UNDECLARED`.
- **Explanation:** the registry and its edges reuse conversions that exist at HEAD as hand-written
  helpers (`liquers-lib/src/records/convert.rs` `to_record`, `polars/util.rs`
  `try_to_polars_dataframe`); Phases 3-4 assume the answers below.
- **Open questions:**
  1. **Open design - scope split:** as above. Recommended: yes; automatic binding needs a declared
     argument type that does not exist yet.
  2. **Open design - query syntax:** one generic command `convert-TARGET` (a type identifier) in the root
     namespace, e.g. `-R/data/sales.csv/-/convert-RecordView` (validated). Recommended over a
     command per target: the target set is the registry, open to integrations.
  3. **Open design - classes:** the issue's `Identical, Exact, Widening, Structural, Lossy,
     Fallible`; automatic use refuses `Lossy` and `Fallible`. A `Structural` edge may still fail on
     a value of the wrong shape (a typed error, never lost information). Recommended: as stated.
  4. **Proposed resolution - edges are value → value:** a conversion produces another value of the
     same value type; extracting a Rust type stays `TryFrom` / `as_*`. The automatic half then
     composes "convert to the declared identifier, then extract".
  5. **Proposed resolution - synchronous, single edge:** conversions are CPU work on a value in hand
     (sync); no transitive `A → B → C` search in v1. A dependency fetch (`to_record`'s `Key` branch)
     is evaluation, not conversion, and stays in its command.
  6. **Proposed resolution - purposes deferred:** a purpose (`table`) is a follow-up question; the
     draft's recommendation (a registered identifier nothing is stored as) is recorded there.

## Purpose

Let a user convert a value to another type by name, and let any part of the system ask whether a
conversion exists and whether it loses information, without each command inventing its own rules
(VISION priority 2, DataFrames and records; "trust the result").

## Problem Example

```
-R/data/sales.csv/-/convert-RecordView/ns-rec/head-5
-R/data/sales.csv/-/convert-polars.DataFrame
```

Today: there is no `convert` command; a CSV becomes a table only inside commands that each parse it
their own way (`to_record` for `ns-rec`, `try_to_polars_dataframe` for `ns-pl`), and nothing can
state that `RecordView → polars.DataFrame` drops field roles or that `I64 → F64` loses precision
above 2⁵³. Should be: both queries return the requested type, the log names the edge and its class,
and `can_convert("RecordView", "polars.DataFrame")` answers `Some(Lossy)` without converting.

## Scope and Acceptance Criteria

- **AC-1** Explicit conversion by identifier
  WHEN `-R/data/sales.csv/-/convert-RecordView` is evaluated on a stored CSV
  THEN the result's `type_identifier` is `RecordView` and its rows are the file's rows
- **AC-2** The edge is recorded
  WHEN any `convert` succeeds
  THEN the asset log has an info entry naming source, target and class
- **AC-3** No edge, a typed error
  WHEN a value is converted to a target with no edge (`Text` → `Image`)
  THEN the error is a `ConversionError` naming both identifiers and the reachable targets
- **AC-4** Classification without work
  WHEN `can_convert(from, to)` is asked
  THEN it returns the class (or `None`) without calling the conversion function
- **AC-5** Automatic policy refuses loss
  WHEN `ConversionRegistry::convert` runs a `Lossy` or `Fallible` edge with `ConversionPolicy::Automatic`
  THEN it fails with a `ConversionError` naming the class, and with `Explicit` it converts
- **AC-6** Scalar edges
  WHEN `I32 → I64`, `I64 → I32` (out of range), `Text "42" → I64`, `I64 → F64` run explicitly
  THEN they give `I64(…)`, a range error, `I64(42)`, and `F64` — classes `Widening`, `Fallible`,
  `Fallible`, `Lossy`
- **AC-7** Integrations extend it
  WHEN an environment is built with an extended registry
  THEN `convert` uses the added edge, and a duplicate edge is refused at registration

Non-goals: automatic conversion anywhere (follow-up); purposes; transitive search; async
conversions; replacing `to_record` / `try_to_polars_dataframe` (they share edge functions, Phase 2).

## Core Interactions

Value types (new conversion capability); Commands (`convert`); Environment (a registry beside the
type registry); Assets (log entry).

## Crate Placement

Registry, classes and the generic scalar edges in `liquers-core/src/conversion.rs` (a contract, like
`TypeRegistry`); library edges and the `convert` command in `liquers-lib`. No new dependency.

## Documentation Intent

- Reference: extend `specs/reference/VALUE_TYPE_SYSTEM.md` with §Conversion (classes, policy, registry).
- Guide: extend `specs/guides/TYPE_SYSTEM_GUIDE.md` with "Declaring conversions".
- Other documents: none.
- Documents to update: `specs/guides/LANGUAGE-INTEGRATION_GUIDE.md` §VALUE "Design for conversion that
  does not exist yet" (it now exists, explicitly).

## Design Dependencies

- `value-interface-capability-split` — **requires** (soft): adds `conversions()` beside `ValueType`;
  either order compiles, the split first avoids editing `ValueInterface` twice.
- `COMMAND-INPUT-OUTPUT-TYPES-ARE-UNDECLARED` (no design) — **required-by** for the automatic follow-up.
- `value-type-definition-macro` — **overlaps**: may later emit edges from a `conversions:` key.
- `serialization-lossy-write-warnings` — **overlaps**: the same lossy notion for value → bytes; no
  shared code.

## Consolidated Findings

- Conversion needs the source's declared data format (CSV text → table), so the input carries it.
- The engine's argument path converts with `TryFrom<V>`; a value → value registry composes with it
  unchanged (question 4).
- Edges are lifted from base to combined value by closure, which is why entries hold
  `Arc<dyn Fn>` rather than `fn` pointers.
- Order in the cluster: after (or with) the split; before `VALUE-EXTENDED-SCALAR-TYPES-MISSING`.

## References

- `specs/issues/VALUE-CONVERSION-CAPABILITY.md`; `specs/design/value-type-system/type-conversion-draft.md`;
  `prior-art.md` §9.
