# Documentation Structure Review — 2026-08-08

A review of *how* the project's documentation is organised, not of what it says. Scope: the
tracking artifacts (`specs/ISSUES.md`, `specs/FEATURES/`, the per-feature design folders, the
root-level ad-hoc documents) and the directory structure they live in.

The question this document answers: **what structure would let anyone — human or agent — see the
open issues with their priority, complexity and associated PR/branch in one place, while keeping
closed issues and superseded designs for reference?**

---

## 1. What exists today

### Inventory

| Location | Contents |
|---|---|
| repo root | `README.md`, `CLAUDE.md`, `ISSUES.md` (a stub redirect), `UNITTEST_GUIDE.md`, `EXAMPLE_SCENARIO_1_SUMMARY.md`, `plan20260707.md`, `review20260707.md`, `liquers-designer.skill`, `liquers-unittest.skill` |
| `specs/` (top level) | 33 markdown documents + `command_registry.yaml` (generated) |
| `specs/FEATURES/` | 20 feature briefs + `FEATURES.md` index |
| `specs/<slug>/` | 27 per-feature folders, mostly the `liquers-designer` 5-file skeleton |
| `.claude/skills/` | 4 skills, each with `SKILL.md` + references |

**191 markdown files, ~440,000 words under `specs/` alone.**

### Six overlapping backlogs

Work is tracked, simultaneously, in:

1. `specs/ISSUES.md` — 1,033 lines, ~20 issues
2. `specs/FEATURES/FEATURES.md` — 20 numbered feature briefs
3. `specs/todo20260219.md` — a TODO/FIXME audit with its own status column
4. `plan20260707.md` §5 — work packages WP-1…WP-n
5. `review20260707.md` — findings with an implied priority order
6. 21 × `specs/<slug>/DESIGN.md` — per-feature phase checklists

Nothing reconciles them. `EXPIRATION-SAFETY` appears as a feature brief (`specs/FEATURES/`), as a
design folder (`specs/expiration-safety/`), and as WP-3 in `plan20260707.md` — three records, three
independent statuses.

### GitHub is unused for tracking

- **Issues: 0.** Never used, in the whole history of the repository.
- **PRs: 19**, all closed/merged. Branches follow `claude/<slug>-<suffix>` and the slug matches a
  `specs/<slug>/` folder — so the link between design, branch and PR *exists by convention* but is
  recorded nowhere. Exactly **one** of 21 `DESIGN.md` files names a PR number.

---

## 2. Structural problems

These are consequences of the structure, not of anyone's carelessness. Each one is a place where
the structure permits drift and nothing detects it.

### 2.1 Status lives in prose, so it cannot be aggregated — and it rots

`specs/ISSUES.md` has an `## Open` section. Inside it sit issues whose own `Status:` line says
otherwise:

| Issue | Section | Its own Status line |
|---|---|---|
| `QUERY-ACTION-PARAMETER-LINK-PARSER` | Open | `Resolved (2026-08-06)` |
| `ASSET-MESSAGE-LIFECYCLE-ROBUSTNESS` | Open | `Partially Resolved (WP-2)` |
| `WEBUI-REPAINT-AFTER-SYNC-MUTATION` | Open | `**Resolved** by webui-fixes (2026-07-25)` |
| `webui: async evaluation engine…` | Open | `**Status: Resolved** by async-wasm-refactor` |

Answering "what is actually open?" requires reading 1,000 lines and reconciling two contradictory
signals per issue. The same holds for design folders: nine `DESIGN.md` files say
`**Status:** In Progress`, several of which describe work that shipped and merged weeks ago.

### 2.2 The priority scale is ambiguous, and complexity is absent

Five distinct spellings are in use: `P0 (High)`, `P1 (Medium-High)`, `P2 (Medium)`,
`P3 (Low)`, and a bare `High`. `P1` is glossed as "Medium-High" — so a reader cannot tell whether
`P1` outranks a bare `High`.

