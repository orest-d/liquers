---
id: CONTEXT-CANNOT-SET-TITLE-OR-DESCRIPTION
kind: issue
title: A command cannot set its asset's title or description through Context
status: draft
priority: P2
complexity: S
area: [core/context]
design: 
created: 2026-09-16
github:
---
## Problem

`Context` lets a running command write to the metadata of the asset it is producing — but only
some of it. `set_filename` (`liquers-core/src/context.rs:831`) reaches through to
`metadata.set_filename`; `set_expires`, `set_payload_required` and `set_error` do the same for
their fields, and the log and progress methods add entries. There is no `set_title` and no
`set_description`.

The fields are not inaccessible in principle: `MetadataRecord::with_title` and `with_description`
exist (`metadata.rs:1174`, `:1179`), and `assets.rs:2708` calls both when an asset adopts its
recipe's human-facing metadata. A command simply has no route to them.

## Impact

The two existing routes both decide the title *before* the value exists — a recipe declares it in
`recipes.yaml`, or a caller supplies a filled `MetadataRecord` on write. Neither can describe a
result in terms of what the computation actually found: "1 284 rows, 7 columns", "3 of 12 checks
failed", "empty after filtering". A command that knows the most about its own output is the one
component that cannot say so.

The asymmetry with `set_filename` is the sharpest form of it. A command may name the file it
produces but not say what it is, which is the wrong way round: the filename is usually derivable
from the query, and the description never is.

The workaround is to encode the summary in the value and let a client read the value — which
defeats the purpose of `AssetInfo` carrying a description at all, since the point of the field is
to be readable from a listing without fetching anything.

## Expected behaviour

`Context::set_title(&self, title: &str)` and `Context::set_description(&self, description: &str)`,
shaped like `set_filename` — async, writing through to the asset's `MetadataRecord`, returning
`Result<(), Error>`.

Questions for whoever picks this up:

- **Precedence against a recipe.** `assets.rs:2708` makes the recipe's title authoritative when
  the asset resolves its own recipe. Does a command's later call override it, or is a
  recipe-declared title final? Overriding seems right — it happens later and knows more — but it
  must be a decision, and the recipe's comment calls its metadata "authoritative".
- **Whether both fields want the same rule.** A recipe title naming the asset plus a command
  description reporting the outcome is a coherent split, and may be better than one rule for both.
- **Persistence.** Whether a title set this way survives into the stored metadata of a keyed
  asset, and whether it participates in the content-hash `version` (it should not — a retitled
  result is not a changed result).

## Discovery

Analysis for `AGENT-MEMORY-SERVICE`, 2026-09-16, on maintainer review of
`METADATA-TITLE-AND-DESCRIPTION-NEVER-POPULATED` — which claimed the fields were never populated
at all. They are, by recipes and by supplied metadata; this is the one route that is genuinely
missing. Verified at HEAD against the `Context` method list.
