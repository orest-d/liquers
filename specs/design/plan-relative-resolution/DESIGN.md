---
id: PLAN-RELATIVE-RESOLUTION
kind: design
title: CWD-relative query resolution in plans
workflow: liquers-project
status: in_review
phase: documentation
area: [core/query, core/plan, core/assets, core/context]
gh_pr: []
issues: [CORE-PLAN-RELATIVE-RESOLUTION-MISSING]
affects_docs: [reference/api/DOC_02_QUERY_LANGUAGE_REFERENCE.md, reference/api/DOC_04_ENVIRONMENT_CONTEXT_EVALUATION.md, reference/api/DOC_08_RECIPES_PLANS.md, guides/LANGUAGE-INTEGRATION_GUIDE.md, reference/ASSET_LIFECYCLE.md, reference/PROJECT_OVERVIEW.md]
created: 2026-08-10
superseded_by:
---
# plan-relative-resolution Design Tracking

**Created:** 2026-08-10

## Phase Status

- [x] Phase 1: High-Level Design — approved 2026-08-10
- [x] Phase 2: Solution & Architecture — approved 2026-08-10
- [x] Phase 3: Examples & Testing — approved 2026-08-10
- [x] Phase 4: Implementation Plan — approved 2026-08-11
- [ ] Phase 5: Documentation
- [ ] Implementation Complete

## Notes

- 2026-08-10: Verified against HEAD before design: `Query::to_absolute` exists, but plan/runtime
  query-bearing steps retain unresolved queries; `validate::tests::cwd_changes_key_not_plan` passes.
- 2026-08-10: Phase 1 feedback clarified that provider-loaded recipes derive `cwd` from the
  containing `recipes.yaml`, YAML must not author it, and programmatic `Recipe::cwd` must work.
  Inspection found built-in `apply_recipe` implementations ignore `Recipe::cwd`; `SetCwd` updates
  context state, but current pre-scheduling and dependency scheduling do not consult that state.
- 2026-08-10: Phase 1 feedback initially considered absolute conversion before plan building,
  alongside a recipe-derived `SetCwd` executable prefix and an init `Info` diagnostic. Explicit
  `-R-cwd/<key>` precedence remained a Phase 2 question because current `Query::to_absolute`
  resolves every resource segment independently against one base.
- 2026-08-10: Phase 1 feedback resolved precedence: CWD is stateful and ordered. A relative
  `SetCwd` key is resolved against the current CWD, then becomes the base for all later keys,
  embedded queries, and nested plans. The parser requires `-R/./hello.txt` inside the link;
  `-R./hello.txt` is not valid resource-query syntax.
- 2026-08-10: Phase 1 responsibility split initially considered `PlanBuilder` recursive ordered
  resolution alongside interpreter-owned live CWD state. This was superseded before Phase 2
  approval: the builder preserves source-relative operands and the interpreter/context is the
  sole semantic CWD authority.
- 2026-08-10: User said `proceed`; Phase 2 started. Relevant command assessment: no new commands
  and no existing `liquers-lib` namespace appears involved, pending user confirmation.
- 2026-08-10: The initial Phase 2 draft (later superseded before approval) used ordered recursive
  `Query::to_absolute`, transient
  `PlanBuilder` CWD state, a recipe-derived executable `SetCwd` prefix plus init diagnostic, and
  `Context`-backed runtime normalization for programmatic plans. Known-issue preflight found no
  blocker; adjacent plan/evaluation issues remain independent.
- 2026-08-10: Parallel Phase 2 reviews found four fixable gaps: ambiguous initial-step wording,
  recipe CWD bypass in nested dependency traversal, unresolved action-link identities in
  `find_dependencies`, and dropped Phase 1 documentation commitments. The architecture now
  specifies a single prefix, recipe-aware dependency planning, recursive link resolution, the
  complete documentation set, and a requirement-to-test evidence matrix.
- 2026-08-10: The required Phase 2 fixer review confirmed all reviewer findings are closed and
  tightened nested keyed-recipe traversal to `Recipe::to_plan_for_key`; only user confirmation of
  the no-relevant-commands assessment remains before the Phase 2 approval gate can close.
