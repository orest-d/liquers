# Phase 2: Solution & Architecture - query-link-parser

## Overview

Three new private parser productions in `liquers-core/src/parse.rs`, plus a positioned
error mapping in `parse_query`. No new public types, no new commands, no signature changes
anywhere. The embedded query is parsed **in band** on the original `Span` — there is no
substring extraction — so inner node positions are absolute in the source text for free.
The resource/transform shorthand is detected inside a link and rejected with a worded
diagnostic rather than reinterpreted.

## Data Structures

### New Structs

**None.** The feature adds productions, not state.

### New Enums

**None.** `ActionParameter::Link(Query, Position)` already exists
(`liquers-core/src/query.rs:540`) and is unchanged. No new match sites on Liquers-owned
enums are introduced, so the no-default-arm rule is not engaged.

### ExtValue Extensions

Not applicable — `liquers-core` has no `ExtValue`.

## Trait Implementations

**None.** No new traits, no new impls. `ActionParameter`'s existing `QueryRenderer`,
`PartialEq`, `Hash` and `Display` impls already handle `Link` (`query.rs:607-669`) and are
untouched.

## Generic Parameters & Bounds

**None.** All new functions are concrete over `Span<'a> = LocatedSpan<&'a str>`, matching
the file's existing style. No trait bounds are introduced.

## Sync vs Async Decisions

| Function | Async? | Rationale |
|---|---|---|
| all new productions | No | Pure CPU-bound text parsing, no I/O |
| `parse_query` | No | Unchanged; already sync, called from both sync and async contexts |

No `AsyncStore` involvement — parsing never opens a store.

## Function Signatures

All new functions are private to `parse.rs` and follow the file's existing plain-`fn`
combinator style (`fn(Span) -> IResult<Span, T>`), so they compose with `alt`, `many0` and
`terminated` without closures or generics.

### New productions

```rust
/// `link-parameter = "~X~", link-query, "~E"`
///
/// Position is taken at the `~X~` marker, matching how `parameter` takes its
/// position at the start of the parameter text.
fn link_parameter(text: Span) -> IResult<Span, ActionParameter>;

/// The query grammar accepted between `~X~` and `~E`.
///
/// Equivalent to `query_parser` minus the two `eof`-gated alternatives, which can
/// never match before a `~E`. Rejects the resource/transform shorthand.
fn link_query(text: Span) -> IResult<Span, Query>;

/// A single action parameter: a link, or a string parameter.
///
/// `parameter` matches the empty string and therefore never fails, so
/// `link_parameter` must be tried first.
fn action_parameter(text: Span) -> IResult<Span, ActionParameter>;
```

### Modified

```rust
// CHANGED: dispatches to action_parameter instead of parameter
fn minus_parameter(text: Span) -> IResult<Span, ActionParameter>;

// CHANGED: link-marker guard, and maps nom errors to a real Position and a
// specific message instead of Position::unknown()
pub fn parse_query(query: &str) -> Result<Query, Error>;
```

`parameter`, `action_request`, `query_parser`, `general_query`,
`resource_transform_query` and every other existing production are **unmodified**.

### Error helpers

```rust
/// Position of the input span a nom error stopped at.
fn nom_error_position(err: &nom::Err<nom::error::Error<Span>>) -> Position;

/// Human-readable cause for a failed query parse.
fn describe_query_failure(err: &nom::Err<nom::error::Error<Span>>) -> String;

/// Diagnose text the parser stopped before consuming.
///
/// A stray `~E` is not a parse failure, so it never reaches `describe_query_failure`.
fn describe_leftover(rest: &str) -> &'static str;
```

## Control Flow

```rust
fn action_parameter(text: Span) -> IResult<Span, ActionParameter> {
    alt((link_parameter, parameter)).parse(text)
}

fn link_parameter(text: Span) -> IResult<Span, ActionParameter> {
    let position: Position = text.into();
    let (text, _) = tag("~X~")(text)?;                  // plain Error -> alt falls through
    // Defensive only: link_query cannot return Err::Error today (D4a).
    let (text, query) = cut(link_query).parse(text)?;
    // Marked with ErrorKind::Fail so the message can name the missing terminator.
    let (text, _) = tag("~E")(text).map_err(|_: nom::Err<nom::error::Error<Span>>| {
        nom::Err::Failure(nom::error::Error::new(text, ErrorKind::Fail))
    })?;
    Ok((text, ActionParameter::Link(query, position)))
}

fn link_query(text: Span) -> IResult<Span, Query> {
    // Reject, do not reinterpret, the resource/transform shorthand.
    //
    // The nesting of peeks is deliberate, do not "simplify" it:
    //   - the inner peek(tag("~E")) asserts the shorthand consumed the whole body
    //     without consuming the terminator, which link_parameter still needs;
    //   - the outer peek is defensive rather than necessary: the result is
    //     discarded by `.is_ok()` and `text` is a Copy local that is never
    //     rebound, so the error position is the start of the body either way.
    //     Kept so the non-consuming intent is explicit at the call site.
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
```

