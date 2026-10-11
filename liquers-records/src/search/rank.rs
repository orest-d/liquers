//! [`search_view`]: filter a view by a predicate and, when ranking, score the surviving rows with
//! BM25, append `score`, `matched` and `excerpt`, sort them, and apply the limit.
//!
//! Constants (`phase2-details.md` §D): k1 = 1.2, b = 0.75; statistics come from the rows that
//! survive the filter; field weights `title` 3, `filename`/`name`/`description` 2, others 1; a
//! phrase counts double; term frequency is the occurrence count (the match count for a regex).

use std::sync::Arc;

use liquers_core::error::Error;
use regex::{Regex, RegexBuilder};

use crate::buffer::Bitmap;
use crate::column::FieldValue;
use crate::mutable::ColumnMut;
use crate::schema::{FieldRole, FieldSchema, FieldType};
use crate::search::kernels::{search_fields, text_at};
use crate::search::predicate::{Predicate, RankTerm, SearchOptions};
use crate::search::structure::front_matter_end;
use crate::RecordView;

const K1: f64 = 1.2;
const B: f64 = 0.75;
const EXCERPT_RADIUS: usize = 80;
/// The columns ranking appends; a re-ranked result drops its old ones first.
const RESULT_COLUMNS: [&str; 3] = ["score", "matched", "excerpt"];

/// How many rows matched and how many are returned, plus the warnings to log.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct SearchReport {
    pub matched: usize,
    pub shown: usize,
    pub warnings: Vec<String>,
}

impl SearchReport {
    /// `N of M matches shown`.
    pub fn description(&self) -> String {
        format!("{} of {} matches shown", self.shown, self.matched)
    }
}

/// Filter `view` by `predicate`; with `rank`, score, sort and append the evidence columns. Then
/// apply `options.limit`.
pub fn search_view(
    view: &Arc<dyn RecordView>,
    predicate: &dyn Predicate,
    options: &SearchOptions,
    rank: bool,
) -> Result<(Arc<dyn RecordView>, SearchReport), Error> {
    let schema = Arc::clone(view.schema());
    let bound = predicate.bind(&schema, options)?;
    let mut warnings = bound.warnings;
    let mask = bound.matcher.mask(view.as_ref())?;
    let rows: Vec<usize> = mask.iter_ones().collect();
    let matched = rows.len();
    let limit = options.limit.unwrap_or(usize::MAX);

    if !rank {
        let filtered = view.filter(&mask)?;
        let shown = matched.min(limit);
        let out = if shown < matched { filtered.slice(0, shown)? } else { filtered };
        warnings.extend(bound.matcher.take_warnings());
        return Ok((out, SearchReport { matched, shown, warnings }));
    }

    let fields = search_fields(&schema, &options.fields)?;
    let terms = predicate.rank_terms();
    let regexes = terms
        .iter()
        .map(|term| term_regex(term, options))
        .collect::<Result<Vec<_>, _>>()?;
    let evidence = bound.matcher.evidence(view.as_ref())?;
    let scores = bm25(view.as_ref(), &rows, &fields, &terms, &regexes)?;

    let mut order: Vec<usize> = (0..rows.len()).collect();
    if !terms.is_empty() {
        order.sort_by(|&a, &b| {
            scores[b].partial_cmp(&scores[a]).unwrap_or(std::cmp::Ordering::Equal)
        });
    }
    order.truncate(limit);

    let mut score_col = ColumnMut::with_capacity(FieldType::Float, order.len());
    let mut matched_col = ColumnMut::with_capacity(FieldType::Text, order.len());
    let mut excerpt_col = ColumnMut::with_capacity(FieldType::Text, order.len());
    let columns: Vec<_> = fields.iter().map(|&f| view.column(f)).collect::<Result<_, _>>()?;
    let mut indices = Vec::with_capacity(order.len());
    for &i in &order {
        let row = rows[i];
        indices.push(u32::try_from(row).map_err(|_| {
            Error::general_error(format!("search: row {row} is beyond the 32-bit row range"))
        })?);
        score_col.push(&FieldValue::Float(scores[i]))?;
        let hits: Vec<usize> = fields
            .iter()
            .copied()
            .filter(|f| evidence.iter().any(|(ef, bits)| ef == f && bit(bits, row)))
            .collect();
        let names: Vec<&str> = hits.iter().map(|&f| schema.fields[f].name.as_str()).collect();
        matched_col.push(&if names.is_empty() {
            FieldValue::Null
        } else {
            FieldValue::Text(Arc::from(names.join(",").as_str()))
        })?;
        let excerpt = fields
            .iter()
            .zip(columns.iter())
            .filter(|(f, _)| hits.contains(f))
            .find_map(|(_, column)| text_at(column, row).and_then(|text| excerpt(text, &regexes)));
        excerpt_col.push(&match excerpt {
            Some(text) => FieldValue::Text(Arc::from(text.as_str())),
            None => FieldValue::Null,
        })?;
    }

    let base = without_result_columns(view)?;
    let taken = base.take(&indices)?;
    let out = taken.with_columns(
        vec![
            FieldSchema::new("score", FieldType::Float).with_role(FieldRole::numeric()),
            FieldSchema::new("matched", FieldType::Text).with_role(FieldRole::keyword()),
            FieldSchema::new("excerpt", FieldType::Text).with_role(FieldRole::stored_only()),
        ],
        vec![score_col.freeze(), matched_col.freeze(), excerpt_col.freeze()],
    )?;
    warnings.extend(bound.matcher.take_warnings());
    let shown = order.len();
    Ok((out, SearchReport { matched, shown, warnings }))
}

