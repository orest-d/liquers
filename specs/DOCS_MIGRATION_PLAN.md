# Documentation Migration Plan

How the documentation that exists today reaches the structure defined in
`DOCS_STRUCTURE_GUIDE.md`. Every current document has a disposition here, along with the edits
required to `CLAUDE.md`, `README.md`, the skills, and the ~50 source files that reference spec
paths.

**This document is transitional.** When the last step lands it moves to
`specs/archive/2026-08-08-docs-migration-plan.md` and stops being maintained.

---

## 1. Rules for the migration itself

1. **`git mv`, never copy-and-delete.** History follows the file; a copy loses it.
2. **No content rewriting in a move PR.** Moves and edits are separate commits, so a reviewer can
   read `git log --follow` and see that nothing changed in transit. The one exception is adding
   front-matter, which is additive and reviewable at a glance.
3. **Every PR leaves the tree consistent.** `cargo test -p liquers-lib --lib --tests` passes at
   every step, and no step leaves a dangling reference — path rewrites land in the same PR as the
   move that breaks them.
4. **Statuses are assigned, not inferred silently.** Where this plan proposes a status it says so,
   and §9 lists what a human must confirm. A migration that guesses is how the current drift
   started.

---

## 2. Blast radius: spec paths are referenced from code

This is the finding that shapes the ordering, and the one most likely to derail the work halfway
through if it is discovered late.

**86 references to `specs/` paths exist across 50 files** — Rust sources, integration tests,
Playwright specs, example READMEs, `CLAUDE.md`, `README.md`, the skills and
`scripts/check-build-matrix.sh`.

| Target | References |
|---|---|
| `specs/<design-slug>/…` | 45 |
| `specs/ISSUES.md` | 15 |
| Top-level `specs/SCREAMING_NAME.md` | ~25 |

So a "documentation-only" reorganization touches `liquers-core/src/parse.rs`,
`liquers-axum/src/assets/handlers.rs`, `liquers-web/tests/eval_EVAL.rs` and forty-odd others.
That is fine — the rewrites are mechanical — but each move step must carry its `sed` and be
reviewed as a code change, not waved through as docs.

> **Decision available here.** Moving 27 design folders into `specs/design/` costs 45 reference
> rewrites. Leaving them at `specs/<slug>/` costs nothing and is a legitimate choice — the folders
> are already visually distinct from the SCREAMING top-level documents. **Recommendation: move
> them.** The rewrite is one `git grep -l | xargs sed` executed once, and afterwards the top level
> of `specs/` holds six entries instead of sixty. But if the migration needs to be smaller, this
> is the part to drop, and §6 becomes a front-matter-only step.

---

## 3. Step 1 — Scaffold and contract

No files move. This step exists so that later steps have somewhere to move things to and a
contract to move them under.

- Add `specs/DOCS_STRUCTURE_GUIDE.md` *(already written)*.
- Add `specs/DOCS_MIGRATION_PLAN.md` *(this file)*.
- Create `specs/reference/`, `specs/guides/`, `specs/design/`, `specs/issues/`, `specs/archive/`
  (each with a `.gitkeep` until populated).
- Add `specs/README.md` as a stub: the five-line preamble plus empty generated-block markers.
- Move `specs/DOCS_STRUCTURE_REVIEW.md` → `specs/archive/2026-08-08-docs-structure-review.md`.
  It is a point-in-time analysis and its §2.7 contains an error (see §10).

**Verification:** `cargo test -p liquers-lib --lib --tests` unaffected. Nothing references the new
directories yet.

---

## 4. Step 2 — Issues

`specs/ISSUES.md` (1,033 lines) becomes one file per issue under `specs/issues/`.

### 4.1 Dispositions

Priorities come from the file. **Complexity is proposed here and must be confirmed** — it has never
been recorded, so every value below is an estimate from the issue text, not a measurement.

| Issue ID (unchanged) | Status | Pri | Cx (proposed) | Area | Design |
|---|---|---|---|---|---|
| `ASSET-EXPIRED-CACHED-BINARY-READ` | `accepted` | P0 | M | `core/assets;core/store` | `expiration-safety` |
| `PARAMETER-ESCAPING-INCOMPLETE` | `accepted` | P1 | M | `core/query` | — |
| `QUEUED-MANAGER-STARTUP-READINESS` | `accepted` | P1 | M | `core/assets` | — |
| `VOLATILE-KEYED-RECIPE-SELF-DELEGATION` | `accepted` | P1 | M | `core/assets` | — |
| `ASSET-MESSAGE-LIFECYCLE-ROBUSTNESS` | `accepted` | **P1** | L | `core/assets` | `wp2-terminal-outcome` |
| `QUERY-BUILDER-TOOLING` | `accepted` | P2 | L | `core/query;core/validate` | **needed** |
| `EXPIRATION-RECOVERY-WEB-API` | `accepted` | P2 | M | `axum;core/assets` | — |
| `PLAN-EXCESS-ACTION-PARAMETERS-DROPPED` | `accepted` | P2 | M | `core/plan` | — |
| `WEBUI-QUERY-CONSOLE-ENTER-KEY-SUBMIT` | `accepted` | P2 | M | `lib/ui` | `ui-events` |
| `WEBUI-SUBMIT-QUERY-STATE-NOT-PRESERVED` | `accepted` | P2 | M | `lib/ui` | `ui-events` |
| `POST-INIT-COMMAND-REGISTRATION` | `accepted` | P3 | M | `core/commands;web` | — |
| `LANGUAGE-EXCEPTION-FIELDS-LOST-IN-TRANSPORT` | `accepted` | P3 | M | `core/error;web;py` | — |
| `WEB-CANCELLATION-INERT` | `accepted` | P3 | M | `web` | — |
| `QUERY-ACTION-PARAMETER-LINK-PARSER` | `closed` | P0 | M | `core/query` | `query-link-parser` |
| `PAYLOAD-NESTED-EVALUATION-INHERITANCE` | `closed` | P0 | L | `core/plan;core/assets` | `payload-nested-evaluation-inheritance` |
| `WEBUI-REPAINT-AFTER-SYNC-MUTATION` | `closed` | P2 | M | `lib/ui` | `webui-fixes` |