### How `link_query` works, step by step

Reading a parameter that begins with `~X~`:

1. **`action_parameter`** tries `link_parameter` first. It must be first because
   `parameter` (parse.rs:295) wraps `many0`, which succeeds on empty input and therefore
   never fails — put second, `link_parameter` would never run.
2. **`tag("~X~")`** either matches or returns an ordinary `nom::Err::Error`, letting `alt`
   fall through to a normal string parameter. This is the only softly-failing step.
3. **The shorthand guard** runs `resource_transform_query` on the body under a `peek`. If
   it succeeds *and* the very next characters are `~E` — i.e. the shorthand accounts for
   the entire body — the body is rejected with `nom::Err::Failure` carrying
   `ErrorKind::Verify`. Nothing is consumed, so the error position is the first character
   of the body. See D3 for what this detects and why.
4. **Otherwise the body is parsed** by `alt((general_query, empty_query))` directly on the
   original span — no slicing, no copying. `general_query` halts by itself at `~E`,
   because no production accepts a bare `~`: `parameter`'s `many0(alt((parameter_text,
   entities)))` stops there, and `~E` is not in the entity table.
5. **`tag("~E")`** consumes the terminator. If the body parse stopped early (leaving junk
   before `~E`), or the input ran out, this fails — raised explicitly as `Err::Failure`,
   so it is a hard failure at the offending character rather than a backtrack. Per D4a
   this is also where a malformed *body* surfaces, which is why the message says
   "Expected ~E here" rather than "unterminated".
6. **Nesting** needs no code: an inner `~X~` inside the body is reached at a parameter
   position and handled by this same sequence, one recursion level down. The `~E` that
   closes the inner link is consumed by the inner step 5, so the outer step 5 sees the
   outer `~E`.

**The soft/hard failure boundary is the load-bearing detail.** `tag("~X~")` must fail
*softly* so `alt` can fall through to an ordinary string parameter. Everything after it
raises `nom::Err::Failure`, which short-circuits every enclosing `alt` and arrives at
`parse_query` with the offending span intact. Without that, `action-~X~hello` would
silently backtrack to an empty string parameter and report a confusing leftover error
further along. Note the boundary is enforced by the explicit `map_err` on the terminator,
not by `cut(link_query)` -- which per D4a currently has nothing to convert.

### `parse_query` (modified)

```rust
pub fn parse_query(query: &str) -> Result<Query, Error> {
    // D5 guard: bound recursion before parsing.
    if query.matches("~X~").count() > MAX_LINK_MARKERS {
        return Err(Error::query_parse_error(
            query,
            "Too many link parameters",
            &Position::unknown(),
        ));
    }

    let (remainder, path) = query_parser(Span::new(query)).map_err(|e| {
        // WAS: Position::unknown() and the Display of the nom error
        Error::query_parse_error(query, &describe_query_failure(&e), &nom_error_position(&e))
    })?;

    // CHANGED: the leftover branch keeps its position and gains a diagnosis.
    if !remainder.fragment().is_empty() {
        let position: Position = remainder.into();
        return Err(Error::query_parse_error(
            query,
            describe_leftover(remainder.fragment()),
            &position,
        ));
    }
    Ok(path)
}

fn nom_error_position(err: &nom::Err<nom::error::Error<Span>>) -> Position {
    match err {
        nom::Err::Error(e) | nom::Err::Failure(e) => e.input.into(),
        nom::Err::Incomplete(_) => Position::unknown(),
    }
}

fn describe_query_failure(err: &nom::Err<nom::error::Error<Span>>) -> String {
    match err {
        nom::Err::Error(e) | nom::Err::Failure(e) => match e.code {
            // Private markers set in link_query / link_parameter; see D6.
            ErrorKind::Verify => "Resource/transform shorthand is not allowed inside \
                 ~X~...~E; use the explicit form, for example -R/a/b/-/c"
                .to_owned(),
            // True whether the terminator is missing or displaced; see D4a.
            ErrorKind::Fail => "Expected ~E here to close ~X~".to_owned(),
            // ErrorKind is an external enum -- a catch-all arm is permitted here.
            // Not "Can't parse query": query_parse_error already prefixes that.
            _ => "unexpected input".to_owned(),
        },
        nom::Err::Incomplete(_) => "Incomplete query".to_owned(),
    }
}

