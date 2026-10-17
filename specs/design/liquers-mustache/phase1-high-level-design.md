# Phase 1: High-Level Design — liquers-mustache, a query-aware template language

## Purpose

A template language that is part of Liquers: Mustache's familiar, logic-less tags combined with
native Liquers capabilities — tags that evaluate queries through the asset layer, Liquers values as
template data, and **parametrized queries** with `{{…}}` inside them, in two forms: hygienic
`{{$ …}}`, which fills the query's AST, and classic `{{$$ …}}`, which expands the text first. Its
parser shadows the query parser (nom, `Position`), so a malformed query is reported at its line and column in the template,
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
  WHEN `{{name}}` appears in a hygienic tag (`{{$ cmd-{{x}}/q}}`) in one of the positions P1–P6 of
  [`query-placeholders.md`](./query-placeholders.md), whole or as a fragment
  (`search-area~ncolon~{{area}}`), and `name` holds a value containing `-`, `/`, `~` or spaces
  THEN the placeholder is filled in the query's AST, never in its text. The structure is unchanged:
  `x = "a-b"` gives one parameter, which encodes as `cmd-a~_b/q`. A value that is not valid in a
  position with no escape form (an action name, a key segment, a filename) is an error at the
  placeholder's position
- **AC-6** Parametrized query, classic
  WHEN `{{name}}` appears in a classic tag (`{{$$ cmd-{{x}}/q}}`)
  THEN the text between `{{$$` and `}}` is expanded first, with values inserted verbatim, and is then
  parsed as a query. `x = "a-b"` gives `cmd-a-b/q`, two parameters. Parse errors are positioned in
  the expanded text, together with the tag's position in the template
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
  WHEN a template contains `{{> key}}`, or a dynamic partial `{{>$ query}}` / `{{>$$ query}}`
  THEN the referenced text is loaded through the asset layer, rendered as a template, and recorded as
  a dependency; a cycle of partials is an error, not a hang
- **AC-13** Portable build
  WHEN `liquers-mustache`, and `liquers-lib` with its `templates` feature, are built for
  `wasm32-unknown-unknown` (`scripts/check-build-matrix.sh`)
  THEN both compile, and `liquers-mustache`'s parser and renderer tests pass under Node
- **AC-14** Parser parity with `liquers-core`
  WHEN a query with no placeholder is parsed by `liquers-core`'s `parse_query` and by
  `liquers-mustache`'s query-template parser, over a corpus (the core parser's tests, the queries in
  `specs/`, recipes and the registry, and generated queries)
  THEN both accept the same queries, the template converts to a `Query` equal to the core result,
  positions included, and both reject the same queries at the same position

Non-goals: a template editor (`NO-TEMPLATE-EDITOR-UI`); Mustache's optional modules (lambdas,
inheritance) — lambdas may later map to commands; logic beyond Mustache (comparisons, arithmetic):
such decisions are commands that add data; replacing the query language.

## Core Interactions

- **Query:** a shadow query parser in `liquers-mustache` produces a `QueryTemplate`, the query AST
  with placeholder nodes in positions P1–P6 ([`query-placeholders.md`](./query-placeholders.md)),
  kept equal to the core parser by AC-14. `liquers-core`'s `Query` is not changed.
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
and the egui rendering of highlighted templates. **`liquers-core` is unchanged** (open question 8):
the template crate has its own complete query parser, built on the public `escape` module,
`Position` and `StyledQueryToken`, rather than opening `parse.rs`, whose combinators are private.

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

Tiers: **[decided]** settled by the maintainer (date); **[accepted]** a recommendation the
maintainer let stand at review (date); **[open design]** needs the maintainer's
decision; **[proposed]** has a recommendation to accept or amend; **[detail]** can be settled in
Phase 2.

1. **[decided 2026-10-17] How `{{…}}` expands inside a query.** Two tags: hygienic `{{$ …}}` fills
   the placeholder in the query's AST, so a serialized result escapes the value
   (`x = "a-b"`: `cmd-{{x}}` becomes `cmd-a~_b`, one parameter). Classic `{{$$ …}}` expands the text,
   then parses it (`cmd-a-b`, two parameters). See [`query-placeholders.md`](./query-placeholders.md).
