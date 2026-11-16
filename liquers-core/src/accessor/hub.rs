//! The hub: one per root, shared by every accessor of that root and its projections.
//!
//! The hub holds the projection tree (children by weak reference, parents by strong reference),
//! the last value read from the root, and a write lock. After every change of the root it derives
//! each live projection and classifies it: **unchanged** (nothing happens), **changed** (same slot,
//! new content: `Changed`), or **invalidated** (its slot moved or is gone: `Invalidated`, its sink
//! is called, and it stays dead). A set is conditional on the part's own fingerprint, so edits of
//! disjoint parts never conflict.
//!
//! Locks: the write lock (`tokio::sync::Mutex`) is held across a whole set, read → compare → put →
//! write, which serialises writers of one hub. The tree and the cache sit behind
//! `std::sync::Mutex`es that are never held across an `.await`; events and sinks are collected
//! under them and fired after.

use std::fmt::Debug;
use std::sync::{Arc, Mutex, Weak};

use tokio::sync::watch;

use super::{
    content_version, stable_hash, AccessorEvent, AccessorInfo, AccessorRoot, AccessorSnapshot,
    Capabilities, ExpiryPolicy, Fingerprint, InvalidationSink, PartDescription, ProjectionStep,
    RootEvent, SetOptions, ValueAccessor,
};
use crate::error::Error;
use crate::maybe_send::BoxFuture;
use crate::metadata::{LogEntry, Metadata, Status, Version};
use crate::query::Query;
use crate::state::State;
use crate::value::ValueInterface;

/// A `NotAvailable` error about `address`.
pub(crate) fn unavailable(address: &Query, message: &str) -> Error {
    let mut error = Error::not_available().with_query(address);
    error.message = message.to_string();
    error
}

/// At most this many pending root events are handled by one drain.
const MAX_EVENTS_PER_DRAIN: usize = 1024;

/// Turns an [`AccessorRoot`] into a full accessor. See the module documentation.
pub struct AccessorHub<V: ValueInterface> {
    root: Arc<dyn AccessorRoot<V>>,
    policy: ExpiryPolicy,
    write_lock: tokio::sync::Mutex<()>,
    cache: Mutex<Cache<V>>,
    root_node: Mutex<Weak<Node<V>>>,
    driver: Mutex<Option<BoxFuture<'static, ()>>>,
    /// Dropped with the hub, which ends a watcher still waiting on the root.
    _closed: watch::Sender<()>,
}

struct Cache<V: ValueInterface> {
    /// `false` until the root has been read once.
    known: bool,
    root: Option<(State<V>, Version)>,
}

impl<V: ValueInterface> AccessorHub<V> {
    /// A full accessor of `root`, named by `address`.
    ///
    /// On native targets, when a tokio runtime is running, a watcher task relays the root's
    /// events as they come; otherwise [`ValueAccessor::driver`] hands that future out, and
    /// [`ValueAccessor::refresh`] pulls events.
    pub fn new(
        root: Arc<dyn AccessorRoot<V>>,
        address: Query,
        policy: ExpiryPolicy,
    ) -> Arc<dyn ValueAccessor<V>> {
        let (closed, closed_rx) = watch::channel(());
        let hub = Arc::new(AccessorHub {
            _closed: closed,
            root,
            policy,
            write_lock: tokio::sync::Mutex::new(()),
            cache: Mutex::new(Cache {
                known: false,
                root: None,
            }),
            root_node: Mutex::new(Weak::new()),
            driver: Mutex::new(None),
        });
        let node = Arc::new(Node {
            hub: hub.clone(),
            parent: None,
            step: None,
            address,
            sender: watch::Sender::new(AccessorEvent::Initial),
            state: Mutex::new(NodeStateOf::default()),
        });
        if let Ok(mut root_node) = hub.root_node.lock() {
            *root_node = Arc::downgrade(&node);
        }
        if hub.root.watch().is_some() {
            let watcher = Self::watcher(Arc::downgrade(&hub), closed_rx);
            Self::start(&hub, watcher);
        }
        Arc::new(HubAccessor { node })
    }

    #[cfg(not(target_arch = "wasm32"))]
    fn start(hub: &Arc<Self>, watcher: BoxFuture<'static, ()>) {
        if tokio::runtime::Handle::try_current().is_ok() {
            tokio::spawn(watcher);
        } else if let Ok(mut driver) = hub.driver.lock() {
            *driver = Some(watcher);
        }
    }

    #[cfg(target_arch = "wasm32")]
    fn start(hub: &Arc<Self>, watcher: BoxFuture<'static, ()>) {
        if let Ok(mut driver) = hub.driver.lock() {
            *driver = Some(watcher);
        }
    }

