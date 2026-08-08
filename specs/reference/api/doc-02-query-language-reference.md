# DOC-02: Query Language, Keys, and Actions

Status: Complete  
Last reviewed: 2026-07-26

## Outcome

DOC-02 establishes the Rust API reference for Liquers query syntax and semantics.
The reference is intentionally not a user guide:

- [`liquers_core::parse`](../../liquers-core/src/parse.rs) is the authoritative
  definition of accepted text.
- [`liquers_core::query`](../../liquers-core/src/query.rs) is the authoritative
  semantic data model.
- [`PlanBuilder`](../../liquers-core/src/plan.rs) is authoritative for
  planner-level interpretations such as resource selectors and the `ns`, `q`, and
  `v` instructions.
- Styled-query rendering is out of scope. It is presentation behavior and does not
  define syntax or semantics.
- Documents in `specs/` provide design intent and terminology only where they agree
  with current code and tests.

The main implementation is module-level rustdoc in `parse.rs` and `query.rs`.
`parse.rs` now contains the normative grammar, escape table, parse precedence,
template syntax, position convention, and examples. `query.rs` documents the
semantic model, equality and provenance rules, headers, encoding, canonicalization,
relative resolution, and planner instructions.

## Sources inspected

Primary implementation:

- [`liquers-core/src/parse.rs`](../../liquers-core/src/parse.rs)
- [`liquers-core/src/query.rs`](../../liquers-core/src/query.rs)
- [`liquers-core/src/plan.rs`](../../liquers-core/src/plan.rs)
- [`liquers-core/src/error.rs`](../../liquers-core/src/error.rs)

Supplementary specifications:

- [`specs/reference/PROJECT_OVERVIEW.md`](../PROJECT_OVERVIEW.md)
- [`specs/guides/COMMAND_REGISTRATION_GUIDE.md`](../COMMAND_REGISTRATION_GUIDE.md)
- [`specs/volatility-system/DESIGN.md`](../volatility-system/DESIGN.md)
- [`specs/volatility-system/phase4-implementation.md`](../volatility-system/phase4-implementation.md)
- [`specs/reference/IMAGE_COMMAND_LIBRARY.md`](../IMAGE_COMMAND_LIBRARY.md)
- [`specs/reference/POLARS_COMMAND_LIBRARY.md`](../POLARS_COMMAND_LIBRARY.md)

## Concept inventory

| Concept | Authoritative public API | Reference responsibility |
|---|---|---|
| Source position | `Position` | Location convention and diagnostic identity |
| Resource component | `ResourceName` | Logical name, `.`/`..`, extension |
| Resource key | `Key` | Ordered logical path and relative resolution |
| Action parameter | `ActionParameter` | Decoded string or programmatic link |
| Action request | `ActionRequest` | Command name and ordered parameters |
| Header parameter | `HeaderParameter` | Undecoded segment-header value |
| Segment header | `SegmentHeader` | Resource marker, name, parameters, reserved level |
| Resource segment | `ResourceQuerySegment` | Header plus key |
| Transform segment | `TransformQuerySegment` | Header, actions, terminal filename |
| Segment union | `QuerySegment` | Resource versus transform |
| Query | `Query` | Ordered segments, leading-slash flag, provenance |
| Query provenance | `QuerySource` | Non-semantic origin metadata |
| Conversion | `TryToQuery`, `TryFrom` | Parse strings or retain constructed values |
| Text parsing | `parse_query`, `parse_key` | Complete-input validation |
| Templates | `SimpleTemplate`, `SimpleTemplateElement` | Text, `$$`, and `$query$` |
| String encoding | `encode_token` | Action-parameter escaping |
| Canonicalization | `canonical` methods | Explicit headers and transform filename normalization |
| Planner instructions | `ns`, `q`, `v` | Namespace, query value, volatility |
| Resource selectors | First resource-header parameter | Asset/store/metadata/directory/key selection |

## Verified syntax contract

The detailed grammar is maintained in the `liquers_core::parse` module rustdoc. The
following summary records the boundaries most likely to cause incorrect generated
code.

### Lexical forms

- An action identifier begins with an ASCII letter or `_`, followed by ASCII
  alphanumeric characters or `_`.
- A resource name begins with one or more ASCII alphanumeric, `_`, or `.`
  characters. Later characters may also contain `-`.
- A transform filename has a possibly empty ASCII alphanumeric/underscore stem,
  one `.`, and a non-empty suffix containing ASCII alphanumeric, `_`, `.`, or `-`.
