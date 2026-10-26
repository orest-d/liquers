---
id: WEB-API-REFERENCE-NOT-GENERATED
kind: feature
title: liquers-web has no generated API reference and no check that every export is documented
status: draft
priority: P3
complexity: M
area: [web, docs]
design:
created: 2026-10-26
github:
---
## Problem

`DOCS04` in `guides/LANGUAGE-INTEGRATION_GUIDE.md` asks for an API reference in the language's
standard form, with a check that every public name carries documentation. `liquers-web` has the
contract reference (`reference/JAVASCRIPT_INTEGRATION.md`) and the user guide
(`guides/JAVASCRIPT_GUIDE.md`), but no generated reference: the generated `liquers_web.d.ts` carries
the Rust doc comments as JSDoc, and several exports have none (for example the `LiquersError.message`,
`Value.identifier` and `Value.typeName` getters).

## Impact

A page author reads the `.d.ts` or the guide; an export added without a doc comment goes unnoticed.

## Expected behaviour

TypeDoc (or equivalent) over the generated `.d.ts`, run by `scripts/check-stubs.sh`, failing when an
exported class, function, method or getter has no comment. The Python counterpart is
`PY-API-REFERENCE-NOT-GENERATED`.

## Discovery

Split from `WEB-HAS-NO-API-REFERENCE-OR-USER-GUIDE` when its contract reference and user guide were
written (2026-10-26).
