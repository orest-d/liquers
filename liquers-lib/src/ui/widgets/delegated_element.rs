//! An element whose behaviour is supplied by an integrated language.
//!
//! `typetag` tags are static, so a language cannot add a Rust element type at run time. Instead,
//! one Rust type, [`DelegatedElement`], stands for every element a language defines. It carries
//! only data — a `kind`, JSON `props` and a `title` — and looks up the behaviour for its `kind` in
//! a registry of [`ElementDelegate`]s kept per thread. Only the data is serialized, so a restored
//! tree renders a placeholder for a kind that is not (yet) defined, and renders properly once it
//! is.
//!
//! The browser integration (`liquers-web`) registers a delegate holding the page's `render` and
//! `onEvent` functions. See `specs/design/web-ui-define/`.

use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;

use serde::{Deserialize, Serialize};

use liquers_core::error::Error;

use crate::ui::element::{UIElement, UpdateMessage, UpdateResponse};
use crate::ui::event::UIEvent;
use crate::ui::handle::UIHandle;
use crate::ui::ui_context::UIContext;

/// Behaviour of one delegated kind. Called on the thread that registered it.
pub trait ElementDelegate {
    /// The markup of `element`, placed inside the node `render_web` wraps around it.
    fn render(&self, element: &DelegatedElement) -> Result<String, Error>;

    /// The element's new props after `event`, or `None` to keep them.
    fn on_event(
        &self,
        element: &DelegatedElement,
        event: &UIEvent,
    ) -> Result<Option<serde_json::Value>, Error>;
}

thread_local! {
    /// Delegates by kind. `delegate_for` clones the `Rc` out, so no borrow is held while a
    /// delegate (page code) runs.
    static DELEGATES: RefCell<HashMap<String, Rc<dyn ElementDelegate>>> =
        RefCell::new(HashMap::new());
}

/// Registers the delegate for `kind`, returning the one it replaces.
pub fn register_delegate(
    kind: &str,
    delegate: Rc<dyn ElementDelegate>,
) -> Option<Rc<dyn ElementDelegate>> {
    DELEGATES.with(|d| d.borrow_mut().insert(kind.to_string(), delegate))
}

/// Removes the delegate for `kind`, returning it.
pub fn unregister_delegate(kind: &str) -> Option<Rc<dyn ElementDelegate>> {
    DELEGATES.with(|d| d.borrow_mut().remove(kind))
}

/// The delegate for `kind` on this thread, if one is registered.
pub fn delegate_for(kind: &str) -> Option<Rc<dyn ElementDelegate>> {
    DELEGATES.with(|d| d.borrow().get(kind).cloned())
}

/// An element defined by an integrated language: a kind, JSON props and a title.
#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct DelegatedElement {
    /// The language-defined type, for example `"gauge"`.
    pub kind: String,
    /// The element's properties. Always JSON, so the element serializes and restores.
    pub props: serde_json::Value,
    /// The element's title.
    pub title: String,
    handle: Option<UIHandle>,
}

impl DelegatedElement {
    /// A new element of `kind` with `props`. Its title defaults to the kind.
    pub fn new(kind: impl Into<String>, props: serde_json::Value) -> Self {
        let kind = kind.into();
        DelegatedElement {
            title: kind.clone(),
            kind,
            props,
            handle: None,
        }
    }

    /// The same element with `title`.
    pub fn with_title(mut self, title: impl Into<String>) -> Self {
        self.title = title.into();
        self
    }

    /// The markup inside the wrapper: the delegate's, a placeholder naming an undefined kind, or
    /// an escaped error box when the delegate fails.
    #[cfg(feature = "webui")]
    fn inner_markup(&self) -> String {
        use crate::ui::web::html::escape_html;
        match delegate_for(&self.kind) {
            Some(delegate) => match delegate.render(self) {
                Ok(markup) => markup,
                Err(e) => format!(
                    "<div class=\"lq-error\">{}</div>",
                    escape_html(&format!(
                        "{} could not be rendered: {}",
                        self.kind, e.message
                    ))
                ),
            },
            None => format!(
                "<div class=\"lq-undefined\">{}</div>",
                escape_html(&format!("Element type \"{}\" is not defined", self.kind))
            ),
        }
    }
}

