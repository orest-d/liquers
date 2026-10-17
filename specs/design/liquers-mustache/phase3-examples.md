# Phase 3: Examples & Use-cases — liquers-template

Every test below is runnable. Tests that need no environment live in `liquers-template` and run on
`MapHost` (no queries) or `RecordingHost`, a test host in `liquers-template/tests/common/` that
records each `QueryRequest`, answers from a map, and holds a small `CommandMetadataRegistry` in
which `cmd` has one argument, `arg`. Tests that evaluate queries live in
`liquers-lib/tests/template_commands.rs` (`#![cfg(feature = "templates")]`; the tests that use
`ns-search` add `#[cfg(feature = "records")]`). It builds its own environment: the four issue files
of `liquers-lib/tests/search_commands.rs` copied as constants, the templates, and the commands
`fixture/ctx`, `fixture/fail` and `fixture/count`, written through `AssetManager::set_binary` so a
rewrite is seen. `search_commands.rs` is not changed.

**Validation.** Every placeholder-free query here, and the Example 1 query instantiated with
`area = core/store`, passes `liquers-validate` against the committed registry plus an overlay
declaring `template/render`, `template/expand`, `template/validate`, `fixture/ctx` and the alias
`pack/open_issues`, with `--command fixture/fail`.
The overlay writes `cwd`'s default as `!Value ''`, because a link default cannot be written as text
(`REGISTRY-QUERY-DEFAULT-NOT-WRITABLE-AS-TEXT`, filed). Tag bodies with `{{…}}` inside cannot be
validated by the CLI; the parser tests cover them.

## Overview Table

| # | Kind | Name | Shows / checks | Scenarios |
|---|---|---|---|---|
| 1 | Example | Context pack of open issues | The problem example end to end through `template/render` | AC-2, AC-3, AC-5, AC-8, AC-11 |
| 2 | Example | What each tag form asks the host | Hygienic, classic, apply, override, dynamic partial | AC-5, AC-6, AC-7, AC-12, AC-16 |
| 3 | Example | Indexing and error modes | `items.0`, `x[y]`, a failing tag and a failing section under each mode | AC-17, AC-18 |
| 4 | Example | A template command | `pack/open_issues`, an alias of `render` with the template as its head | AC-19 |
| 5 | Tests | Conformance and parity | Mustache spec suite, core parser parity | AC-1, AC-14 |
| 6 | Tests | Diagnostics, data, partials, portability | Parse errors, `DataNode` impls, partials, `validate`, wasm32 | AC-4, AC-9, AC-12, AC-13, AC-15 |

## Example 1: context pack of open issues

The fixture holds `specs/issues/{A-EXPIRY,B-CLOSED,C-STORE,D-TOML}.md`; only `C-STORE` is in area
`core/store` and not closed (`search` matches a list-valued `area`, as
`search_commands.rs::field_extract_prefix_and_flatten` shows). The template is adapted from Phase 1's problem
example (bare `{{/}}` closes and a `{{#priority}}` guard), stored at `templates/open_issues.md`:

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
2. The planner evaluates the `template` link, a dependency of the result; the text arrives as the
   `template` argument.
3. The hygienic tag fills `{{area}}` as a fragment of one parameter (P1). The instantiated query
   encodes as `…/search-area~ncolon~core~/store~.~_status~ncolon~closed~nast~-f-f-10`: still one
   parameter, so the `/` in the value cannot start a new segment.
4. No apply marker and no override, so `ContextHost` calls `get_dependency_state` (AC-8): the query
   is cached and recorded as a dependency of the rendered asset.
5. The records iterate; each row is a map of columns, so `{{title}}` and `{{key}}` resolve.

```rust
set(&env, "templates/open_issues.md", OPEN_ISSUES).await?;
let state = eval(&env, "ns-fixture/ctx-core~/store/ns-template/render-~X~-R/templates/open_issues.md~E--strict").await?;
assert_eq!(state.try_into_string()?, "# Open issues in core/store\n- [Store listing](specs/issues/C-STORE.md)\n");
```

The four section lines are standalone and drop with their newlines; `{{#priority}}` is inline and
renders nothing, since `C-STORE` has no priority. The same text renders when the data comes as the
`data` argument instead of the state:
`ns-template/render-~X~-R/templates/open_issues.md~E-~X~ns-fixture/ctx-core~/store~E-strict`. With
both, `data` is the top frame: a state `{"area": "core/assets", "title": "T"}` and the `data`
above give `area = core/store` from `data` and `title = T` from the state. Given the template as
the state, `expand` renders it: `-R/templates/open_issues.md/-/ns-template/expand-~X~ns-fixture/ctx-core~/store~E`.

