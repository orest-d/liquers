# Phase 3: Examples and Tests

The examples show what a client observes. The tests pin determinism.

| # | Kind | Checks |
|---|---|---|
| E1 | example | A command that ends with `context.progress(ProgressEntry::done("Loaded 3 rows"))`: the finished asset's `primary_progress().message == "Loaded 3 rows"`, `is_done()` |
| E2 | example | A command that never reports progress: finished asset shows `done("Finished")` |
| E3 | example | A failing command: `primary_progress().is_off()` |
| E4 | example | A cancelled asset: `done("Cancelled")` |
| T1 | unit | `terminal_progress`-equivalent table (pure function test over `succeeded`/cancelled/last) |
| T2 | unit, stress | Native harness: E1 repeated 100× yields identical progress |
| T3 | unit, stress | Inline harness (`ImmediateAssetManager`): E1 repeated 100× |
| T4 | regression | `interpreter::tests::test_evaluate_immediately` additionally asserts `is_done()` again (deterministic now) |

## Setup

- Register a command with `register_command!(cr, fn report_done(state, context) -> result)`
  whose body calls `context.progress(ProgressEntry::done("Loaded 3 rows".to_string()))` and returns
  the input. Register a failing one with `fn always_fail(state) -> result`.
- Evaluate `report_done` and `always_fail` as queries through `envref.evaluate(&parse_query(..)?)`,
  each on an empty input. The queries are bare action names. No resource segment is needed, so no
  store setup is needed beyond the default memory store.

Test names: `finished_run_keeps_commands_done_progress`,
`finished_run_without_progress_reports_finished`, `failed_run_has_no_progress`,
`cancelled_run_reports_cancelled_progress`, `finished_progress_is_deterministic_native`,
`finished_progress_is_deterministic_inline`.

If the decision is "always clear", E1, E2 and E4 expect `is_off()` (E4 then also clears, which
must be stated), and T2/T3 still apply.
