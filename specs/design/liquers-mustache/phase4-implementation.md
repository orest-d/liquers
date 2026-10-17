# Phase 4: Implementation Plan — liquers-template

## Overview

The build goes bottom-up, so each layer is tested before anything uses it: core additions first
(`DataNode`, `apply_ad_hoc`, `with_default_link`), then the records adapters, then the new
`liquers-template` crate (names, query templates and parity, Mustache parsing, rendering), then the
`liquers-lib` commands, and documentation last. No open issue blocks it (Phase 2 preflight).
`SEARCH-COMMANDS-RECORD-NO-DEPENDENCY-ON-READ-FILES` is designed around in Phase 3, not fixed here.

Signatures re-opened at HEAD for this plan: `ValueInterface` (`liquers-core/src/value.rs:168`,
`try_into_json_value` and `try_from_json_value`), `Context::apply`, `get_dependency_state`,
`reject_relative_query`, `resolve_query_from_cwd` (`pub(crate)`) and `add_dependency`
(`context.rs`), `AssetManager::apply_for(parent, recipe, to, payload)` (`assets.rs:5936`),
`CommandRegistry::register_alias` (`commands.rs:704`), `ArgumentInfo::default`
(`command_metadata.rs:535`, public), `ParameterValue::from_command_parameter_value` (a `Query`
head becomes `DefaultLink`, `plan.rs:580`), the public `escape` functions (`encode_token`,
`decode_token`, `is_resource_name_char`), `maybe_send.rs`, the workspace `members` and
`default-members`, `liquers-lib`'s `[features]` and `register_all_commands!` (`commands.rs:329`).

Disk: every proof is a single-crate `cargo test`; no step runs `--workspace`. Run `cargo clean`
before the wasm32 step.

## Progress
- [x] Step 1: `DataNode` in liquers-core (8fde04a)
- [x] Step 2: `Context::apply_ad_hoc` (see the commit adding `tests/apply_ad_hoc.rs`; D6)
- [x] Step 3: `ArgumentInfo::with_default_link` and link heads in aliases (link heads already worked; test added)
- [x] Step 4: Deprecate `simple_template`
- [x] Step 5: Records adapters
- [x] Step 6: `liquers-template` crate, diagnostics and names
- [x] Step 7: Query template AST, parser, encoder and parity (sentinel parser; D7)
- [x] Step 8: Instantiation
- [x] Step 9: Mustache AST, lexer, parser and encoder
- [x] Step 10: Renderer, hosts and error modes (D8)
- [x] Step 11: Validation
- [ ] Step 12: wasm32 build and Node tests
- [ ] Step 13: liquers-lib commands, hosts and value adapters
- [ ] Step 14: Registry export
- [ ] Step 15: Documentation
- [ ] Step 16: Final checks

## Implementation Steps

### Step 1: `DataNode` in liquers-core
- Files / symbols: `liquers-core/src/data_node.rs` (new): `NodeKind`, `Scalar`, `DataNode`,
  `NodeRef`, `AsyncDataNode`, `AnyNode`, `NodeIter`, `KeyIter`, `NodeStream`, defaulted `index`;
  impls for `Value`, `MetadataRecord`, `serde_json::Value`. `lib.rs` (module).
  `value.rs`: `ValueInterface::data_node(value: Arc<Self>) -> AnyNode`, defaulted to a JSON
  snapshot or `Opaque`.
- Depends on: none
- Proof: `cargo test -p liquers-core --lib data_node` (`value_as_data_node`,
  `metadata_record_as_data_node`, `json_as_data_node`, `sync_consumer_refuses_async_node`), then
  `PYO3_USE_ABI3_FORWARD_COMPATIBILITY=1 cargo check -p liquers-py` (Python here is 3.13; it
  implements `ValueInterface` and must inherit the default)
- Rollback: revert the module and the defaulted method; nothing else uses them yet

### Step 2: `Context::apply_ad_hoc`
- Files / symbols: `liquers-core/src/context.rs` `Context::apply_ad_hoc(&self, recipe: Recipe, to:
  State<E::Value>) -> Result<AssetRef<E>, Error>`: `reject_relative_query`, `resolve_query_from_cwd`,
  `apply_for`, then the ad-hoc asset's dependencies (`AssetRef::get_metadata`,
  `Metadata::get_dependencies`) through `add_dependency`. Volatility is not passed on, as
  `get_dependency_state` does not pass it on (D6); a failed ad-hoc evaluation is returned as its error.
  `liquers-core/tests/apply_ad_hoc.rs` (new)
- Depends on: none (parallel with Step 1)
- Proof: `cargo test -p liquers-core --test apply_ad_hoc` (`apply_ad_hoc_propagates_dependencies`,
  `apply_ad_hoc_rejects_relative_query`, `apply_ad_hoc_volatility_is_static`)
- Rollback: remove the method and the test file

### Step 3: `ArgumentInfo::with_default_link` and link heads in aliases
- Files / symbols: `command_metadata.rs` `ArgumentInfo::with_default_link(self, query: Query) -> Self`;
  `liquers-core/tests/command_alias.rs` gains `alias_head_link_is_evaluated`