Recomputation is shown on a plain key, not on the search: `ns-search` records no dependency on the
files it reads, so its result would stay stale (`SEARCH-COMMANDS-RECORD-NO-DEPENDENCY-ON-READ-FILES`,
filed). `query_dependency_recomputes` renders `{{$ -R/data/count.txt}}`, rewrites `data/count.txt`
through the asset manager, and expects the new text on the next evaluation (AC-2).

## Example 2: what each tag form asks the host

On `RecordingHost` with data `{"x": "a-b", "tags": ["p", "q"], "row": {"n": 1}}`, where the host
answers `to_text` with `"ok"`. Only the recorded `QueryRequest` is shown.

| Tag | `query` recorded | `overrides` | `input` |
|---|---|---|---|
| `{{$ cmd-{{x}}/q}}` | `cmd-a~_b/q` (one parameter) | none | none |
| `{{$$ cmd-{{x}}/q}}` | `cmd-a-b/q` (two parameters) | none | none |
| `{{>$ to_text}}` | `to_text`; `"ok"` is rendered as a template | none | none |
| `{{$. to_text}}` inside `{{#row}}` | `to_text` | none | `{"n": 1}` |
| `{{$.row to_text}}`, `{{$$.row to_text}}` | `to_text` | none | `{"n": 1}` |
| `{{#$.row to_text}}…{{/}}`, `{{>$.row to_text}}` | `to_text` | none | `{"n": 1}` |
| `{{$ cmd-{{tags}}}}` | `cmd` | `[("arg", ["p", "q"])]` | none |
| `{{$ cmd-{{tags}}/q}}` | — | error at the slot: overrides reach only the last action | — |

The first three rows take `ContextHost`'s `get_dependency_state` route; every row with an input or
an override takes `apply_ad_hoc`. Naming the override needs the registry, so `TemplateHost` exposes
one (`command_registry()`, a Phase 2 correction found in this review).

## Example 3: indexing and error modes

Data `{"items": [{"title": "first"}, {"title": "second"}], "i": 1, "m": {"a b": 7}}`;
`ns-fixture/fail` always returns an error:

```
{{items.0.title}} {{items[i].title}} {{items[-1].title}} {{m["a b"]}} [{{items.5.title}}]
{{items[5].title}} {{$ ns-fixture/fail}}
{{#$ ns-fixture/fail}}never{{/}}
```

| Mode | Line 1 | Line 2 | Line 3 | Warnings |
|---|---|---|---|---|
| `strict` | — | — | — | none; the command fails at `2:8`, the out-of-range `[5]` |
| `expand` | `first second second 7 []` | `[template error at 2:8: index 5 out of range for a list of 2] [template error at 2:20: …]` | `[template error at 3:1: …]`, once | 3 |
| `expand_html` | as above | each error as `<span class="liquers-template-error" title="2:8">…</span>` | one span | 3 |
| `expand_md` | as above | each error in a code span | one code span | 3 |
| `silent` | `first second second 7 []` | ` ` (both tags empty) | empty | 3 |

In every mode but strict, `[{{items.5.title}}]` renders `[]`: a dotted index follows Mustache, so a
missing item is empty, not an error. The failing section head makes the whole section the unit:
its body never renders. An unclosed `{{#items}}` fails at parse time in all five modes. `{{a..b}}`
becomes an `Invalid` node, and strict mode fails on it before `ns-fixture/fail` runs, which the
fixture's call counter proves. An `errors` value outside the five, such as `warp`
(`ns-template/render-~X~-R/templates/hello.md~E--warp`), fails before rendering.

## Example 4: a template command

The context pack becomes a command with no Rust function: an alias of `render` whose head is the
template link (Phase 2, Relevant Commands).

```rust
cr.register_alias(
    CommandKey::new("", "pack", "open_issues"),
    CommandKey::new("", "template", "render"),
    vec![CommandParameterValue::Query(parse_query("-R/templates/open_issues.md")?)],
    vec![ArgumentInfo::any_argument("data").with_default(""),
         ArgumentInfo::string_argument("errors").with_default("strict"),
         ArgumentInfo::any_argument("cwd").with_default_link(parse_query("-R-key/.")?)],
)?.with_label("Open issues");
let state = eval(&env, "ns-fixture/ctx-core~/store/ns-pack/open_issues").await?;
assert_eq!(state.try_into_string()?, "# Open issues in core/store\n- [Store listing](specs/issues/C-STORE.md)\n");
```

