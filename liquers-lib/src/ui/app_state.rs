use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;

use serde::{Deserialize, Serialize};

use liquers_core::error::Error;
use liquers_core::state::State;
use liquers_core::value::ValueInterface;

use crate::value::{ExtValueInterface, Value};

use super::element::{ElementSource, StateViewElement, UIElement};
use super::handle::UIHandle;
use super::resolve::{insertion_point_to_add_args, InsertionPoint};

// ─── NodeData ───────────────────────────────────────────────────────────────

/// Per-node data stored in AppState. Holds the element, source, and topology.
/// Internal to DirectAppState — not part of the AppState trait interface.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub(crate) struct NodeData {
    /// Parent handle, or None for root nodes.
    pub parent: Option<UIHandle>,
    /// Ordered children.
    pub children: Vec<UIHandle>,
    /// How this element was generated.
    pub source: ElementSource,
    /// The element itself. None = pending evaluation.
    pub element: Option<Box<dyn UIElement>>,
}

impl NodeData {
    pub fn new(source: ElementSource, parent: Option<UIHandle>) -> Self {
        Self {
            parent,
            children: Vec::new(),
            source,
            element: None,
        }
    }

    pub fn with_element(mut self, element: Box<dyn UIElement>) -> Self {
        self.element = Some(element);
        self
    }
}

// ─── Invalidation ───────────────────────────────────────────────────────────

/// Maximum number of individual changes kept before `Invalidation` escalates to `All`.
///
/// The realistic trigger is the `AppState` lock being held across several render ticks while a
/// burst of mutations lands. Past this point a full re-render is both cheaper to apply and
/// constant-size, so the log is bounded rather than unbounded.
const MAX_CHANGES: usize = 64;

/// One recorded mutation of the UI tree.
///
/// Backend-neutral: a retained-mode backend (web) maps these to DOM operations, while an
/// immediate-mode backend (egui) ignores the detail and simply repaints. Records are produced by
/// `AppState`'s own mutating methods — see the mutation contract on [`AppState`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum UIChange {
    /// `handle` was added under `parent` (`None` = a root) at child index `index`.
    Inserted {
        parent: Option<UIHandle>,
        handle: UIHandle,
        index: usize,
    },
    /// `handle`, with its subtree, was removed from `parent` (`None` = a root).
    Removed {
        parent: Option<UIHandle>,
        handle: UIHandle,
    },
    /// `handle`'s own rendered markup is out of date; its position in the tree did not change.
    Replaced { handle: UIHandle },
}

/// What has changed in the model since a renderer last looked.
///
/// Three states, matching the three things a renderer can do: nothing, apply these changes, or
/// re-render everything. Accumulation is an absorbing state machine — once `All` is reached,
/// further changes are redundant and ignored.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub enum Invalidation {
    /// Nothing changed — the rendered output is still current.
    #[default]
    None,
    /// Apply these changes, in order. Never empty: the empty case is `None`.
    Changes(Vec<UIChange>),
    /// Re-render everything. Used when a change cannot be attributed to individual elements: the
    /// state was deserialized, nothing has been rendered yet, the change log overflowed, or the
    /// `AppState` implementation does not track changes at all.
    All,
}

impl Invalidation {
    /// Append one change. `None` becomes `Changes`; `Changes` grows, escalating to `All` past
    /// [`MAX_CHANGES`]; `All` absorbs (the change is already covered).
    pub fn record(&mut self, change: UIChange) {
        match self {
            Invalidation::None => *self = Invalidation::Changes(vec![change]),
            Invalidation::Changes(changes) => {
                if changes.len() >= MAX_CHANGES {
                    *self = Invalidation::All;
                } else {
                    changes.push(change);
                }
            }
            Invalidation::All => {}
        }
    }

    /// Escalate to `All` from any state.
    pub fn set_all(&mut self) {
        *self = Invalidation::All;
    }

    /// True only for `Invalidation::None`.
    pub fn is_empty(&self) -> bool {
        match self {
            Invalidation::None => true,
            Invalidation::Changes(_) | Invalidation::All => false,
        }
    }

    /// Return the accumulated value and reset to `None`.
    pub fn take(&mut self) -> Invalidation {
        std::mem::take(self)
    }
}

// ─── AppState Trait ─────────────────────────────────────────────────────────

/// Trait for the UI application state. Manages a tree of UI elements.
///
/// All methods are synchronous — AppState holds in-memory data only.
/// When used from async contexts, wrap in `Arc<std::sync::Mutex<dyn AppState>>`.
///
/// # Mutation contract
///
/// `AppState` must only be mutated through its own methods — including by application-defined
/// commands, which reach it as a trait object and are responsible for doing so correctly. That
/// invariant is what makes change recording complete: the mutating methods record a [`UIChange`],
/// and a renderer applies the accumulated [`Invalidation`].
///
/// Two clauses the compiler cannot enforce:
///
/// 1. [`AppState::put_element`] returns an element borrowed for rendering **unchanged** and records
///    nothing; use [`AppState::set_element`] when the element actually changed.
/// 2. A change to an element's *content* is reported by whoever made it — from `update` via the
///    returned `UpdateResponse::NeedsRepaint`, and from anywhere else via
///    [`AppState::record_change`].
pub trait AppState: Send + Sync + std::fmt::Debug {
    // ── Node Creation ────────────────────────────────────────────────────

    /// Add a new node with an auto-generated handle.
    /// If parent is Some, the node is appended to the parent's children.
    /// The `position` parameter specifies the insertion index within parent's children
    /// (0 = first child). If >= children.len(), appends at end.
    fn add_node(
        &mut self,
        parent: Option<UIHandle>,
        position: usize,
        source: ElementSource,
    ) -> Result<UIHandle, Error>;

    /// Insert a node at a specific handle. Errors if handle is already in use.
    fn insert_node(
        &mut self,
        handle: UIHandle,
        parent: Option<UIHandle>,
        position: usize,
        source: ElementSource,
    ) -> Result<(), Error>;

    // ── Element Access ───────────────────────────────────────────────────

    /// Get a reference to the element at this handle.
    fn get_element(&self, handle: UIHandle) -> Result<Option<&dyn UIElement>, Error>;

    /// Get a mutable reference to the element at this handle.
    ///
    /// Records `UIChange::Replaced` eagerly: `&mut` means "I intend to mutate", and there is no
    /// way to learn afterwards whether the caller did. Callers that only read should use
    /// [`AppState::get_element`].
    fn get_element_mut(
        &mut self,
        handle: UIHandle,
    ) -> Result<Option<&mut Box<dyn UIElement>>, Error>;

    /// Install a new or changed element on a node, recording `UIChange::Replaced`.
    ///
    /// Does NOT call init() — the caller (typically AppRunner) is responsible for calling init()
    /// with a UIContext. Use this, not [`AppState::put_element`], whenever the element differs
    /// from what was there before.
    fn set_element(&mut self, handle: UIHandle, element: Box<dyn UIElement>) -> Result<(), Error>;

    /// Temporarily remove the element from a node (for extract-render-replace).
    ///
    /// Records nothing: this is the borrow half of the render path.
    fn take_element(&mut self, handle: UIHandle) -> Result<Box<dyn UIElement>, Error>;

    /// Return an element borrowed with [`AppState::take_element`], **unchanged**.
    ///
    /// Records nothing, because immediate-mode rendering borrows and returns every element on
    /// every frame; recording here would mark the whole tree changed continuously. A caller that
    /// *did* change the element must use [`AppState::set_element`] instead, or record
    /// `UIChange::Replaced` itself.
    fn put_element(&mut self, handle: UIHandle, element: Box<dyn UIElement>) -> Result<(), Error>;

