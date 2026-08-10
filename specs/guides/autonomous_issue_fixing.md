---
title: Autonomous Issue Fixing Procedure
kind: guide
audience: internal
area: [docs, build]
reviewed: 2026-08-10
---

# Autonomous Issue Fixing Procedure

This is the binding procedure for a coding agent asked to fix an issue autonomously. The words
**MUST**, **MUST NOT**, **SHOULD**, and **MAY** are normative. Skipping a required gate, phase,
review, branch, test, or documentation step invalidates the workflow.

This procedure is a deliberately simplified adaptation of the five-phase
[`liquers-project`](../../.agents/skills/liquers-project/SKILL.md) workflow for small, well-bounded
repairs. It preserves that workflow's phase purposes and ordering, but not its persistent artifact,
reviewer-count, or per-phase approval requirements. It does not replace `liquers-project` for
substantial feature or architecture work.

## 1. Applicability and authority

The default scope is an issue whose metadata in `specs/issues/` is both:

- `complexity: S` or `complexity: M`; and
- `priority: P2` or `priority: P3`.

The agent MUST NOT use this autonomous procedure for `L` or `XL`, for `P0` or `P1`, or when either
classification is unknown, unless the user explicitly authorizes this procedure for that specific
out-of-scope item. Permission to fix an issue is not by itself permission to exceed this boundary.
If investigation reveals that the recorded classification is too small or otherwise outside the
boundary, the agent MUST stop at the approval gate in section 5.

The agent MUST read the issue, relevant source and tests, linked design material, and applicable
current documentation before choosing a solution. The
[`DOCS_STRUCTURE_GUIDE.md`](../DOCS_STRUCTURE_GUIDE.md) is authoritative for issue metadata,
design state, documentation placement, and index maintenance. The agent MUST preserve that guide's
ownership rules, including GitHub-owned status and the rule against changing the status of an issue
it did not just file.

## 2. Non-negotiable invariants

1. **Use a separate branch.** All edits, implementation, tests, and documentation changes MUST
   occur on a dedicated branch, never directly on the default or a shared branch. An automated or
   cloud agent MUST create or select that branch automatically before its first edit and report its
   name. It MUST verify the base and preserve unrelated work already present in the worktree.
2. **Do not broaden the issue silently.** Fix the stated problem and the minimum supporting code,
   tests, and documentation. Unrelated defects become proposed issues under section 11.
3. **Do not bypass uncertainty.** A guess that changes public behaviour, data semantics,
   compatibility, security, or architecture is an open question, not an implementation decision.
4. **Keep phases ordered.** Complete and critically review Phases 1 and 2 before applying the only
   approval gate. Complete Phase 3 before Phase 4. Start Phase 5 automatically when Phase 4
   succeeds. Later discoveries that invalidate an earlier phase require that phase and all
   dependent work to be revised.
5. **Validate claims.** Inspect actual signatures and call sites. Reproduce the problem where
   feasible. Run focused tests and proportionate broader checks. Never claim a command or test
   passed unless it was run successfully.
6. **Keep repository records truthful.** Update required specs and generated indexes in the same
   change as the implementation, following sections 9 and 10.

For S/M work, independent review concerns are mandatory but the `liquers-project` reviewer/model
counts are intentionally not required. When the environment provides and authorizes independent
reviewers, the agent SHOULD parallelize the Phase 2 conformity and codebase-alignment reviews and
the Phase 3/4 checks. Otherwise the primary agent MUST perform each concern explicitly. Delegation
never transfers responsibility for the result.

## 3. Workflow overview

```text
Intake and branch
  -> Phase 1: high-level design (WHAT and WHY)
  -> Phase 2: solution, architecture, and quantified risk (HOW)
  -> the only approval decision
       low and unambiguous -> continue automatically
       otherwise -> summarize, ask for "proceed", and stop
  -> Phase 3: examples, reproduction, and tests
  -> Phase 4: implementation plan, implementation, and validation
       success -> start Phase 5 automatically
       blocking issue -> report it; do not start Phase 5 as if implementation succeeded
  -> Phase 5: documentation summary and permitted small maintenance
  -> pull request and closing summary
```

