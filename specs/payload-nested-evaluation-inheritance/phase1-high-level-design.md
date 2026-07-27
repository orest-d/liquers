# Phase 1: High-Level Design - payload-nested-evaluation-inheritance

## Feature Name

Payload Inheritance in Nested Evaluation (resolves ISSUES.md: PAYLOAD-NESTED-EVALUATION-INHERITANCE)

## Purpose

`specs/PAYLOAD_GUIDE.md` and `specs/PROJECT_OVERVIEW.md` promise that nested evaluations inherit
the parent's payload, but `Context::evaluate` / `get_dependency_state` / `apply` schedule through the
`AssetManager` without forwarding `Context::payload` (proven by
`test_payload_not_inherited_in_nested_evaluation`). This feature **implements inheritance** with a
cache-first rule, so the documentation becomes true.

## Chosen Decision (Option 1: Implement Inheritance, cache-first)

The authoritative boundary:

1. **Cache wins.** If nested evaluation can resolve the query to an existing asset through the asset
   manager, that asset is used and the parent payload has **no effect**. Shared assets are never
   re-evaluated per payload, so cached results never become payload-dependent.
2. **Otherwise evaluate with payload.** If no asset is available, evaluation proceeds with the parent's
   payload inherited — which requires the *immediate* (ad-hoc, uncached, non-persisted) path, since
   that is the only path carrying a payload today.
3. **Safety is the command author's responsibility, not enforced.** A command whose result genuinely
   varies with payload should be labeled `volatile`. The framework does not police this.

**Efficiency requirement.** Choosing between the manager path and the immediate path must not require
speculatively evaluating both. The plan must therefore declare whether it needs a payload to run:
`requires_payload` is computed at plan-build time (exactly like the existing two-phase
`Plan::is_volatile` detection, `plan.rs:1363-1366`) and drives the switch. Payload-free plans keep
today's cached, shared, queued behavior with zero overhead.

## Core Interactions

### Query System
No parse/Key-encoding change. Relevant only because a query carries no payload identity — which is
precisely why rule 1 (cache wins) is safe: a cached asset keyed by query is never payload-specific.

### Command System
No new commands. **Command metadata must distinguish payload-derived injection from
environment-service injection**: `ArgumentInfo::injected` is currently one bool
(`command_metadata.rs:387-391`) covering both. Only payload-derived injected arguments make a plan
require a payload. `PayloadType` / `ExtractFromPayload` (`commands.rs:337-353`) are the existing
markers that Phase 2 can build the distinction on.

### Asset System
The heart of the change. `Context::schedule_dependency_asset` → `AssetManager::get_dependency_asset`
→ `get_asset` (`assets.rs:2683-2690`) is the cached path; `apply_immediately`
(`assets.rs:2649-2654`) is the payload-bearing ad-hoc path. Nested evaluation gains a decision point
between them, driven by `Plan::requires_payload` plus asset availability. Dependency recording,
cycle checking, and volatility propagation must keep working on whichever branch is taken.

### Store System / Value Types / Web / UI
Not touched. `liquers-axum` benefits indirectly (request-scoped payload reaches nested commands).

## Crate Placement

**liquers-core**: `src/plan.rs` (`requires_payload`), `src/command_metadata.rs` (payload-injection
flag), `src/context.rs` (nested-evaluation switch), `src/assets.rs` (payload-bearing dependency path).
Possibly `liquers-macro` if the `injected` DSL needs to mark payload arguments. Docs to update:
`specs/PAYLOAD_GUIDE.md`, `specs/PROJECT_OVERVIEW.md`, `liquers_core::context` rustdoc, and the
existing non-inheritance test (which becomes an inheritance test).

## Open Questions

1. How is a payload-derived injected argument distinguished from an environment-injected one in
   `ArgumentInfo` — a new field, or inferred from the type implementing `PayloadType`/`ExtractFromPayload`?
   Type-based inference is not available at metadata level, so this likely needs an explicit flag set
   at registration. **See Design Review below**: argument-level inference alone is unsound because
   `context`-accessing commands declare no payload argument; an explicit
   `payload: required|optional|none` metadata key is needed.
2. "Cache wins" needs an exact predicate: is it "asset exists and is in a usable (non-`Expired`,
   non-`Error`) status", and is it checked without scheduling? Interaction with the
   ASSET-EXPIRED-CACHED-BINARY-READ issue should be checked.
3. Dependency semantics on the immediate branch: an ad-hoc payload-evaluated child is not a cacheable
   graph node — is it still recorded as a dependency, or recorded as untracked/volatile?
4. Does a payload-requiring nested evaluation force the parent to be treated as volatile, or is that
   left entirely to the author's `volatile` label as stated in rule 3?
5. Does `Context::apply` (ad hoc, no dependency recorded) also inherit payload, or does it keep its
   current non-forwarding behavior?
6. What happens when a plan requires a payload but the parent context has none (e.g. background
   evaluation)? Presumably the existing `InjectedFromContext` error, but at which point is it raised?

## Design Review: Payload Purpose and Metadata Declaration

### What payload is for, and the `&mut egui::Ui` limit

