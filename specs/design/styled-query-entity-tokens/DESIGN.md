---
id: STYLED-QUERY-ENTITY-TOKENS
kind: design
title: Styled query tokens mark the entities inside a string parameter
form: compact
status: in_review
phase: implementation
readiness: needs-decision
autofix: not-eligible
area: [core/query]
issues: [QUERY-AST-DISCARDS-ENTITIES]
created: 2026-10-20
---
# Styled query tokens mark the entities inside a string parameter

Bulk design (`specs/guides/autonomous_bulk_design.md`): Phases 1-4 written on 2026-10-20. Not
implemented. The source issue was filed as `L` for an AST redesign; this design takes the cheaper
route the issue itself proposes, under which the work is `S` (Phase 2), so the issue is re-sized.

## Phase 1: High-Level Design

### Purpose

A query console or editor that colours a query cannot tell an entity inside a string parameter
(`~ncolon~`, `~U1F600~`, `~/`) from the literal text around it, because a string parameter is
rendered as one token. Mark entities as `StyledQueryToken::Entity` pieces, derived from the
canonical encoding, so highlighting works without changing `ActionParameter`. This was the one
motivation of `QUERY-AST-DISCARDS-ENTITIES` still open after `parameter-entity-escaping`.

### Problem Example

Verified at HEAD (18732dd), `liquers-core/src/query.rs` `impl QueryRenderer for ActionParameter`.
The query `filter-12~ncolon~30-~U1F600~` parses to two string parameters, `12:30` and `😀`.

```rust
let q = parse_query("filter-12~ncolon~30-~U1F600~")?;
let tokens = StyledQuery::from(&q).tokens;
```

Today the parameters produce `StringParameter("12~ncolon~30")` and `StringParameter("~U1F600~")`,
one token each, so `liquers-lib/src/egui/widgets.rs` `styled_query_to_layout_job` paints the entity
in the parameter colour. What should happen: `StringParameter("12")`, `Entity("~ncolon~")`,
`StringParameter("30")`, then `Entity("~U1F600~")`. The concatenated text is unchanged.

### Scope and Acceptance Criteria

- **AC-1** Entities in a string parameter are separate entity tokens
  WHEN `styled_tokens` runs on a query whose string parameter contains an entity in its canonical encoding
  THEN the parameter yields alternating `StringParameter` and `Entity` tokens, one `Entity` per entity
- **AC-2** The text is unchanged
  WHEN the tokens of any query are concatenated, or the query is rendered with `TrivialQueryRenderStyle`
  THEN the result equals `Query::encode()`
- **AC-3** `render` styles entities too
  WHEN a string parameter with an entity is rendered with a style whose `entity_begin` differs from `string_parameter_begin` (`DarkAnsiQueryRenderStyle`)
  THEN the entity is wrapped in the entity markers and the literal text in the parameter markers
- **AC-4** A plain parameter is one token
  WHEN a string parameter needs no escaping (`hello`), or is empty
  THEN it yields exactly one `StringParameter` token, as today
- **AC-5** Highlight still covers the whole parameter
  WHEN `styled_tokens` is called with the position of a string parameter that contains an entity
  THEN every token of that parameter is a `Highlight` token, as the single token is today

Non-goals: preserving the spelling the user typed (`~I` is shown as the canonical `~/`, `~n~amp~` as
`~namp~`); per-entity source positions; any change to `ActionParameter`, `Query`, serialization or
the parser; resource names (no entities today, see `resource-name-unicode`); `render` of a link's
embedded query (`ActionParameter::render` still calls `query.encode()` there, a separate TODO).

### Design Readiness

- **Readiness:** needs-decision
- **Automatic fixing:** not-eligible — an enhancement of rendering output rather than a bug fix
  (rule 2), and it carries the decision below (rule 5).
