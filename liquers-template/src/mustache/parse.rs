//! Parsing Mustache templates with query tags.
//!
//! Only the structure can fail: an unclosed or mismatched section, a tag with no closing
//! delimiter, a bad set-delimiter tag. A tag whose content does not parse becomes an [`Invalid`]
//! node, and parsing carries on after it.

use liquers_core::error::Error;
use liquers_core::parse::parse_key;
use liquers_core::query::{Position, QuerySource};

use crate::advance;
use crate::mustache::ast::{
    ApplyTarget, ClassicPiece, ClassicQuery, CloseForm, CloseTag, CommentTag, Element, Escape,
    Invalid, MustacheTemplate, PartialTag, PartialTarget, QueryBody, QueryTag, Section, SectionHead,
    SetDelimiterTag, TextElement, VariableTag,
};
use crate::mustache::lexer::{lex, Tag, Token};
use crate::name::parse_name;
use crate::query::ast::Slot;
use crate::query::parse::parse_query_template_at;

/// The content after `skip` bytes, and its position.
fn after(tag: &Tag, content: &str, skip: usize) -> (String, Position) {
    let rest = &content[skip..];
    let leading = rest.len() - rest.trim_start().len();
    let consumed = tag.content.len() - content.len() + skip + leading;
    (
        rest.trim().to_string(),
        advance(&tag.content_position, &tag.content[..consumed]),
    )
}

/// A tag's source with the line a standalone tag lost, so an `Invalid` element encodes it back.
fn source_with_line(tag: &Tag) -> String {
    match &tag.standalone {
        Some(s) => format!("{}{}{}", s.indent, tag.source, s.newline),
        None => tag.source.clone(),
    }
}

fn invalid(source: &str, error: Error, position: &Position) -> Invalid {
    let error = if error.position.is_unknown() {
        error.with_position(position)
    } else {
        error
    };
    Invalid {
        source: source.to_string(),
        error,
        position: position.clone(),
    }
}

/// Parses a query tag's content from its `$`. `escape` is how its result is inserted.
fn query_tag(tag: &Tag, content: &str, escape: Escape) -> QueryTag {
    let classic = content.starts_with("$$");
    let sigil = if classic { 2 } else { 1 };
    let rest = &content[sigil..];
    let mut apply = None;
    let mut body_offset = sigil;
    if let Some(target) = rest.strip_prefix('.') {
        let name_text: String = target.chars().take_while(|c| !c.is_whitespace()).collect();
        let (_, name_position) = after(tag, content, sigil + 1);
        apply = Some(if name_text.is_empty() {
            Ok(ApplyTarget::Current(name_position))
        } else {
            parse_name(&name_text, &name_position).map(ApplyTarget::Name)
        });
        body_offset = sigil + 1 + name_text.len();
    }
    let (body_text, body_position) = after(tag, content, body_offset);
    let separated = content[body_offset..]
        .chars()
        .next()
        .is_some_and(char::is_whitespace);
    let apply = match apply {
        Some(Ok(target)) => Some(target),
        Some(Err(e)) => {
            return QueryTag {
                body: QueryBody::Invalid(invalid(content, e, &body_position)),
                apply: None,
                escape,
                position: tag.position.clone(),
            }
        }
        None => None,
    };
    let body = if body_text.is_empty() || !separated {
        QueryBody::Invalid(invalid(
            content,
            Error::parse_error("a query is expected after the query sigil and a space".to_string()),
            &body_position,
        ))
    } else if classic {
        match classic_query(&body_text, &body_position, &tag.open, &tag.close) {
            Ok(q) => QueryBody::Classic(q),
            Err(e) => QueryBody::Invalid(invalid(content, e, &body_position)),
        }
    } else {
        match parse_query_template_at(&body_text, &tag.open, &tag.close, &body_position) {
            Ok(t) => QueryBody::Hygienic(t),
            Err(e) => QueryBody::Invalid(invalid(content, e, &body_position)),
        }
    };
    QueryTag {
        body,
        apply,
        escape,
        position: tag.position.clone(),
    }
}

