---
id: MARKDOWN-TABLE-CANNOT-DISTINGUISH-NULL-FROM-EMPTY-TEXT
kind: issue
title: A Markdown table reads an empty Text cell back as null, and reads only the first table
status: draft
priority: P3
complexity: S
area: [records]
design: markdown-empty-text-and-tables
created: 2026-09-27
github:
---
# A Markdown table reads an empty Text cell back as null, and reads only the first table

## Problem

`liquers-records/src/formats/markdown.rs` writes both a null and an empty `Text` cell as an empty
cell, so `read_markdown` cannot tell them apart and reads both back as null. Every other text
format distinguishes them: CSV uses an empty unquoted field for null and `""` for the empty string.

The reader also stops after the first table in the document. Any later table, or text between
tables, is ignored without notice.

## Expected behaviour

Give the empty string a distinct written form (for example the `&#32;`-style entity the writer now
uses for significant whitespace), so that `read(write(x)) == x` holds for empty Text too. The first
table is the documented unit of reading, so either say so in the reference, or refuse a document
that holds more than one table.

## Discovery

Found 2026-09-27 while fixing the implementation review's Markdown findings
(`phase5-evidence.md`, review row).
