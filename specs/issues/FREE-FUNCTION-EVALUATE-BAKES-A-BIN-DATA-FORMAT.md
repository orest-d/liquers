---
id: FREE-FUNCTION-EVALUATE-BAKES-A-BIN-DATA-FORMAT
kind: issue
title: Every unkeyed/ad-hoc asset declares a bin data format it usually cannot serialize as
status: closed
priority: P2
complexity: M
area: [core/plan]
design: plan-step-state-metadata
created: 2026-09-26
github:
---
## Problem

Any asset built from a `Recipe` whose own query has no filename extension gets a **declared**
`data_format` of `"bin"` baked into its initial metadata, unconditionally — not left absent. Two
call paths were found to do this, and neither is exotic:

- `liquers_core::interpreter::evaluate` (the free function, `interpreter.rs:707`) always builds its
  top-level asset with `AssetRef::new_temporary(envref)`, from `Recipe::default()`
  (`assets.rs:896`).
- The ordinary, recommended query path — `EnvRef::evaluate` / `Context::get_dependency_state` /
  `AssetManager::get_query_asset` — builds an unkeyed ("expression") asset's recipe from the query
  text itself (`Recipe { query: encoded, ..Default::default() }`), which has the same problem
  whenever *that* query has no filename either (e.g. a plain command call like `target_value`,
  reached as a dependency with no trailing `/name.ext`).

In both cases the asset's `AssetInfo`/initial metadata is built via `Recipe::get_asset_info`, which
sets `asset_info.data_format = Some(self.data_format()?)` (`recipes.rs:418`). `Recipe::data_format()`
(`recipes.rs:212`) falls back to the literal `"bin"` whenever the recipe's query has no filename
extension — which an ad-hoc `Recipe::default()` or a plain command-call query never does.

`MetadataRecord::set_filename`/`set_extension` (`metadata.rs:1330`, `1358`) only seed `data_format`
from a *later* filename when it is currently `None`, so once `"bin"` is declared this way nothing
downstream can correct it — not a trailing `/report.csv` segment (`Step::Filename` only sets the
name), and not the value's own default format either, because `State::effective_data_format()`
(`state.rs:49`) honours a *declared* format unconditionally, never falling through to the value's
default while one is declared.

`"bin"` is also the wrong spelling for `liquers-lib`'s own generic-bytes format: `SimpleValue`'s
own default extension for `Bytes` is `"b"` (`liquers-lib/src/value/simple.rs:211`), and
`SimpleValue::as_bytes` (`simple.rs:553`) has no `"bin"` arm at all — only `"txt"`, `"html"` and
`"json"` — so in `liquers-lib` **every** `SimpleValue` scalar or structured value fails to
serialize once `"bin"` is declared, regardless of which type is picked.

## Impact

Any value reached as an unkeyed dependency or through the free `evaluate()` function, whose own
query has no filename, cannot be read with `state.as_bytes()` / `state.as_bytes_with_data_format`
unless its declared format happens to match what it actually supports (in `liquers-lib`, nothing
does, since `"bin"` is not one of `SimpleValue`'s recognised formats at all). The failure reads as
`SerializationError: Unsupported format bin` and gives no hint that a filename segment or a
different evaluation path would have avoided it.

This bit `liquers-records`' own `classify_state` (`liquers-records/src/sources.rs`, the shared
helper behind both `ChunkResolver` implementations): its fallback branch (a resolved dependency
that is neither a `RecordView` nor a `RecordSource`) calls `state.as_bytes()`, which fails for any
ordinary scalar or JSON-shaped dependency reached this way. `liquers-lib/tests/
resolver_dependency_recording.rs` (Step 5.6) works around it by giving its fixture dependency a
`RecordView` value instead of a scalar, so `classify_state`'s `as_record_view()` check short-circuits
before the broken branch runs — the two tests there do not exercise `classify_state`'s bytes
fallback at all, which is worth noting for whoever eventually adds that coverage.

The workaround for a caller that must go through the broken path anyway is
`state.as_bytes_with_data_format(&state.data.default_data_format())` (bypassing the declared
format entirely) — untested here, since the tests above route around the defect instead of driving
it.

This is a narrower, more concrete symptom than `CORE-EVALUATE-PATH-CONSOLIDATION`'s general
duplication complaint (that issue is closed): the free function's own doc comment already says
"TODO: this should be decommissioned in favor of environment evaluate methods", but nothing records
*why*, and the ordinary query path documented as its replacement has the identical defect for any
query with no filename.

## Expected behaviour

`Recipe::data_format()` should not invent `"bin"` when a recipe's query has no filename; `None` is
already a meaningful, handled case throughout this code (`State::effective_data_format`,
`MetadataRecord::declared_data_format`'s own doc comment: "`None` is meaningful: it says no format
was chosen"). `Recipe::get_asset_info` should therefore leave `asset_info.data_format` unset rather
than `Some("bin")` when `Recipe::extension()` answers `None`, so:
- a later `Step::Filename` can still seed a real format from a trailing query segment, and
- `State::effective_data_format()`'s fallback to the value's own default actually runs for a
  dependency whose query never named a format at all.

Separately, if `"bin"` is ever a deliberate placeholder rather than "no format chosen", it should be
spelled the way each value layer actually recognises its own generic-bytes format (`"b"` in
`liquers-lib`), or the affected value layers should accept `"bin"` as an alias.

## Discovery

Found while implementing Phase 4 Step 5.6 of the record-streams design
(`specs/design/record-streams/phase4-implementation.md`):
- `records_end_to_end.rs`'s `materialize_query_yields_csv_bytes` test evaluated
  `-R/data/sales/daily.manifest.yaml/-/ns-rec/materialize/daily.csv` through the free `evaluate()`
  function and failed with `TableFormat::from_data_format: unknown data format 'bin'` even though
  the query's trailing filename plainly names `.csv`. Confirmed by probing
  `state.metadata.declared_data_format()` immediately after a single `Step::GetAsset`, with no
  `Step::Filename` in the plan at all: it already read `Some("bin")`. Worked around by evaluating
  through `EnvRef::evaluate` instead, whose recipe is built from the real query.
- `resolver_dependency_recording.rs`'s two tests then hit the *same* declared `"bin"` on a plain,
  unkeyed `target_value` dependency reached through `Context::get_dependency_state` — the
  recommended path, not the free function — and failed with `SerializationError: Unsupported format
  bin` from inside `classify_state`'s bytes fallback. Worked around by giving the fixture dependency
  a `RecordView` value instead of a scalar, which `classify_state` never serializes to bytes.

## Resolution

Closed 2026-10-10 by `design/plan-step-state-metadata/`, which absorbed it: once a predecessor's
state is handed on unchanged, what a prefix asset declares reaches the next command, so the two
issues shared one contract. `Recipe::data_format` now returns `Result<Option<String>, Error>`
(`None` without a filename) and `Recipe::get_asset_info` declares exactly that, so an unnamed query
— the free `evaluate()` function's `Recipe::default()` asset included — declares no format and the
value's own default applies; a later filename segment can seed one. Evidence:
`recipes::test::data_format_is_absent_without_a_filename`,
`recipes::test::keyed_asset_takes_its_format_from_the_key`,
`plan_step_state_metadata::unnamed_query_declares_no_data_format`. The workarounds described above
stay in `resolver_dependency_recording.rs` / `records_end_to_end.rs` as fixtures; their comments now
record that the defect is fixed.
