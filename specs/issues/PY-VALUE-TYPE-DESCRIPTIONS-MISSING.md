---
id: PY-VALUE-TYPE-DESCRIPTIONS-MISSING
kind: issue
title: liquers-py's Value describes no types, so its registry would refuse every write
status: closed
priority: P2
complexity: M
area: [py, core/value]
design: foreign-value-type-registration
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

Under the one-identifier-per-variant rule, `generic` must be split into `None`, `Bool`, `I32`,
`I64`, `F64` and `Array` exactly as `value-type-system` step 3 split it in `liquers-core::Value`,
and `python_value` — which fails `identifier_naming_rule_holds` because `_` is a reserved character
— becomes `py.Object`.

## Discovery

Found on 2026-08-26 while surveying registry consumers for the
`foreign-value-type-registration` design, and taken **into** that design's scope on 2026-08-26.

Declaring `value` and `context` in `lib.rs` was measured the same day: `value.rs` alone produces
four compile errors — `try_into_query` returns `crate::parse::Query` where the trait wants
`liquers_core::query::Query`, `from_asset_info` takes one `AssetInfo` where the trait wants a
`Vec<AssetInfo>`, a `match` with incompatible arms, and four unimplemented trait items
(`from_command_metadata`, `try_into_bytes`, `try_into_key`, `try_into_command_metadata`). Repairing
`value.rs` is therefore part of this issue; the rest of `PY-MODULES-NOT-DECLARED-IN-LIB` is not.

## Resolution

**Closed 2026-08-26** by `foreign-value-type-registration` (PR
[#42](https://github.com/orest-d/liquers/pull/42)).

`liquers-py`'s `Value` now has `type_descriptions()` with one entry per variant, identifiers matching
`liquers-core`'s, and `py.Object` for the retained Python object. Getting there required repairing
the file first: `try_into_query`'s return type, `from_asset_info`'s signature and its `todo!()`,
incompatible `match` arms, four unimplemented trait items, and a new `AssetInfo` variant forced by
the trait signature.

Two things beyond the original scope, both necessary to verify the result. `value` and `context` are
now declared in `lib.rs` (see `PY-MODULES-NOT-DECLARED-IN-LIB`, which stays open for the other
five files). And pyo3's `extension-module` moved behind a default feature with `rlib` added to
`crate-type`, because a test binary cannot link against it — the crate had no tests at all, so this
was invisible until one was written.

Evidence: five tests in `liquers-py/src/value.rs`, run with
`cargo test -p liquers-py --lib --no-default-features --features async_store`.
