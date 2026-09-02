//! A behavioural conformance suite for [`AsyncStore`](crate::store::AsyncStore).
//!
//! `AsyncStore` has seven in-tree implementations across three crates. Until this module existed
//! each was tested only against itself, and they did not agree: eleven divergences were enumerated
//! in `STORE-ASYNC-STORE-NO-BEHAVIOURAL-CONFORMANCE-SUITE`, one of which destroyed data. The
//! contract is `specs/reference/STORE_SEMANTICS.md`; this module is that contract made executable,
//! and `specs/guides/STORE_IMPLEMENTATION_GUIDE.md` is the operational counterpart.
//!
//! # Shape
//!
//! A **rule** is one contract claim. It never panics and never returns `Err`: it produces a
//! [`RuleOutcome`], so one run reports *every* divergence rather than stopping at the first — which
//! is what a suite chartered to enumerate divergences needs.
//!
//! A **fixture** is supplied by the caller. This module never constructs a store: there is no
//! universal way to make an empty one, since a filesystem store needs a temporary directory and an
//! HTTP-backed store needs something serving it. A fixture answers three questions — what the store
//! can do ([`StoreCapabilities`]), how much a rule may do to it ([`SafetyLevel`]), and what keys
//! satisfy a precondition ([`KeyRequest`]).
//!
//! Rules **ask the fixture for key names** rather than inventing them. That is what lets a store
//! with a restricted key space — a view onto a database table, keyed by numeric row ID, with no
//! directories — take part at all: it declines the preconditions it cannot meet, with a reason that
//! lands in the report.
//!
//! # Runtime-agnostic on purpose
//!
//! No test attribute, no `tokio`, no `wasm_bindgen_test` appears here. The harness belongs to the
//! consuming crate, which is what lets `liquers-web` run the same rules under `wasm_bindgen_test`
//! while `liquers-core` runs them under `#[tokio::test]`.

use crate::error::{Error, ErrorType};
use crate::maybe_send::BoxFuture;
use crate::query::Key;

pub mod fixture;
pub mod report;
pub mod rules;

pub use fixture::{Fixture, KeyRequest, Unavailable};
pub use report::{AllowedFailure, ConformanceReport, OutcomeCounts, ReportEntry, RuleOutcome};

/// What a store can do.
///
/// This enum **is** the vocabulary: a variant is a capability ID in
/// `STORE_IMPLEMENTATION_GUIDE.md` and a row in a store's status matrix. Adding one is a
/// documentation change as much as a code change.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Capability {
    /// `set` and `set_metadata` — the store accepts writes.
    Write,
    /// `remove` — a single key can be deleted.
    Remove,
    /// `is_dir` and `listdir` answer meaningfully; the key space has a directory structure.
    Directories,
    /// Directories are *derived* from their children, so one retires when its last child goes.
    ///
    /// Distinct from [`Capability::Directories`]: a real filesystem has directories that persist
    /// after their last file is removed, because the directory is an object in its own right.
    /// `STORE_SEMANTICS.md` §2 calls these the three sources of directory truth; only the
    /// index-derived kind retires.
    DerivedDirectories,
    /// `makedir` creates a directory that persists with no children.
    ExplicitDirectories,
    /// `removedir` removes a directory and its subtree.
    RemoveDirectories,
    /// Metadata written with `set_metadata` is read back, rather than derived on the fly.
    StoredMetadata,
    /// `keys()` enumerates the store.
    EnumerateKeys,
}

impl Capability {
    /// Every capability, for exhaustive iteration by the negative rules and the report.
    pub fn all() -> &'static [Capability] {
        &[
            Capability::Write,
            Capability::Remove,
            Capability::Directories,
            Capability::DerivedDirectories,
            Capability::ExplicitDirectories,
            Capability::RemoveDirectories,
            Capability::StoredMetadata,
            Capability::EnumerateKeys,
        ]
    }
}

/// A store's answers to [`Capability`].
///
/// **Deliberately no `Default`.** A fixture must name every field, so adding a capability is a
/// compile error at every fixture rather than a silent `false` that skips the new rules and still
/// reports green. This is the "no default match arm" rule applied to a struct, and it is aimed at
/// the one failure mode a conformance suite cannot tolerate: reporting safety it never checked.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct StoreCapabilities {
    pub write: bool,
    pub remove: bool,
    pub directories: bool,
    pub derived_directories: bool,
    pub explicit_directories: bool,
    pub remove_directories: bool,
    pub stored_metadata: bool,
    pub enumerate_keys: bool,
}

impl StoreCapabilities {
    /// Whether this store claims `capability`.
    ///
    /// Matches [`Capability`] exhaustively, so the enum and this struct cannot drift apart.
    pub fn has(&self, capability: Capability) -> bool {
        match capability {
            Capability::Write => self.write,
            Capability::Remove => self.remove,
            Capability::Directories => self.directories,
            Capability::DerivedDirectories => self.derived_directories,
            Capability::ExplicitDirectories => self.explicit_directories,
            Capability::RemoveDirectories => self.remove_directories,
            Capability::StoredMetadata => self.stored_metadata,
            Capability::EnumerateKeys => self.enumerate_keys,
        }
    }
}