The result equals Example 1's. `make_plan` shows one `render` action with
`origin: Alias { command: pack/open_issues }`, and its dependencies include the template's key and
the alias's metadata key. Rewriting `templates/open_issues.md` (the heading becomes `## Open`)
makes the next evaluation return the new heading, so the template is a real dependency.

## Edge and Error Cases

The cases Phase 2's risks and error table name:

| Symptom | Expected | Test |
|---|---|---|
| A partial that includes itself, or dynamic partials nested past 32 | A cycle or depth error, not a hang (preflight `INLINE-MANAGER-DYNAMIC-DEPENDENCY-CHAIN-NESTS`) | `partial_cycle_is_error`, `partial_depth_capped` |
| An async-only node given to a sync consumer | `not_supported`, never `block_on` | `sync_consumer_refuses_async_node` |
| A frame or record source as data (performance risk) | Rows built one at a time, no whole-value conversion | `frame_rows_as_data`, `record_source_streams_rows` |
| The facts Phase 2 rated medium | A linked `.md` arrives as text; the filename is known when `render` runs; the template key is visible in the state's metadata; dynamically found dependencies keep the result's volatility | `linked_template_arrives_as_text`, `media_type_follows_filename`, `template_key_in_state_metadata`, `volatile_query_makes_result_volatile` |
| `cwd` link does not yield a key | Conversion error before the command runs | `cwd_not_a_key_fails` |
| `data` is a list or a scalar | Error before rendering, naming `data` | `data_must_be_a_map` |
| An alias head that is a link (no alias used one before) | Planned as a `DefaultLink` and evaluated as a dependency | `alias_head_link_is_evaluated` |
| Encoding (AST principle) | `parse(encode(t)) == t`, `Invalid` nodes included | `encode_round_trips` |

## Test Plan

`liquers-template` (`cargo test -p liquers-template`; the crate joins `default-members`):
- `tests/spec.rs` — `spec_interpolation`, `spec_sections`, `spec_inverted`, `spec_comments`,
  `spec_partials`, `spec_delimiters` (AC-1): each vendored spec file passes on `MapHost`.
- `tests/parity.rs` — `parity_core_parser_corpus`, `parity_specs_and_registry_corpus`,
  `parity_generated_queries`, `parity_rejects_at_same_position` (AC-14): the two Phase 2 equalities,
  positions compared separately; generated queries come from a deterministic grammar walk.
- `src/mustache/encode.rs` — `encode_round_trips` (Phase 2 §AST): the spec templates and Example 3.
- `src/mustache/parse.rs` — `nested_braces_make_one_tag`, `malformed_query_error_points_into_query`,
  `unclosed_section_is_irrecoverable`, `bad_set_delimiter_is_irrecoverable`,
  `bare_close_closes_any_section` (`{{/x}}` after `{{#y}}` is reported at both tags) (AC-4);
  `invalid_tag_becomes_invalid_node` (AC-18).
- `src/query/instantiate.rs` — `hygienic_parameter_is_one_escaped_token`,
  `hygienic_fragment_in_parameter`, `hygienic_slot_inside_link`,
  `hygienic_value_must_fit_its_position` (AC-5); `classic_expands_text_then_parses`,
  `classic_parse_error_is_positioned_in_expansion` (AC-6);
  `non_scalar_last_action_becomes_override`, `non_scalar_earlier_action_is_error` (AC-16).
- `src/name.rs` — `dotted_index_selects_item`, `dotted_missing_is_empty`,
  `bracket_lookup_by_kind`, `bracket_missing_is_error`, `bracket_negative_index`,
  `bracket_in_query_slot` (AC-17).
- `tests/render.rs` (`RecordingHost`) — `example_2_requests` (AC-5, AC-6, AC-7, AC-12, AC-16);
  `section_over_list_map_bool`, `inverted_on_empty_result`, `bare_close_ends_query_section` (AC-3);
  `evaluate_has_no_input`, `names_frame_is_not_dot` (AC-8, AC-11); `partial_cycle_is_error`, `partial_depth_capped` (AC-12);
  `example_3_modes`, `failing_section_head_is_one_unit`, `strict_rejects_invalid_before_any_query`,
  `irrecoverable_in_every_mode` (AC-18).
- `src/diagnostics.rs` — `validate_reports_all` (AC-9): every `Invalid` node, an unknown command and
  an unused close; `validate_template` takes no host, so nothing can run.
