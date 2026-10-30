# Phase 2: Solution & Architecture — Multi-language traceback

Written under Phase 1's recommended answers (Q1 (b), Q2 none, Q3 as stated, Q4 as stated).

## Overview

`ErrorPayload` gains `traceback_segments: Vec<TracebackSegment>`, innermost first. Two methods keep
`traceback` equal to the segments' rendering: `with_traceback_segment` appends one, and
`attribute_traceback` fills the command of segments that have none. The interpreter attributes at
the one place it already enriches an action's error, so a binding creating a segment does not need
to know which command it is in. `LogEntry` copies the segments. Python and JavaScript create a
segment where they format a stack today, carry the list through their `LiquersError`, and expose it.

Rejected: a separator in the string only (Q1 (a): text to parse); a language-context field (Q1 (c):
declined by the maintainer); making `traceback` a computed getter with no stored field (it is `pub`,
read in three crates and persisted; a stored derived copy changes nothing for them); native Rust
backtraces (Q2); a segment per *frame* (languages format frames differently; one block per stack is
what every producer already has).

## Known-Issue Preflight

| Issue | Status | Priority | Impact on this design | Fix first? | Blocking? | Action |
|---|---|---|---|---|---|---|
| `LANGUAGE-EXCEPTION-FIELDS-LOST-IN-TRANSPORT` | accepted | P3 | The JavaScript stack is in `message`, not `traceback` | Yes, before step 6 | No (core and Python steps are independent) | Requires; step 6 builds on it |
| `ERROR-WITH-KEY-SETS-QUERY-FIELD` (`error-with-key-field`) | draft | P2 | Also edits `ErrorPayload` and the action error mapping | No | No | Rebase whichever lands second |
| `ERROR-TYPES-TOO-COARSE-AND-GENERAL-OVERUSED` (`error-type-taxonomy`) | draft | P2 | Renames/removes variants; makes `Error::new` private | No | No | Use typed constructors only |
| `PY-METADATA-METHODS-RAISE-PLAIN-EXCEPTION` (`py-metadata-typed-errors`) | draft | P3 | Edits `liquers-py/src/error.rs` and `metadata.rs` | Yes, before step 5 | No | Step 5 rebases on it |
| `PY-METADATA-FIELDS-ARE-METHODS` (`py-metadata-fields-as-properties`) | draft | P3 | `metadata.log()` may become a property | No | No | Later one updates test 13 |
| `WEB-THROWN-BACK-LIQUERSERROR-LOSES-CONTEXT` (`web-thrown-back-error-context`): `js_value_as_liquers_error` drops `query`/`key`/`position` | draft | P3 | A thrown-back error loses fields; same function as step 6 (T3, kept apart by E1) | No | No | Step 6 restores segments and `traceback` only; second to land rebases |

## Interfaces

`liquers-core/src/error.rs` (sync; owned `String`s, because errors cross threads and bindings):

```rust
/// One language's stack, as that language formats it.
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq)]
pub struct TracebackSegment {
    pub language: String,                    // "python", "javascript"; "" = unknown
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub command: Option<String>,             // command label: `name`, `ns/name`, `realm:ns/name`
    pub text: String,
}
impl TracebackSegment {
    pub fn new(language: impl Into<String>, text: impl Into<String>) -> Self;
}

// ErrorPayload, after `traceback`:
#[serde(default, skip_serializing_if = "Vec::is_empty")]
pub traceback_segments: Vec<TracebackSegment>,

impl Error {
    /// Appends a segment (innermost first) and re-renders `traceback`. A flat `traceback` with
    /// no segments is first kept as a segment of unknown language (AC-7).
    pub fn with_traceback_segment(self, segment: TracebackSegment) -> Self;
    /// Fills `command` of every segment that has none, then re-renders.
    pub fn attribute_traceback(self, command: &CommandKey) -> Self;
    // `with_traceback(text)` unchanged in signature; now also clears `traceback_segments`.
}
fn render_traceback(segments: &[TracebackSegment]) -> String;   // private
```

Rendering (AC-2, AC-3): one segment → its `text`. Two or more → for each, a line
`--- <language> (<command>) ---` (`unknown` for an empty language, the parenthesis omitted when
`command` is `None`), then its text, joined by `\n`. `command_label` (already private in
`error.rs`) gives the label.

`liquers-core/src/metadata.rs` `LogEntry`: `#[serde(default, skip_serializing_if = "Vec::is_empty")]
pub traceback_segments: Vec<TracebackSegment>`; `LogEntry::from_error` copies it; `Default` adds
an empty list. No new command; no `ExtValue` variant.

