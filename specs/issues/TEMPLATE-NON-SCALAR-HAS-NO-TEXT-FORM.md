---
id: TEMPLATE-NON-SCALAR-HAS-NO-TEXT-FORM
kind: issue
title: A template cannot index into a Query or Key value, and inserts other non-scalars as JSON
status: draft
priority: P3
complexity: S
area: [lib/commands]
design: 
created: 2026-10-17
github:
---
## Problem

A Query or Key value is a text scalar to a template: `{{x}}` renders `a/b`, but `{{x[0]}}` is an
error. Any other non-scalar (list, map, record) is inserted as its JSON. `try_into_string` refuses
Query, Key, arrays and objects (`liquers-core/src/value.rs`).

## Impact

Convenience only. The workaround is a command that builds the wanted text.

## Expected behaviour

Kept as is on purpose (Orest, 2026-10-17): revisit once templates are in use, since a text form for
non-scalars could surprise. A proposal from that discussion: a `DataNode::text()` written when a
non-scalar is inserted; Query and Key as lists of path elements whose text is `encode()`;
`try_into_string` encoding Query and Key; and in a hygienic slot, a node with a text form inserted
as its text (a Query as a link), not as a parameter override.

## Discovery

design/liquers-mustache, PR #139 discussion.