/// Diagnose text the parser stopped before consuming.
fn describe_leftover(rest: &str) -> &'static str { /* see D6 */ }
```

`nom::Err` is external, so its `Incomplete` variant is matched explicitly rather than
swept into the catch-all — the streaming combinators that would produce it are not used,
and an explicit arm documents that.

## Design Decisions

### D1 — Why `link_query` is not `query_parser`

`query_parser` (parse.rs:649) is:

```rust
alt((
    terminated(resource_transform_query, eof),
    terminated(simple_transform_query, eof),
    general_query,
    empty_query,
))
```

Inside a link there is always a trailing `~E`, so neither `eof` gate can hold and every
embedded query would fall through to `general_query` — silently changing meaning for the
shorthand form. `link_query` therefore states the embedded grammar directly.

### D2 — Why `simple_transform_query` is omitted rather than `~E`-gated

`simple_transform_query` is `opt("/") + transform_segment_without_header`. In
`general_query`, `query_segment0 → transform_qs0` tries **the same**
`transform_segment_without_header` as its first alternative, and the following
`many0(preceded(tag("/"), query_segment1))` can only add segments when more text follows.
So whenever `simple_transform_query` would succeed with the body fully consumed,
`general_query` produces the identical query. Verified empirically over 15 canonical forms
before this document was written; Phase 3 adds a standing equivalence test.

### D3 — The shorthand detector: what it detects and why it is needed

#### What the shorthand is, mechanically

A resource path cannot contain a component starting with `-`: `resource_name`
(parse.rs:223) requires its first character to be alphanumeric, `_` or `.`. So in
`data/report/-/to_text`, `resource_path1` takes `data` and `report`, then **stops** —
`-` cannot begin a resource name. `/-/` is therefore not *inside* the resource query; it
is the **seam** where the resource part ends and the transform part begins:

```text
resource_transform_query = opt("/"), resource_path1, "/", transform_segment_with_header
                                     └── data/report ┘ └── -/to_text ────────────────┘
