# Phase 2: Solution & Architecture — `AsyncStore` conformance

## Overview

Four artefacts, one vocabulary running through all of them:

1. **`liquers_core::store_conformance`** — a shipped module behind the non-default feature
   `store-conformance`. Runtime-agnostic: plain `async fn`s, no `tokio`, no test attributes, no
   panics. It defines the rules, the capability vocabulary, the safety levels, the fixture
   interface and the report.
2. **Fixtures and suites** in `liquers-core`, `liquers-store` and `liquers-web`, running all seven
   in-tree implementations plus the trait defaults.
3. **`liquers-store-check`** — a binary that builds a store or router from a configuration
   document, runs the suite at a chosen safety level, and prints the report.
4. **Documentation** — `STORE_SEMANTICS.md` completed, `STORE_IMPLEMENTATION_GUIDE.md` created,
   `CONFORMANCE_TERMS.md` extracted so this guide and `LANGUAGE-INTEGRATION_GUIDE.md` share one
   status vocabulary, and a test holding all three to the code.

The spine is the **rule ID**. A rule ID names a function in `rules()`, a citation in the contract,
and a row in the guide; the synchronization test asserts the three sets are equal.

## Known-Issue Preflight

Open issues in `core/store`, `store/backends` and `web`, with their bearing on this architecture.

| Issue | P/C | Bearing | Blocker? |
|---|---|---|---|
| `STORE-ASYNC-STORE-NO-BEHAVIOURAL-CONFORMANCE-SUITE` | P1/L | This design closes it. | — |
| `CORE-STORE-KEYS-MEANS-TWO-DIFFERENT-THINGS` | P2/S | Settled by Phase 1 decision 1. Rule `keys01`–`keys02` encode it; `AsyncMemoryStore` changes. Closed by this work. | No |
| `CORE-ASYNC-MEMORY-STORE-IS-SUPPORTED-IGNORES-PREFIX` | P1/S | Its own design (`async-memory-store-prefix-support`) is at `phase: implementation`, Phase 3 approved. Rule `prefix02` will fail `AsyncMemoryStore` until it lands. **Sequencing, not blocking**: if it has not merged when this reaches Phase 4, the rule ships listing that store in its `allowed_failures` citing the issue, and the entry is removed when it merges — the stale-ignore check (below) makes that automatic rather than remembered. | No |
| `CORE-SYNC-STORE-TRAIT-OBSOLETE` | P2/M | Filed by this design. Out of scope; the contract is phrased trait-neutrally so its eventual return inherits the rules. | No |
| `DOCS-ASYNC-STORE-WRAPPER-NO-LONGER-EXISTS` | P2/S | Filed by this design. `UNITTEST_GUIDE.md` and `CLAUDE.md` teach a deleted type. This design edits `CLAUDE.md` §"Adding a Store Backend" anyway, so it fixes the two `CLAUDE.md` passages in passing; the guide and `STORE_CONFIG_FSD.md` passages stay with the issue. | No |
| `CORE-STORE-OPENBIN-MISSING` | P3/M | `openbin` is unimplemented everywhere, so there is nothing to hold to a contract. **No rules cover it.** When it is implemented it needs the absolute-key check and a rule; recorded in the guide's question list. | No |
| `STORE-ABSOLUTE-KEY-NOT-TYPE-ENFORCED` | P3/L | The `keyabs` family is convention-enforced. This suite adopts those IDs as rules, which strengthens the convention but does not replace the type-level fix. | No |
| `RESOURCE-NAME-ASCII-ONLY` | P2/L | Non-ASCII names cannot be parsed into a `Key`, so no rule can request one. `KeyRequest` therefore has no "non-ASCII name" variant, and `STORE_SEMANTICS.md` §7 keeps its ⚠. | No |
| `STORE-OPENDAL-LIST-OPTION-MISPARSED` | P2/S | Affects configuration parsing, which `liquers-store-check` uses. A mis-parsed list option makes the tool build the wrong store — a *setup* failure that would be reported as non-conformance. The tool must print the resolved `StoreConfig` it built. | No |
| `WEB-STORE-CONFIG-NOT-APPLIED-THROUGH-ENVIRONMENT-CONFIG` | P3/M | `liquers-web` hand-rolls its environment configuration. Irrelevant: the web suites construct stores directly, not through a document. | No |
| `CORE-ERROR-STORE-NAME-NOT-STRUCTURED` | P2/S | A store's name is interpolated into the message rather than being a payload field. Rules must therefore assert on `ErrorType`, **never** on message text. Stated as a rule-authoring constraint below. | No |
| `CORE-CONFIGURATION-ERROR-KIND` | P3/S | In progress. `liquers-store-check` should distinguish a configuration error (exit 2) from non-conformance (exit 1); until the error kind exists, it distinguishes by *stage* — a failure before the first rule runs is a setup failure. | No |

