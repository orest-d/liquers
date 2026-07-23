//! Shared, framework-agnostic UI action.
//!
//! `UiAction` is the single action type across all backends: `ui_spec` menus, the web
//! backend's `data-lq-action` attributes, and egui buttons all interpret the same value.
//! Actions are portable *data* (not Rust closures), which is what lets them work uniformly
//! for SSR (emit as an attribute), the live browser (a delegated listener dispatches them),
//! and egui (a click handler runs them). See `specs/webui/phase2-architecture.md`.

use serde::de::{self, Deserializer};
use serde::ser::Serializer;
use serde::{Deserialize, Serialize};

use super::handle::UIHandle;
use super::ui_context::UIContext;

/// What a UI control does when triggered.
///
/// Serde is hand-written (see below) so each variant has a compact **string** form and a
/// bare query string deserializes straight to `Query`.
#[derive(Clone, Debug, PartialEq)]
pub enum UiAction {
    /// Do nothing. The default; also the target of a disabled/placeholder control.
    None,
    /// Request application shutdown (`UIContext::request_quit`).
    Quit,
    /// Submit `query` bound to the element that owns the control (its own handle, or the
    /// active element if the control has none). Cross-element targeting is expressed inside
    /// the query via `lui` navigation words.
    Query(String),
    /// Take the live value of the input control named `input_id`, use it as the input state,
    /// and apply `query` to it, binding the result to `handle`. Reading the live value is the
    /// backend's responsibility (the web driver reads the DOM input and sends
    /// `AppMessage::ApplyToInput`); `dispatch_action` therefore does not handle this variant.
    Apply {
        handle: UIHandle,
        input_id: String,
        query: String,
    },
}

impl Default for UiAction {
    fn default() -> Self {
        UiAction::None
    }
}

/// Dispatch a value-less action against the UI. Shared by egui and web click handling.
///
/// `own_handle` is the element that owns the triggered control; it is the default target for
/// `Query`. `Apply` is intentionally a no-op here — it requires reading a live input value,
/// which the backend does before dispatch (the web driver intercepts `Apply`, reads the input,
/// and sends `AppMessage::ApplyToInput`; egui menus never produce `Apply`).
pub fn dispatch_action(action: &UiAction, ctx: &UIContext, own_handle: Option<UIHandle>) {
    match action {
        UiAction::None => {}
        UiAction::Quit => ctx.request_quit(),
        UiAction::Query(query) => match own_handle {
            Some(handle) => ctx.submit_query(handle, query.clone()),
            None => ctx.submit_root_query(query.clone()),
        },
        UiAction::Apply { .. } => {
            // Handled by the backend's input-reading path before reaching dispatch_action.
        }
    }
}

// ─── Serialization (custom, string-first) ───────────────────────────────────

impl Serialize for UiAction {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        match self {
            UiAction::None => serializer.serialize_str("none"),
            UiAction::Quit => serializer.serialize_str("quit"),
            UiAction::Query(query) => serializer.serialize_str(query),
            UiAction::Apply {
                handle,
                input_id,
                query,
            } => serializer.serialize_str(&format!("apply:{}:{}:{}", handle.0, input_id, query)),
        }
    }
}

/// Parse the string form of a `UiAction`. `"none"`/`"quit"` are reserved keywords; the
/// `"apply:{handle}:{input_id}:{query}"` form is split into 4 (so the query may contain `:`);
/// anything else is a bare `Query`.
fn parse_string_form(s: &str) -> Result<UiAction, String> {
    match s {
        "none" => Ok(UiAction::None),
        "quit" => Ok(UiAction::Quit),
        _ if s.starts_with("apply:") => {
            let parts: Vec<&str> = s.splitn(4, ':').collect();
            if parts.len() != 4 {
                return Err(format!("malformed apply action: '{}'", s));
            }
            let handle = parts[1]
                .parse::<u64>()
                .map_err(|_| format!("invalid handle in apply action: '{}'", parts[1]))?;
            Ok(UiAction::Apply {
                handle: UIHandle(handle),
                input_id: parts[2].to_string(),
                query: parts[3].to_string(),
            })
        }
        other => Ok(UiAction::Query(other.to_string())),
    }
}

