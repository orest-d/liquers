# Phase 3: Examples and Tests

| # | Kind | Checks |
|---|---|---|
| E1 | example | A command reporting `done("Loaded 3 rows")` → finished asset: `primary_progress().message == "Loaded 3 rows"`, `is_done()` |
| E2 | example | A command reporting `ProgressEntry::new("Loading", 3, 10)` and returning → `is_done()`, message `"Loading"` |
| E3 | example | A command reporting nothing → `is_off()` (no bar) |
| E4 | example | A command reporting a tick and then failing → `is_done()` (finished, so no unfinished bar) |
| T1 | unit | `finished_progress` over off / tick / partial / done |
| T2 | stress | Native harness: E1 ×100 → identical |
| T3 | stress | Inline harness (`ImmediateAssetManager`): E1 ×100 → identical |
| T4 | regression | `interpreter::tests::test_evaluate_immediately` again asserts `is_done()` |

## Setup

Register with `register_command!`: `report_done(state, context) -> result`,
`report_partial(state, context) -> result`, `silent(state) -> result`,
`tick_then_fail(state, context) -> result`. Evaluate each as a bare action query (validated with
`liquers-validate --command report_done --command report_partial --command silent --command tick_then_fail`).

Names: `finished_run_keeps_commands_done_progress`, `finished_run_completes_partial_progress`,
`finished_run_without_progress_has_none`, `failed_run_with_progress_reports_done`,
`finished_progress_is_deterministic_native`, `finished_progress_is_deterministic_inline`.
