---
id: STORE-HANDLER-FIELD-ACCESS-HARDCODED
kind: issue
title: Store handlers read and write metadata fields through a hard-coded match of six names
status: draft
priority: P3
complexity: M
area: [core/store, core/value]
design: liquers-mustache
created: 2026-10-17
github:
---

## Problem

The `store_config` handlers reach `MetadataRecord` fields through `text_field` and
`set_text_field` (`liquers-core/src/store/handlers.rs`). These are a `match` over six names
(`title`, `description`, `media_type`, `data_format`, `type_identifier`, `unicode_icon`), guarded by
`check_text_field` against `TEXT_FIELDS`. Any other field, a nested field, or a non-text field
cannot be named in a spec. Every new field needs three edits in step.

## Impact

Store configuration cannot set or require fields beyond the six, for example a front-matter field
or `tags`. It is a parallel, narrower version of the field access that templates need.
Workaround: add the field to the three functions by hand.

## Expected behaviour

The handlers read through the `DataNode` trait proposed in `design/liquers-mustache/`
(`data-node-interface.md`), with `MetadataRecord` implementing it, and write through its
`DataNodeMut` extension. The field vocabulary then comes from one place, the record's implementation.
The handlers stay synchronous: `CompiledFn` is sync by design, which is one of the reasons that note
recommends a dual sync and async interface.

## Discovery

Raised by the maintainer on 2026-10-17, reviewing `liquers-mustache` Phase 1: the template data
interface belongs in core and should replace the extraction of fields from metadata in the store
pipeline. Confirmed by reading `handlers.rs` at `04dd70e`.
