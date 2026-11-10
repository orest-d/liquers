---
id: SET-STATE-WITHOUT-DATA-FORMAT-IGNORES-THE-KEY-EXTENSION
kind: issue
title: set_state on a key with a known extension records application/octet-stream when the state names no format
status: draft
priority: P2
complexity: S
area: [core/assets]
design:
created: 2026-11-10
github:
---

## Problem

`AssetManager::set_state(&key, state)` with a state whose metadata has no `data_format` stores the
asset with the media type `application/octet-stream`, even when the key ends in `.md` or `.txt`.
A file written with `store.set` has no status at all. Commands that select by content, the
`ns-search` commands among them, require a stored-value status and a text media type, so documents
written either way are silently skipped.

`set_binary` is the other half: it refuses an empty type identifier, so the caller cannot leave the
type to the key's extension either.

## Impact

Storing a Markdown document from Rust so that search finds it takes a `MetadataRecord` with
`data_format = Some("md")` built by hand (`liquers-lib/tests/user_guide_GUIDE.rs`, `ch13_search`,
helper `document`). Without it search returns nothing and reports nothing.

## Expected behaviour

When the state names no data format, `set_state` derives it, and the media type, from the key's
extension, as a store read does. Or the search commands say which files they skipped and why.

## Discovery

Writing chapter 13 of the Rust user guide (2026-11-10): the first version of the example found no
documents.
