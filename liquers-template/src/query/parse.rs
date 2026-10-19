//! Parsing a query template.
//!
//! The template is parsed by core's own query parser: each placeholder is first replaced by a
//! sentinel token that is valid wherever a placeholder may stand (an identifier, so it fits an
//! action name, a key segment, a filename stem and a parameter alike), and the parsed query is
//! lifted back, every sentinel becoming its [`Slot`]. A query without placeholders is therefore
//! parsed by exactly the code `parse_query` runs, which is what parity (AC-14) asks for, and
//! positions and errors are mapped back to the template's own coordinates. A sentinel never
//! appears in the template's text: its prefix is lengthened until it does not.

use liquers_core::error::Error;
use liquers_core::parse::parse_query;
use liquers_core::query::{
    ActionParameter, Position, Query, QuerySegment, SegmentHeader,
};

use crate::advance;
use crate::name::parse_name;
use crate::query::ast::{
    literal_pieces, ActionParameterTemplate, ActionRequestTemplate, KeyTemplate, Piece,
    QuerySegmentTemplate, QueryTemplate, ResourceQuerySegmentTemplate, Slot, TokenTemplate,
    TransformQuerySegmentTemplate,
};

/// A sentinel prefix that `text` does not contain.
pub(crate) fn sentinel_base(text: &str) -> String {
    let mut base = String::from("zqsl");
    while text.contains(&base) {
        base.insert(0, 'z');
    }
    base
}

/// The sentinel for slot `i`; the trailing `q` keeps `…1q` from matching inside `…12q`.
pub(crate) fn sentinel(base: &str, i: usize) -> String {
    format!("{base}{i}q")
}

/// A placeholder found in the text: where it is in the original and in the sentinel text.
struct Found {
    slot: Slot,
    source: String,
    original: (usize, usize),
    replaced: (usize, usize),
}

/// Parses a query template with `{{` `}}` placeholders. Positions are relative to `text`.
pub fn parse_query_template(text: &str) -> Result<QueryTemplate, Error> {
    parse_query_template_with(text, "{{", "}}")
}

/// Parses a query template whose placeholders use the given delimiters.
pub(crate) fn parse_query_template_with(
    text: &str,
    open: &str,
    close: &str,
) -> Result<QueryTemplate, Error> {
    parse_query_template_at(text, open, close, &Position::new(0, 1, 1))
}

/// Parses a query template that starts at `start` in a larger text; positions are in that text.
pub(crate) fn parse_query_template_at(
    text: &str,
    open: &str,
    close: &str,
    start: &Position,
) -> Result<QueryTemplate, Error> {
    let base = sentinel_base(text);
    let mut found: Vec<Found> = Vec::new();
    let mut replaced = String::with_capacity(text.len());
    let mut at = 0;
    let start = start.clone();
    while let Some(rel) = text[at..].find(open) {
        let tag_start = at + rel;
        let body_start = tag_start + open.len();
        let Some(close_rel) = text[body_start..].find(close) else {
            return Err(Error::parse_error(format!(
                "placeholder has no closing '{close}'"
            ))
            .with_position(&advance(&start, &text[..tag_start])));
        };
        let body_end = body_start + close_rel;
        let tag_end = body_end + close.len();
        let body = &text[body_start..body_end];
        let tag_position = advance(&start, &text[..tag_start]);
        if body.starts_with('{') || body.starts_with('&') {
            return Err(Error::parse_error(format!(
                "'{open}{body}{close}' is a raw placeholder, which a hygienic query does not take; \
                 use a classic query tag ($$) to insert text verbatim"
            ))
            .with_position(&tag_position));
        }
        let leading = body.len() - body.trim_start().len();
        let trimmed = body.trim();
        let name_position = advance(&start, &text[..body_start + leading]);
        let name = parse_name(trimmed, &name_position)?;
        replaced.push_str(&text[at..tag_start]);
        let token = sentinel(&base, found.len());
        let replaced_start = replaced.len();
        replaced.push_str(&token);
        found.push(Found {
            slot: Slot {
                name,
                position: tag_position,
            },
            source: text[tag_start..tag_end].to_string(),
            original: (tag_start, tag_end - tag_start),
            replaced: (replaced_start, token.len()),
        });
        at = tag_end;
    }
    replaced.push_str(&text[at..]);

    let map = PositionMap {
        original: text,
        found: &found,
        start: start.clone(),
    };
    let query = parse_query(&replaced).map_err(|e| map.error(e, &base))?;
    let slots: Vec<Slot> = found.iter().map(|f| f.slot.clone()).collect();
    let template = lift_mapped(&query, &base, &slots, &map)?;
    Ok(template)
}

