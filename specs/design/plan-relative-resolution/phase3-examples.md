# Phase 3: Examples & Use-cases - Recipe CWD Propagation and Relative Resolution

## High-Level Introduction

These scenarios make the Phase 1 invariant observable: a recipe establishes an ordered working
key, while the interpreter resolves every later relative key, query, and link from the live CWD.
The first scenario covers the normal provider and programmatic recipe workflow. The second exposes
ordered `SetCwd`, recursive link scoping, and nested-plan propagation. The third distinguishes the
missing-CWD root fallback from absolute-query behavior and protects the warning contract.

The user selected runnable prototypes. Phase 3 therefore specifies exact Rust test targets whose
queries and present-day setup APIs are valid. They are post-implementation acceptance prototypes:
the parser validation already passes, but the behavioral assertions intentionally fail against the
verified current implementation until implementation after Phase 4 approval adds the approved CWD semantics. No Rust source or
test file is created during this design phase.

## Example Type

**User choice:** Runnable prototypes.

## Overview Table

| # | Type | Name | Purpose | Intended path |
|---|---|---|---|---|
| 1 | Example | Provider and programmatic recipe CWD | Proves both CWD sources produce one raw `SetCwd`, retain source-relative operands, and fetch the intended stored input | `liquers-core/tests/recipe_cwd_resolution.rs` |
| 2 | Example | Ordered changes and recursive scopes | Proves `a/b` plus `../c` becomes `a/c`, linked queries are scoped, and nested plans share live context | inline tests in `query.rs` and `interpreter.rs` |
| 3 | Example | Root fallback and absolute silence | Proves missing CWD installs `/` and warns once, while absolute operands do neither | inline tests in `context.rs` and `interpreter.rs` |
| 4 | Unit tests | Cursor, recipe, plan, and context contract | Covers pure resolution, plan shape, every link variant, serialization, errors, and exact-once root installation | inline `#[cfg(test)]` modules in five `liquers-core/src` files |
| 5 | Integration tests | Recipe-to-store-to-asset execution | Covers provider loading, command execution, dependency identity, manual plans, and nested recipes | `liquers-core/tests/recipe_cwd_resolution.rs` |

## Example 1: Provider and Programmatic Recipes Resolve the Same Way

### Connection to the High-Level Design

This is the representative workflow from Phase 1. A `DefaultRecipeProvider` supplies the folder
containing `recipes.yaml`; a programmatic caller assigns the same public `Recipe::cwd` field
directly. In both cases `Recipe::to_plan` must record, but not pre-apply, that CWD. The interpreter
then uses the prefix when it reaches the source-relative stored-resource key.

### Scenario

Two recipes read `./input.txt`, pass it through a locally registered test-only `identity` command,
and name the output `result.txt`. One recipe is applied programmatically with CWD `programmatic`;
the other is loaded from `provider/recipes.yaml`. The memory store contains distinguishable values
at `programmatic/input.txt` and `provider/input.txt`, so resolving from root or from the wrong
folder cannot accidentally pass.

The query is:

```text
-R-stored/./input.txt/-/identity/result.txt
```

`-R-stored` is deliberate: it produces the existing `Step::GetResource` and reads the seeded
store value directly. Plain `-R` would produce `Step::GetAsset`, which is a different lifecycle.
`identity` is registered only inside the test and does not add a production command namespace.

### Sequence of Steps

1. Create an `AsyncMemoryStore` rooted at `Key::new()` and seed the two input keys plus
   `provider/recipes.yaml`.
2. Register the test-only `identity` command before converting either recipe to a plan.
3. Set `programmatic_recipe.cwd = Some("programmatic".into())`; let
   `DefaultRecipeProvider` assign `Some("provider")` to the loaded recipe.
4. Assert each plan starts with exactly one raw `Step::SetCwd`, its next data step remains
   `Step::GetResource("./input.txt")`, and `init_steps` contains the recipe CWD `Info`.
5. Apply the programmatic recipe through the built-in environment hook and evaluate
   `provider/result.txt` through the asset manager. Assert the returned texts are respectively
   `programmatic` and `provider` fixture values.

### Core Example Code

The structural half is a complete inline acceptance test for `liquers-core/src/recipes.rs`. It
compiles against the current public types when inserted, but fails today because `to_plan` does not
yet add the prefix or diagnostic.

