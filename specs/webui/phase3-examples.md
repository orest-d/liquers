# Phase 3: Examples & Use-cases - webui

> Drafting note: the liquers-designer workflow suggests spawning haiku drafter/reviewer
> subagents here. Per this environment's guidance (don't spawn agents unless the user
> asks), the drafting and the Phase-1/2/codebase/query reviews were done **inline**. The
> `liquers-unittest` and `rust-best-practices` skills were applied.

## Example Type

**Runnable prototypes.** The user set a working browser example (trunk + Playwright) as a
success criterion, so examples are executable, not conceptual. Three tiers: a pure-native
SSR/`render_web` example (no wasm, fully unit-testable), the wasm browser example
(`ui_spec_demo` port, the e2e success criterion), and a value-rendering edge example.

## Overview Table

| # | Type | Name | Purpose | Runs on |
|---|------|------|---------|---------|
| 1 | Example | SSR render of a `lui` tree | Render a built element tree to an HTML string; prove SSR + `render_web` work with no browser | native |
| 2 | Example | `ui_spec_demo` webui (browser) | Menu click → `UiAction` → query → new element, live in the DOM; **the e2e success criterion** | wasm/browser |
| 3 | Example | `value_to_html` / element states | Render each `Value` variant + element view-modes (progress/value/error), incl. HTML escaping | native |
| 4 | Unit Tests | `ui::web` + `UiAction` suite | `escape_html`, `action_attr`, `value_to_html`, `widgets`, `dataframe_to_html`, per-element `render_web`, `UiAction`/`MenuAction` serde | native |
| 5 | Integration + Corner | SSR e2e + build matrix + wasm runtime | `AppRunner`→`render_app_ssr` flow, SSR/DOM parity, feature-gate build matrix, tokio-on-wasm | native + wasm |

## Example 1: SSR render of a `lui`-built tree

**Scenario:** A server builds a UI element tree (as egui would) and renders it to an HTML
string for a first paint / no-JS fallback.

**Context:** SSR mode — the server-side half of "two modes". No wasm, no DOM; the exact path
a future `liquers-axum` handler would call.

**Code (runnable as `liquers-lib/tests/webui_ssr.rs`, native):**
```rust
use std::sync::Arc;
use liquers_lib::ui::{AppState, DirectAppState, ElementSource, UIElement};
use liquers_lib::ui::web::render_app_ssr;
use liquers_lib::ui::widgets::markdown_element::MarkdownElement;
use liquers_lib::ui::widgets::ui_spec_element::{UISpec, UISpecElement};
use liquers_lib::value::Value;

#[tokio::test]
async fn ssr_renders_tree_to_html() -> Result<(), Box<dyn std::error::Error>> {
    // Build a UISpec root with one markdown child, directly (no query engine needed).
    let mut state = DirectAppState::new();
    let root = state.add_node(None, 0, ElementSource::None)?;
    let spec = UISpec::from_yaml("layout: vertical")?;
    let mut root_el = UISpecElement::from_spec("Dashboard".into(), spec);
    root_el.set_handle(root);
    state.set_element(root, Box::new(root_el))?;

    let child = state.add_node(Some(root), 0, ElementSource::None)?;
    let mut md = MarkdownElement::new("Doc".into(), "# Title\n\nBody".into());
    md.set_handle(child);
    state.set_element(child, Box::new(md))?;

    let app_state: Arc<tokio::sync::Mutex<dyn AppState>> = Arc::new(tokio::sync::Mutex::new(state));
    let html = render_app_ssr(&app_state).await?;

    assert!(html.contains(&format!("id=\"ui-element-{}\"", root.0)));
    assert!(html.contains("<h1>Title</h1>"));  // markdown → HTML via pulldown-cmark
    assert!(html.contains("Body"));
    Ok(())
}
```

**Expected output:** an HTML fragment such as
```html
<div id="ui-element-1" class="lq-element lq-UISpecElement">
  <div class="lq-layout-vertical">
    <div id="ui-element-2" class="lq-element lq-MarkdownElement"><h1>Title</h1><p>Body</p></div>
  </div>
</div>
```

**Validation:** compiles native; deterministic string; no wasm/DOM; exercises `render_web`
recursion (container → child) and the markdown web renderer.

## Example 2: `ui_spec_demo` in the browser (wasm) — the e2e success criterion

**Scenario:** The existing native `examples/ui_spec_demo.rs` (menu-driven dashboard) runs in
a browser: the menu bar renders, clicking **"Add Dashboard"** dispatches a `UiAction`, the
query evaluates, and a nested dashboard appears — all live in the DOM.

**Context:** Browser/full-support mode; validates the whole chain, including the tokio-on-wasm
runtime (the Phase-2 gating risk). Reuses the demo's **verified** commands and queries.