    /// Waits for the root's events and handles them, until the hub is dropped.
    fn watcher(weak: Weak<Self>, mut closed: watch::Receiver<()>) -> BoxFuture<'static, ()> {
        Box::pin(async move {
            loop {
                let wait = match weak.upgrade() {
                    Some(hub) => match hub.root.watch() {
                        Some(wait) => wait,
                        None => break,
                    },
                    None => break,
                };
                let hub_dropped = Box::pin(closed.changed());
                match futures::future::select(wait, hub_dropped).await {
                    futures::future::Either::Left(((), _)) => {}
                    futures::future::Either::Right((_, _)) => break,
                }
                match weak.upgrade() {
                    Some(hub) => hub.drain_events().await,
                    None => break,
                }
            }
        })
    }

    fn root_node(&self) -> Option<Arc<Node<V>>> {
        self.root_node.lock().ok().and_then(|n| n.upgrade())
    }

    fn cached(&self) -> Option<Option<(State<V>, Version)>> {
        match self.cache.lock() {
            Ok(cache) if cache.known => Some(cache.root.clone()),
            Ok(_) | Err(_) => None,
        }
    }

    /// Handles every pending root event.
    async fn drain_events(&self) {
        for _ in 0..MAX_EVENTS_PER_DRAIN {
            let Some(event) = self.root.poll_event().await else {
                break;
            };
            self.handle_event(event).await;
        }
    }

    async fn handle_event(&self, event: RootEvent) {
        match event {
            RootEvent::Status(status) => {
                self.broadcast(AccessorEvent::Status(status));
                if status == Status::Expired && self.policy == ExpiryPolicy::Reload {
                    let _ = self.read_root().await;
                }
            }
            RootEvent::PrimaryProgress(p) => self.broadcast(AccessorEvent::PrimaryProgress(p)),
            RootEvent::SecondaryProgress(p) => {
                self.broadcast(AccessorEvent::SecondaryProgress(p))
            }
            RootEvent::Log(n) => self.broadcast(AccessorEvent::Log(n)),
            RootEvent::Changed(version) => {
                let same = matches!(self.cached(), Some(Some((_, v))) if v == version && !version.is_unknown());
                if !same {
                    let _ = self.read_root().await;
                }
            }
            RootEvent::Removed => {
                self.process_root(None).await;
            }
        }
    }

    /// Sends `event` to every live node that is not invalidated.
    fn broadcast(&self, event: AccessorEvent) {
        let mut senders = Vec::new();
        if let Some(root) = self.root_node() {
            root.collect_live(&mut senders);
        }
        for sender in senders {
            sender.send_replace(event.clone());
        }
    }

    /// Reads the root, updates the cache and notifies the tree of a change.
    async fn read_root(&self) -> Result<Option<(State<V>, Version)>, Error> {
        let read = self.root.read().await?;
        self.process_root(read.clone()).await;
        Ok(read)
    }

    /// Records a new root value and classifies every live node against it.
    async fn process_root(&self, new: Option<(State<V>, Version)>) {
        let unchanged = match self.cache.lock() {
            Ok(mut cache) => {
                let unchanged = cache.known
                    && match (&cache.root, &new) {
                        (Some((_, old)), Some((_, now))) => old == now && !now.is_unknown(),
                        (None, None) => true,
                        (Some(_), None) | (None, Some(_)) => false,
                    };
                cache.known = true;
                cache.root = new.clone();
                unchanged
            }
            Err(_) => false,
        };
        if unchanged {
            return;
        }
        let mut fire = Fire::default();
        if let Some(root) = self.root_node() {
            let root_value = new.as_ref().and_then(|(s, v)| s.value().ok().map(|x| (x, *v)));
            root.classify(root_value.as_ref().map(|(x, v)| (x.as_ref(), *v)), None, &mut fire);
        }
        fire.fire().await;
    }

    async fn refresh(&self) -> Result<(), Error> {
        self.drain_events().await;
        if !self.root.capabilities().notify || self.cached().is_none() {
            self.read_root().await?;
        }
        Ok(())
    }
}

impl<V: ValueInterface> Debug for AccessorHub<V> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("AccessorHub")
            .field("root", &self.root)
            .field("policy", &self.policy)
            .finish()
    }
}

/// Events and sinks collected under the locks, fired after.
#[derive(Default)]
struct Fire {
    events: Vec<(watch::Sender<AccessorEvent>, AccessorEvent)>,
    sinks: Vec<Arc<dyn InvalidationSink>>,
}

impl Fire {
    async fn fire(self) {
        for (sender, event) in self.events {
            sender.send_replace(event);
        }
        for sink in self.sinks {
            sink.invalidated().await;
        }
    }
}

/// One accessor in the tree: the root, or a projection of its parent.
struct Node<V: ValueInterface> {
    hub: Arc<AccessorHub<V>>,
    parent: Option<Arc<Node<V>>>,
    step: Option<Arc<dyn ProjectionStep<V>>>,
    address: Query,
    sender: watch::Sender<AccessorEvent>,
    state: Mutex<NodeStateOf<V>>,
}

struct NodeStateOf<V: ValueInterface> {
    current: Option<Option<Fingerprint>>,
    last_seen: Option<Option<Fingerprint>>,
    invalidated: bool,
    sink: Option<Arc<dyn InvalidationSink>>,
    children: Vec<Weak<Node<V>>>,
}

