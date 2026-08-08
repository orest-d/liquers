# Phase 4: Implementation Plan - query-link-parser

## Overview

**Feature:** action-parameter link parsing (`~X~<query>~E`) — fixes
`QUERY-ACTION-PARAMETER-LINK-PARSER`

**Architecture:** three private productions plus an error mapping in
`liquers-core/src/parse.rs`; one doc comment in `query.rs`; two documentation files. No
new public types, no new commands, no signature changes, no new dependencies.

**Estimated complexity:** Low for the code (~90 lines), Medium for the test and
documentation surface (40 tests, 3 doc-verification items, 5 doc targets).

**Prerequisites:** Phases 1-3 approved. All open questions resolved. `nom = "8.0.0"` and
`nom_locate = "5.0.0"` already provide everything needed.

### Sequencing constraint

The tree is green at the end of every step except one. `parse.rs:1323` currently asserts
the bug (`assert!(parse_query("action-~X~hello~E").is_err())`), so the moment the parser
is wired in, that test fails. **Steps 4 and 5 are therefore a single commit** — wire the
production and re-baseline the contract test together. No step may be left half-applied.

Steps 1-3 add code that is not yet reachable, so they compile clean and change no
behavior. That is deliberate: it keeps the risky wiring step small and isolated.

### Two scoping decisions carried from earlier phases

Both are easy to lose in the step detail, so they are stated once here:

- **`parse_simple_template` gets the marker guard, not the error mapping.** Phase 1 open
  question 3 asked whether the improved error positions should extend to the other public
  entry points; Phase 2 scoped that to `parse_query`. The recursion guard is a separate
  concern and applies wherever `query_parser` is reachable, which includes templates.
- **`link_query` omits `simple_transform_query` deliberately.** Phase 2 D2 established
  that `general_query` subsumes it, verified over 15 canonical forms before the design was
  written. Do not "restore" it during implementation.

## Implementation Steps

### Step 1: Imports and the recursion bound

**File:** `liquers-core/src/parse.rs`

**Action:**
- add `cut` to the existing `nom::combinator` import (l. 174)
- add `use nom::error::ErrorKind;`
- add the `MAX_LINK_MARKERS` constant near the top of the parser section

**Code changes:**
```rust
// MODIFY (l. 174):
use nom::combinator::{cut, eof, not, opt, peek};

// NEW:
use nom::error::ErrorKind;

/// Maximum number of `~X~` link markers accepted in one query.
///
/// Links are the only recursive construct in the query grammar. Recursion depth is
/// bounded by the number of markers, so counting them before parsing bounds the
/// stack without any parser state. See `specs/query-link-parser` D5.
const MAX_LINK_MARKERS: usize = 64;
```

**Validation:**
```bash
cargo check -p liquers-core
# Expected: clean. MAX_LINK_MARKERS is unused so far; the file already has
# #![allow(dead_code)] at l. 167, so no warning.
```

**Rollback:** `git checkout liquers-core/src/parse.rs`

**Agent:** haiku · skills: — · knowledge: parse.rs l. 166-190 · *Rationale: mechanical
import edit against an exact line reference.*

---

### Step 2: The link productions

**File:** `liquers-core/src/parse.rs` (insert after `parameter`, before `minus_parameter`,
~l. 302)

**Action:** add `link_query`, `link_parameter`, `action_parameter` exactly as designed in
Phase 2 "Control Flow". Do not wire them yet.

