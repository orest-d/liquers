# Phase 2: Solution & Architecture — Principals and an access policy at the entry points

## Overview

`liquers-core` gains `access.rs`: an operation/target vocabulary, an `AccessPolicy` trait, a
serde-configurable `RulePolicy`, and `plan_access_requests`, a read-only walk of a built plan
(the `liquers-mcp` `check_allowed` walk, generalized to keys). `User` is the principal. In
`liquers-axum`, each builder takes `with_access_policy(Arc<dyn AccessPolicy>)`; a `Principal`
extractor reads a `User` request extension (default `Anonymous`); every handler, after parsing its
key or query and overrides, asks an `AccessGuard` before acting. Identity never reaches `Context`,
the asset manager or a store, so evaluation and caching are unchanged (Q1).

Rejected: identity on `Context` (issue route 1; breaks shared caching, Q1); the payload (route 2;
cannot reach keyed assets); sessions as assets (route 3; needs this ACL to protect its own folder);
a store decorator (a store sees no caller); one middleware for all routes (re-parses paths, misses
body overrides naming other queries).

## Known-Issue Preflight

| Issue | Status | Priority | Impact on this design | Fix first? | Blocking? | Action |
|---|---|---|---|---|---|---|
| `STORE-COMMAND-NAMESPACE-MISSING` | accepted | P3 | A key passed as a command parameter is not a plan step, so it is authorized only through the command | No | No | Documented limit (Phase 1 non-goals) |
| `ERROR-WITH-KEY-SETS-QUERY-FIELD` (`error-with-key-field`) | draft | P2 | `access_denied` sets `key` and `query` as `status_conflict` does | No | No | Follow today's convention |
| `UNMANAGED-ASSET-NOT-FOLLOWABLE-ACROSS-CONNECTIONS` | draft | P3 | Will consume `Principal` | No | No | Design Dependencies |
| `AGENT-WORKBENCH-SURFACE` | draft | P2 | W5 governance would configure policies per tool set | No | No | Follow-up via the MCP issue |

## Interfaces

`liquers-core/src/access.rs` (new, `pub mod access` in `lib.rs`; sync, no I/O — a policy is
in-memory configuration; `MaybeSend + MaybeSync` for wasm):

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Operation { Read, Write, Evaluate, Admin }

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AccessTarget { Key(Key), Command(CommandKey), Server }

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AccessRequest { pub operation: Operation, pub target: AccessTarget }