- Depends on: none (parallel)
- Proof: `cargo test -p liquers-core --lib command_metadata` (`with_default_link_sets_query_default`),
  `cargo test -p liquers-core --test command_alias`. If the link head fails, the fix is in
  `from_alias_action` or the dependency scan, inside this step
- Rollback: remove the builder and the test

### Step 4: Deprecate `simple_template`
- Files / symbols: `#[deprecated(note = "use liquers-template")]` on `SimpleTemplate`,
  `parse_simple_template` (`parse.rs`) and `evaluate_simple_template` (`interpreter.rs`);
  `#[allow(deprecated)]` at the in-crate uses: the import in `interpreter.rs`, `impl IntoIterator for
  SimpleTemplate` and `impl SimpleTemplate`, the `simple_template` parser function, and their tests
- Depends on: none
- Proof: `cargo test -p liquers-core --lib` and `cargo test -p liquers-core --doc parse`, with no
  deprecation warning
- Rollback: drop the attributes

### Step 5: Records adapters
- Files / symbols: `liquers-records/src/data_node.rs` (new): a row map over `RecordView` (sync,
  `index` builds one row) and `RecordSourceNode::new(source, resolver: Arc<dyn ChunkResolver>)`
  (async), since a source cannot stream without a resolver
- Depends on: Step 1
- Proof: `cargo test -p liquers-records --all-features --lib data_node` (`record_view_rows_as_maps`,
  `record_view_index_builds_one_row`, `record_source_streams_rows`)
- Rollback: remove the module

### Step 6: `liquers-template` crate, diagnostics and names
- Files / symbols: `liquers-template/Cargo.toml` (depends on `liquers-core`, `nom`, `nom_locate`,
  `serde`, `serde_json`, `futures`, `async-trait`; tokio only under
  `[target.'cfg(not(target_arch = "wasm32"))'.dev-dependencies]`, `wasm-bindgen-test` under the
  wasm32 one); workspace `members` and `default-members`; `src/lib.rs`; `src/diagnostics.rs`
  (`Diagnostic`, `Severity`); `src/name.rs` (`Name`, `NamePart`, `LookupKey`, parser, resolution
  and `encode`); `src/stack.rs` (`ContextStack`, `FrameKind`)
- Depends on: Step 1
- Proof: `cargo test -p liquers-template --lib name` (`dotted_index_selects_item`,
  `dotted_missing_is_empty`, `bracket_lookup_by_kind`, `bracket_missing_is_error`,
  `bracket_negative_index`; resolution is tested here against `serde_json::Value` nodes)
- Rollback: remove the crate and its workspace entries

### Step 7: Query template AST, parser, encoder and parity
- Files / symbols: `src/query/{ast,parse}.rs`: the AST of Phase 2 §AST, `parse_query_template`,
  `encode`, `From<&Query>`, `try_into_query`, `slots`; `tests/parity.rs` with the corpus (core
  parser tests, queries under `specs/`, the registry, a deterministic grammar walk)
- Depends on: Step 6
- Proof: `cargo test -p liquers-template --test parity` (the four `parity_*` tests)
- Rollback: the crate is not yet used; revert the files

### Step 8: Instantiation
- Files / symbols: `src/query/instantiate.rs` `QueryTemplate::instantiate`, `Instantiated`
- Depends on: Steps 6, 7
- Proof: `cargo test -p liquers-template --lib instantiate` (the eight AC-5, AC-6 and AC-16 tests)
  and `cargo test -p liquers-template --lib name::tests::bracket_in_query_slot` (AC-17)
- Rollback: revert the file

### Step 9: Mustache AST, lexer, parser and encoder
- Files / symbols: `src/mustache/{ast,lexer,parse,encode}.rs`: `Template`, `MustacheTemplate`,
  `Element` with `Invalid`, `Standalone`, brace balancing, set-delimiter tracking; `parse_template`
- Depends on: Steps 6, 7
- Proof: `cargo test -p liquers-template --lib mustache` (the parse tests and `encode_round_trips`)
- Rollback: revert the files

### Step 10: Renderer, hosts and error modes
- Files / symbols: `src/host.rs` (`TemplateHost` with `command_registry` and `warn`,
  `QueryRequest`, `MapHost`), `src/render.rs` (`render`, `RenderOptions`, `ErrorMode`,
  `ExpandFormat`) and `src/mustache/render.rs` (sections, partial stack and depth cap, error modes); `tests/spec/` (the mustache/spec JSON files, MIT, with their licence), `tests/spec.rs`,
  `tests/common/recording_host.rs`, `tests/render.rs`
- Depends on: Steps 8, 9
- Proof: `cargo test -p liquers-template --test spec --test render`
- Rollback: revert the files

### Step 11: Validation
- Files / symbols: `src/diagnostics.rs` `validate_template(text, syntax, cmr)`
- Depends on: Step 10
- Proof: `cargo test -p liquers-template --lib diagnostics` (`validate_reports_all`)
- Rollback: revert the function

