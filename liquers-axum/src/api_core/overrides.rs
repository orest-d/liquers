//! Argument overrides from an HTTP request (`specs/design/axum-query-named-arguments/`).
//!
//! An **argument override** names an argument of the query's last action outside the query text:
//! a URL query parameter (`?limit=20`), or a member of a POST JSON object body
//! (`{"limit": 20}`). Names beginning with `_` are reserved for options: `_link_<name>` gives
//! argument `<name>` as a link (a query evaluated for its value), and `_entry_format` selects
//! the encoding of a `DataEntry` on the routes that answer one.
//!
//! A request with no override is served exactly as before — a managed asset where the manager's
//! rules make it one. Only a request carrying at least one override is evaluated ad hoc, as an
//! unmanaged asset; an empty query string, an empty body, `{}` or options alone never are.

use std::collections::BTreeMap;

use liquers_core::{
    command_metadata::{ArgumentInfo, CommandMetadataRegistry},
    error::{Error, ErrorType},
    parse::parse_query,
    plan::{ParameterValue, Plan, PlanBuilder, Step},
    query::{Position, Query},
    recipes::Recipe,
};
use serde_json::Value as JsonValue;

use crate::api_core::format::{parse_entry_format, ENTRY_FORMAT_OPTION};
use crate::api_core::SerializationFormat;

/// The prefix of a link override's name: `_link_<argument>`.
pub const LINK_PREFIX: &str = "_link_";

/// How an override arrived: as URL text (one entry per occurrence of the key, in order), or as a
/// typed JSON value from the body.
#[derive(Debug, Clone, PartialEq)]
pub enum OverrideInput {
    Text(Vec<String>),
    Json(JsonValue),
}

/// Which options a route accepts besides `_link_<name>`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RouteOptions {
    /// None: every other `_` name is reserved.
    None,
    /// An entry route: `_entry_format` is accepted.
    Entry,
}

/// The overrides and options of one request.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct OverrideRequest {
    /// Argument overrides by argument name.
    pub values: BTreeMap<String, OverrideInput>,
    /// Link overrides (`_link_<name>`) by argument name, as query text.
    pub links: BTreeMap<String, String>,
    /// `_entry_format`, on a route that accepts it.
    pub entry_format: Option<SerializationFormat>,
}

fn parameter_error(message: String) -> Error {
    Error::from_error(ErrorType::ParameterError, message)
}

impl OverrideRequest {
    /// Collect the overrides and options of a request: its query-string pairs, in order, and its
    /// body. A zero-length body is no body, whatever its `Content-Type`; a non-empty one must be a
    /// JSON object. Refuses a name given both in the query string and in the body, an argument
    /// given both as a value and as a link, and a reserved name the route does not accept.
    pub fn parse(
        pairs: Vec<(String, String)>,
        body: &[u8],
        options: RouteOptions,
    ) -> Result<Self, Error> {
        let mut request = OverrideRequest::default();
        let mut text: BTreeMap<String, Vec<String>> = BTreeMap::new();
        for (name, value) in pairs {
            if let Some(argument) = name.strip_prefix(LINK_PREFIX) {
                request.add_link(argument, value)?;
            } else if name == ENTRY_FORMAT_OPTION {
                request.set_entry_format(&value, options)?;
            } else if name.starts_with('_') {
                return Err(reserved(&name));
            } else {
                text.entry(name).or_default().push(value);
            }
        }
        for (name, values) in text {
            request.values.insert(name, OverrideInput::Text(values));
        }

        if !body.is_empty() {
            let json: JsonValue = serde_json::from_slice(body)
                .map_err(|e| parameter_error(format!("the request body must be JSON: {e}")))?;
            let JsonValue::Object(members) = json else {
                return Err(parameter_error(
                    "the request body must be a JSON object of argument overrides".to_string(),
                ));
            };
            for (name, value) in members {
                if let Some(argument) = name.strip_prefix(LINK_PREFIX) {
                    let JsonValue::String(link) = value else {
                        return Err(parameter_error(format!(
                            "'{name}' must be a query as a JSON string"
                        )));
                    };
                    request.add_link(argument, link)?;
                } else if name == ENTRY_FORMAT_OPTION {
                    let JsonValue::String(format) = value else {
                        return Err(parameter_error(format!(
                            "'{ENTRY_FORMAT_OPTION}' must be a JSON string"
                        )));
                    };
                    request.set_entry_format(&format, options)?;
                } else if name.starts_with('_') {
                    return Err(reserved(&name));
                } else if request.values.contains_key(&name) {
                    return Err(parameter_error(format!(
                        "argument '{name}' is given both in the query string and in the body"
                    )));
                } else {
                    request.values.insert(name, OverrideInput::Json(value));
                }
            }
        }

        if let Some(name) = request.links.keys().find(|n| request.values.contains_key(*n)) {
            return Err(parameter_error(format!(
                "argument '{name}' is given both as a value and as a link ({LINK_PREFIX}{name})"
            )));
        }
        Ok(request)
    }