```rust
#[test]
fn recipe_to_plan_preserves_programmatic_cwd(
) -> Result<(), Box<dyn std::error::Error>> {
    let mut cmr = CommandMetadataRegistry::new();
    let identity = CommandMetadata::new("identity");
    cmr.add_command(&identity);

    let mut recipe = Recipe::new(
        "-R-stored/./input.txt/-/identity/result.txt".to_string(),
        "Relative input".to_string(),
        "Read input relative to the recipe folder".to_string(),
    )?;
    recipe.cwd = Some("programmatic".to_string());

    let plan = recipe.to_plan(&cmr)?;
    match plan.steps.first() {
        Some(Step::SetCwd(key)) => assert_eq!(key.encode(), "programmatic"),
        other => panic!("expected initial SetCwd, got {other:?}"),
    }
    match plan.steps.get(1) {
        Some(Step::GetResource(key)) => assert_eq!(key.encode(), "./input.txt"),
        other => panic!("expected source-relative GetResource, got {other:?}"),
    }
    assert_eq!(
        plan.steps
            .iter()
            .filter(|step| matches!(step, Step::SetCwd(_)))
            .count(),
        1
    );
    assert!(plan.init_steps.iter().any(|step| {
        matches!(step, Step::Info(message) if message.contains("programmatic"))
    }));
    assert_eq!(
        plan.query.encode(),
        "-R-stored/./input.txt/-/identity/result.txt"
    );
    Ok(())
}
```

The following is the complete post-implementation integration prototype for
`liquers-core/tests/recipe_cwd_resolution.rs`. It deliberately uses the public asset-manager
application path (`envref.get_asset_manager().apply(recipe, State::new())`) for the programmatic
recipe and `EnvRef::evaluate` for the provider-owned keyed recipe. It is expected to compile once
the test file is added; its CWD plan-shape and result assertions fail on the current code because
the approved semantics have not yet been implemented.

```rust
use liquers_core::{
    assets::AssetManager,
    context::{Context, EnvRef, Environment, ImmediateEnvironment},
    error::Error,
    metadata::Metadata,
    parse::parse_key,
    plan::Step,
    query::Key,
    recipes::{DefaultRecipeProvider, Recipe},
    state::State,
    store::{AsyncMemoryStore, AsyncStore},
    value::Value,
};
use liquers_macro::register_command;

type CommandEnvironment = ImmediateEnvironment<Value>;

fn identity(state: &State<Value>) -> Result<Value, Error> {
    Ok(Value::from(state.try_into_string()?))
}

fn assert_recipe_prefix(
    recipe: &Recipe,
    envref: &EnvRef<CommandEnvironment>,
    expected_cwd: &str,
) -> Result<(), Error> {
    let plan = recipe.to_plan(envref.get_command_metadata_registry())?;
    assert!(matches!(plan.steps.first(), Some(Step::SetCwd(key)) if key.encode() == expected_cwd));
    assert!(matches!(plan.steps.get(1), Some(Step::GetResource(key)) if key.encode() == "./input.txt"));
    assert_eq!(
        plan.steps.iter().filter(|step| matches!(step, Step::SetCwd(_))).count(),
        1,
    );
    assert_eq!(
        plan.init_steps.iter().filter(|step| matches!(step, Step::Info(message)
            if message == &format!("Recipe set CWD to '{expected_cwd}'"))).count(),
        1,
    );
    Ok(())
}

#[tokio::test]
async fn programmatic_and_provider_cwd_select_their_own_inputs(
) -> Result<(), Box<dyn std::error::Error>> {
    let store = AsyncMemoryStore::new(&Key::new());
    let metadata = Metadata::new();
    store.set(&parse_key("programmatic/input.txt")?, b"programmatic", &metadata).await?;
    store.set(&parse_key("provider/input.txt")?, b"provider", &metadata).await?;
    store.set(
        &parse_key("provider/recipes.yaml")?,
        br#"recipes:
  - query: "-R-stored/./input.txt/-/identity/result.txt"
    title: Provider input
    description: Reads relative to this recipes.yaml
"#,
        &metadata,
    ).await?;

    let mut env = CommandEnvironment::new();
    let registry = &mut env.command_registry;
    register_command!(registry, fn identity(state) -> result)?;
    env.with_async_store(Box::new(store));
    env.with_recipe_provider(Box::new(DefaultRecipeProvider));
    let envref = env.to_ref();

    let mut programmatic = Recipe::new(
        "-R-stored/./input.txt/-/identity/result.txt".to_owned(),
        "Programmatic input".to_owned(),
        "Reads relative to a programmatic CWD".to_owned(),
    )?;
    programmatic.cwd = Some("programmatic".to_owned());
    assert_recipe_prefix(&programmatic, &envref, "programmatic")?;

    let provider_key = parse_key("provider/result.txt")?;
    let provider_recipe = envref
        .get_recipe_provider()
        .recipe_opt(&provider_key, envref.clone())
        .await?
        .expect("provider recipe");
    assert_recipe_prefix(&provider_recipe, &envref, "provider")?;

    let applied = envref
        .get_asset_manager()
        .apply(programmatic, State::new())
        .await?;
    assert_eq!(applied.get().await?.try_into_string()?, "programmatic");

    let keyed = envref.evaluate("-R/provider/result.txt").await?;
    assert_eq!(keyed.get().await?.try_into_string()?, "provider");
    Ok(())
}
```