**Code changes:**
```rust
/// The query grammar accepted between `~X~` and `~E`.
///
/// This is `query_parser` minus its two `eof`-gated alternatives, which can never
/// match before a `~E`. The resource/transform shorthand is rejected rather than
/// silently reinterpreted -- see the module docs.
fn link_query(text: Span) -> IResult<Span, Query> {
    // The nesting of peeks is deliberate, do not "simplify" it:
    //   - the inner peek(tag("~E")) asserts the shorthand accounts for the whole
    //     body without consuming the terminator, which link_parameter still needs;
    //   - the outer peek is defensive rather than necessary: the result is
    //     discarded by `.is_ok()` and `text` is a Copy local never rebound, so
    //     the error position is the body start either way. Kept to make the
    //     non-consuming intent explicit.
    if peek(terminated(resource_transform_query, peek(tag("~E"))))
        .parse(text)
        .is_ok()
    {
        return Err(nom::Err::Failure(nom::error::Error::new(
            text,
            ErrorKind::Verify,
        )));
    }
    alt((general_query, empty_query)).parse(text)
}

/// `link-parameter = "~X~", link-query, "~E"`
fn link_parameter(text: Span) -> IResult<Span, ActionParameter> {
    let position: Position = text.into();
    // Must fail softly: `alt` in action_parameter falls through to a string parameter.
    let (text, _) = tag("~X~")(text)?;
    // `cut` is defensive only: link_query cannot currently return Err::Error,
    // because empty_query never fails. See Phase 2 D4. Keep it so a future
    // failing path in link_query is committed rather than silently backtracked.
    let (text, query) = cut(link_query).parse(text)?;
    // Marked with ErrorKind::Fail so the message can name the missing terminator.
    // This is where a malformed body surfaces too -- see Phase 2 D4.
    let (text, _) = tag("~E")(text).map_err(|_: nom::Err<nom::error::Error<Span>>| {
        nom::Err::Failure(nom::error::Error::new(text, ErrorKind::Fail))
    })?;
    Ok((text, ActionParameter::Link(query, position)))
}

/// A single action parameter: a link, or a string parameter.
///
/// `parameter` wraps `many0` and so succeeds on empty input; it can never fail.
/// `link_parameter` must therefore come first or it would never run.
fn action_parameter(text: Span) -> IResult<Span, ActionParameter> {
    alt((link_parameter, parameter)).parse(text)
}
```

**Validation:**
```bash
cargo check -p liquers-core
cargo test -p liquers-core --lib
# Expected: compiles, all 387 tests still pass. Nothing calls these yet.
```

**Rollback:** `git checkout liquers-core/src/parse.rs`

**Agent:** sonnet · skills: rust-best-practices · knowledge: Phase 2 (Control Flow,
"How link_query works", D1-D4a), parse.rs in full · *Rationale: the load-bearing step. The
soft/hard failure boundary and the peek nesting are both easy to get subtly wrong, and
both fail silently rather than at compile time. Must read D4a before starting.*

---

### Step 3: Error position and message mapping

**File:** `liquers-core/src/parse.rs`

**Action:** add the two error helpers; add the marker guard and error mapping to
`parse_query`; add the marker guard to `parse_simple_template`.

**Code changes:** two points the implementer must not deviate on:

- `parse_query`'s complete-consumption branch (l. 758-767) keeps its **position**
  unchanged; its **message** becomes `describe_leftover(...)`. Both the `map_err` closure
  (l. 754-757) and that branch change.
- `parse_simple_template` gets the marker guard **only**, not the error mapping. Phase 2
  scoped the position work to `parse_query` (Phase 1 open question 3); the guard is
  separate and applies wherever `query_parser` is reachable.

**Three link diagnostics, two mechanisms.** `describe_query_failure` maps the two
`ErrorKind` markers (`Verify` = shorthand, `Fail` = terminator not matched). The new
`describe_leftover` diagnoses text the parser stopped before consuming — a stray `~E` is
not a parse failure, so it never reaches the nom error path. Both `ErrorKind` values are
free: searching `parse.rs` for `verify`, `fail` and `ErrorKind` returns no hits.

The guard goes at the top of each function, before any parsing, and the mapping replaces
the existing closure — spelled out here so neither has to be reconstructed from Phase 2:

```rust
pub fn parse_query(query: &str) -> Result<Query, Error> {
    // NEW: bound recursion before parsing (D5).
    if query.matches("~X~").count() > MAX_LINK_MARKERS {
        return Err(Error::query_parse_error(
            query,
            "Too many link parameters",
            &Position::unknown(),
        ));
    }

    let (remainder, path) = query_parser(Span::new(query)).map_err(|e| {
        // WAS: let message = format!("{}", e);
        //      Error::query_parse_error(query, &message, &Position::unknown())
        Error::query_parse_error(query, &describe_query_failure(&e), &nom_error_position(&e))
    })?;

    // CHANGED (l. 758-767): keeps its position, gains a diagnosis.
    if !remainder.fragment().is_empty() {
        let position: Position = remainder.into();
        return Err(Error::query_parse_error(
            query,
            describe_leftover(remainder.fragment()),
            &position,
        ));
    }
    Ok(path)
}

/// Diagnose text the parser stopped before consuming.
///
/// A stray `~E` is not a parse failure -- `parameter` halts, the action completes,
/// and the terminator is simply left over -- so it cannot be diagnosed from a nom
/// error. See D6.
fn describe_leftover(rest: &str) -> &'static str {
    if rest.starts_with("~E") {
        "Unpaired ~E: link terminator without a matching ~X~"
    } else if rest.starts_with("~X~") {
        "~X~...~E is only valid as a complete action parameter"
    } else {
        "Can't parse query completely"
    }
}
```

