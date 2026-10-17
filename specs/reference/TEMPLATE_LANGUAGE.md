---
title: Template Language
kind: reference
audience: internal
area: [core/query, lib/commands]
reviewed: 2026-10-17
---
# Template Language

A Liquers template is Mustache text whose tags can also evaluate Liquers queries. The language is
the `liquers-template` crate (it depends on `liquers-core` only and builds for wasm32); the commands
that render templates over Liquers values are `liquers-lib`'s `ns-template`, behind the `templates`
feature (on by default). How to write templates and template commands is
[`guides/TEMPLATE_GUIDE.md`](../guides/TEMPLATE_GUIDE.md); why the language is shaped this way is
[`design/liquers-mustache/`](../design/liquers-mustache/DESIGN.md).

## 1. Mustache

Every tag of the Mustache specification behaves as the specification says: variables `{{x}}`
(HTML-escaped when escaping is on), raw `{{{x}}}` and `{{&x}}`, sections `{{#x}}…{{/x}}`, inverted
sections `{{^x}}…{{/x}}`, comments `{{! …}}`, partials `{{> key}}` and set-delimiter tags
`{{=<% %>=}}`, with the specification's standalone-line rules. The required modules of the
official suite (interpolation, sections, inverted, comments, partials, delimiters) are vendored in
`liquers-template/tests/spec/` and pass (`tests/spec.rs`, and under Node in `tests/node.rs`).

Two extensions:
- **`{{/}}` closes any section**, whatever its head. A named close must match its section; a
  mismatched one is reported at both tags.
- **Brackets are reserved in names** (§3). The specification defines dotted names only, so no spec
  case is affected.

## 2. Query tags

| Tag | Does |
|---|---|
| `{{$ q}}` | Evaluates `q` (hygienic, below) and writes its value, escaped like `{{x}}` |
| `{{&$ q}}` | The same, never escaped |
| `{{$$ q}}` | Classic: expands the text of `q`, then parses and evaluates it |
| `{{#$ q}}…{{/}}`, `{{^$ q}}…{{/}}` | A section, or an inverted section, over the query's value |
| `{{>$ q}}`, `{{>$$ q}}` | A dynamic partial: the value is rendered as a template |
| `{{$. q}}`, `{{$.name q}}` | Apply: evaluates `q` with the current context (`.`) or `name` as its input; also `{{$$. q}}`, `{{#$. q}}`, `{{>$. q}}` |

Closes may be written `{{/}}`, `{{/$}}`, `{{/$$}}` or with the section's name.

**Hygienic and classic.** In `{{$ q}}` the query is parsed once, with each `{{x}}` inside it as a
slot of the query's AST (`QueryTemplate`), and a value fills its slot without being re-lexed. With
`x = "a-b"`:

```
{{$ cmd-{{x}}/q}}    ->  cmd("a-b")/q    encodes as cmd-a~_b/q   (one parameter)
{{$$ cmd-{{x}}/q}}   ->  text cmd-a-b/q, then parsed             (two parameters)
```

`{{$$ …}}` can change the query's structure and is unsafe with untrusted data. Inside a query tag,
`{{{x}}}` and `{{&x}}` are an error.

**Where a slot may stand** (hygienic). A slot is a whole token (`cmd-{{x}}`) or part of one
(`search-area~ncolon~{{area}}`, `to_{{fmt}}`).

| # | Position | Example | The value |
|---|---|---|---|
| P1 | Action parameter | `cmd-{{x}}` | Any value; escaped by `encode_token` |
| P2 | Namespace | `ns-{{lib}}/…` | An identifier |
| P3 | Action name | `{{op}}-1`, `to_{{fmt}}` | An identifier (`[A-Za-z_][A-Za-z0-9_]*`) |
| P4 | Resource key segment | `-R/data/{{name}}` | A resource name; no `/`, and not `.` or `..` |
| P5 | Output filename | `…/{{name}}.html` | Keeps the filename shape |
| P6 | Inside a link parameter | `cmd-~X~-R/{{key}}~E` | P1–P5 apply inside the link |

A token with a literal `.` outside slots is a filename, any other token an action; the value must
keep that. A slot cannot stand in a segment header (`-{{r}}/…`, `-R-{{p}}`) or for structure (a
whole action, a segment, `/-/`): use `{{$$ …}}` there. A number or boolean is written as its text;
null or a missing name is an error. A **non-scalar** value (a list, a map, records) is allowed only
as a whole P1 slot of the **last action**, where it becomes a parameter override
(`Recipe::arguments`) of the argument the slot stands for, named from the command metadata registry;
a list given to a `multiple` argument supplies one value per element.

A malformed hygienic query is reported in template coordinates. The parser replaces each slot with
a sentinel token that cannot occur in the text and parses with core's `parse_query`, so its grammar
is core's by construction (`tests/parity.rs` checks it against core's own test corpus).

## 3. Names and data

```
name    = "." | head tail*
tail    = "." field | "." index | "[" key "]"
key     = name | integer | string          -- x[y], x[0], x[-1], x["a b"]
```

| Part | List | Map | Scalar, null, opaque |
|---|---|---|---|
| `.field` | missing | the field, or missing | missing |
| `.digits` | the item, or missing | the field named by the digits, or missing | missing |
| `[key]` | the item (negative counts from the end), else **error** | the field, else **error** | **error** |

