//! The rows of the recipes panel: labels, icons, status, findings, tooltips and menus.

use std::collections::{BTreeMap, BTreeSet};

use serde::{Deserialize, Serialize};

use liquers_core::metadata::{AssetInfo, Status};
use liquers_core::recipe_tree::{node_name, TreeFinding, TreeReport};
use liquers_core::recipes::RecipeList;

/// What a row shows (AC-16).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum RecipeLabel {
    /// The filename, or the node name in a recipe tree.
    #[default]
    Name,
    /// The title, falling back to the name.
    Title,
    /// Both.
    Both,
}

impl RecipeLabel {
    /// `name`, `title` or `both`.
    pub fn as_str(self) -> &'static str {
        match self {
            RecipeLabel::Name => "name",
            RecipeLabel::Title => "title",
            RecipeLabel::Both => "both",
        }
    }

    /// Parses `name`, `title` or `both`.
    pub fn parse(text: &str) -> Option<Self> {
        match text {
            "name" => Some(RecipeLabel::Name),
            "title" => Some(RecipeLabel::Title),
            "both" => Some(RecipeLabel::Both),
            _ => None,
        }
    }
}

/// The panel's view (AC-11).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum RecipesView {
    /// The recipes in file order.
    #[default]
    Outline,
    /// Nodes nothing depends on at the top, direct dependencies as children.
    Dependencies,
}

/// A row's status (AC-17).
#[derive(Debug, Clone, PartialEq)]
pub enum RowStatus {
    /// Nothing is known about the asset.
    None,
    /// The asset manager's status.
    Status(Status),
    /// Progress with a known total: shown as a percentage instead of the status.
    Progress { percent: u8, message: String },
    /// The asset failed.
    Error(String),
}

impl RowStatus {
    /// The status of an asset from its information.
    pub fn of(info: Option<&AssetInfo>) -> RowStatus {
        let Some(info) = info else {
            return RowStatus::None;
        };
        {
            let progress = &info.progress;
            if progress.total > 0 && progress.done < progress.total {
                let percent = (u64::from(progress.done) * 100 / u64::from(progress.total)) as u8;
                return RowStatus::Progress {
                    percent,
                    message: progress.message.clone(),
                };
            }
        }
        if info.is_error || info.status == Status::Error {
            let message = info
                .error_data
                .as_ref()
                .map(|e| e.to_string())
                .filter(|m| !m.is_empty())
                .unwrap_or_else(|| info.message.clone());
            return RowStatus::Error(message);
        }
        RowStatus::Status(info.status)
    }

    /// The tooltip: the status, the progress message or the error.
    pub fn tooltip(&self) -> String {
        match self {
            RowStatus::None => "Not evaluated".to_owned(),
            RowStatus::Status(status) => status_label(*status).to_owned(),
            RowStatus::Progress { percent, message } => {
                if message.is_empty() {
                    format!("Processing · {percent}%")
                } else {
                    format!("Processing · {percent}%\n{message}")
                }
            }
            RowStatus::Error(message) => format!("Error\n{message}"),
        }
    }

    /// Whether the asset is being made, so the panel keeps its status fresh.
    pub fn is_busy(&self) -> bool {
        match self {
            RowStatus::Progress { .. } => true,
            RowStatus::Status(status) => matches!(
                status,
                Status::Submitted | Status::Dependencies | Status::Processing | Status::Storing
            ),
            RowStatus::None | RowStatus::Error(_) => false,
        }
    }

    /// The colour role of the status circle: `ok`, `busy`, `error`, `stale` or `none`.
    pub fn tone(&self) -> &'static str {
        match self {
            RowStatus::None => "none",
            RowStatus::Progress { .. } => "busy",
            RowStatus::Error(_) => "error",
            RowStatus::Status(status) => match status {
                Status::Ready | Status::Source | Status::Override | Status::Volatile => "ok",
                Status::Submitted | Status::Dependencies | Status::Processing | Status::Storing | Status::Partial => {
                    "busy"
                }
                Status::Error | Status::Cancelled => "error",
                Status::Expired => "stale",
                Status::None | Status::Directory | Status::Recipe => "none",
            },
        }
    }
}

