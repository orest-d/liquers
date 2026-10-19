---
title: Template Guide
kind: guide
audience: internal
area: [core/query, lib/commands]
reviewed: 2026-10-17
---
# Template Guide

How to write Liquers templates, render them, and turn one into a command. The language itself is
[`reference/TEMPLATE_LANGUAGE.md`](../reference/TEMPLATE_LANGUAGE.md).

## 1. Writing a template

A template is Mustache text. Plain tags read its data; query tags evaluate queries. A context pack
of the open issues of an area, kept at `templates/open_issues.md`:

```markdown
# Open issues in {{area}}
{{#$ -R-key/specs/issues/-/ns-search/field_extract/search-area~ncolon~{{area}}~.~_status~ncolon~closed~nast~-f-f-10}}
- [{{title}}]({{key}}){{#priority}} — {{priority}}{{/priority}}
{{/}}
{{^$ -R-key/specs/issues/-/ns-search/field_extract/search-area~ncolon~{{area}}~.~_status~ncolon~closed~nast~-f-f-10}}
No open issues.
{{/}}
```

- `{{area}}` inside `{{#$ …}}` is a **hygienic** slot: with `area = core/store` the query encodes
  as `…search-area~ncolon~core~/store~.~_status…`, still one parameter, so a value can never add a
  segment or an action. Prefer `{{$ …}}`; use `{{$$ …}}` only when a value is meant to be query
  text, and never with untrusted data.
- The section iterates the records the search returns; each row is a map, so `{{title}}` and
  `{{key}}` are its fields.
- `{{/}}` closes any section.
- Indexing: `{{items.0.title}}` is Mustache (missing renders empty); `{{items[i].title}}`,
  `{{items[-1]}}` and `{{m["a b"]}}` are strict and fail when the element is absent.

Validate before committing: `-R/templates/open_issues.md/-/ns-template/validate` lists every
invalid tag and unknown command, positioned, and runs nothing. A tag body with `{{…}}` inside
cannot go through `liquers-validate` on its own; validate the instantiated query instead.

## 2. Rendering: `render` or `expand`

The examples use `ns-fixture/ctx-<area>`, the test fixture of `liquers-lib/tests/template_commands.rs`
that returns `{"area": <area>}`; any query producing a map takes its place.

| You have | Use |
|---|---|
| The template in the store, the data produced by the query | `ns-fixture/ctx-core~/store/ns-template/render-~X~-R/templates/open_issues.md~E` |
| The data as a value of its own | `ns-template/render-~X~-R/templates/open_issues.md~E-~X~ns-fixture/ctx-core~/store~E` |
| The template computed or read by the query | `-R/templates/open_issues.md/-/ns-template/expand-~X~ns-fixture/ctx-core~/store~E` |

**How the state and `data` combine.** In `render` the state is the bottom frame and `data` a frame
of names above it: a field of `data` shadows the same field of the state, every other field of the
state stays visible, and `{{.}}` and `{{$. q}}` are the state. In `expand` `data` is the only frame.
`data` must be a map (or empty). A section pushes its value above both, so inside `{{#.}}` the
state's fields win again.

Partials (`{{> templates/header.md}}`, `{{>$ q}}`) are read through the store and recorded as
dependencies, so editing one invalidates every render that used it. A relative partial
(`{{> ./header.md}}`) resolves against `cwd`: set the working directory
(`-R-cwd/templates/-/ns-template/render-~X~-R/./page.md~E`) or pass it as the last argument.

Choose the error mode with `errors`: `strict` while developing, `expand_md` or `expand_html` to see
every failure in the output at once, `silent` for best-effort output. Every recovered error is also
a warning in the result's log.

**Volatility.** A render is cached like any other query, unless one of the template's queries is
volatile: then the render is volatile too and recomputes on every request
(`design/runtime-volatility`). You do not need `v/` or a `volatile` declaration for that.

## 3. Template commands

A command whose behaviour is a template needs no Rust function: register an **alias** of
`template/render` whose head parameter is the template link (registration in
[`COMMAND_REGISTRATION_GUIDE.md`](COMMAND_REGISTRATION_GUIDE.md) §*Registering an alias*):

```rust
cr.register_alias(
    CommandKey::new("", "pack", "open_issues"),
    CommandKey::new("", "template", "render"),
    vec![CommandParameterValue::Query(parse_query("-R/templates/open_issues.md")?)], // `template`
    vec![
        ArgumentInfo::any_argument("data").with_default(""),
        ArgumentInfo::string_argument("errors").with_default("strict"),
        ArgumentInfo::any_argument("cwd").with_default_link(parse_query("-R-key/.")?),
    ],
)?
.with_label("Open issues");
```

`ns-fixture/ctx-core~/store/ns-pack/open_issues` then renders the pack. Because the head is a link,
the template is a dependency of every result, and rewriting it invalidates them; the alias's own
metadata is a dependency too. The arguments after the head line up with `render`'s. `cwd` is
`any_argument`, because a `Key` argument is `ArgumentType::Any`; `render` converts it. Tests:
`template_command_alias_renders` and `template_command_invalidates_on_template_edit` in
`liquers-lib/tests/template_commands.rs`.

Write a template command rather than a Rust one when the result is text assembled from other
queries; write Rust when it computes.

## 4. Rendering from Rust

```rust
use liquers_template::{parse_template, render, ContextStack, FrameKind, MapHost, RenderOptions, TemplateSyntax};
use liquers_core::data_node::AnyNode;

let template = parse_template("Hello {{name}}", TemplateSyntax::Mustache)?;
let mut stack = ContextStack::new();
stack.push(AnyNode::from_json(serde_json::json!({"name": "world"})), FrameKind::Data);
let text = render(&template, stack, &MapHost::new(), &RenderOptions::default()).await?;
```

`MapHost` holds partials and refuses queries. Inside a command use `template::ContextHost::new(context,
cwd)`, which evaluates queries through the command's `Context` (as `render` does). Another host
implements `TemplateHost`: `evaluate`, `partial`, `command_registry` (to name overrides) and `warn`.

## 5. Making a value readable by templates

Override `ValueInterface::data_node` (or, for a liquers-lib extension, `ValueExtension::data_node`)
to return a node that reads in place: `AnyNode::Sync` with a `DataNode` when the data is in memory,
`AnyNode::Async` with an `AsyncDataNode` when it is read on demand. Build children on request
(`NodeRef::Owned` for a row), override `index` when an item can be reached directly, and return
the value from `as_any` when a host should pass it on as itself. The default is a JSON snapshot,
or opaque. Examples: `liquers-records/src/data_node.rs`, `liquers-lib/src/polars/data_node.rs`,
`liquers-lib/src/template/folder.rs`.

## 6. Adding a syntax

`TemplateSyntax` and `Template` have one variant per syntax. A new syntax adds a variant to each,
a parser behind `parse_template`, a renderer behind `render`, and an encoder; names, the context
stack, query templates, hosts and error modes are shared.

## History

| Date | Change | Source |
|---|---|---|
| 2026-10-17 | Volatility advice relaxed: a volatile query in a template makes the render volatile by itself. | `design/runtime-volatility` |
| 2026-10-17 | Corrected the volatility advice: `v` goes before the render action; a trailing `/v` serves the cached render. | `RUNTIME-VOLATILE-DEPENDENCY-DOES-NOT-MAKE-PARENT-VOLATILE` |
| 2026-10-17 | Created: writing and validating templates, `render` and `expand`, template commands, rendering from Rust, data nodes, adding a syntax. | `design/liquers-mustache/` |
