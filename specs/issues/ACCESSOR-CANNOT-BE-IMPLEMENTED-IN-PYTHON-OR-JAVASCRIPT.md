---
id: ACCESSOR-CANNOT-BE-IMPLEMENTED-IN-PYTHON-OR-JAVASCRIPT
kind: feature
title: A new accessor root or projection step cannot be implemented in Python or JavaScript
status: draft
priority: P3
complexity: L
area: [py, web, core/value]
design:
created: 2026-11-16
github:
---
## Problem

`settable-value-accessor` gives Python and JavaScript a wrapper over existing accessors: a script
can get and set a value, project it, read its status, progress and messages, and react to its
events (`lq.Accessor`, the JavaScript `Accessor`). It cannot make a *new kind* of accessor. The
two extension points are Rust traits in `liquers-core/src/accessor/`:

- `AccessorRoot<V>`: `read`, `write`, `info`, `log`, `capabilities`, `poll_event`, `watch`;
- `ProjectionStep<V>`: `get`, `put`, `anchor`, `describe`.

Nothing adapts a Python object or a JavaScript object to either trait. So a value that lives in the
language (a Python object model, a browser form, a JavaScript store), or a part of a value only the
language knows how to address, cannot be edited through accessors. Orest asked for this to be
filed separately while scoping the language support of `settable-value-accessor` (2026-11-16).

## Impact

Applications written in Python or JavaScript can only use the first-class accessors (store, keyed
asset, other assets, memory) and the Rust projection steps. A custom source needs Rust, which
defeats the purpose of the integration for these users. The workaround is a memory accessor that
the script keeps in step with its own source by hand.

## Expected behaviour

A language adapter for each trait, as `LANGUAGE-INTEGRATION_GUIDE.md` describes for stores and
recipe providers (`STORE`, `RECIPE`), under the guide's `ACCESSOR` feature, direction 2. The points
the design must settle:

- **Root adapter.** `read` and `write` call language methods, which may be async (`ASYNCCMD`
  rules). `info` and `log` default to the trait defaults. Notifications: a language callback that
  calls into the hub (`notify_changed`, `notify_status`) instead of `poll_event`.
- **Step adapter.** `get`, `put`, `anchor` and `describe` are synchronous in the trait. A language
  step must therefore be synchronous too, or the step contract must gain an async form (Phase 2 of
  `settable-value-accessor` rejected async steps for cost).
- **Threads.** On native Python, the hub calls a root from a tokio task: the adapter must take the
  GIL there, as the `STORE` adapter does. In the browser everything runs on one thread.
- **Naming.** A language-defined accessor that should be addressable by a query needs a command
  that builds it, registered from the language (`COMMAND`).