```

The detector asks exactly one question: *does the link body parse as
`<resource path>/<transform segment>`, covering everything up to the `~E`?* If yes, the
body is written in the shorthand and is rejected.

#### Why a detector is needed: the misinterpretation, worked

Yes — the problem is precisely that the same text would denote a different query inside a
link than at top level, and both parses succeed, so nothing would flag it.

Take the body `data/report/-/to_text`.

**At top level,** `query_parser` tries `terminated(resource_transform_query, eof)` first
and it succeeds:

```text
Resource[data/report] + Transform[to_text]      encodes: -R/data/report/-/to_text
```

Meaning: *read the stored resource `data/report`, then apply the `to_text` command.*

**Inside a link,** the trailing `~E` means `eof` cannot hold, so that alternative is
skipped and `general_query` parses the same text instead. `action_requests` consumes
`data/` (a `/` not followed by `-`), then stops before `report` because the *next* `/` is
followed by `-`; `filename_or_action` takes `report` as an action; `/-/to_text` becomes a
second transform segment:

```text
Transform[data, report] + Transform[to_text]    encodes: data/report/-/to_text
```

Meaning: *run command `data`, then command `report`, then command `to_text`.*

A store read has silently become three command invocations. Whether that surfaces as an
error depends on luck: if no commands named `data` and `report` are registered, plan
building fails with a confusing "Action 'data' not registered"; if they happen to exist,
the query runs and returns the wrong thing. Rejecting at parse time replaces both outcomes
with one clear message.

#### Verified behavior

Measured with the exact detector expression against `parse_query` (temporary test, output
reproduced verbatim):

| body | detector fires | top-level meaning | `general_query` fallback |
|---|---|---|---|
| `data/report/-/to_text` | **true** | `-R/data/report/-/to_text` | `data/report/-/to_text` ← **differs** |
| `-R/data/report/-/to_text` | false | `-R/data/report/-/to_text` | `-R/data/report/-/to_text` ✅ |
| `abc/def/-/xxx/-q/qqq` | false | `abc/def/-/xxx/-q/qqq` | `abc/def/-/xxx/-q/qqq` ✅ |
| `hello` | false | `hello` | `hello` ✅ |
| `` (empty) | false | `` | `` ✅ |

The detector fires on exactly the one row where the two readings differ. That is the
correctness property: **fires ⟺ the body would mean something different inside a link.**
The `-R/` row shows why the explicit form is always safe — `-R` cannot start a resource
name, so `resource_transform_query` never matches it and the detector never fires.

#### It is a guard clause, not an `alt` arm

Phase 1's original sketch was wrong on this point: written as an `alt` arm,
`terminated(resource_transform_query, peek(tag("~E")))` would have *accepted* the
shorthand and given it the resource+transform reading — the opposite of the decision.
Phase 1 has been corrected to match.

### D4a — `link_query` cannot fail, and that shapes every error path

**This is the least obvious property of the design and the one most likely to mislead an
implementer.** `link_query` ends in `alt((general_query, empty_query))`, and `empty_query`
is `opt(tag("/"))` followed by an unconditional `Ok` — it cannot fail. So `link_query`
returns an error *only* through the shorthand guard, which raises `Err::Failure`. It never
returns `Err::Error`.

Three consequences:

1. **`cut(link_query)` is currently a no-op.** It has nothing to convert. Keep it — it is
   defensive and documents intent if `link_query` ever gains a failing path — but the
   commit boundary that actually matters is established by the `~E` match, not by this
   `cut`.
2. **A malformed body never reports its own error.** There is no "invalid embedded query"
   failure mode. Whatever the body parser could not consume is simply left, and the error
   surfaces at the terminator match, positioned wherever the body parse stopped.
3. **Therefore the terminator message must be true of both cases.** `action-~X~hello` has
   no `~E` at all; `action-~X~a b~E` has one, but the body stopped at the space. Both
   arrive at the same match. "Unterminated link" would be false for the second — it
   visibly ends in `~E` — so the message is **`Expected ~E here to close ~X~`**, which is
   accurate whether the terminator is missing or merely displaced.

An implementer who does not know this will go looking for where to raise a body-specific
error, find no such branch, and either manufacture one with a spurious `verify`/`cut`
(breaking the `ErrorKind::Verify` uniqueness D6 depends on) or conclude the design is
wrong.

### D4 — Nesting and escaping need no code

Nesting (`~X~a-~X~b~E~E`) falls out of the recursion: the inner `~X~` is reached at a
parameter position and handled by the same `link_parameter`. The `~~` escape needs no
scanner-side duplication because `entities` already consumes it, and `~E` is not in the
entity table, so `parameter`'s `many0` halts there naturally.

### D5 — Recursion depth guard (new risk introduced by this feature)

**Scope note: this is a robustness measure, not a limit that real queries will meet.**
A Liquers query is typically a one-liner, and links nested more than two or three deep are
already unusual. Deep recursion is reachable only from a malformed or deliberately hostile
query — which is exactly the case a parser exposed to HTTP input has to survive, so the
guard is worth having, but it should not be read as a constraint on ordinary use.

**The risk was not identified in Phase 1** — it surfaced only when the recursion was
written out concretely. Phase 1 has been annotated to point here.

Links are the **first recursive construct in the query grammar** — before this change,
query parsing had no self-reference and a bounded stack. `parse_query` is reachable from
untrusted input via liquers-axum, so `a-` followed by thousands of `~X~` would recurse
until the stack overflows and the process aborts. A wasm target (1 MB default stack)
overflows far sooner than native.

**Guard:** an O(n) pre-check in the public entry points that reach `query_parser`:

```rust
/// Maximum number of `~X~` link markers accepted in one query.
const MAX_LINK_MARKERS: usize = 64;
```

`parse_query` and `parse_simple_template` reject input containing more than
`MAX_LINK_MARKERS` occurrences of `~X~` with `ErrorType::ParseError`, before parsing.
`parse_key` needs no guard — it calls `resource_path`, which cannot recurse.

**Trade-off, stated plainly:** nesting depth is bounded by the *count* of `~X~` markers,
so the guard also caps non-nested sibling links at 64 per query. That over-rejection is
acceptable (64 links in one query is already pathological) and buys a check with no state,
no `unsafe`, and no type changes. The alternatives were a thread-local depth counter
(correct but stateful) and threading depth through `LocatedSpan`'s `extra` parameter
(clean, but requires the `unsafe fn new_from_raw_offset` to re-stamp mid-parse). If
sibling-heavy generated queries ever appear, switch to true depth tracking.

Phase 3 must include **two** tests, not one: that input exceeding the bound returns
`ParseError` rather than recursing (C7/C8), *and* that input at exactly the permitted
depth — 64 **nested**, not 64 sibling — parses successfully (C8b). Without the second, the
constant is asserted but never validated: the guard's entire purpose is that the depth it
permits is survivable, and 64 nested links is ~1200 nom frames, which is comfortable
natively but not obviously safe on the 1 MiB wasm stack this decision names as its
motivating case. **If C8b overflows, lower `MAX_LINK_MARKERS` — the constant is a
hypothesis until that test passes.**

**Message size.** `Error::query_parse_error` embeds the full query text in the message
(error.rs:232). This guard is the one path deliberately reached by hostile input, so the
largest inputs produce the largest echoes into logs and HTTP responses. Pre-existing for
every parse error and not fixed here, but worth truncating the echo if this path is ever
exposed without a request-size limit in front of it.

### D6 — Error messages without a custom nom error type

The parser is typed on nom's default `nom::error::Error<Span>`, which carries a span and
an `ErrorKind` but no message. Carrying a `String` would mean a custom `ParseError`
implementation and changing the error type on ~45 function signatures — mechanical, but
well beyond this issue's scope.

Instead, link failures are marked with `ErrorKind` values that **no other production in
this file produces** — verified by searching `parse.rs` for `verify`, `fail` and
`ErrorKind`, which returns no hits. `describe_query_failure` maps them back to messages.

Every link-related failure gets a specific diagnosis. There are two mechanisms, and which
one applies depends on whether the parser *failed* or merely *stopped*:

| Condition | Mechanism | Message |
|---|---|---|
| shorthand inside a link body | `ErrorKind::Verify` marker | `Resource/transform shorthand is not allowed inside ~X~...~E; use the explicit form, for example -R/a/b/-/c` |
| `~X~` with no `~E`, or a body that stopped early | `ErrorKind::Fail` marker | `Expected ~E here to close ~X~` |
| **stray `~E` with no `~X~`** | leftover-text inspection | `Unpaired ~E: link terminator without a matching ~X~` |
| `~X~` where a link cannot appear | leftover-text inspection | `~X~...~E is only valid as a complete action parameter` |
| anything else | — | `unexpected input` + position |

**Why two mechanisms.** A stray `~E` is not a parse *failure* at all: `parameter`'s
`many0` simply halts there, the action completes successfully, and the `~E` is left over.
It surfaces in `parse_query`'s existing complete-consumption branch, which already computes
the right position — measured on the current tree, `action-a~E` reports offset 8, exactly
the `~`. What that branch lacks is a diagnosis, so it gains a classifier over the leftover
text. The two marker cases, by contrast, are genuine `Err::Failure`s raised inside
`link_parameter`, where the parser knows precisely what went wrong.

The leftover classifier distinguishes its two cases by what the remaining text starts
with, which is sufficient and needs no parser state:

```rust
fn describe_leftover(rest: &str) -> &'static str {
    if rest.starts_with("~E") {
        "Unpaired ~E: link terminator without a matching ~X~"
    } else if rest.starts_with("~X~") {
        // Reached for `action-abc~X~q~E` (concatenated with text) and for
        // `-R/data~X~q~E` (resource paths cannot contain links). One message
        // covers both: the link is in a position where no link may appear.
        "~X~...~E is only valid as a complete action parameter"
    } else {
        "Can't parse query completely"
    }
}
```

`ErrorKind` is an external enum, so `describe_query_failure` may use a `_ =>` arm; the
Liquers no-default-arm rule covers Liquers-owned enums. Phase 3 pins each message with a
test, so a future `verify()` or `fail()` added elsewhere in the file is caught.

**An honest note on where this is heading.** The marker approach now carries two markers
and a text classifier for three diagnostics. It is still the right call for this change —
a custom nom error type would touch ~45 signatures for a bug fix — but each new message
makes that deferred work more attractive, and a fourth would be the point to stop
extending this and do it properly.

### D7 — The existing rejection test must be replaced, not deleted

`parse.rs:1322-1323` currently asserts the bug:

```rust
// Link encoding exists in query.rs, but parse.rs has no link production.
assert!(parse_query("action-~X~hello~E").is_err());
```

That assertion inverts under this design. It is the issue's own stated verification
("The current rejection test … should be replaced by successful parsing and round-trip
assertions"), so Phase 3 owns the replacement: successful parse, round-trip through
`encode`, and the new negative cases (shorthand inside a link, unterminated `~X~`,
marker-count guard). Deleting it without replacement would silently drop coverage of the
`documented_query_language_contract` test's link clause.

## Positions

| Node | Position |
|---|---|
| `ActionParameter::Link` | offset of `~X~` |
| embedded `Query` segments, actions, parameters | absolute offsets in the *original* query string |

Absolute inner positions are a direct consequence of in-band parsing: the body is never
copied or re-spanned, so `LocatedSpan` offsets are already correct. (The rejected scanner
design would have needed `take(n)` to preserve them.)

## Integration Points

### Crate: liquers-core

**File:** `liquers-core/src/parse.rs` (only source file changed)

- add `link_parameter`, `link_query`, `action_parameter`, `nom_error_position`,
  `describe_query_failure`, `describe_leftover`, `MAX_LINK_MARKERS`
- modify `minus_parameter` (one line) and `parse_query` (guard + error mapping)
- add `cut` to the existing `nom::combinator` import; add `use nom::error::ErrorKind;`
- module docs: delete the "Known link-parser bug" section (l. 59-66), add
  `link-parameter` to the grammar, document the shorthand rule

**File:** `liquers-core/src/query.rs` (doc comment only)

- `ActionParameter::Link` doc (l. 536-540) currently says the encoded form cannot be
  parsed; replace with the accepted syntax and the shorthand restriction

### No other crate changes

- `plan.rs` already consumes `ActionParameter::Link` (l. 615-633) — no change
- `dependencies.rs` already models `DependencyRelation::ParameterLink` — no change
- `liquers-py` wraps `ActionParameter` opaquely (`liquers-py/src/query.rs:59`, `147-148`)
  — no change
- `liquers-lib/src/utils.rs:109` reads `Vec<ActionParameter>` — no change
- `specs/command_registry.yaml` — **not regenerated**; no command signatures change
- UI — not applicable; `QueryRenderer for ActionParameter` (`query.rs:607-644`) already
  renders `Link` as `~X~` / `~E` entities around the embedded query, and now renders
  something the parser can read back

### Dependencies

**None added.** `nom = "8.0.0"` and `nom_locate = "5.0.0"` already provide `cut`, `peek`,
`terminated` and `ErrorKind`.

## Relevant Commands

### New Commands

**None.** This is a parser fix; it registers nothing.

### Relevant Existing Namespaces

**None. This is a pure parser change and no namespace is relevant to it.**

The parser assigns no meaning to command names, so no namespace is privileged or affected.
Two facts make this concrete:

- **Links are not tied to any argument type.** `ParameterValue::from_action_parameter`
  (plan.rs:615-633) turns `ActionParameter::Link` into `ParameterValue::ParameterLink` for
  **any** `arginfo`, whatever its declared type. Every registered command in every
  namespace can already receive a link; nothing about that changes here.
- **Namespace selection (`ns-…`) is orthogonal to link parameters.** `ns` is an ordinary
  action whose parameters happen to name namespaces (`Query::ns`, `query.rs:801`), read by
  `active_namespace` (`liquers-lib/src/utils.rs:106-116`) via
  `ns_params.last().map(|p| p.encode())`. A link written there is syntactically accepted
  but semantically inert: `encode()` on a `Link` yields the literal text `~X~…~E`, which
  can never match a registered namespace. That is pre-existing behavior, unchanged by this
  feature, and not worth special-casing — the parser's job is to produce the syntax tree,
  not to police which parameters are meaningful.

**Consequence for Phase 3:** the earlier question about which namespace to use for a
worked example is withdrawn. Tests belong in `liquers-core` — parser and round-trip tests
in `parse.rs`, and plan-level tests using the commands `liquers-core`'s own test suites
already register. No `liquers-lib` namespace, and therefore no optional feature flag,
needs to be involved.

## Web Endpoints

**None.** liquers-axum passes query text to `parse_query` unchanged; link queries simply
stop being rejected. No route, handler or content-type change.

## Error Handling

No new error types (`liquers_core::error::Error` only), no `Error::new`, no
`unwrap()`/`expect()` outside tests.

All use `Error::query_parse_error` and `ErrorType::ParseError`. Every one carries a
position, and every link-related one carries a specific message:

| Scenario | Position | Message |
|---|---|---|
| shorthand inside a link | start of the link body | names the explicit `-R/` form |
| `~X~` with no `~E`, **or** a body that stopped early | where `~E` was expected | `Expected ~E here to close ~X~` |
| **stray `~E` (no `~X~`)** | at the `~E` | `Unpaired ~E: link terminator without a matching ~X~` |
| link concatenated with text (`action-abc~X~q~E`) | at the leftover `~X~` | `~X~...~E is only valid as a complete action parameter` |
| link inside a resource path (`-R/data~X~q~E`) | at the leftover `~X~` | same as above |
| more than `MAX_LINK_MARKERS` links | `Position::unknown()` (pre-parse) | `Too many link parameters` |

There is deliberately **no** "invalid embedded query" row: per D4a, a malformed body cannot
produce its own error and always surfaces at the terminator.

**Improvement to existing behavior:** `parse_query`'s nom-error path currently reports
`Position::unknown()` (parse.rs:756). It will report the real failure position via
`nom_error_position`. No test asserts the current message or position — verified by
searching for `query_parse_error` and the literal messages — so this is a safe change.

`parse_key` and `parse_simple_template` share the same weakness. **Decision: fix
`parse_query` only.** The other two are not on this issue's path, and changing three entry
points at once widens the blast radius for no gain here. Recorded as follow-up.

## Serialization Strategy

**Unchanged.** `ActionParameter` already derives `Serialize, Deserialize` (`query.rs:531`)
and `Link` already serializes through the derived impl. This feature changes only the
*textual* representation's read path.

## Concurrency Considerations

**No shared state.** All new functions are pure and take `Span` by value (a `Copy` wrapper
around `&str`). Parsing is re-entrant and safe to call from multiple threads. The depth
guard is a pure function of the input string — deliberately not a thread-local, so there
is no per-thread state and no wasm caveat.

## Documentation Deliverables

Carried from Phase 1. Documentation is a deliverable of this feature, not a follow-up:
the link syntax has never been specified anywhere as a *supported* form — only recorded as
a bug — so every target below needs a **positive specification**, not just the removal of
a limitation note.

| Target | Change |
|---|---|
| `liquers-core/src/parse.rs` module docs | delete "Known link-parser bug"; add the `link-parameter` grammar, the entity-table clarification, and the shorthand rule |
| `liquers-core/src/query.rs` | `ActionParameter::Link` doc comment |
| `specs/reference/api/DOC_02_QUERY_LANGUAGE_REFERENCE.md` | see the per-section breakdown below |
| `specs/archive/2026-08-08-issues.md` | mark `QUERY-ACTION-PARAMETER-LINK-PARSER` resolved |

### `parse.rs` module docs — required edits

1. **Delete** the `## Known link-parser bug` section (l. 59-66) in full.
2. **Add** to the `# String action parameters` grammar block:
   ```text
   action          = identifier, { "-", action-parameter }
   action-parameter = link-parameter | string-parameter
   link-parameter  = "~X~", link-query, "~E"
   ```
