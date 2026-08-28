# `RECIPE-PROVIDER-BY-NAME` — Phase 1 and Phase 2

Prepared under [`guides/autonomous_issue_fixing.md`](../../guides/autonomous_issue_fixing.md) for
[`issues/RECIPE-PROVIDER-BY-NAME.md`](../../issues/RECIPE-PROVIDER-BY-NAME.md) (feature, P0 by
maintainer decision, complexity S). Recorded here rather than in a folder of its own because the
issue links to this design; nothing in this file changes the `environment-builder` phase documents
or its workflow contract.

**State: Phase 2 written, awaiting the approval gate. Nothing is implemented.**

---

## Phase 1 — High-level design

### Problem and evidence

A recipe provider is selected by constructing a Rust value. Both providers are unit structs in
`liquers-core/src/recipes.rs` — `TrivialRecipeProvider` (`:567`) and `DefaultRecipeProvider`
(`:609`) — and every selection is a literal: `env.with_recipe_provider(Box::new(DefaultRecipeProvider))`
appears at 20+ call sites across `liquers-core/src/assets.rs`, `interpreter.rs`, `plan.rs`,
`tests/manager_parametric.rs`; `liquers-web/src/environment.rs:103` calls
`with_default_recipe_provider()`. There is no function anywhere that maps a *name* to a provider.

Stores solved the same problem: `StoreConfig` carries a `type` string, and
`liquers-store/src/store_builder.rs` turns that string into a backend, with `StoreFactory` as the
extension point. Recipe providers have no equivalent, so `EnvironmentConfig`
([phase3-examples.md](./phase3-examples.md) §Scenario 4, line 338) sketches `recipes: default` with
no type behind it.

The choice is not cosmetic: `TrivialRecipeProvider::recipe` returns an error for every key, so an
environment given it fails every `-R/` query that resolves through a recipe.

### Expected behaviour and acceptance criteria

1. `liquers-core` exposes a small closed enum whose values are `default` and `trivial`, deriving
   `Serialize` and `Deserialize`.
2. It converts to `Arc<dyn AsyncRecipeProvider<E>>` for any `E: Environment`, by an **exhaustive**
   match with no `_` arm, so a third provider is a compile error.
3. `default` yields a provider that resolves a recipe from a store; `trivial` yields one that
   resolves none. Asserted by behaviour, not by type name.
4. It round-trips through both YAML and JSON, in the lowercase spelling a configuration document
   uses.
5. No existing environment, constructor or default changes behaviour.

### Affected users, workflows and systems

`core/assets` (recipes) only, at rest: nothing calls the new type until the configuration document
of `STORE-CONFIG-IN-CORE` and the `EnvironmentBuilder` exist. The consumers this unblocks are the
JavaScript (`liquers-web`) and Python (`liquers-py`) document-driven setup paths. Query, Store,
Commands and UI are untouched.

### Scope and non-goals

In scope: the enum, its conversion, its string form, and unit tests.

Explicitly **not** in this issue:

- a `RecipeProviderFactory` trait or any host-registration hook — the issue asks for the enum and
  says the factory is "only needed once a host wants to register its own provider by name";
- `EnvironmentConfig` itself, which is blocked on `STORE-CONFIG-IN-CORE`;
- changing `with_recipe_provider` signatures, environment defaults, or any call site;
- wiring the enum into `liquers-web` or `liquers-py`.

### Compatibility constraints

Purely additive. The one durable commitment is the **spelling**: once a document may say
`recipes: default`, renaming the variants breaks published configuration files. Nothing else in the
change is hard to reverse.

### Known questions and assumptions

- **Q1 — which variant is `#[default]`?** The issue's snippet marks `Default`. The
  `environment-builder` Phase 2 decision (§"The recipe-provider default is per-crate") makes
  `EnvironmentBuilder::new()` default to **`TrivialRecipeProvider`**, with `liquers-lib` supplying
  `DefaultRecipeProvider` for its own constructor. If the enum defaults to `Default`, then a
  configuration document that omits `recipes:` and a builder that is not configured disagree. This
  needs a decision; see Phase 2 §Open questions.
- Assumption: both providers stay stateless unit structs, so a value-free enum can construct either.
  Verified — neither has fields.

### Documentation assessment

Small maintenance only: `specs/reference/api/DOC_08_RECIPES_PLANS.md` describes the two providers
and would gain one sentence naming their string forms. No new reference or guide.
`specs/README.md` needs no change (the issue is already listed and its design link is unchanged).

---

## Phase 2 — Solution and architecture

### Chosen solution

A plain serde enum in `liquers-core/src/recipes.rs`, beside the two providers it names, with an
inherent method per output shape:

```rust
/// Names one of the built-in recipe providers, so a configuration document can select it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum RecipeProviderChoice {
    /// [`DefaultRecipeProvider`] — recipes read through the environment's store.
    #[default]
    Default,
    /// [`TrivialRecipeProvider`] — no recipes at all.
    Trivial,
}

impl RecipeProviderChoice {
    /// The provider this choice names, shared.
    pub fn provider<E: Environment>(self) -> Arc<dyn AsyncRecipeProvider<E>> {
        match self {
            RecipeProviderChoice::Default => Arc::new(DefaultRecipeProvider),
            RecipeProviderChoice::Trivial => Arc::new(TrivialRecipeProvider),
        }
    }

    /// The provider this choice names, owned — for `Environment::with_recipe_provider`,
    /// which takes `Box<dyn AsyncRecipeProvider<Self>>` on the four core environments.
    pub fn boxed_provider<E: Environment>(self) -> Box<dyn AsyncRecipeProvider<E>> {
        match self {
            RecipeProviderChoice::Default => Box::new(DefaultRecipeProvider),
            RecipeProviderChoice::Trivial => Box::new(TrivialRecipeProvider),
        }
    }

    /// The name used in a configuration document.
    pub fn as_str(self) -> &'static str {
        match self {
            RecipeProviderChoice::Default => "default",
            RecipeProviderChoice::Trivial => "trivial",
        }
    }
}

impl std::str::FromStr for RecipeProviderChoice { type Err = Error; /* … */ }
impl std::fmt::Display for RecipeProviderChoice { /* delegates to as_str */ }
```

Four exhaustive matches, no `_` arm anywhere, per the repository's match rule.

### Why here, and what was rejected

| Option | Verdict |
|---|---|
| Enum in `liquers-core/src/recipes.rs` | **Chosen.** The two providers, the `AsyncRecipeProvider` trait and `Environment` are all already in scope in that module; the enum adds no import and no module. |
| A new `liquers-core/src/recipe_provider_choice.rs` | Rejected: ~60 lines including tests, with a single dependency on `recipes.rs`. A module for it is overhead. |
| `RecipeProviderFactory` trait keyed by name, mirroring `StoreFactory` | Rejected for now, and there is a technical reason beyond YAGNI: `AsyncRecipeProvider` is generic in `E`, so a factory producing `Arc<dyn AsyncRecipeProvider<E>>` must be generic in `E` too, and `dyn RecipeProviderFactory` is then not object-safe — a name-keyed registry would need one registry per environment type. `StoreFactory` has no such problem because `AsyncStore` is not generic. This is worth recording: the factory route is materially harder than the store precedent suggests. |
| Putting the choice in `liquers-store` next to `StoreConfig` | Rejected: the providers live in `liquers-core`, and `STORE-CONFIG-IN-CORE` is moving configuration *down* into core, not up. |

### Exact symbols involved

- **Added** — `liquers-core/src/recipes.rs`: `RecipeProviderChoice`, its four methods, `FromStr`,
  `Display`, and a `#[cfg(test)] mod` extension.
- **Read, unchanged** — `TrivialRecipeProvider` (`recipes.rs:567`), `DefaultRecipeProvider`
  (`recipes.rs:609`), `AsyncRecipeProvider` (`recipes.rs:476`), `Environment`
  (`liquers-core/src/context.rs:150`).
- **Not touched** — every `with_recipe_provider` (`context.rs:1026`, `:1179`, `:1319`, `:1873`;
  `liquers-lib/src/environment.rs:86`), every call site, `liquers-py/src/context.rs:113`.

### Ownership, errors, sync/async

- The enum is `Copy` and field-free; the `E: Environment` bound sits on the methods, not the type,
  so the type itself stays plain serde data and can be embedded in a configuration struct with no
  type parameter. This is what makes `EnvironmentConfig { recipes: RecipeProviderChoice, … }`
  possible later without leaking `E` into the document type.
- `AsyncRecipeProvider<E>: MaybeSend + MaybeSync` (`recipes.rs:476`), and both unit structs satisfy
  it on native and on wasm, so `Arc<dyn AsyncRecipeProvider<E>>` and `Box<…>` both compile in both
  targets with no `cfg`.
- `FromStr::Err = liquers_core::error::Error`, built with `Error::general_error` — no new error
  type, no `Error::new`.
- Nothing async: the enum is data, and the providers are constructed synchronously exactly as the
  existing call sites construct them.

### API and backward compatibility

Additive only. No signature, trait or default changes; nothing existing can break at compile time
or at run time. The reversible/irreversible split is stated in Phase 1: the variant *spelling* is
the only forward commitment.

### Integration and reuse

Reuses both providers unchanged and reuses `serde`'s derive rather than hand-writing a parser. It
deliberately does **not** reuse the `StoreFactory` machinery — see the rejection table.

### Related open issues

