# Phase 3: Examples & Use-cases — Multi-language traceback

All under Phase 1's recommended answers. Queries validated with
`target/debug/liquers-validate --command outer_calls --command inner_fails -- 'outer_calls' 'inner_fails'`
(both `Ok`, encoded unchanged).

## Overview Table

| # | Kind | Name | Shows / checks | Scenarios |
|---|---|---|---|---|
| 1 | Example | Nested Python commands | Two attributed segments, rendered with headers | AC-1, AC-3 |
| 2 | Example | A language error thrown back | Segments survive Python and JavaScript `LiquersError` | AC-5, AC-6 |
| 3 | Test | `liquers_core::error::tests::single_traceback_segment_renders_bare_text` | One segment, no header | AC-2 |
| 4 | Test | `liquers_core::error::tests::traceback_segments_render_with_headers` | Header format and order | AC-3 |
| 5 | Test | `liquers_core::error::tests::flat_traceback_becomes_an_unknown_segment` | `with_traceback` then a segment | AC-7 |
| 6 | Test | `liquers_core::error::tests::traceback_segments_round_trip_and_old_errors_load` | JSON/YAML; no key → no segments | AC-4 |
| 7 | Test | `liquers_core::metadata::tests::error_traceback::metadata_record_from_error_persists_segments_in_json_and_yaml` | `LogEntry` and `error_data` | AC-4 |
| 8 | Test | `liquers-core/tests/traceback_segments.rs` `action_error_segments_are_attributed_to_their_command` | Interpreter attribution | AC-1, AC-6 |
| 9 | Test | `liquers-py/tests/test_ERROR_errors.py` `test_ERROR_nested_python_traceback_segments` | Problem example end to end | AC-1, AC-3 |
| 10 | Test | `test_ERROR_single_python_segment_renders_bare` | ERROR03 text unchanged | AC-2 |
| 11 | Test | `test_ERROR_liquers_error_segments_round_trip` | Segments set in Python come back plus one | AC-5 |
| 12 | Test | `test_ERROR_flat_traceback_kept_as_unknown_segment` | `LiquersError(traceback=…)` raised | AC-7 |
| 13 | Test | `test_ERROR_log_entry_has_traceback_segments` | `LogEntry.traceback_segments` | AC-4 |
| 14 | Test | `liquers-web/tests/objects_OBJECT.rs` `error_javascript_segment_on_the_evaluation_path` | `javascript` segment, attributed | AC-6 |
| 15 | Test | `liquers-web/tests/objects_OBJECT.rs` `error_thrown_back_liquers_error_keeps_segments` | JS round trip | AC-5 |

## Example 1: Nested Python commands

The problem example. `inner_fails` raises; its adapter converts the exception with one `python`
segment (no command); the interpreter running `inner_fails` attributes it. The error crosses into
Python as a `LiquersError` whose `traceback_segments` carry that segment, propagates through
`outer_calls`, and is converted back with a second `python` segment, which the outer interpreter
attributes to `outer_calls`.

```python
with pytest.raises(lq.ExecutionError) as raised:
    env.evaluate("outer_calls")
segments = raised.value.traceback_segments
assert [(s.language, s.command) for s in segments] == [
    ("python", "inner_fails"), ("python", "outer_calls")]
assert "ValueError: deep" in segments[0].text
tb = raised.value.traceback
assert tb.index("--- python (inner_fails) ---") < tb.index("--- python (outer_calls) ---")
```

## Example 2: A language error thrown back

A Python command raises a `LiquersError` that already carries a segment from elsewhere (a future
embedded language, or an error caught and re-raised):

```python
@env.first_command
def rethrows():
    raise lq.LiquersError(lq.ErrorType.ExecutionError, "nested",
        traceback_segments=(lq.TracebackSegment("rhai", "script", "at line 3"),))
# env.evaluate("rethrows") → segments: ("rhai", "script"), then ("python", "rethrows")
```

In JavaScript the same holds for a `LiquersError` a command rethrows: `js_value_as_liquers_error`
reads `tracebackSegments` back, and the command's own stack is appended as a `javascript` segment
attributed to it (`e.tracebackSegments.map(s => s.language)` is `["javascript", "javascript"]` for
a JavaScript command calling a failing JavaScript command).

## Edge and Error Cases

| Symptom | Expected | Test |
|---|---|---|
| Error with no traceback at all | No segments, no `traceback`; serialized form unchanged | 6 |
| Rust command fails, no language | `attribute_traceback` is a no-op | 8 |
| A segment already attributed passes another action | Not re-attributed | 8 |
| Python `traceback_segments` attribute is malformed | Ignored; flat `traceback` kept | 11 (second case) |
| A `LiquersError` never raised (`__traceback__` is `None`) | Passed back unchanged, no new segment (today's rule) | 11 |
| Old `error_data` / log entry without the key | Loads with an empty list | 6, 7 |

## Test Plan

- Unit: `liquers-core/src/error.rs` `tests` — tests 3-6 as named above.
- Unit: `liquers-core/src/metadata.rs` `tests::error_traceback` — test 7.
- Integration: `liquers-core/tests/traceback_segments.rs` — test 8. A `SimpleEnvironment<Value>` evaluated
  with `interpreter::evaluate`, as in `liquers-core/tests/async_hellow_world.rs`; command `boom` returns
  `Err(Error::execution_error(..).with_traceback_segment(TracebackSegment::new("python", "frames")))`;
  evaluating `boom` yields an error whose single segment has `command == Some("boom")`; a second
  command `relay` that evaluates `boom` through its context leaves the segment's command `boom`.
- Python: `liquers-py/tests/test_ERROR_errors.py` — tests 9-13. Test 13 evaluates `outer_calls`
  with `env.get_asset`, expects `asset.get()` to raise, and reads the last error entry of
  `asset.metadata.log()`.
- Web (Node loop): tests 14-15, registering a throwing command through `liquers.registerCommand`.
- Commands: `cargo test -p liquers-core --lib --tests`; `bash scripts/test-liquers-py.sh`;
  `cargo test -p liquers-web --target wasm32-unknown-unknown --features debug-handles`;
  `./liquers-web/scripts/check-stubs.sh --build`.

## Learning Log

- `liquers-web` never filled `Error::traceback`; the issue text assumed it did.
- The interpreter's action `map_err` is the one place that knows the command for every language
  without any binding change; `command_key` there is skipped when already set, attribution must not be.
- `pyerr_to_error` adds frames only when `err.traceback(py)` is set, which keeps a never-raised
  `LiquersError` lossless; the segment version keeps that rule.
