---
id: LIB-RECIPE-PROVIDERS-HAVE-NO-VALIDATION-CLI
kind: feature
title: No command-line tool validates the recipes of liquers-lib's recipe provider chain
status: draft
priority: P3
complexity: M
area: [core/validate, records]
design: records-recipe-validation-cli
created: 2026-10-20
github:
---

## Problem

`specs/design/validate-through-recipe-providers/` adds a library call
(`liquers_core::validate::provider::validate_provider_recipes`) and a `liquers-validate
--store-root` mode that uses core's `DefaultRecipeProvider` only. The providers that motivated it —
`ManifestRecipeProvider` (`liquers-records`) and whatever `liquers-lib`'s default environment chains
— live above core, so no CLI reaches a manifest's chunk recipes; only a test or program calling the
library does.

## Problem Example

`data/sales.manifest.yaml` holds the explicit chunk `-R/raw/sales/jan.csv/-/ns-rec/to_recrd/jan.csv`.
`liquers-validate -- '<that query>'` reports "Action 'to_recrd' not registered" (checked
2026-10-26), but only if the author copies the query out; nothing walks a manifest's chunks from
the command line, so the error first appears when `-R/data/jan.csv` is evaluated. Requires
`validate-through-recipe-providers` (not yet implemented). Re-sized S → M on 2026-10-26: a binary,
a `cli` feature and a shared chain function in `liquers-records`, plus a `liquers-lib` call site.

## Impact

Manifest chunk queries with an unknown command or a bad argument are found only on evaluation.
Workaround: call the library from a test.

## Expected behaviour

A binary linked against `liquers-lib` (feature `cli`, beside `export-command-registry`), or a
`liquers-records` `cli` feature, builds the same provider chain as the default `liquers-lib`
environment and runs `validate_provider_recipes` over a store root, with the same envelope as
`liquers-validate`. Which crate hosts it is open.

## Discovery

Split from `VALIDATE-CANNOT-SEE-NON-STANDARD-RECIPE-PROVIDERS` during bulk design on 2026-10-20 to
keep the first slice within `liquers-core`.
