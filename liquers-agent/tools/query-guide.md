# Queries over this memory

`memory_query` evaluates one Liquers query. It may use only the read-only commands of the
namespaces `mem`, `rec`, `search` and `md`; anything else is refused before it runs.

## Shape

A query is a path. It starts with a resource and continues with commands after `/-/`:

- `-R-key/./<path>` is a **key**: a folder for `ns-mem/list`, `search`, `check`, or a document
  for `ns-mem/read`, which then reads the file as it is now.
- `-R/./<key>` is a document's **value**, as the service last loaded it; `ns-mem/read` takes it
  too.
- `./` makes the key relative to the memory root. Without it, a key starts at the store's root.
- `ns-mem/` selects the namespace of the commands that follow; `ns-rec/` switches to records.
- A command's arguments follow its name, separated by `-`, in the order its reference lists
  them: `list-2-false-1000` is depth 2, superseded hidden, limit 1000.
- The last element, a filename, picks the output format by its extension: `.md` (a pipe table
  for records), `.csv`, `.json`, `.yaml`.

Inside an argument, write `~_` for `-`, `~.` for a space, `~/` for `/` and `~~` for `~`.

## Examples

```text
-R-key/./notes/idea.md/-/ns-mem/read-l1/idea.md
-R-key/./notes/idea.md/-/ns-mem/read-full-Problem/problem.md
-R-key/./notes/-/ns-mem/search-expiration~.policy/search.md
-R-key/./-/ns-mem/list-0-false-1000/ns-rec/eq-status-draft/ns-rec/sort-l0/drafts.csv
-R-key/./-/ns-mem/check/ns-rec/eq-severity-error/errors.json
```

The fourth lists every memory (depth 0 is unlimited), keeps the drafts and sorts them by title.
`list` applies its own `limit` before `eq` filters, so raise the limit when you filter.

## One document, one section

```text
-R/./notes/idea.md/-/ns-md/outline/outline.md
-R/./notes/idea.md/-/ns-md/section-_.problem/problem.md
-R/./notes/idea.md/-/ns-md/links-_.2/links.md
```

These navigate a document without reading all of it: `outline` lists its headings with their
`address`, `section-<address>` returns one section, and `links` the links in it, with the key
each relative link resolves to. An address is a path of steps separated by `.`: a section
number (`1.2`), heading text (`phase~.2.solution`, matched without case on whole words) or `_`
for any one section, so `_.2` is section 2 under the document's title. `..` is a literal dot and
`__` an underscore. `table-<address>` returns the table in a section as records.

## Commands

The arguments, defaults and documentation of every allowed command are in
`liquers://commands/mem`, `liquers://commands/rec`, `liquers://commands/search` and
`liquers://commands/md`.
