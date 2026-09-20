---
id: RECORD-MANIFEST-FORMAT
kind: analysis
title: The record manifest — a generative recipe format for record streams
workflow: liquers-project
status: draft
area: [lib/value, core/assets]
created: 2026-09-20
---
# The record manifest — a generative recipe format for record streams

A **manifest** describes a record stream as data: which chunks it has, or how to generate them, and
what arguments every chunk query needs. It can be written by hand or produced by a command, and a
manifest — or a folder containing one — can be read as a `RecordSource`.

**Scope.** This specifies the **format**. The recipe provider that would interpret a manifest as
generative recipes is deliberately **not designed here**: it waits on `NO-RECIPE-PROVIDER-CHAIN`, so
that such a provider can sit beside `DefaultRecipeProvider` rather than replace it. Cached batches as
keyed assets likewise wait. What the format must do now is not foreclose either.

## 1. The format mirrors `recipes.yaml`, because the mechanism is the same

`Recipe` (`recipes.rs`) carries `query`, `title`, `description`, `arguments`, `links`, `cwd`,
`volatile` and `expires`. Two of those are the reason a manifest is worth having at all:

```rust
// Recipe::to_plan, recipes.rs:257-277
let mut planbuilder = PlanBuilder::new(query.clone(), cmr).with_placeholders_allowed();
let mut plan = planbuilder.build()?;
for (name, value) in &self.arguments {
    if !(plan.override_value(name, value.clone())) { /* "Argument {} not found in last action" */ }
}
for (name, link) in &self.links {
    if !(plan.override_link(name, parse_query(link)?)) { /* … */ }
}
```

`arguments` binds a JSON value to a parameter of the **last action** *by name*; `links` binds an
encoded query the same way. Planning runs `with_placeholders_allowed`, so the query may leave those
parameters unfilled.

**This is exactly what a complex SQL statement needs.** Multi-line text with quotes, parentheses and
commas cannot be escaped into a query segment in any way a person would want to read — but it goes
into `arguments` as an ordinary YAML scalar and never enters the query string at all. A manifest
reusing this mechanism inherits it rather than inventing an escape.

## 2. File naming: `<filename_prefix>.manifest.yaml`

Several manifests may share a folder with each other and with `recipes.yaml`, distinguished by their
filename prefix. So the prefix **is** the filename stem:

```
data/sales/
  recipes.yaml           ordinary recipes
  daily.manifest.yaml    a record stream whose chunks are daily_0000.csv, daily_0001.csv, …
  monthly.manifest.yaml  a second stream, monthly_0000.parquet, …
```

The prefix is derived from the filename and is **not** a field in the file. One source of truth:
renaming the file renames the stream, collisions are impossible by construction, and a provider finds
manifests by globbing `*.manifest.yaml` without parsing anything.

**Coexistence rule:** a `recipes.yaml` in the same folder must not declare a filename matching any
manifest's chunk pattern `<prefix>_<digits>.<ext>`. That is a validation check, not an invariant the
format can enforce.

## 3. The folder is the `cwd`, and there is no `folder` field

`DefaultRecipeProvider` sets each recipe's `cwd` to the directory holding `recipes.yaml`
(`recipes.rs:622`). A manifest follows: **its folder is its `cwd`**, used both to resolve relative
keys and links and as the folder cached chunks land in.

So no `folder` or `folder_key` field. A manifest is relocatable, a copied folder is a copied stream,
and there is nothing to keep in sync. `cwd` is provider-set and must not appear in the file, exactly
as for recipes.

A manifest produced by a command and never stored takes its `cwd` from the context instead.

## 4. The format

```yaml
# data/sales/daily.manifest.yaml
manifest: record-stream        # discriminator; a folder may hold several kinds of yaml
version: 1

title: Daily order extract
description: |
  One chunk per 1000 orders, ordered by id.

# --- chunk naming, for when chunks are cached as keyed assets ---
number_format: "{:04}"         # daily_0000.csv
extension: csv

# --- shared by every chunk; same semantics as Recipe.arguments / Recipe.links ---
arguments:
  sql: |
    SELECT o.id, o.total, c.name
    FROM orders o
    JOIN customers c ON c.id = o.customer_id
    WHERE o.created_at >= '2026-01-01'
    ORDER BY o.id
links:
  connection: -R/db/prod.yaml

volatile: false
expires: 1d

# --- declared, not assumed; omit when chunks may differ ---
uniform_schema:
  fields:
    - {name: id, data_type: Int, key: Id, role: {indexed: [Exact], stored: true}}
    - {name: total, data_type: Float, role: {indexed: [Range], fast: true}}
    - {name: name, data_type: Text, role: {indexed: [FullText], stored: true}}

# --- exactly one of `chunks:` or `template:` ---

chunks:                        # explicit: enumerable, ChunkList::Known
  - query: ns-sql/sql_query-0-1000
  - query: ns-sql/sql_query-1000-1000
    title: Second batch        # per-chunk overrides are allowed
    arguments: {}              # merged over the shared ones; chunk wins

template:                      # generated: count unknown, ChunkList::Unbounded
  query: ns-sql/sql_query
  first_offset: 0
  step: 1000
  batch_size: 1000
```

