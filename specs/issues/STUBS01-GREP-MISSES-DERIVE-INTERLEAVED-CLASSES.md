---
id: STUBS01-GREP-MISSES-DERIVE-INTERLEAVED-CLASSES
kind: issue
title: check-stubs.sh's STUBS01 grep misses classes with an interleaved derive attribute
status: closed
priority: P3
complexity: S
area: [web]
design: stubs01-class-detection
created: 2026-09-27
github:
---
## Problem

`liquers-web/scripts/check-stubs.sh`'s STUBS01 section derives the list of `#[wasm_bindgen]`
classes it expects to find in the generated `liquers_web.d.ts` from the source itself, with this
pattern (`check-stubs.sh:63-64`):

```bash
expected_classes=$(grep -rhoP '#\[wasm_bindgen\(js_name\s*=\s*\K\w+(?=\)\s*\]\s*\npub struct)' \
    --include='*.rs' -z "$crate/src" 2>/dev/null | tr '\0' '\n' | sort -u)
```

The pattern requires `pub struct` on the line *immediately* following the `#[wasm_bindgen(js_name
= …)]` attribute. `liquers-web/src/objects.rs`'s `LiquersQuery` and `LiquersKey` both put
`#[derive(Clone)]` on the line in between:

```rust
#[wasm_bindgen(js_name = Query)]
#[derive(Clone)]
pub struct LiquersQuery {
```

so the regex silently fails to match either one, and `Query`/`Key` never enter
`expected_classes` — STUBS01 never checks that `export class Query`/`export class Key` exist in
the generated declarations. The script's own fallback list (used only when the grep returns
nothing) still names `Key` and `Query`, which is presumably what the author intended STUBS01 to
cover, but that fallback is dead code today: the grep already matches every *other* exported
class (`Asset`, `Environment`, `LiquersError`, `RecordBatch` as of
`specs/design/record-streams/`'s Step 7.1, `State`, `Store`, `Value`), so it returns a non-empty
list and the fallback branch is never taken.

## Impact

Low severity, no known live breakage: `Query`/`Key`'s declarations are exercised structurally
elsewhere in `tests/stubs/valid_usage.ts` (`liquers.Query.parse(...)`, `liquers.Key.parse(...)`),
so `tsc` would already fail STUBS02 if their declarations were missing or wrong — STUBS01 is a
second, more direct check that has been silently skipped for these two classes. A `.d.ts`
regression specific to `Query`/`Key` (e.g. a typo in `js_class` breaking codegen for just that
class, with no accompanying usage-line change) could pass `check-stubs.sh` today.

## Expected behaviour

STUBS01 should verify every exported class, including `Query` and `Key`. Either loosen the regex
to tolerate intervening attributes between `#[wasm_bindgen(js_name = …)]` and `pub struct` (e.g.
match across `#[…]` lines non-greedily, or grep for the attribute and the `pub struct`/`pub enum`
line separately and pair them by proximity), or drop the fragile single-regex approach in favor of
a small Python/awk pass that tracks "the most recent `js_name` seen" across lines. The comment
already concedes the regex "is brittle across grep builds" — worth revisiting to also fix this gap
in the same pass.

## Discovery

Noticed while validating `specs/design/record-streams/phase4-implementation.md` Step 7.1 (the
`RecordBatch` wasm handle): `check-stubs.sh`'s STUBS01 output listed `RecordBatch` among the
matched classes but never listed `Key`/`Query`, even though both are real exported classes with
their own usage lines in `valid_usage.ts`. Confirmed by hand that `objects.rs`'s `#[derive(Clone)]`
line is what breaks the grep's `\n`-adjacency requirement.

## Resolution (2026-10-06)

Fixed by `design/stubs01-class-detection/`. `liquers-web/scripts/check-stubs.sh` now derives the
expected classes with a POSIX `awk` pass instead of `grep -P -z`: it remembers the name from the
last `#[wasm_bindgen(js_name = X)]` line, keeps it across further attributes, doc comments and
blank lines, emits it at the next `pub struct` / `pub enum`, and clears it on any other line (so
`js_name` on methods and on the extern `RecordColumn` type is ignored). The dead fallback list is
gone; an empty detection is a STUBS01 failure.

Evidence: the awk pass alone over `liquers-web/src` yields Asset, Environment, Key, LiquersError,
Query, RecordBatch, State, Store, Value (the old grep missed Key and Query). See the design's
implementation note for the full `check-stubs.sh` run.
