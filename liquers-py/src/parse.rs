use std::{collections::hash_map::DefaultHasher, hash::{Hash, Hasher}};

use pyo3::prelude::*;


#[pyclass]
#[derive(Clone)]
pub struct Position(liquers_core::query::Position);

#[pymethods]
impl Position {
    #[new]
    pub fn new(offset: usize, line: u32, column: usize) -> Self {
        Position(liquers_core::query::Position {
            offset,
            line,
            column,
        })
    }

    #[staticmethod]
    pub fn unknown() -> Self {
        Position(liquers_core::query::Position::unknown())
    }

    #[getter]
    pub fn offset(&self) -> PyResult<usize> {
        Ok(self.0.offset)
    }

    #[getter]
    pub fn line(&self) -> u32 {
        self.0.line
    }

    #[getter]
    pub fn column(&self) -> usize {
        self.0.column
    }

    pub fn __repr__(&self) -> String {
        format!(
            "Position(offset={}, line={}, column={})",
            self.0.offset, self.0.line, self.0.column
        )
    }

    pub fn __str__(&self) -> String {
        self.0.to_string()
    }
}

#[pyclass]
#[derive(Clone)]
pub struct ActionParameter(liquers_core::query::ActionParameter);

#[pymethods]
impl ActionParameter {
    #[new]
    pub fn new(parameter: String, position: &Position) -> Self {
        ActionParameter(
            liquers_core::query::ActionParameter::new_string(parameter)
                .with_position(position.0.clone()),
        )
    }

    #[getter]
    pub fn position(&self) -> Position {
        Position(self.0.position().clone())
    }

    pub fn encode(&self) -> String {
        self.0.encode()
    }

    pub fn __repr__(&self) -> String {
        format!("{:?}", self.0)
    }

    pub fn __str__(&self) -> String {
        self.0.to_string()
    }
}

#[pyclass]
#[derive(Clone)]
pub struct ResourceName(liquers_core::query::ResourceName);

#[pymethods]
impl ResourceName {
    #[new]
    pub fn new(name: String, position: &Position) -> Self {
        ResourceName(liquers_core::query::ResourceName::new(name).with_position(position.0.clone()))
    }

    #[getter]
    pub fn position(&self) -> Position {
        Position(self.0.position.clone())
    }

    pub fn encode(&self) -> &str {
        self.0.encode()
    }

    pub fn __repr__(&self) -> String {
        format!("{:?}", self.0)
    }

    pub fn __str__(&self) -> String {
        self.0.encode().to_string()
    }
}

#[pyclass]
#[derive(Clone)]
pub struct ActionRequest(liquers_core::query::ActionRequest);

#[pymethods]
impl ActionRequest {
    #[new]
    pub fn new(name: &str) -> Self {
        ActionRequest(liquers_core::query::ActionRequest::new(name.to_owned()))
    }

    #[staticmethod]
    pub fn from_arguments(name: &str) -> Self {
        ActionRequest(liquers_core::query::ActionRequest::new(name.to_owned()))
    }

    #[getter]
    pub fn name(&self) -> String {
        self.0.name.to_string()
    }

    pub fn encode(&self) -> String {
        self.0.encode()
    }

    pub fn to_list(&self) -> Vec<String> {
        let mut result = vec![self.0.name.to_string()];
        for parameter in &self.0.parameters {
            match parameter {
                liquers_core::query::ActionParameter::String(s, _) => result.push(s.to_string()),
                liquers_core::query::ActionParameter::Link(q, _) => result.push(q.encode()),
            }
        }
        result
    }

    pub fn __repr__(&self) -> String {
        format!("{:?}", self.0)
    }

    pub fn __str__(&self) -> String {
        self.0.encode()
    }
}

#[pyclass]
#[derive(Clone)]
pub struct SegmentHeader(liquers_core::query::SegmentHeader);

#[pymethods]
impl SegmentHeader {
    #[new]
    pub fn new() -> Self {
        SegmentHeader(liquers_core::query::SegmentHeader::new())
    }
    #[getter]
    pub fn name(&self) -> String {
        self.0.name.to_string()
    }

    #[getter]
    pub fn position(&self) -> Position {
        Position(self.0.position.clone())
    }

    #[getter]
    pub fn level(&self) -> usize {
        self.0.level
    }
    /*
        #[getter]
        fn parameters(&self) -> Vec<ActionParameter> {
            self.0.parameters.iter().map(|p| HeaderParameter(p.clone())).collect()
        }
    */

    #[getter]
    pub fn resource(&self) -> bool {
        self.0.resource
    }

    pub fn is_trivial(&self) -> bool {
        self.0.is_trivial()
    }

    pub fn encode(&self) -> String {
        self.0.encode()
    }

