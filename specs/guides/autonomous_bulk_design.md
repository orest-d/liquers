---
title: Autonomous Bulk Design Procedure
kind: guide
audience: internal
area: [docs, build]
reviewed: 2026-08-31
---

# Autonomous Bulk Design Procedure

This is the binding procedure for a coding agent asked to create or finish design documents for
one issue or feature, or for a potentially large group of them, without pausing for phase approval.
The words **MUST**, **MUST NOT**, **SHOULD**, and **MAY** are normative. This procedure produces
the first four design phases; it does not authorize implementation.

It adapts the design analysis in
[`autonomous_issue_fixing.md`](autonomous_issue_fixing.md), but removes its implementation work and
approval gate. `DOCS_STRUCTURE_GUIDE.md` remains authoritative for document locations, metadata,
phase names, status, and generated indexes.

## 1. Applicability and authority

Use this procedure when the requested result is durable design material under
`specs/design/<slug>/`, especially when several issues or features need design in one run. It may
also resume a design that has only some of Phases 1-4, beginning with a critical review of every
existing phase in order. A design with a substantively finished Phase 4 is outside this procedure
and MUST NOT be modified or reviewed, regardless of whether it carries readiness metadata.

The agent MUST read each source issue or feature, related designs, current documentation, relevant
implementation and tests before choosing a solution. A batch request authorizes design and
repository-record maintenance only. It does not authorize source implementation, API migration,
GitHub issue creation, or resolving a user-facing design choice by silently selecting a preference.

An existing `workflow` marker remains authoritative. For `workflow: liquers-project`, this
procedure MAY review and draft missing Phase 1-4 artifacts, but MUST NOT claim an approval or
completion that bypasses that workflow's gates. Do not add, remove, or replace a workflow marker
merely to use this procedure.

## 2. Non-negotiable invariants

1. **Always produce Phases 1 and 2.** Every selected item gets a reviewed high-level design and
   architecture, even when feasibility is doubtful or the result is `phase2-blocked`.
2. **Conditionally produce Phases 3 and 4.** Continue only when the problem is reasonably clear
   and the evidence supports at least one working solution. Do not turn an unknown contract into
   speculative examples or a fictional implementation plan.
3. **Never wait for phase approval.** Review, revise, and advance autonomously when the evidence
   permits. A blocker ends work on that design, not on independent items later in the batch.
4. **Stop at design.** Phase 4 is an implementation plan and the absolute latest stopping point for
   each design. Do not edit production code, add the planned tests, run implementation steps, or
   describe implementation as started or complete. After final design review, continue with the
   next eligible issue or feature.
5. **Analyze feasibility continuously.** Reassess signatures, ownership, compatibility, data
   formats, error paths, dependencies, tests, and affected callers in every phase. Later evidence
   that invalidates an earlier phase requires revising that phase and all dependent artifacts.
6. **One source, one design.** Every selected issue and feature gets its own design folder and
   readiness value, and each design names exactly one source ID. Never merge several issues or
   features into one design, even when the implementation overlaps. A duplicate or covered item
   still gets its own design record, which points to the covering design.
7. **Keep records truthful.** Shared findings and dependencies may be cross-linked, but one
   design's certainty MUST NOT conceal another's blocker or replace its separate analysis.

## 3. Readiness and question classification

Every design handled by this procedure MUST set one `readiness` value in `DESIGN.md`:

| Value | Use when |
|---|---|
| `ready` | Phases 1-4 are reviewed and no blocking or open design question remains. |
| `needs-decision` | Phases 1-4 specify a working proposed solution, but a design choice affecting system use remains open. |
| `blocked` | Phases 1-4 exist, but the final review discovered a blocking unknown that makes them unsafe as an implementation basis. |
| `phase2-blocked` | The design stops after Phase 2 because uncertainty prevents valid examples/tests or a working implementation plan. |
| `covered` | The source has its own Phase 1 and 2 record, but another named design covers the work or the source is a duplicate, so no independent Phase 3 or 4 is needed. |

An absent readiness value on a design not handled by this procedure is valid and is rendered as an
empty index field. Readiness is not approval and does not replace `status` or `phase`.