/// Explicit map forms accepted on deserialize (for clarity and `MenuAction` back-compat).
#[derive(Deserialize)]
#[serde(untagged)]
enum UiActionDe {
    Null(()),
    Str(String),
    QueryMap { query: String },
    ApplyMap { apply: ApplyDe },
}

#[derive(Deserialize)]
struct ApplyDe {
    handle: u64,
    input_id: String,
    query: String,
}

impl<'de> Deserialize<'de> for UiAction {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        match UiActionDe::deserialize(deserializer)? {
            UiActionDe::Null(()) => Ok(UiAction::None),
            UiActionDe::Str(s) => parse_string_form(&s).map_err(de::Error::custom),
            UiActionDe::QueryMap { query } => Ok(UiAction::Query(query)),
            UiActionDe::ApplyMap { apply } => Ok(UiAction::Apply {
                handle: UIHandle(apply.handle),
                input_id: apply.input_id,
                query: apply.query,
            }),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn roundtrip(a: &UiAction) -> UiAction {
        let s = serde_json::to_string(a).expect("serialize");
        serde_json::from_str(&s).expect("deserialize")
    }

    #[test]
    fn none_string_form() {
        assert_eq!(serde_json::to_string(&UiAction::None).unwrap(), "\"none\"");
        assert_eq!(roundtrip(&UiAction::None), UiAction::None);
    }

    #[test]
    fn quit_string_form() {
        assert_eq!(serde_json::to_string(&UiAction::Quit).unwrap(), "\"quit\"");
        assert_eq!(roundtrip(&UiAction::Quit), UiAction::Quit);
    }

    #[test]
    fn query_bare_string() {
        let a = UiAction::Query("dashboard/q/ns-lui/add-child".to_string());
        assert_eq!(
            serde_json::to_string(&a).unwrap(),
            "\"dashboard/q/ns-lui/add-child\""
        );
        assert_eq!(roundtrip(&a), a);
    }

    #[test]
    fn bare_string_deserializes_to_query() {
        let a: UiAction = serde_json::from_str("\"dashboard/q/ns-lui/add-child\"").unwrap();
        assert_eq!(a, UiAction::Query("dashboard/q/ns-lui/add-child".to_string()));
    }

    #[test]
    fn apply_string_form_keeps_colons_in_query() {
        let a = UiAction::Apply {
            handle: UIHandle(7),
            input_id: "qc-input-7".to_string(),
            query: "ns-lui/submit".to_string(),
        };
        assert_eq!(
            serde_json::to_string(&a).unwrap(),
            "\"apply:7:qc-input-7:ns-lui/submit\""
        );
        assert_eq!(roundtrip(&a), a);

        // query containing a colon survives splitn(4)
        let a2 = UiAction::Apply {
            handle: UIHandle(3),
            input_id: "f".to_string(),
            query: "a:b:c".to_string(),
        };
        assert_eq!(roundtrip(&a2), a2);
    }

    #[test]
    fn explicit_query_map_wins_over_reserved_word() {
        let a: UiAction = serde_json::from_str("{\"query\":\"quit\"}").unwrap();
        assert_eq!(a, UiAction::Query("quit".to_string()));
    }

    #[test]
    fn null_deserializes_to_none() {
        let a: UiAction = serde_json::from_str("null").unwrap();
        assert_eq!(a, UiAction::None);
    }

    #[test]
    fn yaml_menuaction_forms() {
        // MenuAction back-compat: null, "quit", {query}, and bare string.
        let quit: UiAction = serde_yaml::from_str("quit").unwrap();
        assert_eq!(quit, UiAction::Quit);
        let q: UiAction = serde_yaml::from_str("{ query: \"text-hello\" }").unwrap();
        assert_eq!(q, UiAction::Query("text-hello".to_string()));
        let bare: UiAction = serde_yaml::from_str("\"text-hello/ns-lui/markdown\"").unwrap();
        assert_eq!(bare, UiAction::Query("text-hello/ns-lui/markdown".to_string()));
    }
}
