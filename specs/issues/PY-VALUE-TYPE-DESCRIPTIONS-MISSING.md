---
id: PY-VALUE-TYPE-DESCRIPTIONS-MISSING
kind: issue
title: liquers-py's Value describes no types, so its registry would refuse every write
status: draft
priority: P2
complexity: S
area: [py, core/value]
design:
created: 2026-08-26
github:
---
## Problem

`liquers-py/src/value.rs` implements `ValueInterface` for its own `Value` enum but does not override
`type_descriptions()`, so the default empty `Vec` applies. `PyEnvironment`
(`liquers-py/src/context.rs:43`) seeds its registry with
`TypeRegistry::from_value_type::<Value>()`, which therefore holds exactly one entry: the `error`
pseudo-type added unconditionally by `TypeRegistry::from_value_type`.

Since `value-type-system` step 6, `validate_metadata_hard` (`liquers-core/src/assets.rs:584`)
refuses any identifier the registry does not contain. Every identifier `liquers-py`'s `Value`
reports — `generic`, `text`, `dictionary`, `bytes`, `python_value`, `metadata`, `recipe`,
`command_metadata`, `query`, `key` — is absent, so no value at all could be written through a
`PyEnvironment`.

A second, independent mismatch: those identifiers are not the ones `liquers-core`'s `Value` uses
(`None`, `Text`, `Object`, `Bytes`, `Query`, `Key`, …), so a store written by Python and read by
Rust would disagree on type identity even once the registry is populated.

## Impact

**Currently dormant, not currently broken.** `context.rs` and `value.rs` are among the eight files
`liquers-py/src/lib.rs` never declares as modules (`PY-MODULES-NOT-DECLARED-IN-LIB`), so neither is
compiled. The defect becomes live the moment those modules are re-declared — which is what fixing
that issue means — and would then present as every Python-side write failing with "Type identifier
'…' is not registered in this build".

## Expected behaviour

`liquers-py`'s `Value` describes its types via `type_descriptions()`, with identifiers reconciled
against `liquers-core`'s so that a store is readable from both languages. `python_value` is the
runtime-typed case and depends on `FOREIGN-VALUE-TYPES-NOT-REGISTERED`.

## Discovery

Found on 2026-08-26 while surveying registry consumers for the
`foreign-value-type-registration` design. Deliberately left out of that design's scope: the fix is
`liquers-py`-local and gated behind `PY-MODULES-NOT-DECLARED-IN-LIB`.
