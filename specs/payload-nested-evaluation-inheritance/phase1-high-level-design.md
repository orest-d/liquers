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

1. **Payload-free queries always go to the asset manager.** A query requiring neither a payload nor
   input-state arguments is always requested from the asset manager, and is therefore cached,
   shared, and eligible to be a dependency — exactly today's behavior.
2. **Payload-requiring queries are evaluated immediately with the inherited payload**, on the ad-hoc,
   uncached, non-persisted path — the only path that carries a payload.
3. **Safety is the command author's responsibility, not enforced.** A command whose result genuinely
   varies with payload should be labeled `volatile`. The framework does not police this.

**The switch is on `PayloadRequirement` alone — no cache probe is needed.** The original framing was
"cache wins, otherwise evaluate with payload". With `Optional` deferred (D3), the cache-hit branch
for a payload-requiring query is provably unreachable: such a query is either evaluated with a
payload (immediate path → never stored, per D1) or without one (an error, per D5). It can therefore
never be in the cache, so the probe is vacuous. "Cache wins" survives as a *derived property* of
payload-free queries rather than as a mechanism.

This equivalence depends on `Optional` being absent. If `Optional` is ever added, a payload-free
evaluation of an optional query *can* populate the cache, the cache-hit branch becomes reachable,
and the exact predicate (which asset statuses count as a hit, probed without scheduling) must be
decided then.

**Efficiency requirement.** Choosing between the manager path and the immediate path must not require
speculatively evaluating both. The plan must therefore declare whether it needs a payload to run: a
`PayloadRequirement` (D3) is computed at plan-build time from command metadata — exactly like the
existing two-phase `Plan::is_volatile` detection (`plan.rs:1363-1366`) — and drives the switch.
Payload-free plans keep today's cached, shared, queued behavior with zero overhead.

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

**liquers-core**: `src/plan.rs` (plan-level payload requirement), `src/command_metadata.rs`
(`PayloadRequirement` enum on command metadata), `src/context.rs` (nested-evaluation switch),
`src/assets.rs` (payload-bearing evaluation path). `liquers-macro` for the command-level metadata
statement (D2). `liquers-lib/src/ui/commands.rs` for annotating existing payload-using commands.
Docs to update:
`specs/PAYLOAD_GUIDE.md`, `specs/PROJECT_OVERVIEW.md`, `liquers_core::context` rustdoc, and the
existing non-inheritance test (which becomes an inheritance test).

## Open Questions

Resolved in **Resolved Decisions** below: dependency semantics and cycle detection (D1), how payload
requirement is declared (D2), the state representation, `optional` and the `payload: required`
spelling (D3), scoped payload (D4), and recursive plan analysis with early error (D5). The
cache-hit predicate is dissolved rather than answered — see the Chosen Decision section: with
`Optional` absent, no cache probe is needed at all.

1. **Deferred to Phase 2:** does `Context::apply` (ad hoc, no dependency recorded) also inherit
   payload, or does it keep its current non-forwarding behavior? D1 makes the payload branch
   semantically equivalent to `apply_immediately`, which argues for `apply` inheriting too.
2. Should a *keyed* asset whose recipe requires payload — thereby becoming non-cacheable and
   ineligible as a dependency — produce a plan-time warning, an error, or be silently accepted (D5)?
3. **Awaiting your decision:** should payload requirement be fused with `volatile`? See D6 for
   options and a recommendation.

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

> **Superseded by D1–D4 below.** The review's analysis stands, but the resolutions are: no argument
> keyword (D2 — the command-level statement is the sole mechanism), and `Optional` deferred behind an
> extensible enum (D3). Refinement 3's table is retained as the rationale for reserving `Optional`.

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

**This preserves an invariant the codebase already holds.** `register_scheduled_dependency` is
already documented as "Only keyed assets are graph nodes; an expression is expanded onto its
attribution set (the keyed assets that depend on it)" (`dependencies.rs:412-414`). Payload-evaluated
assets are ad hoc — neither keyed nor query-identifiable — so excluding them from the graph is
consistent with the existing model rather than a new exception. Only keyed, non-payload assets are
dependencies and graph nodes.

