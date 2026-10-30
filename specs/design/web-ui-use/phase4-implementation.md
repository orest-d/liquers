# Phase 4: Implementation Plan — web-ui-use

## Overview

The order is: the prerequisite context, then the backend additions in `liquers-lib`, which are
tested natively, then the liquers-web plumbing that needs no DOM (payload, `lui`, element handles),
then the mount itself, and documentation last. Each step builds and passes its own tests before the
next one starts.

Prerequisite: `js-command-context` (Step 1), decision 2. Steps 2–5 do not depend on it and may be
done first if needed; only Step 8's `uiuse03` waits for it.

## Progress

- [ ] Step 1: Implement `js-command-context` (its own Phase 4, Steps 1–4)
- [ ] Step 2: Runner event outbox, `replace_envref` and stale-result check
- [ ] Step 3: `mount_web_with` and a stoppable `MountHandle`
- [ ] Step 4: Web payload `SimpleUIPayload` and `lui` in the builtins
- [ ] Step 5: `UIElement` handle in both directions
- [ ] Step 6: `mountUI`, `UIMount`, `UISubscription` (Node-testable parts)
- [ ] Step 7: Declarations and the stubs check
- [ ] Step 8: Browser tests and the Playwright spec
- [ ] Step 9: Documentation, conformance row, issue status

## Implementation Steps

### Step 1: Implement `js-command-context`
- Files / symbols: see `specs/design/js-command-context/DESIGN.md` Phase 4.
- Depends on: none.
- Proof: `cargo test -p liquers-web --target wasm32-unknown-unknown --features debug-handles --test commands_COMMAND --test commands_DECLARATION`.
- Rollback: revert its commits; nothing below except `uiuse03` uses it.

### Step 2: Runner events, environment replacement, stale results
- Files / symbols: `liquers-lib/src/ui/runner.rs` `AppRunner` (`events`, `evaluating` now keeps the
  query text), `RunnerEvent`, `replace_envref`, `take_events`; re-export in `ui/mod.rs`.
- Change: as Phase 2. The five event sites; the source re-check in `evaluate_pending_nodes` and
  `poll_evaluating_nodes`; no `_ =>` arms.
- Depends on: none.
- Proof: `cargo test -p liquers-lib --test ui_runner` (the four new tests) and the whole
  `cargo test -p liquers-lib --lib --tests`.
- Rollback: revert the commit. The additions are internal, and `mount_web` does not read them yet.

### Step 3: `mount_web_with` and a stoppable `MountHandle`
- Files / symbols: `liquers-lib/src/ui/web/app.rs` `browser::{MountOptions, mount_web_with,
  MountHandle::stop, is_running, Drop}`; `mount_web` delegates.
- Change: a stop flag, listener removal, root emptied, `current_env` before each tick, `on_tick`
  after `apply_invalidation`. The plain driver drains `take_events`.
- Depends on: Step 2.
- Proof: `cargo check -p liquers-lib --target wasm32-unknown-unknown --no-default-features --features webui`;
  `liquers-lib/examples-web` builds with `trunk build` and its Playwright specs still pass
  (`cd liquers-lib/examples-web && npx playwright test`), after a `cargo clean`.
- Rollback: revert; `mount_web` callers are unaffected either way.

### Step 4: Web payload and `lui`
- Files / symbols: `liquers-web/src/environment.rs` `WebEnvironment`; `builtins.rs`
  `register_builtin_commands`; `tests/eval_EVAL.rs` `eval05`.
- Depends on: none (Step 1 adds `JsPayload` for `SimpleUIPayload`).
- Proof: the whole Node loop, with and without `records`; `uiuse_lui_commands_are_registered`.
- Rollback: revert; the alias change is one line.

### Step 5: `UIElement` handle
- Files / symbols: `liquers-web/src/ui.rs` `LiquersUIElement` and its registry;
  `default_value.rs` `to_js_custom` / `from_js_custom` arms for `ExtValue::UIElement`.
- Depends on: Step 4.
- Proof: `uiuse02`, `uiuse02b`, `uiuse02c` (Node loop).
- Rollback: revert; the arm returns `Ok(None)` again.

### Step 6: `mountUI`, `UIMount`, `UISubscription`
- Files / symbols: `liquers-web/src/ui.rs` `mount_ui`, `MountInner`, `LiquersUIMount` methods;
  `environment.rs` `LiquersEnvironment::mount_ui`; `lib.rs` exports; `Cargo.toml` web-sys
  `Element` and `Document`; `adapter::live_handle_count` includes subscriptions.
- Depends on: Steps 3–5.
- Proof: `uiuse07`, `uiuse07c`, `uiuse08` (Node loop); `cargo check -p liquers-web --target wasm32-unknown-unknown`.
- Rollback: revert the module and the exports.

### Step 7: Declarations
- Files / symbols: `liquers-web/src/typescript.rs` (`MountOptions`, `UIStatusEvent`,
  `UIElementInfo` custom sections and extern types).
- Depends on: Step 6.
- Proof: `./liquers-web/scripts/check-stubs.sh --build`, and with `--records`.
- Rollback: revert.

### Step 8: Browser tests and the Playwright spec
- Files / symbols: `liquers-web/tests/ui_browser_UIUSE.rs`; `liquers-web/tests/e2e/ui.spec.ts`
  and a quickstart page `liquers-web/examples-web/quickstart/ui.html`.
- Depends on: Steps 1–7.
- Proof: `cargo clean`, then `CHROMEDRIVER=$(which chromedriver) cargo test -p liquers-web --target wasm32-unknown-unknown --features browser-tests,debug-handles --test ui_browser_UIUSE`;
  `cd liquers-web/tests/e2e && npx playwright test ui.spec.ts`.
- Rollback: the tests can stay pending only by failing; nothing is skipped or ignored.

### Step 9: Documentation and records
- Files: as Phase 2's documentation architecture; `liquers-web/README.md` conformance row
  (`UIUSE` complete); `WEB-CANNOT-USE-A-LIQUERS-UI` → `closed` with a resolution note;
  `specs/README.md` entry → built.
- Proof: `python3 scripts/docs_index.py --check`; `./liquers-web/scripts/test-guide-examples.sh`.

## Testing Plan

Steps 2 and 3 run the native `liquers-lib` loop. Steps 4–7 run the liquers-web Node loop, both
feature sets. Step 8 runs the browser loops after a `cargo clean`, as CLAUDE.md requires. At the
end: `bash scripts/check-build-matrix.sh`, since Steps 3 and 4 touch `webui`-gated code and the wasm
rows.

## Rollback Plan

Steps 2 and 3 are additive in `liquers-lib`, and they can stay even if the web half is reverted.
Steps 4–8 revert as a group: the payload change on its own is harmless, but with no mount it is
unused. `uiuse03` failing on its own means Step 1 is missing.

## Documentation Updates

`reference/JAVASCRIPT_INTEGRATION.md` and `guides/JAVASCRIPT_GUIDE.md`, each with a `## History`
row and a `reviewed:` bump. `guides/LANGUAGE-INTEGRATION_GUIDE.md` §6 Browser JavaScript (History
row). `liquers-web/README.md`. `specs/index.csv` regenerated. `specs/command_registry.yaml` is
unchanged (no command signature changes); `registry_export` confirms it.

## Phase 5 Entry Criteria
- [ ] Implementation finished and validated
- [ ] User and review comments answered
- [ ] Documentation checkable against implemented and tested behaviour
