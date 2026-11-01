---
id: TEMPLATE-HAS-NO-MDBOOK-INCLUDE-SYNTAX
kind: feature
title: Liquers templates cannot read mdBook's {{#include}} syntax
status: draft
priority: P3
complexity: M
area: [lib/commands, docs]
design:
created: 2026-11-01
github:
---
## Problem

mdBook chapters include files with `{{#include path}}`, optionally narrowed to an anchor
(`path:name`, between `ANCHOR: name` / `ANCHOR_END: name` comments) or a line range
(`path:10:20`), plus `{{#rustdoc_include}}`. In Liquers' Mustache, `{{#include}}` opens a section
named `include` that needs a `{{/include}}`, so such a chapter fails or renders wrongly. Liquers
has its own includes — partials `{{> key}}` and `{{>$ query}}`, and query tags such as a section
read through `ns-mem/read` — but they are a different syntax.

## Impact

`design/user-documentation-guides/` therefore forbids `{{#include}}` in chapters (`tooling.md`
§5), and an existing mdBook book cannot be expanded by Liquers unchanged. Supporting the syntax
would let a chapter's includes work under both plain mdBook and a future Liquers documentation tool.

## Expected behaviour

Owner's suggestion, 2026-11-01. `{{#include <key>}}` with an argument after a space is not a
valid Mustache section name, so it can be recognised unambiguously as an include tag (no closing
tag): the key resolved like a partial's, as a dependency, read as text and not rendered; with
`:anchor` and `:start:end` narrowing as mdBook defines them. Open: whether the included text is
rendered as a template (mdBook does not), whether `{{#rustdoc_include}}` and `{{#playground}}` are
supported, and whether a heading-based selector (`#Heading`, like `ns-mem/read`'s section) is
added beyond mdBook's forms.

## Discovery

Raised by the owner on 2026-11-01 while choosing mdBook for `design/user-documentation-guides/`.
