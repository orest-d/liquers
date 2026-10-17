//! Diagnostics: every problem a template has, found without evaluating anything.

use liquers_core::command_metadata::CommandMetadataRegistry;
use liquers_core::error::Error;
use liquers_core::query::{Position, Query, QuerySegment};
use serde::{Deserialize, Serialize};

use crate::mustache::ast::{Element, PartialTarget, QueryBody, QueryTag, SectionHead, Template};
use crate::query::ast::QuerySegmentTemplate;
use crate::{parse_template, TemplateSyntax};

#[derive(Serialize, Deserialize, Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Severity {
    Error,
    Warning,
}

/// One problem, positioned in the template.
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq)]
pub struct Diagnostic {
    pub severity: Severity,
    pub message: String,
    pub position: Position,
}

impl Diagnostic {
    fn error(error: &Error) -> Self {
        Diagnostic {
            severity: Severity::Error,
            message: error.message.clone(),
            position: error.position.clone(),
        }
    }
}

/// Every problem in `text`: an irrecoverable syntax error alone, or else every invalid tag and,
/// with `cmr`, every action that names no registered command. Nothing is evaluated.
pub fn validate_template(
    text: &str,
    syntax: TemplateSyntax,
    cmr: Option<&CommandMetadataRegistry>,
) -> Vec<Diagnostic> {
    let template = match parse_template(text, syntax) {
        Ok(t) => t,
        Err(e) => return vec![Diagnostic::error(&e)],
    };
    let mut out = Vec::new();
    match &template {
        Template::Mustache(m) => walk(&m.elements, cmr, &mut out),
    }
    out
}

fn walk(elements: &[Element], cmr: Option<&CommandMetadataRegistry>, out: &mut Vec<Diagnostic>) {
    for element in elements {
        match element {
            Element::Invalid(i) => out.push(Diagnostic::error(&i.error)),
            Element::Section(s) => {
                match &s.head {
                    SectionHead::Invalid(i) => out.push(Diagnostic::error(&i.error)),
                    SectionHead::Query(q) => query(q, cmr, out),
                    SectionHead::Name(_) => {}
                }
                walk(&s.elements, cmr, out);
            }
            Element::Partial(p) => match &p.target {
                PartialTarget::Invalid(i) => out.push(Diagnostic::error(&i.error)),
                PartialTarget::Query(q) => query(q, cmr, out),
                PartialTarget::Key(_) => {}
            },
            Element::Query(q) => query(q, cmr, out),
            Element::Text(_) | Element::Variable(_) | Element::Comment(_) | Element::SetDelimiter(_) => {}
        }
    }
}

fn query(tag: &QueryTag, cmr: Option<&CommandMetadataRegistry>, out: &mut Vec<Diagnostic>) {
    match &tag.body {
        QueryBody::Invalid(i) => out.push(Diagnostic::error(&i.error)),
        QueryBody::Hygienic(t) => {
            if let Some(cmr) = cmr {
                if let Some(q) = t.try_into_query() {
                    unknown_commands(&q, cmr, out);
                } else {
                    template_commands(t, cmr, out);
                }
            }
        }
        // The text is known only once its placeholders are filled.
        QueryBody::Classic(_) => {}
    }
}

fn namespaces(query: &Query, cmr: &CommandMetadataRegistry) -> Vec<String> {
    let mut namespaces: Vec<String> = query
        .last_ns()
        .into_iter()
        .flatten()
        .filter_map(|p| match p {
            liquers_core::query::ActionParameter::String(s, _) => Some(s),
            liquers_core::query::ActionParameter::Link(..) => None,
        })
        .collect();
    namespaces.extend(cmr.default_namespaces.iter().cloned());
    namespaces
}

fn unknown(name: &str, realm: &str, position: &Position, out: &mut Vec<Diagnostic>) {
    out.push(Diagnostic {
        severity: Severity::Error,
        message: if realm.is_empty() {
            format!("unknown command '{name}'")
        } else {
            format!("unknown command '{name}' in realm '{realm}'")
        },
        position: position.clone(),
    });
}

