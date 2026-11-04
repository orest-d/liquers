# Recipes and pipelines: which structure holds what

Analysis for the maintainer's review of 2026-11-03 (questions Q26–Q30). It compares ways to relate
`Recipe` and `Pipeline` and recommends one. Earlier context:
[`pipeline-structure-and-sources.md`](./pipeline-structure-and-sources.md) §4–5.

## 1. What `Recipe` is today

`Recipe` (`liquers-core/src/recipes.rs`) is more than a file format. It is the **description of an
asset**, and it travels everywhere:

- Every recipe provider returns it (`AsyncRecipeProvider::recipe` / `recipe_opt`), including
  Python providers (`liquers-py/src/recipe_provider.rs`) and the records `ManifestRecipeProvider`.
- The asset manager holds one per asset (`assets.rs`, `AssetData::recipe`). A query or a key
  becomes one (`From<Query> for Recipe`, `From<Key> for Recipe`), so an unkeyed asset is a recipe
  with only a query.
- It is a value (`Value::Recipe`, `ValueInterface::from_recipe`), read by `-R-recipe/key`, stored
  as `recipes.yaml`, and exposed in Python and JavaScript.
- About 48 source files name it.

Any structural change pays for each of these: serialization, `Clone` / `PartialEq` / `Debug`, the
asset manager, providers, values and bindings.

## 2. The options

### A. Extend the classic recipe (the maintainer's Q26 proposal)

```yaml
- pipeline: [ {value: {...}}, {action: prepare, arguments: {config: {...}}} ]   # runs first
  query: "make_report-x/report.html"                                            # then the query
  arguments: { title: "Q3" }       # as today: the query's last action only
```

The rules:

- With both present, the pipeline runs first and the query continues from its state.
- A filename in the pipeline is an error when there is a query, because the query owns the filename.
- `arguments` and `links` override the query's last action, as today. Without a query they are an
  error.
- A placeholder anywhere is an error.
- `Recipe::to_pipeline` concatenates the pipeline, then the query's elements, with `arguments` and
  `links` as named arguments of the last action.

Critical evaluation:

- **For.** It is additive: one optional field, every file and every API unchanged, and no
  migration. Running the pipeline first gives what the maintainer wanted on 2026-11-01: the input
  state can be initialized (a `value`, or a `resource` read with parameters) and then processed by
  an ordinary query. Keeping `arguments` and `links` tied to the query preserves their meaning
  exactly.
- **Dead pipeline.** A query that starts with a resource (`-R/…`) ignores its input state, so a
  pipeline before it would run for nothing. Make this a validation **error**. Only a query that
  starts with an action can follow a pipeline. This is the main trap in the proposal, and a check
  closes it.
- **Two ways to write one thing.** A pipeline can end with query text (§3), so
  `{pipeline: P, query: Q}` and `{pipeline: P + Q}` mean the same. Both are fine as input. The
  canonical form (what an editor writes back) should keep the author's split rather than merge
  them, so that `arguments` and `links` keep their target.
- **Asset identity.** A recipe with a pipeline that is not representable cannot be keyed by a query.
  This does not matter for a provider recipe, which is keyed by its key, but an unkeyed asset with
  such a recipe cannot be shared or cached (Q29). The asset manager needs a flag for this, and it
  already treats recipes with `arguments`/`links` the same way (its action `query` is `None`).
- **Filename.** With a query, the filename comes from the query as today. With a pipeline only, it
  comes from `Pipeline::filename`. `Recipe::filename()` checks both, so listing and lookup work
  for pipeline-only recipes.

### B. An enum body (the earlier recommendation)

`Recipe { body: RecipeBody::{Query{…} | Pipeline(P)}, … }`. "Query or pipeline" is enforced by the
type, but it cannot express A's "pipeline, then query" without nesting the query into the pipeline,
and it breaks every `Recipe { … }` literal and `recipe.query` user. Once A is preferred, B buys
nothing.

### C. A recipe trait

```rust
trait RecipeLike: Send + Sync {
    fn plan(&self, cmr: &CommandMetadataRegistry) -> Result<Plan, Error>;
    fn filename(&self) -> Result<Option<ResourceName>, Error>;
    fn query(&self) -> Option<Query>;            // identity, when it has one
    fn declarations(&self) -> &AssetDeclarations;
}
// ClassicRecipe and PipelineRecipe implement it; providers return Arc<dyn RecipeLike>
```

