---
id: PLAN-POLICY
kind: design
title: Make the predecessor-boundary policy a stated, configurable choice and retire the stale plan-builder policy markers
form: compact
workflow: liquers-project
status: in_review
phase: high-level
area: [core/plan, core/context]
issues: [CORE-PLAN-POLICY-AND-DEFAULTS]
affects_docs: [specs/reference/api/DOC_08_RECIPES_PLANS.md, specs/reference/ENVIRONMENT_CONFIG.md]
created: 2026-10-10
---
# Make the predecessor-boundary policy a stated, configurable choice and retire the stale plan-builder policy markers

## Phase 1: High-Level Design

### Purpose

`CORE-PLAN-POLICY-AND-DEFAULTS` asks for the plan policies that are compiled in to become a stated,
deliberate choice. The `expand_predecessors` half has been answered (`predecessor-cut-equivalence`).
What is left is three markers at `liquers-core/src/plan.rs`, just above `impl PlanBuilder`:
`// TODO: support cache`, `// TODO: support volatile flags` and `// TODO: support inline flag`. This
design maps each marker to a concrete decision. The one real gap is that **whether a predecessor is
cut into a cached boundary or inlined can be neither configured nor declared**. The design closes
that gap and removes the markers.

### Analysis: what the three markers mean at HEAD

| Marker | What exists today | Decision |
|---|---|---|
| **volatile flags** | Four instruments already set volatility: command metadata `volatile`, the `v` instruction, `Recipe::volatile`, and a volatile `Recipe::expires`. A link to a volatile query propagates it too. Each is recorded as a `VolatilitySource` (`plan.rs`). | **Nothing to add.** Drop the marker. A *config* knob over volatility would be unsound if it ignored declared volatility, and it would duplicate `v` / `volatile: true` if it forced volatility. Positional `v` is a different contract and is already filed as `V-INSTRUCTION-IS-WHOLE-PLAN-NOT-POSITIONAL`. |
| **cache** | *Final asset* caching is already a recipe choice: `Recipe::cached` / `Recipe::stored` (`recipes.rs`, from `record-streams`). *Intermediate* caching is the predecessor boundary, because a boundary is a cache entry (`DOC_08_RECIPES_PLANS.md`, "Predecessor boundaries"). It is hard-wired: `finalize_plan` (`interpreter.rs`) always calls `Plan::cut_predecessor`. | **Make the boundary placement a policy.** It gets an environment-level default, settable from `EnvironmentConfig`. |
| **inline flag** | To inline a predecessor is the opposite of cutting it. The only way to get an expanded plan on an evaluation path today is a hand-written `Environment::apply_recipe` that calls `finalize_plan_expanded`. | **A per-recipe override of the same policy**, folded onto the plan by `Recipe::to_plan`, the way `volatile` and `expires` already are. |

The issue proposed a `PlanBuilderConfig`. This design gives the builder no configuration, and the
issue's wording is answered by that choice (see Open Questions, Q1). Since `plan-cwd-freeze`,
`PlanBuilder` deliberately always expands, and its single knob, `with_placeholders_allowed`, is a
per-call fact rather than a policy. The policy lives where the boundary is cut, in finalization.

### Problem Example

**Example.** A deployment evaluates the recipe below once per request. `sales.csv` is large, the
filtered frame is used by this recipe only, and the operator wants to keep memory low on a small or
wasm host:

```yaml
sales_summary.txt:
  query: "-R/data/sales.csv/-/ns-pl/from_csv/eq-region-EU/describe"
```

The query validates (`liquers-validate`, status `Ok`). It builds four steps (`GetAsset`, `from_csv`,
`eq`, `describe`) with the predecessor `-R/data/sales.csv/-/ns-pl/from_csv/eq-region-EU`.

- **Today:** `finalize_plan` always cuts. The filtered frame becomes its own asset in the asset
  manager, and it stays there after `describe` has consumed it. Neither the operator nor the recipe
  author can say "inline it". The only way out is a custom `Environment` with its own
  `apply_recipe`.
- **Expected:** the author writes `predecessor_boundary: expand` on the recipe, or the operator
  sets `plan: { predecessor_boundary: expand }` in the environment configuration. The plan then
  runs all four steps inline, with no `Step::Evaluate`, and the reason is recorded as a planning
  `Step::Info`. With neither setting, behaviour is unchanged.

### Scope and Acceptance Criteria

- **AC-1** Recipe declares inline
  WHEN a recipe with `predecessor_boundary: expand` is evaluated in a default environment
  THEN its finalized plan contains no `Step::Evaluate` boundary, the result equals the cut result,
  and `init_steps` record that the recipe declined the boundary
