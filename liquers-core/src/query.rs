#![allow(unused_imports)]
#![allow(dead_code)]

use itertools::Itertools;
use std::fmt::Display;
use std::hash::Hash;
use std::ops::{Add, Index, IndexMut};
use std::path::Path;

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct Position {
    pub offset: usize,
    pub line: u32,
    pub column: usize,
}

#[allow(dead_code)]
impl Position {
    pub fn new(offset: usize, line: u32, column: usize) -> Self {
        Position {
            offset: offset,
            line: line,
            column: column,
        }
    }
    pub fn unknown() -> Position {
        Position {
            offset: 0,
            line: 0,
            column: 0,
        }
    }
    pub fn is_unknown(&self) -> bool {
        self.line == 0
    }
}

impl Default for Position {
    fn default() -> Self {
        Position::unknown()
    }
}

impl Display for Position {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        if self.line == 0 {
            write!(f, "(unknown position)")
        } else if self.line > 1 {
            write!(f, "line {}, position {}", self.line, self.column)
        } else {
            write!(f, "position {}", self.column)
        }
    }
}

pub fn encode_token<S: AsRef<str>>(text: S) -> String {
    format!("{}", text.as_ref())
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub enum ActionParameter {
    String(String, Position),
    Link(Query, Position),
}

#[allow(dead_code)]
impl ActionParameter {
    pub fn new_string(parameter: String) -> ActionParameter {
        ActionParameter::String(parameter, Position::unknown())
    }
    pub fn new_link(query: Query) -> ActionParameter {
        ActionParameter::Link(query, Position::unknown())
    }
    pub fn is_string(&self) -> bool {
        match self {
            ActionParameter::String(_, _) => true,
            ActionParameter::Link(_, _) => false,
        }
    }
    pub fn string_value(&self) -> Option<String> {
        match self {
            ActionParameter::String(x, _) => Some(x.to_owned()),
            ActionParameter::Link(_, _) => None,
        }
    }
    pub fn is_link(&self) -> bool {
        match self {
            ActionParameter::String(_, _) => false,
            ActionParameter::Link(_, _) => true,
        }
    }
    pub fn link_value(&self) -> Option<Query> {
        match self {
            ActionParameter::String(_, _) => None,
            ActionParameter::Link(x, _) => Some(x.to_owned()),
        }
    }
    pub fn with_position(self, position: Position) -> Self {
        match self {
            Self::String(s, _) => Self::String(s, position),
            Self::Link(query, _) => Self::Link(query, position),
        }
    }
    pub fn position(&self) -> Position {
        match self {
            Self::String(_, p) => p.to_owned(),
            Self::Link(_, p) => p.to_owned(),
        }
    }
    pub fn encode(&self) -> String {
        match self {
            Self::String(s, _) => encode_token(s),
            Self::Link(query, _) => format!("~X~{}~E", query.encode()),
        }
    }
    /*
    pub fn to_html(&self, mark_position:&Position) -> String {
        match self {
            Self::String(s, _) => encode_token(s),
            Self::Link(query, _) => format!("<a href=\"{}\">{}</a>", query.encode(), query.encode()),
        }
    }
    */
}

impl Display for ActionParameter {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.encode())
    }
}

impl PartialEq for ActionParameter {
    fn eq(&self, other: &Self) -> bool {
        match (self, other) {
            (Self::String(s1, _), Self::String(s2, _)) => s1 == s2,
            (Self::Link(q1, _), Self::Link(q2, _)) => q1.encode() == q2.encode(),
            _ => false,
        }
    }
}

impl Eq for ActionParameter {}

impl Hash for ActionParameter {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        match self {
            Self::String(s, _) => s.hash(state),
            Self::Link(_, _) => self.encode().hash(state),
        }
    }
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct ResourceName {
    pub name: String,
    pub position: Position,
}

#[allow(dead_code)]
impl ResourceName {
    /// Create a new resource name (without a position)
    pub fn new(name: String) -> Self {
        Self {
            name: name,
            position: Position::unknown(),
        }
    }
    /// Equip the resource name with a position
    pub fn with_position(self, position: Position) -> Self {
        Self {
            position: position,
            ..self
        }
    }

    /// Clear the position of the resource name
    pub fn clean_position(&mut self) {
        self.position = Position::unknown();
    }

    /// Is a resource representing the current working directory (i.e. ".")
    pub fn is_cwd(&self) -> bool {
        self.name == "."
    }

    /// Is a resource representing the parent directory (i.e. "..")
    pub fn is_parent(&self) -> bool {
        self.name == ".."
    }

    /// Encode resource name as a string
    pub fn encode(&self) -> &str {
        &self.name
    }
    /// Return file extension if present, None otherwise.
    pub fn extension(&self) -> Option<String> {
        if self.name.contains('.') {
            self.name.split(".").last().map(|s| s.to_owned())
        } else {
            None
        }
    }
}

impl PartialEq for ResourceName {
    fn eq(&self, other: &Self) -> bool {
        self.name == other.name
    }
}

impl Eq for ResourceName {}

impl Hash for ResourceName {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        self.name.hash(state);
    }
}

impl Display for ResourceName {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.encode())
    }
}

#[derive(Serialize, Deserialize, Debug, Clone, Default)]
pub struct ActionRequest {
    pub name: String,
    pub parameters: Vec<ActionParameter>,
    pub position: Position,
}

