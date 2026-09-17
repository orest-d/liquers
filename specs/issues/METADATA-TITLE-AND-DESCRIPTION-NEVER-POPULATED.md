---
id: METADATA-TITLE-AND-DESCRIPTION-NEVER-POPULATED
kind: issue
title: Nothing populates an asset's title or description
status: rejected
priority: P2
complexity: M
area: [core/value, core/store]
design: 
created: 2026-09-16
github:
---
## Rejection

**The premise is false.** Filed 2026-09-16 and rejected the same day: two mechanisms populate
these fields, and the one this issue examined — `AsyncStore::default_metadata` — is not supposed
to be one of them.

**Recipes are the main mechanism, and they work.** `Recipe` carries `title` and `description`
(`recipes.rs:67`, `:73`) precisely because, as its module docs say, they are "human facing data
... which would be difficult in a compact query string". When an asset resolves its own recipe it
makes them authoritative: `assets.rs:2708-2709` calls
`.with_title(recipe.title.clone()).with_description(recipe.description.clone())`, and
`assets.rs:7731` asserts the result. `Recipe::to_asset_info` (`recipes.rs:375-376`) does the same
for a key that has a recipe but has never been evaluated, which is the branch
`AssetManager::get_asset_info` reaches through the recipe provider.

**Supplying filled metadata is the second mechanism.** A client may `POST` a `DataEntry` carrying a
populated `MetadataRecord` to the store API's entry endpoint, and a programmatic caller may hand
one to `AsyncStore::set`.

So `default_metadata` returning empty strings is right, not broken. A bare file that nobody has
described has no title, and inventing one — from the filename, say — would make "untitled"
indistinguishable from "titled after its file". The observation that started this issue, that a
listing over a plain file store shows empty titles, is a statement about a corpus with no recipes
and no supplied metadata, not about a defect.

What survives is much narrower and is filed as `CONTEXT-CANNOT-SET-TITLE-OR-DESCRIPTION`: a
command producing a value cannot set these fields on the asset it is producing, although it can
set the filename.

This record exists so the wrong version is not refiled. Before refiling anything in this area,
check whether a recipe or a supplied `MetadataRecord` is the answer.

## Original problem, as filed

`MetadataRecord` and `AssetInfo` both carry `title` and `description`, and `AssetInfo` is what
`listdir_asset_info` returns for every child of a directory. `AsyncStore::default_metadata` sets
the key, the updated timestamp and `is_dir` and returns; the file store's override does the same;
`MetadataRecord::new` leaves both fields empty. The conclusion drawn — that *nothing* fills them
in — did not survive review, because the filer looked at the store path and not at the recipe path.

## Discovery

Analysis for `AGENT-MEMORY-SERVICE`, 2026-09-16. Rejected the same day on maintainer review.
