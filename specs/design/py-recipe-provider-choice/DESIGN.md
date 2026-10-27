---
id: PY-RECIPE-PROVIDER-CHOICE
kind: design
title: A Python environment accepts every configured recipe provider choice
form: compact
status: in_review
phase: implementation
readiness: needs-decision
autofix: not-eligible
area: [py]
issues: [PY-RECIPE-PROVIDER-NOT-CONFIGURABLE]
created: 2026-10-27
---
# A Python environment accepts every configured recipe provider choice

Bulk design (`specs/guides/autonomous_bulk_design.md`), 2026-10-27: Phases 1-4 written, not
approved, nothing implemented.

## Phase 1: High-Level Design

### Purpose

`lq.Environment(config=…)` refuses any `recipes` value other than `default`, although
`EnvironmentConfig` documents `default` and `trivial` (`reference/ENVIRONMENT_CONFIG.md`) and the
builder accepts both. This design makes the Python handle honour the choice, without losing
`liquers-lib`'s manifest recipe provider on the default path.

### Problem Example

Verified at HEAD (`04667a80`):

```python
env = lq.Environment(config={"recipes": "trivial"})
# today: ConfigurationError "a recipe provider other than the default cannot be configured from
#        Python yet" (EnvCell::new, liquers-py/src/environment.rs)
# should: builds; with a filesystem store holding data/recipes.yaml
#         ("recipes:\n  - query: hello/repeat-3/triple.txt\n"), env.evaluate("-R/data/triple.txt")
#         raises AssetNotFoundError, where the default configuration returns "hellohellohello"
```

`EnvCell::configured_builder` never passes `config.recipes` to the builder, so the provider is
`LibKind::default_recipe_provider` — `[DefaultRecipeProvider, ManifestRecipeProvider]`, since
`liquers-py` links `liquers-lib` with `records`. Passing the choice naively,
`with_recipe_provider_choice(Default)`, would replace that chain with the bare `recipes.yaml`
provider and silently stop manifest chunk keys resolving
(`CONFIGURED-RECIPE-CHOICE-DROPS-MANIFEST-PROVIDER`).

### Scope and Acceptance Criteria

- **AC-1** `trivial` is accepted and resolves nothing
  WHEN `lq.Environment(config={"recipes": "trivial"})` (or the alias `none`) evaluates a key only a
  `recipes.yaml` entry or a manifest could make
  THEN it raises `lq.AssetNotFoundError`
- **AC-2** `default` keeps recipes and manifests
  WHEN the configuration says `recipes: default`, or nothing about recipes
  THEN a `recipes.yaml` key and a manifest chunk key both evaluate, as today
- **AC-3** Validation follows the configured provider
  WHEN `env.validate()` runs on a `trivial` environment
  THEN it reports no issue caused by the choice and builds nothing
- **AC-4** An unknown value is still refused
  WHEN `recipes` names no `RecipeProviderChoice`
  THEN the constructor raises `lq.LiquersError`, as today