impl<V: ValueInterface> Default for NodeStateOf<V> {
    fn default() -> Self {
        NodeStateOf {
            current: None,
            last_seen: None,
            invalidated: false,
            sink: None,
            children: Vec::new(),
        }
    }
}

/// A derived part: the value and its fingerprint.
struct Derived<V> {
    value: V,
    fingerprint: Fingerprint,
}

impl<V: ValueInterface> Node<V> {
    fn is_invalidated(&self) -> bool {
        self.state.lock().map(|s| s.invalidated).unwrap_or(true)
    }

    fn children(&self) -> Vec<Arc<Node<V>>> {
        match self.state.lock() {
            Ok(mut s) => {
                s.children.retain(|c| c.strong_count() > 0);
                s.children.iter().filter_map(|c| c.upgrade()).collect()
            }
            Err(_) => Vec::new(),
        }
    }

    fn collect_live(&self, senders: &mut Vec<watch::Sender<AccessorEvent>>) {
        if self.is_invalidated() {
            return;
        }
        senders.push(self.sender.clone());
        for child in self.children() {
            child.collect_live(senders);
        }
    }

    /// Derives this node from its parent's derived value (the root's value for the root node).
    fn derive(
        &self,
        root: Option<(&V, Version)>,
        parent: Option<&Derived<V>>,
    ) -> Option<Derived<V>> {
        match (&self.step, parent) {
            (None, _) => root.map(|(value, version)| {
                let content = if version.is_unknown() {
                    content_version(value, version)
                } else {
                    version
                };
                Derived {
                    value: value.clone(),
                    fingerprint: Fingerprint { anchor: 0, content },
                }
            }),
            (Some(_), None) => None,
            (Some(step), Some(parent)) => {
                let part = step.get(&parent.value).ok().flatten()?;
                let own = step.anchor(&parent.value).ok().flatten()?;
                let version = root.map(|(_, v)| v).unwrap_or_else(Version::unknown);
                let anchor = stable_hash(&[
                    &parent.fingerprint.anchor.to_le_bytes(),
                    &own.to_le_bytes(),
                ]);
                let content = content_version(&part, version);
                Some(Derived {
                    value: part,
                    fingerprint: Fingerprint { anchor, content },
                })
            }
        }
    }

    /// The chain from the root node to this node, root first.
    fn chain(self: &Arc<Self>) -> Vec<Arc<Node<V>>> {
        let mut chain = vec![self.clone()];
        let mut at = self.parent.clone();
        while let Some(node) = at {
            at = node.parent.clone();
            chain.push(node);
        }
        chain.reverse();
        chain
    }

    /// Derives every node of the chain from a root value, root first; stops at the first absent.
    fn derive_chain(
        self: &Arc<Self>,
        root: Option<(&V, Version)>,
    ) -> Vec<Option<Derived<V>>> {
        let mut out: Vec<Option<Derived<V>>> = Vec::new();
        for node in self.chain() {
            let parent = out.last().and_then(|d| d.as_ref());
            let derived = node.derive(root, parent);
            out.push(derived);
        }
        out
    }

    /// Classifies this node and its subtree against a new root value.
    fn classify(&self, root: Option<(&V, Version)>, parent: Option<&Derived<V>>, fire: &mut Fire) {
        if self.is_invalidated() {
            return;
        }
        let derived = self.derive(root, parent);
        let new_fp = derived.as_ref().map(|d| d.fingerprint);
        let is_root = self.step.is_none();
        let mut invalidate = false;
        if let Ok(mut s) = self.state.lock() {
            match (s.current, new_fp) {
                (None, _) => {}
                (Some(None), Some(fp)) => {
                    fire.events.push((self.sender.clone(), AccessorEvent::Changed(fp)));
                }
                (Some(None), None) => {}
                (Some(Some(_)), None) => {
                    if is_root {
                        fire.events.push((self.sender.clone(), AccessorEvent::Removed));
                    } else {
                        invalidate = true;
                    }
                }
                (Some(Some(old)), Some(fp)) => {
                    if !is_root && old.anchor != fp.anchor {
                        invalidate = true;
                    } else if old != fp {
                        fire.events.push((self.sender.clone(), AccessorEvent::Changed(fp)));
                    }
                }
            }
            s.current = Some(new_fp);
        }
        if invalidate {
            self.invalidate(fire);
            return;
        }
        for child in self.children() {
            child.classify(root, derived.as_ref(), fire);
        }
    }

    /// Marks this node and its subtree invalidated, once.
    fn invalidate(&self, fire: &mut Fire) {
        let sink = match self.state.lock() {
            Ok(mut s) if !s.invalidated => {
                s.invalidated = true;
                s.current = Some(None);
                s.sink.take()
            }
            Ok(_) | Err(_) => return,
        };
        fire.events.push((self.sender.clone(), AccessorEvent::Invalidated));
        if let Some(sink) = sink {
            fire.sinks.push(sink);
        }
        for child in self.children() {
            child.invalidate(fire);
        }
    }

