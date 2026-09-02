//! What a caller supplies so the rules have a store to run against.

use crate::maybe_send::{MaybeSend, MaybeSync};
use crate::query::Key;
use crate::store::AsyncStore;
use async_trait::async_trait;

use super::{SafetyLevel, StoreCapabilities};

/// A precondition a rule needs, stated as a request rather than as an invented key name.
///
/// This is what lets a general suite reach a *specialized* store. A store presenting one database
/// table — "files" are rows, the key is a numeric ID, there are no subdirectories — cannot satisfy
/// [`KeyRequest::FreshPrefixPair`] or [`KeyRequest::FreshNested`], and says so with a reason. A
/// rule that had written `sub/a.txt` itself would instead have produced a failure that looked like
/// a defect in the store.
///
/// **Deliberately not `#[non_exhaustive]`.** An out-of-tree fixture *should* fail to compile when a
/// precondition is added, rather than silently declining a rule that was meant to run. Adding a
/// variant is a breaking change, on purpose.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum KeyRequest {
    /// One key that does not exist and the rule may create.
    Fresh,
    /// `count` fresh keys in one directory.
    FreshSiblings { count: usize },
    /// Two fresh keys where one name is a proper prefix of the other (`sub`, `subway`).
    ///
    /// The sibling rule's whole subject: `removedir("sub")` must not touch `subway/`.
    FreshPrefixPair,
    /// A fresh key at least `depth` segments below the store's prefix.
    FreshNested { depth: usize },
    /// A key that already holds data.
    ///
    /// The only source of subjects on the read-only path, and therefore the only way a read-only
    /// store such as `FetchStore` is testable at all. At [`SafetyLevel::ReadOnly`] the fixture must
    /// *find* one, not create one.
    Existing,
    /// A directory that already exists.
    ExistingDirectory,
    /// A key outside this store's prefix, which `is_supported` must refuse.
    OutsidePrefix,
    /// A key inside the prefix whose *shape* this store cannot address — a name it will not accept.
    ///
    /// Separate from [`KeyRequest::OutsidePrefix`] because they are different refusals: a fixture
    /// answering only one would leave the other rule vacuous.
    UnsupportedShape,
    /// A key inside the prefix that this store *can* address, for the positive `is_supported` case.
    Supported,
    /// A relative key — one containing `.` or `..` — which every method must refuse.
    Relative,
    /// A key whose data path would collide with another key's `.__metadata__` path.
    MetadataCollision,
}

/// A fixture's reasoned decline. **Not an `Error`**: "this store has no directories" is a design
/// fact about the store, and putting it in the error channel would make a correct answer look like
/// a failure.
#[derive(Debug, Clone)]
pub struct Unavailable {
    pub reason: String,
}

impl Unavailable {
    pub fn new(reason: impl Into<String>) -> Self {
        Unavailable {
            reason: reason.into(),
        }
    }
}

/// The store under test, plus everything the rules need to know about it.
///
/// Object-safe: no generic methods, no `Self` by value, no associated types — the suite holds
/// `&dyn Fixture`. The `MaybeSend + MaybeSync` bounds match [`AsyncStore`]'s and are required
/// because every rule body holds a `&dyn Fixture` across an `.await`, and `BoxFuture` is
/// `Send`-bounded off wasm.
#[cfg_attr(not(target_arch = "wasm32"), async_trait)]
#[cfg_attr(target_arch = "wasm32", async_trait(?Send))]
pub trait Fixture: MaybeSend + MaybeSync {
    /// The store under test.
    fn store(&self) -> &dyn AsyncStore;

    /// What this store claims it can do.
    ///
    /// A claim, not a description: the negative rules check that a `false` is honest, so
    /// under-declaring a capability to skip its rules fails instead of passing quietly.
    fn capabilities(&self) -> StoreCapabilities;

    /// How much this fixture permits a rule to do.
    fn safety_level(&self) -> SafetyLevel;

    /// The prefix this store was *configured* with.
    ///
    /// Independent ground truth, deliberately **not** `store.key_prefix()` — that is the thing
    /// under test. Without this, `prefix01` could only compare the method with itself, and a store
    /// returning `Key::new()` from `key_prefix()` would pass the rule written to catch exactly
    /// that.
    fn expected_prefix(&self) -> Key;

    /// Name for the report — the store type and any distinguishing configuration.
    fn label(&self) -> String;

    /// Keys satisfying `request`, or a reason this store cannot supply them.
    ///
    /// `rule_id` lets a fixture namespace what it hands out, so rules do not see each other's
    /// residue. Returns *names*; the rule creates them, except for [`KeyRequest::Existing`] and
    /// [`KeyRequest::ExistingDirectory`], whose subjects must already be present.
    async fn keys_for(&self, request: &KeyRequest) -> Result<Vec<Key>, Unavailable>;

    /// Record a key this run created. A rule calls this immediately after a successful create.
    ///
    /// Sync, so no lock is held across an `.await`. The fixture is the only thing that can know
    /// what to clean up and what was left behind, which is why the record lives here rather than in
    /// the report.
    fn record_created(&self, key: &Key);

