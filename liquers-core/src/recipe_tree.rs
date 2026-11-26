//! Self-contained recipe trees: one folder's `recipes.yaml` read as a graph of nodes.
//!
//! # Purpose
//!
//! A folder of recipes can describe one asset tree — a car built from a body and wheels, a wheel
//! from a tyre and a material — that is checked as a whole, edited without breaking its
//! references, and, in strict mode, copied anywhere and evaluated to the same value. This module
//! reads a [`RecipeList`] as that graph, checks it, and edits it.
//!
//! # Main components
//!
//! - [`TreeDeclaration`]: the optional `tree:` section of `recipes.yaml` (roots, the single
//!   `result`, the shape and the dependency mode). Adding it to any recipe list makes a tree.
//! - [`TreeGraph`]: the nodes (recipes, source declarations — recipes whose query is only a
//!   filename — and, when a folder listing is given, stored files) and the references between
//!   them, built from each recipe's plan.
//! - [`check_strict`]: the per-recipe rules of `dependencies: strict` — keyed dependencies only
//!   directly in the folder, and no link or nested-pipeline arguments written by the recipe.
//!   A command's own default links and pipelines and enum-alias links are trusted.
//! - [`check_tree`]: the whole check — roots and result, cycles, shape, orphans, dangling
//!   references, name collisions, undeclared sources — as a [`TreeReport`] of findings.
//! - [`TreeEditor`]: edits one folder's recipes in memory, keeping only edits that add no error —
//!   rename a node (rewriting every reference and queueing the stored file's move), add a child,
//!   delete, move a reference, declare a source — and saves them through the asset manager.
//!
//! A node's name is its filename without the extension (`wheel` for `wheel.glb`): the name a
//! Node argument (`ArgumentType::Node`) is given in a query, `attach-wheel`.
//!
//! # Where it is used
//!
//! `DefaultRecipeProvider` runs [`check_tree`] on a strict folder and refuses the whole file on an
//! error; `liquers-validate --recipes` adds the report (`ValidationReport::tree`); the editors of
//! `liquers-lib` list node choices for a Node argument and rename through [`TreeEditor`].
//!
//! # Related concepts
//!
//! `specs/reference/RECIPE_TREES.md`; recipes and plans in `specs/reference/RECIPES_AND_PLANS.md`;
//! the design in `specs/design/recipe-tree/`.
//!
//! # Examples
//!
//! ```
//! # use liquers_core::command_metadata::CommandMetadataRegistry;
//! # use liquers_core::recipe_tree::check_tree;
//! # use liquers_core::recipes::RecipeList;
//! let list: RecipeList = serde_yaml::from_str(r#"
//! tree: {dependencies: strict, result: wheel.txt}
//! recipes:
//!   - {query: tyre.txt, title: Tyre}
//!   - {query: "-R/./tyre.txt/-/wheel.txt", title: Wheel}
//! "#).unwrap();
//! let listing = vec!["tyre.txt".to_string()];
//! let report = check_tree(&list, Some(&listing), &CommandMetadataRegistry::new());
//! assert!(report.errors.is_empty(), "{:?}", report.errors);
//! assert_eq!(report.graph.nodes["wheel.txt"].name, "wheel");
//! ```

use std::collections::{BTreeMap, BTreeSet};
use std::fmt;
use std::sync::Arc;

use serde_json::Value as Json;

use crate::command_metadata::{ArgumentType, CommandMetadataRegistry};
use crate::context::{EnvRef, Environment};
use crate::error::Error;
use crate::pipeline::{ActionElement, ElementKind, Located, Operand, Pipeline, QueryOrPipeline, Source};
use crate::plan::{ParameterValue, Plan, Step};
use crate::query::Key;
use crate::recipes::{Recipe, RecipeList};

/// The `tree:` section of a `recipes.yaml`.
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Default)]
pub struct TreeDeclaration {
    /// Declared roots, as filenames. Empty: the recipes nothing depends on (or the `result`).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub roots: Vec<String>,
    /// The single asset the tree stands for, as a filename. It is then the only root.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub result: Option<String>,
    /// `dag` (the default) or `tree` (no node with two parents).
    #[serde(default, skip_serializing_if = "TreeShape::is_default")]
    pub shape: TreeShape,
    /// `open` (the default) or `strict` (the folder is self-contained).
    #[serde(default, skip_serializing_if = "DependencyMode::is_default")]
    pub dependencies: DependencyMode,
}

/// The shape a tree must have.
#[derive(Serialize, Deserialize, Debug, Clone, Copy, PartialEq, Eq, Default)]
#[serde(rename_all = "snake_case")]
pub enum TreeShape {
    /// Any acyclic graph: a node may have several parents.
    #[default]
    Dag,
    /// Every node has at most one parent.
    Tree,
}

impl TreeShape {
    fn is_default(&self) -> bool {
        *self == TreeShape::default()
    }
}

/// Which dependencies the nodes of a tree may have.
#[derive(Serialize, Deserialize, Debug, Clone, Copy, PartialEq, Eq, Default)]
#[serde(rename_all = "snake_case")]
pub enum DependencyMode {
    /// Any dependency.
    #[default]
    Open,
    /// Only files directly in the folder, by relative keys, and no link arguments: the folder
    /// works wherever it is copied.
    Strict,
}

impl DependencyMode {
    fn is_default(&self) -> bool {
        *self == DependencyMode::default()
    }
}

/// Where a node comes from.
#[derive(Serialize, Deserialize, Debug, Clone, Copy, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum NodeOrigin {
    /// The recipe at this index computes it.
    Recipe(usize),
    /// The recipe at this index declares it a source: its value is the stored data.
    Source(usize),
    /// Only the folder listing has it: a stored file no recipe declares.
    Listed,
}

/// One node of a tree.
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
pub struct TreeNode {
    /// The node's filename, `wheel.glb`.
    pub filename: String,
    /// The node's name, its filename without the extension: `wheel`.
    pub name: String,
    /// The recipe's title; empty for a stored file.
    pub title: String,
    /// Where it comes from.
    pub origin: NodeOrigin,
}

/// How a recipe refers to a key.
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum RefKind {
    /// A resource segment or pipeline element (`-R/./x`, `{resource: ./x}`), or a nested query.
    Resource,
    /// A Node argument given by name, in the action at this index of the plan's steps.
    NodeArg {
        /// Index of the action step.
        action: usize,
        /// The argument's name.
        arg: String,
    },
    /// A link or nested pipeline written by the recipe for an argument.
    Link {
        /// The argument's name.
        arg: String,
    },
    /// A key used as a value (`-R-key/./x` as the query itself).
    KeyValue,
}

/// One reference of a recipe.
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
pub struct TreeEdge {
    /// The referring node's filename.
    pub from: String,
    /// The referenced key, encoded as the recipe wrote it (`./tyre.glb`, `data/x.csv`).
    pub to: String,
    /// How it is referenced.
    pub kind: RefKind,
}

impl TreeEdge {
    /// The filename this edge points to, when its key is directly in the folder (`./x`).
    pub fn local_target(&self) -> Option<&str> {
        self.to.strip_prefix("./").filter(|rest| !rest.contains('/'))
    }
}

/// A folder's recipes read as a graph.
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Default)]
pub struct TreeGraph {
    /// Nodes by filename.
    pub nodes: BTreeMap<String, TreeNode>,
    /// Every reference found in the recipes, in recipe order.
    pub edges: Vec<TreeEdge>,
}

/// One problem a tree check found.
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum TreeFinding {
    /// A recipe has no filename, so it cannot be a node.
    NoFilename {
        /// Index of the recipe.
        recipe: usize,
        /// Its query.
        query: String,
    },
    /// Two recipes produce the same file.
    DuplicateFilename {
        /// The filename.
        filename: String,
    },
    /// A recipe could not be planned, so its references are unknown.
    PlanFailed {
        /// The node.
        node: String,
        /// The planning error.
        error: String,
    },
    /// A declared root is not a node (AC-3).
    UnknownRoot {
        /// The root.
        root: String,
    },
    /// The declared result is not a node (AC-19).
    ResultNotANode {
        /// The result.
        result: String,
    },
    /// Roots and a result are both declared, and the roots are not the result alone (AC-19).
    RootsBesideResult {
        /// The result.
        result: String,
        /// The declared roots.
        roots: Vec<String>,
    },
    /// The references form a cycle (AC-4).
    Cycle {
        /// The nodes of the cycle, in order; the first is repeated at the end.
        nodes: Vec<String>,
    },
    /// Under `shape: tree`, a node has more than one parent (AC-5).
    SecondParent {
        /// The node.
        node: String,
        /// Its parents.
        parents: Vec<String>,
    },
    /// No root reaches this node (AC-6). A warning.
    Orphan {
        /// The node.
        node: String,
    },
    /// Under strict mode, a keyed dependency is not directly in the folder (AC-7).
    StrictKey {
        /// The node.
        node: String,
        /// The key.
        key: String,
    },
    /// Under strict mode, the recipe passes an argument as a link or nested pipeline (AC-23).
    StrictLink {
        /// The node.
        node: String,
        /// The argument.
        argument: String,
    },
    /// Under strict mode, the recipe changes the working directory (AC-7).
    StrictCwd {
        /// The node.
        node: String,
        /// The directory it sets.
        key: String,
    },
    /// A reference names a file of the folder that is neither a recipe nor stored (AC-8).
    Dangling {
        /// The node.
        node: String,
        /// The missing key.
        key: String,
    },
    /// A source declaration's file is not in the folder listing (AC-27).
    MissingSource {
        /// The source node.
        node: String,
    },
    /// Under strict mode, a referenced file is not declared by a recipe (AC-28).
    UndeclaredSource {
        /// The node.
        node: String,
        /// The file.
        file: String,
    },
    /// Without a listing, a reference to a file no recipe declares could not be checked. A
    /// warning.
    Unchecked {
        /// The node.
        node: String,
        /// The file.
        file: String,
    },
    /// Nodes share a name and something depends on one of them (AC-16).
    NameCollision {
        /// The shared name.
        name: String,
        /// The filenames.
        filenames: Vec<String>,
    },
}

impl fmt::Display for TreeFinding {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            TreeFinding::NoFilename { recipe, query } => {
                write!(f, "recipe {recipe} ('{query}') has no filename, so it is not a node")
            }
            TreeFinding::DuplicateFilename { filename } => {
                write!(f, "two recipes produce '{filename}'")
            }
            TreeFinding::PlanFailed { node, error } => {
                write!(f, "'{node}' cannot be planned: {error}")
            }
            TreeFinding::UnknownRoot { root } => write!(f, "root '{root}' is not a node"),
            TreeFinding::ResultNotANode { result } => write!(f, "result '{result}' is not a node"),
            TreeFinding::RootsBesideResult { result, roots } => write!(
                f,
                "result '{result}' is the only root, but roots {} are declared",
                roots.join(", ")
            ),
            TreeFinding::Cycle { nodes } => write!(f, "cycle: {}", nodes.join(" -> ")),
            TreeFinding::SecondParent { node, parents } => write!(
                f,
                "'{node}' has more than one parent ({}) in a tree",
                parents.join(", ")
            ),
            TreeFinding::Orphan { node } => write!(f, "'{node}' is reached from no root"),
            TreeFinding::StrictKey { node, key } => write!(
                f,
                "'{node}' depends on '{key}', which is not directly in the folder (strict)"
            ),
            TreeFinding::StrictLink { node, argument } => write!(
                f,
                "'{node}' passes argument '{argument}' as a link or pipeline (strict allows \
                 node names only)"
            ),
            TreeFinding::StrictCwd { node, key } => {
                write!(f, "'{node}' sets the working directory to '{key}' (strict)")
            }
            TreeFinding::Dangling { node, key } => {
                write!(f, "'{node}' refers to '{key}', which is neither a recipe nor stored")
            }
            TreeFinding::MissingSource { node } => {
                write!(f, "source '{node}' is declared but not stored")
            }
            TreeFinding::UndeclaredSource { node, file } => write!(
                f,
                "'{node}' refers to '{file}', which no recipe declares (strict: add \
                 {{query: {file}}})"
            ),
            TreeFinding::Unchecked { node, file } => write!(
                f,
                "'{node}' refers to '{file}', which no recipe declares; not checked (a stored file?)"
            ),
            TreeFinding::NameCollision { name, filenames } => write!(
                f,
                "name '{name}' is shared by {} and one of them is referenced",
                filenames.join(", ")
            ),
        }
    }
}