fn classic_query(text: &str, start: &Position, open: &str, close: &str) -> Result<ClassicQuery, Error> {
    let mut pieces = Vec::new();
    let mut at = 0;
    while let Some(rel) = text[at..].find(open) {
        let slot_start = at + rel;
        if slot_start > at {
            pieces.push(ClassicPiece::Text(
                text[at..slot_start].to_string(),
                advance(start, &text[..at]),
            ));
        }
        let body_start = slot_start + open.len();
        let body_end = text[body_start..]
            .find(close)
            .map(|i| body_start + i)
            .ok_or_else(|| {
                Error::parse_error(format!("placeholder has no closing '{close}'"))
                    .with_position(&advance(start, &text[..slot_start]))
            })?;
        let body = &text[body_start..body_end];
        let leading = body.len() - body.trim_start().len();
        let name = parse_name(body.trim(), &advance(start, &text[..body_start + leading]))?;
        pieces.push(ClassicPiece::Slot(Slot {
            name,
            position: advance(start, &text[..slot_start]),
        }));
        at = body_end + close.len();
    }
    if at < text.len() {
        pieces.push(ClassicPiece::Text(text[at..].to_string(), advance(start, &text[..at])));
    }
    Ok(ClassicQuery {
        pieces,
        position: start.clone(),
    })
}

/// What a tag opens, closes or is.
enum Parsed {
    Element(Element),
    Open {
        inverted: bool,
        head: SectionHead,
        tag: Tag,
    },
    Close(CloseTag, Tag),
}

