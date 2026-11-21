//! The message and evaluation loop of a Liquers UI: one pass per [`AppRunner::run`].
//!
//! A UI's elements live in an [`AppState`]; messages (`AppMessage`) and element sources change it,
//! and the runner turns those changes into evaluations. Each `run` drains the message channel,
//! starts an evaluation for every pending node, and moves finished evaluations into their elements.
//! It never blocks on an evaluation, and it leaves scheduling, repainting and quitting to its host.
//!
//! # Main components
//!
//! - [`AppRunner`]: the loop. [`AppRunner::run`] does one pass; [`AppRunner::take_events`],
//!   [`AppRunner::take_quit_request`] and [`AppRunner::has_pending_messages`] tell the host what it
//!   did and what is left.
//! - [`RunnerEvent`]: a status change of one element (`Processing`, `Ready`, `Error`), in order.
//! - [`ElementStatusInfo`]: an element's status as the runner sees it, for display.
//!
//! # Where it is used
//!
//! - `crate::ui::native` `NativeDriver`, which owns a runner for the egui window and for headless
//!   hosts such as `liquers-py`'s `lq.UI`.
//! - `crate::ui::web`, the browser mount (wasm), which schedules `run` with the browser's timers.
//! - The egui examples in `liquers-lib/examples/` that still drive it by hand.
//!
//! # Related concepts
//!
//! - [`AppState`] and `UIChange`: the element tree the runner evaluates into.
//! - `UIContext`: how commands and widgets send messages to the runner.
//! - Stale results: an evaluation whose element changed source meanwhile is dropped, and the
//!   element is evaluated again (`specs/reference/UI_INTERFACE_FSD.md`).
//!
//! # Examples
//!
//! ```
//! # use std::sync::Arc;
//! # use liquers_core::context::Environment;
//! # use liquers_lib::environment::DefaultEnvironment;
//! # use liquers_lib::ui::{app_message_channel, AppRunner, AppState, DirectAppState, SimpleUIPayload, UIContext};
//! # use liquers_lib::value::Value;
//! # let rt = tokio::runtime::Runtime::new().unwrap();
//! # rt.block_on(async {
//! let envref = DefaultEnvironment::<Value, SimpleUIPayload>::new().to_ref();
//! let app_state: Arc<tokio::sync::Mutex<dyn AppState>> =
//!     Arc::new(tokio::sync::Mutex::new(DirectAppState::new()));
//! let (sender, receiver) = app_message_channel();
//! let mut runner = AppRunner::new(envref, receiver, sender.clone());
//! let _ = sender.send(liquers_lib::ui::AppMessage::Quit);
//! assert!(runner.has_pending_messages());
//! runner.run(&app_state).await.unwrap();
//! assert!(runner.take_quit_request());
//! assert!(!runner.has_pending_messages());
//! # });
//! ```
//!
//! More: `crate::ui::native` (a runner ticked in the background or by a window) and the tests at
//! the end of this file.

use std::collections::HashMap;
use std::sync::Arc;

use liquers_core::accessor::{memory_accessor, AccessorEvent, SetOptions, ValueAccessor};
use liquers_core::command_metadata::CommandMetadataRegistry;
use liquers_core::assets::{AssetManager, AssetNotificationMessage, AssetRef};
use liquers_core::context::{EnvRef, Environment};
use liquers_core::error::{Error, ErrorType};
use liquers_core::metadata::{Metadata, Status};
use liquers_core::parse::{parse_key, parse_query};
use liquers_core::recipes::Recipe;
use liquers_core::state::State;
use liquers_core::value::ValueInterface;

use crate::value::{ExtValueInterface, Value};

use super::app_state::{AppState, UIChange};
use super::element::{AssetViewElement, ElementSource, UpdateMessage, UpdateResponse};
use super::handle::UIHandle;
use super::editor::{DirectoryEntry, DirectoryListing, EditorBinding};
use super::message::{
    AccessorOp, AccessorUpdate, AppMessage, AppMessageReceiver, AppMessageSender, AssetSnapshot,
};
use super::payload::{SimpleUIPayload, UIPayload};
use super::theme::{IconCache, IconLoad, Theme, ThemeCell, ThemeLayer, ThemeSource, UiConfig};
use super::ui_context::UIContext;

/// Entry in the monitoring map. Tracks a monitored asset and its notification channel.
struct MonitoredAsset<E: Environment> {
    asset_ref: AssetRef<E>,
    notification_rx: tokio::sync::watch::Receiver<AssetNotificationMessage>,
}