- `STORE-CONFIG-IN-CORE` (P0) — consumer, not prerequisite. It can land in either order.
- `environment-builder` — this design's `EnvironmentBuilder` is where the choice would be applied;
  `with_recipe_provider(Arc<…>)` there matches `provider()` directly.
- `CORE-PAYLOAD-ENV-RECIPE-PROVIDER-PANIC` — unrelated mechanically, but Q1 below is the same
  "which default" question seen from the configuration side.

### Risk analysis

| Assessment | Record |
|---|---|
| **Files** | 1 source file (`liquers-core/src/recipes.rs`, ~60 lines of code plus ~60 of tests, colocated). Optionally 1 reference doc (`specs/reference/api/DOC_08_RECIPES_PLANS.md`) plus its `reviewed:`/History row, and the regenerated `specs/index.csv`. No generated or configuration files. |
| **Impact area** | `core/assets` (recipes) only. No downstream caller exists yet, by construction. |
| **Module/crate reach** | One module, one crate. Nothing crosses a crate boundary. |
| **Existing-test breakage** | **0.** No existing test references the new type, and no existing symbol changes. |
| **New validation** | 5 unit tests in `recipes.rs`: (a) `default` resolves a recipe from an `AsyncMemoryStore` holding `folder/recipes.yaml` and `trivial` does not — asserted through `AsyncRecipeProvider::recipe`, reusing the harness already at `recipes.rs:1330` (`#[tokio::test]`, `SimpleEnvironment<Value>`); (b) YAML round-trip of both variants; (c) JSON round-trip; (d) `RecipeProviderChoice::default()` is the variant Q1 settles; (e) `FromStr` accepts both names and reports an unknown one. Plus `cargo test -p liquers-core --lib` and `bash scripts/check-build-matrix.sh` (the enum must compile on wasm32 too). |
| **Behavioural risk** | *Compatibility*: additive; no existing behaviour reachable. *Persistence/data*: the variant names become part of a file format — the only lasting commitment. *Concurrency*: not applicable — the type is `Copy` data and the providers are stateless. *Performance*: not applicable — one `Arc<ZST>` allocation per environment construction, off every hot path. *Security*: not applicable — no new input reaches a store or a query. *Error paths*: one new error, from `FromStr` on an unknown name. |
| **Recovery** | Delete the type. Nothing depends on it, so revert is a single-file revert with no migration. |
| **Certainty** | Q1 (default variant) is unresolved and is a real decision, not a preference. Everything else is verified against the code: both providers are unit structs, the trait bound admits `Arc`/`Box`, `serde` is already a dependency of `liquers-core`, and the test harness pattern exists. |

### Open questions for the gate

**Q1 — `#[default]` on `Default` or on `Trivial`?**

- The issue's snippet marks `Default`, which reads correctly in a document: omitting `recipes:`
  gives you working recipes.
- `environment-builder` Phase 2 decided `EnvironmentBuilder::new()` defaults to `Trivial` (core) and
  `liquers-lib` supplies `Default` for its own constructor, so that `liquers-lib`'s existing
  behaviour is preserved.
- These two defaults are reached by different routes and would disagree only for a caller who
  builds a core environment from a document that omits `recipes:`.
- **Recommendation:** keep `#[default] Default` as the issue specifies, and state in the rustdoc
  that this is the *document* default and is deliberately not the same as
  `EnvironmentBuilder::new()`'s unconfigured default. A document that says nothing about recipes
  most plausibly wants them to work. If the maintainer prefers one number for both, the cheaper
  change is on the builder side, not here.

**Q2 — is `boxed_provider` wanted, or only `provider`?** The four `liquers-core` environments take
`Box<dyn AsyncRecipeProvider<Self>>` while `liquers-lib`'s takes `Arc<…>`, so one method cannot
serve both without an `Arc::from(Box)` at the call site. Two methods is the smaller change than
harmonising four public setters. **Recommendation:** ship both; harmonising the setters belongs to
`environment-builder`, which replaces them.

### Review record

*Against Phase 1:* every acceptance criterion has a named test; no work appears in Phase 2 that
Phase 1 did not ask for; the non-goals (factory trait, `EnvironmentConfig`, call-site changes) are
absent from the plan.

*Against the codebase:* signatures were read, not remembered — `AsyncRecipeProvider`'s
`MaybeSend + MaybeSync` supertraits (`recipes.rs:476-478`), the `Box` versus `Arc` split between
`context.rs:1026` and `liquers-lib/src/environment.rs:86`, the unit-struct definitions at
`recipes.rs:567` and `:609`, and the existing provider test harness at `recipes.rs:1330`. The
object-safety obstacle to a `RecipeProviderFactory` was derived from `AsyncRecipeProvider`'s
generic parameter, which is the reason the store precedent does not transfer. Risk is not
understated: the change genuinely cannot break a caller that does not exist, and the one durable
commitment (variant spelling) is recorded rather than glossed.