The agent MUST maintain a short `## Design Readiness` section near the start of
`phase1-high-level-design.md` containing:

- **Readiness:** the exact enum value;
- **Leading issue:** the highest-severity unresolved question, or `None`;
- **Explanation:** one or two sentences stating why later phases are safe or why they stopped; and
- **Open questions:** a severity-ordered list using the tiers below, or `None`.

Classify questions critically:

1. **Blocking question.** No known defensible answer supports a working solution. State the missing
   fact or decision and include a concrete example showing how plausible answers produce
   incompatible behaviour.
2. **Open design question.** The choice affects a public or internal API contract, user workflow,
   query semantics, serialized or persistent data, compatibility, security, or observable error
   behaviour. State the consequences and give a recommended answer when evidence supports one.
3. **Proposed resolution.** A design question has a reasonable recommended solution and a working
   design can be completed around it, but the proposal remains visible for user review. This yields
   `needs-decision`, not `ready`, until resolved.
4. **Implementation detail.** The choice is hidden behind an already clear contract and does not
   change how the system is used. Resolve it autonomously in Phase 2 or 4; do not promote it to a
   user decision merely because several code shapes are possible.

The Phase 1 readiness section is a summary, not a question dump. Merge duplicates, remove questions
answered by repository evidence, and state each remaining question briefly, clearly, and in enough
context to be understood without reopening every phase. Lead each item with its tier, for example
`**Blocking - recipe identity:** ...`.

## 4. Bulk intake and preflight

Before writing phases, the agent MUST:

1. enumerate the requested issues and features and search `specs/index.csv` for duplicates, linked
   designs, shared prerequisites, superseding work, and existing partial artifacts;
2. inspect repository instructions and the worktree without discarding unrelated changes;
3. use a dedicated branch when edits are expected, unless the environment already provides an
   isolated task branch;
4. perform only the minimum eligibility check needed to distinguish a genuinely finished Phase 4
   from a generated or empty template: use explicit phase tracking plus substantive Phase 4 plan
   content; once completion is established, exclude that design immediately without reviewing its
   correctness, reading its earlier phases, adding readiness, or modifying any file;
5. group eligible items by affected subsystem and dependency, then choose an order that handles
   shared prerequisites before dependents;
6. record for each eligible item its canonical source, unique design slug, existing phases,
   expected missing phases, and initial uncertainty; and
7. assign every eligible issue or feature its own design, including duplicates and items covered by
   another design; never group multiple source IDs into one design folder.

A Phase 4 filename, unchecked tracking row, or untouched template is not evidence of a finished
Phase 4. Such a design remains eligible and its existing phases are reviewed under section 11.

For a large group, work in bounded batches that fit the available context. Complete and validate
repository records for each batch before moving on. Do not reduce analysis depth to increase item
count, and do not stop the whole run merely because one design is blocked.

## 5. Design folder and lifecycle metadata

Create `specs/design/<slug>/DESIGN.md` using the contract in `DOCS_STRUCTURE_GUIDE.md`. Its
`issues:` list MUST contain exactly one issue or feature ID, and that source document's `design:`
field MUST link back to this design slug. No other readiness-labeled design may claim the same
source. Similarity, shared code, or a common dependency does not permit combining sources.

When the source is a duplicate or its work is fully covered elsewhere, retain this separate design,
set `readiness: covered`, and state the covering issue and design explicitly in Phase 1 and Phase 2.
Do not copy the covering design's later phases into this folder. Use truthful issue/design lifecycle
metadata such as `duplicate_of` or `superseded_by` when its existing contract applies.

New designs produced here normally omit `workflow`, because this is a simplified four-phase
procedure rather than the five-phase `liquers-project` contract. While drafting, use truthful local
status and phase values:

- after stopping at Phase 2, normally `status: in_review`, `phase: architecture`, and
  `readiness: phase2-blocked`;
- after completing Phase 4, normally `status: in_review`, `phase: implementation`, with
  `readiness: ready` or `needs-decision`;
- after the current run's final review finds a new blocking problem in its four-phase design,
  preserve a truthful lifecycle state and set `readiness: blocked`; and
