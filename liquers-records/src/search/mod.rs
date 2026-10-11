//! Search over records: a predicate written in a short syntax, matched column-wise against any
//! [`RecordView`](crate::RecordView) or [`RecordSource`](crate::RecordSource), and optionally
//! ranked by BM25.
//!
//! Design: `specs/design/store-and-asset-search/` (Phase 2 and `phase2-details.md`). The commands
//! that put this to work (`ns-search`) live in `liquers-lib`, because they need its `Value`.
//!
//! - [`predicate`] — the [`Predicate`] trait every query language implements, and the options.
//! - [`syntax`] — the built-in language: its AST ([`SearchExpr`]), grammar and parser.
//! - [`kernels`] — binding a predicate to a schema and evaluating it to a [`Bitmap`](crate::Bitmap).
//! - [`rank`] — [`search_view`]: filter, BM25 ranking, evidence and the limit.
//! - [`structure`] — front-matter, JSON and YAML parsing for `field_extract`.
//! - [`source`] — [`FilteredSource`], a lazily filtered `RecordSource`.

pub mod kernels;
pub mod predicate;
pub mod rank;
pub mod source;
pub mod structure;
pub mod syntax;

pub use kernels::{is_json_column, search_fields, DEFAULT_SEARCH_FIELDS};
pub use predicate::{Bound, BoundPredicate, Predicate, PredicateSyntax, RankTerm, SearchOptions};
pub use rank::{search_view, SearchReport};
pub use source::FilteredSource;
pub use structure::{front_matter_end, parse_structure, parse_structure_with_report, StructFormat};
pub use syntax::{FieldTest, Node, SearchExpr};
