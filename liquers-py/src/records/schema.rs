//! The schema classes: `lq.FieldType`, `lq.KeyRole`, `lq.FieldSchema`, `lq.RecordSchema` and
//! `lq.ChunkOrigin`.
//!
//! Each wraps the `liquers-records` type of the same name and converts losslessly to and from it.
//! `FieldSchema` takes its type first and its name from the `fields=` key it is given under, so a
//! declaration reads `fields={"id": lq.FieldSchema("text", key="id")}`. Its `key` is the field's
//! structural role (Rust's `FieldSchema::key`, as in `liquers.schema` JSON); Rust's `role`, the
//! index intent, is not exposed. `KeyRole.Data` is Rust's `KeyRole::None`, since `None` is a
//! Python keyword. See `specs/design/liquers-py-records/phase2-architecture.md` §Classes.

use std::sync::Arc;

use liquers_core::error::Error;
use liquers_records::{
    ChunkOrigin as CoreChunkOrigin, FieldSchema as CoreFieldSchema, FieldType as CoreFieldType,
    KeyRole as CoreKeyRole, RecordSchema as CoreRecordSchema,
};
use pyo3::prelude::*;
use pyo3::types::{PyList, PyString};

use crate::error::error_to_pyerr;

/// The type of a record field.
#[pyclass(from_py_object, eq, eq_int, frozen, hash, module = "liquers")]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum FieldType {
    Bool,
    Int,
    UInt,
    Float,
    Text,
    Binary,
    Date,
    Timestamp,
    Vector,
}

impl FieldType {
    pub fn from_core(data_type: CoreFieldType) -> Self {
        match data_type {
            CoreFieldType::Bool => FieldType::Bool,
            CoreFieldType::Int => FieldType::Int,
            CoreFieldType::UInt => FieldType::UInt,
            CoreFieldType::Float => FieldType::Float,
            CoreFieldType::Text => FieldType::Text,
            CoreFieldType::Binary => FieldType::Binary,
            CoreFieldType::Date => FieldType::Date,
            CoreFieldType::Timestamp => FieldType::Timestamp,
            CoreFieldType::Vector => FieldType::Vector,
        }
    }

    pub fn to_core(self) -> CoreFieldType {
        match self {
            FieldType::Bool => CoreFieldType::Bool,
            FieldType::Int => CoreFieldType::Int,
            FieldType::UInt => CoreFieldType::UInt,
            FieldType::Float => CoreFieldType::Float,
            FieldType::Text => CoreFieldType::Text,
            FieldType::Binary => CoreFieldType::Binary,
            FieldType::Date => CoreFieldType::Date,
            FieldType::Timestamp => CoreFieldType::Timestamp,
            FieldType::Vector => CoreFieldType::Vector,
        }
    }

    /// The lowercase name a declaration may use instead of the enum member.
    pub fn name(self) -> &'static str {
        match self {
            FieldType::Bool => "bool",
            FieldType::Int => "int",
            FieldType::UInt => "uint",
            FieldType::Float => "float",
            FieldType::Text => "text",
            FieldType::Binary => "binary",
            FieldType::Date => "date",
            FieldType::Timestamp => "timestamp",
            FieldType::Vector => "vector",
        }
    }

    fn parse(name: &str) -> Result<Self, Error> {
        let all = [
            FieldType::Bool,
            FieldType::Int,
            FieldType::UInt,
            FieldType::Float,
            FieldType::Text,
            FieldType::Binary,
            FieldType::Date,
            FieldType::Timestamp,
            FieldType::Vector,
        ];
        all.into_iter().find(|t| t.name() == name).ok_or_else(|| {
            Error::conversion_error_with_message(
                name,
                "FieldType",
                "use bool, int, uint, float, text, binary, date, timestamp or vector",
            )
        })
    }

    /// A `FieldType` or its lowercase name.
    pub fn extract(object: &Bound<'_, PyAny>) -> Result<Self, Error> {
        if let Ok(value) = object.extract::<FieldType>() {
            return Ok(value);
        }
        if let Ok(text) = object.cast::<PyString>() {
            return FieldType::parse(&text.to_string());
        }
        Err(Error::conversion_error_with_message(
            type_name(object),
            "FieldType",
            "a field type is an lq.FieldType or its lowercase name",
        ))
    }
}

#[pymethods]
impl FieldType {
    fn __str__(&self) -> &'static str {
        self.name()
    }
}

