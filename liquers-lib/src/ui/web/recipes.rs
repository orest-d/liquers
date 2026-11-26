//! Browser markup of the recipe editors (design `recipes-editor`): the side panel, a recipe form
//! and the YAML window.
//!
//! The markup follows the conventions of [`super::html`]: buttons carry `data-lq-event` names
//! that [`panel_commands`](crate::ui::recipes::panel_commands) decodes (`rc:open:<file>`,
//! `rc:mode:dependencies`, `rf:save:<id>`, …) and inputs carry `data-lq-field` names whose values
//! travel with the event. Hover text is `title=`. Rows have a `⋯` menu (a `<details>`) with the
//! actions of the egui context menu; Save as and Export are disabled, since a browser page
//! cannot write a local file here (`WEB-RECIPES-EDITOR-CANNOT-DOWNLOAD`). The panel shows the
//! most recently opened recipe form below the rows, one at a time, so field names stay unique.
//!
//! # Main components
//!
//! - [`recipes_html`]: the panel of a [`RecipesState`].
//! - [`form_html`]: one recipe as a form.
//! - [`RECIPES_CSS`]: the panel's stylesheet, over the theme's custom properties.
//!
//! # Where it is used
//!
//! `RecipesElement::render_web` and `RecipeElement::render_web`.
//!
//! # Related concepts
//!
//! - `crate::egui::recipes`: the egui rendering of the same rows and forms.
//! - [`super::editor`]: the query editor's markup, whose text field a form reuses.
//!
//! # Examples
//!
//! ```
//! use liquers_lib::ui::recipes::RecipesState;
//! use liquers_lib::ui::web::recipes::recipes_html;
//! use liquers_lib::value::Value;
//!
//! let mut state = RecipesState::new(Default::default()).with_folder(Some("data".into()));
//! state.on_snapshot(&Value::from("recipes:\n- {query: -R/x.csv/-/head/a.csv, title: A}\n"), None).unwrap();
//! let html = recipes_html(&state, "Recipes");
//! assert!(html.contains("data-lq-event=\"rc:open:a.csv\""));
//! ```

use super::html::escape_html;
use crate::ui::recipes::{menu_items, yaml_spans, MenuAction, RecipeForm, RecipeLabel, RecipeRow, RecipesState, RecipesView, RowStatus};
use crate::ui::theme::SyntaxRole;

/// The panel's stylesheet.
pub const RECIPES_CSS: &str = r#"
.lq-recipes { max-width: 16rem; font-size: 0.9rem; }
.lq-recipes-toolbar button[aria-pressed="true"] { font-weight: bold; }
.lq-recipe-row { display: flex; align-items: center; gap: 0.25rem; white-space: nowrap; }
.lq-recipe-row .lq-recipe-label { overflow: hidden; text-overflow: ellipsis; flex: 1; }
.lq-recipe-row .lq-recipe-label.lq-open { font-weight: bold; text-decoration: underline; }
.lq-recipe-row .lq-recipe-title { color: var(--lq-ink-muted); font-size: 0.8em; }
.lq-status { display: inline-block; width: 0.55rem; height: 0.55rem; border-radius: 50%; }
.lq-status-ok { background: var(--lq-success); }
.lq-status-busy { background: var(--lq-accent); }
.lq-status-error { background: var(--lq-danger); }
.lq-status-stale { background: var(--lq-warning); }
.lq-status-none { background: var(--lq-ink-faint); }
.lq-percent { font-size: 0.7em; color: var(--lq-accent); }
.lq-recipe-menu summary { list-style: none; cursor: pointer; }
.lq-recipe-menu[open] > div { position: absolute; background: var(--lq-card); border: 1px solid var(--lq-line); z-index: 10; }
.lq-recipe-menu button { display: block; width: 100%; text-align: left; }
.lq-yaml pre span { white-space: pre; }
"#;