/// A readable name of a status.
pub fn status_label(status: Status) -> &'static str {
    match status {
        Status::None => "Not evaluated",
        Status::Directory => "Directory",
        Status::Recipe => "Not evaluated (has a recipe)",
        Status::Submitted => "Submitted",
        Status::Dependencies => "Waiting for dependencies",
        Status::Processing => "Processing",
        Status::Partial => "Partial",
        Status::Error => "Error",
        Status::Storing => "Storing",
        Status::Ready => "Ready",
        Status::Expired => "Expired",
        Status::Cancelled => "Cancelled",
        Status::Source => "Source data",
        Status::Override => "Overridden data",
        Status::Volatile => "Volatile",
    }
}

/// One row of the panel, as renderers draw it.
#[derive(Debug, Clone, PartialEq)]
pub struct RecipeRow {
    /// The row's identity: the filename (`#<index>` for a recipe without one) in the outline,
    /// the path of filenames joined by `/` in the dependencies view.
    pub id: String,
    /// The recipe's filename, if it has one.
    pub filename: Option<String>,
    /// The name shown for the recipe: the filename, or the node name in a tree.
    pub name: String,
    /// The recipe's title (empty when it has none).
    pub title: String,
    /// The recipe's description.
    pub description: String,
    /// The type icon (AC-2).
    pub icon: String,
    /// The status (AC-17).
    pub status: RowStatus,
    /// Check findings about this node (AC-10).
    pub findings: Vec<String>,
    /// Whether the finding list has an error.
    pub has_error: bool,
    /// The declared result.
    pub is_result: bool,
    /// A source declaration (a recipe whose query is only its filename).
    pub is_source: bool,
    /// Depth in the dependencies view (0 in the outline).
    pub depth: usize,
    /// In the dependencies view: `Some(expanded)` when the node has children.
    pub expandable: Option<bool>,
    /// The node already appears elsewhere in the dependencies view.
    pub repeated: bool,
    /// The node closes a cycle: it is not expanded again.
    pub cycle: bool,
    /// The data formats the asset's type can be written in (Export).
    pub formats: Vec<String>,
}

impl RecipeRow {
    /// The text shown for `label` (AC-16): a recipe without a title shows its name.
    pub fn label(&self, label: RecipeLabel) -> (String, Option<String>) {
        match label {
            RecipeLabel::Name => (self.name.clone(), None),
            RecipeLabel::Title => {
                if self.title.is_empty() {
                    (self.name.clone(), None)
                } else {
                    (self.title.clone(), None)
                }
            }
            RecipeLabel::Both => {
                if self.title.is_empty() {
                    (self.name.clone(), None)
                } else {
                    (self.name.clone(), Some(self.title.clone()))
                }
            }
        }
    }

    /// The tooltip of the name (AC-18): the title and the description.
    pub fn name_tooltip(&self) -> String {
        let mut parts = Vec::new();
        if !self.title.is_empty() {
            parts.push(self.title.clone());
        }
        if !self.description.is_empty() {
            parts.push(self.description.clone());
        }
        if let Some(filename) = &self.filename {
            if *filename != self.name {
                parts.push(filename.clone());
            }
        }
        parts.join("\n")
    }

    /// The tooltip of the title (AC-18): the description.
    pub fn title_tooltip(&self) -> String {
        self.description.clone()
    }

    /// The tooltip of the findings marker.
    pub fn findings_tooltip(&self) -> String {
        self.findings.join("\n")
    }
}

/// An item of a row's context menu (AC-19).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MenuAction {
    View,
    Metadata,
    Edit,
    Evaluate,
    Rename,
    Duplicate,
    AddChild,
    SetResult,
    SaveAs,
    Export(String),
    Delete,
}

