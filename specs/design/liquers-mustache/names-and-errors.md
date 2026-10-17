# Names, indexing and error modes

Supporting note for [Phase 2](./phase2-architecture.md), open questions 15 and 16. Written
2026-10-17 from the maintainer's review of the same day: names gain list indexing and a bracket
lookup `x[y]`, and rendering gains three error modes, with syntax errors that are recoverable where
the parser can resynchronize.

## Name grammar

A name appears in variables, section heads and closes, apply targets and slots in query tags, so
everything below works in `{{x}}`, `{{#x}}`, `{{$.x q}}` and `{{$ cmd-{{x[k]}}}}` alike.

```
name    = "." | head tail*
head    = field
tail    = "." field | "." index | "[" key "]"
field   = one or more characters other than whitespace, ".", "[", "]" and the closing delimiter
index   = digits                       -- `items.0`
key     = name | integer | string      -- `x[y]`, `x[0]`, `x[-1]`, `x["a b"]`
string  = '"' (any character but '"' or '\', or '\' followed by '"' or '\')* '"'
integer = "-"? digits
```

```rust
pub struct Name { pub parts: Vec<NamePart>, pub position: Position }      // empty parts = `.`
pub enum NamePart {
    Field(String, Position),        // `a` in `a.b`
    Index(usize, Position),         // `0` in `items.0`
    Lookup(LookupKey, Position),    // `[y]`
}
pub enum LookupKey { Name(Name), Integer(i64), Text(String) }             // `[y]` `[-1]` `["a b"]`
```

**Departure from the Mustache spec.** `[` and `]` become reserved in names. The spec defines names
only as dot-separated and contains no test with a bracket, so the spec suite (AC-1) is unaffected.
A data field whose name contains a bracket or a space is reached as `x["odd [name]"]`.

## Resolution

| Part | List | Map | Scalar, null, opaque |
|---|---|---|---|
| `.field` | missing | the field, or missing | missing |
| `.digits` | the item, or missing when out of range | the field named by the digits, or missing | missing |
| `[key]` | `key` must be an integer, negative counting from the end: the item, else **error** | `key`'s text is the field name: the field, else **error** | **error**: not indexable |

- **Dotted parts follow Mustache.** A missing part makes the whole name missing, which renders as
  the empty string and is falsey in a section. Only the first part searches the context stack, as
  the spec requires. Nothing here is an error, because the spec suite depends on that.
- **A bracket is a strict lookup**, as the maintainer specified: `y` is resolved first, as a whole
  name from the top of the context stack (not relative to `x`), then `x`'s kind chooses index or
  key, and an absent element is an error. A `y` that is not a scalar, or a non-integer used on a
  list, is an error. `x[y[0]]` nests.
- **In a query tag**, a missing name is already an error (Phase 2, Error Handling), so the two
  forms differ only in text and sections.
- Errors are positioned at the part (`NamePart`'s `Position`) and handled by the error mode below.

`DataNode` and `AsyncDataNode` gain one defaulted method each, so no implementation has to change:

```rust
fn index(&self, i: usize) -> Option<NodeRef<'_>> { self.items().nth(i) }                  // DataNode
async fn index(&self, i: usize) -> Result<Option<AnyNode>, Error> { /* nth of items() */ } // AsyncDataNode
```

`RecordView` and the frame adapter override `index` to build one row directly. A negative index
needs `len()`; on a list whose length is unknown (an async source) it is an error.

**Cost.** Small. The name lexer grows from splitting on `.` to a recursive-descent rule of about a
hundred lines, the resolver gains the `Lookup` arm (lookups are sync on sync nodes and one await on
async ones), and `encode()` writes the brackets back. With tests it is about 400 lines, inside the
parser and renderer the design already has, and no other interface changes. The risk is the
reserved characters, which the spec suite checks.

## Error modes

```rust
pub enum ErrorMode { Strict, Expand(ExpandFormat), Silent }
pub enum ExpandFormat { Text, Html, Markdown }
pub struct RenderOptions { pub html_escape: bool, pub max_partial_depth: usize, pub errors: ErrorMode }
```

| Mode | Command value | A recoverable error |
|---|---|---|
| **Strict** (default) | `strict` | Stops rendering. The command fails with the first error, positioned in the template. Before anything runs, the parsed template is checked: a recoverable syntax error fails here, so no query runs for a template that cannot render |
| **Expand** | `expand`, `expand_html`, `expand_md` | Rendered in place of the failing tag and logged as a warning |
| **Silent** | `silent` | Rendered as the empty string and logged as a warning |

The expanded text names the position and the message:

| Format | Rendered as |
|---|---|
| `Text` | `[template error at 12:7: no field "titel" in map]` |
| `Html` | `<span class="liquers-template-error" title="12:7">template error: no field &quot;titel&quot; in map</span>`, HTML-escaped |
| `Markdown` | `` `[template error at 12:7: no field "titel" in map]` ``, a code span fenced by more backticks than the message contains, so it is not read as markup |

Expanded text is not escaped again by `html_escape`, since each format escapes its own message.
Warnings go through a new `TemplateHost::warn(&self, error: &Error)`; `ContextHost` calls
`Context::warning`, which writes to the result asset's log, and `MapHost` and `RecordingHost` collect them in a `warnings` list for tests.

**What each mode covers.** The unit of failure is one tag.

| Error | Recoverable |
|---|---|
| A bracket lookup or a value invalid in its slot (P2–P5, a misplaced non-scalar, a missing name in a query) | yes: the tag |
| A query that fails to evaluate, a classic query that fails to parse after expansion | yes: the tag |
| A partial that is missing, cyclic or too deep | yes: the partial tag |
| A failing section head | yes: the whole section, which renders once as the error (expand) or as nothing (silent) |
| A tag that does not parse but whose end is known (below) | yes: the tag |
| Unclosed or mismatched section, a tag with no closing delimiter, a bad set-delimiter tag | **no**: always fails immediately, in every mode, because the structure after it is unknown |

## Recoverable syntax errors

The lexer finds a tag's end by balancing braces before the tag's content is parsed. Once the end is
known, a bad content can be kept as an error node and parsing continues after it. The AST gains:

```rust
pub struct Invalid { pub source: String, pub error: Error, pub position: Position }  // `source` round-trips
Element::Invalid(Invalid)            // unknown sigil, bad name: `{{#a..b}}` is an error, `{{a b}}` too
SectionHead::Invalid(Invalid)        // the section's structure is still known, so it stays a Section
QueryBody::Invalid(Invalid)          // a hygienic query that does not parse
PartialTarget::Invalid(Invalid)      // a partial key that does not parse
```

`Error` is already `Serialize, Deserialize, Clone`, so the AST keeps its derives. `encode()` writes
`source` back, so `parse(encode(t)) == t` still holds. `parse_template` returns `Err` only for the
irrecoverable cases; `Template::errors()` lists the `Invalid` nodes, and `validate_template`
reports them all, not just the first, which is what an editor needs.
