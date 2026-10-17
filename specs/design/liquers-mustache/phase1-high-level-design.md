# Phase 1: High-Level Design — liquers-template, a query-aware template language

## Purpose

A template language that is part of Liquers: Mustache's familiar, logic-less tags combined with
native Liquers capabilities — tags that evaluate queries through the asset layer, Liquers values as
template data, and **parametrized queries** with `{{…}}` inside them, in two forms: hygienic
`{{$ …}}`, which fills the query's AST, and classic `{{$$ …}}`, which expands the text first. Its
parser shadows the query parser (nom, `Position`), so a malformed query is reported at its line and column in the template. Each query tag either **evaluates** its query (the
default) or **applies** it to the current context as input state, so one render command serves
both, and any value implementing a core data trait (`DataNode`) can be the template's data. It lives in a new crate,
`liquers-template` (renamed from `liquers-mustache` on 2026-10-17), whose Mustache syntax is the first of
possibly several; `liquers-lib` provides the commands.

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
changes. Rendered with `{"area": "core/store"}` as its input state, it renders that area, and
rendered with `{"area": "core/store"}` as its `data` argument it renders the same.

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
- **AC-7** Apply mode, per tag
  WHEN a query tag is written in its apply form, `{{$. q}}` (or `{{$.name q}}`, open question 11)
  THEN `q` is applied with the current context (or the value of `name`) as its input state, through
  `Context::apply_ad_hoc` (ad hoc, uncached); the same holds for `{{$$. q}}` and for the section and partial forms
- **AC-8** Evaluate mode is the default
  WHEN a query tag has no apply marker (`{{$ q}}`)
  THEN `q` is evaluated with no input state, and the template's own data (the input value of the
  render command, or its `data` argument) affects it only through its placeholders
- **AC-9** Validation without evaluation
  WHEN a template is validated
  THEN all errors and warnings (unknown command, malformed partial key, unused section close) are reported
  with positions, and no query, store read or command runs
- ~~**AC-10** Highlighting~~ Withdrawn 2026-10-17 at the maintainer's request: moved, with its
  design, to `TEMPLATE-HAS-NO-HIGHLIGHTER`.
- **AC-11** Commands (revised 2026-10-17)
  WHEN `liquers-lib` is built with its `templates` feature
  THEN `render` renders the template given as its first argument over the state, `expand` renders
  the state as a template, and `validate` checks the state as a template; `data` adds named values
  above the state; results are ordinary assets (text whose media type follows the result's filename, `text/plain` without one). Evaluate
  versus apply is chosen by each tag (AC-7, AC-8), so there are no separate evaluate and apply
  commands
- **AC-12** Partials from the store
  WHEN a template contains `{{> key}}`, or a dynamic partial `{{>$ query}}` / `{{>$$ query}}`
  THEN the referenced text is loaded through the asset layer, rendered as a template, and recorded as
  a dependency; a cycle of partials is an error, not a hang
- **AC-13** Portable build
  WHEN `liquers-template`, and `liquers-lib` with its `templates` feature, are built for
  `wasm32-unknown-unknown` (`scripts/check-build-matrix.sh`)
  THEN both compile, and `liquers-template`'s parser and renderer tests pass under Node
- **AC-14** Parser parity with `liquers-core`
  WHEN a query with no placeholder is parsed by `liquers-core`'s `parse_query` and by
  `liquers-template`'s query-template parser, over a corpus (the core parser's tests, the queries in
  `specs/`, recipes and the registry, and generated queries)
  THEN both accept the same queries, the template converts to a `Query` equal to the core result,
  positions included, and both reject the same queries at the same position
- **AC-15** Any value as template data
  WHEN the context is a type that implements the `DataNode` (sync) or `AsyncDataNode` (async) trait of
  [`data-node-interface.md`](./data-node-interface.md): a core `Value`, a `MetadataRecord`, a record
  view or source, a polars frame, a store folder
  THEN names, sections and placeholders resolve through the trait with no conversion of the whole
  value, and a sync-only consumer given an async-only value reports an error instead of blocking
