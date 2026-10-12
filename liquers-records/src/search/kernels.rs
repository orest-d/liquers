//! Binding a predicate to a schema, and evaluating it column-wise to a [`Bitmap`].
//!
//! The semantics are those of `specs/design/store-and-asset-search/phase2-architecture.md`
//! §Semantics:
//! - free-text clauses match the search fields ([`search_fields`]);
//! - a field test addresses one column, through [`RecordSchema::resolve_field`];
//! - a column declared JSON (`media_type: application/json`) is the only kind whose cells are
//!   parsed: an array cell matches when any element does, and a nested field test walks the
//!   parsed value. Nothing else is ever sniffed;
//! - a null never matches.

use std::collections::HashMap;
use std::sync::Mutex;

use liquers_core::error::Error;
use regex::{Regex, RegexBuilder};

use crate::buffer::Bitmap;
use crate::column::{Column, CompareOp, FieldValue};
use crate::schema::{FieldType, RecordSchema};
use crate::search::predicate::{Bound, BoundPredicate, Predicate, SearchOptions};
use crate::search::syntax::{FieldTest, Node, SearchExpr};
use crate::RecordView;

/// The fields free text is matched in when a schema declares no text-role fields.
pub const DEFAULT_SEARCH_FIELDS: [&str; 5] = ["filename", "title", "description", "doc", "content"];

/// The media type that marks a Text column as holding JSON documents.
pub const JSON_MEDIA_TYPE: &str = "application/json";

/// Whether column `index` is a Text column declared to hold JSON.
pub fn is_json_column(schema: &RecordSchema, index: usize) -> bool {
    schema.fields.get(index).is_some_and(|field| {
        field.data_type == FieldType::Text && field.media_type.as_deref() == Some(JSON_MEDIA_TYPE)
    })
}

/// The columns free-text clauses match: the explicit list (each name must exist); otherwise the
/// schema's text-role fields; otherwise whichever of [`DEFAULT_SEARCH_FIELDS`] exist; otherwise
/// every Text column.
pub fn search_fields(schema: &RecordSchema, requested: &[String]) -> Result<Vec<usize>, Error> {
    if !requested.is_empty() {
        let mut out = Vec::with_capacity(requested.len());
        for name in requested {
            match schema.resolve_field(name)? {
                Some(index) => out.push(index),
                None => {
                    let available: Vec<&str> =
                        schema.fields.iter().map(|f| f.name.as_str()).collect();
                    return Err(Error::general_error(format!(
                        "search field '{name}' is not in the records; available fields: {}",
                        available.join(", ")
                    )));
                }
            }
        }
        return Ok(out);
    }
    let roles = schema.text_fields();
    if !roles.is_empty() {
        return Ok(roles.to_vec());
    }
    let named: Vec<usize> = DEFAULT_SEARCH_FIELDS
        .iter()
        .filter_map(|name| schema.index_of(name))
        .collect();
    if !named.is_empty() {
        return Ok(named);
    }
    Ok(schema
        .fields
        .iter()
        .enumerate()
        .filter(|(_, field)| field.data_type == FieldType::Text)
        .map(|(index, _)| index)
        .collect())
}

// ------------------------------------------------------------------------------------------------
// Bound form
// ------------------------------------------------------------------------------------------------

enum TextMatcher {
    /// Already folded to lower case when the search is case-insensitive.
    Substring(String),
    /// Whitespace-normalised, and folded like `Substring`.
    Phrase(String),
    Regex(Regex),
}

enum CellTest {
    Glob(Glob),
    Regex(Regex),
    /// Lexicographic on the raw text, unaffected by `case_sensitive` (decision D4).
    Compare(CompareOp, String),
}

enum BNode {
    Always,
    Never,
    All(Vec<BNode>),
    Any(Vec<BNode>),
    Not(Box<BNode>),
    Text { matcher: TextMatcher, fields: Vec<usize>, positive: bool },
    TextCell { column: usize, test: CellTest, json: bool },
    Typed { column: usize, op: CompareOp, value: FieldValue },
    Present { column: usize },
    NestedJson { column: usize, expr: SearchExpr },
}

struct BoundTree {
    root: BNode,
    options: SearchOptions,
    warnings: Mutex<Vec<String>>,
}

/// `SearchExpr::bind`: resolve the expression against `schema`.
pub(crate) fn bind_expr(
    expr: &SearchExpr,
    schema: &RecordSchema,
    options: &SearchOptions,
) -> Result<Bound, Error> {
    let fields = search_fields(schema, &options.fields)?;
    let mut warnings = Vec::new();
    let root = bind_node(expr, schema, options, &fields, true, &mut warnings)?;
    Ok(Bound {
        matcher: Box::new(BoundTree {
            root,
            options: options.clone(),
            warnings: Mutex::new(Vec::new()),
        }),
        warnings,
    })
}