**Do not change the position** computed in that branch — it is already correct. Measured
on the current tree: `action-a~E` reports offset 8 (the `~`) and `action-abc~X~q~E`
reports offset 10. Only the message is new.

The same guard block goes at the top of `parse_simple_template`, using
`Error::general_error` to match that function's existing error style.

```rust
fn nom_error_position(err: &nom::Err<nom::error::Error<Span>>) -> Position {
    match err {
        nom::Err::Error(e) | nom::Err::Failure(e) => e.input.into(),
        nom::Err::Incomplete(_) => Position::unknown(),
    }
}

fn describe_query_failure(err: &nom::Err<nom::error::Error<Span>>) -> String {
    match err {
        nom::Err::Error(e) | nom::Err::Failure(e) => match e.code {
            // Private markers set in link_query / link_parameter. No other
            // production in this file uses `verify` or `fail`, so these codes
            // cannot arrive from anywhere else.
            ErrorKind::Verify => "Resource/transform shorthand is not allowed inside \
                 ~X~...~E; use the explicit form, for example -R/a/b/-/c"
                .to_owned(),
            // Covers both a missing terminator and a body that stopped early --
            // "here" is true of either. See Phase 2 D4.
            ErrorKind::Fail => "Expected ~E here to close ~X~".to_owned(),
            // ErrorKind is nom's enum, not ours: a catch-all arm is correct here.
            // Not "Can't parse query" -- query_parse_error already prefixes that.
            _ => "unexpected input".to_owned(),
        },
        nom::Err::Incomplete(_) => "Incomplete query".to_owned(),
    }
}
```

**Validation:**
```bash
cargo test -p liquers-core --lib
# Expected: still 387 passing. No behavior change yet -- the mapping only affects
# messages on inputs that already failed, and no test asserts those messages
# (verified by searching for the literals).
```

**Rollback:** `git checkout liquers-core/src/parse.rs`

**Agent:** sonnet · skills: rust-best-practices · knowledge: Phase 2 (D5, D6, Error
Handling), parse.rs l. 746-813, error.rs · *Rationale: touches three public entry points
and must preserve existing error paths exactly; requires judgment about what not to
change.*

---

### Step 4+5: Wire the production and re-baseline the contract test

**Files:** `liquers-core/src/parse.rs` (both changes in **one commit**)

**Action:**
1. point `minus_parameter` at `action_parameter`
2. invert the link clause of `documented_query_language_contract`

**Code changes:**
```rust
// MODIFY (l. 303-306):
fn minus_parameter(text: Span) -> IResult<Span, ActionParameter> {
    let (text, _) = tag("-")(text)?;
    action_parameter(text)          // WAS: parameter(text)
}

// MODIFY (l. 1322-1323):
// - // Link encoding exists in query.rs, but parse.rs has no link production.
// - assert!(parse_query("action-~X~hello~E").is_err());
let link_query = parse_query("action-~X~hello~E")?;
let link = &link_query.action().expect("action").parameters[0];
assert!(link.is_link());
assert_eq!(link.link_value().expect("link").encode(), "hello");
assert_eq!(link_query.encode(), "action-~X~hello~E");
```

**Validation:**
```bash
cargo test -p liquers-core --lib
# Expected: 387 passing. If documented_query_language_contract fails, the two
# halves of this step were not applied together.
cargo test -p liquers-core --doc
# Expected: the module-level rustdoc example still passes (it does not use links).
```

**Rollback:** `git checkout liquers-core/src/parse.rs` — reverts both halves, which is
exactly why they are one step.

**Agent:** sonnet · skills: rust-best-practices, liquers-unittest · knowledge: Phase 2 D7,
Phase 3 C9, parse.rs l. 295-330 and l. 1298-1325 · *Rationale: small diff, but it is the
moment behavior changes and the only step with a red window if mis-sequenced.*