fn bit(bits: &Bitmap, row: usize) -> bool {
    row < bits.len() && bits.get(row)
}

fn without_result_columns(view: &Arc<dyn RecordView>) -> Result<Arc<dyn RecordView>, Error> {
    let fields = &view.schema().fields;
    if !fields.iter().any(|f| RESULT_COLUMNS.contains(&f.name.as_str())) {
        return Ok(Arc::clone(view));
    }
    let keep: Vec<&str> = fields
        .iter()
        .map(|f| f.name.as_str())
        .filter(|name| !RESULT_COLUMNS.contains(name))
        .collect();
    view.select_columns(&keep)
}

/// Every rank term as a regex, so occurrences and the excerpt position come from one matcher.
fn term_regex(term: &RankTerm, options: &SearchOptions) -> Result<Regex, Error> {
    let pattern = match term {
        RankTerm::Term(text) => regex::escape(text),
        RankTerm::Phrase(text) => text
            .split_whitespace()
            .map(regex::escape)
            .collect::<Vec<_>>()
            .join(r"\s+"),
        RankTerm::Regex(pattern) => pattern.clone(),
    };
    RegexBuilder::new(&pattern)
        .case_insensitive(!options.case_sensitive)
        .build()
        .map_err(|error| Error::general_error(format!("invalid regex /{pattern}/: {error}")))
}

fn field_weight(name: &str) -> f64 {
    match name {
        "title" => 3.0,
        "filename" | "name" | "description" => 2.0,
        _ => 1.0,
    }
}

fn token_count(text: &str) -> usize {
    text.split(|c: char| !c.is_alphanumeric()).filter(|t| !t.is_empty()).count()
}

/// One BM25 score per entry of `rows`.
fn bm25(
    view: &dyn RecordView,
    rows: &[usize],
    fields: &[usize],
    terms: &[RankTerm],
    regexes: &[Regex],
) -> Result<Vec<f64>, Error> {
    let n = rows.len();
    let mut scores = vec![0.0; n];
    if n == 0 || terms.is_empty() {
        return Ok(scores);
    }
    // tf[field][term][row index] and document lengths per field.
    let mut tf = Vec::with_capacity(fields.len());
    let mut lengths = Vec::with_capacity(fields.len());
    for &field in fields {
        let column = view.column(field)?;
        let mut field_tf = vec![vec![0usize; n]; regexes.len()];
        let mut field_len = vec![0usize; n];
        for (i, &row) in rows.iter().enumerate() {
            if let Some(text) = text_at(&column, row) {
                field_len[i] = token_count(text);
                for (t, regex) in regexes.iter().enumerate() {
                    field_tf[t][i] = regex.find_iter(text).count();
                }
            }
        }
        tf.push(field_tf);
        lengths.push(field_len);
    }
    let weights: Vec<f64> = fields
        .iter()
        .map(|&f| field_weight(&view.schema().fields[f].name))
        .collect();
    for t in 0..regexes.len() {
        let df = (0..n).filter(|&i| tf.iter().any(|field_tf| field_tf[t][i] > 0)).count();
        let idf = (1.0 + (n as f64 - df as f64 + 0.5) / (df as f64 + 0.5)).ln();
        let boost = match terms[t] {
            RankTerm::Phrase(_) => 2.0,
            RankTerm::Term(_) | RankTerm::Regex(_) => 1.0,
        };
        for f in 0..fields.len() {
            let avgdl = (lengths[f].iter().sum::<usize>() as f64 / n as f64).max(1.0);
            for i in 0..n {
                let freq = tf[f][t][i] as f64;
                if freq > 0.0 {
                    let dl = lengths[f][i] as f64;
                    let part = freq * (K1 + 1.0) / (freq + K1 * (1.0 - B + B * dl / avgdl));
                    scores[i] += weights[f] * boost * idf * part;
                }
            }
        }
    }
    Ok(scores)
}

