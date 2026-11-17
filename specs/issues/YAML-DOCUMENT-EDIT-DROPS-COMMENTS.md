---
id: YAML-DOCUMENT-EDIT-DROPS-COMMENTS
kind: feature
title: Editing a YAML document through an accessor drops its comments and formatting
status: draft
priority: P3
complexity: M
area: [core/value]
design:
created: 2026-11-16
github:
---
## Problem

The document steps of `specs/design/settable-value-accessor/` (`RecipeStep`, `FieldStep`, `ItemStep`
in Phase 2) edit a YAML document by parsing it into a `serde_yaml::Value` tree, changing one part,
and serializing the tree again. `serde_yaml` (0.9, the only YAML library in the workspace) keeps
every key and the order of mapping keys, but it has no representation for comments, blank lines,
quoting style, flow versus block style or anchors. Setting `n` to `20` in one recipe of a commented
`recipes.yaml` therefore rewrites the whole file without its comments.

Orest accepted this for v1 at the Phase 2 gate (2026-11-16). It narrows Phase 1 AC-2 ("its other
content is unchanged") to content, not presentation.

## Impact

People who maintain `recipes.yaml`, or any YAML configuration, by hand lose their comments the
first time an editor saves through an accessor. The data is intact. The workaround is to keep
hand-maintained commentary in `title` / `description` fields, or to edit such files only as text.

## Expected behaviour

A set through a document step changes only the bytes of the edited part. Comments, blank lines and
the styling of untouched parts survive. Possible routes:
- a comment-preserving YAML editor crate, behind a feature;
- a span-based patch: re-parse with positions, then replace only the edited scalar or subtree's
  text, falling back to a full rewrite for structural edits.

The same question applies to JSON (whitespace and key order) and, later, to template text
(template-link projections).

## Discovery

Raised in Phase 2 review of `settable-value-accessor` (2026-11-15); accepted for v1 by Orest.