fn fold(text: &str, options: &SearchOptions) -> String {
    if options.case_sensitive {
        text.to_string()
    } else {
        text.to_lowercase()
    }
}

fn normalize_ws(text: &str) -> String {
    text.split_whitespace().collect::<Vec<_>>().join(" ")
}

fn build_regex(pattern: &str, options: &SearchOptions) -> Result<Regex, Error> {
    RegexBuilder::new(pattern)
        .case_insensitive(!options.case_sensitive)
        .build()
        .map_err(|error| Error::general_error(format!("invalid regex /{pattern}/: {error}")))
}

fn describe(expr: &SearchExpr) -> String {
    let position = &expr.position;
    if position.line > 0 {
        format!("line {}, column {}", position.line, position.column)
    } else {
        "unknown position".to_string()
    }
}

/// The longest prefix of `path` that names a column, and the rest of the path.
fn resolve_path(
    schema: &RecordSchema,
    path: &[String],
) -> Result<Option<(usize, Vec<String>)>, Error> {
    for k in (1..=path.len()).rev() {
        let name = path[..k].join(".");
        if let Some(index) = schema.resolve_field(&name)? {
            return Ok(Some((index, path[k..].to_vec())));
        }
    }
    Ok(None)
}

fn bind_node(
    expr: &SearchExpr,
    schema: &RecordSchema,
    options: &SearchOptions,
    fields: &[usize],
    positive: bool,
    warnings: &mut Vec<String>,
) -> Result<BNode, Error> {
    let bind = |e: &SearchExpr, pos: bool, w: &mut Vec<String>| {
        bind_node(e, schema, options, fields, pos, w)
    };
    match &expr.node {
        Node::Always => Ok(BNode::Always),
        Node::All(children) => Ok(BNode::All(
            children.iter().map(|c| bind(c, positive, warnings)).collect::<Result<_, _>>()?,
        )),
        Node::Any(children) => Ok(BNode::Any(
            children.iter().map(|c| bind(c, positive, warnings)).collect::<Result<_, _>>()?,
        )),
        Node::Not(inner) => Ok(BNode::Not(Box::new(bind(inner, !positive, warnings)?))),
        Node::Term(text) => Ok(BNode::Text {
            matcher: TextMatcher::Substring(fold(text, options)),
            fields: fields.to_vec(),
            positive,
        }),
        Node::Phrase(text) => Ok(BNode::Text {
            matcher: TextMatcher::Phrase(fold(&normalize_ws(text), options)),
            fields: fields.to_vec(),
            positive,
        }),
        Node::Regex(pattern) => Ok(BNode::Text {
            matcher: TextMatcher::Regex(build_regex(pattern, options)?),
            fields: fields.to_vec(),
            positive,
        }),
        Node::Field { path, test } => {
            let full = path.join(".");
            let Some((column, rest)) = resolve_path(schema, path)? else {
                warnings.push(format!(
                    "field '{full}' ({}) is not in the records; that clause matches nothing",
                    describe(expr)
                ));
                return Ok(BNode::Never);
            };
            let field = &schema.fields[column];
            let is_text = field.data_type == FieldType::Text;
            let json = is_json_column(schema, column);
            if !rest.is_empty() {
                if !json {
                    return Err(Error::general_error(format!(
                        "'{full}' ({}): column '{}' is not declared JSON, so a path inside it \
                         cannot be followed",
                        describe(expr),
                        field.name
                    )));
                }
                let inner = SearchExpr {
                    node: Node::Field { path: rest, test: test.clone() },
                    position: expr.position.clone(),
                };
                return Ok(BNode::NestedJson { column, expr: inner });
            }
            match test {
                FieldTest::Nested(inner) => {
                    if inner.has_field_tests() {
                        if json {
                            Ok(BNode::NestedJson { column, expr: (**inner).clone() })
                        } else {
                            Err(Error::general_error(format!(
                                "'{full}:(…)' ({}): field tests inside a column need a column \
                                 declared JSON, and '{}' is not",
                                describe(expr),
                                field.name
                            )))
                        }
                    } else if is_text {
                        bind_node(inner, schema, options, &[column], positive, warnings)
                    } else {
                        Err(Error::general_error(format!(
                            "'{full}:(…)' ({}): '{}' is not a Text column",
                            describe(expr),
                            field.name
                        )))
                    }
                }
                FieldTest::Glob(value) => {
                    if value == "*" {
                        return Ok(BNode::Present { column });
                    }
                    if is_text {
                        Ok(BNode::TextCell {
                            column,
                            test: CellTest::Glob(Glob::new(value, options.case_sensitive)),
                            json,
                        })
                    } else if value.contains('*') || value.contains('?') {
                        Err(Error::general_error(format!(
                            "'{full}:{value}' ({}): wildcards apply to Text columns only, and \
                             '{}' is {:?}",
                            describe(expr),
                            field.name,
                            field.data_type
                        )))
                    } else {
                        Ok(BNode::Typed {
                            column,
                            op: CompareOp::Eq,
                            value: parse_typed(field.data_type, value, &full)?,
                        })
                    }
                }
                FieldTest::Regex(pattern) => {
                    if is_text {
                        Ok(BNode::TextCell {
                            column,
                            test: CellTest::Regex(build_regex(pattern, options)?),
                            json,
                        })
                    } else {
                        Err(Error::general_error(format!(
                            "'{full}:/{pattern}/' ({}): a regex applies to Text columns only, \
                             and '{}' is {:?}",
                            describe(expr),
                            field.name,
                            field.data_type
                        )))
                    }
                }
                FieldTest::Compare(op, value) => {
                    if is_text {
                        Ok(BNode::TextCell {
                            column,
                            test: CellTest::Compare(*op, value.clone()),
                            json,
                        })
                    } else {
                        Ok(BNode::Typed {
                            column,
                            op: *op,
                            value: parse_typed(field.data_type, value, &full)?,
                        })
                    }
                }
            }
        }
    }
}

