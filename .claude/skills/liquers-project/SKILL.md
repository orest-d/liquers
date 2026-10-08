---
name: liquers-project
description: The Liquers design and backlog workflow. Five gated phases for substantial projects (high-level design, architecture, examples and tests, implementation plan and execution, documentation), plus issue triage and filing with design overlap, spin-off of problems found while fixing an issue, automatic-fix eligibility, bulk design and backlog compaction. Use for new value types, command libraries, storage backends, UI components, API endpoints, cross-crate changes, explicit project-phase requests ("design a new…", "start Phase 1…", "review Phase 2…"), filing or triaging an issue, a problem discovered while fixing another issue, "bulk-design" / "bulk design", and "compaction" / "backlog compaction" / the weekly backlog routine. Supersedes the removed liquers-designer skill. Not for an isolated command or a one-line fix with nothing to file.
---

# Liquers Project

Designs, implements and documents Liquers changes in five phases, and keeps the issue and design
backlog small. Paths below are relative to this file. Repository rules (`CLAUDE.md`) and the
documentation contract (`specs/DOCS_STRUCTURE_GUIDE.md`) apply throughout and are not restated here.

The removed `liquers-designer` skill is not to be used or recreated. Designs it created keep their
four-phase contract; continue them here without adding `workflow: liquers-project` unless the user
adopts the five-phase contract.

## Modes

| Mode | Triggers | Procedure |
|---|---|---|
| **Project** | "design a new…", "plan implementation of…", "architect the…", "start/review Phase N" | This file |
| **Issue triage and filing** | Any new problem, gap or feature, reported or found in passing | [`references/issue-triage.md`](references/issue-triage.md) |
| **Fixing an issue** | "fix <ISSUE-ID>", an autonomous fix | `specs/guides/autonomous_issue_fixing.md` + [`references/spin-off.md`](references/spin-off.md) |
| **Bulk design** | "bulk-design", "bulk design" (any case) | [`references/bulk-design.md`](references/bulk-design.md) |
| **Compaction** | "compaction", "compact the backlog", "backlog compaction", the weekly routine | [`references/compaction.md`](references/compaction.md) |

Shared definitions, one owner each: **overlap** [`references/overlap.md`](references/overlap.md);
**automatic-fix eligibility** [`references/auto-fix.md`](references/auto-fix.md); **readiness**
`DOCS_STRUCTURE_GUIDE.md` §5.1.1 and `specs/guides/autonomous_bulk_design.md` §3.

**The backlog rule.** A problem found while working must not end as a filed-and-forgotten issue.
Triage it (overlap first). Fix it now, in a separate branch or session, when it is eligible for
automatic fixing; otherwise give it a design with readiness and its decisions listed. Filing alone
is the last resort, and the closing summary says why.

## Design depth

Choose the form from the source issue's `complexity` (`DOCS_STRUCTURE_GUIDE.md` §4.5) when the
design starts:

| Complexity | Form | Files | Review |
|---|---|---|---|
| `S`, `M` | **compact** (`form: compact` in `DESIGN.md`) | `DESIGN.md` only; each phase is a `## Phase N` section ([`references/compact-design-template.md`](references/compact-design-template.md)) | One review pass per phase, by the author |
| `L`, `XL`, or no source | **full** | `DESIGN.md` + one file per phase ([`references/phase1-template.md`](references/phase1-template.md) … `phase5-documentation.md`) | Up to two independent reviewers per phase |

Wherever this skill or a guide says "the Phase N document", a compact design means its
`## Phase N` section. If a compact design grows to `L`, convert it to the full form: move each
section into its phase file, drop `form: compact`, and say so in Phase 1's scope changes.

**Size limits.** Phase 1: one page (the problem example and readiness section excluded). Phases 2-4:
three pages each. Phase 5: three pages. A compact design: three pages for `S`, five for `M`. Refer to
code by path and symbol (`liquers-core/src/assets.rs` `AssetManager::get`); paste only new or
changed signatures, never existing code. A design that cannot fit is two designs.

## Phases

