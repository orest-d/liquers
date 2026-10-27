---
id: PLAN-SERIALIZATION-INTERNAL-CONTRACT
kind: design
title: A serialized plan is declared an inspection format of the build that wrote it
form: compact
status: in_review
phase: implementation
readiness: ready
autofix: eligible
area: [core/plan, docs]
issues: [PLAN-SERDE-HAS-NO-COMPATIBILITY-CONTRACT]
created: 2026-10-26
---
# A serialized plan is declared an inspection format of the build that wrote it

Bulk design (`specs/guides/autonomous_bulk_design.md`), 2026-10-26: Phases 1-4 written, not
approved, nothing implemented.

## Phase 1: High-Level Design

### Purpose

`Plan`, `Step`, `ParameterValue` and the types they contain are public and derive `Serialize` /
`Deserialize`. Plans leave the process in two places: the HTTP recipes API
(`GET resolve/{*key}` returns `"plan": {…}`) and Python (`Plan.to_json` / `to_yaml` /
`from_json` / `from_yaml`). Nothing says whether that JSON is a contract. The rustdoc half-promises
one: "except for fields explicitly marked with `serde(default)`, this module does not define a
stable cross-version wire format". The reference says a plan from before freezing "still
deserializes". The project makes no serialized-format backward-compatibility promise yet. So the
honest contract is that a serialized plan is for **inspection and same-build round trips**. To keep
something across versions, keep the query or the recipe and rebuild the plan. This design writes
that contract down where a reader of each surface will find it.

### Problem Example

Verified at HEAD (cd6d5da). A client stores the JSON plan returned by `GET
/recipes/resolve/data/report.csv` (or by Python `Plan.to_json()`) and reads it back with
`Plan.from_json` after an upgrade that added a `Step` variant or made a field required. Today the
documentation does not say whether this is supported. The `Plan` rustdoc implies that the
`serde(default)` fields are stable, while `query`, `steps` and `is_volatile` are required and an
unknown `Step` variant fails. Should be: every place that exposes the format says that it is the
current build's inspection format, that nothing is promised across versions, and what to persist
instead.

### Scope and Acceptance Criteria

- **AC-1** The rustdoc states the contract
  WHEN `Plan`, `Step` or `ParameterValue` rustdoc is read
  THEN it says the serialized form is an inspection format, round-trips within one build, carries no
  cross-version guarantee, and that a query or recipe is what to persist; `serde(default)` is
  described as tolerance, not a promise
- **AC-2** The references state it
  WHEN `RECIPES_AND_PLANS.md` §Serialization and future plan rewriting, and
  `WEB_API_SPECIFICATION.md`'s `resolve` row are read
  THEN they say the same, and the Known limitations bullet for this issue is gone
- **AC-3** Same-build round trip stays pinned
  WHEN the existing serialization tests run
  THEN a plan still round-trips through JSON and YAML unchanged
  (`recipe_plan_round_trip_keeps_raw_operands_and_prefix`,
  `serialization_keeps_raw_cwd_links_positions_and_no_runtime_schema`,
  `test_plan_expires_serialization_roundtrip`)

Non-goals: a versioned schema (an envelope with a version, migrations); removing existing
`serde(default)` attributes (they are harmless tolerance, and `volatility_source_defaults_on_a_legacy_plan`
pins one); changing the Python binding's docstrings (another crate; `PYTHON_INTEGRATION.md` gains
the sentence instead); the serialized form of `Recipe`, which *is* persisted (`recipes.yaml`) and has
its own contract.

### Design Readiness

- **Readiness:** ready
- **Automatic fixing:** eligible — a documentation fix: rustdoc in `liquers-core/src/plan.rs` and
  three reference documents. No code or behaviour changes (rules 2-6).
- **Leading issue:** None
- **Explanation:** of the issue's two options, "version a supported schema" would create a
  compatibility promise the project does not make yet. "Declare it runtime-internal" matches the
  current policy (no wire or serde backward-compatibility requirement, maintainer guidance for this
  bulk-design pass) and the code as it is. The issue's complexity drops from `M` to `S`, because no
  code changes.
- **Open questions:** None. If the project later adopts a compatibility policy for serialized
  formats, a versioned plan schema becomes a new feature that supersedes this contract.

### Design Dependencies

- `query-cache-directives` — **overlaps**: adds a `Plan` field and relies on "`Plan` serialization
  has no compatibility contract". This design makes that statement true and documented. No order.
- `inherited-volatility-bounds-predecessor-cut`, `dependency-summary-reuse` — **overlaps**: both
  deliberately keep new information *out* of the serialized plan. Neither depends on this.

### Consolidated Findings

- Exposure points checked at HEAD: `liquers-axum/src/recipes/handlers.rs` (`serde_json::to_value(&plan)`
  for `resolve`), `liquers-py/src/plan.rs` (`to_json`, `to_yaml`, `from_json`, `from_yaml`). No code
  in the repository persists a plan: asset metadata stores no plan, and recipes are stored as
  `Recipe`.
