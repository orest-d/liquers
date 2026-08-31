# Phase 2: Solution and architecture

Introduce a private outcome returned by `AssetRef::evaluate_recipe`, for example `RecipeEvaluation { state: State<E::Value>, delegated: bool }`. Normal command/recipe evaluation returns `delegated: false`; the existing `Some(asset) if asset.id() != self.id()` hand-off returns the owner state with `delegated: true`. `evaluate_and_store` installs and transitions the state exactly as today, but calls `persist_with_status_tracking` only when `delegated` is false.

Do not test ownership again at persistence time. `bound_owner_key()` correctly controls dependency registration but cannot distinguish every current persistence path; propagation from the one delegation branch has the smallest blast radius. The wrapper is private, contains owned state already returned by value, and crosses no await boundary beyond the existing result flow.

| Risk | Affected files/workflow | Validation and containment | Certainty |
|---|---|---|---|
| Skip too broadly | ordinary keyed writes | false flag on every nondelegated return; existing core tests | High |
| State transition changes | asset status/persistence tracking | retain install and `try_to_set_ready` sequence | High |
| Test misses remote cost | manager tests | counting `AsyncStore::set` wrapper | High |
| Mutable recipe identity misuse | keyed aliases | rely on selected delegation branch, not recipe comparison | High |

The existing delegation design established immutable identity rules; this design does not alter them or dependency records.