impl TreeFinding {
    /// The finding as an error.
    pub fn to_error(&self) -> Error {
        match self {
            TreeFinding::StrictKey { node, key } => {
                Error::strict_dependency(node, key, &self.to_string())
            }
            TreeFinding::StrictLink { node, argument } => {
                Error::strict_dependency(node, argument, &self.to_string())
            }
            TreeFinding::StrictCwd { node, key } => {
                Error::strict_dependency(node, key, &self.to_string())
            }
            TreeFinding::Cycle { nodes } => Error::tree_cycle(nodes),
            TreeFinding::NoFilename { .. }
            | TreeFinding::DuplicateFilename { .. }
            | TreeFinding::PlanFailed { .. }
            | TreeFinding::UnknownRoot { .. }
            | TreeFinding::ResultNotANode { .. }
            | TreeFinding::RootsBesideResult { .. }
            | TreeFinding::SecondParent { .. }
            | TreeFinding::Orphan { .. }
            | TreeFinding::Dangling { .. }
            | TreeFinding::MissingSource { .. }
            | TreeFinding::UndeclaredSource { .. }
            | TreeFinding::Unchecked { .. }
            | TreeFinding::NameCollision { .. } => Error::tree_check(&self.to_string()),
        }
    }
}

/// The result of [`check_tree`].
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Default)]
pub struct TreeReport {
    /// The roots, declared or found.
    pub roots: Vec<String>,
    /// The declared result.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub result: Option<String>,
    /// The graph.
    pub graph: TreeGraph,
    /// Errors: the tree fails its check.
    pub errors: Vec<TreeFinding>,
    /// Warnings.
    pub warnings: Vec<TreeFinding>,
}

impl TreeReport {
    /// The first error as an [`Error`], with every error in its message; `None` when the tree
    /// checks.
    pub fn error(&self) -> Option<Error> {
        match self.errors.as_slice() {
            [] => None,
            [one] => Some(one.to_error()),
            [first, ..] => {
                let all: Vec<String> = self.errors.iter().map(|f| f.to_string()).collect();
                let mut error = first.to_error();
                error.message = format!("{}; all findings: {}", error.message, all.join("; "));
                Some(error)
            }
        }
    }
}

/// A node's name: its filename without the extension.
pub fn node_name(filename: &str) -> &str {
    match filename.rfind('.') {
        Some(i) if i > 0 => &filename[..i],
        Some(_) | None => filename,
    }
}

/// Whether `key` is directly in the folder: `./<filename>`.
fn is_local(key: &Key) -> bool {
    key.len() == 2 && key.iter().next().map(|n| n.name.as_str()) == Some(".")
}

/// A reference found in a plan, before it is attributed to a node.
struct PlanRef {
    to: Key,
    kind: RefKind,
}

/// What [`walk_plan`] collects.
#[derive(Default)]
struct PlanWalk {
    refs: Vec<PlanRef>,
    /// Arguments passed as links or pipelines by the recipe.
    link_arguments: Vec<String>,
    /// Working directories the recipe sets (beyond the leading one of its own `cwd`).
    cwds: Vec<Key>,
    /// Errors met while walking (a nested pipeline that cannot be planned).
    errors: Vec<String>,
}

/// Collects the references of a plan. Steps of a pipeline command's body are command logic:
/// only Node names bound into it from the call are collected.
fn walk_plan(plan: &Plan, cmr: &CommandMetadataRegistry, walk: &mut PlanWalk, body: bool) {
    for (index, step) in plan.steps.iter().enumerate() {
        match step {
            Step::GetAsset(key)
            | Step::GetAssetBinary(key)
            | Step::GetAssetMetadata(key)
            | Step::GetAssetRecipe(key)
            | Step::GetAssetDirectory(key)
            | Step::GetResource(key)
            | Step::GetResourceMetadata(key)
            | Step::GetResourceDirectory(key) => {
                if !body {
                    walk.refs.push(PlanRef {
                        to: key.clone(),
                        kind: RefKind::Resource,
                    });
                }
            }
            Step::UseKeyValue(key) => {
                if !body {
                    walk.refs.push(PlanRef {
                        to: key.clone(),
                        kind: RefKind::KeyValue,
                    });
                }
            }
            Step::Evaluate(query) => {
                if !body {
                    if let Some(key) = query.key() {
                        walk.refs.push(PlanRef {
                            to: key,
                            kind: RefKind::Resource,
                        });
                    }
                }
            }
            Step::SetCwd(key) => {
                if !body {
                    walk.cwds.push(key.clone());
                }
            }
            Step::Action { parameters, .. } => {
                for parameter in &parameters.0 {
                    walk_parameter(parameter, index, cmr, walk, body);
                }
            }
            Step::PipelineCommand {
                parameters, plan, ..
            } => {
                for parameter in &parameters.0 {
                    walk_parameter(parameter, index, cmr, walk, body);
                }
                walk_plan(plan, cmr, walk, true);
            }
            Step::Plan(nested) => walk_plan(nested, cmr, walk, body),
            Step::UseQueryValue(_)
            | Step::UseValue(_)
            | Step::Filename(_)
            | Step::Info(_)
            | Step::Warning(_)
            | Step::Error(_) => {}
        }
    }
}

fn walk_parameter(
    parameter: &ParameterValue,
    action: usize,
    cmr: &CommandMetadataRegistry,
    walk: &mut PlanWalk,
    body: bool,
) {
    match parameter {
        ParameterValue::NodeLink(arg, _, query, _) => {
            if let Some(key) = query.key() {
                walk.refs.push(PlanRef {
                    to: key,
                    kind: RefKind::NodeArg {
                        action,
                        arg: arg.clone(),
                    },
                });
            }
        }
        ParameterValue::ParameterLink(arg, query, _) | ParameterValue::OverrideLink(arg, query) => {
            if body {
                return;
            }
            walk.link_arguments.push(arg.clone());
            if let Some(key) = query.key() {
                walk.refs.push(PlanRef {
                    to: key,
                    kind: RefKind::Link { arg: arg.clone() },
                });
            }
        }
        ParameterValue::ParameterPipeline(arg, pipeline, _)
        | ParameterValue::OverridePipeline(arg, pipeline) => {
            if body {
                return;
            }
            walk.link_arguments.push(arg.clone());
            match pipeline.to_plan(cmr) {
                Ok(plan) => {
                    let mut nested = PlanWalk::default();
                    walk_plan(&plan, cmr, &mut nested, false);
                    for r in nested.refs {
                        walk.refs.push(PlanRef {
                            to: r.to,
                            kind: RefKind::Link { arg: arg.clone() },
                        });
                    }
                    walk.errors.extend(nested.errors);
                }
                Err(error) => walk.errors.push(error.to_string()),
            }
        }
        ParameterValue::MultipleParameters(_, items) => {
            for item in items {
                walk_parameter(item, action, cmr, walk, body);
            }
        }
        // Trusted command logic (AC-23), and values.
        ParameterValue::DefaultLink(_, _)
        | ParameterValue::DefaultPipeline(_, _)
        | ParameterValue::EnumLink(_, _, _)
        | ParameterValue::DefaultValue(_, _)
        | ParameterValue::ParameterValue(_, _, _)
        | ParameterValue::OverrideValue(_, _)
        | ParameterValue::Placeholder(_)
        | ParameterValue::Injected(_)
        | ParameterValue::InputState(_)
        | ParameterValue::FrameArgument(_, _)
        | ParameterValue::None => {}
    }
}

/// The plan of `recipe`, with no working directory so keys stay as written.
fn unfrozen_plan(
    recipe: &crate::recipes::Recipe,
    cmr: &CommandMetadataRegistry,
) -> Result<Plan, Error> {
    let mut recipe = recipe.clone();
    recipe.cwd = None;
    recipe.to_plan(cmr)
}

/// The per-recipe strict rules (AC-7, AC-23) over one recipe's unfrozen plan: keyed
/// dependencies directly in the folder only, no link or nested-pipeline argument written by the
/// recipe, and no change of the working directory. `node` names the recipe in the findings.
pub fn check_strict(node: &str, plan: &Plan, cmr: &CommandMetadataRegistry) -> Vec<TreeFinding> {
    let mut walk = PlanWalk::default();
    walk_plan(plan, cmr, &mut walk, false);
    strict_findings(node, &walk)
}

fn strict_findings(node: &str, walk: &PlanWalk) -> Vec<TreeFinding> {
    let mut findings = Vec::new();
    for key in &walk.cwds {
        findings.push(TreeFinding::StrictCwd {
            node: node.to_owned(),
            key: key.encode(),
        });
    }
    for argument in &walk.link_arguments {
        findings.push(TreeFinding::StrictLink {
            node: node.to_owned(),
            argument: argument.clone(),
        });
    }
    for r in &walk.refs {
        if matches!(r.kind, RefKind::Link { .. }) {
            continue; // already refused as a link argument
        }
        if !is_local(&r.to) {
            findings.push(TreeFinding::StrictKey {
                node: node.to_owned(),
                key: r.to.encode(),
            });
        }
    }
    findings
}

impl TreeGraph {
    /// Plans each recipe (unfrozen, so keys stay relative) and collects its references. The
    /// nodes are the recipes and declared sources, merged with `listing` when given: a `listdir`
    /// of the folder from the store, the asset manager or the recipe provider (names already in
    /// the recipes are merged, so any of the three works). `None` when the listing is unknown.
    ///
    /// Fails only on the first recipe that cannot be planned; [`check_tree`] reports every
    /// such recipe instead.
    pub fn build(
        list: &RecipeList,
        listing: Option<&[String]>,
        cmr: &CommandMetadataRegistry,
    ) -> Result<Self, Error> {
        let (graph, analysis) = analyse(list, listing, cmr);
        match analysis.plan_errors.into_iter().next() {
            Some(error) => Err(error),
            None => Ok(graph),
        }
    }

    /// Filenames of the nodes that `filename` references, directly in the folder, in order and
    /// without repetition.
    pub fn children(&self, filename: &str) -> Vec<String> {
        let mut seen = BTreeSet::new();
        let mut children = Vec::new();
        for edge in self.edges.iter().filter(|e| e.from == filename) {
            if let Some(target) = edge.local_target() {
                if self.nodes.contains_key(target) && seen.insert(target.to_owned()) {
                    children.push(target.to_owned());
                }
            }
        }
        children
    }

