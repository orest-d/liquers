//! Settable value accessors: a handle to the place a value lives, to read it, write it, watch it
//! and project it to a part.
//!
//! An accessor is a **root** (the place: data in the store, a keyed asset, any asset, a variable
//! in memory, or a custom source) plus a chain of **projection steps** (a recipe in a
//! `recipes.yaml`, its query, one action, one argument, a document field, a list item). Every
//! accessor of one root shares one **hub**, which keeps them in step: a set through one of them is
//! seen by all, a set based on a part that has changed since is a conflict (first writer wins), and
//! a structural change (a step inserted before an action) invalidates the projections it moved.
//!
//! # Main components
//!
//! - [`ValueAccessor`]: the contract: `get`, `set` / `set_if`, `info` / `log`, `describe`,
//!   `subscribe`, `project`.
//! - [`memory_accessor`], [`store_accessor`], [`key_accessor`], [`asset_accessor`]: the
//!   first-class roots.
//! - [`AccessorHub::new`]: turns any [`AccessorRoot`] into a full accessor.
//! - [`ProjectionStep`] and the steps of [`steps`]: lenses from a value to a part of it.
//! - [`read_only`] and [`validated`]: wrappers that restrict what an accessor accepts.
//!
//! # Where it is used
//!
//! - [`crate::value::ValueInterface::try_into_accessor`]: a value variant that carries an accessor
//!   (`ExtValue::Accessor` in `liquers-lib`).
//! - `liquers-lib/src/accessor/`: the `acc` commands that build accessors from queries.
//! - `liquers-axum/src/accessor/`: the `/api/acc` routes and their WebSocket.
//! - [Tests](https://github.com/orest-d/liquers/blob/main/liquers-core/tests/value_accessor.rs):
//!   the roots and steps in an environment.
//!
//! Python (`lq.Accessor`) and JavaScript (`Accessor`) wrap the same trait.
//!
//! # Related concepts
//!
//! - [`crate::data_node`]: a read-only view of a value's data; an accessor also writes, watches and
//!   names its part by a query.
//! - [`crate::pipeline::editor`]: validated edits of one query or pipeline in memory; the query
//!   steps here apply them and write the result back to where the query lives.
//! - [`crate::assets::AssetRef`]: one evaluation and its notifications; a keyed-asset accessor
//!   relays them and adds writing and projections.
//!
//! # Events and the holder contract
//!
//! [`ValueAccessor::subscribe`] gives a `watch` receiver: a wake-up, not a record, because it
//! keeps only the latest event. On `Changed`, redraw from `get`; on `Status(Expired)`, call `get`,
//! which re-evaluates an asset and then produces `Changed`; on any event, a holder that may have
//! missed one compares [`AccessorInfo::fingerprint`] with what it drew.
//!
//! # Examples
//!
//! ```
//! # futures::executor::block_on(async {
//! use liquers_core::accessor::{memory_accessor, SetOptions, ValueAccessor};
//! use liquers_core::{query::Query, state::State, value::Value};
//! let variable = memory_accessor::<Value>(None, Query::new());
//! let shared = variable.clone();
//! variable.set(Value::from(42), SetOptions::default()).await.unwrap();
//! assert_eq!(*shared.get().await.unwrap().state.value().unwrap(), Value::from(42));
//! # });
//! ```
//!
//! - [`hub` tests](https://github.com/orest-d/liquers/blob/main/liquers-core/src/accessor/hub.rs):
//!   conflicts, overlapping and disjoint parts, invalidation.
//!
//! Design: `specs/design/settable-value-accessor/`.

use std::borrow::Cow;
use std::fmt::Debug;
use std::sync::Arc;

use serde::{Deserialize, Serialize};
use tokio::sync::watch;

use crate::command_metadata::{ArgumentInfo, CommandKey};
use crate::error::Error;
use crate::maybe_send::{BoxFuture, MaybeSend, MaybeSync};
use crate::metadata::{LogEntry, ProgressEntry, Status, Version};
use crate::query::Query;
use crate::state::State;
use crate::value::ValueInterface;

