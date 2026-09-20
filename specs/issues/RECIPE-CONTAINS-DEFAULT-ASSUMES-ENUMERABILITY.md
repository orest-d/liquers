---
id: RECIPE-CONTAINS-DEFAULT-ASSUMES-ENUMERABILITY
kind: issue
title: AsyncRecipeProvider::contains has a default that silently assumes recipes are enumerable
status: draft
priority: P2
complexity: S
area: [core/assets]
design:
created: 2026-09-20
github:
---
# `AsyncRecipeProvider::contains` has a default that silently assumes recipes are enumerable

## What is wrong

`AsyncRecipeProvider::contains` (`liquers-core/src/recipes.rs:500-514`) has a default implementation
that answers by calling `assets_with_recipes` on the parent directory and searching the result:

```rust
async fn contains(&self, key: &Key, envref: EnvRef<E>) -> Result<bool, Error> {
    if let Some(name) = key.filename() {
        let parent_key = key.parent();
        if self.has_recipes(&parent_key, envref.clone()).await? {
            let recipes = self.assets_with_recipes(&parent_key, envref.clone()).await?;
            return Ok(recipes.iter().any(|resourcename| resourcename == name));
        }
        ...
```

It **enumerates**. That is correct only when everything a provider can produce is also something it
lists — and the two are genuinely different questions, which the trait otherwise keeps apart:

| Method | Question |
|---|---|
| `assets_with_recipes(dir)` | what to **show** in a listing |
| `recipe_opt(key)` / `contains(key)` | what can be **produced** |

A provider that can synthesize a recipe for a key from a *pattern* — but sensibly declines to list
every key the pattern matches — gets `false` from `contains`, with no error and no warning. The
author must somehow know to override a default that looks perfectly reasonable.

## Why it matters

The asset layer already treats these as separate sources: `AssetManager::listdir`
(`assets.rs:4009-4022`) is a **union** of `assets_with_recipes` and `store.listdir`. So "addressable
but not listed" is already expressible — except that the one method answering *addressable* falls
back to *listed*.

Two cases that want it, one already real:

- **Conversion on demand.** `-R/data/table.csv` exists and `-R/data/table.parquet` should be
  producible, without each base name appearing once per supported format in every listing. `contains`
  must confirm it; `assets_with_recipes` must not list it.
- **Record-stream folders.** `specs/design/record-streams/chunking-and-resumability.md` §4a–4b: a
  stream with an unknown chunk count can produce `data_0042.csv` from a template, but cannot
  enumerate its chunks.

A *bounded* generative provider is unaffected and works against the trait as it stands — the
prototype at `orest-d/stockplottertest` (`src/recipes.rs`) generates a finite recipe list per folder
and correctly does not override `contains`. The defect bites only when the generated set is not
enumerable, which is the direction these designs are heading.

## Suggested fix

**Remove the default; make `contains` a required method.** Only `TrivialRecipeProvider` (`:581`) and
`DefaultRecipeProvider` (`:647`) implement the trait, so the cost is two small method bodies —
`Ok(false)` and the current enumerating body respectively. Every future provider author is then made
to decide whether listed and addressable coincide, instead of inheriting an answer.

This is the same failure shape `CLAUDE.md` forbids for `match`: a silent fallthrough where a compile
error is wanted.

## Note on store semantics

Allowing addressable ⊋ listed at the **recipe** layer does not violate `STORE_SEMANTICS.md` §2, which
constrains `contains`/`is_dir`/`listdir` on **stores**. The asset key space is legitimately larger
than the store key space — that is what recipes are. The relaxation should be stated in the asset and
recipe reference so it is not later "fixed" as an inconsistency.
