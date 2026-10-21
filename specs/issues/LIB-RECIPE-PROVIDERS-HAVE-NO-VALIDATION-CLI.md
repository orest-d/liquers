---
id: LIB-RECIPE-PROVIDERS-HAVE-NO-VALIDATION-CLI
kind: feature
title: No command-line tool validates the recipes of liquers-lib's recipe provider chain
status: draft
priority: P3
complexity: S
area: [core/validate, records]
design:
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
