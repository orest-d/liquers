# Phase 1: High-Level Design

## Design Readiness

- **Readiness:** ready
- **Leading issue:** None
- **Explanation:** A pure refactor. The four existing matches agree on every variant today, and
  one shared function with explicit arms replaces them. Observable behaviour is unchanged.
- **Open questions:** None. Whether `Expired` should be accepted at all is the separate issue
  `SUPPLIED-EXPIRED-STATUS-STORED-WITHOUT-REASON` (design `supplied-expired-status-reason`), which
  builds on this function.

## Problem

`ImmediateAssetManager::set_binary` and `set_state` (`liquers-core/src/assets.rs`) decide the
written status with `_ if self.recipe_opt(key).await?.is_some() => Override, _ => Source` default
arms. `DefaultAssetManager`'s two methods list every variant. CLAUDE.md forbids default arms on
enum matches, and four copies of one rule can drift silently when a `Status` is added.

## Expected behaviour

One private function decides the status for all four call sites, with every `Status` variant
listed:

```rust
fn written_status(supplied: Status, has_recipe: bool) -> Status
```

Acceptance: (1) no `_ =>` / `_ if` arm remains in any `set_binary` / `set_state`; (2) a table test
over all variants and both recipe states pins the mapping; (3) all existing asset tests pass
unchanged.

## Scope

Four call sites. The rule (`Expired` and `Error` kept, everything else becomes `Override` with a
recipe or `Source` without) is unchanged.

## Design Dependencies

- `supplied-expired-status-reason` — **required-by**. It changes the `Expired` row of this
  function. Implement this first, or merge the two (recommended merge candidate).
- `external-manager-replacement-surface` — **required-by** (soft). Its `new_installed` validation
  and the minimal manager reuse the rule.

## Documentation assessment

None. Internal refactor. The source issue gets its resolution.

## Consolidated Findings

- `recipe_opt` is async and fallible, so the function takes `has_recipe: bool` and each caller
  computes it once (`self.recipe_opt(key).await?.is_some()`). Today's default-arm form evaluates
  `recipe_opt` only for non-`Expired`/`Error` inputs. Computing it unconditionally adds one
  recipe lookup for those two rare inputs. Alternatively, keep laziness with a two-step match.
  The plan keeps laziness: `match supplied { Expired | Error => supplied, _other => written_status(..) }`
  is not allowed (default arm). Instead the function returns `Option<Status>`: `Some` when decided
  without the recipe, `None` when it needs it. See Phase 2.