    fn invalidated_error(&self) -> Error {
        unavailable(&self.address, "the accessor was invalidated by a structural change; evaluate its query again")
    }
}

/// The [`ValueAccessor`] the hub hands out.
struct HubAccessor<V: ValueInterface> {
    node: Arc<Node<V>>,
}

impl<V: ValueInterface> Debug for HubAccessor<V> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Accessor")
            .field("address", &self.node.address.encode())
            .field("root", &self.node.hub.root)
            .finish()
    }
}

/// What a set is checked against.
enum Expect {
    LastSeen,
    Explicit(Option<Fingerprint>),
}

impl<V: ValueInterface> HubAccessor<V> {
    fn hub(&self) -> &AccessorHub<V> {
        &self.node.hub
    }

    fn snapshot(
        &self,
        root: &Option<(State<V>, Version)>,
        derived: Derived<V>,
    ) -> AccessorSnapshot<V> {
        let (metadata, version) = match root {
            Some((state, version)) => (state.metadata.clone(), *version),
            None => (Arc::new(Metadata::new()), Version::unknown()),
        };
        let state = if self.node.step.is_none() {
            match root {
                Some((state, _)) => state.clone(),
                None => State::from_value_and_metadata(derived.value, metadata),
            }
        } else {
            let mut part_metadata = Metadata::new();
            let _ = part_metadata.set_status(metadata.status());
            State::from_value_and_metadata(derived.value, Arc::new(part_metadata))
        };
        AccessorSnapshot {
            state,
            fingerprint: derived.fingerprint,
            root_version: version,
        }
    }

    fn remember(&self, seen: Option<Fingerprint>) {
        if let Ok(mut s) = self.node.state.lock() {
            s.last_seen = Some(seen);
        }
    }

    async fn do_set(&self, value: V, expect: Expect) -> Result<AccessorSnapshot<V>, Error> {
        if self.node.is_invalidated() {
            return Err(self.node.invalidated_error());
        }
        let hub = self.hub();
        let _guard = hub.write_lock.lock().await;
        hub.drain_events().await;
        let root = hub.read_root().await?;
        let root_view = root
            .as_ref()
            .and_then(|(s, v)| s.value().ok().map(|x| (x, *v)));
        let chain = self
            .node
            .derive_chain(root_view.as_ref().map(|(x, v)| (x.as_ref(), *v)));
        let current = chain.last().and_then(|d| d.as_ref()).map(|d| d.fingerprint);
        let expected = match expect {
            Expect::Explicit(expected) => Some(expected),
            Expect::LastSeen => self.node.state.lock().ok().and_then(|s| s.last_seen),
        };
        if let Some(expected) = expected {
            if expected != current {
                return Err(Error::accessor_conflict(&self.node.address, current.as_ref()));
            }
        }
        // Put the part back up the chain.
        let nodes = self.node.chain();
        let mut new_value = value;
        for i in (1..nodes.len()).rev() {
            let step = nodes[i]
                .step
                .as_ref()
                .ok_or_else(|| Error::unexpected_error("projection without a step".into()))?;
            let parent = chain[i - 1].as_ref().ok_or_else(|| {
                unavailable(&self.node.address, "there is no value to set a part of")
            })?;
            new_value = step.put(&parent.value, new_value)?;
        }
        // A status read back (`Expired`, `Error`) must not travel into the write.
        let mut metadata = match &root {
            Some((state, _)) => (*state.metadata).clone(),
            None => Metadata::new(),
        };
        metadata.set_status(Status::None)?;
        let state = State::from_value_and_metadata(new_value, Arc::new(metadata));
        let new_root = Some(hub.root.write(state).await?);
        hub.process_root(new_root.clone()).await;
        let root_view = new_root
            .as_ref()
            .and_then(|(s, v)| s.value().ok().map(|x| (x, *v)));
        let derived = self
            .node
            .derive_chain(root_view.as_ref().map(|(x, v)| (x.as_ref(), *v)))
            .pop()
            .flatten()
            .ok_or_else(|| {
                unavailable(&self.node.address, "the part is gone after the set")
            })?;
        self.remember(Some(derived.fingerprint));
        Ok(self.snapshot(&new_root, derived))
    }
}

#[cfg_attr(not(target_arch = "wasm32"), async_trait::async_trait)]
#[cfg_attr(target_arch = "wasm32", async_trait::async_trait(?Send))]
impl<V: ValueInterface> ValueAccessor<V> for HubAccessor<V> {
    fn address(&self) -> &Query {
        &self.node.address
    }

