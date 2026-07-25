# Phase 4: Implementation Plan - webui-fixes

## Overview

**Feature:** webui-fixes (W1 Enter-key submit, W2 submitted-query state, W3 repaint after a
synchronous mutation, W4 close the already-resolved wasm issue).

**Architecture:** action attribute on the query input + a click guard for text entry;
`AssetSnapshot.query` reconciled by `QueryConsoleElement::adopt_query`; an `AppRunner` dirty flag
consumed by the render loop.

**Estimated complexity:** Low–Medium (no `liquers-core` changes; ~200 lines of production code,
the rest tests).

**Estimated time:** 3–5 hours including the browser e2e.

**Prerequisites:**
- Phases 1–3 approved.
- `rustup target add wasm32-unknown-unknown`, `cargo install trunk`,
  `npx playwright install chromium` for Steps 8–9 (see `liquers-lib/examples-web/README.md`).

## Implementation Steps

### Step 1: W1 — action on the input + click guard

**Files:** `liquers-lib/src/ui/widgets/query_console_element.rs` (`render_web`),
`liquers-lib/src/ui/web/app.rs` (`browser::dispatch_dom_event`)

**Action:**
- Emit the same `action_attr(&UiAction::Apply { … })` on the `<input>` as on the "Go" `<span>`
  (compute once, interpolate twice).
- Skip *click* events that originate on a text-entry control, so clicking into the field places the
  caret instead of submitting. Enter-key events are unaffected.

**Code changes:**
```rust
// NEW (ui/web/app.rs, browser module)
/// A click landing in a text field is the user placing the caret, not triggering the field's
/// action. Enter-key events on the same field still dispatch.
fn is_text_entry(el: &web_sys::Element) -> bool {
    match el.tag_name().as_str() {
        "TEXTAREA" => true,
        "INPUT" => !matches!(
            el.get_attribute("type").unwrap_or_else(|| "text".to_string()).as_str(),
            "button" | "submit" | "reset" | "checkbox" | "radio"
        ),
        _ => false,
    }
}

// MODIFY dispatch_dom_event, right after `target` is resolved:
if ev.type_() == "click" && is_text_entry(&target) {
    return;
}
```

**Validation:**
```bash
cargo check -p liquers-lib --no-default-features --features webui
cargo check -p liquers-lib --no-default-features --features webui --target wasm32-unknown-unknown
```

**Rollback:**
```bash
git checkout liquers-lib/src/ui/web/app.rs liquers-lib/src/ui/widgets/query_console_element.rs
```

**Agent Specification:**
- **Model:** sonnet
- **Skills:** rust-best-practices
- **Knowledge:** `ui/web/app.rs`, `ui/action.rs`, `specs/webui/phase2-architecture.md`, Phase 2
- **Rationale:** small but subtle DOM-semantics change; needs judgement on the guard's scope.

---

### Step 2: W2 — `AssetSnapshot.query` plumbing

**Files:** `liquers-lib/src/ui/message.rs`, `liquers-lib/src/ui/runner.rs`

**Action:**
- Add `pub query: String` as the first field of `AssetSnapshot`, documented as "query the monitored
  asset was created from; empty when unknown".
- `MonitoredAsset<E>` gains `query: String`, stored by `handle_request_asset_updates`.
- `build_snapshot(asset_ref, query: &str)` stamps it; update both call sites (initial snapshot and
  `poll_monitored_assets`).
- The error snapshot in `handle_request_asset_updates` carries the failing query.
- Fix all 10 struct literals: 2 in `runner.rs`, 6 in `query_console_element.rs` tests, 2 in
  `markdown_element.rs` tests.

**Validation:**
```bash
cargo test -p liquers-lib --lib ui::
```

**Rollback:**
```bash
git checkout liquers-lib/src/ui/message.rs liquers-lib/src/ui/runner.rs
```

**Agent Specification:**
- **Model:** sonnet
- **Skills:** rust-best-practices
- **Knowledge:** `ui/runner.rs`, `ui/message.rs`, Phase 2 §Data Structures
- **Rationale:** mechanical but touches every snapshot construction site.