/// Parse a value for a typed column. A date-only value against a Timestamp column is widened to
/// midnight UTC.
fn parse_typed(data_type: FieldType, value: &str, field: &str) -> Result<FieldValue, Error> {
    match FieldValue::parse_text(data_type, value) {
        Ok(parsed) => Ok(parsed),
        Err(first) => {
            let widened = format!("{value}T00:00:00Z");
            if data_type == FieldType::Timestamp && value.len() == 10 {
                if let Ok(parsed) = FieldValue::parse_text(data_type, &widened) {
                    return Ok(parsed);
                }
            }
            Err(Error::general_error(format!(
                "'{field}': '{value}' cannot be compared with a {data_type:?} column: {first}"
            )))
        }
    }
}

// ------------------------------------------------------------------------------------------------
// Evaluation
// ------------------------------------------------------------------------------------------------

/// One cell of a Text column, or `None` for a null or a non-Text column.
pub(crate) fn text_at(column: &Column, row: usize) -> Option<&str> {
    match column {
        Column::Text { validity, offsets, data } => {
            if let Some(validity) = validity {
                if row >= validity.len() || !validity.get(row) {
                    return None;
                }
            }
            let offsets = offsets.as_slice();
            let start = usize::try_from(*offsets.get(row)?).ok()?;
            let end = usize::try_from(*offsets.get(row + 1)?).ok()?;
            std::str::from_utf8(data.as_bytes().get(start..end)?).ok()
        }
        Column::Bool { .. }
        | Column::Int { .. }
        | Column::UInt { .. }
        | Column::Float { .. }
        | Column::Binary { .. }
        | Column::Date { .. }
        | Column::Timestamp { .. }
        | Column::Vector { .. } => None,
    }
}

struct EvalCtx<'a> {
    view: &'a dyn RecordView,
    columns: HashMap<usize, Column>,
    json: HashMap<usize, Vec<Option<serde_json::Value>>>,
    invalid_json: usize,
}

impl<'a> EvalCtx<'a> {
    fn column(&mut self, index: usize) -> Result<&Column, Error> {
        if !self.columns.contains_key(&index) {
            let column = self.view.column(index)?;
            self.columns.insert(index, column);
        }
        self.columns
            .get(&index)
            .ok_or_else(|| Error::general_error(format!("column {index} is unavailable")))
    }

    fn json_cells(&mut self, index: usize) -> Result<&Vec<Option<serde_json::Value>>, Error> {
        if !self.json.contains_key(&index) {
            let len = self.view.len();
            let mut invalid = 0;
            let column = self.column(index)?;
            let mut cells = Vec::with_capacity(len);
            for row in 0..len {
                match text_at(column, row) {
                    None => cells.push(None),
                    Some(text) => match serde_json::from_str::<serde_json::Value>(text) {
                        Ok(value) => cells.push(Some(value)),
                        Err(_) => {
                            invalid += 1;
                            cells.push(None);
                        }
                    },
                }
            }
            self.invalid_json += invalid;
            self.json.insert(index, cells);
        }
        self.json
            .get(&index)
            .ok_or_else(|| Error::general_error(format!("column {index} is unavailable")))
    }
}