**Rust entry (`examples-web/ui_spec_demo/src/lib.rs`), abbreviated:**
```rust
#[wasm_bindgen(start)]
pub async fn start() -> Result<(), JsValue> {
    console_error_panic_hook::set_once();
    let document = web_sys::window().unwrap().document().unwrap();
    let root = document.get_element_by_id("app").unwrap();

    let mut env = DefaultEnvironment::<Value, SimpleUIPayload>::new();
    env.with_trivial_recipe_provider();
    let envref = {
        let cr = env.get_mut_command_registry();
        register_command!(cr, fn dashboard(state) -> result).map_err(err_to_js)?;
        register_command!(cr, fn dashboard2(state) -> result).map_err(err_to_js)?;
        liquers_lib::register_lui_commands!(cr).map_err(err_to_js)?;
        env.to_ref()
    };

    let mut app_state = DirectAppState::new();
    let root_handle = app_state.add_node(None, 0, ElementSource::None).map_err(err_to_js)?;
    let spec = UISpec::from_yaml(DASHBOARD_YAML).map_err(err_to_js)?;   // same YAML as native demo
    let mut element = UISpecElement::from_spec("Dashboard".into(), spec);
    element.set_handle(root_handle);
    app_state.set_element(root_handle, Box::new(element)).map_err(err_to_js)?;

    let app_state_arc: Arc<tokio::sync::Mutex<dyn AppState>> = Arc::new(tokio::sync::Mutex::new(app_state));
    let (tx, rx) = app_message_channel();
    let mount = mount_web(root, envref, app_state_arc, tx.clone(), rx, None).await.map_err(err_to_js)?;
    std::mem::forget(mount);   // keep the single delegated listener alive
    Ok(())
}
```

The menu button in `DASHBOARD_YAML` carries `action: { query: "dashboard/q/ns-lui/add-child" }`
— the exact query the native demo already runs, now surfaced as a `data-lq-action` attribute
(a `UiAction::Query`) and dispatched by the delegated listener.

**Expected output (browser):** the dashboard menu bar renders; clicking "Add Dashboard" adds a
nested `.lq-UISpecElement` under the root.

**Validation (Playwright, `tests/webui.spec.ts`, run against `trunk serve`):**
```ts
import { test, expect } from '@playwright/test';
test('ui_spec_demo renders and reacts to a menu action', async ({ page }) => {
  await page.goto('http://127.0.0.1:8080');
  await expect(page.locator('#app .lq-UISpecElement')).toBeVisible();
  await expect(page.getByText('Add Dashboard')).toBeVisible();
  const before = await page.locator('#app .lq-UISpecElement').count();
  await page.getByText('Add Dashboard').click();
  await expect.poll(() => page.locator('#app .lq-UISpecElement').count()).toBeGreaterThan(before);
});
```

**Validation checklist:** builds to `wasm32-unknown-unknown` via `trunk build`; runs on the
current-thread tokio runtime (capacity-1 `AssetManager`); the click→query→re-render loop turns
(this is what proves the wasm runtime works).

## Example 3: `value_to_html` and element view-modes

**Scenario:** Render assorted `Value`s and the `AssetViewElement` lifecycle states to HTML,
including hostile text that must be escaped.

**Context:** The internal rendering used whenever a value sits inside a `StateViewElement` /
`AssetViewElement` (which is every value that reaches the tree).

**Code (illustrative; assertions in the unit suite):**
```rust
use liquers_lib::ui::web::html::value_to_html;
use liquers_lib::value::Value;

// Plain text is escaped (no raw HTML injection).
let h = value_to_html(&Value::from("<script>alert(1)</script>"));
assert!(h.contains("&lt;script&gt;"));
assert!(!h.contains("<script>"));

// None / Bool / numbers render as labelled spans.
assert!(value_to_html(&Value::none()).contains("None"));
assert!(value_to_html(&Value::from(true)).contains("true"));
```

**Expected output:** `&lt;script&gt;alert(1)&lt;/script&gt;` (escaped); an `<img src="data:…">`
for `ExtValue::Image`; a `<table>` for `ExtValue::PolarsDataFrame`; delegation to `render_web`
for `ExtValue::UIElement`.

**Validation:** compiles native; asserts escaping (security-relevant) and per-variant markup.

## Corner Cases

### 1. Memory
- **Large DataFrame → HTML.** `dataframe_to_html(df, max_rows)` caps rows (e.g. 100) and notes
  truncation, so a million-row frame never materializes a million `<tr>`s. Test: 10k-row frame,
  assert output row count ≤ cap + header.
- **Deep/large trees.** `render_web` builds one `String`; recursion depth = tree depth. Test:
  a 100-child container renders without unbounded blowup; string length is bounded by content.
- **Browser re-render churn.** Whole-root `innerHTML` only on `needs_repaint()` (not per frame);
  the single delegated listener means no per-node `Closure` accumulation (no leak).

