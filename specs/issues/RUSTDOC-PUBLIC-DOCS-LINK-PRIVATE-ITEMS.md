---
id: RUSTDOC-PUBLIC-DOCS-LINK-PRIVATE-ITEMS
kind: issue
title: Public rustdoc in three modules links to private or absent items, so the links render as plain text
status: closed
priority: P3
complexity: S
area: [core/value, core/plan, core/store]
design: documentation-currentness-small-fixes
created: 2026-09-04
github:
---

## Problem

`cargo doc -p liquers-core --no-deps` emits three link warnings. Each one is a link a reader of the
published documentation cannot follow — rustdoc drops it to plain text rather than failing:

| Location | Link | Why it fails |
|---|---|---|
| `liquers-core/src/escape.rs:7` | `match_entity` | private item |
| `liquers-core/src/plan.rs:1999` | `CwdCursor::resolve_key` | private item |
| `liquers-core/src/store.rs:916` | `AsyncOpenDALStore` | no such item in scope — the type lives in `liquers-store`, which `liquers-core` does not depend on |

The `store.rs` one is the odd case: it is not a visibility problem but a dependency-direction one.
`liquers-core` sits below `liquers-store`, so it cannot link *up*; the reference has to be prose.

## Impact

Cosmetic, and small — the sentences still read correctly, since rustdoc renders an unresolved link
as code text. The cost is that the warnings are permanent noise in `cargo doc` output, which is
what makes a *new* broken link easy to miss. Nothing enforces the count today.

## Expected behaviour

Either link to something public (`Status::read_exposure` was replaced by a link to a module
section, for one such case) or drop the brackets and describe the item in prose. Once the count is
zero, `RUSTDOCFLAGS="-D rustdoc::broken_intra_doc_links -D rustdoc::private_intra_doc_links"` in a
CI doc job would keep it there.

## Discovery

Found on 2026-09-04 while reviewing the asset API documentation. The two warnings in
`assets.rs` (`poll_binary` → `ReadExposure::Value`, `binary_unchecked`) and the one in
`context.rs` (`get_dependency_state` → `schedule_dependency_asset`) were fixed there, since those
are the docs that review covered; these three are in modules it did not touch and are recorded
rather than swept in.

## Resolution

Closed on 2026-09-04. The three inaccessible targets are now code prose rather than intra-doc
links. `cargo doc -p liquers-core --no-deps` passed with both
`rustdoc::broken_intra_doc_links` and `rustdoc::private_intra_doc_links` denied.