#[allow(dead_code)]
impl ActionRequest {
    pub fn new(name: String) -> ActionRequest {
        ActionRequest {
            name: name,
            ..Default::default()
        }
    }
    pub fn with_position(self, position: Position) -> Self {
        Self {
            position: position,
            ..self
        }
    }
    pub fn with_parameters(self, parameters: Vec<ActionParameter>) -> Self {
        Self {
            parameters: parameters,
            ..self
        }
    }
    pub fn is_ns(&self) -> bool {
        self.name == "ns"
    }
    pub fn ns(&self) -> Option<Vec<ActionParameter>> {
        if self.is_ns() {
            Some(self.parameters.clone())
        } else {
            None
        }
    }
    pub fn encode(&self) -> String {
        if self.parameters.len() == 0 {
            return self.name.to_owned();
        } else {
            format!(
                "{}-{}",
                self.name,
                self.parameters.iter().map(|x| x.encode()).join("-")
            )
        }
    }
}

impl Display for ActionRequest {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.encode())
    }
}

impl PartialEq for ActionRequest {
    fn eq(&self, other: &Self) -> bool {
        self.name == other.name && self.parameters == other.parameters
    }
}

impl Eq for ActionRequest {}

impl Hash for ActionRequest {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        self.name.hash(state);
        self.parameters.hash(state);
    }
}

#[derive(Serialize, Deserialize, Debug, Clone, Default)]
pub struct HeaderParameter {
    pub value: String,
    pub position: Position,
}

#[allow(dead_code)]
impl HeaderParameter {
    pub fn new(value: String) -> HeaderParameter {
        HeaderParameter {
            value: value,
            ..Default::default()
        }
    }
    pub fn with_position(self, position: Position) -> Self {
        Self {
            value: self.value,
            position: position,
        }
    }
    pub fn encode(&self) -> &str {
        &self.value
    }
}

impl Display for HeaderParameter {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.value)
    }
}

impl PartialEq for HeaderParameter {
    fn eq(&self, other: &Self) -> bool {
        self.value == other.value
    }
}

impl Eq for HeaderParameter {}

impl Hash for HeaderParameter {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        self.value.hash(state);
    }
}

/// Header of a query segment - both resource and transformation query.
/// Header may contain name (string), level (integer) and parameters (list of strings).
/// The header parameters may influence how the query is interpreted.
/// The interpretation of the header parameters depends on the context object.
#[derive(Serialize, Deserialize, Debug, Clone, Default)]
pub struct SegmentHeader {
    pub name: String,
    pub level: usize,
    pub parameters: Vec<HeaderParameter>,
    pub resource: bool,
    pub position: Position,
}

#[allow(dead_code)]
impl SegmentHeader {
    /// Returns true if the header does not contain any data,
    /// I.e. trivial header has no name, level is 0 and no parameters.
    /// Trivial header can be both for resource and query, it does not depend on the resource flags.
    pub fn is_trivial(&self) -> bool {
        self.name.is_empty() && self.level == 0 && self.parameters.len() == 0
    }

    pub fn new() -> SegmentHeader {
        SegmentHeader {
            name: "".to_owned(),
            level: 0,
            parameters: vec![],
            resource: false,
            position: Position::unknown(),
        }
    }
    pub fn with_position(self, position: Position) -> Self {
        Self {
            position: position,
            ..self
        }
    }

    pub fn encode(&self) -> String {
        let mut encoded: String = std::iter::repeat("-").take(self.level + 1).collect();
        if self.resource {
            encoded.push('R');
        }
        encoded.push_str(&self.name);
        if !self.parameters.is_empty() {
            //assert len(self.name) > 0 or self.resource
            for parameter in self.parameters.iter() {
                encoded.push('-');
                encoded.push_str(parameter.encode());
            }
        }
        encoded
    }
}

impl Display for SegmentHeader {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.encode())
    }
}

impl PartialEq for SegmentHeader {
    fn eq(&self, other: &Self) -> bool {
        self.name == other.name
            && self.level == other.level
            && self.parameters == other.parameters
            && self.resource == other.resource
    }
}

impl Eq for SegmentHeader {}

impl Hash for SegmentHeader {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        self.name.hash(state);
        self.level.hash(state);
        self.parameters.hash(state);
        self.resource.hash(state);
    }
}

/// Query segment representing a transformation, i.e. a sequence of actions applied to a state.
#[derive(Serialize, Deserialize, Debug, Clone, Default)]
pub struct TransformQuerySegment {
    pub header: Option<SegmentHeader>,
    pub query: Vec<ActionRequest>,
    pub filename: Option<ResourceName>,
}

#[allow(dead_code)]
impl TransformQuerySegment {
    pub fn new() -> TransformQuerySegment {
        TransformQuerySegment {
            header: None,
            query: vec![],
            filename: None,
        }
    }

    /// Return name of the transform query segment
    pub fn name(&self) -> String {
        if let Some(header) = &self.header {
            header.name.clone()
        } else {
            "".to_owned()
        }
    }

