#![allow(unused_imports)]
#![allow(dead_code)]

use nom;

extern crate nom_locate;
use nom::branch::alt;
use nom::character::complete::digit1;
use nom::combinator::{eof, not, opt, peek};
use nom::sequence::{preceded, terminated};
use nom_locate::LocatedSpan;

use nom::bytes::complete::{tag, take_while, take_while1};
use nom::character::{is_alphabetic, is_alphanumeric};
use nom::multi::{many0, many1, separated_list0, separated_list1};
use nom::*;

use crate::error::{Error, ErrorType};
use crate::query::{
    ActionParameter, ActionRequest, HeaderParameter, Key, Position, Query, QuerySegment,
    ResourceName, ResourceQuerySegment, SegmentHeader, TransformQuerySegment,
};

type Span<'a> = LocatedSpan<&'a str>;

#[allow(dead_code)]
impl<'a> From<Span<'a>> for Position {
    fn from(span: Span<'a>) -> Position {
        Position {
            offset: span.location_offset(),
            line: span.location_line(),
            column: span.get_utf8_column(),
        }
    }
}

fn identifier(text: Span) -> IResult<Span, String> {
    let (text, a) = take_while1(|c| is_alphabetic(c as u8) || c == '_')(text)?;
    let (text, b) = take_while(|c| is_alphanumeric(c as u8) || c == '_')(text)?;

    Ok((text, format!("{}{}", a, b)))
}

fn filename(text: Span) -> IResult<Span, String> {
    let (text, a) = take_while(|c| is_alphanumeric(c as u8) || c == '_')(text)?;
    let (text, _dot) = nom::character::complete::char('.')(text)?;
    let (text, b) =
        take_while1(|c| is_alphanumeric(c as u8) || c == '_' || c == '.' || c == '-')(text)?;

    Ok((text, format!("{}.{}", a, b)))
}

fn slash_filename(text: Span) -> IResult<Span, String> {
    let (text, _) = tag("/")(text)?;
    let (text, fname) = filename(text)?;
    Ok((text, fname))
}

fn resource_name(text: Span) -> IResult<Span, ResourceName> {
    let position: Position = text.into();
    let (text, a) = take_while1(|c| is_alphanumeric(c as u8) || c == '_' || c == '.')(text)?;
    let (text, b) =
        take_while(|c| is_alphanumeric(c as u8) || c == '_' || c == '.' || c == '-')(text)?;
    Ok((
        text,
        ResourceName::new(format!("{}{}", a, b)).with_position(position),
    ))
}
fn parameter_text(text: Span) -> IResult<Span, String> {
    let (text, a) =
        take_while1(|c| is_alphanumeric(c as u8) || c == '_' || c == '+' || c == '.')(text)?;
    Ok((text, a.to_string()))
}

fn tilde_entity(text: Span) -> IResult<Span, String> {
    let (text, _) = tag("~~")(text)?;
    Ok((text, "~".to_owned()))
}
fn minus_entity(text: Span) -> IResult<Span, String> {
    let (text, _) = tag("~_")(text)?;
    Ok((text, "-".to_owned()))
}
fn islash_entity(text: Span) -> IResult<Span, String> {
    let (text, _) = tag("~I")(text)?;
    Ok((text, "/".to_owned()))
}
fn slash_entity(text: Span) -> IResult<Span, String> {
    let (text, _) = tag("~/")(text)?;
    Ok((text, "/".to_owned()))
}
fn https_entity(text: Span) -> IResult<Span, String> {
    let (text, _) = tag("~H")(text)?;
    Ok((text, "https://".to_owned()))
}
fn http_entity(text: Span) -> IResult<Span, String> {
    let (text, _) = tag("~h")(text)?;
    Ok((text, "http://".to_owned()))
}
fn file_entity(text: Span) -> IResult<Span, String> {
    let (text, _) = tag("~f")(text)?;
    Ok((text, "file://".to_owned()))
}
fn protocol_entity(text: Span) -> IResult<Span, String> {
    let (text, _) = tag("~P")(text)?;
    Ok((text, "://".to_owned()))
}
fn negative_number_entity(text: Span) -> IResult<Span, String> {
    let (text, _) = tag("~")(text)?;
    let (text, n) = digit1(text)?;
    Ok((text, format!("-{n}")))
}
fn space_entity(text: Span) -> IResult<Span, String> {
    let (text, _) = tag("~.")(text)?;
    Ok((text, " ".to_owned()))
}
fn entities(text: Span) -> IResult<Span, String> {
    alt((
        tilde_entity,
        minus_entity,
        negative_number_entity,
        space_entity,
        islash_entity,
        slash_entity,
        http_entity,
        https_entity,
        file_entity,
        protocol_entity,
    ))(text)
}
fn parameter(text: Span) -> IResult<Span, ActionParameter> {
    let position: Position = text.into();
    let (text, par) = many0(alt((parameter_text, entities)))(text)?;
    Ok((
        text,
        ActionParameter::new_string(par.join("")).with_position(position),
    ))
}
fn minus_parameter(text: Span) -> IResult<Span, ActionParameter> {
    let (text, _) = tag("-")(text)?;
    parameter(text)
}
/*
fn parameter(text:Span) ->IResult<Span, ActionParameter>{
    let position:Position = text.into();
    let (text, par) =take_while(|c| {c!='-'&&c!='/'})(text)?;

    Ok((text, ActionParameter::new_string(par.to_string()).with_position(position)))
}
*/
fn action_request(text: Span) -> IResult<Span, ActionRequest> {
    let position: Position = text.into();
    let (text, name) = identifier(text)?;
    let (text, parameters) = many0(minus_parameter)(text)?;
    Ok((
        text,
        ActionRequest::new(name)
            .with_parameters(parameters)
            .with_position(position),
    ))
}