    async fn get(&self) -> Result<AccessorSnapshot<V>, Error> {
        if self.node.is_invalidated() {
            return Err(self.node.invalidated_error());
        }
        let hub = self.hub();
        hub.drain_events().await;
        let root = hub.read_root().await?;
        if self.node.is_invalidated() {
            return Err(self.node.invalidated_error());
        }
        let root_view = root
            .as_ref()
            .and_then(|(s, v)| s.value().ok().map(|x| (x, *v)));
        if root.is_some() && root_view.is_none() {
            // An error or cancelled state: report its error.
            if let Some((state, _)) = &root {
                state.value()?;
            }
        }
        let derived = self
            .node
            .derive_chain(root_view.as_ref().map(|(x, v)| (x.as_ref(), *v)))
            .pop()
            .flatten();
        match derived {
            Some(derived) => {
                self.remember(Some(derived.fingerprint));
                Ok(self.snapshot(&root, derived))
            }
            None => {
                self.remember(None);
                if self.node.step.is_none() {
                    Err(hub.root.not_found())
                } else {
                    Err(unavailable(&self.node.address, "the part does not exist"))
                }
            }
        }
    }

    async fn set(&self, value: V, _options: SetOptions) -> Result<AccessorSnapshot<V>, Error> {
        self.do_set(value, Expect::LastSeen).await
    }

    async fn set_if(
        &self,
        value: V,
        expected: Option<Fingerprint>,
        _options: SetOptions,
    ) -> Result<AccessorSnapshot<V>, Error> {
        self.do_set(value, Expect::Explicit(expected)).await
    }

    async fn info(&self) -> Result<AccessorInfo, Error> {
        let mut info = self.hub().root.info().await?;
        let (invalidated, current) = match self.node.state.lock() {
            Ok(s) => (s.invalidated, s.current),
            Err(_) => (true, None),
        };
        if invalidated {
            info.status = Status::None;
            info.fingerprint = None;
            return Ok(info);
        }
        info.fingerprint = match current {
            Some(fp) => fp,
            None => {
                // Not classified yet: derive from the cache, if there is one.
                match self.hub().cached() {
                    Some(root) => {
                        let view = root
                            .as_ref()
                            .and_then(|(s, v)| s.value().ok().map(|x| (x, *v)));
                        self.node
                            .derive_chain(view.as_ref().map(|(x, v)| (x.as_ref(), *v)))
                            .pop()
                            .flatten()
                            .map(|d| d.fingerprint)
                    }
                    None => None,
                }
            }
        };
        if self.node.step.is_some() && info.fingerprint.is_none() && current.is_some() {
            info.status = Status::None;
        }
        Ok(info)
    }

    async fn log(&self, since: usize) -> Result<Vec<LogEntry>, Error> {
        self.hub().root.log(since).await
    }

    async fn describe(&self) -> Result<PartDescription, Error> {
        if self.node.is_invalidated() {
            return Err(self.node.invalidated_error());
        }
        let root = match self.hub().cached() {
            Some(root) => root,
            None => self.hub().read_root().await?,
        };
        let read_only = !self.capabilities().write;
        let view = root
            .as_ref()
            .and_then(|(s, v)| s.value().ok().map(|x| (x, *v)));
        let chain = self
            .node
            .derive_chain(view.as_ref().map(|(x, v)| (x.as_ref(), *v)));
        match &self.node.step {
            None => {
                let type_identifier = match &root {
                    Some((state, _)) => state.type_identifier().to_string(),
                    None => String::new(),
                };
                Ok(PartDescription {
                    step: "root".into(),
                    type_identifier,
                    read_only,
                    ..PartDescription::default()
                })
            }
            Some(step) => {
                let n = chain.len();
                let parent = chain
                    .get(n.wrapping_sub(2))
                    .and_then(|d| d.as_ref())
                    .ok_or_else(|| {
                        unavailable(&self.node.address, "the part does not exist")
                    })?;
                let mut description = step.describe(&parent.value)?;
                if description.type_identifier.is_empty() {
                    if let Some(Some(part)) = chain.last() {
                        description.type_identifier = part.value.identifier().to_string();
                    }
                }
                description.read_only = description.read_only || read_only;
                if let Some(argument) = &description.argument {
                    let nodes = self.node.chain();
                    description.overridden = description.overridden
                        || nodes.iter().zip(chain.iter()).any(|(node, derived)| {
                            match (&node.step, derived) {
                                (Some(step), Some(derived)) => step
                                    .overridden_arguments(&derived.value)
                                    .contains(&argument.name),
                                (Some(_), None) | (None, _) => false,
                            }
                        });
                }
                Ok(description)
            }
        }
    }

    fn capabilities(&self) -> Capabilities {
        self.hub().root.capabilities()
    }

    async fn refresh(&self) -> Result<(), Error> {
        self.hub().refresh().await
    }

    fn subscribe(&self) -> watch::Receiver<AccessorEvent> {
        self.node.sender.subscribe()
    }