fn all_set(len: usize) -> Bitmap {
    Bitmap::new(len).not()
}

impl BoundTree {
    fn eval(&self, node: &BNode, ctx: &mut EvalCtx) -> Result<Bitmap, Error> {
        let len = ctx.view.len();
        match node {
            BNode::Always => Ok(all_set(len)),
            BNode::Never => Ok(Bitmap::new(len)),
            BNode::All(children) => {
                let mut acc = all_set(len);
                for child in children {
                    acc = acc.and(&self.eval(child, ctx)?)?;
                }
                Ok(acc)
            }
            BNode::Any(children) => {
                let mut acc = Bitmap::new(len);
                for child in children {
                    acc = acc.or(&self.eval(child, ctx)?)?;
                }
                Ok(acc)
            }
            BNode::Not(inner) => Ok(self.eval(inner, ctx)?.not()),
            BNode::Text { matcher, fields, .. } => {
                let mut acc = Bitmap::new(len);
                for &field in fields {
                    acc = acc.or(&self.text_mask(matcher, field, ctx)?)?;
                }
                Ok(acc)
            }
            BNode::TextCell { column, test, json } => {
                let col = ctx.column(*column)?;
                let mut bits = Vec::with_capacity(len);
                for row in 0..len {
                    bits.push(match text_at(col, row) {
                        None => false,
                        Some(text) => cell_matches(text, test, *json),
                    });
                }
                Ok(Bitmap::from_bools(&bits))
            }
            BNode::Typed { column, op, value } => ctx.column(*column)?.compare(*op, value),
            BNode::Present { column } => Ok(ctx.column(*column)?.null_mask().not()),
            BNode::NestedJson { column, expr } => {
                let cells = ctx.json_cells(*column)?;
                let mut bits = Vec::with_capacity(len);
                for cell in cells {
                    bits.push(match cell {
                        None => false,
                        Some(value) => expr.matches_json(value, &self.options)?,
                    });
                }
                Ok(Bitmap::from_bools(&bits))
            }
        }
    }

    fn text_mask(
        &self,
        matcher: &TextMatcher,
        field: usize,
        ctx: &mut EvalCtx,
    ) -> Result<Bitmap, Error> {
        let len = ctx.view.len();
        let col = ctx.column(field)?;
        let mut bits = Vec::with_capacity(len);
        for row in 0..len {
            bits.push(match text_at(col, row) {
                None => false,
                Some(text) => text_matches(text, matcher, &self.options),
            });
        }
        Ok(Bitmap::from_bools(&bits))
    }

    fn collect_evidence(
        &self,
        node: &BNode,
        ctx: &mut EvalCtx,
        out: &mut Vec<(usize, Bitmap)>,
    ) -> Result<(), Error> {
        match node {
            BNode::All(children) | BNode::Any(children) => {
                for child in children {
                    self.collect_evidence(child, ctx, out)?;
                }
                Ok(())
            }
            BNode::Not(_) => Ok(()),
            BNode::Text { matcher, fields, positive } => {
                if !*positive {
                    return Ok(());
                }
                for &field in fields {
                    let mask = self.text_mask(matcher, field, ctx)?;
                    match out.iter_mut().find(|(f, _)| *f == field) {
                        Some((_, acc)) => *acc = acc.or(&mask)?,
                        None => out.push((field, mask)),
                    }
                }
                Ok(())
            }
            BNode::Always
            | BNode::Never
            | BNode::TextCell { .. }
            | BNode::Typed { .. }
            | BNode::Present { .. }
            | BNode::NestedJson { .. } => Ok(()),
        }
    }

    fn push_warning(&self, warning: String) {
        let mut guard = self.warnings.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
        guard.push(warning);
    }
}

impl BoundPredicate for BoundTree {
    fn mask(&self, view: &dyn RecordView) -> Result<Bitmap, Error> {
        let mut ctx = EvalCtx { view, columns: HashMap::new(), json: HashMap::new(), invalid_json: 0 };
        let mask = self.eval(&self.root, &mut ctx)?;
        if ctx.invalid_json > 0 {
            self.push_warning(format!(
                "{} cell(s) of a JSON column are not valid JSON and match nothing",
                ctx.invalid_json
            ));
        }
        Ok(mask)
    }

