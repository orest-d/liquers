use std::{
    collections::hash_map::DefaultHasher,
    hash::{Hash, Hasher},
};

use pyo3::prelude::*;
use pyo3::types::PyString;

use crate::error::Error;

/// `text` as Python's `repr` of a `str` writes it; never fails.
pub(crate) fn quoted(py: Python<'_>, text: &str) -> String {
    PyString::new(py, text)
        .repr()
        .map(|r| r.to_string())
        .unwrap_or_else(|_| format!("'{text}'"))
}

/// A place in a query's text: byte offset, line and column.
///
/// Attached to parsed query elements and to errors, so a message can point at the text it is about.
/// `Position.unknown()` is the position of an element that was not parsed from text. See
/// `PYTHON_GUIDE.md` §8.
#[pyclass(from_py_object, module = "liquers")]
#[derive(Clone)]
pub struct Position(pub liquers_core::query::Position);

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

    /// The position of an element not parsed from text (offset, line and column all zero).
    #[staticmethod]
    pub fn unknown() -> Self {
        Position(liquers_core::query::Position::unknown())
    }

    /// Byte offset from the start of the query text, counting from 0.
    #[getter]
    pub fn offset(&self) -> PyResult<usize> {
        Ok(self.0.offset)
    }

    /// Line number, counting from 1; 0 when the position is unknown.
    #[getter]
    pub fn line(&self) -> u32 {
        self.0.line
    }

    /// Column within the line, counting from 1; 0 when the position is unknown.
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

/// One parameter of an action, as written in the query.
///
/// A parameter is either plain text or a link (a nested query). `encode()` gives it back in query
/// syntax.
#[pyclass(from_py_object, module = "liquers")]
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

    /// Where the parameter starts in the query text.
    #[getter]
    pub fn position(&self) -> Position {
        Position(self.0.position().clone())
    }

    /// The parameter in query syntax, escaped so `parse` reads it back unchanged.
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

/// One element of a key: a folder or file name.
///
/// A `Key` is a sequence of resource names; `key[i]` returns one.
#[pyclass(from_py_object, module = "liquers")]
#[derive(Clone)]
pub struct ResourceName(liquers_core::query::ResourceName);

#[pymethods]
impl ResourceName {
    #[new]
    pub fn new(name: String, position: &Position) -> Self {
        ResourceName(liquers_core::query::ResourceName::new(name).with_position(position.0.clone()))
    }

    /// Where the name starts in the query text.
    #[getter]
    pub fn position(&self) -> Position {
        Position(self.0.position.clone())
    }

    /// The name in query syntax.
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

/// One action of a query: a command name and its parameters.
///
/// `ActionRequest(name)` makes an action without parameters; `ActionRequest.from_arguments` adds
/// text parameters. Actions come out of a parsed query through `TransformQuerySegment.query` and
/// `Query.action()`.
#[pyclass(from_py_object, module = "liquers")]
#[derive(Clone)]
pub struct ActionRequest(liquers_core::query::ActionRequest);

#[pymethods]
impl ActionRequest {
    #[new]
    pub fn new(name: &str) -> Self {
        ActionRequest(liquers_core::query::ActionRequest::new(name.to_owned()))
    }

    /// An action named `name` with `arguments` as its text parameters.
    ///
    /// `encode()` escapes whatever the arguments need, so any text survives a round trip through
    /// `parse`.
    #[staticmethod]
    #[pyo3(signature = (name, arguments=Vec::new()))]
    pub fn from_arguments(name: &str, arguments: Vec<String>) -> Self {
        let mut action = liquers_core::query::ActionRequest::new(name.to_owned());
        for argument in arguments {
            action.parameters.push(liquers_core::query::ActionParameter::String(
                argument,
                liquers_core::query::Position::unknown(),
            ));
        }
        ActionRequest(action)
    }

    /// The command name of the action.
    #[getter]
    pub fn name(&self) -> String {
        self.0.name.to_string()
    }

    /// The action in query syntax: the name followed by its parameters, separated by `-`.
    pub fn encode(&self) -> String {
        self.0.encode()
    }

