//! Backend-neutral user interaction delivered to a `UIElement`.
//!
//! A `UIEvent` describes what the user meant (`Submit`, `Named("reset")`) plus the owning
//! element's field values, never how a device produced it. Each backend's driver translates its
//! own native input (DOM events, egui responses) into a `UIEvent` addressed by `UIHandle`; it is
//! delivered as `UpdateMessage::UI` through `AppMessage::UIEvent`. See
//! `specs/reference/UI_INTERFACE_FSD.md` §Event Handling.

use crate::value::Value;

/// What the user did, independent of backend. Closed: a new kind is a deliberate change.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum UIEventKind {
    /// The control's default/submit action: Enter in a field, a submit button.
    Submit,
    /// An element-defined event named in markup (`data-lq-event="refresh"`).
    Named(String),
}

impl UIEventKind {
    /// `"submit"` -> `Submit`; any other non-empty name -> `Named`; empty -> `None`.
    pub fn from_name(name: &str) -> Option<Self> {
        let name = name.trim();
        if name.is_empty() {
            None
        } else if name == "submit" {
            Some(UIEventKind::Submit)
        } else {
            Some(UIEventKind::Named(name.to_string()))
        }
    }

    /// The name written in markup: `"submit"` or the custom name.
    pub fn name(&self) -> &str {
        match self {
            UIEventKind::Submit => "submit",
            UIEventKind::Named(name) => name,
        }
    }
}

/// A user action addressed to one element, carrying its declared fields' live values.
#[derive(Clone, Debug)]
pub struct UIEvent {
    pub kind: UIEventKind,
    /// `(field name, value)` in document order; text fields arrive as text `Value`s.
    pub fields: Vec<(String, Value)>,
}

impl UIEvent {
    pub fn new(kind: UIEventKind) -> Self {
        UIEvent {
            kind,
            fields: Vec::new(),
        }
    }

    /// Append a field value (builder style).
    pub fn with_field(mut self, name: impl Into<String>, value: Value) -> Self {
        self.fields.push((name.into(), value));
        self
    }

    /// The first value of the named field, if present.
    pub fn field(&self, name: &str) -> Option<&Value> {
        self.fields
            .iter()
            .find(|(field_name, _)| field_name == name)
            .map(|(_, value)| value)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use liquers_core::value::ValueInterface;

    #[test]
    fn ui_event_kind_from_name() {
        assert_eq!(UIEventKind::from_name("submit"), Some(UIEventKind::Submit));
        assert_eq!(
            UIEventKind::from_name("reset"),
            Some(UIEventKind::Named("reset".to_string()))
        );
        assert_eq!(UIEventKind::from_name(""), None);
        assert_eq!(UIEventKind::from_name("  "), None);
        assert_eq!(UIEventKind::Submit.name(), "submit");
        assert_eq!(UIEventKind::Named("reset".to_string()).name(), "reset");
    }

    #[test]
    fn ui_event_field_lookup() {
        let event = UIEvent::new(UIEventKind::Submit)
            .with_field("query", Value::from("about".to_string()))
            .with_field("query", Value::from("second".to_string()));
        let value = event.field("query").expect("field present");
        assert_eq!(value.try_into_string().expect("text"), "about");
        assert!(event.field("missing").is_none());
    }
}
