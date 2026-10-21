# Phase 1: High-Level Design — query-builder

## Purpose

Code that assembles a query from data (UI links, recipe generators, language integrations,
scripts) should never concatenate query text. Give it a supported, validating builder in
`liquers-core`, and a command-line tool, `liquers-encode`, that is the encode-direction counterpart
of `liquers-validate`. Both are guaranteed to produce text that parses back to exactly what was put
in, or to fail with a typed error naming the token that cannot be represented.

## Problem Example

Verified at HEAD (18732dd). A script wants "the file `data/report.csv`, filtered on the user's
value `12:30`, then `to_text`".

Concatenation gets it wrong: `format!("-R/data/report.csv/-/filter-{v}/to_text")` yields
`…/filter-12:30/to_text`, which fails to parse, and forgetting the `/-/` turns the actions into key
segments.

Hand assembly of the AST avoids escaping bugs but has no validation of the raw-emitted positions.
`Query { segments: vec![Resource(..key "data/report.csv"..), Transform(..)] }` with a transform
segment that has no header encodes as `-R/data/report.csv/filter-12~ncolon~30/to_text`, which does
not parse (a `~` cannot appear in a key). With a value that needs no escaping it is worse:
`-R/data/report.csv/filter-x/to_text` parses as `GetAsset[data/report.csv/filter-x/to_text]`
(checked with `liquers-validate`). An action name, key segment or filename
taken from data (`"my file.csv"`) is emitted raw and gives unparseable text. DOC-02 states this:
"Construction methods do not enforce parser grammar."

What should happen:

```rust
let q = QueryBuilder::new()
    .resource(["data", "report.csv"])?
    .action("filter", ["12:30"])?
    .action("to_text", [] as [&str; 0])?
    .build();
assert_eq!(q.encode(), "-R/data/report.csv/-/filter-12~ncolon~30/to_text");
assert_eq!(parse_query(&q.encode())?, q);
```

`.resource(["data", "my file.csv"])` returns a `ParameterError` naming `"my file.csv"` and the
position kind ("a key segment"). On the command line:

```text
liquers-encode --resource data --resource report.csv --action filter --arg 12:30 --action to_text
-R/data/report.csv/-/filter-12~ncolon~30/to_text
```

## Scope and Acceptance Criteria

- **AC-1** Built queries round-trip
  WHEN a query is built with `QueryBuilder` from any string parameter values and valid tokens
  THEN `parse_query(&q.encode())` equals `q`, and every string parameter decodes to the value given
- **AC-2** Unrepresentable tokens are typed errors
  WHEN an action name, namespace, key segment, resource selector or filename is not valid in its position (including `.` and `..` as a key segment)
  THEN the builder step returns `ErrorType::ParameterError` naming the value and the position kind, and no query is produced
- **AC-3** Structure is explicit
  WHEN actions follow a resource
  THEN the transform segment carries an explicit `-/` header, so the actions are never read as key segments
- **AC-4** An existing `Query` can be checked
  WHEN `Query::check_encodable` is called on a programmatically assembled query
  THEN it returns `Ok` exactly when `parse_query(&q.encode())` reproduces `q`, and otherwise the AC-2 error for the first offending token
- **AC-5** The CLI encodes from separate arguments
  WHEN `liquers-encode` is given resource segments, actions and their arguments as separate shell arguments (values may start with `-`)
  THEN it prints the encoded query on stdout and exits 0; feeding the output to `liquers-validate --no-registry` reports `Ok` with `encoded` equal to it
- **AC-6** The CLI reports errors like the validator
  WHEN an argument cannot be represented, or the invocation is malformed
  THEN it exits 1 (with the AC-2 message on stderr) or 2 respectively, with empty stdout
- **AC-7** One definition of each token class
  WHEN the `liquers-template` query placeholders validate an action name, key segment or filename
  THEN they call the same `liquers_core` predicates as the builder and the parser
- **AC-8** An empty parameter is encodable
  WHEN `encodeParam("")` is called in `liquers-web`, or the builder is given an empty parameter
  THEN the result is the empty token, and `cmd-` parses to one empty parameter, as the grammar already allows