`Recipe` is imported from `liquers_core::recipes`; its public `cwd` field is assigned directly.
The local `identity` function and `register_command!` invocation bind the same registry identifier
(`registry`) and use no production command namespace.

### Guide and Executable Example

Extend `specs/guides/LANGUAGE-INTEGRATION_GUIDE.md` with the provider/programmatic distinction and
link the complete integration test above. The guide snippet should show direct assignment to the
existing public `Recipe::cwd`; it must not invent a `with_cwd` builder or encourage a `cwd` field
inside `recipes.yaml`.

**Expected output:** both executions return the fixture under their own folder, the plan retains
`./input.txt`, and metadata records that the recipe established its CWD.

**Acceptance state:** parser/plan construction for the query passes now; the new CWD assertions and
correct relative execution pass only after implementation.

## Example 2: Ordered `SetCwd`, Scoped Links, and Shared Nested Plans

### Additional Mechanism

A recipe starts at `a/b`. Its query changes CWD by `../c`, then passes a relative linked query to a
test-only action:

```text
-R-cwd/../c/-/action-~X~-R/./hello.txt~E
```

At runtime the `SetCwd` argument is resolved first, so the live CWD becomes `a/c`. The linked query
then identifies `a/c/hello.txt`. PlanBuilder and the stored plan retain `../c` and `./hello.txt`;
the interpreter and dependency passes resolve copies immediately before use.

### Runnable Unit Prototype

After Phase 2's crate-private cursor exists, this inline `query.rs` test is the smallest executable
proof of the ordered rule:

```rust
#[test]
fn cwd_cursor_resolves_ordered_cwd_changes(
) -> Result<(), Box<dyn std::error::Error>> {
    let mut cursor = CwdCursor::new(Some(parse_key("a/b")?));
    let resolved_cwd = cursor.set_cwd_from(&parse_key("../c")?);
    assert_eq!(resolved_cwd.encode(), "a/c");
    assert_eq!(cursor.current().as_ref().map(Key::encode).as_deref(), Some("a/c"));

    let linked = cursor.resolve_query_scoped(&parse_query("-R/./hello.txt")?);
    assert_eq!(
        linked.key().as_ref().map(Key::encode).as_deref(),
        Some("a/c/hello.txt")
    );
    assert!(!cursor.take_root_fallback());
    Ok(())
}
```

This test cannot compile before implementation because `CwdCursor` is a new private type. It is a
post-implementation acceptance prototype, not evidence of current behavior.

### Action-Free Chained CWD Test

The minimal interpreter acceptance test uses no transform query, command registry, store, or
recipe provider:

```text
-R-cwd/../-R-cwd/./c/-R-key/.
```

With entry CWD `a/b`, the first `SetCwd(..)` must resolve to `a`; the second
`SetCwd(./c)` must resolve against that updated state to `a/c`; finally `UseKeyValue(.)` must
resolve its own operand against the same live state and return the `Key` value `a/c`. The raw plan
must remain `SetCwd(..)`, `SetCwd(./c)`, `UseKeyValue(.)`, proving that PlanBuilder did not perform
the resolution early.

```rust
use liquers_core::{
    context::{Environment, ImmediateEnvironment},
    interpreter::evaluate,
    parse::parse_key,
    value::{Value, ValueInterface},
};

#[tokio::test]
async fn chained_cwd_updates_resolve_key_value(
) -> Result<(), Box<dyn std::error::Error>> {
    let envref = ImmediateEnvironment::<Value>::new().to_ref();
    let state = evaluate(
        envref,
        "-R-cwd/../-R-cwd/./c/-R-key/.",
        Some(parse_key("a/b")?),
    )
    .await?;

    assert_eq!(state.value()?.try_into_key()?.encode(), "a/c");
    Ok(())
}
```