    /// Filenames of the nodes that reference `filename`.
    pub fn parents(&self, filename: &str) -> Vec<String> {
        let mut parents: Vec<String> = self
            .edges
            .iter()
            .filter(|e| e.local_target() == Some(filename) && e.from != filename)
            .map(|e| e.from.clone())
            .collect();
        parents.sort();
        parents.dedup();
        parents
    }
}

/// What building the graph found besides it.
#[derive(Default)]
struct Analysis {
    findings: Vec<TreeFinding>,
    plan_errors: Vec<Error>,
    strict_findings: Vec<TreeFinding>,
}

fn analyse(
    list: &RecipeList,
    listing: Option<&[String]>,
    cmr: &CommandMetadataRegistry,
) -> (TreeGraph, Analysis) {
    let mut graph = TreeGraph::default();
    let mut analysis = Analysis::default();
    for (index, recipe) in list.recipes.iter().enumerate() {
        let filename = match recipe.filename() {
            Ok(Some(f)) => f.name,
            Ok(None) => {
                analysis.findings.push(TreeFinding::NoFilename {
                    recipe: index,
                    query: recipe.query.clone(),
                });
                continue;
            }
            Err(error) => {
                analysis.findings.push(TreeFinding::PlanFailed {
                    node: format!("recipe {index}"),
                    error: error.to_string(),
                });
                analysis.plan_errors.push(error);
                continue;
            }
        };
        if graph.nodes.contains_key(&filename) {
            analysis.findings.push(TreeFinding::DuplicateFilename {
                filename: filename.clone(),
            });
            continue;
        }
        let origin = if recipe.is_source_declaration() {
            NodeOrigin::Source(index)
        } else {
            NodeOrigin::Recipe(index)
        };
        graph.nodes.insert(
            filename.clone(),
            TreeNode {
                name: node_name(&filename).to_owned(),
                filename: filename.clone(),
                title: recipe.title.clone(),
                origin,
            },
        );
        let plan = match unfrozen_plan(recipe, cmr) {
            Ok(plan) => plan,
            Err(error) => {
                analysis.findings.push(TreeFinding::PlanFailed {
                    node: filename.clone(),
                    error: error.to_string(),
                });
                analysis.plan_errors.push(error);
                continue;
            }
        };
        let mut walk = PlanWalk::default();
        walk_plan(&plan, cmr, &mut walk, false);
        for error in &walk.errors {
            analysis.findings.push(TreeFinding::PlanFailed {
                node: filename.clone(),
                error: error.clone(),
            });
        }
        analysis
            .strict_findings
            .extend(strict_findings(&filename, &walk));
        for r in walk.refs {
            graph.edges.push(TreeEdge {
                from: filename.clone(),
                to: r.to.encode(),
                kind: r.kind,
            });
        }
    }
    if let Some(listing) = listing {
        for name in listing {
            if name == "recipes.yaml" || name.is_empty() || name.contains('/') {
                continue;
            }
            graph.nodes.entry(name.clone()).or_insert_with(|| TreeNode {
                filename: name.clone(),
                name: node_name(name).to_owned(),
                title: String::new(),
                origin: NodeOrigin::Listed,
            });
        }
    }
    (graph, analysis)
}

/// The recipes of `graph` nothing depends on, in filename order.
fn found_roots(graph: &TreeGraph) -> Vec<String> {
    graph
        .nodes
        .values()
        .filter(|n| matches!(n.origin, NodeOrigin::Recipe(_)))
        .filter(|n| graph.parents(&n.filename).is_empty())
        .map(|n| n.filename.clone())
        .collect()
}

impl RecipeList {
    /// Every root of the recipes, for display (design `strict-tree-editing`, AC-17): the
    /// declared result, the declared roots that are nodes, and every recipe nothing depends on,
    /// without duplicates. Declared roots are optional hints; a node not declared can still be a
    /// root.
    pub fn roots(&self, cmr: &CommandMetadataRegistry) -> Vec<String> {
        let (graph, _) = analyse(self, None, cmr);
        let mut roots = Vec::new();
        let declaration = self.tree.clone().unwrap_or_default();
        let declared = declaration.result.iter().chain(declaration.roots.iter());
        for root in declared.chain(found_roots(&graph).iter()) {
            if graph.nodes.contains_key(root) && !roots.contains(root) {
                roots.push(root.clone());
            }
        }
        roots
    }
}

/// Keeps `list` a valid tree where that changes no recipe's meaning (design
/// `strict-tree-editing`, AC-11, AC-16, AC-17). A list with no recipes and no `tree:` section
/// gets an empty one; a non-empty list without a section is left plain. In a tree, a `result`
/// or a `roots` entry that names no node is dropped, an empty `roots` is dropped, and a tree with
/// a recipe that has a filename but no result gets one: its first found root, else its first
/// such recipe.
fn normalize(list: &mut RecipeList, listing: Option<&[String]>, cmr: &CommandMetadataRegistry) {
    if list.tree.is_none() {
        if list.recipes.is_empty() {
            list.tree = Some(TreeDeclaration::default());
        }
        return;
    }
    let (graph, _) = analyse(list, listing, cmr);
    let first_root = found_roots(&graph).into_iter().next();
    let first_recipe = list
        .recipes
        .iter()
        .filter_map(|r| r.filename().ok().flatten().map(|f| f.name))
        .find(|f| graph.nodes.contains_key(f));
    let Some(tree) = list.tree.as_mut() else {
        return;
    };
    tree.roots.retain(|r| graph.nodes.contains_key(r));
    if tree.result.as_ref().is_some_and(|r| !graph.nodes.contains_key(r)) {
        tree.result = None;
    }
    if tree.result.is_none() {
        tree.result = first_root.or(first_recipe);
    }
    if tree.roots.as_slice() == tree.result.as_slice() {
        tree.roots.clear();
    }
}

/// The whole tree check (AC-3–AC-8, AC-16, AC-19, AC-23, AC-27, AC-28). Never fails: every
/// problem is a finding in the report. Without a `tree:` section the list is checked with the
/// defaults (`dag`, `open`).
///
/// `listing` is the folder's `listdir` (see [`TreeGraph::build`]). Without it, a reference to a
/// file no recipe declares is a warning, and an error under strict mode.
pub fn check_tree(
    list: &RecipeList,
    listing: Option<&[String]>,
    cmr: &CommandMetadataRegistry,
) -> TreeReport {
    let declaration = list.tree.clone().unwrap_or_default();
    let strict = declaration.dependencies == DependencyMode::Strict;
    let (graph, analysis) = analyse(list, listing, cmr);
    let mut errors = analysis.findings;
    let mut warnings = Vec::new();
    if strict {
        errors.extend(analysis.strict_findings);
    }

    // References to files of the folder: dangling, undeclared, unchecked (AC-8, AC-28).
    let mut reported = BTreeSet::new();
    for edge in &graph.edges {
        let Some(target) = edge.local_target() else {
            continue;
        };
        if !reported.insert((edge.from.clone(), target.to_owned())) {
            continue;
        }
        match graph.nodes.get(target).map(|n| n.origin) {
            Some(NodeOrigin::Recipe(_)) | Some(NodeOrigin::Source(_)) => {}
            Some(NodeOrigin::Listed) => {
                if strict {
                    errors.push(TreeFinding::UndeclaredSource {
                        node: edge.from.clone(),
                        file: target.to_owned(),
                    });
                }
            }
            None => match (listing.is_some(), strict) {
                (true, _) => errors.push(TreeFinding::Dangling {
                    node: edge.from.clone(),
                    key: edge.to.clone(),
                }),
                (false, true) => errors.push(TreeFinding::UndeclaredSource {
                    node: edge.from.clone(),
                    file: target.to_owned(),
                }),
                (false, false) => warnings.push(TreeFinding::Unchecked {
                    node: edge.from.clone(),
                    file: target.to_owned(),
                }),
            },
        }
    }

    // Declared sources must be stored, when the listing says what is stored (AC-27).
    if let Some(listing) = listing {
        for node in graph.nodes.values() {
            if matches!(node.origin, NodeOrigin::Source(_))
                && !listing.iter().any(|name| *name == node.filename)
            {
                errors.push(TreeFinding::MissingSource {
                    node: node.filename.clone(),
                });
            }
        }
    }

    // Name collisions matter only when something depends on one of the nodes (AC-16).
    let mut by_name: BTreeMap<&str, Vec<&str>> = BTreeMap::new();
    for node in graph.nodes.values() {
        by_name
            .entry(node.name.as_str())
            .or_default()
            .push(node.filename.as_str());
    }
    for (name, filenames) in &by_name {
        if filenames.len() > 1
            && filenames
                .iter()
                .any(|f| graph.edges.iter().any(|e| e.local_target() == Some(f)))
        {
            errors.push(TreeFinding::NameCollision {
                name: (*name).to_owned(),
                filenames: filenames.iter().map(|f| (*f).to_owned()).collect(),
            });
        }
    }

    // Roots and result (AC-3, AC-19).
    let roots: Vec<String> = match &declaration.result {
        Some(result) => {
            if !graph.nodes.contains_key(result) {
                errors.push(TreeFinding::ResultNotANode {
                    result: result.clone(),
                });
            }
            // Roots beside a result are display hints (design `strict-tree-editing`, AC-17).
            for root in &declaration.roots {
                if !graph.nodes.contains_key(root) {
                    warnings.push(TreeFinding::UnknownRoot { root: root.clone() });
                }
            }
            vec![result.clone()]
        }
        None if !declaration.roots.is_empty() => {
            for root in &declaration.roots {
                if !graph.nodes.contains_key(root) {
                    warnings.push(TreeFinding::UnknownRoot { root: root.clone() });
                }
            }
            // Declared roots are display hints: a recipe nothing depends on is a root too
            // (design `strict-tree-editing`, AC-17).
            let mut roots: Vec<String> = declaration
                .roots
                .iter()
                .filter(|r| graph.nodes.contains_key(*r))
                .cloned()
                .collect();
            for root in found_roots(&graph) {
                if !roots.contains(&root) {
                    roots.push(root);
                }
            }
            roots
        }
        // The recipes nothing depends on; a source or a stored file nothing uses is an orphan.
        None => found_roots(&graph),
    };

    errors.extend(find_cycles(&graph));

    if declaration.shape == TreeShape::Tree {
        for node in graph.nodes.keys() {
            let parents = graph.parents(node);
            if parents.len() > 1 {
                errors.push(TreeFinding::SecondParent {
                    node: node.clone(),
                    parents,
                });
            }
        }
    }

    // Orphans (AC-6): reached from no root.
    let mut reached = BTreeSet::new();
    let mut stack: Vec<String> = roots
        .iter()
        .filter(|r| graph.nodes.contains_key(*r))
        .cloned()
        .collect();
    while let Some(node) = stack.pop() {
        if reached.insert(node.clone()) {
            stack.extend(graph.children(&node));
        }
    }
    for node in graph.nodes.keys() {
        if !reached.contains(node) {
            warnings.push(TreeFinding::Orphan { node: node.clone() });
        }
    }

    TreeReport {
        roots,
        result: declaration.result.clone(),
        graph,
        errors,
        warnings,
    }
}

