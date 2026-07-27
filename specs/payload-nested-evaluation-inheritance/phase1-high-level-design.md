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
`liquers-macro` for the `payload` keyword and the unknown-trailing-ident fix (D2), plus migration of
existing `injected` payload sites in `liquers-lib/src/ui/commands.rs`. Docs to update:
`specs/PAYLOAD_GUIDE.md`, `specs/PROJECT_OVERVIEW.md`, `liquers_core::context` rustdoc, and the
existing non-inheritance test (which becomes an inheritance test).

## Open Questions

Resolved and moved to **Resolved Decisions** below: payload-derived vs environment-derived
injection (D2), dependency semantics of the payload branch (D1), parent volatility (D1/D3), and
the `optional` tie-break (D3).

1. "Cache wins" needs an exact predicate: is it "asset exists and is in a usable (non-`Expired`,
   non-`Error`) status", and is it checked without scheduling? Interaction with the
   ASSET-EXPIRED-CACHED-BINARY-READ issue should be checked.
2. Does `Context::apply` (ad hoc, no dependency recorded) also inherit payload, or does it keep its
   current non-forwarding behavior? D1 makes the payload branch semantically equivalent to
   `apply_immediately`, which argues for `apply` inheriting too — but this needs an explicit call.
3. What guard replaces graph-based cycle detection on the payload branch (D1) — recursion-depth
   limit or an active-query set on `Context`?
4. Does the `payload` keyword (D2) need `payload optional` grammar, or does required-vs-optional
   stay exclusively on the command-level statement?
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

## Resolved Decisions

### D1. Payload-evaluated assets are never a dependency

An asset evaluated on the payload branch is never recorded as a dependency of its parent.
This aligns it exactly with `Context::apply`, which already "does not record the result as a
dependency" (`context.rs:467-470`) — so the payload branch is semantically `apply_immediately`,
a path that is already ad hoc, uncached, non-persisted, and dependency-free. No new category
of asset is introduced.

Concretely, the payload branch skips everything `schedule_dependency_asset` does
(`context.rs:369-425`): `register_scheduled_dependency`, `get_dependency_asset`,
`add_dependent_asset`, and `add_dependency`.

Two consequences must be handled in Phase 2:

- **Cycle detection is lost on this branch.** Cycle checking currently lives in
  `register_scheduled_dependency`. A payload-evaluated chain (A→B→A) has no graph edge to
  detect. A separate guard is required — a recursion-depth limit or an active-query set carried
  on `Context`.
- **A parent's recorded dependencies become incomplete.** A keyed, cacheable asset that performs
  payload-evaluated nested calls has untracked inputs. Per rule 3 this is the author's
  responsibility (mark `volatile`) and is *not* enforced — but it should be **visible**: plan
  building can emit a `Step::Warning` into `init_steps`, which already exists for exactly this
  kind of non-blocking diagnostic (`plan.rs:1356-1359`).

### D2. `register_command!` gains a `payload` keyword — as a complement, not a replacement

Recommended: **yes**, with two guards. The keyword slots into the existing optional trailing-ident
position (`registration.rs:1531-1536`), so `fn cmd(state, ui: UiCtx payload)` parses beside
`injected` with a contained macro change.

**Pros**

1. Distinguishes payload-derived from environment-derived injection at the declaration site —
   the exact gap that makes `ArgumentInfo::injected` insufficient — letting metadata set a
   `from_payload` flag and making plan-level `requires_payload` derivable.
2. Zero added ceremony: the author writes `payload` where they would have written `injected`.
3. Self-documenting signatures; today the two kinds are indistinguishable when reading code.
4. Enables a precise error ("command `x` requires payload via argument `ui`") instead of a
   generic `InjectedFromContext` failure.

**Cons**

1. **It does not close the `context`-accessing hole.** `fn cmd(state, context)` calling
   `get_payload_clone()` still declares nothing. The command-level
   `payload: required|optional|none` statement is still required. The keyword is complementary.
2. **Two spellings for adjacent concepts** (`injected` vs `payload`) — payload injection *is* a
   special case of context injection. Needs clear documentation to avoid author confusion.
3. **Declaration can diverge from implementation**: nothing stops `payload` on a type that never
   reads payload, or `injected` on one that does, yielding wrong metadata and mis-routed
   evaluation. *Mitigation:* codegen a bound assertion against `ExtractFromPayload`/`PayloadType`
   so a mismatch is a compile error. This turns the weakest con into an enforced invariant and
   should be treated as part of the feature, not optional.
4. **A bare keyword cannot express three states**; required-vs-optional still comes from the
   command-level statement, or needs `payload optional` grammar.
5. **Silent migration hazard (largest risk).** Existing payload commands written with `injected`
   — all of `liquers-lib/src/ui/commands.rs`, `tests/injection.rs`, and every PAYLOAD_GUIDE
   example — would keep compiling but produce `requires_payload = false`, routing them to the
   manager path and silently losing payload in nested position. Phase 2 must include an audit and
   migration of these sites; the call sites are few enough that this is tractable.

**Additional fix to fold in:** the current parser consumes an unknown trailing ident and silently
treats it as not-injected (`flag == "injected"` → `false`). Adding a second recognized keyword is
the right moment to reject unknown idents, so `payloadx` becomes a compile error rather than a
silent no-op.