    /// The action as a list of strings: the name, then each parameter.
    ///
    /// A text parameter is given as its text; a link parameter as its query, encoded.
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

/// The optional header at the start of a query segment, such as `-R` for a resource segment.
///
/// It carries the segment's name, its level, and whether the segment is a resource segment. The
/// name of a transform segment's header selects the realm its actions are looked up in.
#[pyclass(from_py_object, module = "liquers")]
#[derive(Clone)]
pub struct SegmentHeader(liquers_core::query::SegmentHeader);

#[pymethods]
impl SegmentHeader {
    #[new]
    pub fn new() -> Self {
        SegmentHeader(liquers_core::query::SegmentHeader::new())
    }
    /// The segment name; empty when the header has none.
    #[getter]
    pub fn name(&self) -> String {
        self.0.name.to_string()
    }

    /// Where the header starts in the query text.
    #[getter]
    pub fn position(&self) -> Position {
        Position(self.0.position.clone())
    }

    /// The number of extra leading hyphens of the header; stored and encoded, not yet interpreted.
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

    /// True for a resource segment header (`-R`), false for a transform segment header.
    #[getter]
    pub fn resource(&self) -> bool {
        self.0.resource
    }

    /// True when the header carries nothing: no name, level 0 and no parameters.
    pub fn is_trivial(&self) -> bool {
        self.0.is_trivial()
    }

    /// The header in query syntax.
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

/// A segment of actions applied to a value, optionally ending in a filename.
///
/// For example `to_text/upper/result.txt`: two actions and the filename `result.txt`.
#[pyclass(from_py_object, module = "liquers")]
#[derive(Clone)]
pub struct TransformQuerySegment(liquers_core::query::TransformQuerySegment);

#[pymethods]
impl TransformQuerySegment {
    #[new]
    pub fn new() -> Self {
        TransformQuerySegment(liquers_core::query::TransformQuerySegment::default())
    }

    /// The segment's header, or `None` when the segment has none.
    #[getter]
    pub fn header(&self) -> Option<SegmentHeader> {
        match &self.0.header {
            Some(h) => Some(SegmentHeader(h.clone())),
            None => None,
        }
    }

    /// The segment's actions, in order.
    #[getter]
    pub fn query(&self) -> Vec<ActionRequest> {
        self.0
            .query
            .iter()
            .map(|q| ActionRequest(q.clone()))
            .collect()
    }

    /// The filename that ends the segment, or `None`.
    #[getter]
    pub fn filename(&self) -> Option<String> {
        self.0.filename.as_ref().map(|s| s.to_string())
    }

    /// Splits off the last step of the segment.
    ///
    /// Returns `(rest, last)`: the segment without its filename and the filename on its own when
    /// there is a filename; otherwise the segment without its last action and that action as a
    /// segment. Either part is `None` when there is nothing there.
    pub fn predecessor(&self) -> (Option<TransformQuerySegment>, Option<TransformQuerySegment>) {
        let (p, r) = self.0.predecessor();
        (
            p.map(|s| TransformQuerySegment(s)),
            r.map(|s| TransformQuerySegment(s)),
        )
    }

    /// True when the segment has no actions and no filename; the header does not count.
    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    /// True when the segment is only a filename, with no actions.
    pub fn is_filename(&self) -> bool {
        self.0.is_filename()
    }

    /// True when the segment is exactly one action and no filename.
    pub fn is_action_request(&self) -> bool {
        self.0.is_action_request()
    }

    /// The action when the segment is a single action request (see `is_action_request`), else
    /// `None`.
    pub fn action(&self) -> Option<ActionRequest> {
        self.0.action().map(|a| ActionRequest(a))
    }

    /// The segment in query syntax.
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

/// A path in a store, such as `data/report.txt`: a sequence of resource names.
///
/// `Key("a/b")` parses the text and raises `ParseError` when it is not a valid key. `len(key)` is
/// the number of names and `key[i]` returns one as a `ResourceName`; a negative index counts from
/// the end. Keys compare and hash by value.
#[pyclass(from_py_object, module = "liquers")]
#[derive(Clone, Debug)]
pub struct Key(pub liquers_core::query::Key);

#[pymethods]
impl Key {
    #[new]
    pub fn new(key: &str) -> PyResult<Self> {
        liquers_core::parse::parse_key(key)
            .map(Key)
            .map_err(|e| PyErr::from(Error::from(e)))
    }