| # | `phase:` | Purpose | Must contain | Apply |
|---|---|---|---|---|
| 1 | `high-level` | What and why | Purpose; problem example; acceptance scenarios (`AC-<n>`, WHEN/THEN); systems touched; crate placement; documentation intent; open questions | Overlap triage first |
| 2 | `architecture` | How | Chosen solution and rejected alternatives; known-issue preflight; new and changed interfaces; integration points; errors; sync/async; commands; documentation architecture; risks | `rust-best-practices` |
| 3 | `examples` | Proof | Overview table; primary and secondary example; edge and error cases; test plan with exact test names, each citing its scenarios | `liquers-unittest`, `liquers-validate` |
| 4 | `implementation` | Steps | Progress checklist; ordered steps with files, symbols, validation command and rollback; testing plan; documentation updates | `rust-best-practices` |
| 5 | `documentation` | Record | What was built versus approved; documents created and reviewed; issues closed and filed; learning | — |

Phase-specific rules:

- **Phase 1** states the problem on a concrete example: input, query, call or scenario; what happens
  today; what should happen. Validate any query in it with `liquers-validate`. Acceptance criteria
  are scenarios: `- **AC-<n>** <name>` with a WHEN line and a THEN line (`DOCS_STRUCTURE_GUIDE.md`
  §5.2.1); the problem example is usually AC-1. Ids are never renumbered or reused.
- **Phase 2 known-issue preflight:** check open issues linked to the design or touching its areas
  and integration points. For each relevant one record whether it must be fixed first and whether it
  blocks. A blocker is resolved first or designed around; it is at least `P1` (`P0` only when it meets
  §4.4). List the command namespaces involved (e.g. `pl`, `lui`); the user confirms them at the gate.
- **Phase 3** uses runnable tests by default. Use conceptual examples only where execution is
  impossible, and say why. Every scenario is cited (`AC-<n>`) by at least one test, and no test
  cites an undefined one; `validate_phase.py` and `docs_index.py --check` enforce this whenever
  Phase 1 defines scenarios.
- **Phase 4** opens with a `## Progress` checklist, one item per step (compact: the `### Steps`
  checklist). During implementation tick a step when its proof passes and append the commit.
  Ticking is progress, not a design change, and needs no re-approval. A resumed session or a
  spin-off starts at the first unticked step; Phase 5 starts only when every item is ticked or its
  remainder is an issue. The Phase 4 gate offers execution options: execute now, create a task
  list, revise, or exit (user implements; Phase 5 stays outstanding).
- **Phase 5** starts when the implementation is validated and every user and review comment is
  answered, normally in the same PR before merge. Write the summary
  ([`references/phase5-documentation.md`](references/phase5-documentation.md)); create the planned
  reference and guide documents; review every document in `affects_docs` against the implemented
  behaviour (`DOCS_STRUCTURE_GUIDE.md` §9); update `specs/README.md` links; set every completed issue
  to `closed` / `closed_not_planned` with a resolution note (§4.3); list every problem discovered
  during the work with its triage and spin-off outcome. After approval set `status: complete` and
  remove `phase`.

`DESIGN.md` bookkeeping: set `phase:` at every transition and `status:` per §5.1 (`draft` while
writing, `in_review` at a gate, `approved` after it). A design with `gh_pr` carries no derived status
(§5.5). Designs created in this mode carry `workflow: liquers-project`, which makes Phase 5
mandatory. Create the folder with `scripts/init_feature.py <slug>` (add `--compact` for `S`/`M`).

## Reviews

Every phase ends with a review against [`references/review-checklist.md`](references/review-checklist.md)
and `scripts/validate_phase.py <slug> <N>`, which handles both forms. The review covers four
concerns:

| Concern | Question |
|---|---|
| **Conformity** | Does this phase do what the earlier phases decided, without scope drift? |
| **Codebase** | Do the named files, symbols and signatures exist as described? Is there existing code that does this already? |
| **Soundness** | Will it compile and is it idiomatic (`rust-best-practices`)? Are the queries and tests valid (`liquers-validate`, `liquers-unittest`)? |
| **Records** | Are front-matter, links, readiness and documentation plans truthful? |

