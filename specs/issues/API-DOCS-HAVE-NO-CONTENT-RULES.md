---
id: API-DOCS-HAVE-NO-CONTENT-RULES
kind: feature
title: API documentation has no content rules, no place in the design process and no user-facing home
status: accepted
priority: P2
complexity: L
area: [docs]
design: api-documentation-rules
created: 2026-11-01
github: 232
---
## Problem

Nothing says what a module's API documentation must contain — in Rust (`//!`), Python (module and
class docstrings) or JavaScript (JSDoc carried into the generated `.d.ts`). Module docs are written
ad hoc: some are thorough (`liquers-core/src/store/pipeline.rs` after #231), many are absent. At
`15e06a4`, 85 of the 276 source files under `liquers-*/src/` have no `//!` doc at all, among them
`liquers-core/src/commands.rs`, `command_metadata.rs`, `error.rs`, `metadata.rs`, `state.rs` and
`value.rs`, and 46 of the 87 in `liquers-lib`. Where a doc exists it rarely says where the module
is used, which related concept it should not be confused with, or which example to read.

The design process (`liquers-project` skill, `DOCS_STRUCTURE_GUIDE.md` §9) maintains `reference/`
and `guides/` documents only; API docs are never planned, reviewed or recorded in a phase.

There is no user-facing documentation folder: `docs/` is only reserved (`DOCS_STRUCTURE_GUIDE.md`
§9.6), so generated API docs and user guides have nowhere to live.

## Impact

A reader landing on a module — human, IDE or coding agent — cannot tell what it is for, where its
entry point is, or how it differs from a neighbouring module (store versus asset manager). Coding
agents infer APIs from specs instead, which drift. Without a user-facing home, user documents stay
in the internal `specs/` tree.

## Expected behaviour

Requested by the owner on 2026-11-01 (GitHub #232):

1. Every module doc states its purpose and main components (entry points, not an exhaustive list);
   where it is used (typically 3, at most 5 linked places, plus a sentence on the rest, with
   diverse examples when use is widespread); related concepts and how they differ, with links; and
   a 3-10 line doctest where possible, plus up to 5 linked longer examples with a one-line
   description each and a note where more are.
2. The rules are an API documentation guide written language-agnostically, with Rust, Python and
   JavaScript sections.
3. The rules are part of the design process and the `liquers-project` skill.
4. A top-level user documentation folder with a homepage and a future-proof structure for all
   generated API docs and future user guides.

Bringing existing module docs up to the rules is `MODULE-DOCS-DO-NOT-FOLLOW-THE-API-DOC-RULES`.
Generating the site is `USER-DOCUMENTATION-CANNOT-BE-GENERATED`.

## Discovery

Requested by the owner (Orest) in the project thread on 2026-11-01, after the store_config handler
docs of #230 / PR #231.