    pub fn __repr__(&self) -> String {
        format!("{:?}", self.0)
    }

    pub fn __str__(&self) -> String {
        self.0.encode()
    }
}

#[pyclass]
#[derive(Clone)]
pub struct TransformQuerySegment(liquers_core::query::TransformQuerySegment);

#[pymethods]
impl TransformQuerySegment {
    #[new]
    pub fn new() -> Self {
        TransformQuerySegment(liquers_core::query::TransformQuerySegment::default())
    }

    #[getter]
    pub fn header(&self) -> Option<SegmentHeader> {
        match &self.0.header {
            Some(h) => Some(SegmentHeader(h.clone())),
            None => None,
        }
    }

    #[getter]
    pub fn query(&self) -> Vec<ActionRequest> {
        self.0
            .query
            .iter()
            .map(|q| ActionRequest(q.clone()))
            .collect()
    }

    #[getter]
    pub fn filename(&self) -> Option<String> {
        self.0.filename.as_ref().map(|s| s.to_string())
    }

    pub fn predecessor(&self) -> (Option<TransformQuerySegment>, Option<TransformQuerySegment>) {
        let (p, r) = self.0.predecessor();
        (
            p.map(|s| TransformQuerySegment(s)),
            r.map(|s| TransformQuerySegment(s)),
        )
    }

    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    pub fn is_filename(&self) -> bool {
        self.0.is_filename()
    }

    pub fn is_action_request(&self) -> bool {
        self.0.is_action_request()
    }

    pub fn action(&self) -> Option<ActionRequest> {
        self.0.action().map(|a| ActionRequest(a))
    }

    pub fn encode(&self) -> String {
        self.0.encode()
    }

    pub fn __repr__(&self) -> String {
        format!("{:?}", self.0)
    }

    pub fn __str__(&self) -> String {
        self.0.encode()
    }
}

#[pyclass]
#[derive(Clone)]
pub struct Key(pub liquers_core::query::Key);

#[pymethods]
impl Key {
    #[new]
    pub fn new(key: &str) -> Self {
        Key(liquers_core::parse::parse_key(key).unwrap())
    }

    pub fn encode(&self) -> String {
        self.0.encode()
    }

    pub fn to_absolute(&self, cwd_key:&Key) -> Key {
        Key(self.0.to_absolute(&cwd_key.0))
    }

    pub fn parent(&self) -> Key {
        Key(self.0.parent())
    }   

    pub fn __len__(&self) -> usize {
        self.0.len()
    }

    pub fn __getitem__(&self, index: isize) -> ResourceName {
        ResourceName(self.0 .0.get(index as usize).unwrap().clone())
    }

    pub fn __repr__(&self) -> String {
        format!("{:?}", self.0)
    }

    pub fn __str__(&self) -> String {
        self.0.encode()
    }

    pub fn __eq__(&self, other: &Key) -> bool {
        self.0 == other.0
    }

    pub fn __ne__(&self, other: &Key) -> bool {
        self.0 != other.0
    }

    pub fn __hash__(&self) -> u64 {
        let mut hasher = DefaultHasher::new();
        self.0.hash(&mut hasher);
        hasher.finish()
    }

    /// Return the last element of the key if present, None otherwise.
    /// This is typically interpreted as a filename in a Store object.
    pub fn filename(&self) -> Option<String> {
        self.0.filename().map(|s| s.to_string())
    }

    pub fn to_string(&self) -> String {
        self.0.to_string()
    }
}

#[pyclass]
#[derive(Clone)]
pub struct ResourceQuerySegment(liquers_core::query::ResourceQuerySegment);

#[pymethods]
impl ResourceQuerySegment {
    #[new]
    pub fn new() -> Self {
        ResourceQuerySegment(liquers_core::query::ResourceQuerySegment::default())
    }

    #[getter]
    pub fn header(&self) -> Option<SegmentHeader> {
        match &self.0.header {
            Some(h) => Some(SegmentHeader(h.clone())),
            None => None,
        }
    }

    pub fn segment_name(&self) -> String {
        match self.0.header {
            Some(ref h) => h.name.to_string(),
            None => "".to_string(),
        }
    }

    #[getter]
    pub fn key(&self) -> Key {
        Key(self.0.key.clone())
    }

    pub fn encode(&self) -> String {
        self.0.encode()
    }

    pub fn to_absolute(&self, cwd_key:&Key) -> ResourceQuerySegment {
        ResourceQuerySegment(self.0.to_absolute(&cwd_key.0))
    }

    pub fn __repr__(&self) -> String {
        format!("{:?}", self.0)
    }

