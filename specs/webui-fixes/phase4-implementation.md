# Phase 4: Implementation Plan - webui-fixes

*Scope: W3 (rendering follows the model) + W4 (close a stale record). The earlier version of this
file covered the old, wider scope and is in git history (commit `89875c7`).*

## Overview

**Feature:** invalidation as a property of the model — `AppState` records what changed, the renderer
applies it.

**Architecture:** `UIChange` records (`Inserted` / `Removed` / `Replaced`) accumulated in an
`Invalidation` (`None` / `Changes(Vec)` / `All`), recorded inside `AppState`'s own mutating methods,
consumed by exactly one renderer per application; the browser applies them as DOM operations with
focus and caret preserved.

**Estimated complexity:** Medium. Roughly 400 lines of production code, most of it small and local;
the wasm DOM code is the only part not covered by native tests.

**Estimated time:** 5–8 hours including the browser work.

**Prerequisites:**
- Phases 1–3 approved.
- For steps 5, 7, 9, 10: `rustup target add wasm32-unknown-unknown`, `cargo install --locked trunk`,
  and Playwright's Chromium. All three are present and verified in the current session (`trunk
  0.21.14`; `trunk build` and `npx playwright test` both succeed against `HEAD`).

**Staging.** Steps 1–7 are **stage 1**: change recording and consumption, which closes W3 with no
DOM surgery. Steps 8–10 are **stage 2**: structural DOM insert/remove. Stopping after step 7 leaves
a correct, coarser renderer — that is the intended fallback if stage 2 runs into trouble.

## Implementation Steps

### Step 1: `UIChange`, `Invalidation`, and the `Ord` derive

**Files:** `liquers-lib/src/ui/handle.rs`, `liquers-lib/src/ui/app_state.rs`,
`liquers-lib/src/ui/mod.rs`

**Action:**
- Add `PartialOrd, Ord` to `UIHandle`'s derives (additive; no call-site changes).
- Add `UIChange` and `Invalidation` exactly as specified in Phase 2, with `MAX_CHANGES = 64`.
- Implement `record`, `set_all`, `is_empty`, `take`; `Default` is `Invalidation::None`.
- Re-export both from `ui/mod.rs`.
- Unit tests U11–U14 (take clears, `All` absorbs, overflow escalates, `Default` is `None`).

**Code changes:**
```rust
// NEW in app_state.rs
pub enum UIChange { Inserted { parent: Option<UIHandle>, handle: UIHandle, index: usize },
                    Removed  { parent: Option<UIHandle>, handle: UIHandle },
                    Replaced { handle: UIHandle } }