/// About 160 characters around the first match of any rank term, after any front-matter.
fn excerpt(text: &str, regexes: &[Regex]) -> Option<String> {
    let body_start = front_matter_end(text);
    let body = &text[body_start..];
    let hit = regexes.iter().filter_map(|r| r.find(body)).min_by_key(|m| m.start())?;
    let mut start = hit.start().saturating_sub(EXCERPT_RADIUS);
    while !body.is_char_boundary(start) {
        start -= 1;
    }
    let mut end = (hit.end() + EXCERPT_RADIUS).min(body.len());
    while !body.is_char_boundary(end) {
        end += 1;
    }
    let mut out = String::new();
    if start > 0 {
        out.push('…');
    }
    out.push_str(&body[start..end].split_whitespace().collect::<Vec<_>>().join(" "));
    if end < body.len() {
        out.push('…');
    }
    Some(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mutable::{RecordBatchMut, RecordViewMut};
    use crate::schema::RecordSchema;
    use crate::search::syntax::SearchExpr;

    fn t(text: &str) -> FieldValue {
        FieldValue::Text(Arc::from(text))
    }

    fn docs() -> Arc<dyn RecordView> {
        let schema = Arc::new(
            RecordSchema::new(vec![
                FieldSchema::new("key", FieldType::Text),
                FieldSchema::new("title", FieldType::Text).with_role(FieldRole::text()),
                FieldSchema::new("content", FieldType::Text).with_role(FieldRole::text()),
            ])
            .expect("schema"),
        );
        let mut batch = RecordBatchMut::with_capacity(schema, 4);
        for row in [
            [t("a"), t("Notes"), t("---\nstatus: x\n---\nsome expiration text, and more expiration")],
            [t("b"), t("Expiration safety"), t("short")],
            [t("c"), t("Other"), t("nothing")],
            [t("d"), t("Misc"), t("one expiration mention in a much longer body of text with many words in it")],
        ] {
            batch.append_row(&row).expect("row");
        }
        Arc::new(batch.freeze().expect("freeze"))
    }

    fn run(text: &str, options: SearchOptions, rank: bool) -> (Arc<dyn RecordView>, SearchReport) {
        let expr = SearchExpr::try_from(text).expect("parse");
        search_view(&docs(), &expr, &options, rank).expect("search")
    }

    fn keys(view: &Arc<dyn RecordView>) -> Vec<String> {
        let column = view.column(0).expect("key");
        (0..view.len()).map(|i| text_at(&column, i).unwrap_or("").to_string()).collect()
    }

    fn cell(view: &Arc<dyn RecordView>, name: &str, row: usize) -> FieldValue {
        let index = view.schema().index_of(name).expect(name);
        view.value(row, index).expect("cell")
    }

    #[test]
    fn bm25_orders_by_relevance_with_field_weights() {
        let (view, report) = run("expiration", SearchOptions::default(), true);
        assert_eq!(report.matched, 3);
        // The title match (weight 3) outranks two body occurrences, which outrank one in a long body.
        assert_eq!(keys(&view), vec!["b", "a", "d"]);
        let scores: Vec<f64> = (0..3)
            .map(|i| match cell(&view, "score", i) {
                FieldValue::Float(f) => f,
                other => panic!("{other:?}"),
            })
            .collect();
        assert!(scores[0] > scores[1] && scores[1] > scores[2], "{scores:?}");
    }

    #[test]
    fn matched_and_excerpt_columns() {
        let (view, _) = run("expiration", SearchOptions::default(), true);
        assert_eq!(cell(&view, "matched", 0), t("title"));
        assert_eq!(cell(&view, "matched", 1), t("content"));
        match cell(&view, "excerpt", 1) {
            FieldValue::Text(text) => {
                assert!(text.contains("expiration"), "{text}");
                assert!(!text.contains("---") && !text.contains("status"), "{text}");
            }
            other => panic!("{other:?}"),
        }
        // Ranking a ranked result replaces the evidence columns instead of duplicating them.
        let expr = SearchExpr::try_from("expiration").expect("parse");
        let (again, _) = search_view(&view, &expr, &SearchOptions::default(), true).expect("again");
        assert_eq!(again.schema().fields.len(), view.schema().fields.len());
    }

    #[test]
    fn unranked_filter_only_and_empty_keep_input_order() {
        let (view, _) = run("expiration", SearchOptions::default(), false);
        assert_eq!(keys(&view), vec!["a", "b", "d"]);
        let (view, _) = run("-title:Other", SearchOptions::default(), true);
        assert_eq!(keys(&view), vec!["a", "b", "d"]);
        let (view, report) = run("", SearchOptions::default(), true);
        assert_eq!(keys(&view), vec!["a", "b", "c", "d"]);
        assert_eq!(report.matched, 4);
    }

    #[test]
    fn limit_zero_is_unlimited_negative_is_error() {
        // The command maps `limit = 0` to `None`; here `None` returns every match.
        let (view, _) = run("expiration", SearchOptions { limit: None, ..SearchOptions::default() }, true);
        assert_eq!(view.len(), 3);
        let (view, report) = run("expiration", SearchOptions { limit: Some(1), ..SearchOptions::default() }, false);
        assert_eq!(view.len(), 1);
        assert_eq!(report.description(), "1 of 3 matches shown");
    }

    #[test]
    fn report_counts_matched_and_shown() {
        let (_, report) = run("expiration", SearchOptions { limit: Some(2), ..SearchOptions::default() }, true);
        assert_eq!((report.matched, report.shown), (3, 2));
        assert_eq!(report.description(), "2 of 3 matches shown");
    }
}