- **AC-16** Non-scalar placeholder as a parameter override
  WHEN a whole P1 placeholder in a hygienic tag expands to a non-scalar value (a list, a map,
  records) in the query's last action
  THEN the value is passed as that argument's override, and the query runs through
  `Context::apply_ad_hoc` rather than `get_dependency_state`; in an earlier action it is an error at the
  placeholder, since overrides reach only the last action (`RECIPE-OVERRIDE-ONLY-LAST-ACTION`)
- **AC-17** Indexing into data (added 2026-10-17 at the Phase 2 review)
  WHEN a name indexes a list (`{{items.0.title}}`) or uses a bracket lookup (`{{x[y]}}`, `x[0]`,
  `x["a b"]`)
  THEN a dotted index resolves as Mustache does, empty when absent, and a bracket resolves `y`,
  then indexes a list or looks up a map key by `x`'s kind, and an absent element is an error
- **AC-18** Error modes (added 2026-10-17 at the Phase 2 review)
  WHEN a tag fails while rendering (a query, a lookup, a partial, or a tag that does not parse)
  under `errors` `strict`, `expand` (or `expand_html`, `expand_md`) or `silent`
  THEN strict fails with the first error; expand renders the error in place of the tag and silent
  renders nothing, both logging every error as a warning; an unclosed section or unterminated tag
  fails in every mode
- **AC-19** Template commands (added 2026-10-17)
  WHEN a command is registered as an alias of `render` whose head parameter is a template link
  (`pack/open_issues` → `render` with `-R/templates/open_issues.md`)
  THEN a query naming the alias renders that template over its state, the template and the alias
  are dependencies of the result, and an edit to the template invalidates it

Non-goals: a template editor (`NO-TEMPLATE-EDITOR-UI`); Mustache's optional modules (lambdas,
inheritance) — lambdas may later map to commands; logic beyond Mustache (comparisons, arithmetic):
such decisions are commands that add data; replacing the query language.

## Core Interactions

- **Query:** a shadow query parser in `liquers-template` produces a `QueryTemplate`, the query AST
  with placeholder nodes in positions P1–P6 ([`query-placeholders.md`](./query-placeholders.md)),
  kept equal to the core parser by AC-14. `liquers-core`'s `Query` is not changed.
- **Assets:** a query tag with no apply marker and no override goes through
  `Context::get_dependency_state` (cached, a dependency of the result); an apply form or an override
  goes through a new `Context::apply_ad_hoc` (ad hoc, uncached, its dependencies propagated). Partials are dependencies of the result.
- **Data:** a new `DataNode` / `AsyncDataNode` pair in `liquers-core` lets any value be template
  data, and is meant to replace the store pipeline's hard-coded field access
  (`STORE-HANDLER-FIELD-ACCESS-HARDCODED`).
- **Commands:** a template command set in `liquers-lib`; `simple_template` is subsumed.
- **Value types:** a parsed template may become a value (cached AST); decided in Phase 2.
- **UI:** none; highlighting is `TEMPLATE-HAS-NO-HIGHLIGHTER` and an editor `NO-TEMPLATE-EDITOR-UI`.

## Crate Placement

