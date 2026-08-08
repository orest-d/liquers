# Phase 3: Examples & Use-cases - webui-fixes

*Scope: W3 (rendering follows the model) + W4 (close a stale record). The earlier version of this
file covered the old, wider scope and is in git history (commit `89875c7`).*

## Example Type

**Runnable prototypes.** Every example below is either the existing browser example
(`liquers-lib/examples-web/ui_spec_demo`, verified working against current `HEAD`) or a test that
lands in the repository. Nothing here is illustrative-only: W3 is a behavioural defect, so a
conceptual example could not demonstrate it.

## Overview Table

| # | Type | Name | What it demonstrates / checks | Stage |
|---|------|------|-------------------------------|-------|
| E1 | Example (browser) | *Remove Last Panel* menu entry | An action that resolves fully inline updates the DOM in the same tick — the W3 defect | 1 |
| E2 | Example (browser) | Repeated *Add Dashboard* | Each add inserts one node; existing panels keep their DOM identity | 2 |
| E3 | Example (native) | Restored `AppState` paints | A deserialized state renders in full without any mutation happening first | 1 |
| U1 | Unit | `add_node_records_inserted` | `Inserted { parent, handle, index }` with the resolved index | 1 |
| U2 | Unit | `add_root_records_inserted_with_no_parent` | A root add records `parent: None`, not `All` | 1 |
| U3 | Unit | `insert_node_records_inserted` | The explicit-handle variant records the same way | 1 |
| U4 | Unit | `set_element_records_replaced` | Installing an element marks it out of date | 1 |
| U5 | Unit | `set_source_records_replaced` | A pending node's placeholder is renderable state | 1 |
| U6 | Unit | `remove_records_removed_with_parent` | `Removed { parent, handle }`; one record for the subtree | 1 |
| U7 | Unit | `remove_root_records_parent_none` | Root removal is expressible without `All` | 1 |
| U8 | Unit | `set_active_handle_records_both` | Old and new active elements are both re-rendered | 1 |
| U9 | Unit | `get_element_mut_records_replaced` | The `&mut` escape hatch cannot bypass recording | 1 |
| U10 | Unit | `take_and_put_element_record_nothing` | The render path stays silent (else egui repaints every frame) | 1 |
| U11 | Unit | `take_invalidation_clears` | Second take returns `None` | 1 |
| U12 | Unit | `set_all_absorbs_further_changes` | `Changes → All`, and a later record is ignored | 1 |
| U13 | Unit | `change_log_overflow_escalates_to_all` | `MAX_CHANGES + 1` records collapse to `All` | 1 |
| U14 | Unit | `new_state_starts_all` | A fresh state paints without a "first frame" flag | 1 |
| U15 | Unit | `deserialize_starts_all` | A restored state is fully out of date | 1 |
| U16 | Unit | `serialize_omits_invalidation` | The persisted format is unchanged | 1 |
| U17 | Unit | `default_trait_methods_report_all` | A non-tracking `AppState` degrades to a full re-render, never to stale | 1 |
| I1 | Integration | `sync_mutation_records_change` | `hello/ns-lui/add-child` leaves a recorded change after `run` — the W3 root cause | 1 |
| I2 | Integration | `remove_command_records_removed` | `ns-lui/remove-last` records `Removed` with the right parent | 1 |
| I3 | Integration | `idle_run_records_nothing` | An idle `run` records nothing (no busy re-render) | 1 |
| I4 | Integration | `snapshot_delivery_records_replaced` | A `NeedsRepaint` response becomes a `Replaced` record | 1 |
| I5 | Integration | `unchanged_snapshot_records_nothing` | An `Unchanged` response records nothing | 1 |
| I6 | Integration | `pending_evaluation_records_changes` | Auto-evaluation of a pending node records its element installs | 1 |
| E2E | Playwright | `inline action updates the DOM` | E1 in headless Chromium | 1 |
| E2E | Playwright | `adding a panel preserves existing nodes` | E2: a marker set on an existing panel's node survives the next add | 2 |
| E2E | Playwright | (existing) `dashboard renders and reacts` | Regression: the demo keeps working | 1 |

"Stage" refers to Phase 2's implementation staging: **1** = change recording + consuming the
invalidation (closes W3, no DOM surgery); **2** = structural DOM insert/remove with the
`data-lq-children` opt-in.

## Example 1: an action that resolves inline updates the page (W3, stage 1)

**Scenario.** The demo's menu gains a second entry, *Remove Last Panel*, whose action is the query
`ns-lui/remove-last`. The user adds two panels, then clicks *Remove Last Panel*.

**Today.** The command runs, the node is gone from `AppState`, and the panel stays on screen —
because nothing is left in flight, so `needs_repaint()` is false and the loop skips the render. It
disappears later, when some unrelated activity happens to make that question true.