Non-goals: a Python-defined recipe provider (the guide's `RECIPE` feature); provider chains in
configuration (deliberately absent, `RecipeProviderChoice` doc); changing what `trivial` means for
`liquers-lib` (`kind-aware-recipe-provider-choice`).

### Design Readiness

- **Readiness:** needs-decision
- **Automatic fixing:** not-eligible — a feature, not a fix: the documented Python contract is the
  refusal (`PYTHON_INTEGRATION.md` §3 "refuses … any `recipes` other than `default`", guide §4), so
  accepting `trivial` changes what the binding accepts (rules 2 and 4); and its readiness is not
  `ready` (rule 5).
- **Leading issue:** question 1.
- **Explanation:** the fix is a two-arm match in one function; what remains is whether to support
  `trivial` at all, and the meaning of `trivial` under `liquers-lib`, which another design owns.
- **Open questions:**
  1. **Proposed resolution - support or document:** support every `RecipeProviderChoice`
     (recommended): there are two, the builder takes both, and `trivial` is useful for a sandbox
     where only explicit queries run. The issue's alternative, listing `trivial` as unsupported in
     the guide, keeps today's code and changes only documents.
  2. **Proposed resolution, carried from `kind-aware-recipe-provider-choice` question 2:**
     `trivial` under `liquers-lib` means no recipes at all, manifests included (AC-1). Python
     follows whatever the builder's `with_recipe_provider_choice(Trivial)` installs, so a different
     answer there changes AC-1's manifest half and the guide sentence, not this code.
  3. **Implementation detail - `Default` is not passed to the builder:** the builder's default is
     already the right chain; see Phase 2.

### Design Dependencies

- `kind-aware-recipe-provider-choice` (needs-decision, Phase 4 finished) — **overlaps**. Overlap
  test **T2** holds in part: what `default` and `trivial` install for `LibKind` is that design's
  contract. Not attached: that design is excluded from this run (bulk-design guide §1, finished
  Phase 4), and this design does not need it, because it never names `Default` to the builder.
  Once that design lands (either option), the `Default` arm may become a plain pass-through; no
  order is forced.
- `py-command-decorator-declaration-keywords`, `py-api-reference` — **overlaps**: other edits in
  `liquers-py/src/environment.rs`, different functions. No order.

### Consolidated Findings

- The refusal is one `if` in `EnvCell::new`; the choice is consumed only in
  `EnvCell::configured_builder`, which both `build` and `validate` use, so AC-3 needs no extra code.
- Passing `Default` through would regress manifests in every Python environment; the explicit
  match is what prevents it, and a test pins it (AC-2).
- Documents state the refusal in three places (reference §3 and §8, guide §4 table and §10,
  `liquers-py/README.md` `ENVIRON` row); all change.

## Phase 2: Architecture

### Solution

1. `liquers-py/src/environment.rs` `EnvCell::new`: delete the `config.recipes !=
   RecipeProviderChoice::default()` refusal.
2. `EnvCell::configured_builder`, after `with_asset_manager_options`:

   ```rust
   let mut builder = match self.config.recipes {
       // The builder's default is liquers-lib's chain, manifests included; naming `Default`
       // would replace it with the bare recipes.yaml provider.
       RecipeProviderChoice::Default => builder,
       RecipeProviderChoice::Trivial => {
           builder.with_recipe_provider_choice(RecipeProviderChoice::Trivial)
       }
   };
   ```

   An explicit match with no `_` arm, so a new choice is a compile error here.
3. The unit test `a_non_default_recipe_provider_is_refused` becomes
   `a_trivial_recipe_provider_is_accepted`.

Rejected: `with_recipe_provider_choice(self.config.recipes)` for every value (drops manifests);
`with_recipe_provider_choice(choice).with_records_recipe_provider()` (re-adds manifests to
`trivial` too, contradicting its doc, and duplicates `LibKind`'s chain definition).

### Changes

- `liquers-py/src/environment.rs`: `EnvCell::new`, `EnvCell::configured_builder`, `mod tests`.
- No `liquers-core` / `liquers-lib` change, no command, registry or config-schema change.
- Documents: `specs/reference/PYTHON_INTEGRATION.md` §3 (construction sentence; "The recipe
  provider is the builder's default chain" → per choice) and §8 limitation removed;
  `specs/guides/PYTHON_GUIDE.md` §4 `config` row and lifecycle sentence, §10 limitation removed;
  `liquers-py/README.md` `ENVIRON` row. History rows and `reviewed:` bumps.

### Risks

| Risk | Mitigation | Certainty |
|---|---|---|
| Manifests lost on the default path | `Default` arm leaves the builder alone; AC-2 test with a manifest chunk | high |
| `kind-aware-recipe-provider-choice` decides `trivial` keeps manifests | Python follows the builder; only AC-1's manifest assertion and one guide sentence change | medium |
| A filesystem-stored manifest reads differently from the memory-store Rust fixture | AC-2 test uses the filesystem store, the common Python setup | medium |

Recovery: revert the file. Tests likely to change: the refused-choice unit test only.

## Phase 3: Examples and Tests

### Examples

The Problem Example is the primary case. Secondary: the same filesystem store with
`data/chunks.manifest.yaml` = `manifest: record-stream`, chunk `greet/greet.csv`, and a Python
`first_command` `greet` returning `"hello"`: `-R/data/greet.csv` is `"hello"` by default and
raises `lq.AssetNotFoundError` with `trivial`.

### Tests

In `liquers-py/tests/test_ENVIRON_environment.py` (`tmp_path` filesystem store, commands registered
on the environment under test):

- `test_ENVIRON_trivial_recipes_resolve_nothing` — `{"recipes": "trivial"}` and `"none"`: the
  `recipes.yaml` key and the manifest chunk key raise `lq.AssetNotFoundError`; a plain query still
  evaluates: AC-1
- `test_ENVIRON_default_recipes_keep_recipes_and_manifests` — no `recipes` key, and
  `{"recipes": "default"}`: both keys evaluate: AC-2
- `test_ENVIRON_validate_accepts_a_trivial_provider` — `validate()` reports no error-level issue:
  AC-3
- `test_ENVIRON_an_unknown_recipe_choice_is_refused` — `{"recipes": "bogus"}` raises
  `lq.LiquersError`: AC-4

In `liquers-py/src/environment.rs` `mod tests`: `a_trivial_recipe_provider_is_accepted` —
`EnvCell::new` with `Trivial` is `Ok`: AC-1.

Run: `bash scripts/test-liquers-py.sh -k ENVIRON`.

## Phase 4: Implementation Plan

### Steps

- [ ] 1. `liquers-py/src/environment.rs` `EnvCell::new` — remove the refusal; `mod tests` — rename and invert the unit test — proof: `bash scripts/test-liquers-py.sh -k ENVIRON` (its Rust half runs `a_trivial_recipe_provider_is_accepted`) (AC-1)
- [ ] 2. Same file `EnvCell::configured_builder` — the explicit match — proof: `bash scripts/test-liquers-py.sh -k "recipes or recipe_choice or trivial_provider"` (AC-1, AC-2, AC-3, AC-4)
- [ ] 3. Documents listed in Phase 2 §Changes, History rows, `reviewed:` — proof: `python3 scripts/docs_index.py --check`
- [ ] 4. `specs/issues/PY-RECIPE-PROVIDER-NOT-CONFIGURABLE.md` `status: closed` with a resolution note; `python3 scripts/docs_index.py` — proof: `python3 scripts/docs_index.py --check`

Rollback: steps 1-2 are one file; revert it.

### Validation

`bash scripts/test-liquers-py.sh` (whole suite and the Rust unit tests),
`python3 scripts/docs_index.py --check`. The diff touches `liquers-py/src/environment.rs`,
`liquers-py/tests/`, the listed documents and the issue file only.
