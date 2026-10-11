//! The built-in search language: its AST, its grammar and its parser.
//!
//! ```text
//! expr       = ws , or_expr , ws ;
//! or_expr    = and_expr , { ws , "|" , ws , and_expr } ;
//! and_expr   = unary , { ws1 , unary } ;
//! unary      = "-" , unary | atom ;
//! atom       = group | field_test | phrase | regex | term ;
//! group      = "(" , expr , ")" ;
//! field_test = field_path , ":" , ( group | regex | [ cmp_op ] , ( phrase | value ) ) ;
//! field_path = name , { "." , name } ;
//! cmp_op     = ">=" | "<=" | ">" | "<" ;
//! ```
//!
//! A regex is `/body/`, recognised only at the start of an atom or value, and only when an
//! unescaped `/` is followed by a delimiter (whitespace, `)`, `|` or the end); the body ends at
//! the **first** such `/`. Anything else beginning with `/` is a term or value. An empty value
//! (`f:`) is not a field test, so `f:` parses as a term. The full table of cases is in
//! `specs/design/store-and-asset-search/phase2-details.md` §B.
//!
//! The parser runs over `nom_locate`'s [`LocatedSpan`], as `liquers-core`'s query parser does, so
//! a [`Position`] comes from a span the same way. Positions are offsets into the expression as
//! the parser sees it — after a query parameter has been decoded.

use liquers_core::error::Error;
use liquers_core::query::Position;
use nom::Input;
use nom_locate::LocatedSpan;
use serde::{Deserialize, Serialize};

use crate::column::CompareOp;
use crate::search::kernels;
use crate::search::predicate::{Bound, Predicate, PredicateSyntax, RankTerm, SearchOptions};
use crate::RecordSchema;

type Span<'a> = LocatedSpan<&'a str>;