    pub fn predecessor(&self) -> (Option<TransformQuerySegment>, Option<TransformQuerySegment>) {
        if let Some(filename) = &self.filename {
            (
                Some(TransformQuerySegment {
                    header: self.header.clone(),
                    query: self.query.clone(),
                    filename: None,
                }),
                Some(TransformQuerySegment {
                    header: self.header.clone(),
                    query: vec![],
                    filename: Some(filename.clone()),
                }),
            )
        } else {
            if self.query.is_empty() {
                (None, None)
            } else {
                let mut q = vec![];
                self.query[0..self.query.len() - 1].clone_into(&mut q);
                (
                    Some(TransformQuerySegment {
                        header: self.header.clone(),
                        query: q,
                        filename: None,
                    }),
                    Some(TransformQuerySegment {
                        header: self.header.clone(),
                        query: vec![self.query.last().unwrap().clone()],
                        filename: None,
                    }),
                )
            }
        }
    }

    pub fn is_empty(&self) -> bool {
        self.query.is_empty() && self.filename.is_none()
    }

    pub fn is_filename(&self) -> bool {
        self.query.is_empty() && self.filename.is_some()
    }

    pub fn is_action_request(&self) -> bool {
        self.query.len() == 1 && self.filename.is_none()
    }

    pub fn action(&self) -> Option<ActionRequest> {
        if self.is_action_request() {
            Some(self.query[0].clone())
        } else {
            None
        }
    }
    pub fn is_ns(&self) -> bool {
        self.action().map_or(false, |x| x.is_ns())
    }
    pub fn ns(&self) -> Option<Vec<ActionParameter>> {
        self.action().and_then(|x| x.ns())
    }
    pub fn last_ns(&self) -> Option<Vec<ActionParameter>> {
        self.query.iter().rev().find_map(|x| x.ns())
    }

    pub fn encode(&self) -> String {
        let pure_query = self.query.iter().map(|x| x.encode()).join("/");
        let query = if let Some(filename) = &self.filename {
            if pure_query.is_empty() {
                filename.encode().to_owned()
            } else {
                format!("{}/{}", pure_query, filename.encode())
            }
        } else {
            pure_query
        };

        if let Some(header) = &self.header {
            if query.is_empty() {
                header.encode()
            } else {
                format!("{}/{}", header.encode(), query)
            }
        } else {
            query
        }
    }

    /// Length of query - i.e. number of actions in the query
    fn len(&self) -> usize {
        self.query.len()
    }
}

impl Add for TransformQuerySegment {
    type Output = TransformQuerySegment;

    fn add(self, rhs: Self) -> Self::Output {
        let mut q = self.query.clone();
        q.extend(rhs.query.iter().map(|x| x.clone()));
        TransformQuerySegment {
            header: self.header.clone(),
            query: q,
            filename: rhs.filename.clone(),
        }
    }
}

impl Add<Option<TransformQuerySegment>> for TransformQuerySegment {
    type Output = TransformQuerySegment;

    fn add(self, rhs: Option<TransformQuerySegment>) -> Self::Output {
        match rhs {
            Some(x) => self + x,
            None => self,
        }
    }
}

impl Display for TransformQuerySegment {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.encode())
    }
}

impl PartialEq for TransformQuerySegment {
    fn eq(&self, other: &Self) -> bool {
        self.header == other.header && self.query == other.query && self.filename == other.filename
    }
}

impl Eq for TransformQuerySegment {}

impl Hash for TransformQuerySegment {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        self.header.hash(state);
        self.query.hash(state);
        self.filename.hash(state);
    }
}

#[derive(Serialize, Deserialize, Debug, Clone, Default, PartialEq, Eq, Hash)]
pub struct Key(pub Vec<ResourceName>);
impl Key {
    /// Create a new empty key
    pub fn new() -> Self {
        Self(vec![])
    }

    /// Clean the position of all the elements of the key
    fn clean_position(&mut self) {
        for x in self.0.iter_mut() {
            x.clean_position();
        }
    }

    /// Check if the key is empty
    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    /// Return iterator over the key elements
    pub fn iter(&self) -> std::slice::Iter<'_, ResourceName> {
        self.0.iter()
    }

    /// Return the last element of the key if present, None otherwise.
    /// This is typically interpreted as a filename in a Store object.
    pub fn filename(&self) -> Option<&ResourceName> {
        self.0.last()
    }

    /// Filename extension if present, None otherwise.
    pub fn extension(&self) -> Option<String> {
        self.filename().and_then(|x| x.extension())
    }

    /// Return the length of the key (number of elements)
    pub fn len(&self) -> usize {
        self.0.len()
    }

    /// Return the key as a string.
    pub fn encode(&self) -> String {
        self.0.iter().map(|x| x.encode()).join("/")
    }

    /*
    // Check if the key has a given string prefix.
    pub fn has_prefix<S: AsRef<str>>(&self, prefix: S) -> bool {
        self.encode().starts_with(prefix.as_ref())
    }
    */

    /// Check if the key has a given key prefix.
    pub fn has_key_prefix(&self, key_prefix: &Key) -> bool {
        if self.len() < key_prefix.len() {
            return false;
        }
        for i in 0..key_prefix.len() {
            if self[i].name != key_prefix[i].name {
                return false;
            }
        }
        true
    }

    /// Append a name as a new element at the end of the key
    pub fn join<S: AsRef<str>>(&self, name: S) -> Self {
        let mut key = self.clone();
        key.0.push(ResourceName::new(name.as_ref().to_owned()));
        key
    }

    /// Return a parent key - i.e. a key without the last element.
    pub fn parent(&self) -> Self {
        let mut key = Vec::new();
        if self.is_empty() {
            return Key(vec![]);
        }
        for x in self.iter().take(self.len() - 1) {
            key.push(x.clone());
        }
        Key(key)
    }

    /// Convert a key to an absolute key - i.e. interpret "." and ".." elements.
    /// The cwd_key is a "current working directory" key - i.e. a key to which "." and ".." elements are relative to.
    /// Note that the cwd_key should be absolute, i.e. it should not contain any "." or ".." elements.
    /// This is not checked by the function.
    pub fn to_absolute(&self, cwd_key: &Key) -> Self {
        let mut result = Vec::new();
        let mut use_cwd = true;
        for x in self.iter() {
            if !result.is_empty() {
                use_cwd = false;
            }
            if x.is_cwd() {
                if use_cwd {
                    for y in cwd_key.iter() {
                        result.push(y.clone());
                    }
                }
            } else if x.is_parent() {
                if use_cwd {
                    for y in cwd_key.parent().iter() {
                        result.push(y.clone());
                    }
                } else {
                    result.pop();
                }
            } else {
                result.push(x.clone());
            }
        }
        Key(result)
    }
}

