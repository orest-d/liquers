# Phase 3: Examples & Use-cases — liquers-template

Every test below is runnable. Tests that need no environment live in `liquers-template` and run on
`MapHost` (no queries) or `RecordingHost`, a test host in `liquers-template/tests/common/` that
records each `QueryRequest` and answers from a map. Tests that evaluate queries live in
`liquers-lib/tests/template_commands.rs` (`#![cfg(all(feature = "templates", feature = "records"))]`)
and reuse the four-file issue fixture of `liquers-lib/tests/search_commands.rs`, moved to
`tests/common/issue_fixture.rs`. Every query here passes `liquers-validate` against the committed
registry plus an overlay declaring `template/render`, `template/validate` and `fixture/ctx`; the
overlay writes `cwd`'s default as `!Value ''`, because a link default cannot be written as text
(`REGISTRY-QUERY-DEFAULT-NOT-WRITABLE-AS-TEXT`, filed).

## Overview Table

| # | Kind | Name | Shows / checks | Scenarios |
|---|---|---|---|---|
| 1 | Example | Context pack of open issues | The problem example end to end through `template/render` | AC-2, AC-3, AC-5, AC-8, AC-11 |
| 2 | Example | Hygienic, classic, apply and override | What each tag form sends to the host | AC-5, AC-6, AC-7, AC-16 |
| 3 | Example | Indexing and error modes | `items.0`, `x[y]`, and one failing tag under each mode | AC-17, AC-18 |
| 4 | Tests | Conformance and parity | Mustache spec suite, core parser parity, encode round-trip | AC-1, AC-14 |
| 5 | Tests | Data, partials, validation, portability | `DataNode` impls, partial cycles, `validate`, wasm32 | AC-4, AC-9, AC-12, AC-13, AC-15 |

## Example 1: context pack of open issues

The fixture store holds `specs/issues/{A-EXPIRY,B-CLOSED,C-STORE,D-TOML}.md`; only `C-STORE` is in
area `core/store` and not closed. The template is the problem example of Phase 1, stored at
`templates/open_issues.md`:

```markdown
# Open issues in {{area}}
{{#$ -R-key/specs/issues/-/ns-search/field_extract/search-area~ncolon~{{area}}~.~_status~ncolon~closed~nast~-f-f-10}}
- [{{title}}]({{key}}){{#priority}} — {{priority}}{{/priority}}
{{/}}
{{^$ -R-key/specs/issues/-/ns-search/field_extract/search-area~ncolon~{{area}}~.~_status~ncolon~closed~nast~-f-f-10}}
No open issues.
{{/}}
```

Steps:
1. `ns-fixture/ctx-core~/store` returns `{"area": "core/store"}`; it is the state, so the root context.
2. The planner evaluates the `template` link, a dependency of the result, and the text arrives as
   the `template` argument.
3. The hygienic tag fills `{{area}}` as a fragment of one parameter (P1). The instantiated query
   encodes as `…/search-area~ncolon~core~/store~.~_status~ncolon~closed~nast~-f-f-10`, still one
   parameter, so the `/` in the value cannot start a new segment.
4. No apply marker and no override, so `ContextHost` calls `get_dependency_state` (AC-8): the query
   is cached and recorded as a dependency of the rendered asset.
5. The records iterate; each row is a map of columns, so `{{title}}` and `{{key}}` resolve.

```rust
put(&store, "templates/open_issues.md", OPEN_ISSUES).await?;
let state = eval(&env, "ns-fixture/ctx-core~/store/ns-template/render-~X~-R/templates/open_issues.md~E--strict").await?;
assert_eq!(state.try_into_string()?, "# Open issues in core/store\n- [Store listing](specs/issues/C-STORE.md)\n");
```

`C-STORE` has no priority, so the `{{#priority}}` section renders nothing. A second assertion edits
`C-STORE.md` (status `closed`) and evaluates again: the result is recomputed and renders
`No open issues.`, which proves the dependency (AC-2).

## Example 2: what each tag form asks the host

On `RecordingHost` with data `{"x": "a-b", "tags": ["p", "q"], "row": {"n": 1}}`. Only the
recorded `QueryRequest` is shown; nothing else changes from Example 1.

