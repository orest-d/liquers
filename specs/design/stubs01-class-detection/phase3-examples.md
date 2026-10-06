# Phase 3: Examples and Tests

| # | Kind | Checks |
|---|---|---|
| T1 | run | `./liquers-web/scripts/check-stubs.sh` (after `./liquers-web/examples-web/quickstart/build.sh`) passes and prints `class Key`, `class Query`, `class RecordBatch` |
| T2 | negative (manual) | Temporarily rename `export class Key` in the generated `.d.ts` → STUBS01 fails on `Key` |
| T3 | unit-ish | Running only the awk snippet over `liquers-web/src` prints exactly: Asset, Environment, Key, LiquersError, Query, RecordBatch, State, Store, Value (plus any newer exports) |

Record T1–T3 output in the PR. Run after `cargo clean`, per CLAUDE.md's web loop.
