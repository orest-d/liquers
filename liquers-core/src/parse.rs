//! Authoritative parser for Liquers keys, queries, and simple query templates.
//!
//! This module defines the textual syntax accepted by [`parse_key`],
//! [`parse_query`], and [`parse_simple_template`]. The data model and operations on
//! parsed values are defined in [`crate::query`].
//!
//! # Lexical elements
//!
//! The parser uses the following lexical forms. `ALPHA` and `ALNUM` in this
//! reference mean the ASCII character classes tested by the parser.
//!
//! ```text
//! identifier      = (ALPHA | "_"), { ALNUM | "_" }
//! resource-name   = (ALNUM | "_" | ".")+,
//!                   { ALNUM | "_" | "." | "-" }
//! filename        = { ALNUM | "_" }, ".",
//!                   (ALNUM | "_" | "." | "-")+
//! header-value    = { ALNUM | "_" | "." }
//! ```
//!
//! An action name is an `identifier`. A resource name cannot begin with `-`.
//! Resource names `.` and `..` are accepted and have relative-key semantics in
//! [`crate::query::Key::to_absolute`].
//!
//! A terminal transform component containing a dot and matching `filename` is
//! parsed as a filename, not an action. For example, `result.json` is a
//! filename-only transform query.
//!
//! # String action parameters
//!
//! An action is an identifier followed by zero or more `-parameter` components. A
//! parameter is either a string or a [link](#link-action-parameters):
//!
//! ```text
//! action           = identifier, { "-", action-parameter }
//! action-parameter = link-parameter | string-parameter
//! link-parameter   = "~X~", link-query, "~E"
//! ```
//!
//! Unescaped parameter text accepts ASCII alphanumeric characters, `_`, `+`, and
//! `.`. The following entities decode inside string parameters:
//!
//! | Encoded | Decoded |
//! |---|---|
//! | `~~` | `~` |
//! | `~_` | `-` |
//! | `~I` | `/` |
//! | `~/` | `/` |
//! | `~.` | space |
//! | `~` followed by one or more digits | `-` followed by those digits |
//! | `~H` | `https://` |
//! | `~h` | `http://` |
//! | `~f` | `file://` |
//! | `~P` | `://` |
//!
//! [`crate::query::encode_token`] emits the general escapes for `~`, space, `/`,
//! and `-`. It does not emit the protocol abbreviations, although the parser
//! accepts them.
//!
//! `~X~` and `~E` are **not** entities. They do not decode to a character inside a string
//! parameter; they delimit a different kind of parameter entirely. See below.
//!
//! Empty parameters are accepted: `action-` contains one empty string parameter.
//!
//! # Link action parameters
//!
//! A parameter of the form `~X~<query>~E` is a **link**: the delimited text is a complete
//! Liquers query, and the parameter parses to
//! [`crate::query::ActionParameter::Link`]. The linked query is evaluated and its result
//! supplied as the argument value.
//!
//! ```text
//! action-~X~hello~E                  one link parameter
//! action-before-~X~hello~E-after     a link between two string parameters
//! action-~X~inner-~X~deep~E~E        links nest
//! action-~X~~E                       an empty embedded query
//! ```
//!
//! A link is a *whole* parameter and is never concatenated with parameter text:
//! `action-abc~X~q~E` is a parse error. Links may appear at any parameter position, but
//! only in a transform segment — a resource path cannot contain one.
//!
//! The `~~` escape still applies inside the embedded query, so `~~E` is an escaped tilde
//! followed by `E`, not a terminator: `action-~X~inner-a~~E~E` has the body
//! `inner-a~~E`, whose parameter decodes to `a~E`.
//!
//! ## The shorthand is not allowed inside a link
//!
//! The resource/transform shorthand (`data/report/-/to_text`) is **discouraged
//! everywhere** — prefer the explicit `-R/data/report/-/to_text`, which is what
//! [`crate::query::Query::encode`] emits and what round-trips unchanged — and inside
//! `~X~…~E` it is **rejected**:
//!
//! ```text
//! action-~X~-R/data/report/-/to_text~E    accepted
//! action-~X~data/report/-/to_text~E       parse error
//! ```
//!
//! The reason is that the shorthand is recognised only when it accounts for the entire
//! query, which the parser detects with `eof`. A link has no `eof` before its `~E`, so
//! accepting the shorthand there would make the same text denote a resource fetch at top
//! level and three command invocations inside a link. Rejecting it removes the ambiguity
//! rather than resolving it silently.
//!
//! The same ambiguity exists inside `$…$` template expansions, where the shorthand is
//! currently accepted with the transform reading and is **not** diagnosed. Prefer the
//! explicit form there too.
//!
//! ## Bounds
//!
//! Links are the only recursive construct in the grammar, and parsing is exponential in
//! nesting depth. [`parse_query`] and [`parse_simple_template`] therefore reject, before
//! parsing, any input nested deeper than 8 links or containing more than 64 in total.
//!
//! ## Round-trip limits of programmatic construction
//!
//! [`crate::query::encode_token`] is the only escaping path in the encoder, and it is
//! applied only to string parameters. Resource names, action names, header names and
//! values, and filenames are emitted raw. A programmatically-set token containing `~X~`
//! or `~E` therefore breaks the encode→parse round-trip, and may re-parse as a *different
//! valid query* rather than failing. None of this is reachable from parsed input, since
//! those productions all exclude `~`; it is an instance of the general rule that
//! programmatic construction is not validation.
//!
//! # Segment headers
//!
//! A query contains resource and transform segments.
//!
//! Resource headers have the form:
//!
//! ```text
//! resource-header =
//!     "-", { "-" }, "R", { ALNUM | "_" }, { "-", header-value }
//! ```
//!
//! Examples are `-R`, `-R-meta`, and `-R-meta-extra`. The text after `R` is the
//! header name; subsequent `-` components are header parameters.
//!
//! Transform headers have either a short or named form and always end in `/` in
//! the parsed text:
//!
//! ```text
//! short-transform-header = "-", { "-" }, "/"
//! named-transform-header = "-", { "-" },
//!                          lower-alpha, { ALNUM | "_" },
//!                          { "-", header-value }, "/"
//! ```
//!
//! Examples are `-/`, `-backend/`, and `-backend-option/`.
//!
//! The number of leading `-` characters minus one is stored in
//! [`crate::query::SegmentHeader::level`]. `level` is reserved for a future
//! feature and is currently unused by query interpretation.
//!
//! Header values are not action parameters: they do not use the entity decoding
//! table and may be empty.
//!
//! # Query forms and parse precedence
//!
//! A query may start with `/`. The parser records this in
//! [`crate::query::Query::absolute`].
//!
//! The parser recognizes these forms in order:
//!
//! 1. **Resource/transform shorthand**: a non-empty headerless resource path
//!    followed by `/` and an explicit transform header, for example
//!    `data/input.csv/-/to_text`. Encoding the result makes the resource header
//!    explicit: `-R/data/input.csv/-/to_text`.
//! 2. **Headerless transform query**: actions and an optional terminal filename,
//!    for example `load/process/result.json`.
//! 3. **General segmented query**: an explicit resource or transform segment,
//!    followed by zero or more explicitly headed segments.
//! 4. **Empty query**: `""` or `"/"`.
//!
//! A pure textual resource query must use an explicit resource header:
//! `-R/path/to/resource`. In contrast, [`parse_key`] accepts the headerless key
//! `path/to/resource`.
//!
//! Within a transform segment, `/` separates actions. A slash immediately followed
//! by `-` starts the next explicitly headed segment rather than another action.
//!
//! # Simple templates
//!
//! [`parse_simple_template`] recognizes:
//!
//! | Syntax | Element |
//! |---|---|
//! | ordinary text | [`SimpleTemplateElement::Text`] |
//! | `$$` | a literal `$` text element |
//! | `$<query>$` | [`SimpleTemplateElement::ExpandQuery`] |
//!
//! The embedded text is parsed with the same query parser described above.
//!
//! # Positions and errors
//!
//! Parsed syntax nodes carry a [`crate::query::Position`]. Offsets are zero-based;
//! lines and columns reported by the parser are one-based. An unknown position uses
//! zero for all fields.
//!
//! The three public parse functions require complete input consumption. Remaining
//! input is returned as a Liquers [`crate::error::Error`] with the position at
//! which parsing stopped.
//!
//! # Examples
//!
//! ```
//! use liquers_core::parse::{parse_key, parse_query, parse_simple_template};
//! use liquers_core::query::QuerySegment;
//!
//! let key = parse_key("data/input.csv")?;
//! assert_eq!(key.encode(), "data/input.csv");
//!
//! let query = parse_query("data/input.csv/-/to_text/result.txt")?;
//! assert_eq!(query.encode(), "-R/data/input.csv/-/to_text/result.txt");
//! assert!(matches!(query.segments[0], QuerySegment::Resource(_)));
//! assert!(matches!(query.segments[1], QuerySegment::Transform(_)));
//!
//! let template = parse_simple_template("Result: $load/to_text$")?;
//! assert_eq!(template.encode(), "Result: $load/to_text$");
//! # Ok::<(), liquers_core::error::Error>(())
//! ```
//!
//! Link parameters:
//!
//! ```
//! use liquers_core::parse::parse_query;
//!
//! // The parameter is a query, not a literal.
//! let query = parse_query("greet-~X~greeting~E")?;
//! let parameter = &query.action().expect("action").parameters[0];
//! assert!(parameter.is_link());
//! assert_eq!(parameter.link_value().expect("link").encode(), "greeting");
//! assert_eq!(query.encode(), "greet-~X~greeting~E");
//!
//! // The explicit resource form is accepted inside a link; the shorthand is not.
//! assert!(parse_query("to_text-~X~-R/data/report/-/to_text~E").is_ok());
//! assert!(parse_query("to_text-~X~data/report/-/to_text~E").is_err());
//! # Ok::<(), liquers_core::error::Error>(())
//! ```

