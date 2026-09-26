//! HTML string helpers for the web backend: escaping, action attributes, and value rendering.
//!
//! These are stateless free functions returning owned `String`s. `value_to_html` is the
//! internal replacement for egui's `UIValueExtension::show` — a free function (not a trait),
//! because the web backend only ever renders values already wrapped inside a `UIElement`.

use crate::ui::action::UiAction;
use crate::ui::app_state::AppState;
use crate::value::simple::SimpleValue;
use crate::value::{ExtValue, Value};

/// Escape a string for safe interpolation into HTML text or a (single- or double-quoted)
/// attribute value. Every piece of dynamic text rendered by the web backend passes through
/// this — it is the backend's single defense against broken markup and injection.
pub fn escape_html(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\'' => out.push_str("&#39;"),
            _ => out.push(c),
        }
    }
    out
}

/// Serialize a `UiAction` into a `data-lq-action='{escaped json}'` attribute fragment
/// (leading space, single-quoted, escaped). Returns an empty string only if serialization
/// fails, so a render can always be inlined.
pub fn action_attr(action: &UiAction) -> String {
    match serde_json::to_string(action) {
        Ok(json) => format!(" data-lq-action='{}'", escape_html(&json)),
        Err(_) => String::new(),
    }
}

/// Render any `Value` (base or `ExtValue`) to an HTML fragment. `app_state` is used only to
/// recurse when the value is itself a `UIElement`.
pub fn value_to_html(value: &Value, app_state: &dyn AppState) -> String {
    match value {
        Value::Base(simple) => simple_to_html(simple),
        Value::Extended(ext) => ext_to_html(ext, app_state),
    }
}

fn labelled(class: &str, text: &str) -> String {
    format!(
        "<span class=\"lq-value lq-{}\">{}</span>",
        class,
        escape_html(text)
    )
}

fn simple_to_html(simple: &SimpleValue) -> String {
    match simple {
        SimpleValue::None {} => labelled("none", "None"),
        SimpleValue::Bool { value } => labelled("bool", &value.to_string()),
        SimpleValue::I32 { value } => labelled("int", &value.to_string()),
        SimpleValue::I64 { value } => labelled("int", &value.to_string()),
        SimpleValue::F64 { value } => labelled("float", &value.to_string()),
        SimpleValue::Text { value } => {
            format!("<span class=\"lq-text\">{}</span>", escape_html(value))
        }
        SimpleValue::Array { .. } => labelled("array", "Array"),
        SimpleValue::Object { .. } => labelled("object", "Object"),
        SimpleValue::Bytes { value } => labelled("bytes", &format!("{} bytes", value.len())),
        SimpleValue::Metadata { .. } => labelled("metadata", "Metadata"),
        SimpleValue::AssetInfo { value } => {
            if value.is_empty() {
                labelled("asset-info", "Asset Info: <empty>")
            } else {
                labelled(
                    "asset-info",
                    &format!("Asset Info ({} entries)", value.len()),
                )
            }
        }
        SimpleValue::Recipe { .. } => labelled("recipe", "Recipe"),
        SimpleValue::CommandMetadata { .. } => labelled("command-metadata", "Command Metadata"),
        SimpleValue::Query { value } => labelled("query", &value.encode()),
        SimpleValue::Key { value } => labelled("key", &value.encode()),
    }
}

fn ext_to_html(ext: &ExtValue, app_state: &dyn AppState) -> String {
    match ext {
        ExtValue::Image { value } => image_to_html(value),
        #[cfg(feature = "polars")]
        ExtValue::PolarsDataFrame { value } => super::dataframe::dataframe_to_html(value, 100),
        ExtValue::UIElement { value } => value.render_web(app_state),
        #[cfg(feature = "egui")]
        ExtValue::UiCommand { .. } => {
            "<div class=\"lq-egui-only\">egui command (no web rendering)</div>".to_string()
        }
        #[cfg(feature = "egui")]
        ExtValue::Widget { .. } => {
            "<div class=\"lq-egui-only\">egui widget (no web rendering)</div>".to_string()
        }
        // A value owned by an integrated language runtime. There is no backend-neutral
        // rendering for it — a language integration that wants one renders it before the
        // value reaches the element tree.
        ExtValue::Foreign { value } => format!(
            "<div class=\"lq-foreign\" data-origin=\"{}\">{} value: {}</div>",
            escape_html(value.origin()),
            escape_html(value.origin()),
            escape_html(value.type_name().as_ref()),
        ),
        #[cfg(feature = "records")]
        ExtValue::RecordView { value } => record_view_to_html(value),
        #[cfg(feature = "records")]
        ExtValue::RecordSource { value } => record_source_to_html(value),
    }
}

