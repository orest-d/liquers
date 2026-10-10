# Phase 1: High-Level Design — plan-step-state-metadata

## Purpose

Make the state a command receives as input well defined: in the cut plan it is exactly the state
of the predecessor asset, and the expanded plan gets as close to that as it can. Steps that produce
no value hand their input on untouched, a prefix that only reads a key is not turned into a
boundary, and a query with no filename stops declaring a `bin` format that the value cannot honour.

## Background: who owns metadata

Verified against the code on 2026-10-10.

| Holder | Role |
|---|---|
| **Plan** | How one asset is built. `Evaluate`, `GetAsset*`, `GetResource*` and parameter links read *other* assets as dependencies. |
| **Asset** (`AssetData::metadata`) | The one authoritative record of the asset being built. Seeded from the recipe (`Recipe::get_asset_info`), written through the context while it runs, finalized by `AssetRef::complete_evaluation`, persisted for a keyed asset. |
| **Context** | Holds no metadata of its own. `Context::get_metadata` returns a copy of the asset's record; `set_filename`, `set_title`, `add_log_entry` and the others write to the asset. Its pending dependencies are a buffer merged into the asset at completion. |
| **Input and intermediate states** | Transient. `apply_plan` returns only the value; the asset's record is what survives. |

**Two plan forms.** `PlanBuilder::build` produces the **expanded** plan: every step inline in one
asset (`finalize_plan_expanded`). Evaluation without an input state uses the **cut** plan:
`Plan::cut_predecessor` replaces the outermost cacheable prefix with one `Step::Evaluate` (the
*predecessor boundary*), recursively, so `a/b/c` runs as `Evaluate(a/b), Action(c)` and `a/b` as
`Evaluate(a), Action(b)`. Applying a plan to an input state always uses the expanded form.

**Reference rule.** Most evaluations use the cut form, so it defines what a command receives:
`Evaluate(a/b)` produces the state of asset `a/b`, and **that state is what `c` gets, metadata
included**. The expanded form approximates it. `design/predecessor-cut-equivalence/` made the two
forms agree on the result and left metadata out; this design extends the agreement to the input
state of each command.

## Root cause

`apply_plan` (`liquers-core/src/interpreter.rs`) rebuilds the state after **every** step from the
step's value and `context.get_metadata()` — a copy of the *final* asset's record. Three defects
follow:

1. A fetch step (`Evaluate`, `GetAsset`, `GetAssetBinary`, `GetResource`) keeps only the value of
   the state it fetched; the fetched metadata is dropped. `value_origin_key` patches back one field,
   `key`.
2. A step that produces no value (`Info`, `Warning`, `Error`, `SetCwd`, `Filename`) replaces its
   input's metadata with the asset's — the source issue.
3. The asset's record describes the *output*: its `filename` and `data_format` come from the final
   query. Every intermediate value is labelled with them. When the query has no filename,
   `Recipe::data_format` falls back to `"bin"` and `Recipe::get_asset_info` declares it, so every
   prefix asset and every unnamed query claims a format no value in `liquers-lib` serializes as
   (`FREE-FUNCTION-EVALUATE-BAKES-A-BIN-DATA-FORMAT`).

## Problem Example

Measured on `HEAD` with a throwaway command `fmt` returning `state.metadata.get_data_format()` and
`key`; the store holds `data/x.csv` as `Text` with declared `data_format: txt`:

| Evaluation | `fmt` sees today | Should see |
|---|---|---|
| `apply_plan([Info, fmt], <state with data_format csv>)` — the source issue | `bin` | `csv` |
| `AssetManager::apply(fmt, <state with data_format csv>)` | `csv` | `csv` |
| `-R/data/x.csv/-/fmt` | `bin`, key `data/x.csv` | `txt`, key `data/x.csv` |
| `-R/data/x.csv/-/fmt/out.txt` | `txt`, from the **output's** filename | `txt`, from the stored file |

In `liquers-lib` the first row is `ns-pl/slice` failing with *"Unsupported polars data_format
'bin'"* (found by `design/command-alias-contract/`). The same path means that
`-R/data/x.parquet/-/ns-pl/slice-0-2/-/out.csv` hands `slice` parquet bytes labelled `csv`.