---

### Step 3: W2 — `QueryConsoleElement::adopt_query`

**File:** `liquers-lib/src/ui/widgets/query_console_element.rs`

**Action:**
- Add `adopt_query`, called at the top of the `UpdateMessage::AssetUpdate(snapshot)` arm — before
  the value/status assignment, so the expiry (`request_asset_updates`) and volatile
  (`schedule_volatile_refresh`) branches at the end of the arm already use the adopted query.

**Code changes:**
```rust
// NEW
/// Adopt a query submitted elsewhere (web `lui/submit`, a preset, an init query): set
/// `query_text`, append to history, drop query-scoped caches. Sends no message — the caller is
/// already monitoring the asset. Returns true when the query changed.
fn adopt_query(&mut self, query: &str) -> bool {
    if query.is_empty() || query == self.query_text {
        return false;
    }
    self.query_text = query.to_string();
    if self.history.last().map(String::as_str) != Some(query) {
        self.history.push(query.to_string());
    }
    self.history_index = self.history.len();
    self.next_presets.clear();
    self.last_volatile_refresh_at = None;
    true
}
```

**Validation:**
```bash
cargo test -p liquers-lib --lib ui::widgets::query_console_element
```

**Rollback:**
```bash
git checkout liquers-lib/src/ui/widgets/query_console_element.rs
```

**Agent Specification:**
- **Model:** sonnet
- **Skills:** rust-best-practices, liquers-unittest
- **Knowledge:** `query_console_element.rs`, Phase 2 §Function Signatures
- **Rationale:** ordering inside `update` matters; needs the element's full state model.

---

### Step 4: W3 — `AppRunner` dirty flag

**File:** `liquers-lib/src/ui/runner.rs`

**Action:**
- Add `dirty: bool` to `AppRunner` (`false` in `new`).
- Set `self.dirty = true` when a message is drained in `process_messages`; when an element is set in
  `evaluate_pending_nodes` / `poll_evaluating_nodes`; when `deliver_snapshot` returns
  `Delivered(UpdateResponse::NeedsRepaint)`; when a monitored entry is dropped.
- Introduce `enum DeliveryOutcome { Missing, Delivered(UpdateResponse) }` and change
  `deliver_snapshot` to return it (two call sites, matched exhaustively — no `_` arm).
- Add `take_repaint_request`, documenting that there must be exactly one consumer per render loop.
- Remove the unconditional `println!("AppRunner received message: {:?}", msg)` (noise per message,
  useless on wasm) while editing the same function.

**Code changes:**
```rust
// NEW
/// True when message processing, evaluation, or snapshot delivery changed anything since the last
/// call; clears the flag. Complements `needs_repaint()`, which reports *pending* async work —
/// this reports *completed* work. Exactly one consumer per render loop.
pub fn take_repaint_request(&mut self) -> bool {
    std::mem::replace(&mut self.dirty, false)
}
```

**Validation:**
```bash
cargo test -p liquers-lib --test ui_runner
```

**Rollback:**
```bash
git checkout liquers-lib/src/ui/runner.rs
```

**Agent Specification:**
- **Model:** sonnet
- **Skills:** rust-best-practices
- **Knowledge:** `ui/runner.rs`, Phase 2 §Data Structures
- **Rationale:** must not miss a mutation site, or stale frames return.

---

### Step 5: W3 — consume the flag in the render loops

**Files:** `liquers-lib/src/ui/web/app.rs`;
`liquers-lib/examples/{ui_spec_demo,ui_spec_interactive,ui_query_console_app,ui_button_app,ui_payload_app}.rs`

**Action:**
- Browser loop: call `take_repaint_request()` unconditionally, then
  `if first || changed || runner.needs_repaint() { render_roots_into(…) }`.
- egui apps: `if self.app_runner.take_repaint_request() || self.app_runner.needs_repaint() { ctx.request_repaint(); }`
  (take first, so the flag always clears).

**Validation:**
```bash
cargo check -p liquers-lib --examples
```

**Rollback:**
```bash
git checkout liquers-lib/src/ui/web/app.rs liquers-lib/examples
```