struct PositionMap<'a> {
    original: &'a str,
    found: &'a [Found],
    /// Where `original` starts in the text positions are reported in.
    start: Position,
}

impl PositionMap<'_> {
    fn offset(&self, replaced: usize) -> usize {
        let mut delta: isize = 0;
        for f in self.found {
            let (r_start, r_len) = f.replaced;
            if replaced < r_start {
                break;
            }
            if replaced < r_start + r_len {
                return f.original.0;
            }
            delta = (f.original.0 + f.original.1) as isize - (r_start + r_len) as isize;
        }
        (replaced as isize + delta).max(0) as usize
    }

    fn position(&self, position: &Position) -> Position {
        let origin = self.start == Position::new(0, 1, 1);
        if position.is_unknown() || (self.found.is_empty() && origin) {
            return position.clone();
        }
        let offset = self.offset(position.offset).min(self.original.len());
        let mut boundary = offset;
        while !self.original.is_char_boundary(boundary) {
            boundary -= 1;
        }
        advance(&self.start, &self.original[..boundary])
    }

    fn error(&self, mut error: Error, base: &str) -> Error {
        let position = self.position(&error.position);
        error.position = position;
        let mut message = error.message.clone();
        for (i, f) in self.found.iter().enumerate().rev() {
            message = message.replace(&sentinel(base, i), &f.source);
        }
        error.message = message;
        if error.query.is_some() {
            error.query = Some(self.original.to_string());
        }
        error
    }
}

/// Lifts a parsed query; `slots` are the placeholders whose sentinels `base` names.
pub(crate) fn lift(query: &Query, slots: &[Slot]) -> QueryTemplate {
    let identity = PositionMap {
        original: "",
        found: &[],
        start: Position::new(0, 1, 1),
    };
    // With no sentinel in the text nothing can fail.
    lift_mapped(query, "\u{0}", slots, &identity).unwrap_or_default()
}

fn split(text: &str, base: &str, slots: &[Slot]) -> Vec<Piece> {
    if slots.is_empty() || !text.contains(base) {
        return literal_pieces(text);
    }
    let mut pieces = Vec::new();
    let mut rest = text;
    while let Some(i) = rest.find(base) {
        let after = &rest[i + base.len()..];
        let digits: String = after.chars().take_while(|c| c.is_ascii_digit()).collect();
        let tail = &after[digits.len()..];
        match (digits.parse::<usize>().ok().and_then(|n| slots.get(n)), tail.strip_prefix('q')) {
            (Some(slot), Some(tail)) => {
                if i > 0 {
                    pieces.push(Piece::Literal(rest[..i].to_string()));
                }
                pieces.push(Piece::Slot(slot.clone()));
                rest = tail;
            }
            (Some(_), None) | (None, Some(_)) | (None, None) => {
                // Not a sentinel after all; keep the text and move past the prefix.
                let keep = i + base.len();
                match pieces.last_mut() {
                    Some(Piece::Literal(t)) => t.push_str(&rest[..keep]),
                    Some(Piece::Slot(_)) | None => pieces.push(Piece::Literal(rest[..keep].to_string())),
                }
                rest = &rest[keep..];
            }
        }
    }
    if !rest.is_empty() {
        match pieces.last_mut() {
            Some(Piece::Literal(t)) => t.push_str(rest),
            Some(Piece::Slot(_)) | None => pieces.push(Piece::Literal(rest.to_string())),
        }
    }
    pieces
}

