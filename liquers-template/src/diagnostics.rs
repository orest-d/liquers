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

/// The namespaces an action is looked up in, as the planner builds them: those of the last `ns`
/// before it in its own transform segment, then the registry's defaults.
fn lookup_namespaces(segment_ns: &[String], cmr: &CommandMetadataRegistry) -> Vec<String> {
    let mut namespaces = segment_ns.to_vec();
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
    for segment in &query.segments {
        match segment {
            QuerySegment::Transform(t) => {
                let realm = t.name();
                let mut segment_ns: Vec<String> = Vec::new();
                for action in &t.query {
                    if action.is_ns() {
                        // A link parameter is left to planning, which refuses it.
                        segment_ns = action
                            .parameters
                            .iter()
                            .filter_map(|p| match p {
                                liquers_core::query::ActionParameter::String(s, _) => {
                                    Some(s.clone())
                                }
                                liquers_core::query::ActionParameter::Link(..) => None,
                            })
                            .collect();
                    } else if !is_instruction(&action.name)
                        && cmr
                            .find_command_in_namespaces(
                                &realm,
                                &lookup_namespaces(&segment_ns, cmr),
                                &action.name,
                            )
                            .is_none()
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
/// So is every name after a placeholder namespace (`ns-{{lib}}`) in the same segment, since the
/// namespaces it is looked up in are only known then.
fn template_commands(
    template: &crate::query::ast::QueryTemplate,
    cmr: &CommandMetadataRegistry,
    out: &mut Vec<Diagnostic>,
) {
    for segment in &template.segments {
        match segment {
            QuerySegmentTemplate::Transform(t) => {
                let realm = t.header.as_ref().map(|h| h.name.clone()).unwrap_or_default();
                // The last `ns` so far in this segment; `dynamic` when one of its namespaces is a
                // placeholder, which defers the segment's commands until it is filled.
                let mut segment_ns: Vec<String> = Vec::new();
                let mut dynamic = false;
                for action in &t.query {
                    let name = action.name.as_literal();
                    if name.as_deref() == Some("ns") {
                        segment_ns = Vec::new();
                        dynamic = false;
                        for p in &action.parameters {
                            match p {
                                crate::query::ast::ActionParameterTemplate::String(pieces, _) => {
                                    match crate::query::ast::pieces_literal(pieces) {
                                        Some(ns) => segment_ns.push(ns),
                                        None => dynamic = true,
                                    }
                                }
                                crate::query::ast::ActionParameterTemplate::Link(..) => {}
                            }
                        }
                    } else if let Some(name) = name.filter(|_| !dynamic) {
                        if !is_instruction(&name)
                            && cmr
                                .find_command_in_namespaces(
                                    &realm,
                                    &lookup_namespaces(&segment_ns, cmr),
                                    &name,
                                )
                                .is_none()
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

    /// A placeholder namespace is only known at render time, so its commands are checked then.
    #[test]
    fn dynamic_namespace_defers_command_check() {
        let cmr = CommandMetadataRegistry::new();
        let dynamic = validate_template("{{$ ns-{{lib}}/cmd}}", TemplateSyntax::Mustache, Some(&cmr));
        assert!(dynamic.is_empty(), "{dynamic:#?}");
        let literal = validate_template("{{$ ns-lib/cmd-{{x}}}}", TemplateSyntax::Mustache, Some(&cmr));
        assert_eq!(literal.len(), 1, "a literal namespace is still checked: {literal:#?}");
    }

    /// `head` in `pl`, `fliph` in `img`, `known` in the default namespace.
    fn ns_registry() -> CommandMetadataRegistry {
        let mut cmr = CommandMetadataRegistry::new();
        let mut head = CommandMetadata::new("head");
        head.namespace = "pl".to_string();
        cmr.add_command(&head);
        let mut fliph = CommandMetadata::new("fliph");
        fliph.namespace = "img".to_string();
        cmr.add_command(&fliph);
        cmr.add_command(&CommandMetadata::new("known"));
        cmr
    }

    fn diagnose(text: &str) -> Vec<Diagnostic> {
        validate_template(text, TemplateSyntax::Mustache, Some(&ns_registry()))
    }

    /// AC-1: each action sees the last `ns` before it, not the query's last `ns`.
    #[test]
    fn later_ns_replaces_earlier_ns() {
        let d = diagnose("{{$ ns-pl/head-3/ns-img/fliph}}");
        assert!(d.is_empty(), "{d:#?}");
    }

    /// AC-2: an `ns` applies within its own transform segment only. (The leading `-/` makes the
    /// first segment a transform; without it, `a/-/b` reads `a` as a resource key.)
    #[test]
    fn ns_does_not_cross_a_segment_boundary() {
        let d = diagnose("{{$ -/ns-pl/head-3/-/ns-img/fliph}}");
        assert!(d.is_empty(), "{d:#?}");
        let d = diagnose("{{$ -/ns-img/fliph/-/head-3}}");
        assert_eq!(d.len(), 1, "{d:#?}");
        assert!(d[0].message.contains("'head'"), "{d:#?}");
        assert_eq!(
            d[0].position.offset,
            "{{$ -/ns-img/fliph/-/".len(),
            "{d:#?}"
        );
    }

    /// AC-3: the same rule for a placeholder body.
    #[test]
    fn placeholder_body_ns_is_scoped_per_segment() {
        let text = "{{$ -/ns-pl/head-{{n}}/-/head-2}}";
        let d = diagnose(text);
        assert_eq!(d.len(), 1, "{d:#?}");
        assert!(d[0].message.contains("'head'"), "{d:#?}");
        assert_eq!(
            d[0].position.offset,
            "{{$ -/ns-pl/head-{{n}}/-/".len(),
            "{d:#?}"
        );
        let text = "{{$ head-{{n}}/ns-pl/head-2}}";
        let d = diagnose(text);
        assert_eq!(d.len(), 1, "{d:#?}");
        assert_eq!(d[0].position.offset, "{{$ ".len(), "{d:#?}");
    }

    /// AC-4: a placeholder namespace hides only the commands in its own segment.
    #[test]
    fn placeholder_namespace_defers_only_its_segment() {
        let d = diagnose("{{$ -/ns-{{lib}}/cmd/-/missing}}");
        assert_eq!(d.len(), 1, "{d:#?}");
        assert!(d[0].message.contains("'missing'"), "{d:#?}");
        let d = diagnose("{{$ ns-{{lib}}/cmd}}");
        assert!(d.is_empty(), "{d:#?}");
    }

    /// AC-5: a single leading `ns`, or none, behaves as before.
    #[test]
    fn single_leading_ns_unchanged() {
        assert!(diagnose("{{$ ns-pl/head-3}}").is_empty());
        let d = diagnose("{{$ known/head-3}}");
        assert_eq!(d.len(), 1, "{d:#?}");
        assert!(d[0].message.contains("'head'"), "{d:#?}");
    }
}
