# Phase 2: Solution & Architecture — dispatching a realm to another environment or host

## Overview

**What exists at HEAD (re-checked 2026-10-20):**

- The planner applies a segment's realm to every action in it (`PlanBuilder::process_query` →
  `get_command_metadata(query, realm, …)` → `find_command_in_namespaces(realm, …)`,
  `liquers-core/src/plan.rs`; tests `realm_header_applies_to_every_action` and siblings). This is
  the "realm participates in resolution and in the plan" half of the issue, delivered by
  `store-adapters`.
- `Step::Action { realm, ns, action_name, … }` carries the realm into the interpreter, which builds
  `CommandKey::new(&realm, &ns, &action_name)` and calls `envref.get_command_executor()
  .execute_async(…)` (`liquers-core/src/interpreter.rs`, the `Step::Action` arm of the step
  executor). `CommandRegistry` looks executors up by full key, so **several realms registered in
  one environment already execute**; only `NON_EXECUTABLE_REALMS` are refused
  (`refuse_non_executable_realm`, `commands.rs`).
- The issue's marker moved: `// TODO: RQS realm should should be supported` is now above
  `process_resource_query` (`plan.rs`, ~line 1536) and concerns a *resource* segment's header
  name, which is warned about and reserved (`REALMS.md`). It is not the dispatch gap.
- `Environment` has `type CommandExecutor: CommandExecutor<Self>` (`context.rs`), and
  `apply_recipe` is documented as the hook "useful to implement multi-realm evaluation".

So the gap is narrower than the issue says: nothing binds a realm to an executor that is **not**
the local registry.

**Candidate seams** (one must be chosen with Phase 1 question 1):

| Seam | Unit | Where | Assessment |
|---|---|---|---|
| A. Routing `CommandExecutor` | one action + its input `State` | a `RealmRouter<E>` as `Environment::CommandExecutor`, delegating `realm → Arc<dyn RealmExecutor<E>>`, falling back to `CommandRegistry` | No core change to the interpreter. But the input state must be serialized and uploaded, the remote sees an anonymous call (nothing cached, no key, no dependencies), and no HTTP route executes one command on an uploaded state |
| B. Planner delegation step | the query prefix ending in the realm's segment | `PlanBuilder` emits a delegation step for a segment whose realm has a remote binding; the interpreter asks the binding to evaluate the prefix | Remote part is an ordinary server asset: cached, dependency-tracked, served by today's `GET q/entry/{*query}` (value + `MetadataRecord`). Adds a `Step` variant, so every exhaustive `match` on `Step` changes (`interpreter.rs`, `plan.rs` freeze, `store/realm.rs` `compile_plan`) |
| C. `Environment::apply_recipe` override | a whole recipe | a custom environment splits the plan itself | Leaves every environment to reinvent it; rejected |

**Recommended (not chosen — blocking question 1):** seam B, with a new step rather than reusing
`Step::Evaluate(Query)`, because the binding, not the local asset manager, must evaluate it:

```rust
// liquers-core/src/plan.rs — sketch, pending question 1
pub enum Step {
    // … existing variants …
    /// Evaluate `query` in the realm binding `realm`; `query` is in the remote's spelling.
    Delegate { realm: String, query: Query },
}

// liquers-core/src/context.rs — sketch
#[cfg_attr(not(target_arch = "wasm32"), async_trait)]
#[cfg_attr(target_arch = "wasm32", async_trait(?Send))]
pub trait RealmBinding<E: Environment>: MaybeSend + MaybeSync {
    /// Local realm name → remote realm name (Phase 1 question 2).
    fn remote_realm(&self) -> &str;
    async fn evaluate(&self, query: &Query, context: Context<E>) -> Result<State<E::Value>, Error>;
}
```

Rejected alternative, *the realm as an attribute of a command rather than of a host*: realms already
select command sets per host (`VISION.md` strategy), and a per-command location would make a
query's meaning depend on registry contents, which Vision 2 forbids.

## Known-Issue Preflight