---

### Step 6: Positive parser tests (group A)

**File:** `liquers-core/src/parse.rs`, `mod tests`

**Action:** add A1-A16 from Phase 3, using the Concrete Inputs table verbatim. Do not
invent inputs — every one is specified.

**A16 is the one to write carefully.** `action-~X~inner-a~~E~E` must parse with the body
`inner-a~~E`, not terminate at the first `~E`-looking byte sequence. It is the test that
defends the in-band design against a future refactor to delimiter scanning, which would
pass every other test in the suite.

**Standing rule for steps 6-8:** every test drives the parser through `parse_query` or
`parse_simple_template`. **No test may call `link_parameter`, `link_query` or
`action_parameter` directly** — they are private, and a test that reaches around the
public entry point would not exercise the `alt` ordering, the `cut` boundary or the marker
guard, which is where the behavior actually lives.

**Validation:**
```bash
cargo test -p liquers-core --lib parse::tests
# Expected: 15 new tests pass. A15 (UTF-8) is the one most likely to surprise:
# `café-~X~cmd~E` has `~X~` at byte offset 6 and column 6.
```

**Rollback:** `git checkout liquers-core/src/parse.rs`

**Agent:** haiku · skills: liquers-unittest · knowledge: Phase 3 (overview table, Concrete
Inputs, Examples 1-3), parse.rs `mod tests` · *Rationale: inputs and expected values are
fully specified; this is transcription against a table.*

---

### Step 7: Round-trip and equivalence tests (group B)

**File:** `liquers-core/src/parse.rs`, `mod tests`

**Action:** add B1-B6. B3/B4 iterate the 15-entry canonical corpus; B5 iterates the three
shorthand forms. Both corpora are listed verbatim in Phase 3 "Verified Data".

**Validation:**
```bash
cargo test -p liquers-core --lib parse::tests
# Expected: B3/B4 pass for all 15 entries -- this was measured before the design
# was written, so a failure means the implementation diverged from the design,
# not that the corpus is wrong.
```

**Rollback:** `git checkout liquers-core/src/parse.rs`

**Agent:** sonnet · skills: liquers-unittest, rust-best-practices · knowledge: Phase 3
("Verified Data", "B4's equivalence class"), query.rs `PartialEq`/`Hash` for
`ActionParameter` · *Rationale: B4's equivalence assertion is easy to write in a way that
passes vacuously; needs judgment about what is actually being compared.*

---

### Step 8: Error-path tests (group C)

**File:** `liquers-core/src/parse.rs`, `mod tests`

**Action:** add C1-C8, C8b, C10, C11. (C9 was done in step 4+5.) Use Phase 3's Concrete Inputs
table verbatim for C7, C8 and C10 — the marker-count structures in particular
(`a` + `-~X~q~E` × 64 accepts, × 65 rejects; `a-` + `~X~a-` × 65 + `~E` × 65 for the
nesting case) are easy to get off by one, and an off-by-one there means the test passes
for the wrong reason.

**C1, C3, C5 and C6 each assert a specific message and position**, not just `is_err()` —
they are the tests that prove unpaired delimiters are diagnosed rather than swept into the
generic "Can't parse query completely". C3 and C6 cover the two directions (missing `~E`
and stray `~E`), which reach the error through different mechanisms. C6 and C5 assert
offsets 8 and 10 respectively; those are what the parser reports on the current tree, so a
failure there means the leftover branch's position was changed, which step 3 forbids. If
any message assertion fails, reconcile with Phase 2 D6 rather than loosening it.

**Two constraints that must not be relaxed:**
- C7/C8 assert on the guard's **message**, not merely `is_err()` — otherwise they pass for
  the wrong reason.
- C8 must not attempt to trigger a real overflow. A genuine stack overflow aborts the
  process and cannot be caught; the test asserts the guard rejects *before* recursion.

**Validation:**
```bash
cargo test -p liquers-core --lib parse::tests
# Expected: all pass. C1 pins the D6 message; if it fails on message text,
# reconcile with Phase 2 D6 rather than loosening the assertion.
```

**Rollback:** `git checkout liquers-core/src/parse.rs`