/// A `RecordView` renders as a small `<table>` preview, capped at this many rows — enough to see
/// a table's shape without materializing (or writing) one that could be arbitrarily large just
/// to render it. Bounded independently of `DEFAULT_MATERIALIZE_MAX_ROWS`: this is a UI preview,
/// not the `ns-rec/materialize` command's own bound.
#[cfg(feature = "records")]
const RECORD_VIEW_HTML_PREVIEW_ROWS: usize = 100;

/// Renders at most [`RECORD_VIEW_HTML_PREVIEW_ROWS`] rows of `value` as an HTML table, through
/// `write_table`'s `Html` writer — the same writer a `.html` write of the value goes through.
#[cfg(feature = "records")]
fn record_view_to_html(value: &std::sync::Arc<dyn crate::records::RecordView>) -> String {
    let total = value.len();
    let preview_len = total.min(RECORD_VIEW_HTML_PREVIEW_ROWS);
    let preview = match value.slice(0, preview_len) {
        Ok(preview) => preview,
        Err(err) => {
            return format!(
                "<div class=\"lq-record-view lq-error\">record view preview failed: {}</div>",
                escape_html(&err.to_string())
            )
        }
    };
    match crate::records::write_table(
        preview.as_ref(),
        crate::records::TableFormat::Html,
        &crate::records::WriteOptions::default(),
    ) {
        Ok(bytes) => {
            let mut html = String::from("<div class=\"lq-record-view\">");
            html.push_str(&String::from_utf8_lossy(&bytes));
            if total > preview_len {
                html.push_str(&format!(
                    "<div class=\"lq-record-view-note\">showing {} of {} rows</div>",
                    preview_len, total
                ));
            }
            html.push_str("</div>");
            html
        }
        Err(err) => format!(
            "<div class=\"lq-record-view lq-error\">record view rendering failed: {}</div>",
            escape_html(&err.to_string())
        ),
    }
}

/// A `RecordSource` cannot be rendered as a table here: opening its stream needs an `await`, and
/// this rendering path is synchronous (phase2-architecture.md §"Views are synchronous;
/// asynchronous work is a source"). So it gets a summary — chunk count and whether it has a
/// manifest — built only from `chunks()`/`manifest()`, which do no I/O.
#[cfg(feature = "records")]
fn record_source_to_html(value: &std::sync::Arc<dyn crate::records::RecordSource>) -> String {
    let chunk_summary = match value.chunks() {
        crate::records::ChunkList::Known(ids) => format!("{} chunk(s)", ids.len()),
        crate::records::ChunkList::Unbounded { computed } => {
            format!("at least {} chunk(s) (unbounded)", computed.len())
        }
    };
    let manifest_note = if value.manifest().is_some() {
        " &middot; has a manifest"
    } else {
        ""
    };
    format!(
        "<div class=\"lq-record-source\">Record source: {}{}</div>",
        escape_html(&chunk_summary),
        manifest_note
    )
}

fn image_to_html(image: &image::DynamicImage) -> String {
    match crate::image::serde::serialize_image_to_bytes(image, "png") {
        Ok(bytes) => {
            let b64 = base64::Engine::encode(&base64::engine::general_purpose::STANDARD, &bytes);
            format!(
                "<img class=\"lq-image\" alt=\"image\" src=\"data:image/png;base64,{}\"/>",
                b64
            )
        }
        Err(e) => format!(
            "<div class=\"lq-error\">Image encode error: {}</div>",
            escape_html(&e.to_string())
        ),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ui::app_state::DirectAppState;
    use liquers_core::value::ValueInterface;

    #[test]
    fn escape_html_escapes_all_five() {
        assert_eq!(
            escape_html("<a href=\"x\">&'"),
            "&lt;a href=&quot;x&quot;&gt;&amp;&#39;"
        );
    }

    #[test]
    fn action_attr_contains_data_attr() {
        let a = UiAction::Query("text-hi".to_string());
        let attr = action_attr(&a);
        assert!(attr.contains("data-lq-action"));
        assert!(attr.contains("text-hi"));
    }

    #[test]
    fn value_to_html_escapes_text() {
        let s = DirectAppState::new();
        let h = value_to_html(&Value::from("<script>alert(1)</script>"), &s);
        assert!(h.contains("&lt;script&gt;"));
        assert!(!h.contains("<script>"));
    }

    #[test]
    fn value_to_html_covers_base_variants() {
        let s = DirectAppState::new();
        assert!(value_to_html(&Value::none(), &s).contains("None"));
        assert!(value_to_html(&Value::from(true), &s).contains("true"));
        assert!(value_to_html(&Value::from(42i64), &s).contains("42"));
    }
}