#![allow(unused_imports)]
#![allow(dead_code)]

use nom;

extern crate nom_locate;
use nom::branch::alt;
use nom::character::complete::digit1;
use nom::combinator::{cut, eof, not, opt, peek};
use nom::error::ErrorKind;
use nom::sequence::{preceded, terminated};
use nom_locate::LocatedSpan;

use nom::bytes::complete::{tag, take_while, take_while1};
use nom::multi::{many0, many1, separated_list0, separated_list1};
use nom::*;

use crate::error::{Error, ErrorType};
use crate::query::{
    ActionParameter, ActionRequest, HeaderParameter, Key, Position, Query, QuerySegment,
    ResourceName, ResourceQuerySegment, SegmentHeader, TransformQuerySegment,
};

type Span<'a> = LocatedSpan<&'a str>;

/// Maximum number of `~X~` link markers accepted in one query.
///
/// Links are the only recursive construct in the query grammar. This bounds the total
/// work; [`MAX_LINK_DEPTH`] separately bounds the *nesting*, which is the expensive
/// dimension.
const MAX_LINK_MARKERS: usize = 64;

/// Maximum nesting depth of `~X~...~E` links accepted in one query.
///
/// **Parsing is exponential in nesting depth**, so this bound is a hard requirement
/// rather than a comfort limit. At each transform segment,
/// `transform_segment_without_header` first runs `action_requests`, which parses the
/// action in full — recursing through any nested link — and then discards that work when
/// the required `/` separator does not follow; `filename_or_action` immediately parses the
/// same action again. Two full sub-parses per level gives `T(n) = 2·T(n-1)`.
///
/// Measured on a debug build: depth 10 ≈ 32 ms, 14 ≈ 0.54 s, 16 ≈ 2.3 s, 17 ≈ 4.3 s,
/// doubling per level thereafter. Depth 8 is ≈ 10 ms, which is the worst case this bound
/// admits, and is far above any plausible real query — nesting beyond two or three is
/// already exotic.
///
/// Bounding only the marker count would not help: 64 markers arranged as a single nested
/// chain never finishes. The two bounds are separate because siblings are linear and
/// cheap while nesting is exponential and dangerous.
///
/// Removing this bound requires fixing the double-parse in
/// `transform_segment_without_header`, which is a change to the core query grammar and
/// out of scope here. Tracked in `specs/issues/PARAMETER-ESCAPING-INCOMPLETE.md`.
const MAX_LINK_DEPTH: usize = 8;

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
    let (text, a) = take_while1(|c| AsChar::is_alpha(c as u8) || c == '_')(text)?;
    let (text, b) = take_while(|c| AsChar::is_alphanum(c as u8) || c == '_')(text)?;

    Ok((text, format!("{}{}", a, b)))
}

fn filename(text: Span) -> IResult<Span, String> {
    let (text, a) = take_while(|c| AsChar::is_alphanum(c as u8) || c == '_')(text)?;
    let (text, _dot) = nom::character::complete::char('.')(text)?;
    let (text, b) =
        take_while1(|c| AsChar::is_alphanum(c as u8) || c == '_' || c == '.' || c == '-')(text)?;

    Ok((text, format!("{}.{}", a, b)))
}

fn slash_filename(text: Span) -> IResult<Span, String> {
    let (text, _) = tag("/")(text)?;
    let (text, fname) = filename(text)?;
    Ok((text, fname))
}

fn resource_name(text: Span) -> IResult<Span, ResourceName> {
    let position: Position = text.into();
    let (text, a) = take_while1(|c| AsChar::is_alphanum(c as u8) || c == '_' || c == '.')(text)?;
    let (text, b) =
        take_while(|c| AsChar::is_alphanum(c as u8) || c == '_' || c == '.' || c == '-')(text)?;
    Ok((
        text,
        ResourceName::new(format!("{}{}", a, b)).with_position(position),
    ))
}
fn parameter_text(text: Span) -> IResult<Span, String> {
    let (text, a) =
        take_while1(|c| AsChar::is_alphanum(c as u8) || c == '_' || c == '+' || c == '.')(text)?;
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
    ))
    .parse(text)
}
fn parameter(text: Span) -> IResult<Span, ActionParameter> {
    let position: Position = text.into();
    let (text, par) = many0(alt((parameter_text, entities))).parse(text)?;
    Ok((
        text,
        ActionParameter::new_string(par.join("")).with_position(position),
    ))
}
/// The query grammar accepted between `~X~` and `~E`.
///
/// This is [`query_parser`] minus its two `eof`-gated alternatives, which can never match
/// before a `~E`. Using `query_parser` unchanged would silently route every embedded query
/// through `general_query`, changing the meaning of the resource/transform shorthand.
///
/// The shorthand is rejected here rather than reinterpreted: see the module docs.
///
/// Note this parser cannot return `nom::Err::Error` — `empty_query` never fails, so the
/// only error it produces is the shorthand `Failure`. A malformed body therefore has no
/// error of its own and always surfaces at the terminator match in [`link_parameter`].
fn link_query(text: Span) -> IResult<Span, Query> {
    // Reject, do not reinterpret, the resource/transform shorthand.
    //
    // The inner peek(tag("~E")) asserts that the shorthand accounts for the whole body
    // without consuming the terminator, which link_parameter still needs. The outer peek
    // is defensive rather than necessary: the result is discarded by `.is_ok()` and
    // `text` is a Copy local that is never rebound, so the reported position is the start
    // of the body either way; it is kept to make the non-consuming intent explicit.
    if peek(terminated(resource_transform_query, peek(tag("~E"))))
        .parse(text)
        .is_ok()
    {
        return Err(nom::Err::Failure(nom::error::Error::new(
            text,
            ErrorKind::Verify,
        )));
    }
    alt((general_query, empty_query)).parse(text)
}