**Agent Specification:**
- **Model:** haiku
- **Skills:** rust-best-practices
- **Knowledge:** the browser loop and the five example files
- **Rationale:** repetitive, low-risk edit with an exact pattern to apply.

---

### Step 6: Unit tests (U1–U6)

**File:** `liquers-lib/src/ui/widgets/query_console_element.rs` (`mod tests`)

**Action:** add the six tests from Phase 3 §Unit tests;
`render_web_puts_action_on_input` is `#[cfg(feature = "webui")]`.

**Validation:**
```bash
cargo test -p liquers-lib --lib ui::widgets::query_console_element
cargo test -p liquers-lib --no-default-features --features webui --lib ui::widgets::query_console_element
```

**Rollback:**
```bash
git checkout liquers-lib/src/ui/widgets/query_console_element.rs
```

**Agent Specification:**
- **Model:** sonnet
- **Skills:** liquers-unittest
- **Knowledge:** Phase 3 §Unit tests, the existing test module and its `create_test_context` helper
- **Rationale:** tests must assert behaviour, not implementation details.

---

### Step 7: Integration tests (I1–I4)

**Files:** `liquers-lib/tests/query_console_integration.rs`, `liquers-lib/tests/ui_runner.rs`

**Action:** add I1/I2 (snapshot query reaches the console; `ApplyToInput` → `lui/submit` updates
console state) and I3/I4 (repaint requested after a message-only run; not requested when idle).

**Validation:**
```bash
cargo test -p liquers-lib --test query_console_integration --test ui_runner
```

**Rollback:**
```bash
git checkout liquers-lib/tests
```

**Agent Specification:**
- **Model:** sonnet
- **Skills:** liquers-unittest
- **Knowledge:** existing harnesses in both test files (`setup_env`, `DirectAppState`, `AppRunner`)
- **Rationale:** needs the async runner flow to avoid flaky polling loops.

---

### Step 8: Browser demo — add a query console

**Files:** `liquers-lib/examples-web/ui_spec_demo/src/lib.rs`,
`liquers-lib/examples-web/ui_spec_demo/index.html`

**Action:** register a `hello` command, add an *Add Console* menu button
(`hello/q/ns-lui/query_console/add-child`), keep the dashboard button, and add minimal
`.lq-qc-toolbar` / `.lq-query-input` styling.

**Validation:**
```bash
cd liquers-lib/examples-web/ui_spec_demo && trunk build
```

**Rollback:**
```bash
git checkout liquers-lib/examples-web/ui_spec_demo
```

**Agent Specification:**
- **Model:** sonnet
- **Skills:** —
- **Knowledge:** current `lib.rs`, `register_lui_commands!`, `specs/webui/DESIGN.md`
- **Rationale:** wasm entry point; query wiring must be right or the e2e is meaningless.

---

### Step 9: Playwright e2e

**File:** `liquers-lib/examples-web/ui_spec_demo/tests/webui.spec.ts`

**Action:** add the three cases from Phase 3 §End-to-end (Enter submits; the input keeps the
submitted query; a synchronous action repaints). Leave the existing dashboard test untouched.

**Validation:**
```bash
cd liquers-lib/examples-web/ui_spec_demo && npx playwright test
```

**Rollback:**
```bash
git checkout liquers-lib/examples-web/ui_spec_demo/tests/webui.spec.ts
```

**Agent Specification:**
- **Model:** sonnet
- **Skills:** —
- **Knowledge:** existing spec file and `playwright.config.ts`
- **Rationale:** needs correct locators and generous timeouts for the wasm boot.

---

### Step 10: W4 + documentation

**Files:** `specs/ISSUES.md`, `specs/webui/DESIGN.md`, `specs/webui-fixes/DESIGN.md`,
`liquers-lib/examples-web/README.md`

**Action:**
- Mark W1–W3 **Resolved (webui-fixes)** with the fix and its covering test.
- Rewrite "webui: async evaluation engine does not run on wasm" as **Resolved by
  `async-wasm-refactor`** (evidence: `ImmediateAssetManager`, wasm tokio reduced to `["sync"]`,
  Playwright green in headless Chromium), keeping the two genuinely open follow-ups (full tokio
  removal / executor-agnostic core; Tier-2 browser-native I/O) as their own entry.