fn role_class(role: SyntaxRole) -> &'static str {
    match role {
        SyntaxRole::Header => "lq-syn-header",
        SyntaxRole::Resource => "lq-syn-resource",
        SyntaxRole::Separator => "lq-syn-separator",
        SyntaxRole::Namespace => "lq-syn-namespace",
        SyntaxRole::Action => "lq-syn-action",
        SyntaxRole::Parameter => "lq-syn-parameter",
        SyntaxRole::Entity => "lq-syn-entity",
        SyntaxRole::Link => "lq-syn-link",
        SyntaxRole::Filename => "lq-syn-filename",
        SyntaxRole::Plain => "lq-syn-plain",
        SyntaxRole::Highlight => "lq-syn-highlight",
        SyntaxRole::Error => "lq-syn-error",
    }
}

fn button(event: &str, label: &str, title: &str, disabled: Option<&str>) -> String {
    let title = disabled.unwrap_or(title);
    let title = if title.is_empty() { String::new() } else { format!(" title=\"{}\"", escape_html(title)) };
    let disabled = if disabled.is_some() { " disabled" } else { "" };
    format!(
        "<button type=\"button\" data-lq-event=\"{}\"{title}{disabled}>{}</button>",
        escape_html(event),
        escape_html(label)
    )
}

fn status_html(status: &RowStatus) -> String {
    let tip = escape_html(&status.tooltip());
    match status {
        RowStatus::Progress { percent, .. } => format!("<span class=\"lq-percent\" title=\"{tip}\">{percent}%</span>"),
        RowStatus::None | RowStatus::Status(_) | RowStatus::Error(_) => {
            format!("<span class=\"lq-status lq-status-{}\" title=\"{tip}\"></span>", status.tone())
        }
    }
}

fn row_html(row: &RecipeRow, state: &RecipesState) -> String {
    let file = row.filename.clone().unwrap_or_else(|| row.id.clone());
    let (main, extra) = row.label(state.label());
    let tooltip = match state.label() {
        RecipeLabel::Title if !row.title.is_empty() => row.title_tooltip(),
        RecipeLabel::Title | RecipeLabel::Name | RecipeLabel::Both => row.name_tooltip(),
    };
    let mut html = format!(
        "<div class=\"lq-recipe-row\" data-lq-row=\"{}\" style=\"padding-left:{}rem\">",
        escape_html(&row.id),
        row.depth as f32 * 0.75
    );
    if let Some(expanded) = row.expandable {
        html.push_str(&button(&format!("rc:toggle:{}", row.id), if expanded { "▾" } else { "▸" }, "", None));
    }
    html.push_str(&format!("<span class=\"lq-recipe-icon\">{}</span>", escape_html(&row.icon)));
    let mut classes = String::from("lq-recipe-label");
    if row.is_result {
        classes.push_str(" lq-result");
    }
    if row.repeated || row.cycle {
        classes.push_str(" lq-repeated");
    }
    if state.is_marked(&file) {
        classes.push_str(" lq-open");
    }
    html.push_str(&format!(
        "<button type=\"button\" class=\"{classes}\" data-lq-event=\"rc:open:{}\" title=\"{}\">{}</button>",
        escape_html(&file),
        escape_html(&tooltip),
        escape_html(&main)
    ));
    if let Some(extra) = extra {
        html.push_str(&format!(
            "<span class=\"lq-recipe-title\" title=\"{}\">{}</span>",
            escape_html(&row.title_tooltip()),
            escape_html(&extra)
        ));
    }
    if !row.findings.is_empty() {
        let tone = if row.has_error { "error" } else { "warning" };
        html.push_str(&format!(
            "<span class=\"lq-finding lq-finding-{tone}\" title=\"{}\">⚠</span>",
            escape_html(&row.findings_tooltip())
        ));
    }
    html.push_str(&status_html(&row.status));
    // The row menu: the egui context menu's items (AC-19).
    html.push_str("<details class=\"lq-recipe-menu\"><summary title=\"Actions\">⋯</summary><div>");
    for item in menu_items(row, state.is_tree(), state.read_only().is_none(), false) {
        match &item.action {
            MenuAction::Rename => {
                html.push_str(&format!(
                    "<input data-lq-field=\"rename:{f}\" value=\"{n}\" size=\"10\"{d}>{}",
                    button(&item.action.event(&file), "Rename", "", item.disabled.as_deref()),
                    f = escape_html(&file),
                    n = escape_html(&row.name),
                    d = if item.disabled.is_some() { " disabled" } else { "" }
                ));
            }
            MenuAction::AddChild => {
                let slots = state.child_slots(&file);
                if slots.is_empty() {
                    html.push_str(&button("", &item.action.label(), "", Some("The node has no Node argument")));
                    continue;
                }
                html.push_str(&format!("<input data-lq-field=\"child_title:{}\" placeholder=\"title\" size=\"10\">", escape_html(&file)));
                html.push_str(&format!("<select data-lq-field=\"child_slot:{}\">", escape_html(&file)));
                for (i, slot) in slots.iter().enumerate() {
                    html.push_str(&format!(
                        "<option value=\"{i}\">{}</option>",
                        escape_html(&format!("{} · {} (.{})", slot.command, slot.arg, slot.extension))
                    ));
                }
                html.push_str("</select>");
                html.push_str(&button(&item.action.event(&file), &item.action.label(), "", item.disabled.as_deref()));
            }
            MenuAction::View
            | MenuAction::Metadata
            | MenuAction::Edit
            | MenuAction::Evaluate
            | MenuAction::Duplicate
            | MenuAction::SetResult
            | MenuAction::SaveAs
            | MenuAction::Export(_)
            | MenuAction::Delete => {
                html.push_str(&button(&item.action.event(&file), &item.action.label(), "", item.disabled.as_deref()));
            }
        }
    }
    html.push_str("</div></details></div>");
    html
}

