use std::collections::HashMap;
use std::sync::Arc;

use liquers_core::assets::AssetRef;
use liquers_core::context::{EnvRef, Environment};
use liquers_core::error::Error;
use liquers_core::state::State;

use crate::value::{ExtValueInterface, Value};

use super::app_state::AppState;
use super::element::{AssetViewElement, ElementSource};
use super::handle::UIHandle;
use super::message::{AppMessage, AppMessageReceiver, AppMessageSender};
use super::payload::{SimpleUIPayload, UIPayload};
use super::ui_context::UIContext;

/// Centralized message processing and non-blocking evaluation runner.
///
/// `AppRunner<E>` is generic over the `Environment` and owns:
/// - `envref` for starting evaluations
/// - `evaluating` map for in-flight evaluation tracking
/// - `message_rx` for receiving application messages
/// - `sender` for creating UIContext/payloads
///
/// AppState remains non-generic. AppRunner holds the generic parts
/// (EnvRef, AssetRef) that would otherwise create cyclic type dependencies.
pub struct AppRunner<E: Environment> {
    envref: EnvRef<E>,
    evaluating: HashMap<UIHandle, AssetRef<E>>,
    message_rx: AppMessageReceiver,
    sender: AppMessageSender,
}

impl<E> AppRunner<E>
where
    E: Environment<Value = Value>,
    E::Payload: UIPayload + From<SimpleUIPayload>,
{
    /// Create a new AppRunner.
    ///
    /// - `envref`: environment reference for starting evaluations
    /// - `message_rx`: receiver end of the application message channel
    /// - `sender`: sender end (cloned into UIContext for payload construction)
    pub fn new(
        envref: EnvRef<E>,
        message_rx: AppMessageReceiver,
        sender: AppMessageSender,
    ) -> Self {
        Self {
            envref,
            evaluating: HashMap::new(),
            message_rx,
            sender,
        }
    }

    /// Non-blocking run. Call every frame from the event loop.
    ///
    /// Does NOT hold the `app_state` lock across the entire call —
    /// locks/unlocks as needed for each phase.
    ///
    /// Three phases:
    /// 1. Drain messages (quick lock per message)
    /// 2. Auto-evaluate pending nodes
    /// 3. Poll evaluating nodes, transition to Ready/Error
    pub async fn run(
        &mut self,
        app_state: &Arc<tokio::sync::Mutex<dyn AppState>>,
    ) -> Result<(), Error> {
        // Phase 1: Process messages
        self.process_messages(app_state).await;

        // Phase 2: Auto-evaluate pending nodes
        self.evaluate_pending_nodes(app_state).await;

        // Phase 3: Poll evaluating nodes
        self.poll_evaluating_nodes(app_state).await;

        Ok(())
    }

    /// Phase 1: Drain all pending messages from the channel.
    ///
    /// SubmitQuery is evaluated inline via `evaluate_immediately` with a payload.
    /// Commands are expected to manipulate AppState themselves (e.g. `add` calls
    /// `app_state.set_element`), so we do NOT set element from the return value.
    /// On error, we set an `AssetViewElement::new_error`.
    async fn process_messages(
        &mut self,
        app_state: &Arc<tokio::sync::Mutex<dyn AppState>>,
    ) {
        while let Ok(msg) = self.message_rx.try_recv() {
            match msg {
                AppMessage::SubmitQuery { handle, query } => {
                    // 1. Set progress element (quick lock)
                    {
                        let mut state = app_state.lock().await;
                        let progress = Box::new(AssetViewElement::new_progress(query.clone()));
                        let _ = state.set_element(handle, progress);
                    }

                    // 2. Create payload with handle
                    let ui_context = UIContext::new(app_state.clone(), self.sender.clone())
                        .with_handle(Some(handle));
                    let payload: E::Payload = SimpleUIPayload::new(ui_context).into();

                    // 3. Evaluate inline — commands handle AppState themselves
                    match self.envref.evaluate_immediately(&query, payload).await {
                        Ok(asset_ref) => {
                            // Wait for completion — evaluate_immediately completes inline
                            let state = asset_ref.poll_state().await;
                            if let Some(s) = state {
                                if let Err(e) = s.error_result() {
                                    let mut app = app_state.lock().await;
                                    let _ = app.set_element(
                                        handle,
                                        Box::new(AssetViewElement::new_error(e)),
                                    );
                                }
                                // Do NOT set element from result — commands did it
                            }
                        }
                        Err(e) => {
                            // 4. On error: set error element
                            let mut state = app_state.lock().await;
                            let _ = state.set_element(
                                handle,
                                Box::new(AssetViewElement::new_error(e)),
                            );
                        }
                    }
                }
                AppMessage::Quit => {
                    // Delegate quit handling to the application layer.
                    // The app should check for this via a separate mechanism
                    // (e.g. egui viewport close command).
                    // We don't handle it here to avoid coupling to egui.
                }
                AppMessage::Serialize { path: _ } => {
                    // Serialization is application-specific.
                    // The app layer should handle this.
                }
                AppMessage::Deserialize { path: _ } => {
                    // Deserialization is application-specific.
                    // The app layer should handle this.
                }
            }
        }
    }

    /// Phase 2: Find pending nodes and start their evaluations.
    ///
    /// Pending nodes are evaluated via `evaluate` (no payload, async job queue).
    /// The runner tracks the `AssetRef` and sets the element from the result
    /// when polling completes in Phase 3.
    async fn evaluate_pending_nodes(
        &mut self,
        app_state: &Arc<tokio::sync::Mutex<dyn AppState>>,
    ) {
        // Collect pending nodes while holding the lock briefly
        let pending: Vec<(UIHandle, String)> = {
            let state = app_state.lock().await;
            state
                .pending_nodes()
                .into_iter()
                .filter(|h| !self.evaluating.contains_key(h))
                .filter_map(|h| {
                    match state.get_source(h) {
                        Ok(ElementSource::Query(q)) => Some((h, q.clone())),
                        Ok(ElementSource::None)
                        | Ok(ElementSource::Recipe(_))
                        | Err(_) => None,
                    }
                })
                .collect()
        }; // Lock released here

        for (handle, query) in pending {
            // 1. Set progress element (quick lock)
            {
                let mut state = app_state.lock().await;
                let progress = Box::new(AssetViewElement::new_progress(query.clone()));
                let _ = state.set_element(handle, progress);
            }

            // 2. Start async evaluation (no payload) — returns AssetRef immediately
            match self.envref.evaluate(&query).await {
                Ok(asset_ref) => {
                    // 3. Track for polling in Phase 3
                    self.evaluating.insert(handle, asset_ref);
                }
                Err(e) => {
                    let mut state = app_state.lock().await;
                    let _ = state.set_element(handle, Box::new(AssetViewElement::new_error(e)));
                }
            }
        }
    }

    /// Phase 3: Poll in-flight evaluations and transition completed ones.
    async fn poll_evaluating_nodes(
        &mut self,
        app_state: &Arc<tokio::sync::Mutex<dyn AppState>>,
    ) {
        let handles: Vec<UIHandle> = self.evaluating.keys().copied().collect();
        for handle in handles {
            if let Some(asset_ref) = self.evaluating.get(&handle) {
                if let Some(state) = asset_ref.try_poll_state() {
                    self.evaluating.remove(&handle);
                    let mut app = app_state.lock().await;
                    match state.error_result() {
                        Ok(()) => {
                            match value_to_element(&state) {
                                Ok(element) => {
                                    let _ = app.set_element(handle, element);
                                }
                                Err(e) => {
                                    let _ = app.set_element(
                                        handle,
                                        Box::new(AssetViewElement::new_error(e)),
                                    );
                                }
                            }
                        }
                        Err(e) => {
                            let _ = app.set_element(
                                handle,
                                Box::new(AssetViewElement::new_error(e)),
                            );
                        }
                    }
                }
            }
        }
    }

    /// Check the status of a specific element.
    pub fn element_status(&self, app_state: &dyn AppState, handle: UIHandle) -> ElementStatusInfo {
        if self.evaluating.contains_key(&handle) {
            ElementStatusInfo::Evaluating
        } else {
            match app_state.get_element(handle) {
                Ok(Some(_)) => ElementStatusInfo::Ready,
                Ok(None) => ElementStatusInfo::Pending,
                Err(_) => ElementStatusInfo::Error,
            }
        }
    }

    /// Check if there are any in-flight evaluations.
    pub fn has_evaluating(&self) -> bool {
        !self.evaluating.is_empty()
    }

    /// Number of currently evaluating nodes.
    pub fn evaluating_count(&self) -> usize {
        self.evaluating.len()
    }
}

/// Status information for a UI element.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ElementStatusInfo {
    /// Node exists but has no element and no evaluation in progress.
    Pending,
    /// Evaluation is in progress (AssetRef is being polled).
    Evaluating,
    /// Element is present and ready.
    Ready,
    /// Node not found or other error.
    Error,
}

/// Convert an evaluated State into a UIElement.
///
/// If the value is already a UIElement (ExtValue::UIElement variant),
/// clones it out. Otherwise wraps in an AssetViewElement in Value mode.
fn value_to_element(state: &State<Value>) -> Result<Box<dyn super::element::UIElement>, Error> {
    let value = &*state.data;

    // Check if value is already a UIElement
    if let Ok(ui_elem) = value.as_ui_element() {
        return Ok(ui_elem.clone_boxed());
    }

    // Extract title from metadata
    let title = state.metadata.title().to_string();
    let title = if title.is_empty() {
        "View".to_string()
    } else {
        title
    };

    // Wrap in AssetViewElement
    Ok(Box::new(AssetViewElement::new_value(
        title,
        Arc::new(value.clone()),
    )))
}
