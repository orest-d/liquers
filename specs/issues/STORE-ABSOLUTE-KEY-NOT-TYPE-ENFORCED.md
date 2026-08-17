---
id: STORE-ABSOLUTE-KEY-NOT-TYPE-ENFORCED
kind: issue
title: Store key absoluteness is enforced by convention rather than by the type system
status: draft
priority: P3
complexity: L
area: [core/store, store/backends, core/query]
design:
created: 2026-08-17
github:
---
## Problem

A key handed to a store must be absolute — no segment may be `.` or `..` — because a store never
resolves relative keys; that happens at plan level. `specs/design/store-key-guard/` establishes the
rule, gives `Key` the `is_relative` / `as_absolute` / `try_into_absolute` methods that express it,
and refuses a violation with `ErrorType::KeyNotAbsolute`.

Enforcement, however, is a **convention**: every fallible key-taking method of every store has to
open with `key.as_absolute()?`, roughly sixty call sites across `Store` and `AsyncStore` and six
implementations. The signature does not say the key must be absolute, so nothing stops the next
method — or the next backend — from omitting the check, and a reviewer has to notice its absence
rather than the compiler.

Two mitigations exist and neither closes the gap:

- The file stores and the OpenDAL store make their path builders (`key_to_path`,
  `key_to_path_metadata`, `key_to_lock_path`) fallible, so the *dangerous* conversion cannot be
  reached without passing. That covers the backends where a relative key becomes a filesystem
  escape, not the general case.
- `STORE05` in `specs/guides/LANGUAGE-INTEGRATION_GUIDE.md` requires a conformance test per store,
  which catches an omission after it is written, in stores that run the suite.

`AsyncStore::is_supported` is not enforcement either: only `StoreRouter::find_store` and
`AsyncStoreRouter::find_store` consult it (`liquers-core/src/store.rs:1579`, `:1588`, `:1793`), so a
directly held store skips it.

## Impact

No user-visible defect today — the rule is implemented everywhere it needs to be, and this is
hardening rather than a bug. What it costs is future safety: a new backend, or a new method on an
existing one, can silently reintroduce `STORE-FILESTORE-PATH-TRAVERSAL` in its own store, and the
first evidence would be a security report rather than a build failure.

The exposure grows with the number of store backends. It is worth revisiting when a backend is added
(`WEB-NATIVE-IO-TIER2`'s IndexedDB store, any future S3 or database store) or when `openbin` is
implemented (`CORE-STORE-OPENBIN-MISSING`), since that is a new key-taking method on every store.

Workaround: the conformance test and review. Both work; neither is structural.

## Expected behaviour

Parse, don't validate: an `AbsoluteKey` newtype wrapping a `Key` whose only constructor performs the
check, with `Store` and `AsyncStore` taking `&AbsoluteKey` instead of `&Key`. Then a store method
that forgot the check does not compile, and the precondition is legible in the signature rather than
in prose.

Rejected during `store-key-guard` Phase 2 on cost, not on merit: it changes every method of both
traits, both routers, all six store implementations, and the `liquers-py` and `liquers-web` store
surfaces — which is a wide, cross-crate API break to carry inside a P0 security fix. Complexity `L`
reflects that breadth, and per `DOCS_STRUCTURE_GUIDE.md` §4.5 the work needs its own design folder.

Open questions for whoever picks this up:

- Where the conversion happens. Somewhere between the plan and the store, one boundary must do it;
  putting it too deep reintroduces the same "did you remember" problem one layer down.
- Whether `AbsoluteKey` should also be the type the routers and `AssetManager` carry, or whether it
  converts at the store boundary only.
- What it costs the language bindings, where the extra type has to be expressed or hidden.
- Whether `Deref<Target = Key>` keeps the ergonomics acceptable, and whether that weakens the
  guarantee by making the inner `Key` trivially reachable.

## Discovery

Recorded 2026-08-17 while designing `specs/design/store-key-guard/`, which fixes
`STORE-FILESTORE-PATH-TRAVERSAL`. The newtype was considered as the primary solution and rejected
for scope; filed so the alternative survives the design rather than living only in that folder's
rejected-alternatives table.

`store-key-guard` Phase 2 lists the evidence to gather during implementation that would justify
promoting this: whether `key.as_absolute()?` at the top of sixty method bodies reads as intended or
as noise, and whether the check is in fact forgotten anywhere.
