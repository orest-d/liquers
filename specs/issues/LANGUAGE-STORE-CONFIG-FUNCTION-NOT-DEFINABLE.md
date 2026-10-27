---
id: LANGUAGE-STORE-CONFIG-FUNCTION-NOT-DEFINABLE
kind: feature
title: A store_config handler cannot be defined in the integrated language
status: draft
priority: P3
complexity: L
area: [web, py, core/store, store/config]
design: language-store-config-function
created: 2026-10-15
github:
---
# A store_config handler cannot be defined in the integrated language

## Problem

`design/store-adapters/` makes metadata handlers, storage models and write strategies actions of the
realm `store_config`. Each one is a `CommandMetadata` plus a builder, registered in Rust code. The
builder is a plain `fn` pointer (`StepBuilder`, `ComponentBuilder`), and it cannot capture a
JavaScript or Python callable. So a page or a Python module can configure the built-in handlers on
its stores, but it cannot add a handler of its own, such as a domain-specific field projection.

## Problem Example

```python
env = lq.Environment(config={"store": {"stores": [{"type": "memory", "prefix": "n",
    "metadata": {"on_read": "media_type-fill/page_tag"}}]}})

def page_tag(record):          # wanted: a store_config handler written in Python
    record.info("seen by page_tag")
    return record
```

Today nothing can register `page_tag`: `StoreConfigFunctions::register_step` takes a `StepBuilder`,
a plain `fn` pointer that cannot capture the Python function, and neither binding has an entry
point for it, so the section is refused because `page_tag` is not a registered handler. Expected:
reading `n/a.txt` runs `page_tag` in the `on_read` chain, and a raising handler becomes a warning on
the record. The spec validates (`liquers-validate --command store_config//page_tag --
'-store_config/media_type-fill/page_tag'`). Design: `design/language-store-config-function/`.

## Impact

Small today: the built-in handlers run in Rust around any store, a language store included, so a
language store loses nothing it had before. The gap shows when an application's metadata rule is
naturally written in its own language. That is the same asymmetry `LANGUAGE-STORE-TYPE-NOT-DEFINABLE`
records for store types: commands can be defined in the language; store extensions cannot.

## Expected behaviour

A language registers a handler with a declaration in the Command Declaration Format, in realm
`store_config`, plus a synchronous callable from record (and optional bytes) to record. The design
has to settle what `design/store-adapters/phase2-architecture.md` §Language Integration lists:

- a boxed builder in `StoreConfigFunctions`' table, beside the `fn` pointers;
- synchronous only, since a chain cannot await;
- registration before the store is built (compare `POST-INIT-COMMAND-REGISTRATION`);
- the per-call cost of crossing the bridge on every read in a listing (the GIL in Python);
- exceptions through the ERROR bridge, under the stage rules (a warning on read, abort on validate).

Exposing a compiled chain to the language (`compileStoreConfig`) is a related, smaller question; no
use for it is known.

## Re-checked at HEAD (2026-10-20): persists; re-sized `M` → `L`

`StepBuilder` and `ComponentBuilder` are still plain `fn` pointers, and `StoreConfigFunctions`
holds them in two `HashMap`s (`liquers-core/src/store/realm.rs`). Re-sized to `L`: the fix changes
a public `liquers-core` type (a boxed, capturing builder beside or instead of the `fn` pointer) and
then lands a binding in each integration, so it crosses crates (§4.5). `liquers-web` records
`STORE18` as `NA` pending this issue (`liquers-web/README.md`); `liquers-py` has no `STORE` feature
yet, so its half waits for that.

## Discovery

Raised by the maintainer at the Phase 2 review of `design/store-adapters/`: can a store implemented
in another language use the configuration and its new methods, execute the compiled closures, and
define its own `store_config` command? The first two are answered by that design; the last is not
built there.
