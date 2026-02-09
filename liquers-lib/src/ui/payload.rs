use std::sync::Arc;

use liquers_core::commands::PayloadType;

use super::app_state::{AppState, DirectAppState};
use super::handle::UIHandle;

/// Trait that any payload can implement to provide UI context.
///
/// `app_state` returns the shared element tree. All mutation goes through the
/// `tokio::sync::Mutex`; the concrete state type is `dyn AppState`.
pub trait UIPayload: PayloadType {
    /// The currently focused UI element handle, if any.
    fn handle(&self) -> Option<UIHandle>;

    /// Shared application state containing the element tree.
    fn app_state(&self) -> Arc<tokio::sync::Mutex<dyn AppState>>;
}

/// Newtype for injecting AppState from context.
#[derive(Clone)]
pub struct AppStateRef(pub Arc<tokio::sync::Mutex<dyn AppState>>);

/// Minimal payload for applications that only need UI state.
/// Clone is cheap — it only clones the `Arc`.
#[derive(Clone)]
pub struct SimpleUIPayload {
    current_handle: Option<UIHandle>,
    app_state: Arc<tokio::sync::Mutex<dyn AppState>>,
}

impl SimpleUIPayload {
    pub fn new(app_state: Arc<tokio::sync::Mutex<dyn AppState>>) -> Self {
        Self {
            current_handle: None,
            app_state,
        }
    }

    /// Convenience: create from a DirectAppState.
    pub fn from_direct(state: DirectAppState) -> Self {
        Self::new(Arc::new(tokio::sync::Mutex::new(state)))
    }

    /// Return a new payload focused on a different element.
    /// Does not mutate the original.
    pub fn with_handle(mut self, handle: UIHandle) -> Self {
        self.current_handle = Some(handle);
        self
    }
}

impl PayloadType for SimpleUIPayload {}

impl UIPayload for SimpleUIPayload {
    fn handle(&self) -> Option<UIHandle> {
        self.current_handle
    }

    fn app_state(&self) -> Arc<tokio::sync::Mutex<dyn AppState>> {
        self.app_state.clone()
    }
}