/// One node of a parsed expression, with the position it was parsed from.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SearchExpr {
    pub node: Node,
    pub position: Position,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum Node {
    /// The empty expression: matches every row.
    Always,
    /// Juxtaposition: every child must match.
    All(Vec<SearchExpr>),
    /// `a | b`: at least one child must match.
    Any(Vec<SearchExpr>),
    /// `-x`.
    Not(Box<SearchExpr>),
    /// A word, matched as a substring of the search fields.
    Term(String),
    /// `"a phrase"`, matched as a whitespace-normalised substring.
    Phrase(String),
    /// `/re/`, matched against the search fields.
    Regex(String),
    /// `path:test`.
    Field { path: Vec<String>, test: FieldTest },
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum FieldTest {
    /// `f:v` — exact unless the value has `*` or `?`; `f:*` means "present".
    Glob(String),
    /// `f:>v`, `f:>=v`, `f:<v`, `f:<=v`.
    Compare(CompareOp, String),
    /// `f:/re/`.
    Regex(String),
    /// `f:( … )`.
    Nested(Box<SearchExpr>),
}

impl SearchExpr {
    fn new(node: Node, position: Position) -> Self {
        SearchExpr { node, position }
    }

    /// Parse leniently, for a search box: when the strict parse fails, the text is searched as
    /// whitespace-separated words, and the error (with its position) is returned for a warning.
    pub fn parse_lenient(text: &str) -> (SearchExpr, Option<Error>) {
        match SearchExpr::try_from(text) {
            Ok(expr) => (expr, None),
            Err(error) => {
                let words: Vec<SearchExpr> = text
                    .split_whitespace()
                    .map(|word| SearchExpr::new(Node::Term(word.to_string()), Position::unknown()))
                    .collect();
                let node = match words.len() {
                    0 => Node::Always,
                    _ => Node::All(words),
                };
                (SearchExpr::new(node, Position::unknown()), Some(error))
            }
        }
    }

    fn rank_terms_into(&self, positive: bool, out: &mut Vec<RankTerm>) {
        match &self.node {
            Node::Always => {}
            Node::All(children) | Node::Any(children) => {
                for child in children {
                    child.rank_terms_into(positive, out);
                }
            }
            Node::Not(inner) => inner.rank_terms_into(!positive, out),
            Node::Term(text) => {
                if positive {
                    out.push(RankTerm::Term(text.clone()));
                }
            }
            Node::Phrase(text) => {
                if positive {
                    out.push(RankTerm::Phrase(text.clone()));
                }
            }
            Node::Regex(pattern) => {
                if positive {
                    out.push(RankTerm::Regex(pattern.clone()));
                }
            }
            Node::Field { .. } => {}
        }
    }

    fn render(&self, out: &mut String) {
        match &self.node {
            Node::Always => {}
            Node::All(children) => {
                for (i, child) in children.iter().enumerate() {
                    if i > 0 {
                        out.push(' ');
                    }
                    match &child.node {
                        Node::Any(_) | Node::Always => {
                            out.push('(');
                            child.render(out);
                            out.push(')');
                        }
                        Node::All(_)
                        | Node::Not(_)
                        | Node::Term(_)
                        | Node::Phrase(_)
                        | Node::Regex(_)
                        | Node::Field { .. } => child.render(out),
                    }
                }
            }
            Node::Any(children) => {
                for (i, child) in children.iter().enumerate() {
                    if i > 0 {
                        out.push_str(" | ");
                    }
                    match &child.node {
                        Node::Always => out.push_str("()"),
                        Node::All(_)
                        | Node::Any(_)
                        | Node::Not(_)
                        | Node::Term(_)
                        | Node::Phrase(_)
                        | Node::Regex(_)
                        | Node::Field { .. } => {
                            if matches!(child.node, Node::Any(_)) {
                                out.push('(');
                                child.render(out);
                                out.push(')');
                            } else {
                                child.render(out);
                            }
                        }
                    }
                }
            }
            Node::Not(inner) => {
                out.push('-');
                match &inner.node {
                    Node::All(_) | Node::Any(_) | Node::Always => {
                        out.push('(');
                        inner.render(out);
                        out.push(')');
                    }
                    Node::Not(_)
                    | Node::Term(_)
                    | Node::Phrase(_)
                    | Node::Regex(_)
                    | Node::Field { .. } => inner.render(out),
                }
            }
            Node::Term(text) => render_word(text, out),
            Node::Phrase(text) => render_phrase(text, out),
            Node::Regex(pattern) => render_regex(pattern, out),
            Node::Field { path, test } => {
                out.push_str(&path.join("."));
                out.push(':');
                match test {
                    FieldTest::Glob(value) => render_word(value, out),
                    FieldTest::Compare(op, value) => {
                        out.push_str(compare_symbol(*op));
                        render_word(value, out);
                    }
                    FieldTest::Regex(pattern) => render_regex(pattern, out),
                    FieldTest::Nested(inner) => {
                        out.push('(');
                        inner.render(out);
                        out.push(')');
                    }
                }
            }
        }
    }
}

fn compare_symbol(op: CompareOp) -> &'static str {
    match op {
        CompareOp::Gt => ">",
        CompareOp::Ge => ">=",
        CompareOp::Lt => "<",
        CompareOp::Le => "<=",
        CompareOp::Eq => "",
        CompareOp::Ne => "!",
    }
}

/// A word that would not re-parse as itself (whitespace, operators, a leading `-` or `/`) is
/// written as a phrase instead.
fn render_word(text: &str, out: &mut String) {
    let plain = !text.is_empty()
        && !text.starts_with('-')
        && !text.starts_with('/')
        && text.chars().all(|c| !c.is_whitespace() && !matches!(c, '(' | ')' | '|' | '"'))
        && !looks_like_field_test(text);
    if plain {
        out.push_str(text);
    } else {
        render_phrase(text, out);
    }
}

fn looks_like_field_test(text: &str) -> bool {
    matches!(field_test(Span::new(text)), Some(Ok(_)))
}

fn render_phrase(text: &str, out: &mut String) {
    out.push('"');
    for c in text.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            other => out.push(other),
        }
    }
    out.push('"');
}