    // ── Node Data Access ─────────────────────────────────────────────────

    /// Check whether a node with the given handle exists.
    fn node_exists(&self, handle: UIHandle) -> bool {
        self.get_source(handle).is_ok()
    }

    /// Get the ElementSource for a node.
    fn get_source(&self, handle: UIHandle) -> Result<&ElementSource, Error>;

    // ── Tree Manipulation ────────────────────────────────────────────────

    /// Remove a node and its subtree recursively.
    fn remove(&mut self, handle: UIHandle) -> Result<(), Error>;

    /// Remove all children of a node (and their subtrees), keeping the node itself.
    fn remove_children(&mut self, handle: UIHandle) -> Result<(), Error> {
        let children = self.children(handle)?;
        for child in children {
            self.remove(child)?;
        }
        Ok(())
    }

    // ── InsertionPoint-Based Insertion ───────────────────────────────────

    /// Insert a node with the given source at the specified insertion point.
    /// Creates a node with element=None (pending evaluation).
    /// For `Instead`: replaces the source, clears the element.
    fn insert_source(
        &mut self,
        point: &InsertionPoint,
        source: ElementSource,
    ) -> Result<UIHandle, Error> {
        match point {
            InsertionPoint::Instead(target) => {
                let handle = *target;
                self.remove_children(handle)?;
                self.set_source(handle, source)?;
                // Clear element to make it pending
                if self.get_element(handle)?.is_some() {
                    let _ = self.take_element(handle);
                }
                Ok(handle)
            }
            other => {
                let (parent, pos) = insertion_point_to_add_args(self, other)?;
                self.add_node(parent, pos, source)
            }
        }
    }

    /// Insert a pre-built element at the specified insertion point.
    /// Creates a node with ElementSource::None and sets the element.
    /// For `Instead`: replaces the element in place.
    fn insert_element(
        &mut self,
        point: &InsertionPoint,
        element: Box<dyn UIElement>,
    ) -> Result<UIHandle, Error> {
        eprintln!(
            "Inserting element at {:?} with title {:?}",
            point,
            element.title()
        );
        match point {
            InsertionPoint::Instead(target) => {
                let handle = *target;
                self.remove_children(handle)?;
                self.set_element(handle, element)?;
                Ok(handle)
            }
            other => {
                let (parent, pos) = insertion_point_to_add_args(self, other)?;
                let handle = self.add_node(parent, pos, ElementSource::None)?;
                self.set_element(handle, element)?;
                Ok(handle)
            }
        }
    }

    /// Insert an element derived from a State<Value>.
    ///
    /// If the value is `ExtValue::UIElement`, extract and use `insert_element`.
    /// Otherwise wrap in `StateViewElement`.
    /// Source is `ElementSource::Query` from state metadata if available,
    /// else `ElementSource::None`.
    fn insert_state(
        &mut self,
        point: &InsertionPoint,
        state: &State<Value>,
    ) -> Result<UIHandle, Error> {
        let value = state.data_unchecked().as_ref();

        // Extract source from metadata query
        let source = match state.metadata.query() {
            Ok(q) => {
                let encoded = q.encode();
                if encoded.is_empty() {
                    ElementSource::None
                } else {
                    ElementSource::Query(encoded)
                }
            }
            Err(_) => ElementSource::None,
        };

        // Check if value is a UIElement
        if let Ok(ui_elem) = value.as_ui_element() {
            let element = ui_elem.clone_boxed();
            let handle = self.insert_element(point, element)?;
            // Also set the source if available
            if !matches!(source, ElementSource::None) {
                let _ = self.set_source(handle, source);
            }
            return Ok(handle);
        }

        // Check if value is specifically a Query variant → insert as pending source
        // for lazy evaluation. Only match the Query variant (identifier "query"),
        // not text strings that happen to be parseable as queries.
        if value.identifier() == "query" {
            if let Ok(query) = value.try_into_query() {
                let query_source = ElementSource::Query(query.encode());
                return self.insert_source(point, query_source);
            }
        }

        // Wrap in StateViewElement
        let element = Box::new(StateViewElement::from_state(state));
        let handle = match point {
            InsertionPoint::Instead(target) => {
                let h = *target;
                self.remove_children(h)?;
                self.set_element(h, element)?;
                if !matches!(source, ElementSource::None) {
                    let _ = self.set_source(h, source);
                }
                h
            }
            other => {
                let (parent, pos) = insertion_point_to_add_args(self, other)?;
                let h = self.add_node(parent, pos, source)?;
                self.set_element(h, element)?;
                h
            }
        };

        Ok(handle)
    }

    /// Set/replace the ElementSource for a node.
    fn set_source(&mut self, handle: UIHandle, source: ElementSource) -> Result<(), Error>;

    // ── Navigation ───────────────────────────────────────────────────────

    /// All root handles (nodes with no parent), sorted by handle value.
    fn roots(&self) -> Vec<UIHandle>;

    /// Parent of the given element, or None if it is a root.
    fn parent(&self, handle: UIHandle) -> Result<Option<UIHandle>, Error>;

    /// Ordered children of the given element.
    fn children(&self, handle: UIHandle) -> Result<Vec<UIHandle>, Error>;

    /// Sibling at a relative offset among the parent's children list.
    fn sibling(&self, handle: UIHandle, offset: i32) -> Result<Option<UIHandle>, Error>;

    /// Previous sibling (offset -1).
    fn previous_sibling(&self, handle: UIHandle) -> Result<Option<UIHandle>, Error> {
        self.sibling(handle, -1)
    }

    /// Next sibling (offset +1).
    fn next_sibling(&self, handle: UIHandle) -> Result<Option<UIHandle>, Error> {
        self.sibling(handle, 1)
    }

    /// First child of the given element.
    fn first_child(&self, handle: UIHandle) -> Result<Option<UIHandle>, Error> {
        let ch = self.children(handle)?;
        Ok(ch.first().copied())
    }

    /// Last child of the given element.
    fn last_child(&self, handle: UIHandle) -> Result<Option<UIHandle>, Error> {
        let ch = self.children(handle)?;
        Ok(ch.last().copied())
    }

    // ── Active Element ───────────────────────────────────────────────────

    /// Get the active (focused) element handle.
    fn active_handle(&self) -> Option<UIHandle>;

    /// Set the active element.
    fn set_active_handle(&mut self, handle: Option<UIHandle>);

    // ── Pending Nodes ────────────────────────────────────────────────────

    /// Handles of nodes where element is None (pending evaluation).
    fn pending_nodes(&self) -> Vec<UIHandle>;

    // ── Node Count ───────────────────────────────────────────────────────

    /// Total number of nodes.
    fn node_count(&self) -> usize;

    // ── Invalidation ─────────────────────────────────────────────────────

    /// Record one structural change for the renderer.
    ///
    /// Called by this trait's own mutating methods; also available to a command that mutates an
    /// element's content directly, which must report `UIChange::Replaced` itself.
    ///
    /// Default: escalates to [`AppState::invalidate_all`] — correct, just coarser.
    fn record_change(&mut self, change: UIChange) {
        let _ = change;
        self.invalidate_all();
    }

    /// Record that the whole tree must be re-rendered.
    ///
    /// Default: no-op, paired with the [`AppState::take_invalidation`] default below.
    fn invalidate_all(&mut self) {}

    /// Take and clear the pending invalidation.
    ///
    /// **Exactly one renderer per application may call this** — two consumers would each see only
    /// part of the history.
    ///
    /// Default: `Invalidation::All`. An implementation that does not track changes thereby tells
    /// every renderer to re-render everything, which is the behaviour before invalidation existed:
    /// coarse, but never stale.
    fn take_invalidation(&mut self) -> Invalidation {
        Invalidation::All
    }
}