impl MenuAction {
    /// The item's label.
    pub fn label(&self) -> String {
        match self {
            MenuAction::View => "View".to_owned(),
            MenuAction::Metadata => "Metadata".to_owned(),
            MenuAction::Edit => "Edit".to_owned(),
            MenuAction::Evaluate => "Evaluate".to_owned(),
            MenuAction::Rename => "Rename".to_owned(),
            MenuAction::Duplicate => "Duplicate".to_owned(),
            MenuAction::AddChild => "Add child…".to_owned(),
            MenuAction::SetResult => "Set as result".to_owned(),
            MenuAction::SaveAs => "Save as…".to_owned(),
            MenuAction::Export(format) => format!("Export as {format}…"),
            MenuAction::Delete => "Delete".to_owned(),
        }
    }

    /// The event name a browser button sends for this item on row `filename`.
    pub fn event(&self, filename: &str) -> String {
        match self {
            MenuAction::View => format!("rc:view:{filename}"),
            MenuAction::Metadata => format!("rc:metadata:{filename}"),
            MenuAction::Edit => format!("rc:open:{filename}"),
            MenuAction::Evaluate => format!("rc:evaluate:{filename}"),
            MenuAction::Rename => format!("rc:rename:{filename}"),
            MenuAction::Duplicate => format!("rc:duplicate:{filename}"),
            MenuAction::AddChild => format!("rc:addchild:{filename}"),
            MenuAction::SetResult => format!("rc:result:{filename}"),
            MenuAction::SaveAs => format!("rc:saveas:{filename}"),
            MenuAction::Export(format) => format!("rc:export:{format}:{filename}"),
            MenuAction::Delete => format!("rc:delete:{filename}"),
        }
    }
}

/// A menu item with whether it applies, and why not when it does not.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MenuItem {
    pub action: MenuAction,
    /// `None` when enabled; the reason otherwise.
    pub disabled: Option<String>,
}

/// The context menu of `row` (AC-19). `tree` adds the tree items; `writable` is false for a
/// read-only panel; `exportable` is false where files cannot be saved (the browser).
pub fn menu_items(row: &RecipeRow, tree: bool, writable: bool, exportable: bool) -> Vec<MenuItem> {
    let no_file = row.filename.is_none().then(|| "The recipe has no filename".to_owned());
    let read_only = (!writable).then(|| "The recipes cannot be written".to_owned());
    let edit_block = no_file.clone().or(read_only.clone());
    let no_save = if exportable {
        no_file.clone()
    } else {
        Some("Saving files is not available here".to_owned())
    };
    let mut items = vec![
        MenuItem { action: MenuAction::View, disabled: no_file.clone() },
        MenuItem { action: MenuAction::Metadata, disabled: no_file.clone() },
        MenuItem { action: MenuAction::Edit, disabled: None },
        MenuItem { action: MenuAction::Evaluate, disabled: no_file.clone() },
        MenuItem { action: MenuAction::Rename, disabled: edit_block.clone() },
        MenuItem { action: MenuAction::Duplicate, disabled: edit_block.clone() },
    ];
    if tree {
        items.push(MenuItem { action: MenuAction::AddChild, disabled: edit_block.clone() });
        items.push(MenuItem {
            action: MenuAction::SetResult,
            disabled: edit_block.clone().or_else(|| row.is_result.then(|| "It is the result".to_owned())),
        });
    }
    items.push(MenuItem { action: MenuAction::SaveAs, disabled: no_save.clone() });
    if row.formats.is_empty() {
        items.push(MenuItem {
            action: MenuAction::Export(String::new()),
            disabled: Some(no_save.unwrap_or_else(|| {
                "The formats are known once the value has been evaluated".to_owned()
            })),
        });
    } else {
        for format in &row.formats {
            items.push(MenuItem { action: MenuAction::Export(format.clone()), disabled: no_save.clone() });
        }
    }
    items.push(MenuItem { action: MenuAction::Delete, disabled: edit_block });
    items
}

