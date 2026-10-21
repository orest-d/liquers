# Phase 3: Examples & Use-cases — template-syntax-highlighter

## Overview Table

| # | Kind | Name | Shows / checks | Scenarios |
|---|---|---|---|---|
| 1 | Example | highlighting `templates/page.md` | command output | AC-1 |
| 2 | Test | `liquers-template/tests/highlight.rs::highlight_tiles_spec_suite` | tiling over the vendored Mustache suite | AC-2 |
| 3 | Test | `tests/highlight.rs::highlight_tiles_after_set_delimiter` | `{{=<% %>=}}` then `<% x %>` | AC-2 |
| 4 | Test | `tests/highlight.rs::hygienic_query_tokens_and_slot` | `{{$ cmd-{{x}}/q}}` kinds | AC-3 |
| 5 | Test | `tests/highlight.rs::classic_body_is_slots_and_text` | `{{$$ cmd-{{x}}}}` | AC-4 |
| 6 | Test | `tests/highlight.rs::invalid_tag_is_one_token` | recoverable invalid tag | AC-5 |
| 7 | Test | `tests/highlight.rs::unparseable_template_is_text` | `{{#a}}` unclosed | AC-6 |
| 8 | Test | `liquers-template` `highlight.rs` `html_style_escapes_and_marks_diagnostic` | escaping, `title` | AC-1 |
| 9 | Test | `highlight.rs` `ansi_style_strips_to_source` | strip ANSI = text | AC-7 |
| 10 | Test | `liquers-lib/tests/template_commands.rs::highlight_command_marks_unknown_command` | end to end | AC-1 |
| 11 | Test | `tests/template_commands.rs::highlight_command_rejects_unknown_style` | error | AC-9 |
| 12 | Test | `liquers-lib` `template/egui.rs` `template_layout_text_equals_source` | galley text | AC-8 |

## Example 1: the command

Store: `templates/page.md` holds the template of Phase 1. Query
`-R/templates/page.md/-/ns-template/highlight` (validated with `--command template/highlight`).
Result (abridged):

```html
<span class="lq-tok-Text"># </span><span class="lq-tok-Delimiter">{{</span><span class="lq-tok-Name">title</span><span class="lq-tok-Delimiter">}}</span>…
<span class="lq-tok-Sigil">#$</span> <span class="lq-tok-Header">-R-key</span>…
<span class="lq-tok-ActionName lq-tok-error" title="unknown command 'field_extrct'">field_extrct</span>…
```

`-R/templates/page.md/-/ns-template/highlight-ansi` returns the same text with ANSI colours.

## Example 2: the library

```rust
let text = "Hi {{$ cmd-{{x}}/q}}";
let t = parse_template(text, TemplateSyntax::Mustache)?;
let tokens = highlight(text, &t);
assert_eq!(tokens.iter().map(|k| &text[k.start..k.end]).collect::<String>(), text);
assert!(tokens.iter().any(|k| k.kind == TemplateSyntaxKind::Slot && &text[k.start..k.end] == "{{x}}"));
```

## Edge and Error Cases

| Case | Expected | Test |
|---|---|---|
| Standalone section lines (indent + newline) | covered as `Text` around the tag | 2 |
| Unicode text and names | ranges on char boundaries | 2 (spec suite has Unicode) |
| Partial `{{> key}}` and `{{>$ q}}` | `Sigil`, `Query(..)` / `Name` | 2, 4 |
| Comment `{{! … }}` | one `Comment` token between delimiters | 2 |
| Apply `{{$.name q}}` | `Sigil`, `Name`, query tokens | 4 (row) |
| Link inside a hygienic query | `Query(Entity)` around inner tokens | 4 (row) |
| Template text containing `<script>` | escaped in HTML | 8 |

## Test Plan

- Integration: new `liquers-template/tests/highlight.rs` — tests 2-7 (reads `tests/spec/*.json`
  as `tests/spec.rs` does; `#![cfg(not(target_arch = "wasm32"))]` for file access, like
  `parity.rs`).
- Unit: `liquers-template/src/highlight.rs` — tests 8, 9.
- Integration (lib): `liquers-lib/tests/template_commands.rs` — tests 10, 11 (its existing harness
  with an `AsyncMemoryStore` holding `templates/page.md`).
- Unit (lib, `egui` + `templates`): test 12.
- Commands: `cargo test -p liquers-template --lib --tests`,
  `cargo test -p liquers-lib --test template_commands`, `cargo test -p liquers-lib --lib template`,
  `cargo test -p liquers-lib --test registry_export`.

## Learning Log

- `field_extract` takes an `extension` enum (`md`, `json`, `yaml`, `yml`); an example with a wrong
  value fails as a conversion error, so the example uses `md`.
