# Phase 4: Implementation Plan — web-ui-define

## Overview

The order is: the language-neutral element first, then the JavaScript delegate and `element()`,
then the `mount` reconciliation, then the renderer mount (decision 3: the last milestone). After
that, the `onEvent` half.

Prerequisites:

- `web-ui-use`, all steps.
- `ui-events`, merged. It is implemented in PR #225 (open on 2026-10-31). Step 1 uses its
  `UIEvent` type; Step 6 uses its messages and markup.

## Progress

- [ ] Step 1: `DelegatedElement`, `ElementDelegate` and the per-thread registry
- [ ] Step 2: `defineElement`, `undefineElement`, `element()` and `JsElementDelegate` (render only)
- [ ] Step 3: `mount` reconciliation in the mount's `on_tick`
- [ ] Step 4: `run_headless` and the renderer form of `mountUI`
- [ ] Step 5: Declarations, the stubs check, and documentation of Steps 1–4
- [ ] Step 6: `onEvent` through `ui-events` (uses `ui-events`' messages)

## Implementation Steps

### Step 1: `DelegatedElement`
- Files / symbols: `liquers-lib/src/ui/widgets/delegated_element.rs` (new),
  `ui/widgets/mod.rs`, `ui/mod.rs` re-exports.
- Change: as Phase 2. `on_event` is in the trait from the start, taking `&UIEvent`; `update`
  answers `Unchanged` to `UpdateMessage::UI` until Step 6.
- Depends on: `ui-events` merged (PR #225).
- Proof: `cargo test -p liquers-lib --lib ui::widgets::delegated_element`, and with
  `--no-default-features --features webui`.
- Rollback: revert; nothing else uses it yet.

### Step 2: The JavaScript delegate and `element()`
- Files / symbols: `liquers-web/src/ui_define.rs` (new): `JsElementDelegate`, `define_element`,
  `undefine_element`, `element`; `lib.rs` exports; `live_handle_count`.
- Depends on: Step 1 and `web-ui-use` Step 5 (the `UIElement` handle).
- Proof: `uidef01`, `uidef01b`, `uidef01c`, `uidef02`, `uidef02b`, `uidef05`, `uidef06` (Node loop,
  `--features debug-handles`).
- Rollback: revert the module.

### Step 3: `mount` reconciliation
- Files / symbols: `liquers-web/src/ui.rs` `MountInner` (reconciliation map, called from the
  `on_tick` closure built in `mount_ui`).
- Depends on: Step 2.
- Proof: `uidef06b_mount_cleanup_runs_on_replace_and_remove` and `uidef04_dom_matches_ssr`
  (browser loop). `Cargo.toml` gains the web-sys `NodeList` feature.
- Rollback: revert; elements without `mount` are unaffected.

### Step 4: Renderer mount
- Files / symbols: `liquers-lib/src/ui/web/headless.rs` (new) `HeadlessOptions`, `run_headless`;
  `MountHandle`'s root and listener fields become `Option`; `liquers-web/src/ui.rs` `mountRenderer`, `UITree` and `UIChangeRecord` construction,
  `dispatchEvent`. It is declared in Step 4, and until Step 6 it throws `NotSupported`.
- Depends on: `web-ui-use` Step 3.
- Proof: `uidef07_renderer_receives_tree_and_changes`, without its `dispatchEvent` assertion
  until Step 6 (browser loop);
  `cargo check -p liquers-lib --target wasm32-unknown-unknown --no-default-features --features webui`.
- Rollback: revert both files; container mounts are unaffected.

### Step 5: Declarations and documentation
- Files: `liquers-web/src/typescript.rs` (`ElementDefinition`, `UIRenderer`, `UITree`,
  `UIChangeRecord`, `UIUserEvent`); the documents in Phase 2's documentation architecture;
  `liquers-web/README.md` `UIDEF` row (partial until Step 6).
- Proof: `./liquers-web/scripts/check-stubs.sh --build` (and `--records`);
  `python3 scripts/docs_index.py --check`; `./liquers-web/scripts/test-guide-examples.sh`.

### Step 6: `onEvent`
- Files / symbols: `delegated_element.rs` `update` (the `UpdateMessage::UI` arm calls
  `on_event`); `ui_define.rs` `on_event` and the `UIUserEvent` conversion; `ui.rs` `dispatchEvent`
  sends `AppMessage::UIEvent`.
- Depends on: Steps 1–5.
- Proof: `delegated_user_event_replaces_props`; `uidef03`, `uidef08b`, and `uidef07`'s
  `dispatchEvent` assertion; `README.md` `UIDEF` row complete; `WEB-CANNOT-DEFINE-UI-ELEMENTS` →
  `closed`.
- Rollback: revert; elements fall back to `data-lq-action` queries.

## Testing Plan

Step 1 runs the native `liquers-lib` loop, in the default and `webui`-only configurations.
Steps 2 and 5 run the liquers-web Node loop, both feature sets. Steps 3, 4 and 6 run the browser
loop after a `cargo clean`. At the end: `bash scripts/check-build-matrix.sh`.

## Rollback Plan

Every step is additive. If Step 6 has to wait, Steps 1–5 stand on their own. The
remainder (Step 6) then becomes an issue (`DOCS_STRUCTURE_GUIDE.md` §5.6), and `UIDEF03` is
recorded as partial in the README.

## Documentation Updates

As `web-ui-use`, plus the "Elements and renderers" subsection, each document with a `## History`
row and a `reviewed:` bump. `specs/index.csv` regenerated. `specs/command_registry.yaml` is
unchanged.

## Phase 5 Entry Criteria
- [ ] Implementation finished and validated
- [ ] User and review comments answered
- [ ] Documentation checkable against implemented and tested behaviour