- **Leading issue:** **Proposed resolution — canonical highlighting.** Is highlighting the
  *canonical* spelling acceptable, or must a console show the spelling the user typed? The issue
  asks for this to be answered before any AST change. Recommended: canonical highlighting is
  acceptable, and the segment-list redesign of `ActionParameter::String` is not pursued. A console
  shows parsed queries, which are canonical once round-tripped, and an editor that preserves typed
  text keeps the source text, as the issue's workaround says. If the answer is no, this design is
  replaced by a full-form `L` design for the segment-list AST, and none of the steps below is wasted:
  the styled-token split stays the rendering half of it.
- **Explanation:** `liquers_core::escape::segments` already splits a token into `Text` and
  `Entity { spelling, decoded }`. `encode_token` output always decodes (`decode_token(encode_token(s))
  == s`), so splitting it cannot fail in practice, and the change is local to two methods.
- **Open questions:** None beyond the leading issue.

### Design Dependencies

- `parameter-entity-escaping` (complete, E4): **origin**. It provided `escape::segments` as the
  hook for this issue and the invariant that a string parameter holds the decoded value. This design
  keeps that invariant: nothing is stored, the pieces are derived at render time.
- `resource-name-unicode` (this run): **overlaps**. If resource names gain entities,
  `ResourceName::styled_tokens` should split them the same way; that design owns that change and can
  reuse the helper introduced here.
- `query-builder` (this run): no shared site. None.

### Consolidated Findings

- The pieces are derived from `encode_token(s)` at render time, so the
  `ActionParameter::String` invariant (decoded value, no stored spelling) is untouched, and
  `ACTION-PARAMETER-SET-VALUE-DOUBLE-ENCODES`'s failure mode cannot recur.
- `segments` returns `Result`. Its `Err` arm is unreachable for encoder output and must fall back
  to the single `StringParameter` token, not `unwrap` (library code).
- One consumer exists (`liquers-lib` egui widget), and it already has an `Entity` arm. No binding
  exposes `StyledQueryToken`.
- The decision is the only open item. A "no" turns this into the rendering half of a larger
  design rather than discarding it.

## Phase 2: Architecture

### Solution

Add one private helper in `liquers-core/src/query.rs` that splits an encoded token into
`(is_entity, &str)` pieces with `crate::escape::segments`, and use it in both
`ActionParameter::render` and `ActionParameter::styled_tokens` for the `String` arm. Literal pieces
go through `style.string_parameter` / `StyledQueryToken::StringParameter`, entity pieces through
`style.entity` / `StyledQueryToken::Entity`. The `Link` arm is unchanged.