**Complexity is not recorded anywhere in `ISSUES.md`.** It appears once, in
`specs/todo20260219.md`, as a `Complexity` column (`MEDIUM`/`HARD`) — a good idea confined to a
single point-in-time audit.

### 2.3 Nothing records the branch or PR

The one fact git already knows perfectly is the one the documents omit. `PR #19` appears in
`ISSUES.md` only as narrative ("Raised by review on PR #19"). There is no field to query, so
"which PR fixed this?" is answerable only by reading commit messages.

### 2.4 Indexes drift silently because nothing checks them

`specs/FEATURES/FEATURES.md` lists 5 files that **do not exist**
(`ASSETS-FIX1-PHASE1-RUNTIME-BLOCKERS.md`, `-PHASE2-METADATA-LIFECYCLE.md`,
`-PHASE3-REFACTOR-API-CLEANUP.md`, `-PHASE4-NICE-TO-HAVE.md`, `-PHASE1-IMPLEMENTATION-PLAN.md`)
and omits 2 that do (`PYTHON-BASIC-OBJECTS.md`, `plan-init-section.md`). Its status vocabulary is
`Open` / `Closed` / `Draft` / `Pending` — four values, undefined, partially overlapping.

Contrast with `specs/command_registry.yaml`, which is generated and guarded by
`cargo test -p liquers-lib --test registry_export`. **That file does not drift.** The project
already knows the fix; it simply has not been applied to the prose.

### 2.5 One flat directory mixes four genres

`specs/`'s 33 top-level documents include, undifferentiated:

- **Living reference** that must be true at HEAD — `PROJECT_OVERVIEW.md`, `ASSETS.md`,
  `REGISTER_COMMAND_FSD.md`, `STORE_CONFIG_FSD.md`, `WEB_API_SPECIFICATION.md`,
  `LANGUAGE-INTEGRATION_GUIDE.md`, `PAYLOAD_GUIDE.md`, the command-library specs
- **Point-in-time records** true only on their date — `todo20260219.md`,
  `DEPENDENCIES_STATUS.md`, `JOBQUEUE_FIX.md`, `PHASE3-*` (4 files),
  `EXAMPLE_SCENARIO_1.md`, `EXAMPLE2-CUSTOM-CONFIG.md`, `QUERY_CONSOLE_ELEMENT_EXAMPLE3.md`
- **Design notes, partly superseded** — `UI_INTERFACE_PHASE1.md` / `PHASE1a` / `PHASE1b`,
  `UI_DIOXUS_DESIGN_NOTES.md`, `UI_RATATUI_DESIGN_NOTES.md`, `PYTHON-WRAPPER-*`
- **Generated data** — `command_registry.yaml`

An agent grepping for "how do assets expire" gets a February status report and a July design with
equal authority. **Nothing marks which documents you are allowed to believe.**

### 2.6 Feature folders are only loosely uniform

The `liquers-designer` skeleton (`DESIGN.md` + `phase1..4`) is followed by 17 folders. The rest
diverge: `context-param-order` and `value-list-support` use `FINDINGS.md`/`SOLUTION.md`,
`metadata-consistency` uses `FINDINGS.md`/`PROPOSED_PLAN.md`, `register_command_enum` uses
`DRAFT.md`/`IMPLEMENTATION_PLAN.md`, `api-docs-analysis` uses `README.md` + `doc-01..04`.
`volatility-system` has grown to 17 files including a `.txt` and a filename containing a space
(`Review-comment-to-Phase 4.md`).

Divergence is not itself wrong — a two-page investigation does not need four phases — but there is
no rule saying which shape applies when, so each new folder is an independent invention.

### 2.7 The root directory has stale duplicates

`liquers-designer.skill` (57 KB) and `liquers-unittest.skill` (10 KB) sit at the repo root and
**differ from** the live `.claude/skills/*/SKILL.md`. `UNITTEST_GUIDE.md` and
`EXAMPLE_SCENARIO_1_SUMMARY.md` likewise duplicate-and-differ from copies under `.claude/` and
`specs/axum-assets-recipes-api/`. Root `ISSUES.md` is a stub whose only content is "this moved".

