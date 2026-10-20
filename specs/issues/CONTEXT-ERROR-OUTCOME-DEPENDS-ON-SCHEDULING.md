---
id: CONTEXT-ERROR-OUTCOME-DEPENDS-ON-SCHEDULING
kind: issue
title: Whether Context::error fails an asset depends on when its log message is applied
status: closed
priority: P1
complexity: M
area: [core/context, core/assets]
design:
created: 2026-10-20
github:
---
## Resolution

Fixed in the same change that filed this issue. `Context` carries a synchronous record of the
first error-level entry logged through it (`Context::logged_error`, shared by clones, fresh per
evaluation). The interpreter checks it before the first step and after every step
(`interpreter::apply_plan_state`) and stops the plan with
`Error::execution_error(<first message>)`, so the asset ends in `Error` through the normal
failure path and its dependents see that error. A command that logs an error and then returns
`Err` fails with its own error. The asset manager checks the flag again after
`Environment::apply_recipe` returns, so a custom environment that runs a plan without the
interpreter gets the same outcome. `MetadataRecord::add_log_entry` now sets only `is_error`,
never the status, and only on a record that is not finished, so a late log message cannot make a
finished asset unreadable. `MetadataRecord::with_status` keeps `is_error` while a run waits or
resumes (`Submitted`, `Dependencies`, `Processing`, `Storing`); `None` and `Recipe` restart a run
and clear it. An executable `Step::Error`, and an error-level `init_steps` entry, fail the plan
the same way; the latter before any dependency is scheduled.

`liquers-core/tests/context_error.rs` pins it on the queued manager (current-thread and
multi-thread runtimes) and on the inline manager: a sync command, an `info` after the error, a
dependency wait after the error, an uncached command followed by another action, a command
returning `Err` after logging, and the mid-run state (status `Processing`, `is_error` true). All
of those tests fail without the fix.

## Problem

`Context::error` (`liquers-core/src/context.rs`) sent an `AssetServiceMessage::LogMessage`; the
asset's service loop applied it with `MetadataRecord::add_log_entry`, which set `is_error` **and**
`status = Error` on the metadata while the asset itself still ran. Nothing else acted on the
error. A failure happened only as a side effect: when the step's output state copied that metadata
(`prefix_metadata`), `State::value()` turned an `is_error` state into a `GeneralError` built from
the **last** log message. So:

- a synchronous command that returned without yielding could end `Ready` with its value kept,
  because the log message had not been applied yet;
- a later status change (`set_status(Dependencies)` while waiting for a dependency, then
  `Processing`) erased `is_error` entirely;
- an `info` logged after the error replaced the error's message;
- a log message applied after the asset finished flipped a `Ready` asset's metadata to `Error`.

## Impact

Commands in every language integration use `context.error` to report a failure. Whether the
evaluation failed, and with which message, depended on task scheduling. liquers-py's documentation
had to describe the observed behaviour.

## Expected behaviour

As specified by Orest (2026-10-20): `context.error` logs an error message and sets a flag
(`is_error`). It cannot and does not terminate the command, but after the command returns the
asset turns into the Error state and execution of the rest of the plan stops.

## Discovery

Writing the liquers-py guide: `test_EVAL_context_error_fails_the_result` failed with
`GeneralError` from Python, and tracing it showed the failure was a scheduling side effect.
