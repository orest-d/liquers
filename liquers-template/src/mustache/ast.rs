//! The Mustache template AST. Every element keeps a [`Position`]; equality ignores positions.
//! A tag whose content does not parse is kept as an [`Invalid`] node holding its source, so
//! `encode()` still round-trips.

use liquers_core::error::Error;
use liquers_core::query::{Key, Position, QuerySource};
use serde::{Deserialize, Serialize};

use crate::name::Name;
use crate::query::ast::QueryTemplate;

/// A parsed template; one variant per syntax.
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
pub enum Template {
    Mustache(MustacheTemplate),
}

impl Template {
    /// Every recoverable syntax error, in order.
    pub fn errors(&self) -> Vec<&Error> {
        match self {
            Template::Mustache(m) => m.errors(),
        }
    }

    /// The canonical text.
    pub fn encode(&self) -> String {
        match self {
            Template::Mustache(m) => m.encode(),
        }
    }
}

impl std::fmt::Display for Template {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.encode())
    }
}

#[derive(Serialize, Deserialize, Debug, Clone, Default)]
pub struct MustacheTemplate {
    pub elements: Vec<Element>,
    pub source: QuerySource,
}

impl PartialEq for MustacheTemplate {
    fn eq(&self, other: &Self) -> bool {
        self.elements == other.elements
    }
}

impl MustacheTemplate {
    /// Every recoverable syntax error, in order.
    pub fn errors(&self) -> Vec<&Error> {
        let mut out = Vec::new();
        collect_errors(&self.elements, &mut out);
        out
    }

    /// The canonical text.
    pub fn encode(&self) -> String {
        crate::mustache::encode::encode(self)
    }
}

fn collect_errors<'a>(elements: &'a [Element], out: &mut Vec<&'a Error>) {
    for element in elements {
        match element {
            Element::Invalid(i) => out.push(&i.error),
            Element::Section(s) => {
                match &s.head {
                    SectionHead::Invalid(i) => out.push(&i.error),
                    SectionHead::Query(q) => query_errors(q, out),
                    SectionHead::Name(_) => {}
                }
                collect_errors(&s.elements, out);
            }
            Element::Partial(p) => match &p.target {
                PartialTarget::Invalid(i) => out.push(&i.error),
                PartialTarget::Query(q) => query_errors(q, out),
                PartialTarget::Key(_) => {}
            },
            Element::Query(q) => query_errors(q, out),
            Element::Text(_) | Element::Variable(_) | Element::Comment(_) | Element::SetDelimiter(_) => {}
        }
    }
}

fn query_errors<'a>(tag: &'a QueryTag, out: &mut Vec<&'a Error>) {
    match &tag.body {
        QueryBody::Invalid(i) => out.push(&i.error),
        QueryBody::Hygienic(_) | QueryBody::Classic(_) => {}
    }
}

/// A recoverable syntax error: the tag's end is known, its content does not parse.
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct Invalid {
    /// The text as written, which `encode()` writes back.
    pub source: String,
    pub error: Error,
    pub position: Position,
}