/// `link-parameter = "~X~", link-query, "~E"`
///
/// The position is taken at the `~X~` marker, matching how [`parameter`] takes its
/// position at the start of the parameter text. The embedded query is parsed in band on
/// the original span, so its nodes carry absolute offsets in the source query.
fn link_parameter(text: Span) -> IResult<Span, ActionParameter> {
    let position: Position = text.into();
    // Must fail softly so `alt` in action_parameter can fall through to a string
    // parameter. This is the only softly-failing step.
    let (text, _) = tag("~X~")(text)?;
    // Defensive only: link_query cannot currently return Err::Error. Kept so a future
    // failing path there is committed rather than silently backtracked.
    let (text, query) = cut(link_query).parse(text)?;
    // Marked so the message can name the terminator. Reached both when there is no `~E`
    // at all and when the body parse stopped early, which is why the message says
    // "Expected ~E here" rather than "unterminated".
    let (text, _) = tag("~E")(text).map_err(|_: nom::Err<nom::error::Error<Span>>| {
        nom::Err::Failure(nom::error::Error::new(text, ErrorKind::Fail))
    })?;
    Ok((text, ActionParameter::Link(query, position)))
}

/// A single action parameter: a link, or a string parameter.
///
/// [`parameter`] wraps `many0` and so succeeds on empty input; it can never fail.
/// [`link_parameter`] must therefore be tried first or it would never run.
fn action_parameter(text: Span) -> IResult<Span, ActionParameter> {
    alt((link_parameter, parameter)).parse(text)
}

fn minus_parameter(text: Span) -> IResult<Span, ActionParameter> {
    let (text, _) = tag("-")(text)?;
    action_parameter(text)
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
    let (text, parameters) = many0(minus_parameter).parse(text)?;
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
    let (text, parameter) = take_while(|c| (c as u8).is_alphanum() || c == '_' || c == '.')(text)?;
    Ok((
        text,
        HeaderParameter::new(parameter.to_string()).with_position(position),
    ))
}

fn full_transform_segment_header(text: Span) -> IResult<Span, SegmentHeader> {
    let position: Position = text.into();
    let (text, level_lead) = many1(tag("-")).parse(text)?;
    let (text, lead_name) = take_while1(|c: char| (c as u8).is_alpha() && c.is_lowercase())(text)?;
    let (text, rest_name) = take_while(|c| (c as u8).is_alphanum() || c == '_')(text)?;
    let (text, parameters) = many0(header_parameter).parse(text)?;
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
    let (text, level_lead) = many1(tag("-")).parse(text)?;
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
    ))
    .parse(text)
}

fn resource_segment_header(text: Span) -> IResult<Span, SegmentHeader> {
    let position: Position = text.into();
    let (text, level_lead) = many1(tag("-")).parse(text)?;
    let (text, _) = tag("R")(text)?;
    let (text, name) = take_while(|c: char| (c as u8).is_alphanum() || c == '_')(text)?;
    let (text, parameters) = many0(header_parameter).parse(text)?;

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
    separated_list0(tag("/"), resource_name).parse(text)
}
pub(crate) fn resource_path1(text: Span) -> IResult<Span, Vec<ResourceName>> {
    separated_list1(tag("/"), resource_name).parse(text)
}

fn resource_segment_with_header(text: Span) -> IResult<Span, ResourceQuerySegment> {
    let (text, header) = resource_segment_header(text)?;
    let (text, path) = opt(preceded(tag("/"), resource_path1)).parse(text)?;
    let key = if let Some(path) = path {
        Key(path)
    } else {
        Key(vec![])
    };
    Ok((
        text,
        ResourceQuerySegment {
            header: Some(header),
            key,
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
    alt((filename_or_action1, filename_or_action2)).parse(text)
}

fn transform_segment_with_header(text: Span) -> IResult<Span, TransformQuerySegment> {
    //    eprintln!("transform_segment_with_header: {:?}", text);
    let (text, header) = transform_segment_header(text)?;
    //    eprintln!("  header: {:?}", header);
    //    eprintln!("  text:   {:?}", text);
    //let (text, mut query) = many0(terminated(action_request, tag("/")))(text)?;
    let (text, mut query) = action_requests(text)?;
    //    eprintln!("  query:  {:?}", query);
    //    eprintln!("  text:   {:?}", text);
    let (text, fna) = filename_or_action(text)?;
    //    eprintln!("  fna-text:   {:?}", text);
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
                    query,
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
    ))
    .parse(text)?;
    Ok((text, QuerySegment::Transform(tqs)))
}
fn transform_qs1(text: Span) -> IResult<Span, QuerySegment> {
    //    eprintln!("transform_qs1: {:?}", text);
    let (text, tqs) = transform_segment_with_header(text)?;
    //    eprintln!("  tqs text: {:?}", text);
    //    eprintln!("  tqs:      {:?}", tqs);
    Ok((text, QuerySegment::Transform(tqs)))
}
fn query_segment0(text: Span) -> IResult<Span, QuerySegment> {
    alt((resource_qs, transform_qs0)).parse(text)
}
fn query_segment1(text: Span) -> IResult<Span, QuerySegment> {
    //    eprintln!("query_segment1: {:?}", text);
    let (text, x) = alt((resource_qs, transform_qs1)).parse(text)?;
    //    eprintln!("  qs text: {:?}", text);
    //    eprintln!("  qs:      {:?}", x);
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
    let (text, _) = peek(not(tag("-"))).parse(text)?;
    Ok((text, a))
}

fn action_requests(text: Span) -> IResult<Span, Vec<ActionRequest>> {
    many0(terminated(action_request, nonterminating_separator)).parse(text)
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
                    query,
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
    //    eprintln!("simple_transform_query: {:?}", text);
    let (text, abs) = opt(tag("/")).parse(text)?;
    let (text, tqs) = alt((
        transform_segment_without_header,
        //transform_segment_without_header_and_filename,
    ))
    .parse(text)?;
    //    eprintln!("simple_transform_query SUCCESS");
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
    //    eprintln!("resource_transform_query: {:?}", text);
    let (text, abs) = opt(tag("/")).parse(text)?;
    let (text, resource) = resource_path1(text)?;
    let (text, _slash) = tag("/")(text)?;
    let (text, tqs) = transform_segment_with_header(text)?;
    //    eprintln!("resource_transform_query SUCCESS");
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
    //    eprintln!("general_query: {:?}", text);
    let (text, abs) = opt(tag("/")).parse(text)?;
    let (text, q0) = query_segment0(text)?;
    //    eprintln!("q0: {:?}", q0);
    let (text, mut segments) = many0(preceded(tag("/"), query_segment1)).parse(text)?;
    //    eprintln!("segments: {:?}", segments);

    segments.insert(0, q0);
    //    eprintln!("general_query SUCCESS");
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
    let (text, abs) = opt(tag("/")).parse(text)?;
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
    ))
    .parse(text)
}

/// An element produced by [`parse_simple_template`].
#[derive(Debug, PartialEq, Clone)]
pub enum SimpleTemplateElement {
    /// Literal template text.
    Text(String),
    /// A query delimited by `$` characters in the source template.
    ExpandQuery(Query),
}