/// The structural role of a field: the row's identity (`Id`), the index into the batch's origins
/// (`Source`), or ordinary data (`Data`).
#[pyclass(from_py_object, eq, eq_int, frozen, hash, module = "liquers")]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum KeyRole {
    Id,
    Source,
    Data,
}

impl KeyRole {
    pub fn from_core(key: CoreKeyRole) -> Self {
        match key {
            CoreKeyRole::Id => KeyRole::Id,
            CoreKeyRole::Source => KeyRole::Source,
            CoreKeyRole::None => KeyRole::Data,
        }
    }

    pub fn to_core(self) -> CoreKeyRole {
        match self {
            KeyRole::Id => CoreKeyRole::Id,
            KeyRole::Source => CoreKeyRole::Source,
            KeyRole::Data => CoreKeyRole::None,
        }
    }

    fn name(self) -> &'static str {
        match self {
            KeyRole::Id => "id",
            KeyRole::Source => "source",
            KeyRole::Data => "data",
        }
    }

    /// A `KeyRole` or `"id"`, `"source"` or `"data"`.
    fn extract(object: &Bound<'_, PyAny>) -> Result<Self, Error> {
        if let Ok(value) = object.extract::<KeyRole>() {
            return Ok(value);
        }
        if let Ok(text) = object.cast::<PyString>() {
            return match text.to_string().as_str() {
                "id" => Ok(KeyRole::Id),
                "source" => Ok(KeyRole::Source),
                "data" => Ok(KeyRole::Data),
                other => Err(Error::conversion_error_with_message(
                    other,
                    "KeyRole",
                    "use id, source or data",
                )),
            };
        }
        Err(Error::conversion_error_with_message(
            type_name(object),
            "KeyRole",
            "a key is an lq.KeyRole or one of id, source, data",
        ))
    }
}

#[pymethods]
impl KeyRole {
    fn __str__(&self) -> &'static str {
        self.name()
    }
}

/// One field of a record schema.
///
/// `FieldSchema(type, *, name=None, key="data", label=None, description="", nullable=True)`.
/// Inside `fields=` the dict key supplies the name; in an `lq.RecordSchema` every field needs one.
/// The label defaults from the name, as `"unit_price"` labels itself `"unit price"`.
#[pyclass(frozen, module = "liquers")]
#[derive(Debug, Clone)]
pub struct FieldSchema {
    pub name: Option<String>,
    pub data_type: FieldType,
    pub key: KeyRole,
    /// `None` until given, so a name supplied later still labels the field.
    pub label: Option<String>,
    pub description: String,
    pub nullable: bool,
    /// Rust-only attributes (index intent, media type) of a field that came from Rust, kept so a
    /// schema read from a view converts back unchanged.
    core: Option<CoreFieldSchema>,
}

impl FieldSchema {
    pub fn from_core(field: &CoreFieldSchema) -> Self {
        FieldSchema {
            name: Some(field.name.clone()),
            data_type: FieldType::from_core(field.data_type),
            key: KeyRole::from_core(field.key),
            label: Some(field.label.clone()),
            description: field.description.clone(),
            nullable: field.nullable,
            core: Some(field.clone()),
        }
    }

    /// The Rust field, named `name` when this one has none.
    pub fn to_core(&self, name: Option<&str>) -> Result<CoreFieldSchema, Error> {
        let name = match (name, &self.name) {
            (Some(given), _) => given.to_string(),
            (None, Some(own)) => own.clone(),
            (None, None) => {
                return Err(Error::general_error(
                    "a FieldSchema in a RecordSchema needs a name".to_string(),
                ))
            }
        };
        let mut field = match &self.core {
            Some(core) => {
                let mut core = core.clone();
                core.name = name.clone();
                core.data_type = self.data_type.to_core();
                core
            }
            None => CoreFieldSchema::new(name.clone(), self.data_type.to_core()),
        };
        field.key = self.key.to_core();
        field.label = match &self.label {
            Some(label) => label.clone(),
            None => name.replace('_', " "),
        };
        field.description = self.description.clone();
        field.nullable = self.nullable;
        Ok(field)
    }

    /// A `FieldSchema`, or a field type given alone.
    pub fn extract_declaration(object: &Bound<'_, PyAny>) -> Result<FieldSchema, Error> {
        if let Ok(field) = object.cast::<FieldSchema>() {
            return Ok(field.get().clone());
        }
        Ok(FieldSchema::bare(FieldType::extract(object).map_err(|_| {
            Error::conversion_error_with_message(
                type_name(object),
                "FieldSchema",
                "a declared field is an lq.FieldSchema, an lq.FieldType or a type name",
            )
        })?))
    }