    /// The key as text, in the form `parse_key` reads back.
    pub fn encode(&self) -> String {
        self.0.encode()
    }

    /// The key with its `.` and `..` elements resolved against `cwd_key`.
    ///
    /// `cwd_key` should itself be absolute (no `.` or `..`); that is not checked.
    pub fn to_absolute(&self, cwd_key: &Key) -> Key {
        Key(self.0.to_absolute(&cwd_key.0))
    }

    /// The key without its last element.
    pub fn parent(&self) -> Key {
        Key(self.0.parent())
    }

    pub fn __len__(&self) -> usize {
        self.0.len()
    }

    /// The resource name at `index`; a negative index counts from the end.
    pub fn __getitem__(&self, index: isize) -> PyResult<ResourceName> {
        let len = self.0.len() as isize;
        let position = if index < 0 { index + len } else { index };
        if position < 0 || position >= len {
            return Err(pyo3::exceptions::PyIndexError::new_err("key index out of range"));
        }
        Ok(ResourceName(self.0 .0[position as usize].clone()))
    }

    pub fn __repr__(&self, py: Python<'_>) -> String {
        format!("Key({})", quoted(py, &self.0.encode()))
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

    /// The last element of the key, or `None` for an empty key.
    ///
    /// A store reads it as a file name.
    pub fn filename(&self) -> Option<String> {
        self.0.filename().map(|s| s.to_string())
    }

    /// The key as text.
    pub fn to_string(&self) -> String {
        self.0.to_string()
    }
}

/// A segment that names a resource in a store, such as `-R/data/report.txt`.
///
/// Its `key` is the path; its `header` the `-R` header, if any.
#[pyclass(from_py_object, module = "liquers")]
#[derive(Clone)]
pub struct ResourceQuerySegment(liquers_core::query::ResourceQuerySegment);

#[pymethods]
impl ResourceQuerySegment {
    #[new]
    pub fn new() -> Self {
        ResourceQuerySegment(liquers_core::query::ResourceQuerySegment::default())
    }

    /// The segment's header, or `None` when the segment has none.
    #[getter]
    pub fn header(&self) -> Option<SegmentHeader> {
        match &self.0.header {
            Some(h) => Some(SegmentHeader(h.clone())),
            None => None,
        }
    }

    /// The name in the segment's header; empty when there is no header or it has no name.
    pub fn segment_name(&self) -> String {
        match self.0.header {
            Some(ref h) => h.name.to_string(),
            None => "".to_string(),
        }
    }

    /// The store key the segment names.
    #[getter]
    pub fn key(&self) -> Key {
        Key(self.0.key.clone())
    }

    /// The segment in query syntax.
    pub fn encode(&self) -> String {
        self.0.encode()
    }

