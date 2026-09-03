---
id: REGISTRY-IMPL-VERSION-DRIFT-UNDETECTED
kind: issue
title: The committed command registry carries stale impl_versions and no test detects it
status: draft
priority: P3
complexity: S
area: [lib/commands, build, docs]
design: command-registry-impl-version-freshness
created: 2026-08-25
github:
---
## Problem

`specs/command_registry.yaml` is stale at HEAD. Regenerating it with no source change at all
rewrites two `impl_version` values:

| Command | Committed | Regenerated |
|---|---|---|
| `dep/command_metadata` | `1d837872f60743c09543fdd8895e5d96` | `0adcd172a5b226b3c2534a005d32846c` |
| `dep/command_implementation` | `d6ccb88b7a96ed84e21ddd5dc3c5bacc` | `221e51572f635c0c495c2f6d9e443774` |

Reproduced by checking out HEAD, regenerating to a scratch path, and diffing against the committed
file: those two lines are the only difference besides the header comment and the CHANGELOG block
(both of which differ only because the scratch path had neither).

`cargo test -p liquers-lib --test registry_export` passes anyway. It compares **signatures**, not
file bytes — a deliberate choice so that a reformat is not a failure — and `impl_version` is not
part of a signature. So this drift is invisible to the check that exists to prevent drift.

## Why it matters

`impl_version` is not decoration. `dep/command_implementation` is a command whose entire purpose is
to return it, and the versioning machinery exists so a stored asset can be invalidated when the
command that produced it changes. A committed registry that disagrees with the code means:

- anything consuming the checked-in registry (`liquers-validate`, tooling, humans reading it) sees
  a version that no build produces;
- the next person to regenerate for an unrelated reason gets two mystery lines in their diff and
  has to prove they are not their fault — which is exactly what happened in
  `specs/design/variadic-arguments-declaration/`, Step 6.

## Cause

Not established. `#[command_version]` blake3-hashes the function's whole token stream
(`liquers-macro/src/versioning.rs:15-21`), so any edit to `command_metadata` or
`command_implementation` (`liquers-lib/src/commands.rs:213`, `:~230`) changes it — including a
comment or whitespace change. Most likely one of those functions was edited after the last
regeneration and the file was not regenerated, because nothing failed.

Worth confirming with `git log` on those two functions versus the file, since the answer decides
whether this is a one-off or recurring.

## Fix direction

Two parts, and the second is the one that matters:

1. Regenerate the file. One line, and it fixes today's symptom only.
2. Make the drift detectable. Either include `impl_version` in what `registry_export` compares, or
   add a separate check asserting the committed file is byte-identical to a fresh export modulo the
   hand-maintained CHANGELOG block.

Option 2 has a cost worth weighing before choosing: `impl_version` changes whenever a command
function's text changes at all, including comments. Making the test enforce it means every such
edit requires a regeneration commit. That may be correct — it is what keeping a generated file
checked in implies — but it should be a deliberate decision rather than a side effect, and it is
why this is filed rather than fixed in passing.

## Discovery

Found in `specs/design/variadic-arguments-declaration/` while regenerating the registry after
converting `pl/select_columns` and `pl/drop_columns` to variadic arguments. The design predicted
`impl_version` churn for those two commands; two *unrelated* commands also changed, and stashing
the source changes and regenerating at HEAD showed the drift was already there.
