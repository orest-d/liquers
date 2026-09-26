---
id: MANIFEST-PROVIDER-FOLDER-LISTING-NEVER-REFRESHES
kind: issue
title: ManifestRecipeProvider's per-folder manifest listing cache never refreshes
status: draft
priority: P2
complexity: S
area: [records]
design: record-streams
created: 2026-09-26
github:
---
## Problem

`ManifestRecipeProvider` (`liquers-records/src/provider.rs`, Phase 4 Step 4.3) caches, per folder,
the list of `*.manifest.yaml` filenames it found there (`ManifestRecipeProvider::manifest_names`),
so an explicit-chunk lookup does not call `AsyncStore::listdir` on every request —
`phase4-implementation.md`'s Step 4.3 "Lookup cost" rules ask for exactly this.

The cache has no invalidation path. Once a folder's listing is read, it is kept for the life of the
provider; `AsyncStore` has no directory-level version or change notification the way a single key's
metadata does (`Metadata::version`, which the provider's *other* cache — parsed manifests — does
check on every lookup, per that same Step's design). So:

- adding a new `<prefix>.manifest.yaml` file to a folder after the provider first listed it is
  never noticed — a lookup for one of its chunks answers `None` (or, worse, a fast-path guess that
  happens to also match an *existing* manifest's naming pattern) forever, for that provider
  instance;
- removing a manifest file is symmetric: its filename stays in the cached listing, and lookups keep
  trying to read a key the store no longer has (`get_manifest` handles this gracefully — `Ok(None)`
  on `KeyNotFound` — but pays a wasted store call every time, and the removed manifest's name is
  never dropped from the list).

This is a deliberate, documented tradeoff in the code (`ManifestRecipeProvider`'s own doc comment),
not an oversight discovered after the fact — but CLAUDE.md's filing rule applies to a known,
undischarged limitation as much as to a bug, and this one has no test and no follow-up recorded
anywhere else.

## Impact

A long-lived server process (the `liquers-axum` case this design exists for) that adds a new
manifest-backed data feed to an existing folder without restarting will not serve it: `contains`,
`recipe_opt` and `assets_with_recipes` for that folder all answer as if the new manifest does not
exist, with no error surfaced anywhere. The workaround is restarting the process (a fresh
`ManifestRecipeProvider`), which is disruptive for a live server and easy to forget.

P2 rather than P3: the failure is silent — a key that should resolve answers "no recipe" with no
error anywhere — and adding a manifest next to others on a running server is an ordinary workflow,
not an edge case. Template chunks are unaffected: they are found through the manifest's own key.

## Expected behaviour

One of:

- give `AsyncStore` a cheap, optional directory-level version/signature (mirroring
  `Metadata::version` for a single key) that `manifest_names` can check the way `get_manifest`
  checks a single manifest's version — the general fix, but a `core/store` change, not a
  `liquers-records` one;
- bound the cache's lifetime instead (a TTL, or an explicit `invalidate_folder`/`invalidate_all`
  method the host calls on a config-reload signal) — cheaper, but pushes the "when" onto the host;
- or, at minimum, do not cache silently: document the tradeoff where a host configuring the
  provider chain will see it (`specs/guides/COMMAND_REGISTRATION_GUIDE.md` or wherever
  `ManifestRecipeProvider` construction is documented once `liquers-lib` wires it in at Step 5.x),
  not only in the struct's own doc comment.

Whichever route is chosen should also decide what happens to `manifests` (the per-manifest parse
cache) for a manifest whose folder listing is stale — right now the two caches are independent, and
a `folders` refresh alone would not evict a now-removed manifest's stale `manifests` entry either.

## Discovery

Noted while implementing `ManifestRecipeProvider` (Phase 4 Step 4.3): the per-manifest cache
(`manifests`) has a clear, tested invalidation story via `Metadata::version`
(`a_manifest_edit_with_a_new_stored_version_is_picked_up` in `provider.rs`), but the per-folder
listing cache (`folders`) that Step 4.3 also asks for has no equivalent, because
`AsyncStore` exposes no directory-level version to check it against. Not covered by any existing
test; filed rather than fixed, per `CLAUDE.md`'s "file it before you finish the task."