Dotted names follow Mustache: a missing part renders empty and is falsey. A bracket is a strict
lookup: `y` is resolved as a whole name from the top of the stack, and an absent element is an
error at the part.

**Data is read through `DataNode`** (`liquers-core/src/data_node.rs`), sync, or `AsyncDataNode`
for data read on demand, so a template walks a value without converting it. Every value has one:
`ValueInterface::data_node(Arc<Self>) -> AnyNode`, a JSON snapshot by default.

| Value | Node |
|---|---|
| Core `Value`, liquers-lib `SimpleValue`, `MetadataRecord`, JSON | Sync, in place |
| `RecordView` (`liquers_records::RecordViewNode`) | Sync list of rows; a row is built when asked for |
| Polars `DataFrame` (`liquers_lib::polars::data_node::DataFrameNode`) | Sync list of rows, likewise |
| `RecordSource` (`RecordSourceNode`) | Async; streams through the command's `Context` |
| A directory listing (`-R-dir/…`, `template::folder::FolderNode`) | Async map of children; a child is read, as a dependency, when a template names it |
| Image, UI element, foreign value | Opaque: renders empty |

A sync consumer given an async-only node gets `not_supported`, never a blocking wait.

**The context stack.** Frames are `Data` (looked up, and `.` and the apply input while on top) or
`Names` (looked up only). Sections push their value as a `Data` frame.

## 4. The `ns-template` commands

```
template/render(state, template: String, data: Value = "", errors: String = "strict", cwd: Key = query "-R-key/.")
template/expand(state, data: Value = "", errors: String = "strict", cwd: Key = query "-R-key/.")
template/validate(state)
```

| Command | Template | Bottom frame | Top frame | `.`, `{{$. q}}` |
|---|---|---|---|---|
| `render` | the `template` argument, normally a link (`render-~X~-R/templates/x.md~E`), so it is a dependency | the state (`Data`), unless empty | `data` (`Names`), unless empty | the state |
| `expand` | the state | none | `data` (`Data`) | `data` |
| `validate` | the state | — | — | — |

- `data` is a map of named values or empty; anything else fails before rendering.
- `errors` is `strict`, `expand`, `expand_html`, `expand_md` or `silent` (§6).
- `cwd` is the working directory as a key; relative partial keys and relative keys in query tags
  resolve against it. Its default link yields the query's working directory
  (`-R-cwd/templates/-/ns-template/render-~X~-R/./page.md~E`), or it is passed
  (`-R/templates/page.md/-/ns-template/expand--strict-templates`).
- The result is text; its media type follows the result's filename, and HTML escaping is on when
  that filename is `.html` or `.htm`.
- `validate` returns a list of `{severity, message, position}`: every invalid tag and every unknown
  command. It runs nothing.

A **template command** is an alias of `template/render` whose head parameter is the template link
([`COMMAND_ALIASES.md`](COMMAND_ALIASES.md); worked example in the guide).

## 5. Evaluation

`template::ContextHost` evaluates a query tag:

| Tag | Route | Result |
|---|---|---|
| No input, no override | `Context::get_dependency_state` | Cached, shared, a dependency of the render |
| An apply input or an override | `Context::apply_ad_hoc` | Ad hoc, never cached; its dependencies are passed on |
| `{{> key}}` | `get_dependency_state` of the key | A dependency; a missing partial is an error |

An apply input that is a value node is passed on as that value; any other node goes as its JSON.
Volatility is not passed on (`design/volatility-system`, decision 4): a render over volatile queries
is volatile only when its query ends in `/v`, or when a template command is declared `volatile`.
Partials nest at most `max_partial_depth` (32) deep, and the same partial at the same stack depth is
a cycle error.

## 6. Errors

| Error | Recoverable |
|---|---|
| Unclosed or mismatched section, a tag with no closing delimiter, a bad set-delimiter tag | **No**: fails at parse time in every mode |
| A tag whose content does not parse (kept as an `Invalid` node) | Yes |
| A bracket lookup, a value invalid in its slot, a query that fails, a missing or cyclic partial | Yes: the tag |
| A failing section head | Yes: the whole section, once |

| Mode | A recoverable error |
|---|---|
| `strict` | Fails the command with the first error. Invalid tags are rejected before any query runs |
| `expand` | `[template error at 12:7: message]` in place of the tag, and a warning in the log |
| `expand_html` | `<span class="liquers-template-error" title="12:7">template error: …</span>`, escaped |
| `expand_md` | The text form in a code span |
| `silent` | Nothing, and a warning in the log |

Positions are template line:column. An evaluation error takes its tag's position, because a
position it carried is in the evaluated query's text.

## 7. The AST

`Template::Mustache(MustacheTemplate)`, whose elements are text, variables, query tags, sections,
partials, comments, set-delimiter tags and `Invalid` nodes; a query body is `Hygienic(QueryTemplate)`,
`Classic(ClassicQuery)` or `Invalid`. `encode()` writes a template back so that
`parse(encode(t)) == t`. `QueryTemplate` mirrors `Query`, with `Slot { name, position }` in the
positions of §2; `QueryTemplate::try_into_query` succeeds exactly when it has no slots.

## History

| Date | Change | Source |
|---|---|---|
| 2026-10-17 | Created: tags, slot positions, names, data nodes, the `ns-template` commands, evaluation routes, error modes, the AST. | `design/liquers-mustache/` |