fn header_parameter(text: Span) -> IResult<Span, HeaderParameter> {
    let (text, _) = tag("-")(text)?;
    let position: Position = text.into();
    let (text, parameter) = take_while(|c| is_alphanumeric(c as u8) || c == '_' || c == '.')(text)?;
    Ok((
        text,
        HeaderParameter::new(parameter.to_string()).with_position(position),
    ))
}

fn full_transform_segment_header(text: Span) -> IResult<Span, SegmentHeader> {
    let position: Position = text.into();
    let (text, level_lead) = many1(tag("-"))(text)?;
    let (text, lead_name) =
        take_while1(|c: char| is_alphabetic(c as u8) && c.is_lowercase())(text)?;
    let (text, rest_name) = take_while(|c| is_alphanumeric(c as u8) || c == '_')(text)?;
    let (text, parameters) = many0(header_parameter)(text)?;
    let (text, _) = tag("/")(text)?;

    Ok((
        text,
        SegmentHeader {
            name: format!("{lead_name}{rest_name}"),
            level: level_lead.len() - 1,
            parameters,
            resource: false,
            position,
        },
    ))
}

fn short_transform_segment_header(text: Span) -> IResult<Span, SegmentHeader> {
    let position: Position = text.into();
    let (text, level_lead) = many1(tag("-"))(text)?;
    let (text, _) = tag("/")(text)?;

    Ok((
        text,
        SegmentHeader {
            name: "".to_owned(),
            level: level_lead.len() - 1,
            parameters: vec![],
            resource: false,
            position,
        },
    ))
}

fn transform_segment_header(text: Span) -> IResult<Span, SegmentHeader> {
    alt((
        short_transform_segment_header,
        full_transform_segment_header,
    ))(text)
}

fn resource_segment_header(text: Span) -> IResult<Span, SegmentHeader> {
    let position: Position = text.into();
    let (text, level_lead) = many1(tag("-"))(text)?;
    let (text, _) = tag("R")(text)?;
    let (text, name) = take_while(|c: char| is_alphanumeric(c as u8) || c == '_')(text)?;
    let (text, parameters) = many0(header_parameter)(text)?;

    Ok((
        text,
        SegmentHeader {
            name: name.to_string(),
            level: level_lead.len() - 1,
            parameters,
            resource: true,
            position,
        },
    ))
}

pub(crate) fn resource_path(text: Span) -> IResult<Span, Vec<ResourceName>> {
    separated_list0(tag("/"), resource_name)(text)
}
pub(crate) fn resource_path1(text: Span) -> IResult<Span, Vec<ResourceName>> {
    separated_list1(tag("/"), resource_name)(text)
}

fn resource_segment_with_header(text: Span) -> IResult<Span, ResourceQuerySegment> {
    let (text, header) = resource_segment_header(text)?;
    let (text, path) = opt(preceded(tag("/"), resource_path1))(text)?;
    let key = if let Some(path) = path {
        Key(path)
    } else {
        Key(vec![])
    };
    Ok((
        text,
        ResourceQuerySegment {
            header: Some(header),
            key: key,
        },
    ))
}
fn resource_qs(text: Span) -> IResult<Span, QuerySegment> {
    let (text, rqs) = resource_segment_with_header(text)?;
    Ok((text, QuerySegment::Resource(rqs)))
}
enum FilenameOrAction {
    Filename(ResourceName),
    Action(ActionRequest),
}
fn filename_or_action1(text: Span) -> IResult<Span, FilenameOrAction> {
    let position: Position = text.into();
    let (text, fname) = filename(text)?;
    Ok((
        text,
        FilenameOrAction::Filename(ResourceName::new(fname).with_position(position)),
    ))
}
fn filename_or_action2(text: Span) -> IResult<Span, FilenameOrAction> {
    let (text, action) = action_request(text)?;
    Ok((text, FilenameOrAction::Action(action)))
}
fn filename_or_action(text: Span) -> IResult<Span, FilenameOrAction> {
    alt((filename_or_action1, filename_or_action2))(text)
}

