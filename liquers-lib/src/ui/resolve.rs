use liquers_core::error::Error;

use super::app_state::AppState;
use super::handle::UIHandle;

/// Describes where to insert a new node relative to a reference node.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum InsertionPoint {
    /// Insert as the first child of the reference node.
    FirstChild(UIHandle),
    /// Insert as the last child of the reference node.
    LastChild(UIHandle),
    /// Insert before the reference node (same parent).
    Before(UIHandle),
    /// Insert after the reference node (same parent).
    After(UIHandle),
    /// Replace the reference node.
    Instead(UIHandle),
    /// Insert as a root (no parent) at the end.
    Root,
}

/// Resolve a navigation word to a UIHandle relative to the current active element.
///
/// Navigation words:
/// - `"current"` — the currently active element
/// - `"parent"` — parent of current
/// - `"next"` — next sibling of current
/// - `"prev"` — previous sibling of current
/// - `"first"` — first child of current
/// - `"last"` — last child of current
/// - `"root"` — first root element
/// - `"<number>"` — element with handle = number
pub fn resolve_navigation(
    app_state: &dyn AppState,
    word: &str,
    current: Option<UIHandle>,
) -> Result<UIHandle, Error> {
    match word {
        "current" => current.ok_or_else(|| {
            Error::general_error("No current element".to_string())
        }),
        "parent" => {
            let h = current.ok_or_else(|| {
                Error::general_error("No current element for 'parent'".to_string())
            })?;
            app_state
                .parent(h)?
                .ok_or_else(|| Error::general_error("Current element has no parent".to_string()))
        }
        "next" => {
            let h = current.ok_or_else(|| {
                Error::general_error("No current element for 'next'".to_string())
            })?;
            app_state
                .next_sibling(h)?
                .ok_or_else(|| Error::general_error("No next sibling".to_string()))
        }
        "prev" => {
            let h = current.ok_or_else(|| {
                Error::general_error("No current element for 'prev'".to_string())
            })?;
            app_state
                .previous_sibling(h)?
                .ok_or_else(|| Error::general_error("No previous sibling".to_string()))
        }
        "first" => {
            let h = current.ok_or_else(|| {
                Error::general_error("No current element for 'first'".to_string())
            })?;
            app_state
                .first_child(h)?
                .ok_or_else(|| Error::general_error("No children".to_string()))
        }
        "last" => {
            let h = current.ok_or_else(|| {
                Error::general_error("No current element for 'last'".to_string())
            })?;
            app_state
                .last_child(h)?
                .ok_or_else(|| Error::general_error("No children".to_string()))
        }
        "root" => {
            let roots = app_state.roots();
            roots
                .first()
                .copied()
                .ok_or_else(|| Error::general_error("No root elements".to_string()))
        }
        other => {
            // Try to parse as a handle number
            let n: u64 = other.parse().map_err(|_| {
                Error::general_error(format!(
                    "Unknown navigation word: '{}'. Expected: current, parent, next, prev, first, last, root, or a number",
                    other
                ))
            })?;
            let handle = UIHandle(n);
            // Validate the handle exists
            if !app_state.node_exists(handle) {
                return Err(Error::general_error(format!(
                    "Node not found: {:?}",
                    handle
                )));
            }
            Ok(handle)
        }
    }
}

/// Resolve a position word + reference to an InsertionPoint.
///
/// Position words:
/// - `"before"` — insert before the reference
/// - `"after"` — insert after the reference
/// - `"instead"` — replace the reference
/// - `"first"` — insert as first child of the reference
/// - `"last"` — insert as last child of the reference
/// - `"child"` — same as "last" (append as child)
pub fn resolve_position(
    position_word: &str,
    reference: UIHandle,
) -> Result<InsertionPoint, Error> {
    match position_word {
        "before" => Ok(InsertionPoint::Before(reference)),
        "after" => Ok(InsertionPoint::After(reference)),
        "instead" => Ok(InsertionPoint::Instead(reference)),
        "first" => Ok(InsertionPoint::FirstChild(reference)),
        "last" | "child" => Ok(InsertionPoint::LastChild(reference)),
        other => Err(Error::general_error(format!(
            "Unknown position word: '{}'. Expected: before, after, instead, first, last, child",
            other
        ))),
    }
}

