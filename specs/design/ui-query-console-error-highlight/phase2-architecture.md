# Phase 2: Solution and Architecture - Query Console Highlights Positioned Errors

## Overview

Add a highlighted layouter path that accepts `Option<&Position>` and calls
`StyledQuery::from_query(&query, position)` when present, falling back to `Position::unknown()` for
existing callers. `QueryConsoleElement::render` passes `self.error.as_ref().map(|e| &e.position)`
to that path.

## Known-Issue Preflight

| Issue | Status | Priority | Relevance and action | Blocking? |
|---|---|---|---|---|
| `WEBUI-QUERY-CONSOLE-ENTER-KEY-SUBMIT` | accepted | P2 | Same widget family but event handling, not highlighting. No dependency. | no |
| `PLAN-EXCESS-ACTION-PARAMETERS-DROPPED` | not in current P2/S set | n/a | Motivates positioned errors; this UI design consumes positions but does not change planner errors. | no |

## Files and Symbols

Primary files: `liquers-lib/src/egui/widgets.rs` for `query_to_layout_job` or a sibling
`query_to_layout_job_with_position`; `liquers-lib/src/ui/widgets/query_console_element.rs` for the
egui layouter closure. Existing core symbols: `StyledQuery::from_query`, `Position`,
`StyledQueryToken::Highlight`.

## Data, Ownership, Serialization and Errors

No serialized data changes. The UI borrows `Position` from `Error` during render; if closure
lifetime requires ownership, clone the small `Position` into the layouter state.

## Sync, Async and API Effects

Pure synchronous render-path change. Prefer adding a new helper or defaulting wrapper so existing
callers of `query_to_layout_job(q)` remain source-compatible.

## Alternatives

Rejected: parse and style directly in `QueryConsoleElement`; that duplicates core query styling.
Rejected: highlight only the error label; the issue requires token-level visual feedback.

## Risk Assessment

| Assessment | Record |
|---|---|
| Files | 2 source/test files plus specs/index. |
| Impact area | egui query console rendering. |
| Module/crate reach | `liquers-lib` UI using `liquers-core::query` types; no crate API break if wrapper retained. |
| Existing-test breakage | None expected; visual tests may need expected token update if present. |
| New validation | Unit test for styled query token highlight with known/unknown position; UI helper test if feasible. |
| Behavioural risk | Minimal; render-only. No persistence/concurrency/security concern. |
| Recovery | Revert helper and call-site change. |
| Certainty | High that styling pipeline exists; medium on exact egui layouter lifetime shape. |

## Rust Review

Borrowing the error position is preferred; clone only to satisfy closure lifetime. No unwraps, no
new errors, no async work, and no changes to query parser semantics.