The five phases are required reasoning records, but S/M work does not require the persistent design
folder and five phase documents used by `liquers-project`. The agent MAY keep them in its task plan
or working notes. It MUST create or update `specs/design/<slug>/` only when the issue already links
to that design, the user requests durable design documents, or the complexity rules in
`DOCS_STRUCTURE_GUIDE.md` require them. If a design folder is used, it MUST carry
`workflow: liquers-project`, use the canonical five artifact names, and follow that guide's
front-matter and phase transitions.

## 4. Intake and preflight

Before Phase 1, the agent MUST:

1. identify the canonical issue file and search `specs/index.csv` for duplicates or related work;
2. confirm priority and complexity are within the authorized scope;
3. inspect repository instructions and the current worktree without discarding unrelated changes;
4. switch to or create a dedicated branch from the intended base;
5. when the issue is GitHub-tracked and synchronization is available, synchronize before relying
   on remote status; otherwise state that remote state was not verified;
6. read linked designs and the current documentation for every affected area;
7. locate the implementation, existing tests, call sites, and recent equivalent patterns; and
8. reproduce the defect or explain why reproduction is not feasible.

If the issue is missing or materially underspecified, the agent may investigate enough to prepare
Phases 1 and 2, but MUST treat unresolved meaning or expected behaviour as an open question.

## 5. Phases 1 and 2 and the only approval gate

### Phase 1: High-level design

Phase 1 defines **what** must change and **why**, without implementation detail. Keep the record
concise (normally no more than 30 lines) and include:

- the problem and observed evidence;
- expected behaviour and acceptance criteria;
- affected users, workflows, and Liquers systems (Query, Store, Commands, Assets, bindings, or UI);
- scope and explicit non-goals;
- compatibility constraints and known questions or assumptions; and
- a brief documentation assessment: documents requiring small maintenance and any potentially
  substantive documentation need to revisit in Phase 5.

Critically review Phase 1 for duplication, scope clarity, consistency with the issue, fit with
Liquers' query-based layered architecture, testable acceptance criteria, and blocking unknowns.

### Phase 2: Solution and architecture

Phase 2 defines **how** the repair will work, without implementing it. It MUST be based on inspected
code rather than remembered APIs and include, as applicable:

- the chosen solution and rejected alternatives;
- exact modules, files, types, functions, traits, commands, routes, and call sites involved;
- data ownership, serialization, error handling, and sync/async implications;
- API and backward-compatibility effects;
- integration with existing commands and namespaces;
- reuse of existing code and why duplication is avoided;
- relevant open issues, including prerequisites and blockers; and
- migration, fallback, or rollback considerations.

For Rust changes, the agent MUST apply the repository's `rust-best-practices` guidance during this
phase when that skill is available; otherwise it MUST perform the equivalent ownership, trait,
error-handling, async/sync, and compilation-feasibility review directly.

Phase 2 MUST contain a proportionate but explicit risk analysis:

| Required assessment | What to record |
|---|---|
| Files | Number and names of source, test, spec, generated, and configuration files likely to change. |
| Impact area | Every affected area and workflow, including downstream callers and bindings. |
| Module/crate reach | Whether the change is confined to one module; list every crate crossed. |
| Existing-test breakage | Estimated number and names/groups of existing unit tests likely to require adjustment, with rationale. |
| New validation | Reproduction test, new unit/integration tests, and broader commands to run. |
| Behavioural risk | Compatibility, persistence/data, concurrency, performance, security, and error-path effects; mark an item not applicable only with a reason. |
| Recovery | How to revert or disable the change safely. |
| Certainty | Assumptions, ambiguities, open questions, and evidence still missing. |

Critically review Phase 2 twice: first against Phase 1 for scope and acceptance-criteria conformity,
then against the codebase for signature accuracy, reusable functionality, affected call sites,
Rust feasibility, known-issue impact, and understated risk. Fix all resolvable findings before
applying the gate.