- Header values contain zero or more ASCII alphanumeric, `_`, or `.` characters.
  Empty header values are therefore accepted.
- Empty string action parameters are accepted; `action-` has one parameter whose
  decoded value is the empty string.

### Action-parameter entities

| Text | Decoded string |
|---|---|
| `~~` | `~` |
| `~_` | `-` |
| `~I` or `~/` | `/` |
| `~.` | space |
| `~` plus decimal digits | `-` plus those digits |
| `~H` | `https://` |
| `~h` | `http://` |
| `~f` | `file://` |
| `~P` | `://` |

`encode_token` emits the general escapes for tilde, space, slash, and hyphen. It
does not emit the protocol abbreviations, although the parser accepts them.

`~X~` and `~E` are **not** entities and must not be read as rows of this table. They
do not decode to a character within a string parameter; they delimit a different kind
of parameter. See the next section.

### Link action parameters

A parameter of the form `~X~<query>~E` is a **link**. The delimited text is a complete
Liquers query, parsed with the same grammar, and the parameter parses to
`ActionParameter::Link`. The linked query is evaluated and its result supplied as the
argument value. Links are not restricted to any argument type: `PlanBuilder` turns a
link into `ParameterValue::ParameterLink` for any argument of any command.

```text
action-~X~hello~E                  one link parameter
action-before-~X~hello~E-after     a link between two string parameters
action-~X~inner-~X~deep~E~E        links nest
action-~X~~E                       an empty embedded query
```

Rules:

- A link is a whole parameter and is never concatenated with parameter text.
  `action-abc~X~q~E` is a parse error.
- Links may appear at any parameter position, but only within a transform segment. A
  resource path cannot contain one.
- The `~~` escape applies inside the embedded query, so `~~E` is an escaped tilde
  followed by `E`, not a terminator. `action-~X~inner-a~~E~E` has the body
  `inner-a~~E`, whose parameter decodes to `a~E`.
- The resource/transform shorthand is rejected inside a link; see below.
- Parsing is exponential in nesting depth, so `parse_query` and
  `parse_simple_template` reject input nested deeper than 8 links, or containing more
  than 64 links in total, before parsing.

Every link error carries a source position and a specific message: the shorthand
rejection, a missing or displaced `~E`, an unpaired `~E`, and a link in a position
where none may appear.

#### The shorthand is discouraged, and rejected inside a link

The resource/transform shorthand (`data/report/-/to_text`) is **discouraged
everywhere**. Prefer the explicit form (`-R/data/report/-/to_text`): that is what
`Query::encode` emits and what round-trips unchanged. Inside `~X~...~E` the shorthand
is **not allowed** and is a parse error:

```text
action-~X~-R/data/report/-/to_text~E    accepted
action-~X~data/report/-/to_text~E       parse error
```

The shorthand is recognised only when it accounts for an entire query, which the
parser detects with `eof`. A link has no `eof` before its `~E`, so accepting the
shorthand there would make the same text denote a resource fetch at top level and
three command invocations inside a link. It is rejected rather than silently
reinterpreted.

The same ambiguity exists inside `$...$` template expansions, where the shorthand is
currently accepted with the transform reading and is **not** diagnosed. Prefer the
explicit form there too.

### Segment forms

- A resource header begins with one or more `-`, then `R`, an optional name, and
  zero or more header parameters.
- A short transform header is one or more `-` followed by `/`.
- A named transform header is one or more `-`, a name beginning with a lowercase
  ASCII letter, zero or more header parameters, and `/`.
- The number of leading hyphens minus one is stored in `SegmentHeader::level`.
  This field is reserved and currently unused by interpretation.
- Header parameters do not use action-parameter entity decoding.

### Parse precedence

`parse_query` attempts these complete query forms in order:

1. Headerless resource path followed by an explicit transform header
2. Headerless transform query with an optional terminal filename
3. General query composed of explicitly headed segments
4. Empty query

This precedence has important consequences:

- `data/input.csv/-/load` begins with a resource segment. Encoding makes its
  shorthand header explicit: `-R/data/input.csv/-/load`. The shorthand is
  discouraged in favour of the explicit form, and is rejected inside `~X~...~E`.
- `load/process` is a transform query containing two actions, not a key.
- `file.txt` is a filename-only transform query.
- A pure textual resource query requires `-R/path`; `parse_key("path")` is the
  headerless key parser.
