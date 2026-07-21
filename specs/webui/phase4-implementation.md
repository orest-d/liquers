# Phase 4: Implementation Plan - webui

> Review note: the `rust-best-practices` checks were applied inline to this plan
> (feature-gating hygiene, object safety, typed errors, no default match arms). The
> per-step "Agent" fields are recommendations **if** the plan is executed with
> subagents; in this environment execution is inline unless the user asks otherwise.

## Overview

**Feature:** webui — web-framework-independent (string-first) rendering backend for
`liquers_lib::ui`, in SSR (native) and browser (wasm) modes, with egui made an optional
feature.

**Architecture:** each `UIElement` gains `render_web(&self) -> String` (the shared SSR/DOM
source of truth) and a wasm `show_in_web` (default = `set_inner_html(render_web)`);
interactivity is a serializable `UiAction` in `data-lq-action` attributes dispatched by one
delegated listener; egui is gated behind an `egui` feature; the browser runs the existing
async engine on a current-thread tokio runtime (capacity-1 `AssetManager`).

**Estimated complexity:** High (touches core feature-gating, adds a wasm target, new module
tree, and an e2e harness).

**Estimated time:** ~3–5 focused days for an experienced Rust dev (the wasm-runtime step is
the schedule risk).

**Prerequisites:** Phases 1–3 approved ✅. Open decisions resolved: string-first rendering,
`UiAction` (4 variants, custom string serde), no `WebValueExtension`, Option A wasm runtime
(keep tokio; verify by test), one new shared `lui/submit` command.

**Milestones (each independently green):**
- **M1 (Steps 1–2):** egui becomes optional — build matrix's egui-only + no-egui configs compile.
- **M2 (Steps 3–5):** shared `UiAction` + `ApplyToInput`/`lui/submit` + wasm-compat spawns, native still green.
- **M3 (Steps 6–13):** the `ui::web` backend + SSR tests pass natively.
- **M4 (Steps 14–16):** the browser example builds, runs, and passes Playwright (proves tokio-on-wasm).

---

## Implementation Steps

### Step 1: Cargo features + wasm tokio split

**Files:** `liquers-lib/Cargo.toml`, `liquers-core/Cargo.toml`

**Action:**
- Add features: `egui = [dep:egui, dep:eframe, dep:egui_plot, dep:egui_extras, dep:egui_commonmark]`;
  `webui = [dep:web-sys, dep:wasm-bindgen, dep:js-sys, dep:wasm-bindgen-futures, dep:pulldown-cmark, dep:console_error_panic_hook]`.
  Keep `default = ["egui", "image-support"]`.
- Make the egui family `optional = true`; add the webui deps (see Phase 2 manifest block).
- In **both** manifests, cfg-split `tokio`: native keeps `["sync","rt","macros","time","fs"]`;
  `cfg(target_arch = "wasm32")` uses `["sync","rt","macros","time"]` (no `fs`).

**Validation:**
```bash
cargo check -p liquers-lib                 # default (egui) still compiles
cargo tree -p liquers-lib -e features | grep -E "egui|web-sys" | head
```
**Rollback:** `git checkout liquers-lib/Cargo.toml liquers-core/Cargo.toml`

**Agent:** haiku · rust-best-practices · Knowledge: Phase 2 "Dependencies", both Cargo.toml ·
Rationale: manifest edits following a specified block.

---

### Step 2: Gate egui behind the `egui` feature (M1)

**Files:** `liquers-lib/src/lib.rs`, `src/value/mod.rs`, `src/ui/element.rs`,
`src/ui/widgets/{markdown_element,query_console_element,ui_spec_element}.rs`,
`src/ui/shortcuts.rs`, `liquers-core/src/store.rs`

**Action:**
- `lib.rs`: `#[cfg(feature = "egui")] pub mod egui;`.
- `value/mod.rs`: add `#[cfg(feature = "egui")]` to `ExtValue::UiCommand`/`Widget` **and** to
  every arm referencing them in `identifier`/`type_name`/`default_extension`/`default_filename`/
  `as_image`/`as_polars_dataframe`/`as_ui_element` + serializer arms (pair variant cfg with arm cfg).
- `element.rs`: remove stray `use egui::debug_text::print;` (line 3); `#[cfg(feature="egui")]` on
  `show_in_egui` (trait default + `AssetViewElement`/`StateViewElement` impls) and their egui imports.
