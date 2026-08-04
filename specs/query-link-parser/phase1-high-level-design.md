# Phase 1: High-Level Design - query-link-parser

## Feature Name

Action-parameter link parsing (`~X~<query>~E`) — fixes `QUERY-ACTION-PARAMETER-LINK-PARSER`

## Purpose

`ActionParameter::Link` is a supported part of the query language: it is encoded as
`~X~<query>~E`, resolved by the planner as a linked query parameter, and rendered by the
query renderers — but `liquers_core::parse` has no production for it, so the encoded form
cannot be read back. This feature adds the missing parser production so link parameters
round-trip through `parse_query` and can be written by hand in queries.

## Core Interactions

### Query System
Extends the action-parameter grammar in `liquers-core/src/parse.rs` with one production:
`link-parameter = "~X~", query, "~E"`, yielding `ActionParameter::Link(query, position)`.
The embedded text is parsed with the *same* authoritative query grammar (`query_parser`),
so an inner query means exactly what it means at top level. No other grammar changes:
resource names, headers, filenames and keys are untouched. Link syntax is currently
unparseable, so no existing valid query changes meaning.

### Store System
None. Parsing is purely textual; no store is opened.

### Command System
No new commands. Existing commands gain the ability to receive a link argument written in
query text, which the planner already resolves (`ParameterValue::ParameterLink`).

### Asset System
Indirect: a parsed link becomes a plan dependency (`DependencyRelation::ParameterLink`),
which the existing dependency machinery already handles for programmatically built links.

### Value Types
None.

### Web/API
None directly — link queries arriving over HTTP simply stop being rejected.

### UI
None. `QueryRenderer` already renders `~X~`/`~E` entities.

## Crate Placement

**liquers-core** only (`src/parse.rs`, doc updates in `src/query.rs`). Parsing is a core
concern; `query.rs`, `plan.rs` and `dependencies.rs` already support the semantic model,
so this is a parser-side gap. No downstream crate signature changes (`liquers-py` wraps
`ActionParameter` opaquely).

## Open Questions

1. **Delimiter scanning vs. recursive descent.** Running `query_parser` directly on the
   remainder risks changing inner-query semantics (some top-level forms require `eof`).
   Preferred: scan for the matching `~E`, then parse exactly that slice. → Phase 2.
2. **Nesting and escaping.** Should `~X~a-~X~b~E~E` (a link inside a link) parse? Preferred
   yes, via depth counting that honours the `~~` escape. → Phase 2.
3. **Error reporting.** Unterminated `~X~` should fail hard with a position rather than
   backtrack into an empty string parameter; how much of `parse_query`'s nom→`Error`
   mapping must improve to carry that position? → Phase 2.
4. **Mixed parameters.** Is `action-abc~X~q~E` legal? Preferred: no — a link is a whole
   parameter, never concatenated with text. → Phase 2.
5. **Non-round-trippable inner queries.** A programmatically built inner query whose
   resource name contains `~E` cannot survive encode→parse. Document as a known limit
   (consistent with the existing `SimpleTemplate` `$` caveat) rather than fix here.

## References

- `specs/ISSUES.md` → `QUERY-ACTION-PARAMETER-LINK-PARSER` (P0)
- `liquers-core/src/parse.rs` — grammar reference, "Known link-parser bug" note (l. 59-66),
  current rejection test `documented_query_language_contract` (l. 1322)
- `liquers-core/src/query.rs` — `ActionParameter::Link`, `encode`, `QueryRenderer` (l. 531-644)
- `liquers-core/src/plan.rs` — `ParameterValue::ParameterLink` construction (l. 615-633)
- `specs/api-docs-analysis/doc-02-query-language-reference.md` — records the gap as P0
