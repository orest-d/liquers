---
id: WEB-LIQUERSERROR-NOT-CONSTRUCTIBLE
kind: issue
title: JavaScript cannot construct a LiquersError, so a page cannot raise a typed error
status: draft
priority: P3
complexity: S
area: [web, core/error]
design:
created: 2026-08-09
github:
---

## Problem

`LiquersError` (`liquers-web/src/error.rs:83`) exposes getters to JavaScript but no
`#[wasm_bindgen(constructor)]`, so a page can *receive* a structured Liquers error and cannot
*create* one. Everything a page throws is therefore an ordinary `Error`, and
`js_error_to_liquers` gives it the caller's fallback `ErrorType`.

The error bridge already round-trips a `LiquersError` thrown back at Liquers without degradation
(`js_error_to_liquers` checks for one first), so the receiving half of this contract exists and is
tested — there is simply no way for a page to produce the value it is looking for.

## Impact

A *language*-implemented service cannot say *why* something failed in Liquers' own vocabulary. The
concrete case is a `JsStore` (`specs/design/liquers-web-store/`): a page store that hits a
permission failure, a quota, or an unsupported key can only produce whatever fallback the adapter
chose, and every one of those becomes `KeyReadError`.

Narrow today, because the store protocol was designed around it: absence is signalled by returning
`undefined` rather than by throwing, so the common case needs no typed error. It widens as soon as
another *language*-implemented service is added — a `RECIPE` provider has the same shape, and
`recipe_opt` explicitly must distinguish "not found" from a failure.

Related but distinct: `LANGUAGE-EXCEPTION-FIELDS-LOST-IN-TRANSPORT` is about a language exception's
class and stack being dropped on the way *in*. This is about a page being unable to construct a
Liquers error at all.

## Expected behaviour

`LiquersError` gains a constructor taking an error-type name and a message:

```js
throw new liquers.LiquersError("key_not_found", `no such key: ${key}`);
```

The name is validated against `error_type_from_name`, which already exists and already returns
`None` for an unrecognised name — so an unknown name should be a `TypeError` at construction rather
than a silent downgrade to `general`.

Worth deciding at the same time whether the constructor should also accept the optional `key` and
`query` fields the struct carries, or whether those stay Liquers-populated.

## Discovery

Found on 2026-08-09 while writing the `JsStore` conformance tests for
`specs/design/liquers-web-store/` M5. The Phase 3 example had a page throwing a `LiquersError`;
writing the test showed it could not be constructed. The example was corrected to return
`undefined`, which is the better protocol anyway, so the design has no dependency on this fix.