/// A status change of one element, recorded by the runner and read by the backend after `run`.
///
/// A backend that shows status to its user (the browser mount's `status` subscription) drains these
/// with [`AppRunner::take_events`] after each `run`. They are plain data, recorded in order.
#[derive(Debug, Clone)]
pub struct RunnerEvent {
    /// The element, or `None` for a root query with no element to show its error.
    pub handle: Option<UIHandle>,
    /// `Processing` when an evaluation starts, `Ready` or `Error` when it ends, or a monitored
    /// asset's status.
    pub status: Status,
    /// The error, when `status` is `Error`.
    pub error: Option<Error>,
}

/// How many undrained events the runner keeps. A backend that never calls `take_events` (the egui
/// examples) loses the oldest instead of growing without bound.
const MAX_PENDING_EVENTS: usize = 1024;

/// An editor's accessor (`AppMessage::Accessor`), with its event receiver.
struct WatchedAccessor {
    accessor: Arc<dyn ValueAccessor<Value>>,
    events: tokio::sync::watch::Receiver<AccessorEvent>,
    /// The source pushes no events: the runner calls `refresh` each tick instead.
    pull: bool,
}

/// What every `UIContext` the runner builds carries besides the app state.
struct ContextParts {
    sender: AppMessageSender,
    registry: Arc<CommandMetadataRegistry>,
    theme: ThemeCell,
    icons: IconCache,
}

impl ContextParts {
    fn context(&self, app_state: &Arc<tokio::sync::Mutex<dyn AppState>>) -> UIContext {
        UIContext::new(app_state.clone(), self.sender.clone())
            .with_command_registry(self.registry.clone())
            .with_theme(self.theme.clone())
            .with_icon_cache(self.icons.clone())
    }
}

/// A theme layer from an asset value: YAML text, or an already structured value.
fn theme_layer_of(value: &Value) -> Result<ThemeLayer, Error> {
    match value.try_into_json_value() {
        Ok(serde_json::Value::String(text)) => ThemeLayer::from_yaml(&text),
        Ok(other) => ThemeLayer::from_value(other),
        Err(_) => ThemeLayer::from_yaml(&value.try_into_string()?),
    }
}