Fields that are **never** written by hand, mirroring `recipes.yaml`: `cwd`,
`has_circular_dependencies`, `circular_dependency_key`. A provider supplies them.

## 5. The rule that is easy to get wrong: identity lives in the query

A tempting simplification is to put the per-chunk offset in `arguments` and let every chunk share one
query. **It does not work**, and the reason is worth stating where an implementer will see it.

Liquers keys assets and cache entries by **query**. Two chunks whose queries are identical are the
same asset, whatever their arguments say — so they would collide, and the second would return the
first's data.

| Value | Where it goes | Why |
|---|---|---|
| Varies per chunk (offset, limit, a key range) | **the query** | the query is the chunk's identity |
| Shared by all chunks, and unqueryable (SQL text, a connection link) | **`arguments` / `links`** | no escaping, and no effect on identity |

### A consequence for the commands a manifest drives

Query parameters are **positional**; `arguments` overrides **by name**. So a command intended for
chunked use must put its per-chunk parameters **first**:

```rust
// Good: offset and limit are positional in the query, sql arrives by name from `arguments`.
fn sql_query(state, offset: i64 = 0, limit: i64 = 0, sql: String = "", context) -> result
//           ns-sql/sql_query-1000-1000  +  arguments: {sql: "SELECT …"}

// Bad: sql first forces an empty positional placeholder in every chunk query.
fn sql_query(state, sql: String, offset: i64 = 0, limit: i64 = 0, context) -> result
//           ns-sql/sql_query--1000-1000
```

Both parse, but the second is unreadable and depends on an empty parameter being overridden. Worth a
line in the guide.

## 6. Reading a manifest as a record source

Two forms, both of which validate at plan level today:

```
-R/data/sales/daily.manifest.yaml/-/records   the manifest as a value, converted to a source
-R/data/sales/daily_0010.csv                  a chunk, once a provider serves them as assets
```

**A folder denotes a stream only when unambiguous.** With exactly one `*.manifest.yaml` a folder may
stand for it; with several, the manifest must be named. A command taking a folder should fail with an
error listing the candidates rather than picking one.

## 7. What this format has to avoid foreclosing

The provider is deferred, so the format's job is to leave the doors open. Checked:

| Later capability | Provided for by |
|---|---|
| A generative recipe provider reading the manifest | `template`, plus `number_format`/`extension` giving the chunk-name pattern to match `contains` against |
| Cached chunks as keyed assets | the folder-as-`cwd` rule; nothing else needed |
| Unknown chunk counts | `template` without a count; `chunks` and `template` are exclusive, so a reader always knows which it has |
| Several streams per folder | the filename prefix, and the coexistence rule |
| Hand authoring | plain YAML, no generated fields, `cwd` absent |
| Machine authoring | a command writes the same file; a growing `chunks` list is a resumable record |
| Non-uniform chunks | `uniform_schema` optional |

The one thing deliberately **not** provided for is a manifest describing chunks in *another* folder.
Keeping a stream within one folder is what makes cleanup possible (delete the folder) and what keeps
the `cwd` rule honest.

## 8. Open points

1. **`expires` on a manifest** — does it bound the stream, each chunk, or both? Recipe semantics say
   it bounds the asset the recipe produces, so per chunk is the consistent reading. Worth confirming
   when chunks become assets.
2. **Per-chunk `links`** are allowed by symmetry with `arguments`, but no use case has appeared. They
   may be worth forbidding until one does.
3. **A `version` bump policy.** The field is present so an incompatible change is detectable; nothing
   yet says what a reader does with an unknown version. Refusing is the safe default.
4. **Validation.** `liquers-validate` already checks a `recipes.yaml`; a manifest wants the same
   treatment — that each chunk query plans, that `arguments` names exist in the last action, and that
   no chunk name collides with a sibling `recipes.yaml`.
