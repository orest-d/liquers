//! Browser markup of the query, action, resource and key editors (`crate::ui::editor`).
//!
//! [`editor_html`] renders an [`EditorTree`] in a mode and a density as plain HTML, with the
//! layout the egui renderer draws ([`editor_layout`]): Simple mode shows step chips and one card,
//! Advanced mode every card, closed except one (the last action, or the step named by
//! [`editor_html_open`]). Cards are `<details>` (the browser keeps their open state), with the header of [`header_parts`], the toolbar of
//! [`toolbar_items`] with presets as chips, and a CSS grid form of [`form_rows`]. Hover text is
//! `title=` (an argument's description and default are only there), fields carry `data-lq-field` names and buttons `data-lq-event` names that
//! [`crate::ui::editor::EditorView::from_ui_event`] decodes back into commands (and
//! [`crate::ui::editor::EditorView::mode_event`] into a mode). A selector or a checkbox sends
//! `change:<field>` on the DOM `change` event; Enter in a text field submits every changed field.
//! Lists reorder with up and down buttons (`UI-EVENTS-HAVE-NO-POINTER-DRAG-OR-CHANGE-KINDS`).
//!
//! Colours and sizes are CSS custom properties of the theme ([`Theme::to_css`]); [`EDITOR_CSS`]
//! is the editor's own stylesheet over them, which the mount writes with the theme.
//!
//! # Main components
//!
//! - [`editor_html`]: the whole editor; [`editor_html_open`] names the card Advanced mode opens.
//! - [`icon_html`]: an icon's first variant the browser can draw: a Phosphor web-font glyph, an
//!   SVG (sanitized) or raster image as a `data:` URI, else its Unicode text.
//! - [`EDITOR_CSS`]: the editor's stylesheet.
//!
//! # Where it is used
//!
//! - `EditorElement::render_web`, for server-side rendering and the browser driver.
//!
//! # Related concepts
//!
//! - `crate::egui::editor`: the egui rendering of the same tree and layout.
//! - [`super::html::field_attr`] and [`super::html::event_attr`]: the markup conventions.
//!
//! Design: `specs/design/query-editor-ergonomics/` (AC-1 to AC-7, AC-11, AC-17).
//!
//! # Examples
//!
//! ```
//! use liquers_lib::ui::editor::{EditorMode, EditorTree};
//! use liquers_lib::ui::theme::{Density, Theme};
//! use liquers_lib::ui::web::editor::editor_html;
//!
//! let tree = EditorTree { text: "-R/data.csv".into(), text_editable: true, ..Default::default() };
//! let html = editor_html(&tree, "Editor", EditorMode::Simple, Density::Compact, &Theme::builtin());
//! assert!(html.contains("data-lq-field=\"text\""));
//! assert!(html.contains("data-lq-density=\"compact\""));
//! ```

use base64::Engine;
use liquers_core::command_metadata::ArgumentGUIInfo;

use super::html::{escape_html, event_attr, field_attr};
use crate::ui::editor::{
    editor_layout, event_name, field_id, form_rows, header_parts, toolbar_items, CardGroupKind, EditorField,
    EditorMode, EditorNode, EditorTree, FieldValue, HeaderPart, NodePath, StepChip, ToolItem, RESOURCE_HEADERS,
};
use crate::ui::event::UIEventKind;
use crate::ui::theme::{Density, IconSource, IconVariant, ResolvedIcon, Theme, PHOSPHOR};

/// The most chips a card's toolbar shows before "+N" (AC-7).
const PRESET_CHIPS: usize = 3;