- **Compact form:** the author checks all four concerns in one pass.
- **Full form:** when the host can run independent agents, use at most two reviewers per phase: one
  for conformity and records, one for codebase and soundness. Give each the phase document, the
  earlier phases and the files it names. The author fixes what they find. When the host has no
  agents, the author runs the two passes one after the other.
- **Before the Phase 4 gate (both forms):** one consistency pass over all phases together. Look for
  contradictions, acceptance criteria without tests, steps without proof, and stale assumptions.
- Ask the user only for decisions that the repository cannot answer. Fix everything else directly.

## Approval gates

After each phase, present the result and its open questions, then stop. **Only the word `proceed`
(case-insensitive) advances.** "Looks good", "ok", "yes" and silence do not. After feedback, revise,
review again, and wait again.

At the **Phase 2 gate**, offer both continuation modes:

> Reply **`proceed`** to continue with an approval gate after each remaining phase, or
> **`proceed all`** to pre-approve the remaining phases. With pre-approval I work through Phases 3
> and 4 without stopping, collect every problem and decision I meet, and stop before implementation
> only if one of them needs your decision. Either way you see all open questions and decisions
> before implementation starts.

**Pre-approval (`proceed all`)** covers Phases 3 and 4, the implementation and Phase 5. Reviews still
run. The user-facing questions go into a decision log instead of stopping work:

1. Keep the log in Phase 1's `## Design Readiness` section, tiered as in `autonomous_bulk_design.md`
   §3. Where only the user can decide, take the recommended answer as an explicit assumption, finish
   the phase, and mark it **needs decision**.
2. **Pre-implementation stop:** after the Phase 4 review, present the whole log. If any item is
   blocking or needs a decision, stop and ask for `proceed`. Otherwise show the list (or "None") and
   implement.
3. A discovery during implementation that changes an approved contract voids the pre-approval:
   stop and return to the gate of the earliest affected phase.
4. Phase 5 runs without a gate, but its results are presented before `status: complete` is set.
5. Record `Pre-approved after Phase 2 on YYYY-MM-DD` in `DESIGN.md` so a resumed session knows.

## Problems found during the work

Every defect, gap or limitation noticed in passing goes through
[`references/issue-triage.md`](references/issue-triage.md) at once, then through the spin-off decision
in [`references/spin-off.md`](references/spin-off.md):

- **Belongs to the design in hand:** extend this design (triage case 1); no new issue.
- **Eligible for automatic fixing:** fix it now in its own branch and PR (a child session when the
  host can create one, otherwise a second branch), never in the current PR.
- **Not eligible:** file it with a design and readiness (triage cases 2-3).

When a design ships only in part, the remainder becomes an issue (§5.6). The issue file itself is
always created with `DOCS_STRUCTURE_GUIDE.md` §4.8.

## Host compatibility

This directory is the one implementation for every host (Claude Code, Codex, GitHub Copilot, Zed and
others). Host adapters such as `.agents/skills/liquers-project/` point here and must not redefine it.

- Read `CLAUDE.md` on every host, and `AGENTS.md` too when the host supplies one.
- Run the scripts with whatever Python launcher the host has (`python3`, `python`, `py -3`).
- Generated artifacts are identical on every host. Never write host names or model IDs into them.
- If an auto-applied skill is not registered on the host, read its `SKILL.md` under
  `.claude/skills/` and follow it directly.
- Reviewer and spin-off mechanisms depend on the host. When a host lacks one, do the same work
  sequentially; never change the artifact format to work around a host limitation.

## Files

| File | Use |
|---|---|
| `references/phase{1,2,3,4}-template.md`, `references/phase5-documentation.md` | Full-form phase skeletons |
| `references/compact-design-template.md` | Compact `DESIGN.md` |
| `references/review-checklist.md` | Per-phase review questions |
| `references/issue-triage.md`, `overlap.md`, `auto-fix.md`, `spin-off.md`, `bulk-design.md`, `compaction.md` | Backlog modes |
| `scripts/init_feature.py`, `scripts/validate_phase.py` | Create and validate a design folder |