**After.** `remove` records `Removed { parent: root, handle: panel }`; the loop takes the
invalidation on the next tick and applies it. The panel disappears within one 16 ms tick, with no
other interaction.

**Query validation.** `ns-lui/remove-last` was verified against the current build before writing
this: starting from a two-child root it left one child and no error element, so a query with no
leading input command evaluates correctly and resolves inline.

**Correction from implementation.** *Add Dashboard* was described here as repainting "by accident"
because it leaves a pending node in flight. Measurement against the pre-fix build disproved that:
clicking it produced no DOM change either, since the inline asset manager completes the evaluation
within the same `run()`. Both e2e cases therefore count `[id^="ui-element-"]` nodes instead of
matching text — the pre-existing text assertion was satisfied by the menu label before any click.

**Expected output.** Two panels, then one; the browser console stays clean.

## Example 2: adding a panel leaves its siblings alone (stage 2)

**Scenario.** The user clicks *Add Dashboard* four times.

**Stage 1 behaviour.** Each add records `Inserted`, which the renderer maps to "re-render the
parent" — correct, but every existing panel's DOM node is destroyed and recreated on each add.

**Stage 2 behaviour.** The `UISpecElement` layout wrapper carries `data-lq-children="{handle}"`, so
the renderer inserts exactly one new node into that container and touches nothing else. Existing
panels keep their DOM identity, and a session of N adds costs N renders rather than N².

**How it is checked without a query console.** The e2e test tags an existing panel's node from the
page (`el.dataset.probe = "1"`), clicks *Add Dashboard* again, and asserts the tag survived. A
destroyed-and-recreated node loses it. This is the mechanism that protects focus, caret and scroll,
tested by proxy — see *Testing gap* below.

## Example 3: a restored `AppState` paints (stage 1)

**Scenario.** An application serializes `AppState` to JSON, restarts, deserializes, and attaches a
renderer. No mutation happens afterwards.

**Why it would break naively.** Change records describe *mutations*; a restored state has had none,
so a purely change-based renderer would draw nothing at all.

**After.** `Deserialize` sets `Invalidation::All`, so the first `take_invalidation` returns `All`
and the whole tree renders. The same mechanism covers a freshly-built state (`new()` starts at
`All`), which is why the browser loop's `first`-frame flag disappears.

## Corner Cases

### 1. Memory

- The change log is bounded by `MAX_CHANGES` (64): past that, `Invalidation` escalates to `All`,
  which is both cheaper to apply and constant-size. The realistic trigger is the `AppState` lock
  being held across several browser ticks while a burst of mutations lands.
- `UIChange` is three small `Copy`-ish fields; a bounded `Vec` of them is negligible next to the
  markup it avoids regenerating.
- No new per-element allocation: elements and nodes are untouched.

### 2. Concurrency

- A change is recorded under the same `&mut self` borrow — in practice the same mutex guard — as the
  mutation that caused it, so a renderer cannot observe a change without its record.
- Exactly one renderer per application may call `take_invalidation`; two consumers would each see
  part of the history. Documented on the method, and asserted by U11 only for the single-consumer
  case (the multi-consumer misuse is a documentation matter, not something to encode in a test).
- If the browser loop cannot take the lock, it skips the frame; records accumulate **in order** and
  are applied next tick. Nothing is lost — strictly better than today, where a skipped frame could
  drop the repaint entirely.
- egui apps hold the lock across rendering and take the invalidation inside the same guard.

### 3. Errors

- **Stale record** (insert then remove in one batch): applying re-reads the model, so the insert
  finds no element and skips. No log normalisation needed.
- **Missing container** (`data-lq-children` absent because the widget's markup depends on its child
  set): falls back to `Replaced { parent }` — the documented degradation, and stage 1's behaviour.
- **Missing DOM node** for a `Replaced` handle (never rendered): escalate to the whole-tree render.
- **Index past the container's children**: append rather than fail.
- **Failed DOM operation**: return false, and the caller does the whole-tree render. Nothing returns
  `Err` mid-batch, because a half-updated page is worse than a redundant full render.
- W3's own failure mode — a stale page — is *not* silent after this change: any recorded mutation
  reaches the renderer, and anything unattributable escalates to `All`.

### 4. Serialization

- `Invalidation`/`UIChange` are transient and absent from `DirectAppStateSnapshot`, so the persisted
  JSON is byte-identical to today's (U16 asserts the key is absent).
- Deserialize starts at `All` (U15), which is what makes Example 3 work.
- No schema evolution concern: nothing about the saved format changes, so old files load unchanged.

### 5. Integration

