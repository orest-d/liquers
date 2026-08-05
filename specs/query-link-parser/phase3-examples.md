# Phase 3: Examples & Use-cases - query-link-parser

## Example Type

**Runnable tests, not conceptual snippets.** This was not put to the user as a choice: the
issue's own Verification section specifies "Add parser and round-trip tests covering…" and
enumerates six cases. For a parser fix the examples *are* the tests, and a conceptual
snippet would verify nothing. Every code block below is intended to compile and run.

No `examples/*.rs` demo file is proposed. A link is a syntax feature, not an API to
demonstrate; its worked examples belong in the `parse.rs` module rustdoc (a Phase 2
documentation deliverable) where they are compiled by `cargo test --doc`.

## Overview Table

| # | Group | Test / example | Pins | File |
|---|---|---|---|---|
| A1 | Positive | `link_single_parameter` | `action-~X~hello~E` parses; `is_link()`; embedded `encode()` | parse.rs |
| A2 | Positive | `link_between_string_parameters` | `action-before-~X~hello~E-after`: 3 params, order and kinds | parse.rs |
| A3 | Positive | `link_at_every_parameter_position` | issue req. 5 — first, middle, last | parse.rs |
| A4 | Positive | `link_multi_segment_embedded_query` | `~X~-R/data/report/-/to_text~E` keeps resource+transform | parse.rs |
| A5 | Positive | `link_embedded_entities` | `~_`, `~.` decode *inside* the embedded query | parse.rs |
| A6 | Positive | `link_nested` | `action-~X~inner-~X~deep~E~E` (D4) | parse.rs |
| A7 | Positive | `link_empty_embedded_query` | `action-~X~~E` → empty query, not an error | parse.rs |
| A8 | Positive | `link_position_is_marker_offset` | link `position()` == byte offset of `~X~` | parse.rs |
| A9 | Positive | `link_inner_positions_are_absolute` | embedded nodes carry offsets in the *original* string | parse.rs |
| A10 | Positive | `link_inside_simple_template` | `$…$` template containing a link | parse.rs |
| A11 | Positive | `link_multiple_siblings` | two links in one action | parse.rs |
| B1 | Round-trip | `link_roundtrip_programmatic` | build → `encode` → parse → equal + identical text | parse.rs |
| B2 | Round-trip | `link_roundtrip_handwritten` | parse → encode → parse is stable | parse.rs |
| B3 | Round-trip | `link_body_canonical_corpus` | 15 canonical forms survive as link bodies | parse.rs |
| B4 | Equivalence | `link_body_matches_toplevel_meaning` | `parse(X) == link_body(X)` for non-shorthand X (D2) | parse.rs |
| B5 | Equivalence | `link_body_rejects_shorthand_corpus` | the *other* side of B4: shorthand X is rejected | parse.rs |
| B6 | Round-trip | `link_equality_and_hash` | parsed link == programmatic link; equal hashes | parse.rs |
| C1 | Negative | `link_shorthand_rejected` | ParseError + D6 message + position at body start (D3) | parse.rs |
| C2 | Negative | `link_explicit_resource_accepted` | contrast case: rejection is targeted, not blanket | parse.rs |
| C3 | Negative | `link_unterminated` | `action-~X~hello` → ParseError, useful position | parse.rs |
| C4 | Negative | `link_body_stops_early` | `action-~X~a b~E` → ParseError at the space | parse.rs |
| C5 | Negative | `link_concatenated_with_text` | `action-abc~X~q~E` rejected | parse.rs |
| C6 | Negative | `stray_link_terminator` | `action-a~E` rejected | parse.rs |
| C7 | Negative | `link_marker_guard` | > `MAX_LINK_MARKERS` → ParseError (D5) | parse.rs |
| C8 | Negative | `link_deep_nesting_does_not_overflow` | guard rejects *before* recursion (D5) | parse.rs |
| C9 | Replacement | `documented_query_language_contract` | the bug assertion inverted (D7) | parse.rs |
| D1 | Integration | `plan_textual_link_is_parameter_link` | text link → `ParameterValue::ParameterLink` | plan.rs |
| D2 | Integration | `plan_textual_and_programmatic_links_agree` | both paths produce the same plan | plan.rs |
| D3 | Integration | `plan_link_position_propagates` | plan carries the link's real position | plan.rs |
| D4 | Integration | `link_becomes_parameter_link_dependency` | `DependencyRelation::ParameterLink` | plan.rs |
| D5 | Integration | `evaluate_query_with_textual_link` | end-to-end evaluation | tests/ |
| D6 | Integration | `evaluate_nested_textual_link` | end-to-end with nesting | tests/ |
| E1 | Docs | rustdoc examples in `parse.rs` | compiled by `cargo test --doc` | parse.rs |

