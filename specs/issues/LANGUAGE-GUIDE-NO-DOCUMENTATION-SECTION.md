---
id: LANGUAGE-GUIDE-NO-DOCUMENTATION-SECTION
kind: feature
title: The language integration guide says nothing about writing the integration's own documentation
status: draft
priority: P2
complexity: M
area: [docs, web, py]
design:
created: 2026-08-30
github:
---
## Problem

`specs/guides/LANGUAGE-INTEGRATION_GUIDE.md` prescribes what an *integration* must implement, how to
select features, how to name and dispose of conformance tests, and how to review the design. It says
nothing about the documentation the integration owes its **own users** — the people who will write
commands in that language.

Every feature section names objects to map and tests to pass. None names a document to write. So a
completed integration can satisfy the whole guide and still ship with nothing a Python or JavaScript
author can read.

## Why it matters

The audiences are different and the guide only serves one of them. `specs/` is internal — it
addresses contributors, designers and coding agents (`DOCS_STRUCTURE_GUIDE.md` §2). An integration's
users are not contributors: they want to know how to declare a command in *their* language, what the
defaults will be, and what the error they just saw means.

Without a prescribed section, each integration invents its own answer, and the same drift the shared
declaration format exists to prevent reappears one level up — in the documentation. Two guides
describing the same format in incompatible terms is the documentation equivalent of two hand-written
parsers.

The concrete trigger: `design/command-declaration/` produces a shared reference
(`COMMAND_DECLARATION.md`, promoted to `reference/` at Phase 5) that is explicitly designed as the
basis for the language-specific guides. There is currently nowhere in the integration guide that
tells an author of such a guide to build on it.

## Expected behaviour

A new section of `LANGUAGE-INTEGRATION_GUIDE.md` — for example `§DOCS — Documentation and user
guide` — prescribing, in the same style as the feature sections, what an *integration* must produce
for its users, and requiring it to be built on the shared references rather than restating them.

It should cover at least:

1. **What documents an integration owes its users**, and where they live. `docs/` is reserved for
   user-facing documentation as a future sibling of `specs/` (`DOCS_STRUCTURE_GUIDE.md` §2, §9.6);
   this section should settle whether an integration's user guide belongs there, beside the
   integration's source, or in its package's own documentation.
2. **Command declaration is documented by reference, not by restatement.** The mapping, the
   composition model and the defaulting rules live in `COMMAND_DECLARATION.md`. A language guide
   shows the language's *syntax* for a declaration and links for the semantics. A guide that
   restates the defaulting rules will drift from them.
3. **What is genuinely language-specific and must be written per language:** the declaration syntax
   (decorator, object literal, dict), what the integration's introspection discovers and therefore
   what an author never needs to write, the `hints` keys the integration reads, how values and
   errors cross the boundary, and installation and setup.
4. **Worked examples that demonstrate composition** — an author writing only the difference — since
   an example restating every argument misrepresents the format.
5. **Keeping documentation examples true**, by the same principle as the conformance tests: an
   example that no test executes will rot.

## Related

- `design/command-declaration/` — produces the reference this section must point at; filed from its
  documentation work.
- `DOCS_STRUCTURE_GUIDE.md` §9.6 — the `specs/` versus `docs/` split this section has to respect.
- `specs/design/liquers-web/` — the worked integration design, and the most likely first consumer.

## Verification

The section exists, names the documents an integration must produce, and is applied to at least one
integration — most naturally `liquers-web`, whose user-facing documentation would then be the
section's worked example.
