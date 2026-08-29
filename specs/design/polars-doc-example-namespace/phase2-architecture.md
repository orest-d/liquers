# Phase 2: Solution and Architecture - Polars Reference Examples Select the pl Namespace

## Overview

Edit `specs/reference/POLARS_COMMAND_LIBRARY.md` so full query examples enter the Polars namespace
with `ns-pl` before the first Polars action. Repair or remove the trailing `-R/data/file.csv/-/`
example, then validate extracted queries with `liquers-validate`.

## Known-Issue Preflight

| Issue | Status | Priority | Relevance and action | Blocking? |
|---|---|---|---|---|
| `POLARS-COMMAND-TESTS-BYPASS-COMMANDS` | draft | P2 | Runtime command integration test weakness; documentation validation should use the committed registry and does not depend on fixing tests first. | no |
| `REGISTRY-IMPL-VERSION-DRIFT-UNDETECTED` | draft | P3 | Registry validation can be stale, so the implementation should regenerate/check registry only if current validation exposes drift. | no |

## Files and Symbols

Primary file: `specs/reference/POLARS_COMMAND_LIBRARY.md`. Validation uses
`cargo run -p liquers-core --features cli --bin liquers-validate` with an extracted query file.
No Rust source edits are planned.

## Data, Ownership, Serialization and Errors

Not applicable: no runtime data structures or serialization. Error path is validation feedback
from the existing validator.

## Sync, Async and API Effects

No API or async effects. The documentation contract changes from misleading examples to executable
namespace-qualified examples.

## Alternatives

Rejected: make `pl` a default namespace in runtime configuration; that would change command
resolution globally for a documentation defect. Rejected: mark examples non-runnable; the reference
is more valuable when copied examples work.

## Risk Assessment

| Assessment | Record |
|---|---|
| Files | 1 reference document, issue/design/index specs. |
| Impact area | Internal Polars command reference and example validation. |
| Module/crate reach | Documentation only; validation runs `liquers-core` CLI. |
| Existing-test breakage | None expected. |
| New validation | Extract fenced/inline full queries and run `liquers-validate --query-file`. |
| Behavioural risk | No runtime risk; docs risk is missing snippet examples that intentionally omit context. |
| Recovery | Revert reference edits and History row. |
| Certainty | High that namespace omission is the dominant failure; one malformed trailing transform must be handled explicitly. |

## Rust Review

No Rust implementation is planned. The design still respects command namespace semantics and avoids
using documentation to mask command-registration behaviour.
