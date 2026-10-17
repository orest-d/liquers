---
id: RUNTIME-VOLATILE-DEPENDENCY-DOES-NOT-MAKE-PARENT-VOLATILE
kind: issue
title: A volatile dependency a command evaluates through its context does not make the command's result volatile
status: draft
priority: P2
complexity: M
area: [core/context]
design: 
created: 2026-10-17
github:
---
## Problem

`liquers-core/src/assets.rs` (module docs, §Volatility) and `DOC_03_ASSETS_EXECUTION_LIFECYCLE.md`
say volatility is contagious and that a volatile dependency makes an evaluation volatile. That
holds only for what the planner sees: `rnd/echo` with `rnd` volatile is `Volatile`. A dependency a
command evaluates at run time through its `Context` (`get_dependency_state`, `evaluate`, `submit`
+ `wait_for_dependency`, `apply`, `apply_ad_hoc`) passes nothing on. Probe, 2026-10-17, with `count`
a volatile command returning `n0`, `n1`, … and `depend_on(q)` a command calling
`get_dependency_state(q)`:

```
depend_on-count     Ready     n0
depend_on-count     Ready     n0      <- the volatile value, served from cache
depend_on-count/v   Volatile  n0      <- trailing v serves the cached prefix (DOC_08, positional v)
v/depend_on-count   Volatile  n1      <- only a leading v recomputes
```

Also `Context::is_volatile()` is `false` inside `eval_child-plain/v`, because the command runs in
the cut, non-volatile prefix; `Context::with_volatile` is never called.

`design/volatility-system` decision 4 fixed volatility before evaluation, which is why a run-time
discovery has nowhere to go. Some run-time change already exists: a command that sets an immediate
expiry during evaluation gets its asset evicted (`runtime_volatile_asset_is_evicted_from_key_map`).

## Impact

Any command that evaluates queries it builds (the `template/*` commands, `ns-search`, agent-memory
context packs) caches a result computed from volatile data, and stores it when it is keyed. The
caller has to know to declare the command `volatile` or start the query with `v/`.

## Expected behaviour

When a context's dependency is volatile, the asset being evaluated finishes `Volatile`: not
registered for reuse, not stored, evicted if it already was. To decide: whether this revises
volatility-system decision 4 or is a separate "volatile at finish" path like the runtime expiry one.

## Discovery

design/liquers-mustache D6 (AC-2 "volatility inherited"), verified with a probe test against
`liquers-core/tests/apply_ad_hoc.rs`'s commands.