New crate **`liquers-template`**, depending on `liquers-core` only (nom and nom_locate are already
core dependencies): grammar, AST, positions, diagnostics, instantiation and the renderer, which
reaches queries and partials through a `TemplateHost` trait. **`liquers-lib`**: the commands
(feature `templates`), `ContextHost` (the host over a command's `Context`), the store-folder node,
and the mapping of `ExtValue` (records, polars frames) into template data. **`liquers-core`** gains
`data_node.rs` (open question 10), with its implementations for `Value`, `MetadataRecord` and
`serde_json::Value`, plus three additions settled in Phase 2: `ValueInterface::data_node`,
`Context::apply_ad_hoc` and `ArgumentInfo::with_default_link`. Its parser is not changed (open question 8): the template crate has its own
complete query parser, built on the public `escape` module, `Position` and `StyledQueryToken`,
rather than opening `parse.rs`, whose combinators are private. `liquers-records` implements the
traits for `RecordView` (sync) and `RecordSource` (async).

## Documentation Intent

- Reference: new `specs/reference/TEMPLATE_LANGUAGE.md` — the grammar, tags, data model, escaping,
  modes, diagnostics and conformance.
- Guide: new `specs/guides/TEMPLATE_GUIDE.md` — writing, validating and applying templates, with a
  context pack as the worked example, and template commands (an alias of `render`, AC-19), which
  `COMMAND_REGISTRATION_GUIDE.md`, `COMMAND_DESIGN_GUIDE.md` and `COMMAND_ALIASES.md` refer to.
- Documents to update: `specs/reference/api/DOC_02_QUERY_LANGUAGE_REFERENCE.md` (parametrized
  queries; `simple_template` status), `specs/reference/PROJECT_OVERVIEW.md` (a new crate),
  `specs/reference/VALUE_TYPE_SYSTEM.md` (`DataNode`), `DOC_04_ENVIRONMENT_CONTEXT_EVALUATION.md`
  (`apply_ad_hoc`), the three command documents above, `CLAUDE.md` project structure and dependency
  flow, `specs/README.md`. The full list is Phase 2's documentation table.

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
2. **[accepted 2026-10-17] Which positions are hygienic.** Specified in
   [`query-placeholders.md`](./query-placeholders.md). The positions are action parameters (P1),
   `ns-` namespaces (P2), action names (P3), key segments (P4), output filenames (P5) and the
   inside of link parameters (P6), each whole or as a fragment. Realms, header parameters and whole
   actions or segments are left to `{{$$ …}}`. P3–P5 have no escape form, so a value that is invalid
   there is an error. **2a [decided 2026-10-17]:** a non-scalar value in a whole P1 position is a
   parameter override (AC-16), and anywhere else it is an error.
3. **[accepted 2026-10-17] Escaping of interpolation in text** follows the output media type: none
   for text and Markdown, HTML for HTML, with `{{{x}}}` always raw.
4. **[accepted 2026-10-17] Template data from Liquers values.** Records iterate as rows of objects,
   and an entry's metadata is reachable under one reserved name, `{{_meta.title}}`.
5. **[accepted 2026-10-17] Query tags.** In each pair, the `$` form is hygienic and the `$$` form is classic:

   | Tag | Meaning |
   |---|---|
   | `{{name}}`, `{{{name}}}`, `{{#name}}…{{/name}}`, `{{^name}}…{{/name}}`, `{{! …}}`, `{{=<% %>=}}` | Standard Mustache over the template data |
   | `{{> key}}` | Standard partial. The name is a store key, loaded through the asset layer (AC-12) |
   | `{{$ q}}` / `{{$$ q}}` | Evaluate the query `q` and insert its text result, escaped as in 3 |
   | `{{&$ q}}` / `{{&$$ q}}` | The same, never escaped |
   | `{{#$ q}}…{{/}}` / `{{#$$ q}}…{{/}}` | Evaluate `q` and use the result as section data. Records and lists iterate, an object becomes the context, a boolean decides (AC-3) |
   | `{{^$ q}}…{{/}}` / `{{^$$ q}}…{{/}}` | Render the body when the result is empty or false |
   | `{{>$ q}}` / `{{>$$ q}}` | Dynamic partial: the result of `q` is the template text |
   | `{{$. q}}`, `{{#$. q}}`, `{{&$. q}}`, `{{>$. q}}`, and the `$$.` forms | Apply forms of the tags above: `q` gets the current context as its input state (open question 11) |
   | `{{/}}` | Closes the innermost open section of any kind (decided 2026-10-17). `{{/$}}`, `{{/$$}}` and `{{/name}}` are also accepted, and are checked against the section they close |

   A query section is closed by `{{/}}` (or `{{/$}}`), so the query is not repeated. Mustache's
   inheritance module also uses `{{$name}}`, and this design does not adopt that module.
6. **[accepted 2026-10-17] `simple_template`.** Deprecate `$query$`; Phase 2 found no templates to convert.
7. **[decided 2026-10-17] Command namespace and crate.** The namespace is `template`
   (`ns-template/render`, `expand`, `validate`), and the crate is `liquers-template`, so that other template
   syntaxes can join Mustache there later. The design folder keeps its slug `liquers-mustache`.
8. **[decided 2026-10-17] Parser.** A complete shadow implementation in `liquers-template` is
   acceptable, under parity tests (AC-14): a query without placeholders parses identically in both
   parsers, up to `Query` versus `QueryTemplate`. `liquers-core`'s parser stays private.
9. **[decided 2026-10-17] Nested braces in query tags.** Braces are balanced: the query parser
   consumes nested `{{…}}` as placeholders, and the tag ends at the first unbalanced `}}`. The rule
   applies to both `$` and `$$`.

10. **[accepted 2026-10-17] A data interface in `liquers-core`.** A sync `DataNode` and an async
    `AsyncDataNode`, joined by an `AnyNode` enum. Values in memory implement the sync trait, while
    record sources and store folders implement the async one. The renderer takes both, and sync
    consumers such as store handlers take only sync nodes. The design and the sync, async and dual
    comparison are in [`data-node-interface.md`](./data-node-interface.md). *Recommended: dual.*
11. **[accepted 2026-10-17] Apply syntax.** `{{$. q}}` applies `q` to the current context, as Mustache's
    `{{.}}` names it, and `{{$.name q}}` applies it to the value of `name`. Without the dot, `{{$ q}}`
    evaluates with no input. The marker composes with every query tag (`{{#$. q}}`, `{{$$. q}}`,
    `{{>$. q}}`). A query that itself begins with `.` is written after a space (`{{$ .x}}`), since
    `$.` always means apply. Alternatives considered: a pipe, `{{. | $ q}}`, which is a new operator
    that Mustache has nowhere else; and a section switch, `{{#.}}{{$ q}}{{/}}`, which cannot name a
    value other than the current one. *Recommended: the dot.*

## References

Sources `NO-QUERY-AWARE-TEMPLATE-LANGUAGE`, `SIMPLE-TEMPLATE-HAS-NO-COMMAND`; evaluation of languages
in `design/agent-memory-mvp/template-language-evaluation.md`; query grammar in
`liquers-core/src/parse.rs`, positions and styled tokens in `liquers-core/src/query.rs` (`Position`,
`QueryRenderStyle`, `StyledQueryToken`); the predecessor `expand` / `mustache` commands of
`ranees-liquers`; the [Mustache specification](https://github.com/mustache/spec).

## Design Readiness

Pre-approved after Phase 2 on 2026-10-17 (`proceed all`, given after the post-approval revision of
AC-11 and AC-19, so it also re-approves Phases 1–3). Decision log, tiered as in
`autonomous_bulk_design.md` §3:

| # | Item | Tier | Assumption taken |
|---|---|---|---|
| D1 | `templates` joins `liquers-lib`'s default features | accepted | On by default, like `records`, so the exported registry includes `template` |
| D2 | The Mustache spec suite is vendored (`mustache/spec`, MIT) under `liquers-template/tests/spec/` with its licence | accepted | Only the six required modules |
| D3 | `search` results go stale (`SEARCH-COMMANDS-RECORD-NO-DEPENDENCY-ON-READ-FILES`) | not blocking | Designed around; context packs over search are stale until that issue is fixed |
| D4 | A link default cannot be written in a registry overlay (`REGISTRY-QUERY-DEFAULT-NOT-WRITABLE-AS-TEXT`) | not blocking | The exported registry will carry `render`'s `cwd` default as a serialized `Query` |
| D5 | `wasm-bindgen-test-runner` may be unavailable here | not blocking | Step 12 falls back to a wasm32 `cargo check` and files the gap |
| D6 | Phase 2 assumed `get_dependency_state` makes a parent volatile when a dependency is volatile; Step 2 found it does not, by design (`design/volatility-system`, decision 4: volatility is fixed before evaluation) | not blocking | `apply_ad_hoc` matches it and passes no volatility on. A render over volatile queries is volatile only when its query ends in `v` or its template command is declared `volatile`; the template guide says so. This revises AC-2's "volatility inherited", so it was put to Orest on 2026-10-17; the steps it does not touch continue meanwhile |