### 2. Concurrency
- **SSR render vs async engine.** `render_app_ssr` locks the async `AppState` mutex; the
  immutable `render_web` never mutates, so it cannot self-contend. Test: render the same tree
  twice concurrently (native), assert identical output.
- **Browser single-thread.** wasm is single-threaded: `try_sync_lock` never contends; `Send`
  bounds are vacuous; `AssetManager` capacity 1 serializes jobs.
- **Background asset task.** `AssetViewElement::from_asset_ref` spawns via `spawn_ui_task`
  (tokio native / `spawn_local` wasm); shared state via `Arc<std::sync::RwLock>` is read during
  render. Test (native): drive an asset to completion, assert the element flips to Value mode.

### 3. Errors
- **`render_web` is infallible** (`-> String`) — it always renders *something* (error/placeholder
  markup), mirroring `egui::Ui`. `AssetViewElement` Error mode → `error_html(err)`; a pending
  node → a "Loading…" placeholder.
- **Fallible edges return `Result<_, Error>`** with typed constructors: `render_app_ssr`
  (lock unavailable), `mount_web` (missing `#app` root → `Error::general_error`),
  `show_in_web`/`render_element_dom` (`web-sys`/`JsValue` failures mapped to `Error`). No
  `unwrap`/`expect` in library code.
- **Hostile content** never breaks markup: all text goes through `escape_html` (see Example 3).

### 4. Serialization
- **`UiAction` round-trips** through serde_json (`SubmitQuery`, `Query`, `QueryOn`,
  `SubmitInput`, `Platform`, `Quit`, `None`). Test: each variant JSON round-trips to itself.
- **`MenuAction` YAML back-compat** preserved on `UiAction`'s custom `Deserialize`: `null`→`None`,
  `"quit"`→`Quit`, `{query: "x"}`→`Query("x")`. Test: existing `ui_spec` YAML still parses.
- **AppState ⇄ SSR parity.** Serialize a `DirectAppState`, deserialize, `render_app_ssr` →
  identical HTML topology/ids (element `#[serde(skip)]` runtime values are absent by design;
  ids/titles/structure persist). Test: build tree, JSON round-trip, assert same element ids in HTML.

### 5. Integration (cross-crate / cross-feature)
- **`lui` command → tree → `render_web`.** Register `lui`, submit `add`/`markdown`/`ui_spec`
  queries via the message channel, run `AppRunner`, then `render_app_ssr`. Test asserts the
  produced HTML reflects the built tree. Queries used are those the native `ui_spec_demo`
  already runs (so they are known-registered/valid).
- **`UiAction` dispatch → `submit_query` → new element.** The browser e2e (Example 2) is the
  cross-feature integration: delegated listener → `dispatch_action` → `UIContext::submit_query`
  → `AppRunner` evaluates → re-render.
- **Feature-gate build matrix** (compile-level integration): egui-only, webui-only, both, wasm.
- **tokio on wasm** (the gating risk): the browser example must actually evaluate a query
  (spawn a job, run it, deliver a snapshot) — verified by the Playwright click assertion.

## Test Plan

### Unit tests (inline `#[cfg(test)] mod tests`, native, per new module)

Conventions: `-> Result<(), Box<dyn std::error::Error>>` where `?` is used; typed errors; no
`unwrap`/`expect` outside tests; `#[test]` (these are all synchronous string checks).

**`ui/web/html.rs`:**
```rust
#[test] fn escape_html_escapes_all_five() {
    assert_eq!(escape_html("<a href=\"x\">&'"), "&lt;a href=&quot;x&quot;&gt;&amp;&#39;");
}
#[test] fn action_attr_round_trips() -> Result<(), Box<dyn std::error::Error>> {
    let a = UiAction::Query("text-hi".into());
    let attr = action_attr(&a);               // ` data-lq-action='{...}'`
    assert!(attr.contains("data-lq-action"));
    // extract json, deserialize back to the same action
    Ok(())
}
#[test] fn value_to_html_escapes_text() {
    assert!(value_to_html(&Value::from("<b>")).contains("&lt;b&gt;"));
}
#[test] fn value_to_html_covers_each_variant() { /* None,Bool,I32,I64,F64,Text,Bytes,Image,DataFrame,UIElement,… */ }
```

**`ui/web/widgets.rs` / `dataframe.rs`:** `status_html`/`progress_html`/`asset_info_html`/
`error_html` return non-empty markup containing the salient text; `dataframe_to_html` includes a
`<th>` header row, caps at `max_rows`, and escapes cell values.

**`ui/action.rs`:** `UiAction` serde_json round-trip for every variant; `MenuAction`-form YAML
(`null`/`"quit"`/`{query}`) deserializes to the right `UiAction`; a shared `dispatch_action`
sends the expected `AppMessage` on a test channel (mirrors existing `query_console` tests).