3. **Clarify the entity table** (l. 40-51). `~X~` and `~E` must *not* be added as rows:
   the table lists entities that decode to characters *within* a string parameter, whereas
   `~X~`/`~E` are delimiters that select a different parameter kind. Add a sentence under
   the table saying exactly that, so nobody reads `~X~` as "an escape for X".
4. **State the shorthand rule** (shared wording below) in a new subsection under
   `# String action parameters` or a sibling `# Link action parameters` section, with
   worked examples: `action-~X~hello~E`, `action-before-~X~hello~E-after`,
   `~X~-R/data/report/-/to_text~E` (valid) and `~X~data/report/-/to_text~E` (rejected).
5. **Add** the `~E`-in-resource-name limitation (below).
6. **Note** the `MAX_LINK_MARKERS` bound in the `# Positions and errors` section, since it
   is an observable property of `parse_query`.

### doc-02 (API reference) — required edits, per section

This file has its own factual-verification policy (`specs/reference/api/API_DOCS_GAP_ANALYSIS.md`),
so each claim below must be backed by a test or by source, and the `## Verification`
section updated with what was run.

| Section | Edit |
|---|---|
| `## Outcome` (l. 6-27) | no change — it already points at `parse.rs` as authoritative |
| `### Action-parameter entities` (l. 90-105) | add the clarification that `~X~`/`~E` are link delimiters, not string entities, with a pointer to the new subsection |
| `### Link action parameters` (**new**, after "Action-parameter entities") | the positive specification: grammar production, that the payload is a full query parsed by the same grammar, that the result is `ActionParameter::Link`, that links may appear at any parameter position and may nest, the shorthand restriction, and the `MAX_LINK_MARKERS` bound. Include the valid/rejected examples above. |
| `### Parse precedence` (l. 118-136) | in the "important consequences" list, mark the shorthand as discouraged in favour of the explicit `-R/` form, and note it is rejected inside `~X~…~E` |
| `### Link parameters do not parse` (l. 234-243) | **delete the section.** Replaced by the positive specification; leaving a "limitation" heading would contradict it |
| `### Programmatic construction is not validation` (l. 260-266) | append the `~E`-in-resource-name limitation — it is an instance of exactly this heading's point |
| `## Prioritized remaining improvements` (l. 267-285) | drop the `P0 | Link encoder has no matching parser production` row; the remaining P1/P2 rows are untouched |
| `## Coding-agent performance assessment` (l. 287-289) | item 2 currently reads "Inventing unsupported escapes, **nested-query syntax**, or header semantics". Nested-query syntax is now supported, so this must be reworded — otherwise the doc tells agents the feature does not exist |
| `## Verification` (l. 305+) | add a dated entry recording the tests run for these claims |

