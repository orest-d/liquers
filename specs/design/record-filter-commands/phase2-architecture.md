# Phase 2: Solution & Architecture — Record and polars command interoperability

> **Carried over from the compact draft (2026-10-11), not yet reviewed.** It covers only AC-1 to AC-8.
> Phase 2 replaces this block when it is written.
>
> #### Solution
>
> `liquers-lib/src/records/filters.rs`, registered in `register_records_commands!`. Each command
> builds a one-node `SearchExpr` directly (so nothing is parsed or escaped), then runs the
> `rank = false`, `limit = 0` path of `ns-search/search`: `search_view` for a view, `FilteredSource`
> for a source. `sort` uses `RecordView::take` with indices sorted by `Column::compare`.
>
> ```
> async fn eq(state, column: String, value: String, context) -> result   // ne, gt, gte, lt, lte
> async fn startswith(state, column: String, text: String, case_sensitive: bool = true, context) -> result
>                                                                        // endswith, contains, glob
> async fn isin(state, column: String, values: Vec<String> multiple, context) -> result
> async fn isnull(state, column: String, context) -> result              // notnull
> async fn filter(state, query: String = "", context) -> result
> async fn sort(state, column: String, ascending: bool = true, context) -> result
> ```
>
> Names and the `column` argument mirror `ns-pl`. The input is converted as `ns-search/search`
> converts it (`search_input`).
>
> #### Changes
>
> - New file: `liquers-lib/src/records/filters.rs`.
> - Updated: the registration in `liquers-lib/src/records/commands.rs`.
> - Regenerated: `specs/command_registry.yaml`.
> - Updated: `specs/reference/RECORD_STREAMS.md`, with the command list.
>
> #### Risks
>
> The design depends on `store-and-asset-search` for `SearchExpr`, `search_view` and
> `FilteredSource`. It is otherwise small and additive.


## Overview
<The chosen solution in 2-4 sentences. Rejected alternatives, one line each with the reason.>

## Known-Issue Preflight
| Issue | Status | Priority | Impact on this design | Fix first? | Blocking? | Action |
|---|---|---|---|---|---|---|
<Open issues linked to this design or touching its areas and integration points; `None found` if
none. A blocker is resolved first or designed around, and is at least P1.>

## Interfaces
<New and changed items only, as Rust signatures without bodies: structs and enums (with ownership:
`Arc` / `Box` / owned, and serde), trait impls and bounds, public functions, ExtValue variants,
commands (`register_command!` DSL line). For each: sync or async, and why. Write `None` for a
category that does not apply.>

## Integration Points
<Files and modules to create or modify, per crate, with the call sites affected.>

## Error Handling
<Failure cases and the `Error` constructor each uses (no `Error::new`).>

## Relevant Commands
<New commands with signatures; existing namespaces this interacts with (e.g. `pl`, `lui`).>

## Documentation Architecture
<For each document to create or update: exact path, kind, audience, area, and the change. The
proposed `affects_docs` set. Links to add in `specs/README.md`.>

## Risks
| Assessment | Finding |
|---|---|
| Files likely to change | |
| Crates and workflows affected | |
| Existing tests likely to change | |
| New validation | |
| Compatibility / data / concurrency / performance / security | |
| Recovery | |
| Certainty and open questions | |