    /// The segment with the `.` and `..` elements of its key resolved against `cwd_key`.
    pub fn to_absolute(&self, cwd_key: &Key) -> ResourceQuerySegment {
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

/// One segment of a query: either a resource segment or a transform segment.
///
/// Returned by `Query.segments` and `Query.predecessor()`. Use `is_resource_query_segment()` or
/// `is_transform_query_segment()` to tell which.
#[pyclass(from_py_object, module = "liquers")]
#[derive(Clone)]
pub struct QuerySegment(liquers_core::query::QuerySegment);

#[pymethods]
impl QuerySegment {
    #[new]
    pub fn new() -> Self {
        QuerySegment(liquers_core::query::QuerySegment::default())
    }

    /// The filename that ends the segment, or `None`.
    #[getter]
    pub fn filename(&self) -> Option<String> {
        self.0.filename().map(|s| s.to_string())
    }

    /// The segment's header, or `None` when the segment has none.
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

    /// True for a resource segment (`-R/...`).
    pub fn is_resource_query_segment(&self) -> bool {
        self.0.is_resource_query_segment()
    }

    /// True for a transform segment (actions).
    pub fn is_transform_query_segment(&self) -> bool {
        self.0.is_transform_query_segment()
    }

    /// True when the segment has no actions and no resource names.
    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    /// True when the segment is only a filename.
    pub fn is_filename(&self) -> bool {
        self.0.is_filename()
    }

    /// True when the segment is exactly one action and no filename.
    pub fn is_action_request(&self) -> bool {
        self.0.is_action_request()
    }

    /// The segment in query syntax.
    pub fn encode(&self) -> String {
        self.0.encode()
    }

    /// The segment with `.` and `..` resolved against `cwd_key`.
    ///
    /// Only a resource segment has a key to resolve; a transform segment is returned unchanged.
    pub fn to_absolute(&self, cwd_key: &Key) -> QuerySegment {
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

/// A parsed query: a sequence of resource and transform segments.
///
/// Make one with `parse(text)`; `Query()` is the empty query. Queries compare and hash by their
/// meaning, not by `source`. See `PYTHON_GUIDE.md` §3 and §6.
#[pyclass(from_py_object, module = "liquers")]
#[derive(Clone, Debug)]
pub struct Query(pub liquers_core::query::Query);

#[pymethods]
impl Query {
    #[new]
    pub fn new() -> Self {
        Query(liquers_core::query::Query::default())
    }

    /// True when the query text starts with `/`.
    ///
    /// Relative keys (`.`, `..`) in a rooted query's resource segments resolve against the root of
    /// the store rather than the working directory.
    #[getter]
    pub fn rooted(&self) -> bool {
        self.0.rooted
    }

    /// Where the query was read from (a `QuerySource`); informational, not part of equality.
    #[getter]
    pub fn source(&self) -> QuerySource {
        QuerySource(self.0.source.clone())
    }

    /// The query's segments, in order, as `QuerySegment` objects.
    #[getter]
    pub fn segments(&self) -> Vec<QuerySegment> {
        self.0
            .segments
            .iter()
            .map(|s| QuerySegment(s.clone()))
            .collect()
    }

    /// The filename at the end of the query, or `None`.
    pub fn filename(&self) -> Option<String> {
        self.0.filename().map(|s| s.to_string())
    }

    /// A copy of the query with its final filename removed.
    pub fn without_filename(&self) -> Query {
        Query(self.0.clone().without_filename())
    }

    /// The extension of the query's filename (`txt` for `.../report.txt`), or `None`.
    pub fn extension(&self) -> Option<String> {
        self.0.extension()
    }

    /// True when the query has no segments, like the empty string.
    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    /// True when the query is a single transform segment (no resource segment).
    pub fn is_transform_query(&self) -> bool {
        self.0.is_transform_query()
    }

    /// The segment when the query is a single transform segment, else `None`.
    pub fn transform_query(&self) -> Option<TransformQuerySegment> {
        self.0.transform_query().map(|s| TransformQuerySegment(s))
    }

    /// True when the query is a single resource segment (no actions).
    pub fn is_resource_query(&self) -> bool {
        self.0.is_resource_query()
    }

    /// The segment when the query is a single resource segment, else `None`.
    pub fn resource_query(&self) -> Option<ResourceQuerySegment> {
        self.0.resource_query().map(|s| ResourceQuerySegment(s))
    }

    /// True when the query is a single action and nothing else.
    pub fn is_action_request(&self) -> bool {
        self.0.is_action_request()
    }

    /// The action when the query is a single action request (see `is_action_request`), else `None`.
    pub fn action(&self) -> Option<ActionRequest> {
        if self.0.is_action_request() {
            self.0.action().map(|a| ActionRequest(a))
        } else {
            None
        }
    }

    /// Splits off the last step of the query.
    ///
    /// Returns `(rest, last)`: the query without its last step, and that step as a `QuerySegment`.
    /// A step is the final filename, the final action, or a final resource segment. Either part is
    /// `None` when there is nothing there; both are `None` for the empty query and for a query that
    /// is only a resource segment.
    pub fn predecessor(&self) -> (Option<Query>, Option<QuerySegment>) {
        let (p, r) = self.0.predecessor();
        (p.map(|s| Query(s)), r.map(|s| QuerySegment(s)))
    }

    /// The `(rest, last)` pairs of `predecessor()`, applied step by step from the full query down.
    pub fn all_predecessors(&self) -> Vec<(Option<Query>, Option<QuerySegment>)> {
        self.0
            .all_predecessors()
            .into_iter()
            .map(|(p, r)| (p.map(|s| Query(s)), r.map(|s| QuerySegment(s))))
            .collect()
    }

    /// A shortened form of the query text for display, at most `n` characters of the query.
    //#[args(n = 30)]
    pub fn short(&self, n: usize) -> String {
        self.0.short(n)
    }

    /// The query as text, in the encoded form `parse` reads back.
    pub fn encode(&self) -> String {
        self.0.encode()
    }

    /// The query with the `.` and `..` elements of its resource keys resolved against `cwd_key`.
    pub fn to_absolute(&self, cwd_key: &Key) -> Query {
        Query(self.0.to_absolute(&cwd_key.0))
    }

    /// The query in canonical form: the same meaning, one spelling.
    ///
    /// Two queries that mean the same have the same canonical form, which is what makes it suitable
    /// as a cache key.
    pub fn canonical(&self) -> Query {
        Query(self.0.clone().canonical())
    }

    pub fn __repr__(&self) -> String {
        format!("Query('{:?}')", self.0.encode())
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

/// Where a query was read from: another query, a store key, a string, something else, or
/// unspecified.
///
/// Informational only; it is not encoded and does not take part in equality or hashing. Make one
/// with the static constructors (`QuerySource.query(text)`, `QuerySource.key(key)`, ...).
#[pyclass(from_py_object, module = "liquers")]
#[derive(Clone)]
pub struct QuerySource(liquers_core::query::QuerySource);

#[pymethods]
impl QuerySource {
    /// A source: the query was read from the result of the query `source`.
    #[staticmethod]
    pub fn query(source: String) -> Self {
        Self(liquers_core::query::QuerySource::Query(source))
    }

    /// A source: the query was read from the store at `source`.
    #[staticmethod]
    pub fn key(source: &Key) -> Self {
        Self(liquers_core::query::QuerySource::Key(source.0.clone()))
    }

    /// A source: the query was read from the string `source`.
    #[staticmethod]
    pub fn string(source: String) -> Self {
        Self(liquers_core::query::QuerySource::String(source))
    }

    /// A source described by the free text `source`.
    #[staticmethod]
    pub fn other(source: String) -> Self {
        Self(liquers_core::query::QuerySource::Other(source))
    }

    /// The source of a query whose origin is not recorded.
    #[staticmethod]
    pub fn unspecified() -> Self {
        Self(liquers_core::query::QuerySource::Unspecified)
    }

    /// The source as text, for example `key data/q.txt`.
    pub fn encode(&self) -> String {
        self.0.to_string()
    }

    pub fn __repr__(&self) -> String {
        format!("QuerySource('{:?}')", self.0)
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

/// Parses query text into a `Query`.
///
/// Raises `ParseError` (a `LiquersError`) when the text is not a valid query; the error's
/// `position` points at the problem. See `PYTHON_GUIDE.md` §3.
#[pyfunction]
pub fn parse(query: &str) -> PyResult<Query> {
    match liquers_core::parse::parse_query(query) {
        Ok(q) => Ok(Query(q)),
        Err(e) => Err(PyErr::from(Error::from(e))),
    }
}

/// Parses text into a `Key`, such as `data/report.txt`.
///
/// Raises `ParseError` when the text is not a valid key.
#[pyfunction]
pub fn parse_key(key: &str) -> PyResult<Key> {
    match liquers_core::parse::parse_key(key) {
        Ok(k) => Ok(Key(k)),
        Err(e) => Err(PyErr::from(Error::from(e))),
    }
}