impl PartialEq for Invalid {
    fn eq(&self, other: &Self) -> bool {
        self.source == other.source
    }
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub enum Element {
    Text(TextElement),
    Variable(VariableTag),
    Section(Section),
    Partial(PartialTag),
    Query(QueryTag),
    Comment(CommentTag),
    SetDelimiter(SetDelimiterTag),
    Invalid(Invalid),
}

impl PartialEq for Element {
    fn eq(&self, other: &Self) -> bool {
        match (self, other) {
            (Element::Text(a), Element::Text(b)) => a == b,
            (Element::Variable(a), Element::Variable(b)) => a == b,
            (Element::Section(a), Element::Section(b)) => a == b,
            (Element::Partial(a), Element::Partial(b)) => a == b,
            (Element::Query(a), Element::Query(b)) => a == b,
            (Element::Comment(a), Element::Comment(b)) => a == b,
            (Element::SetDelimiter(a), Element::SetDelimiter(b)) => a == b,
            (Element::Invalid(a), Element::Invalid(b)) => a == b,
            (Element::Text(_), _)
            | (Element::Variable(_), _)
            | (Element::Section(_), _)
            | (Element::Partial(_), _)
            | (Element::Query(_), _)
            | (Element::Comment(_), _)
            | (Element::SetDelimiter(_), _)
            | (Element::Invalid(_), _) => false,
        }
    }
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct TextElement {
    pub text: String,
    pub position: Position,
}

impl PartialEq for TextElement {
    fn eq(&self, other: &Self) -> bool {
        self.text == other.text
    }
}

/// `{{x}}`, `{{{x}}}`, `{{&x}}`.
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct VariableTag {
    pub name: Name,
    pub escape: Escape,
    pub position: Position,
}

impl PartialEq for VariableTag {
    fn eq(&self, other: &Self) -> bool {
        self.name == other.name && self.escape == other.escape
    }
}

/// How an interpolation is written; `Triple` and `Ampersand` are both raw, kept apart for
/// `encode()`.
#[derive(Serialize, Deserialize, Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Escape {
    Escaped,
    Triple,
    Ampersand,
}

/// `{{#head}}…{{/}}`, or inverted `{{^head}}…{{/}}`.
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct Section {
    pub inverted: bool,
    pub head: SectionHead,
    pub elements: Vec<Element>,
    pub close: CloseTag,
    pub standalone: Option<Standalone>,
    pub position: Position,
}

impl PartialEq for Section {
    fn eq(&self, other: &Self) -> bool {
        self.inverted == other.inverted
            && self.head == other.head
            && self.elements == other.elements
            && self.close == other.close
            && self.standalone == other.standalone
    }
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
pub enum SectionHead {
    Name(Name),
    Query(QueryTag),
    Invalid(Invalid),
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct CloseTag {
    pub form: CloseForm,
    pub standalone: Option<Standalone>,
    pub position: Position,
}

impl PartialEq for CloseTag {
    fn eq(&self, other: &Self) -> bool {
        self.form == other.form && self.standalone == other.standalone
    }
}

/// `{{/}}`, `{{/x}}`, `{{/$}}`, `{{/$$}}`.
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
pub enum CloseForm {
    Bare,
    Name(Name),
    Hygienic,
    Classic,
}

/// `{{> key}}`, `{{>$ q}}`, `{{>$$ q}}`.
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct PartialTag {
    pub target: PartialTarget,
    pub standalone: Option<Standalone>,
    pub position: Position,
}

impl PartialEq for PartialTag {
    fn eq(&self, other: &Self) -> bool {
        self.target == other.target && self.standalone == other.standalone
    }
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
pub enum PartialTarget {
    Key(Key),
    Query(QueryTag),
    Invalid(Invalid),
}

/// `{{$ q}}`, `{{&$ q}}`, `{{$$ q}}`, `{{$. q}}`, `{{$.name q}}`, …
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct QueryTag {
    pub body: QueryBody,
    pub apply: Option<ApplyTarget>,
    pub escape: Escape,
    pub position: Position,
}

impl PartialEq for QueryTag {
    fn eq(&self, other: &Self) -> bool {
        self.body == other.body && self.apply == other.apply && self.escape == other.escape
    }
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
pub enum QueryBody {
    /// `$`: placeholders are filled in the AST.
    Hygienic(QueryTemplate),
    /// `$$`: the text is expanded, then parsed.
    Classic(ClassicQuery),
    /// A tag content that does not parse, from its `$` on.
    Invalid(Invalid),
}

impl QueryBody {
    pub fn is_classic(&self) -> bool {
        match self {
            QueryBody::Classic(_) => true,
            QueryBody::Hygienic(_) => false,
            QueryBody::Invalid(i) => i.source.starts_with("$$"),
        }
    }
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct ClassicQuery {
    pub pieces: Vec<ClassicPiece>,
    pub position: Position,
}

impl PartialEq for ClassicQuery {
    fn eq(&self, other: &Self) -> bool {
        self.pieces == other.pieces
    }
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub enum ClassicPiece {
    Text(String, Position),
    Slot(crate::query::ast::Slot),
}

impl PartialEq for ClassicPiece {
    fn eq(&self, other: &Self) -> bool {
        match (self, other) {
            (ClassicPiece::Text(a, _), ClassicPiece::Text(b, _)) => a == b,
            (ClassicPiece::Slot(a), ClassicPiece::Slot(b)) => a == b,
            (ClassicPiece::Text(..), ClassicPiece::Slot(_))
            | (ClassicPiece::Slot(_), ClassicPiece::Text(..)) => false,
        }
    }
}

/// `{{$. q}}` applies the query to the current context, `{{$.a.b q}}` to a named value.
#[derive(Serialize, Deserialize, Debug, Clone)]
pub enum ApplyTarget {
    Current(Position),
    Name(Name),
}

impl PartialEq for ApplyTarget {
    fn eq(&self, other: &Self) -> bool {
        match (self, other) {
            (ApplyTarget::Current(_), ApplyTarget::Current(_)) => true,
            (ApplyTarget::Name(a), ApplyTarget::Name(b)) => a == b,
            (ApplyTarget::Current(_), ApplyTarget::Name(_))
            | (ApplyTarget::Name(_), ApplyTarget::Current(_)) => false,
        }
    }
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct CommentTag {
    pub text: String,
    pub standalone: Option<Standalone>,
    pub position: Position,
}

impl PartialEq for CommentTag {
    fn eq(&self, other: &Self) -> bool {
        self.text == other.text && self.standalone == other.standalone
    }
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct SetDelimiterTag {
    pub open: String,
    pub close: String,
    pub standalone: Option<Standalone>,
    pub position: Position,
}

impl PartialEq for SetDelimiterTag {
    fn eq(&self, other: &Self) -> bool {
        self.open == other.open && self.close == other.close && self.standalone == other.standalone
    }
}

/// The line around a standalone tag, which rendering removes and `encode()` restores.
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq, Hash, Default)]
pub struct Standalone {
    /// Whitespace before the tag.
    pub indent: String,
    /// Whitespace after the tag and the line ending (empty at the end of the text).
    pub newline: String,
}
