# Phase 2 details — Store and asset search

Companion to [Phase 2](./phase2-architecture.md), created 2026-10-11 by the Phase 2 review. It
holds the tables Phase 2 relies on, so that Phase 2 itself stays near its size limit. Everything
here is decided; most of it moves into `specs/reference/SEARCH.md` at Phase 5. The `/` cases in §B
become Phase 3 parser tests.

## A. Writing the syntax in a query

A Liquers action parameter accepts only alphanumerics, `_`, `+` and `.` unescaped
(`liquers-core/src/escape.rs`), so almost every operator of the syntax must be escaped. The
escapes are those of `specs/guides/QUERY_ESCAPING_GUIDE.md`, and
`liquers_core::query::encode_token` produces them, which is how a UI or MCP tool builds the query.

| Syntax | Character | In a query |
|---|---|---|
| words | space | `~.` |
| `field:value` | `:` | `~ncolon~` |
| `-term` | `-` | `~_` |
| `-` before digits (in a date) | `-` | `~` and then the digits (`2026~10~01`) |
| `"phrase"` | `"` | `~nquot~` |
| `( … )` | `(`, `)` | `~nlpar~`, `~nrpar~` |
| `a \| b` | `\|` | `~nverbar~` |
| glob | `*`, `?` | `~nast~`, `~nquest~` |
| comparison | `>`, `<`, `=` | `~ngt~`, `~nlt~`, `~nequals~` |
| `/re/` | `/` | `~/` |
| inside a regex | `\`, `[`, `]` | `~nbsol~`, `~nlsqb~`, `~nrsqb~` |

Examples, each validated with `liquers-validate`, which also confirmed the decoded parameter:

| Expression | Encoded parameter |
|---|---|
| `expiration -status:closed*` | `expiration~.~_status~ncolon~closed~nast~` |
| `area:core/store (priority:P0 \| priority:P1)` | `area~ncolon~core~/store~.~nlpar~priority~ncolon~P0~.~nverbar~~.priority~ncolon~P1~nrpar~` |
| `/fn get_asset_info/` | `~/fn~.get_asset_info~/` |
| `heading:/\bPhase 2/` | `heading~ncolon~~/~nbsol~bPhase~.2~/` |
| `updated:>2026-10-01` | `updated~ncolon~~ngt~2026~_10~_01` |

**Positions** in parse errors and warnings are offsets into the *decoded* expression, not into the
query text.

## B. When `/` is a regex

The regex rule (Phase 2 §Grammar) is: an atom or value that **begins** with `/` is a regex when an
unescaped `/` later in it is immediately followed by a delimiter (whitespace, `)`, `|` or the end).
The body ends at the **first** such `/`. Otherwise the text is a term or value.

| Input | Parsed as |
|---|---|
| `area:core/store` | a field test, value `core/store` (does not begin with `/`) |
| `key:specs/issues/*` | a glob value |
| `a/b` | the term `a/b` |
| `/fn \w+_info/` | a regex term, `fn \w+_info` (the space is inside the regex) |
| `title:/^Phase [12]/` | a field regex |
| `path:/usr/bin` | the value `/usr/bin`; nothing closes it |
| `/a/b/` | the regex `a/b`; the inner `/` is followed by `b`, not by a delimiter |
| `/a/ /b/` | two regex terms, `a` and `b` |
| `title:/x/ \| /y/` | a field regex, OR a regex term |
| `/unclosed` | the term `/unclosed` |
| `f:` | the term `f:`; an empty value is not a field test |

The regex token is a hand-written scanner inside the `nom` parser, tried before `term`.

## C. Record schemas

**`catalog`:**

| Column | Type | Roles |
|---|---|---|
| `key` | Text | Id; keyword |
| `filename`, `title`, `description` | Text | text + stored |
| `content` | Text, nullable | text; present only with `content = true` |
| `extension`, `parent`, `status`, `type_identifier`, `media_type` | Text | keyword |
| `is_dir` | Bool | keyword |
| `size` | Int, nullable | numeric |
| `updated` | Timestamp, nullable | numeric |

**`commands`:**

| Column | Type | Roles | Source |
|---|---|---|---|
| `id` | Text | Id | `CommandKey` display, `realm-namespace-name` |
| `query` | Text | keyword | the call fragment `ns-<namespace>/<name>` |
| `realm`, `namespace`, `name`, `module` | Text | text + keyword | — |
| `title` | Text | text | `label` |
| `description` | Text | text | the generated signature, with argument names, types and defaults |
| `doc` | Text | text | `doc` |
| `volatile` | Bool | keyword | — |
| `output_filename` | Text | keyword | the metadata's `filename` |

**`field_extract` output:**
- The input's columns, minus the dropped rows.
- One column per extracted field, named `prefix + name`:
  - Characters outside `[A-Za-z0-9_]` in a key become `_`, and a name starting with a digit gets a
    leading `_`, so every column is addressable in the syntax.
  - The prefix is sanitised the same way.
- With `flatten`, nested maps become `parent_child` columns. Without it, a nested map is one Text
  column declared JSON (`media_type: application/json`). A list is always such a JSON column.
- Each column's type is unified over its rows:
  - **Int** if every value is an integer;
  - **Float** if every value is a number;
  - **Bool** if every value is a boolean;
  - **Text** otherwise. Values are written as strings, and ISO dates compare correctly as text.
- A column that replaces an input column keeps that column's role. Every other extracted column is
  keyword. The Id column is never replaced.

**`md_split`** (a proof of concept):

| Column | Type | Roles |
|---|---|---|
| `id` (`key#n`) | Text | Id |
| `key` | Text | keyword: the source's full key |
| `filename` | Text | text + keyword: the source's file name |
| `n` | Int | numeric: the section number (0 = text before the first heading) or the line number |
| `line`, `end_line` | Int | numeric: 1-based; equal for a line |
| `heading` | Text, nullable (sections only) | text: the path `H1 > H2 > H3` |
| `level` | Int, nullable | numeric |
| `text` | Text | text |

## D. Ranking constants

| Setting | Value |
|---|---|
| BM25 | k1 = 1.2, b = 0.75 |
| Statistics | from the rows that survive the mask |
| Tokens | split on non-alphanumerics |
| Term frequency | substring occurrence count; match count for a regex |
| Field weights | `title` 3; `filename`, `name`, `description` 2; others 1 |
| Phrase | counts double |
| Excerpt | about 160 characters around the first match in the first matching field, skipping a leading front-matter block |

## E. Structure parsing for `field_extract`

| Extension | Format | Rule |
|---|---|---|
| `json` | `Json` | the whole text; the result must be a map |
| `yaml`, `yml` | `Yaml` | the whole text; the result must be a map |
| `md` | `FrontMatter` | YAML (`---`): the first line is `---`, and a later line is `---` or `...`. TOML (`+++`): the first line is `+++`, and a later line is `+++`. In either case the block must parse to a map |

Conversion to `serde_json::Value`:
- **YAML:** goes through `serde_yaml::Value`. A map key that is a scalar becomes its string form; a
  non-scalar key is skipped, and counted in the Info log entry.
- **TOML:** is converted with an explicit, exhaustive match over `toml::Value`, because
  `serde_json::to_value` would turn a TOML datetime into a private wrapper object. A `Datetime`
  becomes its ISO-8601 string.

## F. Command steps

**`folder_of`** maps the input state to a folder:

| Input state | Folder |
|---|---|
| No value and no key | the root key |
| A `dir` state, or an `AssetInfo` listing (`-R-dir`, `-R-sdir`) | the key in its metadata |
| A `Key`, or a `Query` whose `key()` is `Some`, that is a directory | that key |
| Anything else | `None` |

**`catalog`:**
1. Resolve the folder with `folder_of`.
2. Walk the tree breadth-first with `AssetManager::listdir` and `store.is_dir`, stopping at
   `max_entries`, so the bound also limits the walk. Describe each entry with `get_asset_info`,
   with up to 16 calls in flight (`futures::stream::buffer_unordered`, which needs no tokio).
3. Sort the rows by key.
4. With `content`, read text through `read_text`. That reads stored bytes only, from `Ready`,
   `Source` or `Override` entries with a **text media type**, and cuts them at a UTF-8 boundary
   after `max_bytes`. Then apply the Markdown title fallback (AC-30).

   A **text media type** is `text/*`, `application/json`, `application/x-yaml`,
   `application/x-toml` or `application/xml`, or any type with a `+json` or `+xml` suffix. These are
   the types `liquers-core/src/media_type.rs` assigns from file extensions.

**`field_extract`:**
1. The input is a folder (walked as `catalog` walks it, without content), or records taken to be a
   catalog (`key`; optionally `filename` and `content`).
2. Keep the files with the extension.
3. Read the text of those files only, from `content` or through `read_text`.
4. Parse it (§E), and drop the files with no fields.
5. Build the columns (§C).

**`md_split`** takes the same inputs as `field_extract`, but has no extension argument. *Clarified
2026-10-11 by the Phase 3 review, and confirmed by the user:*
- `unit = "section"` keeps `.md` files only;
- `unit = "line"` keeps every entry `read_text` can read, which includes source code (`.rs` maps to
  `text/x-rustsrc`).
- A section is a heading plus its body, up to the next heading of the same or a higher level. Text
  before the first heading is section 0.
- Lines inside fenced code blocks are never headings.
