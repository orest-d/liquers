# Phase 3: Examples and Tests

| # | Kind | Checks |
|---|---|---|
| T1 | existing | `submit_runs_dependency_at_once_on_inline_manager` |
| T2 | existing | `submit_returns_before_the_dependency_finishes_on_queued_manager` |
| T3 | new | Queued manager with capacity 1: the parent command (occupying the slot) submits `dep`; immediately after `submit`, `dep.status()` is `Submitted`; after `context.wait_for_dependency(&dep)` it is `Ready` |

## T3 setup

- Register `parent_cmd(context) -> result`. Inside, it calls
  `let dep = context.submit(&parse_query("dep_cmd")?).await?`, records `dep.status().await` in a
  shared `Mutex<Vec<Status>>`, then `context.wait_for_dependency(&dep).await?`.
- Register `dep_cmd() -> result` returning `Value::from("d")`.
- Construct the queued manager with a job-queue capacity of 1 (reuse the constructor used by
  existing saturation tests in `assets.rs`).
- Evaluate `parent_cmd` and assert the recorded status is `Status::Submitted` and the final
  result is `Ready`.

Both queries are bare action names. Validate them with
`liquers-validate --command parent_cmd --command dep_cmd`.