- `tests/node.rs` (`#![cfg(target_arch = "wasm32")]`) — `#[wasm_bindgen_test]` copies of a parse, an
  instantiation and a render on `MapHost` (AC-13), because plain `#[test]`s do not run under the
  runner. `wasm-bindgen-test` is a `[target.'cfg(target_arch = "wasm32")'.dev-dependencies]` entry;
  `spec.rs`, `parity.rs` and `render.rs` carry `#![cfg(not(target_arch = "wasm32"))]`, since they read
  files and use tokio. Run: `cargo test -p liquers-template --target wasm32-unknown-unknown` (Node,
  `wasm-bindgen-test-runner` from `.cargo/config.toml`). Build rows: `scripts/check-build-matrix.sh`
  gains `liquers-template` wasm32 and `liquers-lib --features templates` wasm32.

`liquers-core` (`cargo test -p liquers-core --lib --tests`):
- `src/data_node.rs` — `value_as_data_node`, `metadata_record_as_data_node`,
  `json_as_data_node`, `sync_consumer_refuses_async_node` (AC-15).
- `tests/apply_ad_hoc.rs` — `apply_ad_hoc_propagates_dependencies` (AC-7, AC-16),
  `apply_ad_hoc_rejects_relative_query`, `apply_ad_hoc_volatile_makes_parent_volatile` (AC-7).
- `tests/command_alias.rs` — `alias_head_link_is_evaluated` (AC-19): an alias whose head is a
  `CommandParameterValue::Query` plans a `DefaultLink`, evaluates it, and records its dependency;
  `src/command_metadata.rs` — `with_default_link_sets_query_default` (AC-19).

`liquers-records` (`cargo test -p liquers-records --all-features --lib --tests`):
- `record_view_rows_as_maps`, `record_view_index_builds_one_row`, `record_source_streams_rows` (AC-15).

`liquers-lib` (`cargo test -p liquers-lib --test template_commands`):
- `context_pack_renders_open_issues`, `data_argument_supplies_names`, `data_shadows_state_fields`,
  `data_must_be_a_map` (AC-3, AC-5, AC-8, AC-11): Example 1, from the state, from `data`, and both.
- `template_command_alias_renders`, `template_command_invalidates_on_template_edit` (AC-19):
  Example 4.
- `query_dependency_recomputes`, `linked_template_arrives_as_text`, `media_type_follows_filename`,
  `template_key_in_state_metadata`, `volatile_query_makes_result_volatile`,
  `expand_renders_state_as_template` (`-R/templates/hello.md/-/ns-template/expand`) (AC-2, AC-11).
- `partial_from_store_is_dependency`, `dynamic_partial_is_dependency`,
  `relative_partial_uses_cwd_link` (AC-11, AC-12): under `-R-cwd/templates/-/…`, `{{> ./header.md}}`
  reads `templates/header.md`.
- `dot_is_state_in_render_and_data_in_expand` (AC-11); `apply_tag_routes_through_apply_ad_hoc`
  (`{{$. q}}`, `{{$$. q}}` and `{{>$$ q}}` through `ContextHost`) (AC-7, AC-12);
  `override_list_routes_through_apply` (AC-16); `error_modes_log_warnings` (AC-18), reading the log
  with `LogEntryKind::Warning`; `unknown_error_mode_fails`, `cwd_not_a_key_fails` (AC-11, AC-18);
  `validate_command_runs_nothing` (AC-9, AC-11), with `fixture/count` proving no command ran.
- `frame_rows_as_data` (AC-15, also gated on `polars`), `folder_as_data` (AC-15).
- `tests/registry_export.rs` (AC-11): every `cfg(all(egui, image-support, polars, records))` gains
  `feature = "templates"`, `full_registry` gains a `#[cfg(feature = "templates")]` arm,
  `exported_registry_is_nonempty` an anchor command, and `CLAUDE.md`'s "gated on all four groups"
  becomes five.

## Learning Log

- Validating a proposed command needs a `--registry-file` overlay with `default_namespaces` and
  `global_enums` present, not just `commands`; `--command` declares a variadic stand-in, so it
  cannot check arity.
- A default link cannot be written as text in an overlay (`REGISTRY-QUERY-DEFAULT-NOT-WRITABLE-AS-TEXT`).
- `ns-search` results are not invalidated by file edits
  (`SEARCH-COMMANDS-RECORD-NO-DEPENDENCY-ON-READ-FILES`), so a context pack built on a search goes
  stale until that is fixed. Agent-memory context packs depend on it.
- `TemplateHost` needed the command registry to name overrides; Phase 2 was corrected.