    fn evidence(&self, view: &dyn RecordView) -> Result<Vec<(usize, Bitmap)>, Error> {
        let mut ctx = EvalCtx { view, columns: HashMap::new(), json: HashMap::new(), invalid_json: 0 };
        let mut out = Vec::new();
        self.collect_evidence(&self.root, &mut ctx, &mut out)?;
        Ok(out)
    }

    fn take_warnings(&self) -> Vec<String> {
        let mut guard = self.warnings.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
        std::mem::take(&mut *guard)
    }
}

fn text_matches(text: &str, matcher: &TextMatcher, options: &SearchOptions) -> bool {
    match matcher {
        TextMatcher::Substring(needle) => {
            if options.case_sensitive {
                text.contains(needle.as_str())
            } else {
                text.to_lowercase().contains(needle.as_str())
            }
        }
        TextMatcher::Phrase(needle) => {
            let haystack = normalize_ws(text);
            if options.case_sensitive {
                haystack.contains(needle.as_str())
            } else {
                haystack.to_lowercase().contains(needle.as_str())
            }
        }
        TextMatcher::Regex(regex) => regex.is_match(text),
    }
}

fn cell_matches(text: &str, test: &CellTest, json: bool) -> bool {
    if json && text.trim_start().starts_with('[') {
        if let Ok(serde_json::Value::Array(items)) = serde_json::from_str::<serde_json::Value>(text)
        {
            return items
                .iter()
                .filter_map(scalar_text)
                .any(|item| scalar_test(&item, test));
        }
    }
    scalar_test(text, test)
}

fn scalar_test(text: &str, test: &CellTest) -> bool {
    match test {
        CellTest::Glob(glob) => glob.matches(text),
        CellTest::Regex(regex) => regex.is_match(text),
        CellTest::Compare(op, value) => compare_text(text, *op, value),
    }
}

fn compare_text(text: &str, op: CompareOp, value: &str) -> bool {
    let ordering = text.cmp(value);
    match op {
        CompareOp::Eq => ordering.is_eq(),
        CompareOp::Ne => ordering.is_ne(),
        CompareOp::Lt => ordering.is_lt(),
        CompareOp::Le => ordering.is_le(),
        CompareOp::Gt => ordering.is_gt(),
        CompareOp::Ge => ordering.is_ge(),
    }
}

fn scalar_text(value: &serde_json::Value) -> Option<String> {
    match value {
        serde_json::Value::String(s) => Some(s.clone()),
        serde_json::Value::Number(n) => Some(n.to_string()),
        serde_json::Value::Bool(b) => Some(b.to_string()),
        serde_json::Value::Null | serde_json::Value::Array(_) | serde_json::Value::Object(_) => {
            None
        }
    }
}

// ------------------------------------------------------------------------------------------------
// Globs
// ------------------------------------------------------------------------------------------------

/// `*` (any run) and `?` (one character), anchored at both ends.
pub(crate) struct Glob {
    pattern: Vec<char>,
    case_sensitive: bool,
}

impl Glob {
    pub(crate) fn new(pattern: &str, case_sensitive: bool) -> Glob {
        let pattern = if case_sensitive { pattern.to_string() } else { pattern.to_lowercase() };
        Glob { pattern: pattern.chars().collect(), case_sensitive }
    }

    pub(crate) fn matches(&self, text: &str) -> bool {
        let folded;
        let text = if self.case_sensitive {
            text
        } else {
            folded = text.to_lowercase();
            folded.as_str()
        };
        let text: Vec<char> = text.chars().collect();
        let (mut t, mut p) = (0usize, 0usize);
        let mut star: Option<(usize, usize)> = None;
        while t < text.len() {
            if p < self.pattern.len() && (self.pattern[p] == '?' || self.pattern[p] == text[t]) {
                t += 1;
                p += 1;
            } else if p < self.pattern.len() && self.pattern[p] == '*' {
                star = Some((p, t));
                p += 1;
            } else if let Some((star_p, star_t)) = star {
                p = star_p + 1;
                t = star_t + 1;
                star = Some((star_p, star_t + 1));
            } else {
                return false;
            }
        }
        while p < self.pattern.len() && self.pattern[p] == '*' {
            p += 1;
        }
        p == self.pattern.len()
    }
}

// ------------------------------------------------------------------------------------------------
// Matching inside JSON
// ------------------------------------------------------------------------------------------------

