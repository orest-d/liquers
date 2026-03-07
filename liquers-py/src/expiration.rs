use std::{
    collections::hash_map::DefaultHasher,
    hash::{Hash, Hasher},
    str::FromStr,
};

use pyo3::prelude::*;

use crate::error::Error;

#[pyclass]
#[derive(Clone, Debug)]
pub struct Expires(pub liquers_core::expiration::Expires);

#[pymethods]
impl Expires {
    #[new]
    pub fn new(spec: &str) -> PyResult<Self> {
        liquers_core::expiration::Expires::from_str(spec)
            .map(Expires)
            .map_err(|e| PyErr::from(Error::from(e)))
    }

    #[staticmethod]
    pub fn never() -> Self {
        Expires(liquers_core::expiration::Expires::Never)
    }

    #[staticmethod]
    pub fn immediately() -> Self {
        Expires(liquers_core::expiration::Expires::Immediately)
    }

    pub fn encode(&self) -> String {
        self.0.to_string()
    }

    pub fn __repr__(&self) -> String {
        format!("Expires('{:?}')", self.0)
    }

    pub fn __str__(&self) -> String {
        self.0.to_string()
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
pub struct ExpirationTime(pub liquers_core::expiration::ExpirationTime);

#[pymethods]
impl ExpirationTime {
    #[new]
    pub fn new(spec: &str) -> PyResult<Self> {
        liquers_core::expiration::ExpirationTime::from_str(spec)
            .map(ExpirationTime)
            .map_err(|e| PyErr::from(Error::from(e)))
    }

    #[staticmethod]
    pub fn never() -> Self {
        ExpirationTime(liquers_core::expiration::ExpirationTime::Never)
    }

    #[staticmethod]
    pub fn immediately() -> Self {
        ExpirationTime(liquers_core::expiration::ExpirationTime::Immediately)
    }

    pub fn encode(&self) -> String {
        self.0.to_string()
    }

    pub fn is_never(&self) -> bool {
        self.0.is_never()
    }

    pub fn is_immediately(&self) -> bool {
        self.0.is_immediately()
    }

    pub fn __repr__(&self) -> String {
        format!("ExpirationTime('{:?}')", self.0)
    }

    pub fn __str__(&self) -> String {
        self.0.to_string()
    }

    pub fn __eq__(&self, other: &Self) -> bool {
        self.0 == other.0
    }

    pub fn __ne__(&self, other: &Self) -> bool {
        self.0 != other.0
    }
}