`status_source` is `local` for all sixteen; none has ever had a GitHub issue. The three `closed`
rows carry `gh_pr` recovered from git history (#17, #14, #12 respectively).

`ASSET-MESSAGE-LIFECYCLE-ROBUSTNESS` currently reads `Priority: High` with no P-number, the one
entry outside the scale. Mapped to **P1**; confirm.

### 4.2 The two entries that are not issues

`specs/ISSUES.md` contains two `##`-level sections that no index scan would ever find, because
every other entry is `### Issue:`.

| Current | Becomes |
|---|---|
| `## webui: async evaluation engine does not run on wasm (browser)` (Resolved) | `specs/issues/WEBUI-ASYNC-ENGINE-WASM.md` — `closed`, P0, XL, `core/assets;web`, design `async-wasm-refactor` |
| `## async-wasm-refactor follow-ups (out of scope, tracked)` | **Split into two.** `CORE-TOKIO-REMOVAL` (`accepted`, P3, XL, `core/assets`, design needed) and `WEB-NATIVE-IO-TIER2` (`accepted`, P3, L, `web`, design needed). The trailing note about `ui-events` folds into the two `WEBUI-*` issues that already reference it. |

### 4.3 One issue that exists only in a folder

`.claude/skills/rust-best-practices/references/anti-patterns.md:228` and
`.claude/skills/liquers-designer/references/liquers-patterns.md:166` both point at
`specs/ISSUES.md` for the *context-parameter-last* requirement — **which is not in `ISSUES.md`.**
The material lives in `specs/context-param-order/{FINDINGS,SOLUTION}.md`.

Create `specs/issues/COMMAND-CONTEXT-PARAM-ORDER.md` from that folder's findings, and point both
skill references at it. Status and priority to be set at review; the folder stays as a design.

### 4.4 Retiring the old file

- `specs/ISSUES.md` → **deleted.** Its content is now sixteen-plus files; a stub redirect would be
  a third thing to keep honest, and the root `ISSUES.md` stub is the cautionary example.
- Root `ISSUES.md` (a stub pointing at `specs/ISSUES.md`) → **deleted.**

### 4.5 Rewriting the 15 code references

Eleven of the fifteen already name their issue ID, so the rewrite is nearly mechanical —
`` `ID` in `specs/ISSUES.md` `` becomes `` `specs/issues/ID.md` ``:

| File:line | Names an ID? |
|---|---|
| `liquers-web/src/encode.rs:10`, `:103` | `PARAMETER-ESCAPING-INCOMPLETE` |
| `liquers-web/src/asset.rs:27` | `WEB-CANCELLATION-INERT` |
| `liquers-web/tests/async_commands_ASYNCCMD.rs:131` | `WEB-CANCELLATION-INERT` |
| `liquers-web/tests/eval_EVAL.rs:186` | `WEB-CANCELLATION-INERT` |
| `liquers-web/tests/async_ASYNCQ.rs:127` | `WEB-CANCELLATION-INERT` |
| `liquers-core/tests/injection.rs:650` | `PAYLOAD-NESTED-EVALUATION-INHERITANCE` |
| `liquers-core/tests/payload_inheritance.rs:98` | `VOLATILE-KEYED-RECIPE-SELF-DELEGATION` |
| `.claude/skills/liquers-validate/SKILL.md:117` | `PLAN-EXCESS-ACTION-PARAMETERS-DROPPED` |
| `.claude/skills/liquers-validate/references/output-format.md:139` | `PLAN-EXCESS-ACTION-PARAMETERS-DROPPED` |

**Four need a human to identify the issue** — they say "see `specs/ISSUES.md`" and nothing more:

| File:line | Context | Likely |
|---|---|---|
| `liquers-core/src/parse.rs:293` | "out of scope here. Tracked in …" | `PARAMETER-ESCAPING-INCOMPLETE` |
| `liquers-core/src/command_metadata.rs:702` | "deliberately **not** implemented; see …" | unclear — confirm |
| `liquers-core/tests/payload_inheritance.rs:3` | "Covers the behaviour introduced for …" | `PAYLOAD-NESTED-EVALUATION-INHERITANCE` |
| `.claude/skills/rust-best-practices/references/anti-patterns.md:228` | context-last requirement | `COMMAND-CONTEXT-PARAM-ORDER` (§4.3) |

`CLAUDE.md:20` (`**Known issues** are tracked in specs/ISSUES.md`) → `specs/issues/`, with a
pointer to `index.csv`. Handled in Step 6.

---

## 5. Step 3 — Feature briefs

`specs/FEATURES/` becomes issues with `kind: feature`. The briefs are substantial documents; each
becomes the body of its issue file, unchanged.

| Brief | New ID | Status | Pri | Cx | Area |
|---|---|---|---|---|---|
| `ASSETS-FIX1.md` | `ASSETS-FIX1` | `draft` | P1 | XL | `core/assets` |
| `ASSETS-IMPROVEMENTS.md` | `ASSETS-IMPROVEMENTS` | `draft` | P2 | L | `core/assets;core/store` |
| `ASSETS-IMPROVEMENTS-ISSUE4-IMPLEMENTATION-PLAN.md` | — | → `archive/` | | | |
| `BENCHMARK-SUITE.md` | `BENCHMARK-SUITE` | `draft` | P3 | M | `build` |
| `COMBINED-EXPIRES.md` | `COMBINED-EXPIRES` | `draft` | P2 | L | `core/assets` |
| `COMBINED-VALUE-DISCRIMINATION.md` | `COMBINED-VALUE-DISCRIMINATION` | `draft` | P2 | M | `core/value;lib/value` |
| `COMMAND-METADATA-ENHANCEMENTS.md` | `COMMAND-METADATA-ENHANCEMENTS` | `draft` | P2 | L | `core/commands;macro` |
| `EGUI-ASSET-MANAGER-INTEGRATION.md` | `EGUI-ASSET-MANAGER-INTEGRATION` | `draft` | P2 | M | `lib/egui` |
| `EGUI-VALUE-RENDERING.md` | `EGUI-VALUE-RENDERING` | `closed` | P2 | M | `lib/egui` |
| `EXPIRATION-SAFETY.md` | `EXPIRATION-SAFETY-FEATURE` | `closed` | P0 | L | `core/assets` |
| `EXPIRATION-SAFETY-IMPLEMENTATION-PLAN.md` | — | → `archive/` (marked superseded) | | | |
| `EXTENDED-FAST-TRACK.md` | `EXTENDED-FAST-TRACK` | `draft` | P2 | L | `core/assets` |
| `IMAGE-SERIALIZATION-FEATURE-GAPS.md` | `IMAGE-SERIALIZATION-FEATURE-GAPS` | `closed` | P2 | M | `lib/image;lib/value` |
| `KEY-LEVEL-ACL.md` | `KEY-LEVEL-ACL` | `draft` | P2 | L | `core/store;axum` |
| `POLARS-FEATURE-GAPS.md` | `POLARS-FEATURE-GAPS` | `accepted` | P2 | M | `lib/polars` |
| `PYTHON-BASIC-OBJECTS.md` | `PYTHON-BASIC-OBJECTS` | **confirm** | P2 | L | `py` |
| `SCHEDULER-IMPROVEMENTS.md` | `SCHEDULER-IMPROVEMENTS` | `draft` | P1 | L | `core/assets` |
| `TECHNICAL-DEBT-1.md` | `TECHNICAL-DEBT-1` | `accepted` | P2 | L | `core/value;core/assets` |
| `VALUE-DESCRIPTION.md` | `VALUE-DESCRIPTION` | `draft` | P3 | M | `core/value;lib/value` |
| `plan-init-section.md` | — | → `archive/` (a fragment, not a feature) | | | |

Notes:

- **`EXPIRATION-SAFETY` collides** with the design folder of the same name. The feature brief
  becomes `EXPIRATION-SAFETY-FEATURE`; the design keeps `EXPIRATION-SAFETY`. This is the one
  ID collision in the whole set, and it is a direct symptom of the same work being tracked in two
  systems.
- **`POLARS-FEATURE-GAPS`** reads `Status: Partially Implemented`, which the new vocabulary does
  not have and deliberately does not add. Split it: close what shipped, keep the remainder as
  `accepted`. Partial states are how a tracker stops being queryable.
- **`PYTHON-BASIC-OBJECTS`** reads `Draft`, but PR #2 implemented it. Verify against `liquers-py`
  and set `closed` or `accepted` accordingly.
- `specs/FEATURES/FEATURES.md` → **deleted**, superseded by `index.csv`. Its five phantom entries
  (`ASSETS-FIX1-PHASE1-RUNTIME-BLOCKERS.md` and four siblings) name files that have never existed;
  there is nothing to migrate.
- `specs/FEATURES/` is removed once empty.

---

## 6. Step 4 — Design folders

27 folders move to `specs/design/<slug>/` and gain front-matter in `DESIGN.md`. Slugs are
unchanged. Six folders lack a `DESIGN.md` in the standard shape and get one — the existing
documents stay under their current names, since §5 of the guide requires only `DESIGN.md`, not the
four-phase skeleton.

### 6.1 Proposed statuses

Read from the current `**Status:**` lines and cross-checked against `ISSUES.md` and the merged PRs.
**Nine currently say "In Progress" while describing work that shipped** — those are the rows most
in need of confirmation.

Under the phase set in force at migration time (`high-level`, `architecture`, `examples`,
`implementation` — all four required), a design whose code has shipped has no phase outstanding, so
it maps to **`complete`**, not `implemented`. `implemented` describes the narrower state "code
merged, some phase still owed", which does not yet arise because no post-implementation phase
exists. It will the moment a documentation phase is added.

The `gh_pr` column below is written into `DESIGN.md` front-matter by hand, once, during migration —
recovered from merged PR branch names, which match design slugs throughout the repository's
history. From then on `--sync` derives the status from those PRs (guide §5.5) and the `status:`
field is removed from the file. **Every migrated design with a merged PR therefore ends up with no
hand-written status at all**, which is the point: the nine folders that currently claim "In
Progress" about shipped work cannot make that mistake again.

| Slug | Current text | Proposed | Phase | Confirm? |
|---|---|---|---|---|
| `async-wasm-refactor` | In Progress | `complete` | — | ISSUES.md says resolved 2026-07-23 |
| `axum-assets-recipes-api` | In Progress | `complete` | — | **yes** |
| `dependency-management` | In Progress | `complete` | — | **yes** (PR #5/#6) |
| `dependency-scheduling` | In Progress | `complete` | — | **yes** (PR #6) |
| `expiration-mechanism` | In Progress | `complete` | — | **yes** |
| `expiration-monitor-assetref` | In Progress | `complete` | — | **yes** (PR #9) |
| `expiration-safety` | Complete | `complete` | — | PR #11 |
| `keyboard-shortcuts` | ✓ Implemented | `complete` | — | |
| `liquers-web` | Implementation complete — M1-M6 ✅ | `complete` | — | PR #19 |
| `menu-pane-layout` | In Progress | ? | ? | **yes** |
| `payload-nested-evaluation-inheritance` | In Progress | `complete` | — | PR #14; issue resolved |
| `query-console-element` | In Progress | ? | ? | **yes** |
| `query-link-parser` | Complete — designed, implemented, tested | `complete` | — | PR #17 |
| `query-validation` | Complete — designed, implemented, tested | `complete` | — | PR #15 |
| `ui-events` | Phase 1 drafted — awaiting review | `in_review` | `high-level` | |
| `volatility-system` | In Progress | ? | ? | **yes** |
| `web-api-library` | ✅ Complete | `complete` | — | |
| `webui` | Implemented; browser runtime is a tracked follow-up | `complete` | — | PR #10 |
| `webui-fixes` | Implementation complete (2026-07-25) | `complete` | — | PR #12 |
| `wp2-terminal-outcome` | In Progress | ? | ? | **yes** — issue says "Partially Resolved (WP-2)" |
| `liquers-wf` | *(phase 1 only)* | `draft` or `in_review` | `high-level` | |
| `value-accessor` | *(phase 1 only)* | `draft` or `in_review` | `high-level` | |

Cheap confirmation for the flagged rows: check whether the type or function the design introduces
exists at HEAD, and whether the design's slug appears in a merged PR branch name. For the four rows
whose phase is unknown, the highest-numbered phase file that is more than a template stub gives the
answer.

`liquers-wf` and `value-accessor` have only `phase1-high-level-design.md`, which does not by itself
say whether that phase is being written (`draft`) or is waiting on a reviewer (`in_review`). Set
both to `draft` unless someone remembers otherwise — it is the state that invites work rather than
implying someone else owes a response.

### 6.2 Non-conforming folders

| Slug | Contains | Action |
|---|---|---|
| `context-param-order` | `FINDINGS.md`, `SOLUTION.md` | Add `DESIGN.md`; link to `COMMAND-CONTEXT-PARAM-ORDER` (§4.3) |
| `metadata-consistency` | `FINDINGS.md`, `PROPOSED_PLAN.md` | Add `DESIGN.md` |
| `value-list-support` | `FINDINGS.md`, `SOLUTION.md` | Add `DESIGN.md` |
| `register_command_enum` | `DRAFT.md`, `IMPLEMENTATION_PLAN.md` | Add `DESIGN.md`; rename slug to `register-command-enum` (the only underscore slug) |
| `api-docs-analysis` | `README.md`, `doc-01`…`doc-04` | **Not a design.** The four `doc-0*` files are reference material → `specs/reference/api/`; the `README.md` gap analysis → `archive/`. `README.md:44` of the root README points here — update it. |
| `volatility-system` | 17 files incl. `VALIDATION_SUMMARY.txt` and `Review-comment-to-Phase 4.md` | Add front-matter. Rename the file containing a space. Leave the rest — a large folder is not a problem, an unreadable one is. |

### 6.3 Path rewrites

45 references. One pass, verified by a `git grep` that returns nothing afterwards:

```bash
git grep -l 'specs/' -- '*.rs' '*.ts' '*.md' '*.sh' '*.py' \
  | xargs sed -i -E 's#specs/(async-wasm-refactor|axum-assets-recipes-api|context-param-order|dependency-management|dependency-scheduling|expiration-mechanism|expiration-monitor-assetref|expiration-safety|keyboard-shortcuts|liquers-web|liquers-wf|menu-pane-layout|metadata-consistency|payload-nested-evaluation-inheritance|query-console-element|query-link-parser|query-validation|register_command_enum|ui-events|value-accessor|value-list-support|volatility-system|web-api-library|webui|webui-fixes|wp2-terminal-outcome)/#specs/design/\1/#g'
```

Note `specs/liquers-web/` (a design folder) versus the `liquers-web` crate — the pattern anchors on
`specs/`, so crate paths are untouched. `scripts/check-build-matrix.sh` and
`liquers-lib/examples-web/tests/*.spec.ts` are among the files affected; both are executed, so a
mistake here surfaces immediately.

---

## 7. Step 5 — Top-level and root documents

### 7.1 `specs/` top level — all 33 documents

**→ `reference/`** (must be true at HEAD)

| File | Note |
|---|---|
| `PROJECT_OVERVIEW.md` | 4 references; `audience: internal` |
| `ASSETS.md`, `ASSET_LIFECYCLE.md`, `ASSET_SET_OPERATION.md` | |
| `REGISTER_COMMAND_FSD.md`, `STORE_CONFIG_FSD.md`, `UI_INTERFACE_FSD.md` | |
| `WEB_API_SPECIFICATION.md` | `audience: both` — an API contract serves users and implementers |
| `POLARS_COMMAND_LIBRARY.md`, `IMAGE_COMMAND_LIBRARY.md` | `audience: user` — command surfaces users call. Most likely to migrate to `docs/` later; tagging them now makes that a filter |
| `UI_PAYLOAD_DESIGN.md` | **Confirm** it is not superseded by `PAYLOAD_GUIDE.md`; if it is, → `archive/` |

**→ `guides/`** (how-to for designers and coding agents, `audience: internal`)

`COMMAND_REGISTRATION_GUIDE.md`, `LANGUAGE-INTEGRATION_GUIDE.md`, `PAYLOAD_GUIDE.md`
— plus `UNITTEST_GUIDE.md` arriving from the repo root (§7.2).

### 7.1a Seeding `reviewed:` and History

Every document landing in `reference/` or `guides/` needs `reviewed:`, an `area:`, and a `##
History` section (guide §9). **This is the one place the migration cannot be mechanical**, and
getting it wrong quietly defeats the guardrail:

- **Do not seed `reviewed:` with today's date.** That would assert all fifteen documents were
  verified against the code on migration day, which is false, and it would push the first real
  review out by three months.
- **Seed each with the date it was last substantively edited** — `git log -1 --format=%ad`.
  Documents older than 92 days then land *already overdue*, which is the truth and puts them
  straight onto the standing review issue.
- **Seed History with one row per document**: the seed date, "Migrated from `specs/`; content
  unchanged.", source `migration`. Earlier history stays recoverable from git; back-filling it from
  commit messages would be invention.
- Expect most of the set to be overdue on day one. `WEB_API_SPECIFICATION.md` is dated 2026-01-19
  in its own header; `ASSETS.md` and `ASSET_LIFECYCLE.md` have not been touched since before the
  asset-lifecycle work landed. That backlog is not created by the migration — it is made visible by
  it, which is the point.

The first quarterly sweep is therefore real work, not a formality. Sequence it after the migration
lands rather than inside it.

**→ `archive/`** (date-prefixed; the date is when the content was true)

| File | Archive name |
|---|---|
| `todo20260219.md` | `2026-02-19-todo-audit.md` |
| `DEPENDENCIES_STATUS.md` | date from first commit |
| `JOBQUEUE_FIX.md` | date from first commit |
| `PHASE3-UNIT-TESTS.md`, `PHASE3-UNIT-TESTS-SUMMARY.md`, `PHASE3-UNIT-TESTS-IMPLEMENTATION-GUIDE.md`, `PHASE3-TESTS-INDEX.md` | Superseded by the `liquers-unittest` skill |
| `EXAMPLE_SCENARIO_1.md`, `QUERY_CONSOLE_ELEMENT_EXAMPLE3.md` | Worked examples, point-in-time |
| `EXAMPLE2-CUSTOM-CONFIG.md` | **Confirm** against the current config format — if still accurate, → `guides/` instead |
| `UI_INTERFACE_PHASE1_FSD.md`, `UI_INTERFACE_PHASE1a_FSD.md`, `UI_INTERFACE_PHASE1b.md` | Superseded by `UI_INTERFACE_FSD.md`. Note `liquers-lib/tests/ui_phase1b_integration.rs` references `UI_INTERFACE_PHASE1b.md` — rewrite it |
| `UI_DIOXUS_DESIGN_NOTES.md`, `UI_RATATUI_DESIGN_NOTES.md`, `UI_WEB_DESIGN_NOTES.md` | Exploratory notes on backends. `UI_WEB_DESIGN_NOTES.md` is referenced by `liquers-lib/examples-web/README.md` — rewrite it |

**→ `design/python-wrapper/`**

`PYTHON-WRAPPER-ARCHITECTURE.md` and `PYTHON-WRAPPER-HIGH-LEVEL-DESIGN.md` are a design pair
predating the folder convention. Add `DESIGN.md`; set `complete` or `draft` after checking
`liquers-py`.

**Stays put:** `command_registry.yaml` — 21 code and test references, and `CLAUDE.md` documents its
path. Moving it buys nothing.

### 7.2 Repo root

| File | Action |
|---|---|
| `README.md` | **Stays.** Update the links table (§8.2) |
| `CLAUDE.md` | **Stays.** Update references (§8.1) |
| `ISSUES.md` | **Delete** — a stub redirect to a file that no longer exists |
| `UNITTEST_GUIDE.md` | → `specs/guides/UNITTEST_GUIDE.md`, `audience: internal` |
| `EXAMPLE_SCENARIO_1_SUMMARY.md` | → `specs/archive/2026-02-20-example-scenario-1-summary.md`. A near-copy exists at `specs/axum-assets-recipes-api/EXAMPLE_SCENARIO_1_SUMMARY.md` and the two **differ** — diff them, keep one, delete the other |
| `plan20260707.md` | → `specs/archive/2026-07-07-implementation-plan.md` |
| `review20260707.md` | → `specs/archive/2026-07-07-project-review.md` |
| `liquers-designer.skill` | **Delete** — see §8.4 |
| `liquers-unittest.skill` | **Delete** — see §8.4 |

`plan20260707.md` and `review20260707.md` are two of the six competing backlogs. Archiving them is
not filing them away untouched: any still-open work package must first be promoted into
`specs/issues/`. Confirm before archiving that WP-1…WP-n are either shipped or represented by an
issue.

---

## 8. Step 6 — Entry points and skills

### 8.1 `CLAUDE.md`

| Line | Current | Becomes |
|---|---|---|
| 13 | `specs/  # Specifications and design documents` | Expand to the six-directory layout from the guide |
| 18 | `specs/PROJECT_OVERVIEW.md`, `specs/REGISTER_COMMAND_FSD.md`, `specs/ASSETS.md` | `specs/reference/…` |
| 20 | `**Known issues** are tracked in specs/ISSUES.md` | `specs/issues/` — index at `specs/index.csv`; filing rules in `specs/DOCS_STRUCTURE_GUIDE.md` §4.8 |
| 32 | `specs/POLARS_COMMAND_LIBRARY.md` | `specs/reference/POLARS_COMMAND_LIBRARY.md` |
| 217, 229 | `specs/PROJECT_OVERVIEW.md` | `specs/reference/PROJECT_OVERVIEW.md` |
| 243, 249 | `specs/COMMAND_REGISTRATION_GUIDE.md`, `specs/REGISTER_COMMAND_FSD.md` | `specs/guides/…`, `specs/reference/…` |
| 354 | `specs/STORE_CONFIG_FSD.md` | `specs/reference/STORE_CONFIG_FSD.md` |
| 304, 311, 331, 340 | `specs/command_registry.yaml` | unchanged |

**New section, ~12 lines**, placed near the top since it governs every session:

> **Documentation.** Map: `specs/README.md`. Rules: `specs/DOCS_STRUCTURE_GUIDE.md`. Issue index:
> `specs/index.csv`.
> - Found a problem? File `specs/issues/<ID>.md` with `status: draft`. Search `index.csv` first.
> - Never edit a file under `specs/archive/`, and never change the status of an issue that has a
>   `github:` number.
> - A PR that adds a design folder, or moves one to `complete`, updates `specs/README.md`.
> - `specs/index.csv` is generated — run `scripts/docs_index.py`, do not hand-edit.

### 8.2 `README.md`

Rewrite the links table (lines 37–44) for the new paths. `specs/api-docs-analysis/README.md`
(line 44) is archived, so that row points at `specs/README.md` instead.

**Line 10 can be deleted.** It currently reads: *"Documents under `specs/` may describe current
behavior, proposed behavior, or …"* — a caveat that exists precisely because the old flat directory
could not distinguish the two. After the migration the path answers it: `reference/` is current,
`design/` is proposed, `archive/` was true on a date.

### 8.3 Skills — content edits

| Skill / file | Change |
|---|---|
| `liquers-designer/scripts/init_feature.py` | Create under `specs/design/<slug>/`; emit `DESIGN.md` front-matter per guide §5, with `status: draft` and `phase: high-level`; drop the freeform `**Status:** In Progress` line; print a reminder to run `scripts/docs_index.py` |
| `liquers-designer/scripts/validate_phase.py` | Path `specs/parquet-support/…` → `specs/design/…`; add front-matter validation; **advance `phase:` when a phase is approved** — the field is only true if something maintains it, and this script is the one place that already knows a phase just passed |
| `liquers-designer/SKILL.md` | Design folders live under `specs/design/`; the status and phase vocabularies are guide §5.1–5.2; each phase transition updates `phase:` in `DESIGN.md`; **a landing design updates `specs/README.md`**; an `L`/`XL` issue requires a design folder; **add the `documentation` phase** (§8.3a) |
| `liquers-designer/references/phase5-documentation.md` | **New.** The phase-5 template and checklist (§8.3a) |
| `liquers-designer/references/liquers-patterns.md:166` | `see ISSUES.md` → `specs/issues/COMMAND-CONTEXT-PARAM-ORDER.md` |
| `liquers-designer/references/phase*-template.md` | No change — phase files carry no front-matter |
| `liquers-validate/SKILL.md:117` | → `specs/issues/PLAN-EXCESS-ACTION-PARAMETERS-DROPPED.md` |
| `liquers-validate/references/output-format.md:139` | → same |
| `liquers-validate/references/recipes-and-overlays.md` | Example path `specs/my-feature/proposed_commands.yaml` → `specs/design/my-feature/…` |
| `rust-best-practices/references/anti-patterns.md:228` | → `specs/issues/COMMAND-CONTEXT-PARAM-ORDER.md` |
| `liquers-unittest/SKILL.md` | Reference `specs/guides/UNITTEST_GUIDE.md` rather than restating it (§8.5) |

### 8.3a The `documentation` phase (phase 5)

Guide §9.3 puts the primary review trigger on the designer: a change that ships is the moment a
reference becomes wrong, and also the moment someone still has the context to fix it. That
obligation needs a place to live in the workflow, and it does not have one yet — the skill runs
four phases and stops at the implementation plan.

**Adding it is a change to the skill, not to this migration**, and it is the only piece of this
plan that changes how design work is done. It can land before, during or after the document moves;
the guide marks the phase *not yet active* until it does, so nothing is blocked in the meantime.

What phase 5 must do, once the implementation PRs are merged:

1. **Propose the affected set.** List every `reference/` and `guides/` document sharing an `area`
   with the design. This is the candidate list, generated so nothing is missed by forgetting; the
   designer keeps or discards each entry.
2. **Record the decision** as `affects_docs:` in `DESIGN.md`. Discarded candidates are simply
   absent — but a one-line note saying why is worth more than a silent omission when someone
   revisits the design later.
3. **Review each kept document against what actually shipped**, not against the design. The design
   says what was intended; the four `query-validation` "Implementation Notes" show how routinely
   those differ.
4. **Update the document, add a History row, and bump `reviewed:`** — in the same commit, because
   guide check 11 reads the diff and rejects a bumped date with no matching row.
5. **Only then may the design move to `complete`.** Guide check 12 enforces it: a design cannot
   close while a document it named has a `reviewed:` older than its last merged PR.

Step 3 is the part that cannot be delegated to a checklist, and it is where the value is. Steps 1,
2, 4 and 5 are mechanical enough that the skill should do them without being asked.

A note on ordering: phase 5 runs *after* merge, so a design sits at `implemented` between the PR
landing and the documentation being written. That is the state guide §5.1 introduced `implemented`
for, and it becomes reachable for the first time when this phase activates.

### 8.4 Skills — removals

**`liquers-designer.skill` and `liquers-unittest.skill` at the repo root are deleted.**

They are ZIP bundles, and their contents are **byte-identical** to `.claude/skills/liquers-designer/`
and `.claude/skills/liquers-unittest/` (verified by unpacking and `diff -rq`). They are packaged
exports of content that already lives in the repository in editable form — 68 KB of binary that
cannot be reviewed in a diff, cannot be grepped, and will silently fall behind the moment either
skill is edited. That only two of the four skills have bundles is itself evidence they were
incidental rather than part of a process.

- Delete both.
- Add `*.skill` to `.gitignore`.
- If distribution ever needs them, add `scripts/package-skills.sh` that zips
  `.claude/skills/<name>/` on demand — generated, not committed.

No *skill* is obsolete: all four (`liquers-designer`, `liquers-unittest`, `liquers-validate`,
`rust-best-practices`) are current and referenced. The redundancy is in the bundles and in the
documents the skills superseded (§8.5).

### 8.5 Documents the skills superseded

| Document | Overlaps | Action |
|---|---|---|
| `specs/PHASE3-UNIT-TESTS*.md` (4 files) | `liquers-unittest` references | → `archive/` (§7.1). The skill is the live copy |
| `UNITTEST_GUIDE.md` (19 KB, root) | `liquers-unittest/references/test-patterns.md` (16 patterns) and `testable-components.md` | **Keep both, they differ in kind** — the guide is a narrative walkthrough, the references are catalogues. Move the guide to `specs/guides/`, have the skill link to it. Deduplicating the overlapping content is a **follow-up, not part of this migration**: it is a content decision and would hide behind a move |

---

## 9. Step 7 — Tooling and enforcement

- `scripts/docs_index.py` per guide §7 — generate, `--sync`, `--check`.
- Wire `--check` into CI on every PR (offline validations only, no token required).
- Add the post-merge `--sync` job and the weekly scheduled run.

Then **write `specs/README.md`** — the capability map (guide §8), which is the largest single piece
of writing in the migration and the one that cannot be mechanised. Build it in this order, because
each step narrows what the next has to invent:

1. Run `docs_index.py` and take the `unplaced` block: every design folder, every `reference/` and
   `guides/` document, every open feature. That is the raw material, complete by construction.
2. Group into subsystems. The `area` vocabulary (guide §3) is the natural first cut — `core/assets`,
   `core/query`, `lib/ui`, `web` — but the map is for readers, not for the validator, so merge or
   split where that reads better.
3. Give each capability its stage and its single highest-stage link (guide §8.1). Most entries
   resolve mechanically: a capability with a `reference/` document is `documented`; one with only a
   `complete` design is `built`; a `kind: feature` issue with no design is `planned`.
4. Write the connective prose — why two capabilities are coupled, what a subsystem is for, which
   planned item matters first. This is the part that makes the document worth reading rather than
   querying, and it is the only part an LLM should be inventing.
5. Re-run until `unplaced` holds only items deliberately folded behind a broader capability line.

This step lands **last** among the structural ones, because `--check` validating a half-migrated
tree would block the migration on itself, and because the capability map cannot be written until
every document is in its final location.

---

## 10. Correction to carry forward

`DOCS_STRUCTURE_REVIEW.md` §2.7 states that the root `.skill` files "differ from the live
`.claude/skills/*/SKILL.md`". That was based on `diff` between a ZIP archive and a markdown file,
which of course reports a difference. Unpacked, the bundles are **identical** to the live skills.

The recommendation is unchanged — delete them (§8.4) — but for the right reason: they are redundant
binary duplicates, not stale forks. Note this when archiving the review.

---

## 11. What a human must decide

Nothing in this plan should be executed on a guess. These need an answer first:

1. **Every `complexity` value in §4.1 and §5.** Never previously recorded; all are estimates.
2. **`ASSET-MESSAGE-LIFECYCLE-ROBUSTNESS`** — bare `High` mapped to P1. Confirm.
3. **Six design folders** flagged **yes** in §6.1 — is the work shipped, and if not, which phase
   is each one on?
4. **`POLARS-FEATURE-GAPS`** — what shipped, what remains.
5. **`PYTHON-BASIC-OBJECTS`** — closed by PR #2, or still open?
6. **`UI_PAYLOAD_DESIGN.md`** — reference or archive?
7. **`EXAMPLE2-CUSTOM-CONFIG.md`** — accurate against the current config format?
8. **The two `EXAMPLE_SCENARIO_1_SUMMARY.md` copies** — which is canonical?
9. **`command_metadata.rs:702`** — which issue does it mean?
10. **Move design folders into `specs/design/`, or leave them at `specs/<slug>/`?** (§2)
11. **`plan20260707.md` work packages** — all shipped, or does archiving lose open work?
12. **`wp2-terminal-outcome`** — what remains unimplemented? Guide §5.6 forbids a partial status,
    so the remainder becomes an issue and the design takes the status its PR earned. Someone has to
    say what the remainder *is*.
13. **`gh_pr` for each shipped design** — recoverable from branch names, but confirm the mapping
    for `dependency-management` and `dependency-scheduling`, which sit near PRs #5 and #6 and could
    be transposed.
14. **Is 92 days the right review interval?** Guide §9.4 sets it because you asked for no more than
    three months. Fifteen documents at that cadence is roughly one review per week, sustained
    indefinitely — cheap when a document is current, not cheap across a set that starts overdue.
    Halving the set (moving the `*_COMMAND_LIBRARY` specs to `docs/`) or splitting the cadence by
    `audience` are both ways to cut it, if the first sweep shows the pace does not hold.
15. **When does the `documentation` phase activate?** (§8.3a) Until it does, the §9.3 interlock is
    honoured by hand.

---

## 12. Order and size

| Step | Content | Touches code? |
|---|---|---|
| 1 | Scaffold + contract (§3) | No |
| 2 | Issues (§4) | Yes — 15 references |
| 3 | Feature briefs (§5) | No |
| 4 | Design folders (§6) | Yes — 45 references |
| 5 | Top level and root (§7) | Yes — a handful |
| 6 | Entry points and skills (§8) | No |
| 7 | Tooling and enforcement (§9) | Adds `scripts/`, CI |
| 8 | *(optional)* GitHub issues for new work | No |

Steps 1–3 deliver the queryable overview and can stop there — the backlog is usable with no change
to how anyone works. Steps 4–5 are the genre sort, mechanical but wide. Steps 6–7 are what stop it
drifting again. Step 8 is the only change to workflow, and it is optional in the sense that
everything before it stands on its own.
