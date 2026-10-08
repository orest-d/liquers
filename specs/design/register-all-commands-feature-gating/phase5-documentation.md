# Phase 5: Documentation

**Status: executed 2026-10-07; approved 2026-10-08 (maintainer).**

## Summary

Implemented 2026-10-07 as Wave 3 step 22 of
`archive/2026-10-06-p2-p3-s-implementation-order-revised.md`.

- Step 1 (image probe): `liquers-lib/src/image/mod.rs` gates `pub mod commands` on
  `image-support`, so `register_image_commands!` vanishes without it and needs a stand-in too.
- `liquers-lib/src/commands.rs`: four no-op stand-ins (egui, image-support, polars, records), each
  `#[cfg(not(feature = …))] #[macro_export]`, next to `register_all_commands!`, whose doc comment
  now says it registers the enabled domains.
- T1: `liquers-lib/tests/register_all_commands.rs` (ungated) calls the macro and checks one anchor
  command per domain, present exactly when its feature is on. It passes with no optional feature
  and with each single feature from `CLAUDE.md`; T2 is the build matrix.
- T3 (optional) not taken: `registry_export.rs` keeps its group-by-group mirror, which builds what
  the exporter builds. Its comment and the binary's no longer claim the macro fails to compile.

## Conformance and deviations

As designed; the optional T3 cleanup was left out (see above).

## Documentation

None needed: no guide or reference mentions the limitation. The code comments are updated.

## New issues

None.

## Validation

`cargo test -p liquers-lib --test register_all_commands` with default features, with
`--no-default-features`, and with each of `polars`, `egui`, `image-support`, `records`, `webui`;
`bash scripts/check-build-matrix.sh`; `python3 scripts/docs_index.py --check`.