/// `Predicate::matches_json` for the built-in language.
pub(crate) fn matches_json(
    expr: &SearchExpr,
    value: &serde_json::Value,
    options: &SearchOptions,
) -> Result<bool, Error> {
    match &expr.node {
        Node::Always => Ok(true),
        Node::All(children) => {
            for child in children {
                if !matches_json(child, value, options)? {
                    return Ok(false);
                }
            }
            Ok(true)
        }
        Node::Any(children) => {
            for child in children {
                if matches_json(child, value, options)? {
                    return Ok(true);
                }
            }
            Ok(false)
        }
        Node::Not(inner) => Ok(!matches_json(inner, value, options)?),
        Node::Term(text) => {
            let matcher = TextMatcher::Substring(fold(text, options));
            Ok(any_leaf(value, &|leaf| text_matches(leaf, &matcher, options)))
        }
        Node::Phrase(text) => {
            let matcher = TextMatcher::Phrase(fold(&normalize_ws(text), options));
            Ok(any_leaf(value, &|leaf| text_matches(leaf, &matcher, options)))
        }
        Node::Regex(pattern) => {
            let matcher = TextMatcher::Regex(build_regex(pattern, options)?);
            Ok(any_leaf(value, &|leaf| text_matches(leaf, &matcher, options)))
        }
        Node::Field { path, test } => {
            let mut targets = Vec::new();
            navigate(value, path, &mut targets);
            for target in targets {
                if json_test(target, test, options)? {
                    return Ok(true);
                }
            }
            Ok(false)
        }
    }
}

fn any_leaf(value: &serde_json::Value, test: &dyn Fn(&str) -> bool) -> bool {
    match value {
        serde_json::Value::Array(items) => items.iter().any(|item| any_leaf(item, test)),
        serde_json::Value::Object(map) => map.values().any(|item| any_leaf(item, test)),
        serde_json::Value::Null => false,
        serde_json::Value::String(_) | serde_json::Value::Number(_) | serde_json::Value::Bool(_) => {
            scalar_text(value).is_some_and(|text| test(&text))
        }
    }
}

/// Follow `path` through objects; an array on the way fans out to each element.
fn navigate<'v>(value: &'v serde_json::Value, path: &[String], out: &mut Vec<&'v serde_json::Value>) {
    match value {
        serde_json::Value::Array(items) if !path.is_empty() => {
            for item in items {
                navigate(item, path, out);
            }
        }
        serde_json::Value::Array(_)
        | serde_json::Value::Object(_)
        | serde_json::Value::Null
        | serde_json::Value::String(_)
        | serde_json::Value::Number(_)
        | serde_json::Value::Bool(_) => match path.split_first() {
            None => out.push(value),
            Some((head, rest)) => {
                if let Some(next) = value.as_object().and_then(|map| map.get(head)) {
                    navigate(next, rest, out);
                }
            }
        },
    }
}