The last row matters more than its size suggests: doc-02 currently instructs coding agents
that nested-query syntax is something they invent incorrectly. Left alone, it would keep
steering them away from a syntax that now works.

**The shared wording**, to be used consistently:

> The resource/transform shorthand (`data/x.csv/-/to_text`) is **discouraged**. Prefer the
> explicit form (`-R/data/x.csv/-/to_text`) — that is what `Query::encode` emits and what
> round-trips unchanged. Inside `~X~…~E` the shorthand is **not allowed** and is a parse
> error: a link has no `eof` to disambiguate it, so accepting it would make the same text
> denote different queries depending on nesting depth. **The same ambiguity exists inside
> `$…$` template expansions, where the shorthand is currently accepted with the transform
> reading and is *not* diagnosed. Prefer the explicit form there too.**

**On that last sentence — it is not hypothetical, and the wording matters.** The `eof`
problem is not peculiar to links. `template_expand_query` (parse.rs:715-720) calls
`query_parser` with a trailing `$`, so the two `eof`-gated arms cannot fire there either.
Measured on the current tree, with no links involved at all:

```
parse_query("data/report/-/to_text")             -> -R/data/report/-/to_text     (resource read)
parse_simple_template("$data/report/-/to_text$") -> ExpandQuery(data/report/-/to_text)
                                                                                 (three commands)
```

