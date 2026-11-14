---
id: TEMPLATE-ERROR-AT-QUERY-LEVEL-LOSES-THE-TEMPLATE-POSITION
kind: issue
title: A template error raised through a query reports the position in the query, not in the template
status: draft
priority: P3
complexity: M
area: [lib/commands]
design:
created: 2026-11-10
github:
---

## Problem

Rendering a template directly (`liquers_template::render`, `liquers.expand` in JavaScript) reports
a failing tag with its line and column in the template. Rendering the same template through a
query (`-R/templates/x.md/-/ns-template/expand`, or a template command) fails with an error whose
`position` is that of the `expand` action in the query; the tag's position survives only inside
the message text.

## Impact

A caller that shows errors by position, an editor or a server client, points at the query instead
of at the line of the template that is wrong. Workaround: `ns-template/validate`, which returns
the positions as data.

## Expected behaviour

The error of a query-level rendering carries the template's key and the position in the template
as fields, beside the query position.

## Discovery

Writing section 10.4 of the Rust user guide (2026-11-10): the test's assertion on the template
line had to be changed, and the text does not promise a template position.
