//! The query template AST: core's query AST with a placeholder ([`Slot`]) wherever a core node
//! holds one token. Equality ignores positions and provenance, as core's does.

use std::hash::{Hash, Hasher};

use liquers_core::error::Error;
use liquers_core::query::{
    ActionParameter, ActionRequest, Key, Position, Query, QuerySegment, QuerySource, ResourceName,
    ResourceQuerySegment, SegmentHeader, TransformQuerySegment,
};
use serde::{Deserialize, Serialize};

use crate::name::Name;

/// A query with placeholders.
#[derive(Serialize, Deserialize, Debug, Clone, Default)]
pub struct QueryTemplate {
    pub segments: Vec<QuerySegmentTemplate>,
    pub rooted: bool,
    pub source: QuerySource,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub enum QuerySegmentTemplate {
    Resource(ResourceQuerySegmentTemplate),
    Transform(TransformQuerySegmentTemplate),
}

/// A resource segment; its header takes no placeholders.
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct ResourceQuerySegmentTemplate {
    pub header: Option<SegmentHeader>,
    pub key: KeyTemplate,
}

/// A key, one token per segment (P4).
#[derive(Serialize, Deserialize, Debug, Clone, Default)]
pub struct KeyTemplate(pub Vec<TokenTemplate>);

/// A transform segment; its header takes no placeholders, its filename may (P5).
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct TransformQuerySegmentTemplate {
    pub header: Option<SegmentHeader>,
    pub query: Vec<ActionRequestTemplate>,
    pub filename: Option<TokenTemplate>,
}

/// An action: its name may hold placeholders (P3), and so may its parameters (P1, P2 for `ns`).
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct ActionRequestTemplate {
    pub name: TokenTemplate,
    pub parameters: Vec<ActionParameterTemplate>,
    pub position: Position,
}

/// A parameter: decoded text with placeholders (P1), or a link (P6).
#[derive(Serialize, Deserialize, Debug, Clone)]
pub enum ActionParameterTemplate {
    String(Vec<Piece>, Position),
    Link(QueryTemplate, Position),
}

/// A name, segment or filename: literal pieces and placeholders.
#[derive(Serialize, Deserialize, Debug, Clone, Default)]
pub struct TokenTemplate {
    pub pieces: Vec<Piece>,
    pub position: Position,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub enum Piece {
    /// Decoded literal text.
    Literal(String),
    Slot(Slot),
}

/// A placeholder `{{name}}`.
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct Slot {
    pub name: Name,
    pub position: Position,
}

impl PartialEq for QueryTemplate {
    fn eq(&self, other: &Self) -> bool {
        self.segments == other.segments && self.rooted == other.rooted
    }
}
impl Eq for QueryTemplate {}
impl Hash for QueryTemplate {
    fn hash<H: Hasher>(&self, state: &mut H) {
        // Equal templates encode equally, so the canonical text is a valid hash.
        self.encode().hash(state);
    }
}

impl PartialEq for QuerySegmentTemplate {
    fn eq(&self, other: &Self) -> bool {
        match (self, other) {
            (QuerySegmentTemplate::Resource(a), QuerySegmentTemplate::Resource(b)) => a == b,
            (QuerySegmentTemplate::Transform(a), QuerySegmentTemplate::Transform(b)) => a == b,
            (QuerySegmentTemplate::Resource(_), QuerySegmentTemplate::Transform(_))
            | (QuerySegmentTemplate::Transform(_), QuerySegmentTemplate::Resource(_)) => false,
        }
    }
}
impl Eq for QuerySegmentTemplate {}

impl PartialEq for ResourceQuerySegmentTemplate {
    fn eq(&self, other: &Self) -> bool {
        self.header == other.header && self.key == other.key
    }
}
impl Eq for ResourceQuerySegmentTemplate {}

impl PartialEq for KeyTemplate {
    fn eq(&self, other: &Self) -> bool {
        self.0 == other.0
    }
}
impl Eq for KeyTemplate {}

impl PartialEq for TransformQuerySegmentTemplate {
    fn eq(&self, other: &Self) -> bool {
        self.header == other.header && self.query == other.query && self.filename == other.filename
    }
}
impl Eq for TransformQuerySegmentTemplate {}

impl PartialEq for ActionRequestTemplate {
    fn eq(&self, other: &Self) -> bool {
        self.name == other.name && self.parameters == other.parameters
    }
}
impl Eq for ActionRequestTemplate {}

impl PartialEq for ActionParameterTemplate {
    fn eq(&self, other: &Self) -> bool {
        match (self, other) {
            (ActionParameterTemplate::String(a, _), ActionParameterTemplate::String(b, _)) => a == b,
            (ActionParameterTemplate::Link(a, _), ActionParameterTemplate::Link(b, _)) => a == b,
            (ActionParameterTemplate::String(..), ActionParameterTemplate::Link(..))
            | (ActionParameterTemplate::Link(..), ActionParameterTemplate::String(..)) => false,
        }
    }
}
impl Eq for ActionParameterTemplate {}

impl PartialEq for TokenTemplate {
    fn eq(&self, other: &Self) -> bool {
        self.pieces == other.pieces
    }
}
impl Eq for TokenTemplate {}

impl PartialEq for Piece {
    fn eq(&self, other: &Self) -> bool {
        match (self, other) {
            (Piece::Literal(a), Piece::Literal(b)) => a == b,
            (Piece::Slot(a), Piece::Slot(b)) => a == b,
            (Piece::Literal(_), Piece::Slot(_)) | (Piece::Slot(_), Piece::Literal(_)) => false,
        }
    }
}
impl Eq for Piece {}

impl PartialEq for Slot {
    fn eq(&self, other: &Self) -> bool {
        self.name == other.name
    }
}
impl Eq for Slot {}

impl TokenTemplate {
    /// One literal piece (none for empty text).
    pub fn literal(text: &str, position: Position) -> Self {
        TokenTemplate {
            pieces: literal_pieces(text),
            position,
        }
    }

