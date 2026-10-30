# Phase 1: High-Level Design — recipe-provider-chain-config

## Purpose

Make the recipe provider chain the thing a configuration document describes. Today a document
picks one built-in provider by keyword (`recipes: default | trivial`), a build installs it as the
chain's base, and other providers are appended in code. A configured build therefore loses the
providers its crate added (the manifest provider), and a provider from an integration cannot be
placed in the chain from a document. Every environment should hold a chain, and the document
should say which recipe sources apply and in what order ("make for data",
`specs/reference/VISION.md`).

## Problem Example

A `liquers-lib` build with `records`, a store holding `data/recipes.yaml` (key `triple.txt`) and
`data/chunks.manifest.yaml` (chunk `greet.csv`), configured with:

```yaml
store: { stores: [ { type: memory, prefix: data } ] }
# no recipes: key, or recipes: default
```

Today `-R/data/triple.txt` evaluates and `-R/data/greet.csv` fails `AssetNotFound`: applying a
document always installs the bare `recipes.yaml` provider in place of `liquers-lib`'s
`[recipes.yaml, manifest]`, even when the document says nothing about recipes. `liquers-web` loses
manifests the same way through its setter, and Python refuses `recipes: trivial`. There is no way to
write "recipes.yaml, then my Python provider, then manifests". Should be:

```yaml
recipes:
  - recipes_yaml                       # recipes.yaml through the store
  - { type: python, object: notes }    # an integration's provider (liquers-py M4)
  - manifest                           # record manifests (liquers-lib with records)
```

Both keys evaluate; `recipes: []` resolves nothing; no `recipes:` gives the crate's default list.

## Scope and Acceptance Criteria

- **AC-1** The list is the chain
  WHEN a document lists provider entries under `recipes`
  THEN the environment asks exactly those providers, in that order; the first that has a recipe
  for the key makes it
- **AC-2** An empty list resolves nothing
  WHEN `recipes: []`
  THEN no key resolves through a recipe
- **AC-3** Absent means the crate's configured default
  WHEN a document has no `recipes` key
  THEN `liquers-lib` with `records` gets `[recipes_yaml, manifest]` and the Problem Example's
  manifest chunk evaluates; `liquers-core` gets `[recipes_yaml]`
- **AC-4** Existing documents keep their meaning
  WHEN a document says `recipes: default` or `recipes: trivial` (aliases `none`, `no_recipes`)
  THEN it behaves as AC-3 and AC-2 respectively, and an unknown keyword is still a parse error
- **AC-5** A provider from another crate is named in the document
  WHEN an entry names a provider type that the building crate supplies (`manifest` from
  `liquers-lib`)
  THEN that provider is at its place in the list
- **AC-6** An unresolvable entry fails the build
  WHEN an entry names a type nothing supplies, or one unavailable in this build (`manifest`
  without `records`)
  THEN the build fails with a configuration error naming the entry, the known types and, if
  unavailable, the responsible feature
- **AC-7** One recipe, any order
  WHEN exactly one provider in the list has a recipe for a key and the others answer "no recipe"
  THEN that recipe is used whatever the order of the list
- **AC-11** Order decides only conflicts and failures, as documented
  WHEN two providers have a recipe for the same key, or an earlier provider fails for it (a
  malformed `recipes.yaml`)
  THEN the outcome follows the documented rule of question 5, the same for every order where the
  rule says so
- **AC-8** Code builds the same chains
  WHEN code builds an environment without a document, through the existing setters or a new list
  form
  THEN it evaluates as the equivalent document does; existing setters keep compiling and meaning
  what they mean today
- **AC-9** Configured builds in the bindings
  WHEN `lq.Environment(config=…)` or a `liquers-web` environment with `records` is built with any
  `recipes` value from AC-1 to AC-4
  THEN it gets that chain: Python stops refusing `trivial`, both keep manifests on the default
- **AC-10** The document round-trips
  WHEN a configuration is serialized and parsed again
  THEN it describes the same chain, in one canonical form

- **AC-12** Named entries and parameterized entries are the same thing
  WHEN an entry is written as a bare name (`manifest`) or as `{type: manifest}`
  THEN both build the same provider; the map form is needed only to pass arguments
  (`{type: python, object: notes}`), and a crate registers the names it supplies
Non-goals: the Python and JavaScript recipe-provider adapters (liquers-py M4 RECIPE; this design
gives integrations the way to supply entry types such as `python`); per-folder chains; validating
recipes from non-standard providers (`VALIDATE-CANNOT-SEE-NON-STANDARD-RECIPE-PROVIDERS`).