fn classify(tag: Tag) -> Parsed {
    let element = |e: Element| Parsed::Element(e);
    if tag.triple {
        let (name_text, name_position) = after(&tag, &tag.content, 0);
        if name_text.starts_with('$') {
            return element(Element::Invalid(invalid(
                &tag.source,
                Error::parse_error(
                    "a query tag is not written in triple braces; '&$' inserts its result raw"
                        .to_string(),
                ),
                &tag.position,
            )));
        }
        return element(match parse_name(&name_text, &name_position) {
            Ok(name) => Element::Variable(VariableTag {
                name,
                escape: Escape::Triple,
                position: tag.position.clone(),
            }),
            Err(e) => Element::Invalid(invalid(&tag.source, e, &tag.position)),
        });
    }
    let content = tag.content.trim_start().to_string();
    let first = content.chars().next();
    match first {
        Some('!') => element(Element::Comment(CommentTag {
            text: content[1..].to_string(),
            standalone: tag.standalone.clone(),
            position: tag.position.clone(),
        })),
        Some('=') => match &tag.set_delimiter {
            Some((open, close)) => element(Element::SetDelimiter(SetDelimiterTag {
                open: open.clone(),
                close: close.clone(),
                standalone: tag.standalone.clone(),
                position: tag.position.clone(),
            })),
            None => element(Element::Invalid(invalid(
                &tag.source,
                Error::parse_error("bad set-delimiter tag".to_string()),
                &tag.position,
            ))),
        },
        Some(sigil @ ('#' | '^')) => {
            let (head_text, head_position) = after(&tag, &content, 1);
            let head = if head_text.starts_with('$') {
                let rest = &content[1..];
                let skip = rest.len() - rest.trim_start().len();
                SectionHead::Query(query_tag(&tag, &content[1 + skip..], Escape::Escaped))
            } else {
                match parse_name(&head_text, &head_position) {
                    Ok(name) => SectionHead::Name(name),
                    Err(e) => SectionHead::Invalid(invalid(&head_text, e, &head_position)),
                }
            };
            Parsed::Open {
                inverted: sigil == '^',
                head,
                tag,
            }
        }
        Some('/') => {
            let (name_text, name_position) = after(&tag, &content, 1);
            let form = match name_text.as_str() {
                "" => Ok(CloseForm::Bare),
                "$" => Ok(CloseForm::Hygienic),
                "$$" => Ok(CloseForm::Classic),
                other => parse_name(other, &name_position).map(CloseForm::Name),
            };
            match form {
                Ok(form) => Parsed::Close(
                    CloseTag {
                        form,
                        standalone: tag.standalone.clone(),
                        position: tag.position.clone(),
                    },
                    tag,
                ),
                Err(e) => element(Element::Invalid(invalid(&source_with_line(&tag), e, &tag.position))),
            }
        }
        Some('>') => {
            let (target_text, target_position) = after(&tag, &content, 1);
            let target = if target_text.starts_with('$') {
                let rest = &content[1..];
                let skip = rest.len() - rest.trim_start().len();
                PartialTarget::Query(query_tag(&tag, &content[1 + skip..], Escape::Ampersand))
            } else if target_text.is_empty() {
                PartialTarget::Invalid(invalid(
                    &target_text,
                    Error::parse_error("a partial needs a key".to_string()),
                    &target_position,
                ))
            } else {
                match parse_key(&target_text) {
                    Ok(key) => PartialTarget::Key(key),
                    Err(e) => PartialTarget::Invalid(invalid(
                        &target_text,
                        Error::parse_error(format!("'{target_text}' is not a key: {}", e.message)),
                        &target_position,
                    )),
                }
            };
            element(Element::Partial(PartialTag {
                target,
                standalone: tag.standalone.clone(),
                position: tag.position.clone(),
            }))
        }
        Some('&') => {
            let (name_text, name_position) = after(&tag, &content, 1);
            if name_text.starts_with('$') {
                let rest = &content[1..];
                let skip = rest.len() - rest.trim_start().len();
                element(Element::Query(query_tag(&tag, &content[1 + skip..], Escape::Ampersand)))
            } else {
                element(match parse_name(&name_text, &name_position) {
                    Ok(name) => Element::Variable(VariableTag {
                        name,
                        escape: Escape::Ampersand,
                        position: tag.position.clone(),
                    }),
                    Err(e) => Element::Invalid(invalid(&tag.source, e, &tag.position)),
                })
            }
        }
        Some('$') => element(Element::Query(query_tag(&tag, &content, Escape::Escaped))),
        Some(_) | None => {
            let (name_text, name_position) = after(&tag, &content, 0);
            element(match parse_name(&name_text, &name_position) {
                Ok(name) => Element::Variable(VariableTag {
                    name,
                    escape: Escape::Escaped,
                    position: tag.position.clone(),
                }),
                Err(e) => Element::Invalid(invalid(&tag.source, e, &tag.position)),
            })
        }
    }
}

struct Open {
    inverted: bool,
    head: SectionHead,
    tag: Tag,
    elements: Vec<Element>,
}

fn closes(head: &SectionHead, form: &CloseForm) -> bool {
    match (form, head) {
        (CloseForm::Bare, _) => true,
        (CloseForm::Name(n), SectionHead::Name(h)) => n == h,
        (CloseForm::Hygienic, SectionHead::Query(q)) => !q.body.is_classic(),
        (CloseForm::Classic, SectionHead::Query(q)) => q.body.is_classic(),
        (CloseForm::Name(_) | CloseForm::Hygienic | CloseForm::Classic, SectionHead::Invalid(_)) => true,
        (CloseForm::Name(_), SectionHead::Query(_))
        | (CloseForm::Hygienic | CloseForm::Classic, SectionHead::Name(_)) => false,
    }
}