    /// The text, when the token holds no placeholder.
    pub fn as_literal(&self) -> Option<String> {
        pieces_literal(&self.pieces)
    }

    /// Whether the token is exactly one placeholder.
    pub fn whole_slot(&self) -> Option<&Slot> {
        whole_slot(&self.pieces)
    }
}

pub(crate) fn literal_pieces(text: &str) -> Vec<Piece> {
    if text.is_empty() {
        Vec::new()
    } else {
        vec![Piece::Literal(text.to_string())]
    }
}

pub(crate) fn pieces_literal(pieces: &[Piece]) -> Option<String> {
    let mut out = String::new();
    for piece in pieces {
        match piece {
            Piece::Literal(t) => out.push_str(t),
            Piece::Slot(_) => return None,
        }
    }
    Some(out)
}

pub(crate) fn whole_slot(pieces: &[Piece]) -> Option<&Slot> {
    match pieces {
        [Piece::Slot(slot)] => Some(slot),
        _ => None,
    }
}

impl From<&Query> for QueryTemplate {
    /// Lifts a query: every token becomes one literal piece.
    fn from(query: &Query) -> Self {
        crate::query::parse::lift(query, &[])
    }
}

/// How a slot is written when a template is turned back into a query.
pub(crate) trait SlotFill {
    fn fill(&mut self, slot: &Slot) -> Result<String, Error>;
}

impl QueryTemplate {
    /// The canonical text, with placeholders as `{{name}}`.
    pub fn encode(&self) -> String {
        self.encode_with("{{", "}}")
    }

    /// The canonical text, with placeholders written with the given delimiters.
    pub fn encode_with(&self, open: &str, close: &str) -> String {
        let mut literals = String::new();
        self.collect_literals(&mut literals);
        let base = crate::query::parse::sentinel_base(&literals);
        let mut names = Vec::new();
        struct Sentinels<'n> {
            base: &'n str,
            names: &'n mut Vec<String>,
        }
        impl SlotFill for Sentinels<'_> {
            fn fill(&mut self, slot: &Slot) -> Result<String, Error> {
                let i = self.names.len();
                self.names.push(slot.name.encode());
                Ok(crate::query::parse::sentinel(self.base, i))
            }
        }
        let mut fill = Sentinels {
            base: &base,
            names: &mut names,
        };
        let text = match self.build(&mut fill) {
            Ok(query) => query.encode(),
            Err(_) => return String::new(),
        };
        // Replace the highest indices first, so `…1q` never matches inside `…12q`.
        let mut out = text;
        for (i, name) in names.iter().enumerate().rev() {
            out = out.replace(
                &crate::query::parse::sentinel(&base, i),
                &format!("{open}{name}{close}"),
            );
        }
        out
    }

    /// The query, when there is no placeholder.
    pub fn try_into_query(&self) -> Option<Query> {
        struct Refuse;
        impl SlotFill for Refuse {
            fn fill(&mut self, slot: &Slot) -> Result<String, Error> {
                Err(Error::general_error(format!("placeholder {{{{{}}}}}", slot.name)))
            }
        }
        self.build(&mut Refuse).ok()
    }