fn json_test(
    target: &serde_json::Value,
    test: &FieldTest,
    options: &SearchOptions,
) -> Result<bool, Error> {
    if let serde_json::Value::Array(items) = target {
        if !matches!(test, FieldTest::Nested(_)) {
            for item in items {
                if json_test(item, test, options)? {
                    return Ok(true);
                }
            }
            return Ok(false);
        }
    }
    match test {
        FieldTest::Nested(inner) => matches_json(inner, target, options),
        FieldTest::Glob(pattern) => {
            if pattern == "*" {
                return Ok(!target.is_null());
            }
            let glob = Glob::new(pattern, options.case_sensitive);
            Ok(scalar_text(target).is_some_and(|text| glob.matches(&text)))
        }
        FieldTest::Regex(pattern) => {
            let regex = build_regex(pattern, options)?;
            Ok(scalar_text(target).is_some_and(|text| regex.is_match(&text)))
        }
        FieldTest::Compare(op, value) => {
            if let (Some(number), Ok(wanted)) = (target.as_f64(), value.parse::<f64>()) {
                let ordering = number.partial_cmp(&wanted);
                return Ok(match ordering {
                    None => false,
                    Some(ordering) => match op {
                        CompareOp::Eq => ordering.is_eq(),
                        CompareOp::Ne => ordering.is_ne(),
                        CompareOp::Lt => ordering.is_lt(),
                        CompareOp::Le => ordering.is_le(),
                        CompareOp::Gt => ordering.is_gt(),
                        CompareOp::Ge => ordering.is_ge(),
                    },
                });
            }
            Ok(scalar_text(target).is_some_and(|text| compare_text(&text, *op, value)))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mutable::{RecordBatchMut, RecordViewMut};
    use crate::schema::{FieldRole, FieldSchema};
    use std::sync::Arc;

    fn view(fields: Vec<FieldSchema>, rows: Vec<Vec<FieldValue>>) -> Arc<dyn RecordView> {
        let schema = Arc::new(RecordSchema::new(fields).expect("schema"));
        let mut batch = RecordBatchMut::with_capacity(schema, rows.len());
        for row in rows {
            batch.append_row(&row).expect("row");
        }
        Arc::new(batch.freeze().expect("freeze"))
    }

    fn t(text: &str) -> FieldValue {
        FieldValue::Text(Arc::from(text))
    }

    fn docs() -> Arc<dyn RecordView> {
        view(
            vec![
                FieldSchema::new("key", FieldType::Text),
                FieldSchema::new("title", FieldType::Text).with_role(FieldRole::text()),
                FieldSchema::new("content", FieldType::Text).with_role(FieldRole::text()),
                FieldSchema::new("status", FieldType::Text).with_role(FieldRole::keyword()),
                FieldSchema::new("size", FieldType::Int),
                FieldSchema::new("updated", FieldType::Timestamp),
                FieldSchema::new("area", FieldType::Text).with_media_type(JSON_MEDIA_TYPE),
                FieldSchema::new("created", FieldType::Text),
            ],
            vec![
                vec![t("a.md"), t("Expired assets"), t("About Expiration safety."), t("draft"), FieldValue::Int(10), FieldValue::Timestamp(1_759_276_800_000_000), t(r#"["core/assets","docs"]"#), t("2026-09-15")],
                vec![t("b.md"), t("Store"), t("Nothing here"), t("closed"), FieldValue::Int(20), FieldValue::Null, t(r#"["core/store"]"#), t("2026-10-02")],
                vec![t("c.md"), FieldValue::Null, t("expiration"), t("draft"), FieldValue::Null, FieldValue::Timestamp(0), t("not json"), FieldValue::Null],
            ],
        )
    }

    fn rows(text: &str, options: &SearchOptions) -> Result<Vec<usize>, Error> {
        let v = docs();
        let expr = SearchExpr::try_from(text)?;
        let bound = expr.bind(v.schema(), options)?;
        Ok(bound.matcher.mask(v.as_ref())?.iter_ones().collect())
    }

    fn rows_default(text: &str) -> Vec<usize> {
        rows(text, &SearchOptions::default()).unwrap_or_else(|e| panic!("{text}: {e}"))
    }

    #[test]
    fn terms_match_case_insensitively_by_default() {
        assert_eq!(rows_default("expiration"), vec![0, 2]);
        assert_eq!(rows_default("EXPIRED"), vec![0]);
        assert_eq!(rows_default("\"expiration   SAFETY\""), vec![0]);
    }

    #[test]
    fn case_sensitive_flag_is_respected() {
        let cs = SearchOptions { case_sensitive: true, ..SearchOptions::default() };
        assert_eq!(rows("expiration", &cs).expect("cs"), vec![2]);
        assert_eq!(rows("/Expir/", &cs).expect("cs"), vec![0]);
        assert_eq!(rows_default("/expir/"), vec![0, 2]);
        assert_eq!(rows("status:DRAFT", &cs).expect("cs"), Vec::<usize>::new());
        assert_eq!(rows_default("status:DRAFT"), vec![0, 2]);
    }

    #[test]
    fn explicit_fields_restrict_terms() {
        let only_title = SearchOptions { fields: vec!["title".into()], ..SearchOptions::default() };
        assert_eq!(rows("expir", &only_title).expect("title"), vec![0]);
    }

    #[test]
    fn explicit_unknown_search_field_is_error() {
        let bad = SearchOptions { fields: vec!["nosuch".into()], ..SearchOptions::default() };
        let error = rows("x", &bad).expect_err("unknown field");
        assert!(error.to_string().contains("nosuch"), "{error}");
    }

    #[test]
    fn default_fields_prefer_roles_then_names_then_text() {
        let v = docs();
        assert_eq!(search_fields(v.schema(), &[]).expect("roles"), vec![1, 2]);
        let named = view(
            vec![
                FieldSchema::new("x", FieldType::Text),
                FieldSchema::new("title", FieldType::Text),
                FieldSchema::new("n", FieldType::Int),
            ],
            vec![],
        );
        assert_eq!(search_fields(named.schema(), &[]).expect("names"), vec![1]);
        let plain = view(
            vec![FieldSchema::new("x", FieldType::Text), FieldSchema::new("n", FieldType::Int)],
            vec![],
        );
        assert_eq!(search_fields(plain.schema(), &[]).expect("text"), vec![0]);
    }

    #[test]
    fn unknown_field_test_warns_and_matches_nothing() {
        let v = docs();
        let bound = SearchExpr::try_from("nosuch:x").expect("parse").bind(v.schema(), &SearchOptions::default()).expect("bind");
        assert_eq!(bound.warnings.len(), 1);
        assert!(bound.warnings[0].contains("nosuch"), "{:?}", bound.warnings);
        assert_eq!(bound.matcher.mask(v.as_ref()).expect("mask").count_ones(), 0);
    }

    #[test]
    fn nested_text_predicate_is_field_scoped() {
        assert_eq!(rows_default("content:(expiration -safety)"), vec![2]);
        assert_eq!(rows_default("title:(expir)"), vec![0]);
    }

    #[test]
    fn json_column_paths_and_any_element() {
        assert_eq!(rows_default("area:core/assets"), vec![0]);
        assert_eq!(rows_default("area:core/*"), vec![0, 1]);
        let nested = view(
            vec![FieldSchema::new("attr", FieldType::Text).with_media_type(JSON_MEDIA_TYPE)],
            vec![
                vec![t(r#"{"status":"draft","links":{"github":12},"tags":["a","b"]}"#)],
                vec![t(r#"{"status":"closed","links":{"github":3}}"#)],
            ],
        );
        let pick = |text: &str| -> Vec<usize> {
            let bound = SearchExpr::try_from(text).expect("parse").bind(nested.schema(), &SearchOptions::default()).expect("bind");
            bound.matcher.mask(nested.as_ref()).expect("mask").iter_ones().collect()
        };
        assert_eq!(pick("attr.status:draft"), vec![0]);
        assert_eq!(pick("attr:(status:draft)"), vec![0]);
        assert_eq!(pick("attr.links.github:>10"), vec![0]);
        assert_eq!(pick("attr.tags:b"), vec![0]);
        assert_eq!(pick("attr:(closed)"), vec![1]);
    }

    #[test]
    fn nested_field_test_requires_json_column() {
        let v = docs();
        let error = SearchExpr::try_from("content:(status:x)").expect("parse").bind(v.schema(), &SearchOptions::default());
        assert!(error.is_err());
        let error = SearchExpr::try_from("title.sub:x").expect("parse").bind(v.schema(), &SearchOptions::default());
        assert!(error.is_err());
    }

    #[test]
    fn invalid_json_cell_counts_warning() {
        let v = docs();
        let bound = SearchExpr::try_from("area:(x:y)").expect("parse").bind(v.schema(), &SearchOptions::default()).expect("bind");
        bound.matcher.mask(v.as_ref()).expect("mask");
        let warnings = bound.matcher.take_warnings();
        assert_eq!(warnings.len(), 1, "{warnings:?}");
        assert!(warnings[0].starts_with("1 cell"), "{warnings:?}");
        assert!(bound.matcher.take_warnings().is_empty());
    }

    #[test]
    fn wildcard_on_non_text_column_is_error() {
        assert!(rows("size:1*", &SearchOptions::default()).is_err());
        assert!(rows("size:/1/", &SearchOptions::default()).is_err());
        assert_eq!(rows_default("size:10"), vec![0]);
        assert_eq!(rows_default("size:>=15"), vec![1]);
    }

    #[test]
    fn presence_test_on_non_text_column() {
        assert_eq!(rows_default("size:*"), vec![0, 1]);
        assert_eq!(rows_default("-title:*"), vec![2]);
    }

    #[test]
    fn date_only_compare_on_timestamp() {
        // Row 0 is 2025-10-01T00:00:00Z, row 2 the epoch; row 1 is null.
        assert_eq!(rows_default("updated:>=2025-10-01"), vec![0]);
        assert_eq!(rows_default("updated:<2000-01-01"), vec![2]);
    }

    #[test]
    fn text_compare_is_lexicographic() {
        assert_eq!(rows_default("created:>2026-09"), vec![0, 1]);
        assert_eq!(rows_default("created:>=2026-10"), vec![1]);
    }

    #[test]
    fn evidence_reports_positive_fields_only() {
        let v = docs();
        let bound = SearchExpr::try_from("expir -nothing").expect("parse").bind(v.schema(), &SearchOptions::default()).expect("bind");
        let evidence = bound.matcher.evidence(v.as_ref()).expect("evidence");
        let fields: Vec<usize> = evidence.iter().map(|(f, _)| *f).collect();
        assert_eq!(fields, vec![1, 2]);
        assert_eq!(evidence[0].1.iter_ones().collect::<Vec<_>>(), vec![0]);
    }

    #[test]
    fn glob_matching() {
        let g = Glob::new("a*b?c", false);
        assert!(g.matches("aXXbYc"));
        assert!(g.matches("AbZc"));
        assert!(!g.matches("abc"));
        assert!(Glob::new("*", false).matches(""));
        assert!(Glob::new("specs/issues/*", true).matches("specs/issues/x.md"));
    }
}