**Cycle detection stays at plan level, where it already exists.** `find_dependencies` walks recipe
chains with a visited `stack` and returns "Circular dependency detected" (`plan.rs:1687-1694`),
independently of the runtime graph. This is the right layer and is unaffected by the payload branch.

Remaining consequences for Phase 2:

- **Runtime recursion of ad-hoc queries is a pre-existing gap, slightly widened.** A command body
  calling `context.evaluate("/-/b")` where `b`'s body calls `context.evaluate("/-/a")` builds
  queries at runtime that plan analysis never sees. Today, when the parent is ad hoc,
  `schedule_dependency_asset` computes `dependent_opt = None` and registers nothing, so no cycle
  check runs either (`context.rs:391-397`). Payload-evaluated children are *always* ad hoc, so the
  branch widens this existing hole rather than creating it. A recursion-depth limit on `Context`
  remains a cheap belt-and-braces guard — worth considering in Phase 2, not required by this design.
- **A parent's recorded dependencies become incomplete.** A keyed, cacheable asset that performs
  payload-evaluated nested calls has untracked inputs. Per rule 3 this is the author's
  responsibility (mark `volatile`) and is *not* enforced — but it should be **visible**: plan
  building can emit a `Step::Warning` into `init_steps`, which already exists for exactly this
  kind of non-blocking diagnostic (`plan.rs:1356-1359`).

### D2. No `payload` argument keyword — declaration is command-level only

**Decided: not now.** `register_command!` keeps `injected` as its only argument-level modifier.
Payload requirement is declared **exclusively** by a command-level metadata statement, alongside
the existing `volatile:` / `label:` keys (`registration.rs:773-777`).

This is the simpler design, and it is not merely a deferral — it is arguably better:

- **One mechanism, uniformly applied.** An argument keyword could never have covered
  `context`-accessing commands (`fn cmd(state, context)` calling `get_payload_clone()` declares no
  argument), so a command-level statement was needed regardless. Having only the statement removes
  the second, partially-overlapping mechanism.
- **No divergence risk.** With two mechanisms, an argument marked `payload` on a command whose
  statement says otherwise would produce contradictory metadata. That class of bug cannot arise.
- **No `injected`/`payload` confusion**, and no need for a compile-time `ExtractFromPayload` bound
  assertion to keep declaration and implementation honest.

**Residual hazard to document (unchanged by this choice).** Declaration is fully manual: a command
that reads payload but omits the statement defaults to "does not use payload", works normally at
top level via `evaluate_immediately`, and silently loses payload only in nested position. Every
existing payload-using command — `liquers-lib/src/ui/commands.rs`, `tests/injection.rs`, and the
PAYLOAD_GUIDE examples — must be audited and annotated in Phase 2. The plan-time `Step::Warning`
from D1 is the main mitigation.

**Unrelated fix worth taking separately:** the argument parser consumes an unknown trailing ident
and silently treats it as not-injected (`flag == "injected"` → `false`, `registration.rs:1531-1536`),
so a typo like `injcted` is a silent no-op. This is a latent bug independent of payload work.

### D3. Two states now, represented as an enum extensible to three

**Decided: `Optional` is not implemented now**, but the representation must be an **enum, not a
bool**, so it can gain a third state without a breaking representation change:

```rust
/// Whether a command needs an evaluation payload.
/// `Optional` (runs without a payload, but receives one when available)
/// is reserved for a future extension; see specs/ISSUES.md.
pub enum PayloadRequirement {
    /// Does not use payload. Default.
    None,
    /// Refuses to run without a payload.
    Required,
    // Optional, // future — see D3 note
}
```

| State | No payload available | Payload available, cache hit | Payload available, cache miss |
|---|---|---|---|
| `None` | manager path, cached | use cached asset | manager path, cached |
| `Required` | **refuses to run** | use cached asset | immediate path with payload, uncached, not a dependency |