pub enum Invalidation { #[default] None, Changes(Vec<UIChange>), All }

const MAX_CHANGES: usize = 64;   // past this, escalate to All
```

**Validation:**
```bash
cargo test -p liquers-lib --lib ui::app_state
```

**Rollback:** `git checkout liquers-lib/src/ui/{handle.rs,app_state.rs,mod.rs}`

**Agent:** sonnet · skills: `rust-best-practices` · knowledge: Phase 2 §Data Structures. *Rationale:*
the state machine's absorbing behaviour is the part that must be exactly right.

---

### Step 2: `AppState` trait methods and `DirectAppState` recording

**File:** `liquers-lib/src/ui/app_state.rs`

**Action:**
- Add `record_change`, `invalidate_all`, `take_invalidation` to the trait with the **conservative
  defaults** from Phase 2 (`take_invalidation` → `Invalidation::All`).
- Add the `invalidation: Invalidation` field to `DirectAppState`; `new()` starts at `All`.
- Record at every site in Phase 2's table: `add_node`, `insert_node`, `set_element`, `set_source`,
  `remove`, `set_active_handle`, `get_element_mut` (eager). `take_element`/`put_element` record
  **nothing**.
- Doc comments stating the mutation contract: `put_element` = "returning an element borrowed for
  rendering, unchanged"; `set_element` = "installing a new or changed element"; content changes
  outside `update` are reported by the mutator via `record_change`.
- Serde: leave the field out of `DirectAppStateSnapshot`; `Deserialize` sets `All`.
- Unit tests U1–U10, U15–U17 (including the non-tracking test implementor for U17).

**Validation:**
```bash
cargo test -p liquers-lib --lib ui::
cargo test -p liquers-lib --test ui_phase1b_integration   # serialization round trip still passes
```

**Rollback:** `git checkout liquers-lib/src/ui/app_state.rs`

**Agent:** sonnet · skills: `rust-best-practices`, `liquers-unittest` · knowledge: Phase 2
§Trait Implementations + §The mutation contract, existing `app_state.rs` tests. *Rationale:* missing
a recording site reintroduces W3 silently; the U17 implementor needs care.

---

### Step 3: Runner records `Replaced` from the update response

**File:** `liquers-lib/src/ui/runner.rs`

**Action:**
- Add `enum DeliveryOutcome { Missing, Delivered(UpdateResponse) }`; change `deliver_snapshot` to
  return it (two call sites, matched exhaustively — no `_` arm).
- On `Delivered(UpdateResponse::NeedsRepaint)`, record `Replaced { handle }`; on
  `Delivered(UpdateResponse::Unchanged)`, record nothing; on `Missing`, stop monitoring as today.
- Delete the unconditional `println!("AppRunner received message: {:?}", msg)` in
  `process_messages` (noise per message, useless on wasm).

**Validation:**
```bash
cargo test -p liquers-lib --test ui_runner --test query_console_integration
```

**Rollback:** `git checkout liquers-lib/src/ui/runner.rs`

**Agent:** sonnet · skills: `rust-best-practices` · knowledge: `runner.rs`, Phase 2 §Function
Signatures.

---

### Step 4: Integration tests for recording

**File:** `liquers-lib/tests/ui_invalidation.rs` (new)

**Action:** I1–I6 from Phase 3, harness mirroring `tests/ui_runner.rs`. Includes the small
purpose-built element returning `Unchanged` needed by I5 (flagged as Phase 3's open risk).

**Validation:**
```bash
cargo test -p liquers-lib --test ui_invalidation
```

**Rollback:** `rm liquers-lib/tests/ui_invalidation.rs`

**Agent:** sonnet · skills: `liquers-unittest` · knowledge: Phase 3 §Integration tests,
`tests/ui_runner.rs`. *Rationale:* async runner flows are flake-prone; the harness must drain the
initial `All` before asserting.

---

### Step 5: Browser loop consumes the invalidation (stage 1)

**File:** `liquers-lib/src/ui/web/app.rs` (browser module)

**Action:**
- Replace the `needs_repaint()`-driven render with `take_invalidation()`, matched exhaustively:
  `None` → nothing; `All` → whole-tree render; `Changes(list)` → apply each.
- Stage-1 mapping: `Replaced { handle }` → replace `#ui-element-{handle}`'s markup;
  `Inserted`/`Removed` → re-render the parent (whole tree when `parent` is `None`).
- Add `capture_focus`/`restore_focus` around any replacement.
- Remove the now-redundant `first` flag (`DirectAppState::new()` starts at `All`).
- Keep `needs_repaint()` in the loop only to decide whether to keep polling.

**Code changes:**
```rust
// MODIFY the loop
loop {
    let _ = runner.run(&loop_state).await;
    let inv = match loop_state.try_lock() {
        Ok(mut s) => s.take_invalidation(),
        Err(_) => Invalidation::None,      // lock busy: records accumulate, applied next tick
    };
    apply_invalidation(&loop_root, &inv, &loop_state);
    gloo_timers::future::TimeoutFuture::new(16).await;
}
```

**Validation:**
```bash
cargo check -p liquers-lib --no-default-features --features webui
cargo check -p liquers-lib --no-default-features --features webui --target wasm32-unknown-unknown
cd liquers-lib/examples-web/ui_spec_demo && trunk build
```

**Rollback:** `git checkout liquers-lib/src/ui/web/app.rs`

**Agent:** sonnet · skills: `rust-best-practices` · knowledge: `web/app.rs`, Phase 2 §Applying an
invalidation. *Rationale:* wasm-only code with no native test coverage; every DOM lookup must be
`Option`-matched, never `unwrap`.

---

### Step 6: egui examples consume the invalidation

**Files:** `liquers-lib/examples/{ui_spec_demo,ui_spec_interactive,ui_query_console_app,ui_button_app,ui_payload_app}.rs`

**Action:** request a repaint when `take_invalidation()` is not `None`, in addition to
`needs_repaint()`. Take first so the value always clears:

```rust
let changed = !matches!(app_state.take_invalidation(), Invalidation::None);
if changed || self.app_runner.needs_repaint() { ctx.request_repaint(); }
```

**Validation:**
```bash
cargo check -p liquers-lib --examples
cargo run -p liquers-lib --example ui_query_console_app   # manual smoke test
```

**Rollback:** `git checkout liquers-lib/examples`

**Agent:** haiku · skills: `rust-best-practices` · knowledge: the five example files. *Rationale:*
repetitive edit with an exact pattern; the only subtlety is taking unconditionally.

---

### Step 7: Demo gains an inline action + first e2e case (closes stage 1)

**Files:** `liquers-lib/examples-web/ui_spec_demo/src/lib.rs`,
`liquers-lib/examples-web/ui_spec_demo/tests/webui.spec.ts`

**Action:**
- Add a *Remove Last Panel* menu entry to `DASHBOARD_YAML` with action `ns-lui/remove-last`
  (verified in Phase 3 to evaluate and resolve inline).
- Add the Playwright case: add two panels, click *Remove Last Panel*, expect the count to drop with
  no further interaction, assert zero `pageerror`.
- Keep the existing dashboard test untouched as the regression guard.

**Validation:**
```bash
cd liquers-lib/examples-web/ui_spec_demo && trunk build && npx playwright test
```

**Rollback:** `git checkout liquers-lib/examples-web/ui_spec_demo`

**Agent:** sonnet · knowledge: current `lib.rs`, `webui.spec.ts`, `playwright.config.ts`.
*Rationale:* locators and wasm-boot timeouts need judgement.

> **Stage 1 checkpoint.** W3 is closed here: every model change reaches the DOM. Steps 8–10 are an
> improvement on *how*, not *whether*.

---

### Step 8: `data-lq-children` marker (stage 2)

**Files:** `liquers-lib/src/ui/widgets/ui_spec_element.rs`, `liquers-lib/tests/webui_ssr.rs`

**Action:** emit `data-lq-children="{handle}"` on the layout wrapper in `render_web`; extend the SSR
test to assert the marker is present and carries the owning handle.

**Validation:**
```bash
cargo test -p liquers-lib --no-default-features --features webui,image-support --test webui_ssr
cargo test -p liquers-lib --no-default-features --features webui,image-support --test ui_menu_layout_integration
```

**Rollback:** `git checkout liquers-lib/src/ui/widgets/ui_spec_element.rs liquers-lib/tests/webui_ssr.rs`

**Agent:** haiku · knowledge: `ui_spec_element.rs::render_web`, Phase 2 §The container opt-in.

---

### Step 9: Structural DOM insert and remove (stage 2)

**File:** `liquers-lib/src/ui/web/app.rs`

**Action:**
- Add `children_container(root, parent)` — `root` for `parent: None`, else
  `[data-lq-children="{p}"]`.
- `Inserted` → render the child, insert before the container's element child at `index`, appending
  if the container holds fewer; no container → fall back to `Replaced { parent }`; parent node
  missing → whole-tree render.
- `Removed` → remove `#ui-element-{handle}` if present, else skip; no container on the parent →
  fall back to `Replaced { parent }`.
- `Replaced` → as in step 5; handle gone from the model → skip; node absent → whole-tree render.

**Validation:**
```bash
cargo check -p liquers-lib --no-default-features --features webui --target wasm32-unknown-unknown
cd liquers-lib/examples-web/ui_spec_demo && trunk build && npx playwright test
```

**Rollback:** `git checkout liquers-lib/src/ui/web/app.rs` (reverts to step 5's coarser mapping,
which is still correct)

**Agent:** sonnet · skills: `rust-best-practices` · knowledge: Phase 2 §Applying an invalidation.
*Rationale:* the decision table (container present/absent × node present/absent × stale handle) is
the subtlest code in the feature and is only covered end-to-end.

---

### Step 10: Node-identity e2e case (stage 2)

**File:** `liquers-lib/examples-web/ui_spec_demo/tests/webui.spec.ts`

**Action:** tag an existing panel node from the page (`el.dataset.probe = "1"`), click
*Add Dashboard*, assert the tag survived and the panel count increased.

**Validation:**
```bash
cd liquers-lib/examples-web/ui_spec_demo && npx playwright test
```

**Rollback:** delete the added `test(...)` block.

**Agent:** sonnet · knowledge: `webui.spec.ts`.

---

### Step 11: W4 and documentation

**Files:** `specs/ISSUES.md`, `specs/webui/DESIGN.md`, `specs/webui-fixes/DESIGN.md`,
`liquers-lib/examples-web/README.md`

**Action:**
- Mark **W3 Resolved (webui-fixes)** with the mechanism and the covering tests.
- Rewrite the "webui: async evaluation engine does not run on wasm" entry as **Resolved by
  `async-wasm-refactor`** — evidence: `ImmediateAssetManager`, wasm tokio reduced to `["sync"]`, and
  a Playwright run that passes against current `HEAD` (re-verified this session). Keep the two
  genuinely open follow-ups (full tokio removal / executor-agnostic core; Tier-2 browser-native
  I/O) as their own entry.
- Note that W1, W2 and W5 now live in `specs/ui-events/`.
- `specs/webui-fixes/DESIGN.md` → Implementation Complete.
- README: mention the *Remove Last Panel* entry in the demo description.

**Validation:**
```bash
grep -n "WEBUI-REPAINT\|async evaluation engine" specs/ISSUES.md
```

**Rollback:** `git checkout specs liquers-lib/examples-web/README.md`

**Agent:** haiku · knowledge: `specs/ISSUES.md`, `specs/async-wasm-refactor/DESIGN.md`.

## Testing Plan

### Unit Tests

After steps 1–3:
```bash
cargo test -p liquers-lib --lib ui::app_state
cargo test -p liquers-lib --lib ui::
```

### Integration Tests

After step 4, and again after step 6:
```bash
cargo test -p liquers-lib --test ui_invalidation --test ui_runner \
                          --test query_console_integration --test ui_phase1b_integration \
                          --test ui_menu_layout_integration --test ui_spec_integration \
                          --test ui_shortcuts_integration
cargo test -p liquers-lib --no-default-features --features webui,image-support --test webui_ssr
cargo test --workspace
```

Feature matrix (after steps 5 and 9):
```bash
cargo check -p liquers-lib                                          # default
cargo check -p liquers-lib --no-default-features --features webui
cargo check -p liquers-lib --no-default-features --features webui,image-support
cargo check -p liquers-lib --no-default-features --features webui --target wasm32-unknown-unknown
```

### Manual Validation

```bash
cargo run -p liquers-lib --example ui_query_console_app     # egui unchanged
cd liquers-lib/examples-web/ui_spec_demo && trunk serve     # http://127.0.0.1:8080
```

In the browser: add three panels, click *Remove Last Panel* — it disappears immediately, with no
other interaction. After step 9, add another panel and watch devtools: existing panel nodes are not
replaced.

> **Disk note for this environment.** A full `cargo test --workspace` with examples needs several
> GB; this session hit the container's allowance once and needed `cargo clean`. Run the targeted
> commands above rather than repeated full-workspace builds, and check `df -h /` if a build fails
> with "No space left on device".

## Agent Assignment Summary

| Step | Model | Skills | Rationale |
|------|-------|--------|-----------|
| 1 | sonnet | rust-best-practices | Absorbing state machine must be exact |
| 2 | sonnet | rust-best-practices, liquers-unittest | A missed recording site reintroduces W3 silently |
| 3 | sonnet | rust-best-practices | Two call sites, exhaustive matching |
| 4 | sonnet | liquers-unittest | Async runner flows are flake-prone |
| 5 | sonnet | rust-best-practices | wasm-only, no native coverage, no `unwrap` |
| 6 | haiku | rust-best-practices | Repetitive edit, exact pattern |
| 7 | sonnet | — | Locators and wasm-boot timeouts |
| 8 | haiku | — | One attribute plus an assertion |
| 9 | sonnet | rust-best-practices | Subtlest decision table in the feature |
| 10 | sonnet | — | Browser-side assertion |
| 11 | haiku | — | Documentation with evidence in hand |

Steps 1→2→3 are sequential; 4 depends on 3; 5 depends on 2; 6 depends on 2; 7 depends on 5; 8→9→10
are sequential and depend on 7. *In this session the steps will be executed inline rather than by
sub-agents; the assignments record the intended shape for anyone running it differently.*

## Rollback Plan

Each step is an isolated commit with the `git checkout` above. Two properties make this safe:

- **Stage 2 reverts to stage 1, not to broken.** Reverting step 9 restores the coarser mapping,
  which still closes W3.
- **The whole feature reverts cleanly.** Nothing touches `liquers-core`, `register_command!`,
  `liquers-py` or `liquers-axum`; reverting the `liquers-lib/src/ui` hunks plus the new test file
  restores today's behaviour exactly. The one API addition visible downstream is three
  defaulted `AppState` methods, whose defaults reproduce current behaviour.

Highest-risk step: **2** (a missed recording site is a silent stale-DOM bug). Mitigation: the unit
tests are written against Phase 2's recording table, one test per row.

## Documentation Updates

- `specs/ISSUES.md` — W3 resolved, W4 closed, W1/W2/W5 pointed at `specs/ui-events/` (step 11).
- `specs/webui/DESIGN.md` — note that rendering now follows model changes.
- `specs/webui-fixes/DESIGN.md` — phase tracking → Implementation Complete.
- `liquers-lib/examples-web/README.md` — the demo's new menu entry.
- `CLAUDE.md` / `specs/PROJECT_OVERVIEW.md` — no change; no core concepts change.

## Review Findings (inline)

**rust-best-practices (Phase 4 pass)** — steps are ordered so nothing compiles against a
half-migrated API: the types (1) exist before the trait uses them (2), which exists before the
runner (3) and the renderers (5, 6). The `Ord` derive lands in step 1 with the type that needs it.
No step introduces `unwrap`/`expect`, a `_` match arm on a Liquers enum, a new error type, or a
backward crate dependency. Feature/`target_arch` gating is unchanged: all new backend-neutral code
is uncfg'd, and all DOM code sits inside the existing wasm+webui module.

**Phase 1 conformity** — per-element invalidation (steps 2, 9), global fallback (steps 2, 5),
focus/caret preserved (step 5), no diff/patch (step 9 inserts recorded nodes; nothing compares
trees), egui unaffected but improved (step 6).

**Phase 2 conformity** — every recording site, the conservative defaults, the mutation contract's
doc comments, the container opt-in and the apply-time decision table each map to a numbered step.

**Phase 3 conformity** — U1–U17 land in steps 1–2, I1–I6 in step 4, the three e2e cases in steps 7
and 10, the stated focus-testing gap is unchanged.

**Codebase compatibility** — `DirectAppState` is the only `AppState` implementor; `ui_spec_demo`
builds and its Playwright suite passes against current `HEAD`, so step 7 starts from green; the new
test file follows the `ui_*.rs` naming already in `liquers-lib/tests/`.

**Residual risks** — (a) the wasm DOM code has no native tests, mitigated by keeping the decision
table small and reviewable and by two e2e cases; (b) `MAX_CHANGES = 64` is a guess, and the
escalation path it guards is exercised only by a unit test, not in the browser; (c) step 6 changes
five example files whose behaviour is only verified by a manual smoke test.

## Execution Options

1. **Execute now** — steps 1–11 in order, committing per step.
2. **Stage 1 only** — steps 1–7 plus 11, deferring structural DOM operations.
3. **Create task list** — one task per step for later execution.
4. **Revise plan** — adjust staging or scope.