| Tag | `query` recorded | `overrides` | `input` |
|---|---|---|---|
| `{{$ cmd-{{x}}/q}}` | `cmd-a~_b/q` (one parameter) | none | none |
| `{{$$ cmd-{{x}}/q}}` | `cmd-a-b/q` (two parameters) | none | none |
| `{{$. to_text}}` inside `{{#row}}` | `to_text` | none | `{"n": 1}` |
| `{{$.row to_text}}` | `to_text` | none | `{"n": 1}` |
| `{{$ cmd-{{tags}}}}` | `cmd` | `[("arg", ["p", "q"])]` | none |
| `{{$ cmd-{{tags}}/q}}` | — | error at the slot: overrides reach only the last action | — |

The first row is `ContextHost`'s `get_dependency_state` route; every other row with an input or an
override takes `apply_ad_hoc`. `cmd`'s first argument is named `arg` in the test registry.

## Example 3: indexing and error modes

Data `{"items": [{"title": "first"}, {"title": "second"}], "i": 1, "m": {"a b": 7}}`, with
`ns-fixture/fail` registered to return an error:

```
{{items.0.title}} {{items[i].title}} {{items[-1].title}} {{m["a b"]}} [{{items.5.title}}]
{{items[5].title}} {{$ ns-fixture/fail}}
```

| Mode | Line 1 | Line 2 | Warnings in the log |
|---|---|---|---|
| `strict` | — | — | none; the command fails at `2:8`, the out-of-range `[5]` |
| `expand` | `first second second 7 []` | `[template error at 2:8: index 5 out of range for a list of 2] [template error at 2:20: …]` | 2 |
| `expand_html` | as above | each error as `<span class="liquers-template-error" title="2:8">…</span>` | 2 |
| `expand_md` | as above | each error in a code span | 2 |
| `silent` | `first second second 7 []` | ` ` (both tags empty) | 2 |

`[{{items.5.title}}]` renders `[]` in every mode: a dotted index follows Mustache, so a missing
item is empty, not an error. A template with an unclosed `{{#items}}` fails at parse time in all
five modes, and `{{a..b}}` becomes an `Invalid` node: strict mode fails on it before
`ns-fixture/fail` runs, which the fixture's call counter proves.

## Edge and Error Cases

| Symptom | Expected | Test |
|---|---|---|
| A query tag contains `{{…}}` | One tag; the query ends at the first unbalanced `}}` | `nested_braces_make_one_tag` |
| A malformed hygienic query | Error inside the query, in template coordinates; nothing runs | `malformed_query_error_points_into_query` |
| `{{/}}` after `{{#$ q}}`, `{{/x}}` after `{{#y}}` | The first closes; the second is a mismatch reported at both tags | `bare_close_closes_any_section` |
| A value invalid in P3–P5 (`/` in a key segment, `.` in an action name, `..` as a segment) | Positioned error naming the position kind | `hygienic_value_must_fit_its_position` |
| A partial that includes itself | A cycle error, not a hang | `partial_cycle_is_error` |
| An async-only node given to a sync consumer | `not_supported`, never `block_on` | `sync_consumer_refuses_async_node` |
| A frame or record source as data | Rows built one at a time, no whole-value conversion | `frame_rows_as_data`, `record_source_streams_rows` |
| The three facts Phase 2 rated medium | A linked `.md` arrives as text; the asset's filename is known when `render` runs; dynamically found dependencies keep the result's volatility | `linked_template_arrives_as_text`, `media_type_follows_filename`, `volatile_query_makes_result_volatile` |
| Encoding | `parse(encode(t)) == t`, `Invalid` nodes included | `encode_round_trips` |

## Test Plan

`liquers-template` (`cargo test -p liquers-template`):
- `tests/spec.rs` — `spec_interpolation`, `spec_sections`, `spec_inverted`, `spec_comments`,
  `spec_partials`, `spec_delimiters` (AC-1): each vendored spec file passes on `MapHost`.
- `tests/parity.rs` — `parity_core_parser_corpus`, `parity_specs_and_registry_corpus`,
  `parity_generated_queries`, `parity_rejects_at_same_position` (AC-14): the two Phase 2 equalities,
  positions compared separately; generated queries come from a deterministic grammar walk.
