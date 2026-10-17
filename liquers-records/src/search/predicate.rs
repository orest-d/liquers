//! The predicate trait — the interface between a query language and the search machinery.

use std::fmt::Debug;
use std::sync::Arc;

use liquers_core::error::Error;
use liquers_core::maybe_send::{MaybeSend, MaybeSync};

use crate::buffer::Bitmap;
use crate::{RecordSchema, RecordView};

/// What `search` applies to records. Object-safe: the commands hold an `Arc<dyn Predicate>` and
/// never name the language it was written in, so another language can replace the built-in one
/// ([`crate::search::SearchExpr`]) by implementing this trait (and [`PredicateSyntax`]).
pub trait Predicate: Debug + MaybeSend + MaybeSync + 'static {
    /// Resolve names against one schema. Unknown field names are not errors: they become
    /// warnings, and the clause naming them matches nothing.
    fn bind(&self, schema: &RecordSchema, options: &SearchOptions) -> Result<Bound, Error>;

    /// Test one JSON value — the inner predicate of `field:(…)` on a column declared JSON
    /// (`application/json`): field paths walk object keys, an array matches when any element
    /// does, and a term matches any string leaf.
    fn matches_json(&self, value: &serde_json::Value, options: &SearchOptions)
        -> Result<bool, Error>;

    /// Whether any clause is a field test. If none is, a nested predicate is evaluated as a search
    /// restricted to its column, and nothing is parsed.
    fn has_field_tests(&self) -> bool;

    /// The positive terms, phrases and regexes (those not under a negation), for BM25 and the
    /// excerpt. Empty for a filter-only expression, which therefore keeps the input order.
    fn rank_terms(&self) -> Vec<RankTerm>;

    /// The predicate in its own language's syntax — the transport form: what is logged, stored in
    /// a recipe, or handed to an external engine.
    fn to_syntax(&self) -> String;
}

/// A language: a predicate type parseable from text. Parsing is `TryFrom<&str>`, so
/// `SearchExpr::try_from("a -b")?` is how the built-in language parses. `Sized` keeps it off the
/// trait object; a run-time registry of languages waits for the dialect argument.
pub trait PredicateSyntax: Predicate + Sized + for<'a> TryFrom<&'a str, Error = Error> {
    /// The language's name, for logs and a future dialect argument.
    const LANGUAGE: &'static str;
}

/// A predicate resolved against one schema: the matcher, plus the warnings binding produced.
pub struct Bound {
    pub matcher: Box<dyn BoundPredicate>,
    pub warnings: Vec<String>,
}

/// A predicate bound to one schema, evaluated column-wise over a view of that schema.
pub trait BoundPredicate: MaybeSend + MaybeSync {
    /// One bit per row of `view`: set where the predicate matches.
    fn mask(&self, view: &dyn RecordView) -> Result<Bitmap, Error>;

    /// For each search field (by column index), the rows in which a positive term matched — what
    /// the `matched` column reports. The default reports nothing.
    fn evidence(&self, _view: &dyn RecordView) -> Result<Vec<(usize, Bitmap)>, Error> {
        Ok(vec![])
    }

    /// Warnings found while evaluating — cells of a JSON column that are not valid JSON — taken
    /// (and cleared) by the caller after the masks are computed. The default has none.
    fn take_warnings(&self) -> Vec<String> {
        vec![]
    }
}

/// A positive clause that contributes to the score.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RankTerm {
    Term(String),
    Phrase(String),
    Regex(String),
}

/// How a predicate is applied.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct SearchOptions {
    /// Off by default: terms, phrases, regexes and text globs ignore case.
    pub case_sensitive: bool,
    /// The search fields for free-text clauses. Empty means the defaults
    /// ([`crate::search::search_fields`]).
    pub fields: Vec<String>,
    /// At most this many rows. `None` means no limit.
    pub limit: Option<usize>,
}
/// A predicate that refuses to bind when a field it requires is missing from the schema — where
/// the search language would only warn and match nothing. `ns-rec`'s single-column filters wrap
/// their expression in one, so `eq-nope-1` is an error naming the available fields.
#[derive(Debug)]
pub struct StrictPredicate {
    inner: Arc<dyn Predicate>,
    required: Vec<String>,
}

impl StrictPredicate {
    pub fn new(inner: Arc<dyn Predicate>, required: Vec<String>) -> Self {
        StrictPredicate { inner, required }
    }
}

impl Predicate for StrictPredicate {
    fn bind(&self, schema: &RecordSchema, options: &SearchOptions) -> Result<Bound, Error> {
        for name in &self.required {
            if schema.resolve_field(name)?.is_none() {
                let available: Vec<&str> = schema.fields.iter().map(|f| f.name.as_str()).collect();
                return Err(Error::general_error(format!(
                    "no field named '{name}' (available: {})",
                    available.join(", ")
                )));
            }
        }
        self.inner.bind(schema, options)
    }

    fn matches_json(&self, value: &serde_json::Value, options: &SearchOptions) -> Result<bool, Error> {
        self.inner.matches_json(value, options)
    }

    fn has_field_tests(&self) -> bool {
        self.inner.has_field_tests()
    }

    fn rank_terms(&self) -> Vec<RankTerm> {
        self.inner.rank_terms()
    }

    fn to_syntax(&self) -> String {
        self.inner.to_syntax()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::search::{FieldTest, SearchExpr};
    use crate::{FieldSchema, FieldType};

    #[test]
    fn strict_predicate_refuses_a_missing_field() -> Result<(), Error> {
        let schema = RecordSchema::new(vec![
            FieldSchema::new("order_id", FieldType::Text),
            FieldSchema::new("amount", FieldType::Float),
        ])?;
        let options = SearchOptions::default();
        let missing = StrictPredicate::new(
            Arc::new(SearchExpr::field("nope", FieldTest::Glob("x".to_string()))),
            vec!["nope".to_string()],
        );
        let message = missing.bind(&schema, &options).err().map(|e| e.to_string()).unwrap_or_default();
        assert!(message.contains("'nope'") && message.contains("order_id"), "{message}");
        let present = StrictPredicate::new(
            Arc::new(SearchExpr::field("amount", FieldTest::Glob("*".to_string()))),
            vec!["amount".to_string()],
        );
        assert!(present.bind(&schema, &options)?.warnings.is_empty());
        Ok(())
    }
}
