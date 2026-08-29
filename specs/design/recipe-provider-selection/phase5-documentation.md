Reconciles the delivered behaviour with the repository's documentation.

# Phase 5 — Documentation

## Current documented behaviour

`liquers-core::recipes::RecipeProviderChoice` names the two built-in recipe providers so a
configuration document can select one as data:

- `default` → `DefaultRecipeProvider`, which reads `<directory>/recipes.yaml` through the
  environment's store.
- `trivial` → `TrivialRecipeProvider`, which resolves no recipes at all. `none` and `no_recipes`
  are accepted as input spellings of the same choice; serialization always emits `trivial`.

`Default` is the `#[default]` variant, so a document that omits the field gets working recipes.
The rustdoc states that this is the *document* default and is deliberately not the same as an
environment constructor's unconfigured default, which is per crate — `environment-builder`'s
decision that `EnvironmentBuilder::new()` starts trivial is unaffected.

`provider()` yields `Arc<dyn AsyncRecipeProvider<E>>` and `boxed_provider()` yields `Box<…>`,
matching the `liquers-lib` and `liquers-core` setter shapes respectively; the `E: Environment`
bound sits on the methods, so the type itself has no type parameter and can be embedded in a
future `EnvironmentConfig`. `FromStr` accepts the same names as `Deserialize` and reports an
unknown one as an `Error`; `Display` emits the canonical name.

The set is **closed**, by maintainer decision at the gate: custom providers are too varied to
standardize, so a host with its own `AsyncRecipeProvider` continues to pass the value to the
environment directly. There is no registration hook and no `RecipeProviderFactory`. Phase 2 records
the technical obstacle should that ever be revisited — `AsyncRecipeProvider` is generic in `E`, so
a name-keyed factory is not object-safe the way `StoreFactory` is.

Nothing in the workspace consumes the type yet; that is the Phase 1 scope, and its consumers are
`STORE-CONFIG-IN-CORE`'s configuration document and the `environment-builder` design.

## Maintenance performed

- `specs/reference/api/DOC_08_RECIPES_PLANS.md` — a "Selecting a provider by name" subsection under
  the provider contract, with `reviewed:` bumped to 2026-08-29 and a History row.
- `specs/issues/RECIPE-PROVIDER-BY-NAME.md` — `status: closed` with a resolution note.
- `specs/README.md` — capability line for this design, moved to complete.
- `specs/index.csv` — regenerated.

This matched the Phase 1 documentation assessment: small maintenance only, no new reference or
guide.

## Proposed follow-up documentation

None. One documentation *observation* is worth recording rather than proposing: the issue itself
notes a tension between its `P0` and `DOCS_STRUCTURE_GUIDE.md` §4.4, which reserves P0 for
incorrect results, data loss, a panic or a broken documented feature. Resolving that is a change to
the priority vocabulary, not to this issue, and is left to the maintainer.
