---
id: PLAN-STEP-STATE-METADATA
kind: design
title: A plan step's input state describes its value, not the asset being built
form: compact
workflow: liquers-project
status: in_review
phase: high-level
readiness: needs-decision
autofix: not-eligible
area: [core/plan, core/context]
issues: [CONTEXT-STEP-REPLACES-INPUT-STATE-METADATA]
created: 2026-10-10
---
# A plan step's input state describes its value, not the asset being built

## Phase 1: High-Level Design

### Purpose

State who owns metadata during plan execution (asset, context, state) and make the state handed
from step to step describe *its own value*: the input state's metadata survives the steps that do
not produce data, and a value fetched from another asset or the store arrives with that asset's
format and origin rather than with the format of the output being built.

### Metadata roles: how the code works today

The intended model, verified against `HEAD`:

| Holder | Role today | Matches intended model? |
|---|---|---|
| **Plan** | Describes how one asset is built. `GetAsset`, `GetResource`, `Evaluate` and parameter links pull in *other* assets as dependencies; a predecessor cut (`Plan::cut_predecessor`) turns a cacheable prefix into a separate asset; a nested `Step::Plan` runs in the same asset and context. | Yes |
| **Asset** (`AssetData::metadata`, `liquers-core/src/assets.rs`) | The one authoritative record. Seeded from the recipe (`Recipe::get_asset_info`: title, filename, `data_format` from the *output* filename, `bin` when there is none), written during evaluation through the context and the service channel (log, progress), finalized by `AssetRef::complete_evaluation` (type identifier, dependencies, status, version), persisted for a keyed asset. | Yes |
| **Context** (`Context<E>`, `liquers-core/src/context.rs`) | Holds no metadata of its own. `get_metadata` returns a *copy* of the asset's record; `set_filename`, `set_title`, `set_expires`, `add_log_entry` write to the asset. It buffers `pending_dependencies`, which are merged into the asset at completion — a buffer, not a second record. | Yes, with one wording caveat: `get_metadata` reads as if the context owned it |
| **Input state / intermediate states** (`apply_plan`, `liquers-core/src/interpreter.rs`) | Transient. `apply_plan` returns only `Arc<Value>`; the last state's metadata is discarded and the asset's record is what survives. **But** every intermediate state is rebuilt after every step as *step value + `context.get_metadata()`*, patched only in `key` (`value_origin_key`). | **No** — see root cause |

So the first three points of the intended model already hold. The fourth does not: the input
state's metadata is lost after *any* step, not only an action.

### Root cause

`MetadataRecord` plays two roles that `apply_plan` treats as one:

1. **The record of the asset under construction** — status, log, progress, dependencies, version,
   title, and the format the *result* will be written in. One per asset; authoritative.
2. **The description of a value** — what a command needs to interpret its input: `data_format`,
   `media_type`, `filename`, origin `key` (type identifier and name are re-synced from the value by
   `State::with_metadata`, so they are not at risk).

`apply_plan` fills role 2 of every intermediate state with role 1 of the evaluating asset. The
value-describing fields therefore always say "the output this asset will become", never "the value
you hold". `value_origin_key` (added by `design/record-streams/`) patched exactly one of those
fields, `key`, and its doc comment records that `filename` and `data_format` were left as the
asset's on purpose to limit the change. The issue's symptom is one instance; the same cause also
drops the format of every fetched dependency.

### Problem Example

Verified on `HEAD` with a probe command `fmt` that returns `state.metadata.get_data_format()` and
`key` (a throwaway test against `SimpleEnvironment`; the store holds `data/x.csv` as `Text` with
declared `data_format: txt`):

| Evaluation | `fmt` sees today | Should see |
|---|---|---|
| `AssetManager::apply(fmt, <state with data_format csv>)` | `csv` | `csv` |
| `apply_plan([Info, fmt], <state with data_format csv>)` — **the issue** | `bin` | `csv` |
| `-R/data/x.csv/-/fmt` | `bin`, key `data/x.csv` | `txt` (the stored format), key `data/x.csv` |
| `-R/data/x.csv/-/fmt/out.txt` | `txt` — the *output's* format | `txt` (the stored format) |

In a real command the issue's case is `apply(ns-pl/slice-0-2)` behind an `Info` step on a CSV text
state: `slice` gets `bin` and polars fails with *"Unsupported polars data_format 'bin'"*. The
fetched-dependency rows mean, by the same code path, that `-R/data/x.csv/-/ns-pl/slice-0-2` on a CSV
stored as bytes reaches `slice` with `bin`, and `-R/data/x.parquet/-/ns-pl/slice-0-2/-/out.csv` on
parquet bytes reaches it with `csv` (not run; derived from the rows above and
`liquers-lib/src/polars/util.rs` `try_to_polars_dataframe`).

### Scope and Acceptance Criteria