fn render_regex(pattern: &str, out: &mut String) {
    out.push('/');
    out.push_str(pattern);
    out.push('/');
}

impl Predicate for SearchExpr {
    fn bind(&self, schema: &RecordSchema, options: &SearchOptions) -> Result<Bound, Error> {
        kernels::bind_expr(self, schema, options)
    }

    fn matches_json(
        &self,
        value: &serde_json::Value,
        options: &SearchOptions,
    ) -> Result<bool, Error> {
        kernels::matches_json(self, value, options)
    }

    fn has_field_tests(&self) -> bool {
        match &self.node {
            Node::Always | Node::Term(_) | Node::Phrase(_) | Node::Regex(_) => false,
            Node::All(children) | Node::Any(children) => {
                children.iter().any(|child| child.has_field_tests())
            }
            Node::Not(inner) => inner.has_field_tests(),
            Node::Field { .. } => true,
        }
    }

    fn rank_terms(&self) -> Vec<RankTerm> {
        let mut out = Vec::new();
        self.rank_terms_into(true, &mut out);
        out
    }

    fn to_syntax(&self) -> String {
        let mut out = String::new();
        self.render(&mut out);
        out
    }
}

impl PredicateSyntax for SearchExpr {
    const LANGUAGE: &'static str = "liquers-search";
}

impl TryFrom<&str> for SearchExpr {
    type Error = Error;

    /// The strict parse: malformed input is an error carrying the position where it failed.
    fn try_from(text: &str) -> Result<Self, Self::Error> {
        let span = Span::new(text);
        let (rest, expr) = parse_expr(span).map_err(Fail::into_error)?;
        let rest = skip_ws(rest);
        if let Some(c) = rest.fragment().chars().next() {
            return Err(Fail::at(&rest, format!("unexpected '{c}'")).into_error());
        }
        Ok(expr)
    }
}

// ------------------------------------------------------------------------------------------------
// The parser
// ------------------------------------------------------------------------------------------------

struct Fail {
    position: Position,
    message: String,
}

impl Fail {
    fn at(span: &Span, message: String) -> Fail {
        Fail { position: Position::from(*span), message }
    }

    fn into_error(self) -> Error {
        Error::parse_error(format!("search expression: {}", self.message))
            .with_position(&self.position)
    }
}

type PResult<'a, T> = Result<(Span<'a>, T), Fail>;

fn peek(span: &Span) -> Option<char> {
    span.fragment().chars().next()
}

/// Splits off the first `n` bytes: `(rest, taken)`.
fn advance(span: Span, n: usize) -> (Span, Span) {
    span.take_split(n)
}

fn skip_ws(span: Span) -> Span {
    let fragment = span.fragment();
    let n = fragment.len() - fragment.trim_start().len();
    advance(span, n).0
}

fn is_delimiter(c: Option<char>) -> bool {
    match c {
        None => true,
        Some(c) => c.is_whitespace() || c == ')' || c == '|',
    }
}

fn parse_expr(span: Span) -> PResult<SearchExpr> {
    let span = skip_ws(span);
    match peek(&span) {
        None | Some(')') => Ok((span, SearchExpr::new(Node::Always, Position::from(span)))),
        Some(_) => {
            let (rest, expr) = or_expr(span)?;
            Ok((skip_ws(rest), expr))
        }
    }
}

fn or_expr(span: Span) -> PResult<SearchExpr> {
    let start = Position::from(span);
    let (mut rest, first) = and_expr(span)?;
    let mut items = vec![first];
    loop {
        let probe = skip_ws(rest);
        if peek(&probe) != Some('|') {
            break;
        }
        let after = skip_ws(advance(probe, 1).0);
        if matches!(peek(&after), None | Some(')') | Some('|')) {
            return Err(Fail::at(&after, "expected an expression after '|'".to_string()));
        }
        let (next, item) = and_expr(after)?;
        items.push(item);
        rest = next;
    }
    Ok((rest, collapse(items, start, Node::Any)))
}

