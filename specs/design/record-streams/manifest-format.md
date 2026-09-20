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
  # A link is a QUERY, so the statement lives in a proper .sql file — and the chunk
  # then *depends on* it, so editing the file expires the chunks. See §5c.
  sql: -R/queries/daily_orders.sql
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

## 5. Identity: two regimes, and per-chunk arguments belong to only one

Liquers keys assets and cache entries by **query**. So for a stream whose chunks are *not* stored,
two chunks with identical queries are the same asset whatever their arguments say — they collide,
and the second returns the first's data.

But a **keyed** stream has a second source of identity: the chunk's key. `data_0010.csv` and
`data_0011.csv` are distinct assets because their *filenames* differ, exactly as two entries in a
`recipes.yaml` are distinct because they name different files — and there, arguments and links vary
freely per entry.

| | Unkeyed stream (no `ChunkCache`) | Keyed stream (chunks stored in a folder) |
|---|---|---|
| Chunk identity | the **query** | the **key** |
| Per-chunk `arguments` / `links` | **not usable** — chunks would collide | **usable**, as in `recipes.yaml` |
| Per-chunk values must live in | the query | the query **or** the arguments |

**This is a validation rule, not a convention:** a manifest using per-chunk `arguments` or `links`
is valid **only** with a `ChunkCache`. Without one it must be rejected, because the failure is
silent — chunks quietly aliasing rather than erroring.

For a stream that has no cache, the earlier guidance still stands:

| Value | Where it goes | Why |
|---|---|---|
| Varies per chunk (offset, limit, a key range) | **the query** | the query is the chunk's identity |
| Shared by all chunks, and unqueryable (SQL text, a connection) | **`arguments` / `links`** | no escaping, and no effect on identity |

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

## 5c. Links are how a statement lives in a file

A `link` binds a parameter to a **query**, which is what makes it the right home for a SQL statement
rather than `arguments`:

```yaml
links:
  sql: -R/queries/daily_orders.sql
```

Three things follow, and the second is the one that matters:

1. **The statement lives in a `.sql` file** — editable with syntax highlighting, diffable, and not
   escaped into YAML.
2. **The chunk depends on that asset.** A link is evaluated, so the dependency manager records it;
   editing the statement expires every chunk derived from it, through the same cascade §5a relies on.
   An inline `arguments.sql` gives no such edge at that granularity — changing it means editing the
   manifest, which invalidates the whole stream rather than what actually changed.
3. **One statement can serve several manifests**, edited in one place.

`arguments` remains right for a short literal, or for a value with no natural home as an asset.

## 5a. `expires` is per chunk

**Settled.** A manifest's `expires` bounds **each chunk**, not the stream, which is the reading
consistent with `Recipe::expires` — it bounds the asset a recipe produces, and each chunk is such an
asset.

Nothing further is needed for a consumer that reads the whole stream. An asset processing every chunk
**records a dependency on every chunk**, and `dependencies.rs` performs a transitive cascade
(`expire_stale_dependents`, `ExpiredDependents`), so expiring any one chunk expires everything derived
from the stream. Stream-level expiration would be a second mechanism computing what the first already
computes.

**One caveat, because it ties this to a deferred feature.** The cascade works through *recorded
dependencies*, and a dependency is recorded when a chunk is an asset in its own right — the
`ChunkCache` case. A stream consumed inside a single command without cached chunks is one asset with
one expiration, and per-chunk expiry has nothing to act on. That is the correct behaviour for an
uncached stream, and it means `expires` becomes fully meaningful only once chunks are keyed assets.

## 5b. A templated manifest hydrates in the command, not in the manifest

For a `template`, **`arguments` are identical for every chunk**. The SQL statement is passed through
unchanged, and the command hydrates it with the chunk's offset.

This does not contradict §5 — it is the same rule seen from the other side:

| | Where | Varies per chunk? |
|---|---|---|
| offset, limit | the **query** (positional) | yes — and so it is identity |
| the SQL statement | **`arguments`** | no — one value, shared |

So the manifest holds exactly one copy of the statement, chunk queries differ only in their offset,
and chunk identity stays well defined.