- **SSR** (`render_app_ssr`) renders everything on demand and never consults invalidation — the
  existing `webui_ssr.rs` assertions must keep passing unchanged, with one addition: the
  `data-lq-children` marker appears in stage 2's output.
- **egui** keeps working and gets W3's fix for free: the five example apps repaint when the
  invalidation is not `None` instead of only when async work is pending.
- **Commands** (`lui` and any application-defined ones) are untouched; they inherit recording from
  the `AppState` methods they must already use.
- **Feature matrix**: `Invalidation`/`UIChange` are backend-neutral (no cfg), so default,
  `--no-default-features --features webui`, `webui,image-support` and the wasm target all need to
  build.
- **`liquers-py` / `liquers-axum`**: no reachable change (no command signatures, no core types).

## Test Plan

### Unit tests

**File:** `liquers-lib/src/ui/app_state.rs` (existing `#[cfg(test)] mod tests`)

U1–U16 above. Shape, following the file's existing conventions:

```rust
#[test]
fn add_node_records_inserted() -> Result<(), Box<dyn std::error::Error>> {
    let mut s = DirectAppState::new();
    let root = s.add_node(None, 0, ElementSource::None)?;
    let _ = s.take_invalidation();                  // discard the initial All

    let child = s.add_node(Some(root), 0, ElementSource::None)?;

    match s.take_invalidation() {
        Invalidation::Changes(v) => assert_eq!(
            v,
            vec![UIChange::Inserted { parent: Some(root), handle: child, index: 0 }]
        ),
        Invalidation::None | Invalidation::All => panic!("expected a recorded insert"),
    }
    Ok(())
}
```

Note the two conventions this leans on: every test discards the initial `All` before exercising the
behaviour under test, and every `match` on `Invalidation` enumerates all three variants (no `_`
arm), so a future variant is a compile error here too.

U17 needs a minimal `AppState` implementor that overrides none of the three methods. `AppState` has
many required methods, so this is real boilerplate — worth it, because it is the test that pins the
"conservative default" property that the whole trait-extension approach rests on:

```rust
#[test]
fn default_trait_methods_report_all() {
    let mut s = NonTrackingAppState::default();   // test-only, overrides nothing
    s.invalidate_all();                            // default no-op
    assert!(matches!(s.take_invalidation(), Invalidation::All));
}
```

### Integration tests

**File:** `liquers-lib/tests/ui_invalidation.rs` (new)

Harness identical to `tests/ui_runner.rs` (`DefaultEnvironment<Value, SimpleUIPayload>`,
`register_command!(cr, fn hello(state) -> result)`, `register_lui_commands!`, `DirectAppState`,
`AppRunner`), so the file adds no new setup concepts.

| Test | Flow |
|---|---|
| I1 `sync_mutation_records_change` | submit `hello/ns-lui/add-child`; run; assert the invalidation contains an `Inserted` under the root |
| I2 `remove_command_records_removed` | add two children, drain, submit `ns-lui/remove-last`; run; assert `Removed { parent: root, .. }` |
| I3 `idle_run_records_nothing` | drain, `run` with no messages; assert `Invalidation::None` |
| I4 `snapshot_delivery_records_replaced` | console element at a handle; `RequestAssetUpdates { handle, "hello" }`; run; assert `Replaced { handle }` |
| I5 `unchanged_snapshot_records_nothing` | an element whose `update` returns `Unchanged`; deliver a snapshot; assert nothing recorded by the delivery |
| I6 `pending_evaluation_records_changes` | pending node with `ElementSource::Query("hello")`; run to completion; assert the progress and result installs were recorded |

All queries used (`hello`, `hello/ns-lui/add-child`, `ns-lui/remove-last`) contain no spaces or
newlines, use no `-R/` resource part (so no store is required), and refer only to commands
registered by the test or by `register_lui_commands!`. `ns-lui/remove-last` was executed against
the current build to confirm it evaluates and resolves inline.

### End-to-end (Playwright)

**File:** `liquers-lib/examples-web/ui_spec_demo/tests/webui.spec.ts` (extend)

1. *(stage 1)* **`inline action updates the DOM`** — add two panels, click *Remove Last Panel*,
   expect the panel count to drop with no further interaction, and assert zero `pageerror`.
2. *(stage 2)* **`adding a panel preserves existing nodes`** — tag an existing panel node from the
   page, click *Add Dashboard*, expect the tag to survive and the panel count to increase.
3. *(existing)* **`dashboard renders and reacts to a menu action`** — kept as the regression guard;
   it passes today and must keep passing.

The demo needs one addition for (1): the *Remove Last Panel* menu entry in `DASHBOARD_YAML`.

### Testing gap (stated deliberately)

