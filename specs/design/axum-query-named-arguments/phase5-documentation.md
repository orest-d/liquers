# Phase 5: Documentation - Argument overrides over HTTP and WebSocket

## Completion Preconditions

- [x] Implementation is finished and validated (Phase 4 Progress: all ten steps ticked with commits)
- [x] All user comments are answered or incorporated (Q1–Q16, A1–A3)
- [x] All review comments are answered or incorporated (two reviewers per phase, Phases 2–4; the
      cross-phase consistency pass)
- [x] Documentation is consistent with the implemented and tested behavior (examples re-run
      against the example server)
- [x] Documentation is included in the implementation PR when practical (same branch)

## Implementation Summary

A client can name arguments of a query's last action — **argument overrides** — instead of
escaping them positionally into the path:

- **HTTP**: URL query parameters, and a `POST` JSON object body, on the Query API and on the
  Assets API's `q/data`, `q/entry` (both gaining `POST`) and `q/submit`. An override replaces a
  positional value; `_link_<argument>` gives a link; other `_` names are reserved; `_entry_format`
  replaces `format` on every `DataEntry` route.
- **WebSocket**: `subscribe` with `arguments` on `ws/q`, acknowledged at once with `Subscribed`
  (`subscription_id`, echoed `request_id`); `unsubscribe` and `cancel` by id.
- **Managed vs unmanaged**: a request carrying overrides is evaluated ad hoc
  (`AssetManager::apply_queued`, new) and is unmanaged; every answer says so
  (`x-liquers-managed`, `managed`). Empty forms (`?`, empty body, `{}`, options only, `arguments: {}`)
  stay on the managed path.
- **Core**: `Recipe::to_plan` resolves names against `Plan::last_action_command` (new; an alias's
  own arguments), converts mismatched JSON types with a planning warning, reports typed errors,
  and applies overrides in sorted order.

**Conformance.** Built as approved in Phases 1–4, with these changes, each recorded where it was
decided:

- **Added — Q16** (user decision during Step 8): the `Subscribed` acknowledgement and `request_id`.
  Without it a client could not tell concurrent subscribes apart, nor cancel an evaluation still
  being set up on the inline manager. Phases 1–3 amended.
- **Added — required arguments** (found running the example server in this phase): an override
  may supply a required argument the path leaves out; one still missing is a 400
  `ArgumentMissing`. Validation now plans with placeholders allowed, as `Recipe::to_plan` does. The
  test fixtures had only defaulted arguments, so the suites could not catch it; AOH22 now does.
- **Phase 2 amended at the Phase 3 review**: `last_action_command` returns the alias's metadata,
  not the target's, and alias head slots are not overridable.
- **Test changes**: AOW02 identifies its two evaluations by `asset_id` and the acknowledgements,
  since WebSocket notifications carry no value; the test gate is a semaphore, not a `Notify`
  (which stores one permit).

**Documentation.** Current behaviour now lives in the reference and the guide below; this folder
records why.

## Documentation Delivered

### New Reference Documents
None. The behaviour extends existing references (Web API, asset lifecycle, recipes); a separate
document would split one contract across two files.

### New Guide Documents
None. The how-to belongs in `guides/WEB_API_GUIDE.md`, which already covers using the API.

### Existing Documents Reviewed or Updated
`affects_docs` (authoritative), each updated with a `## History` row and `reviewed: 2026-10-12`:

| Document | Change |
|---|---|
| `reference/WEB_API_SPECIFICATION.md` | §2.3 `_entry_format`; new §2.4 argument overrides and options, §2.5 managed and unmanaged assets (table: when, why, what cannot follow, workaround); §5.3 routes; §5.8 `arguments`, `Subscribed`, ids, `cancel`, `managed`, the inline disconnect exception; §6 `_entry_format`; §7 Query API |
| `guides/WEB_API_GUIDE.md` | §4 naming arguments (curl, Python `params=` / `json=`, `_link_`, unmanaged, errors, warnings); §5 submitting what cannot be polled — no result, still useful for side effects; §8 following an evaluation with overrides; `?_entry_format=` |
| `reference/ASSET_LIFECYCLE.md` | `apply_queued`; the terms managed / unmanaged asset (four causes, how to tell) and argument override; the keyed-recipe edge case |
| `guides/ASSET_MANAGER_IMPLEMENTATION_GUIDE.md` | the provided `apply_queued`: what to return on each manager |
| `reference/api/DOC_08_RECIPES_PLANS.md` | the override rules of `Recipe::to_plan` |
| `reference/COMMAND_ALIASES.md` | overrides use the alias's own names; head slots refused (added at review: the rule changed) |

