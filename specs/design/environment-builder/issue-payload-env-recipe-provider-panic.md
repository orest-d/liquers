# `CORE-PAYLOAD-ENV-RECIPE-PROVIDER-PANIC` — Phase 1 and Phase 2

Prepared under [`guides/autonomous_issue_fixing.md`](../../guides/autonomous_issue_fixing.md) for
[`issues/CORE-PAYLOAD-ENV-RECIPE-PROVIDER-PANIC.md`](../../issues/CORE-PAYLOAD-ENV-RECIPE-PROVIDER-PANIC.md)
(issue, P1, complexity S). Recorded here because the issue links to this design; nothing in this
file changes the `environment-builder` phase documents or its workflow contract.

**State: Phase 2 written, awaiting the approval gate. Nothing is implemented.**

---

## Phase 1 — High-level design

### Problem and evidence

`SimpleEnvironmentWithPayload::get_recipe_provider` (`liquers-core/src/context.rs:1954`) aborts the
process instead of falling back:

```rust
fn get_recipe_provider(&self) -> Arc<dyn AsyncRecipeProvider<Self>> {
    if let Some(provider) = &self.recipe_provider {
        return provider.clone();
    }
    panic!("No recipe provider configured in SimpleEnvironmentWithPayload");
}
```

Its three siblings in the same file do not:

| Environment | `get_recipe_provider` with none configured | Site |
|---|---|---|
| `SimpleEnvironment` | `eprintln!` then `TrivialRecipeProvider` | `context.rs:1105` |
| `ImmediateEnvironment` | `TrivialRecipeProvider`, silently | `context.rs:1242` |
| `ImmediateEnvironmentWithPayload` | `TrivialRecipeProvider`, silently | `context.rs:1384` |
| `SimpleEnvironmentWithPayload` | **panics** | `context.rs:1954` |

`recipe_provider` is `Option<Arc<dyn AsyncRecipeProvider<Self>>>` (`context.rs:1819`) and
`SimpleEnvironmentWithPayload::new()` leaves it `None`, so the panic is on the **default** path: any
evaluation that reaches `get_recipe_provider` on a freshly constructed environment aborts.

**One correction to the issue text.** It states that "the struct's own doc comment claims the
`TrivialRecipeProvider` fallback that the other three implement", and that documentation and code
disagree. That is not what is at `HEAD`. The doc comment (`context.rs:1804-1808`) reads:

> A recipe provider must be configured before a keyed recipe lookup; otherwise
> [`Environment::get_recipe_provider`] panics.

The three siblings' doc comments (`:964`, `:1129`, `:1265`) each promise the fallback. So the
divergence is real and documented on both sides — the defect is the **inconsistency between four
environments that are otherwise interchangeable**, not a lie in a doc comment. This matters for
Phase 2: the fix must update that paragraph too, and the "documented behaviour" argument cannot
carry the decision on its own.

This is the same defect class as `LIB-RECIPE-PROVIDER-PANIC` (closed, P0), which fixed
`liquers-lib`'s `DefaultEnvironment` by making the field non-optional
(`liquers-lib/src/environment.rs:171` now returns `self.recipe_provider.clone()`). The
`liquers-core` payload environment was not covered.

### Expected behaviour and acceptance criteria

1. `SimpleEnvironmentWithPayload::new().to_ref()` followed by an evaluation that consults the recipe
   provider does not panic.
2. All four core environments return a provider — never abort — for the unconfigured case, and a
   test asserts that for all four rather than for the one being fixed.
3. The doc comment on `SimpleEnvironmentWithPayload` states what the code does.
4. No behaviour change for a configured provider.

### Affected users, workflows and systems

`core/context`. Reachable from `liquers-core/tests/payload_inheritance.rs` and the payload
documentation. `liquers-lib`, `liquers-axum` and `liquers-web` use their own environments and are
unaffected. `liquers-py` has its own `get_recipe_provider` (`liquers-py/src/context.rs:113`) and is
unaffected.

### Scope and non-goals

In scope: the fallback, the doc comment, and a test covering all four environments.

Not in scope: harmonising the `eprintln!` diagnostic across all four (see Q1 — this is the one open
choice), changing `Option<Arc<…>>` to `Arc<…>` as `liquers-lib` did, consolidating the four
environments (that is `environment-builder`), and `RECIPE-PROVIDER-BY-NAME`.

### Compatibility constraints

Behaviour changes on exactly one path: a process that used to abort now proceeds with a provider
that resolves no recipes, so a `-R/` query returns an error instead of killing the process. That is
the point of the fix. Nothing that works today changes.

### Known questions and assumptions

- **Q1** — diagnostic consistency: `SimpleEnvironment` prints, the two immediate environments do
  not. The issue asks for "the same `eprintln!` … or none at all — but consistently across all
  four", which is an instruction to pick. See Phase 2.