**Declaration spelling** (resolving the D2 open question): a command-level metadata statement
`payload: required`, parsed beside `volatile: true` (`registration.rs:773-777`). Note that
`volatile` parses a `syn::LitBool` whereas `payload` takes a bare ident naming the enum variant —
a small parser addition, and the form that extends naturally to `payload: optional` later.

Because the requirement is known at plan-build time, a `Required` command reached with no payload
fails fast at planning rather than deep inside execution (D5).

Adding `Optional` later is a deliberately breaking change for exhaustive matches — which is exactly
what the project's "no default match arm" convention wants, since every match site is forced to
decide how to treat it. Phase 2 must therefore avoid `_ =>` arms on this enum.

**Why `Optional` is worth reserving.** It is the state for a command that benefits from a payload
but does not need one: cacheable when no payload exists, payload-fed when one does. Its cost is the
tie-break it forces on the cache-miss branch (fidelity vs. cacheability) and the order-dependence
noted below, which is why it is not worth paying for until a concrete use case appears.

**Consequence that must be documented:** under cache-first, a cache hit means the command does not
run at all, so *even a `Required` command's payload is bypassed on a hit*. This is not a correctness
bug given rule 1, but it means: **any command whose cacheable output genuinely varies with payload
must be marked `volatile`.** That is precisely rule 3, and cache-first makes it effectively mandatory
rather than advisory. (With `Optional` this would additionally become order-dependent — whichever
caller arrives first decides whether a cached entry exists — a further reason to defer it.)

### D5. Payload requirement is derived recursively at plan level; a missing payload is an error

Evaluating a payload-requiring plan without a payload is an **error**, raised as early as possible.

**Recursive analysis.** A plan's payload requirement is not just its own commands': a query whose
*non-dynamic* dependency requires a payload is itself payload-requiring, transitively. This mirrors
the existing recursive dependency traversal — `find_dependencies` already walks recipe chains with a
visited `stack`, resolving each key's recipe and recursively building the nested plan
(`plan.rs:1667-1730`). Payload-requirement propagation is the same traversal and should ride along
with it rather than duplicating the walk.

**Architectural note for Phase 2 — this needs two places, not one:**