Drafted in parallel by four agents (positive, round-trip/equivalence, errors, integration)
and consolidated here; the drafts overlapped heavily on nesting and positions, which are
deduplicated above. Full drafts are working material, not deliverables.

## What is genuinely new, and therefore what matters most

`ActionParameter::Link` and its whole downstream path already exist and already work — the
planner builds `ParameterValue::ParameterLink` from it, dependencies track it, renderers
render it. **The only thing that was impossible was reaching any of it from query text.**

So the tests that carry the most weight are not the ones checking that links work; they
are:

1. **B4/B5** — that an embedded query means the same thing it means at top level, or is
   rejected. This is the correctness property the whole design turns on.
2. **D1/D2** — that the newly-reachable textual path lands in exactly the same plan the
   programmatic path already produced.
3. **C7/C8** — that the recursion this feature introduces cannot take the process down.

## Example 1: a link supplying a command argument

**Scenario.** A command needs an argument that is itself the result of a query.

```rust
#[tokio::test]
async fn evaluate_query_with_textual_link() -> Result<(), Box<dyn std::error::Error>> {
    type CommandEnvironment = SimpleEnvironment<Value>;
    let mut env = SimpleEnvironment::<Value>::new();

    fn world(_state: &State<Value>) -> Result<Value, Error> {
        Ok(Value::from("world"))
    }
    fn greeting(_state: &State<Value>) -> Result<Value, Error> {
        Ok(Value::from("Hello"))
    }
    async fn greet(state: State<Value>, greet: String) -> Result<Value, Error> {
        let what = state.try_into_string()?;
        Ok(Value::from(format!("{greet}, {what}!")))
    }

    let cr = &mut env.command_registry;
    register_command!(cr, fn world(state) -> result)?;
    register_command!(cr, fn greeting(state) -> result)?;
    register_command!(cr, async fn greet(state, greet: String = "Hello") -> result)?;

    let envref = env.to_ref();

    // The `greet` argument is not a literal -- it is the result of the query `greeting`.
    let state = evaluate(envref.clone(), "world/greet-~X~greeting~E", None).await?;
    assert_eq!(state.try_into_string()?, "Hello, world!");
    Ok(())
}
```

**Why this shape.** It mirrors `liquers-core/tests/async_hellow_world.rs` (the same
`world`/`greet` commands and `SimpleEnvironment<Value>`), so it needs no new fixtures and
no liquers-lib namespace — consistent with the Phase 2 decision that no namespace is
relevant to a parser change.

**Before this fix** `world/greet-~X~greeting~E` fails to parse. That single line is the
feature.

## Example 2: the shorthand, both ways

**Scenario.** A user writes a link whose body addresses a stored resource.

```rust
#[test]
fn link_shorthand_rejected() -> Result<(), Box<dyn std::error::Error>> {
    // Accepted: explicit resource header.
    let q = parse_query("to_text-~X~-R/data/report/-/to_text~E")?;
    let link = &q.action().expect("action").parameters[0];
    assert!(link.is_link());
    assert_eq!(
        link.link_value().expect("link").encode(),
        "-R/data/report/-/to_text"
    );

    // Rejected: the same query written in the shorthand.
    let err = parse_query("to_text-~X~data/report/-/to_text~E")
        .expect_err("shorthand must be rejected inside a link");
    assert_eq!(err.error_type, ErrorType::ParseError);
    assert!(err.message.contains("-R/"), "message must name the fix: {}", err.message);
    // Position points at the first character of the link body.
    assert_eq!(err.position.offset, "to_text-~X~".len());
    Ok(())
}
```

**Expected failure message** (Phase 2 D6):

```
Resource/transform shorthand is not allowed inside ~X~...~E;
use the explicit form, for example -R/a/b/-/c
```

