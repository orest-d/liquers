use std::{fmt::Display, sync::Arc};

use egui::{text::LayoutJob, Align, Color32, FontSelection, RichText, Style, TextEdit, Widget};
use liquers_core::{
    assets::{AssetManager, AssetRef},
    context::{EnvRef, Environment},
    error::Error,
    metadata::{AssetInfo, ProgressEntry, Status},
    parse::parse_query,
    query::{Key, Position, QueryRenderer, StyledQuery, StyledQueryToken, TryToQuery},
};
use tokio::sync::oneshot;

pub trait WidgetValue: std::fmt::Debug + Send {
    fn show(&mut self, ui: &mut egui::Ui) -> egui::Response;
}

impl Widget for &mut dyn WidgetValue {
    fn ui(self, ui: &mut egui::Ui) -> egui::Response {
        self.show(ui)
    }
}

pub struct TextEditor<E: Environment> {
    pub key: Key,
    pub text_receiver: oneshot::Receiver<Result<String, Error>>,
    pub text: Option<Result<String, Error>>,
    phantom: std::marker::PhantomData<E>,
}

impl<E: Environment> TextEditor<E> {
    pub fn new(key: Key, envref: EnvRef<E>) -> Self {
        let (text_tx, text_receiver) = oneshot::channel();

        let key_clone = key.clone();

        eprintln!("SPAWNING LOADING for key: {}", key_clone.encode());
        tokio::spawn(async move {
            eprintln!("TASK STARTED for key: {}", key_clone.encode());
            let result = async {
                eprintln!("SUBTASK STARTED for key: {}", key_clone.encode());
                let asset_manager = envref.get_asset_manager();
                asset_manager
                    .get(&key_clone)
                    .await?
                    .get()
                    .await?
                    .try_into_string()
            }
            .await;

            // TODO: Use asset manager above when implemented correctly
            /*
            let result = async {
                let store = envref.get_async_store();
                let (bin, _) = store.get(&key_clone).await?;
                Ok(String::from_utf8_lossy(&bin).to_string())
            }.await;
            */
            let _ = text_tx.send(result);
            eprintln!("TASK ENDED for key: {}", key_clone.encode());
        });

        Self {
            key,
            text: None,
            text_receiver,
            phantom: std::marker::PhantomData,
        }
    }
}

impl<E: Environment> std::fmt::Debug for TextEditor<E> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("TextEditor")
            .field("key", &self.key.encode())
            .finish()
    }
}

impl<E: Environment> WidgetValue for TextEditor<E> {
    fn show(&mut self, ui: &mut egui::Ui) -> egui::Response {
        let text_was_loaded = self.text.is_some();

        if !text_was_loaded {
            match self.text_receiver.try_recv() {
                Ok(result) => {
                    self.text = Some(result);
                }
                Err(oneshot::error::TryRecvError::Empty) => {}
                Err(oneshot::error::TryRecvError::Closed) => {
                    self.text = Some(Err(Error::general_error(
                        "Failed to receive text from channel".to_string(),
                    )));
                }
            }
        }

        match &mut self.text {
            Some(Ok(text)) => {
                // Add header outside of scroll area
                ui.label(format!("Text Editor - Key: {}", self.key.encode()));
                ui.separator();

                let remaining_height = ui.available_height();

                egui::ScrollArea::both()
                    .auto_shrink([false, false])
                    .show(ui, |ui| {
                        ui.add_sized(
                            [ui.available_width(), remaining_height],
                            egui::TextEdit::multiline(text),
                        )
                    })
                    .inner
            }
            Some(Err(e)) => {
                display_error(ui, e);
                ui.allocate_response(ui.available_size(), egui::Sense::hover())
            }
            None => {
                ui.vertical_centered(|ui| {
                    ui.add_space(50.0);
                    ui.spinner();
                    ui.label("Loading text...");
                    ui.label(format!("Key: {}", self.key.encode()));
                });
                ui.allocate_response(ui.available_size(), egui::Sense::hover())
            }
        }
    }
}

