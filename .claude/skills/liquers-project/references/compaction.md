# Backlog compaction

**Triggers:** "compaction", "compact the backlog", "backlog compaction", or the weekly backlog
routine.

Compaction shrinks the open backlog in four ways. It fixes what can be fixed automatically, merges
issues and designs that overlap, makes sure every `S` issue has a design with readiness, and plans
the implementation of everything that is `ready`. It runs interactively, where the user can resolve
decisions on the spot, or unattended as a routine.

## Rules that hold throughout

- [`overlap.md`](overlap.md) decides what merges; its exclusions decide what does not. In particular
  **E1**: never merge an item eligible for automatic fixing into one that is not, if that makes the
  result ineligible. Fix the eligible item and leave the other design as it is.
- [`auto-fix.md`](auto-fix.md) decides what is fixed. Each fix is its own branch and PR
  ([`spin-off.md`](spin-off.md)). Compaction's own commit contains only `specs/` changes.
- Designs are written under `specs/guides/autonomous_bulk_design.md` (via
  [`bulk-design.md`](bulk-design.md)). Finished-Phase-4 designs are not reopened (guide §4), except
  to attach a source under triage case 2, which re-reviews only the changed phases.
- Never edit `specs/archive/`, never open GitHub issues, and never change a human-set `accepted` /
  `rejected` judgement.

## Steps

### 1. Preflight

Work on a dedicated branch (`<prefix>/compaction-YYYY-MM-DD`, or the one the environment assigns).
Regenerate the index (`python3 scripts/docs_index.py`), run `--check`, and fix nothing yet; record
the starting counts.

### 2. Inventory

From `specs/index.csv`:
- open issues and features (`draft`, `accepted`, `in_progress`) with priority, complexity, area and
  design;
- open designs (`draft`, `in_review`, `approved`, in implementation) with phase and readiness;
- the `autofix` column (`eligible`, `not-eligible`, or empty for unassessed). Every unassessed
  readiness-labeled design gets a value in this run.

### 3. Overlap map and merges

Run the overlap tests pairwise within each area and across areas that share symbols (overlap.md,
"Finding candidates"). Cluster strong overlaps. For each cluster:

| Cluster shape | Action |
|---|---|
| Design-less issues only | Create one design owning all of them (triage case 3a) |
| Design-less issue(s) and one open design | Attach the issues to it (triage case 2) |
| Two or more open designs | **Merge proposal.** Merging designs is a maintainer decision (`DOCS_STRUCTURE_GUIDE.md` §5.1.1). Interactive: present it and merge on approval. Unattended: list it in the report |
| Blocked by E1-E5 | Link with `overlaps` / `requires` in both designs, and note the exclusion |
| Duplicate (T4, full cover) | Readiness `covered` pointing at the cover. When it is the same problem, propose `status: duplicate` in the report; changing another item's status is a human's call (`DOCS_STRUCTURE_GUIDE.md` §4.8.3) |

### 4. Design every `S` issue

Every open `S` issue or feature without a design gets one, in the compact form: Phases 1-4 per bulk design, with readiness,
the automatic-fix label, and all questions and decisions collected. Design `M` items too as the run's
budget allows, highest priority first. Re-check readiness of existing designs whose sources or
dependencies changed in step 3.

### 5. Fix what is eligible

Order the eligible, `ready`, `P2`/`P3` items by priority and then age, and fix them as spin-offs (one
branch and one PR each), up to the run's cap (default **5**; the user or routine prompt may change
it). Eligible `P0`/`P1` items are listed for authorization. Skip an item whose design has an open
implementing PR.

Unattended runs use mechanism A or B ([`spin-off.md`](spin-off.md)) when available. Mechanism C works
too, but run the fixes one after another.

### 6. Decisions

Collect every **blocking question**, **open design question** and **proposed resolution** from all
open designs, grouped by design, each with its recommendation and the readiness it would unlock.
Put the ones that unlock the most first: a decision shared by several designs, or one that would make
a design `ready` and eligible.

- **Interactive:** present the list and let the user answer any subset (one at a time with a
  question tool when the host has one, otherwise as a numbered list answered in one reply).
  Record each answer in the design's Phase 1 (the question becomes a resolved decision with date and
  "maintainer"), update the affected phases, re-review them, and re-label readiness and eligibility.
  Items that became eligible go back to step 5.
- **Unattended:** the list goes into the report and nothing is decided.

### 7. Implementation plan for `ready` designs

Write an ordered plan covering every `ready` design that is not being fixed in this run:
- dependency order (`requires` edges, shared change sites that must not run in parallel);
- batches that can run in parallel and those that must not;
- per design: automatic (eligible) or human-gated, size, crate, and the PR it becomes;
- `needs-decision` designs one decision away from `ready`, with that decision named.

### 8. Record and validate

Write the run's report to `specs/archive/YYYY-MM-DD-backlog-compaction.md`: a dated record, never
edited later. It holds the starting and final counts, merges done and proposed, designs created,
fixes started (PR links), the decision list, and the implementation plan. Regenerate and `--check`
the index, review the diff for leakage between designs, commit, and open one **design-only PR** for
the `specs/` changes. Fix PRs are separate.

### 9. Report

The archive file's headline numbers, the PR links, and the decisions (interactive: those still open).

## Weekly routine

Compaction is designed to run as a scheduled routine. Creating one is an outward action: offer it and
create it only when the user asks, using the host's scheduler (Claude Code: a routine that starts a
fresh session per run; elsewhere a scheduled CI job or cron calling the agent). Suggested prompt:

```text
Run liquers-project backlog compaction (unattended) on orest-d/liquers.
Follow .claude/skills/liquers-project/references/compaction.md. Fix cap: 5.
Do not merge existing designs and do not decide open questions: list them in the report.
Open one design-only PR for specs/ changes, and one PR per automatic fix.
Finish with the report: counts, PR links, merge proposals, decisions needed.
```

Weekly is the intended cadence. If successive runs find nothing new, say so in one line and change
nothing. An empty compaction PR is noise.
