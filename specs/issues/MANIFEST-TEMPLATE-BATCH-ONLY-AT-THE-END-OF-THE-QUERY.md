---
id: MANIFEST-TEMPLATE-BATCH-ONLY-AT-THE-END-OF-THE-QUERY
kind: feature
title: A records manifest template can put the batch offset and size only at the end of its query
status: draft
priority: P2
complexity: M
area: [records, core/plan]
design:
created: 2026-11-02
github:
---

## Problem

A records manifest's generated tail is a text template (`liquers-records/src/manifest.rs`,
`ChunkTemplate::query_at`). Chunk *i* is the template query with `-<offset>-<batch_size>` appended
to its **last action**, plus the key filename when keyed (`specs/reference/RECORD_STREAMS.md`
§Manifest format):

```yaml
template:
  query: "-R/db/orders/-/ns-mydb/page"   # chunk i: …/page-<first_offset + step*i>-<batch_size>
```

The batch can therefore only feed the last action, as its two trailing positional parameters. The
template cannot:

- pass the offset to an action in the middle, then transform the page further (`page-…/filter-…/select-…`);
- pass it as a named argument, or to a command whose offset is not its last two parameters;
- pass it into a link (`~X~-R/db/orders/-/page-<offset>~E`) or into the resource key;
- use the chunk index itself, or a derived value (the batch number as a date, a file name).

Explicit `chunks` are `recipes.yaml` recipes, so they gain recipe pipelines when
`design/composite-command-definition/` lands (AC-15), but the template does not.

## Impact

Paged sources whose paging parameter is not the last argument need a wrapper command written in
Rust only to reorder arguments. A query that shapes each page after fetching it cannot be a
template at all. Workaround: list the chunks explicitly, which does not scale and loses the
open-ended walk.

## Expected behaviour

A template can be a pipeline (`template: {pipeline: [...], first_offset, step, batch_size}`) whose
placeholders `offset`, `batch_size` and `index` are bound per chunk with `Pipeline::bind`
(composite-command-definition AC-38), so the batch can reach any argument of any step, a link or a
nested pipeline. The text `query` form keeps its current meaning.

Questions for its design:

- **Chunk identity.** An unkeyed chunk is identified by its query (`ChunkId::Query`). A bound
  pipeline that is not representable as a query has no such identity. Options: require a
  representable pipeline for unkeyed chunks; add `ChunkId::Pipeline` with a canonical serialized
  form; or allow pipelines only in keyed manifests.
- **Which placeholders.** `offset`, `batch_size` and `index` at least; whether a manifest can
  declare more (typed, with defaults) as `ArgumentInfo`s, as a pipeline command does.
- **Validation.** Load-time checks that every placeholder is one the manifest binds, and that the
  template plans with sample values, in `liquers-validate` (`RECORDS-RECIPE-VALIDATION-CLI`).

Designed and implemented right after `composite-command-definition`, as the maintainer asked on
2026-11-02.

## Discovery

The maintainer's review of `composite-command-definition` on 2026-11-02 ("the records manifest
should also support pipelines … where the batch number is changing inside of the pipeline").
`specs/index.csv` had no issue for it: the manifest issues on file are about chunk errors, folder
listing, providers and tests.
