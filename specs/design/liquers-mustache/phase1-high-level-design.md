# Phase 1: High-Level Design — liquers-mustache, a query-aware template language

## Purpose

A template language that is part of Liquers: Mustache's familiar, logic-less tags combined with
native Liquers capabilities — tags that evaluate queries through the asset layer, Liquers values as
template data, and **parametrized queries** with `{{…}}` inside them. Its parser extends the query
parser (nom, `Position`), so a malformed query is reported at its line and column in the template,
and its highlighter extends the query's. One AST runs in two modes: **evaluate** (a document whose
queries are run) and **apply** (a function applied to a `Value`). It lives in a new crate,
`liquers-mustache`; `liquers-lib` provides the commands.

## Problem Example

A context pack for an agent: the open issues of one area, as a Markdown list, for any area.

```markdown
# Open issues in {{area}}
{{#$ -R-key/specs/issues/-/ns-search/field_extract/search-area~ncolon~{{area}}~.~_status~ncolon~closed~nast~-f-f-10}}
- [{{title}}]({{key}}) — {{priority}}
{{/$}}
{{^$ …same query…}}No open issues.{{/$}}
```

**Today:** there is no way to write this. `simple_template` (`$query$`) has no variables, sections or
loops, and no command exposes it (`SIMPLE-TEMPLATE-HAS-NO-COMMAND`). The query cannot be
parametrized: `liquers-validate` rejects
`-R-key/specs/issues/-/ns-search/field_extract/search-area~ncolon~{{area}}-f-f-10` with
"Can't parse query completely (line 1, column 66)". With the area fixed (`…search-area~ncolon~core~/store~.~_status~ncolon~closed~nast~-f-f-10`) the
query validates, but turning its records into a list needs a bespoke command. An off-the-shelf
Mustache engine would render the list, but sees the query as an opaque string: no positioned errors,
no caching or dependency tracking, no Liquers value as data.

**After:** the template parses with the query inside it (an error points into the query), `{{area}}`
inside the query becomes one encoded parameter, the query runs through the asset manager, its records
iterate in the section, and the rendered document is an asset that is recomputed when an issue
changes. Applied to `{"area": "core/store"}` it renders that area; evaluated with an `area` argument
it renders the same.

## Scope and Acceptance Criteria

- **AC-1** Mustache conformance
  WHEN the official Mustache specification suite (interpolation, sections, inverted sections,
  comments, partials, set-delimiter) runs against the renderer
  THEN every case passes, with the escaping rule of §"Open questions" 3 applied to interpolation
- **AC-2** Query interpolation through the asset layer
  WHEN a template containing `{{$ query}}` is evaluated
  THEN the query's text result is inserted, the query is evaluated through the environment's asset
  manager, and it is recorded as a dependency (and its volatility inherited) of the rendered result
- **AC-3** Section over a query
  WHEN `{{#$ query}}…{{/$}}` is rendered over a query returning records, a list, an object or a boolean
  THEN records and lists iterate (record fields are names in the section), an object becomes the
  context, a boolean decides, and the inverted form renders on an empty or false result
- **AC-4** Positioned diagnostics
  WHEN a template contains a malformed query, an unclosed section or a bad delimiter change
  THEN parsing reports an error with the template's line and column at the fault — inside the query
  for a query error — and nothing is evaluated; a query tag containing `{{…}}` (as in the problem
  example) is lexed as one tag, by the nesting rule of open question 9
- **AC-5** Parametrized query, hygienic
  WHEN `{{name}}` appears in a parameter or resource-segment position of a query — as the whole
  parameter or segment, or as a fragment inside one (`search-area~ncolon~{{area}}`) — and `name` holds
  a value containing `-`, `/`, `~` or spaces
  THEN the value is encoded into that one parameter or segment, the query's structure is unchanged,
  and its plan shows the parameter with the value in place
- **AC-6** Parametrized query, raw
  WHEN `{{{name}}}` appears inside a query
  THEN its value is spliced as query text before parsing, may add actions or segments, and the
  result is re-parsed with errors positioned in the spliced text