impl Index<usize> for Key {
    type Output = ResourceName;

    fn index(&self, index: usize) -> &Self::Output {
        &self.0[index]
    }
}

impl IndexMut<usize> for Key {
    fn index_mut(&mut self, index: usize) -> &mut Self::Output {
        &mut self.0[index]
    }
}

impl Display for Key {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        if self.is_empty() {
            write!(f, "")?;
        } else {
            write!(f, "{}", self[0].encode())?;
            for x in self.iter().skip(1) {
                write!(f, "/{}", x.encode())?;
            }
        }
        Ok(())
    }
}

/// Query segment representing a resource, i.e. path to a file in a store.
#[derive(Serialize, Deserialize, Debug, Clone, Default)]
pub struct ResourceQuerySegment {
    pub header: Option<SegmentHeader>,
    pub key: Key,
}

#[allow(dead_code)]
impl ResourceQuerySegment {
    /// Create a new empty resource query segment
    pub fn new() -> ResourceQuerySegment {
        ResourceQuerySegment {
            header: None,
            key: Key::new(),
        }
    }

    /// Return name of the resource query segment
    pub fn name(&self) -> String {
        if let Some(header) = &self.header {
            header.name.clone()
        } else {
            "".to_owned()
        }
    }

    /// Return resource query position
    pub fn position(&self) -> Position {
        if let Some(header) = &self.header {
            header.position.to_owned()
        } else {
            if self.key.is_empty() {
                Position::unknown()
            } else {
                self.key[0].position.to_owned()
            }
        }
    }

    pub fn encode(&self) -> String {
        let mut rqs = self.header.as_ref().map_or("".to_owned(), |x| x.encode());
        if !rqs.is_empty() {
            rqs.push('/');
        }
        if self.key.is_empty() {
            rqs
        } else {
            let key = self.key.iter().map(|x| x.encode()).join("/");
            format!("{rqs}{key}")
        }
    }

    pub fn encode_with_header(&self) -> String {
        match &self.header {
            None => {
                if self.key.is_empty() {
                    "-R".to_owned()
                } else {
                    format!("-R/{}", self.key.encode())
                }
            }
            Some(header) => {
                if self.key.is_empty() {
                    header.encode()
                } else {
                    format!("{}/{}", header.encode(), self.key.encode())
                }
            }
        }
    }

    pub fn filename(&self) -> Option<ResourceName> {
        self.key.filename().cloned()
    }

    pub fn is_filename(&self) -> bool {
        self.key.len() == 1
    }

    pub fn len(&self) -> usize {
        self.key.len()
    }

    pub fn is_empty(&self) -> bool {
        self.key.is_empty()
    }

    /// Convert a resource query to an absolute resource query - i.e. interpret "." and ".." elements.
    /// The cwd_key is a "current working directory" key - i.e. a key to which "." and ".." elements are relative to.
    /// This happens regardless the resource name or other header parameters.
    /// Note that the cwd_key should be absolute, i.e. it should not contain any "." or ".." elements.
    /// This is not checked by the function.
    pub fn to_absolute(&self, cwd_key: &Key) -> Self {
        Self {
            header: self.header.clone(),
            key: self.key.to_absolute(cwd_key),
        }
    }
}

impl Display for ResourceQuerySegment {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.encode())
    }
}

impl PartialEq for ResourceQuerySegment {
    fn eq(&self, other: &Self) -> bool {
        self.header == other.header && self.key == other.key
    }
}

impl Eq for ResourceQuerySegment {}

impl Hash for ResourceQuerySegment {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        self.header.hash(state);
        self.key.hash(state);
    }
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub enum QuerySegment {
    Resource(ResourceQuerySegment),
    Transform(TransformQuerySegment),
}

impl QuerySegment {
    /// Create a new empty transform query segment
    pub fn empty_transform_query_segment() -> Self {
        QuerySegment::Transform(TransformQuerySegment::new())
    }
    /// Create a new empty resource query segment
    pub fn empty_resource_query_segment() -> Self {
        QuerySegment::Resource(ResourceQuerySegment::new())
    }

