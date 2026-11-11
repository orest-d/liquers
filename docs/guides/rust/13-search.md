---
title: Search and agent memory
status: ready
sources: [guides/SEARCH_GUIDE.md, reference/SEARCH.md, reference/AGENT_MEMORY_SERVICE.md]
reviewed: 2026-11-10
---
# 13. Search and agent memory

PURPOSE: The reader can list and search stored documents and tables from Rust, and knows what the agent memory and the MCP crates offer.

Finding things is done with commands, like everything else. The commands of the namespace
`search` turn a folder into a table and search tables, and because their results are tables, the
commands of chapter 9 continue where they stop.

## 13.1 Listing and searching a folder

```rust
use liquers_core::assets::AssetManager;
use liquers_core::metadata::MetadataRecord;
use liquers_core::query::{ActionParameter, ActionRequest};
use liquers_records::FieldValue;

// A Markdown document as a state: the text, and the format it is written in.
fn document(text: &str) -> State<Value> {
    let mut record = MetadataRecord::new();
    record.data_format = Some("md".to_string());
    State::from_value_and_metadata(Value::from(text), Arc::new(Metadata::MetadataRecord(record)))
}

let env = library_environment(Arc::new(AsyncMemoryStore::new(&Key::new())))?;
let assets = env.get_asset_manager();
let shopping = "---\ntitle: Shopping\nstatus: open\n---\n# Shopping\n\nRemember the milk and the bread.\n";
let trip = "---\ntitle: Trip to Prague\nstatus: done\n---\n# Trip\n\nBook the train to Prague.\n";
assets.set_state(&parse_key("notes/shopping.md")?, document(shopping)).await?;
assets.set_state(&parse_key("notes/trip.md")?, document(trip)).await?;

// The values of one column of a table, as text.
let column = |query: String, name: &'static str| {
    let env = env.clone();
    async move {
        let table = env.evaluate(query).await?.get().await?.value()?.as_record_view()?;
        let index = table.schema().fields.iter().position(|field| field.name == name);
        let index = index.ok_or_else(|| Error::general_error(format!("no column {name}")))?;
        let mut cells = Vec::new();
        for row in 0..table.len() {
            match table.value(row, index)? {
                FieldValue::Text(text) => cells.push(text.to_string()),
                other => cells.push(format!("{other:?}")),
            }
        }
        Ok::<_, Error>(cells)
    }
};

// A folder as a table: one row for each file.
let files = column("-R-key/notes/-/ns-search/catalog".to_string(), "filename").await?;
assert_eq!(files, ["shopping.md", "trip.md"]);

// Full-text search in the files.
let hits = column("-R-key/notes/-/ns-search/catalog-t-t/search-milk".to_string(), "key").await?;
assert_eq!(hits, ["notes/shopping.md"]);
```

The example first stores two Markdown documents as values. `document` gives each state the data
format `md`, so that the stored file is known to be text.

Two things are new in the queries.

`-R-key/notes` does not read anything. It passes the *key* `notes` to the next command, which is
how you hand a folder to a command. (`-R/notes` would try to read the folder as a value.)

`catalog` makes a table with one row for each entry of the folder: `key`, `filename`,
`extension`, `title`, `status`, `media_type` and more. It has two switches: whether to walk into
subfolders (on by default) and whether to read the text of each file (off by default).
`catalog-t-t` turns both on, which `search` needs in order to look inside the documents.
`search-milk` then keeps the rows that match, best match first, and adds three columns: `score`,
`matched` (where the word was found) and `excerpt`.

Listing does not evaluate anything. A key that exists only as a recipe is listed with the status
`Recipe` and is not computed. The text of a file is read only when the file is known to be
text.

The closure `column` in the example is ordinary record-view code (section 9.2). In a program you
would more likely end the query with a file name, such as `hits.json`, and hand the bytes on.

## 13.2 Searching by fields

Many documents begin with a block of fields, their *front matter*, as the notes above do.
`field_extract` turns those fields into columns, and a search expression can then refer to them:

```rust
// Search by a field of the front matter. The expression has a colon: let the encoder escape it.
let search = ActionRequest::new("search".to_owned()).with_parameters(
    ["status:open", "f", "f", "0"]
        .iter()
        .map(|text| ActionParameter::new_string(text.to_string()))
        .collect(),
);
assert_eq!(search.encode(), "search-status~ncolon~open-f-f-0");
let query = format!("-R-key/notes/-/ns-search/field_extract/{}", search.encode());
assert_eq!(column(query, "title").await?, ["Shopping"]);
```