    fn project(
        &self,
        step: Arc<dyn ProjectionStep<V>>,
        address: Query,
    ) -> Result<Arc<dyn ValueAccessor<V>>, Error> {
        if self.node.is_invalidated() {
            return Err(self.node.invalidated_error());
        }
        let child = Arc::new(Node {
            hub: self.node.hub.clone(),
            parent: Some(self.node.clone()),
            step: Some(step),
            address,
            sender: watch::Sender::new(AccessorEvent::Initial),
            state: Mutex::new(NodeStateOf::default()),
        });
        // Classify the child against what the hub already knows, so its first change is seen.
        if let Some(root) = self.hub().cached() {
            let view = root
                .as_ref()
                .and_then(|(s, v)| s.value().ok().map(|x| (x, *v)));
            let fp = child
                .derive_chain(view.as_ref().map(|(x, v)| (x.as_ref(), *v)))
                .pop()
                .flatten()
                .map(|d| d.fingerprint);
            if let Ok(mut s) = child.state.lock() {
                s.current = Some(fp);
            }
        }
        if let Ok(mut s) = self.node.state.lock() {
            s.children.retain(|c| c.strong_count() > 0);
            s.children.push(Arc::downgrade(&child));
        }
        Ok(Arc::new(HubAccessor { node: child }))
    }

    fn set_invalidation_sink(&self, sink: Arc<dyn InvalidationSink>) {
        if let Ok(mut s) = self.node.state.lock() {
            s.sink = Some(sink);
        }
    }

    fn driver(&self) -> Option<BoxFuture<'static, ()>> {
        self.hub().driver.lock().ok().and_then(|mut d| d.take())
    }
}

#[cfg(test)]
mod tests {
    use std::borrow::Cow;
    use std::collections::BTreeMap;
    use std::sync::atomic::{AtomicUsize, Ordering};

    use super::*;
    use crate::accessor::{memory_accessor, read_only, validated};
    use crate::error::ErrorType;
    use crate::value::Value;

    /// A field of a `Value::Object` (stands in for the document steps of `steps.rs`).
    #[derive(Debug)]
    struct Field(&'static str);
    impl ProjectionStep<Value> for Field {
        fn name(&self) -> Cow<'_, str> {
            Cow::Borrowed(self.0)
        }
        fn get(&self, parent: &Value) -> Result<Option<Value>, Error> {
            match parent {
                Value::Object(map) => Ok(map.get(self.0).cloned()),
                _ => Err(Error::conversion_error("value", "object")),
            }
        }
        fn put(&self, parent: &Value, part: Value) -> Result<Value, Error> {
            match parent {
                Value::Object(map) => {
                    let mut map = map.clone();
                    map.insert(self.0.to_string(), part);
                    Ok(Value::Object(map))
                }
                _ => Err(Error::conversion_error("value", "object")),
            }
        }
        fn describe(&self, _parent: &Value) -> Result<PartDescription, Error> {
            Ok(PartDescription {
                step: format!("field-{}", self.0),
                ..PartDescription::default()
            })
        }
    }