    /// Return name of the query segment
    pub fn name(&self) -> String {
        match self {
            QuerySegment::Resource(rqs) => rqs.name(),
            QuerySegment::Transform(tqs) => tqs.name(),
        }
    }

    /// Encode query segment as a string
    pub fn encode(&self) -> String {
        match self {
            QuerySegment::Resource(rqs) => rqs.encode(),
            QuerySegment::Transform(tqs) => tqs.encode(),
        }
    }

    /// Encode query segment as a string, resource always with a header
    pub fn encode_with_header(&self) -> String {
        match self {
            QuerySegment::Resource(rqs) => rqs.encode_with_header(),
            QuerySegment::Transform(tqs) => tqs.encode(),
        }
    }

    /// Convert a query segment to an absolute query segment - i.e. interpret "." and ".." elements.
    /// See ResourceQuerySegment::to_absolute for details.
    pub fn to_absolute(&self, cwd_key: &Key) -> Self {
        match self {
            QuerySegment::Resource(rqs) => QuerySegment::Resource(rqs.to_absolute(cwd_key)),
            QuerySegment::Transform(_) => self.clone(),
        }
    }

    /// Return filename if present, None otherwise.
    pub fn filename(&self) -> Option<ResourceName> {
        match self {
            QuerySegment::Resource(rqs) => rqs.filename().clone(),
            QuerySegment::Transform(tqs) => tqs.filename.clone(),
        }
    }

    /// Return length of query segment - i.e. number of actions or resource names in the query segment
    pub fn len(&self) -> usize {
        match self {
            QuerySegment::Resource(rqs) => rqs.len(),
            QuerySegment::Transform(tqs) => tqs.len(),
        }
    }

    /// Return true if the query segment is empty, i.e. has no actions or resource names.
    pub fn is_empty(&self) -> bool {
        match self {
            QuerySegment::Resource(rqs) => rqs.is_empty(),
            QuerySegment::Transform(tqs) => tqs.is_empty(),
        }
    }

    /// Return true if the query segment is a namespace definition.
    /// See TransformQuerySegment::is_ns for details.
    pub fn is_ns(&self) -> bool {
        match self {
            QuerySegment::Resource(_) => false,
            QuerySegment::Transform(tqs) => tqs.is_ns(),
        }
    }
    pub fn ns(&self) -> Option<Vec<ActionParameter>> {
        match self {
            QuerySegment::Resource(_) => None,
            QuerySegment::Transform(tqs) => tqs.ns(),
        }
    }
    pub fn last_ns(&self) -> Option<Vec<ActionParameter>> {
        match self {
            QuerySegment::Resource(_) => None,
            QuerySegment::Transform(tqs) => tqs.last_ns(),
        }
    }
    pub fn is_filename(&self) -> bool {
        match self {
            QuerySegment::Resource(rqs) => rqs.is_filename(),
            QuerySegment::Transform(tqs) => tqs.is_filename(),
        }
    }
    pub fn is_resource_query_segment(&self) -> bool {
        match self {
            QuerySegment::Resource(_) => true,
            QuerySegment::Transform(_) => false,
        }
    }
    pub fn is_transform_query_segment(&self) -> bool {
        match self {
            QuerySegment::Resource(_) => false,
            QuerySegment::Transform(_) => true,
        }
    }
    /*
    pub fn resource(&self) -> Option<ResourceQuerySegment> {
        match self {
            QuerySegment::Resource(rqs) => Some(rqs.to_owned()),
            QuerySegment::Transform(_) => None,
        }
    }
    */
    pub fn resource_query_segment(&self) -> Option<ResourceQuerySegment> {
        match self {
            QuerySegment::Resource(rqs) => Some(rqs.to_owned()),
            QuerySegment::Transform(_) => None,
        }
    }
    pub fn transform_query_segment(&self) -> Option<TransformQuerySegment> {
        match self {
            QuerySegment::Resource(_) => None,
            QuerySegment::Transform(tqs) => Some(tqs.to_owned()),
        }
    }
    pub fn is_action_request(&self) -> bool {
        match self {
            QuerySegment::Resource(_) => false,
            QuerySegment::Transform(tqs) => tqs.is_action_request(),
        }
    }
    pub fn action(&self) -> Option<ActionRequest> {
        match self {
            QuerySegment::Resource(_) => None,
            QuerySegment::Transform(tqs) => tqs.action(),
        }
    }
}

impl Default for QuerySegment {
    fn default() -> Self {
        QuerySegment::Resource(ResourceQuerySegment::default())
    }
}

impl Display for QuerySegment {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.encode())
    }
}

impl PartialEq for QuerySegment {
    fn eq(&self, other: &Self) -> bool {
        match (self, other) {
            (QuerySegment::Resource(r1), QuerySegment::Resource(r2)) => r1 == r2,
            (QuerySegment::Transform(t1), QuerySegment::Transform(t2)) => t1 == t2,
            _ => false,
        }
    }
}

impl Eq for QuerySegment {}

impl Hash for QuerySegment {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        match self {
            QuerySegment::Resource(rqs) => rqs.hash(state),
            QuerySegment::Transform(tqs) => tqs.hash(state),
        }
    }
}