#[typetag::serde]
impl UIElement for DelegatedElement {
    fn type_name(&self) -> &'static str {
        "DelegatedElement"
    }

    fn handle(&self) -> Option<UIHandle> {
        self.handle
    }

    fn set_handle(&mut self, handle: UIHandle) {
        self.handle = Some(handle);
    }

    fn title(&self) -> String {
        self.title.clone()
    }

    fn set_title(&mut self, title: String) {
        self.title = title;
    }

    fn clone_boxed(&self) -> Box<dyn UIElement> {
        Box::new(self.clone())
    }

    fn as_any(&self) -> Option<&dyn std::any::Any> {
        Some(self)
    }

    fn init(&mut self, handle: UIHandle, _ctx: &UIContext) -> Result<(), Error> {
        self.set_handle(handle);
        Ok(())
    }

    fn update(&mut self, message: &UpdateMessage, _ctx: &UIContext) -> UpdateResponse {
        match message {
            UpdateMessage::UI(event) => {
                let delegate = match delegate_for(&self.kind) {
                    Some(d) => d,
                    None => return UpdateResponse::Unchanged,
                };
                match delegate.on_event(self, event) {
                    Ok(Some(props)) => {
                        self.props = props;
                        UpdateResponse::NeedsRepaint
                    }
                    Ok(None) => UpdateResponse::Unchanged,
                    // The delegate reports its own failure (the browser one to `console.error`);
                    // the props are kept.
                    Err(e) => {
                        eprintln!("{} event handler failed: {}", self.kind, e.message);
                        UpdateResponse::Unchanged
                    }
                }
            }
            UpdateMessage::AssetNotification(_) => UpdateResponse::Unchanged,
            UpdateMessage::Timer { .. } => UpdateResponse::Unchanged,
            UpdateMessage::Custom(_) => UpdateResponse::Unchanged,
            UpdateMessage::AssetUpdate(_) => UpdateResponse::Unchanged,
        }
    }

    #[cfg(feature = "egui")]
    fn show_in_egui(
        &mut self,
        ui: &mut egui::Ui,
        _ctx: &UIContext,
        _app_state: &mut dyn crate::ui::app_state::AppState,
    ) -> egui::Response {
        ui.label(format!(
            "\"{}\" is defined in another language and cannot be shown in egui",
            self.kind
        ))
    }

    #[cfg(feature = "webui")]
    fn render_web(&self, _app_state: &dyn crate::ui::app_state::AppState) -> String {
        format!(
            "<div id=\"{}\" class=\"lq-element lq-delegated\" data-lq-delegate=\"{}\">{}</div>",
            crate::ui::web::element_dom_id(self.handle()),
            crate::ui::web::html::escape_html(&self.kind),
            self.inner_markup()
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ui::event::UIEventKind;
    use serde_json::json;

    /// Renders `value` from the props and adds 1 on an `inc` event.
    struct Counter;

    impl ElementDelegate for Counter {
        fn render(&self, element: &DelegatedElement) -> Result<String, Error> {
            Ok(format!("<span>{}</span>", element.props["value"]))
        }
        fn on_event(
            &self,
            element: &DelegatedElement,
            event: &UIEvent,
        ) -> Result<Option<serde_json::Value>, Error> {
            match &event.kind {
                UIEventKind::Named(name) if name == "inc" => {
                    let v = element.props["value"].as_i64().unwrap_or(0);
                    Ok(Some(json!({ "value": v + 1 })))
                }
                UIEventKind::Named(_) | UIEventKind::Submit => Ok(None),
            }
        }
    }

    struct Failing;

    impl ElementDelegate for Failing {
        fn render(&self, _element: &DelegatedElement) -> Result<String, Error> {
            Err(Error::general_error("<boom>".to_string()))
        }
        fn on_event(
            &self,
            _element: &DelegatedElement,
            _event: &UIEvent,
        ) -> Result<Option<serde_json::Value>, Error> {
            Err(Error::general_error("no".to_string()))
        }
    }

    fn ctx() -> UIContext {
        let state: std::sync::Arc<tokio::sync::Mutex<dyn crate::ui::app_state::AppState>> =
            std::sync::Arc::new(tokio::sync::Mutex::new(
                crate::ui::app_state::DirectAppState::new(),
            ));
        let (sender, _receiver) = crate::ui::message::app_message_channel();
        UIContext::new(state, sender)
    }

    #[cfg(feature = "webui")]
    fn markup(element: &DelegatedElement) -> String {
        element.render_web(&crate::ui::app_state::DirectAppState::new())
    }

    #[cfg(feature = "webui")]
    #[test]
    fn delegated_renders_through_delegate() {
        register_delegate("counter_r", Rc::new(Counter));
        let mut element = DelegatedElement::new("counter_r", json!({ "value": 50 }));
        element.set_handle(UIHandle(7));
        let html = markup(&element);
        assert!(html.contains("<span>50</span>"), "{html}");
        assert!(html.contains("id=\"ui-element-7\""), "{html}");
        unregister_delegate("counter_r");
    }

    #[cfg(feature = "webui")]
    #[test]
    fn delegated_unknown_kind_renders_placeholder() {
        let element = DelegatedElement::new("never_defined", json!({}));
        let html = markup(&element);
        assert!(html.contains("lq-undefined"), "{html}");
        assert!(html.contains("never_defined"), "{html}");
        register_delegate("never_defined", Rc::new(Counter));
        assert!(markup(&element).contains("<span>null</span>"));
        unregister_delegate("never_defined");
    }

    #[cfg(feature = "webui")]
    #[test]
    fn delegated_markup_wraps_and_escapes() {
        register_delegate("fails<x>", Rc::new(Failing));
        let element = DelegatedElement::new("fails<x>", json!({}));
        let html = markup(&element);
        assert!(html.contains("class=\"lq-element lq-delegated\""), "{html}");
        assert!(
            html.contains("data-lq-delegate=\"fails&lt;x&gt;\""),
            "{html}"
        );
        assert!(html.contains("lq-error"), "{html}");
        assert!(
            !html.contains("<boom>"),
            "the error text is escaped: {html}"
        );
        unregister_delegate("fails<x>");
        let html = markup(&element);
        assert!(
            !html.contains("<x>"),
            "the placeholder escapes the kind: {html}"
        );
    }

    #[test]
    fn delegated_serde_roundtrip_keeps_kind_and_props() {
        let element: Box<dyn UIElement> = Box::new(
            DelegatedElement::new("gauge", json!({ "value": 3, "label": "x" })).with_title("G"),
        );
        let text = serde_json::to_string(&element).expect("serialize");
        let back: Box<dyn UIElement> = serde_json::from_str(&text).expect("deserialize");
        assert_eq!(back.type_name(), "DelegatedElement");
        assert_eq!(back.title(), "G");
        let again = serde_json::to_string(&back).expect("serialize again");
        assert_eq!(text, again, "kind and props survive unchanged");
    }

    #[test]
    fn delegated_user_event_replaces_props() {
        register_delegate("counter_e", Rc::new(Counter));
        let mut element = DelegatedElement::new("counter_e", json!({ "value": 1 }));
        let ctx = ctx();
        let inc = UpdateMessage::UI(UIEvent::new(UIEventKind::Named("inc".to_string())));
        assert_eq!(element.update(&inc, &ctx), UpdateResponse::NeedsRepaint);
        assert_eq!(element.props, json!({ "value": 2 }));
        let other = UpdateMessage::UI(UIEvent::new(UIEventKind::Submit));
        assert_eq!(element.update(&other, &ctx), UpdateResponse::Unchanged);
        assert_eq!(element.props, json!({ "value": 2 }));
        unregister_delegate("counter_e");

        register_delegate("failing_e", Rc::new(Failing));
        let mut failing = DelegatedElement::new("failing_e", json!({ "value": 1 }));
        assert_eq!(failing.update(&inc, &ctx), UpdateResponse::Unchanged);
        assert_eq!(
            failing.props,
            json!({ "value": 1 }),
            "props kept on failure"
        );
        unregister_delegate("failing_e");
    }
}