### Blocking and Priority Decision

**No blockers.** The one issue that could have been — `CORE-ASYNC-MEMORY-STORE-IS-SUPPORTED-IGNORES-PREFIX`,
P1 — has an approved design in implementation, and the architecture absorbs either outcome through
the allowed-failure mechanism it needs anyway. No priority changes are proposed: nothing here meets
the P0 criteria of `DOCS_STRUCTURE_GUIDE.md` §4.4, since the suite discovers divergences rather than
suffering from them.

## Data Structures

### `Capability` — the shared vocabulary

```rust
/// What a store can do. This enum **is** the vocabulary: a variant is a capability ID in
/// `STORE_IMPLEMENTATION_GUIDE.md` and a row in a store's status matrix.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Capability {
    /// `set` and `set_metadata` — the store accepts writes.
    Write,
    /// `remove` — a single key can be deleted.
    Remove,
    /// `is_dir` and `listdir` answer meaningfully; the key space has a directory structure.
    Directories,
    /// `makedir` creates a directory that persists with no children.
    ExplicitDirectories,
    /// `removedir` removes a directory and its subtree.
    RemoveDirectories,
    /// Metadata written with `set_metadata` is read back, rather than derived on the fly.
    StoredMetadata,
    /// `keys()` enumerates the store.
    EnumerateKeys,
}
```

### `StoreCapabilities` — a store's answers

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct StoreCapabilities {
    pub write: bool,
    pub remove: bool,
    pub directories: bool,
    pub explicit_directories: bool,
    pub remove_directories: bool,
    pub stored_metadata: bool,
    pub enumerate_keys: bool,
}
```

**Deliberately no `Default`.** A fixture must name every field, so adding a capability is a compile
error at every fixture rather than a silent `false` that skips new rules and still reports green.
This is the "no default match arm" rule applied to a struct, and it is aimed squarely at the
vacuous-conformance failure `LANGUAGE-INTEGRATION_GUIDE.md` §3 describes.

`StoreCapabilities::has(&self, c: Capability) -> bool` matches `Capability` exhaustively, so the
enum and the struct cannot drift.

### `SafetyLevel`

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum SafetyLevel {
    ReadOnly,      // reads and listings only
    CreateOnly,    // may create a key that does not exist
    Scratch,       // may modify or remove keys this run created
    Unrestricted,  // no restriction
}
```

**Variant order is load-bearing**: `Ord` derives from it, and the gate is `fixture.safety_level() >=
rule.meta.min_level`. Reordering the variants silently changes which rules run. Documented on the
type.

### `KeyRequest` — the precondition vocabulary

A rule never invents a key name. It asks for one, and a fixture whose store cannot supply it
declines with a reason. This is what lets a store keyed by numeric database row IDs, with no
directories, participate at all.

```rust
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum KeyRequest {
    /// One key that does not exist and the rule may create.
    Fresh,
    /// `count` fresh keys in one directory.
    FreshSiblings { count: usize },
    /// Two fresh keys where one name is a proper prefix of the other (`sub`, `subway`).
    /// The sibling rule's whole subject.
    FreshPrefixPair,
    /// A fresh key at least `depth` segments below the store's prefix.
    FreshNested { depth: usize },
    /// A key that already holds data — the read-only path's only source of subjects.
    Existing,
    /// A directory that already exists.
    ExistingDirectory,
    /// A key this store must refuse: `is_supported` is false for it.
    Unsupported,
}
```

