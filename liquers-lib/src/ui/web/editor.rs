//! Browser markup of the query, action, resource and key editors (`crate::ui::editor`).
//!
//! [`editor_html`] renders an [`EditorTree`] as plain HTML: nodes are `<details>`, hover text is
//! `title=`, fields carry `data-lq-field` names and buttons `data-lq-event` names that
//! [`crate::ui::editor::EditorView::from_ui_event`] decodes back into commands. A selector or a
//! checkbox sends `change:<field>` on the DOM `change` event; Enter in a text field submits every
//! changed field. Lists reorder with up and down buttons (the browser has no drag events yet,
//! `UI-EVENTS-HAVE-NO-POINTER-DRAG-OR-CHANGE-KINDS`).
//!
//! # Main components
//!
//! - [`editor_html`]: the whole editor.
//!
//! # Where it is used
//!
//! - `EditorElement::render_web`, for server-side rendering and the browser driver.
//!
//! # Related concepts
//!
//! - `crate::egui::editor`: the egui rendering of the same tree.
//! - [`super::html::field_attr`] and [`super::html::event_attr`]: the markup conventions.
//!
//! # Examples
//!
//! ```
//! use liquers_lib::ui::editor::EditorTree;
//! use liquers_lib::ui::web::editor::editor_html;
//!
//! let tree = EditorTree { text: "-R/data.csv".into(), text_editable: true, ..Default::default() };
//! let html = editor_html(&tree, "Editor");
//! assert!(html.contains("data-lq-field=\"text\""));
//! ```

use liquers_core::command_metadata::ArgumentGUIInfo;

use super::html::{escape_html, event_attr, field_attr};
use crate::ui::editor::{
    event_name, field_id, EditorField, EditorNode, EditorTree, FieldValue, NodePath, RESOURCE_HEADERS,
};
use crate::ui::event::UIEventKind;

fn button(label: &str, verb: &str, path: &NodePath, rest: &str, title: &str) -> String {
    format!(
        "<button type=\"button\" class=\"lq-ed-btn\" title=\"{}\"{}>{}</button>",
        escape_html(title),
        event_attr(&UIEventKind::Named(event_name(verb, path, rest))),
        escape_html(label)
    )
}

fn change(field: &str) -> String {
    event_attr(&UIEventKind::Change(field.to_string()))
}

