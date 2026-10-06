# Phase 1: High-Level Design

## Design Readiness

- **Readiness:** ready
- **Leading issue:** None
- **Explanation:** A check-script fix. The expected set is fully determined by the source
  (`#[wasm_bindgen(js_name = X)]` followed, possibly after other attributes, by `pub struct` or
  `pub enum`).
- **Open questions:** None

## Problem

`liquers-web/scripts/check-stubs.sh` STUBS01 derives expected classes with a `grep -P -z` regex
requiring `pub struct` on the line right after `#[wasm_bindgen(js_name = …)]`. `LiquersQuery` and
`LiquersKey` (`liquers-web/src/objects.rs`) have `#[derive(Clone)]` in between, so `Query` and
`Key` are never checked. The fallback list (used only when the grep finds nothing) is dead and
also lacks `RecordBatch`.

## Expected behaviour and acceptance

1. STUBS01's expected set includes `Query`, `Key` and every other `js_name` class, independent of
   intervening attributes, doc comments or blank lines.
2. The detection is a small `awk` pass (no `grep -P -z`, which the script notes is brittle). Under
   `-z`, the fallback list is removed. An empty detection becomes a hard failure ("STUBS01 found
   no classes — detection is broken").
3. Running `check-stubs.sh` on HEAD's generated declarations passes and lists `Key` and `Query`.

## Scope

STUBS01 class detection. The free-function list below it is hand-maintained and out of scope.

## Design Dependencies

None.

## Documentation assessment

`liquers-web/README.md` only if it describes STUBS01's detection. Otherwise none.

## Consolidated Findings

- `awk` logic: remember the `js_name` of the last `#[wasm_bindgen(js_name = X)]` line. Clear it on
  any line that is not an attribute (`#[`), a doc comment (`///`), or blank. On `pub struct`/`pub
  enum`, emit the remembered name. This is portable (POSIX awk) and handles multi-line attribute
  stacks.
- `pub enum` matters if wasm_bindgen exports an enum under `js_name`. Include it.