Migration: documents without `recipes:` gain manifest support in `liquers-lib` builds
(`liquers-agent` builds without `records`, so it is unaffected).

## Core Interactions

- Environment construction: the configuration document, the builder, the environment kind.
- Recipes: the provider chain, the built-in providers, the manifest provider.
- Bindings: `liquers-py` and `liquers-web` configured builds.

## Crate Placement

`liquers-core`: the document shape, the way a crate supplies entry types, the `recipes_yaml` entry,
the chain-always environment. `liquers-lib`: the `manifest` entry and its default list.
`liquers-py`, `liquers-web`: pass the configured list. Dependency flow unchanged.

## Documentation Intent

- Reference: extend `ENVIRONMENT_CONFIG.md` (`recipes`, the chain), `RECIPES_AND_PLANS.md`
  §Composing providers, `PYTHON_INTEGRATION.md` §3, `JAVASCRIPT_INTEGRATION.md` (recipes).
- Guide: extend `ENVIRONMENT_CONSTRUCTION_GUIDE.md`, `PYTHON_GUIDE.md` §4,
  `LANGUAGE-INTEGRATION_GUIDE.md` §RECIPE (supplying an entry type).
- Other documents: none.
- Documents to update: `liquers-py/README.md` `ENVIRON` row, `liquers-web/README.md`,
  `specs/README.md`.

## Open Questions

1. **Proposed resolution — names.** The `recipes.yaml` entry is `recipes_yaml`, and `default`
   means "the kind's configured default list", both as the scalar and as a list entry that splices
   that list in place (`[default, {type: python, …}]`). This avoids `default` meaning one provider
   in a list but two as a scalar. Alternative: entry `default` = the `recipes.yaml` provider only.
2. **Open design — two defaults.** The unconfigured builder resolves no recipes in core
   (`[]`), while a document without `recipes:` resolves `recipes.yaml`. Recommended: keep both,
   since collapsing either changes existing builds; each kind states both lists.
3. **Proposed resolution — document vs code.** When a document gives `recipes`, it is the whole
   chain; code contributes providers by registering entry types or named objects that the document
   refers to (as store objects are). Code appending after the configured list keeps working.
4. **Proposed resolution — how a crate supplies entry types.** A factory chain mirroring the store
   factories (declared type names with arguments and availability, first claim wins), so AC-6 can
   name a missing feature. Where it is passed is Phase 2.
5. **Open design — conflicts and failures.** Today the chain uses the first provider that has a
   recipe, and an error from an earlier provider stops it. That makes two cases order-dependent:
   a key both `recipes.yaml` and a manifest define resolves from `recipes.yaml` in
   `[recipes_yaml, manifest]` but fails in `[manifest, recipes_yaml]` (the manifest provider refuses
   the clash itself), and a malformed `recipes.yaml` blocks keys a later provider could make.
   Recommended: a key two providers define is a configuration error in any order (the manifest
   provider's existing rule, made the chain's rule); a failing provider still stops the chain,
   since skipping it hides a broken recipe. A duplicate entry (`[manifest, manifest]`) is a
   configuration error.
6. **Blocking, owner decision — the two existing designs.** This design absorbs
   `CONFIGURED-RECIPE-CHOICE-DROPS-MANIFEST-PROVIDER` and `PY-RECIPE-PROVIDER-NOT-CONFIGURABLE`,
   and `kind-aware-recipe-provider-choice` and `py-recipe-provider-choice` become `superseded`.
   Recorded at this gate; the front-matter of all four is updated when it is accepted.
7. **Implementation detail — the chain contract.** "No recipe" means not mine; an error means the
   provider owns the key and failed (AC-7, AC-11). The chain's own doc table, which says `contains` goes
   through `recipe_opt`, is corrected to the code.

## Design Dependencies

- `kind-aware-recipe-provider-choice`, `py-recipe-provider-choice` — **covered by** this design if
  question 6 is accepted.
- `store-factories-in-core`, `record-streams` (complete) — the factory model and the chain; E4.
- `records-recipe-validation-cli`, `validate-through-recipe-providers` — **overlaps**: both build
  the environment's chain to validate it, which then has one definition.
- liquers-py M4 RECIPE (plan, Pre-M4) — **required-by**: the Python provider is an entry type.

## References

- Source `RECIPE-PROVIDER-CHAIN-NOT-CONFIGURABLE`; proposed absorbed issues in question 6.
- `STORE_CONFIG_FSD.md` §"Building stores: the factory model".