/// The editor's markup.
pub fn editor_html(tree: &EditorTree, title: &str) -> String {
    let root = NodePath::default();
    let mut h = String::new();
    h.push_str(&format!("<div class=\"lq-editor\" title=\"{}\">", escape_html(title)));
    // Toolbar (AC-26).
    h.push_str("<div class=\"lq-ed-toolbar\">");
    h.push_str(&format!(
        "<label><input type=\"checkbox\"{}{}{}> Update immediately</label>",
        field_attr("continuous"),
        change("continuous"),
        if tree.continuous { " checked" } else { "" }
    ));
    if !tree.continuous {
        h.push_str(&button("Update", "update", &root, "", "Set the edits into where the value lives"));
        h.push_str(&button("Revert", "revert", &root, "", "Drop the edits and reload"));
    }
    if tree.pending {
        h.push_str("<em class=\"lq-ed-pending\">pending</em>");
    }
    h.push_str("</div>");
    // Text (AC-4, AC-10).
    if tree.text_editable {
        h.push_str(&format!(
            "<input type=\"text\" class=\"lq-ed-text\" value=\"{}\"{}{}>",
            escape_html(&tree.text),
            field_attr("text"),
            event_attr(&UIEventKind::Submit)
        ));
    }
    h.push_str(&format!("<code class=\"lq-ed-query\">{}</code>", escape_html(&tree.text)));
    if let Some(note) = &tree.text_note {
        h.push_str(&format!("<div class=\"lq-ed-note\">{}</div>", escape_html(note)));
    }
    if let Some((_, message)) = &tree.message {
        h.push_str(&format!("<div class=\"lq-ed-message\">{}</div>", escape_html(message)));
    }
    // Nodes.
    h.push_str("<div class=\"lq-ed-nodes\">");
    for node in &tree.nodes {
        node_html(&mut h, node);
    }
    h.push_str("</div>");
    if tree.has_filename {
        h.push_str("<div class=\"lq-ed-append\">");
        h.push_str(&button("+ action", "picker", &root, "", "Append a command"));
        h.push_str(&button("+ q", "append", &root, "q", "Append a quote directive"));
        h.push_str(&button("+ v", "append", &root, "v", "Append a volatile directive"));
        h.push_str("</div>");
        h.push_str(&format!(
            "<div class=\"lq-ed-filename\">filename <input type=\"text\" value=\"{}\"{}{}>",
            escape_html(&tree.filename),
            field_attr("filename"),
            event_attr(&UIEventKind::Submit)
        ));
        if let Some(default) = &tree.filename_default {
            h.push_str(&button("Default", "defaultfilename", &root, "", default));
        }
        h.push_str("</div>");
    }
    if let Some(picker) = &tree.picker {
        let at = picker.insert_at.clone().unwrap_or_default();
        h.push_str("<div class=\"lq-ed-picker\">");
        h.push_str(&format!(
            "<input type=\"search\" value=\"{}\"{}{}>",
            escape_html(&picker.filter),
            field_attr("picker"),
            event_attr(&UIEventKind::Named(event_name("picker", &at, "")))
        ));
        h.push_str(&button("✕", "closepicker", &root, "", "Close"));
        for ns in &picker.catalog {
            h.push_str(&format!(
                "<details open><summary>{}</summary>",
                escape_html(if ns.namespace.is_empty() { "(root)" } else { &ns.namespace })
            ));
            for c in &ns.commands {
                let key = format!("{}/{}/{}", c.key.realm, c.key.namespace, c.key.name);
                h.push_str(&button(&c.key.name, "command", &root, &key, &format!("{}\n{}", c.label, c.doc)));
            }
            h.push_str("</details>");
        }
        h.push_str("</div>");
    }
    if let Some(browser) = &tree.browser {
        h.push_str("<div class=\"lq-ed-browser\">");
        h.push_str(&button("✕", "closebrowser", &root, "", "Close"));
        match &browser.listing {
            None => h.push_str("<em>Listing…</em>"),
            Some(listing) => {
                h.push_str(&format!("<code>/{}</code>", escape_html(&listing.folder)));
                if let Some(e) = &listing.error {
                    h.push_str(&format!("<div class=\"lq-ed-note\">{}</div>", escape_html(e)));
                }
                for entry in &listing.entries {
                    let key = if listing.folder.is_empty() {
                        entry.name.clone()
                    } else {
                        format!("{}/{}", listing.folder, entry.name)
                    };
                    if entry.is_dir {
                        h.push_str(&button(&format!("📁 {}", entry.name), "browse", &browser.path, &key, &key));
                    } else {
                        h.push_str(&button(&entry.name, "pick", &browser.path, &key, &key));
                    }
                }
            }
        }
        h.push_str("</div>");
    }
    if let Some(c) = &tree.confirmation {
        h.push_str("<div class=\"lq-ed-confirm\"><p>This changes which command these actions run:</p><ul>");
        for line in &c.changes {
            h.push_str(&format!("<li>{}</li>", escape_html(line)));
        }
        h.push_str("</ul>");
        h.push_str(&button("Confirm", "confirm", &root, "", ""));
        h.push_str(&button("Cancel", "cancel", &root, "", ""));
        h.push_str("</div>");
    }
    h.push_str("</div>");
    h
}

fn node_buttons(h: &mut String, path: &NodePath) {
    if path.0.is_empty() {
        return;
    }
    h.push_str(&button("+", "picker", path, "", "Insert a command before"));
    h.push_str(&button("↑", "up", path, "", "Move up"));
    h.push_str(&button("↓", "down", path, "", "Move down"));
    h.push_str(&button("✕", "remove", path, "", "Delete"));
}