/// Parses a Mustache template.
pub fn parse_mustache(text: &str) -> Result<MustacheTemplate, Error> {
    let mut stack: Vec<Open> = Vec::new();
    let mut elements: Vec<Element> = Vec::new();
    for token in lex(text)? {
        let parsed = match token {
            Token::Text { text, position } => {
                if text.is_empty() {
                    continue;
                }
                Parsed::Element(Element::Text(TextElement { text, position }))
            }
            Token::Tag(tag) => classify(tag),
        };
        match parsed {
            Parsed::Element(e) => match stack.last_mut() {
                Some(open) => open.elements.push(e),
                None => elements.push(e),
            },
            Parsed::Open { inverted, head, tag } => stack.push(Open {
                inverted,
                head,
                tag,
                elements: Vec::new(),
            }),
            Parsed::Close(close, tag) => match stack.pop() {
                None => elements.push(Element::Invalid(invalid(
                    &source_with_line(&tag),
                    Error::parse_error("this close tag closes no open section".to_string()),
                    &tag.position,
                ))),
                Some(open) => {
                    if !closes(&open.head, &close.form) {
                        return Err(Error::parse_error(format!(
                            "'{}' does not close the section opened by '{}' at {}:{}",
                            tag.source, open.tag.source, open.tag.position.line, open.tag.position.column
                        ))
                        .with_position(&tag.position));
                    }
                    let section = Element::Section(Section {
                        inverted: open.inverted,
                        head: open.head,
                        elements: open.elements,
                        close,
                        standalone: open.tag.standalone.clone(),
                        position: open.tag.position.clone(),
                    });
                    match stack.last_mut() {
                        Some(parent) => parent.elements.push(section),
                        None => elements.push(section),
                    }
                }
            },
        }
    }
    if let Some(open) = stack.last() {
        return Err(Error::parse_error(format!(
            "the section opened by '{}' is not closed",
            open.tag.source
        ))
        .with_position(&open.tag.position));
    }
    Ok(MustacheTemplate {
        elements,
        source: QuerySource::Unspecified,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::name::Name;

    #[test]
    fn nested_braces_make_one_tag() {
        let t = parse_mustache("{{$ cmd-{{x}}/q}}").expect("template");
        match &t.elements[..] {
            [Element::Query(q)] => match &q.body {
                QueryBody::Hygienic(template) => assert_eq!(template.slots().count(), 1),
                other => panic!("{other:?}"),
            },
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn malformed_query_error_points_into_query() {
        let t = parse_mustache("line\nab {{$ cmd/~}}").expect("recoverable");
        let errors = t.errors();
        assert_eq!(errors.len(), 1);
        assert_eq!(errors[0].position.line, 2);
        assert!(errors[0].position.column > 7, "{:?}", errors[0].position);
    }

    #[test]
    fn unclosed_section_is_irrecoverable() {
        let err = parse_mustache("a\n{{#items}}x").expect_err("unclosed");
        assert!(err.message.contains("not closed"), "{}", err.message);
        assert_eq!(err.position.line, 2);
    }

    #[test]
    fn bad_set_delimiter_is_irrecoverable() {
        assert!(parse_mustache("{{= | =}}").is_err());
        assert!(parse_mustache("{{=a b c=}}").is_err());
    }

    #[test]
    fn bare_close_closes_any_section() {
        let t = parse_mustache("{{#a}}{{^$ q}}x{{/}}{{/}}").expect("bare closes");
        assert_eq!(t.errors().len(), 0);
        let err = parse_mustache("{{#y}}\n{{/x}}").expect_err("mismatch");
        assert_eq!(err.position.line, 2);
        assert!(err.message.contains("1:1"), "{}", err.message);
        assert!(parse_mustache("{{#$ q}}x{{/$$}}").is_err());
        assert!(parse_mustache("{{#$$ q}}x{{/$$}}").is_ok());
    }

    #[test]
    fn invalid_tag_becomes_invalid_node() {
        let t = parse_mustache("{{a..b}} {{a b}} {{#c..d}}x{{/}} {{> }} {{{$ q}}} {{/}}").expect("recoverable");
        assert_eq!(t.errors().len(), 6);
        match &t.elements[0] {
            Element::Invalid(i) => assert_eq!(i.source, "{{a..b}}"),
            other => panic!("{other:?}"),
        }
        let t = parse_mustache("{{$.row to_text}}").expect("apply");
        match &t.elements[0] {
            Element::Query(q) => assert_eq!(q.apply, Some(ApplyTarget::Name(Name::field("row")))),
            other => panic!("{other:?}"),
        }
    }
}