    /// Every key [`Fixture::record_created`] was told about, in creation order.
    fn created_keys(&self) -> Vec<Key>;

    /// Best-effort removal of what the run created. Never fails the report.
    ///
    /// At [`SafetyLevel::CreateOnly`] this can do nothing, and everything created survives — which
    /// is why the report lists the residue rather than assuming it is empty.
    async fn cleanup(&self) {}
}

/// A ready-made [`Fixture`] for the common case: a store, its prefix, and what it can do.
///
/// Most of `keys_for` is the same for every store — generating fresh names under a prefix is not
/// where store implementations differ. What *does* differ is the handful of preconditions a
/// particular store can offer: a key outside its prefix, a key shape it refuses, a metadata
/// collision, a pre-seeded key for the read-only path. Those are supplied by builder methods, and
/// anything not supplied is declined with a reason that reaches the report.
///
/// A store whose key space is too unusual for this — a view onto a database table keyed by numeric
/// row ID — implements [`Fixture`] directly instead. This type is a convenience, not a ceiling.
pub struct GenericFixture {
    store: Box<dyn AsyncStore>,
    prefix: Key,
    key_base: Key,
    capabilities: StoreCapabilities,
    level: SafetyLevel,
    label: String,
    outside_prefix: Option<Key>,
    unsupported_shape: Option<Key>,
    metadata_collision: Option<Key>,
    existing: Option<Key>,
    existing_directory: Option<Key>,
    no_supported_key: Option<String>,
    supported: Option<Key>,
    created: std::sync::Mutex<Vec<Key>>,
    counter: std::sync::atomic::AtomicUsize,
    run: String,
}

impl GenericFixture {
    pub fn new(
        label: impl Into<String>,
        store: Box<dyn AsyncStore>,
        prefix: Key,
        capabilities: StoreCapabilities,
        level: SafetyLevel,
    ) -> Self {
        // A per-fixture stem, so two suites against one backing store cannot collide.
        //
        // **Not derived from the clock.** `SystemTime::now()` *panics* on `wasm32-unknown-unknown`
        // — "time not implemented on this platform" — so a time-based stem made this fixture
        // unusable on the one target that motivated a runtime-agnostic suite in the first place.
        // A process-wide counter plus the address of a fresh allocation is unique within a run and
        // needs no platform facilities.
        //
        // It is **not** unique *across* runs. A store that persists between runs — browser
        // `localStorage` is the in-tree case — should pass its own durable stem to
        // [`Self::with_run_id`], or it will meet its own leftovers on the second run.
        static SEQ: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
        let nth = SEQ.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        let boxed = Box::new(0u8);
        let addr = Box::into_raw(boxed) as usize;
        // Safety: the allocation is reclaimed immediately; only its address was wanted.
        drop(unsafe { Box::from_raw(addr as *mut u8) });
        let run = format!("lqconf{addr:x}{nth:x}");
        GenericFixture {
            store,
            key_base: prefix.clone(),
            prefix,
            capabilities,
            level,
            label: label.into(),
            outside_prefix: None,
            unsupported_shape: None,
            metadata_collision: None,
            existing: None,
            existing_directory: None,
            no_supported_key: None,
            supported: None,
            created: std::sync::Mutex::new(Vec::new()),
            counter: std::sync::atomic::AtomicUsize::new(0),
            run,
        }
    }

    /// A durable stem for generated key names, for a store that persists between runs.
    ///
    /// The default stem is unique within a process but not across processes. A browser
    /// `localStorage` store keeps what a previous run wrote, so a suite that reuses the default
    /// passes the first time and meets its own leftovers the second — the failure mode
    /// `store_local_STORE.rs` already documents for that store.
    pub fn with_run_id(mut self, run: impl Into<String>) -> Self {
        self.run = run.into();
        self
    }

    /// Where to generate fresh keys, when that is **not** the store's own prefix.
    ///
    /// The two coincide for an ordinary store and diverge for a composition: an
    /// [`AsyncStoreRouter`](crate::store::AsyncStoreRouter) reports the root as its prefix, because
    /// it spans several, but keys must be generated under one member's prefix or nothing routes.
    /// Conflating them made `prefix01` fail against a correct router — the fixture was wrong, not
    /// the store.
    pub fn with_key_base(mut self, key_base: Key) -> Self {
        self.key_base = key_base;
        self
    }

    /// A key this store is known to accept, when a *generated* name would not be.
    ///
    /// Needed by any store whose key space is a fixed set rather than a shape — `FetchStore`
    /// consults a configured key list, so an invented name is legitimately unsupported and
    /// `prefix04` would fail a correct store without this.
    pub fn with_supported(mut self, key: Key) -> Self {
        self.supported = Some(key);
        self
    }