Same text, two meanings, no diagnostic — a pre-existing hazard this feature neither causes
nor fixes. **Leaving it is a deliberate scope call** (templates are not on this issue's
path), but the documentation must not imply the problem is links-specific, or a reader
will conclude templates are safe. Recorded as a follow-up: the same detector could be
applied at the template boundary, with `peek(tag("$"))` in place of `peek(tag("~E"))`.

Every target states this rule, and the two docs that carry examples
(`parse.rs`, doc-02) show both sides of it:

```text
~X~-R/data/report/-/to_text~E     accepted
~X~data/report/-/to_text~E        ParseError: shorthand not allowed inside a link
```

**Second documented limitation** (Phase 1 open question 4, carried through). The first
draft of this scoped it to resource names; that was too narrow in two ways.

`encode_token` is the **only** escaping path in the encoder, and it is applied only to
`ActionParameter::String`. Every other token is emitted raw:

| Emitted raw by | |
|---|---|
| `ResourceName::encode` (query.rs:734) | also covers the terminal filename |
| `ActionRequest::encode` (query.rs:811) | the action name |
| `SegmentHeader::encode` | the header name |
| `HeaderParameter::encode` (query.rs:905) | header values |

So the limitation is: **any programmatically-set token not routed through `encode_token`
— resource names, action names, header names and values, filenames — containing `~X~` or
`~E` breaks the encode→parse round-trip.** And the failure is worse than "does not
round-trip": such text may re-parse as a *different valid query* rather than erroring.