- **Local requirement** (this plan's own commands) is derivable from command metadata in the
  synchronous `PlanBuilder::build()` (`plan.rs:1004`), exactly like `Plan::is_volatile`.
- **Transitive requirement** (through recipes of non-dynamic dependencies) requires resolving
  recipes, which is async and needs an `EnvRef` — so it cannot live in sync `build()`. It belongs
  with `find_dependencies`, which already has that signature.

`Plan::is_volatile` is a precedent for the local half only; the transitive half has no existing
counterpart and is the genuinely new work.

**Accepted limitation.** Commands with *dynamic* dependencies — queries constructed at runtime and
passed to `context.evaluate` — are invisible to plan analysis. A mislabeled command of this kind will
not be caught early; it simply fails when it discovers it needs a payload and has none. This is the
accepted failure mode, consistent with rule 3 (declaration is the author's responsibility).

**Interaction to flag for Phase 2.** A *keyed* asset whose recipe requires a payload becomes
non-cacheable and cannot be a dependency (D1), even though keyed assets are normally stored and
shared. Whether that should be a plan-time warning, an error, or silently accepted needs a decision.
### D6. Under discussion: fusing payload requirement with `volatile`

**A factual correction first — `volatile` does not mean "evaluated immediately" today.** A volatile
query gets a *fresh, unshared* asset: `get_volatile_query_asset` constructs a new `AssetRef` and,
unlike `get_nonvolatile_query_asset`, never inserts it into the `query_assets` map
(`assets.rs:3774-3788`). It is also excluded from dependency-manager tracking
(`if !lock_is_volatile { dm.track_asset(...) }`, `assets.rs:1849-1856`). But it is still **scheduled
through the normal manager path** — queued under `DefaultAssetManager`, not routed to the ad-hoc
immediate path. It is also still persisted; `persist_with_status_tracking` is gated on
`save_in_background`/`cancelled`, not on volatility (`assets.rs:1333-1356`).

So there are two readings of the premise, and they differ:

- *"Volatile results are never reused"* — **true today**, and it is exactly the property a
  payload-evaluated asset needs.
- *"Volatile assets take the immediate code path"* — **false today**. Fusing would not remove the
  routing work, only the not-caching work.

**What the two concepts actually share.** Both imply "do not cache, do not share, do not track for
invalidation". Where they differ is direction: **`volatile` is a property of the output** (freshness/
reusability); **payload requirement is a property of the input** (something the evaluation needs to
run at all). They correlate but are not equivalent — a command can require a payload and be perfectly
deterministic given it (payload = a DB connection; same query, same rows), and a command can be
volatile without any payload (reads a clock, an external file, a random source).

One further precedent worth noting: `Context::is_volatile` **already propagates to nested
evaluations** (`context.rs:322-324`), which is the very inheritance mechanism payload lacks.

#### Option A — Full fusion: drop `PayloadRequirement`, route on `volatile` alone

*Pros:* smallest possible diff — no new metadata, no plan field, no macro statement, no annotation
migration. Reuses a flag that already exists, already propagates to nested contexts, and already
means "not cached, not shared, not tracked".

*Cons:* (1) **Behavior change for existing volatile commands** — every volatile command that never
touches payload would be re-routed to the immediate path, losing queued/parallel scheduling; a
performance regression on an unrelated feature. (2) **Loses the early error (D5)** — `volatile` says
nothing about *needing* a payload, so a missing payload cannot be detected at plan time; back to
runtime discovery, which was the whole point of D5. (3) **Contradicts rule 3** — volatile-labeling
was explicitly advisory and unenforced; fusion makes it mandatory for payload commands.
(4) Conflates an input requirement with an output property, so neither can be reasoned about alone.

#### Option B — No fusion: keep `PayloadRequirement` and `volatile` fully independent (current design)

*Pros:* clean separation of input need from output freshness; D5's early error works; zero behavior
change for existing volatile commands; `volatile` stays advisory per rule 3.

*Cons:* two flags whose *effects* overlap substantially, which authors must learn to distinguish;
requires building "don't cache / don't share" plumbing for payload assets that duplicates what
volatile already does.

#### Option C — One-way implication: payload-required **implies** volatile (recommended)

Keep `payload: required` as the declared input need, and derive `is_volatile |= requires_payload`
during plan building. Not a fusion — an implication in one direction only.

*Pros:* keeps D5's early error and the correct semantic distinction; **reuses the existing
non-caching machinery** rather than duplicating it (a payload asset is volatile, so it already gets
fresh-per-request, unshared, untracked treatment for free); no behavior change for existing volatile
commands, since the implication does not run backwards; the "payload result must not be reused"
invariant becomes structural instead of relying on author discipline.

*Cons:* makes volatility non-advisory *for payload commands specifically*, a mild narrowing of rule 3
— though arguably correct, since a payload-evaluated result genuinely must not be reused, and under
this design it is uncached regardless. Still two concepts to document. Does **not** remove the
immediate-path routing work, because volatile does not currently imply immediate.

#### Recommendation

**Option C.** It captures the real overlap — both mean "never reuse this result" — without asserting
the false equivalence that would come with Option A. The genuine simplification available here is
*implementation* reuse (payload assets inherit volatile's existing not-cached/not-shared/not-tracked
handling), not *concept* reduction. Option A's saving is smaller than it appears, since routing must
be built either way, and it costs the early error that motivated D5 plus a behavior change to
unrelated volatile commands.

If Option C is adopted, D1's "payload-evaluated assets are never a dependency" should be re-checked
against volatile's current dependency behavior: volatile assets are excluded from DM *tracking* but
are not otherwise barred from being dependencies, so D1 may still need an explicit rule rather than
falling out of volatility.

### D4. No scoped payload — too niche; recorded as a verified-compatible future axis

**Decided: not pursued.** This section is retained as design verification (the current design must
not foreclose it) and as a record of why it was rejected, not as planned work.

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
- `PayloadRequirement` — survives, but with two payload types the enum may need to name which one.
  The enum chosen in D3 leaves room; a hardcoded `requires_payload: bool` would not. This is a
  second, independent argument for D3's enum representation.
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