    /// Declare that **no** key is supported, with the reason.
    ///
    /// For a store that accepts nothing by design — `NoAsyncStore` is the in-tree example, and it
    /// is what an `Environment` holds until a store is configured. Without this, `prefix04` fails
    /// such a store for doing exactly what it exists to do, and a failure that is really a design
    /// fact is the worst kind of entry in a conformance report.
    pub fn without_supported(mut self, reason: impl Into<String>) -> Self {
        self.no_supported_key = Some(reason.into());
        self
    }

    /// A key outside this store's prefix, so `prefix02` can run.
    pub fn with_outside_prefix(mut self, key: Key) -> Self {
        self.outside_prefix = Some(key);
        self
    }
    /// A key whose *shape* this store refuses, so `prefix03` and `sibling05` can run.
    pub fn with_unsupported_shape(mut self, key: Key) -> Self {
        self.unsupported_shape = Some(key);
        self
    }
    /// A key colliding with another key's metadata path, so `sidecar01` can run.
    pub fn with_metadata_collision(mut self, key: Key) -> Self {
        self.metadata_collision = Some(key);
        self
    }
    /// A key already holding data — the read-only path's only source of subjects.
    pub fn with_existing(mut self, key: Key) -> Self {
        self.existing = Some(key);
        self
    }
    /// A directory that already exists.
    pub fn with_existing_directory(mut self, key: Key) -> Self {
        self.existing_directory = Some(key);
        self
    }

    fn stem(&self, base: &str) -> String {
        let n = self
            .counter
            .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        format!("{}-{base}{n}", self.run)
    }

    fn offered(key: &Option<Key>, what: &str) -> Result<Vec<Key>, Unavailable> {
        match key {
            Some(k) => Ok(vec![k.clone()]),
            None => Err(Unavailable::new(format!(
                "this fixture was not given {what}"
            ))),
        }
    }
}

#[cfg_attr(not(target_arch = "wasm32"), async_trait)]
#[cfg_attr(target_arch = "wasm32", async_trait(?Send))]
impl Fixture for GenericFixture {
    fn store(&self) -> &dyn AsyncStore {
        self.store.as_ref()
    }
    fn capabilities(&self) -> StoreCapabilities {
        self.capabilities
    }
    fn safety_level(&self) -> SafetyLevel {
        self.level
    }
    fn expected_prefix(&self) -> Key {
        self.prefix.clone()
    }
    fn label(&self) -> String {
        self.label.clone()
    }

    async fn keys_for(&self, request: &KeyRequest) -> Result<Vec<Key>, Unavailable> {
        let base = &self.key_base;
        match request {
            KeyRequest::Fresh => Ok(vec![base.join(self.stem("f"))]),
            KeyRequest::Supported => match (&self.no_supported_key, &self.supported) {
                (Some(reason), _) => Err(Unavailable::new(reason.clone())),
                (None, Some(key)) => Ok(vec![key.clone()]),
                (None, None) => Ok(vec![base.join(self.stem("s"))]),
            },
            KeyRequest::FreshSiblings { count } => {
                let dir = self.stem("d");
                Ok((0..*count)
                    .map(|i| base.join(&dir).join(format!("s{i}.txt")))
                    .collect())
            }
            KeyRequest::FreshPrefixPair => {
                // One name a proper prefix of the other, which is the whole subject of §1.
                let stem = self.stem("sub");
                Ok(vec![base.join(&stem), base.join(format!("{stem}way"))])
            }
            KeyRequest::FreshNested { depth } => {
                let mut key = base.join(self.stem("n"));
                for i in 0..*depth {
                    key = key.join(format!("d{i}"));
                }
                Ok(vec![key.join("leaf.txt")])
            }
            KeyRequest::Relative => crate::parse::parse_key("data/../../escape.txt")
                .map(|k| vec![k])
                .map_err(|e| Unavailable::new(e.message.clone())),
            KeyRequest::OutsidePrefix => {
                Self::offered(&self.outside_prefix, "a key outside its prefix")
            }
            KeyRequest::UnsupportedShape => {
                Self::offered(&self.unsupported_shape, "a key shape the store refuses")
            }
            KeyRequest::MetadataCollision => {
                Self::offered(&self.metadata_collision, "a metadata-colliding key")
            }
            KeyRequest::Existing => Self::offered(&self.existing, "a pre-seeded existing key"),
            KeyRequest::ExistingDirectory => {
                Self::offered(&self.existing_directory, "a pre-seeded existing directory")
            }
        }
    }

    fn record_created(&self, key: &Key) {
        if let Ok(mut created) = self.created.lock() {
            created.push(key.clone());
        }
    }
    fn created_keys(&self) -> Vec<Key> {
        self.created
            .lock()
            .map(|c| c.clone())
            .unwrap_or_default()
    }
    async fn cleanup(&self) {
        // Both, because a rule may have created a directory with `makedir`, which `remove` will
        // not delete. Best effort: cleanup never fails a report.
        for key in self.created_keys() {
            let _ = self.store.remove(&key).await;
            let _ = self.store.removedir(&key).await;
        }
    }
}