### 2.8 A 1,000-line shared file is a merge-conflict magnet

Every branch that resolves an issue edits `specs/ISSUES.md`. Two concurrent PRs conflict by
construction. With agents working in parallel sessions this is a recurring, avoidable cost.

---

## 3. Root cause

Three independent axes are collapsed into one directory tree:

| Axis | Values |
|---|---|
| **Genre** | reference · guide · design · issue · historical record · generated |
| **Lifecycle** | proposed · active · done · superseded |
| **Granularity** | issue · feature · work package · phase |

`specs/` sorts by none of them. And because every attribute is prose, the only way to compute a
view ("open P0/P1 items and their PRs") is to re-read everything — which is precisely what does not
happen, so the prose drifts.

---

## 4. Option A — Repo-only, LLM-maintained folder structure with explicit rules

Everything stays in git. Structure is enforced by machine-readable front-matter, a generated index,
and a check that fails the build when they disagree.

### Layout

Keep `specs/` as the root name (renaming to `docs/` would churn 21 code/test references to
`specs/command_registry.yaml` for no benefit) and add genre subdirectories:

```
specs/
  README.md                   # the map: what lives where, what to read first
  RULES.md                    # the placement decision procedure (short)
  command_registry.yaml       # generated — stays put, code references it

  reference/                  # must be true at HEAD; fix or delete, never stale
    PROJECT_OVERVIEW.md  ASSETS.md  ASSET_LIFECYCLE.md
    REGISTER_COMMAND_FSD.md  STORE_CONFIG_FSD.md  WEB_API_SPECIFICATION.md ...

  guides/                     # how-to, stable
    COMMAND_REGISTRATION_GUIDE.md  UNITTEST_GUIDE.md
    LANGUAGE-INTEGRATION_GUIDE.md  PAYLOAD_GUIDE.md ...

  design/<slug>/              # one folder per feature; liquers-designer skeleton
    DESIGN.md (front-matter)  phase1..phase4

  issues/
    INDEX.md                  # GENERATED — the single overview table
    <ID>.md                   # one file per issue, front-matter + body

  archive/                    # point-in-time; read-only by rule, never updated
    2026-02-19-todo-audit.md  2026-07-07-review.md  2026-07-07-plan.md
    2026-02-20-dependencies-status.md ...
```

### The contract: front-matter on every issue and design

```yaml
---
id: ASSET-EXPIRED-CACHED-BINARY-READ
title: Expired asset returns stale cached binary on read
kind: issue                 # issue | feature | design
status: open                # proposed | open | in-progress | blocked | done | wontfix | superseded
priority: P0                # P0 blocker/correctness · P1 important · P2 normal · P3 nice-to-have
complexity: M               # S one file · M one crate · L cross-crate or API change · XL needs design
area: [core/assets]
github: 21                  # issue number, when one exists
pr: [19]                    # PRs that touched it
branch: claude/expired-binary-read-x1y2
design: ../design/expiration-safety/
supersedes: null
created: 2026-07-18
updated: 2026-08-06
---
```

One scale, defined once. `complexity` is a new field and the one that most improves triage: it is
what turns a flat priority list into a plan ("three P1/S items before the P0/XL").

### Enforcement — the part that makes it different from today

1. **`scripts/docs_index.py`** regenerates `specs/issues/INDEX.md` from the front-matter and
   validates it: enum values, required fields, dangling `design:` paths, ID uniqueness.
2. **A freshness test**, exactly like `registry_export`: `docs_index.py --check` fails when
   `INDEX.md` is stale or front-matter is invalid. *Drift becomes a red build instead of a habit.*
3. **A ten-line placement rule in `CLAUDE.md`** — the file every agent already reads. Rules that
   live anywhere else are not read.
4. **`init_feature.py` emits the front-matter**, so new design folders are compliant by default.
5. **Rule: `complexity: L` or `XL` requires a `design:` folder.** This is what ties the issue
   tracker to the design tracker, replacing the current accidental overlap.

### `INDEX.md` — the generated overview