/// Parsed representation of a simple query template.
///
/// Use [`parse_simple_template`] to construct a template from text. Iteration
/// consumes the template and yields its [`SimpleTemplateElement`] values.
#[derive(Debug, PartialEq, Clone)]
pub struct SimpleTemplate(pub Vec<SimpleTemplateElement>);

impl IntoIterator for SimpleTemplate {
    type Item = SimpleTemplateElement;
    type IntoIter = std::vec::IntoIter<SimpleTemplateElement>;

    fn into_iter(self) -> Self::IntoIter {
        self.0.into_iter()
    }
}

impl SimpleTemplate {
    /// Encode the template using `$query$` for query expansion and literal text
    /// for text elements.
    ///
    /// A programmatically constructed text element containing `$` is emitted
    /// without escaping, so `encode` is not a general round-trip guarantee for
    /// arbitrary manually constructed templates.
    pub fn encode(&self) -> String {
        self.0
            .iter()
            .map(|e| match e {
                SimpleTemplateElement::Text(t) => t.clone(),
                SimpleTemplateElement::ExpandQuery(q) => format!("${}$", q.encode()),
            })
            .collect()
    }
    fn len(&self) -> usize {
        self.0.len()
    }
}

fn tempate_text(text: Span) -> IResult<Span, SimpleTemplateElement> {
    let (text, a) = take_while1(|c| c != '$')(text)?;
    Ok((text, SimpleTemplateElement::Text(a.to_string())))
}

fn template_escape_expand(text: Span) -> IResult<Span, SimpleTemplateElement> {
    let (text, _) = tag("$$")(text)?;
    Ok((text, SimpleTemplateElement::Text("$".to_string())))
}

fn template_expand_query(text: Span) -> IResult<Span, SimpleTemplateElement> {
    let (text, _) = tag("$")(text)?;
    let (text, q) = query_parser(text)?;
    let (text, _) = tag("$")(text)?;
    Ok((text, SimpleTemplateElement::ExpandQuery(q)))
}

fn simple_template(text: Span) -> IResult<Span, SimpleTemplate> {
    let (text, elements) = many0(alt((
        template_escape_expand,
        template_expand_query,
        tempate_text,
    )))
    .parse(text)?;
    Ok((text, SimpleTemplate(elements)))
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

/// Parse a complete Liquers query.
///
/// See the [module-level syntax reference](self) for accepted query forms,
/// escaping, headers, and parse precedence.
///
/// Returns [`ErrorType::ParseError`] if the parser fails or does not consume the
/// complete input.
pub fn parse_query(query: &str) -> Result<Query, Error> {
    if let Some(message) = link_bounds_exceeded(query) {
        return Err(Error::query_parse_error(
            query,
            message,
            &Position::unknown(),
        ));
    }
    let (remainder, path) = query_parser(Span::new(query)).map_err(|e| {
        Error::query_parse_error(query, &describe_query_failure(&e), &nom_error_position(&e))
    })?;
    if !remainder.fragment().is_empty() {
        let position: Position = remainder.into();
        Err(Error::query_parse_error(
            query,
            describe_leftover(remainder.fragment()),
            &position,
        ))
    } else {
        Ok(path)
    }
}

/// Reject input whose link markers exceed [`MAX_LINK_MARKERS`] or [`MAX_LINK_DEPTH`].
///
/// Returns the diagnostic to report, or `None` when the input is within bounds.
///
/// This is a lexical pre-check, never a parse: it decides only whether parsing may be
/// attempted, and the grammar alone decides what the text *means*. The scan honours the
/// `~~` escape so that `a~~E` — an escaped tilde followed by `E` — is not mistaken for a
/// terminator, matching how `entities` consumes it.
///
/// Both counts are upper bounds on what the parser can actually consume, which is the
/// safe direction: overestimating rejects a query the parser would have survived, while
/// underestimating would let an exponential parse through.
fn link_bounds_exceeded(text: &str) -> Option<&'static str> {
    let bytes = text.as_bytes();
    let (mut i, mut count, mut depth, mut max_depth) = (0usize, 0usize, 0usize, 0usize);
    while i < bytes.len() {
        if bytes[i] == b'~' {
            let rest = &bytes[i..];
            if rest.starts_with(b"~~") {
                i += 2; // escaped tilde: neither marker nor terminator
                continue;
            }
            if rest.starts_with(b"~X~") {
                count += 1;
                depth += 1;
                max_depth = max_depth.max(depth);
                i += 3;
                continue;
            }
            if rest.starts_with(b"~E") {
                depth = depth.saturating_sub(1);
                i += 2;
                continue;
            }
        }
        i += 1;
    }
    if count > MAX_LINK_MARKERS {
        Some("Too many link parameters")
    } else if max_depth > MAX_LINK_DEPTH {
        Some("Link parameters nested too deeply")
    } else {
        None
    }
}

/// Position of the input span a nom error stopped at.
fn nom_error_position(err: &nom::Err<nom::error::Error<Span>>) -> Position {
    match err {
        nom::Err::Error(e) | nom::Err::Failure(e) => e.input.into(),
        nom::Err::Incomplete(_) => Position::unknown(),
    }
}

/// Human-readable cause for a failed query parse.
fn describe_query_failure(err: &nom::Err<nom::error::Error<Span>>) -> String {
    match err {
        nom::Err::Error(e) | nom::Err::Failure(e) => match e.code {
            // Private markers set in link_query / link_parameter. No other production in
            // this file uses `verify` or `fail`, so these codes cannot arrive elsewhere.
            ErrorKind::Verify => "Resource/transform shorthand is not allowed inside \
                 ~X~...~E; use the explicit form, for example -R/a/b/-/c"
                .to_owned(),
            // True whether the terminator is missing or merely displaced by a body that
            // stopped early; a malformed body has no error of its own.
            ErrorKind::Fail => "Expected ~E here to close ~X~".to_owned(),
            // ErrorKind is nom's enum, not ours, so a catch-all arm is correct here.
            // Not "Can't parse query": query_parse_error already prefixes that.
            _ => "unexpected input".to_owned(),
        },
        nom::Err::Incomplete(_) => "Incomplete query".to_owned(),
    }
}

/// The `$...$` expansion spans of a template, excluding the literal text between them.
///
/// Used only to scope the link bounds: literal template text never reaches
/// [`query_parser`], so link markers appearing in prose must not be counted against a
/// query's limits. Mirrors the template grammar's own precedence by treating `$$` as an
/// escaped dollar before looking for an expansion.
///
/// Deliberately conservative rather than exact: an unterminated trailing `$` yields its
/// remainder as a span, which the parser rejects anyway, and mis-slicing can only cause
/// text that *might* be a query to be inspected.
fn template_query_spans(text: &str) -> Vec<&str> {
    let mut spans = Vec::new();
    let mut rest = text;
    while let Some(open) = rest.find('$') {
        let after = &rest[open + 1..];
        if let Some(stripped) = after.strip_prefix('$') {
            rest = stripped; // `$$` is a literal dollar, not an expansion
            continue;
        }
        match after.find('$') {
            Some(close) => {
                spans.push(&after[..close]);
                rest = &after[close + 1..];
            }
            None => {
                spans.push(after);
                break;
            }
        }
    }
    spans
}

/// Diagnose text the parser stopped before consuming.
///
/// A stray `~E` is not a parse failure: `parameter` simply halts there, the action
/// completes, and the terminator is left over. It therefore never reaches
/// [`describe_query_failure`] and has to be diagnosed from the remaining text.
fn describe_leftover(rest: &str) -> &'static str {
    if rest.starts_with("~E") {
        "Unpaired ~E: link terminator without a matching ~X~"
    } else if rest.starts_with("~X~") {
        // Reached for `action-abc~X~q~E` (concatenated with parameter text) and for
        // `-R/data~X~q~E` (resource paths cannot contain links). One message covers both.
        "~X~...~E is only valid as a complete action parameter"
    } else {
        "Can't parse query completely"
    }
}

