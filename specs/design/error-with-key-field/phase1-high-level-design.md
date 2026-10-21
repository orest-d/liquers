# Phase 1: High-Level Design — Structured Error Context for Keys and Nested Queries

Revised 2026-10-20 (bulk design, resumed under `specs/guides/autonomous_bulk_design.md` §11): the
2026-09-03 blockers are carried as recommended answers, the maintainer's "no wire or serde backward
compatibility is needed yet" removes the legacy-projection blocker, and acceptance scenarios are
added.

## Feature Name

Structured error context (`ERROR-WITH-KEY-FIELD`): a key is a key, and an error records its evaluation path.

## Purpose

An `Error` names one `query`, one `key` and one `position`, and every enrichment overwrites them.
`Error::with_key` even writes the key into `query`. So a failure deep in an evaluation cannot say
which asset, which link and which action it passed through, and a keyed asset's failure often does
not name its key at all. Give errors an ordered record of the evaluation path, and make the flat
fields mean one thing: where the error was raised. This serves `VISION.md`'s promise that every
value is traceable to the query that made it — the same must hold for every failure.

## Problem Example

A recipe `reports/out.txt` with query `use_link-~X~-R/missing.txt~E` (`use_link` takes a link
argument), where `missing.txt` is neither stored nor has a recipe. Evaluating `-R/reports/out.txt`
at HEAD (18732dd):

- the `missing.txt` asset fails ("No recipe found for key missing.txt", `General`); `with_key` puts
  `missing.txt` in `query`, `key` stays `None`;
- the link boundary in `interpreter.rs` (`attach_parameter_link_context`) overwrites `position` with
  the link's position — a position in `reports/out.txt`'s query, while `query` still names
  `missing.txt`: the pair now points into the wrong text;
- the failing keyed asset `reports/out.txt` adds nothing (`finish_run_with_result` stores the error
  as it came), so nothing in the persisted `error_data` names `reports/out.txt`.

Should be: flat `key = "missing.txt"` (where it was raised) and an ordered `context`:
`[Asset{key: missing.txt}, Link{query: -R/missing.txt, position: <link in out.txt's query>},
Asset{key: reports/out.txt}]`, innermost first, in `error_data`, over HTTP, in Python and in
JavaScript. (With `key-without-recipe-answer`, the cause is also `KeyNotFound`.)

## Scope and Acceptance Criteria

- **AC-1** A key is a key
  WHEN an error is enriched with `Error::with_key(key)`
  THEN `error.key` holds the key and `error.query` is unchanged
- **AC-2** Keyed constructors fill `key`
  WHEN `key_not_found`, `status_conflict`, `dependency_version_mismatch` or `dependency_cycle` is built for a store key
  THEN `key` holds it and `query` is `None`; a dependency that is not a store key (`-R-dir/…`, a command dependency) is recorded in `query`
- **AC-3** Every failed asset names itself
  WHEN a keyed or query asset's run fails, for any reason
  THEN the stored error ends with an `Asset` frame naming its key (or its query), and `key` is set to the asset's key only if no inner key was set
- **AC-4** The nested path is kept, innermost first
  WHEN an error crosses a link, an action and an asset boundary on its way out
  THEN `context` lists one frame per boundary in crossing order, each frame's position belonging to that frame's query
- **AC-5** The cause's location is not overwritten
  WHEN a command fails inside a nested evaluation that an outer action or link was waiting for
  THEN `position`, `query` and `command_key` keep the innermost values, and the outer action or link appears as a frame
- **AC-6** Store access carries the store
  WHEN a store raises `key_read_error`, `key_write_error` or `key_not_supported`
  THEN the error has a `Resource` frame with the key and the store's name
- **AC-7** Cancellation names its root cause by role
  WHEN a cancellation is recorded or cascades
  THEN the root-cause asset is named in `key` when it is keyed and in `query` otherwise, at every depth
- **AC-8** Context persists and travels
  WHEN an error with frames is stored in metadata `error_data`, serialized to JSON, returned by the HTTP API, raised in Python or thrown in JavaScript
  THEN the frames survive in order (`context`), and an error without frames serializes with no `context` key
- **AC-9** Bounded
  WHEN more than 16 frames would be recorded
  THEN the innermost frame and the 15 most recent are kept; an adjacent duplicate is never recorded
- **AC-10** Logs keep the key
  WHEN `LogEntry::from_error` records an error that has a `key` and no `query`
  THEN the log entry's query is that key's query

Non-goals: rendering frames as HTML or clickable text (a renderer may come later and reads
`context`; `Display` stays one line); a cause chain of whole errors; changing `ErrorType`s (the
missing-key type is `key-without-recipe-answer`).

