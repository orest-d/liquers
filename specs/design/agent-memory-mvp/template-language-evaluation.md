# Template language evaluation — which one to reimplement, and why

Companion to [Phase 1](./phase1-high-level-design.md), written 2026-10-12 in answer to the user's
decision 7: *the template engine must interact with assets and queries, understand the query syntax
and report parse errors and warnings; that probably means reimplementing a template system; choose
the one that is most **suitable**, not the most powerful. An existing engine combined with
`simple_template` is acceptable for now, but we should know which language we will reimplement and
why.*

The interim and the target must be chosen together: templates written during the interim should
survive the reimplementation unchanged.

## 1. What a Liquers template has to do

From [`brainstorm-workflow-improvements.md`](./brainstorm-workflow-improvements.md) §4 and the
decisions of 2026-10-12:

| Need | Example |
|---|---|
| N1 Fill fields into a skeleton, with defaults and lists | Issue and design templates; note templates |
| N2 Embed the result of a query or key | A context pack's sections; a README generated block |
| N3 Iterate over records | `index.md`'s table; a list of open issues |
| N4 Conditional parts | "Design: …" only when the issue has one; an empty-state line |
| N5 Report errors at template positions, without running anything | A malformed query inside a template is a parse error at line:column, like `liquers-validate` reports for a query |
| N6 Evaluate through the asset layer | Embedded queries are assets: cached, run concurrently, and recorded as dependencies of the rendered result, so it is invalidated when they change |
| N7 Coexist with the content | Markdown, YAML front-matter, shell examples, Rust code blocks, and documents *about* templates |

N5 and N6 are what rule out using an existing engine as the final answer: an off-the-shelf engine
sees a query as an opaque string or a function call, cannot point a query's parse error at its
position in the template, and renders synchronously, outside the asset manager. The language can
still be an existing one; the **implementation** has to be ours, with tag contents handed to the
query parser.

## 2. Criteria for suitability

| # | Criterion | Why it matters here |
|---|---|---|
| C1 | **Where the logic lives** | Liquers computes in commands and composes with queries. A template language with its own expression language (arithmetic, comparisons, filters, macros) is a second, competing computation model with its own errors and semantics |
| C2 | **Size of the specification** | A reimplementation is only as hard as the language is large. A small language with a written specification can be implemented completely and checked |
| C3 | **A conformance test suite** | The reimplementation has an oracle, as `docs_index.py` is for `ns-mem` |
| C4 | **Room for queries** | A natural, unambiguous place for a query inside a tag, so that the tag content can go straight to `parse_query` |
| C5 | **Delimiter collisions** | With the content (N7) and with the query syntax itself (`-`, `/`, `~`, `$`) |
| C6 | **Familiarity** | Agents and people will write templates; a language they already know costs nothing to teach |
| C7 | **Portability** | `liquers-py` and `liquers-web` (JavaScript) may need to render the same template; implementations should exist there |
| C8 | **Safety** | A template cannot loop forever or do I/O on its own |

Measured for C5 on this repository's Markdown (`specs/`, `.claude/`, 2026-10-12): `$` occurs **341
times in 85 files** (shell variables and `$(…)` in examples), `{{` 31 times in 12 files, `{%` and `<%`
never. Rust sources contain `{{` 72 times (format-string escapes).

## 3. Candidates

| Language | Logic (C1) | Spec (C2) | Tests (C3) | Room for queries (C4) | Collisions (C5) | Familiar (C6) | Portable (C7) |
|---|---|---|---|---|---|---|---|
| **`simple_template`** (in core) | None — text and `$query$` only | Tiny; ours | Ours | ✔ the whole span is a query | ✘ `$` is everywhere in Markdown (341×); every one must be written `$$` | ✘ unique | ✘ Rust only |
| **Mustache** | Logic-less: interpolation, sections (loop / conditional on truthiness), inverted sections, partials, comments, set-delimiter | **Small, written** (`mustache/spec`) | ✔ **official YAML conformance suite** per module | ✔ partials (`{{> name}}`) are resolved by a loader by design — a name can be a key or a query; section names can be given a query sigil | Medium (`{{` 31×); **set-delimiter `{{=<% %>=}}`** per template resolves any collision | ✔ high | ✔ implementations in every language |
| **Handlebars** | Mustache + block helpers, `else`, paths, subexpressions | Medium; defined by handlebars.js | Partly (mustache suite + handlebars.js tests) | ✔ helpers could be commands | As Mustache | ✔ high | ✔ |
| **Liquid** | Tags (`if`, `for`, `assign`, `case`) + filter chains `{{ x \| f \| g }}` | Medium; defined by Shopify's implementation | Partly (a golden-file suite exists, not a formal spec) | ✔ filter chains resemble action chains | `{{`, `{%` | Medium | Medium |
| **Jinja** (minijinja, Tera) | A full expression language: arithmetic, comparisons, tests, filters, macros, inheritance, `set` | **Large**; defined by Python's jinja2 | ✘ no formal suite | Via functions (`query("…")`) — a string inside an expression, so a query's error position is two levels deep | `{{`, `{%` | ✔ very high | Medium |

## 4. Assessment

