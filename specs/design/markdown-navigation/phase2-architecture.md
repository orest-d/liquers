# Phase 2: Solution & Architecture — Markdown navigation on a Liquers block parser

## Overview

A new crate, `liquers-markdown` (on `liquers-core` only), parses a text into a **flat, lossless
list of blocks** that tiles the input, builds a **section tree** over the headings, finds **links
and code spans** inside inline text, and resolves **section addresses**. `liquers-lib` gets an
`ns-md` namespace of five sync commands over it (feature `markdown`), and every current user of the
line scanner `liquers-lib/src/search/md_split.rs` moves onto it; the scanner is deleted.

Rejected: `pulldown-cmark` (decided 2026-10-31: not lossless or editable, no query awareness); a
nested block tree now (a list or quote is one block, enough for navigation; nesting can come later
without changing sections); the parser in `liquers-records` (not about records; templates need it).

## Known-Issue Preflight

| Issue | Status | Priority | Impact on this design | Fix first? | Blocking? | Action |
|---|---|---|---|---|---|---|
| `NO-MARKDOWN-NAVIGATION-COMMANDS` | draft | P3 | Source; first slice | — | No | Closed by Phase 5 for this slice, or narrowed |
| `NO-QUERY-AWARE-MARKDOWN-PARSER` | draft | P2 | Builds its block layer and inline links/code spans | — | No | Stays open; Phase 5 records what landed |
| `NO-SOURCE-CODE-AND-API-DOC-NAVIGATION` | draft | — | Uses `md_split-line`, which is kept unchanged | No | No | None |

No open issue blocks the design.

## Interfaces

**`liquers-markdown/src/lib.rs`** (sync throughout: pure parsing of an in-memory string).

```rust
pub struct Span { pub start: usize, pub end: usize, pub line: usize, pub end_line: usize } // bytes; lines 1-based, inclusive
pub enum FrontMatterFormat { Yaml, Toml }
pub enum BlockKind {
    FrontMatter(FrontMatterFormat),
    Heading { level: u8, setext: bool, text: Span },   // text: the heading's own text
    Paragraph, List, BlockQuote, Table,
    FencedCode { info: String }, IndentedCode,
    Html, ThematicBreak, Blank,
}
pub struct Block { pub kind: BlockKind, pub span: Span }

pub struct Section {
    pub level: u8, pub heading: String, pub number: Option<String>, // "3.2" of "## 3.2 Foo"
    pub address: String, pub path: Vec<String>, pub slug: String,  // "2.1"; headings from the top; anchor
    pub block: usize, pub span: Span,                               // heading block; to its last subsection
    pub body_end: usize, pub body_end_line: usize,                  // own text: before the first subsection
    pub parent: Option<usize>, pub children: Vec<usize>,
}
pub enum LinkKind { Inline, Reference, Autolink, Image }
pub enum TargetKind { Relative, Anchor, Url, Query }
pub struct Link {
    pub kind: LinkKind, pub text: String, pub target: String, pub target_kind: TargetKind,
    pub anchor: Option<String>, pub query: Option<liquers_core::query::Query>,
    pub span: Span, pub section: Option<usize>,
}
pub struct CodeSpan { pub text: String, pub span: Span, pub query: Option<liquers_core::query::Query> }

pub struct Document { /* text: String, blocks, sections, inline cache */ }
impl Document {
    pub fn parse(text: &str) -> Document;                 // never fails (AC-7)
    pub fn text(&self) -> &str;
    pub fn blocks(&self) -> &[Block];
    pub fn sections(&self) -> &[Section];
    pub fn section_text(&self, id: usize, children: bool, heading: bool) -> &str;
    pub fn resolve(&self, address: &Address) -> Result<usize, liquers_core::error::Error>;
    pub fn find_by_heading(&self, name: &str) -> Option<usize>; // ns-mem's rule: own heading or "A > B"
    pub fn links(&self) -> Vec<Link>;
    pub fn code_spans(&self) -> Vec<CodeSpan>;
    pub fn title(&self) -> Option<&str>;                  // first heading (was first_heading)
    pub fn first_paragraph(&self, within: Option<usize>) -> Option<String>; // was first_paragraph / section_paragraph
    pub fn front_matter(&self) -> Option<(FrontMatterFormat, &str)>;
}
pub enum Resolved { Key(liquers_core::query::Key), OutsideRoot }   // OutsideRoot: `../` above the root, a dead link
impl Link {
    pub fn resolve(&self, document_key: &liquers_core::query::Key) -> Option<Resolved>; // None: not relative
    pub fn key(&self, document_key: &liquers_core::query::Key) -> Option<liquers_core::query::Key>;
}

pub enum Step { Number(usize), Text(String), Any } // `3`, `phase 1`, `_`
pub struct Address(pub Vec<Step>);
impl Address { pub fn parse(text: &str) -> Result<Address, liquers_core::error::Error>; }
impl std::fmt::Display for Address {}                       // inverse of parse: "." joins, ".." escapes
```