### Step 12: wasm32 build and Node tests
- Files / symbols: `liquers-template/tests/node.rs`; `#![cfg(not(target_arch = "wasm32"))]` on
  `spec.rs`, `parity.rs`, `render.rs`; `scripts/check-build-matrix.sh` rows for `liquers-template`
  on wasm32
- Depends on: Step 10
- Prerequisites: `rustup target add wasm32-unknown-unknown`; `cargo install wasm-bindgen-cli`
  pinned to the `wasm-bindgen` version in `Cargo.lock` (the runner `.cargo/config.toml` names);
  Node on `PATH`
- Proof: `cargo clean`, then `cargo test -p liquers-template --target wasm32-unknown-unknown`. If
  the runner cannot be installed, the step proves the build with
  `cargo check -p liquers-template --tests --target wasm32-unknown-unknown` and files the gap
- Rollback: revert the gating and rows

### Step 13: liquers-lib commands, hosts and value adapters
- Files / symbols: `liquers-lib/Cargo.toml` feature `templates = ["dep:liquers-template"]`, added to
  `default`; `src/template/{mod,commands,host,folder}.rs`: `render`, `expand`, `validate`,
  `ContextHost`, the store-folder node; `value/extended.rs` (`CombinedValue::data_node`),
  `value/mod.rs` (`ValueExtension::data_node`, overridden for `RecordView`, `RecordSource`,
  `PolarsDataFrame`) — the trait is in `value/extended.rs`, the overrides in `value/mod.rs`;
  `ContextHost::node_of` wrapping a `RecordSource` with `RecordSourceNode` and a `ContextResolver`;
  `register_template_commands!` in `register_all_commands!` and its no-op stand-in;
  `tests/template_commands.rs`; `scripts/check-build-matrix.sh` `LIB_CONFIGS` rows
  `--no-default-features --features templates` and `templates,records`, and a wasm32
  `--features templates` row
- Depends on: Steps 2, 3, 5, 11
- Proof: `cargo test -p liquers-lib --test template_commands`, then
  `cargo test -p liquers-lib --no-default-features --features templates --lib --tests`
- Rollback: drop the feature from `default`; the module is gated

### Step 14: Registry export
- Files / symbols: regenerate `specs/command_registry.yaml` with a dated CHANGELOG line;
  `tests/registry_export.rs`: `feature = "templates"` in each four-group `cfg`, a `full_registry`
  arm, an anchor in `exported_registry_is_nonempty`; `CLAUDE.md` "all four groups" becomes five
- Depends on: Step 13
- Proof: `cargo test -p liquers-lib --test registry_export`
- Rollback: restore the previous file and gates

### Step 15: Documentation
- Files / symbols: the nine `affects_docs` paths (Phase 2 §Documentation Architecture), each change
  with a `## History` row and a `reviewed:` bump; `CLAUDE.md` (project structure, dependency flow,
  where code goes, a template-commands line in "Adding a Command"); `specs/README.md`
- Depends on: Step 14
- Proof: `python3 scripts/docs_index.py && python3 scripts/docs_index.py --check`; every query in
  the new documents through `liquers-validate`
- Rollback: revert the documents

### Step 16: Final checks
- Files / symbols: none
- Depends on: all
- Proof: `cargo test -p liquers-core --lib --tests`, `cargo test -p liquers-template`,
  `cargo test -p liquers-records --all-features --lib --tests`,
  `cargo test -p liquers-lib --lib --tests`, `bash scripts/check-build-matrix.sh`
- Rollback: none

## Testing Plan

Each step runs its own tests; Steps 1–3 also rerun `cargo test -p liquers-core --lib --tests`,
since they touch core. Step 13 runs the reduced `templates`-only configuration. The final step
runs the four crate suites and the build matrix, which covers the new `cfg(feature)` and the
wasm32 rows. The known flake `volatile_dependency_has_one_value_per_evaluation` is re-run once if
it fails and is not counted against a step.

## Rollback Plan

Everything is additive. The whole change is reverted by dropping `templates` from `liquers-lib`'s
`default` and the crate from the workspace; the core additions stay unused and harmless. If only
some steps land, the Progress list says where to resume, and a step that cannot be finished
becomes an issue (§5.6).

## Documentation Updates

Created: `specs/reference/TEMPLATE_LANGUAGE.md`, `specs/guides/TEMPLATE_GUIDE.md`. Updated:
`VALUE_TYPE_SYSTEM.md`, `DOC_04_ENVIRONMENT_CONTEXT_EVALUATION.md`,
`DOC_02_QUERY_LANGUAGE_REFERENCE.md`, `PROJECT_OVERVIEW.md`, `COMMAND_REGISTRATION_GUIDE.md`,
`COMMAND_DESIGN_GUIDE.md`, `COMMAND_ALIASES.md`, `CLAUDE.md`, `specs/README.md`. Regenerated:
`specs/command_registry.yaml`, `specs/index.csv`, `specs/index.md`.

## Phase 5 Entry Criteria
- [ ] Implementation finished and validated
- [ ] User and review comments answered
- [ ] Documentation checkable against implemented and tested behaviour
