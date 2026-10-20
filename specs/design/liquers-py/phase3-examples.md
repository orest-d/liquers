# Phase 3: Examples & Use-cases — liquers-py

## Overview Table

| # | Kind | Name | Shows / checks | Scenarios |
|---|---|---|---|---|
| 1 | Example | Python step in a pipeline | the problem example, end to end | AC-1, AC-9 |
| 2 | Example | Python feeding polars | a Python DataFrame into `pl/head` | AC-3 |
| 3 | Test | `test_COMMAND01_register_and_execute_first_command` | `hello/repeat-3` | AC-1 |
| 4 | Test | `test_ENVIRON01_default_environment_evaluates_builtin` | `commands_doc` in a fresh environment, nothing registered | AC-2 |
| 5 | Test | `test_VALUE08_representative_extvalue_roundtrip` | DataFrame through `ns-pl/head-2` | AC-3 |
| 6 | Test | `test_VALUE01_primitive_roundtrip`, `test_VALUE03_integer_boundaries`, `test_VALUE04_bytes_are_not_confused_with_text` | the structural table | AC-4 |
| 7 | Test | `test_VALUE05_unknown_object_uses_opaque_value` | permissive vs strict | AC-5 |
| 8 | Test | `test_VALUE06_opaque_serialization_fails_or_uses_its_codec` | `store_set` with and without pickle; the pickle-on read returns the object, not `bytes` | AC-6 |
| 9 | Test | `test_ERROR03_language_exception_includes_class_and_stack` | `ValueError` through `evaluate` | AC-7 |
| 10 | Test | `test_ERROR06_language_raised_error_keeps_its_type` | `KeyNotFound` raised in Python | AC-8 |
| 11 | Test | `test_COMMAND05_metadata_matches_the_declaration` | derived metadata and refusals | AC-9 |
| 12 | Test | `test_COMMAND06_duplicate_and_unregister_policy` | late registration, replacement warnings | AC-10 |
| 13 | Test | `test_ENVIRON05_isolated_test_environments_do_not_leak_registration` | two environments plus the default | AC-11 |
| 14 | Test | `test_ENVIRON08_invalid_metadata_reports_before_failing` | `import_metadata` with an invalid entry | AC-12 |
| 15 | Test | `test_RUNTIME04_nested_evaluation_does_not_deadlock` | `outer` evaluates `hello/repeat-2` | AC-13 |
| 16 | Test | `test_EVAL_text_resource_read_from_store` | `-R/d/in.txt/-/repeat-2` | AC-17 |
| 17 | Test | `test_VALUE_pickle_off_never_unpickles` | two environments configured with one file store under `tmp_path` | AC-18 |
| 17a | Test | `test_COMMAND_late_registration_keeps_store` | `store_set`, late registration, then `-R/` read | AC-10 |
| 17b | Test | `test_RUNTIME_two_threads_evaluate_concurrently` | two Python threads, both finish | AC-13 |
| 18 | Test | `test_EVAL03_metadata_and_logs_available`, `test_EVAL06_cancellation_has_defined_terminal_result` | `get_asset`, `cancel()` | AC-19 |
| 19 | Test | `test_ENVIRON06_shutdown_is_idempotent` | shutdown twice, then evaluate | AC-20 |
| 20 | Test | the Appendix A suites (below) | every remaining ID, `NA`s skipped with reasons | AC-21 |
| 21 | Rust | `liquers-lib` `value::foreign::tests::decoder_rebuilds_registered_identifier` | the hook | AC-6 |
| 22 | Rust | `liquers-core` `assets::tests::formatless_registered_type_reads_degraded` | the read rule | AC-18 |
| 23 | Rust | `liquers-py` `value::tests::py_type_info_agrees_with_instance` | the two spellings agree | AC-6 |

## Example 1: A Python step in a pipeline

A data scientist adds a text step. Decorators register into the shared default environment. The
first `evaluate` builds the environment, the plan resolves `hello` and `repeat`, and each call runs
on a blocking thread under the GIL.

```python
import liquers as lq

@lq.first_command
def hello() -> str:
    return "hello"

@lq.command
def repeat(text: str, count: int = 2) -> str:
    """Repeat the input text."""
    return text * count

assert lq.evaluate("hello/repeat-3") == "hellohellohello"
meta = lq.default_environment().command_metadata("repeat")
assert meta.label == "Repeat" and meta.doc == "Repeat the input text."
assert [(a.name, a.default) for a in meta.arguments] == [("count", 2)]
```

The first parameter, `text`, is the state, so it is not an argument (`StateDelivery::Text`).

## Example 2: Python feeding polars

What this adds: an `ExtValue` crossing in both directions, with the Rust command in the middle.

```python
import polars as pl

env = lq.Environment()

@env.first_command
def make_frame() -> pl.DataFrame:
    return pl.DataFrame({"a": [1, 2, 3]})

df = env.evaluate("make_frame/ns-pl/head-2")
assert isinstance(df, pl.DataFrame) and df.height == 2
```

