use std::sync::Arc;

use liquers_core::commands::PayloadType;

use super::app_state::{AppState, DirectAppState};
use super::handle::UIHandle;
use super::message::AppMessageSender;
use super::ui_context::UIContext;

/// Trait that any payload can implement to provide UI context.
///
/// Phase 1a refactoring: UIPayload provides an interface to UIContext,
/// which is the primary holder of UI context state (app_state, sender,
/// current_handle). The trait provides default implementations for `handle()`
/// and `app_state()` that delegate to `ui_context()`.
pub trait UIPayload: PayloadType {
    /// Get a reference to the UIContext.
    fn ui_context(&self) -> &UIContext;

    /// The currently focused UI element handle, if any.
    /// Default implementation delegates to ui_context().
    fn handle(&self) -> Option<UIHandle> {
        self.ui_context().handle()
    }

    /// Shared application state containing the element tree.
    /// Default implementation delegates to ui_context().
    fn app_state(&self) -> Arc<tokio::sync::Mutex<dyn AppState>> {
        self.ui_context().app_state().clone()
    }
}

/// Newtype for injecting AppState from context.
#[derive(Clone)]
pub struct AppStateRef(pub Arc<tokio::sync::Mutex<dyn AppState>>);

/// Minimal payload for applications that only need UI state.
/// Clone is cheap — it only clones the inner UIContext.
///
/// Phase 1a refactoring: SimpleUIPayload holds a UIContext which contains
/// all UI state (app_state, sender, current_handle). This makes UIContext
/// the central holder of UI context and UIPayload a thin wrapper.
#[derive(Clone)]
pub struct SimpleUIPayload {
    ui_context: UIContext,
}

impl SimpleUIPayload {
    /// Create a new SimpleUIPayload from a UIContext.
    pub fn new(ui_context: UIContext) -> Self {
        Self { ui_context }
    }

    /// Convenience: create from app_state and sender.
    pub fn from_parts(
        app_state: Arc<tokio::sync::Mutex<dyn AppState>>,
        sender: AppMessageSender,
    ) -> Self {
        Self::new(UIContext::new(app_state, sender))
    }

    /// Convenience: create from a DirectAppState and sender.
    pub fn from_direct(state: DirectAppState, sender: AppMessageSender) -> Self {
        let app_state = Arc::new(tokio::sync::Mutex::new(state));
        Self::from_parts(app_state, sender)
    }

    /// Return a new payload focused on a different element.
    /// Does not mutate the original (builder pattern).
    pub fn with_handle(self, handle: UIHandle) -> Self {
        Self {
            ui_context: self.ui_context.with_handle(Some(handle)),
        }
    }
}

impl PayloadType for SimpleUIPayload {}

impl UIPayload for SimpleUIPayload {
    fn ui_context(&self) -> &UIContext {
        &self.ui_context
    }
}