2. **[proposed] Which positions are hygienic.** Specified in
   [`query-placeholders.md`](./query-placeholders.md). The positions are action parameters (P1),
   `ns-` namespaces (P2), action names (P3), key segments (P4), output filenames (P5) and the
   inside of link parameters (P6), each whole or as a fragment. Realms, header parameters and whole
   actions or segments are left to `{{$$ …}}`. P3–P5 have no escape form, so a value that is invalid
   there is an error. **2a [open design]:** may a whole placeholder take a list (several parameters)
   or a `Key` (several segments)? *Recommended: yes*, in a whole P1 or P4 position only.
3. **[accepted 2026-10-17] Escaping of interpolation in text** follows the output media type: none
   for text and Markdown, HTML for HTML, with `{{{x}}}` always raw.
4. **[accepted 2026-10-17] Template data from Liquers values.** Records iterate as rows of objects,
   and an entry's metadata is reachable under one reserved name, `{{_meta.title}}`.
5. **[proposed] Query tags.** In each pair, the `$` form is hygienic and the `$$` form is classic:

   | Tag | Meaning |
   |---|---|
   | `{{name}}`, `{{{name}}}`, `{{#name}}…{{/name}}`, `{{^name}}…{{/name}}`, `{{! …}}`, `{{=<% %>=}}` | Standard Mustache over the template data |
   | `{{> key}}` | Standard partial. The name is a store key, loaded through the asset layer (AC-12) |
   | `{{$ q}}` / `{{$$ q}}` | Evaluate the query `q` and insert its text result, escaped as in 3 |
   | `{{&$ q}}` / `{{&$$ q}}` | The same, never escaped |
   | `{{#$ q}}…{{/$}}` / `{{#$$ q}}…{{/$$}}` | Evaluate `q` and use the result as section data. Records and lists iterate, an object becomes the context, a boolean decides (AC-3) |
   | `{{^$ q}}…{{/$}}` / `{{^$$ q}}…{{/$$}}` | Render the body when the result is empty or false |
   | `{{>$ q}}` / `{{>$$ q}}` | Dynamic partial: the result of `q` is the template text |

   `{{/$}}` closes the innermost open query section, so the query is not repeated. Mustache's
   inheritance module also uses `{{$name}}`, and this design does not adopt that module.
6. **[accepted 2026-10-17] `simple_template`.** Convert the existing templates and deprecate `$query$`.
7. **[proposed] Command namespace.** *Recommended: `mustache`* (`ns-mustache/render`, `apply`,
   `validate`). It names the language and the crate, and it leaves `template` free for a generic
   concept. The earlier `tpl` read like a sibling of `pl` (polars). Confirmed at the Phase 2 gate with
   the other namespaces.
8. **[decided 2026-10-17] Parser.** A complete shadow implementation in `liquers-mustache` is
   acceptable, under parity tests (AC-14): a query without placeholders parses identically in both
   parsers, up to `Query` versus `QueryTemplate`. `liquers-core`'s parser stays private.
9. **[decided 2026-10-17] Nested braces in query tags.** Braces are balanced: the query parser
   consumes nested `{{…}}` as placeholders, and the tag ends at the first unbalanced `}}`. The rule
   applies to both `$` and `$$`.

## References

Sources `NO-QUERY-AWARE-TEMPLATE-LANGUAGE`, `SIMPLE-TEMPLATE-HAS-NO-COMMAND`; evaluation of languages
in `design/agent-memory-mvp/template-language-evaluation.md`; query grammar in
`liquers-core/src/parse.rs`, positions and styled tokens in `liquers-core/src/query.rs` (`Position`,
`QueryRenderStyle`, `StyledQueryToken`); the predecessor `expand` / `mustache` commands of
`ranees-liquers`; the [Mustache specification](https://github.com/mustache/spec).
