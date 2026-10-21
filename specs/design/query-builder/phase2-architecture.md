# Phase 2: Solution & Architecture — query-builder

## Overview

Four parts, all in `liquers-core` except the two consumers:

1. **Token predicates**, one per raw-emitted class, in `liquers_core::escape`. `parse.rs` uses them,
   which removes the `c as u8` truncation (`QUERY-TOKEN-CLASSES-TRUNCATE-NON-ASCII`).
2. **`Query::check_encodable`**, the verifier: it walks a query and checks every raw-emitted token
   and the segment structure.
3. **`QueryBuilder`**, a by-value builder whose steps check as they go and which sets headers itself.
4. **`liquers-encode`**, a `cli` binary over the builder.

Rejected: a builder in `liquers-lib` (bindings and `liquers-template` depend on core only); a
text-template builder (`liquers-template` already exists for that, and is AST-based); returning
`Result` only from `build()` (the error would lose which step failed); a typed-state builder
(resource-then-transform enforced by types). That last one is too heavy for five methods, and the
runtime check is simple.

## Known-Issue Preflight

| Issue | Status | Priority | Impact on this design | Fix first? | Blocking? | Action |
|---|---|---|---|---|---|---|
| `QUERY-TOKEN-CLASSES-TRUNCATE-NON-ASCII` | draft | P2 | Parser and builder must share predicates | preferably | no | Step 1 does it if not yet fixed |
| `RESOURCE-NAME-ASCII-ONLY` (`resource-name-unicode`) | draft | P2 | Changes the key-segment predicate | no | no | Builder calls the predicate, so it follows |
| `PARAMETER-ESCAPING-INCOMPLETE` | closed | P0 | Was this issue's blocker | done | no | None |
| `QUERY-AST-DISCARDS-ENTITIES` (`styled-query-entity-tokens`) | draft | P3 | None (rendering only) | no | no | None |

## Interfaces

`liquers-core/src/escape.rs` — predicates (pure, sync):

```rust
pub fn is_identifier(text: &str) -> bool;       // [A-Za-z_][A-Za-z0-9_]*  (action names, ns)
pub fn is_resource_name(text: &str) -> bool;    // is_resource_name_char first, then also '-'
pub fn is_filename(text: &str) -> bool;         // [A-Za-z0-9_]* '.' [A-Za-z0-9_.-]+
pub fn is_header_value(text: &str) -> bool;     // [A-Za-z0-9_.]*  (selector / header parameter)
pub fn is_identifier_char(c: char) -> bool;     // and the per-char forms parse.rs needs
```

`liquers-core/src/query.rs`:

```rust
impl Query {
    /// Ok when `parse_query(&self.encode())` reproduces `self` (positions ignored).
    pub fn check_encodable(&self) -> Result<(), Error>;
}
```

Checks: each action name and `ns` argument `is_identifier`; each key segment `is_resource_name`
(`.`/`..` allowed here, since a parsed key may carry them); each filename `is_filename`; each
header name/parameter `is_header_value`; every link's query recursively; a transform segment that
follows another segment has a header. It also checks a headerless first transform segment whose text
would be read as the resource shorthand. That check is done by re-parsing its encoding, because the
shorthand rule is a parse-precedence rule (DOC-02 §Parse precedence) and is not restated here.

`liquers-core/src/query_builder.rs` (new, `pub mod query_builder` in `lib.rs`):

```rust
#[derive(Debug, Clone, Default)]
pub struct QueryBuilder { segments: Vec<QuerySegment>, rooted: bool }
impl QueryBuilder {
    pub fn new() -> Self;
    pub fn rooted(self) -> Self;
    pub fn resource<I, S>(self, segments: I) -> Result<Self, Error>
        where I: IntoIterator<Item = S>, S: AsRef<str>;     // refuses "." and ".."
    pub fn resource_key(self, key: &Key) -> Result<Self, Error>;
    pub fn selector(self, name: &str) -> Result<Self, Error>; // last resource segment's header param
    pub fn action<I, S>(self, name: &str, parameters: I) -> Result<Self, Error>
        where I: IntoIterator<Item = S>, S: Into<String>;   // values raw, any string
    pub fn action_request(self, action: ActionRequest) -> Result<Self, Error>; // links checked
    pub fn ns(self, namespace: &str) -> Result<Self, Error>;
    pub fn filename(self, name: &str) -> Result<Self, Error>; // ends the transform segment
    pub fn build(self) -> Query;
}
```