fn node_html(h: &mut String, node: &EditorNode) {
    match node {
        EditorNode::Resource { path, header, key, note } => {
            h.push_str("<div class=\"lq-ed-resource\">resource ");
            if let Some(header) = header {
                let name = format!("header:{}", path.id());
                h.push_str(&format!("<select{}{}>", field_attr(&name), change(&name)));
                for option in RESOURCE_HEADERS {
                    h.push_str(&format!(
                        "<option value=\"{}\"{}>{}</option>",
                        escape_html(option),
                        if option == header { " selected" } else { "" },
                        escape_html(option)
                    ));
                }
                h.push_str("</select>");
            }
            h.push_str(&format!(
                "<input type=\"text\" value=\"{}\"{}{}>",
                escape_html(key),
                field_attr(&format!("key:{}", path.id())),
                event_attr(&UIEventKind::Submit)
            ));
            let folder = key.rsplit_once('/').map(|(f, _)| f.to_string()).unwrap_or_default();
            h.push_str(&button("📁", "browse", path, &folder, "Browse the store"));
            node_buttons(h, path);
            if let Some(note) = note {
                h.push_str(&format!("<em>{}</em>", escape_html(note)));
            }
            h.push_str("</div>");
        }
        EditorNode::Namespaces { path, list, unused } => {
            h.push_str("<div class=\"lq-ed-ns\">ns ");
            h.push_str(&format!(
                "<input type=\"hidden\" value=\"{}\"{}>",
                escape_html(&list.join(",")),
                field_attr(&format!("ns:{}", path.id()))
            ));
            for (i, ns) in list.iter().enumerate() {
                h.push_str(&format!("<code>{}</code>", escape_html(ns)));
                h.push_str(&button("↑", "nsup", path, &i.to_string(), "Earlier"));
                h.push_str(&button("↓", "nsdown", path, &i.to_string(), "Later"));
                h.push_str(&button("✕", "nsdel", path, &i.to_string(), "Remove"));
            }
            if !unused.is_empty() {
                h.push_str(&format!("<select{}>", field_attr(&format!("nsnew:{}", path.id()))));
                for ns in unused {
                    h.push_str(&format!("<option value=\"{0}\">{0}</option>", escape_html(ns)));
                }
                h.push_str("</select>");
                h.push_str(&button("+", "nsadd", path, "", "Append the namespace"));
            }
            h.push_str("</div>");
        }
        EditorNode::Action { path, command, label, doc, presets, next, filename, fields, excess, .. } => {
            h.push_str(&format!(
                "<details open class=\"lq-ed-action\"><summary title=\"{}\">{}</summary>",
                escape_html(&format!("{label}\n{doc}")),
                escape_html(&command.name)
            ));
            for (i, p) in presets.iter().enumerate() {
                h.push_str(&button(&format!("preset: {}", p.label), "preset", path, &i.to_string(), &p.description));
            }
            for (i, p) in next.iter().enumerate() {
                h.push_str(&button(&format!("next: {}", p.label), "next", path, &i.to_string(), &p.description));
            }
            if let Some(name) = filename {
                h.push_str(&button("filename", "filenamefrom", path, "", name));
            }
            node_buttons(h, path);
            h.push_str("<table class=\"lq-ed-fields\">");
            for field in fields {
                field_html(h, path, field);
            }
            h.push_str("</table>");
            if !excess.is_empty() {
                h.push_str(&format!("<em>Extra parameters (not used): {}</em>", escape_html(&excess.join(", "))));
            }
            h.push_str("</details>");
        }
        EditorNode::Directive { path, name } => {
            h.push_str(&format!("<div class=\"lq-ed-directive\"><code>{}</code>", escape_html(name)));
            node_buttons(h, path);
            h.push_str("</div>");
        }
        EditorNode::Value { json, .. } => {
            h.push_str(&format!("<code class=\"lq-ed-value\">{}</code>", escape_html(json)));
        }
        EditorNode::Nested { label, nodes, .. } => {
            h.push_str(&format!("<details class=\"lq-ed-nested\"><summary>{}</summary>", escape_html(label)));
            for n in nodes {
                node_html(h, n);
            }
            h.push_str("</details>");
        }
        EditorNode::Opaque { path, text, reason } => {
            h.push_str(&format!(
                "<div class=\"lq-ed-opaque\" title=\"{}\"><code>{}</code> <em>{}</em>",
                escape_html(reason),
                escape_html(text),
                escape_html(reason)
            ));
            node_buttons(h, path);
            h.push_str("</div>");
        }
    }
}