/// Query source - characterizes the place (string) where the query was read from.
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq, Hash)]
pub enum QuerySource {
    /// Query was read from a result of another query
    Query(String),
    /// Query was read from a store
    Key(Key),
    /// Query was read from a string
    String(String),
    /// Query was read from an unknown source
    Other(String),
    /// The source of the query is unspecified
    Unspecified,
}

impl Default for QuerySource {
    fn default() -> Self {
        QuerySource::Unspecified
    }
}

impl Display for QuerySource {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            QuerySource::Query(s) => write!(f, "query {}", s),
            QuerySource::Key(k) => write!(f, "key {}", k),
            QuerySource::String(s) => write!(f, "string {}", s),
            QuerySource::Other(s) => write!(f, "other {}", s),
            QuerySource::Unspecified => write!(f, "unspecified"),
        }
    }
}

/// Query is a sequence of query segments.
/// Typically this will be a resource and and/or a transformation applied to a resource.
#[derive(Serialize, Deserialize, Debug, Clone, Default)]
pub struct Query {
    pub segments: Vec<QuerySegment>,
    pub absolute: bool,
    pub source: QuerySource,
}

#[allow(dead_code)]
impl Query {
    /// Create a new empty query
    pub fn new() -> Query {
        Query {
            segments: vec![],
            absolute: false,
            source: QuerySource::Unspecified,
        }
    }

    /// Return filename if present, None otherwise.
    pub fn filename(&self) -> Option<ResourceName> {
        match self.segments.last() {
            None => None,
            Some(QuerySegment::Transform(tqs)) => tqs.filename.clone(),
            Some(QuerySegment::Resource(rqs)) => rqs.filename(),
        }
    }

    /// Return file extension if present, None otherwise.
    pub fn extension(&self) -> Option<String> {
        self.filename().and_then(|x| x.extension())
    }
    /// Returns true if the query is empty, i.e. has no segments and thus is equivalent to an empty string.
    pub fn is_empty(&self) -> bool {
        self.segments.is_empty()
    }
    /// Returns true if the query is a namespace definition.
    pub fn is_ns(&self) -> bool {
        self.transform_query().map_or(false, |x| x.is_ns())
    }
    /// Returns the namespace definition if path is a namespace action.
    pub fn ns(&self) -> Option<Vec<ActionParameter>> {
        self.transform_query().and_then(|x| x.ns())
    }

    /// Returns the last namespace definition if available
    pub fn last_ns(&self) -> Option<Vec<ActionParameter>> {
        self.transform_query().and_then(|x| x.last_ns())
    }

    /// Returns the last transform query name if available
    pub fn last_transform_query_name(&self) -> Option<String> {
        self.transform_query().map(|x| x.name())
    }

    /// Convert a query to an absolute query - i.e. interpret "." and ".." elements.
    /// See ResourceQuerySegment::to_absolute for details.
    pub fn to_absolute(&self, cwd_key: &Key) -> Self {
        Self {
            segments: self
                .segments
                .iter()
                .map(|x| x.to_absolute(cwd_key))
                .collect(),
            absolute: self.absolute,
            source: self.source.clone(),
        }
    }

    /// Returns true if the query is a pure transformation query - i.e. a sequence of actions.
    pub fn is_transform_query(&self) -> bool {
        self.segments.len() == 1
            && match &self.segments[0] {
                QuerySegment::Transform(_) => true,
                _ => false,
            }
    }

    /// Returns TransformQuerySegment if the query is a pure transformation query, None otherwise.
    pub fn transform_query(&self) -> Option<TransformQuerySegment> {
        if self.segments.len() == 1 {
            match &self.segments[0] {
                QuerySegment::Transform(tqs) => Some(tqs.clone()),
                _ => None,
            }
        } else {
            None
        }
    }

    /// Returns true if the query is a pure resource query
    pub fn is_resource_query(&self) -> bool {
        self.segments.len() == 1
            && match &self.segments[0] {
                QuerySegment::Resource(_) => true,
                _ => false,
            }
    }

    /// Returns ResourceQuerySegment if the query is a pure resource query, None otherwise.
    pub fn resource_query(&self) -> Option<ResourceQuerySegment> {
        if self.segments.len() == 1 {
            match &self.segments[0] {
                QuerySegment::Resource(rqs) => Some(rqs.clone()),
                _ => None,
            }
        } else {
            None
        }
    }

    /// Returns true if the query is a single action request.
    pub fn is_action_request(&self) -> bool {
        self.transform_query()
            .map_or(false, |x| x.is_action_request())
    }

    /// Returns ActionRequest if the query is a single action request, None otherwise.
    pub fn action(&self) -> Option<ActionRequest> {
        self.transform_query().and_then(|x| x.action())
    }

    fn up_to_last_segment(&self) -> Vec<QuerySegment> {
        let mut seg = vec![];
        self.segments[0..self.segments.len() - 1].clone_into(&mut seg);
        seg
    }