**No `#[non_exhaustive]`, on purpose.** An out-of-tree fixture matching this enum *should* fail to
compile when a precondition is added — otherwise it silently declines a rule that was meant to run,
and the report loses a check without saying so. Adding a variant is a deliberate breaking change,
and the guide says so.

### `RuleMeta` and `Rule`

```rust
pub struct RuleMeta {
    pub id: &'static str,                    // "sibling01"
    pub title: &'static str,
    pub contract: &'static str,              // "STORE_SEMANTICS.md §1"
    pub requires: &'static [Capability],
    pub min_level: SafetyLevel,
}

/// A rule body. Boxed rather than `async fn` in a static: function pointers are const-constructible
/// and `async fn` is not.
pub type RuleFn = for<'a> fn(&'a dyn Fixture) -> BoxFuture<'a, RuleOutcome>;

pub struct Rule { pub meta: RuleMeta, pub run: RuleFn }

pub fn rules() -> &'static [Rule];
```

`BoxFuture` is `liquers_core::maybe_send::BoxFuture` — `Send`-bounded natively, bare on wasm — so
the same signature compiles on both targets. Each rule body is an ordinary `async fn`; a
declarative `rule!` macro emits the `Rule` entry and the boxing shim, so the boilerplate is written
once.

### `RuleOutcome` and `ConformanceReport`

```rust
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum RuleOutcome {
    Passed,
    Failed { detail: String },
    SkippedCapability { missing: Capability },
    SkippedPrecondition { request: KeyRequest, reason: String },
    NotRunSafetyLevel { required: SafetyLevel },
    Blocked { issue: String, detail: String },
    Errored { error_type: ErrorType, message: String },
}
```

`Errored` carries `ErrorType` and the message separately rather than an `Error`, so the report stays
plainly serializable and a rule cannot smuggle a payload into it.

**A rule body is written with `?`, and still cannot return `Err`.** `RuleFn` returns
`BoxFuture<'_, RuleOutcome>` with no `Result` in sight, which would make every store call a
four-line `match`. The idiomatic resolution is a `From` impl and a two-part rule:

```rust
impl From<Error> for RuleOutcome {
    fn from(e: Error) -> Self {
        RuleOutcome::Errored { error_type: e.error_type, message: e.message }
    }
}

// The body may use `?`: its error type *is* an outcome.
async fn remove01_body(f: &dyn Fixture) -> Result<(), RuleOutcome> { ... }

// The rule the registry holds cannot fail.
async fn remove01(f: &dyn Fixture) -> RuleOutcome {
    match remove01_body(f).await { Ok(()) => RuleOutcome::Passed, Err(outcome) => outcome }
}
```

The `rule!` macro emits the wrapper, so a rule author writes only the body. `?` on a store call
becomes `Errored`; `return Err(failed("..."))` is how a rule reports a contract violation, which
keeps the two distinguishable — a store that errored is not a store that disagreed.

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReportEntry { pub id: String, pub title: String, pub contract: String, pub outcome: RuleOutcome }

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConformanceReport {
    pub store: String,
    pub capabilities: StoreCapabilities,
    pub level: SafetyLevel,
    pub entries: Vec<ReportEntry>,
    /// Keys the run created, whether or not they survived.
    pub created: Vec<Key>,
    /// Keys that were still present after `cleanup` — what this run left in the store.
    pub residue: Vec<Key>,
}
```

**`residue` is not bookkeeping; it is the level-2 safety requirement.** At `CreateOnly` a rule may
create and may not remove, so `cleanup` can remove nothing and every created key survives by
design. A run that leaves keys behind without saying which is a slow leak with no record, so
`run_all` re-checks each created key with `contains` after `cleanup` — a read, permitted at every
level — and the tool prints the residue prominently rather than in a trailer. At `Scratch` and above
a non-empty `residue` means cleanup did not do its job, which is worth seeing too.

`ConformanceReport` implements `Display` for the human form and derives serde for the tool's
`--format yaml|json` and for generating the guide's status matrix.

## Trait Implementations

### Trait: `Fixture` (new, in `store_conformance::fixture`)

```rust
#[cfg_attr(not(target_arch = "wasm32"), async_trait)]
#[cfg_attr(target_arch = "wasm32", async_trait(?Send))]
pub trait Fixture {
    /// The store under test.
    fn store(&self) -> &dyn AsyncStore;
    /// What this store claims it can do.
    fn capabilities(&self) -> StoreCapabilities;
    /// How much this fixture permits a rule to do.
    fn safety_level(&self) -> SafetyLevel;
    /// Name for the report — the store type and any distinguishing configuration.
    fn label(&self) -> String;
    /// Keys satisfying a precondition, or a reason this store cannot supply them.
    async fn keys_for(&self, request: &KeyRequest) -> Result<Vec<Key>, Unavailable>;
    /// Record a key this run created. A rule calls this immediately after a successful create.
    ///
    /// Sync, so no lock is held across an `.await`. The fixture is the only thing that can know
    /// what to clean up and what was left behind, which is why the record lives here rather than
    /// in the report.
    fn record_created(&self, key: &Key);
    /// Every key `record_created` was told about, in creation order.
    fn created_keys(&self) -> Vec<Key>;
    /// Best-effort removal of what the run created. Never fails the report.
    async fn cleanup(&self) {}
}

