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

/// Render a query string as inline code with one `<span class="lq-tok-<role>">` per highlighter
/// token (AC-15), coloured by the theme CSS ([`crate::ui::theme::Theme::to_css`]). A text that
/// does not parse is escaped as it is.
pub fn query_to_html(query: &str) -> String {
    use liquers_core::query::{StyledQuery, StyledQueryToken};
    use crate::ui::theme::SyntaxRole;
    let Ok(parsed) = liquers_core::parse::parse_query(query) else {
        return format!("<code class=\"lq-query\">{}</code>", escape_html(query));
    };
    let styled = StyledQuery::from(parsed);
    let mut out = String::from("<code class=\"lq-query\">");
    for token in &styled.tokens {
        let text = match token {
            StyledQueryToken::StringParameter(t)
            | StyledQueryToken::Entity(t)
            | StyledQueryToken::Separator(t)
            | StyledQueryToken::ResourceName(t)
            | StyledQueryToken::ActionName(t)
            | StyledQueryToken::Header(t)
            | StyledQueryToken::Highlight(t) => t,
        };
        out.push_str(&format!(
            "<span class=\"lq-tok-{}\">{}</span>",
            SyntaxRole::from(token).name(),
            escape_html(text)
        ));
    }
    out.push_str("</code>");
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    // Test 21 (AC-15).
    #[test]
    fn query_to_html_emits_token_classes() {
        let html = query_to_html("-R/data/sales.csv/-/head-20");
        assert!(html.contains("<span class=\"lq-tok-action\">head</span>"), "{html}");
        assert!(html.contains("lq-tok-resource"), "{html}");
        assert!(html.contains("lq-tok-parameter"), "{html}");
        let broken = query_to_html("a/<b>/-/~");
        if !broken.contains("lq-tok-") {
            assert!(broken.contains("&lt;b&gt;"), "{broken}");
        }
        assert!(!query_to_html("<script>").contains("<script>"));
    }
}
