---
title: Python backlog compaction and bulk design, 2026-10-27
kind: archive
audience: internal
area: [py, docs]
created: 2026-10-27
---

# Python backlog compaction and bulk design, 2026-10-27

This is a dated record of the compaction (`liquers-project` skill, `references/compaction.md`)
of the open Python-related issues, run interactively for the maintainer. It is never edited.

**Scope.** The run covered every open issue or feature whose `area` includes `py` (22 at the
start), plus the problems found while designing them. Designs that already had a finished Phase 4
were excluded from review, as `autonomous_bulk_design.md` §1 requires. Their open decisions are
listed under "Decisions" below.

## Counts

| | Start | End |
|---|---|---|
| Open `py`-area issues and features | 22 | 27 (5 new `py` issues filed) |
| ... with a readiness-labeled design | 4 | 26 (all but PY-NO-LANGUAGE-DEFINED-STORE) |
| ... labeled `autofix: eligible` | 0 | 3 (`py-docstrings`, `py-encoded-reprs`, `py-metadata-typed-errors`) |
| New non-`py` issues filed | — | 2 (`WEB-THROWN-BACK-LIQUERSERROR-LOSES-CONTEXT`, eligible; `VALIDATE-PHASE-COMPACT-SIZE-IGNORES-SOURCE-COMPLEXITY`, no design) |

New designs by readiness: 4 `ready` (all eligible), 18 `needs-decision`. None is `blocked`,
`phase2-blocked` or `covered`.

## Merges (strong overlap, `overlap.md`)

- `language-binding-tests-in-ci`: LANGUAGE-BINDING-TESTS-NOT-RUN-IN-CI (leading) and
  PY-TESTS-NOT-RUN-IN-CI. T4 holds, since the Python half is covered, and T3 holds, since both
  add the same workflow.
- `py-command-decorator-declaration-keywords`: PY-COMMAND-DECORATOR-CANNOT-DECLARE-ARGUMENT-METADATA
  (leading) and PY-COMMAND-DECORATOR-CANNOT-SET-REALM. T3 holds: both change the same
  `_decorator` / `_register` keyword lists.

Items kept apart despite overlap:

- `py-metadata-typed-errors` vs `py-metadata-fields-as-properties`: kept apart by E1.
- `web-thrown-back-error-context` vs `multi-language-traceback`: kept apart by E1.
- `py-pypy-ci-job` vs `language-binding-tests-in-ci`: a close call. T2 and T3 hold, but the
  maintainer's pending matrix decision scopes the latter without PyPy.
- `multi-language-traceback` vs `error-with-key-field`: kept apart by E2/E4.

No merge of existing designs is proposed.

## Splits

Two designs had an eligible slice, which was split into its own issue and design:

- PY-REPRS-SHOW-RUST-DEBUG-FORMAT → PY-ENCODED-REPRS-QUOTE-THE-DEBUG-STRING (`py-encoded-reprs`)
- PY-API-REFERENCE-NOT-GENERATED → PY-PUBLIC-NAMES-HAVE-NO-DOCSTRINGS (`py-docstrings`)

## Designs created (one per row; leading source first)

| Design | Sources | Form | Readiness / auto-fix |
|---|---|---|---|
| `language-binding-tests-in-ci` | LANGUAGE-BINDING-TESTS-NOT-RUN-IN-CI, PY-TESTS-NOT-RUN-IN-CI | compact M | needs-decision / not-eligible: the CI matrix and its cost are the owner's call. Eligible once approved |
| `py-metadata-typed-errors` | PY-METADATA-METHODS-RAISE-PLAIN-EXCEPTION | compact M | **ready / eligible** |
| `py-encoded-reprs` | PY-ENCODED-REPRS-QUOTE-THE-DEBUG-STRING | compact S | **ready / eligible** |
| `py-docstrings` | PY-PUBLIC-NAMES-HAVE-NO-DOCSTRINGS | compact M | **ready / eligible** |
| `web-thrown-back-error-context` | WEB-THROWN-BACK-LIQUERSERROR-LOSES-CONTEXT | compact S | **ready / eligible** |
| `py-python-style-reprs` | PY-REPRS-SHOW-RUST-DEBUG-FORMAT | compact M | needs-decision |
| `py-metadata-fields-as-properties` | PY-METADATA-FIELDS-ARE-METHODS | compact S | needs-decision |
| `py-command-decorator-declaration-keywords` | two decorator issues | compact S | needs-decision |
| `py-recipe-provider-choice` | PY-RECIPE-PROVIDER-NOT-CONFIGURABLE | compact S | needs-decision |
| `py-api-reference` | PY-API-REFERENCE-NOT-GENERATED | compact S | needs-decision |
| `py-record-values-to-python` | PY-RECORD-VALUES-CANNOT-CROSS-TO-PYTHON | compact M | needs-decision |
| `py-pypy-ci-job` | PY-PYPY-SUPPORT-UNVERIFIED | compact S | needs-decision |
| `py-query-segment-registration` | PY-QUERY-SEGMENT-CLASS-NOT-REGISTERED | compact S | needs-decision |
| `language-bridge-extended-scalars` | EXTENDED-SCALARS-NOT-MAPPED-BY-LANGUAGE-BRIDGES | compact M | needs-decision (requires `extended-scalar-value-types`) |
| `language-global-enum-import` | GLOBAL-ENUMS-NOT-IMPORTED-BY-LANGUAGE-INTEGRATIONS | compact M | needs-decision (requires `global-enum-registration`) |
| `language-late-registration-without-rebuild` | LANGUAGE-INTEGRATIONS-REBUILD-ENVIRONMENT-ON-REGISTRATION | compact M | needs-decision (requires `post-init-command-registration`) |
| `query-builder-in-bindings` | QUERY-BUILDER-NOT-IN-BINDINGS | compact M | needs-decision (requires `query-builder`) |
| `language-store-type-registration` | LANGUAGE-STORE-TYPE-NOT-DEFINABLE | compact M (web only) | needs-decision |
| `multi-language-traceback` | MULTI-LANGUAGE-TRACEBACK-IS-A-FLAT-STRING | full L | needs-decision |
| `language-store-config-function` | LANGUAGE-STORE-CONFIG-FUNCTION-NOT-DEFINABLE | full L | needs-decision |
| `records-arrow-c-data-export` | RECORDS-ARROW-C-DATA-EXPORT-NOT-BUILT (re-pointed from the complete `record-streams`) | full L | needs-decision |
| `agent-command-proposal` | AGENT-CANNOT-PROPOSE-A-COMMAND | full L | needs-decision (requires `post-init-command-registration`) |