| Issue | Status | Priority | Impact on this design | Fix first? | Blocking? | Action |
|---|---|---|---|---|---|---|
| `NO-REMOTE-STORE-OR-ASSET-MANAGER` | draft | P2 | The remote evaluation and its notifications (AC-5) | yes, its asset-manager half | yes, for the remote binding | `requires` |
| `TYPE-REGISTRY-NOT-REALM-AWARE` | draft | P2 | Whether a remote result can be held locally | its first slice | no | `required-by` |
| `CORE-SESSION-AND-KEY-ACL` | accepted | P2 | Identity of a realm call | yes | yes, for any deployment with users | `requires` |
| `UNMANAGED-ASSET-NOT-FOLLOWABLE-ACROSS-CONNECTIONS` | draft | P3 | Delegation with overrides is unmanaged remotely | no | no | note |
| `WEB-CANCELLATION-INERT` | accepted | P3 | A remote step in the browser cannot be cancelled | no | no | note |
| `VALUE-SERIALIZATION-IS-SYNCHRONOUS-AND-WHOLE-VALUE` | draft | P2 | Results cross whole | no | no | note |
| `PY-COMMAND-DECORATOR-CANNOT-SET-REALM` | draft | P3 | Python commands cannot be placed in a bound realm | no | no | note |

## Interfaces

None committed. The sketches above are the shape seam B needs; seam A needs only a
`CommandExecutor` implementation outside core. Both are `async` (a network call), and both use the
`maybe_send` markers so the browser binding is `?Send`.

## Integration Points

- `liquers-core/src/plan.rs`: `PlanBuilder::process_query` (emit `Delegate` for a bound segment);
  the plan needs to know bindings, so `PlanBuilder` gains read access to them through the
  environment it already plans for (`make_plan_with_cwd` has the `EnvRef`).
- `liquers-core/src/interpreter.rs`: one new arm; it records a `DependencyKey` for the remote part
  (AC-5), which `DependencyManager` cannot version without remote notifications.
- `liquers-core/src/store/realm.rs` `compile_plan`: refuses `Delegate` like `Evaluate`.
- `liquers-web`: a `fetch` binding over `GET q/entry/{*query}` (`?_entry_format=cbor`).
- native: the same over an HTTP client, beside the remote store of `remote-store-and-asset-manager`.
- Command metadata import under the bound realm (`REGISTRIES-NOT-PUBLISHED-ACROSS-REALMS`).

## Error Handling

Remote unreachable: `Error::general_error` today has no network type; a remote error decoded from
`ErrorDetail` keeps its `ErrorType` (`api_core/error.rs` round-trips every variant) and gains the
realm and remote query in its message (AC-4). An unbound realm stays `action_not_registered` (AC-3).

## Relevant Commands

None new. Any command, in any namespace, can sit in a bound realm.

## Documentation Architecture

`specs/reference/REALMS.md` (reference, internal, `core/plan`): "How a realm is executed" and the
open question it lists. Proposed `affects_docs`: `REALMS.md`, `PROJECT_OVERVIEW.md`,
`WEB_API_SPECIFICATION.md`.

## Risks

| Assessment | Finding |
|---|---|
| Files likely to change | `plan.rs`, `interpreter.rs`, `context.rs`, `store/realm.rs`, `environment_builder.rs`; a web and a native transport |
| Crates and workflows affected | core, web, a native client; every exhaustive `match` on `Step` |
| Existing tests likely to change | none for AC-2; exhaustive `Step` matches in tests gain an arm |
| New validation | an in-process binding (a second `SimpleEnvironment`) proves AC-1..AC-5 without a network; an axum round trip proves the transport |
| Compatibility / data / concurrency / performance / security | a new `Step` changes the serialized plan; a delegated query carries user data to another host (needs ACL); latency per remote segment |
| Recovery | bindings are opt-in: an environment without one plans exactly as today |
| Certainty and open questions | High on the seams; blocked on question 1 (unit) and question 2 (realm name on the wire), whose answers change the plan format and cache identity |
