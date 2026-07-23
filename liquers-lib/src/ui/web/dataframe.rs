//! Polars DataFrame → HTML rendering for the web backend.

use super::html::escape_html;

/// Render a DataFrame to HTML, capped at `max_rows`. Uses Polars' Display inside a
/// scrollable `<pre>` for a robust first-cut; a real `<table>` can replace this later.
pub fn dataframe_to_html(df: &polars::frame::DataFrame, max_rows: usize) -> String {
    let head = df.head(Some(max_rows));
    let total = df.height();
    let mut html = String::from("<div class=\"lq-dataframe\"><pre>");
    html.push_str(&escape_html(&format!("{}", head)));
    html.push_str("</pre>");
    if total > max_rows {
        html.push_str(&format!(
            "<div class=\"lq-df-note\">showing {} of {} rows</div>",
            max_rows, total
        ));
    }
    html.push_str("</div>");
    html
}