- **AC-7** Apply mode
  WHEN a template is applied to a `Value` — an object, a list, records, text
  THEN that value is the template's data and the result is the same as evaluating the template with
  equivalent named arguments
- **AC-8** Evaluate mode
  WHEN a template is evaluated with named arguments and no input value
  THEN every statement and query is run and the arguments are the data
- **AC-9** Validation without evaluation
  WHEN a template is validated
  THEN all errors and warnings (unknown command, unknown partial, unused section close) are reported
  with positions, and no query, store read or command runs
- **AC-10** Highlighting
  WHEN a template is rendered for display (ANSI or HTML)
  THEN template tags, data names and the queries inside tags are styled, the queries with the same
  token kinds the query highlighter uses
- **AC-11** Commands
  WHEN `liquers-lib` is built with its template feature
  THEN commands render (evaluate), apply and validate a template given by key or as the state, and
  their results are ordinary assets (text with the template's media type)
- **AC-12** Partials from the store
  WHEN a template contains `{{> key or query}}`
  THEN the referenced text is loaded through the asset layer, rendered as a template, and recorded as
  a dependency; a cycle of partials is an error, not a hang
- **AC-13** Portable build
  WHEN `liquers-mustache`, and `liquers-lib` with its `templates` feature, are built for
  `wasm32-unknown-unknown` (`scripts/check-build-matrix.sh`)
  THEN both compile, and `liquers-mustache`'s parser and renderer tests pass under Node

Non-goals: a template editor (`NO-TEMPLATE-EDITOR-UI`); Mustache's optional modules (lambdas,
inheritance) — lambdas may later map to commands; logic beyond Mustache (comparisons, arithmetic):
such decisions are commands that add data; replacing the query language.

## Core Interactions

- **Query:** the template grammar extends the query grammar; parametrized queries add a placeholder
  to the query AST in parameter and resource-segment positions (open question 1).
- **Assets:** evaluation goes through the asset manager; embedded queries and partials are
  dependencies of the result.
- **Commands:** a template command set in `liquers-lib`; `simple_template` is subsumed.
- **Value types:** a parsed template may become a value (cached AST); decided in Phase 2.
- **UI:** the highlighter's styled tokens feed the existing query renderers; no editor.

## Crate Placement

New crate **`liquers-mustache`**, depending on `liquers-core` only (nom and nom_locate are already
core dependencies): grammar, AST, positions, diagnostics, highlighter, data model over core `Value`,
and both evaluation modes over the `Environment` / asset-manager traits. **`liquers-lib`**: the
commands (feature `templates`), the mapping of `ExtValue` (records, polars frames) into template data,
and the egui rendering of highlighted templates. **`liquers-core` changes in two places:** the query
AST gains a placeholder node (open question 1), and the parser gains a **public, span-based surface**
— today every combinator in `parse.rs` is private (only `resource_path` is `pub(crate)`), so another
crate cannot extend it (open question 8).

## Documentation Intent

- Reference: new `specs/reference/TEMPLATE_LANGUAGE.md` — the grammar, tags, data model, escaping,
  modes, diagnostics and conformance.
- Guide: new `specs/guides/TEMPLATE_GUIDE.md` — writing, validating and applying templates, with a
  context pack as the worked example.
- Other documents: none.
- Documents to update: `specs/reference/api/DOC_02_QUERY_LANGUAGE_REFERENCE.md` (parametrized
  queries; `simple_template` status), `specs/reference/PROJECT_OVERVIEW.md` (a new crate), `CLAUDE.md`
  project structure and dependency flow, `specs/README.md`.

## Design Dependencies

- **Required by** `agent-memory-mvp` (context packs AC-21, `index.md` AC-22, issue and design
  templates).
- **Covers** `SIMPLE-TEMPLATE-HAS-NO-COMMAND` (attached as a source).
- **Overlaps** design [`query-link-parser`](../query-link-parser/) (complete; link parameters `~X~…~E` are the precedent for a
  query nested in a query), `VALUE-TO-JSON-PANICS-ON-NON-FINITE-FLOAT` (the data model converts values),
  `COMMAND-CANNOT-BE-RUN-WITH-A-RESTRICTED-CONTEXT` (templates from untrusted sources).
