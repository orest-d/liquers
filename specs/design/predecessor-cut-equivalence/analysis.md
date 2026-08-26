# Analysis: where cutting and expanding still disagree

## How this was measured

`cut_predecessor` has no production caller, so the divergences are only visible when it is
forced on. The measurement adds three lines to `finalize_plan`, immediately after the
volatility and expiration passes and therefore after freezing:

```rust
if std::env::var("LQ_FORCE_CUT").is_ok() {
    plan.cut_predecessor()?;
}
```

then runs `LQ_FORCE_CUT=1 cargo test -p liquers-core --tests --no-fail-fast`. The probe is
temporary and is not part of the change set — the suite in `solution.md` §4 calls
`cut_predecessor` directly on a finalized plan instead, which needs no production switch.

Result at `d1bd02e`, reproducing the issue's table:

| Suite | Passed | Failed | Failing test |
|---|---|---|---|
| `--lib` | 613 | 1 | `interpreter::tests::absolute_outer_resource_keeps_relative_link_on_live_cwd` |
| `injection` | 14 | 1 | `test_chained_commands_with_payload` |
| `recipe_cwd_resolution` | 6 | 2 | `programmatic_and_provider_cwd_select_their_own_inputs`, `recursive_links_and_multiple_parameters_use_active_cwd` |
| the other 16 suites | all | 0 | — |

`liquers-lib` was measured the same way, with the §1 fix in place:
`LQ_FORCE_CUT=1 cargo test -p liquers-lib --lib --tests --no-fail-fast` exits 0 — the
cross-crate default loop is fully green under the cut.

## Cause 1 — the boundary query is frozen one CWD step too early

**This is the defect.** Both `recipe_cwd_resolution` failures reduce to it.

`Plan::freeze_cwd_with` resolves the recorded predecessor before it walks the steps:

```rust
// The predecessor is the leading steps, so it resolves from the entry state of this walk.
if let Some(predecessor) = &mut self.predecessor {
    let mut scoped = cursor.clone();
    *predecessor = scoped.resolve_query_scoped(predecessor);
}
```

The comment holds for a plan built straight from a query. It is false for a plan built from
a recipe carrying `cwd:`, because `Recipe::to_plan` **prepends** a step the builder never
emitted for the query:

```rust
if let Some(cwd) = self.get_cwd()? {
    plan.steps.insert(0, Step::SetCwd(cwd.clone()));
    if plan.predecessor.is_some() {
        plan.predecessor_steps += 1;      // the count is compensated
    }
    ...
}
```

The *count* is compensated; the *cursor* is not. So the predecessor's steps execute under
`recipe.cwd`, while the predecessor **query** — the only thing a cut boundary carries — is
resolved under the entry CWD. Every relative operand inside the boundary silently loses its
folder prefix.

Nothing shows while the plan stays expanded: at runtime `Step::SetCwd` executes first and the
inlined steps resolve correctly against the live cursor. Cutting is what promotes the frozen
query from provenance to the sole carrier of meaning, and the stale resolution becomes the
answer.

This is the same prefix that `plan-cwd-freeze` already tripped over once — the reference
pitfall "a step-range recorded before a prefix is inserted". The count was fixed there; the
cursor was missed, because no test cut a plan built from a recipe with a `cwd:`.

### Worked example — the divergence in one recipe

`liquers-core/tests/recipe_cwd_resolution.rs::programmatic_and_provider_cwd_select_their_own_inputs`.

Store:

```
programmatic/input.txt   = "programmatic"
provider/input.txt       = "provider"
```

Recipe:

```yaml
query: "-R-stored/./input.txt/-/identity/result.txt"
cwd:   "programmatic"
```

`-R-stored/./input.txt` reads the *stored* bytes of a CWD-relative key; `identity` passes the
state through; `result.txt` is the filename. The predecessor is `-R-stored/./input.txt` and
the last action `identity` stays in the parent.