/// Every cycle of the node graph, each once, in reference order (AC-4).
fn find_cycles(graph: &TreeGraph) -> Vec<TreeFinding> {
    #[derive(Clone, Copy, PartialEq)]
    enum Mark {
        New,
        Open,
        Done,
    }
    fn visit(
        node: &str,
        graph: &TreeGraph,
        marks: &mut BTreeMap<String, Mark>,
        path: &mut Vec<String>,
        found: &mut Vec<TreeFinding>,
    ) {
        marks.insert(node.to_owned(), Mark::Open);
        path.push(node.to_owned());
        for child in graph.children(node) {
            match marks.get(&child).copied().unwrap_or(Mark::New) {
                Mark::New => visit(&child, graph, marks, path, found),
                Mark::Open => {
                    if let Some(start) = path.iter().position(|n| *n == child) {
                        let mut nodes: Vec<String> = path[start..].to_vec();
                        nodes.push(child.clone());
                        found.push(TreeFinding::Cycle { nodes });
                    }
                }
                Mark::Done => {}
            }
        }
        path.pop();
        marks.insert(node.to_owned(), Mark::Done);
    }
    let mut marks: BTreeMap<String, Mark> = BTreeMap::new();
    let mut found = Vec::new();
    for node in graph.nodes.keys() {
        if marks.get(node).copied().unwrap_or(Mark::New) == Mark::New {
            visit(node, graph, &mut marks, &mut Vec::new(), &mut found);
        }
    }
    // A self-reference is a cycle of one.
    for edge in &graph.edges {
        if edge.local_target() == Some(edge.from.as_str()) {
            let nodes = vec![edge.from.clone(), edge.from.clone()];
            if !found.contains(&TreeFinding::Cycle {
                nodes: nodes.clone(),
            }) {
                found.push(TreeFinding::Cycle { nodes });
            }
        }
    }
    found
}

// ---------------------------------------------------------------------------------------------
// Editing a tree
// ---------------------------------------------------------------------------------------------

/// A node a Node argument can name, as an editor lists it (AC-18).
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq)]
pub struct NodeChoice {
    /// The name written in the query: `wheel`.
    pub name: String,
    /// The file: `wheel.glb`.
    pub filename: String,
    /// The recipe's title; empty for a stored file no recipe declares.
    pub title: String,
}

/// A Node argument of a recipe that a new child can be named in (design `recipes-editor`,
/// AC-7): the argument `arg` of the action at index `action` (counting the recipe's actions
/// from 0), taking a node of `extension`.
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq)]
pub struct ChildSlot {
    /// The action's index among the recipe's actions.
    pub action: usize,
    /// The command's name, for display.
    pub command: String,
    /// The argument's name.
    pub arg: String,
    /// The extension a node named there must have.
    pub extension: String,
}

/// A stored file [`TreeEditor::save`] renames.
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq)]
pub struct DataRename {
    /// The current filename.
    pub from: String,
    /// The new filename.
    pub to: String,
}

/// In-memory edits of one folder's recipes that keep the tree valid (AC-1, AC-11–AC-15).
///
/// Every edit works on a copy and replaces the recipes only when [`check_tree`] reports no error
/// the recipes did not already have; a refused edit changes nothing. Renames of stored files are
/// collected ([`Self::pending_data_renames`]) and performed by [`Self::save`], or by a client that
/// has no environment (a UI talking to a server).
#[derive(Debug, Clone)]
pub struct TreeEditor {
    list: RecipeList,
    listing: Option<Vec<String>>,
    cmr: Arc<CommandMetadataRegistry>,
    renames: Vec<DataRename>,
}

/// What a reference rewrite does to a Node name it matches.
#[derive(Clone, Copy)]
enum Rewrite<'a> {
    /// Rename the node: its file, its name and every reference.
    Rename { to_file: &'a str },
    /// Clear Node arguments naming the node (they take their defaults).
    Clear,
}

/// Rewrites the references of one recipe to one file of the folder.
struct Rewriter<'a> {
    from_file: &'a str,
    from_name: &'a str,
    mode: Rewrite<'a>,
    cmr: &'a CommandMetadataRegistry,
}