// ─── DirectAppState ─────────────────────────────────────────────────────────

/// In-memory implementation of AppState. Uses a HashMap for O(1) lookups.
///
/// Share via `Arc<std::sync::Mutex<DirectAppState>>`.
#[derive(Debug)]
pub struct DirectAppState {
    nodes: HashMap<UIHandle, NodeData>,
    next_id: AtomicU64,
    active_handle: Option<UIHandle>,
    /// Transient render bookkeeping — never serialized. Starts at `All` so a renderer attaching to
    /// a fresh or restored state always paints once, with no "first frame" special case.
    invalidation: Invalidation,
}

impl DirectAppState {
    pub fn new() -> Self {
        Self {
            nodes: HashMap::new(),
            next_id: AtomicU64::new(1),
            active_handle: None,
            invalidation: Invalidation::All,
        }
    }

    fn generate_handle(&self) -> UIHandle {
        UIHandle(self.next_id.fetch_add(1, Ordering::SeqCst))
    }

    /// Private helper: get a reference to the full NodeData.
    fn get_node(&self, handle: UIHandle) -> Result<&NodeData, Error> {
        self.nodes
            .get(&handle)
            .ok_or_else(|| Error::general_error(format!("Node not found: {:?}", handle)))
    }

    /// Remove a subtree recursively (helper).
    fn remove_subtree(&mut self, handle: UIHandle) -> Vec<UIHandle> {
        let mut removed = vec![handle];
        if let Some(node) = self.nodes.remove(&handle) {
            for child in node.children {
                removed.extend(self.remove_subtree(child));
            }
        }
        removed
    }
}

impl Default for DirectAppState {
    fn default() -> Self {
        Self::new()
    }
}

impl AppState for DirectAppState {
    fn add_node(
        &mut self,
        parent: Option<UIHandle>,
        position: usize,
        source: ElementSource,
    ) -> Result<UIHandle, Error> {
        // Validate parent exists
        if let Some(parent_handle) = parent {
            if !self.nodes.contains_key(&parent_handle) {
                return Err(Error::general_error(format!(
                    "Parent not found: {:?}",
                    parent_handle
                )));
            }
        }

        let handle = self.generate_handle();
        let node = NodeData::new(source, parent);
        self.nodes.insert(handle, node);

        // Link to parent
        let mut index = position;
        if let Some(parent_handle) = parent {
            if let Some(parent_node) = self.nodes.get_mut(&parent_handle) {
                let insert_pos = position.min(parent_node.children.len());
                parent_node.children.insert(insert_pos, handle);
                index = insert_pos;
            }
        } else {
            // Roots are rendered in ascending handle order, which is insertion order.
            index = self.roots().iter().position(|h| *h == handle).unwrap_or(0);
        }

        self.record_change(UIChange::Inserted {
            parent,
            handle,
            index,
        });
        Ok(handle)
    }

    fn insert_node(
        &mut self,
        handle: UIHandle,
        parent: Option<UIHandle>,
        position: usize,
        source: ElementSource,
    ) -> Result<(), Error> {
        if self.nodes.contains_key(&handle) {
            return Err(Error::general_error(format!(
                "Handle already in use: {:?}",
                handle
            )));
        }

        // Validate parent exists
        if let Some(parent_handle) = parent {
            if !self.nodes.contains_key(&parent_handle) {
                return Err(Error::general_error(format!(
                    "Parent not found: {:?}",
                    parent_handle
                )));
            }
        }

        // Keep auto-generated handles above any manually inserted one.
        let current = self.next_id.load(Ordering::SeqCst);
        if handle.0 >= current {
            self.next_id.store(handle.0 + 1, Ordering::SeqCst);
        }

        let node = NodeData::new(source, parent);
        self.nodes.insert(handle, node);

        // Link to parent
        let mut index = position;
        if let Some(parent_handle) = parent {
            if let Some(parent_node) = self.nodes.get_mut(&parent_handle) {
                let insert_pos = position.min(parent_node.children.len());
                parent_node.children.insert(insert_pos, handle);
                index = insert_pos;
            }
        } else {
            index = self.roots().iter().position(|h| *h == handle).unwrap_or(0);
        }

        self.record_change(UIChange::Inserted {
            parent,
            handle,
            index,
        });
        Ok(())
    }

    fn get_element(&self, handle: UIHandle) -> Result<Option<&dyn UIElement>, Error> {
        let node = self
            .nodes
            .get(&handle)
            .ok_or_else(|| Error::general_error(format!("Node not found: {:?}", handle)))?;
        Ok(node.element.as_ref().map(|e| &**e))
    }

    fn get_element_mut(
        &mut self,
        handle: UIHandle,
    ) -> Result<Option<&mut Box<dyn UIElement>>, Error> {
        // Record before handing out `&mut`: there is no way to learn afterwards whether the
        // caller mutated, so `&mut` is taken at its word.
        if self.nodes.contains_key(&handle) {
            self.record_change(UIChange::Replaced { handle });
        }
        let node = self
            .nodes
            .get_mut(&handle)
            .ok_or_else(|| Error::general_error(format!("Node not found: {:?}", handle)))?;
        Ok(node.element.as_mut())
    }

    fn set_element(&mut self, handle: UIHandle, element: Box<dyn UIElement>) -> Result<(), Error> {
        let node = self
            .nodes
            .get_mut(&handle)
            .ok_or_else(|| Error::general_error(format!("Node not found: {:?}", handle)))?;
        node.element = Some(element);
        self.record_change(UIChange::Replaced { handle });
        Ok(())
    }

    fn take_element(&mut self, handle: UIHandle) -> Result<Box<dyn UIElement>, Error> {
        let node = self
            .nodes
            .get_mut(&handle)
            .ok_or_else(|| Error::general_error(format!("Node not found: {:?}", handle)))?;
        node.element
            .take()
            .ok_or_else(|| Error::general_error(format!("Element not present at {:?}", handle)))
    }

    fn put_element(&mut self, handle: UIHandle, element: Box<dyn UIElement>) -> Result<(), Error> {
        let node = self
            .nodes
            .get_mut(&handle)
            .ok_or_else(|| Error::general_error(format!("Node not found: {:?}", handle)))?;
        node.element = Some(element);
        Ok(())
    }

    fn node_exists(&self, handle: UIHandle) -> bool {
        self.nodes.contains_key(&handle)
    }

    fn get_source(&self, handle: UIHandle) -> Result<&ElementSource, Error> {
        Ok(&self.get_node(handle)?.source)
    }

    fn set_source(&mut self, handle: UIHandle, source: ElementSource) -> Result<(), Error> {
        let node = self
            .nodes
            .get_mut(&handle)
            .ok_or_else(|| Error::general_error(format!("Node not found: {:?}", handle)))?;
        node.source = source;
        // A pending node renders a placeholder, so its source is renderable state.
        self.record_change(UIChange::Replaced { handle });
        Ok(())
    }

    fn remove(&mut self, handle: UIHandle) -> Result<(), Error> {
        let node = self
            .nodes
            .get(&handle)
            .ok_or_else(|| Error::general_error(format!("Node not found: {:?}", handle)))?;

        // Unlink from parent
        let parent = node.parent;
        let children_to_remove: Vec<UIHandle> = node.children.clone();

        // Remove from parent's children list
        if let Some(parent_handle) = parent {
            if let Some(parent_node) = self.nodes.get_mut(&parent_handle) {
                parent_node.children.retain(|h| *h != handle);
            }
        }

        // Remove the node itself
        self.nodes
            .remove(&handle)
            .ok_or_else(|| Error::general_error(format!("Node not found: {:?}", handle)))?;

        // Recursively remove children
        for child in &children_to_remove {
            self.remove_subtree(*child);
        }

        // Clear active if it was removed
        if self.active_handle == Some(handle) {
            self.active_handle = None;
        }

        // One record for the subtree root; its descendants go with it.
        self.record_change(UIChange::Removed { parent, handle });
        Ok(())
    }

