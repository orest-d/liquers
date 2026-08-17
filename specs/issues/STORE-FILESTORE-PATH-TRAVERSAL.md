---
id: STORE-FILESTORE-PATH-TRAVERSAL
kind: issue
title: A key containing `..` escapes the file store root
status: closed
priority: P0
complexity: M
area: [core/store, store/backends, axum]
design: store-key-guard
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

## Resolution

Closed 2026-08-17 by `specs/design/store-key-guard/`. A store now requires an absolute key:
`Key::is_relative` / `as_absolute` / `try_into_absolute` express the rule, `ErrorType::KeyNotAbsolute`
names the violation, and every store checks before using a key. The file and OpenDAL stores get it
structurally — their path builders are fallible — so the backend cannot be reached without passing.

Option 1 was taken, as the issue proposed, with one correction: `is_supported` is not sufficient
even as a shared helper, because only the store routers consult it, so a directly held store would
skip it. `to_absolute` is untouched.

Two things the issue did not have right:

- **The escape was demonstrated, as the Discovery section asked.** `../SECRET.txt` both reads and
  writes outside the root. But a *deep* traversal escapes only when the intermediate directory
  exists — the kernel resolves `..` by walking real directories — which is why the obvious test
  passes against unfixed code.
- **The query path was narrower than described.** Under `evaluate`, a *leading* `..` never reached
  the store: with no CWD in scope the cursor defaults to the logical root and resolves it there.
  Only an interior `..` (`data/../../etc`) survived to the store. The HTTP store API resolves
  nothing, so both forms reached the store there.

Evidence: `keyabs01`–`keyabs16`, in `liquers-core/src/query.rs`, `liquers-core/src/store.rs`,
`liquers-core/tests/store_key_absolute.rs`, `liquers-store/src/opendal_store.rs`,
`liquers-axum/src/api_core/error.rs`, and the three `liquers-web` `STORE05` suites.

Follow-up: `STORE-ABSOLUTE-KEY-NOT-TYPE-ENFORCED` (enforcement is per-method convention, not a
signature).
