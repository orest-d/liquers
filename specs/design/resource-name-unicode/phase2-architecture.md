# Phase 2: Solution & Architecture — resource-name-unicode

## Overview

Resource names get the same two-layer model string parameters already have: the AST holds the
decoded value, and escaping happens only in `encode`. The parser accepts literal Unicode
alphanumerics plus any entity, and refuses a decoded `/` or control character. `ResourceName::encode`
escapes with a name-specific canonical spelling that leaves every current ASCII name untouched.
Stores stop using the encoded text as a physical name and use the decoded path, and each backend
refuses a name it cannot represent.

Rejected: contract 1 (encoded text as physical name) and contract 3 (per-backend mapping), see Phase
1. Rejected: reusing `encode_token` for names. It would emit `~4` for `-4` and `~/` style mnemonics,
changing `report-2024.csv` (AC-3). Rejected: validating only at the store boundary. The parser could
then build keys whose names contain `/`, and errors would lose their query position.

## Known-Issue Preflight

| Issue | Status | Priority | Impact on this design | Fix first? | Blocking? | Action |
|---|---|---|---|---|---|---|
| `STORE-FILESTORE-PATH-TRAVERSAL` | closed | P0 | Prerequisite named by the issue | done | no | None |
| `STORE-ABSOLUTE-KEY-NOT-TYPE-ENFORCED` (`store-backend-absolute-key`) | draft | P3 | Same mapping sites | preferably | no | Land it first; else add the name check to `as_absolute` callers' path builders |
| `QUERY-BUILDER-TOOLING` (`query-builder`) | accepted | P2 | Shares the key-segment predicate | no | no | Use `is_valid_resource_name` |
| `QUERY-TOKEN-CLASSES-TRUNCATE-NON-ASCII` | draft | P2 | Neighbouring productions in `parse.rs` | no | no | None |

## Interfaces

`liquers-core/src/escape.rs`:

```rust
/// A decoded name a key may hold: non-empty, no '/', no C0 control or DEL.
pub fn is_valid_resource_name(name: &str) -> bool;
/// Canonical text of one resource name (Phase 1 Q2); borrowed when nothing needs escaping.
pub fn encode_resource_name(name: &str) -> Cow<'_, str>;
/// Literal characters the parser accepts in a name besides entities.
pub fn is_literal_resource_name_char(c: char) -> bool;   // c.is_alphanumeric() || '_' || '.'
```

`is_resource_name_char` (ASCII) stays as the encoder's literal class.

`liquers-core/src/query.rs`:

```rust
impl ResourceName {
    pub fn try_new(name: String) -> Result<Self, Error>;  // is_valid_resource_name, ParameterError
    pub fn encode(&self) -> Cow<'_, str>;                  // was &str
}
impl Key {
    pub fn try_join<S: AsRef<str>>(&self, name: S) -> Result<Self, Error>;
    /// Decoded names joined with '/': the physical name for a store. Not query text.
    pub fn path(&self) -> String;
}
```

`liquers-core/src/parse.rs` `resource_name`: the first piece is `alt((literal1, resource_entity))`,
then `many0(alt((literal_or_dash, resource_entity)))`. `resource_entity` wraps
`escape::match_entity`, like `entities` for parameters. After decoding, a fragment containing `/`
or a control character is a committed failure at the `~` span. `describe_query_failure` names it
("~U2F~ decodes to '/', which separates key segments and cannot be part of a name"). The plain ASCII
case stays one `take_while1`.

`ResourceName::styled_tokens` / `render`: split the encoded name into literal and entity tokens,
reusing `styled-query-entity-tokens`' helper if present.

## Integration Points

- `liquers-core/src/store.rs`: `AsyncFileStore::key_to_path`, `key_to_path_metadata`,
  `key_to_lock_path` build the path by pushing each decoded name (not `key.to_string()`) after
  checking that `Path::new(name).components()` is exactly one `Component::Normal` equal to `name`;
  otherwise `Error::key_not_supported`. `listdir` already returns decoded names, so it is unchanged.
  `AsyncMemoryStore` is keyed by `Key` and needs nothing.