- 2026-08-10: User challenged dual builder/runtime CWD semantics before approval. Critical review
  accepted the concern: PlanBuilder now preserves relative operands; Recipe::to_plan only prepends
  the CWD step and diagnostic; interpreter, dependency analysis, and pre-scheduling share ordered
  cursor semantics. Static absolute conversion is deferred to a future semantics-preserving plan
  pass, and observable SetCwd steps cannot be removed merely because operands are absolute. The
  shared cursor is a pure AST helper; only Context holds live CWD, and recursive async
  pre-scheduling uses a boxed helper to avoid an invalid recursive async function.
- 2026-08-10: Missing-base semantics clarified: the first genuinely relative operand defaults the
  resolution cursor and live Context to logical root `/` with a warning; ordinary keys and absolute
  queries neither change CWD nor warn.
- 2026-08-10: User said `proceed`, confirming that no command namespace is relevant and approving
  the interpreter-authoritative Phase 2 architecture. Phase 3 started.
- 2026-08-10: User selected option 1, runnable prototypes. Phase 3 will specify complete,
  test-shaped Rust acceptance examples and their intended repository paths; because the approved
  behavior is not implemented yet, they are expected to become passing executables during Phase 4
  implementation rather than being misrepresented as passing against the current code.
- 2026-08-10: Phase 3 drafting, synthesis, and three-way review completed. The fixer replaced the
  placeholder integration skeleton with a complete compile-checked acceptance prototype, made
  link scoping observable, added Context/cache/error/concurrency coverage, and restored all Phase 1
  documentation targets. All six queries documented at that revision validate at plan level; the
  action-free seventh query was added immediately below. The Phase 3 validator passes. Phase 3 now
  awaits user approval.
- 2026-08-10: User added an action-free chained-CWD acceptance case. With entry CWD `a/b`,
  `-R-cwd/../-R-cwd/./c/-R-key/.` must return the `Key` value `a/c`, proving both relative
  `SetCwd` updates and `UseKeyValue` resolve against the current interpreter CWD.
- 2026-08-10: User said `proceed`, approving Phase 3 and starting the Phase 4 implementation plan.
- 2026-08-10: Phase 4 four-way review and final holistic review completed. The plan now uses a
  fresh-plan-only dependency call graph, immutable AssetRef-bound owner identity, atomic runtime
  fallback, exact recursive parameter materialization, ordered regression gates, and the complete
  Phase 3 documentation/learning evidence. Final reviewer confidence is 97% with no blocker.
- 2026-08-11: Re-reviewed all phases against pulled HEAD `5783eef` / PR #27. Reconciled Phase 2
  with the final owner, atomic-root, and single-pass dependency architecture; restored exact Phase 3
  test mapping; specified variadic argument wiring and eligible immediate-manager store metadata;
  added the existing `manager_parametric` gate; and normalized design metadata/document paths.
  Focused current-code tests, the seven-query matrix, phase validators, and documentation checks
  pass. Fresh holistic confidence remains 97% with no blocker; Phase 4 still awaits user approval.
- 2026-08-11: User said `proceed`, approving the re-reviewed Phase 4 implementation plan. Awaiting
  selection of an execution option; no implementation has started.
- 2026-08-11: User selected execution option 1. Step 1 implementation started under the approved
  serial green-gate plan; Phase 5 remains mandatory after code and validation complete.
- 2026-08-11: Implementation and executable review completed. The final library suite passes 507
  tests, `manager_parametric` passes 16 tests, the new recipe-CWD integration suite passes all eight
  scenarios, the seven-query validator matrix is clean, and the wasm/CLI/format gates pass. A
  public no-feature build remains an existing unsupported configuration because the core async
  dependencies are feature-gated; the supported wasm gate uses `--features async_store`. Phase 5
  current-state documentation started.
- 2026-08-11: The broad integration gate found that expiration persistence and dependency tracking
  still inferred ownership from the mutable provider recipe. Both now share the verified immutable
  bound-owner identity used by Context. The expiration suite passes all 32 tests and
  `cargo test -p liquers-core --tests` passes in full.
- 2026-08-11: Phase 5 current-state documentation and critical review completed with no behavioral
  blocker. The design is in review pending explicit user approval; Phase 5 and implementation
  completion remain unchecked until that approval.

## Links

- [Phase 1](./phase1-high-level-design.md)
- [Phase 2](./phase2-architecture.md)
- [Phase 3](./phase3-examples.md)
- [Phase 4](./phase4-implementation.md)
- [Phase 5](./phase5-documentation.md)