This test directly protects both key-bearing interpreter branches: a relative `Step::SetCwd`
must resolve before updating Context, and `Step::UseKeyValue` must resolve before calling
`ValueInterface::from_key`. A temporary Phase 3 regression probe compiled and failed against the
current implementation with left value `"."` and expected value `"a/c"`, independently confirming
the missing runtime resolution.

### Link Scope Versus Nested-Plan Scope

The locally registered `pass`, `cwd`, and `append_cwd` commands make scope observable rather than
merely structural. `pass` returns its linked argument, `cwd` returns
`Context::get_cwd_key().unwrap().encode()`, and `append_cwd` appends the outer CWD to its input.
Thus one returned value proves both scopes:

```text
pass-~X~-R-cwd/./child/-/cwd~E/append_cwd
```

The test-local signatures are exactly:

```rust
async fn pass(
    _state: State<Value>,
    use_link: String,
) -> Result<Value, Error> {
    Ok(Value::from(use_link))
}

fn cwd(context: Context<CommandEnvironment>) -> Result<Value, Error> {
    Ok(Value::from(
        context.get_cwd_key().expect("test CWD").encode(),
    ))
}

fn append_cwd(
    state: &State<Value>,
    context: Context<CommandEnvironment>,
) -> Result<Value, Error> {
    Ok(Value::from(format!(
        "{}|{}",
        state.try_into_string()?,
        context.get_cwd_key().expect("test CWD").encode(),
    )))
}
```

Register them with the local registry binding, not an expression that hides the binding:

```rust
let registry = &mut env.command_registry;
register_command!(registry, async fn pass(state, use_link) -> result)?;
register_command!(registry, fn cwd(context) -> result)?;
register_command!(registry, fn append_cwd(state, context) -> result)?;
```

The result must be `a/b/child|a/b`: the embedded query receives a fork of the outer cursor, so the
linked `cwd` sees `a/b/child`, while `append_cwd` sees the unchanged outer `a/b`. Conversely, a
manually constructed `Step::Plan` shares the same `Context`; if it executes `SetCwd(./child)`, both
the nested observer and the following outer observer see `a/b/child`.

**Expected outcomes:**

| Mechanism | Child observes | Following outer step observes |
|---|---|---|
| linked child query with `SetCwd(./child)` | `a/b/child` | `a/b` |
| nested `Step::Plan` with `SetCwd(./child)` | `a/b/child` | `a/b/child` |

Protect this with `context::tests::resolver_scopes_nested_links` and
`interpreter::tests::nested_plan_inherits_and_updates_cwd`. Recursive traversal must include
`ParameterLink`, `DefaultLink`, `OverrideLink`, `EnumLink`, and links inside
`MultipleParameters`.

### Observable Runtime and Identity Acceptance

The integration action used for the ordered case is `use_link`: it receives the linked value and
returns its string unchanged. A second `cwd` observer returns the current CWD. Seed
`a/c/hello.txt = "a/c/hello"` with a `MetadataRecord` carrying key `a/c/hello.txt`, status
`Source`, and type identifier `text`, then assert that
`-R-cwd/../c/-/use_link-~X~-R/./hello.txt~E` returns `a/c/hello`; a trailing `/cwd` returns
`a/c`. This proves both the linked value and live-context state, not just an encoded plan.

For the recipe whose initial CWD is `a/b`, assert the raw, pre-execution plan in this exact order:

1. the sole recipe prefix `Step::SetCwd(a/b)`;
2. the query-derived `Step::SetCwd(../c)`;
3. the action whose parameter remains the raw relative link `-R/./hello.txt`.

Assert that `init_steps` contains exactly one `Step::Info` whose message identifies the recipe CWD;
that metadata entry must not be counted as an executable step. Re-run the same resolved keyed query
through the asset manager and assert the same asset identity (or the same `AssetRef::id()` where the
manager contract guarantees reuse) and the same `a/c/hello` value. This is cache-identity evidence:
two syntactically distinct requests that resolve to the same absolute dependency must not produce
separate cache entries, while a sibling key such as `a/b/hello.txt` must not be reused.

Use three tiny local commands to cover every public context boundary, each with
`Context<CommandEnvironment>` injected by `register_command!`:

| Command | Required body | Assertion |
|---|---|---|
| `via_evaluate` | `context.evaluate(&parse_query("-R/./hello.txt")?).await?.get().await?` | returns `a/c/hello` and records the resolved dependency identity |
| `via_state` | `context.get_dependency_state(&parse_query("-R/./hello.txt")?).await?` | returns the same value and resolved dependency identity |
| `via_apply` | `context.apply(&parse_query("-R/./identity")?, State::new().with_data(Value::from("a/c/hello"))).await?` | applies from the active CWD and returns `a/c/hello` without recording an ad-hoc asset as a dependency |

The exact command result strings include the method name, so a wiring error cannot pass merely
because all paths return some text. Run each as a non-volatile keyed asset twice and assert the
same resolved dependency key and manager cache reuse; do not use the ad-hoc `apply` result itself
as cache-reuse evidence because that API explicitly does not insert it into the key/query cache.

## Example 3: Missing CWD Falls Back Once; Absolute Queries Stay Silent

### Pitfall 1: Treating Missing CWD as an Error

- **Symptom:** a manual plan containing `./hello.txt` fails before store lookup.
- **Cause:** the resolver requires an explicitly supplied CWD.
- **Correct behavior:** install logical root `Key::new()`, resolve the operand as `hello.txt`, and
  log exactly one `Relative key/query has no CWD; using logical root '/'.` warning for the shared
  evaluation context.
- **Protective test:** `interpreter::tests::relative_operand_without_cwd_warns_and_uses_root`.

### Pitfall 2: Letting an Absolute Outer Query Absolutize Its Links

The absolute query `/-R/./data/-/use_link-~X~-R/./hello.txt~E` uses a temporary root only for its own
resource path. Its linked query is still relative and therefore uses the live Context CWD, or the
normal root fallback when none exists. In
`/-R/./data/-/use_link-~X~/-R/./hello.txt~E`, both resource paths are absolute; the two *resource
paths* neither consult nor change Context CWD and neither triggers a fallback warning. This does
not make a general promise that the complete action execution cannot emit another warning.

- **Symptom:** a relative link unexpectedly resolves from root merely because its outer query has
  a leading `/`.
- **Cause:** absolute status was incorrectly propagated into the nested query AST.
- **Correction:** assess every embedded query independently and fork the semantic cursor.
- **Protective tests:** `context::tests::absolute_operands_ignore_missing_cwd` and
  `context::tests::absolute_query_does_not_absolutize_relative_link`.

### Pitfall 3: Removing an Apparently Redundant `SetCwd`

An action may inspect `Context::get_cwd_key`, call `Context::evaluate` or `Context::apply` with a
relative query, or enter a nested plan. Converting visible static operands to absolute form does
not prove the CWD state unobservable. `interpreter::tests::action_observes_current_cwd` must fail
if a future optimizer removes the recipe prefix without such a proof.

## Corner Cases

### 1. Memory

Resolution clones parsed `Key` and `Query` trees; it does not buffer resource data. Test a deeply
nested `MultipleParameters` link tree and a long key to catch accidental recursion omissions or
quadratic encode/reparse behavior. No manual allocation or unsafe memory is introduced, so an OOM
fixture or leak benchmark would not produce feature-specific evidence. Preserve `QuerySource` and
`Position` fields to prove that resolution transforms the AST directly rather than encoding and
reparsing it.

### 2. Concurrency

All clones of one execution `Context` share `cwd_key`. On every target, run a deterministic
clone test: resolve `./one` on the first clone, then `../two` on the second, and assert both see
the installed logical root and the collected final metadata has exactly one fallback warning.
The assertion is made only after the owning asset has completed: `take_pending_dependencies` and
the analogous metadata drain are finalization primitives, so draining them mid-evaluation would
change the observable result and invalidate the test.

On native targets only (`#[cfg(not(target_arch = "wasm32"))]`), also use a Tokio barrier to start
two cloned contexts concurrently on their first missing-base resolution. Assert one successful
root installation and one warning in finalized metadata, and put a timeout around completion to
expose a lock/queue deadlock. Do not require this race test on wasm, where the executor model does
not provide the same parallel interleaving; the deterministic clone test remains cross-target.
Drop the CWD mutex guard before `Context::warning` and before every `.await`. The dependency
pre-pass owns a private cursor and must never copy its simulated final CWD back to live Context.

### 3. Errors