pub mod hub;
pub mod root;
pub mod steps;
pub mod wrap;

pub use hub::AccessorHub;
pub use root::{
    asset_accessor, key_accessor, memory_accessor, store_accessor, AssetRefRoot, KeyRoot, MemoryRoot,
    StoreRoot, StoreView,
};
pub use wrap::{read_only, validated};

/// What a set was based on: where the part sits (`anchor`) and what it held (`content`).
///
/// Two fingerprints are compared with `==`; [`Version::unknown`] is not a wildcard here. The
/// [`token`](Self::token) is the form that leaves the process: an HTTP `ETag`, a Python or
/// JavaScript string.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Fingerprint {
    /// A structural fingerprint of the slot: changes when the part moves or disappears.
    pub anchor: u64,
    /// The content: the hash of the part's JSON form, or the root's version.
    pub content: Version,
}

impl Fingerprint {
    /// The fingerprint as text: the anchor and the content as 16 and 32 hex digits, joined by `-`.
    pub fn token(&self) -> String {
        format!("{:016x}-{}", self.anchor, self.content)
    }

    /// Parses a [`token`](Self::token) back; anything else is a `ParseError`.
    pub fn from_token(token: &str) -> Result<Self, Error> {
        let token = token.trim().trim_matches('"');
        let invalid = || Error::parse_error(format!("'{token}' is not an accessor token"));
        let (anchor, content) = token.split_once('-').ok_or_else(invalid)?;
        if anchor.len() != 16 || content.len() != 32 {
            return Err(invalid());
        }
        let anchor = u64::from_str_radix(anchor, 16).map_err(|_| invalid())?;
        let content = u128::from_str_radix(content, 16).map_err(|_| invalid())?;
        Ok(Fingerprint {
            anchor,
            content: Version::new(content),
        })
    }
}

impl std::fmt::Display for Fingerprint {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.token())
    }
}

impl Serialize for Fingerprint {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(&self.token())
    }
}

impl<'de> Deserialize<'de> for Fingerprint {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let s = <String as Deserialize<'de>>::deserialize(deserializer)?;
        Fingerprint::from_token(&s).map_err(|e| serde::de::Error::custom(e.message.clone()))
    }
}

/// The text a conflict message ends with, followed by the current token (or `none`).
pub const CURRENT_TOKEN_MARKER: &str = "current token: ";

/// The current token carried by a conflict error, if the error is one and the part exists.
pub fn conflict_token(error: &Error) -> Option<Fingerprint> {
    let at = error.message.rfind(CURRENT_TOKEN_MARKER)?;
    Fingerprint::from_token(&error.message[at + CURRENT_TOKEN_MARKER.len()..]).ok()
}

/// A read: the state of the part, its fingerprint and the root version it was derived from.
#[derive(Debug, Clone)]
pub struct AccessorSnapshot<V: ValueInterface> {
    pub state: State<V>,
    pub fingerprint: Fingerprint,
    pub root_version: Version,
}

/// A wake-up for the holders of an accessor.
///
/// `Status`, the progress events, `Log` and `Removed` are relayed from the source; `Changed` and
/// `Invalidated` come from the hub. `Log` carries the new number of messages: read them with
/// [`ValueAccessor::log`].
#[derive(Debug, Clone, PartialEq)]
pub enum AccessorEvent {
    Initial,
    Status(Status),
    PrimaryProgress(ProgressEntry),
    SecondaryProgress(ProgressEntry),
    Log(usize),
    Changed(Fingerprint),
    Invalidated,
    Removed,
}