/// Instructions the planner handles itself, which are not registered commands.
fn is_instruction(name: &str) -> bool {
    matches!(name, "ns" | "q" | "v")
}

fn unknown_commands(query: &Query, cmr: &CommandMetadataRegistry, out: &mut Vec<Diagnostic>) {
    let namespaces = namespaces(query, cmr);
    for segment in &query.segments {
        match segment {
            QuerySegment::Transform(t) => {
                let realm = t.name();
                for action in &t.query {
                    if !is_instruction(&action.name)
                        && cmr.find_command_in_namespaces(&realm, &namespaces, &action.name).is_none()
                    {
                        unknown(&action.name, &realm, &action.position, out);
                    }
                    for p in &action.parameters {
                        match p {
                            liquers_core::query::ActionParameter::Link(link, _) => unknown_commands(link, cmr, out),
                            liquers_core::query::ActionParameter::String(..) => {}
                        }
                    }
                }
            }
            QuerySegment::Resource(_) => {}
        }
    }
}

/// Literal action names of a template with placeholders; a filled name is checked when it runs.
fn template_commands(
    template: &crate::query::ast::QueryTemplate,
    cmr: &CommandMetadataRegistry,
    out: &mut Vec<Diagnostic>,
) {
    let mut namespaces = cmr.default_namespaces.clone();
    for segment in &template.segments {
        if let QuerySegmentTemplate::Transform(t) = segment {
            for action in &t.query {
                if action.name.as_literal().as_deref() == Some("ns") {
                    for p in &action.parameters {
                        if let crate::query::ast::ActionParameterTemplate::String(pieces, _) = p {
                            if let Some(ns) = crate::query::ast::pieces_literal(pieces) {
                                namespaces.insert(0, ns);
                            }
                        }
                    }
                }
            }
        }
    }
    for segment in &template.segments {
        match segment {
            QuerySegmentTemplate::Transform(t) => {
                let realm = t.header.as_ref().map(|h| h.name.clone()).unwrap_or_default();
                for action in &t.query {
                    if let Some(name) = action.name.as_literal() {
                        if !is_instruction(&name)
                            && cmr.find_command_in_namespaces(&realm, &namespaces, &name).is_none()
                        {
                            unknown(&name, &realm, &action.position, out);
                        }
                    }
                    for p in &action.parameters {
                        match p {
                            crate::query::ast::ActionParameterTemplate::Link(link, _) => {
                                match link.try_into_query() {
                                    Some(q) => unknown_commands(&q, cmr, out),
                                    None => template_commands(link, cmr, out),
                                }
                            }
                            crate::query::ast::ActionParameterTemplate::String(..) => {}
                        }
                    }
                }
            }
            QuerySegmentTemplate::Resource(_) => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use liquers_core::command_metadata::CommandMetadata;

    #[test]
    fn validate_reports_all() {
        let mut cmr = CommandMetadataRegistry::new();
        cmr.add_command(&CommandMetadata::new("known"));
        let text = "{{a..b}}\n{{$ known/unknown_one}}\n{{$ known-{{x}}/to_{{fmt}}/missing_two}}\n{{/}}\n{{$ cmd/~}}";
        let diagnostics = validate_template(text, TemplateSyntax::Mustache, Some(&cmr));
        let lines: Vec<u32> = diagnostics.iter().map(|d| d.position.line).collect();
        assert_eq!(lines, vec![1, 2, 3, 4, 5], "{diagnostics:#?}");
        assert!(diagnostics[1].message.contains("unknown_one"));
        assert!(diagnostics[2].message.contains("missing_two"));
        assert!(diagnostics[3].message.contains("closes no open section"));
        let broken = validate_template("{{#a}}", TemplateSyntax::Mustache, None);
        assert_eq!(broken.len(), 1);
    }
}