- for a duplicate or covered item concluded after Phase 2, use `readiness: covered` and truthful
  terminal or live lifecycle metadata as appropriate.

Do not write `approved` unless approval actually exists. Do not write `complete` merely because the
design documents are complete: a reviewed design waiting for implementation remains at the
appropriate live lifecycle state.

## 6. Phase 1: High-level design

Phase 1 defines **what** should change and **why**, without committing to code structure. It MUST
include:

- the problem and observed evidence;
- expected behaviour and testable acceptance criteria;
- affected users, workflows, and Liquers systems;
- scope, dependencies, and explicit non-goals;
- compatibility, migration, security, and data-format constraints where relevant;
- the Design Readiness section from section 3; and
- a documentation assessment naming current documents likely to change.

Phase 1 MUST contain a `## Design Dependencies` section. List every discovered dependency by design
slug, classify it as `requires`, `required-by`, `covered-by`, or `overlaps`, and state how it affects
implementation order or feasibility. Write `None` when there is no known design dependency. A code
dependency that does not order or constrain another design remains an implementation detail.

When Phase 4 is completed in the current run, Phase 1 MUST also contain a concise
`## Consolidated Findings` section. This section is populated during the final review in section 10,
not guessed during initial drafting.

Critically review Phase 1 for duplication, coherent scope, architectural fit, testability, and
unknown terms or expected behaviour. Set a provisional readiness, then update this same document
after every later phase so it remains the concise entry point to feasibility and open questions.

## 7. Phase 2: Solution and architecture

Phase 2 defines **how** a working solution would fit the current codebase. It MUST be based on
inspected signatures and call sites and include, as applicable:

- the chosen solution and rejected alternatives;
- exact crates, modules, files, types, traits, functions, commands, routes, and call sites;
- ownership, serialization, persistence, errors, and sync/async behaviour;
- API and compatibility effects, including migration or fallback;
- reuse of existing code and dependencies;
- interactions with related issues and designs; and
- all blockers, open design questions, proposed resolutions, and resolved implementation details.

It MUST include an explicit risk table covering likely files, affected workflows and crates,
existing-test impact, new validation, compatibility/data/concurrency/performance/security risks,
recovery, and certainty. Review it once against Phase 1 and once against the codebase. Resolve every
question answerable from repository evidence, then update Phase 1 readiness.

For Rust designs, apply the repository's `rust-best-practices` guidance when it is available;
otherwise perform the equivalent ownership, borrowing, trait, error-handling, async/sync, and
compilation-feasibility review directly.

## 8. Autonomous continuation decision

There is no user approval gate. After Phase 2, continue to Phases 3 and 4 only when all are true:

- expected behaviour and the system boundary are sufficiently clear;
- at least one solution is technically feasible in the inspected code;
- examples and tests can distinguish correct behaviour from regressions;
- no blocking question changes the API, data contract, compatibility promise, or core semantics;
  and
- any remaining open design question has a concrete proposed resolution under which a working
  solution can be fully described.

If Phase 2 establishes that the source is a duplicate or fully covered by another named design,
set `readiness: covered`, record the `covered-by` relationship in Phase 1, and continue to the next
source without producing independent Phase 3 or 4 documents.

If any condition is false, set `readiness: phase2-blocked`, make Phase 1's leading issue a blocking
question with an example, leave later phase links absent or explicitly incomplete, and continue to
the next independent design. Do not ask for approval during the run. Collect decisions needed from
the user in the final report.

## 9. Phase 3: Examples and tests

Phase 3 specifies externally meaningful examples and validation; it does not add tests. Include:

- primary and normal use cases with expected results;
- relevant error, edge, compatibility, persistence, serialization, concurrency, or binding cases;
- exact unit and integration tests to add or change; and
- setup, registered commands, stores, fixtures, and valid Liquers queries needed to run them.

