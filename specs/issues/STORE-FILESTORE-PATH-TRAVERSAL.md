---
id: STORE-FILESTORE-PATH-TRAVERSAL
kind: issue
title: A key containing `..` escapes the file store root
status: accepted
priority: P0
complexity: M
area: [core/store, store/backends, axum]
design:
created: 2026-08-09
github:
---

## Problem

`..` is a valid `ResourceName`. The parser's `resource_name` accepts `.` as a name character
(`liquers-core/src/parse.rs:331`), so `parse_key("../../etc/passwd")` succeeds and yields
`Key(["..", "..", "etc", "passwd"])`. Nothing rejects it downstream:

- `AsyncFileStore::key_to_path` is `path.push(key.to_string())`
  (`liquers-core/src/store.rs:835-839`), and `Key`'s `Display` joins segments with `/`
  (`liquers-core/src/query.rs:1663`). `PathBuf::push("../../etc/passwd")` resolves relative to the
  store root, so the resulting path is *outside* it. `key_to_path_metadata` and `key_to_lock_path`
  have the same shape.
- `Key::to_absolute` (`liquers-core/src/query.rs:1528`) does resolve `..`, but it is only applied
  where relative resolution happens. A key that arrives at the store already absolute is never
  normalized, so the guard does not sit on this path.
- `AsyncStore::is_supported` is the designated place to refuse a key, and `AsyncFileStore` returns
  `true` unconditionally (`liquers-core/src/store.rs:809`).

The same key shape is a namespace escape rather than a filesystem escape in other backends; see
`STORE-OPENDAL-SLASH-HANDLING` for the adjacent OpenDAL key-shape problem, which is a different
cause.

## Impact

Reads and writes outside the configured store root. This is reachable from a query
(`-R/../../etc/passwd`), and `liquers-axum` serves queries over HTTP, so a deployment that exposes
the query API to untrusted callers exposes the filesystem around the store root with the server
process's privileges. Writes are affected as well as reads: `set` builds its path the same way.

No workaround exists at the store layer. A deployment can only mitigate it upstream, by rejecting
keys containing `..` before the query reaches Liquers — which requires knowing to do so.

Marked P1 rather than P0 because exploitation requires an exposed query endpoint reachable by an
untrusted caller, which is a deployment posture rather than the default.

## Expected behaviour

A key containing a `..` segment is refused with `Error::key_not_supported`, not resolved and not
silently normalized. Rejection rather than normalization, because a key is an address and not a
path: `a/../b` and `b` are different addresses, and quietly treating them as one would make two
distinct assets alias.

Where the check belongs is the open part:

1. A shared helper in `liquers_core::store` that every backend calls from `is_supported`, which
   also fixes the backends not yet written. Empty segments and `.` deserve the same treatment.
2. `AsyncFileStore::is_supported` alone, which fixes the exploitable case with the smallest change
   but leaves the next backend to rediscover it.
3. Refusal at parse time, which is the widest net but changes the meaning of an existing accepted
   input and would break `to_absolute`'s legitimate use of `..` in relative resolution.

Option 1 looks right, with `to_absolute` left alone since it consumes `..` before a store sees it.

## Discovery

Found on 2026-08-09 while specifying the key-rejection rule (`STORE05`) for
`specs/design/liquers-web-store/`: the browser stores must refuse `..` so that a key cannot escape
a URL prefix or a storage namespace, and checking how the existing stores do it showed that they
do not. Not verified by running an exploit — read from the code — so the `PathBuf::push` behaviour
is worth confirming with a test before fixing.