**Per element `render_web` (in each element's test module):**
- `Placeholder` → title block with `ui-element-{h}` id.
- `StateViewElement` → contains the value's `value_to_html`.
- `AssetViewElement` → Progress (spinner/progress markup), Value (value html), Error (`error_html`),
  Metadata (asset-info html) per `view_mode`.
- `MarkdownElement` → markdown converted to HTML (`# H` → `<h1>`).
- `QueryConsoleElement` → toolbar (query input with `data-lq-action` submit) + content.
- `UISpecElement` → menu bar (`UiAction` attributes) + each `LayoutSpec` (horizontal/vertical/
  grid/tabs/windows) mapped to the expected container markup.

### Integration tests (`liquers-lib/tests/webui_ssr.rs`, native, `#[tokio::test]`)

`type CommandEnvironment = DefaultEnvironment<Value, SimpleUIPayload>;` before `register_command!`.
- `ssr_renders_tree_to_html` (Example 1) — direct construction.
- `ssr_via_lui_query` — register `lui` + a local `content` command, submit a query, run
  `AppRunner`, `render_app_ssr`, assert HTML reflects the tree.
- `ssr_dom_parity` — same tree rendered twice yields identical HTML (determinism / no hidden state).
- `appstate_roundtrip_ssr` — serialize→deserialize `DirectAppState`, assert identical element ids
  in the rendered HTML.

### Browser e2e (`examples-web/ui_spec_demo`, Playwright)

`webui.spec.ts` (Example 2): navigate → menu visible → click "Add Dashboard" → assert a new
`.lq-UISpecElement` appears. This is the **working-example success criterion** and the wasm-runtime
proof. CI: `trunk build` then `playwright test` against `trunk serve` (headless Chromium — already
available in this environment).

### Build-matrix checks (compile-level)

```bash
cargo check -p liquers-lib                                                   # default (egui)
cargo check -p liquers-lib --no-default-features --features webui,image-support   # no egui symbols
cargo check -p liquers-lib --no-default-features --features egui,image-support    # no web symbols
cargo check -p liquers-lib --features egui,webui                             # both
trunk build (examples-web/ui_spec_demo)                                      # wasm32 + tokio-on-wasm
```

### Manual validation
```bash
cargo test -p liquers-lib ui::web            # unit tests
cargo test -p liquers-lib --test webui_ssr   # SSR integration
cd liquers-lib/examples-web/ui_spec_demo && trunk serve --open   # eyeball in a browser
npx playwright test                          # e2e (against trunk serve)
```
**Success criteria:** SSR test asserts expected HTML; the build matrix is green (egui and webui
independent); the Playwright test passes (browser example works end-to-end, proving tokio-on-wasm).

## Query Validation

All queries used in examples/tests are either (a) built by direct element construction (no query),
or (b) the **exact queries the existing native `ui_spec_demo` already runs**
(`dashboard/q/ns-lui/add-child`, `dashboard2/ns-lui/ui_spec/q/add-instead`) — single-line, no
spaces/newlines, using the registered `lui` namespace + the demo's `dashboard`/`dashboard2`
commands. No `-R/` resource queries are used, so no store must be pre-populated. This keeps every
query grounded in code that already compiles and runs.

## Inline Review (Phase 1 / Phase 2 / codebase+query conformity)

- **Phase 1 conformity:** examples cover both modes (SSR native + browser wasm), the framework-
  independent goal (no dioxus; only `web-sys`/strings), and egui/webui independence (build matrix).
  No functionality outside Phase 1 scope.
- **Phase 2 conformity:** examples/tests use the Phase-2 signatures (`render_web(&self, &dyn
  AppState) -> String`, `render_app_ssr`, `mount_web`, `value_to_html`, `UiAction`,
  `dispatch_action`), the string-first model, event delegation, and the capacity-1 wasm
  `AssetManager`. No `WebValueExtension` (removed in Phase 2).
- **Codebase + query validation:** element constructors/fields match the current source
  (`UISpecElement::from_spec`, `MarkdownElement::new`, `DirectAppState::{add_node,set_element}`,
  `app_message_channel`, `register_lui_commands!`); queries are the demo's verified ones; tests
  follow liquers conventions (`Result<(), Box<dyn Error>>`, `#[tokio::test]`, `type
  CommandEnvironment`, typed errors, no `unwrap` in lib).

## Open Items Surfaced (for Phase 4, not blocking)
- Exact `pulldown-cmark` render options (sanitization is moot — we escape and control markup).
- `UISpecElement` `Windows`/`Tabs` web layout fidelity (first cut = stacked panels + tab buttons).
- Confirming tokio `time` on wasm during the first `trunk` run (the one item that can only be
  settled by running Example 2 — captured as the Phase-4 gating step).