- `liquers-core/src/store.rs` metadata: `set_filename(name.encode())` (line ~120) → the decoded name.
- `liquers-store/src/opendal_store.rs`: `key_to_path`, `key_to_path_dir`, `key_to_path_metadata`
  use `key.path()`; the same component check for the `fs` service only.
- `liquers-web/src/store/local_storage.rs` (storage key), `js_store.rs` (the key passed to JS
  callbacks: decoded path, documented), `fetch.rs` `key_to_url` (percent-encode each decoded name).
- `liquers-lib/src/mem/layout.rs`: file names from keys → `path()`.
- Every other `Key::encode` / `Display` caller (interpreter, error messages, axum handlers, records
  chunk identities, templates) keeps the canonical text: it is query text. Step 4 of Phase 4 is the
  audit, file by file.
- `liquers-template/src/query/instantiate.rs` `is_key_segment` → `is_valid_resource_name` (plus its
  `.`/`..` refusal). `query-builder`'s key-segment check, if landed, uses the same predicate.
- `liquers-axum`: a key in a URL path arrives percent-decoded and is parsed with `parse_key`, which
  now accepts literal names and entities. No route change.

## Error Handling

Parse failures: `ErrorType::ParseError` at the entity's position (as parameter entities). Checked
constructors: `Error::from_error(ErrorType::ParameterError, …)`. Backend refusal:
`Error::key_not_supported(key, store_name)`, the existing refusal for keys a store cannot address
(`STORE_SEMANTICS.md` §8 vocabulary). A key that is relative and unrepresentable reports
`KeyNotAbsolute` first, as today.

## Relevant Commands

None. Commands that take a key (`-R-key/`, store commands) receive the decoded key as before.

## Documentation Architecture

| Document | Change |
|---|---|
| `DOC_02` §Lexical forms, new §Resource-name entities | grammar, canonical spelling, forbidden set, `path()` vs `encode()` |
| `PROJECT_OVERVIEW.md` | Key encoding paragraph and the Technical Debt caveat (resource names now escaped) |
| `STORE_SEMANTICS.md` §7 | physical name = decoded name; component rule; normalization note; remove the ⚠ line |
| `QUERY_ESCAPING_GUIDE.md` | resource names section; the "non-ASCII name does not parse" trap removed |
| `STORE_IMPLEMENTATION_GUIDE.md` | "use `Key::path()`, never `encode()`, for physical names" |
| `TEMPLATE_LANGUAGE.md` P4 | the key-segment rule is the decoded-name rule |

## Risks

| Assessment | Finding |
|---|---|
| Files likely to change | `escape.rs`, `parse.rs`, `query.rs`, `store.rs`, `opendal_store.rs`, three web stores, `mem/layout.rs`, `instantiate.rs`, plus call sites found by the audit |
| Crates and workflows affected | every path that turns a key into a physical name; query parsing of resource segments |
| Existing tests likely to change | `ResourceName::encode` callers that bind `&str` (type change); `query_backward_compatibility.rs` gains rows; `liquers-core/tests/query_backward_compatibility.rs` pins `-R/data/ŁŁ.csv` and `-R/data/Łódź.csv` as rejected (~line 139); both now parse as literal names, so those rows move to the accepted list |
| New validation | parser tests, encoding corpus (AC-3), conformance fixture with Unicode names, file-store component test |
| Compatibility | additive grammar; encodings of HEAD-parsable keys unchanged; `ResourceName::encode` signature change; JS store callbacks receive decoded paths (equal to today's for ASCII) |
| Performance | ASCII fast path kept in `resource_name`; `encode` borrows when no escaping |
| Security | decoded `/` refused at parse time and by constructors; component check in the file and OpenDAL fs paths; `..` unchanged |
| Recovery | revert the PR; no stored data is written under a new name for ASCII keys |
| Certainty and open questions | medium-high; the audit's breadth is the main risk. Open: the Phase 1 decisions |