fn and_expr(span: Span) -> PResult<SearchExpr> {
    let start = Position::from(span);
    let (mut rest, first) = unary(span)?;
    let mut items = vec![first];
    loop {
        let probe = skip_ws(rest);
        match peek(&probe) {
            None | Some(')') | Some('|') => break,
            Some(c) => {
                if probe.location_offset() == rest.location_offset() {
                    return Err(Fail::at(&probe, format!("unexpected '{c}'")));
                }
                let (next, item) = unary(probe)?;
                items.push(item);
                rest = next;
            }
        }
    }
    Ok((rest, collapse(items, start, Node::All)))
}

fn collapse(mut items: Vec<SearchExpr>, start: Position, wrap: fn(Vec<SearchExpr>) -> Node) -> SearchExpr {
    if items.len() == 1 {
        if let Some(only) = items.pop() {
            return only;
        }
    }
    SearchExpr::new(wrap(items), start)
}

fn unary(span: Span) -> PResult<SearchExpr> {
    if peek(&span) == Some('-') {
        let start = Position::from(span);
        let rest = advance(span, 1).0;
        if is_delimiter(peek(&rest)) {
            return Err(Fail::at(&rest, "expected a term after '-'".to_string()));
        }
        let (rest, inner) = unary(rest)?;
        return Ok((rest, SearchExpr::new(Node::Not(Box::new(inner)), start)));
    }
    atom(span)
}

fn atom(span: Span) -> PResult<SearchExpr> {
    let start = Position::from(span);
    match peek(&span) {
        Some('(') => group(span),
        Some('"') => {
            let (rest, text) = phrase(span)?;
            Ok((rest, SearchExpr::new(Node::Phrase(text), start)))
        }
        Some('/') => match regex(span) {
            Some((rest, pattern)) => {
                validate_regex(&pattern, &span)?;
                Ok((rest, SearchExpr::new(Node::Regex(pattern), start)))
            }
            None => word_term(span),
        },
        Some(_) => match field_test(span) {
            Some(result) => result,
            None => word_term(span),
        },
        None => Err(Fail::at(&span, "unexpected end of expression".to_string())),
    }
}

fn group(span: Span) -> PResult<SearchExpr> {
    let open = span;
    let (rest, inner) = parse_expr(advance(span, 1).0)?;
    let rest = skip_ws(rest);
    if peek(&rest) != Some(')') {
        return Err(Fail::at(&open, "unbalanced '('".to_string()));
    }
    Ok((advance(rest, 1).0, inner))
}

fn word_term(span: Span) -> PResult<SearchExpr> {
    let start = Position::from(span);
    let (rest, text) = word(span);
    if text.is_empty() {
        let c = peek(&span).map(String::from).unwrap_or_default();
        return Err(Fail::at(&span, format!("unexpected '{c}'")));
    }
    Ok((rest, SearchExpr::new(Node::Term(text), start)))
}

/// A term or value: everything up to whitespace, a parenthesis, `|` or `"`.
fn word(span: Span) -> (Span, String) {
    let fragment = span.fragment();
    let n = fragment
        .char_indices()
        .find(|(_, c)| c.is_whitespace() || matches!(c, '(' | ')' | '|' | '"'))
        .map(|(i, _)| i)
        .unwrap_or(fragment.len());
    let (rest, taken) = advance(span, n);
    (rest, taken.fragment().to_string())
}

fn phrase(span: Span) -> PResult<String> {
    let fragment = span.fragment();
    let mut text = String::new();
    let mut escaped = false;
    for (i, c) in fragment.char_indices().skip(1) {
        if escaped {
            match c {
                '"' | '\\' => text.push(c),
                other => {
                    text.push('\\');
                    text.push(other);
                }
            }
            escaped = false;
        } else if c == '\\' {
            escaped = true;
        } else if c == '"' {
            return Ok((advance(span, i + 1).0, text));
        } else {
            text.push(c);
        }
    }
    Err(Fail::at(&span, "unterminated phrase".to_string()))
}