/// The highlighted YAML text, as spans.
fn yaml_html(text: &str) -> String {
    let mut out = String::new();
    for (range, role) in yaml_spans(text) {
        out.push_str(&format!("<span class=\"{}\">{}</span>", role_class(role), escape_html(&text[range])));
    }
    out
}

/// The panel (AC-1 to AC-19).
pub fn recipes_html(state: &RecipesState, title: &str) -> String {
    let mut html = format!("<div class=\"lq-recipes\"><div class=\"lq-recipes-head\"><strong>{}</strong></div>", escape_html(title));
    let outline = state.view() == RecipesView::Outline;
    let pressed = |on: bool| if on { "true" } else { "false" };
    html.push_str("<div class=\"lq-recipes-toolbar\">");
    html.push_str(&format!(
        "<button type=\"button\" data-lq-event=\"rc:mode:outline\" aria-pressed=\"{}\" title=\"Outline\">≡</button>",
        pressed(outline)
    ));
    html.push_str(&format!(
        "<button type=\"button\" data-lq-event=\"rc:mode:dependencies\" aria-pressed=\"{}\" title=\"Dependencies\">⑂</button>",
        pressed(!outline)
    ));
    html.push_str(&button("rc:yaml", "{}", "Edit as YAML", None));
    html.push_str("<select data-lq-field=\"label\" data-lq-event=\"change:label\" title=\"Show\">");
    for label in [RecipeLabel::Name, RecipeLabel::Title, RecipeLabel::Both] {
        let selected = if state.label() == label { " selected" } else { "" };
        html.push_str(&format!("<option value=\"{0}\"{selected}>{0}</option>", label.as_str()));
    }
    html.push_str("</select>");
    let checked = if state.continuous() { "off" } else { "on" };
    html.push_str(&button(
        &format!("rc:continuous:{checked}"),
        if state.continuous() { "Immediate" } else { "On demand" },
        "Apply edits immediately or on Update",
        None,
    ));
    html.push_str("</div>");
    if state.read_only().is_none() {
        // An empty `.node` node, named by the first free integer; its query and filename are set
        // in its form (design `strict-tree-editing`, AC-5, AC-7).
        html.push_str(
            "<div class=\"lq-recipes-new\"><input data-lq-field=\"new_title\" placeholder=\"New node title (optional)\" size=\"16\">",
        );
        html.push_str(&button("rc:new", "＋", "New node", None));
        html.push_str("</div>");
    }
    if !state.continuous() && state.is_pending() {
        html.push_str(&button("rc:update", "Update", "", None));
        html.push_str(&button("rc:revert", "Revert", "", None));
    }
    if let Some(reason) = state.read_only() {
        html.push_str(&format!("<div class=\"lq-warning\">{}</div>", escape_html(reason)));
    }
    // AC-9: an invalid tree is used as plain recipes, and says so.
    if let Some(notice) = state.tree_notice() {
        html.push_str(&format!("<div class=\"lq-warning lq-tree-notice\">{}</div>", escape_html(&notice)));
    }
    // AC-4: edits a failed write left in memory only.
    if let Some(error) = state.unsaved() {
        html.push_str(&format!(
            "<div class=\"lq-warning lq-unsaved\" title=\"{}\">Unsaved edits</div>",
            escape_html(error)
        ));
    }
    if let Some(message) = state.message() {
        html.push_str(&format!("<div class=\"lq-message\">{}</div>", escape_html(message)));
    }
    html.push_str("<div class=\"lq-recipe-rows\">");
    for row in state.rows() {
        html.push_str(&row_html(row, state));
    }
    html.push_str("</div>");
    if let Some(form) = state.forms().last() {
        html.push_str(&format!(
            "<div class=\"lq-recipe-window\"><div class=\"lq-window-head\">{}{}</div>{}</div>",
            escape_html(&form.filename().unwrap_or_else(|| form.id.clone())),
            button(&format!("rc:close:{}", form.id), "✕", "Close", None),
            form_html(form, None, state.read_only().is_some())
        ));
    }
    if let Some(buffer) = state.yaml() {
        html.push_str("<div class=\"lq-yaml\"><div class=\"lq-window-head\">recipes.yaml</div>");
        html.push_str(&format!("<pre class=\"lq-yaml-preview\">{}</pre>", yaml_html(&buffer.text)));
        html.push_str(&format!(
            "<textarea data-lq-field=\"yaml\" rows=\"12\" cols=\"48\" spellcheck=\"false\">{}</textarea>",
            escape_html(&buffer.text)
        ));
        if let Some((line, message)) = &buffer.error {
            let at = line.map(|l| format!("line {l}: ")).unwrap_or_default();
            html.push_str(&format!("<div class=\"lq-error\">{}</div>", escape_html(&format!("{at}{message}"))));
        }
        html.push_str(&button("rc:yaml-apply", "Apply", "", None));
        html.push_str(&button("rc:yaml-close", "Close", "", None));
        html.push_str("</div>");
    }
    html.push_str("</div>");
    html
}