- `/` sets `Query::absolute`; it is not a filesystem root marker.

## Verified semantic contract

### Resource and transformation flow

A `ResourceQuerySegment` references a resource by logical `Key`. The referenced
resource is typically a keyed asset and can be thought of as a file: it has a
logical path and may have data and metadata. This analogy does not make a `Key` an
operating-system path. Resource-header parameters can select different views or
operations, including asset data, binary data, metadata, directories, recipes, or
stored resources.

A `TransformQuerySegment` represents an ordered sequence of actions applied to its
input. That input is typically the resource or transformation result produced by
the preceding query segment. A transform segment with no preceding segment starts
without resource input. Its optional terminal filename describes the output
filename rather than an additional action.

The semantic reference in [`liquers_core::query`](../../liquers-core/src/query.rs)
links back to the authoritative syntax in
[`liquers_core::parse`](../../liquers-core/src/parse.rs), including the segment,
header, parameter, and parse-precedence rules.

### Identity and positions

- Positions are zero-based by byte offset and one-based by line and column.
  `Position::unknown()` contains zero in all fields.
- Positions do not participate in equality or hashing for resource names,
  parameters, action requests, or segment headers.
- `QuerySource` records provenance only. It is not encoded and is ignored by
  `Query` equality and hashing.
- `Query::absolute` is encoded as a leading slash and does participate in equality
  and hashing.

### Encoding versus canonicalization

- `Query::encode` always supplies an explicit `-R` header to a resource segment
  that lacks one.
- `encode` otherwise preserves a transform filename.
- `canonical` supplies missing segment headers.
- Transform canonicalization changes the terminal filename basename to `data` and
  preserves its extension. For example, `result.json` becomes `data.json`.
- Construction methods do not enforce parser grammar. They can produce values whose
  encoded text cannot be reparsed.

### Relative keys

`Key::to_absolute(cwd)` applies these rules:

- A leading `.` starts from `cwd`.
- A leading `..` starts from `cwd.parent()`.
- Later `.` components are discarded.
- Later `..` components remove the preceding component when possible.
- Resolution does not move above an empty root key.

`Query::to_absolute` applies the same operation independently to every resource
segment. It neither reads nor changes `Query::absolute`.

### Headers

- The resource flag distinguishes resource and transform headers.
- A transform header name supplies the command realm only when
  `Query::last_transform_query_name` can obtain a pure, single transform segment.
  Multi-segment realm behavior must not be inferred from the design documents.
- A resource header name is currently ignored during plan building and produces a
  plan warning.
- Only the first resource-header parameter affects the generated plan. Extra
  parameters are ignored with a warning.

Resource selector mapping:

| First parameter | Planner step |
|---|---|
| absent, `data`, `value` | `GetAsset` |
| `b`, `bin`, `binary` | `GetAssetBinary` |
| `meta`, `metadata` | `GetAssetMetadata` |
| `dir`, `directory` | `GetAssetDirectory` |
| `sdir`, `store_directory` | `GetResourceDirectory` |
| `r`, `recipe` | `GetAssetRecipe` |
| `stored`, `stored_binary`, `stored_bin`, `sbin` | `GetResource` |
| `stored_meta`, `stored_metadata` | `GetResourceMetadata` |
| `cwd` | `SetCwd` |
| `key` | `UseKeyValue` |

An unrecognized first parameter produces `ErrorType::NotSupported`.

### Planner instructions

- `ns` is a namespace selector. Planner command lookup uses the last `ns` action
  in the final transform segment together with configured default namespaces.
  Namespace parameters must be strings.
- A terminal `q` removes itself and makes the preceding query a query-valued plan
  step. It rejects parameters.
- `v` marks the plan volatile, creates no action step, and is intercepted wherever
  it is processed. The current planner does not reject parameters on `v`.

## Important implementation limitations

### Realm scope is narrower than the design language suggests

The project overview describes named transform headers as realms. The current
planner obtains a realm through `Query::last_transform_query_name`, which delegates
to `transform_query` and therefore succeeds only for a query containing exactly one
transform segment. The reference documents this implemented boundary and does not
generalize it to multi-segment queries.

### Absolute is a stored syntax flag

No execution use of `Query::absolute` was found in the inspected code. Relative
resolution uses `.` and `..` resource names and a supplied `cwd`; it does not use
the flag. The reference therefore describes what the flag stores, encodes, compares,
and hashes, without assigning it an unverified execution meaning.

