---
id: STORE-CONFORMANCE-VALIDATION-TOOL
kind: issue
title: No way to run the conformance suite against a store outside a test binary
status: accepted
priority: P2
complexity: M
area: [store/backends, core/store]
design:
created: 2026-09-02
github:
---
## Problem

`STORE-ASYNC-STORE-NO-BEHAVIOURAL-CONFORMANCE-SUITE` gives `AsyncStore` an executable contract, but
it can only be executed from a test binary against a hand-written fixture. Two things it cannot do:

- **Validate a store described by a configuration document.** A deployment's `stores.yaml` names
  store types and arguments; nothing runs the contract against what that document actually builds.
- **Debug a store under development** without writing a fixture, a test file, and a `cargo test`
  invocation first.

Both are the same missing thing: a command that takes a store description, runs the suite, and
prints the report.

## Impact

A store that passes in-tree does not thereby pass *as configured* — a mis-parsed option
(`STORE-OPENDAL-LIST-OPTION-MISPARSED`) builds a different store than the document describes, and
nothing catches it. And the feedback loop for someone writing a store is a test file rather than a
command, which is the difference between checking after every change and checking at the end.

Not urgent: the suite itself delivers the divergence census this depends on. This is the ergonomics
layer above it.

## Design already decided

Deferred out of `design/store-conformance-suite/` at its Phase 4 gate for size (that project was
judged `XL` with the tool in it, `L` without) — **not** because these questions were open. They were
settled through four review rounds and are recorded here so the work does not restart from nothing.

### The binary

`liquers-store-check`, in **`liquers-store`** — it reaches the most store types, while `liquers-core`
owns `StoreRouterBuilder` and the configuration format. An explicit `[[bin]]` with
`required-features = ["cli", "store-conformance"]`, because an auto-discovered binary cannot carry
one; this is why `liquers-validate` has an explicit block too. Both features are new to
`liquers-store`, and `clap` is a new optional dependency there.

It cannot reach `liquers-web`'s stores — those exist only in a browser. A limitation to state, not
to work around.

### Command surface

```text
liquers-store-check --config <store.yaml> [--store <prefix>] [--rule <id>]...
                    [--level read-only|create-only|scratch] [--format text|yaml|json]
liquers-store-check --scratch <store-type> [--arg k=v]...
```

### Safety — the part that matters

The suite writes, removes, and calls `removedir`. A tool that takes a configuration document and
does that will be pointed at production storage in its first week. Three ordered levels, each rule
declaring the lowest it can run at:

| Level | Permits | Refuses |
|---|---|---|
| `read-only` | reads and listings | every mutation |
| `create-only` | creating a key that does not exist | overwriting, removing, `removedir` |
| `scratch` | anything, to keys this run created | anything that was already there |

- **Defaults follow provenance.** `--config` defaults to `read-only` — it is somebody's data.
  `--scratch` defaults to `scratch` — the factory just made it. Raising the level on `--config` is
  always explicit.
- **Listing residue is a requirement, not a nicety.** At `create-only` the tool cannot remove what
  it made, so the operator now owns keys they did not have. The residue list prints **before** the
  summary. A `create-only` run reporting no residue with a non-empty `created` set is a bug.
- **"Not run" is never "passed".** The report names the rules the level excluded and the level that
  would run them. A clean `read-only` run exercises 9 rules of 31 and misses every divergence the
  suite was built for, so a bare "conformant" would be actively misleading.
- **Level 3 is rule discipline, not a guarantee** — rules check before they mutate, and
  check-then-write is not atomic. Documented as a limit.

### Exit codes

**0** conformant · **1** non-conformant · **2** invocation or setup failure — matching
`liquers-validate`. The tool prints the resolved `StoreConfig` before running, so a mis-parsed
option shows as a setup problem rather than as a store defect.

### `StoreFactory::create_fixture`

Deferred with the tool, having no other consumer. Additive and defaulted so no implementor changes:

```rust
#[cfg(feature = "store-conformance")]
fn create_fixture(&self, config: &StoreConfig)
    -> Result<Option<Box<dyn store_conformance::Fixture>>, Error> { Ok(None) }
```

A factory that can build a fresh, empty store of its type — a temporary directory, a scratch prefix,
a fresh table — returns a fixture owning whatever must be cleaned up. This is what lets `--scratch`
test a type named in a document with no fixture code, and it is the safest path by construction: a
factory-made fixture is expendable where a configured store is somebody's data.

**Known limitation:** it is synchronous, matching `create`. A factory needing async setup —
provisioning a scratch bucket, opening a connection — cannot supply a fixture. Widening `create` to
async is a separate change.

## The unsolved problem

**Where does a `--config` store's fixture come from?** `--config` is the primary mode and the one
the provenance defaults were designed around, but nothing says where its `StoreCapabilities` and its
`keys_for` answers come from:

- `create_fixture` builds a *scratch* store; a `--config` store comes from `create`.
- `StoreCapabilities` deliberately has no `Default` (so that adding a capability is a compile error
  at every fixture rather than a silent `false`), so the tool cannot synthesize one.
- `KeyRequest::Existing` would mean listing somebody's production store to pick a subject.

Candidate answers, none chosen: a capabilities block in the YAML; CLI flags; probing the store and
inferring; or restricting the tool to `--scratch` and dropping `--config` entirely. **Settle this
before implementing** — it is the difference between a two-mode tool and a one-mode tool.

## Depends on

`STORE-ASYNC-STORE-NO-BEHAVIOURAL-CONFORMANCE-SUITE`. Specifically it needs `run_all`, a serializable
`ConformanceReport`, and the `SafetyLevel` gate — all of which that project delivers regardless of
this one, since the guide's per-store status matrix is generated from the same reports.

## Discovery

Split out on 2026-09-02 at the Phase 4 gate of `design/store-conformance-suite/`, when the final
review sized that project `XL` and identified this as the cleanest removal: nothing else depends on
it, and it carries the project's one unsolved design question.