impl AccessorEvent {
    /// The event's kind as one lowercase word: `initial`, `status`, `progress`, `log`, `changed`,
    /// `invalidated`, `removed`. The Python, JavaScript and WebSocket forms use it.
    pub fn kind(&self) -> &'static str {
        match self {
            AccessorEvent::Initial => "initial",
            AccessorEvent::Status(_) => "status",
            AccessorEvent::PrimaryProgress(_) | AccessorEvent::SecondaryProgress(_) => "progress",
            AccessorEvent::Log(_) => "log",
            AccessorEvent::Changed(_) => "changed",
            AccessorEvent::Invalidated => "invalidated",
            AccessorEvent::Removed => "removed",
        }
    }

    /// The JSON form of the WebSocket, Python and JavaScript events: `{"event": kind, …}`.
    pub fn to_json(&self) -> serde_json::Value {
        let mut map = serde_json::Map::new();
        map.insert("event".into(), self.kind().into());
        match self {
            AccessorEvent::Initial | AccessorEvent::Invalidated | AccessorEvent::Removed => {}
            AccessorEvent::Status(status) => {
                map.insert(
                    "status".into(),
                    serde_json::to_value(status).unwrap_or(serde_json::Value::Null),
                );
            }
            AccessorEvent::PrimaryProgress(p) => {
                map.insert(
                    "primary".into(),
                    serde_json::to_value(p).unwrap_or(serde_json::Value::Null),
                );
            }
            AccessorEvent::SecondaryProgress(p) => {
                map.insert(
                    "secondary".into(),
                    serde_json::to_value(p).unwrap_or(serde_json::Value::Null),
                );
            }
            AccessorEvent::Log(count) => {
                map.insert("count".into(), (*count).into());
            }
            AccessorEvent::Changed(fp) => {
                map.insert("etag".into(), fp.token().into());
            }
        }
        serde_json::Value::Object(map)
    }
}

/// The current state behind the events. `status` is [`Status::None`] when the value does not
/// exist; `fingerprint` is then `None` too.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AccessorInfo {
    pub status: Status,
    pub message: String,
    pub primary_progress: Option<ProgressEntry>,
    pub secondary_progress: Option<ProgressEntry>,
    pub log_len: usize,
    pub fingerprint: Option<Fingerprint>,
}

impl AccessorInfo {
    /// Info with only a status.
    pub fn with_status(status: Status) -> Self {
        AccessorInfo {
            status,
            message: String::new(),
            primary_progress: None,
            secondary_progress: None,
            log_len: 0,
            fingerprint: None,
        }
    }
}

/// What the hub does when the source reports `Expired`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ExpiryPolicy {
    /// Relay the status; the holder calls `get`, which re-evaluates.
    #[default]
    Relay,
    /// Relay the status and read the root at once, so `Changed` follows with no holder action.
    Reload,
}

/// What an accessor can do.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct Capabilities {
    /// A set can succeed.
    pub write: bool,
    /// Changes are pushed as events; otherwise they are found on `get` and `refresh`.
    pub notify: bool,
}

/// Options of a set. Reserved for later (locking, `VALUE-ACCESSOR-CANNOT-LOCK`).
#[derive(Debug, Clone, Default)]
#[non_exhaustive]
pub struct SetOptions {}

/// What a part is, for an editor: the step that made it, its type, and for a query argument the
/// argument's metadata and command.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct PartDescription {
    pub step: String,
    pub type_identifier: String,
    pub argument: Option<ArgumentInfo>,
    pub command: Option<CommandKey>,
    /// A recipe's `arguments` or `links` names this argument, so its value in the query is not the
    /// one evaluation uses.
    pub overridden: bool,
    pub read_only: bool,
}