Ownership: `Document` owns its `String` and plain vectors; the commands build one per call and
drop it, so no `Arc` is needed. No serde: records are the external form.

**Parsing rules** (the ones navigation depends on; CommonMark where stated). Front matter only
at byte 0 (after a BOM), `---` closed by `---` or `...`, `+++` by `+++`, as
`liquers_records::search::front_matter_end` reads it (a test pins the two together). Fences: 3+ backticks or tildes, closed by a fence of the
same character at least as long, or the end of the text (CommonMark). ATX headings `#`–`######`
followed by a space or the line end, up to 3 spaces of indent; setext headings are a paragraph
followed by a `===` or `---` line. A line indented 4+ spaces outside a list is indented code; it cannot interrupt a paragraph.
An HTML block opens on `<!--` (running to `-->`), on a CommonMark block tag (`<div`, `<details`,
…), or on a line that is one complete tag, and runs to a blank line; an autolink `<https://…>` or
prose `<T>` opens none. Only a comment interrupts a paragraph. A table is a pipe line followed by a delimiter row with as many cells; it may take a
paragraph's last line as its header, as GFM does. List and quote blocks
run to the first blank line followed by a line that does not continue them. Inline: code spans by
backtick runs (CommonMark), then `[text](target "title")`, `![…](…)`, `[text][ref]` with
`[ref]: target` definitions, and `<scheme:…>` autolinks, outside code spans; emphasis is not
parsed. A target is a **query** when it begins with `-` or contains `/-/` and `parse_query`
accepts it; query text is never split by Markdown markup (AC-8). `#x` is an anchor; a scheme
(`https:`, `mailto:`) or a leading `/` is a url; anything else is relative.

**Addresses** (AC-2, AC-11). `Address::parse` splits on `.`, reading `..` as a literal dot; an
empty step or empty address is an error. A step of ASCII digits only is `Number`, a step that is
exactly `_` is `Any`, anything else `Text`, in which `__` reads as a literal `_` and any other `_`
is refused as reserved. Resolution keeps a set of candidate sections, starting
from the top-level sections (no title skipped): `Number(n)` takes each candidate's `n`-th child;
`Any` takes all their children;
`Text(t)` takes the
children whose heading matches `t` on whole words, ignoring case, preferring exact (the heading,
or the heading without its number) over prefix over contained, so `phase 1` never matches
"Phase 10". At the end exactly one section must
remain (AC-3). A step matches direct children only; when nothing matches but a
deeper section would, the error names its full address.

**Commands** (`liquers-lib/src/markdown/commands.rs`, after `COMMAND_REGISTRATION_GUIDE.md`; sync
`fn`, since parsing never waits and the input is already a text value):

```rust
register_command!(cr, fn outline(state) -> result namespace: "md" label: "Outline"
    doc: "The headings of a Markdown document as records: n, level, heading, address, number, path, slug, line, end_line, words")?;
register_command!(cr, fn section(state, address: String, children: bool = true, heading: bool = true) -> result
    namespace: "md" label: "Section" doc: "…")?;
register_command!(cr, fn sections(state, address: String = "", level: i64 = 0) -> result namespace: "md" …)?;
register_command!(cr, fn links(state, address: String = "") -> result namespace: "md" …)?;
register_command!(cr, fn table(state, address: String, n: i64 = 1) -> result namespace: "md" …)?;
// No `context`: the Markdown table reader reports no warnings.
```

The input is the document's text (`state.try_into_string()`); the key that `links` resolves
against is the input's own `state.metadata` key, as the guide says for "the input's description".
`address=""` means the whole document for `sections` and `links`. `table` takes the `n`-th
`Table` block inside the section and reads its bytes with
`liquers_records::formats::read_table_with_report(…, TableFormat::Markdown, ReadSchema::Infer, …)`;
the Markdown reader never warns, so `table` needs no `context`. Every reader returns `Value::from_record_view`, except `section` (text).

## Integration Points

- **New crate** `liquers-markdown/` (workspace member and default member; `Cargo.toml` with
  `liquers-core` only); `lib.rs`, `block.rs`, `inline.rs`, `section.rs`, `address.rs`.