pub struct AssetStatus<E: Environment> {
    pub asset_ref: AssetRef<E>,
    pub asset_info: Arc<std::sync::RwLock<Option<AssetInfo>>>,
    pub error_message: Arc<std::sync::RwLock<Option<String>>>,
    phantom: std::marker::PhantomData<E>,
}

impl<E: Environment> AssetStatus<E> {
    pub fn new(asset_ref: AssetRef<E>) -> Self {
        let asset_info = Arc::new(std::sync::RwLock::new(None));
        let error_message = Arc::new(std::sync::RwLock::new(None));

        // Clone references for the spawned task
        let asset_ref_clone = asset_ref.clone();
        let asset_info_clone = asset_info.clone();
        let error_message_clone = error_message.clone();

        // Spawn task to listen to asset notifications
        tokio::spawn(async move {
            eprintln!(
                "Starting AssetStatus notification listener for asset {}",
                asset_ref_clone.id()
            );

            let mut rx = asset_ref_clone.subscribe_to_notifications().await;

            loop {
                match rx.changed().await {
                    Ok(_) => {
                        let notification = rx.borrow().clone();
                        eprintln!("AssetStatus received notification: {:?}", notification);

                        // Get updated asset info
                        match asset_ref_clone.get_asset_info().await {
                            Ok(info) => {
                                let mut asset_info_lock = asset_info_clone.write().unwrap();
                                *asset_info_lock = Some(info.clone());

                                // Clear error message on successful info update
                                let mut error_lock = error_message_clone.write().unwrap();
                                *error_lock = None;

                                eprintln!("AssetStatus updated info: status={:?}", info.status);

                                // Check if asset is finished
                                if info.status.is_finished() {
                                    eprintln!(
                                        "AssetStatus: Asset {} finished with status {:?}",
                                        asset_ref_clone.id(),
                                        info.status
                                    );
                                    break;
                                }
                            }
                            Err(e) => {
                                eprintln!("AssetStatus failed to get asset info: {}", e);
                                let mut error_lock = error_message_clone.write().unwrap();
                                *error_lock = Some(format!("Failed to get asset info: {}", e));
                            }
                        }
                    }
                    Err(e) => {
                        eprintln!("AssetStatus notification channel error: {}", e);
                        let mut error_lock = error_message_clone.write().unwrap();
                        *error_lock = Some(format!("Notification channel error: {}", e));
                        break;
                    }
                }
            }

            eprintln!(
                "AssetStatus notification listener finished for asset {}",
                asset_ref_clone.id()
            );
        });

        Self {
            asset_ref,
            asset_info,
            error_message,
            phantom: std::marker::PhantomData,
        }
    }
}

impl<E: Environment> std::fmt::Debug for AssetStatus<E> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("AssetStatus")
            .field("asset_id", &self.asset_ref.id())
            .finish()
    }
}

impl<E: Environment> WidgetValue for AssetStatus<E> {
    fn show(&mut self, ui: &mut egui::Ui) -> egui::Response {
        ui.vertical(|ui| {
            ui.heading(format!("Asset Status - ID: {}", self.asset_ref.id()));
            ui.separator();

            // Check for error messages first
            if let Ok(error_lock) = self.error_message.read() {
                if let Some(error) = error_lock.as_ref() {
                    ui.colored_label(egui::Color32::RED, format!("❌ Error: {}", error));
                    return ui.allocate_response(ui.available_size(), egui::Sense::hover());
                }
            }

            // Display asset info
            if let Ok(info_lock) = self.asset_info.read() {
                if let Some(info) = info_lock.as_ref() {
                    display_asset_info(ui, info);
                } else {
                    ui.spinner();
                    ui.label("🔄 Obtaining asset info...");
                }
            } else {
                ui.colored_label(egui::Color32::RED, "❌ Failed to read asset info");
            }

            ui.allocate_response(ui.available_size(), egui::Sense::hover())
        })
        .response
    }
}