Structure rules: `resource` after a transform starts a new resource segment. The first `action`
after a resource opens a transform segment with `SegmentHeader::new()` (encodes `-/`). Later
actions append to it. `action` after `filename` is an error. `build` never fails: every step
already checked. `build()` on an empty builder is the empty query.

`liquers-core/src/bin/liquers_encode.rs` (new `[[bin]]`, `required-features = ["cli"]`):
clap options `--rooted`, `--resource SEG` (append), `--selector NAME`, `--action NAME` (append),
`--arg VALUE` (append, `allow_hyphen_values`), `--filename NAME`. Order is reconstructed from
`ArgMatches::indices_of`. An `--arg` attaches to the nearest preceding `--action`. An `--arg`
before any action is a usage error (exit 2). The tool builds, re-parses its own output and compares
(`parse_query(&text)? == query`), then prints the text plus a newline to stdout.

## Integration Points

- `liquers-core/src/escape.rs`: predicates. `liquers-core/src/parse.rs`: `identifier`, `filename`,
  `header_parameter`, `full_transform_segment_header`, `resource_segment_header` use the per-char
  forms.
- `liquers-core/src/query.rs`: `check_encodable`. `liquers-core/src/lib.rs`: `pub mod query_builder`.
- `liquers-core/Cargo.toml`: second `[[bin]]`.
- `liquers-template/src/query/instantiate.rs`: `is_identifier`, `is_key_segment`, `is_filename`
  call the core predicates (`is_key_segment` keeps its own `.`/`..` refusal).
- `liquers-web/src/encode.rs` `encode_param`: drop the empty-string refusal; replace the test
  `web_encode_param_refuses_the_empty_string` with a round-trip of `""`; update the `lib.rs` doc
  comment on `encodeParam`.

## Error Handling

All builder and `check_encodable` failures are `Error::from_error(ErrorType::ParameterError,
format!("{value:?} is not valid as {kind}: {rule}"))`, where `kind` is one of: an action name,
a namespace, a key segment, a resource selector, a filename. The rule texts match
`liquers-template`'s `Place::describe`. Structure errors (an `action` after `filename`, a
`selector` with no resource) are `ParameterError` too. The CLI prints `error: {message}` to stderr
and exits 1; clap usage errors exit 2.

## Relevant Commands

None registered; no `register_command!` change and no registry regeneration. The CLI is a binary,
not a command.

## Documentation Architecture

| Document | Change |
|---|---|
| `specs/guides/QUERY_ESCAPING_GUIDE.md` | §Escaping a value programmatically leads with `QueryBuilder`; new §From the shell: `liquers-encode` |
| `specs/reference/api/DOC_02_QUERY_LANGUAGE_REFERENCE.md` | §Programmatic construction: `check_encodable` and the builder validate; the P1 gap row "Public constructors allow non-round-trippable values" is struck with a pointer |
| `specs/reference/PROJECT_OVERVIEW.md` | Technical Debt: the raw-emission caveat is narrowed to code bypassing both |
| `specs/reference/TEMPLATE_LANGUAGE.md` | P1-P5 validity rules are the `liquers_core::escape` predicates |
| `CLAUDE.md`, `.claude/skills/liquers-validate/SKILL.md` | one line each on `liquers-encode` |

## Risks

| Assessment | Finding |
|---|---|
| Files likely to change | those under Integration Points; ~8 files in three crates |
| Crates and workflows affected | query construction; parsing only through the predicate swap |
| Existing tests likely to change | `parse.rs` `a15_non_ascii_is_not_part_of_the_grammar` comment; `liquers-web` empty-string test (replaced); any test that parses `Ł…` identifiers (none found) |
| New validation | builder round-trip property test over a corpus of awkward values; CLI integration test; `liquers-template` tests unchanged |
| Compatibility | additive in core; the parser narrows to the documented ASCII classes; `encodeParam("")` stops throwing |
| Performance | `parse.rs` predicates are `#[inline]` char tests replacing the same tests; no measurable change on the hot path |
| Security | the builder refuses `.`/`..` segments from values, so data cannot climb the key tree |
| Recovery | each step reverts independently; the CLI and builder are additive |
| Certainty and open questions | high; open: the CLI shape (Phase 1 leading issue) and the empty-parameter change in `liquers-web` |