Rejected: storing segments in the AST (the issue's options 1-3; wide API break, and option 3 drifts),
unless the leading decision goes the other way. Rejected: re-parsing in the egui widget (puts
grammar knowledge in `liquers-lib`).

Known issues: none blocking. `QUERY-AST-DISCARDS-ENTITIES` is the source.

### Changes

`liquers-core/src/query.rs`:

```rust
/// The literal and entity pieces of an encoded parameter token, in order.
/// Falls back to one literal piece if `segments` refuses the text (unreachable for
/// `encode_token` output).
fn token_pieces(encoded: &str) -> Vec<(bool, &str)>;
```

- `impl QueryRenderer for ActionParameter` `render`, `String` arm: concatenate
  `style.entity(piece, position)` or `style.string_parameter(piece, position)` over
  `token_pieces(&encode_token(s))`.
- `styled_tokens`, `String` arm: map the pieces to tokens, apply `to_highlight_if_matching(p,
  position)` to each, and return `Either::Left(tokens.into_iter())` (the `Left` type changes from
  `Once` to `vec::IntoIter`; the trait returns `impl Iterator`, so no caller sees it).
- An empty parameter yields one empty `StringParameter`, as today (AC-4): `token_pieces("")`
  returns `[(false, "")]`.

`TokenSegment::Entity { spelling, .. }` borrows from the input, so the pieces borrow from the local
`String`; the tokens own their text (`to_owned`), as now. Sync, no allocation beyond one `Vec` per
parameter. No error paths change.

Documents: DOC-02 says styled rendering is out of scope, so no reference change. The issue gets a
resolution; `specs/issues/QUERY-AST-DISCARDS-ENTITIES.md` keeps the segment-list analysis as
history.

### Risks

| Assessment | Finding |
|---|---|
| Files likely to change | `liquers-core/src/query.rs` only |
| Existing tests likely to change | none: the `encode_action_request` and `encode_link_action_parameter` tests compare concatenated text and rendered text with the trivial style, which AC-2 keeps identical |
| Compatibility | `StyledQuery.tokens` gains more tokens for escaped parameters; the egui widget already handles `Entity` |
| Performance | `segments` per string parameter on render only; not on the parse or encode path |
| Recovery | revert the two arms |
| Certainty | high; `escape::segments` doc tests already cover the splitting |

## Phase 3: Examples and Tests

### Examples

| Parameter value | Encoded | Tokens |
|---|---|---|
| `12:30` | `12~ncolon~30` | `S("12") E("~ncolon~") S("30")` |
| `😀` | `~U1F600~` | `E("~U1F600~")` |
| `a/b c` | `a~/b~.c` | `S("a") E("~/") S("b") E("~.") S("c")` |
| `-5` | `~5` | `E("~5")` |
| `hello` | `hello` | `S("hello")` |
| (empty) | (empty) | `S("")` |

Queries used, validated with `liquers-validate --no-registry`: `filter-12~ncolon~30-~U1F600~`
(encoded unchanged) and `f-a~/b~.c` (encoded unchanged).

### Tests

New unit tests in `liquers-core/src/query.rs` `mod tests`:

- `styled_tokens_split_entities_in_string_parameter` (AC-1): the problem example's query; assert
  the exact token sequence for the parameters, including `Entity("~ncolon~")` and
  `Entity("~U1F600~")`.
- `styled_tokens_concatenate_to_encode` (AC-2): for each row of the examples table, and for
  `filter-12~ncolon~30-~U1F600~`, the concatenated tokens and `render(&TrivialQueryRenderStyle)`
  both equal `encode()`.
- `dark_ansi_render_wraps_entities` (AC-3): render `f-12~ncolon~30` with
  `DarkAnsiQueryRenderStyle`; the output contains the entity begin marker immediately before
  `~ncolon~` and the parameter begin marker before `12`.
- `plain_and_empty_parameters_are_one_token` (AC-4): `hello` and the empty parameter of `f-`
  yield one `StringParameter` each.
- `highlight_covers_every_piece_of_a_parameter` (AC-5): `styled_tokens` with the parameter's
  position returns only `Highlight` tokens for that parameter's three pieces.

```bash
cargo test -p liquers-core --lib query::tests
```

## Phase 4: Implementation Plan

### Steps

- [ ] 1. `liquers-core/src/query.rs` — add `token_pieces` beside `impl QueryRenderer for ActionParameter` — `cargo check -p liquers-core`
- [ ] 2. `liquers-core/src/query.rs` `ActionParameter::styled_tokens` `String` arm — emit pieces; add `styled_tokens_split_entities_in_string_parameter`, `plain_and_empty_parameters_are_one_token`, `highlight_covers_every_piece_of_a_parameter` — `cargo test -p liquers-core --lib query::tests`
- [ ] 3. `liquers-core/src/query.rs` `ActionParameter::render` `String` arm — style pieces; add `styled_tokens_concatenate_to_encode`, `dark_ansi_render_wraps_entities` — `cargo test -p liquers-core --lib query::tests`
- [ ] 4. `specs/issues/QUERY-AST-DISCARDS-ENTITIES.md` — `status: closed` with a resolution recording the decision (canonical highlighting) and that the segment-list AST is not pursued — `python3 scripts/docs_index.py --check`

### Validation

`cargo test -p liquers-core --lib`, then `cargo test -p liquers-lib --lib --tests` (the egui
consumer compiles against the same enum). No feature, dependency or registry change. Diff review:
one file of code, one issue file.
