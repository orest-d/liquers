# Phase 1: High-Level Design - Query Console Highlights Positioned Errors

## Problem and Evidence

`Error` carries `Position`, and `StyledQuery::from_query` can turn matching tokens into
`StyledQueryToken::Highlight`, but the egui query console calls `query_to_layout_job` without any
position and therefore renders only a red error label below the input.

## Expected Behaviour and Acceptance Criteria

When the console has an error with a known position, the matching token in the query text field is
underlined/highlighted. Unknown positions keep current message-only behaviour.

## Affected Systems

The egui UI rendering path and core query styling are affected. Query parsing, command execution,
web HTML rendering and asset state updates are not changed in this issue.

## Scope and Non-Goals

Scope is threading an optional `Position` into the egui layouter. Do not implement browser-side
token highlighting or redesign `StyledQuery`.

## Compatibility, Assumptions and Questions

Existing callers of `query_to_layout_job` should keep a no-position path for compatibility.
Assumption: cloning `Position` for UI rendering is acceptable and cheap.

## Documentation Assessment

No new reference or guide is expected. If UI reference docs mention error presentation, update one
sentence; otherwise Phase 5 can record no docs update needed.

## Review

The change fits the existing query styling path, is localized, and has observable UI/test criteria.