/// Parse a complete headerless resource key.
///
/// The empty string produces [`Key::new`]. A leading `/` is not accepted. Key
/// elements are separated by `/`; see the module-level `resource-name` grammar.
///
/// Returns [`ErrorType::ParseError`] if the parser fails or does not consume the
/// complete input.
pub fn parse_key<S: AsRef<str>>(key: S) -> Result<Key, Error> {
    let (remainder, path) = resource_path(Span::new(key.as_ref())).map_err(|e| {
        let em = format!("{}", e);
        Error::key_parse_error(key.as_ref(), &em, &Position::unknown())
    })?;
    if !remainder.fragment().is_empty() {
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

/// Parse a complete simple query template.
///
/// `$$` represents a literal dollar sign and `$query$` represents a query
/// expansion. The embedded query uses [`parse_query`]'s grammar.
pub fn parse_simple_template<S: AsRef<str>>(template_text: S) -> Result<SimpleTemplate, Error> {
    // Templates reach query_parser through `$...$` expansion, so they inherit the link
    // recursion and need the same bound. It applies to the expansion spans ONLY: literal
    // template text is never parsed as a query, so `~X~` appearing in prose — including
    // prose documenting this very syntax — must not be counted. Only the guard applies
    // here; the positioned error mapping is scoped to parse_query.
    for span in template_query_spans(template_text.as_ref()) {
        if let Some(message) = link_bounds_exceeded(span) {
            return Err(Error::general_error(message.to_owned()));
        }
    }
    let (remainder, template) =
        simple_template(Span::new(template_text.as_ref())).map_err(|e| {
            let em = format!("{}", e);
            Error::general_error(format!("Error parsing template: {}", em))
        })?;
    if !remainder.fragment().is_empty() {
        let position: Position = remainder.into();
        Err(
            Error::general_error("Can't parse template completely".to_owned())
                .with_position(&position),
        )
    } else {
        Ok(template)
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
        eprintln!("REMAINDER: {:#?}", remainder);
        eprintln!("PATH:      {:#?}", path);
        assert_eq!(remainder.fragment().len(), 0);
        assert_eq!(remainder.to_string().len(), 0);
        Ok(())
    }

    #[test]
    fn transform_segment_without_header_test() -> Result<(), Box<dyn std::error::Error>> {
        let (remainder, tqs) = transform_segment_without_header(Span::new("abc/def/file.txt"))?;
        eprintln!("REMAINDER: {:#?}", remainder);
        eprintln!("TQS:      {:#?}", tqs);
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
        eprintln!("remainder {s}");
        eprintln!("path      {:?}", p);
        assert_eq!(s.fragment().len(), 0);

        let (s, rqs) = resource_segment_with_header(Span::new("-R/a/b/c"))?;
        eprintln!("remainder {s}");
        eprintln!("rqs     {:?}", rqs);
        eprintln!("rqs enc {}", rqs.encode());
        assert_eq!(s.fragment().len(), 0);
        let path = parse_query("-R/a/b/c")?;
        assert_eq!(path.len(), 1);
        let path = parse_query("-R/a/b/-/c/d")?;
        assert_eq!(path.len(), 2);
        let (s, q) = resource_transform_query(Span::new("a/b/-/c/d"))?;
        eprintln!("remainder '{s}'");
        eprintln!("query     {:?}", q);
        eprintln!("query enc {:?}", q.encode());
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
        eprintln!("rest: {:?}", rest);
        eprintln!("ar:   {:?}", ar);
        eprintln!();
        assert_eq!(ar.len(), 1);
        let (rest, q) = transform_segment_without_header(Span::new("abc/def/-/xxx/-q/qqq"))?;
        eprintln!("rest: {:?}", rest);
        eprintln!("tqs:  {:?}", q);
        eprintln!();
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
        eprintln!("rest: {:?}", rest);
        eprintln!("gq:  {}", q.encode());
        eprintln!("gq:  {:#?}", q);
        eprintln!();
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
        eprintln!("rest: {:?}", rest);
        eprintln!("gq:  {}", q.encode());
        eprintln!("gq:  {:#?}", q);
        eprintln!();
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
        eprintln!("rest: {:?}", rest);
        eprintln!("gq:  {}", q.encode());
        eprintln!("gq:  {:#?}", q);
        eprintln!();
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
        eprintln!("rest: {:?}", rest);
        eprintln!("gq:  {}", q.encode());
        eprintln!("gq:  {:#?}", q);
        eprintln!();
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
            eprintln!("pr: {:#?}", &pr);
            assert!(pr.filename.is_some());
            assert_eq!(pr.encode(), "ghi/jkl/file.txt");
        } else {
            assert!(false);
        }

        Ok(())
    }

    #[test]
    fn simple_template_parsing() -> Result<(), Error> {
        let t = parse_simple_template("abc")?;
        assert_eq!(t.0.len(), 1);
        assert_eq!(t.0[0], SimpleTemplateElement::Text("abc".to_owned()));
        let t = parse_simple_template("abc$def/ghi$")?;
        assert_eq!(t.0.len(), 2);
        assert_eq!(t.0[0], SimpleTemplateElement::Text("abc".to_owned()));
        eprintln!("Template: {:?}", t);
        if let SimpleTemplateElement::ExpandQuery(q) = &t.0[1] {
            assert_eq!(q.encode(), "def/ghi");
        } else {
            assert!(false);
        }
        Ok(())
    }

    #[test]
    fn documented_query_language_contract() -> Result<(), Error> {
        let shorthand = parse_query("data/input.csv/-/load-x~_y")?;
        assert_eq!(shorthand.encode(), "-R/data/input.csv/-/load-x~_y");
        assert!(shorthand.segments[0].is_resource_query_segment());
        assert_eq!(
            shorthand.segments[1].action().unwrap().parameters[0].string_value(),
            Some("x-y".to_owned())
        );

        let absolute = parse_query("/--R-meta-extra/data/input.csv")?;
        assert!(absolute.absolute);
        let resource = absolute.resource_query().unwrap();
        let header = resource.header.unwrap();
        assert_eq!(header.level, 1);
        assert_eq!(header.parameters[0].value, "meta");
        assert_eq!(header.parameters[1].value, "extra");

        let empty_parameter = parse_query("action-")?.action().unwrap();
        assert_eq!(
            empty_parameter.parameters[0].string_value(),
            Some(String::new())
        );

        // Link parameters parse and round-trip.
        let link_query = parse_query("action-~X~hello~E")?;
        let link = &link_query.action().expect("action").parameters[0];
        assert!(link.is_link());
        assert_eq!(link.link_value().expect("link").encode(), "hello");
        assert_eq!(link_query.encode(), "action-~X~hello~E");
        Ok(())
    }
}


#[cfg(test)]
mod link_tests {
    use super::*;
    use crate::query::ActionParameter;

    /// The 15 canonical forms `Query::encode` can emit. Each re-encodes to itself.
    const CANONICAL: [&str; 15] = [
        "",
        "/",
        "abc-def",
        "action-",
        "file.txt",
        "ghi/jkl/file.txt",
        "abc/def/-/xxx/-q/qqq",
        "-R",
        "-R/a/b",
        "-R/a/b/-/c",
        "-R-meta/-/dr",
        "-R/abc/def/-/ghi/jkl/file.txt",
        "-x/ghi/jkl/file.txt",
        "/--R-meta-extra/data/input.csv",
        "-R/x/y/-R/a/b/-/c/d",
    ];

    /// Bodies where the in-link reading would differ from the top-level one.
    const SHORTHAND: [&str; 3] = ["abc/def/-/xxx", "data/report/-/to_text", "a/b/-/c"];

    /// The parameters of the query's single action.
    fn params(query: &str) -> Result<Vec<ActionParameter>, Error> {
        Ok(parse_query(query)?
            .action()
            .ok_or_else(|| Error::general_error("no action".to_owned()))?
            .parameters)
    }

    /// The embedded query of parameter `n`, encoded.
    fn link_body(query: &str, n: usize) -> Result<String, Error> {
        let p = params(query)?;
        p[n].link_value()
            .map(|q| q.encode())
            .ok_or_else(|| Error::general_error(format!("parameter {n} is not a link")))
    }

    // ---------- Group A: positive parsing ----------

    #[test]
    fn a1_link_single_parameter() -> Result<(), Error> {
        let q = parse_query("action-~X~hello~E")?;
        let action = q.action().expect("action");
        assert_eq!(action.name, "action");
        assert_eq!(action.parameters.len(), 1);
        assert!(action.parameters[0].is_link());
        assert_eq!(link_body("action-~X~hello~E", 0)?, "hello");
        assert_eq!(q.encode(), "action-~X~hello~E");
        Ok(())
    }

    #[test]
    fn a2_link_between_string_parameters() -> Result<(), Error> {
        let p = params("action-before-~X~hello~E-after")?;
        assert_eq!(p.len(), 3);
        assert_eq!(p[0].string_value(), Some("before".to_owned()));
        assert!(p[1].is_link());
        assert_eq!(p[2].string_value(), Some("after".to_owned()));
        assert_eq!(
            parse_query("action-before-~X~hello~E-after")?.encode(),
            "action-before-~X~hello~E-after"
        );
        Ok(())
    }

    #[test]
    fn a3_link_at_every_parameter_position() -> Result<(), Error> {
        // First.
        let p = params("action-~X~q1~E-b-c")?;
        assert!(p[0].is_link() && p[1].is_string() && p[2].is_string());
        // Middle.
        let p = params("action-a-~X~q2~E-c")?;
        assert!(p[0].is_string() && p[1].is_link() && p[2].is_string());
        // Last.
        let p = params("action-a-b-~X~q3~E")?;
        assert!(p[0].is_string() && p[1].is_string() && p[2].is_link());
        Ok(())
    }

    #[test]
    fn a4_link_multi_segment_embedded_query() -> Result<(), Error> {
        let text = "action-~X~-R/data/report/-/to_text~E";
        assert_eq!(link_body(text, 0)?, "-R/data/report/-/to_text");
        let inner = params(text)?[0].link_value().expect("link");
        assert_eq!(inner.segments.len(), 2);
        assert!(inner.segments[0].is_resource_query_segment());
        assert!(inner.segments[1].is_transform_query_segment());
        assert_eq!(parse_query(text)?.encode(), text);
        Ok(())
    }

    #[test]
    fn a5_link_embedded_entities() -> Result<(), Error> {
        let inner = params("action-~X~cmd-x~_y~E")?[0].link_value().expect("link");
        assert_eq!(
            inner.action().expect("inner").parameters[0].string_value(),
            Some("x-y".to_owned())
        );
        let inner = params("action-~X~cmd-x~.y~E")?[0].link_value().expect("link");
        assert_eq!(
            inner.action().expect("inner").parameters[0].string_value(),
            Some("x y".to_owned())
        );
        Ok(())
    }

    #[test]
    fn a6_link_nested() -> Result<(), Error> {
        let text = "action-~X~inner-~X~deep~E~E";
        assert_eq!(link_body(text, 0)?, "inner-~X~deep~E");
        let inner = params(text)?[0].link_value().expect("outer link");
        let deep = &inner.action().expect("inner action").parameters[0];
        assert!(deep.is_link());
        assert_eq!(deep.link_value().expect("deep").encode(), "deep");
        assert_eq!(parse_query(text)?.encode(), text);
        Ok(())
    }

    #[test]
    fn a7_link_empty_embedded_query() -> Result<(), Error> {
        let p = params("action-~X~~E")?;
        assert_eq!(p.len(), 1);
        assert!(p[0].is_link());
        assert_eq!(link_body("action-~X~~E", 0)?, "");
        assert_eq!(parse_query("action-~X~~E")?.encode(), "action-~X~~E");
        Ok(())
    }

    #[test]
    fn a8_link_position_is_marker_offset() -> Result<(), Error> {
        //                0123456789
        let position = params("action-~X~hello~E")?[0].position();
        assert_eq!(position.offset, 7);
        assert_eq!(position.line, 1);
        assert_eq!(position.column, 8); // 1-based
        Ok(())
    }

    #[test]
    fn a9_link_inner_positions_are_absolute() -> Result<(), Error> {
        //                0         1
        //                0123456789012345678
        let text = "action-~X~cmd-param~E";
        let link = &params(text)?[0];
        assert_eq!(link.position().offset, 7);
        // In-band parsing: the embedded action carries an offset in the ORIGINAL string.
        let inner = link.link_value().expect("link");
        assert_eq!(inner.action().expect("inner").position.offset, 10);
        Ok(())
    }

    #[test]
    fn a10_link_inside_simple_template() -> Result<(), Error> {
        let t = parse_simple_template("Result: $action-~X~nested~E$")?;
        assert_eq!(t.0.len(), 2);
        assert_eq!(t.0[0], SimpleTemplateElement::Text("Result: ".to_owned()));
        match &t.0[1] {
            SimpleTemplateElement::ExpandQuery(q) => {
                assert_eq!(q.encode(), "action-~X~nested~E");
                assert!(q.action().expect("action").parameters[0].is_link());
            }
            SimpleTemplateElement::Text(other) => panic!("expected a query, got {other:?}"),
        }
        assert_eq!(t.encode(), "Result: $action-~X~nested~E$");
        Ok(())
    }

    #[test]
    fn a11_link_multiple_siblings() -> Result<(), Error> {
        let text = "action-~X~q1~E-~X~q2~E";
        let p = params(text)?;
        assert_eq!(p.len(), 2);
        assert!(p[0].is_link() && p[1].is_link());
        assert_eq!(link_body(text, 0)?, "q1");
        assert_eq!(link_body(text, 1)?, "q2");
        assert_ne!(p[0].position().offset, p[1].position().offset);
        assert_eq!(parse_query(text)?.encode(), text);
        Ok(())
    }

    #[test]
    fn a12_link_followed_by_filename() -> Result<(), Error> {
        let q = parse_query("action-~X~q~E/out.json")?;
        assert_eq!(q.filename().expect("filename").encode(), "out.json");
        let action = q.segments[0]
            .transform_query_segment()
            .expect("transform")
            .query[0]
            .clone();
        assert!(action.parameters[0].is_link());
        assert_eq!(q.encode(), "action-~X~q~E/out.json");
        Ok(())
    }

    #[test]
    fn a13_link_in_named_header_segment() -> Result<(), Error> {
        let q = parse_query("-backend/action-~X~q~E")?;
        let seg = q.segments[0].transform_query_segment().expect("transform");
        assert_eq!(seg.header.as_ref().expect("header").name, "backend");
        assert!(seg.query[0].parameters[0].is_link());
        assert_eq!(q.encode(), "-backend/action-~X~q~E");
        Ok(())
    }

    #[test]
    fn a14_link_in_multi_segment_query() -> Result<(), Error> {
        let text = "first/second-~X~q~E/-/third";
        let q = parse_query(text)?;
        assert_eq!(q.segments.len(), 2);
        let first = q.segments[0].transform_query_segment().expect("transform");
        assert_eq!(first.query.len(), 2);
        assert!(first.query[1].parameters[0].is_link());
        assert_eq!(q.encode(), text);
        Ok(())
    }

    #[test]
    fn a15_non_ascii_is_not_part_of_the_grammar() -> Result<(), Error> {
        // The identifier grammar is ASCII-only: `identifier` tests `c as u8`, so a
        // multi-byte char can never begin or continue an action name. A link cannot
        // rescue it. Pinned because the Phase 3 plan originally assumed otherwise.
        let err = parse_query("caf\u{e9}-~X~cmd~E").expect_err("non-ASCII must be rejected");
        assert_eq!(err.error_type, ErrorType::ParseError);
        assert_eq!(err.position.offset, 3); // the 'é'
        // The ASCII prefix alone parses, link and all.
        assert!(parse_query("caf-~X~cmd~E").is_ok());
        Ok(())
    }

    #[test]
    fn a16_link_body_with_escaped_tilde_before_e() -> Result<(), Error> {
        // `~~E` is an escaped tilde followed by `E`, NOT a terminator. This is the case
        // a delimiter scanner would have to special-case; in-band parsing gets it free.
        let text = "action-~X~inner-a~~E~E";
        assert_eq!(link_body(text, 0)?, "inner-a~~E");
        let inner = params(text)?[0].link_value().expect("link");
        assert_eq!(
            inner.action().expect("inner").parameters[0].string_value(),
            Some("a~E".to_owned())
        );
        assert_eq!(parse_query(text)?.encode(), text);
        Ok(())
    }

    // ---------- Group B: round-trip and grammar equivalence ----------

    #[test]
    fn b1_link_roundtrip_programmatic() -> Result<(), Error> {
        for body in ["hello", "load/process", "-R/data/file.csv", "action-x~_y"] {
            let link = ActionParameter::new_link(parse_query(body)?);
            let action = ActionRequest::new("act".to_owned()).with_parameters(vec![link.clone()]);
            let encoded = action.encode();
            assert_eq!(encoded, format!("act-~X~{body}~E"));
            let reparsed = parse_query(&encoded)?;
            assert_eq!(reparsed.encode(), encoded);
            assert_eq!(reparsed.action().expect("action").parameters[0], link);
        }
        Ok(())
    }

    #[test]
    fn b2_link_roundtrip_handwritten() -> Result<(), Error> {
        for text in [
            "action-~X~hello~E",
            "fetch-~X~-R/data/file.csv~E",
            "action-before-~X~hello~E-after",
            "action-~X~inner-~X~deep~E~E",
        ] {
            let once = parse_query(text)?.encode();
            assert_eq!(once, text);
            assert_eq!(parse_query(&once)?.encode(), once);
        }
        Ok(())
    }

    #[test]
    fn b3_link_body_canonical_corpus() -> Result<(), Error> {
        for body in CANONICAL {
            let text = format!("act-~X~{body}~E");
            let parsed = parse_query(&text)
                .unwrap_or_else(|e| panic!("canonical body {body:?} rejected: {e}"));
            assert_eq!(parsed.encode(), text, "round-trip failed for {body:?}");
        }
        Ok(())
    }

    #[test]
    fn b4_link_body_matches_toplevel_meaning() -> Result<(), Error> {
        // The property the whole design turns on: an embedded query means what the same
        // text means at top level. Ranges over exactly the canonical corpus.
        for body in CANONICAL {
            let top = parse_query(body)?.encode();
            let embedded = link_body(&format!("act-~X~{body}~E"), 0)?;
            assert_eq!(embedded, top, "divergent reading for {body:?}");
        }
        Ok(())
    }

    #[test]
    fn b5_link_body_rejects_shorthand_corpus() -> Result<(), Error> {
        // The other side of b4: every body whose reading WOULD diverge is refused.
        for body in SHORTHAND {
            let text = format!("act-~X~{body}~E");
            let err = parse_query(&text)
                .expect_err(&format!("shorthand body {body:?} must be rejected"));
            assert_eq!(err.error_type, ErrorType::ParseError);
            assert!(
                err.message.contains("-R/"),
                "message must name the explicit form, got: {}",
                err.message
            );
            // Position is the first character of the link body.
            assert_eq!(err.position.offset, "act-~X~".len());
            // The same query written explicitly is accepted.
            assert!(parse_query(&format!("act-~X~-R/{body}~E")).is_ok());
        }
        Ok(())
    }

    #[test]
    fn b6_link_equality_and_hash() -> Result<(), Error> {
        use std::collections::hash_map::DefaultHasher;
        use std::hash::{Hash, Hasher};

        let parsed = params("act-~X~load/process~E")?[0].clone();
        let built = ActionParameter::new_link(parse_query("load/process")?);
        assert_eq!(parsed, built, "equality ignores position");

        let hash = |p: &ActionParameter| {
            let mut h = DefaultHasher::new();
            p.hash(&mut h);
            h.finish()
        };
        assert_eq!(hash(&parsed), hash(&built));

        let other = ActionParameter::new_link(parse_query("different")?);
        assert_ne!(parsed, other);
        Ok(())
    }

    // ---------- Group C: error paths and pinned behavior ----------

    #[test]
    fn c1_link_shorthand_rejected() -> Result<(), Error> {
        let err = parse_query("to_text-~X~data/report/-/to_text~E")
            .expect_err("shorthand must be rejected inside a link");
        assert_eq!(err.error_type, ErrorType::ParseError);
        assert!(
            err.message.contains("shorthand") && err.message.contains("-R/"),
            "message must diagnose and name the fix, got: {}",
            err.message
        );
        assert_eq!(err.position.offset, "to_text-~X~".len());
        Ok(())
    }

    #[test]
    fn c2_link_explicit_resource_accepted() -> Result<(), Error> {
        // The contrast case: the rejection is targeted, not a blanket ban on resources.
        let text = "to_text-~X~-R/data/report/-/to_text~E";
        assert_eq!(link_body(text, 0)?, "-R/data/report/-/to_text");
        Ok(())
    }

    #[test]
    fn c3_link_unterminated() -> Result<(), Error> {
        let err = parse_query("action-~X~hello").expect_err("unterminated link");
        assert_eq!(err.error_type, ErrorType::ParseError);
        assert!(
            err.message.contains("~E"),
            "message must name the terminator, got: {}",
            err.message
        );
        assert_eq!(err.position.offset, "action-~X~hello".len());
        Ok(())
    }

    #[test]
    fn c4_link_body_stops_early() -> Result<(), Error> {
        // A space cannot appear in parameter text, so the body halts there and the
        // terminator match fails AT the space -- which is why the message says
        // "Expected ~E here" rather than claiming the link is unterminated.
        let err = parse_query("action-~X~a b~E").expect_err("body stops at the space");
        assert_eq!(err.error_type, ErrorType::ParseError);
        assert!(err.message.contains("~E"), "got: {}", err.message);
        assert_eq!(err.position.offset, "action-~X~a".len());
        Ok(())
    }

    #[test]
    fn c5_link_concatenated_with_text() -> Result<(), Error> {
        // A link is a whole parameter, never concatenated with parameter text.
        let err = parse_query("action-abc~X~q~E").expect_err("concatenation is invalid");
        assert_eq!(err.error_type, ErrorType::ParseError);
        assert!(
            err.message.contains("complete action parameter"),
            "got: {}",
            err.message
        );
        assert_eq!(err.position.offset, 10);
        Ok(())
    }

    #[test]
    fn c6_unpaired_link_terminator() -> Result<(), Error> {
        // A stray ~E is not a parse failure -- `parameter` halts and it is left over --
        // so it is diagnosed from the remaining text, not from a nom error.
        let err = parse_query("action-a~E").expect_err("unpaired terminator");
        assert_eq!(err.error_type, ErrorType::ParseError);
        assert!(err.message.contains("Unpaired ~E"), "got: {}", err.message);
        assert_eq!(err.position.offset, 8);

        let err = parse_query("action-a~Eb").expect_err("unpaired terminator mid-text");
        assert!(err.message.contains("Unpaired ~E"), "got: {}", err.message);
        assert_eq!(err.position.offset, 8);
        Ok(())
    }

    #[test]
    fn c7_link_marker_guard() -> Result<(), Error> {
        // Siblings are linear and cheap, so the count bound is generous. The limit is
        // inclusive: 64 sibling links parse, 65 do not.
        let accept = format!("a{}", "-~X~q~E".repeat(MAX_LINK_MARKERS));
        assert_eq!(accept.matches("~X~").count(), MAX_LINK_MARKERS);
        let parsed = parse_query(&accept)?;
        assert_eq!(
            parsed.action().expect("action").parameters.len(),
            MAX_LINK_MARKERS
        );

        let reject = format!("a{}", "-~X~q~E".repeat(MAX_LINK_MARKERS + 1));
        let err = parse_query(&reject).expect_err("over the marker limit");
        assert_eq!(err.error_type, ErrorType::ParseError);
        // Assert the exact literal: `contains("too many")` is case-sensitive.
        assert!(
            err.message.contains("Too many link parameters"),
            "got: {}",
            err.message
        );
        Ok(())
    }

    /// `a-` followed by `n` nested links.
    fn nested_links(n: usize) -> String {
        format!("a-{}{}", "~X~a-".repeat(n), "~E".repeat(n))
    }

    #[test]
    fn c8_link_deep_nesting_rejected_by_guard() -> Result<(), Error> {
        // This does NOT prove that nothing bad would happen -- it asserts the guard
        // rejects BEFORE parsing. That matters more than it sounds: parsing is
        // exponential in nesting depth, so the failure mode here is an unbounded hang,
        // not a stack overflow. See MAX_LINK_DEPTH.
        let text = nested_links(MAX_LINK_DEPTH + 1);
        let err = parse_query(&text).expect_err("over the depth limit");
        assert_eq!(err.error_type, ErrorType::ParseError);
        assert!(
            err.message.contains("nested too deeply"),
            "must be rejected by the depth guard, not by parsing: {}",
            err.message
        );

        // A nested chain within the *count* bound but over the depth bound must still be
        // refused -- bounding markers alone would let an exponential parse through.
        let text = nested_links(MAX_LINK_MARKERS);
        assert!(text.matches("~X~").count() <= MAX_LINK_MARKERS);
        assert!(parse_query(&text).is_err());
        Ok(())
    }

    #[test]
    fn c8b_link_max_permitted_depth_parses() -> Result<(), Error> {
        // Validates the constant rather than merely asserting it: the deepest input the
        // guard permits must actually parse, and quickly. This test is what caught
        // MAX_LINK_DEPTH being set far too high in the original design.
        let text = nested_links(MAX_LINK_DEPTH);
        let started = std::time::Instant::now();
        let parsed = parse_query(&text)?;
        let elapsed = started.elapsed();
        assert_eq!(parsed.encode(), text);
        assert!(
            elapsed < std::time::Duration::from_secs(1),
            "parsing at the permitted depth took {elapsed:?}; parsing is exponential in \
             depth, so MAX_LINK_DEPTH is too high"
        );

        // Confirm the nesting really is MAX_LINK_DEPTH deep.
        let mut depth = 0;
        let mut current = parsed;
        while let Some(link) = current
            .action()
            .and_then(|a| a.parameters.first().and_then(|p| p.link_value()))
        {
            depth += 1;
            current = link;
        }
        assert_eq!(depth, MAX_LINK_DEPTH);
        Ok(())
    }

    #[test]
    fn c8c_escaped_tilde_does_not_confuse_the_bounds_scan() -> Result<(), Error> {
        // The pre-scan honours `~~`, so `a~~E` is not counted as a terminator and the
        // depth bookkeeping stays correct. Regression guard for the scan, not the parser.
        assert!(link_bounds_exceeded("action-~X~inner-a~~E~E").is_none());
        assert!(parse_query("action-~X~inner-a~~E~E").is_ok());
        Ok(())
    }

    #[test]
    fn c10_link_not_allowed_in_resource_path() -> Result<(), Error> {
        // Not by any rule about links: `resource_name` accepts `-` but halts at `~`,
        // leaving text nothing can parse.
        let err = parse_query("-R/data-~X~q~E").expect_err("links are transform-only");
        assert_eq!(err.error_type, ErrorType::ParseError);
        assert!(
            err.message.contains("complete action parameter"),
            "got: {}",
            err.message
        );
        Ok(())
    }

    #[test]
    fn c11_predecessor_does_not_descend_into_link() -> Result<(), Error> {
        // Link dependencies are resolved in plan.rs via ParameterValue::ParameterLink,
        // which is a separate mechanism from predecessor decomposition. Pinned so the
        // question is not re-litigated.
        let q = parse_query("first/second-~X~inner~E")?;
        let encodings: Vec<String> = q
            .all_predecessors()
            .iter()
            .filter_map(|(p, _)| p.as_ref().map(|x| x.encode()))
            .collect();
        assert!(
            !encodings.iter().any(|e| e == "inner"),
            "embedded query must not appear in the predecessor chain: {encodings:?}"
        );
        assert!(encodings.contains(&"first/second-~X~inner~E".to_owned()));
        assert!(encodings.contains(&"first".to_owned()));
        Ok(())
    }

    #[test]
    fn c12_template_bounds_apply_only_to_expansion_spans() -> Result<(), Error> {
        // Literal template text is never parsed as a query, so link markers in prose --
        // including prose documenting the link syntax -- must not count against the
        // bounds. Regression test for a review finding on PR #17.
        let prose = "~X~".repeat(MAX_LINK_DEPTH + 1);
        let template = parse_simple_template(&prose)?;
        assert_eq!(template.0.len(), 1);
        assert_eq!(template.0[0], SimpleTemplateElement::Text(prose.clone()));

        // Documentation about nesting, in literal text: accepted.
        let doc = format!(
            "Links nest: {}{}",
            "~X~a-".repeat(MAX_LINK_DEPTH + 4),
            "~E".repeat(MAX_LINK_DEPTH + 4)
        );
        assert!(parse_simple_template(&doc).is_ok());

        // Inside an expansion the bound still applies.
        let over = format!(
            "$a-{}{}$",
            "~X~a-".repeat(MAX_LINK_DEPTH + 1),
            "~E".repeat(MAX_LINK_DEPTH + 1)
        );
        let err = parse_simple_template(&over).expect_err("expansion is still bounded");
        assert!(err.message.contains("nested too deeply"), "got: {}", err.message);

        // Prose over the limit plus a legal expansion: still accepted.
        let mixed = format!("{prose} and $a-~X~b~E$");
        let template = parse_simple_template(&mixed)?;
        assert_eq!(template.0.len(), 2);
        Ok(())
    }

    #[test]
    fn c12b_template_query_spans_skips_escaped_dollars() -> Result<(), Error> {
        assert_eq!(template_query_spans("no expansion here"), Vec::<&str>::new());
        assert_eq!(template_query_spans("a $q$ b"), vec!["q"]);
        assert_eq!(template_query_spans("$a$ text $b$"), vec!["a", "b"]);
        // `$$` is an escaped dollar and must not open a span.
        assert_eq!(template_query_spans("cost: $$5 and $q$"), vec!["q"]);
        assert_eq!(template_query_spans("$$"), Vec::<&str>::new());
        Ok(())
    }
}