/// One recipe as a form (AC-5). `rf:save:<id>` sends every field; the element applies the
/// changed ones.
pub fn form_html(form: &RecipeForm, message: Option<&str>, read_only: bool) -> String {
    let recipe = form.recipe();
    let disabled = if read_only { " disabled" } else { "" };
    let input = |field: &str, label: &str, value: &str| {
        format!(
            "<label>{}<input data-lq-field=\"{}\" value=\"{}\"{disabled}></label>",
            escape_html(label),
            escape_html(field),
            escape_html(value)
        )
    };
    let mut html = String::from("<div class=\"lq-recipe-form\">");
    html.push_str(&input("title", "Title", &recipe.title));
    html.push_str(&format!(
        "<label>Description<textarea data-lq-field=\"description\" rows=\"2\"{disabled}>{}</textarea></label>",
        escape_html(&recipe.description)
    ));
    html.push_str(&input("filename", "Filename", &form.filename().unwrap_or_default()));
    html.push_str(&input("query", "Query", &form.query_editor().tree().text));
    html.push_str(&button(
        &format!("rf:options:{}", form.id),
        if form.options_open { "▾ Options" } else { "▸ Options" },
        "",
        None,
    ));
    if form.options_open {
        html.push_str(&input("cwd", "Working folder", recipe.cwd.as_deref().unwrap_or("")));
        html.push_str(&input("volatile", "Volatile", if recipe.volatile { "true" } else { "false" }));
        html.push_str(&input("expires", "Expires", &recipe.expires.to_string()));
        html.push_str(&input("stored", "Stored", &recipe.stored.map(|s| s.to_string()).unwrap_or_default()));
    }
    if !read_only {
        html.push_str(&button(&format!("rf:save:{}", form.id), "Save", "", None));
    }
    if let Some(message) = message.or(form.message.as_deref()) {
        html.push_str(&format!("<div class=\"lq-error\">{}</div>", escape_html(message)));
    }
    html.push_str("</div>");
    html
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ui::recipes::RecipesCommand;
    use crate::value::Value;

    fn state() -> RecipesState {
        let mut state = RecipesState::new(Default::default()).with_folder(Some("data".into()));
        state
            .on_snapshot(
                &Value::from("recipes:\n- {query: -R/x.csv/-/head/a.csv, title: A, description: First}\n"),
                None,
            )
            .expect("snapshot");
        state
    }

    /// `recipes-editor-open-action` AC-4: the row clicked last is marked; Edit sends `rc:edit`.
    #[test]
    fn web_marks_the_open_row() {
        let mut state = RecipesState::new(Default::default())
            .with_folder(Some("data".into()))
            .with_open_command(Some(crate::ui::recipes::OpenCommand::new("lui", "submit")));
        state
            .on_snapshot(&Value::from("recipes:\n- {query: -R/x.csv/-/head/a.csv, title: A}\n"), None)
            .expect("snapshot");
        assert!(!recipes_html(&state, "Recipes").contains("lq-open"));
        state.apply(RecipesCommand::Open("a.csv".into())).expect("open");
        let html = recipes_html(&state, "Recipes");
        assert!(html.contains("lq-recipe-label lq-open"), "{html}");
        assert!(html.contains("data-lq-event=\"rc:edit:a.csv\""), "{html}");
    }

    /// AC-16 to AC-19: rows, tooltips, a status dot and the row menu.
    #[test]
    fn panel_html_has_rows_tooltips_status_and_menu() {
        let html = recipes_html(&state(), "Recipes");
        assert!(html.contains("data-lq-event=\"rc:open:a.csv\""));
        assert!(html.contains("title=\"A\nFirst\""), "{html}");
        assert!(html.contains("lq-status lq-status-none"));
        assert!(html.contains("data-lq-event=\"rc:view:a.csv\""));
        assert!(html.contains("data-lq-event=\"rc:delete:a.csv\""));
        // Save as is disabled in a browser.
        assert!(html.contains("data-lq-event=\"rc:saveas:a.csv\" title=\"Saving files is not available here\" disabled"));
        assert!(html.contains("data-lq-event=\"rc:mode:dependencies\""));
    }

    /// AC-5: the form has its fields and a save button.
    #[test]
    fn recipe_form_html_has_fields() {
        let mut state = state();
        state.apply(RecipesCommand::Open("a.csv".into())).expect("open");
        let html = form_html(&state.forms()[0], None, false);
        for field in ["title", "description", "filename", "query"] {
            assert!(html.contains(&format!("data-lq-field=\"{field}\"")), "{field}");
        }
        assert!(html.contains("data-lq-event=\"rf:save:a.csv\""));
        assert!(recipes_html(&state, "R").contains("lq-recipe-window"));
        assert!(!form_html(&state.forms()[0], None, true).contains("rf:save"));
    }

    /// strict-tree-editing AC-7: New asks for an optional title only.
    #[test]
    fn web_panel_new_has_no_extension_field() {
        let html = recipes_html(&state(), "Recipes");
        assert!(html.contains("data-lq-event=\"rc:new\""));
        assert!(html.contains("data-lq-field=\"new_title\""));
        assert!(!html.contains("new_ext"), "{html}");
    }

    /// strict-tree-editing AC-9, AC-4: the notice of an invalid tree, and the unsaved marker.
    #[test]
    fn web_panel_shows_the_notice() {
        let mut state = RecipesState::new(Default::default()).with_folder(Some("p".into()));
        state
            .on_snapshot(&Value::from("tree: {}\nrecipes:\n- {query: -R/x.csv/-/head/a.csv}\n- {query: -R/y.csv/-/head/a.csv}\n"), None)
            .expect("snapshot");
        let html = recipes_html(&state, "Recipes");
        assert!(html.contains("lq-tree-notice"), "{html}");
        assert!(html.contains("Not a valid recipe tree: plain recipes."));
        assert!(!recipes_html(&super::tests::state(), "R").contains("lq-tree-notice"));
        state.apply(RecipesCommand::Duplicate("a.csv".into())).ok();
        state.on_write_failed(&liquers_core::error::Error::general_error("disk full".to_owned()));
        assert!(recipes_html(&state, "Recipes").contains("lq-unsaved"));
    }
}