/// The files a finding is about.
fn finding_nodes(finding: &TreeFinding, list: &RecipeList) -> Vec<String> {
    match finding {
        TreeFinding::NoFilename { recipe, .. } => {
            let _ = list;
            vec![format!("#{recipe}")]
        }
        TreeFinding::DuplicateFilename { filename } => vec![filename.clone()],
        TreeFinding::PlanFailed { node, .. } => vec![node.clone()],
        TreeFinding::UnknownRoot { root } => vec![root.clone()],
        TreeFinding::ResultNotANode { result } => vec![result.clone()],
        TreeFinding::RootsBesideResult { result, .. } => vec![result.clone()],
        TreeFinding::Cycle { nodes } => nodes.clone(),
        TreeFinding::SecondParent { node, .. } => vec![node.clone()],
        TreeFinding::Orphan { node } => vec![node.clone()],
        TreeFinding::StrictKey { node, .. } => vec![node.clone()],
        TreeFinding::StrictLink { node, .. } => vec![node.clone()],
        TreeFinding::StrictCwd { node, .. } => vec![node.clone()],
        TreeFinding::Dangling { node, .. } => vec![node.clone()],
        TreeFinding::MissingSource { node } => vec![node.clone()],
        TreeFinding::UndeclaredSource { node, .. } => vec![node.clone()],
        TreeFinding::Unchecked { node, .. } => vec![node.clone()],
        TreeFinding::NameCollision { filenames, .. } => filenames.clone(),
    }
}

/// The findings of `report` by row id, each as `(message, is_error)`.
pub fn findings_by_row(report: &TreeReport, list: &RecipeList) -> BTreeMap<String, Vec<(String, bool)>> {
    let mut out: BTreeMap<String, Vec<(String, bool)>> = BTreeMap::new();
    for (finding, error) in report
        .errors
        .iter()
        .map(|f| (f, true))
        .chain(report.warnings.iter().map(|f| (f, false)))
    {
        for node in finding_nodes(finding, list) {
            out.entry(node).or_default().push((finding.to_string(), error));
        }
    }
    out
}

/// What the rows are built from.
pub struct RowSource<'a> {
    pub list: &'a RecipeList,
    pub report: &'a TreeReport,
    pub assets: &'a BTreeMap<String, AssetInfo>,
    pub formats: &'a BTreeMap<String, Vec<String>>,
}