Malformed programmatic `Recipe::cwd` remains a parse error from `Recipe::get_cwd`; it must not use
the root fallback. Provider-loaded YAML that authors `cwd` remains rejected. Missing resources,
command failures, and unsafe store traversal continue through their existing typed errors. Test
that resolution attaches no new error kind and does not hide `KeyNotFound` from the resolved key.
Also close or substitute the context's asset-service/log receiver in the existing unit harness, run
the first missing-base resolution, and assert that the error from `Context::warning` propagates to
the caller rather than being ignored after root installation. This is the exact logging-error path:
the CWD may be installed once, but a failed warning delivery is still a returned `Error`.

### 4. Serialization

Round-trip a recipe plan through JSON and YAML. The serialized form must retain the raw
`SetCwd(../c)`, source-relative keys and links, `QuerySource`/positions where supported, and the
recipe init `Info`. Deserialization is not an implicit normalization pass. `Context` and
`CwdCursor` remain runtime-only and are not serialized; there is no schema evolution, compression,
or binary metadata change to test.

### 5. Integration

Use `AsyncMemoryStore`, `DefaultRecipeProvider`, `ImmediateEnvironment<Value>`, the asset manager, and
test-only commands. Current HEAD's `ImmediateAssetManager::get` fast-tracks a stored plain asset only
when its metadata is eligible, so every fixture reached through plain `-R` supplies its key, `Source`
status, and `text` type identifier; direct `-R-stored` fixtures need no asset fast-track metadata.
Verify that store lookup, dependency records, cycle checks, pre-scheduling, execution, and
finalization all name the same resolved key. No Web/API, UI, Python, or
`liquers-lib` behavior changes, so separate cross-crate fixtures would add no signal; native and
wasm compilation remain regression gates because `liquers-core` is shared.

## Documentation and Learning Log

### Guide Candidate Workflows and Examples

- **How do I use recipe CWD?** Let `DefaultRecipeProvider` derive it from the containing
  `recipes.yaml`; for a custom/programmatic recipe, assign the public `Recipe::cwd` field.
- **How do I achieve reliable relative links?** Keep parsed operands relative and execute the
  recipe through the normal environment/interpreter path; do not pre-resolve them in PlanBuilder.
- **What is the typical workflow?** Load or create recipe, inspect its raw plan prefix and
  diagnostic, execute it, and verify the resolved dependency/store key.
- The guide should reuse the short programmatic assignment and link
  `liquers-core/tests/recipe_cwd_resolution.rs` as the complete executable evidence.

### Usage and Meaning

CWD is observable ordered execution state, not a textual convenience. The same resolution governs
store keys, asset identity, dependency scheduling, nested evaluations, action links, and manual
plans. `Plan::init_steps` explains that a recipe established CWD, but only the executable
`Step::SetCwd` changes Context.

### Phase 1 Documentation Mapping

| Phase 1 target | Phase 5 documentation evidence |
|---|---|
| `specs/reference/api/DOC_02_QUERY_LANGUAGE_REFERENCE.md` | Define source-relative resource keys and nested links; show `-R-cwd/../c` followed by a relative link, and distinguish a leading `/` query from a relative link inside it. |
| `specs/reference/api/DOC_04_ENVIRONMENT_CONTEXT_EVALUATION.md` | State that `Context` owns the live CWD, `SetCwd` is ordered and observable, and `evaluate`, `get_dependency_state`, and `apply` resolve against that state. Document one root-fallback warning per shared context. |
| `specs/guides/LANGUAGE-INTEGRATION_GUIDE.md` | Show provider-derived CWD versus programmatic `Recipe::cwd`, and link the full integration test rather than recommending YAML-authored `cwd`. |
| `specs/reference/PROJECT_OVERVIEW.md` | Add the cross-cutting invariant that source-relative plans are interpreted by core Context rather than environment adapters. |
| `specs/README.md` | Add/find the above reference and guide entry in the contributor documentation index. |
| `specs/issues/CORE-PLAN-RELATIVE-RESOLUTION-MISSING.md` | Mark the implemented behavior and tests, including absolute-query and root-warning boundaries, when the issue is closed. |

### Repeatable Development Guidance

When adding a new key/query-bearing step or parameter variant, update both the pure recursive
cursor walk and the interpreter use-site, then add a source-relative test that distinguishes the
correct target from root and a wrong sibling. Never validate resolution by encode/reparse. Treat
opaque actions and nested plans as optimizer barriers unless observability has been proved absent.

### Corrections and Unexpected Learning

- The verified current implementation accepts `Recipe::cwd` publicly and from Serde even though
  `Recipe::to_plan` ignores it; the default provider is the layer that rejects authored YAML CWD.
