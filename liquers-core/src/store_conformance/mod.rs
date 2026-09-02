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

pub use fixture::{Fixture, GenericFixture, KeyRequest, Unavailable};
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
    /// A capability this rule runs only when the store declares it **absent**.
    ///
    /// Capability gating alone makes a `false` an exit: a fully writable store could declare
    /// everything `false`, skip every write, removal and enumeration check, and still satisfy
    /// `assert_conformant` — which contradicts what [`Fixture::capabilities`] promises. A refuting
    /// rule turns each `false` into a claim that can fail.
    pub refutes: Option<Capability>,
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
pub(crate) async fn run_one(fixture: &dyn Fixture, rule: &Rule) -> ReportEntry {
    let capabilities = fixture.capabilities();
    // A refuting rule is the inverse of the gate below: it applies precisely when the capability is
    // declared absent, and checks the store really does refuse.
    if let Some(refuted) = rule.meta.refutes {
        let outcome = if capabilities.has(refuted) {
            RuleOutcome::SkippedCapabilityPresent { present: refuted }
        } else if fixture.safety_level() < rule.meta.min_level {
            RuleOutcome::NotRunSafetyLevel {
                required: rule.meta.min_level,
            }
        } else {
            (rule.run)(fixture).await
        };
        return entry_for(rule, outcome);
    }
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
    entry_for(rule, outcome)
}