Prefer runnable test designs over conceptual examples. Review coverage against Phase 1 acceptance
criteria and every Phase 2 risk. If this exposes ambiguity that prevents a reliable expected
result, return to Phase 1 and 2 and set `phase2-blocked`; do not preserve speculative Phase 3 claims
as if they were valid. Follow [`UNITTEST_GUIDE.md`](UNITTEST_GUIDE.md) and the repository's
`liquers-unittest` guidance when available.

## 10. Phase 4: Implementation plan

Phase 4 is an executable plan, not execution. Each ordered step MUST name exact files and symbols,
the intended change, dependencies on earlier steps, proof by a Phase 3 test or validation command,
and rollback or containment for risky work. Include:

- implementation and test changes;
- required issue, feature, design, reference, guide, and generated-index maintenance;
- formatting, focused tests, and proportionate crate or workspace checks; and
- a final diff review for scope, generated files, debug code, and unrelated edits.

Review the plan for conformity with Phases 1-3, feasible ordering, completeness, and smallest
coherent implementation scope. Reinspect any signature on which a step depends. Then perform one
final review of all four design documents for contradictions, missing acceptance coverage,
feasibility, dependencies, risks, decisions, test obligations, and documentation effects.

Collect every important finding from the design process and final review into Phase 1's
`## Consolidated Findings` section. Summarize rather than duplicate later documents, but retain the
information an implementer must see before acting: architectural constraints, dependency ordering,
compatibility or data consequences, critical risks, required validation, and decisions or blockers.
Update Phase 1's readiness, leading issue, open questions, and Design Dependencies from this review.

A design is `ready` only when all four phases and the consolidated findings agree and no blocking or
open design question remains; a viable plan containing a visible proposed choice is
`needs-decision`. If the final review exposes a blocker, use `blocked` and explain why the completed
plan is not safe to execute.

This final synthesis completes processing of the design. The agent MUST NOT execute any Phase 4
step or begin implementation. Move directly to the next eligible issue or feature in the batch.

## 11. Resuming partial designs

This section applies only after the eligibility check in section 4 establishes that Phase 4 is not
substantively finished. If Phase 4 is finished, stop inspecting that design and make no change. For
an eligible partly completed design, do not begin at the first missing filename. First:

1. read `DESIGN.md`, the source issues/features, and every existing phase in order;
2. verify that lifecycle metadata and links still describe the current artifacts;
3. recheck code signatures, tests, dependencies, related designs, and assumptions against HEAD;
4. revise stale or contradictory earlier phases before adding later ones; and
5. create or update the Phase 1 Design Readiness section before deciding whether Phase 3 is safe.

Missing phases may then be produced under sections 8-10. Never infer that an unchecked box means
the preceding phase is correct, and never mark a phase complete solely because a file exists.

## 12. Repository maintenance and validation

After each bounded batch:

1. ensure `DESIGN.md`, phase links, source `design:` links, readiness, status, and phase are truthful;
2. regenerate `specs/index.csv` and generated README blocks with
   `python3 scripts/docs_index.py`;
3. run `python3 scripts/docs_index.py --check`;
4. review the diff for cross-design leakage, stale readiness explanations, and accidental source
   implementation; and
5. commit or hand off the design changes according to the environment's requested workflow.

Do not create implementation pull requests. If the task explicitly includes publishing the design
branch, a design-only pull request may be created after validation; otherwise report the branch and
leave publication to the user.

## 13. Closing report

Report each design independently, grouped by readiness:

- design slug and source issue/feature IDs;
- phases reviewed, revised, and created;
- final readiness and its brief explanation;
- leading issue and decisions needed, with blocking examples;
- validation actually run; and
- files or phases intentionally not produced.

State aggregate counts for `ready`, `needs-decision`, `blocked`, `phase2-blocked`, and `covered`,
plus the number of finished-Phase-4 designs excluded without review. Distinguish questions
requiring a system-design decision from resolved implementation details. Never describe a design
as implementation-ready merely because four files exist.

## History

| Date | Change | Source |
|---|---|---|
| 2026-08-31 | Added the autonomous four-phase bulk-design procedure, one-source-per-design enforcement, finished-Phase-4 exclusion, dependency recording, continuous feasibility review, final Phase 1 synthesis, tiered questions, partial-design resumption, and indexed readiness states. | documentation |