- widget elements: `#[cfg(feature="egui")]` on each `show_in_egui` + egui/`egui_commonmark` imports.
- `shortcuts.rs`: `#[cfg(feature="egui")]` on `Key::to_egui` + the `egui` import.
- `store.rs`: `#[cfg(not(target_arch = "wasm32"))]` on `AsyncFileStore` + its `tokio::fs` block
  (keep `AsyncMemoryStore`).

**Validation:**
```bash
cargo check -p liquers-lib                                                # egui on
cargo check -p liquers-lib --no-default-features --features image-support # no egui, no web → must compile
```
**Rollback:** `git checkout` the listed files.

**Agent:** sonnet · rust-best-practices · Knowledge: Phase 2 "ExtValue"/"Integration", the six
files · Rationale: cfg hygiene across exhaustive matches needs care (the #1 feature-flag bug source).

---

### Step 3: Shared `UiAction` + `dispatch_action`; migrate `MenuAction` (M2)

**Files:** `liquers-lib/src/ui/action.rs` (new), `src/ui/mod.rs`,
`src/ui/widgets/ui_spec_element.rs`

**Action:**
- New `action.rs`: `pub enum UiAction { None, Quit, Query(String), Apply { handle, input_id, query } }`
  with **hand-written** `Serialize`/`Deserialize` (string forms `"none"`/`"quit"`/bare-query/
  `"apply:{h}:{id}:{q}"` via `splitn(4, ':')`; map forms `{query}`/`{apply}`; bare string → `Query`).
  `pub fn dispatch_action(action: &UiAction, ctx: &UIContext, own_handle: Option<UIHandle>)`.
- `mod.rs`: `pub mod action; pub use action::{UiAction, dispatch_action};`.
- `ui_spec_element.rs`: replace `MenuAction` with `UiAction` (or alias), keep the YAML forms;
  `handle_menu_action` → `dispatch_action`. `Apply`/`Quit` route to `ctx`; `None` no-ops.

**Validation:**
```bash
cargo test -p liquers-lib ui::action ui_spec_element     # serde + menu parsing
cargo check -p liquers-lib
```
**Rollback:** `rm src/ui/action.rs`; `git checkout ui/mod.rs ui_spec_element.rs`.

**Agent:** sonnet · rust-best-practices, liquers-unittest · Knowledge: Phase 2 "UiAction",
`ui_spec_element.rs` (MenuAction) · Rationale: custom serde + a shared refactor.

---

### Step 4: `AppMessage::ApplyToInput` + runner handling + `lui/submit` (M2)

**Files:** `liquers-lib/src/ui/message.rs`, `src/ui/runner.rs`, `src/ui/commands.rs`

**Action:**
- `message.rs`: add `AppMessage::ApplyToInput { handle: UIHandle, input: String, query: String }`.
  Update every exhaustive `AppMessage` match (runner `process_messages`; the `query_console` test).
- `runner.rs`: new arm builds `State::from_parts(Arc::new(Value::from(input)), Arc::new(Metadata::new()))`,
  parses `query`, and calls `self.envref.get_asset_manager().apply_immediately(Recipe::from(q), state, Some(payload))`
  with a `SimpleUIPayload` bound to `handle`; deliver result like `SubmitQuery`.
- `commands.rs`: add `fn submit(state, context)` (sync) — read state as query string, submit bound
  to current handle via the console's path (`RequestAssetUpdates`); register in `register_lui_commands!`.

**Validation:**
```bash
cargo test -p liquers-lib ui::runner ui::commands
cargo check -p liquers-lib
```
**Rollback:** `git checkout` the three files.

**Agent:** sonnet · rust-best-practices, liquers-unittest · Knowledge: Phase 2 "How Apply is used",
`runner.rs`, `commands.rs`, `assets.rs` (`apply_immediately`) · Rationale: async runner logic + a new command.

---

### Step 5: wasm-compat spawns/timers (M2)

**Files:** `src/ui/element.rs` (`AssetViewElement::from_asset_ref`),
`src/ui/widgets/query_console_element.rs` (`schedule_volatile_refresh`)

**Action:**
- Replace direct `tokio::spawn(...)` with `crate::ui::spawn_ui_task(...)` (native/​wasm split already
  exists).
- For the console's `tokio::time::sleep`, use `spawn_ui_task` + a cfg'd timer (native `tokio::time`;
  wasm a `gloo-timers`/`setTimeout` future) — or keep `tokio::time` if the Step-15 wasm test confirms it.

**Validation:**
```bash
cargo check -p liquers-lib                                # native unaffected
cargo test -p liquers-lib ui::element ui::widgets
```
**Rollback:** `git checkout` both files.

**Agent:** haiku · rust-best-practices · Knowledge: `ui/mod.rs::spawn_ui_task`, the two sites ·
Rationale: mechanical spawn-routing.

---

### Step 6: `ui/web/html.rs` — escaping, action attr, value_to_html (M3)

**Files:** `src/ui/web/mod.rs` (new), `src/ui/web/html.rs` (new)

**Action:**
- `escape_html`, `action_attr(&UiAction) -> String`, `element_dom_id(Option<UIHandle>) -> String`,
  `value_to_html(&Value) -> String` (explicit match over base + `ExtValue`; egui variants under
  `#[cfg(feature="egui")]` as inert placeholders; `Image`→data-URI, `PolarsDataFrame`→`dataframe_to_html`,
  `UIElement`→`render_web`).
- `mod.rs`: `#[cfg(feature="webui")]`-gated module wiring + `pub fn element_dom_id`.

**Validation:**
```bash
cargo test -p liquers-lib --no-default-features --features webui,image-support ui::web::html
```
**Rollback:** `rm -r src/ui/web` (until wired in Step 11 keep isolated).

**Agent:** sonnet · rust-best-practices, liquers-unittest · Knowledge: Phase 2 "html helpers",
Phase 3 Example 3, `value/mod.rs` (variants), egui `UIValueExtension::show` · Rationale: exhaustive
value match + escaping correctness.

---

### Step 7: `ui/web/widgets.rs` + `dataframe.rs` (M3)

**Files:** `src/ui/web/widgets.rs` (new), `src/ui/web/dataframe.rs` (new)

**Action:** `status_html`, `progress_html(&ProgressEntry)`, `asset_info_html(&AssetInfo)`,
`error_html(&Error)`, `query_to_html(&str)`; `dataframe_to_html(&DataFrame, max_rows)` (escaped
`<th>`/`<td>`, row cap + truncation note). Web peers of `egui/widgets.rs`/`egui/dataframe.rs`.

**Validation:**
```bash
cargo test -p liquers-lib --no-default-features --features webui,image-support ui::web::widgets ui::web::dataframe
```
**Rollback:** `rm src/ui/web/widgets.rs src/ui/web/dataframe.rs`.

**Agent:** haiku · rust-best-practices · Knowledge: `egui/widgets.rs`, `egui/dataframe.rs`, Phase 2
signatures · Rationale: string builders mirroring existing helpers.

---

### Step 8: `render_web` trait method + per-element overrides (M3)

**Files:** `src/ui/element.rs`, the three `widgets/*` files

**Action:**
- Add `#[cfg(feature="webui")] fn render_web(&self, app_state: &dyn AppState) -> String` to the
  `UIElement` trait with the titled-block default.
- Override for `Placeholder`, `StateViewElement`, `AssetViewElement` (progress/value/error/metadata
  by `view_mode`), `MarkdownElement` (`pulldown-cmark` → HTML), `QueryConsoleElement` (toolbar with
  the query input + `Apply` action + content), `UISpecElement` (menu bar `UiAction` attrs + each
  `LayoutSpec` → container markup; children via `app_state.get_element(child).render_web`).

**Validation:**
```bash
cargo test -p liquers-lib --no-default-features --features webui,image-support   # per-element render_web tests
cargo check -p liquers-lib --features egui,webui                                 # both backends coexist
```
**Rollback:** `git checkout` the four files.

**Agent:** sonnet · rust-best-practices, liquers-unittest · Knowledge: Phase 2 "render_web",
Phase 3 tests, each element's `show_in_egui` (for parity) · Rationale: per-element rendering + recursion.

---

### Step 9: `show_in_web` (wasm) + `render_element_web`/`render_element_dom` (M3)

**Files:** `src/ui/element.rs`, `src/ui/web/mod.rs`

**Action:**
- `#[cfg(all(feature="webui", target_arch="wasm32"))] fn show_in_web(&mut self, document, container,
  ctx, app_state) -> Result<(), Error>` default = `set_inner_html(&render_web)`; override in
  `QueryConsoleElement` for in-place input-focus preservation.
- `render_element_web(handle, &dyn AppState) -> String` (SSR, immutable) and
  `render_element_dom(document, container, handle, ctx, &Arc<Mutex<AppState>>)` (wasm,
  extract-render-replace).

**Validation:**
```bash
cargo check -p liquers-lib --no-default-features --features webui,image-support
cargo build --target wasm32-unknown-unknown -p liquers-lib --no-default-features --features webui,image-support
```
**Rollback:** `git checkout src/ui/element.rs src/ui/web/mod.rs`.

**Agent:** sonnet · rust-best-practices · Knowledge: Phase 2 "Rendering Model"/"granularity",
`render_element` (egui) · Rationale: wasm DOM + extract-render-replace.

---

### Step 10: `ui/web/app.rs` — `render_app_ssr`, `mount_web`, `MountHandle` (M3/M4)

**Files:** `src/ui/web/app.rs` (new)

**Action:**
- `render_app_ssr(&Arc<Mutex<dyn AppState>>) -> Result<String, Error>` (all targets): lock, render
  every root via `render_element_web`, concatenate.
- `mount_web(...)` + `MountHandle` (wasm): enter a current-thread tokio runtime; build/serve initial
  DOM; attach the single delegated `data-lq-action` listener (`click`/`keydown`) → parse `UiAction`
  → `dispatch_action` / `ApplyToInput`; drive `AppRunner::run` on a `requestAnimationFrame` loop,
  re-rendering targeted elements on `needs_repaint()`. Construct `AssetManager` with capacity 1.

**Validation:**
```bash
cargo test -p liquers-lib --no-default-features --features webui,image-support ui::web::app  # render_app_ssr
cargo build --target wasm32-unknown-unknown -p liquers-lib --no-default-features --features webui,image-support
```
**Rollback:** `rm src/ui/web/app.rs`.

**Agent:** sonnet · rust-best-practices · Knowledge: Phase 2 "Browser Runtime & Workflow",
`ui_spec_demo.rs` (setup), `runner.rs` · Rationale: the driver + event delegation + runtime entry.

---

### Step 11: Wire `ui::web` into `ui/mod.rs` (M3)

**Files:** `src/ui/mod.rs`

**Action:** `#[cfg(feature="webui")] pub mod web;` + `#[cfg(feature="webui")] pub use
web::{render_element_web, WebAction? no}` → export `render_element_web`, `element_dom_id`, and
(wasm) `mount_web`/`MountHandle`; `render_app_ssr`.

**Validation:**
```bash
cargo check -p liquers-lib --no-default-features --features webui,image-support
cargo check -p liquers-lib --features egui,webui
```
**Rollback:** `git checkout src/ui/mod.rs`.

**Agent:** haiku · rust-best-practices · Knowledge: `ui/mod.rs`, Phase 2 module list · Rationale: re-exports.

---

### Step 12: Unit tests for `ui::web` + `UiAction` (M3)

**Files:** inline `#[cfg(test)]` in `action.rs`, `web/html.rs`, `web/widgets.rs`, `web/dataframe.rs`,
and each element module.

**Action:** the Phase 3 unit suite — `escape_html`, `action_attr` round-trip, `value_to_html` per
variant + escaping, widgets/dataframe markup + row cap, `UiAction` string-form round-trips + bare-query
→ `Query` + `MenuAction` YAML back-compat, per-element `render_web` assertions.

**Validation:**
```bash
cargo test -p liquers-lib --no-default-features --features webui,image-support
cargo test -p liquers-lib     # egui build unit tests still pass (no regressions)
```
**Rollback:** remove the added test modules.

**Agent:** sonnet · rust-best-practices, liquers-unittest · Knowledge: Phase 3 "Test Plan" · Rationale:
comprehensive coverage incl. security (escaping).

---

### Step 13: SSR integration test (M3)

**Files:** `liquers-lib/tests/webui_ssr.rs` (new)

**Action:** `#[tokio::test]`s from Phase 3: `ssr_renders_tree_to_html`, `ssr_via_lui_query`,
`ssr_dom_parity`, `appstate_roundtrip_ssr`, `lui_submit_submits_input_as_query`. `type
CommandEnvironment = DefaultEnvironment<Value, SimpleUIPayload>;`.

**Validation:**
```bash
cargo test -p liquers-lib --no-default-features --features webui,image-support --test webui_ssr
```
**Rollback:** `rm liquers-lib/tests/webui_ssr.rs`.

**Agent:** sonnet · rust-best-practices, liquers-unittest · Knowledge: Phase 3 examples,
`ui_spec_demo.rs`, `async_hellow_world.rs` · Rationale: end-to-end native flow.

---

### Step 14: Browser example crate (M4)

**Files:** `liquers-lib/examples-web/ui_spec_demo/{Cargo.toml,src/lib.rs,index.html,Trunk.toml}` (new)

**Action:** `cdylib` crate depending on `liquers-lib` (`--no-default-features --features webui,image-support`);
`#[wasm_bindgen(start)] start()` (Phase 3 Example 2 setup: env + `dashboard`/`dashboard2` + `lui`,
`DirectAppState` root `UISpecElement`, `mount_web`, `std::mem::forget(mount)`); `index.html`
(`<div id="app">` + `data-trunk rel="rust"`); `Trunk.toml`.

**Validation:**
```bash
cd liquers-lib/examples-web/ui_spec_demo && trunk build          # wasm32 + tokio-on-wasm compiles
```
**Rollback:** `rm -r liquers-lib/examples-web/ui_spec_demo`.

**Agent:** sonnet · rust-best-practices · Knowledge: Phase 3 Example 2, `ui_spec_demo.rs`, Phase 2
"Examples, Browser Setup" · Rationale: wasm entry + build wiring.

---

### Step 15: ⚠ Gating step — run the browser example; verify tokio-on-wasm (M4)

**Files:** `liquers-lib/examples-web/ui_spec_demo/tests/webui.spec.ts`, `playwright.config.ts` (new)

**Action:** `trunk serve` the example; Playwright (headless Chromium, pre-installed) navigates,
asserts the menu renders, clicks "Add Dashboard", asserts a new `.lq-UISpecElement`. **This proves the
async engine (spawn + time) runs on wasm.** *If it fails on the runtime:* apply the contingency —
swap `tokio` → `tokio_with_wasm` on the `cfg(wasm32)` target (Step 1 manifests), re-run. Only this
step decides whether the contingency is needed.

**Validation:**
```bash
cd liquers-lib/examples-web/ui_spec_demo && trunk serve &        # http://127.0.0.1:8080
npx playwright test                                              # must pass
```
**Rollback:** stop `trunk serve`; `rm` the test files. (Runtime contingency documented above.)

**Agent:** sonnet · rust-best-practices · Knowledge: Phase 2 "Browser Runtime", Phase 3 e2e ·
Rationale: the highest-risk step; may trigger the runtime contingency decision.

---

### Step 16: Build matrix + final validation

**Action:** run the full matrix and workspace tests; fmt/clippy.

**Validation:**
```bash
cargo check -p liquers-lib                                                        # default (egui)
cargo check -p liquers-lib --no-default-features --features webui,image-support   # webui only (no egui)
cargo check -p liquers-lib --no-default-features --features egui,image-support    # egui only
cargo check -p liquers-lib --features egui,webui                                  # both
cargo build --target wasm32-unknown-unknown -p liquers-lib --no-default-features --features webui,image-support
cargo test --workspace                                                            # no regressions
cargo fmt --all -- --check && cargo clippy -p liquers-lib --all-features -- -D warnings
```
**Rollback:** N/A (final gate).

**Agent:** sonnet · rust-best-practices · Knowledge: all impl files, Phase 2 "Compilation Validation" ·
Rationale: judgment on any surfaced issue.

---

### Step 17: Docs

**Files:** `CLAUDE.md`, `specs/webui/DESIGN.md`, optionally `specs/PROJECT_OVERVIEW.md`

**Action:** note the `egui`/`webui` features + the two rendering methods in CLAUDE.md's UI section;
mark webui phases complete in `DESIGN.md`; cross-link `UI_WEB_DESIGN_NOTES.md`. No core-concept change
for PROJECT_OVERVIEW beyond a pointer.

**Validation:** manual read; links resolve.

**Agent:** haiku · — · Knowledge: CLAUDE.md UI section, this plan · Rationale: documentation.

---

## Testing Plan

- **Unit** (after Steps 3,6,7,8,12): `cargo test -p liquers-lib --no-default-features --features webui,image-support`
  (web modules + `UiAction`) and `cargo test -p liquers-lib` (egui build, no regressions).
- **Integration** (after Step 13): `cargo test -p liquers-lib --no-default-features --features webui,image-support --test webui_ssr`.
- **Browser e2e** (Step 15): `trunk serve` + `npx playwright test` — the working-example success
  criterion and the tokio-on-wasm proof.
- **Build matrix** (Step 16): the five configs above must all be green — this *is* the egui/webui
  independence guarantee.

**Success criteria:** all matrix configs compile; native unit + SSR integration tests pass; the
Playwright test passes (browser renders and reacts).

## Agent Assignment Summary

| Step | Model | Skills | Rationale |
|------|-------|--------|-----------|
| 1 | haiku | rust-best-practices | Manifest edits (specified block) |
| 2 | sonnet | rust-best-practices | cfg hygiene across exhaustive matches |
| 3 | sonnet | rbp, liquers-unittest | Custom serde + shared refactor |
| 4 | sonnet | rbp, liquers-unittest | Async runner + new command |
| 5 | haiku | rust-best-practices | Mechanical spawn-routing |
| 6 | sonnet | rbp, liquers-unittest | Exhaustive value match + escaping |
| 7 | haiku | rust-best-practices | String builders mirroring egui helpers |
| 8 | sonnet | rbp, liquers-unittest | Per-element rendering + recursion |
| 9 | sonnet | rust-best-practices | wasm DOM + extract-render-replace |
| 10 | sonnet | rust-best-practices | Driver + delegation + runtime entry |
| 11 | haiku | rust-best-practices | Module re-exports |
| 12 | sonnet | rbp, liquers-unittest | Coverage incl. escaping security |
| 13 | sonnet | rbp, liquers-unittest | Native end-to-end |
| 14 | sonnet | rust-best-practices | wasm entry + build wiring |
| 15 | sonnet | rust-best-practices | Gating: e2e + runtime contingency |
| 16 | sonnet | rust-best-practices | Final validation judgment |
| 17 | haiku | — | Documentation |

## Rollback Plan

**Per-step:** commands listed in each step. **Milestone-scoped:** M1–M4 are ordered so each ends on a
green build; revert to the last green milestone if a later one stalls.

**Full feature rollback:**
```bash
git checkout <default-branch> -- liquers-lib/Cargo.toml liquers-core/Cargo.toml \
  liquers-lib/src/lib.rs liquers-lib/src/value/mod.rs liquers-lib/src/ui/mod.rs \
  liquers-lib/src/ui/element.rs liquers-lib/src/ui/shortcuts.rs \
  liquers-lib/src/ui/message.rs liquers-lib/src/ui/runner.rs liquers-lib/src/ui/commands.rs \
  liquers-lib/src/ui/widgets/*.rs liquers-core/src/store.rs
rm -r liquers-lib/src/ui/web liquers-lib/src/ui/action.rs \
  liquers-lib/tests/webui_ssr.rs liquers-lib/examples-web
# Remove egui/webui features + optional markers from the manifests.
```
**Partial completion:** work is already on branch `claude/liquers-webui-design-hvy9c2`; commit WIP per
milestone and record status in `specs/webui/DESIGN.md`.

## Documentation Updates
- **CLAUDE.md** — UI section: the `egui`/`webui` features and `render_web`/`show_in_web` methods (new pattern).
- **specs/webui/DESIGN.md** — mark phases complete.
- **PROJECT_OVERVIEW.md** — a one-line pointer to the web backend (no core-concept change).

## Risks & Open Items
1. **tokio-on-wasm (Step 15)** — the one item settled only by running the example; contingency
   (`tokio_with_wasm`) is pre-staged and swaps in via one manifest line.
2. **`UISpecElement` `Windows`/`Tabs`** — first cut = stacked panels + tab buttons (drag deferred).
3. **`pulldown-cmark` options** — sanitization moot (we escape + control markup); pick a sensible default.
4. **Focus preservation** — relies on the `QueryConsoleElement::show_in_web` override + targeted re-render;
   validated by manual/e2e check that typing survives a repaint.

## Confidence

**~95%** for the native scope (M1–M3): all signatures, files, and patterns are verified against the
current source. The one sub-95% area is **M4's wasm runtime** — deliberately isolated as the gating
Step 15 with a pre-staged contingency, so even a failure there has a known, bounded resolution rather
than an architectural rethink.