- **AC-2** Environment default
  WHEN an environment built with `plan.predecessor_boundary: expand` (from configuration or the
  builder) evaluates a recipe that says nothing
  THEN no boundary is cut, and `init_steps` name the environment policy as the reason
- **AC-3** Recipe overrides environment
  WHEN a recipe with `predecessor_boundary: outermost` is evaluated in an `expand` environment
  THEN exactly that recipe's outermost cacheable predecessor is cut
- **AC-4** Defaults unchanged
  WHEN neither the environment nor the recipe sets a policy
  THEN plans are cut exactly as today, and every existing cut and expansion test passes unchanged
- **AC-5** Soundness outranks policy
  WHEN the policy is `outermost` but the plan is declared volatile, needs a payload in every
  candidate, or is applied to an input state
  THEN no boundary is cut, exactly as today
- **AC-6** Serialized forms are backward compatible
  WHEN an existing `recipes.yaml`, a serialized `Plan` or an environment configuration without the
  new keys is deserialized
  THEN it loads unchanged, and serializing it again emits none of the new keys
- **AC-7** Markers retired
  WHEN `plan.rs` is read
  THEN the three `TODO: support …` markers are gone, and the builder's documentation states its
  fixed behaviour (always expands; placeholders rejected unless allowed) and where each former
  marker's concern now lives

**Non-goals.** Positional `v` (`V-INSTRUCTION-IS-WHOLE-PLAN-NOT-POSITIONAL`). Cutting more than one
boundary per plan. An asset-manager retention policy (`CORE-ASSET-GC`). A query-level instruction
for inlining (see Q2). Any change to `PlanBuilder`'s output or to `liquers-validate`, which keeps
showing the expanded plan.

**Systems touched and crate placement.** All in `liquers-core`: `plan.rs` (policy type, a plan
field, docs), `recipes.rs` (recipe field and the fold in `to_plan`), `interpreter.rs`
(`finalize_plan`), `context.rs` (`Environment` default method, `EnvRef`, `GenericEnvironment`),
`environment_builder.rs` and `environment_config.rs` (the `plan:` section). `liquers-py` and other
custom `Environment` impls compile unchanged through a default trait method. No command, no
`ExtValue`, no registry change.

**Documentation intent.** Reference: update `DOC_08_RECIPES_PLANS.md` ("Predecessor boundaries"
and the recipe field table) and `ENVIRONMENT_CONFIG.md` (new `plan:` section). Guide: none. Other:
close `CORE-PLAN-POLICY-AND-DEFAULTS` with a resolution note that maps each marker. Update
`specs/README.md` for the new design folder.

### Design Dependencies

- **overlaps** `V-INSTRUCTION-IS-WHOLE-PLAN-NOT-POSITIONAL` (draft, P3). Weak overlap: same area,
  and both touch volatility near `PlanBuilder::mark_volatile`. No T1-T4 test holds, because this
  design changes no volatility semantics. It stays separate.
- **overlaps** `CORE-ASSET-GC`. That issue owns the memory counterweight to cutting; this design
  only lets a deployment opt out of cutting.
- **predecessor:** `predecessor-cut-equivalence` (complete) lists this issue among its sources. It
  is frozen (overlap E4), so the remainder gets this new design. The issue's empty `design:` field
  is set to `plan-policy`.

### Open Questions

- **Q1 (proposed resolution) — name and home.** The issue says `PlanBuilderConfig`. Recommendation:
  call it `PlanPolicy`, held by the environment and applied in `finalize_plan`, with no builder
  config at all. A builder config would carry one field (`allow_placeholders`) and invite putting
  back a builder-time cut switch, which `plan-cwd-freeze` removed for a reason.
- **Q2 (open design) — where the inline flag lives.** Recommendation: on the recipe only. A query
  instruction (`…/inline/…`) would change the query, and with it the asset's cache key. It would also
  add grammar to the performance-sensitive parser. A recipe is where authors already declare
  `volatile`, `cached` and `stored`.
- **Q3 (open design) — vocabulary.** Recommendation: two values, `outermost` (default, today's
  behaviour) and `expand`. A third value such as `innermost` or `all` would be a new cutting
  algorithm, not a policy, and can be added to the enum later without breaking anything.
- **Q4 (implementation detail).** The recipe override is recorded on `Plan` as
  `boundary_policy: Option<BoundaryPolicy>`, with a serde default, skipped when `None`. That keeps
  `finalize_plan`'s signature unchanged and makes a serialized plan self-describing.