### The only approval gate

Unlike `liquers-project`, this simplified workflow has no gate after Phase 1, 3, 4, or 5. The agent
MAY pass the Phase 2 gate and continue automatically only when **all** of these are true:

- the change is localized to one cohesive implementation module;
- at most three existing unit tests are expected to break or require adjustment;
- there is no ambiguity, unresolved design choice, missing evidence, open question, or blocker; and
- the issue remains within the complexity and priority scope authorized in section 1.

The file count alone does not determine locality: colocated tests and required spec/index updates
may be separate files, while a one-file public trait change may still be cross-module in impact.

If any condition is false, the agent MUST present a concise checkpoint containing the core Phase 1
and Phase 2 design, alternatives, quantified risks, file/test/workflow impact, open questions, and
the agent's recommendation. It MUST end with **Reply `proceed` to approve continuation.** The agent
MUST then stop. Only the explicit word `proceed` (case-insensitive) authorizes continuation;
“approved”, “looks good”, “yes”, silence, or feedback without `proceed` does not. After feedback,
revise and re-review Phases 1 and 2, then ask again. Approval authorizes only the scoped proposal.

## 6. Phase 3: Examples, reproduction, and tests

After automatic clearance or explicit approval, the agent MUST complete Phase 3 autonomously.
Choose runnable tests whenever feasible; use conceptual examples only when execution is impossible
and state why. No preference question or approval gate is required. Record an overview of:

- the primary reproduction and expected corrected behaviour;
- a normal/happy path protected against regression;
- relevant error and edge cases;
- unit tests to add or change;
- integration tests when behaviour crosses a public boundary; and
- corner cases involving memory, concurrency, serialization, persistence, or bindings when
  Phase 2 identified those risks.

Tests MUST assert externally meaningful behaviour, not merely mirror the implementation. Liquers
queries used in examples or tests MUST be syntactically valid; referenced commands must be
registered, and resource queries must have an appropriate store. The agent MUST use the
repository's `liquers-unittest` guidance when available and follow
[`UNITTEST_GUIDE.md`](UNITTEST_GUIDE.md) where it applies.

Critically review Phase 3 against Phase 1 acceptance criteria, Phase 2 signatures and risks, and
the repository's existing test conventions. Resolve all fixable discrepancies, then continue
directly to Phase 4.

## 7. Phase 4: Implementation plan and execution

Before editing implementation code, the agent MUST create a concise, executable plan. Each step
must name exact files and symbols, the intended change, dependencies, proof by test or validation,
and rollback or containment for risky work. The plan MUST include implementation, tests, required
spec maintenance, index regeneration, formatting, focused checks, and a proportionate final test.

Review the plan for conformity with Phases 1–3, ordering, codebase compatibility, completeness, and
absence of unrelated work. For Rust changes, apply `rust-best-practices` again to the concrete
steps. Then, without another approval gate, execute the plan:

1. add or update a failing regression test when feasible, then implement the smallest complete fix;
2. follow existing project patterns and avoid speculative refactoring;
3. run formatting and focused tests after relevant steps;
4. run broader crate/workspace checks proportionate to Phase 2's impact;
5. inspect failures rather than weakening tests merely to make them pass;
6. update the plan and risk analysis if implementation reveals new reach; and
7. review the final diff for scope, generated files, debug code, and unrelated edits.

A failing unrelated test does not authorize fixing another issue silently. Record the evidence and
determine whether it blocks confidence in this fix.

When implementation and required validation complete successfully, Phase 4 is complete and the
agent MUST start Phase 5 immediately without asking. If an issue prevents Phase 4 from completing
successfully, the agent MUST stop, report the blocker and partial-work safety, and ask only for the
decision or information needed to resolve it; it MUST NOT present Phase 5 as completed. A discovery
that invalidates the approved scope or risk assessment returns the workflow to section 5.

## 8. Phase 5: Documentation

Phase 5 reconciles the implemented and tested behaviour with repository documentation. For this
simplified issue workflow, assume that no new guide or reference document is needed. Phase 5 MUST:

1. produce a concise current-state documentation summary of the behaviour delivered;
2. review affected existing documentation and the Phase 1 documentation assessment; and
3. make only small necessary updates to existing documents, such as correcting or adding links,
   linking a new or existing design, or changing one or two sentences to keep current facts true.

If truthful documentation would require a new reference or guide, a substantive rewrite, a new
section, or similarly larger work, the agent MUST NOT silently expand this issue. It MUST describe
the proposed documentation need, exact documents and rationale in the Phase 5/closing summary and
ask for permission. That request does not undo a successful Phase 4; it records documentation work
outside this procedure's simplified scope. If permission is granted, perform and review that work
before declaring the overall workflow complete.

There is no Phase 5 approval gate for the summary or permitted small maintenance. After completing
them, continue automatically to repository-record maintenance and pull-request handoff.

## 9. Pull request on successful completion

If implementation, validation, and required in-scope documentation maintenance complete
successfully, the agent MUST commit the change and make the branch available for review through a
pull request. When the environment requires explicit publication, it MUST push the dedicated branch
and create the PR. When a cloud environment provides PR creation as a built-in completion action,
the agent MUST use or clearly hand off to that mechanism and MUST NOT create a duplicate PR. If
neither path is available, report that limitation, the branch name, and the exact next action.

The PR title and description MUST identify the issue, summarize the fix and risk assessment, list
tests and documentation checks, and disclose unresolved validation or proposed documentation work.
A PR MUST NOT be described as a successful handoff while required in-scope work remains incomplete.

## 10. Required specs and documentation maintenance

Repository records MUST comply with `specs/DOCS_STRUCTURE_GUIDE.md`:

- Do not invent or overwrite issue status. GitHub-tracked status is synchronized; status changes
  to pre-existing local issues remain human-owned under the guide's rules.
- If a linked design exists, keep its `DESIGN.md`, phase, status, issue links, PR links, and
  `affects_docs` truthful. Do not create a design merely because this workflow has five phases.
- For any permitted existing reference/guide edit, follow the guide's `reviewed:` and History rules.
- If a distinct problem is discovered, search first and file it as `status: draft` using section
  4.8 of the structure guide. Never create a GitHub issue unless the user separately asks.
- Regenerate `specs/index.csv` with `python scripts/docs_index.py` after tracked-document changes,
  and run `python scripts/docs_index.py --check` before completion.
- Update `specs/README.md` only when the capability map or “When to use what” navigation changed.

Commit issue/spec/design/documentation changes together with the code that makes them true. If a
required in-scope record cannot be updated, implementation is incomplete. Keep substantive
documentation proposed under Phase 5 distinct from missing required record maintenance.

## 11. Closing summary

Always finish with a closing summary, including unsuccessful or partially validated attempts. It
MUST state:

1. **Completion:** whether Phases 1–5, validation, and required specs updates completed. Include the
   branch, commit, PR or built-in PR handoff, and checks actually run.
2. **Documentation:** summarize current documented behaviour and small maintenance performed; list
   any substantive documentation proposal awaiting permission.
3. **Unresolved problems:** what remains, evidence, whether it blocked Phase 4, and whether partial
   changes are safe to keep.
4. **Questions:** decisions or information needed; write “None” when there are none.
5. **Proposed issues:** mark each discovery **required first** or **optional/independent**; state
   “None” when no new issue is warranted.

Do not describe implementation as successful when required tests did not run, required in-scope
specs are stale, or an approval condition was bypassed. Distinguish a verified failure from a check
that could not be run.

## History

| Date | Change | Source |
|---|---|---|
| 2026-08-10 | Aligned the autonomous procedure with the five `liquers-project` phases, retained one post-Phase-2 gate, and constrained Phase 5 to a summary and small existing-document maintenance. | documentation |
| 2026-08-09 | Added the binding four-phase procedure, risk-based approval gate, and successful-completion PR handoff for autonomous S/M issue fixes. | documentation |
