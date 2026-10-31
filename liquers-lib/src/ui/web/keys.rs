//! Keyboard routing for the web driver, as a pure function so it is testable natively.
#![cfg_attr(not(target_arch = "wasm32"), allow(dead_code))]
//!
//! Precedence: a field's declared Enter action, then menu shortcuts (inside an editable field
//! only with Ctrl/Cmd or Alt held), then the browser's own behaviour. See
//! `specs/reference/UI_INTERFACE_FSD.md` §Event Handling.

use crate::ui::event::UIEventKind;
use crate::ui::shortcuts::KeyboardShortcut;

/// The parts of a `KeyboardEvent` the router needs.
pub(crate) struct KeyPress<'a> {
    pub key: &'a str,
    pub code: &'a str,
    /// `ctrlKey || metaKey` — the semantic command modifier.
    pub command: bool,
    pub alt: bool,
    pub shift: bool,
    /// `isComposing`: part of an IME composition.
    pub composing: bool,
}

/// The parts of the event target the router needs.
pub(crate) struct KeyTarget<'a> {
    /// `INPUT`, `TEXTAREA`, `SELECT` or a content-editable node.
    pub editable: bool,
    /// A single-line text input, where Enter has no native meaning beyond form submission.
    pub single_line: bool,
    /// The target's `data-lq-event`, if any.
    pub event: Option<&'a str>,
}

/// What the driver should do with a key press.
#[derive(Debug, PartialEq, Eq)]
pub(crate) enum KeyRoute {
    /// Send this UI event to the target's owning element.
    FieldEvent(UIEventKind),
    /// Run the rendered node whose `data-lq-shortcut` equals this shortcut.
    Shortcut(KeyboardShortcut),
    /// Leave the key to the browser; do not prevent its default.
    Native,
}

pub(crate) fn route_keydown(press: &KeyPress<'_>, target: &KeyTarget<'_>) -> KeyRoute {
    if press.composing {
        return KeyRoute::Native;
    }
    let plain = !press.command && !press.alt && !press.shift;
    if press.key == "Enter" && plain && target.editable && target.single_line {
        if let Some(kind) = target.event.and_then(UIEventKind::from_name) {
            return KeyRoute::FieldEvent(kind);
        }
    }
    if press.key == "Tab" {
        return KeyRoute::Native;
    }
    if target.editable && !press.command && !press.alt {
        return KeyRoute::Native;
    }
    match KeyboardShortcut::from_key_event(
        press.key,
        press.code,
        press.command,
        press.alt,
        press.shift,
    ) {
        Some(shortcut) => KeyRoute::Shortcut(shortcut),
        None => KeyRoute::Native,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn press<'a>(key: &'a str, code: &'a str) -> KeyPress<'a> {
        KeyPress {
            key,
            code,
            command: false,
            alt: false,
            shift: false,
            composing: false,
        }
    }

    const INPUT_WITH_SUBMIT: KeyTarget<'static> = KeyTarget {
        editable: true,
        single_line: true,
        event: Some("submit"),
    };
    const PLAIN_INPUT: KeyTarget<'static> = KeyTarget {
        editable: true,
        single_line: true,
        event: None,
    };
    const BODY: KeyTarget<'static> = KeyTarget {
        editable: false,
        single_line: false,
        event: None,
    };

    #[test]
    fn enter_in_declared_field_is_a_field_event() {
        assert_eq!(
            route_keydown(&press("Enter", "Enter"), &INPUT_WITH_SUBMIT),
            KeyRoute::FieldEvent(UIEventKind::Submit)
        );
    }

    #[test]
    fn enter_without_declaration_or_with_modifier_is_not_a_field_event() {
        assert_eq!(
            route_keydown(&press("Enter", "Enter"), &PLAIN_INPUT),
            KeyRoute::Native
        );
        let mut shift_enter = press("Enter", "Enter");
        shift_enter.shift = true;
        assert_eq!(
            route_keydown(&shift_enter, &INPUT_WITH_SUBMIT),
            KeyRoute::Native
        );
        let textarea = KeyTarget {
            editable: true,
            single_line: false,
            event: Some("submit"),
        };
        assert_eq!(
            route_keydown(&press("Enter", "Enter"), &textarea),
            KeyRoute::Native
        );
    }

    #[test]
    fn composition_and_tab_are_native() {
        let mut composing = press("Enter", "Enter");
        composing.composing = true;
        assert_eq!(
            route_keydown(&composing, &INPUT_WITH_SUBMIT),
            KeyRoute::Native
        );
        assert_eq!(route_keydown(&press("Tab", "Tab"), &BODY), KeyRoute::Native);
    }

    #[test]
    fn typing_in_a_field_is_native() {
        assert_eq!(
            route_keydown(&press("n", "KeyN"), &PLAIN_INPUT),
            KeyRoute::Native
        );
        let mut shift_n = press("N", "KeyN");
        shift_n.shift = true;
        assert_eq!(route_keydown(&shift_n, &PLAIN_INPUT), KeyRoute::Native);
    }

    #[test]
    fn modified_keys_are_shortcuts_even_in_a_field() {
        let mut alt_n = press("n", "KeyN");
        alt_n.alt = true;
        let expected = KeyboardShortcut::parse("Alt+N").expect("parse");
        assert_eq!(
            route_keydown(&alt_n, &PLAIN_INPUT),
            KeyRoute::Shortcut(expected.clone())
        );
        assert_eq!(route_keydown(&alt_n, &BODY), KeyRoute::Shortcut(expected));
    }

    #[test]
    fn unmodified_keys_outside_fields_are_shortcut_candidates() {
        assert_eq!(
            route_keydown(&press("F5", "F5"), &BODY),
            KeyRoute::Shortcut(KeyboardShortcut::parse("F5").expect("parse"))
        );
        assert_eq!(
            route_keydown(&press("Control", "ControlLeft"), &BODY),
            KeyRoute::Native
        );
    }
}
