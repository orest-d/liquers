---
id: QUERY-CANNOT-MARK-CACHED-INTERMEDIATES
kind: feature
title: A query cannot mark which of its intermediates are worth caching
status: draft
priority: P3
complexity: M
area: [core/plan, core/assets, core/query]
design: 
created: 2026-10-10
github:
---
## Problem

**Example.** `-R/data/big.parquet/-/from_parquet/t1/t2/…/tn/summary`, with a 100 MB frame flowing
through every step. Every cut boundary is evaluated as its own query asset and cut again, so every
prefix becomes a cached asset (measured in `plan-policy`, `DESIGN.md` notes). That is one copy of
the frame per step. A query author cannot say "cache the parsed frame and the result, nothing in
between" inside the query. The only instruments are a command's declaration and a recipe's
`cached` policy, both added by `plan-policy`.

## Impact

Memory, for long chains over large values. The workaround is to split the query into recipes: a
reusable prefix recipe that is cached, and derived recipes with intermediates not cached. That
works, but it costs one recipe per reusable point. P3, deferred by the maintainer until it becomes
important.

## Expected behaviour

A positional switch, assessed in the `plan-policy` discussion on 2026-10-10. One reserved
directive, `cache-on` / `cache-off` / `cache-this`, holds from its position like positional `v`.
The cached points are the predecessor boundaries. Decisions already worked out:

- **Retention directives must not be part of asset identity.** Otherwise `x/y/cache-this` and
  `x/y` are different assets, and a marked prefix is shared only with consumers that spell the
  switches identically, which defeats the purpose. The asset key is the query with the directives
  removed, and the boundary step keeps the full text. When two queries reach one prefix with
  different switches they share one asset, which is sound because the values are identical. This
  normalisation is the main cost.
- Open questions: does the final result follow the mode in force at the end? Does `cache-this`
  inside a volatile region warn? Links do not inherit the mode.
- Rejected in that discussion: per-value `cached-<bool>` directives (one directive per step on a
  long chain), `inline` and `qinline` command flags.

## Discovery

Raised by the maintainer in design `plan-policy`, Phase 1 discussion, 2026-10-10. Deferred
there in favour of the recipe-level policy and the command-level `cached` declaration.
