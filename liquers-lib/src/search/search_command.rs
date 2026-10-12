//! `ns-search/search`. Named `search_command` to avoid the `search::search` module inception.

use std::sync::Arc;

use liquers_core::context::{Context, Environment};
use liquers_core::error::Error;
use liquers_core::state::State;

use crate::records::{ChunkResolver, ContextResolver, RecordView, DEFAULT_MATERIALIZE_MAX_ROWS};
use crate::records::search::{search_view, FilteredSource, Predicate, SearchExpr, SearchOptions, SearchReport};
use crate::value::{ExtValueInterface, Value};

use super::{non_negative, search_input, SearchInput};

async fn report_to<E: Environment<Value = Value>>(
    report: &SearchReport,
    context: &Context<E>,
) -> Result<(), Error> {
    let description = report.description();
    context.set_description(&description).await?;
    context.info(&description)?;
    for warning in &report.warnings {
        context.warning(warning)?;
    }
    Ok(())
}

/// `ns-search/search`. See the command's `doc`.
pub async fn search<E: Environment<Value = Value>>(
    state: State<Value>,
    query: String,
    rank: bool,
    case_sensitive: bool,
    limit: i64,
    fields: Vec<String>,
    context: Context<E>,
) -> Result<Value, Error> {
    let limit = non_negative(limit, "limit")?;
    let options = SearchOptions {
        case_sensitive,
        fields,
        limit: (limit > 0).then_some(limit),
    };
    let (expr, parse_error) = SearchExpr::parse_lenient(&query);
    if let Some(error) = parse_error {
        context.warning(&format!("{error}; searching the words of the expression instead"))?;
    }
    let predicate: Arc<dyn Predicate> = Arc::new(expr);
    let view = match search_input(state, &context).await? {
        SearchInput::View(view) => view,
        SearchInput::Source(source) => {
            if !rank {
                if let Some(schema) = source.schema() {
                    for warning in predicate.bind(&schema, &options)?.warnings {
                        context.warning(&warning)?;
                    }
                }
                let description = match options.limit {
                    Some(limit) => format!("first {limit} matches; more may exist"),
                    None => "all matches, filtered lazily".to_string(),
                };
                context.set_description(&description).await?;
                let filtered = FilteredSource::new(source, predicate, options);
                return Ok(Value::from_record_source(Arc::new(filtered)));
            }
            let resolver: Arc<dyn ChunkResolver> = Arc::new(ContextResolver::new(context.clone()));
            let batch = source
                .materialize(resolver, DEFAULT_MATERIALIZE_MAX_ROWS)
                .await
                .map_err(|error| {
                    Error::general_error(format!(
                        "{error}; ranking needs every row in memory, so for a source this large use \
                         rank=false, which filters lazily"
                    ))
                })?;
            batch as Arc<dyn RecordView>
        }
    };
    let (out, report) = search_view(&view, predicate.as_ref(), &options, rank)?;
    report_to(&report, &context).await?;
    Ok(Value::from_record_view(out))
}