**What it would have meant without the guard** — `-R/data/report/-/to_text` (read the
stored resource, convert it) versus `data/report/-/to_text` (run three commands). Both
parse; only the rejection makes the difference visible.

## Example 3: nesting and position arithmetic

```rust
#[test]
fn link_inner_positions_are_absolute() -> Result<(), Box<dyn std::error::Error>> {
    //            0         1         2
    //            0123456789012345678901
    let text =   "action-~X~cmd-param~E";
    let q = parse_query(text)?;
    let link = &q.action().expect("action").parameters[0];

    // The link's own position is the `~X~` marker.
    assert_eq!(link.position().offset, 7);
    assert_eq!(link.position().line, 1);
    assert_eq!(link.position().column, 8);   // columns are 1-based

    // Nodes inside the embedded query carry offsets in the ORIGINAL string,
    // not offsets relative to the body. `cmd` starts at 10.
    let inner = link.link_value().expect("link");
    assert_eq!(inner.action().expect("inner action").position.offset, 10);
    Ok(())
}
```

**Position arithmetic is determinate, not a guess.** `From<Span> for Position`
(parse.rs:191-199) takes `offset` from `location_offset()` (0-based), `line` from
`location_line()` and `column` from `get_utf8_column()` (both 1-based). In-band parsing
means the body is never re-spanned, so inner offsets are absolute for free — this test is
what would catch a future refactor to a slice-and-reparse design.

## Verified Data

Facts below were measured against the current tree, not inferred. Both temporary tests
were reverted after measurement.

### The canonical corpus is safe as link bodies

Every string `Query::encode()` can emit must survive as a link body, or requirement 4
fails. Running the exact D3 detector expression over the corpus:

```
fires=false  (empty)                              fires=false  -R
fires=false  /                                    fires=false  -R/a/b
fires=false  abc-def                              fires=false  -R/a/b/-/c
fires=false  action-                              fires=false  -R-meta/-/dr
fires=false  file.txt                             fires=false  -R/abc/def/-/ghi/jkl/file.txt
fires=false  ghi/jkl/file.txt                     fires=false  -x/ghi/jkl/file.txt
fires=false  abc/def/-/xxx/-q/qqq                 fires=false  /--R-meta-extra/data/input.csv
                                                  fires=false  -R/x/y/-R/a/b/-/c/d
--- known shorthand, must fire ---
fires=true   abc/def/-/xxx    fires=true   data/report/-/to_text    fires=true   a/b/-/c
```

All 15 canonical entries pass; all three shorthand forms are caught. **Test B3 must
therefore assert acceptance for all 15, and B5 must assert rejection for the three.**

One correction worth recording, because the wrong reason would mislead a future reader.
`abc/def/-/xxx/-q/qqq` does not fire — but *not* because "the `/` is followed by `-` so
it is an explicit header". `resource_transform_query` matches its prefix `abc/def/-/xxx`
perfectly well. It does not fire because the remaining text is `/-q/qqq~E`, so the inner
`peek(tag("~E"))` fails. Compare `abc/def/-/xxx` alone, which *does* fire. The detector
keys on **whether the shorthand accounts for the entire body**, not on the presence of a
header.

## Corner Cases

### Memory

Not a concern. Parsing borrows `&str`; `Span` is a `Copy` wrapper. A link allocates only
the `Query` it produces. No large-input path exists — see the recursion note below for the
one real bound.

### Concurrency

Not a concern. All productions are pure functions of their input; the `MAX_LINK_MARKERS`
guard is deliberately a pure function of the input string rather than a thread-local
(Phase 2 D5), so there is no per-thread state and no wasm caveat. No test needed beyond
noting this.

### Recursion — the one genuine hazard

Links are the first recursive construct in the query grammar. **A real stack overflow
aborts the process and cannot be caught by `#[should_panic]` or `catch_unwind`** — this
was the sharpest observation to come out of drafting, and it dictates the test design:

- **C7** exercises the guard directly: 65 `~X~` markers → `ParseError`, no recursion
  entered.
- **C8** must *not* attempt to trigger an overflow. It asserts that input which would
  recurse deeply is rejected by the marker count **before** parsing begins. A test that
  tried to prove the overflow is real would abort the suite when the guard works.
- A boundary test at exactly 64 markers documents that the limit is inclusive.

### Errors

