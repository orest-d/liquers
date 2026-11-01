---
id: MARKDOWN-CODE-BLOCKS-FRONT-MATTER-AND-BACKLINKS-ARE-NOT-COMMANDS
kind: feature
title: Fenced blocks, front matter, folders and incoming links cannot be read with ns-md
status: draft
priority: P3
complexity: M
area: [lib/commands]
design:
created: 2026-11-01
github:
---
# Fenced blocks, front matter, folders and incoming links cannot be read with ns-md

## Problem

`design/markdown-navigation/` delivered the first slice of `NO-MARKDOWN-NAVIGATION-COMMANDS`:
`ns-md/outline`, `section`, `sections`, `links` and `table` over one document. The rest of that
issue's command set is missing:

- **fenced blocks by language** (`md/blocks`: `lang`, `info`, `text`, `section`, `line`, a `lang=`
  filter); the parser already has `BlockKind::FencedCode { info }`;
- **front matter as one record** (`md/front_matter`), absorbing the Markdown branch of
  `ns-search/field_extract`; the parser already has `Document::front_matter`;
- **the commands over a folder** (the maintainer allowed them beside the one-document form,
  2026-10-31), as `ns-search/md_split` reads a folder today;
- **incoming links**: the documents of a folder that link to a key, from the `links` records of
  each document.

## Impact

An agent still reads a whole document to find its `yaml` examples or its front matter, and cannot
ask which documents link to the one it is changing (the check before a rename or a supersession).

## Expected behaviour

The four commands above, in `ns-md`, returning records that compose with `ns-search` and `ns-rec`,
and named in `specs/reference/MARKDOWN.md` §5.

## Discovery

The remainder of `NO-MARKDOWN-NAVIGATION-COMMANDS` after `design/markdown-navigation/` (2026-11-01),
filed so the issue can close with the first slice.