pub struct Unavailable { pub reason: String }
```

Object-safe: no generic methods, no `Self` by value, no associated types. The suite holds
`&dyn Fixture`.

`keys_for` returns *names*; it does not create the keys, except for `Existing` and
`ExistingDirectory`, where the subject must already be present and the fixture is the only thing
that can put it there — that is how a read-only store (`FetchStore`) has anything to be tested
against.

### Trait: `AsyncStore` (unchanged)

No signature changes. The suite tests the trait as it is; the three ⚠ rows are resolved by changing
`AsyncMemoryStore` and the doc comments, not the shape.

### Trait: `StoreFactory` (additive extension, in `liquers-core::store_factory`)

```rust
    /// Build a throwaway, empty store of this type for conformance testing, if this factory can.
    ///
    /// The default is `Ok(None)` — "this factory cannot make a scratch store" — so no existing
    /// implementor changes. A factory that can (a temporary directory, a scratch prefix, a fresh
    /// table) returns a fixture that owns whatever must be cleaned up.
    #[cfg(feature = "store-conformance")]
    fn create_fixture(
        &self,
        config: &StoreConfig,
    ) -> Result<Option<Box<dyn crate::store_conformance::Fixture>>, Error> {
        let _ = config;
        Ok(None)
    }
```

Additive and defaulted, per the "extend, don't mutate" rule — `liquers-py` and every other
implementor keep compiling. The `#[cfg]` on the method is required because its return type only
exists under the feature.

**Known limitation:** `create_fixture` is synchronous, matching `create`. A factory needing async
setup (provisioning a scratch bucket) cannot participate; making the factory async is a separate
change and out of scope. Recorded in the guide.

## Generic Parameters & Bounds

**None.** The suite is entirely `dyn`-based: `&dyn Fixture`, `&dyn AsyncStore`, `Box<dyn Fixture>`.
Rules must be storable in a `&'static [Rule]` and callable across crates and both targets, which a
generic `F: Fixture` would prevent (no `&'static [Rule]` of monomorphised bodies, no object-safe
`create_fixture`). Dynamic dispatch costs nothing that matters: every rule performs store I/O.

The only bound anywhere is the one `AsyncStore` already carries (`MaybeSend + MaybeSync`), inherited
rather than restated.

## Sync vs Async Decisions

| Item | Sync/Async | Rationale |
|---|---|---|
| Rule bodies | async | They call `AsyncStore`. |
| `run_all`, `run_rule` | async | Fold over async rules. |
| `Fixture::keys_for` | async | A fixture may have to ask its backend what exists. |
| `Fixture::cleanup` | async | Removal is store I/O. |
| `Fixture::store/capabilities/safety_level/label` | sync | Pure accessors; making them async would force `.await` at every rule's first line for nothing. |
| `StoreFactory::create_fixture` | sync | Matches `create`. See the limitation above. |
| Report inspection (`conformant`, `counts`, `assert_conformant`) | sync | Pure data. |

**No runtime is named anywhere in the module.** No `tokio`, no `wasm_bindgen_test`, no
`#[tokio::test]`. The harness lives in the consuming crate; this is what lets `liquers-web` run the
same rules under `wasm_bindgen_test`.