- `src/mustache/encode.rs` — `encode_round_trips` (AC-1, AC-18): the spec templates and Example 3.
- `src/mustache/parse.rs` — `nested_braces_make_one_tag`, `malformed_query_error_points_into_query`,
  `unclosed_section_is_irrecoverable` (AC-4); `bare_close_closes_any_section` (AC-3);
  `invalid_tag_becomes_invalid_node` (AC-18).
- `src/query/instantiate.rs` — `hygienic_parameter_is_one_escaped_token`,
  `hygienic_fragment_in_parameter`, `hygienic_slot_inside_link`,
  `hygienic_value_must_fit_its_position` (AC-5); `classic_expands_text_then_parses`,
  `classic_parse_error_is_positioned_in_expansion` (AC-6);
  `non_scalar_last_action_becomes_override`, `non_scalar_earlier_action_is_error` (AC-16).
- `src/mustache/name.rs` — `dotted_index_selects_item`, `dotted_missing_is_empty`,
  `bracket_lookup_by_kind`, `bracket_missing_is_error`, `bracket_negative_index`,
  `bracket_in_query_slot` (AC-17).
- `tests/render.rs` (`RecordingHost`) — `example_2_requests` (AC-5, AC-6, AC-7, AC-16);
  `section_over_list_map_bool`, `inverted_on_empty_result` (AC-3); `evaluate_has_no_input` (AC-8);
  `partial_cycle_is_error` (AC-12); `example_3_modes`, `strict_rejects_invalid_before_any_query`,
  `irrecoverable_in_every_mode` (AC-18).
- `src/diagnostics.rs` — `validate_reports_all_without_running` (AC-9): every `Invalid` node, an
  unknown command and an unused close, with `MapHost` asserting no call.
- `tests/node.rs` — the parser and renderer tests under `wasm-bindgen-test` (AC-13), run with
  `cargo test -p liquers-template --target wasm32-unknown-unknown`; the build rows are
  `scripts/check-build-matrix.sh`'s `liquers-template` wasm32 and `liquers-lib --features templates`
  wasm32 lines.

`liquers-core` (`cargo test -p liquers-core --lib --tests`):
- `src/data_node.rs` — `value_as_data_node`, `metadata_record_as_data_node`,
  `json_as_data_node`, `sync_consumer_refuses_async_node` (AC-15).
- `tests/apply_ad_hoc.rs` — `apply_ad_hoc_propagates_dependencies` (AC-7, AC-16),
  `apply_ad_hoc_rejects_relative_query` (AC-7).

`liquers-records` (`cargo test -p liquers-records --all-features --lib --tests`):
- `record_view_rows_as_maps`, `record_view_index_builds_one_row`, `record_source_streams_rows` (AC-15).

`liquers-lib` (`cargo test -p liquers-lib --test template_commands`):
- `context_pack_renders_open_issues`, `context_pack_recomputes_when_issue_changes` (AC-2, AC-3,
  AC-5, AC-8, AC-11): Example 1.
- `linked_template_arrives_as_text`, `media_type_follows_filename`,
  `volatile_query_makes_result_volatile` (AC-2, AC-11).
- `partial_from_store_is_dependency`, `relative_partial_uses_cwd_link` (AC-11, AC-12): under
  `-R-cwd/templates/-/…`, `{{> ./header.md}}` reads `templates/header.md`.
- `override_list_routes_through_apply` (AC-16); `error_modes_log_warnings` (AC-18), reading the
  log with `LogEntryKind::Warning`; `validate_command_runs_nothing` (AC-9, AC-11).
- `frame_rows_as_data` (AC-15, also gated on `polars`), `folder_as_data` (AC-15).
- `committed_registry_is_fresh` in `tests/registry_export.rs` gains the `templates` group (AC-11).

## Learning Log

- Validating a proposed command needs a `--registry-file` overlay with `default_namespaces` and
  `global_enums` present, not just `commands`; `--command` declares a variadic stand-in, so it
  cannot check arity.
- A default link cannot be written as text in an overlay; filed as
  `REGISTRY-QUERY-DEFAULT-NOT-WRITABLE-AS-TEXT`.
- To confirm in the first test: `search` matches `area:core/store` against a list-valued
  front-matter field (`area: [core/store]`), as the Phase 1 example assumes.