```markdown
## Open

| ID | Pri | Cx | Area | Status | PR / branch | Design |
|----|-----|----|------|--------|-------------|--------|
| ASSET-EXPIRED-CACHED-BINARY-READ | P0 | M | core/assets | open | — | expiration-safety |
| PARAMETER-ESCAPING-INCOMPLETE | P1 | M | core/parse | in-progress | #22 · claude/param-esc | — |
| QUEUED-MANAGER-STARTUP-READINESS | P1 | L | core/assets | open | — | — |

## Closed  (kept for reference)

| ID | Pri | Cx | Resolved | By |
|----|-----|----|----------|-----|
| QUERY-ACTION-PARAMETER-LINK-PARSER | P0 | M | 2026-08-06 | #17 · query-link-parser |
```

### Pros

- **Everything the agent needs is in the checkout.** No API, no auth, no network. Works in
  sandboxes, offline, and under any future tool surface. This matters here: agents are the primary
  consumers of these documents.
- **Greppable and diffable.** "When did this become P0, and why?" is `git log -p`.
- **Status changes are code-reviewed** in the same PR as the fix — they land atomically or not at
  all.
- **No vendor dependency**; survives a move off GitHub intact.
- **One file per issue kills the merge conflicts** on the shared 1,000-line file.
- **Precedent exists.** `command_registry.yaml` proves the generate-plus-check pattern works and is
  trusted in this repo.

### Cons

- **Duplicates what git already knows.** `pr:` and `branch:` are hand-copied facts that go stale —
  the one field class this option cannot keep honest by itself.
- **The script is code to maintain** (~150 lines) and a check that can annoy when it fires on a
  doc-only edit.
- **No board, no notifications, no cross-linking** from a PR back to the issue.
- Discipline is still required for the *body*: nothing detects a `status: open` issue that was
  quietly fixed. The check verifies consistency, not truth.

---

## 5. Option B — GitHub-native

Issues become the tracker. Priority and complexity become labels (`P0`…`P3`, `cx:S`…`cx:XL`); a
GitHub Project gives the board with custom fields and saved views. PRs close issues with
`Closes #N`. `specs/` keeps only reference, guides and design documents.

### Pros

- **PR ↔ issue ↔ branch linkage is free and always correct.** No field to maintain, no drift; the
  timeline shows the branch, the commits, the review and the merge.
- **Labels are real filterable data.** "Open P0 with complexity ≥ L" is a query, not a grep.
- **Closed issues are kept forever**, fully searchable, and never pollute the open view — exactly
  the requirement, with zero effort.
- **Projects gives priority/complexity as typed fields** with a board and roadmap, plus automation
  (auto-close, auto-move on merge).
- **Nothing to build.** No script, no check, no format to enforce.
- Notifications, cross-references, and mentions come along for free.

### Cons

- **The content leaves the repository.** An agent cannot grep issue bodies, cannot diff them,
  cannot read them without network and auth. For a project where the documents *are* the agents'
  working memory across sessions, this is the decisive cost.
- **Design documents do not fit in issues.** `specs/` stays regardless, so there are still two
  homes — and now a boundary to police, which is the problem this review is trying to remove.
- **Status changes are not code review.** Closing an issue is a click, not a reviewed diff.
- **It is a genuine workflow change, not a formalisation of one.** 19 PRs and 0 issues says the
  habit does not exist yet; adopting it means building the habit *and* migrating ~40 records.
- **Lock-in.** The backlog is no longer in the artifact you clone.

---

## 6. Option C — Hybrid (recommended)

**Give every fact exactly one owner, chosen by who can keep it honest.**

| Fact | Owner | Why |
|---|---|---|
| Problem statement, analysis, proposed solution | **Repo** (`specs/issues/<ID>.md`) | Long-form, needs review, must be greppable offline |
| Priority, complexity, area | **Repo** (front-matter) | Editorial judgements — they belong in a reviewed diff |
| Design documents | **Repo** (`specs/design/<slug>/`) | Already there and working |
| Open/closed, branch, PR, merge state | **GitHub** | Git already knows; any copy is a lie waiting to happen |
| The board / roadmap view | **GitHub Projects**, optional | Purely a human convenience |