fn input_html(field: &EditorField, name: &str, text: &str) -> String {
    let shown = if text.is_empty() { field.default_text.clone().unwrap_or_default() } else { text.to_string() };
    let attrs = field_attr(name);
    match &field.widget {
        ArgumentGUIInfo::Checkbox | ArgumentGUIInfo::RadioBoolean { .. } => format!(
            "<input type=\"checkbox\"{attrs}{}{}>",
            change(name),
            if matches!(shown.as_str(), "true" | "t" | "yes" | "y" | "1") { " checked" } else { "" }
        ),
        ArgumentGUIInfo::IntegerSlider { min, max, step } => format!(
            "<input type=\"range\" min=\"{min}\" max=\"{max}\" step=\"{step}\" value=\"{}\"{attrs}{}>",
            escape_html(&shown),
            change(name)
        ),
        ArgumentGUIInfo::IntegerRange { min, max } => format!(
            "<input type=\"number\" min=\"{min}\" max=\"{max}\" value=\"{}\"{attrs}{}>",
            escape_html(text),
            event_attr(&UIEventKind::Submit)
        ),
        ArgumentGUIInfo::FloatSlider { min, max, step } => format!(
            "<input type=\"range\" min=\"{min}\" max=\"{max}\" step=\"{step}\" value=\"{}\"{attrs}{}>",
            escape_html(&shown),
            change(name)
        ),
        ArgumentGUIInfo::IntegerField | ArgumentGUIInfo::FloatField => format!(
            "<input type=\"number\" value=\"{}\" placeholder=\"{}\"{attrs}{}>",
            escape_html(text),
            escape_html(&field.default_text.clone().unwrap_or_default()),
            event_attr(&UIEventKind::Submit)
        ),
        ArgumentGUIInfo::ColorString => format!(
            "<input type=\"text\" value=\"{}\"{attrs}{}>",
            escape_html(text),
            event_attr(&UIEventKind::Submit)
        ),
        ArgumentGUIInfo::DateField(_) => format!(
            "<input type=\"text\" placeholder=\"YYYYMMDD\" value=\"{}\"{attrs}{}>",
            escape_html(text),
            event_attr(&UIEventKind::Submit)
        ),
        ArgumentGUIInfo::EnumSelector | ArgumentGUIInfo::HorizontalRadioEnum | ArgumentGUIInfo::VerticalRadioEnum
            if !field.alternatives.is_empty() && !field.others_allowed =>
        {
            let mut s = format!("<select{attrs}{}>", change(name));
            for a in &field.alternatives {
                s.push_str(&format!(
                    "<option value=\"{0}\"{1}>{0}</option>",
                    escape_html(a),
                    if *a == shown { " selected" } else { "" }
                ));
            }
            s.push_str("</select>");
            s
        }
        ArgumentGUIInfo::EnumSelector | ArgumentGUIInfo::HorizontalRadioEnum | ArgumentGUIInfo::VerticalRadioEnum => {
            let list = format!("{name}-list");
            let mut s = format!(
                "<input type=\"text\" list=\"{}\" value=\"{}\"{attrs}{}><datalist id=\"{}\">",
                escape_html(&list),
                escape_html(text),
                event_attr(&UIEventKind::Submit),
                escape_html(&list)
            );
            for a in &field.alternatives {
                s.push_str(&format!("<option value=\"{}\">", escape_html(a)));
            }
            s.push_str("</datalist>");
            s
        }
        ArgumentGUIInfo::TextArea(_, rows) | ArgumentGUIInfo::CodeArea(_, rows, _) => format!(
            "<textarea rows=\"{rows}\"{attrs}>{}</textarea>",
            escape_html(text)
        ),
        ArgumentGUIInfo::Hide => String::new(),
        ArgumentGUIInfo::TextField(_) | ArgumentGUIInfo::CodeField(_, _) | ArgumentGUIInfo::None => format!(
            "<input type=\"text\" value=\"{}\" placeholder=\"{}\"{attrs}{}>",
            escape_html(text),
            escape_html(&field.default_text.clone().unwrap_or_default()),
            event_attr(&UIEventKind::Submit)
        ),
    }
}

fn field_html(h: &mut String, path: &NodePath, field: &EditorField) {
    h.push_str(&format!(
        "<tr><th title=\"{}\">{}</th><td>",
        escape_html(&field.hover()),
        escape_html(&field.label)
    ));
    let name = field_id(path, &field.name, None);
    match &field.value {
        FieldValue::Link { query, nodes, .. } => {
            h.push_str(&format!(
                "<input type=\"text\" value=\"{}\"{}{}>",
                escape_html(query),
                field_attr(&name),
                event_attr(&UIEventKind::Submit)
            ));
            h.push_str("<details open class=\"lq-ed-link\"><summary>link</summary>");
            for n in nodes {
                node_html(h, n);
            }
            h.push_str("</details>");
        }
        FieldValue::Items(items) => items_html(h, path, field, items),
        FieldValue::Other(text) => h.push_str(&format!("<em>{}</em>", escape_html(text))),
        FieldValue::Text(text) if !field.multiple => h.push_str(&input_html(field, &name, text)),
        FieldValue::Unset if !field.multiple => h.push_str(&input_html(field, &name, "")),
        FieldValue::Text(_) | FieldValue::Unset => items_html(h, path, field, &[]),
    }
    h.push_str("</td><td>");
    for (i, p) in field.presets.iter().enumerate() {
        h.push_str(&button(&p.label, "argpreset", path, &format!("{}:{i}", field.name), &p.description));
    }
    if matches!(field.value, FieldValue::Link { .. }) {
        h.push_str(&button("value", "tovalue", path, &field.name, "Make it a value"));
    } else {
        h.push_str(&button("link", "tolink", path, &field.name, "Make it a link"));
    }
    h.push_str(&button("↺", "reset", path, &field.name, "Reset to the default"));
    h.push_str("</td></tr>");
}