**Agent:** sonnet · skills: liquers-unittest · knowledge: Phase 3 (Corner Cases → Errors
table, Recursion), Phase 2 D5/D6, error.rs public fields · *Rationale: error assertions
are where tests most often pass vacuously; the recursion test has a genuine footgun.*

---

### Step 9: Plan-level integration tests (group D1-D4)

**File:** `liquers-core/src/plan.rs`, `mod tests` (already imports `parse_query` and
`command_metadata::*` at l. 2136-2141)

**Action:** add D1-D4 — textual link → `ParameterValue::ParameterLink`, agreement with the
programmatic path, position propagation, dependency extraction.

**Validation:**
```bash
cargo test -p liquers-core --lib plan::tests
```

**Rollback:** `git checkout liquers-core/src/plan.rs`

**Agent:** sonnet · skills: liquers-unittest · knowledge: plan.rs l. 590-650 and
l. 1900-1950, dependencies.rs, Phase 3 group D · *Rationale: requires understanding how
`ParameterValue` is built and where dependencies are collected.*

---

### Step 10: End-to-end tests (group D5-D6)

**File:** `liquers-core/tests/action_parameter_link.rs` (new; underscores match the
existing files in that directory)

**Action:** add D5 and D6, modelled on `async_hellow_world.rs` — `SimpleEnvironment<Value>`,
`type CommandEnvironment` alias, `register_command!`, `evaluate`. Phase 3 Example 1 is the
template and was API-verified against the real `evaluate` signature.

**Validation:**
```bash
cargo test -p liquers-core --test action_parameter_link
# Expected: "Hello, world!" from `world/greet-~X~greeting~E`.
```

**Rollback:** `rm liquers-core/tests/action_parameter_link.rs`

**Agent:** sonnet · skills: liquers-unittest · knowledge: `liquers-core/tests/async_hellow_world.rs`,
Phase 3 Example 1, register_command! rules (sync takes `&State`, async takes owned) ·
*Rationale: new file, environment setup, async runtime — more room to go wrong than a
transcription step.*

---

### Step 11: `parse.rs` module documentation

**File:** `liquers-core/src/parse.rs` (module docs, l. 1-164)

**Action:** the six edits itemised in Phase 2 "parse.rs module docs — required edits":
delete the "Known link-parser bug" section (l. 59-66); add the `link-parameter` grammar;
clarify that `~X~`/`~E` are **not** entity-table rows; add a link section with the shared
shorthand wording and worked examples; add the `~E`-in-resource-name limitation; note
`MAX_LINK_MARKERS` under "Positions and errors".

**The examples go in a ```` ```rust ```` block, not ```` ```text ````** — that is test E1,
and it is what makes the documentation verifiable rather than assertion.

**Validation:**
```bash
cargo test -p liquers-core --doc
cargo doc -p liquers-core --no-deps
# Expected: doc examples pass; no broken intra-doc links.
```

**Rollback:** `git checkout liquers-core/src/parse.rs`

**Agent:** haiku · skills: — · knowledge: Phase 2 "Documentation Deliverables" (the exact
edit list and shared wording), parse.rs module docs · *Rationale: the wording is already
written in Phase 2; this is applying a specified edit list.*

---

### Step 12: `query.rs` doc comment

**File:** `liquers-core/src/query.rs` l. 536-540

**Action:** the `ActionParameter::Link` doc currently says the encoded form cannot be
parsed — the opposite of the new behavior. Replace with the accepted syntax, the shorthand
restriction, and a short runnable example (test E2).

**Validation:**
```bash
cargo test -p liquers-core --doc
```

**Rollback:** `git checkout liquers-core/src/query.rs`

**Agent:** haiku · skills: — · knowledge: Phase 2 Documentation Deliverables, query.rs
l. 531-545 · *Rationale: one doc comment plus a three-line example.*

---

### Step 13: API reference (doc-02)

**File:** `specs/reference/api/doc-02-query-language-reference.md`

**Action:** the eight per-section edits tabulated in Phase 2. The two that are easy to
miss:
- **delete** `### Link parameters do not parse` — leaving a "limitation" heading would
  contradict the new specification
- reword `## Coding-agent performance assessment` item 2, which currently lists
  "nested-query syntax" as something agents invent incorrectly

Add a dated `## Verification` entry (test E3) naming the test groups and the exact `cargo
test` invocations run, mirroring the existing 2026-07-26 entry. That folder's
factual-verification policy requires it.