impl RowSource<'_> {
    fn is_tree(&self) -> bool {
        self.list.tree.is_some()
    }

    fn result(&self) -> Option<&str> {
        self.list.tree.as_ref().and_then(|t| t.result.as_deref())
    }

    /// The row of recipe `index` (outline) or of node `filename` with no recipe.
    fn row(&self, id: String, filename: Option<&str>, index: Option<usize>, findings: &BTreeMap<String, Vec<(String, bool)>>) -> RecipeRow {
        let recipe = index.and_then(|i| self.list.recipes.get(i));
        let node = filename.and_then(|f| self.report.graph.nodes.get(f));
        let title = recipe
            .map(|r| r.title.clone())
            .or_else(|| node.map(|n| n.title.clone()))
            .unwrap_or_default();
        let description = recipe.map(|r| r.description.clone()).unwrap_or_default();
        let name = match filename {
            Some(f) if self.is_tree() => node_name(f).to_owned(),
            Some(f) => f.to_owned(),
            None => id.clone(),
        };
        let info = filename.and_then(|f| self.assets.get(f));
        let icon = match info.map(|i| i.unicode_icon.trim()).filter(|i| !i.is_empty()) {
            Some(icon) => icon.to_owned(),
            None => match recipe {
                Some(r) => r.unicode_icon().trim().to_owned(),
                None => match filename.and_then(|f| f.rsplit_once('.')) {
                    Some((_, ext)) => liquers_core::icons::file_extension_to_unicode_icon(ext).trim().to_owned(),
                    None => liquers_core::icons::DEFAULT_ICON.trim().to_owned(),
                },
            },
        };
        let key = filename.map(str::to_owned).unwrap_or_else(|| id.clone());
        let row_findings = findings.get(&key).cloned().unwrap_or_default();
        RecipeRow {
            id,
            filename: filename.map(str::to_owned),
            name,
            title,
            description,
            icon,
            status: RowStatus::of(info),
            has_error: row_findings.iter().any(|(_, e)| *e),
            findings: row_findings.into_iter().map(|(m, _)| m).collect(),
            is_result: filename.is_some() && filename == self.result(),
            is_source: recipe.is_some_and(|r| r.is_source_declaration()),
            depth: 0,
            expandable: None,
            repeated: false,
            cycle: false,
            formats: filename.and_then(|f| self.formats.get(f)).cloned().unwrap_or_default(),
        }
    }

    /// One row per recipe, in file order (AC-1, AC-3).
    pub fn outline(&self) -> Vec<RecipeRow> {
        let findings = findings_by_row(self.report, self.list);
        self.list
            .recipes
            .iter()
            .enumerate()
            .map(|(index, recipe)| {
                let filename = recipe.filename().ok().flatten().map(|f| f.name);
                let id = filename.clone().unwrap_or_else(|| format!("#{index}"));
                self.row(id, filename.as_deref(), Some(index), &findings)
            })
            .collect()
    }

    /// The dependencies view (AC-11): the roots at the top (the declared result or roots, else
    /// the nodes nothing depends on, then any node left unreached, which only a cycle leaves),
    /// direct dependencies as children. A node under several parents appears under each; a
    /// node closing a cycle is marked and not expanded. `collapsed` holds row ids.
    pub fn dependencies(&self, collapsed: &BTreeSet<String>) -> Vec<RecipeRow> {
        let findings = findings_by_row(self.report, self.list);
        let index_of = |file: &str| {
            self.list
                .recipes
                .iter()
                .position(|r| matches!(r.filename(), Ok(Some(f)) if f.name == file))
        };
        let mut roots: Vec<String> = self.report.roots.clone();
        let mut reached = BTreeSet::new();
        let mut rows = Vec::new();
        let mut seen = BTreeSet::new();
        // Nodes reachable from the roots.
        let mut stack: Vec<String> = roots.clone();
        while let Some(node) = stack.pop() {
            if reached.insert(node.clone()) {
                stack.extend(self.report.graph.children(&node));
            }
        }
        for recipe in &self.list.recipes {
            if let Ok(Some(f)) = recipe.filename() {
                if !reached.contains(&f.name) {
                    roots.push(f.name.clone());
                    let mut stack = vec![f.name];
                    while let Some(node) = stack.pop() {
                        if reached.insert(node.clone()) {
                            stack.extend(self.report.graph.children(&node));
                        }
                    }
                }
            }
        }
        for root in roots {
            self.walk(&root, &mut Vec::new(), collapsed, &findings, &index_of, &mut seen, &mut rows);
        }
        rows
    }

    #[allow(clippy::too_many_arguments)]
    fn walk(
        &self,
        file: &str,
        path: &mut Vec<String>,
        collapsed: &BTreeSet<String>,
        findings: &BTreeMap<String, Vec<(String, bool)>>,
        index_of: &dyn Fn(&str) -> Option<usize>,
        seen: &mut BTreeSet<String>,
        rows: &mut Vec<RecipeRow>,
    ) {
        let cycle = path.iter().any(|p| p == file);
        path.push(file.to_owned());
        let id = path.join("/");
        let mut row = self.row(id.clone(), Some(file), index_of(file), findings);
        row.depth = path.len() - 1;
        row.repeated = !seen.insert(file.to_owned());
        row.cycle = cycle;
        let children = if cycle { Vec::new() } else { self.report.graph.children(file) };
        let expanded = !collapsed.contains(&id);
        row.expandable = (!children.is_empty()).then_some(expanded);
        rows.push(row);
        if expanded {
            for child in children {
                self.walk(&child, path, collapsed, findings, index_of, seen, rows);
            }
        }
        path.pop();
    }
}