/// Convert an InsertionPoint to (parent, position) arguments for AppState.add_node.
///
/// Returns (parent_handle, position_index).
/// For `Instead`, the caller must handle replacement separately.
pub fn insertion_point_to_add_args<S: AppState + ?Sized>(
    app_state: &S,
    point: &InsertionPoint,
) -> Result<(Option<UIHandle>, usize), Error> {
    match point {
        InsertionPoint::FirstChild(parent) => Ok((Some(*parent), 0)),
        InsertionPoint::LastChild(parent) => {
            let children = app_state.children(*parent)?;
            Ok((Some(*parent), children.len()))
        }
        InsertionPoint::Before(sibling) => {
            let parent = app_state
                .parent(*sibling)?
                .ok_or_else(|| Error::general_error("Cannot insert before a root element".to_string()))?;
            let children = app_state.children(parent)?;
            let index = children
                .iter()
                .position(|h| *h == *sibling)
                .ok_or_else(|| {
                    Error::general_error("Sibling not found in parent's children".to_string())
                })?;
            Ok((Some(parent), index))
        }
        InsertionPoint::After(sibling) => {
            let parent = app_state
                .parent(*sibling)?
                .ok_or_else(|| Error::general_error("Cannot insert after a root element".to_string()))?;
            let children = app_state.children(parent)?;
            let index = children
                .iter()
                .position(|h| *h == *sibling)
                .ok_or_else(|| {
                    Error::general_error("Sibling not found in parent's children".to_string())
                })?;
            Ok((Some(parent), index + 1))
        }
        InsertionPoint::Instead(_) => {
            Err(Error::general_error(
                "Instead insertion must be handled by the caller".to_string(),
            ))
        }
        InsertionPoint::Root => Ok((None, 0)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ui::app_state::DirectAppState;
    use crate::ui::element::ElementSource;

    fn build_tree() -> (DirectAppState, UIHandle, UIHandle, UIHandle, UIHandle) {
        let mut s = DirectAppState::new();
        let root = s.add_node(None, 0, ElementSource::None).unwrap();
        let c1 = s.add_node(Some(root), 100, ElementSource::None).unwrap();
        let c2 = s.add_node(Some(root), 100, ElementSource::None).unwrap();
        let c3 = s.add_node(Some(root), 100, ElementSource::None).unwrap();
        (s, root, c1, c2, c3)
    }

    // ── resolve_navigation ────────────────────────────────────────────────

    #[test]
    fn test_resolve_current() {
        let (s, _root, c1, _c2, _c3) = build_tree();
        assert_eq!(resolve_navigation(&s, "current", Some(c1)).unwrap(), c1);
    }

    #[test]
    fn test_resolve_current_no_active() {
        let (s, _, _, _, _) = build_tree();
        assert!(resolve_navigation(&s, "current", None).is_err());
    }

    #[test]
    fn test_resolve_parent() {
        let (s, root, c1, _c2, _c3) = build_tree();
        assert_eq!(resolve_navigation(&s, "parent", Some(c1)).unwrap(), root);
    }

    #[test]
    fn test_resolve_parent_of_root() {
        let (s, root, _, _, _) = build_tree();
        assert!(resolve_navigation(&s, "parent", Some(root)).is_err());
    }

    #[test]
    fn test_resolve_next() {
        let (s, _root, c1, c2, _c3) = build_tree();
        assert_eq!(resolve_navigation(&s, "next", Some(c1)).unwrap(), c2);
    }

    #[test]
    fn test_resolve_next_at_end() {
        let (s, _root, _c1, _c2, c3) = build_tree();
        assert!(resolve_navigation(&s, "next", Some(c3)).is_err());
    }

    #[test]
    fn test_resolve_prev() {
        let (s, _root, c1, c2, _c3) = build_tree();
        assert_eq!(resolve_navigation(&s, "prev", Some(c2)).unwrap(), c1);
    }

    #[test]
    fn test_resolve_prev_at_start() {
        let (s, _root, c1, _c2, _c3) = build_tree();
        assert!(resolve_navigation(&s, "prev", Some(c1)).is_err());
    }

    #[test]
    fn test_resolve_first_child() {
        let (s, root, c1, _c2, _c3) = build_tree();
        assert_eq!(resolve_navigation(&s, "first", Some(root)).unwrap(), c1);
    }

    #[test]
    fn test_resolve_last_child() {
        let (s, root, _c1, _c2, c3) = build_tree();
        assert_eq!(resolve_navigation(&s, "last", Some(root)).unwrap(), c3);
    }

    #[test]
    fn test_resolve_root() {
        let (s, root, _, _, _) = build_tree();
        assert_eq!(resolve_navigation(&s, "root", None).unwrap(), root);
    }

    #[test]
    fn test_resolve_root_empty() {
        let s = DirectAppState::new();
        assert!(resolve_navigation(&s, "root", None).is_err());
    }

    #[test]
    fn test_resolve_number() {
        let (s, _root, c1, _c2, _c3) = build_tree();
        let n = c1.0;
        assert_eq!(
            resolve_navigation(&s, &n.to_string(), None).unwrap(),
            c1
        );
    }

    #[test]
    fn test_resolve_number_not_found() {
        let (s, _, _, _, _) = build_tree();
        assert!(resolve_navigation(&s, "9999", None).is_err());
    }

    #[test]
    fn test_resolve_unknown_word() {
        let (s, _, _, _, _) = build_tree();
        assert!(resolve_navigation(&s, "foobar", None).is_err());
    }

    // ── resolve_position ──────────────────────────────────────────────────

    #[test]
    fn test_position_before() {
        let h = UIHandle(1);
        assert_eq!(
            resolve_position("before", h).unwrap(),
            InsertionPoint::Before(h)
        );
    }

    #[test]
    fn test_position_after() {
        let h = UIHandle(1);
        assert_eq!(
            resolve_position("after", h).unwrap(),
            InsertionPoint::After(h)
        );
    }

    #[test]
    fn test_position_instead() {
        let h = UIHandle(1);
        assert_eq!(
            resolve_position("instead", h).unwrap(),
            InsertionPoint::Instead(h)
        );
    }

    #[test]
    fn test_position_first() {
        let h = UIHandle(1);
        assert_eq!(
            resolve_position("first", h).unwrap(),
            InsertionPoint::FirstChild(h)
        );
    }

    #[test]
    fn test_position_last() {
        let h = UIHandle(1);
        assert_eq!(
            resolve_position("last", h).unwrap(),
            InsertionPoint::LastChild(h)
        );
    }

    #[test]
    fn test_position_child() {
        let h = UIHandle(1);
        assert_eq!(
            resolve_position("child", h).unwrap(),
            InsertionPoint::LastChild(h)
        );
    }

    #[test]
    fn test_position_unknown() {
        assert!(resolve_position("above", UIHandle(1)).is_err());
    }

    // ── insertion_point_to_add_args ───────────────────────────────────────

    #[test]
    fn test_add_args_first_child() {
        let (s, root, _c1, _c2, _c3) = build_tree();
        let (parent, pos) =
            insertion_point_to_add_args(&s, &InsertionPoint::FirstChild(root)).unwrap();
        assert_eq!(parent, Some(root));
        assert_eq!(pos, 0);
    }

    #[test]
    fn test_add_args_last_child() {
        let (s, root, _c1, _c2, _c3) = build_tree();
        let (parent, pos) =
            insertion_point_to_add_args(&s, &InsertionPoint::LastChild(root)).unwrap();
        assert_eq!(parent, Some(root));
        assert_eq!(pos, 3); // 3 existing children
    }

    #[test]
    fn test_add_args_before() {
        let (s, root, _c1, c2, _c3) = build_tree();
        let (parent, pos) =
            insertion_point_to_add_args(&s, &InsertionPoint::Before(c2)).unwrap();
        assert_eq!(parent, Some(root));
        assert_eq!(pos, 1); // c2 is at index 1
    }

    #[test]
    fn test_add_args_after() {
        let (s, root, _c1, c2, _c3) = build_tree();
        let (parent, pos) =
            insertion_point_to_add_args(&s, &InsertionPoint::After(c2)).unwrap();
        assert_eq!(parent, Some(root));
        assert_eq!(pos, 2); // after c2 at index 1
    }

    #[test]
    fn test_add_args_root() {
        let (s, _, _, _, _) = build_tree();
        let (parent, pos) =
            insertion_point_to_add_args(&s, &InsertionPoint::Root).unwrap();
        assert_eq!(parent, None);
        assert_eq!(pos, 0);
    }

    #[test]
    fn test_add_args_instead_errors() {
        let (s, _, c1, _, _) = build_tree();
        assert!(insertion_point_to_add_args(&s, &InsertionPoint::Instead(c1)).is_err());
    }
}
