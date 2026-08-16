//! Semantic data model for Liquers queries and resource keys.
//! The query is a central concept in Liquers and gave the project its name.
//! A single query can refer to resources and apply transformations to them. A query
//! can be considered a very simple scripting language or, more precisely, a
//! domain-specific language (DSL) for creating pipelines that query and transform
//! resources.
//!
//! [`crate::parse`] parses queries and contains the authoritative
//! [query syntax reference][crate::parse]. This module defines the query's abstract
//! syntax tree (AST). A [`Query`] is typically turned into a
//! [`crate::plan::Plan`], which can be considered a sequence of instructions
//! analogous to bytecode and can be executed by the [`crate::interpreter`] module.
//! These details are normally hidden from the user; a query can be evaluated
//! directly with [`crate::context::EnvRef::evaluate`].
//!
//! Queries can also be constructed, manipulated, encoded, and inspected using the
//! types in this module. This module defines what parsed elements mean, how they
//! are encoded, how relative resource names are resolved, and how query values are
//! compared. Rendering and styled-query facilities in this file are presentation
//! APIs; they do not define the language.
//!
//! # Data model
//!
//! A [`Query`] is an ordered sequence of [`QuerySegment`] values:
//!
//! - [`ResourceQuerySegment`] references a resource by [`Key`], with an optional
//!   resource [`SegmentHeader`] selecting how it is retrieved. The resource is
//!   typically an asset and can be thought of as a file: it has a logical path
//!   ([`Key`]) and data accompanied by metadata. Although a key may correspond to
//!   a filesystem path, it is a Liquers logical identifier rather than an
//!   operating-system path. Not all filesystem paths are valid keys; see [`Key`]
//!   and the syntax description in [`crate::parse`] for details.
//!
//! - [`TransformQuerySegment`] describes an ordered sequence of action requests
//!   applied to its input. Its input is typically the resource or transformation
//!   result produced by the preceding segment; a transform-only query instead
//!   starts without a preceding resource. A transform segment may also specify a
//!   terminal output filename and an optional header.
//! - [`ActionRequest`] contains a command name and ordered [`ActionParameter`]
//!   values. It represents an action that can be applied to an input state. An
//!   action request is effectively a function call on the input state or, more
//!   precisely, a closure in which the action request specifies every argument
//!   except the first argument, which receives the input state. A transformation query
//!   segment is a sequence of action requests.
//!
//! A common query therefore has (typically) this semantic flow:
//!
//! ```text
//! resource reference -> action -> action -> optional output filename
//! ```
//!
//! More complex queries are possible. A segment header can specify a level, realm,
//! and arguments:
//!
//! - Level (not implemented yet) may allow nested queries in the future.
//! - Realm (partly implemented, supported by the command registry) can separate environments; for example, it can
//!   specify which part of a query should run on the client or server.
//! - Arguments can pass data to the query segment. [`crate::plan::PlanBuilder`]
//!   already interprets several arguments.
//!
//! Query AST elements contain additional diagnostic information:
//!
//! - [`Position`] values preserve source locations for diagnostics. They are ignored
//!   when resource names, parameters, actions, and headers are compared or hashed.
//! - [`QuerySource`] is provenance metadata: [`Query`] equality and hashing ignore it,
//!   and [`Query::encode`] does not include it.
//! - The [`Query::absolute`] flag is part of equality and hashing and is rendered
//!   as a leading `/`. It currently has no semantic meaning.
//!
//! Constructors in this module do not validate parser grammar. Use
//! [`crate::parse::parse_query`] or [`crate::parse::parse_key`] when untrusted text
//! must be validated.
//!
//! # Headers
//!
//! [`SegmentHeader::resource`] distinguishes resource headers from transform
//! headers. A transform header's `name` is used as the command realm when the
//! entire query is a single transform segment. Resource-header behavior is selected
//! primarily by its first parameter during plan construction; see
//! [`crate::plan::PlanBuilder`].
//!
//! [`SegmentHeader::level`] records additional leading hyphens. It is a reserved
//! feature and is currently not interpreted. Header parameters are distinct from
//! action parameters and are not entity-decoded.
//!
//! # Encoding and canonical form
//!
//! [`Query::encode`] emits an explicit header for every resource segment, even if
//! the input used resource/transform shorthand. Encoding preserves ordinary
//! transform filenames. [`Query::canonical`] additionally supplies missing headers
//! and normalizes the basename of a terminal transform filename to `data` while
//! preserving its extension.
//!
//! String action parameters are escaped by [`encode_token`]. Link parameters can
//! be constructed with [`ActionParameter::Link`] and encode as `~X~<query>~E`.
//! This is intended supported syntax, but the current parser has no production for
//! it, so links do not yet round-trip through [`crate::parse::parse_query`]. The
//! omission is tracked as `QUERY-ACTION-PARAMETER-LINK-PARSER`.
//!
//! # Relative resource names
//!
//! [`Key::to_absolute`] interprets `.` and `..` relative to a supplied current
//! working-directory key. [`Query::to_absolute`] applies that operation to every
//! resource segment. It does not change or consult [`Query::absolute`].
//! [`Query::to_absolute`] can only be meaningfully applied when a current working
//! directory (CWD) is known. This is typically the case when a query is part of a
//! [`crate::recipes::Recipe`] located in a particular directory. In many contexts,
//! the CWD is undefined; for example, when a query is passed through the web API.
//!
//! # Interpreter instructions
//!
//! The planner gives three action names structural meaning:
//!
//! - `ns` selects command namespaces. The last `ns` action in the last transform
//!   segment is used for lookup.
//! - A terminal `q` asks the planner to use the preceding query as a query value.
//! - `v` marks the plan volatile and does not create a command-execution step.
//!
//! These are semantic rules of plan construction, not additional parser grammar.

#![allow(unused_imports)]
#![allow(dead_code)]

use ansic::ansi;
use itertools::{Either, Itertools};
use nom::Err;
use std::borrow::Cow;
use std::fmt::Display;
use std::hash::Hash;
use std::ops::{Add, Index, IndexMut};
use std::path::Path;

use crate::error::Error;

static UNKNOWN_POSITION: Position = Position {
    offset: 0,
    line: 0,
    column: 0,
};

pub trait QueryRenderStyle {
    fn position(&self) -> &Position;
    fn highlight(&self, position: &Position) -> bool {
        if self.position().is_unknown() {
            return false;
        }
        *position == *self.position()
    }
    fn highlight_or<F: Fn(&str) -> String>(&self, text: &str, position: &Position, f: F) -> String {
        if self.highlight(position) {
            self.highlighted_text(text)
        } else {
            f(text)
        }
    }
    fn string_parameter_begin(&self, position: &Position) -> Cow<'static, str>;
    fn string_parameter_end(&self, position: &Position) -> Cow<'static, str>;
    fn string_parameter(&self, parameter: &str, position: &Position) -> String {
        self.highlight_or(parameter, position, |text| {
            format!(
                "{}{}{}",
                self.string_parameter_begin(position),
                text,
                self.string_parameter_end(position)
            )
        })
    }
    fn entity_begin(&self, position: &Position) -> Cow<'static, str>;
    fn entity_end(&self, position: &Position) -> Cow<'static, str>;
    fn entity(&self, name: &str, position: &Position) -> String {
        self.highlight_or(name, position, |text| {
            format!(
                "{}{}{}",
                self.entity_begin(position),
                text,
                self.entity_end(position)
            )
        })
    }
    fn separator_begin(&self, position: &Position) -> Cow<'static, str>;
    fn separator_end(&self, position: &Position) -> Cow<'static, str>;
    fn separator(&self, name: &str, position: &Position) -> String {
        self.highlight_or(name, position, |text| {
            format!(
                "{}{}{}",
                self.separator_begin(position),
                text,
                self.separator_end(position)
            )
        })
    }
    fn resource_name_begin(&self, position: &Position) -> Cow<'static, str>;
    fn resource_name_end(&self, position: &Position) -> Cow<'static, str>;
    fn resource_name(&self, name: &str, position: &Position) -> String {
        self.highlight_or(name, position, |text| {
            format!(
                "{}{}{}",
                self.resource_name_begin(position),
                text,
                self.resource_name_end(position)
            )
        })
    }
    fn action_name_begin(&self, position: &Position) -> Cow<'static, str>;
    fn action_name_end(&self, position: &Position) -> Cow<'static, str>;
    fn action_name(&self, name: &str, position: &Position) -> String {
        self.highlight_or(name, position, |text| {
            format!(
                "{}{}{}",
                self.action_name_begin(position),
                text,
                self.action_name_end(position)
            )
        })
    }
    fn header_begin(&self, position: &Position) -> Cow<'static, str>;
    fn header_end(&self, position: &Position) -> Cow<'static, str>;
    fn header(&self, txt: &str, position: &Position) -> String {
        self.highlight_or(txt, position, |text| {
            format!(
                "{}{}{}",
                self.header_begin(position),
                text,
                self.header_end(position)
            )
        })
    }
    fn highlight_begin(&self) -> Cow<'static, str>;
    fn highlight_end(&self) -> Cow<'static, str>;
    fn highlighted_text(&self, txt: &str) -> String {
        format!("{}{}{}", self.highlight_begin(), txt, self.highlight_end())
    }
}

pub enum StyledQueryToken {
    StringParameter(String),
    Entity(String),
    Separator(String),
    ResourceName(String),
    ActionName(String),
    Header(String),
    Highlight(String),
}

pub struct StyledQuery {
    pub tokens: Vec<StyledQueryToken>,
}

impl StyledQuery {
    pub fn new() -> Self {
        Self { tokens: Vec::new() }
    }
    pub fn from_query<T: QueryRenderer>(x: &T, position: &Position) -> Self {
        let tokens = x.styled_tokens(position).collect();
        StyledQuery { tokens }
    }
}

impl From<Query> for StyledQuery {
    fn from(query: Query) -> Self {
        let tokens = query.styled_tokens(&Position::unknown()).collect();
        StyledQuery { tokens }
    }
}

impl From<&Query> for StyledQuery {
    fn from(query: &Query) -> Self {
        let tokens = query.styled_tokens(&Position::unknown()).collect();
        StyledQuery { tokens }
    }
}