fn transform_segment_with_header(text: Span) -> IResult<Span, TransformQuerySegment> {
    //    println!("transform_segment_with_header: {:?}", text);
    let (text, header) = transform_segment_header(text)?;
    //    println!("  header: {:?}", header);
    //    println!("  text:   {:?}", text);
    //let (text, mut query) = many0(terminated(action_request, tag("/")))(text)?;
    let (text, mut query) = action_requests(text)?;
    //    println!("  query:  {:?}", query);
    //    println!("  text:   {:?}", text);
    let (text, fna) = filename_or_action(text)?;
    //    println!("  fna-text:   {:?}", text);
    match fna {
        FilenameOrAction::Filename(fname) => Ok((
            text,
            TransformQuerySegment {
                header: Some(header),
                query,
                filename: Some(fname),
            },
        )),
        FilenameOrAction::Action(action) => {
            query.push(action);
            Ok((
                text,
                TransformQuerySegment {
                    header: Some(header),
                    query: query,
                    filename: None,
                },
            ))
        }
    }
}
fn transform_qs0(text: Span) -> IResult<Span, QuerySegment> {
    let (text, tqs) = alt((
        transform_segment_without_header,
        transform_segment_with_header,
    ))(text)?;
    Ok((text, QuerySegment::Transform(tqs)))
}
fn transform_qs1(text: Span) -> IResult<Span, QuerySegment> {
    //    println!("transform_qs1: {:?}", text);
    let (text, tqs) = transform_segment_with_header(text)?;
    //    println!("  tqs text: {:?}", text);
    //    println!("  tqs:      {:?}", tqs);
    Ok((text, QuerySegment::Transform(tqs)))
}
fn query_segment0(text: Span) -> IResult<Span, QuerySegment> {
    alt((resource_qs, transform_qs0))(text)
}
fn query_segment1(text: Span) -> IResult<Span, QuerySegment> {
    //    println!("query_segment1: {:?}", text);
    let (text, x) = alt((resource_qs, transform_qs1))(text)?;
    //    println!("  qs text: {:?}", text);
    //    println!("  qs:      {:?}", x);
    Ok((text, x))
}
/*
fn _transform_segment_without_header(text: Span) -> IResult<Span, TransformQuerySegment> {
    let (text, query) = separated_list1(tag("/"), action_request)(text)?;
    let position: Position = text.into();
    let (text, fname) = opt(slash_filename)(text)?;
    Ok((
        text,
        TransformQuerySegment {
            header: None,
            query,
            filename: fname.map(|name| ResourceName::new(name).with_position(position)),
        },
    ))
}
*/

fn nonterminating_separator(text: Span) -> IResult<Span, Span> {
    let (text, a) = tag("/")(text)?;
    let (text, _) = peek(not(tag("-")))(text)?;
    Ok((text, a))
}

fn action_requests(text: Span) -> IResult<Span, Vec<ActionRequest>> {
    many0(terminated(action_request, nonterminating_separator))(text)
}

fn transform_segment_without_header(text: Span) -> IResult<Span, TransformQuerySegment> {
    let (text, mut query) = action_requests(text)?;
    let (text, fna) = filename_or_action(text)?;

    match fna {
        FilenameOrAction::Filename(fname) => Ok((
            text,
            TransformQuerySegment {
                header: None,
                query,
                filename: Some(fname),
            },
        )),
        FilenameOrAction::Action(action) => {
            query.push(action);
            Ok((
                text,
                TransformQuerySegment {
                    header: None,
                    query: query,
                    filename: None,
                },
            ))
        }
    }
}
/*
fn transform_segment_without_header_and_filename(
    text: Span,
) -> IResult<Span, TransformQuerySegment> {
    let (text, mut query) = action_requests(text)?;
    let (text, last) = action_request(text)?;
    query.push(last);
    Ok((
        text,
        TransformQuerySegment {
            header: None,
            query,
            filename: None,
        },
    ))
}
*/