## Function Signatures

### Module: `liquers_core::store_conformance`

```rust
pub fn rules() -> &'static [Rule];
pub fn rule(id: &str) -> Option<&'static Rule>;

/// Run every rule against the fixture. Never panics, never returns `Err`: the report is the result.
pub async fn run_all(fixture: &dyn Fixture) -> ConformanceReport;

/// Run one rule by ID. `None` if the ID is unknown.
pub async fn run_rule(fixture: &dyn Fixture, id: &str) -> Option<ReportEntry>;

impl ConformanceReport {
    pub fn counts(&self) -> OutcomeCounts;
    pub fn failures(&self) -> impl Iterator<Item = &ReportEntry>;
    /// How many rules were not run at each level, and which level would run them.
    pub fn not_run_by_level(&self) -> Vec<(SafetyLevel, usize)>;
    /// The assertion a suite makes. `allowed` names rules this store is permitted to fail, each
    /// with the issue that permits it.
    pub fn assert_conformant(&self, allowed: &[AllowedFailure]) -> Result<(), Error>;
}

pub struct AllowedFailure { pub rule: &'static str, pub issue: &'static str }
```

**`assert_conformant` fails in both directions.** A rule that failed and is not allowed is an
error; **a rule that is allowed and passed is also an error**, naming the entry to delete. Without
that, an ignore list written for a good reason outlives the reason — the same discipline
`LANGUAGE-INTEGRATION_GUIDE.md` §3 imposes with its reversing conditions, made mechanical.

### Rule-authoring constraints (normative for Phase 3/4)

- A rule **asserts on `ErrorType`, never on message text.** `CORE-ERROR-STORE-NAME-NOT-STRUCTURED`
  means the store's name is interpolated into the message, so message text is neither stable nor
  portable.
- A rule **checks before it mutates** at `Scratch`: `contains`/`is_dir` first, and abandon with
  `SkippedPrecondition` if the key is already there. This is Phase 1 decision 9 — upheld by the
  rules on trust, with no guard wrapping the store.
- A rule **never panics** and never returns `Err`. An unexpected store error becomes
  `RuleOutcome::Errored`.
- A rule **records every key it creates**, through `Fixture::record_created`, immediately after the
  create succeeds. This is what makes `cleanup` and the residue report possible; a rule that
  creates without recording leaks silently, which is the one failure the safety levels exist to
  prevent.
- A rule **declares every capability it needs**; `run_all` checks `requires` before calling it, so
  a rule never has to test for its own applicability.

### Module: `liquers-store` binary `liquers-store-check`

```text
liquers-store-check --config <store.yaml> [--store <prefix>] [--rule <id>]...
                    [--level read-only|create-only|scratch|unrestricted]
                    [--format text|yaml|json]
liquers-store-check --scratch <store-type> [--arg k=v]...   # factory-built fixture
```