/// `/body/`, when the body ends at an unescaped `/` followed by a delimiter. `None` otherwise.
fn regex(span: Span) -> Option<(Span, String)> {
    let fragment = span.fragment();
    let mut escaped = false;
    let mut chars = fragment.char_indices().skip(1).peekable();
    while let Some((i, c)) = chars.next() {
        if escaped {
            escaped = false;
            continue;
        }
        match c {
            '\\' => escaped = true,
            '/' => {
                let next = chars.peek().map(|(_, c)| *c);
                if i > 1 && is_delimiter(next) {
                    return Some((advance(span, i + 1).0, fragment[1..i].to_string()));
                }
            }
            _ => {}
        }
    }
    None
}

fn validate_regex(pattern: &str, at: &Span) -> Result<(), Fail> {
    regex::Regex::new(pattern)
        .map(|_| ())
        .map_err(|error| Fail::at(at, format!("invalid regex /{pattern}/: {error}")))
}

/// `name { "." name }`, returning the rest and the segments; `None` when no name starts here.
fn field_path(span: Span) -> Option<(Span, Vec<String>)> {
    let mut rest = span;
    let mut segments = Vec::new();
    loop {
        let fragment = rest.fragment();
        let mut chars = fragment.char_indices();
        match chars.next() {
            Some((_, c)) if c.is_alphabetic() || c == '_' => {}
            Some(_) | None => return None,
        }
        let n = fragment
            .char_indices()
            .find(|(_, c)| !(c.is_alphanumeric() || *c == '_'))
            .map(|(i, _)| i)
            .unwrap_or(fragment.len());
        let (after, name) = advance(rest, n);
        segments.push(name.fragment().to_string());
        if peek(&after) == Some('.') {
            rest = advance(after, 1).0;
        } else {
            return Some((after, segments));
        }
    }
}

/// `path:test`, or `None` when the text here is not a field test (and is therefore a term).
fn field_test(span: Span) -> Option<PResult<SearchExpr>> {
    let start = Position::from(span);
    let (rest, path) = field_path(span)?;
    if peek(&rest) != Some(':') {
        return None;
    }
    let value_start = advance(rest, 1).0;
    let make = |test: FieldTest| SearchExpr::new(Node::Field { path: path.clone(), test }, start.clone());
    match peek(&value_start) {
        Some('(') => Some(group(value_start).map(|(rest, inner)| (rest, make(FieldTest::Nested(Box::new(inner)))))),
        Some('/') => match regex(value_start) {
            Some((rest, pattern)) => Some(
                validate_regex(&pattern, &value_start).map(|()| (rest, make(FieldTest::Regex(pattern)))),
            ),
            None => plain_value(value_start).map(|(rest, value)| Ok((rest, make(FieldTest::Glob(value))))),
        },
        Some(_) => {
            let (after_op, op) = compare_op(value_start);
            let test_value = if peek(&after_op) == Some('"') {
                match phrase(after_op) {
                    Ok((rest, text)) => Some((rest, text)),
                    Err(fail) => return Some(Err(fail)),
                }
            } else {
                plain_value(after_op)
            }?;
            let (rest, value) = test_value;
            let test = match op {
                Some(op) => FieldTest::Compare(op, value),
                None => FieldTest::Glob(value),
            };
            Some(Ok((rest, make(test))))
        }
        None => None,
    }
}

fn compare_op(span: Span) -> (Span, Option<CompareOp>) {
    let fragment = span.fragment();
    for (symbol, op) in [
        (">=", CompareOp::Ge),
        ("<=", CompareOp::Le),
        (">", CompareOp::Gt),
        ("<", CompareOp::Lt),
    ] {
        if fragment.starts_with(symbol) {
            return (advance(span, symbol.len()).0, Some(op));
        }
    }
    (span, None)
}