Payload's purpose is to carry **complex, non-serializable, process-local resources** — graphics
contexts, DB connections, hardware handles — that cannot be expressed in a query. The current
constraint is `PayloadType: Clone + MaybeSend + MaybeSync + 'static` (`commands.rs:343-346`), which
on native resolves to `Clone + Send + Sync + 'static`.

`&mut egui::Ui` cannot pass through payload, and **`Send` is not the binding reason**. Three
constraints fail independently, two of them fatally:

1. **`'static`** — `&mut egui::Ui<'_>` is a borrow whose lifetime is tied to the frame closure.
   No relaxation of `Send`/`Sync` fixes this. Fatal.
2. **`Clone`** — payload is cloned into each action's context; a unique `&mut` borrow is not `Clone`.
   Fatal.
3. **`Send`/`Sync`** — only binding on native, and moot given 1 and 2.

So this is not a bug to fix by loosening bounds; an immediate-mode frame borrow is structurally
incompatible with a cloneable `'static` payload. **The codebase already solves this correctly**:
`UIContext` (`ui/ui_context.rs`) carries only `Arc<Mutex<dyn AppState>>` + `AppMessageSender` +
`UIHandle` — all `Clone + Send + Sync + 'static` — and commands act by **mutating the retained
element tree and sending messages**, while `&mut egui::Ui` is passed as a *direct parameter* down
the render path (`Element::show_in_egui(&mut self, ui: &mut egui::Ui, ctx: &UIContext, ...)`,
`ui/element.rs:122-127`). Payload carries the *durable handle*; the frame borrow stays on the stack.

Worth noting for future UI work: `egui::Context` (unlike `Ui`) **is** `Clone + Send + Sync + 'static`
and *could* legitimately live in a payload — it grants repaint requests, fonts, textures, and input
state, just not positional immediate-mode drawing.

### Verdict on the metadata conclusion

**The conclusion is correct and necessary** — with three refinements.

**Why it is necessary.** The cache-first switch must choose the manager path or the immediate path
*before* evaluating, so "does this plan need a payload" has to be statically derivable. Command
metadata is the only serializable, pre-execution source, and `Plan::is_volatile` is the exact
precedent. Further, `ArgumentInfo::injected` is a **single bool conflating payload injection with
environment-service injection** (`command_metadata.rs:387-391`); without separating them, every
env-injected argument would force the payload path — badly over-conservative.

**Refinement 1 — the axis is *availability*, not *dependence*.** Two questions could plausibly live
in metadata: (a) can this run without a payload? (b) does its result depend on the payload? Only (a)
belongs here. Question (b) is already answered by the existing `volatile` flag, and per the chosen
rule 3 it is deliberately the author's responsibility, not enforced. Keeping metadata to (a) avoids
duplicating `volatile`.

**Refinement 2 — `context`-accessing commands are invisible to metadata (the real hole).**
PAYLOAD_GUIDE Pattern 3 (`fn cmd(state, context)` calling `get_payload_clone()`) declares no
argument at all — `register_command!(cr, fn get_context_data(state, context) -> result)` gives
metadata nothing to infer from. Argument-derived inference alone is therefore **unsound**. This needs
an explicit metadata attribute alongside the existing `volatile:` / `label:` keys, e.g.
`payload: required | optional | none`, defaulting to the argument-inferred value.

**Refinement 3 — `optional` needs an explicit tie-break, or it is not worth a third state.** Note
that under cache-first, even `required` does not guarantee a payload: a cached asset always wins.
The three states only govern what happens on a **cache miss**:

| State | Cache hit | Cache miss |
|---|---|---|
| `none` | use asset | manager path (cached, shared) |
| `required` | use asset | immediate path with inherited payload |
| `optional` | use asset | **ambiguous — must be decided** |

For `optional`, going immediate is more faithful to inheritance; going through the manager preserves
sharing and caching. Proposed rule: **treat `optional` as `required` when the parent context actually
has a payload, and as `none` when it does not** — never fails, maximizes fidelity, and costs
cacheability only in the case where a payload genuinely exists to inherit. If this rule is rejected,
`optional` collapses into one of the other two and a plain `requires_payload: bool` suffices.

## References

- `specs/ISSUES.md` — Issue: PAYLOAD-NESTED-EVALUATION-INHERITANCE
- `specs/PAYLOAD_GUIDE.md`, `specs/PROJECT_OVERVIEW.md` (claims this change makes true)
- `liquers-core/src/context.rs:76-80, 448-474` (payload doc + nested-evaluation methods)
- `liquers-core/src/assets.rs:2630-2714` (`AssetManager`: `get_dependency_asset`, `apply`, `apply_immediately`)
- `liquers-core/src/plan.rs:1353-1380, 1424-1427` (`Plan::is_volatile` — precedent for `requires_payload`)
- `liquers-core/src/command_metadata.rs:387-391` (`ArgumentInfo::injected`)
- `liquers-core/src/commands.rs:337-353` (`PayloadType`, `ExtractFromPayload`)
- `liquers-core/tests/injection.rs::test_payload_not_inherited_in_nested_evaluation`