pub trait AccessPolicy: MaybeSend + MaybeSync {
    /// `Ok(())` if `principal` may perform `request`, else `Error::access_denied`.
    fn check(&self, principal: &User, request: &AccessRequest) -> Result<(), Error>;
    fn check_all(&self, principal: &User, requests: &[AccessRequest]) -> Result<(), Error> { … }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Effect { Allow, Deny }

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AccessRule {
    pub principals: Vec<String>,   // "*", "anonymous", "system", "user:*", "user:NAME"
    pub operations: Vec<Operation>,
    #[serde(default)] pub keys: Vec<String>,      // key prefixes by element ("" = every key)
    #[serde(default)] pub commands: Vec<String>,  // CommandKey Display, or a prefix ending in '*'
    #[serde(default = "Effect::allow")] pub effect: Effect,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RulePolicy { pub default: Effect, pub rules: Vec<AccessRule> }
impl RulePolicy { pub fn from_yaml(text: &str) -> Result<Self, Error>; }  // keys parsed with parse_key
impl AccessPolicy for RulePolicy { … }  // first rule matching principal, operation and target wins

/// Every key and command `plan` names: key steps (`GetAsset*`, `GetResource*`) as `Read`, actions
/// as `Evaluate`, recursing into `Step::Plan`, `Evaluate`, `UseQueryValue` and link parameters
/// (planned with `cmr`, placeholders allowed). Recipes behind keys are not expanded (Q5).
pub fn plan_access_requests(plan: &Plan, cmr: &CommandMetadataRegistry)
    -> Result<Vec<AccessRequest>, Error>;
```

`liquers-core/src/context.rs`: `User` derives `Clone, Debug, PartialEq, Eq, Hash, Serialize,
Deserialize`; `impl Display for User` (`anonymous`, `system`, `user:NAME`). Module doc: identity
is checked where requests enter; `Context` stays identity-free by design.

`liquers-core/src/error.rs`: `ErrorType::AccessDenied`; `Error::access_denied(principal: &User,
request: &AccessRequest) -> Error` (message "user:bob may not write 'notes/alice/todo.md'"; sets
`key`/`query` for a key target).

`liquers-axum/src/api_core/access.rs` (new):

```rust
pub struct Principal(pub User);                       // FromRequestParts: Extension<User> or Anonymous
#[derive(Clone, Default)] pub struct AccessGuard(Option<Arc<dyn AccessPolicy>>);
impl AccessGuard {
    pub fn key(&self, who: &User, op: Operation, key: &Key) -> Result<(), Error>;
    pub fn admin(&self, who: &User) -> Result<(), Error>;      // AccessTarget::Server
    pub fn query(&self, who: &User, query: &Query, overrides: &OverrideRequest,
                 cmr: &CommandMetadataRegistry) -> Result<(), Error>;  // plan, or overrides.to_recipe(..).to_plan(..)
    pub fn readable<'a>(&self, who: &User, keys: impl Iterator<Item = &'a Key>) -> Vec<&'a Key>;
}
```

Builders (`StoreApiBuilder`, `AssetsApiBuilder`, `RecipesApiBuilder`, `QueryApiBuilder`):
`pub fn with_access_policy(self, policy: Arc<dyn AccessPolicy>) -> Self`; `build` layers
`Extension(AccessGuard)` on every route (an absent policy is `AccessGuard(None)`, which allows).

## Integration Points

| Entry | Check |
|---|---|
| Store API reads (`data`, `metadata`, `entry` GET, `is_dir`, `contains`) | `key(Read)` |
| Store API `listdir`, `keys` | `key(Read)` on the directory, entries filtered with `readable` |
| Store API writes (`PUT data/metadata/entry`, `DELETE`, `makedir`, `removedir`, `upload`, GET alternatives) | `key(Write)` per key |
| Recipes API (`data`, `metadata`, `entry`, `resolve`, `listdir`) | `key(Read)`; `listdir` filtered |
| Query API, Assets `q/data|entry|submit|info|metadata|version|cancel` | `query(...)` |
| Assets `key/` reads, `contains`, `can_make`, `recover`, `listdir` | `key(Read)`; `listdir` filtered |
| Assets `key/` mutations, `key/cancel` | `key(Write)` |
| Assets `admin/` | `admin` |
| `ws/q`, `ws/key` | principal captured in the upgrade handler; each `subscribe` runs `query`/`key(Read)`; a refusal is an `Error` message |

Handlers gain `Principal` and `Extension<AccessGuard>` extractors; the check is the first statement
after parsing. `liquers-py/src/error.rs` (both directions) and `liquers-web/src/error.rs`
(`"access_denied"`): the variant. `liquers-core/src/assets.rs` persistence `match`: `AccessDenied`
→ not persisted.

## Error Handling

All refusals use `Error::access_denied` → `ErrorType::AccessDenied` → 403 in
`api_core/error.rs` `error_to_status_code` and `parse_error_type` (and `keyabs15b`'s list). A
malformed policy is `Error::configuration_error` from `RulePolicy::from_yaml` (500 at startup, never
per request). A plan that cannot be built is the planner's error (400), as today, before any check.

## Relevant Commands

None added. Namespaces: any — `commands` patterns name them (`ns-mem/*`, `root/*`).

## Documentation Architecture

- `specs/reference/WEB_API_SPECIFICATION.md` §9 rewritten (the table above, 403, principal seam,
  Q5 recipe rule, no-policy default); §3.1 403 row; Appendix B drops "access control and
  sessions"; History, `reviewed:`.
- `specs/guides/WEB_API_GUIDE.md` new §"Restricting access": YAML policy, an axum layer inserting
  `User` from a bearer token, the 403 body.
- `specs/reference/PROJECT_OVERVIEW.md` stateless section; `DOC_04` Session/User paragraph;
  `POSITIONING.md` caveat; each with History and `reviewed:`.
- `specs/README.md`: "Sessions and key-level authorization" planned → built (slice 1).
- Proposed `affects_docs`: as in `DESIGN.md`.

## Risks

| Assessment | Finding |
|---|---|
| Files likely to change | `access.rs` (new), `context.rs`, `error.rs`, `assets.rs` (one arm), `lib.rs`; axum `api_core/access.rs` (new), `api_core/error.rs`, four builders, ~60 handlers, `websocket.rs`; py and web `error.rs` |
| Crates and workflows affected | core, axum, py, web; every HTTP route |
| Existing tests likely to change | none functionally (AC-5); `keyabs15b` gains the variant |
| New validation | core unit tests of `RulePolicy` and the walk; axum integration suite `access_policy.rs` |
| Compatibility | additive when no policy is set; `User` gains derives; a new `ErrorType` string |
| Security | the walk must see every key a request names: overrides and `_link_` are included by planning the override recipe; recipes are trusted by design (Q5); listings filtered; denial reveals existence (Q3) |
| Performance | one plan build per query request — the evaluation builds it again; acceptable (parsing and planning are cheap next to evaluation), measurable later |
| Concurrency | policy is immutable `Arc`; no locks |
| Recovery | no policy configured = HEAD behaviour; the variant and derives are additive |
| Certainty | medium-high: every entry point opened; the walk is proven in `liquers-mcp`; Q1 and Q3 can still change the contract |