None of this is reachable from parsed input — `identifier`, `resource_name` and
`header_parameter` all exclude `~` — so it belongs exactly where doc-02 already files this
class of problem, under "Programmatic construction is not validation". It mirrors the
`SimpleTemplate::encode` caveat about a literal `$` in a text element (`parse.rs:684-690`).
Not fixed here: adding escaping to those tokens is a change to `Key`/`Query` encoding with
its own compatibility question.

## Issue Requirement Coverage

The issue's six "Expected behavior" items, mapped to this architecture:

| # | Requirement | Where satisfied |
|---|---|---|
| 1 | parser recognizes `~X~<query>~E` | `link_parameter`, reached via `action_parameter` |
| 2 | `<query>` parsed using the authoritative grammar | `link_query` — see the note below |
| 3 | result is `ActionParameter::Link(query, position)` | `link_parameter` return value; position = `~X~` offset |
| 4 | encode/reparse preserves link and embedded semantics | D1 + D2: `link_query` accepts every form `Query::encode` emits, verified over 15 canonical forms |
| 5 | links work at every parameter position | `minus_parameter` dispatches to `action_parameter` for *every* parameter, so position in the list is irrelevant |
| 6 | malformed link → `ErrorType::ParseError` with a useful position | `cut` in `link_parameter` + `nom_error_position` / `describe_query_failure` |

**On requirement 2, precisely.** `link_query` is not literally `query_parser`, so the
requirement is met in substance rather than by identity: for every input that can appear
before a `~E`, `link_query` yields the same `Query` as `query_parser` would, with one
deliberate exception — the resource/transform shorthand, which is rejected instead of
being given a second meaning. The two `eof`-gated alternatives `link_query` omits cannot
match before a `~E` at all (D1), and of those, `simple_transform_query` is subsumed by
`general_query` (D2). So the embedded grammar is the authoritative grammar restricted to
what is unambiguous in a delimited context, and every restriction is a rejection, never a
silent reinterpretation.

## Compilation Validation

- [x] All signatures specified, concrete types only
- [x] No `unwrap()` / `expect()` in the design
- [x] No new error types; `Error::query_parse_error` only
- [x] No default match arms on Liquers-owned enums (`_ =>` used once, on external
      `ErrorKind`, with a documented reason)
- [x] Imports named (`nom::combinator::cut`, `nom::error::ErrorKind`)
- [x] No generics or trait bounds introduced

**Check in Phase 4:** `cargo test -p liquers-core --lib` (core alone; the feature touches
no other crate).

## References to liquers-patterns.md

- [x] Crate dependency flow respected — change is confined to `liquers-core`
- [x] No `ExtValue` involvement (core has none)
- [x] No commands registered, so `register_command!` is not involved
- [x] AsyncStore pattern not applicable (no I/O)
- [x] Error handling uses typed constructors (`Error::query_parse_error`)
- [x] Async default not applicable — parsing is CPU-bound and sync by design
