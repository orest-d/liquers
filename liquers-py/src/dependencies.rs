use std::{
    collections::hash_map::DefaultHasher,
    hash::{Hash, Hasher},
};

use pyo3::prelude::*;

use crate::metadata::DependencyKey;

/// How an asset depends on another: through its input state, an argument link, a recipe, ...
///
/// Made with the static constructors; `name` is the argument the link fills. Relations compare and
/// hash by value.
#[pyclass(from_py_object, module = "liquers")]
#[derive(Clone, Debug)]
pub struct DependencyRelation(pub liquers_core::dependencies::DependencyRelation);

#[pymethods]
impl DependencyRelation {
    /// The dependency is the input state the action receives.
    #[staticmethod]
    pub fn state_argument() -> Self {
        Self(liquers_core::dependencies::DependencyRelation::StateArgument)
    }

    /// The argument `name` is a link (a nested query) written in the query.
    #[staticmethod]
    pub fn parameter_link(name: String) -> Self {
        Self(liquers_core::dependencies::DependencyRelation::ParameterLink(name))
    }

    /// The argument `name` was not given, and its default is a link.
    #[staticmethod]
    pub fn default_link(name: String) -> Self {
        Self(liquers_core::dependencies::DependencyRelation::DefaultLink(
            name,
        ))
    }

    /// The argument `name` is filled by a link from the recipe.
    #[staticmethod]
    pub fn recipe_link(name: String) -> Self {
        Self(liquers_core::dependencies::DependencyRelation::RecipeLink(
            name,
        ))
    }

    /// The argument `name` is filled by an override link.
    #[staticmethod]
    pub fn override_link(name: String) -> Self {
        Self(liquers_core::dependencies::DependencyRelation::OverrideLink(name))
    }

    /// The argument `name` is an enum alternative that maps to a link.
    #[staticmethod]
    pub fn enum_link(name: String) -> Self {
        Self(liquers_core::dependencies::DependencyRelation::EnumLink(
            name,
        ))
    }

    /// The command evaluated `query` itself, through `Context.evaluate`.
    #[staticmethod]
    pub fn context_evaluate(query: String) -> Self {
        Self(liquers_core::dependencies::DependencyRelation::ContextEvaluate(query))
    }

    /// The dependency is the command's registered metadata.
    #[staticmethod]
    pub fn command_metadata() -> Self {
        Self(liquers_core::dependencies::DependencyRelation::CommandMetadata)
    }

    /// The dependency is the command's implementation (its version).
    #[staticmethod]
    pub fn command_implementation() -> Self {
        Self(liquers_core::dependencies::DependencyRelation::CommandImplementation)
    }

    /// The dependency is the recipe itself, apart from the asset's data.
    #[staticmethod]
    pub fn recipe() -> Self {
        Self(liquers_core::dependencies::DependencyRelation::Recipe)
    }

    /// The relation as text, for example `ParameterLink("name")`.
    pub fn encode(&self) -> String {
        format!("{:?}", self.0)
    }

    pub fn __repr__(&self) -> String {
        format!("DependencyRelation('{:?}')", self.0)
    }

    pub fn __str__(&self) -> String {
        format!("{:?}", self.0)
    }

    pub fn __eq__(&self, other: &Self) -> bool {
        self.0 == other.0
    }

    pub fn __ne__(&self, other: &Self) -> bool {
        self.0 != other.0
    }

    pub fn __hash__(&self) -> u64 {
        let mut hasher = DefaultHasher::new();
        self.0.hash(&mut hasher);
        hasher.finish()
    }
}

/// One dependency of a plan: what is depended on (`key`) and how (`relation`).
///
/// `PlanDependency(key, relation)` takes a `DependencyKey` and a `DependencyRelation`.
#[pyclass(from_py_object, module = "liquers")]
#[derive(Clone, Debug)]
pub struct PlanDependency(pub liquers_core::dependencies::PlanDependency);

#[pymethods]
impl PlanDependency {
    #[new]
    pub fn new(key: &DependencyKey, relation: &DependencyRelation) -> Self {
        Self(liquers_core::dependencies::PlanDependency::new(
            key.inner.clone(),
            relation.0.clone(),
        ))
    }

    /// What the plan depends on, as a `DependencyKey`.
    #[getter]
    pub fn key(&self) -> DependencyKey {
        DependencyKey {
            inner: self.0.key.clone(),
        }
    }

    /// How the plan depends on it, as a `DependencyRelation`.
    #[getter]
    pub fn relation(&self) -> DependencyRelation {
        DependencyRelation(self.0.relation.clone())
    }

    pub fn __repr__(&self) -> String {
        format!("{:?}", self.0)
    }

    pub fn __str__(&self) -> String {
        format!("{:?}", self.0)
    }

    pub fn __eq__(&self, other: &Self) -> bool {
        self.0 == other.0
    }

    pub fn __ne__(&self, other: &Self) -> bool {
        self.0 != other.0
    }

    pub fn __hash__(&self) -> u64 {
        let mut hasher = DefaultHasher::new();
        self.0.hash(&mut hasher);
        hasher.finish()
    }
}