/// The editor's stylesheet, over the theme's custom properties.
pub const EDITOR_CSS: &str = r#"
.lq-editor{background:var(--lq-panel);color:var(--lq-ink);font-family:var(--lq-font-ui),sans-serif;font-size:var(--lq-size-body);padding:8px}
.lq-editor button,.lq-editor input,.lq-editor select,.lq-editor textarea{background:var(--lq-control);color:var(--lq-ink);border:1px solid var(--lq-line);border-radius:6px;min-height:var(--lq-control-height);font:inherit}
.lq-editor button:hover{background:var(--lq-control-hover)}
.lq-ed-toolbar,.lq-ed-chips,.lq-ed-footer,.lq-card-tools{display:flex;flex-wrap:wrap;align-items:center;gap:6px;margin:4px 0}
.lq-mode-on{background:var(--lq-accent-wash)!important;color:var(--lq-accent)!important}
.lq-ed-text{width:100%;font-family:var(--lq-font-mono),monospace}
.lq-query{font-family:var(--lq-font-mono),monospace}
.lq-ed-chip{color:var(--lq-ink-muted)}
.lq-ed-body{display:flex;gap:12px;align-items:flex-start}
.lq-ed-cards{flex:1;display:flex;flex-direction:column;gap:var(--lq-card-gap)}
.lq-card{background:var(--lq-card);border:1px solid var(--lq-line);border-radius:var(--lq-card-radius);padding:var(--lq-card-padding);box-shadow:0 1px 3px var(--lq-shadow)}
.lq-card.lq-err{border-color:var(--lq-danger)}
.lq-card-head{display:flex;align-items:center;gap:8px;cursor:pointer}
.lq-icon{display:inline-flex;align-items:center;justify-content:center;width:var(--lq-icon-size);height:var(--lq-icon-size);border-radius:6px;color:var(--lq-card-hue,var(--lq-accent));background:color-mix(in srgb,var(--lq-card-hue,var(--lq-accent)) 22%,transparent)}
.lq-icon img{width:80%;height:80%}
.lq-icon i::before{content:attr(data-fallback);font-style:normal}
.lq-id,.lq-realm,.lq-pos{color:var(--lq-ink-faint);font-size:var(--lq-size-small);font-family:var(--lq-font-mono),monospace}
.lq-label{font-weight:600}
.lq-mod{color:var(--lq-modified)}
.lq-errmark{color:var(--lq-danger)}
.lq-head-end{margin-left:auto;display:flex;gap:4px;align-items:center}
.lq-chip.lq-active{background:var(--lq-accent);color:var(--lq-on-accent)}
.lq-delete{margin-left:auto;color:var(--lq-danger)}
.lq-form{display:grid;grid-template-columns:var(--lq-label-width) 1fr auto;gap:var(--lq-row-gap) 8px;align-items:start;margin-top:6px}
.lq-flabel{text-align:right;color:var(--lq-ink-muted);padding-top:4px}
.lq-help{color:var(--lq-ink-faint);font-size:var(--lq-size-small)}
.lq-fmsg{color:var(--lq-danger);font-size:var(--lq-size-small)}
.lq-factions{opacity:.45}
.lq-form>div:hover+.lq-factions,.lq-factions:hover,.lq-factions:focus-within{opacity:1}
.lq-link-tree{border-left:1px dashed var(--lq-line-strong);padding-left:8px;margin-top:4px}
.lq-qc-body{display:flex;gap:8px;align-items:flex-start}
.lq-qc-content{flex:1;min-width:0}
.lq-qc-editor{flex:0 0 340px;max-width:45%;overflow:auto;border-left:1px solid var(--lq-line)}
"#;

fn button(label: &str, verb: &str, path: &NodePath, rest: &str, title: &str) -> String {
    button_class(label, verb, path, rest, title, "lq-ed-btn")
}

fn button_class(label: &str, verb: &str, path: &NodePath, rest: &str, title: &str, class: &str) -> String {
    format!(
        "<button type=\"button\" class=\"{}\" title=\"{}\"{}>{}</button>",
        escape_html(class),
        escape_html(title),
        event_attr(&UIEventKind::Named(event_name(verb, path, rest))),
        escape_html(label)
    )
}

fn change(field: &str) -> String {
    event_attr(&UIEventKind::Change(field.to_string()))
}

/// The name of a mode, as `ed:mode::<name>` events carry it.
fn mode_name(mode: EditorMode) -> &'static str {
    match mode {
        EditorMode::Simple => "simple",
        EditorMode::Advanced => "advanced",
    }
}

fn density_name(density: Density) -> &'static str {
    match density {
        Density::Comfortable => "comfortable",
        Density::Compact => "compact",
    }
}