**Validation:** manual read-through against Phase 2's table; confirm no remaining sentence
claims links do not parse:
```bash
grep -rn "do not parse\|does not parse\|no link production\|nested-query syntax" \
    specs/api-docs-analysis/ liquers-core/src/parse.rs liquers-core/src/query.rs
# Expected: no hits.
```

**Rollback:** `git checkout specs/reference/api/doc-02-query-language-reference.md`

**Agent:** sonnet · skills: — · knowledge: doc-02 in full, `specs/archive/2026-03-02-api-docs-gap-analysis.md`
(the verification policy), Phase 2 Documentation Deliverables · *Rationale: eight sections
with a compliance policy; requires judgment about what each claim now says.*

---

### Step 14: Close the issue

**File:** `specs/archive/2026-08-08-issues.md`

**Action:** mark `QUERY-ACTION-PARAMETER-LINK-PARSER` Resolved. Record which test covers
each of the issue's six Verification items (Phase 3 has the mapping). Note the two
behaviors the fix added beyond the issue: the shorthand restriction and the recursion
guard.

**Validation:** manual.

**Rollback:** `git checkout specs/archive/2026-08-08-issues.md`

**Agent:** haiku · skills: — · knowledge: Phase 3 coverage table, ISSUES.md l. 167-218 ·
*Rationale: transcription from an existing mapping.*

---

### Step 14b: Project overview consistency

**File:** `specs/reference/PROJECT_OVERVIEW.md`

**Action:** the two edits described under "Documentation Updates" below — move the
`~X~...~E` row out of the "Special encoding" table (l. 117), and resolve or narrow the
"Query encoding needs more careful design for embedded links" technical-debt entry
(l. 347).

**Validation:**
```bash
grep -n "~X~" specs/reference/PROJECT_OVERVIEW.md
# Expected: the link syntax is described, but not as a row in the entity table.
```

**Rollback:** `git checkout specs/reference/PROJECT_OVERVIEW.md`

**Agent:** sonnet · skills: — · knowledge: PROJECT_OVERVIEW.md l. 110-120 and l. 335-350,
Phase 2 Documentation Deliverables · *Rationale: the technical-debt entry needs a judgment
call about whether the fix discharges it fully or leaves a named residue.*

---

### Step 15: Full validation

**Action:** run the whole suite plus the acceptance demo.

**Validation:**
```bash
CARGO_INCREMENTAL=0 cargo test -p liquers-core --lib
CARGO_INCREMENTAL=0 cargo test -p liquers-core --tests
cargo test -p liquers-core --doc
cargo test -p liquers-lib --lib --tests      # the project's default loop; must be
                                             # unaffected -- no liquers-lib change

# Acceptance demo, through a real entry point:
cargo run -p liquers-core --features cli --bin liquers-validate -- \
    -- 'to_text-~X~-R/data/report/-/to_text~E'    # accepted
cargo run -p liquers-core --features cli --bin liquers-validate -- \
    -- 'to_text-~X~data/report/-/to_text~E'       # rejected, with the D6 message
```

**Expected:** all green; the two demo invocations show accept and reject with the worded
message. `specs/command_registry.yaml` needs no regeneration (no command signature
changed), so `cargo test -p liquers-lib --test registry_export` stays green untouched.

**Agent:** sonnet · skills: rust-best-practices · knowledge: all phase documents ·
*Rationale: final judgment call on whether the feature is complete and the docs match the
code.*

## Testing Plan

| When | Command | Gate |
|---|---|---|
| After each of steps 1-3 | `cargo test -p liquers-core --lib` | 387 passing, unchanged |
| After step 4+5 | `cargo test -p liquers-core --lib` | 387 passing, contract test inverted (C9 is that inversion, not a new test) |
| After steps 6-8 | `cargo test -p liquers-core --lib parse::tests` | +33 new fns: A(16) + B(6) + C minus C9(11); C9 was inverted in step 4+5, not added |
| After step 9 | `cargo test -p liquers-core --lib plan::tests` | +4 tests |
| After step 10 | `cargo test -p liquers-core --test action_parameter_link` | +2 tests |
| After steps 11-12 | `cargo test -p liquers-core --doc` | doc examples pass |
| Step 15 | full suite + demo | all green |

