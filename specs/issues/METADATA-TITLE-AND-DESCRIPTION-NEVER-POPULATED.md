---
id: METADATA-TITLE-AND-DESCRIPTION-NEVER-POPULATED
kind: issue
title: Nothing populates an asset's title or description
status: draft
priority: P2
complexity: M
area: [core/value, core/store]
design: 
created: 2026-09-16
github:
---
## Problem

`MetadataRecord` and `AssetInfo` both carry `title` and `description`, and `AssetInfo` is what
`listdir_asset_info` returns for every child of a directory. They are the fields that let a client
decide whether an entry is worth fetching without fetching it.

Nothing fills them in. `AsyncStore::default_metadata` (`liquers-core/src/store.rs`) sets the key,
the updated timestamp and `is_dir`, and returns; the file store's override does the same. Neither
touches `title` or `description`, and `MetadataRecord::new` leaves both as the empty string. The
setters `with_title` and `with_description` exist (`metadata.rs:1175`, `:1180`) and are called by
value-producing paths, so a computed asset can carry a title — but a document that was simply put
into a store has none, and there is no derivation, no fallback to the filename, and no way to ask
for one.

So for any store-backed corpus, `listdir_asset_info` returns a list of entries whose descriptive
fields are all `""`.

## Impact

A directory listing is the cheapest thing a client can ask for and the natural place to describe
what is in a directory. Today it describes nothing, so every client that wants to know what an
entry *is* must fetch the entry — which is the read amplification the fields exist to avoid, and
it is worse over a network.

The fields being present but empty is the part that makes this a defect rather than a missing
feature: a consumer cannot distinguish "this asset has no title" from "nothing ever sets titles",
so it cannot even fall back sensibly.

Found while designing `AGENT-MEMORY-SERVICE`, which reads `title` as its L0 summary tier and
`description` as L1. The tiers are structurally already there; they are empty.

## Expected behaviour

Some path fills these in, and the contract says which. Candidates, in increasing ambition:

1. **Fall back to the filename** for `title` when nothing better is known. Cheap, always correct
   enough to beat `""`, and it makes a listing readable immediately.
2. **Preserve what a writer supplies.** A caller that sets `title` in the `MetadataRecord` handed
   to `set` should get it back from `get_metadata` and from a listing. Worth confirming per store
   — `finalize_metadata` runs on the write path and its interaction with these fields is
   unspecified.
3. **Derive from content**, for formats that carry a title — Markdown front-matter or a leading
   `#` heading being the obvious case. This probably does not belong in `liquers-core`; a command
   or a recipe that writes metadata back is the more Liquers-shaped answer, and is what
   `AGENT-MEMORY-SERVICE` would use.

(1) and (2) are the contract question and belong here. (3) is a consumer's job, but it only works
if (2) is guaranteed.

`STORE_SEMANTICS.md` should state what a store promises about these fields either way.

## Discovery

Analysis for `AGENT-MEMORY-SERVICE`, 2026-09-16. Verified at HEAD by reading `default_metadata` in
`liquers-core/src/store.rs` and both its overrides, and `MetadataRecord::new`.