`env.command` and `env.first_command` are the decorators bound to one environment. `lq.command` is
the same decorator bound to `default_environment()`.

## Edge and Error Cases

| Symptom | Expected | Test |
|---|---|---|
| `int` 2⁶³ returned | `ConversionError`, never wrapped around | `test_VALUE03_integer_boundaries` |
| `True` returned | `Bool`, not `I64` (`bool` is checked before `int`) | `test_VALUE01_primitive_roundtrip` |
| `KeyboardInterrupt` raised in a command | `ExecutionError` naming the class, and the process keeps running | `test_ERROR04_non_error_throw_has_safe_fallback` |
| A panic inside a Rust command called from Python | a typed error, no abort | `test_RUNTIME06_panic_and_exception_containment` |
| A command replaced after an evaluation | the new code runs, stored results of the old one are not reused (`unregister` clears `impl_version`) | `test_COMMAND06_duplicate_and_unregister_policy` |
| A late registration rebuilds | earlier `store_set` data survives; an evaluation in flight finishes against the retired environment | `test_COMMAND_late_registration_keeps_store` (AC-10) |
| Two Python threads evaluate at once | both make progress (the GIL is released while waiting) | `test_RUNTIME_two_threads_evaluate_concurrently` |
| Callables after `unregister` and `shutdown` | the live-handle count returns to zero (`debug-handles`) | `test_RUNTIME05_cancellation_and_shutdown_release_handles` |
| A cycle in a returned `list` | `ConversionError` naming the cycle, no recursion overflow | `test_VALUE07_cycles_follow_policy` |

## Test Plan

- **pytest**, `liquers-py/tests/`, one file per feature. Test names are the Appendix A names.
  - `test_OBJECT_basic_objects.py` and `test_OBJECT_parser.py`: existing, extended with OBJECT05–09.
  - `test_ERROR_errors.py`: ERROR01–06 (AC-7, AC-8).
  - `test_RUNTIME_runtime.py`: RUNTIME01, 03–06 (AC-13), plus RUNTIME02 skipped with `na("no Pyodide target")`.
  - `test_VALUE_value_bridge.py`: VALUE01–13 (AC-3–6, AC-18), plus VALUE12 skipped with `na("no Value wrapper")`.
  - `test_ENVIRON_environment.py`: ENVIRON01–08 (AC-2, AC-11, AC-12, AC-20).
  - `test_EVAL_evaluation.py`: EVAL01–06 (AC-19), plus `test_EVAL_text_resource_read_from_store` (AC-17).
  - `test_COMMAND_registration.py`: COMMAND01–14 (AC-1, AC-9, AC-10).
- **Shared fixtures** in `conftest.py`, `register_fixture_commands(e)`:
  - `hello`, `number(n=0)`, `boom` (raises), `make_obj`, `same_obj`, `outer`, and a volatile sync
    `slow(seconds=60.0)` that counts its calls and **raises** once `context.is_cancelled()` is true
    (returning would end the run `Ready`);
  - `env` is an isolated `lq.Environment()` per test, shut down in teardown.
  - `slow` is sync here. The async variant is `liquers-py-async`'s.
- **EVAL06 names its result.** The queued `DefaultAssetManager` is used. `cancel()` on a `Submitted`
  asset gives `Status("Cancelled")` and the command never runs. The test sets
  `config={"assets": {"job_capacity": 1}}`, holds the slot with `slow-5`, asserts that `slow-6` is
  `Submitted`, cancels it, and checks the call counter. Distinct queries matter: a repeated query
  reuses the running asset. A cancelled `Processing` `slow` also ends `Cancelled`, via
  `is_cancelled`.
- **Rust:** the three unit tests in the table, in the named modules.
- **Commands:**
  - `bash scripts/test-liquers-py.sh` installs `polars` and `pillow` into its venv, builds with
    `--features debug-handles`, and runs the pytest suites and the liquers-py unit tests;
  - `cargo test -p liquers-lib --lib value::foreign`;
  - `cargo test -p liquers-core --lib assets::tests::formatless`.
- **Queries:** all were validated with `liquers-validate`, with fixture commands as `--command`. Each
  is `Ok`, and its encoded form is the query itself: `hello/repeat-3`, `hello/to_text`,
  `make_frame/ns-pl/head-2` (the alias `pl/head` → `pl/slice`, offset 0, n=2), `commands_doc`, `slow-6`, `-R/d/in.txt/-/repeat-2`, `-R/o/obj`,
  `make_obj/same_obj`, `outer`, `slow-5`, `boom`.

## Learning Log

- A first parameter named `state` delivers the `State` wrapper, not the value. The plan's §3.4 and
  the first Phase 1 draft both got this wrong, and the review caught it.
- A formatless foreign value is persisted as metadata only and never refused
  (`foreign-value-type-registration`). An opaque value that fails to serialize is therefore
  observable only through an explicit write. That is why AC-6 uses `store_set`.