**Disk note.** `liquers-core` alone is a small build; the workspace-wide guidance in
CLAUDE.md about `cargo test --workspace` does not apply here. `cargo test -p liquers-lib
--lib --tests` in step 15 is the one heavier run — use `CARGO_INCREMENTAL=0`, and
`cargo clean` first only if disk is tight.

## Agent Assignment Summary

| Step | Model | Skills | Rationale |
|---|---|---|---|
| 1 | haiku | — | mechanical import edit |
| 2 | sonnet | rust-best-practices | load-bearing; `cut` boundary and peek nesting fail silently |
| 3 | sonnet | rust-best-practices | three public entry points; must preserve existing paths |
| 4+5 | sonnet | rust-best-practices, liquers-unittest | the behavior-change moment; must be atomic |
| 6 | haiku | liquers-unittest | transcription from a specified input table |
| 7 | sonnet | liquers-unittest, rust-best-practices | equivalence assertions can pass vacuously |
| 8 | sonnet | liquers-unittest | error assertions + the recursion footgun |
| 9 | sonnet | liquers-unittest | requires plan/dependency internals |
| 10 | sonnet | liquers-unittest | new file, env setup, async |
| 11 | haiku | — | applying a written edit list |
| 12 | haiku | — | one doc comment |
| 13 | sonnet | — | eight sections under a verification policy |
| 14 | haiku | — | transcription from Phase 3's mapping |
| 15 | sonnet | rust-best-practices | completeness judgment |

## Rollback Plan

**Per step:** each step lists its own `git checkout`. Steps 1-3 are behavior-neutral, so
reverting any of them leaves a working tree. Step 4+5 is atomic by construction.

**Partial completion.** The safe stopping points are: after step 3 (code present but
unreachable — ships harmlessly), after step 4+5 (feature works, untested), after step 10
(feature works and is tested, docs stale). **Stopping between 4+5 and 11 leaves the
documentation contradicting the code** — parse.rs would still carry the "Known
link-parser bug" section describing behavior that no longer exists. If work must stop
there, either finish step 11 or revert to step 3.

**Full rollback:**
```bash
git revert <first-commit>..<last-commit>     # preferred: preserves history
# or, if the branch is not shared:
git reset --hard <commit-before-step-1>
```

Nothing outside `liquers-core` and `specs/` is touched, so no downstream crate can be left
inconsistent.

## Documentation Updates

Beyond the four Phase 2 deliverables (steps 11-14):

- **`CLAUDE.md`** — no change. The link syntax is not a build, test or command-registration
  workflow, which is what that file covers.
- **`specs/reference/PROJECT_OVERVIEW.md`** — **not a no-op**, contrary to a first assumption; two
  places need attention and were found by grepping rather than reasoning:

  - **l. 117** lists `~X~...~E → nested query (embedded link)` inside the "Special
    encoding" table, alongside `~~`, `~_`, `~.` and the protocol abbreviations. This is
    the same category error Phase 2 identified in doc-02's entity table: `~X~`/`~E` are
    delimiters that select a parameter *kind*, not entities that decode to a character.
    Now that both other documents draw that distinction explicitly, leaving this row in a
    list titled "Special encoding" makes the overview the odd one out. Move it to its own
    line below the table with a pointer to `parse.rs`.
  - **l. 347**, under Technical Debt → Code Quality: *"Query encoding needs more careful
    design for embedded links."* This entry predates the fix and is at least partly
    discharged by it. Judgment call for the implementer, not a mechanical edit: decide
    whether it is now fully resolved (delete) or whether a residue remains — the
    `~E`-in-resource-name limitation is exactly such a residue — and reword to name that
    specific gap instead of the general one.

  Neither is required by CLAUDE.md's "update PROJECT_OVERVIEW when Query/Key encoding
  changes" rule, since encoding is genuinely unchanged. Both are needed to stop the
  overview contradicting the two documents this feature rewrites.
- **`specs/command_registry.yaml`** — not regenerated; no command signature changed.

## Execution Options

1. **Execute now** — work through steps 1-15 in order.
2. **Create a task list** — defer, with the steps as tasks.
3. **Revise** — return to any earlier phase.
4. **Exit** — implement manually from this plan.

---

## Implementation Findings (2026-08-06)