`-R/data/x.manifest.yaml/-/ns-rec/materialize` builds as `GetAsset, Action`, and the cut turns it
into `Evaluate(-R/data/x.manifest.yaml/-/ns-rec), Action`: an extra query asset that only fetches a
keyed asset and has no key of its own, which `value_origin_key` exists to work around.

## Scope and Acceptance Criteria

"Value-describing fields" below means `key`, `filename`, `data_format` and `media_type`.

- **AC-1** A step that produces no value hands its input on
  WHEN a plan runs `Info`, `Warning`, `Error`, `SetCwd` or `Filename` before an action
  THEN the action's input state has the metadata of the state before that step
- **AC-2** A cut boundary hands on its asset's state
  WHEN `a/b/c` is evaluated with the cut plan `Evaluate(a/b), Action(c)`
  THEN `c`'s input state has the same metadata as asset `a/b`
- **AC-3** A fetched asset arrives with its own metadata
  WHEN `GetAsset` or `GetAssetBinary` reads a key
  THEN the next step's input state carries that asset's value-describing fields and `key`
- **AC-4** A fetched resource arrives with its stored metadata
  WHEN `GetResource` reads a key whose stored metadata declares a `data_format`
  THEN the next step's input state carries that `data_format`
- **AC-5** The output's format does not label the input
  WHEN `-R/data/x.txt/-/fmt/out.csv` is evaluated and the stored file declares `txt`
  THEN `fmt` sees `txt`, and the asset's own metadata still has `filename: out.csv` and
  `data_format: csv`
- **AC-6** An expanded plan approximates the cut plan
  WHEN `a/b/c` is applied as an expanded plan
  THEN `c`'s input state agrees with the cut plan's on `query` (the prefix `a/b`) and on the
  value-describing fields, and the asset's log contains the entries `a` and `b` wrote
- **AC-7** No boundary for a bare key read
  WHEN `-R/data/x.manifest.yaml/-/ns-rec/materialize` is evaluated
  THEN the evaluated plan is `GetAsset(data/x.manifest.yaml), Action(rec/materialize)` and
  `materialize` sees `key: data/x.manifest.yaml`
- **AC-8** An unnamed query declares no format
  WHEN an asset is built from a query with no filename (`a/b`, or the free `evaluate` function)
  THEN its declared `data_format` is absent, `state.as_bytes()` uses the value's default format,
  and a query ending `/out.csv` still declares `csv`
- **AC-9** Polars end to end
  WHEN `ns-pl/slice` runs behind an `Info` step on a CSV text state, and on
  `-R/data/x.csv/-/ns-pl/slice-0-2` over CSV stored as bytes with `data_format: csv`
  THEN both return the sliced frame
- **AC-10** The asset record stays authoritative
  WHEN any plan finishes
  THEN the asset's metadata (log, title, description, filename, dependencies, status, version) is
  what it is today, except that a query with no filename no longer declares `data_format: bin`

- **AC-11** An action's output state names its prefix
  WHEN `a/b/c` runs in either form
  THEN the state `c` receives has `query: a/b` and no `key`; in the cut form this is asset
  `a/b`'s own metadata, in the expanded form it is recorded by the plan

**Non-goals.** Letting a command return metadata with its value. State variables (liquer's
mechanism for carrying values along a query in metadata) — the rule above lets them flow, but the
mechanism is not built here. Changing what the asset records for a query with a filename. Making
the expanded form's log match the cut form's (in the expanded form one asset records all three
commands, by construction).

## Core Interactions

- **Plan building:** `Plan::cut_predecessor` declines a boundary over a prefix that only reads a
  key. `Recipe::data_format` / `Recipe::get_asset_info` stop inventing `bin`.
- **Interpreter:** `apply_plan` / `do_step` hand on fetched and passed-through states;
  `value_origin_key` and `fetched_key` are removed.
- **Assets:** unchanged in behaviour except the absent default format (AC-8, AC-10).
- **Commands:** none change; `liquers-lib` tests prove AC-9.

## Crate Placement

`liquers-core` only (`interpreter.rs`, `plan.rs`, `recipes.rs`). `liquers-lib/src/egui/widgets.rs`
reads `Recipe::data_format` for display and adapts to its new answer. Tests in both crates.

## Documentation Intent