- **Spun off** `NO-TEMPLATE-EDITOR-UI`.

## Open Questions

Tiers: **[open design]** needs the user's decision; **[proposed]** has a recommendation to accept or
amend; **[detail]** can be settled in Phase 2.

1. **[open design] How `{{…}}` expands inside a query.** (a) *String*: expand the whole query as text, then parse;
   simple, but a value containing `/` or `-` silently changes the query's structure (injection), and
   errors point into the expanded text. (b) *Syntactic*: placeholders allowed only in certain
   positions — a parameter value, a resource segment, perhaps a whole action — parsed into the query
   AST and filled with **encoded** values, so a value can never change structure (hygienic, like a
   macro). (c) *Both*. *Recommended: (c), mapped onto Mustache's own escaped/unescaped distinction*:
   `{{x}}` is syntactic and encoded (hygienic), `{{{x}}}` is a raw splice before parsing, for the rare
   query that must add structure — documented as unsafe for untrusted data.
2. **[open design] Which positions are syntactic in (b)?** *Recommended:* action parameter values and
   resource segments, whole or as fragments inside them (encoded piecewise), in the MVP; action and
   namespace names later if needed.
3. **[proposed] Escaping of interpolation in text.** Mustache HTML-escapes `{{x}}` by default; Liquers documents
   are mostly Markdown and text. *Recommended:* escaping follows the template's output media type —
   none for text and Markdown, HTML for HTML — with `{{{x}}}` always raw.
4. **[proposed] Template data from Liquers values.** Records iterate as rows of objects; should an entry's
   metadata (title, description, key) be reachable, e.g. as `{{_meta.title}}`? *Recommended: yes,
   under one reserved name*, since tiers live in metadata.
5. **[proposed] Query tag syntax.** `{{$ query}}`, `{{#$ query}}…{{/$}}`, `{{> key or query}}` as proposed in
   `design/agent-memory-mvp/template-language-evaluation.md` §5, or another sigil?
   *Recommended: as proposed*; Mustache's inheritance module also uses `$`, which this design does not
   adopt.
6. **[proposed] `simple_template`.** Keep `$query$` as a compatibility syntax inside templates, or convert
   existing templates and deprecate it? *Recommended: convert* — no command uses it today, and `$`
   collides with Markdown.
7. **[detail] Command namespace and names** — e.g. `ns-tpl/render`, `apply`, `validate`.
   *Recommended:* `tpl`, confirmed at the Phase 2 gate with the other namespaces.
8. **[open design] The public parser surface of `liquers-core`.** Expose the `Span` type and the query,
   action, parameter and segment combinators, or one span-based entry point that parses a query
   starting at a given offset and returns its end and positioned diagnostics? *Recommended:* the entry
   point plus a small set of combinators — a narrower public API that keeps `parse.rs` free to change.
9. **[open design] Nested braces in query tags.** In `{{#$ …search-{{area}}…}}` the inner `}}` closes
   the outer tag under plain Mustache lexing. Balance braces inside query tags (the query parser decides
   where the tag ends), or require a set-delimiter change for such tags? *Recommended: balance* — the
   template parser hands the tag body to the query parser, which consumes nested `{{…}}` as
   placeholders, so the tag ends at the first unbalanced `}}`.

## References

Sources `NO-QUERY-AWARE-TEMPLATE-LANGUAGE`, `SIMPLE-TEMPLATE-HAS-NO-COMMAND`; evaluation of languages
in `design/agent-memory-mvp/template-language-evaluation.md`; query grammar in
`liquers-core/src/parse.rs`, positions and styled tokens in `liquers-core/src/query.rs` (`Position`,
`QueryRenderStyle`, `StyledQueryToken`); the predecessor `expand` / `mustache` commands of
`ranees-liquers`; the [Mustache specification](https://github.com/mustache/spec).