    fn roots(&self) -> Vec<UIHandle> {
        let mut roots: Vec<UIHandle> = self
            .nodes
            .iter()
            .filter(|(_, node)| node.parent.is_none())
            .map(|(handle, _)| *handle)
            .collect();
        roots.sort_unstable_by_key(|h| h.0);
        roots
    }

    fn parent(&self, handle: UIHandle) -> Result<Option<UIHandle>, Error> {
        Ok(self.get_node(handle)?.parent)
    }

    fn children(&self, handle: UIHandle) -> Result<Vec<UIHandle>, Error> {
        Ok(self.get_node(handle)?.children.clone())
    }

    fn sibling(&self, handle: UIHandle, offset: i32) -> Result<Option<UIHandle>, Error> {
        let node = self.get_node(handle)?;
        let parent_handle = match node.parent {
            Some(h) => h,
            None => return Ok(None),
        };

        let parent = self.get_node(parent_handle)?;
        let current_index = parent
            .children
            .iter()
            .position(|h| *h == handle)
            .ok_or_else(|| {
                Error::general_error(format!(
                    "Element {:?} not in parent's children list",
                    handle
                ))
            })?;

        let target = current_index as i64 + offset as i64;
        if target < 0 || target >= parent.children.len() as i64 {
            Ok(None)
        } else {
            Ok(Some(parent.children[target as usize]))
        }
    }

    fn active_handle(&self) -> Option<UIHandle> {
        self.active_handle
    }

    fn set_active_handle(&mut self, handle: Option<UIHandle>) {
        let previous = self.active_handle;
        if previous == handle {
            return;
        }
        self.active_handle = handle;
        // Active state is renderable, so both the old and the new element are out of date.
        for changed in [previous, handle].into_iter().flatten() {
            if self.nodes.contains_key(&changed) {
                self.record_change(UIChange::Replaced { handle: changed });
            }
        }
    }

    fn pending_nodes(&self) -> Vec<UIHandle> {
        let mut pending: Vec<UIHandle> = self
            .nodes
            .iter()
            .filter(|(_, node)| node.element.is_none())
            .map(|(handle, _)| *handle)
            .collect();
        pending.sort_unstable_by_key(|h| h.0);
        pending
    }

    fn node_count(&self) -> usize {
        self.nodes.len()
    }

    fn record_change(&mut self, change: UIChange) {
        self.invalidation.record(change);
    }

    fn invalidate_all(&mut self) {
        self.invalidation.set_all();
    }

    fn take_invalidation(&mut self) -> Invalidation {
        self.invalidation.take()
    }
}

// ─── Serialization Support ──────────────────────────────────────────────────

/// Serializable snapshot of DirectAppState.
#[derive(Serialize, Deserialize, Debug)]
struct DirectAppStateSnapshot {
    nodes: HashMap<UIHandle, NodeData>,
    next_id: u64,
    active_handle: Option<UIHandle>,
}

impl Serialize for DirectAppState {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        let snapshot = DirectAppStateSnapshot {
            nodes: self.nodes.clone(),
            next_id: self.next_id.load(Ordering::SeqCst),
            active_handle: self.active_handle,
        };
        snapshot.serialize(serializer)
    }
}