- Builder-side absolute rewriting plus an inserted `SetCwd` is ambiguous. The approved design
  keeps PlanBuilder source-relative and assigns semantic authority to the interpreter.
- The user example requires `-R/./hello.txt`, not `-R./hello.txt`.
- Plain `-R/./input.txt` plans as `Step::GetAsset`; the primary raw-store fixture must use
  `-R-stored/./input.txt` to exercise `Step::GetResource`.
- An absolute outer query does not confer absolute status on linked queries.
- The accumulated evidence confirms the Phase 2 decision to extend existing reference and guide
  documents; no additional document is needed.

## Test Plan

### Unit Tests

| File | Test | Required assertion |
|---|---|---|
| `liquers-core/src/query.rs` | `cwd_cursor_resolves_only_leading_dot_and_parent` | `./x` and `../x` use `a/b`; ordinary `plain/x` is unchanged |
| same | `cwd_cursor_missing_relative_base_uses_root_once` | root is installed and `take_root_fallback()` is true once |
| same | `cwd_cursor_absolute_query_uses_private_root_without_fallback` | `/-R/./x` and `/-R/../x` use temporary root without changing cursor |
| same | `cwd_cursor_scopes_child_cwd` | child `SetCwd` does not leak; missing-base root initialization does |
| same | `cwd_cursor_preserves_query_source_and_positions` | AST provenance survives without encode/reparse |
| `liquers-core/src/recipes.rs` | `recipe_to_plan_preserves_programmatic_cwd` | one initial raw `SetCwd`, init `Info`, raw query and `GetResource` retained |
| same | `recipe_prefix_info_is_exactly_once_and_precedes_query_steps` | exactly one init `Info`; raw order is recipe `SetCwd(a/b)`, explicit `SetCwd(../c)`, then raw action link `-R/./hello.txt` |
| same | extend `test_default_recipe_provider` | provider folder assigned; YAML-authored `cwd` rejected |
| same | `recipe_to_plan_rejects_invalid_programmatic_cwd` | existing parse error is returned |
| same | `recipe_plan_round_trip_keeps_raw_operands_and_prefix` | JSON/YAML retain prefix, links, raw operands, and diagnostic |
| `liquers-core/src/plan.rs` | `find_dependencies_resolves_all_link_variants_with_ordered_cwd` | every link variant and nested multiple uses the action CWD |
| same | `find_dependencies_child_query_cwd_does_not_leak` | later parent dependency retains parent CWD |
| same | `find_dependencies_nested_plan_propagates_cwd` | nested-plan final CWD affects later outer steps |
| same | `find_dependencies_respects_nested_recipe_cwd` | keyed child uses `Recipe::to_plan_for_key` and resolved identity |
| `liquers-core/src/context.rs` | `resolver_installs_root_once_across_context_clones` | atomic one-time installation and warning |
| same | `root_fallback_warning_delivery_error_propagates` | a closed logging/service channel returns its error; it is not swallowed |
| same | `absolute_operands_ignore_missing_cwd` | an ordinary key and an absolute query's own resource path do not install root or emit the fallback warning |
| same | `absolute_query_does_not_absolutize_relative_link` | nested relative link has an independent base decision |
| `liquers-core/src/interpreter.rs` | `resolves_ordered_cwd_changes` | `a/b` plus `../c` yields `a/c` before later operands |
| same | `chained_cwd_updates_resolve_key_value` | entry `a/b`, then `..`, `./c`, and key `.` return the `Key` value `a/c` without actions |
| same | `dependency_preschedule_tracks_cwd_without_mutating_context` | scheduled identity is resolved; live context keeps entry CWD |
| same | `evaluate_cwd_applies_before_dependency_analysis` | legacy entry CWD reaches both analysis and execution |
| same | `manual_plan_resolves_relative_steps_from_context` | every key/query-bearing manual step resolves at use |
| same | `nested_plan_inherits_and_updates_cwd` | nested plan shares live Context state |
| same | `finalize_relative_plan_uses_context_owner_key` | no dependency edge uses raw relative `plan.query.key()` |
| same | `action_observes_current_cwd` | recipe `SetCwd` remains an observable ordering barrier |
| same | `relative_operand_without_cwd_warns_and_uses_root` | pre-pass/runtime combined log one warning and use `/` |
| same | `context_entry_points_resolve_relative_queries` | `evaluate`, `get_dependency_state`, and `apply` all resolve the same active-CWD target |
| same, native only | `concurrent_root_fallback_warns_once` | barrier-raced context clones finalize with one warning and no deadlock |

