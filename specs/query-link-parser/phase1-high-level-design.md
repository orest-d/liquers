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

Adds one leaf production to `liquers-core/src/parse.rs`:

```text
link-parameter = "~X~", link-query, "~E"
```

yielding `ActionParameter::Link(query, position)`. A link is a *whole* action parameter —
never concatenated with text, so `action-abc~X~q~E` is invalid.

**Embedded grammar (decision: approach D+).** `link-query` is parsed by the same
productions as a top-level query, assembled into a link-specific entry point rather than
by reusing `query_parser` unchanged. The reason is that `query_parser`'s first two
alternatives are gated on `eof`, which can never hold before a `~E`; reusing it as-is
would silently drop every embedded query into `general_query`. Instead:

```rust
fn link_query(text: Span) -> IResult<Span, Query> {
    // resource_transform_query, terminated by a peeked `~E`, detects the shorthand.
    // It is a *guard clause that returns an error*, not an accepting alt arm —
    // see Phase 2 D3. (An earlier draft of this document sketched it as an alt
    // arm, which would have accepted the shorthand rather than rejecting it.)
    alt((general_query, empty_query))
}
```

Verified empirically (temporary test over 15 canonical forms, since `general_query` is
private): `alt((general_query, empty_query))` accepts every string `Query::encode()` can
emit, consuming it completely and producing a query identical to `parse_query`'s. This
holds because `Query::encode` routes each segment through `encode_with_header`
(`query.rs:2477`, `1719`), which always writes the explicit `-R` form. The single
divergence is the **resource/transform shorthand** (`a/b/-/c`), which `general_query`
reads as transform+transform instead of resource+transform.

**The shorthand is rejected inside a link, not reinterpreted.** The
`resource_transform_query` alternative above exists to *detect* the shorthand and fail
with a clear diagnostic ("resource/transform shorthand is not allowed inside `~X~…~E`;
write `-R/a/b/-/c`") rather than let it parse into a different query than the same text
means at top level. Nesting (`~X~a-~X~b~E~E`) and the `~~` escape need no special
handling — both fall out of the recursion and the existing `entities` production.

**Consequence identified during Phase 2, recorded here for traceability:** links are the
first recursive construct in the query grammar, so this feature introduces a
stack-overflow risk on untrusted input. Phase 2 D5 designs the guard.

No other grammar changes: resource names, headers, filenames and keys are untouched. Link
syntax is currently unparseable, so no existing valid query changes meaning.

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

### Documentation
A deliverable of this feature, not an afterthought:

1. `liquers-core/src/parse.rs` module docs — delete the "Known link-parser bug" section
   (l. 59-66), add `link-parameter` to the grammar and the entity table, and state the
   embedded-query rule.
2. `specs/api-docs-analysis/doc-02-query-language-reference.md` — replace the "Link
   parameters do not parse" limitation and drop the P0 row from the improvement table.
3. `liquers-core/src/query.rs` — `ActionParameter::Link` doc comment currently says the
   encoded form cannot be parsed (l. 536-540).
4. `specs/archive/2026-08-08-issues.md` — mark the issue resolved.

All four must say the same thing about the shorthand: **it is discouraged in general** —
`-R/data/x.csv/-/to_text` is preferred over `data/x.csv/-/to_text` because the explicit
form is what the encoder emits and what round-trips — and **it is not allowed at all
inside `~X~…~E`**, where it is a parse error with a message naming the explicit
replacement. The rationale to document: inside a link there is no `eof` to disambiguate,
so accepting the shorthand there would mean the same text denotes different queries
depending on nesting depth.

## Crate Placement

**liquers-core** only (`src/parse.rs`, doc updates in `src/query.rs`). Parsing is a core
concern; `query.rs`, `plan.rs` and `dependencies.rs` already support the semantic model,
so this is a parser-side gap. No downstream crate signature changes (`liquers-py` wraps
`ActionParameter` opaquely). Documentation changes reach `specs/`.

## Open Questions

1. **Carrying a specific message out of nom.** Both the shorthand rejection and an
   unterminated `~X~` want a *worded* error, but the parser is typed on nom's default
   `Error<Span>`, which carries only a span and an `ErrorKind`. Options: `cut`/`Failure`
   plus a position-aware `parse_query` mapping with a generic message; a `ErrorKind`
   convention; or a custom nom error type (invasive — changes every signature). → Phase 2.
2. **Is `simple_transform_query` redundant?** The verification test suggests
   `general_query` covers everything it accepts, which is why `link_query` omits it. Worth
   confirming before relying on it. → Phase 2.
3. **`parse_key` / `parse_simple_template` error positions.** They share the same
   nom→`Error` mapping weakness. Fix alongside, or leave scoped to `parse_query`?
   → Phase 2.
4. **Non-round-trippable inner queries.** A programmatically built inner query whose
   resource name contains `~E` cannot survive encode→parse. Document as a known limit
   (consistent with the existing `SimpleTemplate` `$` caveat) rather than fix here.

## References

- `specs/archive/2026-08-08-issues.md` → `QUERY-ACTION-PARAMETER-LINK-PARSER` (P0)
- `liquers-core/src/parse.rs` — grammar reference, "Known link-parser bug" note (l. 59-66),
  query form precedence (l. 101-123), `query_parser` (l. 649), current rejection test
  `documented_query_language_contract` (l. 1322)
- `liquers-core/src/query.rs` — `ActionParameter::Link`, `encode`, `QueryRenderer`
  (l. 531-644); `Query::encode` (l. 2477); `encode_with_header` (l. 1719, 1898)
- `liquers-core/src/plan.rs` — `ParameterValue::ParameterLink` construction (l. 615-633)
- `specs/api-docs-analysis/doc-02-query-language-reference.md` — records the gap as P0