Non-goals: Python and JavaScript builder classes (follow-up `QUERY-BUILDER-NOT-IN-BINDINGS`, filed with this design); a
second escaping scheme; non-ASCII key segments (they follow `resource-name-unicode`'s decision
automatically, because the builder calls the parser's predicate); `-R-key/`, `cwd` and other selector
semantics beyond writing the selector token; link parameters built from text (a link takes a
`Query`, which the builder checks recursively).

## Core Interactions

- Query: new `liquers_core::query_builder` module; `Query::check_encodable`; token predicates in
  `liquers_core::escape` used by `parse.rs`.
- Validate: a second `cli` binary, `liquers-encode`.
- Template: `liquers-template/src/query/instantiate.rs` validators delegate to core.
- Web: `liquers-web/src/encode.rs` `encode_param` stops refusing the empty string.

## Crate Placement

`liquers-core` (the builder sits beside `Query`, has no dependencies and must be usable from every
integration). The CLI is behind the existing `cli` feature, like `liquers-validate`.

## Documentation Intent

- Reference: `DOC_02` §Programmatic construction is not validation (the builder and
  `check_encodable` validate); `PROJECT_OVERVIEW.md` Technical Debt (the raw-emission caveat now
  applies only to code that bypasses both).
- Guide: `QUERY_ESCAPING_GUIDE.md` §Escaping a value programmatically (builder first) and a short
  `liquers-encode` section; `TEMPLATE_LANGUAGE.md` names the shared predicates.
- Other documents: `CLAUDE.md` §Validating queries gains one line on `liquers-encode`; the
  `liquers-validate` skill's SKILL.md names it as the encode direction.
- Documents to update: the four in `affects_docs`, plus `CLAUDE.md` and the skill.

## Design Readiness

- **Readiness:** needs-decision
- **Automatic fixing:** not-eligible — new `pub` module, type and binary in `liquers-core` (rules 3
  and 4), `L`.
- **Leading issue:** **Open design question — CLI shape.** Recommended: a new binary
  `liquers-encode` with ordered, repeatable options (`--resource SEG`, `--selector NAME`,
  `--rooted`, `--action NAME`, `--arg VALUE`, `--filename NAME`). Each value is one shell argument,
  so the shell does the quoting and a value such as `-5` is passed with `--arg=-5` or
  `--arg -5`. The alternative is an `--encode` mode of `liquers-validate`, whose flags and output
  envelope are built around a list of queries. Merging the two would make both harder to read.
- **Explanation:** the encoder half is sound since `parameter-entity-escaping` (`encode_token` is
  total), so the issue's requirement 3 ("raise a typed error for values they cannot represent")
  now applies only to the raw-emitted positions. That is what the builder validates.
- **Open questions:**
  1. **Proposed resolution — error type.** `ErrorType::ParameterError` (as `encode_param` already
     uses), not `ParseError`: nothing was parsed.
  2. **Proposed resolution — `.` and `..` segments.** `resource([...])` refuses them, so a value
     cannot climb the key tree (the template rule P4). A caller that means a relative key passes a
     parsed `Key` to `resource_key(&key)`, which allows them.
  3. **Proposed resolution — empty parameter in `liquers-web`.** `encodeParam("")` returns `""`
     instead of an error, because the grammar accepts it and the builder does (AC-8). This changes a
     documented JavaScript behaviour (`web_encode_param_refuses_the_empty_string`).

## Design Dependencies

- `parameter-entity-escaping` (complete, E4): **origin** — made `encode_token` total, which was this
  issue's blocker.
- `QUERY-TOKEN-CLASSES-TRUNCATE-NON-ASCII` (issue filed with this design, `S`, no design):
  **requires**, soft. Its fix puts the ASCII token predicates in `escape.rs`, which this design
  reuses. If it has not landed, Step 1 here does that work and closes it. It is kept separate
  because it is eligible for automatic fixing and this design is not (E1).
- `resource-name-unicode` (this run): **overlaps** (T2 — what a valid key segment is). The builder
  calls `escape::is_resource_name`, so whichever lands second needs no change to the other. Merging
  would exceed `L` (E2).
- `liquers-mustache` (complete, E4): its query placeholders already validate filled tokens; AC-7
  makes them share the predicates.

## Consolidated Findings

- The builder is thin. String parameters need nothing new (`ActionParameter::new_string` plus
  `encode`). The value it adds is validation of the five raw-emitted token classes, plus structure:
  an explicit `-/` header on a transform after a resource, without which the actions become key
  segments.
- One predicate per token class lives in `liquers_core::escape` and is used by the parser, the
  builder, `check_encodable` and `liquers-template`. Today there are two copies (`parse.rs` with the
  `c as u8` truncation, `instantiate.rs` with ASCII), and they disagree on `Ł`.
- `check_encodable` is the verifier. The builder's steps call the same token checks, so they cannot
  disagree. Its AC-4 test compares against a real `parse_query` round trip.
- The CLI re-parses its own output before printing, as a self-check. A mismatch is an internal
  error (exit 1), never silent output.
- Bindings are a follow-up issue. `liquers-web` changes only to accept the empty value.