### D3. Three-state semantics confirmed

| State | No payload available | Payload available, cache hit | Payload available, cache miss |
|---|---|---|---|
| `none` | manager path, cached | use cached asset | manager path, cached |
| `optional` | runs, manager path, cached | use cached asset | immediate path with payload, uncached, not a dependency |
| `required` | **refuses to run** | use cached asset | immediate path with payload, uncached, not a dependency |

`required` refuses to run without a payload; `optional` runs without one. Because
`requires_payload` is known at plan-build time, a `required` command reached with no payload can
fail fast at planning rather than deep inside execution.

**Consequence that must be documented:** under cache-first, a cache hit means the command does not
run at all, so *even a `required` command's payload is bypassed on a hit*. For `optional` commands
this makes payload delivery **order-dependent** — whichever caller arrives first determines whether
a cached entry exists. This is not a correctness bug given rule 1, but it means: **any command whose
cacheable output genuinely varies with payload must be marked `volatile`.** That is precisely rule 3,
and cache-first makes it effectively mandatory rather than advisory.

### D4. Scoped (unconstrained) payload — future axis, verified compatible, not built

A second payload type with no `Clone + Send + Sync + 'static` bound, passed as `&mut` and requiring
same-thread inline execution.

**This is a distinct third axis, not the availability axis.** The design now has three orthogonal
questions:

| Axis | Question | Where answered |
|---|---|---|
| Availability | Does the plan need a payload to run? | `payload: required/optional/none` |
| Dependence | Does the output vary with payload? | existing `volatile` |
| **Mobility** | Can the payload cross task/thread/time boundaries? | *implicit today* |

Mobility is **already a real axis in this codebase**: `PayloadType: Clone + MaybeSend + MaybeSync +
'static` resolves to `Clone + Send + Sync + 'static` on native but only `Clone + 'static` on wasm32
(`maybe_send.rs:25-39`). A scoped payload would be a third level (drop `Clone` and `'static`).

**Compatibility verification — the current design survives adding this later:**

- `Plan::requires_payload` — survives, and becomes *more* necessary: an immobile payload must force
  inline same-thread execution.
- Cache-first rule — survives; a scoped payload only narrows the miss branch from "immediate" to
  "immediate, same-thread, non-escaping".
- **D1 (never a dependency)** — survives and becomes non-negotiable: a scoped result cannot outlive
  the frame, so it can never be stored or shared.
- Three-state metadata — survives, but with two payload types the key may need to name which one.
  The chosen small-enum representation leaves room; a hardcoded single bool would not. This is an
  argument for the enum over `requires_payload: bool`.
- `AssetManager::apply_immediately(recipe, to, payload: Option<E::Payload>)` takes an owned payload
  (`assets.rs:2649-2654`) and could not carry a scoped one — a separate entry point would be needed.

**Implementation options, if ever pursued:** (a) a lifetime parameter `Context<'a, E>` — rejected,
`Context` is cloned into `'static` futures and stored in asset data, so this infects everything;
(b) scoped thread-local with lifetime erasure — realistic but needs `unsafe` and a take-and-restore
guard to prevent `&mut` aliasing under re-entrancy; (c) pass down the call stack explicitly.

**Potential use cases:** immediate-mode GUI drawing (`&mut egui::Ui`); GPU frame resources
(`wgpu::RenderPass<'a>` borrows the encoder); ambient DB transaction (`&mut sqlx::Transaction<'c>`,
genuinely `!'static`); streaming response body (`&mut dyn Write`); arena allocator (`&'a Bump`);
exclusive hardware handle. Note that most of these — streaming, arena, hardware, and DB pool access
— are already serviceable with an `Arc<Mutex<…>>` mobile payload. The only cases that *genuinely*
require a non-`'static` borrow are those where the resource is itself a borrow of a caller-owned
frame (GUI, GPU) or a lifetime-parameterized type (transactions).

**Recommendation: do not build this.** Beyond having no current use case, option (c) is what the
codebase already does and it works: `Element::show_in_egui(&mut self, ui: &mut egui::Ui, ctx:
&UIContext, …)` passes the frame borrow on the stack while payload carries the durable handle. More
importantly, direct drawing from commands would be **backend-specific** and would undermine the
multi-backend UI abstraction — the same element tree also renders through `render_web` with no egui
present. The retained-tree + message-passing design is not a workaround here; it is the portable
design.

## References

- `specs/ISSUES.md` — Issue: PAYLOAD-NESTED-EVALUATION-INHERITANCE
- `specs/PAYLOAD_GUIDE.md`, `specs/PROJECT_OVERVIEW.md` (claims this change makes true)
- `liquers-core/src/context.rs:76-80, 448-474` (payload doc + nested-evaluation methods)
- `liquers-core/src/assets.rs:2630-2714` (`AssetManager`: `get_dependency_asset`, `apply`, `apply_immediately`)
- `liquers-core/src/plan.rs:1353-1380, 1424-1427` (`Plan::is_volatile` — precedent for `requires_payload`)
- `liquers-core/src/command_metadata.rs:387-391` (`ArgumentInfo::injected`)
- `liquers-core/src/commands.rs:337-353` (`PayloadType`, `ExtractFromPayload`)
- `liquers-core/tests/injection.rs::test_payload_not_inherited_in_nested_evaluation`