    /// Return tuple of (predecessor, remainder).
    /// Remainder is a last element (action or filename) or None if not available.
    /// Predecessor is a query without the remainder (or None).
    pub fn predecessor(&self) -> (Option<Query>, Option<QuerySegment>) {
        match &self.segments.last() {
            None => (None, None),
            Some(QuerySegment::Resource(rqs)) => {
                if self.is_resource_query() {
                    (None, None)
                } else {
                    (
                        Some(Query {
                            segments: self.up_to_last_segment(),
                            absolute: self.absolute,
                            ..Default::default()
                        }),
                        Some(QuerySegment::Resource(rqs.clone())),
                    )
                }
            }
            Some(QuerySegment::Transform(tqs)) => {
                let (p, r) = tqs.predecessor();
                if p.as_ref().map_or(true, |x| x.is_empty()) {
                    (
                        Some(Query {
                            segments: self.up_to_last_segment(),
                            absolute: self.absolute,
                            ..Default::default()
                        }),
                        r.map(|x| QuerySegment::Transform(x)),
                    )
                } else {
                    let mut seg = self.up_to_last_segment();
                    seg.push(QuerySegment::Transform(p.unwrap()));
                    (
                        Some(Query {
                            segments: seg,
                            absolute: self.absolute,
                            ..Default::default()
                        }),
                        r.map(|x| QuerySegment::Transform(x)),
                    )
                }
            }
        }
    }

    /// Return all predecessors of the query as a vector.
    pub fn all_predecessors(&self) -> Vec<(Option<Query>, Option<QuerySegment>)> {
        let mut result = vec![];
        let mut qp = Some(self);
        let mut qr: Option<QuerySegment> = None;
        let mut buff: Option<Query>;
        while qp.is_some() {
            /*
            println!(
                "qp/qr: {}  {}",
                qp.unwrap().encode(),
                qr.as_ref().map_or("None".to_owned(), |x| x.encode())
            );
            */
            if qp.unwrap().is_empty() {
                break;
            }
            let x = (qp.map(|x| x.clone()), (&qr).clone());
            result.push(x);
            let (q, r) = qp.unwrap().predecessor();
            buff = q;
            qp = buff.as_ref();
            qr = match (&qr, r) {
                (None, None) => None,
                (None, Some(r)) => Some(r),
                (Some(x), None) => Some(x.clone()),
                (Some(QuerySegment::Transform(x)), Some(QuerySegment::Transform(r))) => {
                    Some(QuerySegment::Transform(r + x.clone()))
                }
                _ => None,
            };
        }
        result
    }

    pub fn all_predecessor_tuples(&self) -> Vec<(Query, QuerySegment)> {
        let mut result = vec![];
        let mut qp = Some(self.clone());
        let mut last = None;
        fn add_to_result(
            result: &mut Vec<(Query, QuerySegment)>,
            qp: &Option<Query>,
            qr: &Option<QuerySegment>,
        ) {
            match (qp, qr) {
                (Some(qp), Some(qr)) => {
                    if (!qp.is_empty()) || (!qr.is_empty()) {
                        result.push((qp.clone(), qr.clone()));
                    }
                }
                (Some(qp), None) => {
                    if !qp.is_empty() {
                        result.push((qp.clone(), QuerySegment::empty_transform_query_segment()));
                    }
                }
                (None, Some(qr)) => {
                    if !qr.is_empty() {
                        result.push((Query::new(), qr.clone()));
                    }
                }
                (None, None) => {}
            }
        }
        while qp.is_some() {
            if !qp.as_ref().unwrap().is_empty() {
                last = qp.clone();
            } else {
                last = None;
            }
            let (p, r) = qp.unwrap().predecessor();
            add_to_result(&mut result, &p, &r);
            qp = p;
        }

        if let Some(r) = last {
            add_to_result(
                &mut result,
                &None,
                &r.resource_query().map(|x| QuerySegment::Resource(x)),
            );
        }
        result
    }

    /// Query without the filename.
    pub fn without_filename(self) -> Query {
        if (&self).filename().is_none() {
            self
        } else {
            if let (Some(p), _) = self.predecessor() {
                p
            } else {
                Query {
                    segments: vec![],
                    absolute: self.absolute,
                    ..Default::default()
                }
            }
        }
    }

    /// Make a shortened version of the at most n characters of a query for printout purposes
    pub fn short(&self, n: usize) -> String {
        if let (_, Some(r)) = self.predecessor() {
            r.encode()
        } else {
            let q = self.encode();
            if q.len() > n {
                format!("...{}", &q[q.len() - n..])
            } else {
                q
            }
        }
    }

    pub fn encode(&self) -> String {
        if self.segments.is_empty() {
            if self.absolute {
                return "/".to_owned();
            } else {
                return "".to_owned();
            }
        }
        let q = self
            .segments
            .iter()
            .map(|x| x.encode_with_header())
            .join("/");
        if self.absolute {
            format!("/{q}")
        } else {
            q
        }
    }
    pub fn len(&self) -> usize {
        self.segments.len()
    }
}

impl Display for Query {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.encode())
    }
}

impl PartialEq for Query {
    fn eq(&self, other: &Self) -> bool {
        self.segments == other.segments && self.absolute == other.absolute
    }
}

impl Eq for Query {}

impl Hash for Query {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        self.segments.hash(state);
        self.absolute.hash(state);
    }
}

impl Index<usize> for Query {
    type Output = QuerySegment;

    fn index(&self, index: usize) -> &Self::Output {
        &self.segments[index]
    }
}

impl IndexMut<usize> for Query {
    fn index_mut(&mut self, index: usize) -> &mut Self::Output {
        &mut self.segments[index]
    }
}

#[cfg(test)]
mod tests {
    use crate::parse::parse_key;

    use super::*;

