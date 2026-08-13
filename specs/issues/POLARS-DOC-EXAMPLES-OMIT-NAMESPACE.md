---
id: POLARS-DOC-EXAMPLES-OMIT-NAMESPACE
kind: issue
title: Polars reference example queries omit the ns-pl namespace and do not resolve
status: draft
priority: P2
complexity: S
area: [docs, lib/commands]
design: 
created: 2026-08-12
github:
---
## Problem

Almost every example query in `specs/reference/POLARS_COMMAND_LIBRARY.md` names polars commands
without selecting their namespace, so none of them resolves. Polars commands are registered with
`namespace: "pl"` (`liquers-lib/src/polars/*.rs`), while the default namespaces are `""` and
`root` — the query needs an `ns-pl` instruction.

Measured: 14 distinct example queries were extracted from the document and run through
`liquers-validate` against the committed 95-command registry. **Thirteen failed**, twelve of them
with `ActionNotRegistered`:

```
-R/data/sales.csv/-/from_csv                   ActionNotRegistered: 'from_csv' not registered in namespaces '', 'root'
-R/data.csv/-/head-10                          ActionNotRegistered: 'head' not registered in namespaces '', 'root'
-R/sales.csv/-/gt-amount-1000/eq-status-completed   ActionNotRegistered: 'gt' ...
```

Adding the namespace makes them resolve:

```
-R/sales.csv/-/ns-pl/gt-amount-1000/eq-status-completed        status Ok
-R/data/sales.tsv/-/ns-pl/from_csv-tab/head-10                 status Ok
```

One further example does not parse at all: `-R/data/file.csv/-/` (`ParseError`: a trailing `/-/`
with no transform).

## Impact

A reader copying any example from the reference gets `ActionNotRegistered`. The document is the
primary reference for the `pl` namespace, so this is the first thing a new user of polars commands
encounters.

## Expected behaviour

Every example query in the document resolves against the committed registry, and the document is
explicit that polars commands live in the `pl` namespace and need `ns-pl` (or a realm/namespace
already in scope).

## Fix direction

Prefix the example queries with `ns-pl/` at the point the transform segment starts, drop or repair
the `-R/data/file.csv/-/` example, and add a sentence near the top stating the namespace
requirement once so each example does not have to explain itself.

Then re-run the extraction to confirm: harvest the queries and pass them to
`cargo run -p liquers-core --features cli --bin liquers-validate -- --query-file <file>`. This is
cheap and offline, and it is what turned the defect up.

Worth checking the other command-library references — `IMAGE_COMMAND_LIBRARY.md` documents an
`img` namespace and may have the same defect.

## Discovery

Found while correcting the `select_columns` / `drop_columns` spelling in the same document for
`specs/design/excess-action-parameters-error/`. Validating those examples to confirm the arity fix
showed that they had never resolved for an unrelated reason. Kept separate because the namespace
defect predates that design and is independent of it.
