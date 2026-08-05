# Phase 4: Implementation Plan - query-link-parser

## Overview

**Feature:** action-parameter link parsing (`~X~<query>~E`) — fixes
`QUERY-ACTION-PARAMETER-LINK-PARSER`

**Architecture:** three private productions plus an error mapping in
`liquers-core/src/parse.rs`; one doc comment in `query.rs`; two documentation files. No
new public types, no new commands, no signature changes, no new dependencies.

**Estimated complexity:** Low for the code (~90 lines), Medium for the test and
documentation surface (37 tests, 4 doc targets).

**Prerequisites:** Phases 1-3 approved. All open questions resolved. `nom = "8.0.0"` and
`nom_locate = "5.0.0"` already provide everything needed.

### Sequencing constraint

The tree is green at the end of every step except one. `parse.rs:1323` currently asserts
the bug (`assert!(parse_query("action-~X~hello~E").is_err())`), so the moment the parser
is wired in, that test fails. **Steps 4 and 5 are therefore a single commit** — wire the
production and re-baseline the contract test together. No step may be left half-applied.

Steps 1-3 add code that is not yet reachable, so they compile clean and change no
behavior. That is deliberate: it keeps the risky wiring step small and isolated.

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
    //   - the outer peek discards the parse entirely, so `text` still points at the
    //     start of the body and the error position is the start of the offending
    //     query rather than its end.
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
    // Committed from here: a malformed link is an error, not a backtrack.
    let (text, query) = cut(link_query).parse(text)?;
    let (text, _) = cut(tag("~E")).parse(text)?;
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
"How link_query works", D1-D4), parse.rs in full · *Rationale: the load-bearing step. The
`cut` boundary and the peek nesting are both easy to get subtly wrong, and both are silent
failures rather than compile errors.*

---

### Step 3: Error position and message mapping

**File:** `liquers-core/src/parse.rs`

**Action:** add the two error helpers; add the marker guard and error mapping to
`parse_query`; add the marker guard to `parse_simple_template`.

**Code changes:** as specified in Phase 2 "`parse_query` (modified)". Two points the
implementer must not deviate on:

- `parse_query`'s existing complete-consumption branch (l. 758-767) is **unchanged**. Only
  the `map_err` closure at l. 754-757 changes.
- `parse_simple_template` gets the marker guard **only**, not the error mapping. Phase 2
  scoped the position work to `parse_query`; the guard is separate and applies wherever
  `query_parser` is reachable.

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
            // Private marker set by link_query. No other production in this file
            // uses `verify`, so this code cannot arrive from anywhere else.
            ErrorKind::Verify => "Resource/transform shorthand is not allowed inside \
                 ~X~...~E; use the explicit form, for example -R/a/b/-/c"
                .to_owned(),
            // ErrorKind is nom's enum, not ours: a catch-all arm is correct here.
            _ => "Can't parse query".to_owned(),
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

**Action:** add A1-A15 from Phase 3, using the Concrete Inputs table verbatim. Do not
invent inputs — every one is specified.

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

**Action:** add C1-C8, C10, C11. (C9 was done in step 4+5.)

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

**File:** `specs/api-docs-analysis/doc-02-query-language-reference.md`

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

**Rollback:** `git checkout specs/api-docs-analysis/doc-02-query-language-reference.md`

**Agent:** sonnet · skills: — · knowledge: doc-02 in full, `specs/api-docs-analysis/README.md`
(the verification policy), Phase 2 Documentation Deliverables · *Rationale: eight sections
with a compliance policy; requires judgment about what each claim now says.*

---

### Step 14: Close the issue

**File:** `specs/ISSUES.md`

**Action:** mark `QUERY-ACTION-PARAMETER-LINK-PARSER` Resolved. Record which test covers
each of the issue's six Verification items (Phase 3 has the mapping). Note the two
behaviors the fix added beyond the issue: the shorthand restriction and the recursion
guard.

**Validation:** manual.

**Rollback:** `git checkout specs/ISSUES.md`

**Agent:** haiku · skills: — · knowledge: Phase 3 coverage table, ISSUES.md l. 167-218 ·
*Rationale: transcription from an existing mapping.*

---

### Step 14b: Project overview consistency

**File:** `specs/PROJECT_OVERVIEW.md`

**Action:** the two edits described under "Documentation Updates" below — move the
`~X~...~E` row out of the "Special encoding" table (l. 117), and resolve or narrow the
"Query encoding needs more careful design for embedded links" technical-debt entry
(l. 347).

**Validation:**
```bash
grep -n "~X~" specs/PROJECT_OVERVIEW.md
# Expected: the link syntax is described, but not as a row in the entity table.
```

**Rollback:** `git checkout specs/PROJECT_OVERVIEW.md`

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
| After step 4+5 | `cargo test -p liquers-core --lib` | 387 passing, contract test inverted |
| After steps 6-8 | `cargo test -p liquers-core --lib parse::tests` | +32 tests |
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
- **`specs/PROJECT_OVERVIEW.md`** — **not a no-op**, contrary to a first assumption; two
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