### How it works

1. Every issue is a file with front-matter (§4), minus `pr:`/`branch:`/`status`-as-truth.
2. When work starts, open a GitHub issue — title + one-line link back to the file. Record its
   number in `github:`. That single number is the only hand-maintained cross-reference.
3. The PR says `Closes #N`. GitHub owns the transition from open to closed.
4. `scripts/docs_index.py` generates `specs/issues/INDEX.md`, reading front-matter from disk **and
   state/PR/branch from the GitHub API** for items that have a `github:` number. The generated
   index is committed, so the offline reader still gets the whole table — just possibly a few days
   stale on the state column, which is the only column git cannot serve.
5. The freshness check runs on front-matter validity always, and on the API-derived columns only
   when a token is available — so the check never breaks an offline or sandboxed build.
6. Items never worked on need no GitHub issue at all. The backlog does not require an account.

### Pros

- **No fact is stored twice**, so nothing can disagree with itself. That is the specific failure
  mode of §2.1–2.4, addressed at the root.
- **Agents read the repo; humans get a board.** Both audiences are served by their native medium.
- **Drift becomes a build failure** for everything the repo owns, and is structurally impossible
  for everything GitHub owns.
- **Incremental and reversible.** Front-matter plus the index deliver most of the value on their
  own; the GitHub half can be adopted per-item, and abandoned without losing anything, because the
  bodies never left the repo.
- The `github:` number is a single integer written once — the cheapest possible cross-reference.

### Cons

- **Two systems, so the ownership rule must be stated crisply and kept short.** A vague boundary
  degenerates back into "record it in both places" — the failure this replaces.
- **The generator has an optional network path**, which means two code paths and a token in CI.
- **The state column can be stale** in a fresh checkout between index regenerations. Mitigated by
  regenerating on merge, but worth naming honestly.
- Still ~150 lines of script to own.

---

## 7. Recommendation

**Option C**, adopted in four steps, each one PR.

| Step | Work | Result |
|---|---|---|
| **1. Split and tag** | `ISSUES.md` → `specs/issues/<ID>.md`, one per issue, with front-matter. Fix the 4 mis-filed resolved issues. Collapse the priority scale to one definition. Add `complexity` to every item. Fold `FEATURES/FEATURES.md` entries in as `kind: feature`. Generate `INDEX.md`. | The overview the review asks for exists, and merge conflicts stop. |
| **2. Sort by genre** | `git mv` only, no content edits: point-in-time documents → `specs/archive/` with dates in the filename; living documents → `reference/` and `guides/`. Delete the stale root `.skill` duplicates and the `ISSUES.md` stub; move `plan20260707.md`, `review20260707.md`, `EXAMPLE_SCENARIO_1_SUMMARY.md`, `UNITTEST_GUIDE.md`. Update the ~15 references in `CLAUDE.md` and the skills. | "Which documents may I believe?" is answered by the path. |
| **3. Enforce** | `scripts/docs_index.py` + freshness check; the placement rule in `CLAUDE.md`; front-matter emitted by `init_feature.py`; a `DESIGN.md` status vocabulary matching the issue one. | Drift becomes a red build. |
| **4. Turn on GitHub issues** | For active work only, from here forward. `github:` in front-matter, `Closes #N` in the PR. | PR/branch/state stop being hand-copied. |

Steps 1–3 are self-contained and deliver the requested overview without touching the workflow.
Step 4 is the only behavioural change and can be deferred or dropped.

### Two things worth doing regardless of the option chosen

- **`specs/README.md` — the map.** 191 files with no entry point means every reader starts by
  guessing. This is the single highest-value document that does not exist.
- **Retire, do not update, the point-in-time trackers.** `todo20260219.md`, `plan20260707.md` §5
  and `review20260707.md` should be frozen in `archive/` and their still-open items promoted into
  the issue set. Six backlogs is the root cause; the fix is subtraction.
