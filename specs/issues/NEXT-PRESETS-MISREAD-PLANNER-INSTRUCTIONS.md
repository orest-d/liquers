---
id: NEXT-PRESETS-MISREAD-PLANNER-INSTRUCTIONS
kind: issue
title: find_next_presets reads planner instructions differently from the planner
status: draft
priority: P3
complexity: S
area: [lib/commands]
design: next-presets-follow-planner-rules
created: 2026-10-20
github:
---
## Problem

`find_next_presets` (`liquers-lib/src/utils.rs`) finds the query's last command with its own reading
of the planner instructions, which differs from `PlanBuilder` (`RECIPES_AND_PLANS.md` §Planner
instructions) in four ways:

1. it skips `ns` and `q` but not `v`, so a query ending in `v` has no presets;
2. it flattens the parameters of every `ns` in the segment and keeps only the last one, so
   `ns-a-b/cmd` looks in `b` only, while the planner searches `a` then `b`;
3. it uses the segment's last `ns` even when that comes after the command (`ns-a/cmd/ns-b`), while
   the planner uses only an `ns` before the action;
4. it always looks in realm `""`, while the planner uses the segment header's realm (`-r/cmd`).

## Problem Example

`uppercase` declares a `next` preset `lowercase`. `find_next_presets("text-Hello/uppercase/v", …)`
returns nothing; it should return `text-Hello/uppercase/v/lowercase`. Likewise `ns-pl-img/head`
with `head` in `pl` returns nothing.

## Impact

Next-command presets are missing in the query console after a trailing `v`, a multi-namespace `ns`,
a trailing `ns`, or in a realm segment, and the `ns-` injection decision is made against the wrong
namespace list.

## Expected behaviour

Resolve the last command as the planner does: skip `ns`, `q` and `v`; use the namespaces of the
last `ns` before it, then the defaults; use the segment's realm. The namespace list for the
appended preset is the whole segment's last `ns`, as `plan::append_action` computes. A shared core
helper is `PLANNER-INSTRUCTION-NAMES-DUPLICATED`.

## Discovery

Found while reviewing `reference/api/DOC_08_RECIPES_PLANS.md` against the code at `96279f14` before promoting it to [`reference/RECIPES_AND_PLANS.md`](../reference/RECIPES_AND_PLANS.md) (2026-10-20).
