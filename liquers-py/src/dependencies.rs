use std::{
    collections::hash_map::DefaultHasher,
    hash::{Hash, Hasher},
};

use pyo3::prelude::*;

use crate::metadata::DependencyKey;

#[pyclass]
#[derive(Clone, Debug)]
pub struct DependencyRelation(pub liquers_core::dependencies::DependencyRelation);

#[pymethods]
impl DependencyRelation {
    #[staticmethod]
    pub fn state_argument() -> Self {
        Self(liquers_core::dependencies::DependencyRelation::StateArgument)
    }

    #[staticmethod]
    pub fn parameter_link(name: String) -> Self {
        Self(liquers_core::dependencies::DependencyRelation::ParameterLink(name))
    }

    #[staticmethod]
    pub fn default_link(name: String) -> Self {
        Self(liquers_core::dependencies::DependencyRelation::DefaultLink(
            name,
        ))
    }

    #[staticmethod]
    pub fn recipe_link(name: String) -> Self {
        Self(liquers_core::dependencies::DependencyRelation::RecipeLink(
            name,
        ))
    }

    #[staticmethod]
    pub fn override_link(name: String) -> Self {
        Self(liquers_core::dependencies::DependencyRelation::OverrideLink(name))
    }

    #[staticmethod]
    pub fn enum_link(name: String) -> Self {
        Self(liquers_core::dependencies::DependencyRelation::EnumLink(
            name,
        ))
    }

    #[staticmethod]
    pub fn context_evaluate(query: String) -> Self {
        Self(liquers_core::dependencies::DependencyRelation::ContextEvaluate(query))
    }

    #[staticmethod]
    pub fn command_metadata() -> Self {
        Self(liquers_core::dependencies::DependencyRelation::CommandMetadata)
    }

    #[staticmethod]
    pub fn command_implementation() -> Self {
        Self(liquers_core::dependencies::DependencyRelation::CommandImplementation)
    }

    #[staticmethod]
    pub fn recipe() -> Self {
        Self(liquers_core::dependencies::DependencyRelation::Recipe)
    }

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

#[pyclass]
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

    #[getter]
    pub fn key(&self) -> DependencyKey {
        DependencyKey {
            inner: self.0.key.clone(),
        }
    }

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