/// Result of pushing an update at an element.
enum DeliveryOutcome {
    /// The element no longer exists — the caller stops monitoring it.
    Missing,
    /// Delivered; the element's own repaint verdict has already been recorded as a change.
    Delivered,
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
    /// In-flight evaluations, with the query text each one started, so a node whose source changed
    /// meanwhile is evaluated again instead of receiving the stale result.
    evaluating: HashMap<UIHandle, (AssetRef<E>, String)>,
    monitoring: HashMap<UIHandle, MonitoredAsset<E>>,
    /// The accessors of editor elements, by handle; removed with the element.
    accessors: HashMap<UIHandle, WatchedAccessor>,
    message_rx: AppMessageReceiver,
    sender: AppMessageSender,
    events: Vec<RunnerEvent>,
    quit_requested: bool,
    /// One snapshot of the command metadata registry, given to every `UIContext` this runner
    /// builds (the editors read their widgets from it).
    registry: Arc<CommandMetadataRegistry>,
    /// The theme every `UIContext` of this runner reads (AC-16), and the icon images loaded by
    /// key (AC-17).
    theme: ThemeCell,
    icons: IconCache,
    /// A theme given as a key or query: to load on the next `run`, then watched.
    theme_query: Option<String>,
    theme_watch: Option<(String, MonitoredAsset<E>)>,
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
        let registry = envref.command_metadata_snapshot();
        Self {
            envref,
            registry,
            evaluating: HashMap::new(),
            monitoring: HashMap::new(),
            accessors: HashMap::new(),
            message_rx,
            sender,
            events: Vec::new(),
            quit_requested: false,
            theme: ThemeCell::default(),
            icons: IconCache::default(),
            theme_query: None,
            theme_watch: None,
        }
    }

    /// Reads the theme from `cell` (shared with the host) and, when `ui.theme` is a key or query,
    /// loads it on the next `run` and reloads it whenever it changes (AC-16).
    pub fn with_ui_config(mut self, ui: &UiConfig, cell: ThemeCell) -> Self {
        self.set_ui_config(ui, cell);
        self
    }

    /// [`AppRunner::with_ui_config`] in place, for a host that owns the runner already.
    pub fn set_ui_config(&mut self, ui: &UiConfig, cell: ThemeCell) {
        self.theme = cell;
        self.theme_query = match &ui.theme {
            Some(ThemeSource::Query(q)) => Some(q.clone()),
            Some(ThemeSource::Builtin(_)) | None => None,
        };
        self.theme_watch = None;
    }

    /// Shares `cache` as the icon images this runner loads (builder style), for a host that
    /// builds its own `UIContext`.
    pub fn with_icon_cache(mut self, cache: IconCache) -> Self {
        self.icons = cache;
        self
    }

    /// The theme cell this runner gives to every `UIContext`.
    pub fn theme_cell(&self) -> &ThemeCell {
        &self.theme
    }

    /// The icon images this runner has loaded by key.
    pub fn icon_cache(&self) -> &IconCache {
        &self.icons
    }

    fn context_parts(&self) -> ContextParts {
        ContextParts {
            sender: self.sender.clone(),
            registry: self.registry.clone(),
            theme: self.theme.clone(),
            icons: self.icons.clone(),
        }
    }

    /// Uses `envref` for every evaluation started from now on. Evaluations already in flight keep
    /// the environment they started in.
    pub fn replace_envref(&mut self, envref: EnvRef<E>) {
        self.registry = envref.command_metadata_snapshot();
        self.envref = envref;
    }

    /// The command metadata registry snapshot this runner gives to every `UIContext`.
    pub fn command_registry(&self) -> &Arc<CommandMetadataRegistry> {
        &self.registry
    }

    /// A context over `app_state` and this runner's channel, carrying the registry snapshot.
    fn context(&self, app_state: &Arc<tokio::sync::Mutex<dyn AppState>>) -> UIContext {
        self.context_parts().context(app_state)
    }

    /// Whether an `AppMessage::Quit` arrived since the last call; clears the request.
    ///
    /// The runner does not close anything itself: the host (a native window, the browser
    /// mount, a headless loop) decides what quitting means.
    pub fn take_quit_request(&mut self) -> bool {
        std::mem::take(&mut self.quit_requested)
    }

    /// Whether messages are waiting in the channel for the next `run`.
    pub fn has_pending_messages(&self) -> bool {
        !self.message_rx.is_empty()
    }

    /// Drains the status events recorded since the last call, in recording order.
    pub fn take_events(&mut self) -> Vec<RunnerEvent> {
        std::mem::take(&mut self.events)
    }

    fn record(&mut self, handle: Option<UIHandle>, status: Status, error: Option<Error>) {
        if self.events.len() >= MAX_PENDING_EVENTS {
            self.events.remove(0);
        }
        self.events.push(RunnerEvent {
            handle,
            status,
            error,
        });
    }

    /// Whether `handle`'s source is still the query `text`.
    fn source_is(state: &dyn AppState, handle: UIHandle, text: &str) -> bool {
        match state.get_source(handle) {
            Ok(ElementSource::Query(q)) => q == text,
            Ok(ElementSource::None) | Ok(ElementSource::Recipe(_)) | Err(_) => false,
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

        // Phase 5: Poll editor accessors, push their changes (boxed: the wasm stack is small).
        Box::pin(self.poll_accessors(app_state)).await;

        // Phase 6: Load or reload the theme given as a key or query.
        Box::pin(self.poll_theme()).await;

        Ok(())
    }

    /// Phase 1: Drain all pending messages from the channel.
    ///
    /// SubmitQuery is evaluated inline via `evaluate_immediately` with a payload.
    /// Commands are expected to manipulate AppState themselves (e.g. `add` calls
    /// `app_state.set_element`), so we do NOT set element from the return value.
    /// On error, we set an `AssetViewElement::new_error`.
    async fn process_messages(&mut self, app_state: &Arc<tokio::sync::Mutex<dyn AppState>>) {
        while let Ok(msg) = self.message_rx.try_recv() {
            match msg {
                AppMessage::SubmitQuery { handle, query } => {
                    let ui_context =
                        self.context(app_state).with_handle(handle);

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
                                    self.record(handle, Status::Error, Some(e.clone()));
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
                            self.record(handle, Status::Error, Some(e.clone()));
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
                AppMessage::UIEvent { handle, event } => {
                    // An event for an element removed meanwhile is dropped.
                    let _ = Self::deliver_update(
                        handle,
                        UpdateMessage::UI(event),
                        app_state,
                        &self.context_parts(),
                    )
                    .await;
                }
                AppMessage::LoadIcon { key } => {
                    Box::pin(self.handle_load_icon(key)).await;
                }
                AppMessage::Quit => {
                    // Recorded for the host, which owns the window (or the loop) to close:
                    // read it with `take_quit_request` after `run`.
                    self.quit_requested = true;
                }
                AppMessage::Serialize { path: _ } => {
                    // Serialization is application-specific.
                    // The app layer should handle this.
                }
                AppMessage::Deserialize { path: _ } => {
                    // Deserialization is application-specific.
                    // The app layer should handle this.
                }
                AppMessage::Accessor {
                    handle,
                    binding,
                    op,
                } => {
                    Box::pin(self.handle_accessor(handle, binding, op, app_state)).await;
                }
                AppMessage::ListDirectory { handle, folder } => {
                    Box::pin(self.handle_list_directory(handle, folder, app_state)).await;
                }
                AppMessage::RequestAssetUpdates { handle, query } => {
                    self.handle_request_asset_updates(handle, query, app_state)
                        .await;
                }
                AppMessage::ApplyToInput {
                    handle,
                    input,
                    query,
                } => {
                    self.handle_apply_to_input(handle, input, query, app_state)
                        .await;
                }
            }
        }
    }

    /// Handle `ApplyToInput`: build an input state from `input`, apply `query` to it with a
    /// UI payload bound to `handle`, and run it inline. Commands (e.g. `lui/submit`) perform
    /// their own AppState side effects; on error an error element is set at `handle`.
    async fn handle_apply_to_input(
        &mut self,
        handle: UIHandle,
        input: String,
        query: String,
        app_state: &Arc<tokio::sync::Mutex<dyn AppState>>,
    ) {
        let ui_context =
            self.context(app_state).with_handle(Some(handle));
        let payload: E::Payload = SimpleUIPayload::new(ui_context.clone()).into();

        let input_state = State::from_parts(
            Arc::new(Value::from(input.as_str())),
            Arc::new(Metadata::new()),
        );

        let result = match parse_query(&query) {
            Ok(q) => {
                self.envref
                    .get_asset_manager()
                    .apply(Recipe::from(q), input_state, Some(payload))
                    .await
            }
            Err(e) => Err(e),
        };

        match result {
            Ok(asset_ref) => {
                if let Some(s) = asset_ref.poll_state().await {
                    if let Err(e) = s.error_result() {
                        self.record(Some(handle), Status::Error, Some(e.clone()));
                        let mut app = app_state.lock().await;
                        Self::set_element_and_init(
                            &mut *app,
                            handle,
                            Box::new(AssetViewElement::new_error(e)),
                            &ui_context,
                        );
                    }
                    // On success, commands handled AppState themselves — do not overwrite.
                }
            }
            Err(e) => {
                self.record(Some(handle), Status::Error, Some(e.clone()));
                let mut app = app_state.lock().await;
                Self::set_element_and_init(
                    &mut *app,
                    handle,
                    Box::new(AssetViewElement::new_error(e)),
                    &ui_context,
                );
            }
        }
    }

    /// Phase 2: Find pending nodes and start their evaluations.
    ///
    /// Pending nodes are evaluated via `evaluate` (no payload, async job queue).
    /// The runner tracks the `AssetRef` and sets the element from the result
    /// when polling completes in Phase 3.
    async fn evaluate_pending_nodes(&mut self, app_state: &Arc<tokio::sync::Mutex<dyn AppState>>) {
        // Collect pending nodes while holding the lock briefly
        let pending: Vec<(UIHandle, String)> = {
            let state = app_state.lock().await;
            state
                .pending_nodes()
                .into_iter()
                .filter_map(|h| match state.get_source(h) {
                    Ok(ElementSource::Query(q)) => Some((h, q.clone())),
                    Ok(ElementSource::None) | Ok(ElementSource::Recipe(_)) | Err(_) => None,
                })
                // An evaluation of the same text is already in flight; one of another text is
                // stale, and is replaced below.
                .filter(|(h, q)| match self.evaluating.get(h) {
                    Some((_, started)) => started != q,
                    None => true,
                })
                .collect()
        }; // Lock released here

        for (handle, query) in pending {
            let ui_context =
                self.context(app_state).with_handle(Some(handle));

            self.evaluating.remove(&handle);

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
            self.record(Some(handle), Status::Processing, None);

            // 2. Start async evaluation (no payload). On wasm this awaits the whole evaluation,
            //    during which the node's source may be replaced.
            let started = self.envref.evaluate(&query).await;
            let current = {
                let state = app_state.lock().await;
                Self::source_is(&*state, handle, &query)
            };
            if !current {
                // The node now shows something else; it is pending again and the next tick
                // evaluates its new source. Nothing is recorded for the stale query.
                continue;
            }
            match started {
                Ok(asset_ref) => {
                    // 3. Track for polling in Phase 3
                    self.evaluating.insert(handle, (asset_ref, query));
                }
                Err(e) => {
                    self.record(Some(handle), Status::Error, Some(e.clone()));
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
    async fn poll_evaluating_nodes(&mut self, app_state: &Arc<tokio::sync::Mutex<dyn AppState>>) {
        // Collect handles of completed evaluations
        let completed: Vec<UIHandle> = self
            .evaluating
            .iter()
            .filter_map(|(handle, (asset_ref, _))| {
                if asset_ref.try_poll_state().is_some() {
                    Some(*handle)
                } else {
                    None
                }
            })
            .collect();

        for handle in completed {
            let (asset_ref, query) = match self.evaluating.remove(&handle) {
                Some(entry) => entry,
                None => continue,
            };
            let ui_context =
                self.context(app_state).with_handle(Some(handle));

            // Poll state (already confirmed available above)
            let state = match asset_ref.poll_state().await {
                Some(s) => s,
                None => continue,
            };

            let mut app = app_state.lock().await;
            if !Self::source_is(&*app, handle, &query) {
                // The source changed while the query ran: drop the stale result.
                continue;
            }
            match state.error_result() {
                Ok(()) => {
                    self.record(Some(handle), Status::Ready, None);
                    // Check if value is already a UIElement
                    if let Ok(ui_elem) = state.data_unchecked().as_ui_element() {
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
                            if t.is_empty() {
                                "View".to_string()
                            } else {
                                t
                            }
                        };
                        // Drop lock before async call
                        drop(app);
                        let element = AssetViewElement::from_asset_ref::<E>(title, asset_ref).await;
                        let mut app = app_state.lock().await;
                        if !Self::source_is(&*app, handle, &query) {
                            continue;
                        }
                        Self::set_element_and_init(
                            &mut *app,
                            handle,
                            Box::new(element),
                            &ui_context,
                        );
                    }
                }
                Err(e) => {
                    self.record(Some(handle), Status::Error, Some(e.clone()));
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

    /// Binds the accessor of `binding`: evaluates an accessor query and takes the accessor it
    /// gives, or keeps a memory binding's value in a memory accessor.
    async fn bind_accessor(&self, binding: &EditorBinding) -> Result<Arc<dyn ValueAccessor<Value>>, Error> {
        match binding {
            EditorBinding::Accessor(query) => {
                let query = parse_query(query)?;
                let state = self.envref.evaluate(&query).await?.get().await?;
                state.value()?.try_into_accessor()
            }
            EditorBinding::Memory(text) => Ok(memory_accessor(
                Some(State::new().with_data(Value::from(text.clone()))),
                liquers_core::query::Query::new(),
            )),
        }
    }

    async fn deliver_accessor(
        &self,
        handle: UIHandle,
        update: AccessorUpdate,
        app_state: &Arc<tokio::sync::Mutex<dyn AppState>>,
    ) -> DeliveryOutcome {
        Self::deliver_update(
            handle,
            UpdateMessage::Accessor(update),
            app_state,
            &self.context_parts(),
        )
        .await
    }

    /// The accessor's current value, as an update.
    async fn read_accessor(accessor: &Arc<dyn ValueAccessor<Value>>) -> AccessorUpdate {
        match accessor.get().await {
            Ok(snapshot) => AccessorUpdate::Snapshot {
                value: snapshot.state.data_unchecked().clone(),
                fingerprint: Some(snapshot.fingerprint),
            },
            Err(e) => AccessorUpdate::Failed(e),
        }
    }

    /// Handle `AppMessage::Accessor`: bind, read or write an editor's accessor.
    async fn handle_accessor(
        &mut self,
        handle: UIHandle,
        binding: EditorBinding,
        op: AccessorOp,
        app_state: &Arc<tokio::sync::Mutex<dyn AppState>>,
    ) {
        let rebind = matches!(op, AccessorOp::Watch | AccessorOp::Adopt(_))
            || !self.accessors.contains_key(&handle);
        if rebind {
            let bound = match &op {
                AccessorOp::Adopt(accessor) => Ok(accessor.clone()),
                AccessorOp::Watch | AccessorOp::Get | AccessorOp::SetIf { .. } => {
                    self.bind_accessor(&binding).await
                }
            };
            match bound {
                Ok(accessor) => {
                    if let Some(driver) = accessor.driver() {
                        super::spawn_ui_task(driver);
                    }
                    let mut events = accessor.subscribe();
                    events.borrow_and_update();
                    let capabilities = accessor.capabilities();
                    let bound = AccessorUpdate::Bound {
                        address: accessor.address().encode(),
                        writable: capabilities.write,
                    };
                    self.accessors.insert(
                        handle,
                        WatchedAccessor {
                            accessor: accessor.clone(),
                            events,
                            pull: cfg!(target_arch = "wasm32") || !capabilities.notify,
                        },
                    );
                    if let DeliveryOutcome::Missing = self.deliver_accessor(handle, bound, app_state).await {
                        self.accessors.remove(&handle);
                        return;
                    }
                }
                Err(e) => {
                    self.record(Some(handle), Status::Error, Some(e.clone()));
                    let _ = self.deliver_accessor(handle, AccessorUpdate::Failed(e), app_state).await;
                    return;
                }
            }
        }
        let Some(accessor) = self.accessors.get(&handle).map(|w| w.accessor.clone()) else {
            return;
        };
        let update = match op {
            AccessorOp::Watch | AccessorOp::Adopt(_) | AccessorOp::Get => {
                Self::read_accessor(&accessor).await
            }
            AccessorOp::SetIf { value, expected } => {
                match accessor.set_if(value, expected, SetOptions::default()).await {
                    Ok(snapshot) => {
                        // The writer's own change event is not news to it.
                        if let Some(w) = self.accessors.get_mut(&handle) {
                            w.events.borrow_and_update();
                        }
                        AccessorUpdate::Written {
                            fingerprint: snapshot.fingerprint,
                        }
                    }
                    Err(e) if e.error_type == ErrorType::StatusConflict => match accessor.get().await {
                        Ok(live) => AccessorUpdate::Conflict {
                            value: live.state.data_unchecked().clone(),
                            fingerprint: Some(live.fingerprint),
                        },
                        Err(_) => AccessorUpdate::Failed(e),
                    },
                    Err(e) => AccessorUpdate::Failed(e),
                }
            }
        };
        if let DeliveryOutcome::Missing = self.deliver_accessor(handle, update, app_state).await {
            self.accessors.remove(&handle);
        }
    }

    /// Handle `AppMessage::ListDirectory`: the asset manager's listing of `folder` (recipe keys
    /// included), for an editor's file selector.
    async fn handle_list_directory(
        &mut self,
        handle: UIHandle,
        folder: String,
        app_state: &Arc<tokio::sync::Mutex<dyn AppState>>,
    ) {
        let listing = match parse_key(&folder) {
            Ok(key) => match self.envref.get_asset_manager().listdir_asset_info(&key).await {
                Ok(infos) => DirectoryListing {
                    folder: folder.clone(),
                    entries: infos
                        .into_iter()
                        .map(|info| DirectoryEntry {
                            name: info
                                .filename
                                .clone()
                                .filter(|f| !f.is_empty())
                                .or_else(|| info.key.as_ref().and_then(|k| k.filename().map(|f| f.to_string())))
                                .unwrap_or_default(),
                            is_dir: info.is_dir,
                        })
                        .collect(),
                    error: None,
                },
                Err(e) => DirectoryListing {
                    folder: folder.clone(),
                    entries: vec![],
                    error: Some(e.to_string()),
                },
            },
            Err(e) => DirectoryListing {
                folder: folder.clone(),
                entries: vec![],
                error: Some(e.to_string()),
            },
        };
        let _ = Self::deliver_update(
            handle,
            UpdateMessage::Directory(listing),
            app_state,
            &self.context_parts(),
        )
        .await;
    }

    /// Phase 5: delivers each editor accessor's change as a fresh read, and drops the accessors
    /// of elements that are gone.
    async fn poll_accessors(&mut self, app_state: &Arc<tokio::sync::Mutex<dyn AppState>>) {
        let handles: Vec<UIHandle> = self.accessors.keys().copied().collect();
        let mut to_remove = Vec::new();
        for handle in handles {
            let exists = app_state.lock().await.node_exists(handle);
            if !exists {
                to_remove.push(handle);
                continue;
            }
            let Some(watched) = self.accessors.get_mut(&handle) else {
                continue;
            };
            if watched.pull {
                let _ = watched.accessor.refresh().await;
            }
            if !watched.events.has_changed().unwrap_or(false) {
                continue;
            }
            let event = watched.events.borrow_and_update().clone();
            let accessor = watched.accessor.clone();
            let update = match event {
                AccessorEvent::Changed(_)
                | AccessorEvent::Invalidated
                | AccessorEvent::Status(Status::Expired) => Self::read_accessor(&accessor).await,
                AccessorEvent::Removed => AccessorUpdate::Failed(Error::general_error(
                    "the edited value was removed".to_string(),
                )),
                AccessorEvent::Initial
                | AccessorEvent::Status(_)
                | AccessorEvent::PrimaryProgress(_)
                | AccessorEvent::SecondaryProgress(_)
                | AccessorEvent::Log(_) => continue,
            };
            if let DeliveryOutcome::Missing = self.deliver_accessor(handle, update, app_state).await {
                to_remove.push(handle);
            }
        }
        for handle in to_remove {
            self.accessors.remove(&handle);
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
                let status = snapshot.status;
                let error = snapshot.error.clone();
                let outcome = Self::deliver_update(
                    handle,
                    UpdateMessage::AssetUpdate(snapshot),
                    app_state,
                    &self.context_parts(),
                )
                .await;
                match outcome {
                    DeliveryOutcome::Delivered => self.record(Some(handle), status, error),
                    DeliveryOutcome::Missing => {}
                }

                // 5. Store in monitoring map (replaces existing if any).
                //    If the element doesn't exist, don't start monitoring.
                match outcome {
                    DeliveryOutcome::Delivered => {
                        self.monitoring.insert(
                            handle,
                            MonitoredAsset {
                                asset_ref,
                                notification_rx,
                            },
                        );
                    }
                    DeliveryOutcome::Missing => {}
                }
            }
            Err(e) => {
                // Build error snapshot and deliver
                let error_snapshot = AssetSnapshot {
                    value: None,
                    metadata: liquers_core::metadata::Metadata::new(),
                    error: Some(e),
                    status: Status::Error,
                };
                let error = error_snapshot.error.clone();
                let outcome = Self::deliver_update(
                    handle,
                    UpdateMessage::AssetUpdate(error_snapshot),
                    app_state,
                    &self.context_parts(),
                )
                .await;
                match outcome {
                    DeliveryOutcome::Delivered => {
                        self.record(Some(handle), Status::Error, error)
                    }
                    DeliveryOutcome::Missing => {}
                }
            }
        }
    }

    /// Phase 4: Poll all monitored assets for notification changes.
    /// Build and deliver AssetSnapshot on change.
    /// Remove entries where the element no longer exists (auto-stop).
    async fn poll_monitored_assets(&mut self, app_state: &Arc<tokio::sync::Mutex<dyn AppState>>) {
        let mut to_remove = Vec::new();
        let mut delivered = Vec::new();
        let parts = self.context_parts();

        for (handle, monitored) in self.monitoring.iter_mut() {
            // Check if notification has changed (non-blocking)
            if monitored.notification_rx.has_changed().unwrap_or(false) {
                // Acknowledge the change
                let _notification = monitored.notification_rx.borrow_and_update().clone();

                // Build fresh snapshot
                let snapshot = Self::build_snapshot(&monitored.asset_ref).await;

                // Deliver to element
                let status = snapshot.status;
                let error = snapshot.error.clone();
                let outcome = Self::deliver_update(
                    *handle,
                    UpdateMessage::AssetUpdate(snapshot),
                    app_state,
                    &parts,
                )
                .await;
                match outcome {
                    DeliveryOutcome::Delivered => delivered.push((*handle, status, error)),
                    DeliveryOutcome::Missing => to_remove.push(*handle),
                }
            } else {
                // Even without notification changes, check if element still exists
                let state = app_state.lock().await;
                if !state.node_exists(*handle) {
                    to_remove.push(*handle);
                }
            }
        }

        for (handle, status, error) in delivered {
            self.record(Some(handle), status, error);
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
                let val = Some(s.data_unchecked().clone());
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

    /// Deliver an update message (an asset snapshot or a user event) to an element via update().
    ///
    /// Uses the extract-update-replace pattern to avoid holding the AppState lock while calling
    /// update() (which requires a UIContext that references AppState). The element's
    /// `UpdateResponse` is carried back to the caller, which turns `NeedsRepaint` into a
    /// `UIChange::Replaced` record — `put_element` deliberately records nothing, because it is
    /// also the render path.
    async fn deliver_update(
        handle: UIHandle,
        message: UpdateMessage,
        app_state: &Arc<tokio::sync::Mutex<dyn AppState>>,
        parts: &ContextParts,
    ) -> DeliveryOutcome {
        // Extract element from AppState (take_element errors if not found)
        let mut element = {
            let mut state = app_state.lock().await;
            match state.take_element(handle) {
                Ok(elem) => elem,
                Err(_) => return DeliveryOutcome::Missing,
            }
        }; // Lock released

        // Create UIContext and call update (lock not held)
        let ctx = parts.context(app_state).with_handle(Some(handle));
        let response = element.update(&message, &ctx);

        // Put element back, and record the element's own repaint verdict.
        {
            let mut state = app_state.lock().await;
            let _ = state.put_element(handle, element);
            match response {
                UpdateResponse::NeedsRepaint => {
                    state.record_change(UIChange::Replaced { handle });
                }
                UpdateResponse::Unchanged => {}
            }
        }

        DeliveryOutcome::Delivered
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

    /// True when there are live monitored assets that can publish snapshots.
    pub fn has_monitoring(&self) -> bool {
        !self.monitoring.is_empty()
    }

    /// True when the UI should keep repainting to surface async updates.
    pub fn needs_repaint(&self) -> bool {
        self.has_evaluating() || self.has_monitoring()
    }

    /// Phase 6: start loading a theme given as a key or query, and re-read it when it changes.
    async fn poll_theme(&mut self) {
        if let Some(query) = self.theme_query.take() {
            match self.envref.evaluate(&query).await {
                Ok(asset_ref) => {
                    let mut notification_rx = asset_ref.subscribe_to_notifications().await;
                    let _ = notification_rx.borrow_and_update();
                    self.apply_theme_asset(&asset_ref).await;
                    self.theme_watch = Some((query, MonitoredAsset { asset_ref, notification_rx }));
                }
                Err(e) => {
                    eprintln!("theme '{query}' could not be evaluated: {e}");
                    self.record(None, Status::Error, Some(e));
                }
            }
            return;
        }
        let changed = match &mut self.theme_watch {
            Some((query, watch)) if watch.notification_rx.has_changed().unwrap_or(false) => {
                let removed = matches!(
                    *watch.notification_rx.borrow_and_update(),
                    AssetNotificationMessage::Removed
                );
                Some((query.clone(), watch.asset_ref.clone(), removed))
            }
            Some(_) | None => None,
        };
        if let Some((query, asset_ref, removed)) = changed {
            // The theme's key was rewritten (the asset is replaced) or its value expired:
            // evaluate the query again on the next run.
            if removed || matches!(asset_ref.status().await, Status::Expired) {
                self.theme_watch = None;
                self.theme_query = Some(query);
            } else {
                self.apply_theme_asset(&asset_ref).await;
            }
        }
    }

    /// Composes the theme asset's value over the built-in theme and swaps it into the cell; a
    /// value that is not a theme keeps the previous one (AC-16).
    async fn apply_theme_asset(&mut self, asset_ref: &AssetRef<E>) {
        let snapshot = Self::build_snapshot(asset_ref).await;
        if let Some(e) = snapshot.error {
            eprintln!("theme asset failed: {e}");
            self.record(None, Status::Error, Some(e));
            return;
        }
        let Some(value) = snapshot.value else {
            return;
        };
        match theme_layer_of(&value).and_then(|layer| Theme::compose(&[layer])) {
            Ok(theme) => {
                self.theme.set_theme(Arc::new(theme));
                self.icons.clear();
            }
            Err(e) => {
                eprintln!("theme refused, keeping the previous one: {e}");
                self.record(None, Status::Error, Some(e));
            }
        }
    }

    /// Loads an icon image by key or query into the icon cache (AC-17).
    async fn handle_load_icon(&mut self, key: String) {
        let query = if key.starts_with('-') { key.clone() } else { format!("-R/{key}") };
        let load = match self.envref.evaluate(&query).await {
            Ok(asset_ref) => match asset_ref.poll_state().await {
                Some(state) => match state.error_result() {
                    Ok(_) => {
                        let value = state.data_unchecked();
                        match value.try_into_bytes().or_else(|_| value.try_into_string().map(String::into_bytes)) {
                            Ok(bytes) => IconLoad::Ready(Arc::from(bytes.into_boxed_slice())),
                            Err(_) => IconLoad::Failed,
                        }
                    }
                    Err(_) => IconLoad::Failed,
                },
                None => IconLoad::Failed,
            },
            Err(_) => IconLoad::Failed,
        };
        if matches!(load, IconLoad::Failed) {
            eprintln!("icon '{key}' could not be loaded");
        }
        self.icons.set(&key, load);
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
