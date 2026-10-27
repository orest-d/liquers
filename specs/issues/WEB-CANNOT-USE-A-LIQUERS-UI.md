---
id: WEB-CANNOT-USE-A-LIQUERS-UI
kind: feature
title: A page cannot mount, drive or observe a Liquers UI through liquers-web
status: draft
priority: P2
complexity: L
area: [web, lib/ui]
design: web-ui-use
created: 2026-10-27
github:
---
## Problem

`liquers-lib`'s web UI backend runs in the browser (`liquers-lib/src/ui/web/app.rs` `mount_web`,
`AppRunner`, the `lui` commands, `UISpecElement`, `QueryConsoleElement`), but only from a Rust crate
that builds its own environment, as `liquers-lib/examples-web/ui_spec_demo` does. `liquers-web`
exposes none of it:

- `WebEnvironment` is `DefaultEnvironment<Value, ()>` (`liquers-web/src/environment.rs`), and
  `mount_web` / `AppRunner` need `E::Payload: UIPayload + From<SimpleUIPayload>`.
- `register_builtin_commands` (`liquers-web/src/builtins.rs`) does not call
  `register_lui_commands!`, so `ns-lui/...` queries fail with an unknown command.
- An `ExtValue::UIElement` has no JavaScript form (`default_value.rs` `to_js_custom` returns
  `Ok(None)` for it).
- There is no exported function to mount a UI, submit to an element, observe status, or dispose.

The `UIUSE` feature of `guides/LANGUAGE-INTEGRATION_GUIDE.md` is "not started (deferred)" in
`liquers-web/README.md`.

## Impact

A page using liquers-web cannot show the query console, a `ui_spec` dashboard or any Liquers element
without writing and compiling its own Rust crate. Workaround: build a custom wasm crate as the
`examples-web` apps do.

## Expected behaviour

One call mounts a UI into a DOM element from a query or a `UISpec`, returning a handle that submits
queries, reports element status and errors through callbacks, and releases everything on dispose.
See `specs/design/web-ui-use/`.

## Discovery

Starting the UI features of the language integration guide for liquers-web (2026-10-27).