## Core Interactions

- **Query:** `Query::from(&Key)` and `Query::key()` unchanged; frames hold encoded text.
- **Store:** three store constructors add a `Resource` frame with the store name.
- **Assets:** the run-failure path adds the asset frame; cancellation attribution uses `key`.
- **Commands / plan:** link and action boundaries in `interpreter.rs` add frames instead of
  overwriting the cause's location.
- **Web/API, bindings:** `ErrorDetail`, `LiquersError` (JavaScript) and the Python exception gain
  `context`.

## Crate Placement

`liquers-core` owns the model (`error.rs`) and the producers (`assets.rs`, `interpreter.rs`,
`metadata.rs`); `liquers-py`, `liquers-web`, `liquers-axum` and the `liquers-lib` egui error widget
expose it. Nothing moves between crates.

## Documentation Intent

- Reference: new `specs/reference/ERROR_CONTEXT.md` — the flat-field rule, frames, roles, order,
  bound, serialization, binding shapes.
- Guide: extend `specs/guides/LANGUAGE-INTEGRATION_GUIDE.md` §ERROR (map `context`).
- Other documents: none.
- Documents to update: `PROJECT_OVERVIEW.md`, `ASSETS.md`, `ASSET_LIFECYCLE.md`,
  `WEB_API_SPECIFICATION.md`, `PYTHON_INTEGRATION.md`, `api/DOC_03_ASSETS_EXECUTION_LIFECYCLE.md`
  (cancellation root cause in `key`), `specs/README.md` (link the new reference).

## Design Readiness and Open Questions

- **Readiness:** needs-decision
- **Automatic fixing:** not-eligible — `L`, new `pub` types and fields across four crates (rules 1,
  3, 4).
- **Leading issue:** Q2, what the flat fields mean.
- **Explanation:** every 2026-09-03 blocker now has a recommended answer that Phases 3-4 are written
  under; none is unknowable, and the maintainer waived serde compatibility. The answers are visible
  choices, so the design is not `ready`.
- **Open questions:**
  1. **Open design - model:** an ordered `context: Vec<ErrorContextFrame>` on `ErrorPayload`,
     innermost first (recommended), or a recursive cause chain (rejected in Phase 2: serde depth,
     equality, bindings).
  2. **Open design - flat fields:** they describe the innermost cause; a boundary sets one only when
     it is unset and always appends a frame (recommended), versus outermost-wins (today's accident).
  3. **Open design - roles:** a closed enum `Asset`, `Link`, `Action`, `Resource` (recommended; a
     recipe is the keyed asset that runs it, a dependency failure arrives as the dependency's own
     `Asset` frame), versus the 2026-09 candidate's seven roles.
  4. **Proposed resolution - bound and duplicates:** 16 frames, keeping the innermost and the most
     recent; equal adjacent frames collapse.
  5. **Proposed resolution - cancellation:** the root cause moves from `query` to `key` for keyed
     assets (`ErrorType::Cancelled`'s doc and `DOC_03` change).
  6. **Proposed resolution - bindings:** `context` as a list of plain records (Python dicts,
     JavaScript objects, JSON objects); no new classes.

## Design Dependencies

- `key-without-recipe-answer` — **overlaps**: sets `key` directly in its constructors; independent.
  Landing it first gives the Problem Example its `KeyNotFound`.
- `core-tracing-migration` — **overlaps**: rewrites `eprintln!` diagnostics in the same files; no
  semantic interaction.
- `axum-assets-endpoints` (in implementation, E3) — **overlaps**: it notes `with_key` writes `query`
  and avoids it; nothing to extend there.

## Consolidated Findings

- The single site where every run failure passes is `AssetRef::finish_run_with_result` →
  `attribute_run_error`; adding the asset frame there covers keyed recipes, query assets and
  dependencies (a dependency's error re-enters the parent's run and gets the parent's frame), so no
  per-call-site audit of `?` is needed.
- `Error::with_key` writing `query` is load-bearing today: the cancellation contract
  (`ErrorType::Cancelled` doc, `attribute_run_error`'s `query.is_none()` test,
  `cancelled_dependency_cause`) and seven assertions in `asset_cancellation.rs`, `assets.rs` and
  `plan.rs` read it. They change in the same step as the fix.
- Python already carries `query` and `key` on its exceptions (`_from_native`); the 2026-09 statement
  that it "exposes neither" was about the `liquers.Error` wrapper class only.
- No query, command or registry change; `specs/command_registry.yaml` is untouched.