    fn bare(data_type: FieldType) -> FieldSchema {
        FieldSchema {
            name: None,
            data_type,
            key: KeyRole::Data,
            label: None,
            description: String::new(),
            nullable: true,
            core: None,
        }
    }
}

#[pymethods]
impl FieldSchema {
    #[new]
    #[pyo3(signature = (r#type, *, name=None, key=None, label=None, description="".to_string(), nullable=true))]
    fn new(
        py: Python<'_>,
        r#type: &Bound<'_, PyAny>,
        name: Option<String>,
        key: Option<&Bound<'_, PyAny>>,
        label: Option<String>,
        description: String,
        nullable: bool,
    ) -> PyResult<Self> {
        let built = (|| -> Result<FieldSchema, Error> {
            let mut field = FieldSchema::bare(FieldType::extract(r#type)?);
            field.name = name;
            if let Some(key) = key {
                field.key = KeyRole::extract(key)?;
            }
            field.label = label;
            field.description = description;
            field.nullable = nullable;
            Ok(field)
        })();
        built.map_err(|e| error_to_pyerr(py, &e))
    }

    /// The field's name, `None` until a `fields=` key or a `RecordSchema` names it.
    #[getter]
    fn name(&self) -> Option<String> {
        self.name.clone()
    }

    /// The field's `FieldType`.
    #[getter]
    fn r#type(&self) -> FieldType {
        self.data_type
    }

    /// The field's structural role: `KeyRole.Id`, `Source` or `Data`.
    #[getter]
    fn key(&self) -> KeyRole {
        self.key
    }

    /// The label: given, or derived from the name (underscores become spaces).
    #[getter]
    fn label(&self) -> Option<String> {
        match (&self.label, &self.name) {
            (Some(label), _) => Some(label.clone()),
            (None, Some(name)) => Some(name.replace('_', " ")),
            (None, None) => None,
        }
    }

    /// What the field means; empty when not given.
    #[getter]
    fn description(&self) -> String {
        self.description.clone()
    }

    /// Whether the field may hold nulls.
    #[getter]
    fn nullable(&self) -> bool {
        self.nullable
    }

    fn __eq__(&self, other: &Bound<'_, PyAny>) -> bool {
        match other.cast::<FieldSchema>() {
            Ok(other) => {
                let other = other.get();
                self.name == other.name
                    && self.data_type == other.data_type
                    && self.key == other.key
                    && self.label() == other.label()
                    && self.description == other.description
                    && self.nullable == other.nullable
            }
            Err(_) => false,
        }
    }

    fn __repr__(&self) -> String {
        let mut parts = vec![format!("'{}'", self.data_type.name())];
        if let Some(name) = &self.name {
            parts.push(format!("name='{name}'"));
        }
        if self.key != KeyRole::Data {
            parts.push(format!("key='{}'", self.key.name()));
        }
        if !self.nullable {
            parts.push("nullable=False".to_string());
        }
        format!("FieldSchema({})", parts.join(", "))
    }
}

/// The schema of a record view: its fields in order, and optionally the Liquers type the rows
/// describe. `RecordSchema(fields, *, type_identifier=None)` checks what Rust checks: names are
/// unique, and there is at most one `Id` and one `Source` field.
#[pyclass(frozen, module = "liquers")]
pub struct RecordSchema {
    pub inner: Arc<CoreRecordSchema>,
}

impl RecordSchema {
    pub fn from_core(inner: Arc<CoreRecordSchema>) -> Self {
        RecordSchema { inner }
    }
}

#[pymethods]
impl RecordSchema {
    #[new]
    #[pyo3(signature = (fields, *, type_identifier=None))]
    fn new(py: Python<'_>, fields: &Bound<'_, PyAny>, type_identifier: Option<String>) -> PyResult<Self> {
        let built = (|| -> Result<CoreRecordSchema, Error> {
            let mut core_fields = Vec::new();
            for item in fields.try_iter().map_err(|e| crate::error::pyerr_to_error(py, &e))? {
                let item = item.map_err(|e| crate::error::pyerr_to_error(py, &e))?;
                let field = item.cast::<FieldSchema>().map_err(|_| {
                    Error::conversion_error_with_message(
                        type_name(&item),
                        "FieldSchema",
                        "a RecordSchema is built from lq.FieldSchema objects",
                    )
                })?;
                core_fields.push(field.get().to_core(None)?);
            }
            let mut schema = CoreRecordSchema::new(core_fields)?;
            schema.type_identifier = type_identifier;
            Ok(schema)
        })();
        built
            .map(|schema| RecordSchema { inner: Arc::new(schema) })
            .map_err(|e| error_to_pyerr(py, &e))
    }

