---
id: WORKSPACE-SERDE-DERIVE-UNDECLARED
kind: issue
title: Three crates use serde derive macros without declaring the `derive` feature
status: accepted
priority: P2
complexity: L
area: [build, core/value, lib/ui, axum]
design:
created: 2026-08-09
github:
---

## Problem

`liquers-core`, `liquers-lib` and `liquers-axum` declare `serde = "1.0.181"` — without
`features = ["derive"]` — and then import the derive macros from it:

| Crate | Declaration | Example use |
|---|---|---|
| `liquers-core` | `serde = "1.0.181"` | `liquers-core/src/metadata.rs` |
| `liquers-lib` | `serde = "1.0.181"` | `liquers-lib/src/ui/action.rs` |
| `liquers-axum` | `serde = "1.0.181"` | `liquers-axum/src/assets/websocket.rs` |

`use serde::{Serialize, Deserialize}` as *derive macros* requires `serde/derive`. These crates
compile only because some other crate in the dependency graph enables that feature and Cargo
unifies it across the build. Nothing declares it, so nothing guarantees it.

`liquers-web` is the one crate that gets it right
(`serde = { version = "1.0", features = ["derive"] }`).

## Impact

The build works today and breaks the moment the graph changes — specifically, whenever a
dependency that happened to supply the feature becomes optional or is dropped. That is not
hypothetical: it is exactly what happened to `liquers-store` on 2026-08-09, when making `opendal`
optional produced 13 errors of the form *"cannot find derive macro `Serialize` in this scope"* in
`config.rs`, a file that had not been touched. `liquers-store` is now fixed and carries a comment
explaining why.

The failure is confusing out of proportion to its size: the errors appear in a file unrelated to
the change, and name a macro that is plainly imported at the top of it.

Low-ish priority because it is latent, not active — but the fix is three lines, and each of the
three crates is one optional-dependency change away from the same afternoon.

## Expected behaviour

Each crate that imports serde's derive macros declares them:

```toml
serde = { version = "1.0.181", features = ["derive"] }
```

Worth checking at the same time whether the separate `serde_derive` dependency these crates also
carry is used at all; if the macros are always imported through `serde`, `serde_derive` is dead
weight and can go.

## Discovery

Found on 2026-08-09 while implementing M1 Step 1 of `specs/design/liquers-web-store/` — making
`opendal` optional in `liquers-store` immediately surfaced it there. The other three crates were
then checked and have the same declaration gap; they were left alone because fixing them was
outside that milestone's scope.