/// Build a report entry, lifting any failing keys onto it.
fn entry_for(rule: &Rule, outcome: RuleOutcome) -> ReportEntry {
    // The report must say *where* and not only *what*.
    let subject = match &outcome {
        RuleOutcome::Failed { subject, .. } => subject.clone(),
        RuleOutcome::Passed
        | RuleOutcome::SkippedCapability { .. }
        | RuleOutcome::SkippedCapabilityPresent { .. }
        | RuleOutcome::SkippedPrecondition { .. }
        | RuleOutcome::NotRunSafetyLevel { .. }
        | RuleOutcome::Blocked { .. }
        | RuleOutcome::Errored { .. } => Vec::new(),
    };
    ReportEntry {
        id: rule.meta.id.to_owned(),
        title: rule.meta.title.to_owned(),
        contract: rule.meta.contract.to_owned(),
        subject,
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

/// A rule's disagreement with the contract, on no particular key.
pub fn failed(detail: impl Into<String>) -> RuleOutcome {
    RuleOutcome::Failed {
        detail: detail.into(),
        subject: Vec::new(),
    }
}

/// A rule's disagreement with the contract, naming the keys it was looking at.
pub fn failed_at(detail: impl Into<String>, subject: Vec<Key>) -> RuleOutcome {
    RuleOutcome::Failed {
        detail: detail.into(),
        subject,
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

#[cfg(test)]
mod tests {
    //! `H1`–`H8` — the harness itself, proven before any real rule exists.
    //!
    //! A rule that fails because the gating is wrong is indistinguishable from a store that
    //! genuinely diverges, and "which stores diverge" is this suite's entire output. So the
    //! machinery is tested first, against stub rules whose behaviour is known exactly.
    //!
    //! `H6` and `H7` are different in kind: they are properties of *rules*, not of the harness.
    //! Their checkers are built and proven here — including against a deliberately bad stub rule,
    //! so the checker itself is not vacuous — and pointed at the real inventory in the step that
    //! introduces it.

    use super::*;
    use crate::metadata::{Metadata, MetadataRecord};
    use crate::store::AsyncStore;
    use async_trait::async_trait;
    use std::sync::Mutex;

    // ---------------------------------------------------------------- stub store

    /// What the stub store was asked to do, in order. `H6` reads this.
    #[derive(Debug, Clone, PartialEq, Eq)]
    enum Call {
        Read(Key),
        Mutate(Key),
    }

    /// A minimal in-memory store that records what it was asked to do.
    #[derive(Default)]
    struct StubStore {
        data: Mutex<Vec<(Key, Vec<u8>)>>,
        calls: Mutex<Vec<Call>>,
    }

    impl StubStore {
        fn calls(&self) -> Vec<Call> {
            self.calls.lock().map(|c| c.clone()).unwrap_or_default()
        }
        fn written(&self) -> Vec<Key> {
            self.calls()
                .into_iter()
                .filter_map(|c| match c {
                    Call::Mutate(k) => Some(k),
                    Call::Read(_) => None,
                })
                .collect()
        }
        fn note(&self, call: Call) {
            if let Ok(mut c) = self.calls.lock() {
                c.push(call);
            }
        }
    }

    #[cfg_attr(not(target_arch = "wasm32"), async_trait)]
    #[cfg_attr(target_arch = "wasm32", async_trait(?Send))]
    impl AsyncStore for StubStore {
        async fn get(&self, key: &Key) -> Result<(Vec<u8>, Metadata), Error> {
            self.note(Call::Read(key.clone()));
            let data = self.data.lock().map_err(|_| Error::general_error("poisoned".to_owned()))?;
            match data.iter().find(|(k, _)| k == key) {
                Some((_, bytes)) => Ok((
                    bytes.clone(),
                    Metadata::MetadataRecord(MetadataRecord::new()),
                )),
                None => Err(Error::key_not_found(key)),
            }
        }

        async fn set(&self, key: &Key, data: &[u8], _metadata: &Metadata) -> Result<(), Error> {
            self.note(Call::Mutate(key.clone()));
            let mut store = self.data.lock().map_err(|_| Error::general_error("poisoned".to_owned()))?;
            store.retain(|(k, _)| k != key);
            store.push((key.clone(), data.to_vec()));
            Ok(())
        }

        async fn set_metadata(&self, key: &Key, _metadata: &Metadata) -> Result<(), Error> {
            self.note(Call::Mutate(key.clone()));
            Ok(())
        }

        async fn contains(&self, key: &Key) -> Result<bool, Error> {
            self.note(Call::Read(key.clone()));
            let data = self.data.lock().map_err(|_| Error::general_error("poisoned".to_owned()))?;
            Ok(data.iter().any(|(k, _)| k == key))
        }

        async fn is_dir(&self, key: &Key) -> Result<bool, Error> {
            self.note(Call::Read(key.clone()));
            Ok(false)
        }

        async fn remove(&self, key: &Key) -> Result<(), Error> {
            self.note(Call::Mutate(key.clone()));
            let mut store = self.data.lock().map_err(|_| Error::general_error("poisoned".to_owned()))?;
            store.retain(|(k, _)| k != key);
            Ok(())
        }
    }

    // ---------------------------------------------------------------- stub fixture

    struct StubFixture {
        store: StubStore,
        capabilities: StoreCapabilities,
        level: SafetyLevel,
        created: Mutex<Vec<Key>>,
    }

    /// Every field named — that is the point of `StoreCapabilities` having no `Default`.
    fn all_capabilities() -> StoreCapabilities {
        StoreCapabilities {
            write: true,
            remove: true,
            directories: true,
            derived_directories: true,
            explicit_directories: true,
            remove_directories: true,
            stored_metadata: true,
            enumerate_keys: true,
        }
    }

    impl StubFixture {
        fn new(capabilities: StoreCapabilities, level: SafetyLevel) -> Self {
            StubFixture {
                store: StubStore::default(),
                capabilities,
                level,
                created: Mutex::new(Vec::new()),
            }
        }
    }

    #[cfg_attr(not(target_arch = "wasm32"), async_trait)]
    #[cfg_attr(target_arch = "wasm32", async_trait(?Send))]
    impl Fixture for StubFixture {
        fn store(&self) -> &dyn AsyncStore {
            &self.store
        }
        fn capabilities(&self) -> StoreCapabilities {
            self.capabilities
        }
        fn safety_level(&self) -> SafetyLevel {
            self.level
        }
        fn expected_prefix(&self) -> Key {
            Key::new()
        }
        fn label(&self) -> String {
            "stub".to_owned()
        }
        async fn keys_for(&self, request: &KeyRequest) -> Result<Vec<Key>, Unavailable> {
            match request {
                KeyRequest::Fresh => Ok(vec![Key::new().join("fresh.txt")]),
                _ => Err(Unavailable::new("the stub supplies only Fresh")),
            }
        }
        fn record_created(&self, key: &Key) {
            if let Ok(mut c) = self.created.lock() {
                c.push(key.clone());
            }
        }
        fn created_keys(&self) -> Vec<Key> {
            self.created.lock().map(|c| c.clone()).unwrap_or_default()
        }
    }

    // ---------------------------------------------------------------- stub rules

    /// Passes, but **touches the store first** so that "was this rule called?" is observable on
    /// the fixture rather than through global state.
    ///
    /// An earlier draft recorded the call in a global keyed by rule ID, and the body always wrote
    /// the same literal — so `assert!(!was_called("needs_write"))` held whether or not the rule
    /// ran. That is the vacuous-assertion trap this suite exists to catch, met in its own tests.
    async fn stub_pass(f: &dyn Fixture) -> RuleOutcome {
        let _ = f.store().contains(&Key::new().join("probe.txt")).await;
        RuleOutcome::Passed
    }
    async fn stub_fail(_f: &dyn Fixture) -> RuleOutcome {
        failed("the store disagreed")
    }
    /// Well-behaved: checks before it mutates, and records what it created.
    async fn stub_good_write(f: &dyn Fixture) -> RuleOutcome {
        let keys = match keys_for(f, KeyRequest::Fresh).await {
            Ok(k) => k,
            Err(outcome) => return outcome,
        };
        let key = &keys[0];
        match f.store().contains(key).await {
            Ok(true) => return RuleOutcome::SkippedPrecondition {
                request: KeyRequest::Fresh,
                reason: "the key already exists".to_owned(),
            },
            Ok(false) => {}
            Err(e) => return e.into(),
        }
        if let Err(e) = f
            .store()
            .set(key, b"x", &Metadata::MetadataRecord(MetadataRecord::new()))
            .await
        {
            return e.into();
        }
        f.record_created(key);
        RuleOutcome::Passed
    }
    /// Deliberately bad: writes without checking, and never records. The `H6`/`H7` checkers must
    /// catch this, or they are decoration.
    async fn stub_bad_write(f: &dyn Fixture) -> RuleOutcome {
        let keys = match keys_for(f, KeyRequest::Fresh).await {
            Ok(k) => k,
            Err(outcome) => return outcome,
        };
        if let Err(e) = f
            .store()
            .set(&keys[0], b"x", &Metadata::MetadataRecord(MetadataRecord::new()))
            .await
        {
            return e.into();
        }
        RuleOutcome::Passed
    }

    fn stub_rules() -> Vec<Rule> {
        vec![
            rules::rule!("pass", "always passes", "test", [], ReadOnly, stub_pass),
            rules::rule!("fail", "always fails", "test", [], ReadOnly, stub_fail),
        ]
    }

    /// Run one rule through **the gate `run_all` actually uses**, not a copy of it.
    ///
    /// An earlier draft reimplemented the gating here; a bug in `run_one` would then have been
    /// invisible to every one of these tests. `run_one` takes `&Rule` rather than `&'static Rule`
    /// precisely so the tests can reach it with a locally built stub.
    async fn gated(fixture: &dyn Fixture, rule: &Rule) -> RuleOutcome {
        run_one(fixture, rule).await.outcome
    }

    fn report_of(entries: Vec<ReportEntry>) -> ConformanceReport {
        ConformanceReport {
            store: "stub".to_owned(),
            capabilities: all_capabilities(),
            level: SafetyLevel::Scratch,
            entries,
            created: Vec::new(),
            residue: Vec::new(),
        }
    }

    fn entry(id: &str, outcome: RuleOutcome) -> ReportEntry {
        ReportEntry {
            id: id.to_owned(),
            title: "t".to_owned(),
            contract: "test".to_owned(),
            subject: Vec::new(),
            outcome,
        }
    }

    // ---------------------------------------------------------------- H1–H8

    /// `H1` — a rule whose capability is missing is skipped, and **is not called**.
    ///
    /// The "not called" half is the load-bearing one: a rule that runs anyway might pass by luck
    /// against a store that does not support what it tests.
    #[tokio::test]
    async fn h1_missing_capability_skips_without_calling_the_rule() {
        let mut caps = all_capabilities();
        caps.write = false;
        let fixture = StubFixture::new(caps, SafetyLevel::Scratch);
        let rule = rules::rule!("needs_write", "t", "test", [Write], ReadOnly, stub_pass);

        let outcome = gated(&fixture, &rule).await;
        assert_eq!(
            outcome,
            RuleOutcome::SkippedCapability { missing: Capability::Write }
        );
        assert!(
            fixture.store.calls().is_empty(),
            "the rule body must not run: it touched the store {:?}",
            fixture.store.calls()
        );

        // The control: with the capability present, the same rule *does* touch the store — so the
        // assertion above is about gating rather than about a rule that never does anything.
        let permitted = StubFixture::new(all_capabilities(), SafetyLevel::Scratch);
        let rule = rules::rule!("needs_write", "t", "test", [Write], ReadOnly, stub_pass);
        assert_eq!(gated(&permitted, &rule).await, RuleOutcome::Passed);
        assert!(!permitted.store.calls().is_empty());
    }

    /// `H2` — a rule needing a higher level is not run, and the report says which level would.
    #[tokio::test]
    async fn h2_insufficient_level_is_not_run() {
        let fixture = StubFixture::new(all_capabilities(), SafetyLevel::ReadOnly);
        let rule = rules::rule!("needs_scratch", "t", "test", [], Scratch, stub_pass);

        assert_eq!(
            gated(&fixture, &rule).await,
            RuleOutcome::NotRunSafetyLevel { required: SafetyLevel::Scratch }
        );
        assert!(
            fixture.store.calls().is_empty(),
            "the rule body must not run below its level"
        );

        // The control, as in `H1`: at a sufficient level the same rule runs and touches the store.
        let permitted = StubFixture::new(all_capabilities(), SafetyLevel::Scratch);
        let rule = rules::rule!("needs_scratch", "t", "test", [], Scratch, stub_pass);
        assert_eq!(gated(&permitted, &rule).await, RuleOutcome::Passed);
        assert!(!permitted.store.calls().is_empty());
    }

    /// `H3` — a failure that is not allowed is an error, and the message names the rule.
    #[test]
    fn h3_assert_conformant_reports_a_failure() {
        let report = report_of(vec![entry("r1", failed("boom"))]);
        let error = report
            .assert_conformant(&[])
            .expect_err("a failed rule must not be conformant");
        assert!(error.message.contains("r1"), "{}", error.message);
        assert!(error.message.contains("boom"), "{}", error.message);
    }

    /// `H4` — a failure listed as allowed is accepted.
    #[test]
    fn h4_allowed_failure_is_accepted() {
        let report = report_of(vec![entry("r1", failed("known"))]);
        report
            .assert_conformant(&[AllowedFailure { rule: "r1", issue: "SOME-ISSUE" }])
            .expect("an allowed failure is not a defect");
    }

    /// `H5` — **an allowed rule that passed is also an error.**
    ///
    /// Without this, an ignore list written for a good reason outlives the reason, which is the
    /// staleness this whole suite exists to prevent. It is what makes a fixed issue force the
    /// entry's removal instead of relying on someone remembering.
    #[test]
    fn h5_stale_allowed_failure_is_reported() {
        let report = report_of(vec![entry("r1", RuleOutcome::Passed)]);
        let error = report
            .assert_conformant(&[AllowedFailure { rule: "r1", issue: "FIXED-ISSUE" }])
            .expect_err("an allowed rule that passed must be reported");
        assert!(error.message.contains("r1"), "{}", error.message);
        assert!(error.message.contains("remove the entry"), "{}", error.message);

        // An allowed failure naming a rule that does not exist is also caught.
        let report = report_of(vec![entry("r1", RuleOutcome::Passed)]);
        let error = report
            .assert_conformant(&[AllowedFailure { rule: "ghost", issue: "X" }])
            .expect_err("an unknown rule id must be reported");
        assert!(error.message.contains("ghost"), "{}", error.message);
    }

    /// `H6` — the checker for "a rule checks before it mutates", proven against a bad rule.
    ///
    /// A checker that only ever saw well-behaved rules would be decoration; `stub_bad_write` is
    /// what makes this test mean something.
    #[tokio::test]
    async fn h6_check_before_mutate_checker_catches_a_bad_rule() {
        for (rule, expect_ok) in [
            (rules::rule!("good", "t", "test", [], Scratch, stub_good_write), true),
            (rules::rule!("bad", "t", "test", [], Scratch, stub_bad_write), false),
        ] {
            let fixture = StubFixture::new(all_capabilities(), SafetyLevel::Scratch);
            let _ = (rule.run)(&fixture).await;
            let calls = fixture.store.calls();
            let checked_first = match calls.iter().position(|c| matches!(c, Call::Mutate(_))) {
                None => true,
                Some(first_mutation) => calls[..first_mutation]
                    .iter()
                    .any(|c| matches!(c, Call::Read(_))),
            };
            assert_eq!(
                checked_first, expect_ok,
                "rule `{}` check-before-mutate", rule.meta.id
            );
        }
    }

    /// `H7` — the checker for "a rule records every key it creates", proven against a bad rule.
    ///
    /// Under-recording is the one failure the safety levels cannot survive: an unrecorded key is
    /// neither cleaned up nor reported as residue, so it leaks silently.
    #[tokio::test]
    async fn h7_records_created_checker_catches_a_bad_rule() {
        for (rule, expect_ok) in [
            (rules::rule!("good", "t", "test", [], Scratch, stub_good_write), true),
            (rules::rule!("bad", "t", "test", [], Scratch, stub_bad_write), false),
        ] {
            let fixture = StubFixture::new(all_capabilities(), SafetyLevel::Scratch);
            let _ = (rule.run)(&fixture).await;
            let recorded = fixture.created_keys();
            let all_recorded = fixture
                .store
                .written()
                .iter()
                .all(|k| recorded.contains(k));
            assert_eq!(all_recorded, expect_ok, "rule `{}` records what it creates", rule.meta.id);
        }
    }

    /// `H8` — the report round-trips through JSON and YAML.
    ///
    /// Required independently of the deferred validation tool: the guide's per-store status matrix
    /// is generated from these reports rather than hand-maintained.
    #[test]
    fn h8_report_round_trips() -> Result<(), Box<dyn std::error::Error>> {
        let report = report_of(vec![
            entry("r1", RuleOutcome::Passed),
            entry("r2", failed("detail")),
            entry("r3", RuleOutcome::SkippedCapability { missing: Capability::Directories }),
            entry("r4", RuleOutcome::NotRunSafetyLevel { required: SafetyLevel::Scratch }),
            entry(
                "r5",
                RuleOutcome::SkippedPrecondition {
                    request: KeyRequest::FreshPrefixPair,
                    reason: "numeric ids".to_owned(),
                },
            ),
            entry(
                "r6",
                RuleOutcome::Errored {
                    error_type: ErrorType::KeyNotFound,
                    message: "gone".to_owned(),
                },
            ),
        ]);

        let json: ConformanceReport = serde_json::from_str(&serde_json::to_string(&report)?)?;
        assert_eq!(json.entries, report.entries);
        let yaml: ConformanceReport = serde_yaml::from_str(&serde_yaml::to_string(&report)?)?;
        assert_eq!(yaml.entries, report.entries);
        Ok(())
    }

    // ------------------------------------------------- the real inventory

    /// A fixture over `AsyncMemoryStore`, so `H6`/`H7` and the smoke test below exercise the real
    /// rules rather than stubs.
    struct MemFixture {
        store: crate::store::AsyncMemoryStore,
        created: Mutex<Vec<Key>>,
        seq: Mutex<usize>,
    }

    impl MemFixture {
        fn new() -> Self {
            MemFixture {
                store: crate::store::AsyncMemoryStore::new(&Key::new()),
                created: Mutex::new(Vec::new()),
                seq: Mutex::new(0),
            }
        }
        /// Unique stems, so rules do not collide with each other's keys.
        fn stem(&self, base: &str) -> String {
            let mut n = self.seq.lock().expect("seq");
            *n += 1;
            format!("{base}{n}")
        }
    }

    #[cfg_attr(not(target_arch = "wasm32"), async_trait)]
    #[cfg_attr(target_arch = "wasm32", async_trait(?Send))]
    impl Fixture for MemFixture {
        fn store(&self) -> &dyn AsyncStore {
            &self.store
        }
        fn capabilities(&self) -> StoreCapabilities {
            StoreCapabilities {
                write: true,
                remove: true,
                directories: true,
                derived_directories: true,
                explicit_directories: true,
                remove_directories: true,
                stored_metadata: true,
                enumerate_keys: true,
            }
        }
        fn safety_level(&self) -> SafetyLevel {
            SafetyLevel::Scratch
        }
        fn expected_prefix(&self) -> Key {
            Key::new()
        }
        fn label(&self) -> String {
            "AsyncMemoryStore (in-test fixture)".to_owned()
        }
        async fn keys_for(&self, request: &KeyRequest) -> Result<Vec<Key>, Unavailable> {
            let base = Key::new();
            match request {
                KeyRequest::Fresh => Ok(vec![base.join(self.stem("fresh"))]),
                KeyRequest::FreshSiblings { count } => {
                    let dir = self.stem("dir");
                    Ok((0..*count)
                        .map(|i| base.join(&dir).join(format!("s{i}.txt")))
                        .collect())
                }
                KeyRequest::FreshPrefixPair => {
                    let stem = self.stem("sub");
                    Ok(vec![base.join(&stem), base.join(format!("{stem}way"))])
                }
                KeyRequest::FreshNested { depth } => {
                    let mut key = base.join(self.stem("nest"));
                    for i in 0..*depth {
                        key = key.join(format!("d{i}"));
                    }
                    Ok(vec![key.join("leaf.txt")])
                }
                KeyRequest::Existing | KeyRequest::ExistingDirectory => Err(Unavailable::new(
                    "this fixture seeds nothing; the store starts empty",
                )),
                KeyRequest::OutsidePrefix => Err(Unavailable::new(
                    "the fixture's prefix is the root, so no absolute key is outside it",
                )),
                KeyRequest::UnsupportedShape => Err(Unavailable::new(
                    "AsyncMemoryStore accepts every absolute key under its prefix",
                )),
                KeyRequest::Supported => Ok(vec![base.join(self.stem("supported"))]),
                KeyRequest::Relative => match crate::parse::parse_key("../escape.txt") {
                    Ok(key) => Ok(vec![key]),
                    Err(e) => Err(Unavailable::new(e.message.clone())),
                },
                KeyRequest::MetadataCollision => Err(Unavailable::new(
                    "AsyncMemoryStore keeps metadata beside the data rather than in a sidecar key",
                )),
            }
        }
        fn record_created(&self, key: &Key) {
            if let Ok(mut c) = self.created.lock() {
                c.push(key.clone());
            }
        }
        fn created_keys(&self) -> Vec<Key> {
            self.created.lock().map(|c| c.clone()).unwrap_or_default()
        }
        async fn cleanup(&self) {
            // Both, because a rule may have created a directory (`explicit01` calls `makedir`) and
            // `remove` does not delete one. The residue report is what surfaced the omission.
            for key in self.created_keys() {
                let _ = self.store.remove(&key).await;
                let _ = self.store.removedir(&key).await;
            }
        }
    }

    /// `H7`, pointed at the real inventory: every rule records every key it creates.
    ///
    /// An unrecorded key is neither cleaned up nor reported as residue, so it leaks silently — the
    /// one failure the safety levels cannot survive. The checker was proven against a deliberately
    /// bad rule in `h7_records_created_checker_catches_a_bad_rule`.
    #[tokio::test]
    async fn h7_every_real_rule_records_what_it_creates() {
        assert!(!rules().is_empty(), "an empty inventory would pass vacuously");
        for rule in rules() {
            let fixture = MemFixture::new();
            let entry = run_one(&fixture, rule).await;
            let recorded = fixture.created_keys();
            for key in fixture.store.keys().await.unwrap_or_default() {
                // `keys()` also enumerates the directories above the data keys and the store's own
                // prefix (§9). No rule *creates* those — they are derived — so only data keys are
                // the rule's to record.
                if key == fixture.expected_prefix()
                    || matches!(fixture.store.is_dir(&key).await, Ok(true))
                {
                    continue;
                }
                assert!(
                    recorded.contains(&key),
                    "rule `{}` created {} without recording it ({:?})",
                    rule.meta.id,
                    key.encode(),
                    entry.outcome
                );
            }
        }
    }

    /// The whole inventory against `AsyncMemoryStore`, printed.
    ///
    /// This is the divergence census in miniature: it does not assert conformance — the store is
    /// known to diverge on `keys()` until that is fixed — it asserts that every rule reaches a
    /// *decided* outcome, and prints the report so the divergences are visible rather than
    /// summarised.
    #[tokio::test]
    async fn census_against_the_memory_store() {
        let fixture = MemFixture::new();
        let report = run_all(&fixture).await;
        eprintln!("{report}");
        let counts = report.counts();
        assert_eq!(
            counts.ran() + counts.skipped_capability + counts.skipped_precondition + counts.not_run_level,
            rules().len(),
            "every rule must reach a decided outcome"
        );
        assert_eq!(counts.errored, 0, "no rule should error: {report}");

        // No allowed failures. `keys02` was listed here while `AsyncMemoryStore` returned data
        // keys only; step 10 fixed that, and `H5` reported the entry as stale rather than letting
        // it outlive its reason — which is what `assert_conformant` failing in both directions is
        // for. `CORE-STORE-KEYS-MEANS-TWO-DIFFERENT-THINGS` is closed by that change.
        report
            .assert_conformant(&[])
            .expect("AsyncMemoryStore is expected to be conformant");
    }

    /// The sibling family against a store that is expected to satisfy it.
    ///
    /// `AsyncMemoryStore` addresses by key rather than by string prefix, so it should pass every
    /// rule it can reach. A rule it cannot reach reports why, and that is not a pass.
    #[tokio::test]
    async fn sibling_rules_hold_for_the_memory_store() {
        let fixture = MemFixture::new();
        let mut reached = 0;
        for rule in rules().iter().filter(|r| r.meta.id.starts_with("sibling")) {
            let entry = run_one(&fixture, rule).await;
            match &entry.outcome {
                RuleOutcome::Passed => reached += 1,
                RuleOutcome::SkippedPrecondition { .. } => {}
                other => panic!("{} on AsyncMemoryStore: {other:?}", rule.meta.id),
            }
        }
        assert!(reached >= 4, "only {reached} sibling rules were reachable");
    }

    /// A store that deletes by **string prefix**, reproducing the defect `sibling01` exists for.
    ///
    /// This is `STORE-OPENDAL-SLASH-HANDLING` defect 1 in miniature: `removedir("sub")` takes
    /// `subway/` with it, and reports success. Without this test the sibling rules would be green
    /// against a store that cannot break them, which proves nothing about the rules.
    struct PrefixDeletingStore {
        inner: crate::store::AsyncMemoryStore,
    }

    #[cfg_attr(not(target_arch = "wasm32"), async_trait)]
    #[cfg_attr(target_arch = "wasm32", async_trait(?Send))]
    impl AsyncStore for PrefixDeletingStore {
        async fn get(&self, key: &Key) -> Result<(Vec<u8>, Metadata), Error> {
            self.inner.get(key).await
        }
        async fn set(&self, key: &Key, data: &[u8], metadata: &Metadata) -> Result<(), Error> {
            self.inner.set(key, data, metadata).await
        }
        async fn set_metadata(&self, key: &Key, metadata: &Metadata) -> Result<(), Error> {
            self.inner.set_metadata(key, metadata).await
        }
        async fn contains(&self, key: &Key) -> Result<bool, Error> {
            self.inner.contains(key).await
        }
        async fn is_dir(&self, key: &Key) -> Result<bool, Error> {
            self.inner.is_dir(key).await
        }
        async fn remove(&self, key: &Key) -> Result<(), Error> {
            self.inner.remove(key).await
        }
        /// The bug: every key whose *encoded name* starts with this one goes.
        async fn removedir(&self, key: &Key) -> Result<(), Error> {
            let doomed: Vec<Key> = self
                .inner
                .keys()
                .await?
                .into_iter()
                .filter(|k| k.encode().starts_with(&key.encode()))
                .collect();
            for k in doomed {
                self.inner.remove(&k).await?;
            }
            Ok(())
        }
    }

    struct BrokenFixture {
        store: PrefixDeletingStore,
        created: Mutex<Vec<Key>>,
    }

    #[cfg_attr(not(target_arch = "wasm32"), async_trait)]
    #[cfg_attr(target_arch = "wasm32", async_trait(?Send))]
    impl Fixture for BrokenFixture {
        fn store(&self) -> &dyn AsyncStore {
            &self.store
        }
        fn capabilities(&self) -> StoreCapabilities {
            StoreCapabilities {
                write: true,
                remove: true,
                directories: true,
                derived_directories: true,
                explicit_directories: false,
                remove_directories: true,
                stored_metadata: true,
                enumerate_keys: true,
            }
        }
        fn safety_level(&self) -> SafetyLevel {
            SafetyLevel::Scratch
        }
        fn expected_prefix(&self) -> Key {
            Key::new()
        }
        fn label(&self) -> String {
            "PrefixDeletingStore".to_owned()
        }
        async fn keys_for(&self, request: &KeyRequest) -> Result<Vec<Key>, Unavailable> {
            match request {
                KeyRequest::FreshPrefixPair => Ok(vec![
                    Key::new().join("sub"),
                    Key::new().join("subway"),
                ]),
                _ => Err(Unavailable::new("only the prefix pair is needed here")),
            }
        }
        fn record_created(&self, key: &Key) {
            if let Ok(mut c) = self.created.lock() {
                c.push(key.clone());
            }
        }
        fn created_keys(&self) -> Vec<Key> {
            self.created.lock().map(|c| c.clone()).unwrap_or_default()
        }
    }

    /// **`sibling01` catches the data-loss defect.** The rule is only worth having if it fails here.
    #[tokio::test]
    async fn sibling01_catches_a_prefix_deleting_store() {
        let fixture = BrokenFixture {
            store: PrefixDeletingStore {
                inner: crate::store::AsyncMemoryStore::new(&Key::new()),
            },
            created: Mutex::new(Vec::new()),
        };
        let rule = rule("sibling01").expect("sibling01 is registered");
        let entry = run_one(&fixture, rule).await;

        match &entry.outcome {
            RuleOutcome::Failed { detail, .. } => {
                assert!(detail.contains("destroyed"), "{detail}");
                assert!(
                    entry.subject.iter().any(|k| k.encode().contains("subway")),
                    "the failure must name the sibling it lost: {:?}",
                    entry.subject
                );
            }
            other => panic!("sibling01 must fail against a prefix-deleting store, got {other:?}"),
        }
    }

    /// The gate itself, end to end: `run_all` produces one entry per rule, in order.
    #[tokio::test]
    async fn harness_runs_every_rule_in_order() {
        let fixture = StubFixture::new(all_capabilities(), SafetyLevel::Scratch);
        let mut entries = Vec::new();
        for rule in stub_rules() {
            entries.push(entry(rule.meta.id, gated(&fixture, &rule).await));
        }
        let report = report_of(entries);
        let counts = report.counts();
        assert_eq!(counts.passed, 1);
        assert_eq!(counts.failed, 1);
        assert_eq!(counts.ran(), 2);
    }
}