    fn add_link(&mut self, argument: &str, link: String) -> Result<(), Error> {
        if argument.is_empty() {
            return Err(parameter_error(format!("'{LINK_PREFIX}' needs an argument name")));
        }
        if self.links.insert(argument.to_string(), link).is_some() {
            return Err(parameter_error(format!(
                "'{LINK_PREFIX}{argument}' is given more than once"
            )));
        }
        Ok(())
    }

    fn set_entry_format(&mut self, value: &str, options: RouteOptions) -> Result<(), Error> {
        match options {
            RouteOptions::Entry => {
                if self.entry_format.is_some() {
                    return Err(parameter_error(format!(
                        "'{ENTRY_FORMAT_OPTION}' is given more than once"
                    )));
                }
                self.entry_format = Some(parse_entry_format(value)?);
                Ok(())
            }
            RouteOptions::None => Err(reserved(ENTRY_FORMAT_OPTION)),
        }
    }

    /// No argument override at all; options do not count. Such a request is served exactly as
    /// one without a query string or body.
    pub fn is_empty(&self) -> bool {
        self.values.is_empty() && self.links.is_empty()
    }

    /// The recipe evaluating `query` with these overrides, validated by building its plan once,
    /// so that an unknown name (`ParameterError`) or an unconvertible value (`ConversionError`)
    /// fails before anything is evaluated.
    ///
    /// URL text converts exactly as a path parameter of the argument would
    /// (`ParameterValue::from_string`), without a warning; a JSON value is passed through and
    /// converted, with a warning in the log when its type does not match, by `Recipe::to_plan`.
    pub fn to_recipe(&self, query: &Query, cmr: &CommandMetadataRegistry) -> Result<Recipe, Error> {
        // Placeholders allowed, as `Recipe::to_plan` does: a required argument the path leaves
        // out is exactly what an override may supply.
        let plan = PlanBuilder::new(query.clone(), cmr)
            .with_placeholders_allowed()
            .build()?;
        let command = plan.last_action_command(cmr);
        let mut recipe = Recipe::new(query.encode(), String::new(), String::new())?;
        for (name, input) in &self.values {
            let argument = command
                .as_ref()
                .and_then(|c| c.arguments.iter().find(|a| &a.name == name));
            match (input, argument) {
                (OverrideInput::Json(value), _) => {
                    recipe.arguments.insert(name.clone(), value.clone());
                }
                // Not an argument (or no action): `to_plan` below reports it as a ParameterError.
                (OverrideInput::Text(texts), None) => {
                    let value = texts.first().cloned().unwrap_or_default();
                    recipe.arguments.insert(name.clone(), JsonValue::String(value));
                }
                (OverrideInput::Text(texts), Some(argument)) => {
                    match text_to_override(argument, texts).map_err(|e| e.with_query(query))? {
                        TextOverride::Value(value) => {
                            recipe.arguments.insert(name.clone(), value);
                        }
                        TextOverride::Link(link) => {
                            recipe.links.insert(name.clone(), link.encode());
                        }
                    }
                }
            }
        }
        for (name, link) in &self.links {
            parse_query(link).map_err(|e| e.with_query(query))?;
            recipe.links.insert(name.clone(), link.clone());
        }
        let plan = recipe.to_plan(cmr)?;
        if let Some(name) = unfilled_placeholder(&plan) {
            return Err(Error::from_error(
                ErrorType::ArgumentMissing,
                format!(
                    "argument '{name}' has no value: give it in the query or as an override"
                ),
            )
            .with_query(query));
        }
        Ok(recipe)
    }
}

/// The first required argument that neither the query nor an override gave a value.
fn unfilled_placeholder(plan: &Plan) -> Option<String> {
    plan.steps.iter().find_map(|step| {
        if let Step::Action { parameters, .. } = step {
            parameters.iter().find_map(|p| {
                if let ParameterValue::Placeholder(name) = p {
                    Some(name.clone())
                } else {
                    None
                }
            })
        } else {
            None
        }
    })
}

fn reserved(name: &str) -> Error {
    parameter_error(format!(
        "'{name}' is reserved: names beginning with '_' are options, and this route does not \
         accept it (argument overrides use the argument's own name)"
    ))
}

/// URL text converted for one argument: a value, or a link (an enum alias naming a query).
enum TextOverride {
    Value(JsonValue),
    Link(Query),
}