Executed on branch `claude/query-action-parameter-link-parser-ijoa47`. The plan above is
left as it was approved; this section records where reality differed. Two findings changed
the design, one invalidated a test input, and one step was relocated.

### 1. The depth bound was wrong by orders of magnitude — and was itself the hazard

**Designed:** `MAX_LINK_MARKERS = 64`, justified as bounding recursion depth, with C8b
added to check that depth 64 was survivable and the instruction "if C8b overflows, lower
the constant".

**Found:** C8b did not overflow. It *hung*. Parsing is **exponential in nesting depth**:

| depth | 10 | 12 | 14 | 15 | 16 | 17 |
|---|---|---|---|---|---|---|
| debug parse time | 32 ms | 139 ms | 0.54 s | 1.09 s | 2.26 s | 4.28 s |

The cause is a double parse per level, pre-existing and unrelated to links.
`transform_segment_without_header` runs `action_requests` first, which parses the action in
full — recursing through any nested link — then discards it when the required `/` separator
does not follow. `filename_or_action` immediately parses the same action again. So
`T(n) = 2·T(n-1)`.

At depth 64 the parse never finishes, which means **the guard as designed did not merely
fail to protect: it was the denial-of-service vector.** A ~200-byte query would hang the
parser.

**Changed:** two separate bounds, because the two dimensions have different costs —
siblings are linear and cheap, nesting is exponential and dangerous. A single count bound
must be sized for the exponential case and then over-restricts the cheap one.

```rust
const MAX_LINK_MARKERS: usize = 64;  // total links; siblings are cheap
const MAX_LINK_DEPTH: usize = 8;     // nesting; ~10 ms worst case
```

`link_bounds_exceeded` does one linear scan computing both, honouring the `~~` escape so an
escaped tilde before an `E` is not miscounted as a terminator (test C8c).

**This is exactly what C8b was written to catch**, and the only reason it was caught before
merge. The Phase 3 reasoning was right; only the predicted failure mode was wrong — hang,
not overflow. Recorded as follow-up `QUERY-LINK-EXPONENTIAL-BACKTRACKING`; removing the
depth bound requires restructuring the double parse, which is a change to the core grammar.

### 2. A15's input was impossible

**Designed:** `café-~X~cmd~E`, asserting `~X~` at byte offset 6 and column 6, to exercise
`get_utf8_column`.

**Found:** `café` does not parse at all. `identifier` tests `AsChar::is_alpha(c as u8)`, and
that cast truncates: `é` is U+00E9 → 233, which is not alpha. The query fails at offset 3.
Non-ASCII characters are not part of the query grammar anywhere — `resource_name`,
`parameter_text` and `header_parameter` all use the same truncating test — so a multi-byte
character can never precede a reported position, and `get_utf8_column` is not observable
through valid input.

**Changed:** A15 now pins the actual behavior — non-ASCII is rejected with a position at the
offending byte, and the ASCII prefix (`caf-~X~cmd~E`) parses links normally.

### 3. D4 could not live in the integration test file

`find_dependencies` is `pub(crate)`, so it is unreachable from `tests/`. D4 moved to
`plan.rs`'s test module as a `#[tokio::test]`, alongside D1-D3. The remaining end-to-end
tests (D5, D6, plus a shorthand rejection check) are in
`liquers-core/tests/action_parameter_link.rs` as planned.

### 4. Minor

- `parse_query`'s error message for a non-link failure was changed from `"Can't parse
  query"` to `"unexpected input"`, since `Error::query_parse_error` already prefixes
  `Can't parse query '<q>': `. Caught in review, applied here.
- One test bug of my own: B5 initially used `unwrap_or_else` on a `Result`, which panics on
  `Err` — so it fired precisely when the shorthand *was* correctly rejected. Fixed to
  `expect_err`.

### Result

| Check | Outcome |
|---|---|
| `cargo test -p liquers-core --lib` | **425 passed** (was 387; +34 parser, +4 plan) |
| `cargo test -p liquers-core --tests` | all integration binaries pass, including 3 new |
| `cargo test -p liquers-core --doc` | **5 passed** (was 3; +2 link examples) |
| `cargo doc -p liquers-core --no-deps` | no new warnings |
| Acceptance demo via `liquers-validate` | accepts the explicit form; rejects the shorthand and an unpaired `~E`, each with its worded message |

Steps 1-15 are complete, including all five documentation targets.

