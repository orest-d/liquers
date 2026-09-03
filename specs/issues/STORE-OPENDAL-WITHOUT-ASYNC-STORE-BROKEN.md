---
id: STORE-OPENDAL-WITHOUT-ASYNC-STORE-BROKEN
kind: issue
title: liquers-store's `opendal` feature does not compile without `async_store`
status: closed
priority: P3
complexity: S
area: [store/backends]
design: opendal-feature-without-async-store
created: 2026-08-29
github:
---
## Problem

`liquers-store`'s two features are independent in the manifest but not in the source.
`store_factory.rs` imports the backend under the `opendal` feature alone:

```rust
#[cfg(feature = "opendal")]
use crate::opendal_store::AsyncOpenDALStore;
```

while the type it names is gated on the *other* feature (`opendal_store.rs:220`):

```rust
#[cfg(feature = "async_store")]
pub struct AsyncOpenDALStore { … }
```

So `cargo check -p liquers-store --no-default-features --features opendal` fails:

```
error[E0432]: unresolved import `crate::opendal_store::AsyncOpenDALStore`
note: found an item that was configured out
      the item is gated behind the `async_store` feature
```

Confirmed against `35bba67` (before `STORE-OPENDAL-SERVICES-NOT-ENABLED` was fixed), so this is
not a regression from the service-feature work.

## Impact

Low, and the reason is worth stating: the broken configuration is one nobody has a reason to
select. `opendal` without `async_store` gives a crate whose only contribution is backends it
cannot expose, and `default` enables both. No in-tree caller, no example and no documented
configuration selects it.

What it costs is a build-matrix row. `scripts/check-build-matrix.sh` has to spell the
service-less configuration `--no-default-features --features async_store,opendal` and explain
why, rather than exercising the `opendal` feature on its own. A reader who takes the two features
at face value writes the shorter form and gets a confusing error.

## Expected behaviour

Either configuration should compile, or the manifest should say they are not independent. Two
ways, and the choice is a judgement about what the crate promises:

1. **Make the source honest** — gate the import and its uses on
   `#[cfg(all(feature = "opendal", feature = "async_store"))]`, leaving a build with `opendal`
   alone that compiles and offers no OpenDAL store type. Truthful, and the availability
   reporting in `store_factory.rs` already has the vocabulary for it.
2. **Make the manifest honest** — `opendal = ["dep:opendal", "async_store"]`, so selecting
   OpenDAL selects the machinery that exposes it. One line, and it removes the configuration
   rather than fixing it.

(2) is smaller and matches how the crate is actually used; (1) preserves the ability to build a
sync-only OpenDAL configuration, which nothing needs today. `CLAUDE.md` says stores are async
only, which argues for (2).

## Discovery

Found while fixing `STORE-OPENDAL-SERVICES-NOT-ENABLED`, when adding a build-matrix row for
"OpenDAL linked with no service features" — the state that issue was about. The natural spelling
of that row, `--no-default-features --features opendal`, failed to compile for this unrelated
reason and had to be written `--no-default-features --features async_store,opendal` with a
comment pointing here.

## Resolution, 2026-09-02

Option 1 taken, as the issue proposed: the `AsyncOpenDALStore` import in `store_factory.rs` and the
`create()` branch that uses it are gated on `all(feature = "opendal", feature = "async_store")`, and
the fallback branch reports every type unavailable — truthful for a build that links OpenDAL but can
expose no store. `cargo check -p liquers-store --no-default-features --features opendal` builds.

Folded into [`design/opendal-path-mapping/`](../design/opendal-path-mapping/), which was already
editing the file.