**Expanded (today's behaviour) — correct:**

```
step 1/4  SetCwd(programmatic)                    <- cursor now "programmatic"
step 2/4  GetResource(./input.txt)                <- resolved live -> programmatic/input.txt
step 3/4  Action{identity}
step 4/4  Filename(result.txt)
=> "programmatic"
```

**Cut — wrong:**

```
step 1/4  SetCwd(programmatic)                    <- kept, but now only provenance
step 2/4  Evaluate(-R-stored/input.txt)           <- frozen against the ENTRY cwd, not "programmatic"
step 3/4  Action{identity}
step 4/4  Filename(result.txt)
```

The boundary is evaluated as its own asset, with no CWD of its own (the trace shows
`Recipe { query: "-R-stored/input.txt", ..., cwd: None }`), so it looks for `input.txt` at
logical root:

```
Error during evaluation of asset 1001: Key not found: 'input.txt'
Error during evaluation of asset 1000: Key not found: 'input.txt'
```

Expanded returns `"programmatic"`; cut raises `KeyNotFound: 'input.txt'`. The correct
boundary query is `-R-stored/programmatic/input.txt`.

The second failure is the same mechanism producing a wrong *value* rather than an error,
which is the more dangerous shape:

```yaml
query: "pass-~X~-R-cwd/./child/-/cwd~E/append_cwd/result.txt"
cwd:   "a/c"
```

```
expanded: "a/c/child|a/c"
cut:      "child|a/c"        <- the link's ./child froze against "" instead of "a/c"
```

No error, no warning; a folder-relative operand quietly resolved one level up.

## Cause 2 — a payload does not survive becoming a cache entry

`injection::test_chained_commands_with_payload` evaluates
`/-/first_cmd/second_cmd/third_cmd` with a payload. `first_cmd` and `third_cmd` each take an
`injected` parameter; neither declares `payload: required`.

Cut, the predecessor `/-/first_cmd/second_cmd` becomes a `Step::Evaluate`, which reaches
`Context::schedule_dependency_asset`. That function asks the boundary query whether it
requires a payload — it re-plans it, finds no declaration, and schedules it as an ordinary
dependency asset. No payload is forwarded:

```
Command 'first_cmd' failed: No payload for UserId at position 4
```

(The cause is chained through to the parent, so the `plan-cwd-freeze` fix for pitfall 4 is
working; the diagnosis is not hidden.)

This is **not a bug in cutting**, and forwarding the payload anyway would be worse than the
error. A cut boundary is a cache entry: non-keyed queries land in `query_assets`, keyed by
the query AST. A payload is deliberately *not* part of a dependency key —
`schedule_payload_dependency_asset` states the rule and its consequence: "No graph edge is
registered, and the parent is not recorded as a dependent asset of this query — nothing may
hold a reference *to* a payload-evaluated asset." Silently forwarding a payload into an
ordinarily-cached boundary would let the first caller's payload determine a value that every
later caller reads.

**An injected parameter does not imply a payload requirement**: `injected` means
`InjectedFromContext`, and a value may be injected from the environment alone — `()`
implements the trait and reads no payload at all. So the plan must not infer the requirement
from injection, in either direction.

It does not need to. The payload need is declared on command metadata
(`CommandMetadata::payload_required`), read by `PlanBuilder::action_payload_requirement`, and
ORed up into `Plan::payload_required` — an exact signal, available per command. `first_cmd`
and `third_cmd` here read the payload and declare nothing, so they are **mis-declared**; this
divergence is `plan-cwd-freeze`'s documented "declare it, or lose it" rule (E8), and the fix
is to the test, not to the code.

The code question it raises is a different one, and it is a correctness question: *where may a
boundary be cut in a chain that does read a payload somewhere?* `Plan::payload_required` is a
whole-query flag and answers it wrongly in both directions. `solution.md` §2 answers it per
candidate boundary, by building each candidate's own plan and cutting in front of the payload
need.

## Cause 3 — a test that asserts the expanded shape

`absolute_outer_resource_keeps_relative_link_on_live_cwd` asserts, twice, that
`plan.steps[1]` is `Step::GetAsset("data")` after `finalize_plan`. Under the cut it is
`Step::Evaluate`, and the test fails at `interpreter.rs:1221` — before evaluating anything.

Measured: with those two shape assertions relaxed and everything else left alone, the test
passes under the cut. It produces `"root-data|linked"` and leaves the context CWD at `a/c`,
exactly as expanded. So the shape assertions are the only divergence, and shape E5 — an
absolute query resolving against logical root across a boundary, under a recipe CWD — is
already equivalent.

## Why the existing harness could not have found any of this

`evaluate_both_ways` (in `interpreter.rs`'s `#[cfg(test)] mod`) covers three shapes and
compares one property. Two structural blind spots matter more than the shape count:

1. **It never sets a recipe CWD.** Every call site builds `Recipe::new(query, "", "")` — whose
   `cwd` is `None` — and passes `cwd: None`. Cause 1 lives entirely in the prologue that a
   recipe `cwd:` creates, so the harness cannot reach it no matter how many query shapes are
   added.
2. **It compares only the value.** Phase 3 specified four comparisons: value, `is_volatile`,
   `payload_required`, and the surfaced error. A divergence in volatility — pitfall 3, the one
   that made a volatile command run once instead of twice — is invisible to a value check that
   happens to agree.

Both are addressed in `solution.md` §4. The lesson generalises: the four divergences that
remain were found by running the *existing* suite under the cut, not by the harness written
for the purpose, because the existing suite varies the one axis the harness holds fixed.

## Latent hazard found in passing

`Plan::split` copies `is_volatile`, `payload_required`, `expires`, `error` and `dependencies`
into both halves, and drops `predecessor`, `predecessor_steps` (and would drop
`prologue_steps`). Both halves therefore claim to have no predecessor. Only tests call
`split` today, so nothing is broken; filed as `PLAN-SPLIT-DROPS-PREDECESSOR-FIELDS`.