- **For.** It is open to new recipe-like structures: generated chunk recipes, recipe-tree nodes
  (`RECIPES-CANNOT-DESCRIBE-A-SELF-CONTAINED-ASSET-TREE`), endpoint recipes for the web follow-up.
  Providers stop caring which kind they serve.
- **Against.** Everything in §1 needs a concrete type:
  - `Value::Recipe`, `-R-recipe/`, YAML files and the bindings must serialize and deserialize a
    `dyn` (so a registry of kinds, as `typetag` gives, or a closed enum after all);
  - `AssetData` needs `Clone` and `PartialEq` on a trait object;
  - Python providers must say which kind they return.

  This is the largest refactor of the options, across about 48 files. And every kind named so far
  is a generator that produces a recipe, not a new kind of recipe: a manifest chunk, a tree node.
- **Middle ground.** A closed `enum Recipe { Classic(ClassicRecipe), … }` behind the same methods
  gets the uniform interface without `dyn`. It is still the full rename-and-wrap refactor, and it
  has nothing to wrap until a second kind exists.

### D. A hierarchy of structures (the maintainer's Q27 idea)

Each structure has one job, and each layer adds only what it needs:

| Layer | Holds | Used by |
|---|---|---|
| **Steps**: `Vec<PipelineElement>` | elements only | a pipeline command's body, a nested pipeline argument, a manifest template |
| **`Pipeline`** | realm, rooted, header parameters, steps, filename | anything equivalent to a query: conversions, `Value::Pipeline`, editors, `evaluate` |
| **`Recipe`** (classic, extended as A) | optional pipeline + optional query + arguments/links + cwd + title, description + asset declarations | anything that defines an asset: `recipes.yaml`, providers, the asset manager, a standalone pipeline document |

This answers Q27 and Q30 together. **Title, description and the asset block belong to the recipe,
not to the pipeline.** A standalone pipeline that wants them, such as a web request, is sent as a
recipe with only a `pipeline`. The pipeline stays exactly as expressive as a query (plus structured
arguments), so its conversions and editors stay simple. A pipeline command uses just the steps
(with a realm), because a filename or title would mean nothing there. Its label and doc are already
in `CommandMetadata`.

## 3. The splice element (Q28)

`{query: "…"}`, renamed `{steps: "…"}` in the previous draft, meant **inline these query steps
here**. The text is parsed and its elements are inserted in place, continuing from the current
state, exactly as if the elements had been written out. It is not evaluated separately. `evaluate`
is the other thing:

| | splice (inline) | `evaluate` |
|---|---|---|
| input state | continues from the current one | ignored: the query runs on its own |
| identity | part of this pipeline's query, so prefixes are shared | its own asset, keyed by that query, recorded as a dependency |
| step | the query's own steps, inlined | `Step::Evaluate(query)` |
| a query starting with an action | applies to the current state | applies to nothing, which is usually an error |

Both are query evaluation in the broad sense. The difference is whether the query's steps become
part of this pipeline (inline) or run as a separate, cached asset whose result is used (evaluate).

**Recommended naming.** Drop the named splice element altogether: **a bare string in a pipeline's
element list is query text, inlined**. That is the natural reading, and it needs no name:

```yaml
pipeline:
  - "-R/data/sales.csv/-/from_csv"
  - {action: filter, arguments: {where: {region: [EU, US]}}}
  - "sort-amount/head-5"
```

`evaluate` keeps its name, for a query's or a pipeline's result as a separate asset.

## 4. Recommendation

**D, with the recipe extended as A. C is deferred.**

- `Pipeline` is the query-equivalent. `Recipe` gains an optional `pipeline` that runs before the
  optional `query`, under A's rules, with the dead-pipeline check.
- Title, description and the asset block stay on `Recipe` (Q27, Q30).
- A bare string in the element list is inlined query text (Q28).
- A non-representable pipeline is evaluated, not managed (Q29, agreed). As part of a keyed recipe,
  it is managed and cached through the key.
- **Re-evaluate C** when a structure appears that defines an asset but cannot be written as a
  `Recipe`. Introduce it then as a closed enum behind `Recipe`'s methods. Nothing seen so far needs
  it: manifests and recipe trees generate recipes.