Defaults follow provenance, as Phase 1 recommended and this phase decides:
**`--config` defaults to `read-only`** (it is somebody's data); **`--scratch` defaults to
`scratch`** (the factory just made it). Raising the level on `--config` is always explicit.

**At `--level create-only` the tool prints the residue list before the summary**, because that level
cannot clean up after itself and the operator now owns whatever it made. Exit codes match
`liquers-validate`: **0** conformant · **1** non-conformant · **2** invocation or setup failure. The tool prints the resolved `StoreConfig` before running, so a mis-parsed option
(`STORE-OPENDAL-LIST-OPTION-MISPARSED`) is visible as a setup problem rather than as a store defect,
and it prints the not-run counts per level so a clean `read-only` report cannot be mistaken for
conformance.

## Integration Points

### Crate: `liquers-core`

- New module directory `src/store_conformance/` — `mod.rs`, `fixture.rs`, `report.rs`,
  `rules/{sibling,directories,removal,absence,prefix,keyshape,sidecar,enumerate}.rs`.
- `src/store_factory.rs` — the additive `create_fixture` method.
- `src/store.rs` — `AsyncMemoryStore::keys` changed for decision 1; `removedir` doc comments
  corrected to the postcondition.
- `Cargo.toml` — `store-conformance = []` feature, **not in `default`**. Cargo unifies features
  additively across the workspace and `liquers-lib`/`liquers-store` depend on core with defaults on,
  so anything in `default` is unavoidable for the wasm bundle — the same reasoning that keeps
  `entities-html5` out. `serde`/`serde_derive` are already unconditional dependencies, so the report
  adds none.
- `tests/store_conformance_CONF.rs` — fixtures and suites for `AsyncMemoryStore`, `AsyncFileStore`,
  `AsyncStoreRouter`, and the trait defaults (`MinimalStore`).
- `tests/conformance_docs_CONF.rs` — the synchronization test.

### Crate: `liquers-store`

- `Cargo.toml` — `store-conformance` forwarding to `liquers-core/store-conformance`; a **new** `cli`
  feature with `clap` as a **new optional dependency** (`liquers-store` has neither today, unlike
  `liquers-core`); an explicit `[[bin]]` with `required-features = ["cli", "store-conformance"]`,
  because an auto-discovered binary cannot carry one — the same reason `liquers-validate` has an
  explicit block. `cli` stays out of `default`, matching `liquers-core`.
- `src/bin/liquers_store_check.rs` — the tool.
- `src/store_factory.rs` — `create_fixture` for the OpenDAL types that can make a scratch location.
- `tests/store_conformance_CONF.rs` — `AsyncOpenDALStore` over the `fs` service in a temp directory.

### Crate: `liquers-web`

- `Cargo.toml` — dev-dependency on `liquers-core` with `store-conformance` (it already depends on
  core; the feature is added for tests).
- `tests/store_conformance_CONF.rs` — `LocalStorageStore` (behind `browser-tests`, since
  `localStorage` needs a real browser), `FetchStore` (read-only, `Existing` keys served by the test
  harness), `JsStore` (a stub JS object). Driven by `#[wasm_bindgen_test]`.

### Crate: `liquers-py`

Untouched. `create_fixture` is defaulted, so its `StoreFactory` usage is unaffected.

### Dependencies

**None added.** `serde`, `serde_derive`, `async-trait` and `futures` are already core dependencies;
`clap` is already an optional core dependency behind `cli` and gains a `liquers-store` counterpart.
Temporary directories in tests use `std::env::temp_dir()` with a unique name, matching
`store_key_absolute.rs` — no `tempfile` dependency.

## Documentation Architecture

### Reference Plan

**Extend** `specs/reference/STORE_SEMANTICS.md` (kind: reference, audience: internal, area:
`core/store, store/backends, web`):

- §5 — replace the ⚠ with the postcondition: `Ok(())` means the directory does not exist
  afterwards; recursion follows; the trait default's `Err(KeyNotSupported)` is legitimate for a
  store declaring no `RemoveDirectories`.
- §6 — remove the `AsyncMemoryStore::is_supported` ⚠ when
  `async-memory-store-prefix-support` lands, or keep it citing that design.
- §9 — replace the ⚠ with the rule: `keys()` returns data keys, directories and the prefix, and
  **every returned key starts with the prefix**.
- Every section's *Enforced by* line becomes rule IDs from `rules()`.
- Rules stated trait-neutrally where they hold for both traits, with a note that only `AsyncStore`
  must satisfy them today (`CORE-SYNC-STORE-TRAIT-OBSOLETE`).
- `## History` row and `reviewed:` bump in the same commit (§9.2).

**Create** `specs/reference/CONFORMANCE_TERMS.md` (kind: reference, audience: internal, area:
`docs`): the requirement levels, the implementation states (`NA`, `NS`, `DESIGN`, `PARTIAL`,
`COMPLETE`, `BLOCKED`, `CONFORMANT`), and the `NA` discipline — argued, with a reversing condition.
**Extraction scope, checked against the source rather than estimated.**
`LANGUAGE-INTEGRATION_GUIDE.md:81–100` is a self-contained 20-line block — the three requirement
levels and the seven implementation states — whose only language-specific reference is `ASYNCQ` as
an example of `Profile`. That block moves; the guide keeps a link and re-states its `Profile`
example in its own terms.

Two neighbouring passages **stay where they are**: the dependency-constraint paragraph (`:104`) is
written in terms of *hard* and *soft dependency*, which are the language guide's own; and the `NA`
discipline (`:177–201`) is carried almost entirely by language-specific examples (`ASYNCCMD01`,
`PACKAGE06`, `STUBS02`). `CONFORMANCE_TERMS.md` states the `NA` *principle* — argued, with a
reversing condition — and each guide keeps its own worked version. Extracting the examples would
leave both guides poorer.

> Phase 1 preferred extraction and permitted duplication as a fallback. With the boundary now
> measured at 20 lines and one example, extraction is the recommendation. If review still prefers
> not to touch that guide, duplicating the block into the new one is the fallback, at the cost of
> two copies that will drift.

### Guide Plan

**Create** `specs/guides/STORE_IMPLEMENTATION_GUIDE.md` (kind: guide, audience: internal, area:
`core/store, store/backends, web`), structured after `LANGUAGE-INTEGRATION_GUIDE.md`:

1. Purpose and scope; what a *store*, a *backend*, an *internal key* and a *fixture* are.
2. Terminology — links `CONFORMANCE_TERMS.md`, adds store-specific terms.
3. How to use the capabilities: the `Capability` vocabulary, the status matrix, `NA` discipline.
4. **What implementing a store means** — new struct only, extend a factory, write a `StoreFactory`,
   chain it into `default_store_factory()`. Links `STORE_FACTORY_GUIDE.md`; does not repeat it.
5. The design questions, per capability — internal key space; key mapping, round-tripping and
   unrepresentable keys; prefix addressing and the sibling rule; metadata stored or derived and
   which source wins; read-only or writable; restricted key spaces; directory support and the three
   sources of truth; authoritative backends and why they may not keep an index; absence versus
   failure; prefix in the path or stripped; atomicity, concurrency and quota boundaries;
   enumeration cost; error mapping; `openbin` when it exists.
6. **Testing your store** — writing a fixture, the `KeyRequest` vocabulary, the four safety levels,
   what each level buys, `assert_conformant` and allowed failures, and `liquers-store-check`.
7. **Safety precautions** — a temporary folder or throwaway database; treat any store under test as
   expendable; no third-party service unless explicitly permitted, and never one holding data you
   did not create. States plainly that level 3 is rule discipline, not a guarantee. **And that a
   `create-only` run leaves everything it created behind**, by definition — what the residue list
   is for, and that clearing it is the operator's job, not the tool's.
8. **A worked restricted store** — the database-table view: rows as files, numeric IDs, no
   directories. Shows a fixture declining `FreshNested` and `FreshPrefixPair`, and a status matrix
   where many argued `NA`s are the expected outcome rather than a smell.
9. Per-store status matrix for the seven in-tree implementations.

### Other Documents to Create

None.

### New Reference or Guide Documents

`specs/reference/CONFORMANCE_TERMS.md`, `specs/guides/STORE_IMPLEMENTATION_GUIDE.md`. Both get
front matter, a `## History` row and `reviewed:` on creation.

### Existing Documents to Review or Update

| Document | Change |
|---|---|
| `specs/reference/STORE_SEMANTICS.md` | As above. |
| `specs/guides/LANGUAGE-INTEGRATION_GUIDE.md` | §3 status vocabulary → link `CONFORMANCE_TERMS.md`. §STORE direction-2 questions → cross-link the new guide instead of answering twice. |
| `specs/guides/STORE_FACTORY_GUIDE.md` | Cross-link; document `create_fixture`. |
| `specs/reference/STORE_CONFIG_FSD.md` | Cross-link; note `liquers-store-check` as a way to validate a document's stores. |
| `CLAUDE.md` | §"Adding a Store Backend" gains a step (run the suite) and points at the new guide; the two `AsyncStoreWrapper` passages are corrected in passing. |
| `specs/guides/UNITTEST_GUIDE.md` | Left to `DOCS-ASYNC-STORE-WRAPPER-NO-LONGER-EXISTS` unless that lands first. |
| `specs/README.md` | Capability-map entries for the new guide and reference. |
| `specs/index.csv` | Regenerated. |

**Proposed `affects_docs`:** `STORE_SEMANTICS`, `STORE_IMPLEMENTATION_GUIDE`, `CONFORMANCE_TERMS`,
`LANGUAGE-INTEGRATION_GUIDE`, `STORE_FACTORY_GUIDE`, `STORE_CONFIG_FSD`.

### Design and Capability Links

`specs/README.md` gains the guide under store work; `STORE_SEMANTICS.md` links the guide as its
operational counterpart and the guide links back. The three store documents form one path:
**implement** (new guide) → **declare a type** (`STORE_FACTORY_GUIDE.md`) → **configure**
(`STORE_CONFIG_FSD.md`).

### Evidence to Collect During Implementation

Which rules each in-tree store fails on first run (the divergence census the issue predicts); the
runnable-rule count per safety level; the residue a `create-only` run actually leaves; every `NA` a fixture declines and its reason; any rule whose
assertion turned out to pass whatever the store did.

## Relevant Commands

### New Commands

**None.** This project adds no command and no namespace. It touches the store layer, below command
execution; nothing here is reachable from a query, and no `register_command!` invocation changes.

### Relevant Existing Namespaces

**None applicable.** No query is evaluated by the suite or the tool — rules call `AsyncStore`
directly, which is the point: a conformance rule that went through query evaluation would be
testing the interpreter as well as the store. No namespace review is needed, so the usual
Phase 2 question about relevant namespaces has no subject here.

## Web Endpoints

None. `liquers-axum`'s store handlers are unchanged. They remain the reason the sibling rule
matters — `DELETE /api/store/removedir/{*key}` is what made it a P0 — but no route is added or
altered.

## Error Handling

### New Error Types

**None.** Everything uses `liquers_core::error::Error` with existing `ErrorType` variants.

### Error Constructors

- `assert_conformant` returns `Error::general_error(...)` listing the offending rules.
- `create_fixture` implementations return `Error::general_error` or an existing typed constructor;
  never `Error::new`.
- Rules construct no errors at all: they classify the ones the store returns into `RuleOutcome`.
- `Unavailable` is **not** an `Error`. It is a fixture's reasoned decline, which belongs in the
  report as `SkippedPrecondition`, not in the error channel — treating "this store has no
  directories" as an error would put a design fact in a failure path.

## Decisions from Review

1. **Extract `CONFORMANCE_TERMS.md`.** Confirmed. `LANGUAGE-INTEGRATION_GUIDE.md:81–100` moves; the
   dependency-constraint paragraph and the `NA` discipline stay, as measured above.

2. **A duplicated per-store test is deleted only once its replacement is replicated and passing.**
   Not a cleanup at the end — an ordering constraint on the implementation, in three steps per ID:
   1. add the shared rule under the adopted ID;
   2. run it against the same store and see it pass, and see it *fail* when the behaviour it checks
      is broken — a rule that would pass either way replaces nothing;
   3. only then delete the per-store test.

   This needs a per-ID mapping — old test location → new rule → the store it was proving — which
   Phase 3 produces as a table and Phase 4 executes row by row. An ID whose old test covers
   something the shared rule does not is **not** deleted; it is kept and the difference recorded.
   `keyabs12`–`keyabs14` are already known to be in that category.

3. **Implement the `FetchStore` and `JsStore` fixtures**, unless one turns out to qualify on its own
   as `M` or larger — the same threshold as decision 5. Checked rather than assumed, and both look
   comfortably below it:
   - `FetchStore` reads `fetch` off `js_sys::global()` at call time
     (`liquers-web/src/store/fetch.rs:217`), *not* from `web_sys::Window`. A test can therefore
     install a stub `fetch` on the global object and serve an in-memory corpus — no HTTP server, no
     WebDriver, and it runs under Node in the routine loop rather than behind `browser-tests`.
     Its `is_supported` also consults a configured known-key set (`fetch.rs:400`), which is exactly
     the source for `KeyRequest::Existing` on a read-only store.
   - `JsStore` delegates to a JavaScript object, so its fixture is that object: a small stub
     implementing the methods it forwards.

   If either exceeds the threshold once written, it is filed as its own issue, the fixture falls
   back to `ReadOnly` over a hand-placed corpus, and the rules it cannot reach are reported as
   `SkippedPrecondition` rather than quietly absent.
