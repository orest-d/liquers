# Phase 1: High-Level Design

## Design Readiness

- **Readiness:** ready
- **Automatic fixing:** not-eligible — test and documentation work only, but in two crates: the
  OpenDAL rows come from `liquers-store/tests/store_conformance_CONF.rs`, which `liquers-core`'s
  tests cannot run (rule 6)
- **Leading issue:** None
- **Explanation:** Decided (Maintainer decision, 2026-10-10): build a small generator. Since the
  issue was filed the guide already says the table is maintained by hand; the generator makes the
  native rows reproducible.
- **Open questions:** None.

## Problem

`specs/guides/STORE_IMPLEMENTATION_GUIDE.md` §9 is a table of rule counts and statuses per
in-tree store. It is edited by hand. `ConformanceReport` (`liquers-core/src/store_conformance/report.rs`)
derives serde so a generator could exist, but none does.

## Expected behaviour and acceptance (generator)

1. An `#[ignore]`d `status_table` test in `liquers-core/tests/store_conformance_CONF.rs` runs each
   core suite (memory, file, router, trait defaults, `NoAsyncStore`, and `FetchStore` if its suite
   is native) and prints its §9 rows as Markdown to **stderr**; a matching `status_table` in
   `liquers-store/tests/store_conformance_CONF.rs` prints the two OpenDAL rows.
2. The guide gives both regeneration commands
   (`cargo test -p liquers-core --test store_conformance_CONF -- --ignored --nocapture status_table`
   and the same with `-p liquers-store --features store-conformance`) and says that browser-only rows
   (`JsStore`, `LocalStorageStore`) are merged by hand from the browser suites.
3. No new dependency.

## Scope

Native suites only, in `liquers-core` and `liquers-store`. The browser suites run under wasm and cannot be run by the native generator.

## Design Dependencies

- `memory-store-metadata-only-entry` — **overlaps** (adds a rule; the table changes).

## Documentation assessment

- Guide: `STORE_IMPLEMENTATION_GUIDE.md` §9: regeneration instruction. History, `reviewed:`.

## Consolidated Findings

- CLAUDE.md: "Library code must never write to stdout … (it applies inside `#[cfg(test)]` modules
  too)". An integration test under `tests/` is not library code, but to stay unambiguous the
  generator should write to **stderr** with `eprintln!`, and the instruction should say so.
- The "Notes" column is prose and stays hand-maintained. The generator emits it from a small
  `match` on the store name, or leaves it empty for a human to fill. Recommended: keep notes in a
  `const` map inside the generator, so the whole table is reproducible.
