//! Web analogs of `egui/widgets.rs` helpers — all return HTML strings.

use liquers_core::error::Error;
use liquers_core::metadata::{AssetInfo, ProgressEntry, Status};

use super::html::escape_html;

/// Render an asset status as a labelled span (with a status-specific CSS class).
pub fn status_html(status: Status) -> String {
    format!(
        "<span class=\"lq-status lq-status-{:?}\">{:?}</span>",
        status, status
    )
}

/// Render primary progress (matches `AssetInfo.progress`, a `ProgressEntry`).
pub fn progress_html(progress: &ProgressEntry) -> String {
    format!(
        "<div class=\"lq-progress\">{}</div>",
        escape_html(&format!("{:?}", progress))
    )
}

/// Render a compact asset-info summary.
pub fn asset_info_html(info: &AssetInfo) -> String {
    format!(
        "<div class=\"lq-asset-info\"><pre>{}</pre></div>",
        escape_html(&format!("{:#?}", info))
    )
}

/// Render an error in a red-styled block.
pub fn error_html(error: &Error) -> String {
    format!(
        "<div class=\"lq-error\">{}</div>",
        escape_html(&error.to_string())
    )
}

/// Render a query string as (currently non-highlighted) inline code. The egui backend
/// syntax-highlights; the web MVP escapes and wraps in `<code>`.
pub fn query_to_html(query: &str) -> String {
    format!("<code class=\"lq-query\">{}</code>", escape_html(query))
}