Resolved here, since the drafts left three cases as "depends on the grammar". Each is
determinate from the productions:

| Input | Mechanism | Outcome |
|---|---|---|
| `action-a~E` | `parameter` = `many0(alt((parameter_text, entities)))`; `~E` matches neither, so it halts leaving `~E` unconsumed | `ParseError`, "Can't parse query completely" at the `~E` |
| `action-~X~a b~E` | space is not in `parameter_text`; body parse stops at it; `cut(tag("~E"))` then fails | `ParseError` (hard failure, no backtrack) at the space |
| `action-abc~X~q~E` | `parameter` consumes `abc`, halts at `~`; `many0(minus_parameter)` needs `-`; `~X~q~E` left over | `ParseError` at the `~X~` |

The third is how "a link is a whole parameter, never concatenated" is enforced — it falls
out of the existing grammar and needs no new rule.

### Serialization

`ActionParameter` already derives `Serialize, Deserialize` and `Link` already round-trips
through serde. This feature changes only the textual read path, so no new serde test is
warranted. (One drafter suggested liquers-py needs coverage because `Link` is a "new
variant" — it is not new; `liquers-py/src/query.rs:147-148` has always handled it.)

### Integration

Covered by group D. Neither liquers-py nor liquers-axum needs a test here: py wraps
`ActionParameter` opaquely and axum passes query text to `parse_query` unchanged, so
"link queries stop being rejected" is fully covered by the core parser tests.

## Test Plan

### Unit tests — `liquers-core/src/parse.rs`, `mod tests`

Groups A, B, C above (26 tests). Existing style: `#[test] fn name() -> Result<(), Error>`
or `-> Result<(), Box<dyn std::error::Error>>` where `?` is used on mixed error types.

**C9 replaces, does not delete**, the link clause of `documented_query_language_contract`:

```rust
// BEFORE (records the bug):
// assert!(parse_query("action-~X~hello~E").is_err());

// AFTER:
let link_query = parse_query("action-~X~hello~E")?;
let link = &link_query.action().expect("action").parameters[0];
assert!(link.is_link());
assert_eq!(link.link_value().expect("link").encode(), "hello");
assert_eq!(link_query.encode(), "action-~X~hello~E");
```

### Integration tests

**`liquers-core/src/plan.rs`, `mod tests`** — D1-D4. The module already imports
`parse_query` and `command_metadata::*` (plan.rs:2136-2141), so these need no new fixtures.

**`liquers-core/tests/action_parameter_link.rs`** (new) — D5, D6. Modelled on
`async_hellow_world.rs`: `SimpleEnvironment<Value>`, `type CommandEnvironment` alias,
`register_command!`, `evaluate`.

### Manual validation

```bash
cargo test -p liquers-core --lib parse::          # groups A, B, C
cargo test -p liquers-core --lib plan::           # group D1-D4
cargo test -p liquers-core --test action_parameter_link
cargo test -p liquers-core --doc                  # E1: rustdoc examples
cargo run -p liquers-core --features cli --bin liquers-validate -- \
    -- 'to_text-~X~-R/data/report/-/to_text~E'    # accepted
cargo run -p liquers-core --features cli --bin liquers-validate -- \
    -- 'to_text-~X~data/report/-/to_text~E'       # rejected, with the D6 message
```

The two `liquers-validate` invocations double as the acceptance demo: they show the
feature working through a real entry point, and they exercise the D6 message end to end.

### Regression surface

`cargo test -p liquers-core --lib` must stay green apart from the deliberate C9 inversion.
No currently-passing test other than `documented_query_language_contract` asserts anything
about `~X~`, verified by searching the tree for the marker.

## Coverage against the issue's Verification section

| Issue verification item | Test |
|---|---|
| 1. A single link: `action-~X~hello~E` | A1, C9 |
| 2. A link between strings | A2, A3 |
| 3. An embedded multi-segment query | A4 |
| 4. An embedded query containing encoded parameter entities | A5 |
| 5. Malformed and missing `~E` delimiters | C3, C4, C5, C6 |
| 6. Source positions for the link and embedded query | A8, A9, D3 |
| "the rejection test … should be replaced" | C9 |

Two areas go beyond the issue's list, both because the design created them: the shorthand
restriction (C1, C2, B5) and the recursion guard (C7, C8).
