# Phase 3: Examples and Tests

## Examples

`from_csv/head-10` becomes `ns-pl/from_csv/head-10` when it is a complete action pipeline. A
resource input uses a syntactically valid resource/query boundary before `ns-pl`; a command table
entry such as `head-n` remains unchanged because it is not a full query.

## Validation Cases

1. Inventory every fenced or inline string intended as a complete query in
   `POLARS_COMMAND_LIBRARY.md` and classify it as runnable, placeholder-bearing, or fragment.
2. Validate runnable entries with `liquers-validate` and `specs/command_registry.yaml`; expected
   result is no `ActionNotRegistered` and no parse error.
3. Check representative `from_csv`, selection, variadic `select_columns`, aggregation, and output
   pipelines so namespace insertion does not alter parameter grouping.
4. Run the docs-index checker and link checker for reference integrity.

No unit-test source is added for this docs-only issue. The executable acceptance artifact is the
curated query file passed to the existing validator during implementation and removed afterward
unless the repository has an established fixture location.