fn text_to_override(argument: &ArgumentInfo, texts: &[String]) -> Result<TextOverride, Error> {
    if argument.multiple {
        let mut values = Vec::with_capacity(texts.len());
        for text in texts {
            match text_to_override(&single(argument), std::slice::from_ref(text))? {
                TextOverride::Value(value) => values.push(value),
                TextOverride::Link(link) => {
                    return Err(Error::conversion_error_with_message(
                        link.encode(),
                        &argument.name,
                        "a link alias inside a variadic override",
                    ))
                }
            }
        }
        return Ok(TextOverride::Value(JsonValue::Array(values)));
    }
    let [text] = texts else {
        return Err(parameter_error(format!(
            "argument '{}' is given more than once but takes one value",
            argument.name
        )));
    };
    match ParameterValue::from_string(argument, text, &Position::unknown())? {
        ParameterValue::EnumLink(_, link, _) => Ok(TextOverride::Link(link)),
        other => other.value().map(TextOverride::Value).ok_or_else(|| {
            Error::conversion_error_with_message(text, &argument.name, "no value for this text")
        }),
    }
}

/// The argument as a single element, for converting one element of a variadic.
fn single(argument: &ArgumentInfo) -> ArgumentInfo {
    let mut element = argument.clone();
    element.multiple = false;
    element
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pairs(list: &[(&str, &str)]) -> Vec<(String, String)> {
        list.iter()
            .map(|(k, v)| (k.to_string(), v.to_string()))
            .collect()
    }

    fn error_type(result: Result<OverrideRequest, Error>) -> Option<ErrorType> {
        result.err().map(|e| e.error_type)
    }

    #[test]
    fn parse_query_string_repeated_keys_keep_order() -> Result<(), Error> {
        let request = OverrideRequest::parse(
            pairs(&[("fields", "b"), ("limit", "3"), ("fields", "a")]),
            b"",
            RouteOptions::None,
        )?;
        assert_eq!(
            request.values.get("fields"),
            Some(&OverrideInput::Text(vec!["b".to_string(), "a".to_string()]))
        );
        assert_eq!(
            request.values.get("limit"),
            Some(&OverrideInput::Text(vec!["3".to_string()]))
        );
        Ok(())
    }

    #[test]
    fn parse_empty_body_with_content_type_is_none() -> Result<(), Error> {
        // The parser sees only bytes: a zero-length body is no body, whatever header came with it.
        let request = OverrideRequest::parse(vec![], b"", RouteOptions::None)?;
        assert!(request.is_empty());
        assert_eq!(request, OverrideRequest::default());
        Ok(())
    }

    #[test]
    fn parse_empty_object_and_options_only_is_empty() -> Result<(), Error> {
        assert!(OverrideRequest::parse(vec![], b"{}", RouteOptions::None)?.is_empty());
        let request =
            OverrideRequest::parse(pairs(&[("_entry_format", "json")]), b"", RouteOptions::Entry)?;
        assert!(request.is_empty());
        assert_eq!(request.entry_format, Some(SerializationFormat::Json));
        Ok(())
    }

    #[test]
    fn parse_body_not_object_rejected() {
        for body in [&b"[1, 2]"[..], b"\"text\"", b"not json"] {
            assert_eq!(
                error_type(OverrideRequest::parse(vec![], body, RouteOptions::None)),
                Some(ErrorType::ParameterError)
            );
        }
    }

    #[test]
    fn parse_duplicate_name_rejected() {
        let result = OverrideRequest::parse(
            pairs(&[("query", "a")]),
            br#"{"query": "b"}"#,
            RouteOptions::None,
        );
        assert_eq!(error_type(result), Some(ErrorType::ParameterError));
    }

    #[test]
    fn parse_reserved_name_rejected() {
        assert_eq!(
            error_type(OverrideRequest::parse(pairs(&[("_foo", "1")]), b"", RouteOptions::Entry)),
            Some(ErrorType::ParameterError)
        );
        assert_eq!(
            error_type(OverrideRequest::parse(vec![], br#"{"_foo": 1}"#, RouteOptions::None)),
            Some(ErrorType::ParameterError)
        );
    }

    #[test]
    fn parse_entry_format_only_with_entry_options() {
        assert_eq!(
            error_type(OverrideRequest::parse(
                pairs(&[("_entry_format", "json")]),
                b"",
                RouteOptions::None
            )),
            Some(ErrorType::ParameterError)
        );
        assert_eq!(
            error_type(OverrideRequest::parse(
                pairs(&[("_entry_format", "xml")]),
                b"",
                RouteOptions::Entry
            )),
            Some(ErrorType::ParameterError)
        );
    }

    #[test]
    fn parse_link_option() -> Result<(), Error> {
        let request = OverrideRequest::parse(
            pairs(&[("_link_query", "make_text")]),
            br#"{"_link_extra": "-R/a/b.txt"}"#,
            RouteOptions::None,
        )?;
        assert!(!request.is_empty());
        assert_eq!(request.links.get("query").map(String::as_str), Some("make_text"));
        assert_eq!(request.links.get("extra").map(String::as_str), Some("-R/a/b.txt"));
        Ok(())
    }

    #[test]
    fn parse_value_and_link_rejected() {
        let result = OverrideRequest::parse(
            pairs(&[("query", "a"), ("_link_query", "make_text")]),
            b"",
            RouteOptions::None,
        );
        assert_eq!(error_type(result), Some(ErrorType::ParameterError));
    }
}