### Integration Tests

**File:** `liquers-core/tests/recipe_cwd_resolution.rs`

Use `type CommandEnvironment = ImmediateEnvironment<Value>;` before `register_command!`, async
`#[tokio::test]` functions, `AsyncMemoryStore`, `DefaultRecipeProvider`, and
`Result<(), Box<dyn std::error::Error>>`.

| Test | End-to-end evidence |
|---|---|
| `programmatic_and_provider_cwd_select_their_own_inputs` | both CWD sources reach distinct seeded store keys and expose the required plan prefix/info |
| `explicit_cwd_overrides_recipe_cwd_for_later_relative_link` | requested `a/b` -> `a/c/hello.txt` behavior, with locally registered action |
| `resolved_dependency_identity_reuses_cached_asset` | asset-backed relative resolution records `a/c/hello.txt`, shares the non-volatile cached asset for equivalent requests, and does not reuse `a/b/hello.txt` |
| `context_boundary_commands_use_active_cwd` | test commands for `Context::evaluate`, `get_dependency_state`, and `apply` each return the resolved target with their boundary name |
| `recursive_links_and_multiple_parameters_use_active_cwd` | nested links and collections schedule/execute the same resolved identities |
| `nested_keyed_recipe_cwd_is_not_bypassed` | provider-derived child CWD and overrides survive dependency planning |
| `root_fallback_is_single_warning_under_shared_context` | several relative operands resolve from root with exactly one warning and no deadlock |
| `absolute_outer_query_keeps_relative_link_independent` | outer absolute path stays silent; nested relative link uses Context or fallback |

No FileStore fixture is required: logical normalization occurs before store dispatch, and
`AsyncMemoryStore` makes the selected key directly observable.

### Customized Test Templates from `liquers-unittest`

The unit template above follows the inline `#[cfg(test)] mod tests { use super::*; }` pattern. The
integration template is the complete Example 1 prototype above: it imports the exact public
traits, seeds its store, supplies `recipes.yaml`, binds `registry` in `register_command!`, applies
the programmatic recipe through `AssetManager::apply`, and evaluates the keyed provider recipe.
It intentionally contains no placeholder fixture comments or fake production command. Tests may
use `panic!`/`expect` for variant diagnostics, while library implementation must continue to use
typed errors and explicit enum matches.

### Manual Validation

Validate every documented query without executing commands or opening a store:

```powershell
cargo run -q -p liquers-core --features cli --bin liquers-validate -- --no-registry --command action --command pass --command cwd --command append_cwd --command use_link --command identity --detail summary -- '-R-stored/./input.txt/-/identity/result.txt' '-R-cwd/../c/-/action-~X~-R/./hello.txt~E' '-R-cwd/../-R-cwd/./c/-R-key/.' 'pass-~X~-R-cwd/./child/-/cwd~E/append_cwd' '/-R/./hello.txt' '/-R/./data/-/use_link-~X~-R/./hello.txt~E' '/-R/./data/-/use_link-~X~/-R/./hello.txt~E'
```

Observed during Phase 3: status `Ok`, seven results, zero warnings, zero errors. The `--command`
entries validate parser syntax and plan shape only; they model locally registered test commands
and do not claim production commands or execution semantics.

After implementation, run:

```powershell
cargo test -p liquers-core recipe_to_plan_preserves_programmatic_cwd
cargo test -p liquers-core cwd_cursor_resolves_ordered_cwd_changes
cargo test -p liquers-core --test recipe_cwd_resolution
cargo test -p liquers-core --lib
cargo check -p liquers-core
```

Success means all focused and crate tests pass; the intended `a/c/hello.txt` identity appears in
dependency/store evidence; each shared evaluation context emits one fallback warning only when an
operand actually needs a missing base; an absolute query's own resource path emits no such fallback
warning (while an independent relative nested link may); and serialized plans remain source-relative.

## Auto-Invoke: liquers-unittest Skill Output

The Liquers unit-test conventions determined the inline-versus-integration split, async test
signatures, `CommandEnvironment` placement, memory-store fixture, local command registration, and
explicit happy/error/edge coverage above. The generated templates cover recipe plan shape, pure
cursor rules, interpreter ordering, store/asset integration, typed errors, concurrency, and
serialization without adding unrelated cross-crate tests.