    /// Every placeholder, in order, links included.
    pub fn slots(&self) -> impl Iterator<Item = &Slot> {
        let mut slots = Vec::new();
        self.collect_slots(&mut slots);
        slots.into_iter()
    }

    fn collect_slots<'s>(&'s self, out: &mut Vec<&'s Slot>) {
        fn pieces<'s>(pieces: &'s [Piece], out: &mut Vec<&'s Slot>) {
            for piece in pieces {
                match piece {
                    Piece::Literal(_) => {}
                    Piece::Slot(slot) => out.push(slot),
                }
            }
        }
        for segment in &self.segments {
            match segment {
                QuerySegmentTemplate::Resource(r) => {
                    for token in &r.key.0 {
                        pieces(&token.pieces, out);
                    }
                }
                QuerySegmentTemplate::Transform(t) => {
                    for action in &t.query {
                        pieces(&action.name.pieces, out);
                        for parameter in &action.parameters {
                            match parameter {
                                ActionParameterTemplate::String(p, _) => pieces(p, out),
                                ActionParameterTemplate::Link(link, _) => link.collect_slots(out),
                            }
                        }
                    }
                    if let Some(filename) = &t.filename {
                        pieces(&filename.pieces, out);
                    }
                }
            }
        }
    }

    fn collect_literals(&self, out: &mut String) {
        fn pieces(pieces: &[Piece], out: &mut String) {
            for piece in pieces {
                match piece {
                    Piece::Literal(t) => out.push_str(t),
                    Piece::Slot(_) => {}
                }
            }
        }
        for segment in &self.segments {
            match segment {
                QuerySegmentTemplate::Resource(r) => {
                    for token in &r.key.0 {
                        pieces(&token.pieces, out);
                    }
                }
                QuerySegmentTemplate::Transform(t) => {
                    for action in &t.query {
                        pieces(&action.name.pieces, out);
                        for parameter in &action.parameters {
                            match parameter {
                                ActionParameterTemplate::String(p, _) => pieces(p, out),
                                ActionParameterTemplate::Link(link, _) => link.collect_literals(out),
                            }
                        }
                    }
                    if let Some(filename) = &t.filename {
                        pieces(&filename.pieces, out);
                    }
                }
            }
        }
    }

    /// Builds a core query, writing each slot as `fill` says.
    pub(crate) fn build(&self, fill: &mut dyn SlotFill) -> Result<Query, Error> {
        fn text(pieces: &[Piece], fill: &mut dyn SlotFill) -> Result<String, Error> {
            let mut out = String::new();
            for piece in pieces {
                match piece {
                    Piece::Literal(t) => out.push_str(t),
                    Piece::Slot(slot) => out.push_str(&fill.fill(slot)?),
                }
            }
            Ok(out)
        }
        let mut segments = Vec::with_capacity(self.segments.len());
        for segment in &self.segments {
            segments.push(match segment {
                QuerySegmentTemplate::Resource(r) => {
                    let mut names = Vec::with_capacity(r.key.0.len());
                    for token in &r.key.0 {
                        names.push(
                            ResourceName::new(text(&token.pieces, fill)?)
                                .with_position(token.position.clone()),
                        );
                    }
                    QuerySegment::Resource(ResourceQuerySegment {
                        header: r.header.clone(),
                        key: Key(names),
                    })
                }
                QuerySegmentTemplate::Transform(t) => {
                    let mut actions = Vec::with_capacity(t.query.len());
                    for action in &t.query {
                        let mut parameters = Vec::with_capacity(action.parameters.len());
                        for parameter in &action.parameters {
                            parameters.push(match parameter {
                                ActionParameterTemplate::String(p, pos) => {
                                    ActionParameter::String(text(p, fill)?, pos.clone())
                                }
                                ActionParameterTemplate::Link(link, pos) => {
                                    ActionParameter::Link(link.build(fill)?, pos.clone())
                                }
                            });
                        }
                        actions.push(ActionRequest {
                            name: text(&action.name.pieces, fill)?,
                            parameters,
                            position: action.position.clone(),
                        });
                    }
                    let filename = match &t.filename {
                        Some(f) => Some(
                            ResourceName::new(text(&f.pieces, fill)?).with_position(f.position.clone()),
                        ),
                        None => None,
                    };
                    QuerySegment::Transform(TransformQuerySegment {
                        header: t.header.clone(),
                        query: actions,
                        filename,
                    })
                }
            });
        }
        Ok(Query {
            segments,
            rooted: self.rooted,
            source: self.source.clone(),
        })
    }
}

impl std::fmt::Display for QueryTemplate {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.encode())
    }
}
