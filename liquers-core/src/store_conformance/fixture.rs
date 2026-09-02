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