/// A handle to a settable value. See the module documentation.
#[cfg_attr(not(target_arch = "wasm32"), async_trait::async_trait)]
#[cfg_attr(target_arch = "wasm32", async_trait::async_trait(?Send))]
pub trait ValueAccessor<V: ValueInterface>: MaybeSend + MaybeSync + Debug {
    /// The query that names this accessor (empty for one made in code).
    fn address(&self) -> &Query;
    /// Reads the part. A value that does not exist is the root's not-found error.
    async fn get(&self) -> Result<AccessorSnapshot<V>, Error>;
    /// Sets the part, checked against this handle's last-seen fingerprint. One query asset is one
    /// handle shared by all its holders, so this catches other handles and outside writers, not
    /// another holder of the same handle: a holder that needs that keeps its own token and uses
    /// [`set_if`](Self::set_if).
    async fn set(&self, value: V, options: SetOptions) -> Result<AccessorSnapshot<V>, Error>;
    /// Sets the part if its fingerprint is `expected`; `None` means it must not exist yet (a
    /// conditional create). A mismatch is `Error::accessor_conflict`.
    async fn set_if(
        &self,
        value: V,
        expected: Option<Fingerprint>,
        options: SetOptions,
    ) -> Result<AccessorSnapshot<V>, Error>;
    /// Status, progress, log length and fingerprint, without evaluating anything.
    async fn info(&self) -> Result<AccessorInfo, Error>;
    /// The log messages from index `since`.
    async fn log(&self, since: usize) -> Result<Vec<LogEntry>, Error>;
    /// What the part is, for an editor.
    async fn describe(&self) -> Result<PartDescription, Error>;
    fn capabilities(&self) -> Capabilities;
    /// Pulls the source's pending events (and, for a source that cannot notify, checks its
    /// version). On wasm, and wherever no watcher runs, this is how events arrive.
    async fn refresh(&self) -> Result<(), Error>;
    fn subscribe(&self) -> watch::Receiver<AccessorEvent>;
    /// A projection of this accessor through `step`, named by `address`.
    fn project(
        &self,
        step: Arc<dyn ProjectionStep<V>>,
        address: Query,
    ) -> Result<Arc<dyn ValueAccessor<V>>, Error>;
    /// Called once when a structural change invalidates this accessor.
    fn set_invalidation_sink(&self, sink: Arc<dyn InvalidationSink>);
    /// The watcher future, for a host that spawns it itself (wasm, or native with no runtime at
    /// construction). `None` when there is nothing to watch or it already runs.
    fn driver(&self) -> Option<BoxFuture<'static, ()>> {
        None
    }
}

/// A lens from a value to a part of it. Synchronous: only the root does I/O.
pub trait ProjectionStep<V: ValueInterface>: MaybeSend + MaybeSync + Debug {
    fn name(&self) -> Cow<'_, str>;
    /// The part, or `None` when the slot is gone.
    fn get(&self, parent: &V) -> Result<Option<V>, Error>;
    /// The parent with the part replaced.
    fn put(&self, parent: &V, part: V) -> Result<V, Error>;
    /// Where the part sits in `parent`; `None` when the slot is gone. A different anchor means the
    /// part moved, which invalidates its projection.
    fn anchor(&self, parent: &V) -> Result<Option<u64>, Error> {
        Ok(self.get(parent)?.map(|_| 0))
    }
    fn describe(&self, parent: &V) -> Result<PartDescription, Error>;
    /// Names of arguments whose query value `part` supersedes (a recipe's `arguments` and
    /// `links`), so a projection further down reports them as `overridden`.
    fn overridden_arguments(&self, _part: &V) -> Vec<String> {
        Vec::new()
    }
}

/// Told when a structural change invalidates an accessor (the `acc` commands expire its asset).
#[cfg_attr(not(target_arch = "wasm32"), async_trait::async_trait)]
#[cfg_attr(target_arch = "wasm32", async_trait::async_trait(?Send))]
pub trait InvalidationSink: MaybeSend + MaybeSync {
    async fn invalidated(&self);
}

/// An event a root reports to its hub.
#[derive(Debug, Clone, PartialEq)]
pub enum RootEvent {
    Status(Status),
    PrimaryProgress(ProgressEntry),
    SecondaryProgress(ProgressEntry),
    Log(usize),
    Changed(Version),
    Removed,
}

