---
id: KEY-NAMES-CANNOT-HOLD-COMMON-WEB-FILENAMES
kind: issue
title: Key names cannot hold filenames common in web applications, so such files cannot be stored or served
status: draft
priority: P3
complexity: M
area: [core/query, axum]
design:
created: 2026-11-06
github:
---

## Problem

A key name accepts letters, digits, `_` and `.`, and `-` after the first character
(`liquers-core/src/escape.rs`, `liquers-core/src/parse.rs`). Files that web bundlers and frameworks
produce routinely use other characters: `chunk~vendors.js`, `+page.svelte`, `[slug]/index.html`,
`@scope/` folders, spaces and non-ASCII names. Such a file in a filesystem mount has no key, so it
cannot be listed, read or served.

## Impact

A web application copied into the store's `web` folder (`axum-server-assembly`) breaks when one of
its files has such a name. Workaround: configure the bundler to emit plain names.

## Expected behaviour

Either an escaping rule that maps any filename to a key and back (and the web route decodes
percent-encoded paths through it), or a documented, enforced limit with a clear error listing the
offending file.

## Discovery

`axum-server-assembly` Phase 1 review, 2026-11-06.