fn check_header(
    header: &Option<SegmentHeader>,
    base: &str,
    map: &PositionMap<'_>,
) -> Result<Option<SegmentHeader>, Error> {
    let Some(header) = header else {
        return Ok(None);
    };
    let mut header = header.clone();
    let in_name = header.name.contains(base);
    let in_parameters = header.parameters.iter().any(|p| p.value.contains(base));
    if in_name || in_parameters {
        return Err(Error::parse_error(
            "a placeholder cannot stand in a segment header (the realm or a header parameter); \
             use a classic query tag ($$) for that"
                .to_string(),
        )
        .with_position(&map.position(&header.position)));
    }
    header.position = map.position(&header.position);
    for parameter in header.parameters.iter_mut() {
        parameter.position = map.position(&parameter.position);
    }
    Ok(Some(header))
}

fn lift_mapped(
    query: &Query,
    base: &str,
    slots: &[Slot],
    map: &PositionMap<'_>,
) -> Result<QueryTemplate, Error> {
    let mut segments = Vec::with_capacity(query.segments.len());
    for segment in &query.segments {
        segments.push(match segment {
            QuerySegment::Resource(r) => QuerySegmentTemplate::Resource(ResourceQuerySegmentTemplate {
                header: check_header(&r.header, base, map)?,
                key: KeyTemplate(
                    r.key
                        .0
                        .iter()
                        .map(|name| TokenTemplate {
                            pieces: split(&name.name, base, slots),
                            position: map.position(&name.position),
                        })
                        .collect(),
                ),
            }),
            QuerySegment::Transform(t) => {
                let mut actions = Vec::with_capacity(t.query.len());
                for action in &t.query {
                    let mut parameters = Vec::with_capacity(action.parameters.len());
                    for parameter in &action.parameters {
                        parameters.push(match parameter {
                            ActionParameter::String(s, pos) => {
                                ActionParameterTemplate::String(split(s, base, slots), map.position(pos))
                            }
                            ActionParameter::Link(link, pos) => ActionParameterTemplate::Link(
                                lift_mapped(link, base, slots, map)?,
                                map.position(pos),
                            ),
                        });
                    }
                    let position = map.position(&action.position);
                    actions.push(ActionRequestTemplate {
                        name: TokenTemplate {
                            pieces: split(&action.name, base, slots),
                            position: position.clone(),
                        },
                        parameters,
                        position,
                    });
                }
                QuerySegmentTemplate::Transform(TransformQuerySegmentTemplate {
                    header: check_header(&t.header, base, map)?,
                    query: actions,
                    filename: t.filename.as_ref().map(|f| TokenTemplate {
                        pieces: split(&f.name, base, slots),
                        position: map.position(&f.position),
                    }),
                })
            }
        });
    }
    Ok(QueryTemplate {
        segments,
        rooted: query.rooted,
        source: query.source.clone(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::name::Name;

    #[test]
    fn placeholders_become_slots() {
        let t = parse_query_template("-R/data/{{name}}.csv/-/cmd-a{{x}}b-~X~-R/{{key}}~E/to_{{fmt}}/out.{{ext}}")
            .expect("template");
        let names: Vec<String> = t.slots().map(|s| s.name.encode()).collect();
        assert_eq!(names, vec!["name", "x", "key", "fmt", "ext"]);
        assert!(t.try_into_query().is_none());
        assert_eq!(
            t.encode(),
            "-R/data/{{name}}.csv/-/cmd-a{{x}}b-~X~-R/{{key}}~E/to_{{fmt}}/out.{{ext}}"
        );
        let x = t.slots().nth(1).expect("x");
        assert_eq!((x.position.line, x.position.column), (1, 29));
        assert_eq!(x.name, Name::field("x"));
    }

    #[test]
    fn errors_are_in_template_coordinates() {
        let err = parse_query_template("cmd-{{x}}/~").expect_err("bad query");
        assert!(err.message.contains("{{x}}"), "{}", err.message);
        assert!(!err.message.contains("zqsl"), "{}", err.message);
        let err = parse_query_template("-{{r}}/cmd").expect_err("header slot");
        assert!(err.message.contains("segment header"), "{}", err.message);
        let err = parse_query_template("cmd-{{{x}}}").expect_err("raw slot");
        assert!(err.message.contains("raw placeholder"), "{}", err.message);
    }

    #[test]
    fn sentinel_never_collides() {
        let t = parse_query_template("cmd-zqsl0q-{{x}}").expect("template");
        assert_eq!(t.encode(), "cmd-zqsl0q-{{x}}");
        assert_eq!(t.slots().count(), 1);
    }
}
