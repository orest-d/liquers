# Phase 4: Implementation Plan — query-builder

## Overview

First the shared predicates (they are the base of everything else and fix
`QUERY-TOKEN-CLASSES-TRUNCATE-NON-ASCII` if it is still open), then the verifier, the builder, the
CLI, the two consumers and the documents. Each step compiles and tests on its own. Prerequisite:
the Phase 1 decisions (CLI shape, empty parameter in `liquers-web`).

## Progress

- [ ] Step 1: token predicates in `escape.rs`; parser uses them
- [ ] Step 2: `Query::check_encodable`
- [ ] Step 3: `QueryBuilder`
- [ ] Step 4: `liquers-encode` binary
- [ ] Step 5: `liquers-template` delegates to the predicates
- [ ] Step 6: `liquers-web` accepts the empty parameter
- [ ] Step 7: link the bindings follow-up
- [ ] Step 8: documents and issues

## Implementation Steps

### Step 1: token predicates
- Files / symbols: `liquers-core/src/escape.rs` (new `is_identifier`, `is_identifier_start_char`,
  `is_identifier_char`, `is_resource_name`, `is_filename`, `is_filename_char`, `is_header_value`,
  `is_header_char`); `liquers-core/src/parse.rs` `identifier`, `filename`, `header_parameter`,
  `full_transform_segment_header`, `resource_segment_header`
- Change: replace every `c as u8` test with the char predicates; correct the `a15` test comment.
  If `QUERY-TOKEN-CLASSES-TRUNCATE-NON-ASCII` is already fixed, only add the string-level
  predicates
- Depends on: none
- Proof: `cargo test -p liquers-core --lib parse escape` with `token_predicates_match_the_parser`;
  `cargo test -p liquers-core --test query_backward_compatibility`
- Rollback: revert the two files

### Step 2: `Query::check_encodable`
- Files / symbols: `liquers-core/src/query.rs` `impl Query`
- Change: the walk in Phase 2 Interfaces, with the `ParameterError` messages
- Depends on: Step 1
- Proof: `cargo test -p liquers-core --lib query::tests::check_encodable` (AC-4)
- Rollback: remove the method

### Step 3: `QueryBuilder`
- Files / symbols: new `liquers-core/src/query_builder.rs`; `pub mod query_builder;` in
  `liquers-core/src/lib.rs`
- Change: the type and methods in Phase 2. Each step calls the Step 1 predicates; `build` returns
  the query (with a `debug_assert!(q.check_encodable().is_ok())`)
- Depends on: Steps 1-2
- Proof: `cargo test -p liquers-core --lib query_builder` (AC-1, AC-2, AC-3, AC-8)
- Rollback: delete the module

### Step 4: `liquers-encode`
- Files / symbols: new `liquers-core/src/bin/liquers_encode.rs`; `liquers-core/Cargo.toml` second
  `[[bin]]` with `required-features = ["cli"]`; new `liquers-core/tests/liquers_encode_cli.rs`
- Change: clap options and ordered reconstruction (Phase 2). Self-check re-parse before printing.
  Exit codes 0/1/2
- Depends on: Step 3
- Proof: `cargo test -p liquers-core --features cli --test liquers_encode_cli` (AC-5, AC-6); then
  pipe Example 2's output to `target/debug/liquers-validate --command filter --`
- Rollback: remove the bin entry and file

### Step 5: `liquers-template` delegates
- Files / symbols: `liquers-template/src/query/instantiate.rs` `is_identifier`, `is_key_segment`,
  `is_filename`
- Change: call the `liquers_core::escape` predicates; keep the `.`/`..` refusal in
  `is_key_segment`
- Depends on: Step 1
- Proof: `cargo test -p liquers-template` (AC-7)
- Rollback: restore the local bodies

### Step 6: `liquers-web` empty parameter
- Files / symbols: `liquers-web/src/encode.rs` `encode_param`, its test
  `web_encode_param_refuses_the_empty_string` → `web_encode_param_accepts_the_empty_string`;
  `liquers-web/src/lib.rs` `encode_param_js` doc comment; `liquers-web/README.md` if it lists the
  refusal
- Depends on: none
- Proof: after `cargo clean`, `cargo test -p liquers-web --target wasm32-unknown-unknown --features debug-handles` (AC-8)
- Rollback: restore the refusal

### Step 7: link the bindings follow-up
- `specs/issues/QUERY-BUILDER-NOT-IN-BINDINGS.md` (filed with this design, P3, M): add a pointer
  to the landed builder API so the follow-up starts from it
- Proof: `python3 scripts/docs_index.py --check`

### Step 8: documents and issues
- Files: the `affects_docs` set, `CLAUDE.md` §Validating queries, the `liquers-validate` skill;
  History rows and `reviewed:` bumps; close `QUERY-BUILDER-TOOLING` (and
  `QUERY-TOKEN-CLASSES-TRUNCATE-NON-ASCII` if Step 1 fixed it) with resolutions
- Proof: `python3 scripts/docs_index.py --check`; `cargo test -p liquers-core --doc`

## Testing Plan

After each step, the proof in it. Final: `cargo test -p liquers-core --lib --tests`,
`cargo test -p liquers-core --features cli --tests`, `cargo test -p liquers-template`,
`cargo test -p liquers-lib --lib --tests` (templates feature uses the predicates). No new optional
dependency (clap is already behind `cli`), so the build matrix is not required. The `liquers-web`
loop runs separately after `cargo clean`.

## Rollback Plan

Steps 3-4 are additive and revert alone. Step 1 narrows the parser to the documented grammar; if a
real query relying on a truncated character turns up, revert Step 1 alone, and the builder keeps
working with its stricter ASCII checks.

## Documentation Updates

`QUERY_ESCAPING_GUIDE.md`, `DOC_02`, `PROJECT_OVERVIEW.md`, `TEMPLATE_LANGUAGE.md`, `CLAUDE.md`, the
`liquers-validate` skill. No `command_registry.yaml` change. `specs/index.csv` regenerates for the
new and closed issues.

## Phase 5 Entry Criteria

None — four-phase design.