    #[test]
    fn test_has_key_prefix() -> Result<(), Box<dyn std::error::Error>> {
        let key = parse_key("a/b/c").unwrap();
        assert!(key.has_key_prefix(&Key::new()));
        assert!(key.has_key_prefix(&parse_key("a").unwrap()));
        assert!(key.has_key_prefix(&parse_key("a/b").unwrap()));
        assert!(!key.has_key_prefix(&parse_key("a/c").unwrap()));
        Ok(())
    }

    #[test]
    fn encode_link_action_parameter() -> Result<(), Box<dyn std::error::Error>> {
        let q = Query {
            segments: vec![QuerySegment::Transform(TransformQuerySegment {
                query: vec![ActionRequest::new("hello".to_owned())],
                ..Default::default()
            })],
            absolute: false,
            ..Default::default()
        };
        let ap = ActionParameter::Link(q, Position::unknown());
        assert_eq!(ap.encode(), "~X~hello~E");
        Ok(())
    }

    #[test]
    fn encode_action_request() -> Result<(), Box<dyn std::error::Error>> {
        let a = ActionRequest {
            name: "action".to_owned(),
            position: Position::unknown(),
            parameters: vec![],
        };
        assert_eq!(a.encode(), "action");
        let a = ActionRequest::new("action1".to_owned());
        assert_eq!(a.encode(), "action1");
        let q = Query {
            segments: vec![QuerySegment::Transform(TransformQuerySegment {
                query: vec![ActionRequest::new("hello".to_owned())],
                ..Default::default()
            })],
            absolute: false,
            ..Default::default()
        };
        let a = ActionRequest {
            name: "action".to_owned(),
            position: Position::unknown(),
            parameters: vec![
                ActionParameter::Link(q, Position::unknown()),
                ActionParameter::String("world".to_string(), Position::unknown()),
            ],
        };
        assert_eq!(a.encode(), "action-~X~hello~E-world");
        let q = Query {
            segments: vec![QuerySegment::Transform(TransformQuerySegment {
                query: vec![ActionRequest::new("hello".to_owned())],
                ..Default::default()
            })],
            absolute: false,
            ..Default::default()
        };
        let a = ActionRequest::new("action1".to_owned()).with_parameters(vec![
            ActionParameter::new_link(q),
            ActionParameter::new_string("world".to_owned()),
        ]);
        assert_eq!(a.encode(), "action1-~X~hello~E-world");
        Ok(())
    }

    #[test]
    fn encode_segment_header() -> Result<(), Box<dyn std::error::Error>> {
        let head = SegmentHeader::new();
        assert_eq!(head.encode(), "-");
        Ok(())
    }

    #[test]
    fn add_filename() {
        let action = ActionRequest::new("action".to_owned());
        let filename = ResourceName::new("file.txt".to_owned());
        let a = TransformQuerySegment {
            query: vec![action],
            filename: None,
            ..Default::default()
        };
        let f = TransformQuerySegment {
            query: vec![],
            filename: Some(filename),
            ..Default::default()
        };

        let q = a + f;
        assert_eq!(q.encode(), "action/file.txt");
    }

    #[test]
    fn to_absolute1() {
        let cwd_key = parse_key("a/b/c").unwrap();
        assert_eq!(
            parse_key("./x").unwrap().to_absolute(&cwd_key).encode(),
            "a/b/c/x"
        );
        assert_eq!(
            parse_key("../x").unwrap().to_absolute(&cwd_key).encode(),
            "a/b/x"
        );
        assert_eq!(
            parse_key("../../x").unwrap().to_absolute(&cwd_key).encode(),
            "a/x"
        );
        assert_eq!(
            parse_key("../../../x")
                .unwrap()
                .to_absolute(&cwd_key)
                .encode(),
            "x"
        );
        assert_eq!(
            parse_key("../../../../x")
                .unwrap()
                .to_absolute(&cwd_key)
                .encode(),
            "x"
        );
        assert_eq!(
            parse_key("A/B/./x").unwrap().to_absolute(&cwd_key).encode(),
            "A/B/x"
        );
        assert_eq!(
            parse_key("A/B/../x")
                .unwrap()
                .to_absolute(&cwd_key)
                .encode(),
            "A/x"
        );
    }
    #[test]
    fn key_parent() {
        let key = parse_key("a/b/c").unwrap();
        assert_eq!(key.parent().encode(), "a/b");
        assert_eq!(key.parent().parent().encode(), "a");
        assert_eq!(key.parent().parent().parent().encode(), "");
        assert_eq!(key.parent().parent().parent().parent().encode(), "");
    }
    #[test]
    fn test_key_extension() {
        let key = parse_key("").unwrap();
        assert_eq!(key.extension(), None);
        let key = parse_key("a").unwrap();
        assert_eq!(key.extension(), None);
        let key = parse_key("a/b/c").unwrap();
        assert_eq!(key.extension(), None);
        let key = parse_key("a/b/c.txt").unwrap();
        assert_eq!(key.extension(), Some("txt".to_owned()));
        let key = parse_key("c.txt").unwrap();
        assert_eq!(key.extension(), Some("txt".to_owned()));
        let key = parse_key(".txt").unwrap();
        assert_eq!(key.extension(), Some("txt".to_owned()));
        let key = parse_key("arch.tar.gz").unwrap();
        assert_eq!(key.extension(), Some("gz".to_owned()));
    }
}