    /// An item of a `Value::Array`, anchored on the list length.
    #[derive(Debug)]
    struct Item(usize);
    impl ProjectionStep<Value> for Item {
        fn name(&self) -> Cow<'_, str> {
            Cow::Owned(format!("item-{}", self.0))
        }
        fn get(&self, parent: &Value) -> Result<Option<Value>, Error> {
            match parent {
                Value::Array(items) => Ok(items.get(self.0).cloned()),
                _ => Err(Error::conversion_error("value", "array")),
            }
        }
        fn put(&self, parent: &Value, part: Value) -> Result<Value, Error> {
            match parent {
                Value::Array(items) if self.0 < items.len() => {
                    let mut items = items.clone();
                    items[self.0] = part;
                    Ok(Value::Array(items))
                }
                _ => Err(Error::conversion_error("value", "array")),
            }
        }
        fn anchor(&self, parent: &Value) -> Result<Option<u64>, Error> {
            match parent {
                Value::Array(items) if self.0 < items.len() => Ok(Some(items.len() as u64)),
                _ => Ok(None),
            }
        }
        fn describe(&self, _parent: &Value) -> Result<PartDescription, Error> {
            Ok(PartDescription::default())
        }
    }

    fn object(pairs: &[(&str, i32)]) -> Value {
        Value::Object(
            pairs
                .iter()
                .map(|(k, v)| (k.to_string(), Value::from(*v)))
                .collect::<BTreeMap<_, _>>(),
        )
    }

    fn state(value: Value) -> State<Value> {
        State::from_value_and_metadata(value, Arc::new(Metadata::new()))
    }

    fn value_of(snap: &AccessorSnapshot<Value>) -> Value {
        (*snap.state.value().unwrap()).clone()
    }

    fn take(rx: &mut watch::Receiver<AccessorEvent>) -> Option<AccessorEvent> {
        if rx.has_changed().unwrap_or(false) {
            Some(rx.borrow_and_update().clone())
        } else {
            None
        }
    }

    #[tokio::test]
    async fn memory_accessor_is_a_shared_variable() {
        let variable = memory_accessor::<Value>(None, Query::new());
        assert_eq!(variable.info().await.unwrap().status, Status::None);
        assert!(variable.get().await.is_err());
        let other = variable.clone();
        let mut rx = other.subscribe();
        other.get().await.unwrap_err();
        variable.set(Value::from(1), SetOptions::default()).await.unwrap();
        assert!(matches!(take(&mut rx), Some(AccessorEvent::Changed(_))));
        assert_eq!(value_of(&other.get().await.unwrap()), Value::from(1));
        assert_eq!(other.info().await.unwrap().status, Status::Source);
    }

    #[tokio::test]
    async fn overlapping_projections_stay_in_step() {
        let root = memory_accessor(Some(state(object(&[("a", 1), ("b", 2)]))), Query::new());
        let a = root.project(Arc::new(Field("a")), Query::new()).unwrap();
        root.get().await.unwrap();
        a.get().await.unwrap();
        let mut root_rx = root.subscribe();
        let mut a_rx = a.subscribe();
        a.set(Value::from(10), SetOptions::default()).await.unwrap();
        assert!(matches!(take(&mut root_rx), Some(AccessorEvent::Changed(_))));
        assert_eq!(value_of(&root.get().await.unwrap()), object(&[("a", 10), ("b", 2)]));
        root.set(object(&[("a", 11), ("b", 2)]), SetOptions::default()).await.unwrap();
        assert!(matches!(take(&mut a_rx), Some(AccessorEvent::Changed(_))));
        assert_eq!(value_of(&a.get().await.unwrap()), Value::from(11));
    }

    #[tokio::test]
    async fn first_writer_wins_on_one_part() {
        let root = memory_accessor(Some(state(object(&[("a", 1)]))), Query::new());
        let a1 = root.project(Arc::new(Field("a")), Query::new()).unwrap();
        let a2 = root.project(Arc::new(Field("a")), Query::new()).unwrap();
        let base = a1.get().await.unwrap().fingerprint;
        a1.set_if(Value::from(2), Some(base), SetOptions::default()).await.unwrap();
        let err = a2
            .set_if(Value::from(3), Some(base), SetOptions::default())
            .await
            .unwrap_err();
        assert_eq!(err.error_type, ErrorType::StatusConflict);
        let current = a2.get().await.unwrap();
        assert_eq!(crate::accessor::conflict_token(&err), Some(current.fingerprint));
        assert_eq!(value_of(&current), Value::from(2));
        assert_eq!(value_of(&a1.get().await.unwrap()), Value::from(2));
    }

    #[tokio::test]
    async fn disjoint_parts_do_not_conflict() {
        let root = memory_accessor(Some(state(object(&[("a", 1), ("b", 2)]))), Query::new());
        let a = root.project(Arc::new(Field("a")), Query::new()).unwrap();
        let b = root.project(Arc::new(Field("b")), Query::new()).unwrap();
        let fa = a.get().await.unwrap().fingerprint;
        let fb = b.get().await.unwrap().fingerprint;
        a.set_if(Value::from(10), Some(fa), SetOptions::default()).await.unwrap();
        b.set_if(Value::from(20), Some(fb), SetOptions::default()).await.unwrap();
        assert_eq!(value_of(&root.get().await.unwrap()), object(&[("a", 10), ("b", 20)]));
        // `set` on the same handle uses its last-seen fingerprint, which the other set left valid.
        a.set(Value::from(11), SetOptions::default()).await.unwrap();
    }

    struct CountSink(Arc<AtomicUsize>);
    #[async_trait::async_trait]
    impl InvalidationSink for CountSink {
        async fn invalidated(&self) {
            self.0.fetch_add(1, Ordering::SeqCst);
        }
    }

    #[tokio::test]
    async fn structural_change_invalidates_and_content_change_keeps() {
        let list = Value::Array(vec![Value::from(1), Value::from(2)]);
        let root = memory_accessor(Some(state(list)), Query::new());
        root.get().await.unwrap();
        let item = root.project(Arc::new(Item(1)), Query::new()).unwrap();
        let count = Arc::new(AtomicUsize::new(0));
        item.set_invalidation_sink(Arc::new(CountSink(count.clone())));
        let mut rx = item.subscribe();
        // A content change of the item: Changed, still valid.
        root.set(Value::Array(vec![Value::from(1), Value::from(5)]), SetOptions::default())
            .await
            .unwrap();
        assert!(matches!(take(&mut rx), Some(AccessorEvent::Changed(_))));
        assert_eq!(value_of(&item.get().await.unwrap()), Value::from(5));
        // A change of the other item only: nothing for this one.
        root.set(Value::Array(vec![Value::from(9), Value::from(5)]), SetOptions::default())
            .await
            .unwrap();
        assert_eq!(take(&mut rx), None);
        // An insertion before it: Invalidated, the sink once, and the accessor stays dead.
        root.set(
            Value::Array(vec![Value::from(0), Value::from(9), Value::from(5)]),
            SetOptions::default(),
        )
        .await
        .unwrap();
        assert_eq!(take(&mut rx), Some(AccessorEvent::Invalidated));
        assert_eq!(count.load(Ordering::SeqCst), 1);
        assert!(item.get().await.is_err());
        root.set(Value::Array(vec![Value::from(0)]), SetOptions::default()).await.unwrap();
        assert_eq!(count.load(Ordering::SeqCst), 1);
    }

    #[tokio::test]
    async fn read_only_and_validated_refuse_sets() {
        let inner = memory_accessor(Some(state(object(&[("a", 1)]))), Query::new());
        let ro = read_only(inner.clone());
        let err = ro.set(object(&[]), SetOptions::default()).await.unwrap_err();
        assert_eq!(err.error_type, ErrorType::NotSupported);
        assert!(!ro.capabilities().write);
        let ro_a = ro.project(Arc::new(Field("a")), Query::new()).unwrap();
        assert!(ro_a.set(Value::from(2), SetOptions::default()).await.is_err());

        let positive = validated(
            inner.clone(),
            Arc::new(|v: &Value| match v {
                Value::Object(map) if map.values().all(|x| x.try_into_i64().map(|n| n > 0).unwrap_or(false)) => Ok(()),
                _ => Err(Error::general_error("values must be positive".into())),
            }),
        );
        let err = positive.set(object(&[("a", -1)]), SetOptions::default()).await.unwrap_err();
        assert_eq!(err.message, "values must be positive");
        let a = positive.project(Arc::new(Field("a")), Query::new()).unwrap();
        assert!(a.set(Value::from(-5), SetOptions::default()).await.is_err());
        a.set(Value::from(5), SetOptions::default()).await.unwrap();
        assert_eq!(value_of(&inner.get().await.unwrap()), object(&[("a", 5)]));
    }

    /// A root that reports events through `poll_event` only.
    #[derive(Debug)]
    struct Pushed {
        value: Mutex<Option<(State<Value>, Version)>>,
        events: Mutex<Vec<RootEvent>>,
    }
    #[async_trait::async_trait]
    impl AccessorRoot<Value> for Pushed {
        async fn read(&self) -> Result<Option<(State<Value>, Version)>, Error> {
            Ok(self.value.lock().unwrap().clone())
        }
        async fn write(&self, state: State<Value>) -> Result<(State<Value>, Version), Error> {
            let version = Version::new_unique();
            *self.value.lock().unwrap() = Some((state.clone(), version));
            Ok((state, version))
        }
        async fn info(&self) -> Result<AccessorInfo, Error> {
            Ok(AccessorInfo::with_status(Status::Ready))
        }
        fn capabilities(&self) -> Capabilities {
            Capabilities { write: true, notify: true }
        }
        async fn poll_event(&self) -> Option<RootEvent> {
            let mut events = self.events.lock().unwrap();
            if events.is_empty() { None } else { Some(events.remove(0)) }
        }
    }

    #[tokio::test]
    async fn custom_root_gets_projections_conflicts_and_events() {
        let root = Arc::new(Pushed {
            value: Mutex::new(Some((state(object(&[("a", 1)])), Version::new_unique()))),
            events: Mutex::new(Vec::new()),
        });
        let acc = AccessorHub::new(root.clone(), Query::new(), ExpiryPolicy::Relay);
        let a = acc.project(Arc::new(Field("a")), Query::new()).unwrap();
        let fp = a.get().await.unwrap().fingerprint;
        // Another writer changes the source and reports it.
        let version = Version::new_unique();
        *root.value.lock().unwrap() = Some((state(object(&[("a", 2)])), version));
        root.events.lock().unwrap().push(RootEvent::Status(Status::Processing));
        root.events.lock().unwrap().push(RootEvent::Changed(version));
        let mut rx = a.subscribe();
        a.refresh().await.unwrap();
        assert!(matches!(take(&mut rx), Some(AccessorEvent::Changed(_))));
        let err = a.set_if(Value::from(3), Some(fp), SetOptions::default()).await.unwrap_err();
        assert_eq!(err.error_type, ErrorType::StatusConflict);
        assert_eq!(value_of(&a.get().await.unwrap()), Value::from(2));
    }

    #[tokio::test]
    async fn hub_error_cases() {
        // An empty memory get.
        let empty = memory_accessor::<Value>(None, Query::new());
        assert_eq!(empty.get().await.unwrap_err().error_type, ErrorType::NotAvailable);
        // A projection set on an absent root.
        let part = empty.project(Arc::new(Field("a")), Query::new()).unwrap();
        assert_eq!(
            part.set(Value::from(1), SetOptions::default()).await.unwrap_err().error_type,
            ErrorType::NotAvailable
        );
        // A create on an existing value conflicts; on an absent one it creates.
        empty.set_if(Value::from(1), None, SetOptions::default()).await.unwrap();
        let err = empty.set_if(Value::from(2), None, SetOptions::default()).await.unwrap_err();
        assert_eq!(err.error_type, ErrorType::StatusConflict);
        // A parent the step cannot read: the part does not exist.
        let text = memory_accessor(Some(state(Value::from("text"))), Query::new());
        let field = text.project(Arc::new(Field("a")), Query::new()).unwrap();
        assert!(field.get().await.is_err());
    }
}