- `RECIPES_AND_PLANS.md` §Serialization already calls the form "a current data contract, not a
  versioned stable wire-format guarantee". The change is to make "current" mean "this build", and
  to state what to persist instead.

## Phase 2: Architecture

### Solution

Documentation only.

1. `liquers-core/src/plan.rs` rustdoc:
   - `Plan`: replace the last paragraph ("Serde derives expose the current internal representation.
     Except for fields explicitly marked …") with: "The serialized form is an **inspection format**
     of the build that wrote it: it round-trips within one build and is what the HTTP `resolve`
     endpoint and Python's `Plan.to_json` show. It carries no compatibility guarantee across
     versions — a field may become required, a `Step` variant may appear or change. Persist the
     query or the `Recipe` and rebuild the plan instead. `serde(default)` on some fields lets an
     older plan load where that happens to work; it is not a promise."
   - `Step` and `ParameterValue`: one sentence pointing to `Plan`'s paragraph.
2. `specs/reference/RECIPES_AND_PLANS.md` §Serialization and future plan rewriting: the first
   paragraph's last sentence becomes the contract above. The second paragraph ("so a plan serialized
   before freezing existed still deserializes…") is kept as a description of the tolerance, prefixed
   "Within that tolerance,". The issue link and the Known limitations bullet are removed. History
   row, `reviewed:` bump.
3. `specs/reference/WEB_API_SPECIFICATION.md`, `resolve` row: append "the plan is the server
   build's inspection format, not a stable schema (`RECIPES_AND_PLANS.md` §Serialization)". History
   row, `reviewed:` bump.
4. `specs/reference/PYTHON_INTEGRATION.md`: one sentence where `Plan` is described (or in the
   bindings' type list): `Plan.to_json` / `from_json` round-trip within one version only. History
   row, `reviewed:` bump.

Rejected: a versioned schema (no policy asks for it, and the cost is permanent); `#[doc(hidden)]`
or `pub(crate)` serde (would break the two legitimate inspection uses).

### Changes

- `liquers-core/src/plan.rs`: rustdoc on `Plan`, `Step`, `ParameterValue`.
- `specs/reference/RECIPES_AND_PLANS.md`, `specs/reference/WEB_API_SPECIFICATION.md`,
  `specs/reference/PYTHON_INTEGRATION.md`.

### Risks

| Assessment | Finding |
|---|---|
| Files likely to change | one source file (comments only), three references |
| Existing tests | unaffected; `cargo test --doc -p liquers-core` covers the rustdoc |
| Compatibility | none in code. A user who persisted plans learns that it was never supported |
| Recovery | revert |
| Certainty | high |

## Phase 3: Examples and Tests

### Examples

Supported: `plan = build_plan("-R/data/x.csv/-/upper"); Plan.from_json(plan.to_json())` in the same
process or version. Unsupported: storing that JSON and reading it after an upgrade. Store the query
`-R/data/x.csv/-/upper` (validated: `GetAsset[data, x.csv]`, `Action{upper}` with `--command upper`)
and rebuild instead.

### Tests

No new test: the contract is that nothing is promised across versions, and the same-build round
trip is already pinned.

- `liquers-core/src/recipes.rs` `recipe_plan_round_trip_keeps_raw_operands_and_prefix` and
  `serialization_keeps_raw_cwd_links_positions_and_no_runtime_schema` (AC-3).
- `liquers-core/tests/expiration_integration.rs` `test_plan_expires_serialization_roundtrip` (AC-3).
- `cargo test --doc -p liquers-core plan` (AC-1: the rustdoc builds; review checks its wording).
- `python3 scripts/docs_index.py --check` and review of the three references (AC-2).

## Phase 4: Implementation Plan

### Steps

- [ ] 1. `liquers-core/src/plan.rs` — rustdoc of `Plan`, `Step`, `ParameterValue` — `cargo test --doc -p liquers-core` and `cargo test -p liquers-core --lib recipes::tests`
- [ ] 2. `RECIPES_AND_PLANS.md` §Serialization and Known limitations; `WEB_API_SPECIFICATION.md` `resolve` row; `PYTHON_INTEGRATION.md` sentence; History rows and `reviewed:` in each — `python3 scripts/docs_index.py --check`
- [ ] 3. Close `PLAN-SERDE-HAS-NO-COMPATIBILITY-CONTRACT` with a resolution note ("declared an inspection format; versioning would be a new feature"); this design to `complete` — `python3 scripts/docs_index.py` then `--check`

### Validation

`cargo doc -p liquers-core --no-deps` builds without new warnings; `python3 scripts/docs_index.py
--check`. Diff review: comments in `plan.rs` and the three documents only.