Candidates sharing an area and discarded after review, because no statement in them changed:
`api/DOC_01`, `api/DOC_03`, `api/DOC_04`, `ASSET_SET_OPERATION.md`, `ASSETS.md`,
`DEPENDENCIES_STATUS.md`, `ENVIRONMENT_CONFIG.md`, `PROJECT_OVERVIEW.md`,
`DEPENDENCY_CONSISTENCY_GUIDE.md`, `ENVIRONMENT_CONSTRUCTION_GUIDE.md`,
`LANGUAGE-INTEGRATION_GUIDE.md`, `UNITTEST_GUIDE.md`. Also updated, outside `specs/`:
`liquers-axum/README.md`, `examples/WEBSOCKET_EXAMPLE.md`, `examples/assets_recipes_basic.rs`,
`examples/websocket_client.rs` (Step 9).

### Links and Capability Map
`specs/README.md`: the line now points to `reference/WEB_API_SPECIFICATION.md` §2.4–2.5, with the
guide's §4 and this design as secondary links.

## Issues Filed

- `UNMANAGED-ASSET-NOT-FOLLOWABLE-ACROSS-CONNECTIONS` (P3) — deliberately omitted scope (Q15 (b)):
  a server-wide registry of unmanaged assets. To be designed once sessions exist, so an id can be
  checked against its owner.
- `VALIDATE-COMMAND-STUB-REJECTS-POSITIONAL-EMPTIES` (P3) — `liquers-validate --command` cannot
  check `cmd----x`; found in Phase 3.
- `AXUM-QUERY-API-QUERY-KEEPS-LEADING-SLASH` (P3) — the Query API at `/liquer/q` evaluates
  `/text-hi`, not `text-hi`, so it caches separately from the Assets API; found in this phase.

Closed: `QUERY-API-ARGUMENTS-ONLY-IN-QUERY-PATH`, with a resolution note.

## Important Learning

- **A validating query can still mean something else.** `echo_args----20` filled `fields`, not
  `limit`, until the fixture matched `search`'s shape. Read the plan's parameters, not just
  `status: Ok`.
- **Fixtures with only defaulted arguments hide a whole class of bugs.** The required-argument
  defect appeared only against the example server. A test fixture should include a command with a
  required argument.
- **Timing claims need deterministic tests.** One run seemed to show that an inline evaluation
  survives a disconnect; it was a race between opening the gate and the server noticing the close.
  Tests now wait for the server's reply before releasing a gate.
- **`apply` is not followable.** It waits for the evaluation, and on the inline manager it returns
  the error instead of the asset. `apply_queued` exists for callers that follow an ad-hoc asset.
- **Managed is a property to ask the manager about, not to infer**: same id from
  `lookup_*_asset`, and not volatile. The terms are now in `ASSET_LIFECYCLE.md` §4.

## Conformance and Remaining Work

| | Requested (issue) | Approved (Phases 1–4) | Implemented |
|---|---|---|---|
| Query API overrides (query string, body) | yes | yes | yes |
| Assets `q/data`, `q/entry`, `q/submit` | — | yes (Q6) | yes |
| WebSocket subscribe with overrides, ids, cancel | — | yes (Q9–Q11) | yes, plus the `Subscribed` acknowledgement (Q16) |
| Links by name | open question | `_link_<name>` (Q3) | yes |
| Caching of overridden requests | open question | unmanaged, ad hoc (Q4) | yes |
| Type conversion with warnings | open question | yes (Q5) | yes |
| `_entry_format` everywhere | — | yes (Q12) | yes |
| Following across connections | — | deferred (Q15) | issue filed |

Nothing remains in this design. The follow-up is the filed issue.

## Validation

- `python3 scripts/docs_index.py --check`: 0 errors.
- `validate_phase.py axum-query-named-arguments 1`–`5`: pass.
- Guide examples (curl and both Python scripts) re-run against `examples/assets_recipes_basic.rs`
  on 2026-10-12; outputs as printed.
- Code: `cargo test -p liquers-core --lib --tests` (44 suites), `-p liquers-axum` (14),
  `-p liquers-lib --lib --tests` (36) and `--test registry_export`: all green;
  `cargo check -p liquers-core --target wasm32-unknown-unknown` and
  `PYO3_USE_ABI3_FORWARD_COMPATIBILITY=1 cargo check -p liquers-py`: clean.
