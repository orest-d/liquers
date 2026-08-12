---
id: QUERY-BUILDER-TOOLING
kind: issue
title: No programmatic builder for constructing queries
status: accepted
priority: P2
complexity: L
area: [core/query, core/validate]
design: 
created: 2026-08-08
github:
---
## Problem

There is no supported way to *build* a query from untrusted string values. Liquers has good tooling
for the parse direction — `parse_query`, and `liquers-validate` for checking that a query means what
was intended — and nothing for the reverse. Every consumer that assembles a query from data
therefore hand-concatenates strings and hopes the escaping is right.

This affects more than one caller: UI code building links, recipe generators, and **every language
integration**, since accepting a string parameter from a host language and putting it into a query is
the most basic thing such an integration does. `specs/liquers-web` needs it for `encodeParam`.

## Current workaround

Build the query programmatically and encode it, rather than concatenating text:

```rust
use liquers_core::query::{ActionRequest, Query};
// Construct the ActionRequest with owned parameter values, then:
let text = query.encode();
```

`ActionParameter::String` runs its value through `encode_token` on encode, so the escaping is at
least applied rather than forgotten.

**The workaround is only correct within the current escaping limits.** `encode_token` emits
unparseable text for any value containing a colon or a non-ASCII character — see
`PARAMETER-ESCAPING-INCOMPLETE`, which is the blocker for making this correct in general. Until that
is fixed, programmatic construction is safe for values drawn from `[A-Za-z0-9_+.]` plus the
characters `encode_token` does handle (`~`, space, `/`, `-`), and unsafe for anything else.

## Expected behavior

1. A **CLI utility** for constructing a query — the encode-direction counterpart of
   `liquers-validate`. Given a command name and parameter values as separate arguments (so the shell
   does the quoting, not the user), it emits the encoded query. Round-tripping its output through
   `liquers-validate` should reproduce the inputs exactly.
2. A supported **library API** for the same, usable from `liquers-lib` and from language
   integrations, so that `encodeParam`-style helpers delegate rather than reimplement.
3. Both depend on `PARAMETER-ESCAPING-INCOMPLETE` being resolved to be correct for arbitrary input;
   until then they should **raise a typed error** for values they cannot represent rather than emit
   text that will not parse.

## Discovery

Raised while designing `specs/liquers-web`, where a JavaScript command takes a URL as a parameter.
The example initially assumed percent-encoding, which the grammar does not support at all; checking
the real mechanism surfaced both this gap and the encoder defect it depends on.