- Note the console demo in the examples README.

**Validation:**
```bash
grep -n "WEBUI-\|wasm" specs/ISSUES.md
```

**Rollback:**
```bash
git checkout specs liquers-lib/examples-web/README.md
```

**Agent Specification:**
- **Model:** haiku
- **Skills:** —
- **Knowledge:** `specs/ISSUES.md`, `specs/async-wasm-refactor/DESIGN.md`
- **Rationale:** documentation edit with the evidence already collected.

## Testing Plan

### Unit Tests

After Steps 3 and 6:
```bash
cargo test -p liquers-lib --lib ui::
cargo test -p liquers-lib --no-default-features --features webui --lib ui::
```

### Integration Tests

After Step 7, and again after Step 5:
```bash
cargo test -p liquers-lib --test ui_runner --test query_console_integration \
                          --test ui_spec_integration --test ui_shortcuts_integration
cargo test -p liquers-lib --no-default-features --features webui,image-support --test webui_ssr
cargo check -p liquers-lib --no-default-features --features webui --target wasm32-unknown-unknown
cargo test --workspace
```

After Steps 8–9:
```bash
cd liquers-lib/examples-web/ui_spec_demo && npx playwright test
```

### Manual Validation

```bash
cargo run -p liquers-lib --example ui_query_console_app
```
Type a query, press Enter: the egui console behaves exactly as before (result renders, one history
entry per submit — `adopt_query` must be a no-op there).

```bash
cd liquers-lib/examples-web/ui_spec_demo && trunk serve   # http://127.0.0.1:8080
```
*Add Console* → type → Enter → result renders and the input keeps the submitted query.

## Agent Assignment Summary

| Step | Model | Skills | Rationale |
|------|-------|--------|-----------|
| 1 | sonnet | rust-best-practices | Subtle DOM event semantics |
| 2 | sonnet | rust-best-practices | Touches every snapshot construction site |
| 3 | sonnet | rust-best-practices, liquers-unittest | Ordering inside `update` matters |
| 4 | sonnet | rust-best-practices | Must catch every mutation site |
| 5 | haiku | rust-best-practices | Repetitive, exact pattern |
| 6 | sonnet | liquers-unittest | Behavioural assertions |
| 7 | sonnet | liquers-unittest | Async runner flow, flake-prone |
| 8 | sonnet | — | wasm entry point + query wiring |
| 9 | sonnet | — | Locators and wasm-boot timeouts |
| 10 | haiku | — | Documentation with evidence in hand |

Steps 1, 2+3 and 4+5 are independent and can proceed in parallel; 6–9 depend on them.

## Rollback Plan

Each step is an isolated commit with the `git checkout` above. The only changes with any blast
radius are:

- **`AssetSnapshot.query`** (Step 2) — additive field; reverting it also requires reverting Step 3.
- **`deliver_snapshot -> DeliveryOutcome`** (Step 4) — private to `runner.rs`, two call sites.

Nothing touches `liquers-core`, `register_command!`, `liquers-py`, or `liquers-axum`, so reverting
the `liquers-lib/src/ui` hunks restores today's behaviour exactly.

## Documentation Updates

- `specs/ISSUES.md` — W1–W4 statuses (Step 10).
- `specs/webui/DESIGN.md` — note the query-console interaction gaps are closed.
- `specs/webui-fixes/DESIGN.md` — phase tracking → Implementation Complete.
- `liquers-lib/examples-web/README.md` — the console demo and how to run the e2e suite.
- `CLAUDE.md` / `specs/PROJECT_OVERVIEW.md` — no change (no core concepts change).

## Execution Options

1. **Execute now** — run Steps 1–10 in order (or the parallel groups) on this branch.
2. **Create task list** — one task per step, executed later.
3. **Revise plan** — adjust scope (e.g. drop Steps 8–9 if the browser demo should stay minimal).
4. **Exit** — implement manually using this document.