/// Removes `<script>` elements and `on*` attributes from SVG text.
fn sanitize_svg(svg: &str) -> String {
    let mut out = String::with_capacity(svg.len());
    let mut rest = svg;
    // Drop <script ...>...</script> (case-insensitive).
    loop {
        let lower = rest.to_ascii_lowercase();
        match lower.find("<script") {
            Some(start) => {
                out.push_str(&rest[..start]);
                let end = lower[start..].find("</script>").map(|e| start + e + "</script>".len());
                match end {
                    Some(end) => rest = &rest[end..],
                    None => {
                        rest = "";
                        break;
                    }
                }
            }
            None => break,
        }
    }
    out.push_str(rest);
    // Drop on* attributes: ` onxxx="..."` or `'...'`.
    let mut clean = String::with_capacity(out.len());
    let bytes = out.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        let at_attr = bytes[i].is_ascii_whitespace()
            && bytes.get(i + 1).is_some_and(|b| b.eq_ignore_ascii_case(&b'o'))
            && bytes.get(i + 2).is_some_and(|b| b.eq_ignore_ascii_case(&b'n'))
            && bytes.get(i + 3).is_some_and(|b| b.is_ascii_alphabetic());
        if at_attr {
            let mut j = i + 1;
            while j < bytes.len() && (bytes[j].is_ascii_alphanumeric() || bytes[j] == b'-') {
                j += 1;
            }
            while j < bytes.len() && bytes[j].is_ascii_whitespace() {
                j += 1;
            }
            if bytes.get(j) == Some(&b'=') {
                j += 1;
                while j < bytes.len() && bytes[j].is_ascii_whitespace() {
                    j += 1;
                }
                match bytes.get(j) {
                    Some(q @ (b'"' | b'\'')) => {
                        let q = *q;
                        j += 1;
                        while j < bytes.len() && bytes[j] != q {
                            j += 1;
                        }
                        j += 1;
                    }
                    Some(_) | None => {
                        while j < bytes.len() && !bytes[j].is_ascii_whitespace() && bytes[j] != b'>' && bytes[j] != b'/' {
                            j += 1;
                        }
                    }
                }
            }
            i = j.min(bytes.len());
            continue;
        }
        // Push the whole UTF-8 character starting at i.
        let ch_len = out[i..].chars().next().map(char::len_utf8).unwrap_or(1);
        clean.push_str(&out[i..i + ch_len]);
        i += ch_len;
    }
    clean
}

/// The media type of raster image bytes, by their signature.
fn raster_type(bytes: &[u8]) -> Option<&'static str> {
    if bytes.starts_with(b"\x89PNG") {
        Some("image/png")
    } else if bytes.starts_with(&[0xFF, 0xD8]) {
        Some("image/jpeg")
    } else if bytes.len() > 12 && &bytes[..4] == b"RIFF" && &bytes[8..12] == b"WEBP" {
        Some("image/webp")
    } else {
        None
    }
}

/// An icon as HTML: its first variant the browser can draw, else its Unicode text (AC-17). A
/// Phosphor icon uses the Phosphor web font (`<i class="ph ph-<name>">`; without the font,
/// [`EDITOR_CSS`] shows the Unicode text instead); an image given by key
/// is skipped (the browser renderer has no icon loader yet).
pub fn icon_html(icon: &ResolvedIcon<'_>) -> String {
    let b64 = |bytes: &[u8]| base64::engine::general_purpose::STANDARD.encode(bytes);
    for variant in &icon.variants {
        let variant: &IconVariant = variant;
        match variant {
            IconVariant::Set { set, name } => {
                if set.as_deref() == Some(PHOSPHOR) && crate::ui::theme::is_icon_name(name) {
                    return format!(
                        "<i class=\"ph ph-{}\" aria-hidden=\"true\" data-fallback=\"{}\"></i>",
                        escape_html(name),
                        escape_html(&icon.unicode)
                    );
                }
            }
            IconVariant::Library(_) => {}
            IconVariant::Svg(IconSource::Inline(bytes)) => {
                if let Ok(text) = std::str::from_utf8(bytes.as_slice()) {
                    let clean = sanitize_svg(text);
                    return format!("<img alt=\"\" src=\"data:image/svg+xml;base64,{}\">", b64(clean.as_bytes()));
                }
            }
            IconVariant::Raster(IconSource::Inline(bytes)) => {
                if let Some(mime) = raster_type(bytes.as_slice()) {
                    return format!("<img alt=\"\" src=\"data:{mime};base64,{}\">", b64(bytes.as_slice()));
                }
            }
            IconVariant::Svg(IconSource::Key(_)) | IconVariant::Raster(IconSource::Key(_)) => {}
        }
    }
    format!("<span class=\"lq-uni\">{}</span>", escape_html(&icon.unicode))
}

fn node_icon<'t>(theme: &'t Theme, node: &'t EditorNode, fallback: &'t IconVariant) -> ResolvedIcon<'t> {
    match node {
        EditorNode::Action { command, label, hint_icon, .. } => theme.icon_for(command, label, hint_icon.as_ref(), None),
        EditorNode::Resource { .. }
        | EditorNode::Namespaces { .. }
        | EditorNode::Directive { .. }
        | EditorNode::Value { .. }
        | EditorNode::Nested { .. }
        | EditorNode::Opaque { .. } => ResolvedIcon { variants: vec![std::borrow::Cow::Borrowed(fallback)], unicode: "▤".into(), hue: None },
    }
}

fn fallback_icon(node: &EditorNode) -> IconVariant {
    let name = match node {
        EditorNode::Resource { .. } => "file",
        EditorNode::Namespaces { .. } => "tag",
        EditorNode::Directive { .. } => "quotes",
        EditorNode::Value { .. } => "brackets-curly",
        EditorNode::Nested { .. } => "tree-structure",
        EditorNode::Opaque { .. } | EditorNode::Action { .. } => "question",
    };
    IconVariant::Set { set: Some(PHOSPHOR.into()), name: name.into() }
}

