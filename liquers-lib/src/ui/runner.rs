use std::collections::HashMap;
use std::sync::Arc;

use liquers_core::assets::{AssetNotificationMessage, AssetRef};
use liquers_core::context::{EnvRef, Environment};
use liquers_core::error::Error;
use liquers_core::metadata::Status;

use crate::value::{ExtValueInterface, Value};

use super::app_state::AppState;
use super::element::{AssetViewElement, ElementSource, UpdateMessage};
use super::handle::UIHandle;
use super::message::{AppMessage, AppMessageReceiver, AppMessageSender, AssetSnapshot};
use super::payload::{SimpleUIPayload, UIPayload};
use super::ui_context::UIContext;

/// Entry in the monitoring map. Tracks a monitored asset and its notification channel.
struct MonitoredAsset<E: Environment> {
    asset_ref: AssetRef<E>,
    notification_rx: tokio::sync::watch::Receiver<AssetNotificationMessage>,
}

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
    monitoring: HashMap<UIHandle, MonitoredAsset<E>>,
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
            monitoring: HashMap::new(),
            message_rx,
            sender,
        }
    }

    /// Set an element on a node and call init() with a UIContext.
    ///
    /// This is the correct way to place elements — set_element on AppState
    /// no longer calls init itself.
    fn set_element_and_init(
        state: &mut dyn AppState,
        handle: UIHandle,
        mut element: Box<dyn super::element::UIElement>,
        ctx: &UIContext,
    ) {
        let _ = element.init(handle, ctx);
        let _ = state.set_element(handle, element);
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

        // Phase 4: Poll monitored assets, push snapshots
        self.poll_monitored_assets(app_state).await;

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
            println!("AppRunner received message: {:?}", msg);
            match msg {
                AppMessage::SubmitQuery { handle, query } => {
                    let ui_context = UIContext::new(app_state.clone(), self.sender.clone())
                        .with_handle(handle);

                    // 1. Create payload with handle
                    let payload: E::Payload = SimpleUIPayload::new(ui_context.clone()).into();

                    // 2. Evaluate inline — commands handle AppState themselves
                    //    Do NOT set a progress element here: the command manipulates
                    //    AppState directly, and overwriting the element on `handle`
                    //    would destroy container elements (e.g. UISpecElement).
                    match self.envref.evaluate_immediately(&query, payload).await {
                        Ok(asset_ref) => {
                            // Wait for completion — evaluate_immediately completes inline
                            let state = asset_ref.poll_state().await;
                            if let Some(s) = state {
                                if let Err(e) = s.error_result() {
                                    if let Some(h) = handle {
                                        let mut app = app_state.lock().await;
                                        Self::set_element_and_init(
                                            &mut *app,
                                            h,
                                            Box::new(AssetViewElement::new_error(e)),
                                            &ui_context,
                                        );
                                    } else {
                                        eprintln!("Root query error (no handle): {}", e);
                                    }
                                }
                                // Do NOT set element from result — commands did it
                            }
                        }
                        Err(e) => {
                            // On error: set error element (if we have a handle)
                            if let Some(h) = handle {
                                let mut state = app_state.lock().await;
                                Self::set_element_and_init(
                                    &mut *state,
                                    h,
                                    Box::new(AssetViewElement::new_error(e)),
                                    &ui_context,
                                );
                            } else {
                                eprintln!("Root query error (no handle): {}", e);
                            }
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
                AppMessage::RequestAssetUpdates { handle, query } => {
                    self.handle_request_asset_updates(handle, query, app_state)
                        .await;
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
            let ui_context = UIContext::new(app_state.clone(), self.sender.clone())
                .with_handle(Some(handle));

            // 1. Set progress element (quick lock)
            {
                let mut state = app_state.lock().await;
                Self::set_element_and_init(
                    &mut *state,
                    handle,
                    Box::new(AssetViewElement::new_progress(query.clone())),
                    &ui_context,
                );
            }

            // 2. Start async evaluation (no payload) — returns AssetRef immediately
            match self.envref.evaluate(&query).await {
                Ok(asset_ref) => {
                    // 3. Track for polling in Phase 3
                    self.evaluating.insert(handle, asset_ref);
                }
                Err(e) => {
                    let mut state = app_state.lock().await;
                    Self::set_element_and_init(
                        &mut *state,
                        handle,
                        Box::new(AssetViewElement::new_error(e)),
                        &ui_context,
                    );
                }
            }
        }
    }

    /// Phase 3: Poll in-flight evaluations and transition completed ones.
    ///
    /// When a value is not a UIElement, creates an `AssetViewElement::from_asset_ref`
    /// to give the element its own notification channel for future updates.
    async fn poll_evaluating_nodes(
        &mut self,
        app_state: &Arc<tokio::sync::Mutex<dyn AppState>>,
    ) {
        // Collect handles of completed evaluations
        let completed: Vec<UIHandle> = self
            .evaluating
            .iter()
            .filter_map(|(handle, asset_ref)| {
                if asset_ref.try_poll_state().is_some() {
                    Some(*handle)
                } else {
                    None
                }
            })
            .collect();

        for handle in completed {
            let asset_ref = match self.evaluating.remove(&handle) {
                Some(ar) => ar,
                None => continue,
            };
            let ui_context = UIContext::new(app_state.clone(), self.sender.clone())
                .with_handle(Some(handle));

            // Poll state (already confirmed available above)
            let state = match asset_ref.poll_state().await {
                Some(s) => s,
                None => continue,
            };

            let mut app = app_state.lock().await;
            match state.error_result() {
                Ok(()) => {
                    // Check if value is already a UIElement
                    if let Ok(ui_elem) = state.data.as_ui_element() {
                        Self::set_element_and_init(
                            &mut *app,
                            handle,
                            ui_elem.clone_boxed(),
                            &ui_context,
                        );
                    } else {
                        // Create AssetViewElement with live notification channel
                        let title = {
                            let t = state.metadata.title().to_string();
                            if t.is_empty() { "View".to_string() } else { t }
                        };
                        // Drop lock before async call
                        drop(app);
                        let element = AssetViewElement::from_asset_ref::<E>(title, asset_ref).await;
                        let mut app = app_state.lock().await;
                        Self::set_element_and_init(
                            &mut *app,
                            handle,
                            Box::new(element),
                            &ui_context,
                        );
                    }
                }
                Err(e) => {
                    Self::set_element_and_init(
                        &mut *app,
                        handle,
                        Box::new(AssetViewElement::new_error(e)),
                        &ui_context,
                    );
                }
            }
        }
    }

    /// Handle RequestAssetUpdates: evaluate query, subscribe to notifications,
    /// build initial snapshot, deliver to widget, store in monitoring map.
    async fn handle_request_asset_updates(
        &mut self,
        handle: UIHandle,
        query: String,
        app_state: &Arc<tokio::sync::Mutex<dyn AppState>>,
    ) {
        // 1. Evaluate the query (async, returns AssetRef)
        match self.envref.evaluate(&query).await {
            Ok(asset_ref) => {
                // 2. Subscribe to notifications
                let notification_rx = asset_ref.subscribe_to_notifications().await;

                // 3. Build initial snapshot
                let snapshot = Self::build_snapshot(&asset_ref).await;

                // 4. Deliver snapshot to element
                let delivered = Self::deliver_snapshot(
                    handle,
                    snapshot,
                    app_state,
                    &self.sender,
                )
                .await;

                // 5. Store in monitoring map (replaces existing if any)
                if delivered {
                    self.monitoring.insert(
                        handle,
                        MonitoredAsset {
                            asset_ref,
                            notification_rx,
                        },
                    );
                }
                // If element doesn't exist, don't start monitoring
            }
            Err(e) => {
                // Build error snapshot and deliver
                let error_snapshot = AssetSnapshot {
                    value: None,
                    metadata: liquers_core::metadata::Metadata::new(),
                    error: Some(e),
                    status: Status::Error,
                };
                Self::deliver_snapshot(handle, error_snapshot, app_state, &self.sender).await;
            }
        }
    }

    /// Phase 4: Poll all monitored assets for notification changes.
    /// Build and deliver AssetSnapshot on change.
    /// Remove entries where the element no longer exists (auto-stop).
    async fn poll_monitored_assets(
        &mut self,
        app_state: &Arc<tokio::sync::Mutex<dyn AppState>>,
    ) {
        let mut to_remove = Vec::new();

        for (handle, monitored) in self.monitoring.iter_mut() {
            // Check if notification has changed (non-blocking)
            if monitored.notification_rx.has_changed().unwrap_or(false) {
                // Acknowledge the change
                monitored.notification_rx.borrow_and_update();

                // Build fresh snapshot
                let snapshot = Self::build_snapshot(&monitored.asset_ref).await;

                // Deliver to element
                let delivered =
                    Self::deliver_snapshot(*handle, snapshot, app_state, &self.sender).await;
                if !delivered {
                    to_remove.push(*handle);
                }
            } else {
                // Even without notification changes, check if element still exists
                let state = app_state.lock().await;
                if !state.node_exists(*handle) {
                    to_remove.push(*handle);
                }
            }
        }

        // Remove entries for elements that no longer exist (auto-stop)
        for handle in to_remove {
            self.monitoring.remove(&handle);
        }
    }

    /// Build an AssetSnapshot from an AssetRef.
    async fn build_snapshot(asset_ref: &AssetRef<E>) -> AssetSnapshot {
        // Get status
        let status = asset_ref.status().await;

        // Try to get the state (non-blocking first, then blocking)
        let state = asset_ref.try_poll_state();

        let (value, error) = match &state {
            Some(s) => {
                let val = Some(s.data.clone());
                let err = s.error_result().err();
                (val, err)
            }
            None => (None, None),
        };

        // Get metadata (always available)
        let metadata = asset_ref
            .get_metadata()
            .await
            .unwrap_or_else(|_| liquers_core::metadata::Metadata::new());

        AssetSnapshot {
            value,
            metadata,
            error,
            status,
        }
    }

    /// Deliver an AssetSnapshot to an element via update().
    /// Uses the extract-update-replace pattern to avoid holding the AppState lock
    /// while calling update() (which requires a UIContext that references AppState).
    /// Returns false if element no longer exists (caller should remove from monitoring).
    async fn deliver_snapshot(
        handle: UIHandle,
        snapshot: AssetSnapshot,
        app_state: &Arc<tokio::sync::Mutex<dyn AppState>>,
        sender: &AppMessageSender,
    ) -> bool {
        // Extract element from AppState (take_element errors if not found)
        let mut element = {
            let mut state = app_state.lock().await;
            match state.take_element(handle) {
                Ok(elem) => elem,
                Err(_) => return false,
            }
        }; // Lock released

        // Create UIContext and call update (lock not held)
        let ctx = UIContext::new(app_state.clone(), sender.clone())
            .with_handle(Some(handle));
        let _response = element.update(&UpdateMessage::AssetUpdate(snapshot), &ctx);

        // Put element back
        {
            let mut state = app_state.lock().await;
            let _ = state.put_element(handle, element);
        }

        true
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

