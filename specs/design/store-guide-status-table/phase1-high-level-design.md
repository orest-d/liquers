# Phase 1: High-Level Design

## Design Readiness

- **Readiness:** needs-decision
- **Leading issue:** **Open design question — build a generator, or close the issue now that the
  guide is honest.** Since the issue was filed, `STORE_IMPLEMENTATION_GUIDE.md` §9 was rewritten.
  It now says the table is "maintained by hand from the printed reports — no generator exists
  yet", which is the issue's second option, already done.
- **Explanation:** The remaining question is whether a generator is worth building. A small
  design for one is specified, so the decision can be "build" without further design work.
- **Open questions:**
  1. **Proposed resolution — build a small generator test.** Hand counts drift on every rule
     addition (`sidecar04` changed every count at once). A generator also lets
     `memory-store-metadata-only-entry`'s new rule update the table mechanically. Alternative:
     close as resolved by the documentation change, with no code.

## Problem

`specs/guides/STORE_IMPLEMENTATION_GUIDE.md` §9 is a table of rule counts and statuses per
in-tree store. It is edited by hand. `ConformanceReport` (`liquers-core/src/store_conformance/report.rs`)
derives serde so a generator could exist, but none does.

## Expected behaviour and acceptance (generator)

1. An `#[ignore]`d test in `liquers-core/tests/store_conformance_CONF.rs` (or a small example)
   runs each native suite and prints the §9 table as Markdown to stdout. It is a test, so stdout
   is the binary's own output, which is acceptable under the stdout rule (tests' printed output is
   captured).
2. The guide says how to regenerate (`cargo test -p liquers-core --test store_conformance_CONF -- --ignored --nocapture status_table`)
   and that browser-only rows (`JsStore`, `LocalStorageStore`) are merged by hand from the browser
   suites.
3. No new dependency.

## Scope

Native suites only. The browser suites run under wasm and cannot be run by the native generator.

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