impl From<Key> for StyledQuery {
    fn from(key: Key) -> Self {
        let tokens = key.styled_tokens(&Position::unknown()).collect();
        StyledQuery { tokens }
    }
}

impl From<&Key> for StyledQuery {
    fn from(key: &Key) -> Self {
        let tokens = key.styled_tokens(&Position::unknown()).collect();
        StyledQuery { tokens }
    }
}

impl Display for StyledQuery {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        for token in &self.tokens {
            write!(f, "{}", token.get_text())?;
        }
        Ok(())
    }
}

impl StyledQueryToken {
    pub fn into_text(self) -> String {
        match self {
            StyledQueryToken::StringParameter(s) => s,
            StyledQueryToken::Entity(s) => s,
            StyledQueryToken::Separator(s) => s,
            StyledQueryToken::ResourceName(s) => s,
            StyledQueryToken::ActionName(s) => s,
            StyledQueryToken::Header(s) => s,
            StyledQueryToken::Highlight(s) => s,
        }
    }
    pub fn get_text(&self) -> &str {
        match self {
            StyledQueryToken::StringParameter(s) => s,
            StyledQueryToken::Entity(s) => s,
            StyledQueryToken::Separator(s) => s,
            StyledQueryToken::ResourceName(s) => s,
            StyledQueryToken::ActionName(s) => s,
            StyledQueryToken::Header(s) => s,
            StyledQueryToken::Highlight(s) => s,
        }
    }
    pub fn to_highlight_if_matching(self, p1: &Position, p2: &Position) -> Self {
        if p1.highlight(p2) {
            StyledQueryToken::Highlight(self.into_text())
        } else {
            self
        }
    }
}

pub trait QueryRenderer {
    fn render<S: QueryRenderStyle>(&self, style: &S) -> String;
    fn styled_tokens(&self, position: &Position) -> impl Iterator<Item = StyledQueryToken>;
}

pub struct TrivialQueryRenderStyle;
impl QueryRenderStyle for TrivialQueryRenderStyle {
    fn position(&self) -> &Position {
        &UNKNOWN_POSITION
    }
    fn string_parameter_begin(&self, _position: &Position) -> Cow<'static, str> {
        "".into()
    }
    fn string_parameter_end(&self, _position: &Position) -> Cow<'static, str> {
        "".into()
    }
    fn entity_begin(&self, _position: &Position) -> Cow<'static, str> {
        "".into()
    }
    fn entity_end(&self, _position: &Position) -> Cow<'static, str> {
        "".into()
    }
    fn separator_begin(&self, _position: &Position) -> Cow<'static, str> {
        "".into()
    }
    fn separator_end(&self, _position: &Position) -> Cow<'static, str> {
        "".into()
    }
    fn resource_name_begin(&self, _position: &Position) -> Cow<'static, str> {
        "".into()
    }
    fn resource_name_end(&self, _position: &Position) -> Cow<'static, str> {
        "".into()
    }
    fn action_name_begin(&self, _position: &Position) -> Cow<'static, str> {
        "".into()
    }
    fn action_name_end(&self, _position: &Position) -> Cow<'static, str> {
        "".into()
    }
    fn header_begin(&self, _position: &Position) -> Cow<'static, str> {
        "".into()
    }
    fn header_end(&self, _position: &Position) -> Cow<'static, str> {
        "".into()
    }
    fn highlight_begin(&self) -> Cow<'static, str> {
        "".into()
    }
    fn highlight_end(&self) -> Cow<'static, str> {
        "".into()
    }
}

pub struct DarkAnsiQueryRenderStyle(Position);
impl QueryRenderStyle for DarkAnsiQueryRenderStyle {
    fn position(&self) -> &Position {
        &self.0
    }
    fn string_parameter_begin(&self, _position: &Position) -> Cow<'static, str> {
        ansi!(bg.black yellow).into()
    }
    fn string_parameter_end(&self, _position: &Position) -> Cow<'static, str> {
        ansi!(reset).into()
    }
    fn entity_begin(&self, _position: &Position) -> Cow<'static, str> {
        ansi!(bg.black yellow dim).into()
    }
    fn entity_end(&self, _position: &Position) -> Cow<'static, str> {
        ansi!(reset).into()
    }
    fn separator_begin(&self, _position: &Position) -> Cow<'static, str> {
        ansi!(bg.black white dim).into()
    }
    fn separator_end(&self, _position: &Position) -> Cow<'static, str> {
        ansi!(reset).into()
    }
    fn resource_name_begin(&self, _position: &Position) -> Cow<'static, str> {
        ansi!(bg.black cyan bold).into()
    }
    fn resource_name_end(&self, _position: &Position) -> Cow<'static, str> {
        ansi!(reset).into()
    }
    fn action_name_begin(&self, _position: &Position) -> Cow<'static, str> {
        ansi!(bg.black blue bold).into()
    }
    fn action_name_end(&self, _position: &Position) -> Cow<'static, str> {
        ansi!(reset).into()
    }
    fn header_begin(&self, _position: &Position) -> Cow<'static, str> {
        ansi!(bg.black magenta bold).into()
    }
    fn header_end(&self, _position: &Position) -> Cow<'static, str> {
        ansi!(reset).into()
    }
    fn highlight_begin(&self) -> Cow<'static, str> {
        ansi!(bg.red yellow bold).into()
    }
    fn highlight_end(&self) -> Cow<'static, str> {
        ansi!(reset).into()
    }
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq, Hash)]
/// A source location attached to a parsed query element.
///
/// Parser-produced offsets are zero-based, while line and column values are
/// one-based. [`Position::unknown`] uses zero for every field.
pub struct Position {
    /// Zero-based byte offset in the parsed input.
    pub offset: usize,
    /// One-based line, or zero when unknown.
    pub line: u32,
    /// One-based column, or zero when unknown.
    pub column: usize,
}