/// Single-line text edit field with syntax highlighting for queries
pub fn edit_query(ui: &mut egui::Ui, query_text: &mut String) -> egui::Response {
    let qt = query_text.clone();
    let mut layouter = |ui: &egui::Ui, _buf: &dyn egui::TextBuffer, _wrap_width: f32| {
        let layout_job = query_to_layout_job(&qt);
        ui.fonts_mut(|f| f.layout_job(layout_job))
    };
    ui.horizontal(|ui| {
        //ui.colored_label(egui::Color32::GREEN, "OK");
        ui.add_sized(
            [
                ui.available_width(),
                ui.text_style_height(&egui::TextStyle::Body),
            ],
            TextEdit::singleline(query_text)
                .layouter(&mut layouter)
                .hint_text("Enter query here..."),
        );
    })
    .response
}

/// Helper function to display styled query tokens (used by both display functions)
fn styled_query_to_layout_job(styled_query: &StyledQuery) -> LayoutJob {
    let style = Style::default();
    let mut layout_job = LayoutJob::default();
    for token in styled_query.tokens.iter() {
        match token {
            StyledQueryToken::StringParameter(text) => RichText::new(text)
                .color(Color32::from_hex("#0b89ffff").unwrap())
                .monospace()
                .append_to(
                    &mut layout_job,
                    &style,
                    FontSelection::default(),
                    Align::LEFT,
                ), // Green
            StyledQueryToken::Entity(text) => RichText::new(text)
                .color(Color32::from_hex("#c5e243ff").unwrap())
                .monospace()
                .append_to(
                    &mut layout_job,
                    &style,
                    FontSelection::default(),
                    Align::LEFT,
                ), // Orange
            StyledQueryToken::Separator(text) => RichText::new(text)
                .color(Color32::from_hex("#afd1daff").unwrap())
                .monospace()
                .append_to(
                    &mut layout_job,
                    &style,
                    FontSelection::default(),
                    Align::LEFT,
                ), // Gray
            StyledQueryToken::ResourceName(text) => RichText::new(text)
                .color(Color32::from_hex("#22aa4aff").unwrap())
                .monospace()
                .append_to(
                    &mut layout_job,
                    &style,
                    FontSelection::default(),
                    Align::LEFT,
                ), // Blue
            StyledQueryToken::ActionName(text) => RichText::new(text)
                .color(Color32::from_hex("#6543ffff").unwrap())
                .monospace()
                .append_to(
                    &mut layout_job,
                    &style,
                    FontSelection::default(),
                    Align::LEFT,
                ), // Light Orange
            StyledQueryToken::Header(text) => RichText::new(text)
                .color(Color32::from_hex("#c359f9ff").unwrap())
                .monospace()
                .append_to(
                    &mut layout_job,
                    &style,
                    FontSelection::default(),
                    Align::LEFT,
                ), // Purple
            StyledQueryToken::Highlight(text) => RichText::new(text)
                .color(Color32::from_rgb(255, 100, 100))
                .monospace()
                .underline()
                .append_to(
                    &mut layout_job,
                    &style,
                    FontSelection::default(),
                    Align::LEFT,
                ), // Yellow
        };
    }
    layout_job
}

fn query_to_layout_job<Q: TryToQuery + Display + Clone>(q: Q) -> LayoutJob {
    let rquery = q.clone().try_to_query();
    if let Ok(query) = rquery {
        let styled_query = StyledQuery::from(query);
        styled_query_to_layout_job(&styled_query)
    } else {
        let mut layout_job = LayoutJob::default();
        RichText::new(format!("{}", &q))
            .color(Color32::from_rgb(200, 200, 200))
            .monospace()
            .append_to(
                &mut layout_job,
                &Style::default(),
                FontSelection::default(),
                Align::LEFT,
            );
        layout_job
    }
}

/// Helper function to display styled query tokens (used by both display functions)
fn display_styled_query_tokens(ui: &mut egui::Ui, styled_query: &StyledQuery) {
    let layout_job = styled_query_to_layout_job(styled_query);
    ui.label(layout_job);
}