**Jinja is the most powerful and the least suitable.** Its expression language (C1) duplicates what
queries and commands already do: a comparison or a count in a template is a computation Liquers
cannot cache, version, test as a command, or see in a plan. It has no specification or conformance
suite (C2, C3), so "reimplementing Jinja" means "reimplementing jinja2's behaviour", open-ended. And
a query inside it is a string argument of a function call, so the template parser does not know it is
a query. minijinja is an excellent engine to *use*; it is the wrong language to *reimplement*.

**Liquid and Handlebars sit in the middle.** Liquid's filter chain is the closest in spirit to a
Liquers action chain, which is attractive, but it also brings tags with their own logic and has no
formal specification. Handlebars is Mustache plus helpers; the helpers are what Liquers would
implement as commands anyway.

**Mustache is the most suitable.**

- **Logic-less is the Liquers position (C1).** Everything a template needs to decide — a count, a
  filter, an ordering — is computed by a command and arrives as data; the template only places it.
  That keeps every computation visible in a plan, cached as an asset, and testable as a command.
- **Small and specified, with a test suite (C2, C3).** The core modules (interpolation, sections,
  inverted, partials, comments, delimiters) are a few pages; the official YAML suite is the oracle a
  reimplementation runs against, the way `ns-mem` runs against `docs_index.py`.
- **It already has the extension point Liquers needs (C4).** Partials are resolved by a loader the
  host supplies; making the partial name a key or a query is using the spec, not bending it.
- **Collisions are solvable inside the language (C5).** The set-delimiter tag lets a template that
  discusses Mustache, or contains `{{`, switch delimiters locally.
- **Familiar and portable (C6, C7).** Mustache renders identically in Python and JavaScript, which
  matters for `liquers-py` and `liquers-web`.

The costs, stated honestly: Mustache cannot compare (no `if x > 3`) — such a decision becomes a
command that adds a boolean to the data; and its truthiness rules (empty list and empty string are
false) must be the spec's, not Rust's.

## 5. The target: Mustache with query tags

A reimplementation in `liquers-core` (beside `parse.rs`, where `simple_template` lives), sketched for
Phase 2:

| Form | Meaning |
|---|---|
| `{{name}}`, `{{{name}}}`, `{{#name}}…{{/name}}`, `{{^name}}…{{/name}}`, `{{! comment }}`, `{{=<% %>=}}` | The Mustache spec, unchanged, over the data model (the state's JSON, plus named arguments) |
| `{{> key or query}}` | Partial: the text of the evaluated key or query, itself rendered as a template if it is one (spec partial semantics) |
| `{{$ query}}` | Interpolate the text result of a query (the role of `simple_template`'s `$…$`) |
| `{{#$ query}}…{{/$}}` | A section over a query's result: records iterate row by row, an object is the context, a boolean decides |

- **Parsing (N5):** the template parser recognises tags; the content of every `$` and `>` tag is
  passed to `parse_query` with its offset, so a query error is reported at the template's line and
  column. A template can be validated offline, and `liquers-validate` can grow a `--template` mode.
- **Evaluation (N6):** all query tags are collected, evaluated through the environment's asset
  manager concurrently, and recorded as dependencies (and volatility) of the rendered result —
  the fix `SIMPLE-TEMPLATE-HAS-NO-COMMAND` asks for, done once for both forms.
- **`simple_template` compatibility:** `$query$` stays accepted where it is today; a template can be
  converted mechanically (`$q$` → `{{$ q}}`).
- **Conformance:** the official Mustache suite for the spec part; query tags tested as an extension.

The `{{$ …}}` sigil is a proposal. It collides with no Mustache tag type (Mustache's optional
inheritance module uses `{{$name}}` for blocks; Liquers would not adopt that module, and a block name
cannot start a query, so the two are distinguishable — Phase 2 decides).

## 6. The interim

Until the reimplementation exists, templates should already be **Mustache**, so they survive it:

- **`render-<template key>`**: an existing Mustache engine over the state's JSON, followed by an
  `expand` pass over `$query$` spans — the ranees-liquers pattern (`mustache` then `simple_template`),
  where rendering first lets data generate queries.
- **The crate** is chosen in Phase 2 by spec conformance and maintenance: ranees used `mustache` 0.9
  (old, unmaintained); `ramhorns` and others are candidates; Handlebars in strict mode renders the
  Mustache subset. Not minijinja: templates written in Jinja would have to be rewritten.
- **`expand`** fixes `simple_template`'s evaluator (asset manager, dependencies, concurrency) — work
  the reimplementation reuses.

## 7. Decision asked of the user

Confirm **Mustache with query tags** as the language to reimplement, and a Mustache-compatible
interim (`render` + `expand`) rather than minijinja. Phase 1 records it as the recommendation of open
question 7.

Sources: [Mustache specification and conformance suite](https://github.com/mustache/spec) ·
[Handlebars](https://handlebarsjs.com/) · [Liquid](https://shopify.github.io/liquid/) ·
[minijinja](https://github.com/mitsuhiko/minijinja) · [ramhorns](https://docs.rs/ramhorns) ·
ranees-liquers `expand` / `mustache` (`orest-d/ranees-website`, `ranees-liquers/src/commands.rs`).