Focus and caret preservation is **not** directly e2e-tested in this feature, because the demo has
no focusable input — the query console belongs to `specs/ui-events/`. What is tested here is the
mechanism that protects it: node identity across an insert (E2E 2). The caret-restore path around a
whole-tree fallback render gets its own e2e test when the console lands in the browser demo. This is
a deliberate scope boundary, not an oversight.

### Manual validation

```bash
# native
cargo test -p liquers-lib --lib ui::app_state
cargo test -p liquers-lib --test ui_invalidation --test ui_runner --test query_console_integration
cargo test -p liquers-lib --no-default-features --features webui,image-support --test webui_ssr
cargo check -p liquers-lib --no-default-features --features webui --target wasm32-unknown-unknown
cargo test --workspace

# browser
cd liquers-lib/examples-web/ui_spec_demo && trunk serve      # http://127.0.0.1:8080
npx playwright test
```

Manual browser check: add three panels, remove the last — it disappears immediately; add another —
the remaining panels do not flicker (stage 2 makes this observable in devtools as unchanged nodes).

For W4 there is nothing to run: verification is that `specs/archive/2026-08-08-issues.md` records the wasm issue as
resolved by `async-wasm-refactor`, and that the existing Playwright suite still passes — which it
does today.

## Auto-Invoke: liquers-unittest Skill Output

Applying the project's test conventions (in-file `#[cfg(test)] mod tests`, `#[tokio::test]` for
async, `-> Result<(), Box<dyn std::error::Error>>` where `?` is used, no `unwrap()`/`expect()`
outside tests, explicit match arms, `type CommandEnvironment` alias before any `register_command!`):

```rust
// liquers-lib/tests/ui_invalidation.rs — integration skeleton
type CommandEnvironment = DefaultEnvironment<Value, SimpleUIPayload>;

fn hello(_state: &State<Value>) -> Result<Value, Error> {
    Ok(Value::from("Hello from test!"))
}

fn register(env: &mut DefaultEnvironment<Value, SimpleUIPayload>) -> Result<(), Error> {
    let cr = env.get_mut_command_registry();
    register_command!(cr, fn hello(state) -> result)?;
    liquers_lib::register_lui_commands!(cr)?;
    Ok(())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn sync_mutation_records_change() {
    let (app_state, ui_context, mut runner) = harness();   // as in tests/ui_runner.rs
    let root = { /* add a root node */ };
    { let mut s = app_state.lock().await; let _ = s.take_invalidation(); }

    ui_context.submit_query(root, "hello/ns-lui/add-child");
    runner.run(&app_state).await.expect("runner.run");

    let mut s = app_state.lock().await;
    match s.take_invalidation() {
        Invalidation::Changes(v) => assert!(
            v.iter().any(|c| matches!(c, UIChange::Inserted { parent: Some(p), .. } if *p == root)),
            "expected an Inserted under the root, got {v:?}"
        ),
        Invalidation::None => panic!("a completed mutation must be recorded — this is W3"),
        Invalidation::All => {} // acceptable escalation, but not expected here
    }
}
```

Coverage assessment: the recording sites (U1–U10) and the state machine (U11–U13) are covered
exhaustively; the runner's two paths (`NeedsRepaint` / `Unchanged`) are covered by I4/I5; the
serialization contract by U15/U16; the conservative default — the property the trait-extension
approach depends on — by U17. The DOM application logic is covered only end-to-end, because it is
wasm-only code; its decision table (container present/absent, node present/absent, stale handle)
is therefore worth keeping small and reviewable.

## Review findings (inline)

**Phase 1 conformity** — the examples exercise exactly the Phase 1 decisions: per-element
invalidation (E2), global fallback (E3), no diff/patch (nothing compares trees), egui unaffected
(corner case 5). The focus/caret decision is honoured by design and, as stated above, only
proxy-tested here.

**Phase 2 conformity** — every recording site in Phase 2's table has a unit test; the
`take_element`/`put_element` silence rule is pinned by U10; the `get_element_mut` hole by U9; the
`MAX_CHANGES` escalation by U13; the container opt-in by the stage-2 e2e test.

**Codebase + query validation** — all queries are space-free, resource-free, and use registered
commands; `ns-lui/remove-last` was executed against the current build rather than assumed; the
integration harness mirrors `tests/ui_runner.rs`; the new test file name `ui_invalidation.rs`
follows the existing `ui_*.rs` convention.

**Open risk** — I5 (`unchanged_snapshot_records_nothing`) needs an element whose `update` returns
`Unchanged` for an `AssetUpdate`. `QueryConsoleElement` always returns `NeedsRepaint` for that
message, so the test needs a small purpose-built element in the test file, as `tests/ui_runner.rs`
already does with `TestWidget`.