fn simple_transform_query(text: Span) -> IResult<Span, Query> {
    //    println!("simple_transform_query: {:?}", text);
    let (text, abs) = opt(tag("/"))(text)?;
    let (text, tqs) = alt((
        transform_segment_without_header,
        //transform_segment_without_header_and_filename,
    ))(text)?;
    //    println!("simple_transform_query SUCCESS");
    Ok((
        text,
        Query {
            segments: vec![QuerySegment::Transform(tqs)],
            absolute: abs.is_some(),
            ..Default::default()
        },
    ))
}

fn resource_transform_query(text: Span) -> IResult<Span, Query> {
    //    println!("resource_transform_query: {:?}", text);
    let (text, abs) = opt(tag("/"))(text)?;
    let (text, resource) = resource_path1(text)?;
    let (text, _slash) = tag("/")(text)?;
    let (text, tqs) = transform_segment_with_header(text)?;
    //    println!("resource_transform_query SUCCESS");
    Ok((
        text,
        Query {
            segments: vec![
                QuerySegment::Resource(ResourceQuerySegment {
                    header: None,
                    key: Key(resource),
                }),
                QuerySegment::Transform(tqs),
            ],
            absolute: abs.is_some(),
            ..Default::default()
        },
    ))
}
fn general_query(text: Span) -> IResult<Span, Query> {
    //    println!("general_query: {:?}", text);
    let (text, abs) = opt(tag("/"))(text)?;
    let (text, q0) = query_segment0(text)?;
    //    println!("q0: {:?}", q0);
    let (text, mut segments) = many0(preceded(tag("/"), query_segment1))(text)?;
    //    println!("segments: {:?}", segments);

    segments.insert(0, q0);
    //    println!("general_query SUCCESS");
    Ok((
        text,
        Query {
            segments,
            absolute: abs.is_some(),
            ..Default::default()
        },
    ))
}

fn empty_query(text: Span) -> IResult<Span, Query> {
    let (text, abs) = opt(tag("/"))(text)?;
    Ok((
        text,
        Query {
            segments: vec![],
            absolute: abs.is_some(),
            ..Default::default()
        },
    ))
}

fn query_parser(text: Span) -> IResult<Span, Query> {
    alt((
        terminated(resource_transform_query, eof),
        terminated(simple_transform_query, eof),
        general_query,
        empty_query,
    ))(text)
}
/*
fn parse_action(text:Span) ->IResult<Span, ActionRequest>{
    let position:Position = text.into();
    let (text, name) =identifier(text)?;
    let (text, p) =many0(pair(tag("-"),parameter))(text)?;

    Ok((text, ActionRequest{name:name, position, parameters:p.iter().map(|x| x.1.clone()).collect()}))
}

fn parse_action_path(text: Span) -> IResult<Span, Vec<ActionRequest>> {
    separated_list0(tag("/"), action_request)(text)
}
*/

pub fn parse_query(query: &str) -> Result<Query, Error> {
    let (remainder, path) = query_parser(Span::new(query)).map_err(|e| {
        let message = format!("{}", e);
        Error::query_parse_error(query, &message, &Position::unknown())
    })?;
    if remainder.fragment().len() > 0 {
        let position: Position = remainder.into();
        Err(Error::query_parse_error(
            query,
            "Can't parse query completely",
            &position,
        ))
    } else {
        Ok(path)
    }
}

pub fn parse_key<S: AsRef<str>>(key: S) -> Result<Key, Error> {
    let (remainder, path) = resource_path(Span::new(key.as_ref())).map_err(|e| {
        let em = format!("{}", e);
        Error::key_parse_error(key.as_ref(), &em, &Position::unknown())
    })?;
    if remainder.fragment().len() > 0 {
        let position: Position = remainder.into();
        Err(Error::key_parse_error(
            remainder.fragment(),
            "Can't parse completely",
            &position,
        ))
    } else {
        Ok(Key(path))
    }
}

impl TryFrom<&str> for Key {
    type Error = Error;
    fn try_from(s: &str) -> Result<Self, Self::Error> {
        parse_key(s)
    }
}