- Reference: extend `specs/reference/api/DOC_04_ENVIRONMENT_CONTEXT_EVALUATION.md` with a
  section *Metadata ownership during evaluation* (the Background table and reference rule, made
  normative), and rewrite its §Context lifetime paragraph on step states. Rewrite
  `specs/reference/api/DOC_08_RECIPES_PLANS.md` §Plan fields and execution (step states, the
  bare-key exception to cutting, recipe `data_format`). Check `VALUE_TYPE_SYSTEM.md` §The
  encoding axis, which already states the absent-format rule.
- Guide: `specs/guides/COMMAND_REGISTRATION_GUIDE.md` — a command reads its input's description
  from `state.metadata`, and writes the asset's through `Context`.
- Other documents: `Context::get_metadata` doc comment (a copy of the asset's record).
- Documents to update: the four in `affects_docs`.

## Decisions

Settled with the maintainer on 2026-10-10; Phase 2 implements them.

1. **The expanded form's approximation.** After an action at prefix `p`, the state's value is the
   action's result and its metadata is a copy of the asset's record with corrections: `query` is
   `p`, there is no `key`, and the fields the *final* recipe seeded (`filename`, `data_format`,
   `media_type`, a recipe-declared title or description) are replaced by what a recipe for `p`
   would seed. Values commands wrote through the context stay.
2. **Where the prefix query comes from:** the plan records it on each action step (Phase 2
   option A). The builder already holds it when it emits the step; it is resolved and promoted
   exactly like `Plan::predecessor`.
3. **The key:** an action's output has none (in the cut form the prefix is a query asset); a step
   that reads a key hands on the fetched state with that key.
4. **Bare key read:** a prefix whose steps, apart from `SetCwd`, are a single `GetAsset`,
   `GetAssetBinary`, `GetAssetMetadata`, `GetAssetDirectory`, `GetAssetRecipe`, `GetResource`,
   `GetResourceMetadata` or `GetResourceDirectory` (with any namespace declarations) is not cut.
5. **The rest of a fetched state** — status, log, version — is handed on whole (AC-2); commands
   treat it as information about their input.
6. **A recipe with argument or link overrides** records no prefix queries; its asset already has
   no query, and the text would not describe what ran.
7. **Assumption, to confirm at the Phase 2 gate:** a plan applied to an input state still records
   the prefix query, although the value depends on the input; the applied asset's own record
   already carries its recipe's query in the same situation.

## Open Questions

None blocking. Decision 7 is an assumption awaiting confirmation at the Phase 2 gate.

## Design Dependencies

- **extends, complete** `design/predecessor-cut-equivalence/` (result equivalence; this adds
  input-state equivalence) and `design/record-streams/` (introduced `value_origin_key`, removed
  here).
- **overlaps, open** `design/context-title-predecessor-inheritance/` (in review, implementation
  phase, no PR): it would copy command-set title and description across the same `Step::Evaluate`
  arm into the *final asset's* record (T3, same change site; the contracts differ — that design
  changes the asset record, this one the input state). Neither blocks the other; whichever lands
  second rebases onto the other's `Step::Evaluate` arm.
- **related, complete** `design/command-alias-contract/` moved the alias message to
  `plan.init_steps` to work around the source issue; the workaround can stay.

## Scope Changes

**2026-10-10 — reference rule, fetch steps, bare-key boundaries, `bin`.** Agreed in discussion
with the maintainer, replacing the compact draft's "descriptor overlay".

- Added: fetch steps hand on the fetched state (T1, same root cause as the source issue); the cut
  plan is the reference and the expanded plan approximates it; no boundary over a bare key read
  (it exists only to work around the dropped metadata); `FREE-FUNCTION-EVALUATE-BAKES-A-BIN-DATA-FORMAT`
  attached (T2: once a prefix's metadata is handed on as it is, what that metadata declares is
  this design's contract). Leading source by the overlap rule (equal priority, older issue).
- Example: the four-row table and the manifest query above.
- Effect: AC-2..AC-11 added; size M → L; converted to the full form.
- Phases updated: Phase 1 only (no later phase existed). Approval: returns to the Phase 1 gate.

## References

- `specs/issues/CONTEXT-STEP-REPLACES-INPUT-STATE-METADATA.md`,
  `specs/issues/FREE-FUNCTION-EVALUATE-BAKES-A-BIN-DATA-FORMAT.md`
- `specs/reference/api/DOC_04_ENVIRONMENT_CONTEXT_EVALUATION.md`,
  `specs/reference/api/DOC_08_RECIPES_PLANS.md`, `specs/reference/VALUE_TYPE_SYSTEM.md`