/// How much a rule may do to the store under test.
///
/// **Variant order is load-bearing.** `Ord` derives from it and the gate is
/// `fixture.safety_level() >= rule.min_level`, so reordering these silently changes which rules
/// run.
///
/// There is deliberately no `Unrestricted`: every rule in the inventory is satisfied at
/// [`SafetyLevel::Scratch`] or below, so a fourth level could only permit damage no check asked
/// for. A fixture that cannot record what it creates declares [`SafetyLevel::CreateOnly`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum SafetyLevel {
    /// Reads and listings. No mutation of any kind.
    ReadOnly,
    /// May create a key that does not exist. May not overwrite, remove, or `removedir`.
    ///
    /// A run at this level **cannot clean up after itself** — everything it creates survives, by
    /// definition. [`ConformanceReport::residue`] is what makes that visible rather than a leak.
    CreateOnly,
    /// May modify or remove keys *this run created*, and nothing that was already there.
    ///
    /// Upheld by the rules themselves — each checks whether a key exists before creating it — not
    /// by a guard wrapping the store. A limit, not a guarantee: check-then-write is not atomic, and
    /// a buggy rule can breach it. Run against a scratch or throwaway store.
    Scratch,
}

/// What a rule is, for the report and for gating.
#[derive(Debug, Clone, Copy)]
pub struct RuleMeta {
    /// Stable ID, e.g. `sibling01`. Cited by `STORE_SEMANTICS.md` and the implementation guide;
    /// the three sets are asserted equal by a test.
    pub id: &'static str,
    /// One line, in the contract's own words.
    pub title: &'static str,
    /// Where the claim is written down, e.g. `STORE_SEMANTICS.md §1`.
    pub contract: &'static str,
    /// Capabilities the store must have for this rule to apply.
    pub requires: &'static [Capability],
    /// The lowest level at which this rule can run.
    pub min_level: SafetyLevel,
}

/// A rule body.
///
/// A function pointer returning a boxed future rather than an `async fn`: function pointers are
/// const-constructible, so the whole inventory lives in a `&'static [Rule]`. [`BoxFuture`] is
/// `Send`-bounded natively and bare on wasm, so one signature compiles on both targets.
pub type RuleFn = for<'a> fn(&'a dyn Fixture) -> BoxFuture<'a, RuleOutcome>;

/// One contract claim, and the code that checks it.
pub struct Rule {
    pub meta: RuleMeta,
    pub run: RuleFn,
}

/// Every rule, in execution order.
pub fn rules() -> &'static [Rule] {
    rules::all()
}

/// One rule by ID, or `None` if no such rule exists.
pub fn rule(id: &str) -> Option<&'static Rule> {
    rules().iter().find(|r| r.meta.id == id)
}

/// Run every rule against `fixture`, then clean up and account for what was left behind.
///
/// Never panics and never returns `Err`: the report is the result. Rules run in [`rules()`] order.
pub async fn run_all(fixture: &dyn Fixture) -> ConformanceReport {
    let mut entries = Vec::with_capacity(rules().len());
    for rule in rules() {
        entries.push(run_one(fixture, rule).await);
    }

    let created = fixture.created_keys();
    fixture.cleanup().await;

    // Residue is what survived cleanup. `contains` is a read, so this is permitted at every level.
    let mut residue = Vec::new();
    for key in &created {
        if matches!(fixture.store().contains(key).await, Ok(true)) {
            residue.push(key.clone());
        }
    }

    ConformanceReport {
        store: fixture.label(),
        capabilities: fixture.capabilities(),
        level: fixture.safety_level(),
        entries,
        created,
        residue,
    }
}

/// Run one rule by ID. `None` if the ID is unknown.
pub async fn run_rule(fixture: &dyn Fixture, id: &str) -> Option<ReportEntry> {
    let rule = rule(id)?;
    Some(run_one(fixture, rule).await)
}

/// Gate a single rule on capability and level, then run it if it applies.
async fn run_one(fixture: &dyn Fixture, rule: &'static Rule) -> ReportEntry {
    let capabilities = fixture.capabilities();
    let outcome = match rule
        .meta
        .requires
        .iter()
        .find(|c| !capabilities.has(**c))
    {
        // Capability gating comes first: a store with no directories should never be asked for a
        // directory key, so the rule is not called at all.
        Some(missing) => RuleOutcome::SkippedCapability { missing: *missing },
        None if fixture.safety_level() < rule.meta.min_level => RuleOutcome::NotRunSafetyLevel {
            required: rule.meta.min_level,
        },
        None => (rule.run)(fixture).await,
    };
    ReportEntry {
        id: rule.meta.id.to_owned(),
        title: rule.meta.title.to_owned(),
        contract: rule.meta.contract.to_owned(),
        subject: Vec::new(),
        outcome,
    }
}

/// Classify a store error a rule did not expect.
///
/// Rules never assert on message text — a store's name is interpolated into it
/// (`CORE-ERROR-STORE-NAME-NOT-STRUCTURED`), so the text is neither stable nor portable. The
/// `ErrorType` is what a rule may reason about.
impl From<Error> for RuleOutcome {
    fn from(error: Error) -> Self {
        // `Error` derefs to a boxed payload, so the message is cloned rather than moved.
        RuleOutcome::Errored {
            error_type: error.error_type,
            message: error.message.clone(),
        }
    }
}

/// A rule's disagreement with the contract, with the keys it was looking at.
pub fn failed(detail: impl Into<String>) -> RuleOutcome {
    RuleOutcome::Failed {
        detail: detail.into(),
    }
}

/// Helper: the `ErrorType` a store returned, or `None` if it did not fail.
pub(crate) fn error_type_of<T>(result: &Result<T, Error>) -> Option<ErrorType> {
    match result {
        Ok(_) => None,
        Err(e) => Some(e.error_type),
    }
}

/// Ask the fixture for keys, turning a decline into the outcome that reports it.
pub(crate) async fn keys_for(
    fixture: &dyn Fixture,
    request: KeyRequest,
) -> Result<Vec<Key>, RuleOutcome> {
    match fixture.keys_for(&request).await {
        Ok(keys) => Ok(keys),
        Err(Unavailable { reason }) => Err(RuleOutcome::SkippedPrecondition { request, reason }),
    }
}