- **AC-1** Pass-through step keeps the input's description
  - WHEN a plan applied to a supplied state runs `Info`, `Warning`, `Error`, `SetCwd` or `Filename`
    before its first action
  - THEN the action's input state has the supplied state's `data_format`, `media_type`, `filename`
    and `key`
- **AC-2** Fetched asset arrives with its own description
  - WHEN a step `GetAsset`, `GetAssetBinary` or `Evaluate` fetches a dependency
  - THEN the next step's input state carries the dependency's `data_format`, `media_type` and
    `filename`, and the key it was fetched from (as today)
- **AC-3** Fetched resource arrives with its stored description
  - WHEN `GetResource` reads a key whose stored metadata declares a `data_format`
  - THEN the next step's input state carries that `data_format`
- **AC-4** Output format does not leak into the input
  - WHEN a query ends in a filename (`…/-/fmt/out.txt`) and fetches its input from another key
  - THEN the action's input state carries the input's format, while the asset's own metadata
    still has `filename: out.txt` and `data_format: txt`
- **AC-5** The asset record stays authoritative
  - WHEN any plan finishes
  - THEN the asset's metadata (log, title, filename, `data_format`, dependencies, status) is the
    same as before this change; only intermediate states differ
- **AC-6** Polars end to end
  - WHEN `ns-pl/slice` is applied through a plan `[Info, Action(ns-pl/slice)]` to a CSV text state
  - THEN it returns the sliced frame (the issue's original failure)

**Non-goals.** Changing what `Recipe::data_format` seeds into the asset (`bin`; see Design
Dependencies). Letting a command return metadata with its value. Changing the state after an
action step (Open question Q2). Changing `Metadata` or `State` types.

**Scope note (2026-10-10).** The source issue names pass-through steps only. The fetch steps lose
metadata by the same code path (overlap test T1, same root cause), found while verifying the issue,
so they are in this design rather than a second issue. Complexity stays `M`.

**Systems touched.** `liquers-core` only: `interpreter.rs` (`apply_plan`, `do_step`,
`value_origin_key`), reference documents. No `liquers-lib` code change expected; `liquers-lib`
tests prove AC-6. No command, query syntax or serialized format changes.

**Documentation intent.**
- Reference: a new section *Metadata ownership during evaluation* in
  `specs/reference/api/DOC_04_ENVIRONMENT_CONTEXT_EVALUATION.md` (the roles table above, made
  normative); rewrite of the step-state paragraphs in DOC_04 §Context lifetime and in
  `specs/reference/api/DOC_08_RECIPES_PLANS.md` §Plan fields and execution.
- Guide: one paragraph in `specs/guides/COMMAND_REGISTRATION_GUIDE.md` — read the input's format
  from `state.metadata`, the asset's record through `Context`.
- Other: `Context::get_metadata` doc comment says it returns a snapshot of the asset's record.
- `specs/reference/ASSETS.md` layer description checked, updated only if it contradicts.

### Design Readiness

`needs-decision` — Phases 2-4 not yet written; the questions below shape Phase 2.

- **Q1 (open design — mechanism).** How the next state gets its value description.
  Recommended: **(A) descriptor overlay** — the next state is the asset's record (role 1, so
  logs and title stay current) with the four value-describing fields taken from the step's
  *source*: the previous state for a pass-through step, the dependency's state for
  `GetAsset`/`GetAssetBinary`/`Evaluate`, the stored metadata for `GetResource`, key only for
  directory listings (as today). Needs `do_step` to hand back the source metadata, via a private
  `do_step_state` beside the public `do_step`, which keeps its signature.
  Alternatives: **(B)** pass-through steps return the input state untouched and nothing else
  changes — fixes the issue and AC-1 only, leaves AC-2..AC-4; smallest change. **(C)** hand the
  dependency's whole state on — also carries its status, log and version into the next command,
  which then sees another asset's record; rejected.
- **Q2 (open design — after an action).** An action's value is new, so its format is honestly
  "undeclared". Recommended: keep today's behaviour (asset record) in this design — commands that
  write `set_filename` for the next step rely on it, and the `bin` default belongs to
  `FREE-FUNCTION-EVALUATE-BAKES-A-BIN-DATA-FORMAT`. Alternative: clear the four fields after an
  action so the value's own default applies.
- **Q3 (proposed resolution — documentation).** Make the roles table normative in DOC_04 rather
  than in a new reference document, since DOC_04 already owns the context contract.

### Design Dependencies

- **overlaps** `FREE-FUNCTION-EVALUATE-BAKES-A-BIN-DATA-FORMAT` (draft, no design): the asset's
  declared `bin` is what leaks into intermediate states today. Weak overlap (different change
  site, `Recipe::get_asset_info`; neither fix subsumes the other): after this design the leak
  reaches only the state after an action (Q2).
- **related, complete** `design/record-streams/` introduced `value_origin_key`; this design
  generalizes it. `design/command-alias-contract/` worked around the issue by moving the alias
  message to `init_steps`; that workaround can stay.