/// The editor's markup in `mode` and `density`, with the icons of `theme`.
pub fn editor_html(tree: &EditorTree, title: &str, mode: EditorMode, density: Density, theme: &Theme) -> String {
    editor_html_open(tree, title, mode, density, theme, None)
}

/// [`editor_html`], with the top-level card Advanced mode shows open: `open`, else the card
/// Simple mode shows (normally the last action). Every other card starts closed.
pub fn editor_html_open(
    tree: &EditorTree,
    title: &str,
    mode: EditorMode,
    density: Density,
    theme: &Theme,
    open: Option<&NodePath>,
) -> String {
    let open = match open {
        Some(path) => Some(path.clone()),
        None => crate::ui::editor::open_card(tree, None),
    };
    let root = NodePath::default();
    let mut h = String::new();
    h.push_str(&format!(
        "<div class=\"lq-editor\" title=\"{}\" data-lq-mode=\"{}\" data-lq-density=\"{}\">",
        escape_html(title),
        mode_name(mode),
        density_name(density)
    ));
    // Mode switch and toolbar (AC-1, AC-2, AC-26 of the previous design).
    h.push_str("<div class=\"lq-ed-toolbar\">");
    for m in [EditorMode::Simple, EditorMode::Advanced] {
        let class = if m == mode { "lq-ed-btn lq-mode-on" } else { "lq-ed-btn" };
        let label = match m {
            EditorMode::Simple => "Simple",
            EditorMode::Advanced => "Advanced",
        };
        h.push_str(&button_class(label, "mode", &root, mode_name(m), "Simple shows the last step; Advanced every step (Ctrl+B)", class));
    }
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
    // Text (AC-4, AC-10, AC-15).
    if tree.text_editable {
        h.push_str(&format!(
            "<input type=\"text\" class=\"lq-ed-text\" value=\"{}\"{}{}>",
            escape_html(&tree.text),
            field_attr("text"),
            event_attr(&UIEventKind::Submit)
        ));
    }
    if !tree.hide_text {
        h.push_str(&super::widgets::query_to_html(&tree.text));
    }
    if let Some(note) = &tree.text_note {
        h.push_str(&format!("<div class=\"lq-ed-note\">{}</div>", escape_html(note)));
    }
    if let Some((path, message)) = &tree.message {
        if path.0.is_empty() {
            h.push_str(&format!("<div class=\"lq-ed-message\">{}</div>", escape_html(message)));
        }
    }
    // The layout of the mode.
    let layout = editor_layout(tree, mode);
    if !layout.before.is_empty() {
        chips_html(&mut h, &layout.before);
    }
    h.push_str("<div class=\"lq-ed-body\">");
    h.push_str("<div class=\"lq-ed-cards\">");
    let mut position = 0;
    for group in &layout.groups {
        let part = match group.kind {
            CardGroupKind::KeyPart => "key",
            CardGroupKind::QueryPart => "query",
        };
        h.push_str(&format!("<section class=\"lq-group\" data-part=\"{part}\">"));
        for card in &group.cards {
            position += 1;
            let is_open = open.as_ref() == Some(card.node.path());
            card_html(&mut h, tree, card.node, mode, theme, position, is_open);
        }
        h.push_str("</section>");
    }
    h.push_str("</div></div>");
    if !layout.after.is_empty() {
        chips_html(&mut h, &layout.after);
    }
    // Footer.
    h.push_str("<div class=\"lq-ed-footer\">");
    if mode == EditorMode::Simple {
        if let Some(EditorNode::Action { path, next, .. }) = layout.groups.first().and_then(|g| g.cards.first()).map(|c| c.node) {
            if !next.is_empty() {
                h.push_str("<span class=\"lq-help\">Continue with</span>");
                for (i, p) in next.iter().enumerate() {
                    h.push_str(&button(&p.label, "next", path, &i.to_string(), &p.description));
                }
            }
        }
    }
    if crate::ui::editor::offers_key(tree) {
        h.push_str(&button("Add key…", "addkey", &root, "", "Start the query with a key (a resource)"));
    }
    if tree.has_filename {
        h.push_str(&button("Add action…", "picker", &root, "", "Append a command (Ctrl+K)"));
        if mode == EditorMode::Advanced {
            h.push_str(&button("+ q", "append", &root, "q", "Append a quote directive"));
            h.push_str(&button("+ v", "append", &root, "v", "Append a volatile directive"));
        }
    }
    h.push_str("</div>");
    if tree.has_filename && !tree.hide_text && mode == EditorMode::Advanced {
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
    windows_html(&mut h, tree);
    h.push_str("</div>");
    h
}

fn chips_html(h: &mut String, chips: &[StepChip]) {
    h.push_str("<div class=\"lq-ed-chips\">");
    for (i, chip) in chips.iter().enumerate() {
        if i > 0 {
            h.push_str("<span class=\"lq-help\">›</span>");
        }
        h.push_str(&button_class(&chip.label, "mode", &chip.path, "advanced", "Open in Advanced", "lq-ed-btn lq-ed-chip"));
    }
    h.push_str("</div>");
}

fn card_html(h: &mut String, tree: &EditorTree, node: &EditorNode, mode: EditorMode, theme: &Theme, position: usize, open: bool) {
    let path = node.path();
    let fallback = fallback_icon(node);
    let icon = node_icon(theme, node, &fallback);
    let ns = match node {
        EditorNode::Action { command, .. } => command.namespace.clone(),
        EditorNode::Resource { .. }
        | EditorNode::Namespaces { .. }
        | EditorNode::Directive { .. }
        | EditorNode::Value { .. }
        | EditorNode::Nested { .. }
        | EditorNode::Opaque { .. } => String::new(),
    };
    let hue = icon.hue.map(|h| format!(" style=\"--lq-card-hue:var(--lq-hue-{})\"", escape_html(h))).unwrap_or_default();
    let refused = tree.message.as_ref().is_some_and(|(p, _)| p == path);
    let class = if refused { "lq-card lq-err" } else { "lq-card" };
    let (open_tag, close_tag, head_tag) = match mode {
        EditorMode::Advanced if open => ("<details open", "</details>", "summary"),
        EditorMode::Advanced => ("<details", "</details>", "summary"),
        EditorMode::Simple => ("<div", "</div>", "div"),
    };
    h.push_str(&format!(
        "{open_tag} class=\"{class}\" data-ns=\"{}\" data-path=\"{}\"{hue}>",
        escape_html(&ns),
        escape_html(&path.id())
    ));
    // Header (AC-4): collapsed order in Advanced, where the summary is the collapsed view.
    h.push_str(&format!("<{head_tag} class=\"lq-card-head\">"));
    let parts = header_parts(tree, node, mode == EditorMode::Advanced, position);
    let mut end_open = false;
    for part in &parts {
        match part {
            HeaderPart::Chevron => {}
            HeaderPart::Icon => h.push_str(&format!("<span class=\"lq-icon\">{}</span>", icon_html(&icon))),
            HeaderPart::Realm(r) => h.push_str(&format!("<span class=\"lq-realm\">[{}]</span>", escape_html(r))),
            HeaderPart::Label(l) => h.push_str(&format!("<span class=\"lq-label\">{}</span>", escape_html(l))),
            HeaderPart::CommandId(id) => h.push_str(&format!("<span class=\"lq-id\">{}</span>", escape_html(id))),
            HeaderPart::Modified | HeaderPart::Error | HeaderPart::Position(_) | HeaderPart::Menu => {
                if !end_open {
                    h.push_str("<span class=\"lq-head-end\">");
                    end_open = true;
                }
                match part {
                    HeaderPart::Modified => h.push_str("<span class=\"lq-mod\" title=\"An argument differs from its default\">●</span>"),
                    HeaderPart::Error => h.push_str("<span class=\"lq-errmark\" title=\"This step has a problem\">⚠</span>"),
                    HeaderPart::Position(n) => h.push_str(&format!("<span class=\"lq-pos\">#{n}</span>")),
                    HeaderPart::Menu => {
                        if !path.0.is_empty() {
                            h.push_str(&button("+", "picker", path, "", "Insert a command before"));
                        }
                    }
                    HeaderPart::Chevron | HeaderPart::Icon | HeaderPart::Realm(_) | HeaderPart::Label(_) | HeaderPart::CommandId(_) => {}
                }
            }
        }
    }
    if end_open {
        h.push_str("</span>");
    }
    h.push_str(&format!("</{head_tag}>"));
    match node {
        EditorNode::Action { path, presets, next, filename, excess, doc, .. } => {
            if !doc.is_empty() {
                h.push_str(&format!("<div class=\"lq-help\">{}</div>", escape_html(doc)));
            }
            // Toolbar (AC-5, AC-7).
            h.push_str("<div class=\"lq-card-tools\">");
            for item in toolbar_items(node, mode) {
                match item {
                    ToolItem::Reset => {
                        h.push_str(&button("↺ Reset", "resetall", path, "", "Reset every argument to its default"));
                    }
                    ToolItem::Presets => {
                        for (i, p) in presets.iter().enumerate().take(PRESET_CHIPS) {
                            let class = if p.active { "lq-ed-btn lq-chip lq-active" } else { "lq-ed-btn lq-chip" };
                            h.push_str(&button_class(&format!("{} {}", i + 1, p.label), "preset", path, &i.to_string(), &p.description, class));
                        }
                        if presets.len() > PRESET_CHIPS {
                            h.push_str(&format!("<details class=\"lq-more\"><summary>+{}</summary>", presets.len() - PRESET_CHIPS));
                            for (i, p) in presets.iter().enumerate().skip(PRESET_CHIPS) {
                                let class = if p.active { "lq-ed-btn lq-chip lq-active" } else { "lq-ed-btn lq-chip" };
                                h.push_str(&button_class(&format!("{} {}", i + 1, p.label), "preset", path, &i.to_string(), &p.description, class));
                            }
                            h.push_str("</details>");
                        }
                    }
                    ToolItem::Next => {
                        h.push_str("<details class=\"lq-more\"><summary>Next</summary>");
                        for (i, p) in next.iter().enumerate() {
                            h.push_str(&button(&p.label, "next", path, &i.to_string(), &p.description));
                        }
                        h.push_str("</details>");
                    }
                    ToolItem::Filename => {
                        if let Some(name) = filename {
                            h.push_str(&button("Filename", "filenamefrom", path, "", name));
                        }
                    }
                    ToolItem::MoveUp => h.push_str(&button("↑", "up", path, "", "Move up")),
                    ToolItem::MoveDown => h.push_str(&button("↓", "down", path, "", "Move down")),
                    ToolItem::Delete => h.push_str(&button_class("Delete", "remove", path, "", "Delete this step", "lq-ed-btn lq-delete")),
                }
            }
            h.push_str("</div>");
            // Form grid (AC-6).
            h.push_str("<div class=\"lq-form\">");
            for row in form_rows(tree, node, None) {
                let field = row.field;
                let label = if row.required { format!("{} *", row.label) } else { row.label.clone() };
                h.push_str(&format!(
                    "<label class=\"lq-flabel\" title=\"{}\">{}</label><div class=\"lq-fedit\">",
                    escape_html(&field.hover()),
                    escape_html(&label)
                ));
                field_editor_html(h, tree, path, field, mode, theme);
                if let Some(message) = &row.message {
                    h.push_str(&format!("<div class=\"lq-fmsg\">{}</div>", escape_html(message)));
                }
                h.push_str("</div><div class=\"lq-factions\">");
                for (i, p) in field.presets.iter().enumerate() {
                    h.push_str(&button(&p.label, "argpreset", path, &format!("{}:{i}", field.name), &p.description));
                }
                if matches!(field.value, FieldValue::Link { .. }) {
                    h.push_str(&button("value", "tovalue", path, &field.name, "Make it a value"));
                } else {
                    h.push_str(&button("link", "tolink", path, &field.name, "Make it a link"));
                }
                h.push_str(&button("↺", "reset", path, &field.name, "Reset to the default"));
                h.push_str("</div>");
            }
            h.push_str("</div>");
            if !excess.is_empty() {
                h.push_str(&format!("<em>Extra parameters (not used): {}</em>", escape_html(&excess.join(", "))));
            }
        }
        EditorNode::Resource { .. }
        | EditorNode::Namespaces { .. }
        | EditorNode::Directive { .. }
        | EditorNode::Value { .. }
        | EditorNode::Nested { .. }
        | EditorNode::Opaque { .. } => {
            node_body_html(h, tree, node, mode, theme);
            if let Some((p, message)) = &tree.message {
                if p == path {
                    h.push_str(&format!("<div class=\"lq-fmsg\">{}</div>", escape_html(message)));
                }
            }
        }
    }
    h.push_str(close_tag);
}

fn field_editor_html(h: &mut String, tree: &EditorTree, path: &NodePath, field: &EditorField, mode: EditorMode, theme: &Theme) {
    let name = field_id(path, &field.name, None);
    match &field.value {
        FieldValue::Link { query, nodes, is_key } => {
            h.push_str(&format!(
                "<input type=\"text\" value=\"{}\"{}{}>",
                escape_html(query),
                field_attr(&name),
                event_attr(&UIEventKind::Submit)
            ));
            if mode == EditorMode::Advanced {
                h.push_str(&format!(
                    "<div class=\"lq-link-tree\"><span class=\"lq-help\">{}</span>",
                    if *is_key { "key link" } else { "link" }
                ));
                for (i, n) in nodes.iter().enumerate() {
                    card_html(h, tree, n, mode, theme, i + 1, true);
                }
                h.push_str("</div>");
            }
        }
        FieldValue::Items(items) => items_html(h, path, field, items),
        FieldValue::Other(text) => h.push_str(&format!("<em>{}</em>", escape_html(text))),
        FieldValue::Text(text) if !field.multiple => h.push_str(&input_html(field, &name, text)),
        FieldValue::Unset if !field.multiple => h.push_str(&input_html(field, &name, "")),
        FieldValue::Text(_) | FieldValue::Unset => items_html(h, path, field, &[]),
    }
}

fn node_body_html(h: &mut String, tree: &EditorTree, node: &EditorNode, mode: EditorMode, theme: &Theme) {
    match node {
        EditorNode::Resource { path, header, key, note } => {
            h.push_str("<div class=\"lq-ed-resource\">");
            if let Some(header) = header {
                let name = format!("header:{}", path.id());
                h.push_str(&format!("<select{}{}>", field_attr(&name), change(&name)));
                for option in RESOURCE_HEADERS {
                    h.push_str(&format!(
                        "<option value=\"{}\"{}>{}</option>",
                        escape_html(option),
                        if option == header { " selected" } else { "" },
                        escape_html(if option.is_empty() { "(asset)" } else { option })
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
            if !path.0.is_empty() {
                h.push_str(&button("✕", "remove", path, "", "Delete"));
            }
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
        EditorNode::Directive { path, name } => {
            h.push_str(&format!(
                "<div class=\"lq-ed-directive lq-help\">{}</div>",
                if name == "q" { "Quote: pass the query so far as a value" } else { "Volatile: never cache" }
            ));
            if !path.0.is_empty() {
                h.push_str(&button("✕", "remove", path, "", "Delete"));
            }
        }
        EditorNode::Value { json, .. } => {
            h.push_str(&format!("<code class=\"lq-ed-value\">{}</code>", escape_html(json)));
        }
        EditorNode::Nested { nodes, .. } => {
            for (i, n) in nodes.iter().enumerate() {
                card_html(h, tree, n, mode, theme, i + 1, true);
            }
        }
        EditorNode::Opaque { path, text, reason } => {
            h.push_str(&format!(
                "<div class=\"lq-ed-opaque\" title=\"{}\"><code>{}</code> <em>{}</em>",
                escape_html(reason),
                escape_html(text),
                escape_html(reason)
            ));
            if !path.0.is_empty() {
                h.push_str(&button("✕", "remove", path, "", "Delete"));
            }
            h.push_str("</div>");
        }
        EditorNode::Action { .. } => {}
    }
}

fn windows_html(h: &mut String, tree: &EditorTree) {
    let root = NodePath::default();
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
        // A node of the folder's recipe tree, by filename and title, with Rename (design
        // `recipe-tree`, AC-18, AC-25).
        ArgumentGUIInfo::NodeSelector if !field.node_choices.is_empty() => {
            let mut s = format!("<select{attrs}{}>", change(name));
            if !field.node_choices.iter().any(|c| c.name == shown) {
                s.push_str(&format!("<option value=\"{0}\" selected>{0}</option>", escape_html(&shown)));
            }
            for c in &field.node_choices {
                let label = if c.title.is_empty() {
                    c.filename.clone()
                } else {
                    format!("{} \u{2014} {}", c.filename, c.title)
                };
                s.push_str(&format!(
                    "<option value=\"{}\"{}>{}</option>",
                    escape_html(&c.name),
                    if c.name == shown { " selected" } else { "" },
                    escape_html(&label)
                ));
            }
            s.push_str("</select>");
            // `name` is `f:<path>:<arg>`; the rename input and button carry the same pair.
            if let (Some(rest), false) = (name.strip_prefix("f:"), shown.is_empty()) {
                s.push_str(&format!(
                    "<input type=\"text\" size=\"10\" value=\"{}\"{}>",
                    escape_html(&shown),
                    field_attr(&format!("nrename:{rest}"))
                ));
                s.push_str(&format!(
                    "<button type=\"button\" class=\"lq-ed-btn\" title=\"Rename this node in the folder\"{}>Rename</button>",
                    event_attr(&UIEventKind::Named(format!("ed:rename:{rest}:{shown}")))
                ));
            }
            s
        }
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
        ArgumentGUIInfo::EnumSelector
        | ArgumentGUIInfo::HorizontalRadioEnum
        | ArgumentGUIInfo::VerticalRadioEnum
        | ArgumentGUIInfo::NodeSelector => {
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
        let html = editor_html(state().tree(), "Editor", EditorMode::Advanced, Density::Comfortable, &Theme::builtin());
        assert!(html.contains("data-lq-field=\"text\""), "{html}");
        assert!(html.contains("data-lq-field=\"f:1:column\""), "{html}");
        assert!(html.contains("data-lq-field=\"f:1:values:0\""), "{html}");
        assert!(html.contains("data-lq-event=\"ed:additem:1:values:1\""), "{html}");
        assert!(html.contains("type=\"checkbox\""), "{html}");
        assert!(html.contains("data-lq-event=\"change:f:2:ascending\""), "{html}");
        assert!(html.contains("<select data-lq-field=\"header:0\""), "{html}");
        assert!(html.contains("title=\"column (String)"), "{html}");
    }

    // Test 17 (AC-4, AC-6, AC-11, AC-17).
    #[test]
    fn cards_and_theme_css_in_html() {
        let tree = state().tree().clone();
        let html = editor_html(&tree, "Editor", EditorMode::Simple, Density::Compact, &Theme::builtin());
        assert!(html.contains("data-lq-density=\"compact\""), "{html}");
        assert!(html.contains("class=\"lq-card\" data-ns=\"pl\""), "{html}");
        assert!(html.contains("class=\"lq-form\""), "{html}");
        assert!(html.contains("lq-flabel"), "{html}");
        assert!(html.contains("--lq-card-hue:var(--lq-hue-teal)"), "{html}");
        assert!(html.contains("lq-tok-action"), "{html}");
        // Header order: icon, label, id.
        let icon = html.find("lq-icon").expect("icon");
        let label = html.find("lq-label").expect("label");
        assert!(icon < label);
        assert!(html.contains("data-lq-event=\"ed:mode::advanced\""), "{html}");
        let advanced = editor_html(&tree, "Editor", EditorMode::Advanced, Density::Comfortable, &Theme::builtin());
        assert!(!advanced.contains("lq-outline") && !advanced.contains("Query part") && advanced.contains("data-part=\"key\""));
        // Only the last action's card is open.
        assert_eq!(advanced.matches("<details open class=\"lq-card\"").count(), 1, "{advanced}");
        assert!(advanced.contains("<details open class=\"lq-card\" data-ns=\"pl\" data-path=\"2\""), "{advanced}");
        let first = editor_html_open(&tree, "Editor", EditorMode::Advanced, Density::Comfortable, &Theme::builtin(), Some(&NodePath::element(1)));
        assert!(first.contains("<details open class=\"lq-card\" data-ns=\"pl\" data-path=\"1\""), "{first}");
        let css = format!("{}{}", Theme::builtin().to_css(), EDITOR_CSS);
        assert!(css.contains("--lq-card:") && css.contains(".lq-form{"));

        // Icons: SVG sanitized into a data URI, a raster as a data URI, Unicode last.
        let svg = IconVariant::Svg(IconSource::Inline(crate::ui::theme::IconBytes::embedded(
            b"<svg onload=\"alert(1)\"><script>alert(2)</script><path d='M0 0'/></svg>",
        )));
        let icon = ResolvedIcon { variants: vec![std::borrow::Cow::Borrowed(&svg)], unicode: "x".into(), hue: None };
        let out = icon_html(&icon);
        let b64 = out.split("base64,").nth(1).and_then(|r| r.split('"').next()).expect("data uri");
        let decoded = String::from_utf8(base64::engine::general_purpose::STANDARD.decode(b64).expect("b64")).expect("utf8");
        assert!(!decoded.contains("<script") && !decoded.contains("onload"), "{decoded}");
        assert!(decoded.contains("<path"));
        let png = IconVariant::Raster(IconSource::Inline(crate::ui::theme::IconBytes::embedded(b"\x89PNG\r\n\x1a\n....")));
        let icon = ResolvedIcon { variants: vec![std::borrow::Cow::Borrowed(&png)], unicode: "x".into(), hue: None };
        assert!(icon_html(&icon).contains("data:image/png;base64,"));
        let key = IconVariant::Svg(IconSource::Key("-R/icons/a.svg".into()));
        let icon = ResolvedIcon { variants: vec![std::borrow::Cow::Borrowed(&key)], unicode: "⚙".into(), hue: None };
        assert!(icon_html(&icon).contains("⚙"));
    }

    #[test]
    fn mode_and_reset_all_events() {
        let tree = state().tree().clone();
        let mode = UIEvent::new(UIEventKind::Named("ed:mode:1:advanced".into()));
        assert_eq!(EditorView::mode_event(&mode), Some(EditorMode::Advanced));
        let reset = UIEvent::new(UIEventKind::Named("ed:resetall:1:".into()));
        let commands = EditorView::from_ui_event(&reset, &tree);
        assert!(commands.iter().all(|c| matches!(c, EditorCommand::Reset { .. })) && !commands.is_empty());
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