fn plain_value(span: Span) -> Option<(Span, String)> {
    let (rest, value) = word(span);
    if value.is_empty() {
        None
    } else {
        Some((rest, value))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse(text: &str) -> SearchExpr {
        SearchExpr::try_from(text).unwrap_or_else(|e| panic!("{text:?}: {e}"))
    }

    fn node(text: &str) -> Node {
        parse(text).node
    }

    fn term(text: &str) -> Node {
        Node::Term(text.to_string())
    }

    fn field(path: &str, test: FieldTest) -> Node {
        Node::Field { path: path.split('.').map(String::from).collect(), test }
    }

    /// The node tree with positions dropped, for comparisons.
    fn shape(expr: &SearchExpr) -> String {
        format!("{:?}", strip(expr))
    }

    fn strip(expr: &SearchExpr) -> Node {
        match &expr.node {
            Node::All(c) => Node::All(c.iter().map(|e| SearchExpr::new(strip(e), Position::unknown())).collect()),
            Node::Any(c) => Node::Any(c.iter().map(|e| SearchExpr::new(strip(e), Position::unknown())).collect()),
            Node::Not(i) => Node::Not(Box::new(SearchExpr::new(strip(i), Position::unknown()))),
            Node::Field { path, test: FieldTest::Nested(inner) } => Node::Field {
                path: path.clone(),
                test: FieldTest::Nested(Box::new(SearchExpr::new(strip(inner), Position::unknown()))),
            },
            other => other.clone(),
        }
    }

    fn kinds(expr: &SearchExpr) -> Vec<Node> {
        match &expr.node {
            Node::All(c) | Node::Any(c) => c.iter().map(strip).collect(),
            other => vec![other.clone()],
        }
    }

    #[test]
    fn parses_terms_phrases_negation_or_groups() {
        assert_eq!(node(""), Node::Always);
        assert_eq!(node("   "), Node::Always);
        assert_eq!(node("expiration"), term("expiration"));
        assert_eq!(node("\"expiration safety\""), Node::Phrase("expiration safety".into()));
        let e = parse("a -b");
        assert_eq!(
            kinds(&e),
            vec![term("a"), Node::Not(Box::new(SearchExpr::new(term("b"), Position::unknown())))]
        );
        assert!(matches!(node("a | b"), Node::Any(ref c) if c.len() == 2));
        // Juxtaposition binds tighter than `|`: `a b | c` is `(a b) | c`.
        match node("a b | c") {
            Node::Any(c) => assert!(matches!(c[0].node, Node::All(_))),
            other => panic!("{other:?}"),
        }
        match node("(a | b) c") {
            Node::All(c) => assert!(matches!(c[0].node, Node::Any(_))),
            other => panic!("{other:?}"),
        }
        assert_eq!(node("x-ray"), term("x-ray"));
        assert_eq!(node("a\\b"), term("a\\b"));
    }

    #[test]
    fn parses_field_glob_compare_nested() {
        assert_eq!(node("status:draft"), field("status", FieldTest::Glob("draft".into())));
        assert_eq!(node("f:*"), field("f", FieldTest::Glob("*".into())));
        assert_eq!(
            node("updated:>=2026-10-01"),
            field("updated", FieldTest::Compare(CompareOp::Ge, "2026-10-01".into()))
        );
        assert_eq!(node("size:<5"), field("size", FieldTest::Compare(CompareOp::Lt, "5".into())));
        assert_eq!(node("attr.status:draft"), field("attr.status", FieldTest::Glob("draft".into())));
        assert_eq!(node("title:\"a b\""), field("title", FieldTest::Glob("a b".into())));
        match node("content:(a -b)") {
            Node::Field { path, test: FieldTest::Nested(inner) } => {
                assert_eq!(path, vec!["content".to_string()]);
                assert!(matches!(inner.node, Node::All(_)));
            }
            other => panic!("{other:?}"),
        }
        assert_eq!(node("key:specs/issues/*"), field("key", FieldTest::Glob("specs/issues/*".into())));
    }

    #[test]
    fn regex_body_ends_at_first_closing_slash() {
        assert_eq!(node("/fn \\w+_info/"), Node::Regex("fn \\w+_info".into()));
        assert_eq!(node("/a/b/"), Node::Regex("a/b".into()));
        assert_eq!(
            kinds(&parse("/a/ /b/")),
            vec![Node::Regex("a".into()), Node::Regex("b".into())]
        );
        assert_eq!(node("title:/^Phase [12]/"), field("title", FieldTest::Regex("^Phase [12]".into())));
        match node("title:/x/ | /y/") {
            Node::Any(c) => {
                assert_eq!(c[0].node, field("title", FieldTest::Regex("x".into())));
                assert_eq!(c[1].node, Node::Regex("y".into()));
            }
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn slash_without_closing_delimiter_is_literal() {
        assert_eq!(node("area:core/store"), field("area", FieldTest::Glob("core/store".into())));
        assert_eq!(node("path:/usr/bin"), field("path", FieldTest::Glob("/usr/bin".into())));
        assert_eq!(node("/unclosed"), term("/unclosed"));
        assert_eq!(node("a/b"), term("a/b"));
        assert_eq!(node("f:"), term("f:"));
        assert_eq!(node("f:>"), term("f:>"));
    }

    #[test]
    fn malformed_expression_reports_position() {
        let error = SearchExpr::try_from("x (a").expect_err("unbalanced");
        let position = error.position.clone();
        assert_eq!(position.offset, 2, "{error}");
        assert!(error.to_string().contains("unbalanced"), "{error}");
        for bad in ["\"open", "a |", "-", "a)"] {
            assert!(SearchExpr::try_from(bad).is_err(), "{bad:?} should fail");
        }
        let (expr, error) = SearchExpr::parse_lenient("x (a");
        assert!(error.is_some());
        assert_eq!(kinds(&expr), vec![term("x"), term("(a")]);
    }

    #[test]
    fn invalid_regex_reports_position() {
        let error = SearchExpr::try_from("ok /[/").expect_err("invalid regex");
        assert_eq!(error.position.offset, 3, "{error}");
        assert!(error.to_string().contains("invalid regex"), "{error}");
    }

    #[test]
    fn to_syntax_round_trips() {
        let cases = [
            "",
            "expiration",
            "expiration safety -status:closed*",
            "area:core/store (priority:P0 | priority:P1)",
            "\"expiration safety\" title:\"a b\"",
            "/fn get_asset_info/",
            "heading:/\\bPhase 2/",
            "updated:>2026-10-01 size:<=5",
            "content:(a -b) attr.status:draft",
            "a b | c",
            "-(a | b)",
            "/a/ /b/",
            "path:/usr/bin",
            "f:",
            "key:specs/issues/*",
        ];
        for text in cases {
            let first = parse(text);
            let printed = first.to_syntax();
            let second = parse(&printed);
            assert_eq!(shape(&first), shape(&second), "{text:?} printed as {printed:?}");
            assert_eq!(second.to_syntax(), printed, "{text:?}");
        }
        // Hand-built trees that would not come out of the parser still print re-parseably.
        let built = SearchExpr::new(
            Node::All(vec![
                SearchExpr::new(Node::Term("two words".into()), Position::unknown()),
                SearchExpr::new(Node::Term("-dash".into()), Position::unknown()),
                SearchExpr::new(Node::Term("k:v".into()), Position::unknown()),
            ]),
            Position::unknown(),
        );
        let printed = built.to_syntax();
        assert_eq!(parse(&printed).to_syntax(), printed);
    }

    #[test]
    fn rank_terms_skip_negated_and_field_clauses() {
        let terms = parse("a \"b c\" /d/ -e status:x (f | -g)").rank_terms();
        assert_eq!(
            terms,
            vec![
                RankTerm::Term("a".into()),
                RankTerm::Phrase("b c".into()),
                RankTerm::Regex("d".into()),
                RankTerm::Term("f".into()),
            ]
        );
        assert!(parse("status:x").rank_terms().is_empty());
        assert!(parse("status:x").has_field_tests());
        assert!(!parse("a -b").has_field_tests());
    }
}