- **Q2** — overlap with `environment-builder`, whose Phase 2 says this issue is "fixed by
  construction" once the builder resolves the default in `build()`. Fixing it now duplicates work
  that design will delete. See Phase 2 §Relationship to `environment-builder`.

### Documentation assessment

Small maintenance only: the struct doc comment in `context.rs`, and a check of
`specs/reference/PAYLOAD_GUIDE.md` for any sentence describing the panic. No new document.
`environment-builder`'s Phase 2 table (`phase2-architecture.md:220`) records the panic as current
behaviour; if this lands first, that row becomes stale and needs one word changed — but that
document is an approved phase artifact of a live design, so the change belongs to that design's
owner, not to this issue.

---

## Phase 2 — Solution and architecture

### Chosen solution

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

### Q1 — which diagnostic, on which environments

Three options were considered:

| Option | Effect | Verdict |
|---|---|---|
| Payload environment matches `SimpleEnvironment`: `eprintln!` + `Trivial` | Two native queued environments warn; two immediate environments stay silent | **Recommended.** Smallest change, and the split has a defensible line: the immediate environments are the wasm-capable ones, where `eprintln!` goes to the console on every construction and is noise. |
| Add the `eprintln!` to all four | Fully uniform | Rejected for this issue: it changes two environments the issue is not about, and one of them is the wasm path. If the maintainer wants uniformity, this is the variant to ask for. |
| Remove the `eprintln!` from `SimpleEnvironment`, all four silent | Fully uniform, quieter | Rejected: it removes a diagnostic that exists because a silently-trivial provider makes every `-R/` query fail for a reason nobody can see. `LIB-RECIPE-PROVIDER-PANIC` was hard to diagnose for exactly that reason. |

The recommendation satisfies acceptance criterion 2 ("all four return a provider") but not a strict
reading of "consistent diagnostics across all four". That gap is deliberate and is Q1 at the gate.

### Relationship to `environment-builder` (Q2)

`environment-builder` Phase 2 (`phase2-architecture.md:23,207,220`) consolidates the four
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

### Exact symbols involved

- **Changed** — `liquers-core/src/context.rs:1954-1959` (`impl Environment for
  SimpleEnvironmentWithPayload<V, P>`, method `get_recipe_provider`) and the struct doc comment at
  `:1804-1808`.
- **Read, unchanged** — the three sibling implementations (`:1105`, `:1242`, `:1384`),
  `TrivialRecipeProvider` (`recipes.rs:567`), `with_recipe_provider` (`:1873`).
- **Test added** — `liquers-core/src/context.rs`'s existing `#[cfg(test)] mod tests` (`:1398`),
  which already holds `#[tokio::test]` cases (`:1497`, `:1530`, `:1739`), so the harness exists.

### Ownership, errors, sync/async

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

### API and backward compatibility

No signature changes. The only behavioural difference is on the path that currently aborts.

### Reuse

Reuses the sibling implementation literally — the fix is to stop diverging, so introducing a shared
helper would be a third pattern. `environment-builder` is the change that removes the duplication
properly; duplicating one more `if let … else` here is the honest minimum until then.

### Related open issues

- `LIB-RECIPE-PROVIDER-PANIC` (closed, P0) — the precedent, with its test at
  `liquers-lib/src/environment.rs:196-203` (`default_environment_has_a_recipe_provider`). The new
  test mirrors it.
- `environment-builder` — see Q2.
- `RECIPE-PROVIDER-BY-NAME` — its Q1 (which provider is the document default) is the same choice
  seen from the configuration side; the two should not disagree without a reason.

### Risk analysis

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

### Open questions for the gate

1. **Q1 — diagnostic.** Payload environment matches `SimpleEnvironment` (recommended), or add the
   `eprintln!` to all four, or remove it from all four?
2. **Q2 — timing.** Fix now (recommended) or leave it to `environment-builder`? Fixing now also
   makes one row of that design's approved Phase 2 table stale.

### Review record

*Against Phase 1:* the acceptance criteria map to one test and one doc edit; the non-goals
(non-optional field, environment consolidation, provider naming) are absent from the plan; the
issue's inaccurate claim about the doc comment is corrected rather than repeated.

*Against the codebase:* all four `get_recipe_provider` implementations, the `Option` field, the
struct doc comments, the `eprintln!` at `:1109`, the existing test module and its `#[tokio::test]`
cases, and `payload_inheritance.rs`'s explicit provider were read at `HEAD`. The stdout rule was
checked (`eprintln!`, not `println!`). Risk is not understated: the change is genuinely one line,
but it is recorded that it conflicts with a live design, which is why it goes to the gate rather
than through automatic clearance.