The expression `status:open` contains a colon, which must be escaped in a query. The example
builds the action from its parts and lets `encode` do the escaping (section 3.9), which is the
safe way to put a search expression typed by a user into a query.

The arguments of `search` are, in order: the expression, whether to rank the results (`t` by
default), whether the case of letters matters (`f`), the largest number of results (50; 0 means
all), and then the names of the columns to search in. `search-…-f-f-0` therefore means: not
ranked, not case-sensitive, all matches.

## 13.3 Search expressions

| Write | It matches when |
|---|---|
| `word` | the word occurs in one of the searched columns |
| `a b` | both occur |
| `a \| b` | either occurs |
| `-a` | `a` does not occur |
| `"a phrase"` | the words occur in this order |
| `/re/` | the regular expression matches |
| `field:value` | the column equals the value; `*` and `?` are wildcards, `field:*` means "has a value" |
| `field:>5`, `>=`, `<`, `<=` | the column compares so, for numbers and dates |
| `( … )` | grouping |

A malformed expression does not fail. Its words are searched for, and a warning with the
position of the mistake is written to the log of the result. When a search returns something
unexpected, read the log in the metadata of its state.

## 13.4 Other things to search

The same `search` command works on any table:

| Query | Finds |
|---|---|
| `-R/data/sales.csv/-/ns-search/search-feb-f-f-0` | the rows of a stored table that contain `feb` |
| `-R-key/notes/-/ns-search/md_split/search-packing` | the *sections* of Markdown documents that match, with their headings and line numbers |
| `-R-key/src/-/ns-search/md_split-line/search-TODO-f-f-0` | the matching *lines* of text files, like `grep -n` |
| `ns-search/commands/search-resize` | the registered commands that have to do with resizing |

The last one is a quick way to find a command when you do not know its name. Each row has a
`query` column with the text that calls the command, such as `ns-img/resize`.

**Your own tables.** A table that your command returns is searched best when its schema says how
(section 9.2). Give `FieldRole::text()` to the columns a person would search by words, such as
names, titles and descriptions, and `FieldRole::keyword()` to codes and statuses.

Some limits. Search reads the folder each time its result has to be computed again; there is no
index yet, so narrow the folder before you search the content of many files. It matches words,
not meanings. And like every result, a search result is refreshed when files change through
Liquers, not when another program edits them while Liquers runs.

For Markdown there is also the namespace `md` (feature `markdown`), which reads one document by
its structure: its outline, one section by its heading, its links.

## 13.5 The agent memory

The same building blocks, a store of documents and commands that list, search, read and write
them, make a *memory* for AI agents: a folder of Markdown documents with a schema that says
which collections exist and which fields their documents carry. The commands are in the
namespace `mem` (feature `mem`):

| Command | Does |
|---|---|
| `ns-mem/overview`, `ns-mem/describe` | describe the memory and its collections |
| `ns-mem/list`, `ns-mem/search`, `ns-mem/similar` | find documents |
| `ns-mem/read` | reads a document, or a summary of it |
| `ns-mem/new`, `ns-mem/write`, `ns-mem/edit` | create and change documents, with a log of every change |
| `ns-mem/check` | checks the memory against its schema |

Their natural user is an agent, and for agents Liquers has a program, `liquers-agent`. It serves
these commands as tools over the Model Context Protocol (MCP), so that an MCP client such as a
coding assistant can use a memory directly. It is configured with a file, and the same commands
are available from its command line.

Why offer queries to an agent at all? An agent normally works either with a fixed list of tools,
which is safe but rigid, or by running code it writes, which is flexible but hard to check.
Queries sit in between. An agent can combine commands freely, and yet each query can be checked
before it runs, is cached, and leaves a record of what it read and produced.

## 13.6 Your commands as tools

The crate behind `liquers-agent` is `liquers-mcp`, and it is not specific to memories. A program
declares *tool sets*. Each tool is one query whose last action is the command the tool runs, and
a set may add a general query tool that is limited by a list of allowed commands. The crate
works out the arguments of every tool from the command metadata, evaluates a call as a query,
and returns the value in the format that the query's file name selects. The descriptions that
the agent sees are the `doc:` and the argument descriptions of your commands (sections 5.3
and 5.6), so a command that is documented well for people is documented for agents too.

`liquers-mcp` depends on `liquers-core` only. A program that serves its own commands to agents
does not need the command library.

EDITOR: still to be written: a worked example of a tool set served with `liquers-mcp` (`ToolSetDecl`, `McpServer`, `serve_stdio`).
SOURCE: reference/AGENT_MEMORY_SERVICE.md
