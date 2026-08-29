Based on `HEAD`, read rather than remembered. Nothing here is implemented.

# Phase 2 — Solution and architecture

## Chosen solution

Replace the `panic!` with the `SimpleEnvironment` fallback, verbatim, and correct the doc comment:

```rust
fn get_recipe_provider(&self) -> Arc<dyn AsyncRecipeProvider<Self>> {
    if let Some(provider) = &self.recipe_provider {
        return provider.clone();
    }
    eprintln!("No recipe provider configured in SimpleEnvironmentWithPayload");
    Arc::new(crate::recipes::TrivialRecipeProvider)
}
```

`eprintln!`, never `println!` — library code must not write to stdout, and `SimpleEnvironment`
already uses `eprintln!` at `context.rs:1109`.

Doc comment (`context.rs:1807-1808`) changes from "otherwise `get_recipe_provider` panics" to the
sentence its three siblings use: *"If no recipe provider is configured, this environment returns
`TrivialRecipeProvider`."*

## Q1 — which diagnostic, on which environments

Three options were considered:

| Option | Effect | Verdict |
|---|---|---|
| Payload environment matches `SimpleEnvironment`: `eprintln!` + `Trivial` | Two native queued environments warn; two immediate environments stay silent | **Recommended.** Smallest change, and the split has a defensible line: the immediate environments are the wasm-capable ones, where `eprintln!` goes to the console on every construction and is noise. |
| Add the `eprintln!` to all four | Fully uniform | Rejected for this issue: it changes two environments the issue is not about, and one of them is the wasm path. If the maintainer wants uniformity, this is the variant to ask for. |
| Remove the `eprintln!` from `SimpleEnvironment`, all four silent | Fully uniform, quieter | Rejected: it removes a diagnostic that exists because a silently-trivial provider makes every `-R/` query fail for a reason nobody can see. `LIB-RECIPE-PROVIDER-PANIC` was hard to diagnose for exactly that reason. |

The recommendation satisfies acceptance criterion 2 ("all four return a provider") but not a strict
reading of "consistent diagnostics across all four". That gap is deliberate and is Q1 at the gate.

## Relationship to `environment-builder` (Q2)

`environment-builder` Phase 2
([`phase2-architecture.md`](../environment-builder/phase2-architecture.md):23,207,220) consolidates the four
environments into `GenericEnvironment<V, P, K>` aliases and resolves the recipe provider once in
`build()`, so `recipe_provider` is never `Option` in the environment and this panic cannot exist.
That design is at `phase: examples` with an open PR.

Two ways forward:

- **Fix now (recommended).** The change is one line plus a doc sentence. If `environment-builder`
  lands, it deletes the line — a zero-cost discard. If it slips or is scoped down, a live panic on
  a default path has been removed in the meantime. The issue's own Fix direction says exactly this:
  "If that project does not land, fix it directly."
- **Wait.** Avoids a trivial merge conflict in `context.rs` with a design that rewrites the file,
  and avoids the stale row in that design's approved Phase 2 table.

The merge-conflict argument is the only real cost, and it is one line in a file that design rewrites
wholesale — the conflict resolution is "take the builder's version". Recommendation: fix now, and
tell the `environment-builder` owner so its Phase 2 table row can be corrected by that design.

## Exact symbols involved

- **Changed** — `liquers-core/src/context.rs:1954-1959` (`impl Environment for
  SimpleEnvironmentWithPayload<V, P>`, method `get_recipe_provider`) and the struct doc comment at
  `:1804-1808`.
- **Read, unchanged** — the three sibling implementations (`:1105`, `:1242`, `:1384`),
  `TrivialRecipeProvider` (`recipes.rs:567`), `with_recipe_provider` (`:1873`).
- **Test added** — `liquers-core/src/context.rs`'s existing `#[cfg(test)] mod tests` (`:1398`),
  which already holds `#[tokio::test]` cases (`:1497`, `:1530`, `:1739`), so the harness exists.

## Ownership, errors, sync/async

- `Arc::clone` on the configured path is unchanged; the fallback allocates one `Arc` per call, as
  `SimpleEnvironment` already does. Neither is on a hot path — `get_recipe_provider` is called per
  keyed-recipe lookup, not per value.
- No `Result` is involved: the trait method returns `Arc<dyn AsyncRecipeProvider<Self>>`, so a
  provider must be produced. Returning an error instead would change the trait and every
  implementation — out of scope, and `environment-builder` removes the question by making the field
  non-optional.