### Programmatic construction is not validation

Public constructors such as `ResourceName::new`, `ActionRequest::new`, and
`Key::join` do not validate textual grammar. Coding agents should use `parse_key`
and `parse_query` for validation and should not assume that every programmatically
constructed value round-trips.

`encode_token` is the only escaping path in the encoder, and it is applied only to
string action parameters. Resource names, action names, header names and values, and
filenames are emitted raw. A programmatically-set token containing `~X~` or `~E`
therefore breaks the encode/parse round-trip, and may re-parse as a *different valid
query* rather than failing. None of this is reachable from parsed input, since those
productions all exclude `~`.

## Prioritized remaining improvements

The P0 reference gap is closed, but implementation/API issues constrain what the
documentation can promise.

| Priority | Gap | Human impact | Coding-agent impact | Recommended action |
|---:|---|---|---|---|
| P1 | Realm behavior is limited and partly documented as future work | Medium | High | Define intended multi-segment semantics, implement them, then update reference |
| P1 | Resource selector error text does not list valid selectors | Medium | High | Return a precise diagnostic with accepted values |
| P1 | Public constructors allow non-round-trippable values | Medium | High | Add validated constructors or clearly named unchecked constructors |
| P1 | `ActionParameter::set_value` pre-encodes its stored value, which `encode` then escapes again | Medium | High | Clarify/fix the method contract and add a round-trip test |
| P2 | `v` parameters are silently ignored | Medium | Medium | Decide whether to reject them like `q`, then test and document |
| P2 | `absolute` has no verified runtime interpretation | Medium | Medium | Define intended semantics or rename/document it strictly as syntax metadata |
| P2 | Some query helpers return owned clones where borrowing could be clearer | Low | Medium | Consider borrowed accessors in a later API review |

The `ActionParameter::set_value` behavior is recorded as a gap rather than described
as intended semantics: it stores `encode_token(value)`, while `encode()` applies
`encode_token` again.

## Coding-agent performance assessment

This work should materially reduce three frequent classes of generated-code error:

1. Treating ordinary slash-separated text as a resource key inside `parse_query`
2. Inventing unsupported escapes or header semantics, or assuming nested-query
   syntax is unavailable (`~X~<query>~E` is supported and documented above)
3. Confusing textual encoding, canonical identity, relative resolution, and
   provenance

The reference is structured for retrieval: exact grammar is next to parser entry
points; semantic invariants are next to the types; planner-only meanings are
explicitly labeled. A coding agent no longer needs to reconcile styled rendering or
design proposals to determine accepted syntax.

For human developers, the module references provide exact lookup tables and type
contracts without requiring a tutorial flow. A separate user guide can later derive
task-oriented explanations from this verified base.

## Verification

Completed on 2026-08-06 (link action parameters, `QUERY-ACTION-PARAMETER-LINK-PARSER`):

- Added 34 parser tests in `liquers-core/src/parse.rs` (`link_tests`) covering
  positive parsing, encode/parse round-trip, grammar equivalence with the top-level
  reading over the 15-entry canonical corpus, and every error path.
- Added 4 plan-level tests in `liquers-core/src/plan.rs` and 3 end-to-end tests in
  `liquers-core/tests/action_parameter_link.rs`.
- Ran `cargo test -p liquers-core --lib`: 425 passed (was 387).
- Ran `cargo test -p liquers-core --tests`: all integration binaries passed.
- Ran `cargo test -p liquers-core --doc`: 5 passed, 2 intentionally ignored. The link
  examples in this reference are compiled from the `parse.rs` and
  `ActionParameter::Link` rustdoc.
- Ran `cargo doc -p liquers-core --no-deps`: no new warnings.
- The exponential-depth claim is measured, not asserted: debug-build parse times were
  32 ms at depth 10, 0.54 s at 14, 2.3 s at 16 and 4.3 s at 17, doubling per level.

Completed on 2026-07-26:

- Added and passed the focused `documented_query_language_contract` parser test.
- Ran `cargo test -p liquers-core --lib`: 327 passed.
- Passed the `liquers-core` rustdoc example containing key, query, shorthand, and
  template parsing.
- Ran `cargo test -p liquers-core --doc`: 2 passed, 2 intentionally ignored.
- Ran `cargo doc -p liquers-core --no-deps`: documentation generated successfully.
- Rustdoc emitted no broken-link or documentation warnings. Existing compiler
  warnings outside DOC-02 remain.