    pub fn __str__(&self) -> String {
        self.0.encode()
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
#[derive(Clone)]
pub struct QuerySegment(liquers_core::query::QuerySegment);

#[pymethods]
impl QuerySegment {
    #[new]
    pub fn new() -> Self {
        QuerySegment(liquers_core::query::QuerySegment::default())
    }

    #[getter]
    pub fn filename(&self) -> Option<String> {
        self.0.filename().map(|s| s.to_string())
    }

    #[getter]
    pub fn header(&self) -> Option<SegmentHeader> {
        match &self.0 {
            liquers_core::query::QuerySegment::Transform(t) => {
                t.header.as_ref().map(|h| SegmentHeader(h.clone()))
            }
            liquers_core::query::QuerySegment::Resource(r) => {
                r.header.as_ref().map(|h| SegmentHeader(h.clone()))
            }
        }
    }

    pub fn is_resource_query_segment(&self) -> bool {
        self.0.is_resource_query_segment()
    }

    pub fn is_transform_query_segment(&self) -> bool {
        self.0.is_transform_query_segment()
    }

    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    pub fn is_filename(&self) -> bool {
        self.0.is_filename()
    }

    pub fn is_action_request(&self) -> bool {
        self.0.is_action_request()
    }

    pub fn encode(&self) -> String {
        self.0.encode()
    }

    pub fn to_absolute(&self, cwd_key:&Key) -> QuerySegment {
        QuerySegment(self.0.to_absolute(&cwd_key.0))
    }

    pub fn __repr__(&self) -> String {
        format!("{:?}", self.0)
    }

    pub fn __str__(&self) -> String {
        self.0.encode()
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
#[derive(Clone)]
pub struct Query(pub liquers_core::query::Query);

#[pymethods]
impl Query {
    #[new]
    pub fn new() -> Self {
        Query(liquers_core::query::Query::default())
    }

    #[getter]
    pub fn absolute(&self) -> bool {
        self.0.absolute
    }

    #[getter]
    pub fn segments(&self) -> Vec<QuerySegment> {
        self.0
            .segments
            .iter()
            .map(|s| QuerySegment(s.clone()))
            .collect()
    }

    pub fn filename(&self) -> Option<String> {
        self.0.filename().map(|s| s.to_string())
    }

    pub fn without_filename(&self) -> Query {
        Query(self.0.clone().without_filename())
    }

    pub fn extension(&self) -> Option<String> {
        self.0.extension()
    }

    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    pub fn is_transform_query(&self) -> bool {
        self.0.is_transform_query()
    }

    pub fn transform_query(&self) -> Option<TransformQuerySegment> {
        self.0.transform_query().map(|s| TransformQuerySegment(s))
    }

    pub fn is_resource_query(&self) -> bool {
        self.0.is_resource_query()
    }

    pub fn resource_query(&self) -> Option<ResourceQuerySegment> {
        self.0.resource_query().map(|s| ResourceQuerySegment(s))
    }

    pub fn is_action_request(&self) -> bool {
        self.0.is_action_request()
    }

    pub fn action(&self) -> Option<ActionRequest> {
        if self.0.is_action_request() {
            self.0.action().map(|a| ActionRequest(a))
        } else {
            None
        }
    }

    pub fn predecessor(&self) -> (Option<Query>, Option<QuerySegment>) {
        let (p, r) = self.0.predecessor();
        (p.map(|s| Query(s)), r.map(|s| QuerySegment(s)))
    }

    pub fn all_predecessors(&self) -> Vec<(Option<Query>, Option<QuerySegment>)> {
        self.0
            .all_predecessors()
            .into_iter()
            .map(|(p, r)| (p.map(|s| Query(s)), r.map(|s| QuerySegment(s))))
            .collect()
    }

    //#[args(n = 30)]
    pub fn short(&self, n: usize) -> String {
        self.0.short(n)
    }

    pub fn encode(&self) -> String {
        self.0.encode()
    }

    pub fn to_absolute(&self, cwd_key:&Key) -> Query {
        Query(self.0.to_absolute(&cwd_key.0))
    }

    pub fn __repr__(&self) -> String {
        format!("{:?}", self.0)
    }

    pub fn __str__(&self) -> String {
        self.0.encode()
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

#[pyfunction]
pub fn parse(query: &str) -> PyResult<Query> {
    match liquers_core::parse::parse_query(query) {
        Ok(q) => Ok(Query(q)),
        Err(e) => Err(PyErr::new::<pyo3::exceptions::PyException, _>(
            e.to_string(),
        )),
    }
}

#[pyfunction]
pub fn parse_key(key: &str) -> PyResult<Key> {
    match liquers_core::parse::parse_key(key) {
        Ok(k) => Ok(Key(k)),
        Err(e) => Err(PyErr::new::<pyo3::exceptions::PyException, _>(
            e.to_string(),
        )),
    }
}