**The manifest performs no string interpolation, and this is a deliberate boundary.** It passes the
`sql` argument through verbatim; expanding it is the command's business. That keeps the manifest a
pure data format with no templating syntax to specify, no escaping rules, and — decisively — **no
injection story**. A manifest that substituted text into SQL would have to answer for what happens
when a field contains a quote.

Leaving hydration to the command also allows the right answer, which is not textual at all:

```sql
-- The statement in `arguments`, with bind placeholders the command supplies values for.
SELECT o.id, o.total FROM orders o ORDER BY o.id LIMIT $1 OFFSET $2
```

Bound through sqlx, this is parameterized rather than interpolated: injection-safe by construction,
and the database can reuse the prepared plan across chunks.

**A convenience worth allowing and documenting, not defaulting to:** a command may instead accept a
bare `SELECT` and append `LIMIT n OFFSET m` itself. Simpler to author, and it breaks on statements
where appending is wrong — one that already ends in `LIMIT`, a `UNION` where the clause binds to the
last branch rather than the whole, or a CTE whose outer query is not where the author expects. A
command should say which contract it implements.

**Per-chunk `arguments` are therefore unused by templates.** They remain available for a
heterogeneous explicit `chunks:` list, where chunks may genuinely draw on different sources.

## 5d. The explicit form *is* a `RecipeList` — a unification worth taking

An explicit chunk entry carries a query, arguments, links, a title, a description, `volatile` and
`expires`, and it names the asset produced by its query's filename. **That is `Recipe`, field for
field.** The resemblance is not an analogy to be noted and moved past; it says the two should be one
type.

So the chunk list is literally a `RecipeList`:

```
manifest  =  stream header  +  RecipeList
```

where the header is the part `recipes.yaml` has no concept of — `number_format`, `extension`,
`uniform_schema`, and `template` — and the `RecipeList` is the chunks.

**What this buys, all of it for free:**

| | Because |
|---|---|
| No new parsing | `Recipe` and `RecipeList` already deserialize |
| `arguments`, `links`, `volatile`, `expires` semantics | inherited exactly, not re-specified |
| Planning | `Recipe::to_plan` works unmodified on a chunk |
| Validation | whatever checks a `recipes.yaml` checks a manifest's chunks |
| "A folder with a manifest is like a folder with `recipes.yaml`" | **literally true**, not merely similar |

`RecipeList` documents its recipes as being "in file order", so the sequence a stream needs is
already guaranteed.

**What a manifest still adds** over a bare `RecipeList` is exactly the header: the chunks are an
*ordered sequence belonging to one stream* rather than independent assets, plus the naming pattern
and the optional template for chunks that do not exist yet.

### The further unification, recorded but not taken

If an explicit manifest is a `RecipeList` plus a header, then a plain `recipes.yaml` could in
principle be *read* as a record stream by selecting the recipes whose filenames match
`<prefix>_<digits>.<ext>` and ordering them by number. No new file, no new format — a stream would be
a naming convention over ordinary recipes.

Attractive, and not taken, for two reasons: a stream would then have nowhere to declare
`uniform_schema` or a `template`, and its existence would be implicit in filenames rather than
stated. The explicit header is worth its file. The possibility is recorded because it suggests the
right shape if `RecipeList` ever grows an optional `streams:` section — one file declaring several
streams over its own recipes — which is the version of this idea that would not lose anything.

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

1. ~~Does `expires` bound the stream or each chunk?~~ **Settled: per chunk** — see §5a.
2. **Per-chunk `links`** are allowed by symmetry with `arguments`, but no use case has appeared —
   and §5b removes the templated case from consideration, since a template's arguments are shared by
   construction. Worth forbidding until a heterogeneous explicit list needs them.
3. **A `version` bump policy.** The field is present so an incompatible change is detectable; nothing
   yet says what a reader does with an unknown version. Refusing is the safe default.
4. **Validation.** `liquers-validate` already checks a `recipes.yaml`; a manifest wants the same
   treatment — that each chunk query plans, that `arguments` names exist in the last action, and that
   no chunk name collides with a sibling `recipes.yaml`.
