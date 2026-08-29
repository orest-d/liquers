---
id: WORKSPACE-NOT-RUSTFMT-CLEAN
kind: issue
title: The workspace is not rustfmt-clean, so formatting drift hides in every diff
status: draft
priority: P3
complexity: M
area: [build, docs]
design:
created: 2026-08-29
github:
---

## Problem

`cargo fmt --all -- --check` reports deviations in **39 files** across every crate at `HEAD`
(2026-08-29), concentrated in `liquers-core/src/plan.rs` (28), `liquers-core/tests/plan_cwd_freeze.rs`
(16), `liquers-core/src/assets.rs` (10) and `liquers-core/src/interpreter.rs` (9), with the rest
spread over `liquers-store`, `liquers-lib`, `liquers-web`, `liquers-py`, `liquers-axum` and
`liquers-macro`. Nothing in `scripts/` or CI runs the check, so the drift accumulates silently.

Two of the deviations in `liquers-core/src/assets.rs` are not merely cosmetic. At `:4842` and
`:4945` a `///` doc comment sits in statement position inside a method body:

```rust
let _mutation = self.key_mutation_lock.lock().await;

/// Adds non-fatal metadata consistency warnings for externally supplied values.

// 1. Cancel any existing processing asset for this key
```

It documents nothing and `rustc` warns `unused doc comment` on both. The intent was presumably to
document `add_soft_consistency_warnings_enum`, whose own definition may or may not carry it.

## Impact

Low severity, steady cost. A contributor who runs `cargo fmt` on a file they touched produces a diff
containing unrelated reformatting, which either obscures review or has to be reverted by hand — this
issue was filed after doing exactly that while adding `RecipeProviderChoice` to `recipes.rs`, where
two pre-existing deviations at lines 922 and 959 had to be restored to keep the change scoped. The
two unused doc comments add permanent noise to every `cargo check` of `liquers-core`, which makes a
genuinely new warning easier to miss.

The workaround — revert what you did not mean to change — works but is manual and easy to forget.

## Expected behaviour

`cargo fmt --all -- --check` passes at `HEAD`, and stays passing. Routes there, not mutually
exclusive:

1. One mechanical `cargo fmt --all` commit, kept separate from any behavioural change so it can be
   skipped in `git blame` (a `.git-blame-ignore-revs` entry).
2. A check in `scripts/` alongside `check-build-matrix.sh`, so the condition is verifiable
   locally, plus CI once `BUILD-MATRIX-NOT-RUN-IN-CI` is addressed.
3. The two misplaced doc comments in `assets.rs` moved to the item they describe or demoted to `//`,
   which is a behaviour-preserving fix independent of the bulk reformat.

Route 3 is worth doing on its own even if the bulk reformat is declined, since it removes two
compiler warnings rather than moving whitespace.

## Discovery

Found while implementing `RECIPE-PROVIDER-BY-NAME`: `cargo fmt -p liquers-core -- --check` reported
deviations in files the change did not touch, and `rustfmt` on `recipes.rs` reformatted two
pre-existing test assertions that had to be reverted to keep the diff to the new code. The
workspace-wide survey and the `unused doc comment` warnings followed from that.
