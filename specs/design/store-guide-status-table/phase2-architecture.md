# Phase 2: Solution and Architecture

## Generator

Decided (maintainer, 2026-10-10): build it. In `liquers-core/tests/store_conformance_CONF.rs`, and
the same shape in `liquers-store/tests/store_conformance_CONF.rs` for the OpenDAL rows:

```rust
#[tokio::test]
#[ignore = "generator: prints the STORE_IMPLEMENTATION_GUIDE §9 table to stderr"]
async fn status_table() {
    // Run each native suite through the same fixture constructors as the suite tests, collect
    // ConformanceReport { store, rules_run, status, … }, render Markdown rows with NOTES[store].
}
```

Refactor each suite test so the report-producing part is a function returning `ConformanceReport`.
The existing test asserts on it, and the generator collects it. The fields needed are already public
(`ConformanceReport`, `OutcomeCounts` in `liquers-core/src/store_conformance/report.rs`), so no
library change is needed. Rows are written with `eprintln!`.

The header line "As of <date>, from the suites above: N rules are registered" uses the registry's
rule count (the rule list in `store_conformance/mod.rs`).

## Alternative

Close the issue as resolved by the documentation (no code). Rejected by the maintainer.

## Known-issue preflight

None.

## Risk table

| Area | Assessment |
|---|---|
| Likely files | `liquers-core/tests/store_conformance_CONF.rs`, `liquers-store/tests/store_conformance_CONF.rs`; the guide |
| Risk | Refactoring suite tests changes no assertions. Keep each suite's allowed-failure list as is. |
| Certainty | High |
