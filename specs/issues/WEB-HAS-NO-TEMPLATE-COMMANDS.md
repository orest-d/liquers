---
id: WEB-HAS-NO-TEMPLATE-COMMANDS
kind: feature
title: The browser environment has no template commands
status: draft
priority: P3
complexity: S
area: [web]
design: 
created: 2026-10-25
github:
---
## Problem

`liquers-web` depends on `liquers-lib` with `default-features = false, features = ["webui"]`
(`liquers-web/Cargo.toml`), without `templates`, and `register_builtin_commands`
(`liquers-web/src/builtins.rs`) does not call `register_template_commands!`. The browser
environment therefore has no `ns-template/render`, `expand` or `validate`, and none of the
`TEMPLATE` feature of `guides/LANGUAGE-INTEGRATION_GUIDE.md` is reachable from a page.

The in-command half of `TEMPLATE` additionally needs a JavaScript-visible command context, which
`liquers-web` does not pass yet (`COMMAND07` is scoped to the Rust side,
`liquers-web/tests/commands_COMMAND.rs`).

## Problem Example

```javascript
await liquers.evaluate("ns-template/render-~X~-R/templates/hello.md~E");  // valid natively
// today:    rejects — the browser registry has no `template` namespace
// expected: the rendered text, as from a native liquers-lib environment
```

Re-checked at HEAD (2026-10-26): `liquers-template` already builds for wasm32 —
`scripts/check-build-matrix.sh` checks `liquers-lib` with `--target wasm32-unknown-unknown
--features webui,templates` — so the build question in the expected behaviour is answered. What
remains is a `templates` feature on `liquers-web`, the registration call, and the `TEMPLATE`
dispositions: one crate, re-sized `M` → `S`.

## Impact

A page cannot use templates at all, even through a query. No workaround short of rendering in
JavaScript, which loses query tags and dependency tracking.

## Expected behaviour

Decide whether `templates` is default or opt-in for `liquers-web` (as `records` is opt-in, for
artifact size), check that `liquers-template` builds for `wasm32-unknown-unknown`, register the
commands in the built-in set, and add the `TEMPLATE` dispositions to the web conformance record.
Programmatic `render` / `expand` from JavaScript and a JavaScript-object `DataNode` adapter follow
as `TEMPLATE` and `DATANODE` work.

## Discovery

Writing the `TEMPLATE` section of the language integration guide (2026-10-25).
