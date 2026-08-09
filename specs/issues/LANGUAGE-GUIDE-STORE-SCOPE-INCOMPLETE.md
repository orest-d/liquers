---
id: LANGUAGE-GUIDE-STORE-SCOPE-INCOMPLETE
kind: issue
title: The guide's STORE feature omits integration-provided stores and store configuration
status: draft
priority: P2
complexity: M
area: [web, py, core/store]
design:
created: 2026-08-09
github:
---

## Problem

`specs/guides/LANGUAGE-INTEGRATION_GUIDE.md` §5 `STORE` is written entirely around one shape:
"Adapt a *language value* to the complete `AsyncStore` contract" — a store object implemented in
the *integrated language*. Its design questions ("Which methods are mandatory versus safely
defaulted? Are bytes copied or viewed? How are language sync methods scheduled?") and all seven
prescribed tests `STORE01`–`STORE07` follow from that shape alone.

Two things an *integration* actually has to decide are therefore unasked:

1. **Stores the *integration itself* provides.** A browser integration's most useful stores are
   written in Rust, not in JavaScript: a `fetch`-backed read-only HTTP store, a `localStorage` or
   IndexedDB store. Nothing in §5 asks what a *language*-appropriate store backend should be, how a
   read-only store reports its write refusals, how metadata is inferred when the backend carries
   none (extension, response media type), or how a backend without directory semantics satisfies
   `STORE03`'s listing invariants.
2. **Store configuration and composition.** The "Objects/API to map or implement" list mentions
   "Store builder/configuration and composition objects from `liquers-store`, where selected" as a
   single bullet, and then no design question and no test addresses it. Whether one configuration
   document means the same thing on native and in the *language*'s host, how routing prefixes
   interact with a store's own key prefix, and what `${VAR}` expansion means where there is no
   environment are all real decisions with wrong answers available.

## Impact

An *integration* design following §5 literally satisfies the letter of `STORE` while leaving its
largest surface undesigned and untested — a store the *integration* ships is not covered by any of
`STORE01`–`STORE07` unless the designer reinterprets the tests on their own initiative.
`specs/design/liquers-web-store/` hit this immediately: three of its four stores and its entire
configuration layer fall outside what the feature asks about. Workaround is exactly that
reinterpretation, which is unreliable precisely because §3 ("When a prescribed test does not
apply") warns that reinterpretation is where inventories go soft.

`liquers-py` will hit the same gap when it selects `STORE`, so this is not browser-specific.

## Expected behaviour

§5 `STORE` covers both directions. Concretely, at least:

- Design questions for an *integration*-provided store backend: which host storage mechanisms are
  appropriate, read-only versus read-write, metadata inference where the backend has no metadata,
  directory semantics on a flat key-value backend, and quota or size limits.
- Design questions for configuration: is one configuration document portable across native and the
  *language* host, how store `key_prefix` relates to router prefixes, and what variable expansion
  means in the host.
- Prescribed tests for both. Candidates, keeping the existing IDs untouched and appending:
  `STORE08` an *integration*-provided store satisfies the same contract as a *language*-defined
  one; `STORE09` a read-only store refuses writes with the right error rather than silently
  succeeding; `STORE10` metadata inference from filename and backend response; `STORE11` a store
  router built from configuration routes by prefix.

Whether these belong under `STORE` or split into a separate feature ID is open — a separate ID
would let an *integration* select configuration without a *language*-defined store, which is
`liquers-web`'s actual situation.

## Discovery

Found while answering Phase 1 open question `Q1` of `specs/design/liquers-web-store/`
(2026-08-09): the design's four stores split two-and-two across a distinction the guide does not
make, and its configuration layer matched no design question in §5. Not fixed inside that design,
since amending the guide from within the first design that trips over it would make the design its
own conformance definition.
