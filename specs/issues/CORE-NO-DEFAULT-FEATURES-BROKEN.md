---
id: CORE-NO-DEFAULT-FEATURES-BROKEN
kind: issue
title: liquers-core does not build without default features
status: closed
priority: P2
complexity: S
area: [core/store, build]
design: core-no-default-features-compatibility
created: 2026-08-29
github:
---
## Problem

`cargo check -p liquers-core --no-default-features` fails with 14 errors.

`liquers-core/Cargo.toml` gates `futures` and `async-trait` behind the `async_store` feature, which
is in `default`:

```toml
default=["async_store"]
async_store=["futures", "async-trait"]
```

But three modules import them **unconditionally**:

```
liquers-core/src/context.rs:101      use futures::FutureExt;
liquers-core/src/interpreter.rs:5    use futures::FutureExt;
liquers-core/src/store.rs:51         use async_trait::async_trait;
```

So the configuration the feature exists to describe has never compiled. Sample output:

```
error[E0432]: unresolved import `futures`
   --> liquers-core/src/context.rs:101:5
error[E0432]: unresolved import `async_trait`
  --> liquers-core/src/store.rs:51:5
```

## Impact

Nothing is broken for any current consumer: every crate in the workspace takes `liquers-core` with
default features, so `async_store` is always on. The cost is that a declared configuration is a
fiction — a consumer who tries the reduced build hits a wall of errors in files that have nothing to
do with their choice, and neither the feature's name nor its documentation warns them.

It also blocks a planned build-matrix row. `scripts/check-build-matrix.sh` has **no `liquers-core`
rows at all** (see `design/store-factories-in-core/` Phase 4 Step 10), and the obvious set to add
includes `--no-default-features`. That row cannot be added until this is fixed, which is how the
defect was found.

## Expected behaviour

Either configuration should build, or the feature should not exist.

Two directions, and the choice is a design decision rather than a mechanical fix:

1. **Gate the uses.** Put `#[cfg(feature = "async_store")]` on the imports and on everything that
   depends on them. Likely large: `AsyncStore` is declared with `#[async_trait]` and is woven
   through `store.rs`, `context.rs` and `interpreter.rs`, so the reduced build would lose a
   substantial part of the crate's surface. Worth checking whether what remains is coherent.
2. **Remove `async_store` from `liquers-core` and make `futures`/`async-trait` non-optional.**
   Honest if async really is the default and the sync path is not a supported configuration —
   which is what `CLAUDE.md` §"Async Patterns" says ("Default to async … Sync wrappers only for
   Python compatibility"). `liquers-store`, `liquers-axum` and `liquers-py` each declare their own
   `async_store` feature too; those would want the same look.

Direction 2 is the smaller change and matches the documented architecture, but it removes a
declared capability, so it should be confirmed rather than assumed. `tokio_exec` should be reviewed
at the same time — it enables the same two dependencies plus `async_store`, and nothing in the
workspace selects it.

## Discovery

Found while implementing `design/store-factories-in-core/` Phase 4, running the validation command
that design's Step 10 proposes adding to the build matrix. Pre-existing and unrelated to that work:
the three failing imports are in files it does not touch, and it modifies only `Cargo.toml`,
`error.rs`, `lib.rs` and new modules.

## Resolution

Closed on 2026-08-30 by making `liquers-core`'s async store surface unconditional. `futures` and
`async-trait` are normal dependencies, source-level `async_store` gates were removed, and
`async_store` remains only as a no-op compatibility feature for existing Cargo selectors. The build
matrix now includes `liquers-core --no-default-features`.

Evidence:

- `cargo check -p liquers-core --no-default-features`
- `cargo test -p liquers-core --no-default-features`
- `cargo check -p liquers-core`
- `cargo test -p liquers-core`