`liquers-py`: `python/liquers/_errors.py` gains `TracebackSegment = namedtuple("TracebackSegment",
"language command text")`, exported as `lq.TracebackSegment`; `LiquersError.__init__` gains
keyword `traceback_segments=()` stored as a tuple; `_from_native` gains a seventh argument.
`liquers-py/src/metadata.rs` `LogEntry` gains a read-only getter `traceback_segments`.

`liquers-web`: `LiquersError` gains getter `tracebackSegments`, independent of `jsClass` / `jsStack` (left as
`LANGUAGE-EXCEPTION-FIELDS-LOST-IN-TRANSPORT` leaves them: alias or removal is its decision) (`unchecked_return_type =
"LiquersTracebackSegment[]"`), declared in `src/typescript.rs` as
`interface LiquersTracebackSegment { language: string; command?: string; text: string }`.

## Integration Points

- `liquers-core/src/error.rs`: type, field, `ErrorPayload::new` (empty list), methods, tests.
- `liquers-core/src/interpreter.rs`: in the `execute_async(...).map_err` closure, call
  `attribute_traceback(&command_key)` on every error (independently of the `command_key.is_none()`
  test, so an error that lost `command_key` crossing Python is still attributed).
- `liquers-core/src/metadata.rs`: `LogEntry` field, `from_error`, `Default`.
- `liquers-py/src/error.rs`: `error_to_pyerr` passes the segments as a list of tuples;
  `liquers_error_fields` reads `traceback_segments` back; `pyerr_to_error` replaces the string
  concatenation and both `with_traceback` calls by `with_traceback_segment(TracebackSegment::new(
  "python", frames))`. The cancellation branch does the same.
- `liquers-web/src/error.rs`: after `LANGUAGE-EXCEPTION-FIELDS-LOST-IN-TRANSPORT`,
  `js_error_to_liquers` creates a `javascript` segment instead of a flat traceback;
  `js_value_as_liquers_error` reads `traceback` and `tracebackSegments` back reflectively.
- Unchanged consumers: `liquers-axum/src/api_core/error.rs` `error_to_detail` (splits the derived
  string), `liquers-core/src/assets.rs` (persists `Error` and `LogEntry` through serde).

## Error Handling

No new failure. A malformed `traceback_segments` attribute on a Python or JavaScript error (not a
sequence of three-field items) is ignored with the flat `traceback` kept, never an error: losing
structure is better than replacing the user's error with a conversion error.

## Relevant Commands

None added or changed; `specs/command_registry.yaml` does not change. Namespaces: none.

## Documentation Architecture

| Path | Kind | Change |
|---|---|---|
| `specs/reference/PYTHON_INTEGRATION.md` §6 | reference | Segments, rendering, attribution; drop "is open" |
| `specs/guides/PYTHON_GUIDE.md` (error fields table) | guide | `traceback_segments` row; `traceback` described as derived |
| `specs/guides/LANGUAGE-INTEGRATION_GUIDE.md` §ERROR, §6 Python | guide | Answer the traceback design question: one segment per crossing, appended where the stack is formatted |
| `liquers-py/README.md`, `liquers-web/README.md` | readme | `ERROR` row notes |

`affects_docs`: the three `specs/` documents above. `specs/README.md`: this design's row.

## Risks

| Assessment | Finding |
|---|---|
| Files likely to change | `liquers-core/src/{error,metadata,interpreter}.rs`; `liquers-py/src/{error,metadata}.rs`, `python/liquers/{_errors,__init__}.py`; `liquers-web/src/{error,typescript}.rs` |
| Crates and workflows affected | `liquers-core`, `liquers-py`, `liquers-web`; native, Python and wasm loops |
| Existing tests likely to change | None should fail: `test_ERROR_nested_python_tracebacks_are_concatenated` checks substrings and order, which the headers keep; `traceback_round_trips_as_a_flat_key` uses no segments |
| New validation | Phase 3 tests; `check-stubs.sh --build` for the `.d.ts` |
| Compatibility / data | Additive serde; empty lists not written. The multi-segment string gains header lines (observable, intended) |
| Concurrency / performance | One `Vec` in the boxed payload; rendering only on the error path |
| Security | Tracebacks already reach logs and HTTP; no new exposure |
| Recovery | Each step reverts on its own; core without bindings is inert |
| Certainty | High for core and Python (code read); medium for web (depends on the prerequisite's final shape) |
