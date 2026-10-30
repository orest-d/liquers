# Phase 3: Examples & Use-cases — web-ui-define

The queries were checked with `liquers-validate --command gauge`. `gauge-50` and
`gauge-50/ns-lui/add-child` both return `Ok`.

The tests that use `onEvent` (AC-2's event half, AC-3, AC-8's `onEvent` half) need `ui-events`
(`UpdateMessage::User`, `data-lq-event`). They are written now and enabled in the step after
`ui-events` lands (Phase 4). Every other test runs without it.

## Overview Table

| # | Kind | Name | Shows / checks | Scenarios |
|---|---|---|---|---|
| 1 | Example | A gauge with a button | define, place, event, re-render | AC-1, AC-2, AC-3 |
| 2 | Example | A canvas widget with `mount` | DOM lifecycle | AC-6, AC-8 |
| 3 | Example | A page renderer | headless tree and changes | AC-7 |
| 4 | Test | `liquers-lib/src/ui/widgets/delegated_element.rs` tests (native) | render, fallback, serde, events | AC-1, AC-2, AC-4, AC-5 |
| 5 | Test | `liquers-web/tests/ui_define_UIDEF.rs` (Node loop) | `defineElement`, `element`, refusals, handles | AC-1, AC-2, AC-5, AC-6 |
| 6 | Test | `liquers-web/tests/ui_browser_UIDEF.rs` (`browser-tests`) | events, `mount`, renderer, re-entry | AC-3, AC-6, AC-7, AC-8 |

## Example 1: A gauge with a button

```js
liquers.defineElement({
  type: "gauge",
  render: (p) => `<span class="v">${p.value}%</span><button data-lq-event="inc">+</button>`,
  onEvent: (p, e) => (e.kind === "inc" ? { ...p, value: p.value + 1 } : p),
});
liquers.registerCommand({ name: "gauge", arguments: [{ name: "v", type: "int" }],
                          run: (v) => liquers.element("gauge", { value: v }) });
const ui = await liquers.mountUI(app, { query: "gauge-50" });
// after a tick: <div id="ui-element-1" class="lq-element lq-delegated" data-lq-delegate="gauge">
//                 <span class="v">50%</span><button data-lq-event="inc">+</button></div>
app.querySelector("button").click();
// after a tick: the span reads 51%; only #ui-element-1 was replaced
ui.show("gauge-7");                 // a second gauge; clicking the first leaves this one at 7%
```

## Example 2: A canvas widget with `mount`

```js
liquers.defineElement({
  type: "spark", render: () => `<canvas width="80" height="20"></canvas>`,
  mount: (node, p) => { draw(node.querySelector("canvas"), p.points); return () => stopTimers(); },
});
// mount runs once the node is in the DOM; when the element is replaced or removed, its cleanup
// runs before the next mount
```

## Example 3: A page renderer

```js
const ui = await liquers.mountUI({
  render(tree, changes) {
    if (changes === "all") rebuild(tree); else patch(tree, changes);
  },
}, { query: "-R/docs/readme.md/-/ns-lui/markdown" });
// first call: changes === "all", tree.roots = [1], tree.elements[1].typeName = "MarkdownElement",
// and its html is the element's render_web markup
ui.dispatchEvent(1, "submit");      // a renderer forwards a user event
```

The query in Example 3 needs a store holding `docs/readme.md`. The test uses
`hello/q/ns-lui/query_console` instead, which needs no store.

## Edge and Error Cases

| Symptom | Expected behaviour | Test |
|---|---|---|
| a tree saved, then restored before `defineElement` | placeholder naming `gauge`; correct markup once defined | `delegated_unknown_kind_renders_placeholder`, `uidef05_restored_element_renders_after_define` |
| `render` throws or returns a number | an escaped error box in the element, and an `error` status event | `uidef01b_render_failure_is_contained` |
| `element("gauge", { f: () => 1 })` | `ConversionError` | `uidef02b_non_json_props_refused` |
| `render` calls `ui.show(...)` | `show` throws the busy-lock error; `ui.submit` from `render` works on the next tick | `uidef08_reentry_from_render_is_deferred_or_refused` |
| `onEvent` calls `ui.show(...)` | it succeeds, because the lock is free during `update` | `uidef08b_on_event_may_call_back` |
| `defineElement` twice | existing instances re-render with the new `render` | `uidef01c_redefine_rerenders` |
| `undefineElement`, then dispose | functions released (`live_handle_count` back to baseline) | `uidef06_dispose_releases_callbacks` |

## Test Plan

- Native, in `delegated_element.rs` `mod tests`, with a Rust test delegate:
  - `delegated_renders_through_delegate` (AC-1).
  - `delegated_unknown_kind_renders_placeholder` (AC-5).
  - `delegated_serde_roundtrip_keeps_kind_and_props` (AC-2): `serde_json` round trip of
    `Box<dyn UIElement>`.
  - `delegated_ssr_matches_render_web` (AC-4): `render_app_ssr` and `render_element_web` agree.
  - `delegated_user_event_replaces_props` (AC-2, AC-3, after `ui-events`).
  - Run: `cargo test -p liquers-lib --lib ui::widgets::delegated_element` (also with
    `--no-default-features --features webui`).
- Node loop, `liquers-web/tests/ui_define_UIDEF.rs` (`--features debug-handles`):
  - `uidef01_element_handle_renders_defined_markup` (AC-1): `liquers.element(...).renderHtml()`.
  - `uidef01b_render_failure_is_contained` and `uidef01c_redefine_rerenders` (AC-1).
  - `uidef02_props_roundtrip_through_a_command` (AC-2): a page command returns
    `element("gauge", {value: 3})`, and a second command reads its props back.
  - `uidef02b_non_json_props_refused` (AC-2).
  - `uidef05_restored_element_renders_after_define` (AC-5).
  - `uidef06_dispose_releases_callbacks` (AC-6).
- Browser, `liquers-web/tests/ui_browser_UIDEF.rs` (`--features browser-tests,debug-handles`):
  - `uidef03_event_reaches_only_its_instance` (AC-3): Example 1 with two gauges.
  - `uidef06b_mount_cleanup_runs_on_replace_and_remove` (AC-6): Example 2.
  - `uidef07_renderer_receives_tree_and_changes` (AC-7): Example 3. The first call is `"all"`;
    after `show`, the call carries an `inserted` record, and `dispatchEvent` reaches the element.
  - `uidef08_reentry_from_render_is_deferred_or_refused` and `uidef08b_on_event_may_call_back`
    (AC-8).
- AC-4 is proved natively, by `delegated_ssr_matches_render_web`, and in the browser, by
  `uidef03`. egui is not claimed, so it has no test. `README.md` records `UIDEF04` with that scope.

## Learning Log

- A language-defined element is data (a kind and JSON props) plus behaviour registered per
  thread. Only the data is serialized, which is why a restored tree needs the placeholder path.
- The incremental driver replaces markup with `set_outer_html`. A DOM lifecycle therefore belongs
  after the tick, keyed by node identity, and not in the element trait.