impl Rewriter<'_> {
    fn new_key(&self, key: &str) -> Option<String> {
        let parsed = crate::parse::parse_key(key).ok()?;
        if !is_local(&parsed) || parsed.filename().map(|f| f.name.as_str()) != Some(self.from_file) {
            return None;
        }
        match self.mode {
            Rewrite::Rename { to_file, .. } => Some(format!("./{to_file}")),
            Rewrite::Clear => None,
        }
    }

    /// Rewrites query text; `Ok(false)` when it has nothing to rewrite.
    fn text(&self, text: &mut String) -> Result<bool, Error> {
        let mut pipeline = Pipeline::from_query_text(text)?;
        if self.pipeline(&mut pipeline)? {
            *text = pipeline.to_query_lenient(self.cmr)?.encode();
            Ok(true)
        } else {
            Ok(false)
        }
    }

    fn pipeline(&self, pipeline: &mut Pipeline) -> Result<bool, Error> {
        let realm = pipeline.realm.clone();
        let mut changed = false;
        for element in pipeline.elements.iter_mut() {
            changed |= match &mut element.item {
                ElementKind::Resource {
                    resource: Operand::Literal(key),
                    ..
                }
                | ElementKind::KeyValue(Operand::Literal(key)) => match self.new_key(key) {
                    Some(new) => {
                        *key = new;
                        true
                    }
                    None => false,
                },
                ElementKind::Evaluate(Operand::Literal(inner))
                | ElementKind::QueryValue(Operand::Literal(inner)) => match inner {
                    QueryOrPipeline::Query(text) => self.text(text)?,
                    QueryOrPipeline::Pipeline(p) => self.pipeline(p)?,
                },
                ElementKind::Text(text) => self.text(text)?,
                ElementKind::Action(action) => self.action(action, &realm)?,
                ElementKind::Pipeline(nested) => self.pipeline(nested)?,
                ElementKind::Resource {
                    resource: Operand::Placeholder(_),
                    ..
                }
                | ElementKind::KeyValue(Operand::Placeholder(_))
                | ElementKind::Evaluate(Operand::Placeholder(_))
                | ElementKind::QueryValue(Operand::Placeholder(_))
                | ElementKind::Cwd(_)
                | ElementKind::Value(_)
                | ElementKind::Quote
                | ElementKind::Volatile => false,
            };
        }
        Ok(changed)
    }

    fn action(&self, action: &mut ActionElement, realm: &str) -> Result<bool, Error> {
        let md = crate::pipeline::resolve_action(self.cmr, realm, action).ok();
        let slots: Vec<ArgumentType> = md
            .as_ref()
            .map(|md| {
                crate::pipeline::positional_arguments(md)
                    .iter()
                    .map(|a| a.argument_type.clone())
                    .collect()
            })
            .unwrap_or_default();
        let multiple = md
            .as_ref()
            .map(|md| {
                crate::pipeline::positional_arguments(md)
                    .iter()
                    .map(|a| a.multiple)
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default();
        let mut changed = false;
        let mut slot = 0usize;
        for parameter in action.parameters.iter_mut() {
            changed |= self.source(&mut parameter.item, slots.get(slot))?;
            if !multiple.get(slot).copied().unwrap_or(false) {
                slot += 1;
            }
        }
        for (name, argument) in action.arguments.iter_mut() {
            let ty = md
                .as_ref()
                .and_then(|md| md.arguments.iter().find(|a| a.name == *name))
                .map(|a| &a.argument_type);
            changed |= self.source(&mut argument.item, ty)?;
        }
        if changed && matches!(self.mode, Rewrite::Clear) {
            // A cleared trailing parameter is dropped, so the query does not end in `-`.
            while action
                .parameters
                .last()
                .is_some_and(|p| p.item == Source::Value(Json::String(String::new())))
            {
                action.parameters.pop();
            }
            action
                .arguments
                .retain(|_, a| a.item != Source::Value(Json::String(String::new())));
        }
        Ok(changed)
    }

    fn source(&self, source: &mut Source, ty: Option<&ArgumentType>) -> Result<bool, Error> {
        match source {
            Source::Value(Json::String(name)) => match ty.and_then(|t| t.node()) {
                Some(node) if node.filename(name) == self.from_file => {
                    *name = match self.mode {
                        // A `.node` file is named by its filename; a file of another extension
                        // than the argument's is named in full, and the check reports it.
                        Rewrite::Rename { to_file, .. } => {
                            node.name_for(to_file).unwrap_or_else(|| to_file.to_owned())
                        }
                        Rewrite::Clear => String::new(),
                    };
                    Ok(true)
                }
                Some(_) | None => Ok(false),
            },
            Source::Link(text) => match self.mode {
                Rewrite::Rename { .. } => self.text(text),
                Rewrite::Clear => Ok(false),
            },
            Source::Pipeline(p) => match self.mode {
                Rewrite::Rename { .. } => self.pipeline(p),
                Rewrite::Clear => Ok(false),
            },
            Source::Multiple(items) => {
                let mut changed = false;
                for item in items.iter_mut() {
                    changed |= self.source(item, ty)?;
                }
                Ok(changed)
            }
            Source::Value(_) | Source::Placeholder(_) | Source::State(_) => Ok(false),
        }
    }

    /// Rewrites one recipe, its own filename included. Refuses a recipe whose references cannot
    /// be read (AC-15).
    fn recipe(&self, index: usize, recipe: &mut Recipe) -> Result<(), Error> {
        let label = recipe_label(recipe);
        let refuse = |e: Error| {
            Error::tree_check(&format!(
                "recipe {index} ('{label}') cannot be rewritten: {}",
                e.message
            ))
        };
        let mentions = |text: &str| text.contains(self.from_name);
        if !recipe.query.is_empty() {
            match Pipeline::from_query_text(&recipe.query) {
                Ok(mut pipeline) => {
                    let mut changed = self.pipeline(&mut pipeline).map_err(&refuse)?;
                    changed |= self.own_filename(&mut pipeline.filename);
                    if changed {
                        recipe.query = pipeline
                            .to_query_lenient(self.cmr)
                            .map_err(&refuse)?
                            .encode();
                    }
                }
                Err(e) if mentions(&recipe.query) => return Err(refuse(e)),
                Err(_) => {}
            }
        }
        if let Some(pipeline) = recipe.pipeline.as_mut() {
            self.pipeline(pipeline).map_err(&refuse)?;
            self.own_filename(&mut pipeline.filename);
        }
        for link in recipe.links.values_mut() {
            match self.text(link) {
                Ok(_) => {}
                Err(e) if mentions(link) => return Err(refuse(e)),
                Err(_) => {}
            }
        }
        if !recipe.arguments.is_empty() {
            let command = unfrozen_plan(recipe, self.cmr)
                .ok()
                .and_then(|plan| plan.last_action_command(self.cmr));
            for (name, value) in recipe.arguments.iter_mut() {
                let ty = command
                    .as_ref()
                    .and_then(|c| c.arguments.iter().find(|a| a.name == *name))
                    .map(|a| a.argument_type.clone());
                let mut source = Source::Value(value.clone());
                if self.source(&mut source, ty.as_ref())? {
                    if let Source::Value(new) = source {
                        *value = new;
                    }
                }
            }
        }
        Ok(())
    }

    fn own_filename(&self, filename: &mut Option<String>) -> bool {
        match (self.mode, filename.as_deref()) {
            (Rewrite::Rename { to_file, .. }, Some(f)) if f == self.from_file => {
                *filename = Some(to_file.to_owned());
                true
            }
            _ => false,
        }
    }
}

fn recipe_label(recipe: &Recipe) -> String {
    match recipe.filename() {
        Ok(Some(f)) => f.name,
        Ok(None) | Err(_) => recipe.query.clone(),
    }
}

fn extension_of(filename: &str) -> &str {
    match filename.rfind('.') {
        Some(i) if i > 0 => &filename[i + 1..],
        Some(_) | None => "",
    }
}

impl TreeEditor {
    /// An editor over `list`, with the folder's `listdir` output when known (from the store,
    /// the asset manager or the recipe provider; recipe names are merged with it). The working
    /// directory a provider sets on each recipe is cleared: it is never written to
    /// `recipes.yaml`.
    pub fn new(
        mut list: RecipeList,
        listing: Option<Vec<String>>,
        cmr: Arc<CommandMetadataRegistry>,
    ) -> Result<Self, Error> {
        for recipe in list.recipes.iter_mut() {
            recipe.cwd = None;
        }
        normalize(&mut list, listing.as_deref(), &cmr);
        Ok(TreeEditor {
            list,
            listing,
            cmr,
            renames: Vec::new(),
        })
    }

    /// The check of the current recipes.
    pub fn report(&self) -> TreeReport {
        check_tree(&self.list, self.listing.as_deref(), &self.cmr)
    }

    /// The current recipes.
    pub fn recipes(&self) -> &RecipeList {
        &self.list
    }

    /// The stored-file renames [`Self::save`] performs.
    pub fn pending_data_renames(&self) -> &[DataRename] {
        &self.renames
    }

    /// The key of the declared result in `folder` (AC-19); `None` without one.
    pub fn result_key(&self, folder: &Key) -> Option<Key> {
        self.list
            .tree
            .as_ref()
            .and_then(|t| t.result.as_ref())
            .map(|result| folder.join(result))
    }

    /// The nodes a Node argument of `extension` can name, by filename (AC-18).
    pub fn node_choices(&self, extension: &str) -> Vec<NodeChoice> {
        let (graph, _) = analyse(&self.list, self.listing.as_deref(), &self.cmr);
        graph
            .nodes
            .values()
            .filter(|n| {
                let ext = extension_of(&n.filename);
                ext == extension || ext == crate::metadata::NODE_EXTENSION
            })
            .map(|n| NodeChoice {
                // A `.node` file is named by its filename in any Node argument (AC-12).
                name: if extension_of(&n.filename) == crate::metadata::NODE_EXTENSION {
                    n.filename.clone()
                } else {
                    n.name.clone()
                },
                filename: n.filename.clone(),
                title: n.title.clone(),
            })
            .collect()
    }

    /// A filename for a new node titled `title`: the title in lower case, words joined by `_`,
    /// made unique among the folder's names ([`Self::unique_filename`]).
    pub fn suggest_filename(&self, title: &str, extension: &str) -> String {
        let mut stem = String::new();
        for c in title.chars() {
            if c.is_alphanumeric() {
                stem.extend(c.to_lowercase());
            } else if !stem.ends_with('_') && !stem.is_empty() {
                stem.push('_');
            }
        }
        let stem = stem.trim_end_matches('_').to_owned();
        let stem = if stem.is_empty() { "node".to_owned() } else { stem };
        let wanted = if extension.is_empty() {
            stem
        } else {
            format!("{stem}.{extension}")
        };
        self.unique_filename(&wanted, None)
    }

    /// `wanted`, or the first free variant of it among the folder's node names (design
    /// `strict-tree-editing`, AC-14, AC-15): `net.obj`, then `net1.obj`, `net2.obj`, …; a stem
    /// that is only digits takes the first free integer from 1. Names compare by node name, so
    /// `net.obj` and `net.png` collide. `except` is the node being renamed, whose own name is
    /// free.
    pub fn unique_filename(&self, wanted: &str, except: Option<&str>) -> String {
        let (graph, _) = analyse(&self.list, self.listing.as_deref(), &self.cmr);
        let used: BTreeSet<&str> = graph
            .nodes
            .values()
            .filter(|n| Some(n.filename.as_str()) != except)
            .map(|n| n.name.as_str())
            .collect();
        let (stem, extension) = match wanted.rfind('.') {
            Some(i) if i > 0 => (&wanted[..i], &wanted[i..]),
            Some(_) | None => (wanted, ""),
        };
        if !used.contains(stem) && !(stem.is_empty()) {
            return wanted.to_owned();
        }
        let numeric = !stem.is_empty() && stem.chars().all(|c| c.is_ascii_digit());
        let candidate = |n: usize| {
            if numeric || stem.is_empty() {
                n.to_string()
            } else {
                format!("{stem}{n}")
            }
        };
        let mut n = 1;
        while used.contains(candidate(n).as_str()) {
            n += 1;
        }
        format!("{}{extension}", candidate(n))
    }

    /// Adds an empty node `{query: <n>.<extension>, title}`, `<n>` the first free integer from 1,
    /// and returns its filename (design `strict-tree-editing`, AC-5). With the extension
    /// [`crate::metadata::NODE_EXTENSION`] (the default of the editors) it is no source
    /// declaration: it evaluates to the empty value, and a tree stays valid.
    pub fn add_empty(&mut self, title: &str, extension: &str) -> Result<String, Error> {
        let extension = extension.trim_start_matches('.');
        let extension = if extension.is_empty() {
            crate::metadata::NODE_EXTENSION
        } else {
            extension
        };
        let filename = self.unique_filename(&format!("1.{extension}"), None);
        let recipe = Recipe::new(filename.clone(), title.to_owned(), String::new())?;
        self.add(recipe)?;
        Ok(filename)
    }

    /// The filename of the node `node` names: a filename, or a name shared by no other node.
    fn resolve(&self, node: &str) -> Result<String, Error> {
        let (graph, _) = analyse(&self.list, self.listing.as_deref(), &self.cmr);
        if graph.nodes.contains_key(node) {
            return Ok(node.to_owned());
        }
        let matches: Vec<&String> = graph
            .nodes
            .values()
            .filter(|n| n.name == node)
            .map(|n| &n.filename)
            .collect();
        match matches.as_slice() {
            [one] => Ok((*one).clone()),
            [] => Err(Error::tree_check(&format!("no node '{node}'"))),
            _ => Err(Error::tree_check(&format!(
                "'{node}' names {}; give the filename",
                matches
                    .iter()
                    .map(|s| s.as_str())
                    .collect::<Vec<_>>()
                    .join(", ")
            ))),
        }
    }

    fn recipe_index(&self, filename: &str) -> Option<usize> {
        self.list
            .recipes
            .iter()
            .position(|r| matches!(r.filename(), Ok(Some(f)) if f.name == filename))
    }

    /// Replaces the recipes with `candidate` when it adds no error (AC-14).
    fn commit(
        &mut self,
        candidate: RecipeList,
        listing: Option<Vec<String>>,
        renames: Vec<DataRename>,
    ) -> Result<&mut Self, Error> {
        self.commit_new(candidate, listing, renames, None)
    }

    /// [`Self::commit`] of an edit adding the recipe of `added`: a new recipe is a source
    /// declaration until its query is written, so its own `MissingSource` is not an added error
    /// (design `recipes-editor`, AC-7).
    fn commit_new(
        &mut self,
        candidate: RecipeList,
        listing: Option<Vec<String>>,
        renames: Vec<DataRename>,
        added: Option<&str>,
    ) -> Result<&mut Self, Error> {
        let mut candidate = candidate;
        normalize(&mut candidate, listing.as_deref(), &self.cmr);
        let before = self.report().errors;
        let after = check_tree(&candidate, listing.as_deref(), &self.cmr);
        let tolerated = |e: &TreeFinding| matches!(e, TreeFinding::MissingSource { node } if Some(node.as_str()) == added);
        if let Some(new) = after.errors.iter().find(|e| !before.contains(e) && !tolerated(e)) {
            return Err(new.to_error());
        }
        self.list = candidate;
        self.listing = listing;
        self.renames = renames;
        Ok(self)
    }

    /// Declares the stored file `filename` a source with a title and description (AC-27).
    pub fn declare_source(
        &mut self,
        filename: &str,
        title: &str,
        description: &str,
    ) -> Result<&mut Self, Error> {
        if self.recipe_index(filename).is_some() {
            return Err(Error::tree_check(&format!(
                "'{filename}' already has a recipe"
            )));
        }
        let recipe = Recipe::new(filename.to_owned(), title.to_owned(), description.to_owned())?;
        if !recipe.is_source_declaration() {
            return Err(Error::invalid_token(filename, "a filename"));
        }
        let mut candidate = self.list.clone();
        candidate.recipes.push(recipe);
        self.commit(candidate, self.listing.clone(), self.renames.clone())
    }

    /// Renames node `from` to `to` (a name; or a filename, to change the extension too) and
    /// rewrites every reference to it (AC-1, AC-25): Node arguments naming it, resource
    /// segments and links reading its file, overrides, the tree declaration and its own
    /// filename. A stored file is renamed by [`Self::save`]. A reference that cannot be read
    /// refuses the rename (AC-15).
    pub fn rename(&mut self, from: &str, to: &str) -> Result<&mut Self, Error> {
        let from_file = self.resolve(from)?;
        let to_file = if to.contains('.') {
            to.to_owned()
        } else {
            match extension_of(&from_file) {
                "" => to.to_owned(),
                ext => format!("{to}.{ext}"),
            }
        };
        if !crate::escape::is_resource_name(&to_file) || to_file == "recipes.yaml" {
            return Err(Error::invalid_token(to, "a node name"));
        }
        // A used name is made unique, `net.obj` → `net1.obj` (design `strict-tree-editing`,
        // AC-14).
        let to_file = self.unique_filename(&to_file, Some(&from_file));
        if to_file == from_file {
            return Ok(self);
        }
        let (graph, _) = analyse(&self.list, self.listing.as_deref(), &self.cmr);
        let from_name = node_name(&from_file).to_owned();
        let rewriter = Rewriter {
            from_file: &from_file,
            from_name: &from_name,
            mode: Rewrite::Rename {
                to_file: &to_file,
            },
            cmr: &self.cmr,
        };
        let mut candidate = self.list.clone();
        for (index, recipe) in candidate.recipes.iter_mut().enumerate() {
            rewriter.recipe(index, recipe)?;
        }
        if let Some(tree) = candidate.tree.as_mut() {
            for root in tree.roots.iter_mut() {
                if *root == from_file {
                    *root = to_file.clone();
                }
            }
            if tree.result.as_deref() == Some(from_file.as_str()) {
                tree.result = Some(to_file.clone());
            }
        }
        // The stored file moves with the node: a listed file, or a source's data.
        let stored = match &self.listing {
            Some(listing) => listing.contains(&from_file),
            None => matches!(graph.nodes.get(&from_file).map(|n| n.origin), Some(NodeOrigin::Source(_))),
        };
        let mut listing = self.listing.clone();
        let mut renames = self.renames.clone();
        if stored {
            if let Some(listing) = listing.as_mut() {
                for name in listing.iter_mut() {
                    if *name == from_file {
                        *name = to_file.clone();
                    }
                }
            }
            match renames.iter_mut().find(|r| r.to == from_file) {
                Some(earlier) => earlier.to = to_file.clone(),
                None => renames.push(DataRename {
                    from: from_file.clone(),
                    to: to_file.clone(),
                }),
            }
            renames.retain(|r| r.from != r.to);
        }
        self.commit(candidate, listing, renames)
    }

    /// Sets argument `arg` of the action at index `action` (counting the actions of the
    /// recipe's query, from 0) of `recipe` to `source`.
    fn set_argument(
        &self,
        recipe: &mut Recipe,
        action: usize,
        arg: &str,
        source: Source,
    ) -> Result<(), Error> {
        let label = recipe_label(recipe);
        let mut pipeline = match (&recipe.pipeline, recipe.query.is_empty()) {
            (Some(p), true) => p.clone(),
            _ => Pipeline::from_query_text(&recipe.query)?,
        };
        let realm = pipeline.realm.clone();
        let element = pipeline
            .elements
            .iter_mut()
            .filter_map(|e| match &mut e.item {
                ElementKind::Action(a) => Some(a),
                _ => None,
            })
            .nth(action)
            .ok_or_else(|| {
                Error::tree_check(&format!("'{label}' has no action {action}"))
            })?;
        let md = crate::pipeline::resolve_action(&self.cmr, &realm, element)?;
        let slot = crate::pipeline::positional_arguments(&md)
            .iter()
            .position(|a| a.name == arg)
            .ok_or_else(|| {
                Error::tree_check(&format!(
                    "command '{}' of '{label}' has no argument '{arg}'",
                    element.action
                ))
            })?;
        if slot < element.parameters.len() {
            element.parameters[slot].item = source;
        } else if slot == element.parameters.len() {
            element.parameters.push(Located::new(source));
        } else {
            element.arguments.insert(arg.to_owned(), Located::new(source));
        }
        if recipe.query.is_empty() {
            recipe.pipeline = Some(pipeline);
        } else {
            recipe.query = pipeline.to_query_with(&self.cmr)?.encode();
        }
        Ok(())
    }

    /// Adds `recipe` and names it in argument `arg` of action `action` of `parent` (AC-11). A
    /// filename or a name already used in the folder is refused.
    pub fn add_child(
        &mut self,
        parent: &str,
        recipe: Recipe,
        action: usize,
        arg: &str,
    ) -> Result<&mut Self, Error> {
        let filename = recipe
            .filename()?
            .ok_or_else(|| Error::tree_check("a child recipe needs a filename"))?
            .name;
        let name = node_name(&filename).to_owned();
        // A `.node` child is named by its filename in any Node argument (AC-12).
        let argument_name = if extension_of(&filename) == crate::metadata::NODE_EXTENSION {
            filename.clone()
        } else {
            name.clone()
        };
        let (graph, _) = analyse(&self.list, self.listing.as_deref(), &self.cmr);
        if let Some(used) = graph.nodes.values().find(|n| n.name == name) {
            return Err(Error::tree_check(&format!(
                "name '{name}' is already used by '{}'",
                used.filename
            )));
        }
        let parent_file = self.resolve(parent)?;
        let index = self
            .recipe_index(&parent_file)
            .ok_or_else(|| Error::tree_check(&format!("'{parent_file}' has no recipe")))?;
        let mut candidate = self.list.clone();
        self.set_argument(
            &mut candidate.recipes[index],
            action,
            arg,
            Source::Value(Json::String(argument_name)),
        )?;
        candidate.recipes.push(recipe);
        self.commit_new(candidate, self.listing.clone(), self.renames.clone(), Some(&filename))
    }

    /// The Node arguments of the recipe of `parent` a new child can be named in (design
    /// `recipes-editor`, AC-7), for [`Self::add_child`].
    pub fn child_slots(&self, parent: &str) -> Result<Vec<ChildSlot>, Error> {
        let file = self.resolve(parent)?;
        let index = self
            .recipe_index(&file)
            .ok_or_else(|| Error::tree_check(&format!("'{file}' has no recipe")))?;
        let recipe = &self.list.recipes[index];
        let pipeline = match (&recipe.pipeline, recipe.query.is_empty()) {
            (Some(p), true) => p.clone(),
            (Some(_), false) | (None, true) | (None, false) => Pipeline::from_query_text(&recipe.query)?,
        };
        let mut slots = Vec::new();
        let actions = pipeline.elements.iter().filter_map(|e| match &e.item {
            ElementKind::Action(a) => Some(a),
            _ => None,
        });
        for (action, element) in actions.enumerate() {
            let Ok(md) = crate::pipeline::resolve_action(&self.cmr, &pipeline.realm, element) else {
                continue;
            };
            for argument in crate::pipeline::positional_arguments(&md) {
                if let ArgumentType::Node(node) = &argument.argument_type {
                    slots.push(ChildSlot {
                        action,
                        command: element.action.clone(),
                        arg: argument.name.clone(),
                        extension: node.extension.clone(),
                    });
                }
            }
        }
        Ok(slots)
    }

    /// Appends `recipe` as a new node (design `recipes-editor`, AC-7). A recipe without a
    /// filename, or with the filename of another recipe, is refused.
    pub fn add(&mut self, recipe: Recipe) -> Result<&mut Self, Error> {
        let filename = recipe
            .filename()?
            .ok_or_else(|| Error::tree_check("a new recipe needs a filename"))?
            .name;
        if filename == "recipes.yaml" || !crate::escape::is_resource_name(&filename) {
            return Err(Error::invalid_token(&filename, "a recipe filename"));
        }
        if self.recipe_index(&filename).is_some() {
            return Err(Error::tree_check(&format!("'{filename}' already has a recipe")));
        }
        let mut candidate = self.list.clone();
        candidate.recipes.push(recipe);
        self.commit_new(candidate, self.listing.clone(), self.renames.clone(), Some(&filename))
    }

    /// Appends a copy of the recipe of `node` under `to` (a name, keeping the extension, or a
    /// filename), made unique as in [`Self::unique_filename`]; the copy's references are those of
    /// the original (design `recipes-editor`, AC-9; `strict-tree-editing`, AC-14). Stored data is
    /// not copied.
    pub fn duplicate(&mut self, node: &str, to: &str) -> Result<&mut Self, Error> {
        let from_file = self.resolve(node)?;
        let index = self
            .recipe_index(&from_file)
            .ok_or_else(|| Error::tree_check(&format!("'{from_file}' has no recipe")))?;
        let to_file = if to.contains('.') {
            to.to_owned()
        } else {
            match extension_of(&from_file) {
                "" => to.to_owned(),
                ext => format!("{to}.{ext}"),
            }
        };
        if !crate::escape::is_resource_name(&to_file) || to_file == "recipes.yaml" {
            return Err(Error::invalid_token(to, "a node name"));
        }
        // A used name is made unique, `net.obj` → `net1.obj` (design `strict-tree-editing`,
        // AC-14).
        let to_file = self.unique_filename(&to_file, None);
        let from_name = node_name(&from_file).to_owned();
        let rewriter = Rewriter {
            from_file: &from_file,
            from_name: &from_name,
            mode: Rewrite::Rename {
                to_file: &to_file,
            },
            cmr: &self.cmr,
        };
        let mut copy = self.list.recipes[index].clone();
        rewriter.recipe(index, &mut copy)?;
        let mut candidate = self.list.clone();
        candidate.recipes.push(copy);
        self.commit(candidate, self.listing.clone(), self.renames.clone())
    }

    /// Replaces the recipe with filename `filename` by `recipe`, which must keep the filename (a
    /// rename is [`Self::rename`]); refused when it adds a check error (design `recipes-editor`,
    /// AC-5, AC-14).
    pub fn set_recipe(&mut self, filename: &str, recipe: Recipe) -> Result<&mut Self, Error> {
        let index = self
            .recipe_index(filename)
            .ok_or_else(|| Error::tree_check(&format!("'{filename}' has no recipe")))?;
        match recipe.filename()? {
            Some(name) if name.name == filename => {}
            Some(_) | None => {
                return Err(Error::tree_check(&format!(
                    "the recipe '{filename}' must keep its filename; rename the node instead"
                )))
            }
        }
        let mut candidate = self.list.clone();
        candidate.recipes[index] = recipe;
        self.commit(candidate, self.listing.clone(), self.renames.clone())
    }

    /// Declares `node` the tree's single result, or clears the result with `None` (AC-19). A
    /// list without a `tree:` section gets one.
    pub fn set_result(&mut self, node: Option<&str>) -> Result<&mut Self, Error> {
        let file = match node {
            Some(node) => Some(self.resolve(node)?),
            None => None,
        };
        let mut candidate = self.list.clone();
        let tree = candidate.tree.get_or_insert_with(TreeDeclaration::default);
        tree.result = file;
        self.commit(candidate, self.listing.clone(), self.renames.clone())
    }

    /// Replaces the whole list, as typed in a YAML editor (design `recipes-editor`, AC-12). It
    /// is not checked: a file being repaired may have errors, which the report then shows. The
    /// working directory a provider set on each recipe is cleared, as in [`Self::new`].
    pub fn replace(&mut self, mut list: RecipeList) -> Result<&mut Self, Error> {
        for recipe in list.recipes.iter_mut() {
            recipe.cwd = None;
        }
        self.list = list;
        Ok(self)
    }

    /// Deletes the recipe of `node` (AC-12). A node something references is refused, naming its
    /// parents. With `cascade`, the descendants no remaining node references are deleted too.
    /// Stored data is never deleted: a deleted source's file stays, as an orphan.
    pub fn delete(&mut self, node: &str, cascade: bool) -> Result<&mut Self, Error> {
        let file = self.resolve(node)?;
        let (graph, _) = analyse(&self.list, self.listing.as_deref(), &self.cmr);
        let parents = graph.parents(&file);
        if !parents.is_empty() {
            return Err(Error::tree_check(&format!(
                "'{file}' is referenced by {}",
                parents.join(", ")
            )));
        }
        let mut removed: BTreeSet<String> = BTreeSet::from([file.clone()]);
        if cascade {
            let mut frontier = vec![file.clone()];
            while let Some(current) = frontier.pop() {
                for child in graph.children(&current) {
                    if !removed.contains(&child)
                        && graph.parents(&child).iter().all(|p| removed.contains(p))
                    {
                        removed.insert(child.clone());
                        frontier.push(child);
                    }
                }
            }
        }
        let mut candidate = self.list.clone();
        candidate
            .recipes
            .retain(|r| !matches!(r.filename(), Ok(Some(f)) if removed.contains(&f.name)));
        if let Some(tree) = candidate.tree.as_mut() {
            tree.roots.retain(|r| !removed.contains(r));
            if tree.result.as_ref().is_some_and(|r| removed.contains(r)) {
                tree.result = None;
            }
        }
        self.commit(candidate, self.listing.clone(), self.renames.clone())
    }

    /// Moves the reference to `node` from recipe `from` to argument `arg` of action `action` of
    /// recipe `to` (AC-13). Only a Node argument naming `node` can be moved; the argument in
    /// `from` takes its default.
    pub fn move_reference(
        &mut self,
        node: &str,
        from: &str,
        to: &str,
        action: usize,
        arg: &str,
    ) -> Result<&mut Self, Error> {
        let node_file = self.resolve(node)?;
        let from_file = self.resolve(from)?;
        let to_file = self.resolve(to)?;
        let from_index = self
            .recipe_index(&from_file)
            .ok_or_else(|| Error::tree_check(&format!("'{from_file}' has no recipe")))?;
        let to_index = self
            .recipe_index(&to_file)
            .ok_or_else(|| Error::tree_check(&format!("'{to_file}' has no recipe")))?;
        let node_name_text = node_name(&node_file).to_owned();
        let rewriter = Rewriter {
            from_file: &node_file,
            from_name: &node_name_text,
            mode: Rewrite::Clear,
            cmr: &self.cmr,
        };
        let mut candidate = self.list.clone();
        rewriter.recipe(from_index, &mut candidate.recipes[from_index])?;
        let (graph, _) = analyse(&candidate, self.listing.as_deref(), &self.cmr);
        if graph.children(&from_file).contains(&node_file) {
            return Err(Error::tree_check(&format!(
                "'{from_file}' refers to '{node_file}' other than by a Node argument; that \
                 reference cannot be moved"
            )));
        }
        self.set_argument(
            &mut candidate.recipes[to_index],
            action,
            arg,
            Source::Value(Json::String(node_name_text.clone())),
        )?;
        self.commit(candidate, self.listing.clone(), self.renames.clone())
    }

    /// Writes the recipes to `folder`: first the stored-file renames, then `recipes.yaml`, both
    /// through the asset manager, so the recipe provider and dependent assets follow.
    pub async fn save<E: Environment>(&self, folder: &Key, envref: EnvRef<E>) -> Result<(), Error> {
        use crate::assets::AssetManager;
        use crate::value::ValueInterface;
        let manager = envref.get_asset_manager();
        let store = envref.get_async_store();
        for rename in &self.renames {
            let from = folder.join(&rename.from);
            let to = folder.join(&rename.to);
            // The bytes move with their stored record as it is; only the key changes. The
            // removal goes through the asset manager, which tells the provider and the assets.
            // A listing can name a node that was never stored (a recipe's file, listed by the
            // provider): there is nothing to move then.
            let (bytes, metadata) = match store.get(&from).await {
                Ok(entry) => entry,
                Err(e) if e.error_type == crate::error::ErrorType::KeyNotFound => continue,
                Err(e) => return Err(e),
            };
            let metadata = match metadata.metadata_record() {
                Some(mut record) => {
                    record.key = Some(to.clone());
                    crate::metadata::Metadata::MetadataRecord(record)
                }
                None => metadata,
            };
            store.set(&to, &bytes, &metadata).await?;
            manager.remove(&from).await?;
        }
        let yaml = serde_yaml::to_string(&self.list).map_err(|e| {
            Error::general_error(format!("Cannot write recipes.yaml: {e}"))
        })?;
        let key = folder.join("recipes.yaml");
        let value = E::Value::from_string(yaml.clone());
        let mut record = crate::metadata::MetadataRecord::new();
        record.key = Some(key.clone());
        record.type_identifier = value.identifier().to_string();
        record.type_name = value.type_name().to_string();
        record.data_format = Some("txt".to_owned());
        manager.set_binary(&key, yaml.as_bytes(), record).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::command_metadata::{ArgumentInfo, ArgumentType, CommandKey, NodeArgument};

    fn cmr() -> CommandMetadataRegistry {
        let mut cmr = CommandMetadataRegistry::new();
        let mut attach = crate::command_metadata::CommandMetadata::from_key(CommandKey::new_name(
            "attach",
        ));
        let mut part = ArgumentInfo::any_argument("part");
        part.argument_type = ArgumentType::Node(NodeArgument::new("glb"));
        attach.with_argument(part);
        cmr.add_command(&attach);
        let mut material = crate::command_metadata::CommandMetadata::from_key(
            CommandKey::new_name("material"),
        );
        let mut m = ArgumentInfo::any_argument("m");
        m.argument_type = ArgumentType::Node(NodeArgument::new("mat"));
        material.with_argument(m);
        cmr.add_command(&material);
        let mut with_dir =
            crate::command_metadata::CommandMetadata::from_key(CommandKey::new_name("with_dir"));
        let mut dir = ArgumentInfo::any_argument("dir");
        dir.default = crate::command_metadata::CommandParameterValue::Query(
            crate::parse::parse_query("-R-key/.").expect("query"),
        );
        with_dir.with_argument(dir);
        cmr.add_command(&with_dir);
        let mut any = crate::command_metadata::CommandMetadata::from_key(CommandKey::new_name("any"));
        any.with_argument(ArgumentInfo::any_argument("x"));
        cmr.add_command(&any);
        cmr
    }

    fn list(yaml: &str) -> RecipeList {
        serde_yaml::from_str(yaml).expect("recipes.yaml")
    }

    const CAR: &str = r#"
recipes:
  - {query: body.glb, title: Body}
  - {query: tyre.glb, title: Tyre}
  - {query: rubber.mat, title: Rubber}
  - {title: Wheel, query: "-R/./tyre.glb/-/material-rubber/wheel.glb"}
  - {title: Car, query: "-R/./body.glb/-/attach-wheel/car.glb"}
"#;

    fn car(tree: &str) -> RecipeList {
        list(&format!("{tree}{CAR}"))
    }

    fn listing() -> Vec<String> {
        ["body.glb", "tyre.glb", "rubber.mat", "recipes.yaml"]
            .iter()
            .map(|s| s.to_string())
            .collect()
    }

    fn has<F: Fn(&TreeFinding) -> bool>(findings: &[TreeFinding], f: F) -> bool {
        findings.iter().any(f)
    }

    /// AC-1, AC-27: resource segments and Node names are edges; sources are nodes with titles.
    #[test]
    fn graph_lists_resource_node_and_link_edges() {
        let cmr = cmr();
        let graph = TreeGraph::build(&car(""), Some(&listing()), &cmr).expect("graph");
        assert_eq!(graph.children("car.glb"), vec!["body.glb", "wheel.glb"]);
        assert_eq!(graph.children("wheel.glb"), vec!["tyre.glb", "rubber.mat"]);
        assert!(graph.edges.iter().any(|e| e.from == "car.glb"
            && e.to == "./wheel.glb"
            && matches!(&e.kind, RefKind::NodeArg { arg, .. } if arg == "part")));
        assert!(graph
            .edges
            .iter()
            .any(|e| e.from == "car.glb" && e.to == "./body.glb" && e.kind == RefKind::Resource));
        let linked = list(
            "recipes:\n  - {query: \"-R/./a.glb/-/any-~X~-R/./b.glb~E/c.glb\"}\n  - {query: a.glb}\n  - {query: b.glb}\n",
        );
        let graph = TreeGraph::build(&linked, None, &cmr).expect("graph");
        assert!(graph.edges.iter().any(|e| e.to == "./b.glb"
            && matches!(&e.kind, RefKind::Link { arg } if arg == "x")));
    }

    /// AC-27: a source declaration is a node with the recipe's title.
    #[test]
    fn source_declaration_is_a_node_with_title() {
        let graph = TreeGraph::build(&car(""), None, &cmr()).expect("graph");
        let body = &graph.nodes["body.glb"];
        assert_eq!(body.title, "Body");
        assert_eq!(body.name, "body");
        assert!(matches!(body.origin, NodeOrigin::Source(0)));
        assert!(matches!(graph.nodes["car.glb"].origin, NodeOrigin::Recipe(4)));
    }

    /// AC-3: without declarations the roots are the recipes nothing depends on.
    #[test]
    fn default_roots_are_unreferenced_nodes() {
        let report = check_tree(&car(""), Some(&listing()), &cmr());
        assert!(report.errors.is_empty(), "{:?}", report.errors);
        assert_eq!(report.roots, vec!["car.glb"]);
        assert!(report.warnings.is_empty(), "{:?}", report.warnings);
    }

    /// AC-3: a declared root must be a node.
    #[test]
    fn declared_root_must_be_a_node() {
        let report = check_tree(&car("tree: {roots: [car.glb, boat.glb]}\n"), None, &cmr());
        // A display hint only: an unknown root is a warning (design `strict-tree-editing`).
        assert!(has(&report.warnings, |f| matches!(f, TreeFinding::UnknownRoot { root } if root == "boat.glb")));
        assert!(!has(&report.warnings, |f| matches!(f, TreeFinding::UnknownRoot { root } if root == "car.glb")));
        assert!(report.errors.is_empty(), "{:?}", report.errors);
    }

    /// AC-4: a cycle is reported with its nodes in order.
    #[test]
    fn cycle_reported_in_order() {
        let cyclic = list(
            "recipes:\n  - {query: \"-R/./b.glb/-/a.glb\"}\n  - {query: \"-R/./c.glb/-/b.glb\"}\n  - {query: \"-R/./a.glb/-/c.glb\"}\n",
        );
        let report = check_tree(&cyclic, None, &cmr());
        let cycles: Vec<&TreeFinding> = report
            .errors
            .iter()
            .filter(|f| matches!(f, TreeFinding::Cycle { .. }))
            .collect();
        assert_eq!(cycles.len(), 1, "{:?}", report.errors);
        assert_eq!(
            cycles[0],
            &TreeFinding::Cycle {
                nodes: vec!["a.glb".into(), "b.glb".into(), "c.glb".into(), "a.glb".into()]
            }
        );
        assert_eq!(cycles[0].to_error().error_type, crate::error::ErrorType::DependencyCycle);
    }

    const SHARED: &str = r#"
recipes:
  - {query: tyre.glb}
  - {query: "-R/./tyre.glb/-/front.glb"}
  - {query: "-R/./tyre.glb/-/rear.glb"}
  - {query: "-R/./front.glb/-/attach-rear/car.glb"}
"#;

    /// AC-5: under `shape: tree` a node with two parents is refused.
    #[test]
    fn tree_shape_refuses_two_parents() {
        let report = check_tree(&list(&format!("tree: {{shape: tree}}{SHARED}")), None, &cmr());
        assert!(has(&report.errors, |f| matches!(f,
            TreeFinding::SecondParent { node, parents }
                if node == "tyre.glb" && parents == &vec!["front.glb".to_string(), "rear.glb".to_string()])));
    }

    /// AC-5: a DAG allows it.
    #[test]
    fn dag_allows_two_parents() {
        let report = check_tree(&list(SHARED), None, &cmr());
        assert!(report.errors.is_empty(), "{:?}", report.errors);
    }

    /// AC-6: a node reached from no root is a warning.
    #[test]
    fn orphan_is_a_warning() {
        let mut files = listing();
        files.push("spare.glb".into());
        let report = check_tree(&car("tree: {result: car.glb}\n"), Some(&files), &cmr());
        assert!(report.errors.is_empty(), "{:?}", report.errors);
        assert!(has(&report.warnings, |f| matches!(f, TreeFinding::Orphan { node } if node == "spare.glb")));
    }

    /// AC-7: strict refuses absolute, parent and subfolder keys.
    #[test]
    fn strict_refuses_absolute_parent_and_subfolder_keys() {
        let strict = list(
            r#"
tree: {dependencies: strict}
recipes:
  - {query: "-R/data/x.glb/-/a.glb"}
  - {query: "-R/../x.glb/-/b.glb"}
  - {query: "-R/./sub/x.glb/-/c.glb"}
  - {query: "-R/./ok.glb/-/d.glb"}
  - {query: ok.glb}
"#,
        );
        let report = check_tree(&strict, None, &cmr());
        for (node, key) in [("a.glb", "data/x.glb"), ("b.glb", "../x.glb"), ("c.glb", "./sub/x.glb")] {
            assert!(
                has(&report.errors, |f| matches!(f, TreeFinding::StrictKey { node: n, key: k } if n == node && k == key)),
                "{node}: {:?}",
                report.errors
            );
        }
        assert!(!has(&report.errors, |f| matches!(f, TreeFinding::StrictKey { node, .. } if node == "d.glb")));
        let open = check_tree(&list(&strict_yaml_without_tree()), None, &cmr());
        assert!(!has(&open.errors, |f| matches!(f, TreeFinding::StrictKey { .. })));
    }

    fn strict_yaml_without_tree() -> String {
        "recipes:\n  - {query: \"-R/data/x.glb/-/a.glb\"}\n".to_string()
    }

    /// AC-8: a reference to a file that is neither a recipe nor stored is an error.
    #[test]
    fn dangling_reference_is_an_error() {
        let report = check_tree(&car(""), Some(&["body.glb".to_string()]), &cmr());
        assert!(has(&report.errors, |f| matches!(f,
            TreeFinding::MissingSource { node } if node == "tyre.glb")));
        let dangling = list("recipes:\n  - {query: \"-R/./gone.glb/-/a.glb\"}\n");
        let report = check_tree(&dangling, Some(&[]), &cmr());
        assert!(has(&report.errors, |f| matches!(f,
            TreeFinding::Dangling { node, key } if node == "a.glb" && key == "./gone.glb")));
        let report = check_tree(&dangling, None, &cmr());
        assert!(report.errors.is_empty(), "{:?}", report.errors);
        assert!(has(&report.warnings, |f| matches!(f, TreeFinding::Unchecked { .. })));
    }

    /// AC-16: two nodes with one name are refused when one is referenced.
    #[test]
    fn name_collision_refused_when_referenced() {
        let files = vec!["logo.png".to_string(), "logo.jpg".to_string()];
        let used = list("recipes:\n  - {query: \"-R/./logo.png/-/a.txt\"}\n");
        let report = check_tree(&used, Some(&files), &cmr());
        assert!(has(&report.errors, |f| matches!(f,
            TreeFinding::NameCollision { name, filenames }
                if name == "logo" && filenames.len() == 2)));
    }

    /// AC-16: they are allowed while nothing depends on either.
    #[test]
    fn name_collision_allowed_when_unreferenced() {
        let files = vec!["logo.png".to_string(), "logo.jpg".to_string()];
        let report = check_tree(&list("recipes:\n  - {query: a.txt}\n"), Some(&files), &cmr());
        assert!(!has(&report.errors, |f| matches!(f, TreeFinding::NameCollision { .. })));
    }

    /// AC-19: the result is the only root, and must be a node.
    #[test]
    fn result_is_the_only_root() {
        let report = check_tree(&car("tree: {result: wheel.glb}\n"), Some(&listing()), &cmr());
        assert_eq!(report.roots, vec!["wheel.glb"]);
        assert_eq!(report.result.as_deref(), Some("wheel.glb"));
        assert!(has(&report.warnings, |f| matches!(f, TreeFinding::Orphan { node } if node == "car.glb")));
        let report = check_tree(&car("tree: {result: boat.glb}\n"), None, &cmr());
        assert!(has(&report.errors, |f| matches!(f, TreeFinding::ResultNotANode { .. })));
        let report = check_tree(&car("tree: {result: car.glb, roots: [wheel.glb]}\n"), None, &cmr());
        // Roots beside the result are display hints (design `strict-tree-editing`).
        assert!(!has(&report.errors, |f| matches!(f, TreeFinding::RootsBesideResult { .. })));
    }

    /// AC-23: a strict recipe may not pass a link, whatever the argument's type.
    #[test]
    fn strict_refuses_link_argument() {
        let strict = list(
            r#"
tree: {dependencies: strict}
recipes:
  - {query: a.glb}
  - {query: b.glb}
  - {query: "-R/./a.glb/-/any-~X~-R/./b.glb~E/c.glb"}
  - {query: "-R/./a.glb/-/attach-~X~-R/./b.glb~E/d.glb"}
"#,
        );
        let report = check_tree(&strict, None, &cmr());
        for (node, arg) in [("c.glb", "x"), ("d.glb", "part")] {
            assert!(
                has(&report.errors, |f| matches!(f,
                    TreeFinding::StrictLink { node: n, argument } if n == node && argument == arg)),
                "{node}: {:?}",
                report.errors
            );
        }
        let error = report.error().expect("an error");
        assert_eq!(error.error_type, crate::error::ErrorType::ParameterError);
    }

    /// AC-23: a command's default link is trusted.
    #[test]
    fn strict_trusts_default_and_enum_links() {
        let strict = list(
            "tree: {dependencies: strict}\nrecipes:\n  - {query: a.glb}\n  - {query: \"-R/./a.glb/-/with_dir/b.glb\"}\n",
        );
        let report = check_tree(&strict, None, &cmr());
        assert!(report.errors.is_empty(), "{:?}", report.errors);
    }

    /// AC-28: a strict folder declares every file it uses.
    #[test]
    fn strict_requires_declared_sources() {
        let mut stored = list("tree: {dependencies: strict}\nrecipes:\n  - {query: \"-R/./a.glb/-/b.glb\"}\n");
        let files = vec!["a.glb".to_string()];
        let report = check_tree(&stored, Some(&files), &cmr());
        assert!(has(&report.errors, |f| matches!(f,
            TreeFinding::UndeclaredSource { node, file } if node == "b.glb" && file == "a.glb")));
        let report = check_tree(&stored, None, &cmr());
        assert!(has(&report.errors, |f| matches!(f, TreeFinding::UndeclaredSource { .. })));
        stored.recipes.push(crate::recipes::Recipe::new("a.glb".into(), "A".into(), String::new()).expect("recipe"));
        let report = check_tree(&stored, Some(&files), &cmr());
        assert!(report.errors.is_empty(), "{:?}", report.errors);
    }

    /// AC-18: node choices list the nodes of one extension with their titles.
    #[test]
    fn node_choices_and_suggested_filenames() {
        let mut files = listing();
        files.push("spare.glb".into());
        let editor = TreeEditor::new(car(""), Some(files), Arc::new(cmr())).expect("editor");
        let choices = editor.node_choices("glb");
        let names: Vec<(&str, &str)> = choices
            .iter()
            .map(|c| (c.name.as_str(), c.title.as_str()))
            .collect();
        assert_eq!(
            names,
            vec![
                ("body", "Body"),
                ("car", "Car"),
                ("spare", ""),
                ("tyre", "Tyre"),
                ("wheel", "Wheel")
            ]
        );
        assert_eq!(editor.node_choices("mat").len(), 1);
        assert_eq!(editor.suggest_filename("Front Wheel!", "glb"), "front_wheel.glb");
        assert_eq!(editor.suggest_filename("Wheel", "glb"), "wheel1.glb");
        assert_eq!(
            editor.result_key(&crate::parse::parse_key("a").expect("key")),
            None
        );
    }

    /// AC-2: the `tree:` section round-trips; defaults are not written.
    #[test]
    fn tree_section_round_trips() {
        let yaml = "tree:\n  result: car.glb\n  dependencies: strict\nrecipes: []\n";
        let parsed = list(yaml);
        let declaration = parsed.tree.clone().expect("tree");
        assert_eq!(declaration.shape, TreeShape::Dag);
        let written = serde_yaml::to_string(&parsed).expect("yaml");
        assert_eq!(list(&written).tree, Some(declaration));
        assert!(!written.contains("shape"), "{written}");
        assert!(serde_yaml::to_string(&list("recipes: []\n")).expect("yaml").find("tree").is_none());
    }

    fn car_editor() -> TreeEditor {
        TreeEditor::new(car(""), Some(listing()), Arc::new(cmr())).expect("editor")
    }

    /// recipes-editor AC-7: a new recipe is appended; a used filename is refused.
    #[test]
    fn add_appends_and_refuses_a_used_name() {
        let mut editor = car_editor();
        let boat = Recipe::new("-R/./body.glb/-/boat.glb".into(), "Boat".into(), String::new()).expect("recipe");
        editor.add(boat.clone()).expect("added");
        assert_eq!(editor.recipes().recipes.last().map(|r| r.title.as_str()), Some("Boat"));
        assert!(editor.add(boat).is_err());
        let no_name = Recipe::new("-R/./body.glb/-/attach-wheel".into(), String::new(), String::new()).expect("recipe");
        assert!(editor.add(no_name).is_err());
        // A new recipe is a source declaration until its query is written: it is added, and
        // the check reports it.
        let fresh = Recipe::new("note.txt".into(), "Note".into(), String::new()).expect("recipe");
        editor.add(fresh).expect("a new, unstored source is added");
        assert!(editor
            .report()
            .errors
            .iter()
            .any(|f| matches!(f, TreeFinding::MissingSource { node } if node == "note.txt")));
    }

    /// recipes-editor AC-9: a copy under a new name, with the original's references.
    #[test]
    fn duplicate_copies_under_a_new_name() {
        let mut editor = car_editor();
        editor.duplicate("wheel", "spare").expect("duplicated");
        let spare = editor.recipes().get("spare.glb").expect("spare").clone();
        assert_eq!(spare.query, "-R/./tyre.glb/-/material-rubber/spare.glb");
        assert!(editor.recipes().get("wheel.glb").is_some());
        editor.duplicate("wheel", "spare").expect("made unique");
        assert!(editor.recipes().get("spare1.glb").is_some());
    }

    /// recipes-editor AC-5, AC-14: a recipe is replaced in place; one adding a cycle is refused.
    #[test]
    fn set_recipe_refuses_a_new_cycle() {
        let mut editor = car_editor();
        let mut wheel = editor.recipes().get("wheel.glb").expect("wheel").clone();
        wheel.title = "Rim".into();
        editor.set_recipe("wheel.glb", wheel).expect("set");
        assert_eq!(editor.recipes().get("wheel.glb").expect("wheel").title, "Rim");
        let before = editor.recipes().clone();
        let cyclic = Recipe::new("-R/./car.glb/-/material-rubber/tyre.glb".into(), "Tyre".into(), String::new()).expect("recipe");
        assert!(editor.set_recipe("tyre.glb", cyclic).is_err());
        assert_eq!(editor.recipes(), &before);
        let renamed = Recipe::new("rim.glb".into(), String::new(), String::new()).expect("recipe");
        assert!(editor.set_recipe("tyre.glb", renamed).is_err());
    }

    /// recipes-editor AC-3: the result is declared, creating the `tree:` section.
    #[test]
    fn set_result_declares_the_result() {
        let mut editor = car_editor();
        editor.set_result(Some("car")).expect("result");
        assert_eq!(editor.recipes().tree.as_ref().and_then(|t| t.result.clone()).as_deref(), Some("car.glb"));
        // A tree with a node always has a result: clearing it picks a found root again (design
        // `strict-tree-editing`, AC-16).
        editor.set_result(None).expect("cleared");
        assert_eq!(editor.recipes().tree.as_ref().and_then(|t| t.result.clone()).as_deref(), Some("car.glb"));
    }

    /// recipes-editor AC-12: a parsed list replaces the recipes even with findings.
    #[test]
    fn replace_takes_any_parsed_list() {
        let mut editor = car_editor();
        let dangling = list("recipes:\n  - {query: \"-R/./nothing.glb/-/attach-wheel/x.glb\"}\n");
        editor.replace(dangling.clone()).expect("replaced");
        assert_eq!(editor.recipes(), &dangling);
    }

    /// recipes-editor AC-7: the Node arguments a child can be named in.
    #[test]
    fn child_slots_list_node_arguments() {
        let editor = car_editor();
        let slots = editor.child_slots("car").expect("slots");
        assert_eq!(slots.len(), 1);
        assert_eq!((slots[0].action, slots[0].arg.as_str(), slots[0].extension.as_str()), (0, "part", "glb"));
        assert!(editor.child_slots("body").expect("slots").is_empty());
    }
}