Process: each design was drafted by its author. Two independent reviewers then checked it, one for
conformity and records and one for codebase and soundness. The author fixed every finding.
`validate_phase.py` passes Phases 1-4 for every design. One label changed in review: two designs
first labeled eligible became `needs-decision`, because they settle a matrix the owner had reserved.

## Filed without a design

- PY-NO-LANGUAGE-DEFINED-STORE (L): a Python-implemented store. It is outside this run's S/M-first
  scope.
- VALIDATE-PHASE-COMPACT-SIZE-IGNORES-SOURCE-COMPLEXITY (S, tooling): outside the Python scope.
  It is left for the next compaction.

## Fixes

Order: priority, then age. Each fix gets its own branch and PR. Skipped because they are already
handled by the separate issue-fixing work: every item with an open `claude/project-thread-iiek2v-*`
PR, among them FOREIGN-VALUE-TEXT-AND-JSON-HOOKS-NEVER-CALLED (#209).

1. `py-docstrings` (P2, M)
2. `py-metadata-typed-errors` (P3, M)
3. `py-encoded-reprs` (P3, S)
4. `web-thrown-back-error-context` (P3, S; liquers-web, found during this run)
5. `language-binding-tests-in-ci` (P2), if the owner approves the matrix

Each fix PR records its own outcome in its design and issue.

## Decisions

Each design's Phase 1 `Design Readiness` holds its questions with recommendations. The leading
ones:

- **CI matrix and cost** (`language-binding-tests-in-ci`). Recommended: 3.9, 3.14 and 3.14t, plus
  the web Node loop with and without `records`. Approving it unblocks one design and makes it
  eligible; the PyPy row (`py-pypy-ci-job`) is decided after it.
- **Python repr convention** (`py-python-style-reprs`). Recommended: Python's rule. Classes with a
  text form show `Name('<text>')`, the others `<liquers.Name field=value …>`.
- **Methods to properties without deprecation** (`py-metadata-fields-as-properties`).
  Recommended: switch cleanly, since no wheel is published.
- **Decorator `realm=` and `arguments=`** (`py-command-decorator-declaration-keywords`).
  Recommended: add both, and refuse `realm="store_config"`.
- **`trivial` recipe provider from Python** (`py-recipe-provider-choice`). Recommended: support
  every choice.
- **Generator** (`py-api-reference`). Recommended: pdoc.
- **Python type of a record view** (`py-record-values-to-python`). Recommended: `pyarrow.Table`.
- **Export `QuerySegment`** (`py-query-segment-registration`). Recommended: register it as it is.
- **Traceback shape** (`multi-language-traceback`). Recommended: segments plus the derived string.
- **Where `unsafe` lives** (`records-arrow-c-data-export`). Recommended: a feature-gated
  `cdata.rs` module in `liquers-records`.
- **Binding API for store-config handlers** (`language-store-config-function`). Recommended: a
  dedicated decorator and function.
- **Where `registerStoreType` lives** (`language-store-type-registration`). Recommended: an
  `Environment` method.
- Four integration designs mostly wait on their core design's decisions, which are carried here as
  assumptions:
  - `extended-scalar-value-types`
  - `global-enum-registration`
  - `post-init-command-registration`: its trait shape also gates `agent-command-proposal`
  - `query-builder`

Existing Python-related designs, not reviewed (their Phase 4 is finished), with their leading open
questions:

- `value-interface-capability-split`: extract only `ValueType` (identity and description), and
  rename `identifier` to `type_identifier`, in JavaScript too.
- `error-type-taxonomy`: the set of new error types and categories, the HTTP codes, and keeping
  `General` but refusing it in workspace code.
- `error-with-key-field`: an ordered context frame list, with flat fields describing the innermost
  cause.

## Implementation plan for the `needs-decision` designs (after their decisions)

- **No dependency; can start once decided:**
  - `py-metadata-fields-as-properties`: after `py-metadata-typed-errors`.
  - `py-command-decorator-declaration-keywords`.
  - `py-recipe-provider-choice`.
  - `py-python-style-reprs`: after `py-encoded-reprs`.
  - `py-api-reference`: after `py-docstrings`.
  - `py-query-segment-registration`.
  - `py-record-values-to-python`: before Part B of `records-arrow-c-data-export`.
  - `language-store-type-registration`.
  - `py-pypy-ci-job`: after `language-binding-tests-in-ci`.
- **After their core design is implemented:**
  - `language-bridge-extended-scalars`.
  - `language-global-enum-import` and `language-late-registration-without-rebuild`: both change
    `rebuild_with`, so whichever lands second rebases.
  - `query-builder-in-bindings`.
  - `agent-command-proposal`.
- **L, human-gated:**
  - `multi-language-traceback`: after `py-metadata-typed-errors`.
  - `language-store-config-function`.
  - `records-arrow-c-data-export`.