/// What a custom source implements; [`AccessorHub::new`] supplies everything else.
#[cfg_attr(not(target_arch = "wasm32"), async_trait::async_trait)]
#[cfg_attr(target_arch = "wasm32", async_trait::async_trait(?Send))]
pub trait AccessorRoot<V: ValueInterface>: MaybeSend + MaybeSync + Debug {
    /// The value and its version, or `None` when it does not exist.
    async fn read(&self) -> Result<Option<(State<V>, Version)>, Error>;
    /// Writes a new value, creating it when it does not exist; returns the value as written (its
    /// status the root's: `Source`, `Override`) and its new version. The hub passes a state whose
    /// status was reset to `None`.
    async fn write(&self, _state: State<V>) -> Result<(State<V>, Version), Error> {
        Err(Error::not_supported("this accessor is read-only".into()))
    }
    /// Status, message, progress and log length (the fingerprint is the hub's).
    async fn info(&self) -> Result<AccessorInfo, Error>;
    /// The log messages from index `since`.
    async fn log(&self, _since: usize) -> Result<Vec<LogEntry>, Error> {
        Ok(Vec::new())
    }
    fn capabilities(&self) -> Capabilities;
    /// The next pending event, without waiting.
    async fn poll_event(&self) -> Option<RootEvent> {
        None
    }
    /// A future that completes when `poll_event` has something. It owns what it waits on.
    fn watch(&self) -> Option<BoxFuture<'static, ()>> {
        None
    }
    /// The not-found error of this root, for a `get` of a value that does not exist.
    fn not_found(&self) -> Error {
        Error::not_available()
    }
}

/// A type-erased accessor, so a generic value type can carry one through a downcast.
pub trait ErasedAccessor: MaybeSend + MaybeSync + Debug {
    fn as_any(&self) -> &dyn std::any::Any;
}

/// The carrier [`ErasedAccessor`] downcasts to.
#[derive(Debug, Clone)]
pub struct AccessorBox<V: ValueInterface>(pub Arc<dyn ValueAccessor<V>>);

impl<V: ValueInterface> ErasedAccessor for AccessorBox<V> {
    fn as_any(&self) -> &dyn std::any::Any {
        self
    }
}

/// The content fingerprint of a value: the hash of its JSON, or `fallback` without one.
pub(crate) fn content_version<V: ValueInterface>(value: &V, fallback: Version) -> Version {
    match value.try_into_json_value() {
        Ok(json) => match serde_json::to_vec(&json) {
            Ok(bytes) => Version::from_bytes(&bytes),
            Err(_) => fallback,
        },
        Err(_) => fallback,
    }
}

/// A stable 64-bit hash (blake3), for anchors.
pub(crate) fn stable_hash(parts: &[&[u8]]) -> u64 {
    let mut hasher = blake3::Hasher::new();
    for part in parts {
        hasher.update(&(part.len() as u64).to_le_bytes());
        hasher.update(part);
    }
    let bytes = hasher.finalize();
    let mut eight = [0u8; 8];
    eight.copy_from_slice(&bytes.as_bytes()[0..8]);
    u64::from_le_bytes(eight)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fingerprint_token_round_trips() {
        let fp = Fingerprint {
            anchor: 0x0123_4567_89ab_cdef,
            content: Version::from_bytes(b"x"),
        };
        let token = fp.token();
        assert_eq!(Fingerprint::from_token(&token).unwrap(), fp);
        assert_eq!(Fingerprint::from_token(&format!("\"{token}\"")).unwrap(), fp);
        for bad in ["", "abc", "0123-45", "zzzzzzzzzzzzzzzz-00000000000000000000000000000000"] {
            assert!(Fingerprint::from_token(bad).is_err(), "{bad}");
        }
        let json = serde_json::to_string(&fp).unwrap();
        assert_eq!(serde_json::from_str::<Fingerprint>(&json).unwrap(), fp);
    }

    #[test]
    fn conflict_token_is_read_back_from_the_error() {
        let fp = Fingerprint {
            anchor: 7,
            content: Version::new(9),
        };
        let e = Error::accessor_conflict(&Query::new(), Some(&fp));
        assert_eq!(e.error_type, crate::error::ErrorType::StatusConflict);
        assert_eq!(conflict_token(&e), Some(fp));
        assert_eq!(conflict_token(&Error::accessor_conflict(&Query::new(), None)), None);
    }
}