    /// The fields, in order.
    #[getter]
    fn fields(&self) -> Vec<FieldSchema> {
        self.inner.fields.iter().map(FieldSchema::from_core).collect()
    }

    /// The field names, in order.
    #[getter]
    fn names(&self) -> Vec<String> {
        self.inner.fields.iter().map(|f| f.name.clone()).collect()
    }

    /// The field called `name`. Raises `KeyError` when there is none.
    fn field(&self, name: &str) -> PyResult<FieldSchema> {
        match self.inner.index_of(name) {
            Some(index) => Ok(FieldSchema::from_core(&self.inner.fields[index])),
            None => Err(pyo3::exceptions::PyKeyError::new_err(name.to_string())),
        }
    }

    /// The Liquers type the rows describe, when there is one.
    #[getter]
    fn type_identifier(&self) -> Option<String> {
        self.inner.type_identifier.clone()
    }

    /// The schema as `liquers.schema` JSON holds it, as Python data.
    fn to_dict(&self, py: Python<'_>) -> PyResult<Py<PyAny>> {
        let json = serde_json::to_value(self.inner.as_ref())
            .map_err(|e| error_to_pyerr(py, &Error::general_error(e.to_string())))?;
        crate::store::json_to_py(py, &json)
    }

    fn __len__(&self) -> usize {
        self.inner.fields.len()
    }

    fn __eq__(&self, other: &Bound<'_, PyAny>) -> bool {
        match other.cast::<RecordSchema>() {
            Ok(other) => self.inner.as_ref() == other.get().inner.as_ref(),
            Err(_) => false,
        }
    }

    fn __repr__(&self, py: Python<'_>) -> String {
        let fields: Vec<String> = self.fields().iter().map(FieldSchema::__repr__).collect();
        let _ = py;
        format!("RecordSchema([{}])", fields.join(", "))
    }
}

/// Where the rows indexed by a `Source` field came from: the asset, and the query that re-produces
/// the rows.
#[pyclass(frozen, module = "liquers")]
pub struct ChunkOrigin {
    pub inner: CoreChunkOrigin,
}

#[pymethods]
impl ChunkOrigin {
    /// The asset the rows were projected from.
    #[getter]
    fn asset(&self) -> crate::query::Query {
        crate::query::Query(self.inner.asset.clone())
    }

    /// The query that re-produces the rows.
    #[getter]
    fn chunk(&self) -> crate::query::Query {
        crate::query::Query(self.inner.chunk.clone())
    }

    /// The origin as Python data, with its `info` and `locator` too.
    fn to_dict(&self, py: Python<'_>) -> PyResult<Py<PyAny>> {
        let json = serde_json::to_value(&self.inner)
            .map_err(|e| error_to_pyerr(py, &Error::general_error(e.to_string())))?;
        crate::store::json_to_py(py, &json)
    }

    fn __eq__(&self, other: &Bound<'_, PyAny>) -> bool {
        match other.cast::<ChunkOrigin>() {
            Ok(other) => self.inner == other.get().inner,
            Err(_) => false,
        }
    }

    fn __repr__(&self) -> String {
        format!("ChunkOrigin(asset='{}', chunk='{}')", self.inner.asset.encode(), self.inner.chunk.encode())
    }
}

/// The qualified name of an object's type, for an error message.
pub(crate) fn type_name(object: &Bound<'_, PyAny>) -> String {
    object
        .get_type()
        .name()
        .map(|n| n.to_string())
        .unwrap_or_else(|_| "?".to_string())
}

/// A list of `ChunkOrigin` objects.
pub(crate) fn origins_to_py(py: Python<'_>, origins: &[CoreChunkOrigin]) -> PyResult<Py<PyList>> {
    let items: PyResult<Vec<Py<ChunkOrigin>>> = origins
        .iter()
        .map(|origin| Py::new(py, ChunkOrigin { inner: origin.clone() }))
        .collect();
    Ok(PyList::new(py, items?)?.unbind())
}