fn items_html(h: &mut String, path: &NodePath, field: &EditorField, items: &[FieldValue]) {
    for (k, item) in items.iter().enumerate() {
        let name = field_id(path, &field.name, Some(k));
        match item {
            FieldValue::Text(text) => h.push_str(&input_html(field, &name, text)),
            FieldValue::Link { query, .. } => h.push_str(&format!("<code>{}</code>", escape_html(query))),
            FieldValue::Unset | FieldValue::Items(_) | FieldValue::Other(_) => h.push_str("<em>…</em>"),
        }
        let at = format!("{}:{k}", field.name);
        h.push_str(&button("↑", "itemup", path, &at, "Earlier"));
        h.push_str(&button("↓", "itemdown", path, &at, "Later"));
        h.push_str(&button("✕", "delitem", path, &at, "Remove"));
    }
    h.push_str(&button("+", "additem", path, &format!("{}:{}", field.name, items.len()), "Append an item"));
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;

    use liquers_core::command_metadata::{ArgumentInfo, CommandMetadata, CommandMetadataRegistry};
    use liquers_core::value::ValueInterface;

    use crate::ui::editor::{EditorCommand, EditorKind, EditorState, EditorView};
    use crate::ui::event::UIEvent;
    use crate::value::Value;

    fn state() -> EditorState {
        let mut cmr = CommandMetadataRegistry::new();
        let mut is_in = CommandMetadata::new("is_in");
        is_in
            .with_namespace("pl")
            .with_argument(ArgumentInfo::string_argument("column"))
            .with_argument(ArgumentInfo::string_argument("values").set_multiple());
        cmr.add_command(&is_in);
        let mut sort = CommandMetadata::new("sort");
        sort.with_namespace("pl")
            .with_argument(ArgumentInfo::boolean_argument("ascending").with_default(true));
        cmr.add_command(&sort);
        let mut state = EditorState::new(EditorKind::Query, Arc::new(cmr));
        let q = liquers_core::parse::parse_query("-R/a.csv/-/ns-pl/is_in-region-EU/sort").expect("query");
        state.on_snapshot(&Value::from_query(&q), None).expect("snapshot");
        state
    }

    /// query-pipeline-editor AC-11, AC-3: fields, events and hover text are in the markup.
    #[test]
    fn editor_html_marks_fields_and_events() {
        let html = editor_html(state().tree(), "Editor");
        assert!(html.contains("data-lq-field=\"text\""), "{html}");
        assert!(html.contains("data-lq-field=\"f:1:column\""), "{html}");
        assert!(html.contains("data-lq-field=\"f:1:values:0\""), "{html}");
        assert!(html.contains("data-lq-event=\"ed:additem:1:values:1\""), "{html}");
        assert!(html.contains("type=\"checkbox\""), "{html}");
        assert!(html.contains("data-lq-event=\"change:f:2:ascending\""), "{html}");
        assert!(html.contains("<select data-lq-field=\"header:0\""), "{html}");
        assert!(html.contains("title=\"column (String)"), "{html}");
    }

    /// query-pipeline-editor AC-11, AC-2: browser events decode to the editor's commands.
    #[test]
    fn ui_events_decode_to_commands() {
        let mut state = state();
        let tree = state.tree().clone();
        let submit = UIEvent::new(UIEventKind::Submit)
            .with_field("text", Value::from(tree.text.clone()))
            .with_field("f:1:column", Value::from("country".to_string()));
        let commands = EditorView::from_ui_event(&submit, &tree);
        assert_eq!(
            commands,
            vec![EditorCommand::SetField { path: NodePath::element(1), arg: "column".into(), text: "country".into() }]
        );
        let add = UIEvent::new(UIEventKind::Named("ed:additem:1:values:1".into()));
        assert!(matches!(EditorView::from_ui_event(&add, &tree)[0], EditorCommand::InsertItem { at: 1, .. }));
        let toggle = UIEvent::new(UIEventKind::Change("f:2:ascending".into()))
            .with_field("f:2:ascending", Value::from("false".to_string()));
        let commands = EditorView::from_ui_event(&toggle, &tree);
        assert_eq!(commands.len(), 1);
        for c in commands {
            state.apply(c).expect("apply");
        }
        assert_eq!(state.tree().text, "-R/a.csv/-/ns-pl/is_in-region-EU/sort-false");
    }
}
