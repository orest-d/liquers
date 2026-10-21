# Phase 3: Examples & Use-cases — query-editor-live-validation

## Overview Table

| # | Kind | Name | Shows / checks | Scenarios |
|---|---|---|---|---|
| 1 | Example | typing a bad parameter in the egui console | live planning diagnostic | AC-1 |
| 2 | Example | browser console after a submit | classed spans and error title | AC-7 |
| 3 | Test | `liquers-core/tests/syntax_tokens.rs::tokens_tile_core_parse_corpus` | concatenation = source over the corpus | AC-3 |
| 4 | Test | `liquers-core` `query.rs` `syntax_tokens_kinds_for_resource_and_link` | kinds, `-R/`, `~X~…~E` | AC-3 |
| 5 | Test | `liquers-core` `query.rs` `syntax_tokens_unparseable_is_one_plain_token` | fallback | AC-4 |
| 6 | Test | `liquers-lib` `ui/query_editor.rs` `analysis_reports_conversion_error_at_parameter` | 11..12 range | AC-1 |
| 7 | Test | `ui/query_editor.rs` `analysis_reports_unregistered_action` | range of `query_c` | AC-2 |
| 8 | Test | `ui/query_editor.rs` `analysis_without_registry_is_parse_level` | no plan diagnostic | AC-5 |
| 9 | Test | `ui/query_editor.rs` `analysis_parse_error_spans_to_end` | `hello/` | AC-4 |
| 10 | Test | `ui/query_editor.rs` `model_reuses_analysis_for_same_text` | cache | AC-8 |
| 11 | Test | `ui/query_editor.rs` `execution_error_kept_until_text_changes` | AC-6 rule | AC-6 |
| 12 | Test | `ui/widgets/query_console_element.rs` `console_update_records_execution_error_in_editor` | replaces the old position test | AC-6 |
| 13 | Test | `egui/widgets.rs` `analysis_layout_text_equals_buffer` | galley text = buffer for `ns-pl/-/head` | AC-3, AC-1 |
| 14 | Test | `ui/runner.rs` tests or `tests/ui_runner.rs::runner_contexts_carry_registry` | registry reaches `init`/`update` | AC-1 |
| 15 | Test | `tests/webui_ssr.rs::console_renders_token_spans_and_error` | SSR markup | AC-7 |
| 16 | Test | `ui/web/widgets.rs` `query_to_html_is_token_classed` | `query_to_html` | AC-7 |
| 17 | Browser | `examples-web/tests/ui_query_console_app.spec.ts` "preview shows highlighted query" | real DOM | AC-7 |

## Example 1: typing a bad parameter (egui)

`ui_query_console_app` attaches the registry: `UIContext::new(app_state, tx).with_command_registry(runner.command_registry())`.
The user types `ns-pl/head-x` (validated: `ConversionError`, "Can't convert 'x' to integer", byte
11). Each frame the layouter calls `model.analysis("ns-pl/head-x", Some(reg))`: the first call
builds the plan, later frames reuse it. The galley's text is the buffer; `x` is red and underlined,
hovering shows the message. No `AppMessage` is sent until Enter.

```rust
let reg = registry(); // CommandMetadataRegistry with the ns-pl group, or from liquers-validate's file
let a = analyse_query("ns-pl/head-x", Some(&reg));
assert_eq!((a.diagnostics[0].start, a.diagnostics[0].end), (11, 12));
assert!(a.tokens.iter().map(|t| &"ns-pl/head-x"[t.start..t.end]).collect::<String>() == "ns-pl/head-x");
```

## Example 2: browser console

After submitting `hello/q/ns-lui/query_c` (validated: `ActionNotRegistered` at byte 15), the
SSR/browser markup holds
`<div class="lq-qc-preview">…<span class="lq-tok-ActionName lq-tok-error" title="Action 'query_c' not registered…">query_c</span></div>`.

## Edge and Error Cases

| Case | Expected | Test |
|---|---|---|
| Empty text | no tokens, no diagnostics | 9 (`""` row) |
| `hello/` incomplete | one `Plain` token, parse diagnostic offset..end | 9 |
| Non-ASCII parameter (`text-ä`) | ranges on char boundaries | 3 (corpus) + 4 (`ä` row) |
| Link parameter `a-~X~b~E` | `Entity`, inner `ActionName`, `Entity` | 4 |
| Normalising parse (`ns-pl/-/head`) | tokens still the source text | 4, 13 |
| No registry in context | parse-level only | 8 |
| Text changes after an execution error | execution error dropped | 11 |
| HTML injection (`<b>` in a parameter) | escaped in every span | 16 |

## Test Plan

- Unit (core): `liquers-core/src/query.rs` `mod tests` — tests 4, 5.
- Integration (core): new `liquers-core/tests/syntax_tokens.rs` — test 3: every `parse_query("…")`
  literal in `liquers-core/src` and `liquers-core/tests` that parses tiles exactly (same harvesting
  as `liquers-template/tests/parity.rs`).
- Unit (lib): `liquers-lib/src/ui/query_editor.rs` — tests 6-11, registry built with
  `ValidationRegistryBuilder` from `specs/command_registry.yaml` text (`include_str!`, read at
  test time, as `registry_export` does) or from a test environment's registry.
- Unit (lib): test 12 in the console; test 13 in `egui/widgets.rs` (`cfg(feature = "egui")`);
  test 16 in `ui/web/widgets.rs` (`cfg(feature = "webui")`).
- Integration (lib): test 14 in `tests/ui_runner.rs`; test 15 in `tests/webui_ssr.rs`
  (`#![cfg(feature = "webui")]` already).
- Browser (designed, not runnable here): test 17, after `cargo clean`, per `CLAUDE.md`.
- Commands: `cargo test -p liquers-core --lib syntax_tokens`, `cargo test -p liquers-core --test syntax_tokens`,
  `cargo test -p liquers-lib --lib query_editor`, `cargo test -p liquers-lib --lib --tests`,
  `cargo test -p liquers-lib --no-default-features --features webui --lib --tests`.

## Learning Log

- `liquers-validate` confirms both error offsets used here (11 and 15) and that `ns-pl/-/head`
  re-encodes with `-R/`, the case that breaks the current layouter.