/// Display a StyledQuery with different colors for each token type
/// Supports any type that implements Into<StyledQuery>
pub fn display_styled_query<T>(ui: &mut egui::Ui, query: T) -> egui::Response
where
    T: Into<StyledQuery>,
{
    let styled_query: StyledQuery = query.into();

    ui.horizontal_wrapped(|ui| {
        display_styled_query_tokens(ui, &styled_query);
    })
    .response
}

/// Display a StyledQuery with different colors for each token type
/// Supports any type that implements Into<StyledQuery>
pub fn display_styled_query_with_position<T: QueryRenderer>(
    ui: &mut egui::Ui,
    query: &T,
    position: &Position,
) -> egui::Response
where
    T: Into<StyledQuery>,
{
    let styled_query = StyledQuery::from_query(query, position);

    ui.horizontal_wrapped(|ui| {
        display_styled_query_tokens(ui, &styled_query);
    })
    .response
}

/// Display a StyledQuery with a label
/// Supports any type that implements Into<StyledQuery>
pub fn labeled_styled_query<T>(ui: &mut egui::Ui, label: &str, query: T) -> egui::Response
where
    T: Into<StyledQuery>,
{
    ui.horizontal_wrapped(|ui| {
        ui.label(format!("{}:", label));
        let styled_query: StyledQuery = query.into();
        display_styled_query_tokens(ui, &styled_query);
    })
    .response
}

/// Display an Error object with syntax highlighting for query and key if available
pub fn display_error(ui: &mut egui::Ui, error: &Error) -> egui::Response {
    ui.vertical(|ui| {
        // Error header with type and color coding
        let error_color = Color32::from_rgb(255, 100, 100); // Light red
        ui.colored_label(error_color, format!("❌ {:?}", error.error_type));

        // Error message
        ui.colored_label(Color32::from_rgb(200, 200, 200), &error.message);

        // Position if available
        if !error.position.is_unknown() {
            ui.label(format!("Position: {}", error.position));
        }

        // Query with syntax highlighting if available
        if let Some(query_str) = &error.query {
            ui.separator();
            if let Ok(query) = parse_query(query_str) {
                display_styled_query_with_position(ui, &query, &error.position);
            } else {
                // If parsing fails, display as plain text
                ui.horizontal_wrapped(|ui| {
                    ui.label("Query:");
                    ui.label(query_str);
                });
            }
        }

        // Key as plain text if available
        if let Some(key_str) = &error.key {
            ui.separator();
            ui.horizontal_wrapped(|ui| {
                ui.label("Key:");
                ui.label(key_str);
            });
        }
    })
    .response
}

pub fn display_status(ui: &mut egui::Ui, status: Status) -> egui::Response {
    match status {
        Status::None => ui.colored_label(Color32::from_rgb(150, 150, 150), "None"),
        Status::Directory => ui.colored_label(Color32::GREEN, "DIR"),
        Status::Recipe => ui.colored_label(Color32::BLUE, "Recipe"),
        Status::Submitted => ui.colored_label(Color32::YELLOW, "Submitted"),
        Status::Dependencies => ui.colored_label(Color32::YELLOW, "Dep."),
        Status::Processing => ui.colored_label(Color32::YELLOW, "Processing"),
        Status::Partial => ui.colored_label(Color32::from_hex("#5dc000ff").unwrap(), "Partial"),
        Status::Error => ui.colored_label(Color32::RED, "Error"),
        Status::Storing => ui.colored_label(Color32::YELLOW, "Storing"),
        Status::Ready => ui.colored_label(Color32::GREEN, "Ready"),
        Status::Expired => ui.colored_label(Color32::from_hex("#57b157ff").unwrap(), "Expired"),
        Status::Cancelled => ui.colored_label(Color32::RED, "Cancelled"),
        Status::Source => ui.colored_label(Color32::from_hex("#00ff73ff").unwrap(), "Source"),
    }
}