- **`liquers-lib/Cargo.toml`:** `records` adds `dep:liquers-markdown` (its search, store-config and
  `mem` code needs the parser); new `markdown = ["records"]` in `default`, gating only `ns-md`.
- **`liquers-lib/src/markdown/`** (new, `#[cfg(feature = "markdown")]`): commands, record schemas,
  `register_markdown_commands!`, and its no-op stand-in in `commands.rs` beside the others, called
  from `register_all_commands!`.
- **`search/md_split.rs`:** the scanner goes; its helpers (`sections(text, skip_front_matter)`,
  `find_section`, `first_heading`, `first_paragraph`, `section_paragraph`) become thin adapters
  over `Document`, and `md_split` and `ns-mem/search unit=section` build rows from them (row 0,
  the preamble, for text before the first heading, as now; front matter excluded for `ns-mem`).
- **`mem/read.rs`, `write.rs`, `rules.rs`, `tiers.rs`:** call the adapters; `blank_front_matter`
  goes; `ns-mem/read section` and `edit section` keep their matching rule via `find_by_heading`.
- **`mem/check.rs` `dead_links`:** iterates `Document::links()` with `TargetKind::Relative` and
  `Link::resolve` (`OutsideRoot` is dead), replacing its private scanner and `strip_inline_code`.
- **`liquers-agent`:** builds `liquers-lib` with `markdown`; `tools/memory.yaml` allows `-md-*`
  and serves `liquers://commands/md`; `tools/query-guide.md` shows `outline` and `section`.
- **`store_config.rs` `markdown_tiers`:** `Document::title` and `first_paragraph`.
- **Registry and matrix:** `specs/command_registry.yaml` gains the `markdown` group (export group
  list, Origin header, and the "all groups" gate of `liquers-lib/tests/registry_export.rs`);
  `scripts/check-build-matrix.sh` gains `markdown` rows and `liquers-markdown`'s own, wasm32 too.

## Error Handling

| Case | Error |
|---|---|
| Empty address, empty step (trailing or leading `.`), a reserved `_` (`my_var`) | `Error::general_error("md: invalid address '…': …")` |
| No section matches | `Error::general_error` naming the address, the step that failed and, if a deeper section matches, its full address |
| Several sections match a step | `Error::general_error` listing each candidate's address, heading and line |
| No `n`-th table in the section | `Error::general_error` naming the section's address and the count found |
| Table bytes unreadable | the records reader's error, unchanged |
| Input is not text | `try_into_string`'s conversion error |

Parsing itself has no error (AC-7).

## Relevant Commands

New: the five `ns-md` commands. Same signatures and output, new internals: `ns-search/md_split`,
`ns-mem/read`, `edit`, `check`, `search`, and `markdown_tiers`. Namespaces: `md`, `search`, `mem`,
realm `store_config`.

## Documentation Architecture

- `specs/reference/MARKDOWN.md` (new; reference, internal, `lib/commands`): the block model and
  its rules above, sections and addresses (with the warning that section numbers move when a
  section is inserted, so stored references use text steps), the five commands and their fields.
- `specs/reference/AGENT_MEMORY_SERVICE.md` §4: navigating a document with `ns-md`.
- `specs/reference/PROJECT_OVERVIEW.md` and `CLAUDE.md`: the new crate, its place in the
  dependency flow, the `markdown` feature in the feature lists.
- `liquers-agent` query guide: a section on `ns-md/outline`, `section` and `links`.
- `affects_docs`: these five; `specs/README.md` gains a link to `MARKDOWN.md` at Phase 5.

## Risks

| Assessment | Finding |
|---|---|
| Files and crates | as Integration Points; `liquers-agent` through `mem`; `liquers-web`'s wasm32 build links the crate with `records` |
| Existing tests likely to change | none by intent (AC-9); `md_split` unit tests move to the crate. Two behaviour changes are intended: setext headings now count, and `#` lines in HTML or indented blocks no longer do |
| New validation | crate unit tests per rule; a round-trip and tiling test over every `specs/**/*.md` (AC-7); relative links over `specs/` compared once, before and after (Phase 3 learning log) |
| Compatibility / data / concurrency / performance / security | `dead_links` newly sees reference-style links, so it may report links the scanner missed (stated in AC-5's test as an expected difference, not a regression). Linear time; `markdown_tiers` runs on every read, so the crate's tests parse a 100 KB document. No unsafe, no I/O |
| Recovery | each step reverts on its own: the crate and `ns-md` are additive; the callers switch one module at a time behind passing tests |
| Certainty and open questions | High for blocks and sections; medium for list and HTML-block edge cases. No open questions |