#[allow(dead_code)]
impl Position {
    pub fn new(offset: usize, line: u32, column: usize) -> Self {
        Position {
            offset,
            line,
            column,
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
    pub fn or(self, other: Position) -> Position {
        if self.is_unknown() {
            other
        } else {
            self
        }
    }
    /// Returns `true` if the two positions are equal and not unknown.
    pub fn highlight(&self, position: &Position) -> bool {
        if self.is_unknown() {
            return false;
        }
        *self == *position
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

/// Encode a string value for use as an action parameter token.
///
/// Re-exported from [`crate::escape`], which owns the escaping tables in both directions. The
/// path `liquers_core::query::encode_token` is unchanged.
pub use crate::escape::encode_token;

#[derive(Serialize, Deserialize, Debug, Clone)]
/// A value supplied after an action name.
pub enum ActionParameter {
    /// A decoded string parameter value and its source position.
    String(String, Position),
    /// A nested query and its source position.
    ///
    /// Encodes as `~X~<query>~E` and parses back from that form. The linked query is
    /// evaluated and its result supplied as the argument value.
    ///
    /// The resource/transform shorthand is **not** accepted inside a link — write the
    /// explicit `-R/` form. See [`crate::parse`] for the full syntax and the reason.
    ///
    /// ```
    /// use liquers_core::parse::parse_query;
    /// use liquers_core::query::ActionParameter;
    ///
    /// let parsed = &parse_query("greet-~X~greeting~E")?
    ///     .action()
    ///     .expect("action")
    ///     .parameters[0];
    /// assert!(parsed.is_link());
    ///
    /// // A parsed link equals the equivalent programmatically built one.
    /// let built = ActionParameter::new_link(parse_query("greeting")?);
    /// assert_eq!(*parsed, built);
    /// assert_eq!(built.encode(), "~X~greeting~E");
    /// # Ok::<(), liquers_core::error::Error>(())
    /// ```
    Link(Query, Position),
}

#[allow(dead_code)]
impl ActionParameter {
    /// A string parameter holding `parameter` as its **decoded** value.
    ///
    /// The argument is the value itself, not query text: it may contain `~`, `/`, `-`, spaces, or
    /// anything else. [`ActionParameter::encode`] escapes it on the way out.
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

    /// Replace this parameter with a string value.
    ///
    /// `value` is the **decoded** value, like every other path into
    /// [`ActionParameter::String`] — pass the string you mean, not query text. Escaping happens in
    /// [`ActionParameter::encode`] and nowhere else.
    ///
    /// ```
    /// use liquers_core::query::ActionParameter;
    /// let mut p = ActionParameter::new_string(String::new());
    /// p.set_value("12:30");
    /// assert_eq!(p.string_value(), Some("12:30".to_owned()));
    /// assert_eq!(p.encode(), "12~ncolon~30");
    /// ```
    ///
    /// This used to store `encode_token(value)`, so `encode` escaped a second time and
    /// `string_value` returned something other than what was set
    /// (`ACTION-PARAMETER-SET-VALUE-DOUBLE-ENCODES`). That came from a different model, in which a
    /// string parameter was an elementary, already-encoded token; it is rejected because a caller
    /// building a query should not have to know the grammar.
    pub fn set_value(&mut self, value: &str) {
        *self = Self::String(value.to_owned(), Position::unknown())
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

impl QueryRenderer for ActionParameter {
    fn render<S: QueryRenderStyle>(&self, style: &S) -> String {
        match self {
            Self::String(s, position) => {
                let token = encode_token(s);
                style.string_parameter(&token, position)
            }
            Self::Link(query, position) => {
                let entity_begin = style.entity("~X~", position);
                let entity_end = style.entity("~E", position);
                let rendered_query = query.encode(); // Switch to render once ready
                format!("{entity_begin}{rendered_query}{entity_end}")
            }
        }
    }

    fn styled_tokens(&self, position: &Position) -> impl Iterator<Item = StyledQueryToken> {
        match self {
            Self::String(s, p) => {
                let token = StyledQueryToken::StringParameter(encode_token(s))
                    .to_highlight_if_matching(p, position);
                Either::Left(std::iter::once(token))
            }
            Self::Link(query, p) => {
                let begin = StyledQueryToken::Entity("~X~".to_owned())
                    .to_highlight_if_matching(p, position);
                let query_tokens = query.styled_tokens(position);
                let end =
                    StyledQueryToken::Entity("~E".to_owned()).to_highlight_if_matching(p, position);
                Either::Right(
                    std::iter::once(begin)
                        .chain(query_tokens)
                        .chain(std::iter::once(end)),
                )
            }
        }
    }
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
            // Explicit rather than `_ =>`, so a new variant is a compile error here.
            (Self::String(_, _), Self::Link(_, _)) | (Self::Link(_, _), Self::String(_, _)) => {
                false
            }
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
/// One component of a logical resource [`Key`].
///
/// Equality, ordering, and hashing use only [`Self::name`], not the position.
pub struct ResourceName {
    /// Component text.
    pub name: String,
    /// Source position or [`Position::unknown`].
    pub position: Position,
}

impl PartialOrd for ResourceName {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.name.cmp(&other.name))
    }
}

impl Ord for ResourceName {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        self.name.cmp(&other.name)
    }
}

impl PartialEq for ResourceName {
    fn eq(&self, other: &Self) -> bool {
        self.name == other.name
    }
}

impl Eq for ResourceName {}

#[allow(dead_code)]
impl ResourceName {
    /// Creates a resource name with an unknown position.
    pub fn new(name: String) -> Self {
        Self {
            name,
            position: Position::unknown(),
        }
    }
    /// Assigns a source position to the resource name.
    pub fn with_position(self, position: Position) -> Self {
        Self { position, ..self }
    }

    /// Clears the resource name's source position.
    pub fn clean_position(&mut self) {
        self.position = Position::unknown();
    }

    /// Returns `true` if the resource represents the current working directory (`.`).
    pub fn is_cwd(&self) -> bool {
        self.name == "."
    }

    /// Returns `true` if the resource represents the parent directory (`..`).
    pub fn is_parent(&self) -> bool {
        self.name == ".."
    }

    /// Encodes the resource name as a string.
    pub fn encode(&self) -> &str {
        &self.name
    }
    /// Returns the file extension, if present.
    pub fn extension(&self) -> Option<String> {
        if self.name.contains('.') {
            self.name.split(".").last().map(|s| s.to_owned())
        } else {
            None
        }
    }
}

impl QueryRenderer for ResourceName {
    fn render<S: QueryRenderStyle>(&self, style: &S) -> String {
        style.resource_name(&self.name, &self.position)
    }
    fn styled_tokens(&self, position: &Position) -> impl Iterator<Item = StyledQueryToken> {
        std::iter::once(
            StyledQueryToken::ResourceName(self.name.to_owned())
                .to_highlight_if_matching(position, &self.position),
        )
    }
}

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
/// A command name and its ordered parameters within a transform segment.
///
/// Equality and hashing ignore [`Self::position`].
pub struct ActionRequest {
    /// Command name.
    pub name: String,
    /// Ordered decoded parameter values.
    pub parameters: Vec<ActionParameter>,
    /// Source position of the command name.
    pub position: Position,
}

#[allow(dead_code)]
impl ActionRequest {
    pub fn new(name: String) -> ActionRequest {
        ActionRequest {
            name,
            ..Default::default()
        }
    }
    pub fn with_position(self, position: Position) -> Self {
        Self { position, ..self }
    }
    pub fn with_parameters(self, parameters: Vec<ActionParameter>) -> Self {
        Self { parameters, ..self }
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
    pub fn is_q(&self) -> bool {
        self.name == "q"
    }
    pub fn encode(&self) -> String {
        if self.parameters.is_empty() {
            self.name.to_owned()
        } else {
            format!(
                "{}-{}",
                self.name,
                self.parameters.iter().map(|x| x.encode()).join("-")
            )
        }
    }
}

impl QueryRenderer for ActionRequest {
    fn render<S: QueryRenderStyle>(&self, style: &S) -> String {
        let action_name = style.action_name(&self.name, &self.position);
        let sep = style.separator("-", &Position::unknown());
        let parameters = self
            .parameters
            .iter()
            .map(|x| format!("{sep}{}", x.render(style)))
            .collect::<Vec<_>>()
            .join("");
        format!("{action_name}{parameters}")
    }
    fn styled_tokens(&self, position: &Position) -> impl Iterator<Item = StyledQueryToken> {
        let action_token = StyledQueryToken::ActionName(self.name.to_owned())
            .to_highlight_if_matching(position, &self.position);
        let params_tokens = self.parameters.iter().flat_map(|p| {
            std::iter::once(StyledQueryToken::Separator("-".to_owned()))
                .chain(p.styled_tokens(position))
        });
        std::iter::once(action_token).chain(params_tokens)
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

impl Index<usize> for ActionRequest {
    type Output = ActionParameter;

    fn index(&self, index: usize) -> &Self::Output {
        &self.parameters[index]
    }
}

impl IndexMut<usize> for ActionRequest {
    fn index_mut(&mut self, index: usize) -> &mut Self::Output {
        &mut self.parameters[index]
    }
}

#[derive(Serialize, Deserialize, Debug, Clone, Default)]
/// An undecoded parameter belonging to a segment header.
pub struct HeaderParameter {
    /// Parameter text as accepted by the header grammar.
    pub value: String,
    /// Source position.
    pub position: Position,
}

#[allow(dead_code)]
impl HeaderParameter {
    pub fn new(value: String) -> HeaderParameter {
        HeaderParameter {
            value,
            ..Default::default()
        }
    }
    pub fn with_position(self, position: Position) -> Self {
        Self {
            value: self.value,
            position,
        }
    }
    pub fn encode(&self) -> &str {
        &self.value
    }
}

impl QueryRenderer for HeaderParameter {
    fn render<S: QueryRenderStyle>(&self, style: &S) -> String {
        style.string_parameter(&self.value, &self.position)
    }
    fn styled_tokens(&self, position: &Position) -> impl Iterator<Item = StyledQueryToken> {
        std::iter::once(
            StyledQueryToken::StringParameter(self.value.to_owned())
                .to_highlight_if_matching(position, &self.position),
        )
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

/// Header of a resource or transform query segment.
///
/// Resource headers have [`Self::resource`] set. For transform headers, `name`
/// identifies the realm in the single-transform-segment case. Resource-header
/// interpretation is performed by [`crate::plan::PlanBuilder`].
#[derive(Serialize, Deserialize, Debug, Clone, Default)]
pub struct SegmentHeader {
    /// Header name, excluding the `R` resource marker.
    pub name: String,
    /// Level (the number of extra leading hyphens).
    ///
    /// Reserved for a future feature; currently stored and encoded but not
    /// interpreted.
    pub level: usize,
    /// Ordered header parameters. These are not action parameters.
    /// Interpretation of header parameters is performed by [`crate::plan::PlanBuilder`].
    pub parameters: Vec<HeaderParameter>,
    /// Whether this is a resource header.
    pub resource: bool,
    /// Source position of the header.
    pub position: Position,
}

#[allow(dead_code)]
impl SegmentHeader {
    /// Returns `true` if the header contains no data.
    ///
    /// A trivial header has no name, has level zero, and has no parameters. A
    /// resource or transform header can be trivial; triviality does not depend on
    /// the resource flag.
    pub fn is_trivial(&self) -> bool {
        self.name.is_empty() && self.level == 0 && self.parameters.is_empty()
    }

    // Create empty segment header
    // Resource flag is false
    pub fn new() -> SegmentHeader {
        SegmentHeader {
            name: "".to_owned(),
            level: 0,
            parameters: vec![],
            resource: false,
            position: Position::unknown(),
        }
    }
    // Like new, just set the resource flag to true
    pub fn new_resource_header() -> SegmentHeader {
        SegmentHeader {
            name: "".to_owned(),
            level: 0,
            parameters: vec![],
            resource: true,
            position: Position::unknown(),
        }
    }
    pub fn with_position(self, position: Position) -> Self {
        Self { position, ..self }
    }

    pub fn encode(&self) -> String {
        let mut encoded: String = std::iter::repeat_n("-", self.level + 1).collect();
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

impl QueryRenderer for SegmentHeader {
    fn render<S: QueryRenderStyle>(&self, style: &S) -> String {
        let mut head: String = std::iter::repeat_n("-", self.level + 1).collect();
        if self.resource {
            head.push('R');
        }
        if !self.name.is_empty() {
            head.push_str(&style.entity(&self.name, &self.position));
        }
        let mut styled_head = style.header(&head, &self.position);
        if !self.parameters.is_empty() {
            for parameter in self.parameters.iter() {
                styled_head.push_str(&style.separator("-", &Position::unknown()));
                styled_head.push_str(&parameter.render(style));
            }
        }
        styled_head
    }
    fn styled_tokens(&self, position: &Position) -> impl Iterator<Item = StyledQueryToken> {
        let mut head: String = std::iter::repeat_n("-", self.level + 1).collect();
        if self.resource {
            head.push('R');
        }
        if !self.name.is_empty() {
            head.push_str(&self.name);
        }
        let head_token =
            StyledQueryToken::Header(head).to_highlight_if_matching(position, &self.position);
        let params_tokens = self.parameters.iter().flat_map(|p| {
            std::iter::once(StyledQueryToken::Separator("-".to_owned()))
                .chain(p.styled_tokens(position))
        });
        std::iter::once(head_token).chain(params_tokens)
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

/// An ordered sequence of actions applied to an input.
///
/// The input is a [`crate::state::State`], normally the resource or transformation
/// result produced by the preceding query segment. In a transform-only query there
/// is no preceding resource, so the sequence starts without resource input. Actions
/// are evaluated in [`Self::query`] order. [`Self::filename`] is an optional
/// terminal output filename rather than another action. It determines the output
/// filename for the terminal action. Typically filename specifies a file resulting from a recipe.
/// Unless specified otherwise, the output filename extension determines the output format, even if the file is not saved.
/// It is therefore useful to explicitly specify the output filename with the extension e.g. in a web API request.
/// Only the last segment's [`Self::filename`] is used as the terminal output filename.
///
/// See the [`crate::parse`] *String action parameters*, *Segment headers*, and
/// *Query forms and parse precedence* sections for the accepted textual syntax.
#[derive(Serialize, Deserialize, Debug, Clone, Default)]
pub struct TransformQuerySegment {
    /// Explicit header, or `None` for a headerless first transform segment.
    pub header: Option<SegmentHeader>,
    /// Actions applied in order.
    pub query: Vec<ActionRequest>,
    /// Optional terminal output filename.
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

    /// Returns the name of the transform query segment.
    pub fn name(&self) -> String {
        if let Some(header) = &self.header {
            header.name.clone()
        } else {
            "".to_owned()
        }
    }

    pub fn position(&self) -> Position {
        if let Some(header) = &self.header {
            header.position.to_owned()
        } else if self.query.is_empty() {
            if let Some(filename) = &self.filename {
                filename.position.to_owned()
            } else {
                Position::unknown()
            }
        } else {
            self.query[0].position.to_owned()
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
        } else if self.query.is_empty() {
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

    /// Returns `true` if the segment has no actions and no filename.
    ///
    /// The header does not affect whether the segment is empty.
    pub fn is_empty(&self) -> bool {
        self.query.is_empty() && self.filename.is_none()
    }

    /// Returns `true` if the segment contains a filename and no actions.
    pub fn is_filename(&self) -> bool {
        self.query.is_empty() && self.filename.is_some()
    }

    /// Returns `true` if the segment contains exactly one action and no filename.
    pub fn is_action_request(&self) -> bool {
        self.query.len() == 1 && self.filename.is_none()
    }

    /// Returns the action if this segment is an action request.
    ///
    /// See [`Self::is_action_request`].
    pub fn action(&self) -> Option<ActionRequest> {
        if self.is_action_request() {
            Some(self.query[0].clone())
        } else {
            None
        }
    }
    /// Returns `true` if the segment is an `ns` action request.
    pub fn is_ns(&self) -> bool {
        self.action().is_some_and(|x| x.is_ns())
    }
    pub fn ns(&self) -> Option<Vec<ActionParameter>> {
        self.action().and_then(|x| x.ns())
    }
    pub fn last_ns(&self) -> Option<Vec<ActionParameter>> {
        self.query.iter().rev().find_map(|x| x.ns())
    }
    /// Returns `true` if the final action is a `q` instruction.
    pub fn is_q(&self) -> bool {
        self.query.last().is_some_and(|x| x.is_q())
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

    /// Helper function to make a canonical filename
    fn canonical_filename(filename: Option<ResourceName>) -> Option<ResourceName> {
        if let Some(name) = &filename {
            if name.name.starts_with("data.") {
                filename
            } else {
                if let Some(i) = name.name.find('.') {
                    let mut fname = name.name.clone();
                    let ext = fname.split_off(i);
                    Some(ResourceName {
                        name: format!("data{ext}"),
                        position: name.position.clone(),
                    })
                } else {
                    Some(ResourceName {
                        name: "data".to_owned(),
                        position: name.position.clone(),
                    })
                }
            }
        } else {
            None
        }
    }

    /// Removes ambiguity from the transform query by creating a standard form.
    /// The standard form has the same meaning as the original query.
    /// If a query is used as a key (for example, for assets), use the canonical
    /// form to prevent duplicates.
    /// Note that this is done automatically if possible.
    /// There are two potential changes:
    ///
    /// - If there is no header, an unnamed header without arguments is created.
    /// - If a filename exists, its extension is retained because it determines the
    ///   potential format, but its basename is changed to `data`. For example,
    ///   `image.png` becomes `data.png`.
    // TODO: the canonical filename may be a problem - the metadata contains a filename, so assets with different filenames are not equivalent event if the asset value is the same
    pub fn canonical(self) -> Self {
        if self.header.is_none() {
            Self {
                header: Some(SegmentHeader::new()),
                query: self.query,
                filename: Self::canonical_filename(self.filename),
            }
        } else {
            Self {
                header: self.header,
                query: self.query,
                filename: Self::canonical_filename(self.filename),
            }
        }
    }

    /// Returns the number of actions in the segment.
    fn len(&self) -> usize {
        self.query.len()
    }
}

impl QueryRenderer for TransformQuerySegment {
    fn render<S: QueryRenderStyle>(&self, style: &S) -> String {
        let mut styled_query = if let Some(header) = &self.header {
            header.render(style)
        } else {
            String::new()
        };
        for action in self.query.iter() {
            if !styled_query.is_empty() {
                styled_query.push_str(&style.separator("/", &Position::unknown()));
            }
            styled_query.push_str(&action.render(style));
        }

        if let Some(filename) = &self.filename {
            if !styled_query.is_empty() {
                styled_query.push_str(&style.separator("/", &Position::unknown()));
            }
            styled_query.push_str(&filename.render(style));
        }
        styled_query
    }
    fn styled_tokens(&self, position: &Position) -> impl Iterator<Item = StyledQueryToken> {
        let mut tokens = if let Some(header) = &self.header {
            header.styled_tokens(position).collect::<Vec<_>>()
        } else {
            vec![]
        };
        for action in self.query.iter() {
            if !tokens.is_empty() {
                tokens.push(StyledQueryToken::Separator("/".to_owned()));
            }
            tokens.extend(action.styled_tokens(position));
        }
        if let Some(filename) = &self.filename {
            if !tokens.is_empty() {
                tokens.push(StyledQueryToken::Separator("/".to_owned()));
            }
            tokens.extend(filename.styled_tokens(position));
        }
        tokens.into_iter()
    }
}

impl Add for TransformQuerySegment {
    type Output = TransformQuerySegment;

    fn add(self, rhs: Self) -> Self::Output {
        let mut q = self.query.clone();
        q.extend(rhs.query.iter().cloned());
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

impl Index<usize> for TransformQuerySegment {
    type Output = ActionRequest;

    fn index(&self, index: usize) -> &Self::Output {
        &self.query[index]
    }
}

impl IndexMut<usize> for TransformQuerySegment {
    fn index_mut(&mut self, index: usize) -> &mut Self::Output {
        &mut self.query[index]
    }
}

#[derive(Serialize, Deserialize, Debug, Clone, Default, PartialEq, Eq, Hash, PartialOrd, Ord)]
/// A logical resource path represented as ordered resource names.
///
/// Parse textual keys with [`crate::parse::parse_key`]. In general, a key is not
/// an operating-system path; it is a key in an [`crate::store::AsyncStore`].
pub struct Key(pub Vec<ResourceName>);
impl Key {
    /// Creates an empty key.
    pub fn new() -> Self {
        Self(vec![])
    }

    /// Clean the position of all the elements of the key
    fn clean_position(&mut self) {
        for x in self.0.iter_mut() {
            x.clean_position();
        }
    }

    /// Returns `true` if the key is empty.
    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    /// Returns an iterator over the key elements.
    pub fn iter(&self) -> std::slice::Iter<'_, ResourceName> {
        self.0.iter()
    }

    /// Returns the final key element, if present.
    ///
    /// A store typically interprets this element as a filename.
    pub fn filename(&self) -> Option<&ResourceName> {
        self.0.last()
    }

    /// Returns the filename extension, if present.
    pub fn extension(&self) -> Option<String> {
        self.filename().and_then(|x| x.extension())
    }

    /// Returns the number of elements in the key.
    pub fn len(&self) -> usize {
        self.0.len()
    }

    /// Encodes the key as a string.
    pub fn encode(&self) -> String {
        self.0.iter().map(|x| x.encode()).join("/")
    }

    /*
    // Check if the key has a given string prefix.
    pub fn has_prefix<S: AsRef<str>>(&self, prefix: S) -> bool {
        self.encode().starts_with(prefix.as_ref())
    }
    */

    /// Returns `true` if the key has the specified key prefix.
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

    /// Returns a new key containing exactly the first `n` elements.
    ///
    /// Returns `None` if the key has fewer than `n` elements.
    pub fn prefix_of_size(&self, n: usize) -> Option<Self> {
        let mut key = Vec::new();
        if self.len() < n {
            return None;
        }
        for x in self.iter().take(n) {
            key.push(x.clone());
        }
        Some(Key(key))
    }

    /// Appends a name as a new final key element.
    pub fn join<S: AsRef<str>>(&self, name: S) -> Self {
        let mut key = self.clone();
        key.0.push(ResourceName::new(name.as_ref().to_owned()));
        key
    }

    /// Returns the parent key, which omits the final element.
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

    /// Resolves `.` and `..` elements relative to a current working-directory key.
    ///
    /// `cwd_key` should be absolute, meaning that it should not contain `.` or
    /// `..`. This function does not check that condition.
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

impl QueryRenderer for Key {
    fn render<S: QueryRenderStyle>(&self, style: &S) -> String {
        if self.is_empty() {
            "".to_owned()
        } else {
            let first = self[0].render(style);
            let rest = self
                .iter()
                .skip(1)
                .map(|x| {
                    format!(
                        "{}{}",
                        style.separator("/", &Position::unknown()),
                        &x.render(style)
                    )
                })
                .collect::<Vec<_>>()
                .join("");
            format!("{first}{rest}")
        }
    }
    fn styled_tokens(&self, position: &Position) -> impl Iterator<Item = StyledQueryToken> {
        if self.is_empty() {
            Either::Left(std::iter::empty())
        } else {
            Either::Right(
                self[0]
                    .styled_tokens(position)
                    .chain(self.iter().skip(1).flat_map(|x| {
                        std::iter::once(StyledQueryToken::Separator("/".to_owned()))
                            .chain(x.styled_tokens(position))
                    })),
            )
        }
    }
}

impl From<Key> for ResourceQuerySegment {
    fn from(value: Key) -> Self {
        ResourceQuerySegment {
            header: None,
            key: value,
        }
    }
}

impl From<ResourceQuerySegment> for Key {
    fn from(value: ResourceQuerySegment) -> Self {
        value.key
    }
}

impl From<Key> for QuerySegment {
    fn from(value: Key) -> Self {
        QuerySegment::Resource(value.into())
    }
}

impl From<Key> for Query {
    fn from(value: Key) -> Self {
        Query {
            segments: vec![value.into()],
            source: QuerySource::Unspecified,
            absolute: false,
        }
    }
}

impl From<&Key> for Query {
    fn from(value: &Key) -> Self {
        Query {
            segments: vec![value.clone().into()],
            source: QuerySource::Unspecified,
            absolute: false,
        }
    }
}

impl TryFrom<Query> for Key {
    type Error = Error;

    fn try_from(value: Query) -> Result<Self, Self::Error> {
        if let Some(segment) = value.resource_query() {
            Ok(segment.key)
        } else {
            Err(Error::general_error(format!(
                "Query {value} cannot convert to key"
            )))
        }
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

/// A reference to a resource by logical key, with an optional resource header.
///
/// Resource-header parameters may determine how the resource is accessed. It can
/// be read and parsed into a value or read as a binary blob, and the query can
/// return its data, metadata, or key.
///
/// The resource is typically a keyed asset and can be thought of as a file with a
/// logical path.
///
/// See the [`crate::parse`] *Segment headers* and *Query forms and parse
/// precedence* sections for the accepted textual syntax.
#[derive(Serialize, Deserialize, Debug, Clone, Default)]
pub struct ResourceQuerySegment {
    /// Explicit resource header, or `None` when constructed from shorthand.
    pub header: Option<SegmentHeader>,
    /// Logical resource key.
    pub key: Key,
}

#[allow(dead_code)]
impl ResourceQuerySegment {
    /// Creates an empty resource query segment.
    pub fn new() -> ResourceQuerySegment {
        ResourceQuerySegment {
            header: None,
            key: Key::new(),
        }
    }

    /// Returns the name of the resource query segment.
    pub fn name(&self) -> String {
        if let Some(header) = &self.header {
            header.name.clone()
        } else {
            "".to_owned()
        }
    }

    /// Returns the resource query's source position.
    pub fn position(&self) -> Position {
        if let Some(header) = &self.header {
            header.position.to_owned()
        } else if self.key.is_empty() {
            Position::unknown()
        } else {
            self.key[0].position.to_owned()
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

    /// Resolves `.` and `..` elements relative to a current working-directory key.
    ///
    /// This happens regardless of the resource name or other header parameters.
    /// `cwd_key` should be absolute, meaning that it should not contain `.` or
    /// `..`. This function does not check that condition.
    pub fn to_absolute(&self, cwd_key: &Key) -> Self {
        Self {
            header: self.header.clone(),
            key: self.key.to_absolute(cwd_key),
        }
    }

    /// Removes ambiguity from the resource query by creating a standard form.
    /// The standard form has the same meaning as the original query.
    /// If a query is used as a key (for example, for assets), use the canonical
    /// form to prevent duplicates.
    /// Note that this is done automatically if possible.
    /// If there is no header, a header without arguments is created.
    /// It may be useful to call [`Self::to_absolute`] before canonicalizing the
    /// query.
    pub fn canonical(self) -> Self {
        if self.header.is_none() {
            Self {
                key: self.key,
                header: Some(SegmentHeader::new_resource_header()),
            }
        } else {
            self
        }
    }
}

impl QueryRenderer for ResourceQuerySegment {
    fn render<S: QueryRenderStyle>(&self, style: &S) -> String {
        let mut styled_query = if let Some(header) = &self.header {
            header.render(style)
        } else {
            String::new()
        };
        if !self.key.is_empty() {
            if !styled_query.is_empty() {
                styled_query.push_str(&style.separator("/", &Position::unknown()));
            }
            styled_query.push_str(&self.key.render(style));
        }
        styled_query
    }
    fn styled_tokens(&self, position: &Position) -> impl Iterator<Item = StyledQueryToken> {
        let mut tokens = if let Some(header) = &self.header {
            header.styled_tokens(position).collect::<Vec<_>>()
        } else {
            Vec::new()
        };
        if !self.key.is_empty() {
            if !tokens.is_empty() {
                tokens.push(StyledQueryToken::Separator("/".to_owned()));
            }
            tokens.extend(self.key.styled_tokens(position));
        }
        tokens.into_iter()
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

impl Index<usize> for ResourceQuerySegment {
    type Output = ResourceName;

    fn index(&self, index: usize) -> &Self::Output {
        &self.key[index]
    }
}

impl IndexMut<usize> for ResourceQuerySegment {
    fn index_mut(&mut self, index: usize) -> &mut Self::Output {
        &mut self.key[index]
    }
}

/// One resource or transformation segment of a [`Query`].
///
/// See [`crate::parse`] for the syntax that distinguishes the two variants.
#[derive(Serialize, Deserialize, Debug, Clone)]
pub enum QuerySegment {
    /// Select or inspect a logical resource.
    Resource(ResourceQuerySegment),
    /// Apply an ordered sequence of actions.
    Transform(TransformQuerySegment),
}

impl QuerySegment {
    /// Creates an empty transform query segment.
    pub fn empty_transform_query_segment() -> Self {
        QuerySegment::Transform(TransformQuerySegment::new())
    }
    /// Creates an empty resource query segment.
    pub fn empty_resource_query_segment() -> Self {
        QuerySegment::Resource(ResourceQuerySegment::new())
    }

    /// Returns the query segment's source position.
    pub fn position(&self) -> Position {
        match self {
            QuerySegment::Resource(rqs) => rqs.position(),
            QuerySegment::Transform(tqs) => tqs.position(),
        }
    }

    /// Returns the query segment's name.
    pub fn name(&self) -> String {
        match self {
            QuerySegment::Resource(rqs) => rqs.name(),
            QuerySegment::Transform(tqs) => tqs.name(),
        }
    }

    /// Encodes the query segment as a string.
    pub fn encode(&self) -> String {
        match self {
            QuerySegment::Resource(rqs) => rqs.encode(),
            QuerySegment::Transform(tqs) => tqs.encode(),
        }
    }

    /// Encodes the query segment, always including a resource header.
    pub fn encode_with_header(&self) -> String {
        match self {
            QuerySegment::Resource(rqs) => rqs.encode_with_header(),
            QuerySegment::Transform(tqs) => tqs.encode(),
        }
    }

    /// Resolves `.` and `..` elements in a resource segment.
    ///
    /// See [`ResourceQuerySegment::to_absolute`] for details. Transform segments
    /// are returned unchanged.
    pub fn to_absolute(&self, cwd_key: &Key) -> Self {
        match self {
            QuerySegment::Resource(rqs) => QuerySegment::Resource(rqs.to_absolute(cwd_key)),
            QuerySegment::Transform(_) => self.clone(),
        }
    }

    /// Returns the filename, if present.
    pub fn filename(&self) -> Option<ResourceName> {
        match self {
            QuerySegment::Resource(rqs) => rqs.filename().clone(),
            QuerySegment::Transform(tqs) => tqs.filename.clone(),
        }
    }

    /// Returns the number of actions or resource names in the segment.
    pub fn len(&self) -> usize {
        match self {
            QuerySegment::Resource(rqs) => rqs.len(),
            QuerySegment::Transform(tqs) => tqs.len(),
        }
    }

    /// Returns `true` if the segment has no actions or resource names.
    pub fn is_empty(&self) -> bool {
        match self {
            QuerySegment::Resource(rqs) => rqs.is_empty(),
            QuerySegment::Transform(tqs) => tqs.is_empty(),
        }
    }

    /// Returns `true` if the segment is a namespace definition.
    ///
    /// See [`TransformQuerySegment::is_ns`] for details.
    pub fn is_ns(&self) -> bool {
        match self {
            QuerySegment::Resource(_) => false,
            QuerySegment::Transform(tqs) => tqs.is_ns(),
        }
    }
    /// Returns the namespaces if the segment is an `ns` action.
    pub fn ns(&self) -> Option<Vec<ActionParameter>> {
        match self {
            QuerySegment::Resource(_) => None,
            QuerySegment::Transform(tqs) => tqs.ns(),
        }
    }
    /// Returns the final `ns` action's parameters, if present.
    pub fn last_ns(&self) -> Option<Vec<ActionParameter>> {
        match self {
            QuerySegment::Resource(_) => None,
            QuerySegment::Transform(tqs) => tqs.last_ns(),
        }
    }
    /// Returns `true` if the segment is a filename.
    pub fn is_filename(&self) -> bool {
        match self {
            QuerySegment::Resource(rqs) => rqs.is_filename(),
            QuerySegment::Transform(tqs) => tqs.is_filename(),
        }
    }
    /// Returns `true` for a resource query segment.
    pub fn is_resource_query_segment(&self) -> bool {
        match self {
            QuerySegment::Resource(_) => true,
            QuerySegment::Transform(_) => false,
        }
    }
    /// Returns `true` for a transform query segment.
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

    /// Removes ambiguity from the query segment by creating a standard form.
    /// The standard form has the same meaning as the original query.
    /// If a query is used as a key (for example, for assets), use the canonical
    /// form to prevent duplicates.
    /// Note that this is done automatically if possible.
    pub fn canonical(self) -> Self {
        match self {
            QuerySegment::Resource(resource_query_segment) => {
                QuerySegment::Resource(resource_query_segment.canonical())
            }
            QuerySegment::Transform(transform_query_segment) => {
                QuerySegment::Transform(transform_query_segment.canonical())
            }
        }
    }
}

impl QueryRenderer for QuerySegment {
    fn render<S: QueryRenderStyle>(&self, style: &S) -> String {
        match self {
            QuerySegment::Resource(rqs) => rqs.render(style),
            QuerySegment::Transform(tqs) => tqs.render(style),
        }
    }
    fn styled_tokens(&self, position: &Position) -> impl Iterator<Item = StyledQueryToken> {
        match self {
            QuerySegment::Resource(rqs) => Either::Left(rqs.styled_tokens(position)),
            QuerySegment::Transform(tqs) => Either::Right(tqs.styled_tokens(position)),
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

/// Provenance describing where a query was read from.
///
/// This value is not encoded and is ignored by [`Query`] equality and hashing.
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq, Hash, Default)]
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
    #[default]
    Unspecified,
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

/// Root node of the query AST.
///
/// A query is an ordered sequence of resource and transformation query segments.
///
/// A resource segment references a resource, usually a keyed asset. A transform
/// segment represents actions applied to its input, normally the result established
/// by preceding segments, or no resource input when no segment precedes it.
///
/// Accepted text and parse precedence are defined by
/// [`crate::parse::parse_query`] and the [`crate::parse`] module reference.
/// Equality and hashing include `segments` and `absolute`, but ignore `source` and
/// all nested source positions.
#[derive(Serialize, Deserialize, Debug, Clone, Default)]
pub struct Query {
    /// Segments in evaluation order.
    pub segments: Vec<QuerySegment>,
    /// Whether the textual form had a leading `/`.
    ///
    /// This is stored and encoded; it is independent of relative `.` and `..`
    /// resolution by [`Self::to_absolute`].
    pub absolute: bool,
    /// Non-semantic provenance metadata.
    pub source: QuerySource,
}

pub(crate) const RELATIVE_WITHOUT_CWD_WARNING: &str =
    "Relative key/query has no CWD; using logical root '/'.";

/// Pure, ordered working-key state used while resolving query AST copies.
///
/// Runtime ownership of the working key remains with `Context`; this cursor is
/// cloned when entering a linked query so an explicit child `cwd` cannot leak
/// back to its parent.
#[derive(Clone, Default)]
pub(crate) struct CwdCursor {
    cwd: Option<Key>,
    defaulted_to_root: bool,
    /// Set when [`Self::resolve_key`] took its relative branch, i.e. this cursor actually
    /// consumed the CWD rather than passing an absolute operand through.
    ///
    /// Read by the freeze migration assertion: once a plan is frozen, every runtime resolution
    /// should leave this clear, because an absolute key returns early.
    consumed_cwd: bool,
}

#[allow(dead_code)]
impl CwdCursor {
    pub(crate) fn new(cwd: Option<Key>) -> Self {
        Self {
            cwd,
            defaulted_to_root: false,
            consumed_cwd: false,
        }
    }

    /// Whether `key` is expressed relative to a CWD, i.e. it starts with `.` or `..`.
    pub(crate) fn is_relative(key: &Key) -> bool {
        key.0
            .first()
            .is_some_and(|name| name.is_cwd() || name.is_parent())
    }

    fn is_cwd_resource(resource: &ResourceQuerySegment) -> bool {
        resource
            .header
            .as_ref()
            .and_then(|header| header.parameters.first())
            .is_some_and(|parameter| parameter.value == "cwd")
    }

    pub(crate) fn resolve_key(&mut self, key: &Key) -> Key {
        if !Self::is_relative(key) {
            return key.clone();
        }
        self.consumed_cwd = true;

        let cwd = self.cwd.get_or_insert_with(|| {
            self.defaulted_to_root = true;
            Key::new()
        });
        key.to_absolute(cwd)
    }

    pub(crate) fn resolve_query_scoped(&mut self, query: &Query) -> Query {
        let mut resolved = query.clone();
        let mut scoped = self.clone();
        let mut absolute_resource_cursor = query.absolute.then(|| CwdCursor::new(Some(Key::new())));

        for segment in &mut resolved.segments {
            match segment {
                QuerySegment::Resource(resource) => {
                    let is_cwd = Self::is_cwd_resource(resource);
                    if let Some(resource_cursor) = &mut absolute_resource_cursor {
                        let key = if is_cwd {
                            resource_cursor.set_cwd_from(&resource.key)
                        } else {
                            resource_cursor.resolve_key(&resource.key)
                        };
                        resource.key = key.clone();
                        if is_cwd {
                            scoped.cwd = Some(key);
                        }
                    } else if is_cwd {
                        resource.key = scoped.set_cwd_from(&resource.key);
                    } else {
                        resource.key = scoped.resolve_key(&resource.key);
                    }
                }
                QuerySegment::Transform(transform) => {
                    for action in &mut transform.query {
                        for parameter in &mut action.parameters {
                            match parameter {
                                ActionParameter::String(_, _) => {}
                                ActionParameter::Link(link, _) => {
                                    *link = scoped.resolve_query_scoped(link);
                                }
                            }
                        }
                    }
                }
            }
        }

        if self.cwd.is_none() && scoped.defaulted_to_root {
            self.cwd = Some(Key::new());
            self.defaulted_to_root = true;
        }
        // Resolution runs on clones, so consumption observed by either the scoped cursor or the
        // absolute-resource cursor has to be reported back to the caller.
        self.consumed_cwd |= scoped.consumed_cwd
            || absolute_resource_cursor
                .as_ref()
                .is_some_and(|cursor| cursor.consumed_cwd);

        resolved
    }

    pub(crate) fn set_cwd_from(&mut self, key: &Key) -> Key {
        let resolved = self.resolve_key(key);
        self.cwd = Some(resolved.clone());
        resolved
    }

    pub(crate) fn current(&self) -> Option<Key> {
        self.cwd.clone()
    }

    pub(crate) fn take_root_fallback(&mut self) -> bool {
        std::mem::take(&mut self.defaulted_to_root)
    }

    /// Whether any resolution performed by this cursor consumed the CWD, clearing the flag.
    pub(crate) fn take_consumed_cwd(&mut self) -> bool {
        std::mem::take(&mut self.consumed_cwd)
    }

    /// Merges the *diagnostic* flags observed by a scoped child cursor back into this one.
    ///
    /// Deliberately does not merge the working key: a child scope exists precisely so that a
    /// `-R-cwd` inside a link cannot move its parent. Whether the child fell back to logical root,
    /// or consumed a CWD at all, is not scoped — it describes the resolution as a whole, and the
    /// caller owns the single warning that follows from it.
    pub(crate) fn absorb_diagnostics(&mut self, child: &CwdCursor) {
        self.defaulted_to_root |= child.defaulted_to_root;
        self.consumed_cwd |= child.consumed_cwd;
    }
}

#[allow(dead_code)]
impl Query {
    /// Creates an empty query.
    pub fn new() -> Query {
        Query {
            segments: vec![],
            absolute: false,
            source: QuerySource::Unspecified,
        }
    }

    /// Returns the query's source position.
    pub fn position(&self) -> Position {
        if self.segments.is_empty() {
            Position::unknown()
        } else {
            self.segments[0].position()
        }
    }

    /// Returns the filename, if present.
    pub fn filename(&self) -> Option<ResourceName> {
        match self.segments.last() {
            None => None,
            Some(QuerySegment::Transform(tqs)) => tqs.filename.clone(),
            Some(QuerySegment::Resource(rqs)) => rqs.filename(),
        }
    }

    /// Returns the file extension, if present.
    pub fn extension(&self) -> Option<String> {
        self.filename().and_then(|x| x.extension())
    }
    /// Returns `true` if the query has no segments and is equivalent to an empty string.
    pub fn is_empty(&self) -> bool {
        self.segments.is_empty()
    }
    /// Returns `true` if the query is a namespace definition.
    pub fn is_ns(&self) -> bool {
        self.transform_query().is_some_and(|x| x.is_ns())
    }
    /// Returns the namespace definition if the query is a namespace action.
    pub fn ns(&self) -> Option<Vec<ActionParameter>> {
        self.transform_query().and_then(|x| x.ns())
    }

    /// Returns the last namespace definition if available.
    /// Namespace is scoped to the last transform segment only.
    pub fn last_ns(&self) -> Option<Vec<ActionParameter>> {
        if let Some(QuerySegment::Transform(tqs)) = self.segments.last() {
            tqs.last_ns()
        } else {
            None
        }
    }

    /// Returns `true` if the query's final action is a `q` instruction.
    pub fn is_q(&self) -> bool {
        // Check the last segment
        if let Some(QuerySegment::Transform(tqs)) = self.segments.last() {
            tqs.is_q()
        } else {
            false
        }
    }

    /// Returns the final transform query name, if available.
    pub fn last_transform_query_name(&self) -> Option<String> {
        self.transform_query().map(|x| x.name())
    }

    /// Resolves `.` and `..` elements in every resource segment.
    ///
    /// See [`ResourceQuerySegment::to_absolute`] for details.
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

    /// Returns `true` if the query is a pure transformation query; that is, a
    /// sequence of actions.
    ///
    /// Returns true even if the transform query is empty (has no action requests).
    pub fn is_transform_query(&self) -> bool {
        self.segments.len() == 1
            && match &self.segments[0] {
                QuerySegment::Transform(_) => true,
                _ => false,
            }
    }

    /// Returns the transform segment if this is a pure transformation query.
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

    /// Returns `true` if the query is a pure resource query.
    pub fn is_resource_query(&self) -> bool {
        self.segments.len() == 1
            && match &self.segments[0] {
                QuerySegment::Resource(_) => true,
                _ => false,
            }
    }

    /// Returns the resource segment if this is a pure resource query.
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

    /// Returns `true` if the query is a single action request.
    pub fn is_action_request(&self) -> bool {
        self.transform_query()
            .is_some_and(|x| x.is_action_request())
    }

    /// Returns the action if the query is a single action request.
    pub fn action(&self) -> Option<ActionRequest> {
        self.transform_query().and_then(|x| x.action())
    }

    /// Returns `true` if the query is a simple key.
    /// This requires that the resource query segment is present and has no header or a trivial header,
    /// i.e. no name and parameters.
    pub fn is_key(&self) -> bool {
        if let Some(rq) = self.resource_query() {
            rq.header.is_none() || rq.header.as_ref().is_some_and(|x| x.is_trivial())
        } else {
            false
        }
    }

    /// Returns the key if the query has no header or has a trivial header.
    pub fn key(&self) -> Option<Key> {
        if self.is_key() {
            self.header_key()
        } else {
            None
        }
    }

    /// Returns the key if this is a resource query, disregarding its header.
    pub fn header_key(&self) -> Option<Key> {
        if let Some(rq) = self.resource_query() {
            Some(rq.key.clone())
        } else {
            None
        }
    }

    /// Internal function to return a vector of segments up to the last segment.
    fn up_to_last_segment(&self) -> Vec<QuerySegment> {
        let mut seg = vec![];
        self.segments[0..self.segments.len() - 1].clone_into(&mut seg);
        seg
    }

    /// Returns `(predecessor, remainder)`.
    ///
    /// The remainder is the final action or filename, if available. The predecessor
    /// is the query without that remainder, if available.
    /// Whether any resource operand in this query is CWD-relative, including inside link
    /// parameters.
    ///
    /// This is the test that decides whether a query can name an asset on its own. It asks about
    /// **operand form**, not about [`Query::absolute`]: a query with no key operand at all, such as
    /// `greet-Hello`, means the same thing in every directory and is therefore not relative.
    pub fn has_relative_operand(&self) -> bool {
        self.segments.iter().any(|segment| match segment {
            QuerySegment::Resource(resource) => CwdCursor::is_relative(&resource.key),
            QuerySegment::Transform(transform) => transform.query.iter().any(|action| {
                action.parameters.iter().any(|parameter| match parameter {
                    ActionParameter::Link(link, _) => link.has_relative_operand(),
                    ActionParameter::String(_, _) => false,
                })
            }),
        })
    }

    /// Position of the first CWD-relative resource operand, for diagnostics.
    pub(crate) fn first_relative_operand_position(&self) -> Option<Position> {
        for segment in self.segments.iter() {
            match segment {
                QuerySegment::Resource(resource) => {
                    if CwdCursor::is_relative(&resource.key) {
                        return resource
                            .key
                            .0
                            .first()
                            .map(|name| name.position.clone())
                            .or_else(|| resource.header.as_ref().map(|h| h.position.clone()));
                    }
                }
                QuerySegment::Transform(transform) => {
                    for action in transform.query.iter() {
                        for parameter in action.parameters.iter() {
                            if let ActionParameter::Link(link, position) = parameter {
                                if link.has_relative_operand() {
                                    return link
                                        .first_relative_operand_position()
                                        .or_else(|| Some(position.clone()));
                                }
                            }
                        }
                    }
                }
            }
        }
        None
    }

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
                if p.as_ref().is_none_or(|x| x.is_empty()) {
                    (
                        Some(Query {
                            segments: self.up_to_last_segment(),
                            absolute: self.absolute,
                            ..Default::default()
                        }),
                        r.map(QuerySegment::Transform),
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
                        r.map(QuerySegment::Transform),
                    )
                }
            }
        }
    }

    /// Returns all predecessors of the query.
    pub fn all_predecessors(&self) -> Vec<(Option<Query>, Option<QuerySegment>)> {
        let mut result = vec![];
        let mut qp = Some(self);
        let mut qr: Option<QuerySegment> = None;
        let mut buff: Option<Query>;
        while qp.is_some() {
            /*
            eprintln!(
                "qp/qr: {}  {}",
                qp.unwrap().encode(),
                qr.as_ref().map_or("None".to_owned(), |x| x.encode())
            );
            */
            if qp.unwrap().is_empty() {
                break;
            }
            let x = (qp.cloned(), qr.clone());
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
                &r.resource_query().map(QuerySegment::Resource),
            );
        }
        result
    }

    /// Returns the query without its filename.
    pub fn without_filename(self) -> Query {
        if self.filename().is_none() {
            self
        } else if let (Some(p), _) = self.predecessor() {
            p
        } else {
            Query {
                segments: vec![],
                absolute: self.absolute,
                ..Default::default()
            }
        }
    }

    /// Returns a shortened representation containing at most `n` query characters.
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

    /// Encodes the query as a string.
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

    /// Removes ambiguity from the query by creating a standard form.
    /// The standard form has the same meaning as the original query.
    /// If a query is used as a key (for example, for assets), use the canonical
    /// form to prevent duplicates. The absolute flag and source are copied;
    /// effectively, only the segments are transformed.
    pub fn canonical(self) -> Self {
        Self {
            segments: self
                .segments
                .into_iter()
                .map(|seg| seg.canonical())
                .collect(),
            absolute: self.absolute,
            source: self.source,
        }
    }

    pub fn len(&self) -> usize {
        self.segments.len()
    }
}

impl QueryRenderer for Query {
    fn render<S: QueryRenderStyle>(&self, style: &S) -> String {
        if self.segments.is_empty() {
            if self.absolute {
                style.separator("/", &Position::unknown())
            } else {
                "".to_owned()
            }
        } else {
            let first = self.segments[0].render(style);
            let rest = self
                .segments
                .iter()
                .skip(1)
                .map(|x| {
                    format!(
                        "{}{}",
                        style.separator("/", &Position::unknown()),
                        &x.render(style)
                    )
                })
                .collect::<Vec<_>>()
                .join("");
            if self.absolute {
                format!(
                    "{}{}{}",
                    style.separator("/", &Position::unknown()),
                    first,
                    rest
                )
            } else {
                format!("{first}{rest}")
            }
        }
    }
    fn styled_tokens(&self, position: &Position) -> impl Iterator<Item = StyledQueryToken> {
        let first_tokens = if self.segments.is_empty() {
            Either::Left(std::iter::empty())
        } else {
            Either::Right(self.segments[0].styled_tokens(position))
        };
        let rest_tokens = self.segments.iter().skip(1).flat_map(|x| {
            std::iter::once(StyledQueryToken::Separator("/".to_owned()))
                .chain(x.styled_tokens(position))
        });
        if self.absolute {
            Either::Left(
                std::iter::once(StyledQueryToken::Separator("/".to_owned()))
                    .chain(first_tokens)
                    .chain(rest_tokens),
            )
        } else {
            Either::Right(first_tokens.chain(rest_tokens))
        }
    }
}

/// Conversion used by evaluation APIs that accept either query text or a query.
///
/// String implementations call [`crate::parse::parse_query`]; query
/// implementations return the value or a clone without reparsing.
pub trait TryToQuery: std::fmt::Debug + Display + Clone {
    /// Convert to a validated or already constructed [`Query`].
    fn try_to_query(self) -> Result<Query, Error>;
}

impl TryToQuery for &str {
    fn try_to_query(self) -> Result<Query, Error> {
        crate::parse::parse_query(self)
    }
}

impl TryToQuery for String {
    fn try_to_query(self) -> Result<Query, Error> {
        crate::parse::parse_query(&self)
    }
}

impl TryToQuery for &String {
    fn try_to_query(self) -> Result<Query, Error> {
        crate::parse::parse_query(self)
    }
}

impl TryToQuery for Query {
    fn try_to_query(self) -> Result<Query, Error> {
        Ok(self)
    }
}

impl TryToQuery for &Query {
    fn try_to_query(self) -> Result<Query, Error> {
        Ok(self.clone())
    }
}

impl From<&Query> for Query {
    fn from(value: &Query) -> Self {
        value.clone()
    }
}

impl TryFrom<&str> for Query {
    type Error = Error;

    fn try_from(value: &str) -> Result<Self, Self::Error> {
        crate::parse::parse_query(value)
    }
}

impl TryFrom<String> for Query {
    type Error = Error;

    fn try_from(value: String) -> Result<Self, Self::Error> {
        crate::parse::parse_query(&value)
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
    use crate::parse::{self, parse_key, parse_query};

    use super::*;

    fn resource_key(query: &Query, segment: usize) -> &Key {
        match &query.segments[segment] {
            QuerySegment::Resource(resource) => &resource.key,
            QuerySegment::Transform(_) => panic!("expected resource segment"),
        }
    }

    fn link_query(query: &Query, segment: usize, action: usize, parameter: usize) -> &Query {
        match &query.segments[segment] {
            QuerySegment::Transform(transform) => {
                match &transform.query[action].parameters[parameter] {
                    ActionParameter::Link(link, _) => link,
                    ActionParameter::String(_, _) => panic!("expected linked query"),
                }
            }
            QuerySegment::Resource(_) => panic!("expected transform segment"),
        }
    }

    #[test]
    fn cwd_cursor_resolves_only_leading_dot_and_parent() -> Result<(), Box<dyn std::error::Error>> {
        let mut cursor = CwdCursor::new(Some(parse_key("a/b")?));

        assert_eq!(cursor.resolve_key(&parse_key("./c")?).encode(), "a/b/c");
        assert_eq!(cursor.resolve_key(&parse_key("../c")?).encode(), "a/c");
        assert_eq!(
            cursor.resolve_key(&parse_key("plain/./c")?).encode(),
            "plain/./c"
        );
        assert_eq!(
            cursor.resolve_key(&parse_key("plain/../c")?).encode(),
            "plain/../c"
        );
        assert!(!cursor.take_root_fallback());
        Ok(())
    }

    #[test]
    fn cwd_cursor_resolves_ordered_cwd_changes() -> Result<(), Box<dyn std::error::Error>> {
        let mut cursor = CwdCursor::new(Some(parse_key("a/b")?));

        assert_eq!(cursor.set_cwd_from(&parse_key("..")?).encode(), "a");
        assert_eq!(cursor.set_cwd_from(&parse_key("./c")?).encode(), "a/c");
        assert_eq!(cursor.current().expect("current CWD").encode(), "a/c");
        Ok(())
    }

    #[test]
    fn cwd_cursor_missing_relative_base_uses_root_once() -> Result<(), Box<dyn std::error::Error>> {
        let mut cursor = CwdCursor::default();

        assert_eq!(
            RELATIVE_WITHOUT_CWD_WARNING,
            "Relative key/query has no CWD; using logical root '/'."
        );
        assert_eq!(cursor.resolve_key(&parse_key("./one")?).encode(), "one");
        assert_eq!(cursor.current().expect("logical root"), Key::new());
        assert!(cursor.take_root_fallback());
        assert!(!cursor.take_root_fallback());
        assert_eq!(cursor.resolve_key(&parse_key("../two")?).encode(), "two");
        assert!(!cursor.take_root_fallback());
        Ok(())
    }

    #[test]
    fn cwd_cursor_child_root_fallback_updates_parent_and_sibling(
    ) -> Result<(), Box<dyn std::error::Error>> {
        let query = parse_query("/-/action-~X~-R/./one~E-~X~-R/./two~E")?;
        let mut cursor = CwdCursor::default();

        let resolved = cursor.resolve_query_scoped(&query);

        assert_eq!(
            resource_key(link_query(&resolved, 0, 0, 0), 0).encode(),
            "one"
        );
        assert_eq!(
            resource_key(link_query(&resolved, 0, 0, 1), 0).encode(),
            "two"
        );
        assert_eq!(cursor.current().expect("logical root"), Key::new());
        assert!(cursor.take_root_fallback());
        assert!(!cursor.take_root_fallback());
        Ok(())
    }

    #[test]
    fn cwd_cursor_absolute_query_uses_private_root_without_fallback(
    ) -> Result<(), Box<dyn std::error::Error>> {
        let query = parse_query("/-R/./data/-/action-~X~-R/./linked~E")?;
        let mut cursor = CwdCursor::new(Some(parse_key("a/b")?));

        let resolved = cursor.resolve_query_scoped(&query);

        assert!(resolved.absolute);
        assert_eq!(resource_key(&resolved, 0).encode(), "data");
        assert_eq!(
            resource_key(link_query(&resolved, 1, 0, 0), 0).encode(),
            "a/b/linked"
        );
        assert_eq!(cursor.current().expect("original CWD").encode(), "a/b");
        assert!(!cursor.take_root_fallback());
        Ok(())
    }

    #[test]
    fn cwd_cursor_scopes_child_cwd() -> Result<(), Box<dyn std::error::Error>> {
        let query = parse_query("/-/action-~X~-R-cwd/./child/-R/./one~E-~X~-R/./two~E")?;
        let mut cursor = CwdCursor::new(Some(parse_key("a/b")?));

        let resolved = cursor.resolve_query_scoped(&query);
        let first = link_query(&resolved, 0, 0, 0);
        let second = link_query(&resolved, 0, 0, 1);

        assert_eq!(resource_key(first, 0).encode(), "a/b/child");
        assert_eq!(resource_key(first, 1).encode(), "a/b/child/one");
        assert_eq!(resource_key(second, 0).encode(), "a/b/two");
        assert_eq!(cursor.current().expect("parent CWD").encode(), "a/b");
        Ok(())
    }

    #[test]
    fn cwd_cursor_preserves_query_source_and_positions() -> Result<(), Box<dyn std::error::Error>> {
        let mut query = parse_query("/-R/./data/-/action-~X~-R/./child~E")?;
        query.source = QuerySource::Other("outer provenance".to_owned());
        let link = match &mut query.segments[1] {
            QuerySegment::Transform(transform) => match &mut transform.query[0].parameters[0] {
                ActionParameter::Link(link, _) => link,
                ActionParameter::String(_, _) => panic!("expected linked query"),
            },
            QuerySegment::Resource(_) => panic!("expected transform segment"),
        };
        link.source = QuerySource::Key(parse_key("provenance/link")?);

        let outer_resource_position = resource_key(&query, 0)
            .filename()
            .expect("outer resource filename")
            .position
            .clone();
        let (action_position, parameter_position) = match &query.segments[1] {
            QuerySegment::Transform(transform) => (
                transform.query[0].position.clone(),
                transform.query[0].parameters[0].position(),
            ),
            QuerySegment::Resource(_) => panic!("expected transform segment"),
        };
        let linked_resource_position = resource_key(link_query(&query, 1, 0, 0), 0)
            .filename()
            .expect("linked resource filename")
            .position
            .clone();

        let mut cursor = CwdCursor::new(Some(parse_key("a/b")?));
        let resolved = cursor.resolve_query_scoped(&query);

        assert_eq!(resolved.source, query.source);
        assert!(resolved.absolute);
        assert_eq!(
            resource_key(&resolved, 0)
                .filename()
                .expect("resolved outer resource filename")
                .position,
            outer_resource_position
        );
        match &resolved.segments[1] {
            QuerySegment::Transform(transform) => {
                assert_eq!(transform.query[0].position, action_position);
                assert_eq!(
                    transform.query[0].parameters[0].position(),
                    parameter_position
                );
            }
            QuerySegment::Resource(_) => panic!("expected transform segment"),
        }
        let resolved_link = link_query(&resolved, 1, 0, 0);
        assert_eq!(
            resolved_link.source,
            QuerySource::Key(parse_key("provenance/link")?)
        );
        assert_eq!(
            resource_key(resolved_link, 0)
                .filename()
                .expect("resolved linked resource filename")
                .position,
            linked_resource_position
        );
        Ok(())
    }

    #[test]
    fn cwd_cursor_resolves_deep_links_and_long_key_without_reparse(
    ) -> Result<(), Box<dyn std::error::Error>> {
        let mut names =
            vec![ResourceName::new(".".to_owned()).with_position(Position::new(1, 1, 2))];
        names.extend((0..128).map(|index| {
            ResourceName::new(format!("part{index}")).with_position(Position::new(
                index + 2,
                1,
                index + 3,
            ))
        }));
        let final_position = names.last().expect("long key element").position.clone();
        let mut nested = Query {
            segments: vec![QuerySegment::Resource(ResourceQuerySegment {
                header: Some(SegmentHeader::new_resource_header()),
                key: Key(names),
            })],
            source: QuerySource::Other("leaf".to_owned()),
            ..Default::default()
        };

        for depth in 0..32 {
            nested =
                Query {
                    segments: vec![QuerySegment::Transform(TransformQuerySegment {
                        query: vec![ActionRequest::new(format!("level{depth}")).with_parameters(
                            vec![ActionParameter::Link(
                                nested,
                                Position::new(depth + 200, 1, depth + 201),
                            )],
                        )],
                        ..Default::default()
                    })],
                    source: QuerySource::Other(format!("level{depth}")),
                    ..Default::default()
                };
        }

        let mut cursor = CwdCursor::new(Some(parse_key("base")?));
        let resolved = cursor.resolve_query_scoped(&nested);
        let mut leaf = &resolved;
        for depth in (0..32).rev() {
            assert_eq!(leaf.source, QuerySource::Other(format!("level{depth}")));
            leaf = link_query(leaf, 0, 0, 0);
        }
        assert_eq!(leaf.source, QuerySource::Other("leaf".to_owned()));
        assert_eq!(resource_key(leaf, 0).len(), 129);
        assert_eq!(resource_key(leaf, 0)[0].name, "base");
        assert_eq!(resource_key(leaf, 0)[128].position, final_position);
        assert!(!cursor.take_root_fallback());
        Ok(())
    }

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
        assert_eq!(ap.render(&TrivialQueryRenderStyle), "~X~hello~E");
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
        assert_eq!(a.render(&TrivialQueryRenderStyle), "action");
        assert_eq!(
            a.styled_tokens(&Position::unknown())
                .map(|t| t.into_text())
                .collect::<Vec<_>>()
                .concat(),
            "action"
        );
        let a = ActionRequest::new("action1".to_owned());
        assert_eq!(a.encode(), "action1");
        assert_eq!(a.render(&TrivialQueryRenderStyle), "action1");
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
        assert_eq!(
            a.render(&TrivialQueryRenderStyle),
            "action-~X~hello~E-world"
        );
        assert_eq!(
            a.styled_tokens(&Position::unknown())
                .map(|t| t.into_text())
                .collect::<Vec<_>>()
                .concat(),
            "action-~X~hello~E-world"
        );

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
        assert_eq!(
            a.render(&TrivialQueryRenderStyle),
            "action1-~X~hello~E-world"
        );
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
        assert_eq!(q.render(&TrivialQueryRenderStyle), "action/file.txt");
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

    #[test]
    fn test_encode_token() -> Result<(), Box<dyn std::error::Error>> {
        // Test tilde escaping: ~ -> ~~
        assert_eq!(encode_token("~"), "~~");
        assert_eq!(encode_token("hello~world"), "hello~~world");

        // Test space escaping: space -> ~.
        assert_eq!(encode_token(" "), "~.");
        assert_eq!(encode_token("hello world"), "hello~.world");

        // Test slash escaping: / -> ~/
        assert_eq!(encode_token("/"), "~/");
        assert_eq!(encode_token("path/to/file"), "path~/to~/file");

        // Test minus followed by digit: -<digit> -> ~<digit>
        assert_eq!(encode_token("-1"), "~1");
        assert_eq!(encode_token("-9"), "~9");
        assert_eq!(encode_token("value-123"), "value~123");
        assert_eq!(encode_token("-0something"), "~0something");

        // Test minus not followed by digit: - -> ~_
        assert_eq!(encode_token("-"), "~_");
        assert_eq!(encode_token("hello-world"), "hello~_world");
        assert_eq!(encode_token("-abc"), "~_abc");
        assert_eq!(encode_token("test-"), "test~_");

        // Test normal characters remain unchanged
        assert_eq!(encode_token("hello"), "hello");
        assert_eq!(encode_token("abc123"), "abc123");
        assert_eq!(encode_token("test.txt"), "test.txt");

        // Test complex combinations
        assert_eq!(
            encode_token("hello world/path-123"),
            "hello~.world~/path~123"
        );
        assert_eq!(encode_token("~test -5 file/name"), "~~test~.~5~.file~/name");
        assert_eq!(encode_token("value-abc"), "value~_abc");
        assert_eq!(encode_token(""), "");

        Ok(())
    }

    #[test]
    fn test_canonical() -> Result<(), Box<dyn std::error::Error>> {
        assert_eq!(parse_query("hello")?.encode(), "hello");
        assert_eq!(parse_query("hello")?.canonical().encode(), "-/hello");
        assert_eq!(
            parse_query("hello/worl.txt")?.canonical().encode(),
            "-/hello/data.txt"
        );
        assert_eq!(
            parse_query("-R/xxx/yyy/-/hello/world.txt")?
                .canonical()
                .encode(),
            "-R/xxx/yyy/-/hello/data.txt"
        );
        let q = parse_query("-Rname-key/xxx/yyy/-/hello-abc-123/xxx-yyy/world.txt")?;
        let position = q[1].position();
        eprintln!("Colored: {}", q.render(&DarkAnsiQueryRenderStyle(position)));
        let position = q[1].transform_query_segment().unwrap().query[0]
            .position
            .clone();
        eprintln!("Colored: {}", q.render(&DarkAnsiQueryRenderStyle(position)));
        let position = q[1].transform_query_segment().unwrap().query[0].parameters[1].position();
        eprintln!("Colored: {}", q.render(&DarkAnsiQueryRenderStyle(position)));
        let position = q[1]
            .transform_query_segment()
            .unwrap()
            .filename
            .as_ref()
            .unwrap()
            .position
            .clone();
        eprintln!("Colored: {}", q.render(&DarkAnsiQueryRenderStyle(position)));
        Ok(())
    }
}