- No async, no locks, no `unwrap`/`expect`.
- `SimpleEnvironmentWithPayload` is `#[cfg(not(target_arch = "wasm32"))]`, so the new test must be
  too, and needs a Tokio runtime (`DefaultAssetManager` spawns its queue at construction).

## API and backward compatibility

No signature changes. The only behavioural difference is on the path that currently aborts.

## Reuse

Reuses the sibling implementation literally — the fix is to stop diverging, so introducing a shared
helper would be a third pattern. `environment-builder` is the change that removes the duplication
properly; duplicating one more `if let … else` here is the honest minimum until then.

## Related open issues

- `LIB-RECIPE-PROVIDER-PANIC` (closed, P0) — the precedent, with its test at
  `liquers-lib/src/environment.rs:196-203` (`default_environment_has_a_recipe_provider`). The new
  test mirrors it.
- `environment-builder` — see Q2.
- `RECIPE-PROVIDER-BY-NAME` — its Q1 (which provider is the document default) is the same choice
  seen from the configuration side; the two should not disagree without a reason.

## Risk analysis

| Assessment | Record |
|---|---|
| **Files** | 1 source file (`liquers-core/src/context.rs`): one method body, one doc paragraph, one colocated test. Plus the issue file's `status:` at Phase 5 and the regenerated `specs/index.csv`. No generated, configuration or spec-reference files. |
| **Impact area** | `core/context`. Callers reached: `assets.rs`, `plan.rs`, `interpreter.rs` all call `get_recipe_provider` through `EnvRef`, but only for `SimpleEnvironmentWithPayload` instances that have **no** provider — today every one of those aborts, so there is no working behaviour to regress. |
| **Module/crate reach** | One module, one crate. Nothing crosses a crate boundary. |
| **Existing-test breakage** | **0 expected.** `liquers-core/tests/payload_inheritance.rs:131` configures `DefaultRecipeProvider` explicitly, so it never reaches the fallback. No test asserts the panic — searched for `should_panic` and for the panic message; neither appears outside the implementation. |
| **New validation** | One `#[tokio::test]` in `context.rs`'s test module asserting that all four environments produce a provider with none configured, and that the provider resolves no recipes (`AsyncRecipeProvider::has_recipes` is `false`, `recipe` is an error) — behaviour, not type identity. Commands: `cargo test -p liquers-core --lib`, `cargo test -p liquers-core --test payload_inheritance`, `cargo test -p liquers-lib --lib --tests`, and `bash scripts/check-build-matrix.sh` (the wasm rows must stay green given the `cfg`). |
| **Behavioural risk** | *Compatibility*: a panic becomes an error return — strictly an improvement, but a caller relying on the abort as a configuration check loses it. Considered and accepted: an abort is not an API. *Persistence/data*: not applicable — nothing is written. *Concurrency*: not applicable — the method reads an immutable field. *Performance*: one `Arc<ZST>` allocation on an already-degraded path. *Security*: not applicable. *Error paths*: `-R/` queries on an unconfigured payload environment now fail with `TrivialRecipeProvider`'s "No recipes defined by the trivial recipe provider" instead of aborting. |
| **Recovery** | Single-line revert. No migration, no persisted state. |
| **Certainty** | Q1 (diagnostic uniformity) and Q2 (fix now versus wait for `environment-builder`) are both genuine decisions, recommended above but not the agent's to make. Everything else is verified at `HEAD`: the four implementations, the `Option` field, the doc comments, the absence of any test depending on the panic, and the `liquers-lib` precedent. |

## Open questions for the gate

1. **Q1 — diagnostic.** Payload environment matches `SimpleEnvironment` (recommended), or add the
   `eprintln!` to all four, or remove it from all four?
2. **Q2 — timing.** Fix now (recommended) or leave it to `environment-builder`? Fixing now also
   makes one row of that design's approved Phase 2 table stale.

## Review record

*Against Phase 1:* the acceptance criteria map to one test and one doc edit; the non-goals
(non-optional field, environment consolidation, provider naming) are absent from the plan; the
issue's inaccurate claim about the doc comment is corrected rather than repeated.

*Against the codebase:* all four `get_recipe_provider` implementations, the `Option` field, the
struct doc comments, the `eprintln!` at `:1109`, the existing test module and its `#[tokio::test]`
cases, and `payload_inheritance.rs`'s explicit provider were read at `HEAD`. The stdout rule was
checked (`eprintln!`, not `println!`). Risk is not understated: the change is genuinely one line,
but it is recorded that it conflicts with a live design, which is why it goes to the gate rather
than through automatic clearance.
