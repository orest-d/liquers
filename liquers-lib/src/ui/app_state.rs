use std::collections::HashMap;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};

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

// ─── AppState Trait ─────────────────────────────────────────────────────────

/// Trait for the UI application state. Manages a tree of UI elements.
///
/// All methods are synchronous — AppState holds in-memory data only.
/// When used from async contexts, wrap in `Arc<std::sync::Mutex<dyn AppState>>`.
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
    fn get_element_mut(&mut self, handle: UIHandle) -> Result<Option<&mut Box<dyn UIElement>>, Error>;

    /// Set the element for a node. Does NOT call init() — the caller
    /// (typically AppRunner) is responsible for calling init() with a UIContext.
    fn set_element(&mut self, handle: UIHandle, element: Box<dyn UIElement>) -> Result<(), Error>;

    /// Temporarily remove the element from a node (for extract-render-replace).
    fn take_element(&mut self, handle: UIHandle) -> Result<Box<dyn UIElement>, Error>;

    /// Put back an element that was taken with take_element.
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
        match point {
            InsertionPoint::Instead(target) => {
                let handle = *target;
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
        let value = &*state.data;

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
}

impl DirectAppState {
    pub fn new() -> Self {
        Self {
            nodes: HashMap::new(),
            next_id: AtomicU64::new(1),
            active_handle: None,
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
        if let Some(parent_handle) = parent {
            if let Some(parent_node) = self.nodes.get_mut(&parent_handle) {
                let insert_pos = position.min(parent_node.children.len());
                parent_node.children.insert(insert_pos, handle);
            }
        }

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
        if let Some(parent_handle) = parent {
            if let Some(parent_node) = self.nodes.get_mut(&parent_handle) {
                let insert_pos = position.min(parent_node.children.len());
                parent_node.children.insert(insert_pos, handle);
            }
        }

        Ok(())
    }

    fn get_element(&self, handle: UIHandle) -> Result<Option<&dyn UIElement>, Error> {
        let node = self
            .nodes
            .get(&handle)
            .ok_or_else(|| Error::general_error(format!("Node not found: {:?}", handle)))?;
        Ok(node.element.as_ref().map(|e| &**e))
    }

    fn get_element_mut(&mut self, handle: UIHandle) -> Result<Option<&mut Box<dyn UIElement>>, Error> {
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
        self.active_handle = handle;
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
        assert_eq!(
            s.roots(),
            vec![UIHandle(1), UIHandle(2), UIHandle(3)]
        );
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
        s.set_element(
            c1,
            Box::new(Placeholder::new().with_title("Child1".into())),
        )
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
        assert!(matches!(
            s.get_source(h).unwrap(),
            ElementSource::Query(_)
        ));
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
        assert!(matches!(
            s.get_source(h).unwrap(),
            ElementSource::Query(_)
        ));
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

        let state = State { data: Arc::new(Value::from("test value")), metadata: Arc::new(liquers_core::metadata::Metadata::new()) };
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

        let state = State { data: Arc::new(Value::from("replaced")), metadata: Arc::new(liquers_core::metadata::Metadata::new()) };
        let returned = s
            .insert_state(&InsertionPoint::Instead(h), &state)
            .unwrap();
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
        assert!(matches!(
            s.get_source(h).unwrap(),
            ElementSource::Query(_)
        ));
    }

    #[test]
    fn test_set_source_not_found() {
        let mut s = DirectAppState::new();
        assert!(s
            .set_source(UIHandle(99), ElementSource::None)
            .is_err());
    }
}