pub fn display_progress(ui: &mut egui::Ui, progress: &ProgressEntry) -> egui::Response {
    if progress.is_done() {
        ui.colored_label(Color32::GREEN, "✔ Done")
    } else if progress.is_off() {
        ui.colored_label(Color32::GRAY, " - ")
    } else if progress.is_tick() {
        ui.spinner()
    } else {
        let progress_ratio = if progress.total > 0 {
            progress.done as f32 / progress.total as f32
        } else {
            0.0
        };
        let msg = if let Some(eta) = &progress.eta {
            format!(
                "{}/{} {} (ETA: {})",
                progress.done, progress.total, progress.message, eta
            )
        } else {
            format!("{}/{} {}", progress.done, progress.total, progress.message)
        };
        ui.add(egui::ProgressBar::new(progress_ratio).text(msg))
    }
}

/// Display an AssetInfo structure with comprehensive information
pub fn display_asset_info(ui: &mut egui::Ui, asset_info: &AssetInfo) -> egui::Response {
    ui.vertical(|ui| {
        // Header with title and icon
        ui.horizontal(|ui| {
            if !asset_info.unicode_icon.is_empty() {
                ui.label(egui::RichText::new(&asset_info.unicode_icon).size(20.0));
            }
            if let Some(error) = &asset_info.error_data {
                display_status(ui, asset_info.status).on_hover_ui(|ui| {
                    display_error(ui, error);
                });
            } else {
                display_status(ui, asset_info.status);
            }

            let title = if !asset_info.title.is_empty() {
                asset_info.title.clone()
            } else if let Some(filename) = &asset_info.filename {
                filename.clone()
            } else {
                "<no title>".to_string()
            };
            ui.label(&title);

            ui.label(asset_info.filename.as_deref().unwrap_or("<no filename>"));
            if let Some(file_size) = asset_info.file_size {
                ui.colored_label(Color32::LIGHT_GRAY, " Size:");
                ui.label(format_file_size(file_size))
                    .on_hover_text(format!("{} bytes", file_size));
            }
            display_progress(ui, &asset_info.progress);
        });

        ui.heading(asset_info.title.clone());
        ui.separator();
        if let Some(query) = &asset_info.query {
            ui.horizontal(|ui| {
                ui.colored_label(Color32::LIGHT_GRAY, "• Query:");
                display_styled_query(ui, query.clone());
            });
        }
        if let Some(key) = &asset_info.key {
            ui.horizontal(|ui| {
                ui.colored_label(Color32::LIGHT_GRAY, "• Key:");
                ui.label(key.encode());
            });
        }
        if !asset_info.message.is_empty() {
            ui.horizontal(|ui| {
                ui.colored_label(Color32::LIGHT_GRAY, "• Message:");
                ui.label(&asset_info.message);
            });
        }
        if !asset_info.description.is_empty() {
            ui.separator();
            ui.heading("Description:");
            ui.label(&asset_info.description);
        }
        ui.separator();
        ui.horizontal(|ui| {
            ui.colored_label(Color32::LIGHT_GRAY, "• Type:");
            ui.label(&asset_info.type_identifier);
        });
        ui.horizontal(|ui| {
            ui.colored_label(Color32::LIGHT_GRAY, "• Media Type:");
            ui.label(&asset_info.media_type);
        });
        if let Some(data_format) = &asset_info.data_format {
            ui.horizontal(|ui| {
                ui.colored_label(Color32::LIGHT_GRAY, "• Data Format:");
                ui.label(data_format);
            });
        }

        // Last updated time if available
        if !asset_info.updated.is_empty() {
            ui.horizontal(|ui| {
                ui.colored_label(Color32::LIGHT_GRAY, " Updated:");
                ui.label(&asset_info.updated);
            });
        }
    })
    .response
}

/// Helper function to format file sizes in human-readable format
fn format_file_size(size: u64) -> String {
    const UNITS: &[&str] = &["B", "KB", "MB", "GB", "TB"];
    let mut size_f = size as f64;
    let mut unit_index = 0;

    while size_f >= 1024.0 && unit_index < UNITS.len() - 1 {
        size_f /= 1024.0;
        unit_index += 1;
    }

    if unit_index == 0 {
        format!("{} {}", size, UNITS[unit_index])
    } else {
        format!("{:.1} {}", size_f, UNITS[unit_index])
    }
}
