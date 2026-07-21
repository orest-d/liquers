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
        SimpleValue::Text { value } => format!("<span class=\"lq-text\">{}</span>", escape_html(value)),
        SimpleValue::Array { .. } => labelled("array", "Array"),
        SimpleValue::Object { .. } => labelled("object", "Object"),
        SimpleValue::Bytes { value } => labelled("bytes", &format!("{} bytes", value.len())),
        SimpleValue::Metadata { .. } => labelled("metadata", "Metadata"),
        SimpleValue::AssetInfo { value } => {
            if value.is_empty() {
                labelled("asset-info", "Asset Info: <empty>")
            } else {
                labelled("asset-info", &format!("Asset Info ({} entries)", value.len()))
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
    }
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
        Err(e) => format!("<div class=\"lq-error\">Image encode error: {}</div>", escape_html(&e.to_string())),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ui::app_state::DirectAppState;
    use liquers_core::value::ValueInterface;

    #[test]
    fn escape_html_escapes_all_five() {
        assert_eq!(escape_html("<a href=\"x\">&'"), "&lt;a href=&quot;x&quot;&gt;&amp;&#39;");
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