impl<'de> Deserialize<'de> for DirectAppState {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let snapshot = DirectAppStateSnapshot::deserialize(deserializer)?;
        Ok(DirectAppState {
            nodes: snapshot.nodes,
            next_id: AtomicU64::new(snapshot.next_id),
            active_handle: snapshot.active_handle,
            // A restored state has never been rendered by the attached renderer, and there are no
            // change records describing how it got here — so everything is out of date.
            invalidation: Invalidation::All,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ui::element::Placeholder;

    // ── CRUD ──────────────────────────────────────────────────────────────

    #[test]
    fn test_add_node_root() {
        let mut s = DirectAppState::new();
        let h = s.add_node(None, 0, ElementSource::None).unwrap();
        assert_eq!(s.node_count(), 1);
        assert_eq!(s.roots(), vec![h]);
    }

    #[test]
    fn test_add_node_with_parent() {
        let mut s = DirectAppState::new();
        let p = s.add_node(None, 0, ElementSource::None).unwrap();
        let c = s
            .add_node(Some(p), 0, ElementSource::Query("/-/hello".into()))
            .unwrap();

        assert_eq!(s.children(p).unwrap(), vec![c]);
        assert_eq!(s.parent(c).unwrap(), Some(p));
        assert_eq!(s.roots(), vec![p]);
    }

    #[test]
    fn test_add_node_parent_not_found() {
        let mut s = DirectAppState::new();
        assert!(s
            .add_node(Some(UIHandle(99)), 0, ElementSource::None)
            .is_err());
    }

    #[test]
    fn test_add_node_position_ordering() {
        let mut s = DirectAppState::new();
        let p = s.add_node(None, 0, ElementSource::None).unwrap();
        let c1 = s.add_node(Some(p), 0, ElementSource::None).unwrap();
        let c2 = s.add_node(Some(p), 0, ElementSource::None).unwrap(); // insert at front
        let c3 = s.add_node(Some(p), 1, ElementSource::None).unwrap(); // insert at index 1

        assert_eq!(s.children(p).unwrap(), vec![c2, c3, c1]);
    }

    #[test]
    fn test_insert_node() {
        let mut s = DirectAppState::new();
        let h = UIHandle(42);
        s.insert_node(h, None, 0, ElementSource::None).unwrap();
        assert!(s.node_exists(h));
        assert_eq!(s.parent(h).unwrap(), None);

        // next auto-generated handle must be > 42
        let h2 = s.add_node(None, 0, ElementSource::None).unwrap();
        assert!(h2.0 > 42);
    }

    #[test]
    fn test_insert_duplicate_errors() {
        let mut s = DirectAppState::new();
        let h = UIHandle(10);
        s.insert_node(h, None, 0, ElementSource::None).unwrap();
        assert!(s.insert_node(h, None, 0, ElementSource::None).is_err());
    }

    // ── Element Access ────────────────────────────────────────────────────

    #[test]
    fn test_set_element_stores_without_init() {
        let mut s = DirectAppState::new();
        let h = s.add_node(None, 0, ElementSource::None).unwrap();

        let placeholder = Box::new(Placeholder::new().with_title("Test".to_string()));
        s.set_element(h, placeholder).unwrap();

        let elem = s.get_element(h).unwrap().unwrap();
        // set_element no longer calls init — handle remains None
        assert_eq!(elem.handle(), None);
        assert_eq!(elem.title(), "Test");
    }

    #[test]
    fn test_take_and_put_element() {
        let mut s = DirectAppState::new();
        let h = s.add_node(None, 0, ElementSource::None).unwrap();
        s.set_element(h, Box::new(Placeholder::new())).unwrap();

        let elem = s.take_element(h).unwrap();
        assert_eq!(elem.type_name(), "Placeholder");

        // Element is gone from node
        assert!(s.get_element(h).unwrap().is_none());

        // Put it back
        s.put_element(h, elem).unwrap();
        assert!(s.get_element(h).unwrap().is_some());
    }

    #[test]
    fn test_take_element_missing() {
        let mut s = DirectAppState::new();
        let h = s.add_node(None, 0, ElementSource::None).unwrap();
        // No element set — take should fail
        assert!(s.take_element(h).is_err());
    }

    #[test]
    fn test_pending_nodes() {
        let mut s = DirectAppState::new();
        let h1 = s.add_node(None, 0, ElementSource::None).unwrap();
        let h2 = s
            .add_node(None, 0, ElementSource::Query("/-/test".into()))
            .unwrap();

        assert_eq!(s.pending_nodes(), vec![h1, h2]);

        s.set_element(h1, Box::new(Placeholder::new())).unwrap();
        assert_eq!(s.pending_nodes(), vec![h2]);
    }

    // ── Navigation ────────────────────────────────────────────────────────

    #[test]
    fn test_root_sorted() {
        let mut s = DirectAppState::new();
        s.insert_node(UIHandle(3), None, 0, ElementSource::None)
            .unwrap();
        s.insert_node(UIHandle(1), None, 0, ElementSource::None)
            .unwrap();
        s.insert_node(UIHandle(2), None, 0, ElementSource::None)
            .unwrap();
        assert_eq!(s.roots(), vec![UIHandle(1), UIHandle(2), UIHandle(3)]);
    }

    #[test]
    fn test_root_excludes_children() {
        let mut s = DirectAppState::new();
        let p = s.add_node(None, 0, ElementSource::None).unwrap();
        let _c = s.add_node(Some(p), 0, ElementSource::None).unwrap();
        assert_eq!(s.roots(), vec![p]);
    }

    #[test]
    fn test_sibling_navigation() {
        let mut s = DirectAppState::new();
        let p = s.add_node(None, 0, ElementSource::None).unwrap();
        let c0 = s.add_node(Some(p), 100, ElementSource::None).unwrap();
        let c1 = s.add_node(Some(p), 100, ElementSource::None).unwrap();
        let c2 = s.add_node(Some(p), 100, ElementSource::None).unwrap();

        assert_eq!(s.previous_sibling(c0).unwrap(), None);
        assert_eq!(s.previous_sibling(c1).unwrap(), Some(c0));
        assert_eq!(s.previous_sibling(c2).unwrap(), Some(c1));

        assert_eq!(s.next_sibling(c0).unwrap(), Some(c1));
        assert_eq!(s.next_sibling(c1).unwrap(), Some(c2));
        assert_eq!(s.next_sibling(c2).unwrap(), None);

        // offset 0 returns self
        assert_eq!(s.sibling(c1, 0).unwrap(), Some(c1));
    }

    #[test]
    fn test_root_has_no_siblings() {
        let mut s = DirectAppState::new();
        let r = s.add_node(None, 0, ElementSource::None).unwrap();
        assert_eq!(s.previous_sibling(r).unwrap(), None);
        assert_eq!(s.next_sibling(r).unwrap(), None);
    }

    #[test]
    fn test_first_last_child() {
        let mut s = DirectAppState::new();
        let p = s.add_node(None, 0, ElementSource::None).unwrap();
        let c0 = s.add_node(Some(p), 100, ElementSource::None).unwrap();
        let _c1 = s.add_node(Some(p), 100, ElementSource::None).unwrap();
        let c2 = s.add_node(Some(p), 100, ElementSource::None).unwrap();

        assert_eq!(s.first_child(p).unwrap(), Some(c0));
        assert_eq!(s.last_child(p).unwrap(), Some(c2));
    }

    #[test]
    fn test_first_last_child_empty() {
        let mut s = DirectAppState::new();
        let p = s.add_node(None, 0, ElementSource::None).unwrap();
        assert_eq!(s.first_child(p).unwrap(), None);
        assert_eq!(s.last_child(p).unwrap(), None);
    }

    // ── Remove ────────────────────────────────────────────────────────────

    #[test]
    fn test_remove_cleans_parent() {
        let mut s = DirectAppState::new();
        let p = s.add_node(None, 0, ElementSource::None).unwrap();
        let c = s.add_node(Some(p), 0, ElementSource::None).unwrap();

        s.remove(c).unwrap();
        assert!(s.children(p).unwrap().is_empty());
        assert!(!s.node_exists(c));
    }

    #[test]
    fn test_remove_recursive() {
        let mut s = DirectAppState::new();
        let p = s.add_node(None, 0, ElementSource::None).unwrap();
        let c1 = s.add_node(Some(p), 0, ElementSource::None).unwrap();
        let c2 = s.add_node(Some(c1), 0, ElementSource::None).unwrap();
        let c3 = s.add_node(Some(c2), 0, ElementSource::None).unwrap();

        s.remove(c1).unwrap();

        assert!(!s.node_exists(c1));
        assert!(!s.node_exists(c2));
        assert!(!s.node_exists(c3));
        assert!(s.children(p).unwrap().is_empty());
    }

    #[test]
    fn test_remove_not_found() {
        let mut s = DirectAppState::new();
        assert!(s.remove(UIHandle(99)).is_err());
    }

    #[test]
    fn test_remove_clears_active() {
        let mut s = DirectAppState::new();
        let h = s.add_node(None, 0, ElementSource::None).unwrap();
        s.set_active_handle(Some(h));
        assert_eq!(s.active_handle(), Some(h));

        s.remove(h).unwrap();
        assert_eq!(s.active_handle(), None);
    }

    // ── Active Element ────────────────────────────────────────────────────

    #[test]
    fn test_active_handle() {
        let mut s = DirectAppState::new();
        assert_eq!(s.active_handle(), None);

        let h = s.add_node(None, 0, ElementSource::None).unwrap();
        s.set_active_handle(Some(h));
        assert_eq!(s.active_handle(), Some(h));

        s.set_active_handle(None);
        assert_eq!(s.active_handle(), None);
    }

    // ── Serialization ─────────────────────────────────────────────────────

    #[test]
    fn test_serialization_roundtrip_topology() {
        let mut s = DirectAppState::new();
        let p = s.add_node(None, 0, ElementSource::None).unwrap();
        let c1 = s
            .add_node(Some(p), 100, ElementSource::Query("/-/hello".into()))
            .unwrap();
        let c2 = s.add_node(Some(p), 100, ElementSource::None).unwrap();

        s.set_element(p, Box::new(Placeholder::new().with_title("Root".into())))
            .unwrap();
        s.set_element(c1, Box::new(Placeholder::new().with_title("Child1".into())))
            .unwrap();
        s.set_active_handle(Some(c1));

        let json = serde_json::to_string(&s).expect("serialize");
        let restored: DirectAppState = serde_json::from_str(&json).expect("deserialize");

        assert_eq!(restored.roots(), vec![p]);
        assert_eq!(restored.children(p).unwrap(), vec![c1, c2]);
        assert_eq!(restored.parent(c1).unwrap(), Some(p));
        assert_eq!(restored.active_handle(), Some(c1));

        let elem = restored.get_element(p).unwrap().unwrap();
        assert_eq!(elem.title(), "Root");

        // c2 has no element (pending)
        assert!(restored.get_element(c2).unwrap().is_none());
        assert_eq!(restored.pending_nodes(), vec![c2]);
    }

    #[test]
    fn test_serialization_preserves_handle_counter() {
        let mut s = DirectAppState::new();
        let _h1 = s.add_node(None, 0, ElementSource::None).unwrap();
        let _h2 = s.add_node(None, 0, ElementSource::None).unwrap();

        let json = serde_json::to_string(&s).expect("serialize");
        let mut restored: DirectAppState = serde_json::from_str(&json).expect("deserialize");

        // Next handle should not collide
        let h3 = restored.add_node(None, 0, ElementSource::None).unwrap();
        assert!(h3.0 > _h2.0);
    }

    // ── Multiple Children ─────────────────────────────────────────────────

    #[test]
    fn test_multiple_children_order_preserved() {
        let mut s = DirectAppState::new();
        let p = s.add_node(None, 0, ElementSource::None).unwrap();
        let mut handles = Vec::new();
        for _ in 0..5 {
            handles.push(s.add_node(Some(p), 100, ElementSource::None).unwrap());
        }
        assert_eq!(s.children(p).unwrap(), handles);
    }

    // ── InsertionPoint-Based Insertion ────────────────────────────────

    #[test]
    fn test_insert_source_last_child() {
        let mut s = DirectAppState::new();
        let p = s.add_node(None, 0, ElementSource::None).unwrap();
        s.set_element(p, Box::new(Placeholder::new())).unwrap();

        let h = s
            .insert_source(
                &InsertionPoint::LastChild(p),
                ElementSource::Query("/-/hello".into()),
            )
            .unwrap();
        assert_eq!(s.children(p).unwrap(), vec![h]);
        assert!(s.get_element(h).unwrap().is_none()); // pending
        assert!(matches!(s.get_source(h).unwrap(), ElementSource::Query(_)));
    }

    #[test]
    fn test_insert_source_first_child() {
        let mut s = DirectAppState::new();
        let p = s.add_node(None, 0, ElementSource::None).unwrap();
        let c = s.add_node(Some(p), 0, ElementSource::None).unwrap();

        let h = s
            .insert_source(
                &InsertionPoint::FirstChild(p),
                ElementSource::Query("/-/q".into()),
            )
            .unwrap();
        assert_eq!(s.children(p).unwrap(), vec![h, c]);
    }

    #[test]
    fn test_insert_source_before() {
        let mut s = DirectAppState::new();
        let p = s.add_node(None, 0, ElementSource::None).unwrap();
        let c1 = s.add_node(Some(p), 100, ElementSource::None).unwrap();
        let c2 = s.add_node(Some(p), 100, ElementSource::None).unwrap();

        let h = s
            .insert_source(&InsertionPoint::Before(c2), ElementSource::None)
            .unwrap();
        assert_eq!(s.children(p).unwrap(), vec![c1, h, c2]);
    }

    #[test]
    fn test_insert_source_after() {
        let mut s = DirectAppState::new();
        let p = s.add_node(None, 0, ElementSource::None).unwrap();
        let c1 = s.add_node(Some(p), 100, ElementSource::None).unwrap();
        let c2 = s.add_node(Some(p), 100, ElementSource::None).unwrap();

        let h = s
            .insert_source(&InsertionPoint::After(c1), ElementSource::None)
            .unwrap();
        assert_eq!(s.children(p).unwrap(), vec![c1, h, c2]);
    }

    #[test]
    fn test_insert_source_instead_replaces_source_clears_element() {
        let mut s = DirectAppState::new();
        let h = s.add_node(None, 0, ElementSource::None).unwrap();
        s.set_element(h, Box::new(Placeholder::new())).unwrap();
        assert!(s.get_element(h).unwrap().is_some());

        let returned = s
            .insert_source(
                &InsertionPoint::Instead(h),
                ElementSource::Query("/-/new".into()),
            )
            .unwrap();
        assert_eq!(returned, h);
        assert!(matches!(s.get_source(h).unwrap(), ElementSource::Query(_)));
        // Element cleared — now pending
        assert!(s.get_element(h).unwrap().is_none());
    }

    #[test]
    fn test_insert_source_root() {
        let mut s = DirectAppState::new();
        let h = s
            .insert_source(&InsertionPoint::Root, ElementSource::None)
            .unwrap();
        assert!(s.node_exists(h));
        assert_eq!(s.parent(h).unwrap(), None);
    }

    #[test]
    fn test_insert_element_last_child() {
        let mut s = DirectAppState::new();
        let p = s.add_node(None, 0, ElementSource::None).unwrap();

        let h = s
            .insert_element(
                &InsertionPoint::LastChild(p),
                Box::new(Placeholder::new().with_title("Inserted".into())),
            )
            .unwrap();

        let elem = s.get_element(h).unwrap().unwrap();
        assert_eq!(elem.title(), "Inserted");
        assert_eq!(s.children(p).unwrap(), vec![h]);
    }

    #[test]
    fn test_insert_source_instead_removes_children() {
        let mut s = DirectAppState::new();
        let p = s.add_node(None, 0, ElementSource::None).unwrap();
        s.set_element(p, Box::new(Placeholder::new())).unwrap();
        let c1 = s.add_node(Some(p), 0, ElementSource::None).unwrap();
        let c2 = s.add_node(Some(c1), 0, ElementSource::None).unwrap();

        s.insert_source(
            &InsertionPoint::Instead(p),
            ElementSource::Query("/-/new".into()),
        )
        .unwrap();

        // Children and grandchildren should be removed
        assert!(!s.node_exists(c1));
        assert!(!s.node_exists(c2));
        assert!(s.children(p).unwrap().is_empty());
    }

    #[test]
    fn test_insert_element_instead_removes_children() {
        let mut s = DirectAppState::new();
        let p = s.add_node(None, 0, ElementSource::None).unwrap();
        s.set_element(p, Box::new(Placeholder::new().with_title("Old".into())))
            .unwrap();
        let c1 = s.add_node(Some(p), 0, ElementSource::None).unwrap();
        let _c2 = s.add_node(Some(c1), 0, ElementSource::None).unwrap();

        s.insert_element(
            &InsertionPoint::Instead(p),
            Box::new(Placeholder::new().with_title("New".into())),
        )
        .unwrap();

        let elem = s.get_element(p).unwrap().unwrap();
        assert_eq!(elem.title(), "New");
        assert!(!s.node_exists(c1));
        assert!(s.children(p).unwrap().is_empty());
    }

    #[test]
    fn test_insert_state_instead_removes_children() {
        let mut s = DirectAppState::new();
        let p = s.add_node(None, 0, ElementSource::None).unwrap();
        s.set_element(p, Box::new(Placeholder::new())).unwrap();
        let c1 = s.add_node(Some(p), 0, ElementSource::None).unwrap();

        let state = State::from_parts(Arc::new(Value::from("replaced")), Arc::new(liquers_core::metadata::Metadata::new()));
        s.insert_state(&InsertionPoint::Instead(p), &state).unwrap();

        assert!(!s.node_exists(c1));
        assert!(s.children(p).unwrap().is_empty());
        let elem = s.get_element(p).unwrap().unwrap();
        assert_eq!(elem.type_name(), "StateViewElement");
    }

    #[test]
    fn test_insert_element_instead_replaces() {
        let mut s = DirectAppState::new();
        let h = s.add_node(None, 0, ElementSource::None).unwrap();
        s.set_element(h, Box::new(Placeholder::new().with_title("Old".into())))
            .unwrap();

        let returned = s
            .insert_element(
                &InsertionPoint::Instead(h),
                Box::new(Placeholder::new().with_title("New".into())),
            )
            .unwrap();
        assert_eq!(returned, h);
        let elem = s.get_element(h).unwrap().unwrap();
        assert_eq!(elem.title(), "New");
    }

    #[test]
    fn test_insert_state_wraps_in_state_view() {
        let mut s = DirectAppState::new();
        let p = s.add_node(None, 0, ElementSource::None).unwrap();

        let state = State::from_parts(Arc::new(Value::from("test value")), Arc::new(liquers_core::metadata::Metadata::new()));
        let h = s
            .insert_state(&InsertionPoint::LastChild(p), &state)
            .unwrap();

        let elem = s.get_element(h).unwrap().unwrap();
        assert_eq!(elem.type_name(), "StateViewElement");
    }

    #[test]
    fn test_insert_state_instead() {
        let mut s = DirectAppState::new();
        let h = s.add_node(None, 0, ElementSource::None).unwrap();
        s.set_element(h, Box::new(Placeholder::new())).unwrap();

        let state = State::from_parts(Arc::new(Value::from("replaced")), Arc::new(liquers_core::metadata::Metadata::new()));
        let returned = s.insert_state(&InsertionPoint::Instead(h), &state).unwrap();
        assert_eq!(returned, h);
        let elem = s.get_element(h).unwrap().unwrap();
        assert_eq!(elem.type_name(), "StateViewElement");
    }

    #[test]
    fn test_set_source() {
        let mut s = DirectAppState::new();
        let h = s.add_node(None, 0, ElementSource::None).unwrap();
        assert!(matches!(s.get_source(h).unwrap(), ElementSource::None));

        s.set_source(h, ElementSource::Query("/-/hello".into()))
            .unwrap();
        assert!(matches!(s.get_source(h).unwrap(), ElementSource::Query(_)));
    }

    #[test]
    fn test_set_source_not_found() {
        let mut s = DirectAppState::new();
        assert!(s.set_source(UIHandle(99), ElementSource::None).is_err());
    }

    // ── Invalidation: recording sites ─────────────────────────────────────

    /// Drain the initial `All` so a test can assert on what the operation under test recorded.
    fn drained() -> DirectAppState {
        let mut s = DirectAppState::new();
        let _ = s.take_invalidation();
        s
    }

    /// The recorded changes, or a panic naming what was found instead.
    fn changes(s: &mut DirectAppState) -> Vec<UIChange> {
        match s.take_invalidation() {
            Invalidation::Changes(v) => v,
            Invalidation::None => panic!("expected recorded changes, got None"),
            Invalidation::All => panic!("expected recorded changes, got All"),
        }
    }

    #[test]
    fn add_node_records_inserted() -> Result<(), Box<dyn std::error::Error>> {
        let mut s = drained();
        let root = s.add_node(None, 0, ElementSource::None)?;
        let _ = s.take_invalidation();

        let child = s.add_node(Some(root), 0, ElementSource::None)?;

        assert_eq!(
            changes(&mut s),
            vec![UIChange::Inserted {
                parent: Some(root),
                handle: child,
                index: 0
            }]
        );
        Ok(())
    }

    #[test]
    fn add_node_records_resolved_index() -> Result<(), Box<dyn std::error::Error>> {
        let mut s = drained();
        let root = s.add_node(None, 0, ElementSource::None)?;
        let _first = s.add_node(Some(root), 0, ElementSource::None)?;
        let _ = s.take_invalidation();

        // Position beyond the end is clamped when linking; the record must carry the real index.
        let last = s.add_node(Some(root), 99, ElementSource::None)?;

        assert_eq!(
            changes(&mut s),
            vec![UIChange::Inserted {
                parent: Some(root),
                handle: last,
                index: 1
            }]
        );
        Ok(())
    }

    #[test]
    fn add_root_records_inserted_with_no_parent() -> Result<(), Box<dyn std::error::Error>> {
        let mut s = drained();
        let root = s.add_node(None, 0, ElementSource::None)?;

        assert_eq!(
            changes(&mut s),
            vec![UIChange::Inserted {
                parent: None,
                handle: root,
                index: 0
            }],
            "a root add is expressible as an insert, not an escalation to All"
        );
        Ok(())
    }

    #[test]
    fn insert_node_records_inserted() -> Result<(), Box<dyn std::error::Error>> {
        let mut s = drained();
        let root = s.add_node(None, 0, ElementSource::None)?;
        let _ = s.take_invalidation();

        s.insert_node(UIHandle(42), Some(root), 0, ElementSource::None)?;

        assert_eq!(
            changes(&mut s),
            vec![UIChange::Inserted {
                parent: Some(root),
                handle: UIHandle(42),
                index: 0
            }]
        );
        Ok(())
    }

    #[test]
    fn set_element_records_replaced() -> Result<(), Box<dyn std::error::Error>> {
        let mut s = drained();
        let h = s.add_node(None, 0, ElementSource::None)?;
        let _ = s.take_invalidation();

        s.set_element(h, Box::new(Placeholder::new()))?;

        assert_eq!(changes(&mut s), vec![UIChange::Replaced { handle: h }]);
        Ok(())
    }

    #[test]
    fn set_source_records_replaced() -> Result<(), Box<dyn std::error::Error>> {
        let mut s = drained();
        let h = s.add_node(None, 0, ElementSource::None)?;
        let _ = s.take_invalidation();

        s.set_source(h, ElementSource::Query("hello".into()))?;

        assert_eq!(changes(&mut s), vec![UIChange::Replaced { handle: h }]);
        Ok(())
    }

    #[test]
    fn remove_records_removed_with_parent() -> Result<(), Box<dyn std::error::Error>> {
        let mut s = drained();
        let root = s.add_node(None, 0, ElementSource::None)?;
        let child = s.add_node(Some(root), 0, ElementSource::None)?;
        let _ = s.take_invalidation();

        s.remove(child)?;

        assert_eq!(
            changes(&mut s),
            vec![UIChange::Removed {
                parent: Some(root),
                handle: child
            }]
        );
        Ok(())
    }

    #[test]
    fn remove_records_one_change_for_a_subtree() -> Result<(), Box<dyn std::error::Error>> {
        let mut s = drained();
        let root = s.add_node(None, 0, ElementSource::None)?;
        let child = s.add_node(Some(root), 0, ElementSource::None)?;
        let _grandchild = s.add_node(Some(child), 0, ElementSource::None)?;
        let _ = s.take_invalidation();

        s.remove(child)?;

        assert_eq!(
            changes(&mut s).len(),
            1,
            "descendants go with the subtree root; they need no records of their own"
        );
        Ok(())
    }

    #[test]
    fn remove_root_records_parent_none() -> Result<(), Box<dyn std::error::Error>> {
        let mut s = drained();
        let root = s.add_node(None, 0, ElementSource::None)?;
        let _ = s.take_invalidation();

        s.remove(root)?;

        assert_eq!(
            changes(&mut s),
            vec![UIChange::Removed {
                parent: None,
                handle: root
            }]
        );
        Ok(())
    }

    #[test]
    fn set_active_handle_records_both() -> Result<(), Box<dyn std::error::Error>> {
        let mut s = drained();
        let a = s.add_node(None, 0, ElementSource::None)?;
        let b = s.add_node(None, 1, ElementSource::None)?;
        s.set_active_handle(Some(a));
        let _ = s.take_invalidation();

        s.set_active_handle(Some(b));

        assert_eq!(
            changes(&mut s),
            vec![
                UIChange::Replaced { handle: a },
                UIChange::Replaced { handle: b },
            ]
        );
        Ok(())
    }

    #[test]
    fn set_active_handle_to_same_records_nothing() -> Result<(), Box<dyn std::error::Error>> {
        let mut s = drained();
        let a = s.add_node(None, 0, ElementSource::None)?;
        s.set_active_handle(Some(a));
        let _ = s.take_invalidation();

        s.set_active_handle(Some(a));

        assert_eq!(s.take_invalidation(), Invalidation::None);
        Ok(())
    }

    #[test]
    fn get_element_mut_records_replaced() -> Result<(), Box<dyn std::error::Error>> {
        let mut s = drained();
        let h = s.add_node(None, 0, ElementSource::None)?;
        s.set_element(h, Box::new(Placeholder::new()))?;
        let _ = s.take_invalidation();

        let _elem = s.get_element_mut(h)?;

        assert_eq!(
            changes(&mut s),
            vec![UIChange::Replaced { handle: h }],
            "&mut access must not be able to bypass recording"
        );
        Ok(())
    }

    #[test]
    fn take_and_put_element_record_nothing() -> Result<(), Box<dyn std::error::Error>> {
        let mut s = drained();
        let h = s.add_node(None, 0, ElementSource::None)?;
        s.set_element(h, Box::new(Placeholder::new()))?;
        let _ = s.take_invalidation();

        // The extract-render-replace pair an immediate-mode backend runs every frame.
        let elem = s.take_element(h)?;
        s.put_element(h, elem)?;

        assert_eq!(
            s.take_invalidation(),
            Invalidation::None,
            "recording here would mark the tree changed on every rendered frame"
        );
        Ok(())
    }

    #[test]
    fn new_state_starts_all() {
        let mut s = DirectAppState::new();
        assert_eq!(
            s.take_invalidation(),
            Invalidation::All,
            "a renderer attaching to a fresh state must paint once"
        );
    }

    // ── Invalidation: serialization ───────────────────────────────────────

    #[test]
    fn serialize_omits_invalidation() -> Result<(), Box<dyn std::error::Error>> {
        let mut s = DirectAppState::new();
        let h = s.add_node(None, 0, ElementSource::None)?;
        s.set_element(h, Box::new(Placeholder::new()))?;

        let json = serde_json::to_string(&s)?;

        assert!(
            !json.contains("invalidation"),
            "the persisted format must be unchanged: {json}"
        );
        Ok(())
    }

    #[test]
    fn deserialize_starts_all() -> Result<(), Box<dyn std::error::Error>> {
        let mut s = DirectAppState::new();
        let h = s.add_node(None, 0, ElementSource::None)?;
        s.set_element(h, Box::new(Placeholder::new()))?;
        let json = serde_json::to_string(&s)?;

        let mut restored: DirectAppState = serde_json::from_str(&json)?;

        assert_eq!(
            restored.take_invalidation(),
            Invalidation::All,
            "a restored state has no change records, so it must ask for a full render"
        );
        Ok(())
    }

    // ── Invalidation: the conservative trait defaults ─────────────────────

    /// An `AppState` that overrides none of the invalidation methods. Exists to pin the property
    /// the whole trait-extension approach rests on: not tracking degrades to a full re-render,
    /// never to a stale one.
    #[derive(Debug, Default)]
    struct NonTrackingAppState {
        inner: DirectAppState,
    }

    impl AppState for NonTrackingAppState {
        fn add_node(
            &mut self,
            parent: Option<UIHandle>,
            position: usize,
            source: ElementSource,
        ) -> Result<UIHandle, Error> {
            self.inner.add_node(parent, position, source)
        }
        fn insert_node(
            &mut self,
            handle: UIHandle,
            parent: Option<UIHandle>,
            position: usize,
            source: ElementSource,
        ) -> Result<(), Error> {
            self.inner.insert_node(handle, parent, position, source)
        }
        fn get_element(&self, handle: UIHandle) -> Result<Option<&dyn UIElement>, Error> {
            self.inner.get_element(handle)
        }
        fn get_element_mut(
            &mut self,
            handle: UIHandle,
        ) -> Result<Option<&mut Box<dyn UIElement>>, Error> {
            self.inner.get_element_mut(handle)
        }
        fn set_element(
            &mut self,
            handle: UIHandle,
            element: Box<dyn UIElement>,
        ) -> Result<(), Error> {
            self.inner.set_element(handle, element)
        }
        fn take_element(&mut self, handle: UIHandle) -> Result<Box<dyn UIElement>, Error> {
            self.inner.take_element(handle)
        }
        fn put_element(
            &mut self,
            handle: UIHandle,
            element: Box<dyn UIElement>,
        ) -> Result<(), Error> {
            self.inner.put_element(handle, element)
        }
        fn get_source(&self, handle: UIHandle) -> Result<&ElementSource, Error> {
            self.inner.get_source(handle)
        }
        fn set_source(&mut self, handle: UIHandle, source: ElementSource) -> Result<(), Error> {
            self.inner.set_source(handle, source)
        }
        fn remove(&mut self, handle: UIHandle) -> Result<(), Error> {
            self.inner.remove(handle)
        }
        fn roots(&self) -> Vec<UIHandle> {
            self.inner.roots()
        }
        fn parent(&self, handle: UIHandle) -> Result<Option<UIHandle>, Error> {
            self.inner.parent(handle)
        }
        fn children(&self, handle: UIHandle) -> Result<Vec<UIHandle>, Error> {
            self.inner.children(handle)
        }
        fn sibling(&self, handle: UIHandle, offset: i32) -> Result<Option<UIHandle>, Error> {
            self.inner.sibling(handle, offset)
        }
        fn active_handle(&self) -> Option<UIHandle> {
            self.inner.active_handle()
        }
        fn set_active_handle(&mut self, handle: Option<UIHandle>) {
            self.inner.set_active_handle(handle)
        }
        fn pending_nodes(&self) -> Vec<UIHandle> {
            self.inner.pending_nodes()
        }
        fn node_count(&self) -> usize {
            self.inner.node_count()
        }
    }

    #[test]
    fn default_trait_methods_report_all() -> Result<(), Box<dyn std::error::Error>> {
        let mut s = NonTrackingAppState::default();
        s.add_node(None, 0, ElementSource::None)?;
        s.invalidate_all(); // default: no-op

        assert_eq!(
            s.take_invalidation(),
            Invalidation::All,
            "an implementation that tracks nothing must ask for a full render, never report None"
        );
        Ok(())
    }

    // ── Invalidation state machine ────────────────────────────────────────

    #[test]
    fn invalidation_default_is_none() {
        let inv = Invalidation::default();
        assert_eq!(inv, Invalidation::None);
        assert!(inv.is_empty());
    }

    #[test]
    fn invalidation_records_in_order() {
        let mut inv = Invalidation::default();
        inv.record(UIChange::Replaced { handle: UIHandle(1) });
        inv.record(UIChange::Replaced { handle: UIHandle(2) });

        match inv {
            Invalidation::Changes(changes) => assert_eq!(
                changes,
                vec![
                    UIChange::Replaced { handle: UIHandle(1) },
                    UIChange::Replaced { handle: UIHandle(2) },
                ]
            ),
            Invalidation::None | Invalidation::All => panic!("expected recorded changes"),
        }
    }

    #[test]
    fn invalidation_take_clears() {
        let mut inv = Invalidation::default();
        inv.record(UIChange::Replaced { handle: UIHandle(1) });

        let taken = inv.take();
        assert!(!taken.is_empty());
        assert_eq!(inv, Invalidation::None);
        assert_eq!(inv.take(), Invalidation::None);
    }

    #[test]
    fn invalidation_set_all_absorbs_further_changes() {
        let mut inv = Invalidation::default();
        inv.record(UIChange::Replaced { handle: UIHandle(1) });
        inv.set_all();
        // A change recorded after All is already covered by it.
        inv.record(UIChange::Replaced { handle: UIHandle(2) });
        assert_eq!(inv, Invalidation::All);
        assert!(!inv.is_empty());
    }

    #[test]
    fn invalidation_overflow_escalates_to_all() {
        let mut inv = Invalidation::default();
        for i in 0..MAX_CHANGES {
            inv.record(UIChange::Replaced {
                handle: UIHandle(i as u64),
            });
        }
        match &inv {
            Invalidation::Changes(changes) => assert_eq!(changes.len(), MAX_CHANGES),
            Invalidation::None | Invalidation::All => panic!("should still be tracking changes"),
        }

        inv.record(UIChange::Replaced {
            handle: UIHandle(MAX_CHANGES as u64),
        });
        assert_eq!(inv, Invalidation::All, "past MAX_CHANGES it must escalate");
    }
}