impl TryFrom<String> for Key {
    type Error = Error;
    fn try_from(s: String) -> Result<Self, Self::Error> {
        parse_key(s)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::query::ActionParameter;

    #[test]
    fn parse_action_test() -> Result<(), Box<dyn std::error::Error>> {
        let (_remainder, action) = action_request(Span::new("abc-def"))?;
        assert_eq!(action.name, "abc");
        assert_eq!(action.parameters.len(), 1);
        match &action.parameters[0] {
            ActionParameter::String(txt, _) => assert_eq!(txt, "def"),
            _ => assert!(false),
        }
        Ok(())
    }

    #[test]
    fn parse_filename_test() -> Result<(), Box<dyn std::error::Error>> {
        let (_remainder, fname) = filename(Span::new("file1.txt"))?;
        assert_eq!(fname, "file1.txt");
        let (_remainder, fname) = slash_filename(Span::new("/file2.txt"))?;
        assert_eq!(fname, "file2.txt");
        Ok(())
    }

    #[test]
    fn parse_path_test() -> Result<(), Box<dyn std::error::Error>> {
        let (remainder, path) = query_parser(Span::new("abc-def/xxx-123"))?;
        println!("REMAINDER: {:#?}", remainder);
        println!("PATH:      {:#?}", path);
        assert_eq!(remainder.fragment().len(), 0);
        assert_eq!(remainder.to_string().len(), 0);
        Ok(())
    }

    #[test]
    fn transform_segment_without_header_test() -> Result<(), Box<dyn std::error::Error>> {
        let (remainder, tqs) = transform_segment_without_header(Span::new("abc/def/file.txt"))?;
        println!("REMAINDER: {:#?}", remainder);
        println!("TQS:      {:#?}", tqs);
        Ok(())
    }

    #[test]
    fn parse_query_filename() -> Result<(), Box<dyn std::error::Error>> {
        let q = parse_query("abc/def/file.txt")?;
        assert_eq!(q.filename().unwrap().encode(), "file.txt");
        Ok(())
    }

    #[test]
    fn parse_query_filename0() -> Result<(), Box<dyn std::error::Error>> {
        let q = parse_query("file.txt")?;
        assert_eq!(q.filename().unwrap().encode(), "file.txt");
        Ok(())
    }

    #[test]
    fn parse_query_test() -> Result<(), Error> {
        let path = parse_query("")?;
        assert_eq!(path.len(), 0);
        let path = parse_query("abc-def")?;
        assert_eq!(path.len(), 1);
        let path = parse_query("abc-def/xxx-123")?;
        assert_eq!(path.len(), 1);

        assert_eq!(path.segments[0].len(), 2);
        Ok(())
    }
    #[test]
    fn parse_query_test2() -> Result<(), Box<dyn std::error::Error>> {
        let (s, p) = resource_path1(Span::new("a/b/c"))?;
        println!("remainder {s}");
        println!("path      {:?}", p);
        assert_eq!(s.fragment().len(), 0);

        let (s, rqs) = resource_segment_with_header(Span::new("-R/a/b/c"))?;
        println!("remainder {s}");
        println!("rqs     {:?}", rqs);
        println!("rqs enc {}", rqs.encode());
        assert_eq!(s.fragment().len(), 0);
        let path = parse_query("-R/a/b/c")?;
        assert_eq!(path.len(), 1);
        let path = parse_query("-R/a/b/-/c/d")?;
        assert_eq!(path.len(), 2);
        let (s, q) = resource_transform_query(Span::new("a/b/-/c/d"))?;
        println!("remainder '{s}'");
        println!("query     {:?}", q);
        println!("query enc {:?}", q.encode());
        assert_eq!(s.fragment().len(), 0);
        let path = parse_query("a/b/-/c/d")?;
        assert_eq!(path.len(), 2);
        Ok(())
    }
    #[test]
    fn parse_ns() -> Result<(), Error> {
        let path = parse_query("ns-abc")?;
        assert!(path.is_ns());
        assert_eq!(path.ns().unwrap().len(), 1);
        assert_eq!(path.ns().unwrap()[0].encode(), "abc");
        Ok(())
    }
    #[test]
    fn parse_last_ns() -> Result<(), Error> {
        let path = parse_query("ns-abc/test")?;
        assert!(!path.is_ns());
        assert_eq!(path.last_ns().unwrap().len(), 1);
        assert_eq!(path.last_ns().unwrap()[0].encode(), "abc");
        let path = parse_query("test")?;
        assert!(!path.is_ns());
        assert!(path.last_ns().is_none());
        Ok(())
    }

    #[test]
    fn root1a() -> Result<(), Error> {
        let q = parse_query("-R/a")?;
        assert_eq!(q.segments.len(), 1);
        assert_eq!(q.encode(), "-R/a");
        let q = parse_query("-R/a/-/dr")?;
        assert_eq!(q.segments.len(), 2);
        assert_eq!(q.encode(), "-R/a/-/dr");
        Ok(())
    }
    #[test]
    fn root1b() -> Result<(), Error> {
        let q = parse_query("-R")?;
        assert_eq!(q.segments.len(), 1);
        assert_eq!(q.encode(), "-R");
        Ok(())
    }
    #[test]
    fn root2() -> Result<(), Error> {
        let q = parse_query("-R/-/dr")?;
        assert_eq!(q.segments.len(), 2);
        assert_eq!(q.encode(), "-R/-/dr");
        assert_eq!(
            q.segments[0]
                .resource_query_segment()
                .unwrap()
                .header
                .unwrap()
                .encode(),
            "-R"
        );
        Ok(())
    }
    #[test]
    fn root3() -> Result<(), Error> {
        let q = parse_query("-R-meta/-/dr")?;
        assert_eq!(q.segments.len(), 2);
        assert_eq!(q.encode(), "-R-meta/-/dr");
        assert_eq!(
            q.segments[0]
                .resource_query_segment()
                .unwrap()
                .header
                .unwrap()
                .encode(),
            "-R-meta"
        );
        Ok(())
    }
    #[test]
    fn query1() -> Result<(), Error> {
        let q = parse_query("-R/abc/def/-/ghi/jkl/file.txt")?;
        assert_eq!(q.segments.len(), 2);
        assert_eq!(q.filename(), Some(ResourceName::new("file.txt".to_owned())));
        assert_eq!(q.extension(), Some("txt".to_string()));
        assert_eq!(q.encode(), "-R/abc/def/-/ghi/jkl/file.txt");
        Ok(())
    }
    #[test]
    fn query2() -> Result<(), Error> {
        let q = parse_query("abc/def/-/xxx")?;
        assert_eq!(q.segments.len(), 2);
        assert_eq!(q.filename(), None);
        assert_eq!(q.extension(), None);
        assert_eq!(q.encode(), "-R/abc/def/-/xxx");
        let q = parse_query("xxx/-q/qqq")?;
        assert_eq!(q.segments.len(), 2);
        assert_eq!(q.filename(), None);
        assert_eq!(q.extension(), None);
        assert_eq!(q.encode(), "-R/xxx/-q/qqq");
        Ok(())
    }
    #[test]
    fn actionreqests() -> Result<(), Box<dyn std::error::Error>> {
        let (rest, ar) = action_requests(Span::new("abc/def/-/xxx/-q/qqq"))?;
        println!("rest: {:?}", rest);
        println!("ar:   {:?}", ar);
        println!();
        assert_eq!(ar.len(), 1);
        let (rest, q) = transform_segment_without_header(Span::new("abc/def/-/xxx/-q/qqq"))?;
        println!("rest: {:?}", rest);
        println!("tqs:  {:?}", q);
        println!();
        assert_eq!(q.encode(), "abc/def");
        Ok(())
    }
    #[test]
    fn nonterminating_separator_test() -> Result<(), Box<dyn std::error::Error>> {
        let (rest, sep) = nonterminating_separator(Span::new("/x"))?;
        assert_eq!(sep.fragment().to_string(), "/");
        assert_eq!(rest.fragment().to_string(), "x");
        let (rest, sep) = nonterminating_separator(Span::new("/"))?;
        assert_eq!(sep.fragment().to_string(), "/");
        assert_eq!(rest.fragment().len(), 0);
        assert!(nonterminating_separator(Span::new("/-")).is_err());
        Ok(())
    }
    #[test]
    fn general_query1() -> Result<(), Box<dyn std::error::Error>> {
        //let (rest, q) = general_query(Span::new("abc/def/-/xxx/-q/qqq"))?;
        let (rest, q) = general_query(Span::new("abc/def/-/xxx/yyy"))?;
        println!("rest: {:?}", rest);
        println!("gq:  {}", q.encode());
        println!("gq:  {:#?}", q);
        println!();
        assert_eq!(q.segments.len(), 2);
        assert!(q.segments[0].is_transform_query_segment());
        assert!(q.segments[1].is_transform_query_segment());
        assert_eq!(q.encode(), "abc/def/-/xxx/yyy");
        Ok(())
    }
    #[test]
    fn general_query2() -> Result<(), Box<dyn std::error::Error>> {
        //let (rest, q) = general_query(Span::new("abc/def/-/xxx/-q/qqq"))?;
        let (rest, q) = general_query(Span::new("abc/def/-/xxx/-/yyy"))?;
        println!("rest: {:?}", rest);
        println!("gq:  {}", q.encode());
        println!("gq:  {:#?}", q);
        println!();
        assert_eq!(q.segments.len(), 3);
        assert!(q.segments[0].is_transform_query_segment());
        assert!(q.segments[1].is_transform_query_segment());
        assert!(q.segments[2].is_transform_query_segment());
        assert_eq!(q.encode(), "abc/def/-/xxx/-/yyy");
        Ok(())
    }
    #[test]
    fn general_query3() -> Result<(), Box<dyn std::error::Error>> {
        //let (rest, q) = general_query(Span::new("abc/def/-/xxx/-q/qqq"))?;
        let (rest, q) = general_query(Span::new("abc/def/-/xxx/-q/qqq"))?;
        println!("rest: {:?}", rest);
        println!("gq:  {}", q.encode());
        println!("gq:  {:#?}", q);
        println!();
        assert_eq!(q.segments.len(), 3);
        assert!(q.segments[0].is_transform_query_segment());
        assert!(q.segments[1].is_transform_query_segment());
        assert!(q.segments[2].is_transform_query_segment());
        assert_eq!(q.encode(), "abc/def/-/xxx/-q/qqq");
        Ok(())
    }
    #[test]
    fn query3a() -> Result<(), Box<dyn std::error::Error>> {
        //let (rest, q) = general_query(Span::new("abc/def/-/xxx/-q/qqq"))?;
        let (rest, q) = general_query(Span::new("abc/def/-/xxx/yyy/-/xxx/yyy"))?;
        println!("rest: {:?}", rest);
        println!("gq:  {}", q.encode());
        println!("gq:  {:#?}", q);
        println!();
        assert_eq!(q.segments.len(), 3);
        let q = parse_query("-R/abc/def/-q/xxx/-q/qqq")?;
        assert_eq!(q.segments.len(), 3);
        assert_eq!(q.filename(), None);
        assert_eq!(q.extension(), None);
        assert_eq!(q.encode(), "-R/abc/def/-q/xxx/-q/qqq");
        Ok(())
    }
    #[test]
    fn query3b() -> Result<(), Error> {
        let q = parse_query("abc/def/-/xxx/-q/qqq")?;
        assert_eq!(q.segments.len(), 3);
        assert_eq!(q.filename(), None);
        assert_eq!(q.extension(), None);
        assert_eq!(q.encode(), "abc/def/-/xxx/-q/qqq");
        Ok(())
    }

    #[test]
    fn predecessor1() -> Result<(), Error> {
        let q = parse_query("-x/ghi/jkl/file.txt")?;
        let (p, r) = q.predecessor();
        assert_eq!(p.as_ref().unwrap().encode(), "-x/ghi/jkl");
        assert!(p.as_ref().unwrap().is_transform_query());
        assert_eq!(r.as_ref().unwrap().encode(), "-x/file.txt");
        assert!(r.as_ref().unwrap().is_transform_query_segment());
        assert!(!r.as_ref().unwrap().is_empty());
        assert!(r.as_ref().unwrap().is_filename());

        let (p, r) = p.unwrap().predecessor();
        assert_eq!(p.as_ref().unwrap().encode(), "-x/ghi");
        assert!(p.as_ref().unwrap().is_transform_query());
        assert_eq!(r.as_ref().unwrap().encode(), "-x/jkl");
        assert!(r.as_ref().unwrap().is_transform_query_segment());
        assert!(!r.as_ref().unwrap().is_empty());
        assert!(!r.as_ref().unwrap().is_filename());
        assert!(r.as_ref().unwrap().is_action_request());

        let (p, r) = p.unwrap().predecessor();
        assert!(p.as_ref().unwrap().is_empty());
        assert_eq!(r.as_ref().unwrap().encode(), "-x/ghi");
        assert!(r.as_ref().unwrap().is_transform_query_segment());

        let (p, r) = p.unwrap().predecessor();
        assert!(p.is_none());
        assert!(r.is_none());

        Ok(())
    }

    #[test]
    fn predecessor2() -> Result<(), Error> {
        let q = parse_query("-R/abc/def/-x/ghi/jkl/file.txt")?;
        let (p, r) = q.predecessor();
        assert_eq!(p.as_ref().unwrap().encode(), "-R/abc/def/-x/ghi/jkl");
        assert_eq!(r.as_ref().unwrap().encode(), "-x/file.txt");
        assert!(!r.as_ref().unwrap().is_empty());
        assert!(r.as_ref().unwrap().is_filename());

        let (p, r) = p.unwrap().predecessor();
        assert_eq!(p.as_ref().unwrap().encode(), "-R/abc/def/-x/ghi");
        assert_eq!(r.as_ref().unwrap().encode(), "-x/jkl");
        assert!(!r.as_ref().unwrap().is_empty());
        assert!(!r.as_ref().unwrap().is_filename());
        assert!(r.as_ref().unwrap().is_action_request());

        let (p, r) = p.unwrap().predecessor();
        assert_eq!(p.as_ref().unwrap().encode(), "-R/abc/def");
        assert_eq!(r.as_ref().unwrap().encode(), "-x/ghi");
        assert!(!r.as_ref().unwrap().is_empty());
        assert!(!r.as_ref().unwrap().is_filename());
        assert!(r.as_ref().unwrap().is_action_request());

        let (p, r) = p.unwrap().predecessor();
        assert!(p.is_none());
        assert!(r.is_none());

        Ok(())
    }

    #[test]
    fn predecessor3() -> Result<(), Error> {
        let q = parse_query("-R/a/b/-/c/d")?;
        let (p, r) = q.predecessor();
        assert_eq!(p.as_ref().unwrap().encode(), "-R/a/b/-/c");
        assert_eq!(r.as_ref().unwrap().encode(), "-/d");
        let (p, r) = p.unwrap().predecessor();
        assert_eq!(p.as_ref().unwrap().encode(), "-R/a/b");
        assert_eq!(r.as_ref().unwrap().encode(), "-/c");
        let (p, r) = p.unwrap().predecessor();
        assert!(p.is_none());
        assert!(r.is_none());
        Ok(())
    }

    #[test]
    fn predecessor3a() -> Result<(), Error> {
        let q = parse_query("-R/x/y/-R/a/b/-/c/d")?;
        let (p, r) = q.predecessor();
        assert_eq!(p.as_ref().unwrap().encode(), "-R/x/y/-R/a/b/-/c");
        assert_eq!(r.as_ref().unwrap().encode(), "-/d");
        let (p, r) = p.unwrap().predecessor();
        assert_eq!(p.as_ref().unwrap().encode(), "-R/x/y/-R/a/b");
        assert_eq!(r.as_ref().unwrap().encode(), "-/c");
        let (p, r) = p.unwrap().predecessor();
        assert_eq!(p.as_ref().unwrap().encode(), "-R/x/y");
        assert_eq!(r.as_ref().unwrap().encode(), "-R/a/b");
        let (p, r) = p.unwrap().predecessor();
        assert!(p.is_none());
        assert!(r.is_none());
        Ok(())
    }

    #[test]
    fn all_predecessors1() -> Result<(), Error> {
        let p: Vec<_> = parse_query("ghi/jkl/file.txt")?
            .all_predecessors()
            .iter()
            .flat_map(|(x, _y)| x.as_ref().map(|xx| xx.encode()))
            .collect();
        assert_eq!(p, vec!["ghi/jkl/file.txt", "ghi/jkl", "ghi"]);
        let r: Vec<_> = parse_query("ghi/jkl/file.txt")?
            .all_predecessors()
            .iter()
            .map(|(_x, y)| y.as_ref().map(|xx| xx.encode()))
            .collect();
        assert_eq!(
            r,
            vec![
                None,
                Some("file.txt".to_owned()),
                Some("jkl/file.txt".to_owned())
            ]
        );
        Ok(())
    }

    #[test]
    fn all_predecessors_tuples1() -> Result<(), Error> {
        let p: Vec<_> = parse_query("ghi/jkl/file.txt")?
            .all_predecessor_tuples()
            .iter()
            .map(|(x, y)| format!("{} - {}", x.encode(), y.encode()))
            .collect();
        assert_eq!(p, vec!["ghi/jkl - file.txt", "ghi - jkl", " - ghi"]);
        Ok(())
    }

    #[test]
    fn all_predecessors_tuples1a() -> Result<(), Error> {
        let p: Vec<_> = parse_query("-R/xxx/yyy/-/ghi/jkl/file.txt")?
            .all_predecessor_tuples()
            .iter()
            .map(|(x, y)| format!("{} - {}", x.encode(), y.encode()))
            .collect();
        assert_eq!(
            p,
            vec![
                "-R/xxx/yyy/-/ghi/jkl - -/file.txt",
                "-R/xxx/yyy/-/ghi - -/jkl",
                "-R/xxx/yyy - -/ghi",
                " - -R/xxx/yyy"
            ]
        );
        Ok(())
    }

    #[test]
    fn predecessor_add_filename1() -> Result<(), Error> {
        let q = parse_query("ghi/jkl/file.txt")?;
        if let (Some(p), Some(r)) = q.predecessor() {
            let tp = p.segments[0].transform_query_segment().unwrap();
            let tr = r.transform_query_segment().unwrap();
            assert_eq!(tp.encode(), "ghi/jkl");
            assert_eq!(tr.encode(), "file.txt");
            assert!(!tp.is_filename());
            assert!(tr.is_filename());
            let pr = tp + tr;
            println!("pr: {:#?}", &pr);
            assert!(pr.filename.is_some());
            assert_eq!(pr.encode(), "ghi/jkl/file.txt");
        } else {
            assert!(false);
        }

        Ok(())
    }
}
