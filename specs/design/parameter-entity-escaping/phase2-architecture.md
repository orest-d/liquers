# Phase 2: Solution & Architecture - Parameter Entity Escaping

## Overview

Two new modules in `liquers-core` own the entity mechanism in both directions: `entities.rs` holds
the named-entity tables, `escape.rs` holds the character classes, the mnemonic table, the numeric
codec, the single entity matcher and `encode_token`. `parse.rs` keeps its nom combinators but stops
holding entity knowledge — its entity parser becomes a thin wrapper over
`escape::match_entity`, so encoder and decoder cannot drift. `query.rs` re-exports `encode_token`
from its new home, so no downstream import path changes. The only crate outside `liquers-core` that
must change is `liquers-web`, whose `encode_param` exists solely to work around this defect.

## Known-Issue Preflight

Searched `specs/index.csv` for locally open (`draft`/`accepted`/`in_progress`) records in `core/query`
and `core/store`, then every issue naming `encode_token`, `ActionParameter`, `parse.rs` or query
encoding. Also read `DOC_02_QUERY_LANGUAGE_REFERENCE.md`'s own risk table, which turned out to carry
a P1 defect that had never been filed.

| Issue | Status | Priority | Relevance and solution impact | First? | Blocking? | Required action | Priority action |
|---|---|---|---|---|---|---|---|
| `PARAMETER-ESCAPING-INCOMPLETE` | accepted | P0 | The defect this resolves | n/a | no | Close in Phase 5 | keep P0 |
| `ACTION-PARAMETER-SET-VALUE-DOUBLE-ENCODES` | draft | P1 | **Filed during this phase.** `set_value` stores `encode_token(v)` where every other path stores the decoded value, so `encode` escapes twice. It *worsens* here: today it corrupts only values containing `~ / -` or space; after this change it corrupts everything outside `[A-Za-z0-9_+.]`. It also falsifies this design's `parse(encode(s)) == s` promise through a public method | no | **no** — but see below | **Fix inside this design** (S, three lines + a test) | keep P1 |
| `STORE-FILESTORE-PATH-TRAVERSAL` | accepted | P0 | Would become blocking if resource names gained entities: a decoded `~U2F~` is a `/`, which is path injection into `key_to_path`. **D10 keeps resource names ASCII-alphanumeric with no entity production**, so this design does not touch that surface | no | no | Monitor; do not add entities to `resource_name` | keep P0 |
| `RESOURCE-NAME-ASCII-ONLY` | draft | P2 | Created by this design's D10. Records the deferred limitation and the option space | no | no | Leave open | keep P2 |
| `QUERY-AST-DISCARDS-ENTITIES` | draft | P2 | Out of scope, but Phase 1 requires this design not make it harder. `escape::segments` (below) is the hook | no | no | Provide the segment API; do not change `ActionParameter` | keep P2 |
| `QUERY-BUILDER-TOOLING` | accepted | P2 | Blocked in part *by* this defect — a builder cannot be correct while `encode_token` is not. This design unblocks it | no | no | Note in Phase 5 that the encoder half is now sound | keep P2 |
| `UI-QUERY-CONSOLE-NO-ERROR-HIGHLIGHT` | draft | P2 | Consumes parse error positions. This design adds entity-specific diagnostics with accurate positions, which helps rather than hinders | no | no | None | keep P2 |
| `PLAN-EXCESS-ACTION-PARAMETERS-DROPPED` | accepted | P2 | Touches `ActionParameter` but at plan level, not encoding | no | no | None | keep P2 |

### Blocking and Priority Decision

**No blockers.** The one P0 that could have blocked — `STORE-FILESTORE-PATH-TRAVERSAL` — is avoided
by architecture rather than tolerated: D10 leaves `resource_name` without an entity production, so no
decoded `/` can reach `key_to_path` through this change. If a later design takes
`RESOURCE-NAME-ASCII-ONLY` to option B or C, the traversal fix becomes a hard prerequisite; that is
recorded in both issues.

`ACTION-PARAMETER-SET-VALUE-DOUBLE-ENCODES` is not a blocker in the "cannot proceed" sense, but
shipping around it would mean publishing a round-trip guarantee that a public setter on the same
type violates, and making the corruption wider at the same time. It is three lines and a test, so
**the recommendation is to fix it here**; this is the one scope question for the approval gate.

## Data Structures

### Module `liquers-core/src/entities.rs` — named entities only

Reserved by the maintainer for named entities. It holds no encoder logic and no character classes.

#### `EntityTable` — the blob-plus-offset representation (D11)

```rust
/// A set of named entities stored as two concatenated blobs and their offset indexes.
///
/// Names are sorted lexicographically so lookup is a binary search over `name_offsets`.
/// `&[(&str, &str)]` was measured against this and rejected: it spends 32 bytes per entry
/// on fat pointers before any character data.
pub(crate) struct EntityTable {
    /// All names, concatenated, in ascending lexicographic order.
    names: &'static str,
    /// `count + 1` byte offsets into `names`; entry `i` is `names[o[i]..o[i+1]]`.
    name_offsets: &'static [u32],
    /// All decoded values, concatenated, in the same order as `names`.
    values: &'static str,
    /// `count + 1` byte offsets into `values`.
    value_offsets: &'static [u32],
}
```

**Ownership:** every field is `&'static` — the tables are compile-time constants, never built at
runtime, never cloned. No `Arc`, no allocation, no `OnceLock`.

**Serialization:** none. These are code, not data; they never cross a serialization boundary.

**Two instances, deliberately separate:**

```rust
/// Annex B. Compiled unconditionally. The encoder's repertoire, and therefore frozen.
pub(crate) static CURATED: EntityTable = /* generated */;

/// The remaining HTML5 names. Decode-only, and free to grow.
#[cfg(feature = "entities-html5")]
pub(crate) static HTML5_EXTRA: EntityTable = /* generated */;
```

Separation is what makes the compatibility surface visible in the code: `CURATED` is the one that
cannot change without a version bump, and nothing in the encoder can reach `HTML5_EXTRA` because it
does not exist in a default build.

#### The reverse index, for encoding

```rust
/// Curated entities that decode to exactly one character, sorted by code point.
///
/// `u16` is an index into `CURATED`, not an offset — the name is fetched through the
/// table so there is exactly one copy of it. Multi-character curated entities are absent
/// by construction: the encoder works one `char` at a time.
static CURATED_BY_CHAR: &[(char, u16)] = /* generated, sorted */;
```

**Rationale for a sorted slice over a `HashMap`:** ≈205 entries, static, no allocation, binary
search is ~8 comparisons, and it works in a `const` context. A hash map would need `OnceLock` and a
heap allocation for a lookup that is already fast.

### Module `liquers-core/src/escape.rs` — the general escaping algorithm

#### `Radix`

```rust
/// The radix an entity opener selects.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Radix {
    /// `~U…~`
    Hexadecimal,
    /// `~D…~`
    Decimal,
    /// `~O…~`
    Octal,
    /// `~B…~`
    Binary,
}
```

**Variant semantics:** one per opener letter. `Hexadecimal` is the only one the encoder emits.
**No default match arm** on this enum anywhere.

#### `Mnemonic` — the fixed-length legacy table

```rust
/// A fixed-length entity: exact encoded text, exact decoded text.
pub(crate) struct Mnemonic {
    pub encoded: &'static str,
    pub decoded: &'static str,
    /// Whether `encode_token` may emit this spelling. `~I` decodes `/` but is never
    /// emitted, because `~/` occupies the same slot and only one can be canonical.
    pub emitted: bool,
}

/// Ordered longest-`decoded`-first, which is what makes the encoder's step 1 a greedy
/// longest match rather than a search.
pub(crate) static MNEMONICS: &[Mnemonic] = &[ /* ~H ~h ~f ~P ~~ ~. ~/ ~I ~_ */ ];
```

`~<digit>` is not in this table: it is contextual (a `-` *followed by* a digit) and is handled
explicitly in the encoder and by `negative_number_entity` in the parser.

#### `TokenSegment` — the hook for `QUERY-AST-DISCARDS-ENTITIES`

```rust
/// One piece of a parameter token: literal text, or an entity and what it decoded to.
///
/// This is what Phase 1's scope note means by "the segment information exists inside the
/// new decoder and should be reachable". It does **not** change `ActionParameter`; it
/// makes the eventual change in `QUERY-AST-DISCARDS-ENTITIES` a matter of storing what
/// this already produces.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TokenSegment<'a> {
    /// Literal characters, accepted unescaped.
    Text(&'a str),
    /// An entity: its exact spelling in the source, and its decoded text.
    Entity {
        spelling: &'a str,
        decoded: Cow<'a, str>,
    },
}
```

**`Cow` rationale:** a named or mnemonic entity decodes to `&'static str` straight out of a table
(`Cow::Borrowed`); a numeric entity decodes to a `char` that must be materialised
(`Cow::Owned`). Returning `String` unconditionally would allocate for every `~H`.

**No default match arm.**

## Trait Implementations

None. This design adds no trait and implements none — it is functions over static tables. The
derives above (`Debug, Clone, Copy, PartialEq, Eq`) are the whole of it.

Deliberately **not** implemented:

- **No `Display` for `Radix`** — nothing formats it; the opener letter is a parser detail.
- **No `FromStr` for anything** — `decode_token` is the entry point and it returns
  `Result<String, Error>`, which `FromStr` would only rename.
- **No `Serialize`/`Deserialize`** — nothing here crosses a serialization boundary.

## Generic Parameters & Bounds

One generic, and it is the existing one:

```rust
pub fn encode_token<S: AsRef<str>>(text: S) -> String
```

Preserved exactly as it is today (`query.rs:503`) so no call site changes. Every other function takes
`&str`, because there is no second input type to abstract over and a bound that buys nothing is a
bound that constrains a future change for free.

## Sync vs Async Decisions

| Function | Async? | Rationale |
|---|---|---|
| everything in `entities.rs` | No | Binary search over a static table. No I/O exists in this design. |
| everything in `escape.rs` | No | Pure string transformation. |
| the `parse.rs` combinators | No | Already sync; parsing has never been async. |

There is no I/O anywhere in this change, so the "default to async" rule does not apply — it governs
things that touch a store, a socket or a clock, none of which appear here.

## Function Signatures

### `liquers-core/src/entities.rs`

```rust
/// Decoded text for a named entity, or `None` if the name is not in any compiled table.
///
/// Searches `CURATED` first, then `HTML5_EXTRA` when `entities-html5` is enabled. Names are
/// case-sensitive, as in HTML5: `amp` and `Amp` are different entities.
pub fn lookup(name: &str) -> Option<&'static str>;

/// The curated name for a character, or `None` if it has none.
///
/// This is the encoder's only entry point into this module, and it deliberately cannot see
/// `HTML5_EXTRA` — canonical output must not depend on a cargo feature.
pub fn curated_name(c: char) -> Option<&'static str>;

/// Whether `name` is in the curated set. Used by the diagnostic that distinguishes
/// "no such entity" from "that entity needs the `entities-html5` feature".
pub fn is_curated(name: &str) -> bool;

/// Number of names compiled into this build. Reported by the diagnostic and asserted by tests.
pub fn compiled_count() -> usize;
```

### `liquers-core/src/escape.rs`

```rust
/// Encode a string as an action-parameter token.
///
/// Infallible: `&str` guarantees every `char` is a Unicode scalar value, and every scalar
/// value has a `~U<hex>~` spelling. Output is pure ASCII (D6).
///
/// Priority at each position: longest mnemonic, then literal if accepted, then curated
/// named entity, then `~U<hex>~`.
pub fn encode_token<S: AsRef<str>>(text: S) -> String;

/// Decode a parameter token, the exact inverse of what the parser accepts.
///
/// Fallible where the encoder is not: an out-of-range code point, a surrogate, an unknown
/// name, an empty body or a missing terminator are all errors.
pub fn decode_token(text: &str) -> Result<String, Error>;

/// The segments of a token, in order. Errors on the first malformed entity.
///
/// `decode_token` is this, concatenated; both exist because the AST work
/// (`QUERY-AST-DISCARDS-ENTITIES`) needs the pieces and every current caller needs the join.
pub fn segments(text: &str) -> Result<Vec<TokenSegment<'_>>, Error>;

/// Whether a character may appear unescaped in a string action parameter.
///
/// `char::is_alphanumeric() || c == '_' || c == '+' || c == '.'` (D6). Replaces the
/// `AsChar::is_alphanum(c as u8)` truncation at `parse.rs:340`. This **widens** the
/// accepted set, which is the backward-compatible direction.
pub fn is_unescaped_parameter_char(c: char) -> bool;

/// Whether a character may appear in a resource name.
///
/// `c.is_ascii_alphanumeric() || c == '_' || c == '.'`, plus `-` after the first character
/// (D10). This **narrows**: `-R/data/ŁŁ.csv` parses at HEAD and will not after.
pub fn is_resource_name_char(c: char) -> bool;

/// Match one entity at the start of `text`.
///
/// The single decode implementation, and the only place D13's optional separator tilde is
/// handled: after a long-form opener it consumes one optional `~` before the body. That is
/// unambiguous because body characters are `[A-Za-z0-9_-]`, which excludes `~`, so a tilde
/// there can only be the separator. Short mnemonics and `~X` are matched before this point
/// and never see it.
///
/// `Ok(None)` means "no entity starts here"; `Err` means "an entity starts here and is
/// malformed", which the parser turns into a committed failure rather than a backtrack.
pub(crate) fn match_entity(text: &str) -> Result<Option<(usize, Cow<'_, str>)>, Error>;

/// Explain why the entity at the start of `text` is malformed, for `describe_query_failure`.
///
/// Returns `None` when `text` does not begin an entity, so the caller falls through to its
/// existing generic message.
pub(crate) fn explain_entity_error(text: &str) -> Option<String>;
```

**Parameter choices:** every input is `&str` and every output is owned or borrowed from the input —
nothing here needs to own its argument, and `encode_token` keeps `AsRef<str>` only for call-site
compatibility.

### `liquers-core/src/query.rs`

```rust
// Replaces the current definition at query.rs:503. The function moves; the path does not.
pub use crate::escape::encode_token;
```

### `liquers-core/src/parse.rs`

```rust
/// One entity, of any form. Replaces the `alt((tilde_entity, minus_entity, …))` list.
///
/// Recognises the opener with nom, then delegates the whole decision to
/// `escape::match_entity`, so this file no longer contains an entity table.
fn entities(text: Span) -> IResult<Span, String>;
```

The nine existing single-purpose parsers (`tilde_entity`, `minus_entity`, `islash_entity`,
`slash_entity`, `https_entity`, `http_entity`, `file_entity`, `protocol_entity`, `space_entity`,
`negative_number_entity`) are **deleted**; their content moves into `MNEMONICS`. This is the
consolidation the issue asks for.

## Error Handling

### How an entity error reaches the user

The parser is `IResult<Span, T, nom::error::Error<Span>>` throughout `parse.rs`, and changing that
type is a file-wide refactor. Three options were weighed:

| Option | Mechanism | Verdict |
|---|---|---|
| **A. Re-inspect at the reported position** | `cut` commits the parse once an opener is seen, so the failure position *is* the entity start; `describe_query_failure` calls `escape::explain_entity_error(e.input)` | **Chosen.** No type surgery, accurate positions, and the explanation lives beside the table that knows the answer |
| B. Custom nom error type | `impl ParseError + FromExternalError` carrying an `Error` | Principled, and the right move if a fourth or fifth error kind appears. Rewrites every signature in a 2000-line file for one feature |
| C. Distinct `nom::ErrorKind` markers | As `Verify`/`Fail` are used today | Rejected: `ErrorKind` is nom's enum and the unused-code argument that holds for two markers gets fragile at six |

Option A depends on `cut`, which the file already imports and uses. Once `~U`, `~D`, `~O`, `~B` or
`~n` is recognised, the entity cannot be anything else, so committing is correct on its own terms
and is what makes the position accurate.

### The errors

All are `liquers_core::error::Error` via typed constructors — no new error type, no `Error::new`.

| Input | Message |
|---|---|
| `~U110000~` | code point out of range (max `10FFFF`) |
| `~UD800~` | surrogate code point, not a Unicode scalar value |
| `~U~` | empty entity body |
| `~Uzz~` | not a valid hexadecimal digit |
| `~nfoo~` | unknown named entity `foo` |
| `~nhellip~` without the feature | **names the feature**: entity `hellip` is not in this build's curated set; enable `entities-html5`, or write `~U2026~` |
| `~U41` | unterminated entity: expected `~` |

The feature-specific message is a Phase 1 requirement, not a nicety: without it a wasm build reports
a generic parse failure for a query that works on the server.

## Integration Points

> **Correction to Phase 1.** Its Core Interactions section says "Store / Command / Asset / Value /
> Web / UI — no change". That is wrong for **Web**, and the error was found by enumerating callers
> of `encode_token` rather than by reasoning about layers: `liquers-web/src/encode.rs` is a
> hand-written encoder that exists because of this defect, and it carries a test asserting the
> defect still exists. Phase 1's claim holds for Store, Command, Asset, Value and UI.

### Crate `liquers-core`

| File | Change |
|---|---|
| `src/lib.rs:132` area | Add `pub mod entities;` and `pub mod escape;` to the module list |
| `src/entities.rs` | New content (currently a declared-nowhere empty file) |
| `src/escape.rs` | New file |
| `src/parse.rs:339-342` | `parameter_text` uses `escape::is_unescaped_parameter_char` |
| `src/parse.rs:329-338` | `resource_name` uses `escape::is_resource_name_char` |
| `src/parse.rs:345-400` | Ten entity parsers deleted; `entities` delegates to `escape::match_entity` |
| `src/parse.rs:998` | `describe_query_failure` consults `escape::explain_entity_error` first |
| `src/parse.rs:40-60` | Module-doc entity table updated, with the "emitted by encoder" column |
| `src/query.rs:503` | `encode_token` body removed, replaced by `pub use` |
| `src/query.rs:614` | `set_value` stops double-encoding (scope question above) |

### Crate `liquers-web` — the one downstream change

`liquers-web/src/encode.rs` exists **only** as a workaround for this defect, and says so: *"When
the entity redesign lands, `encodeParam` delegates to the fixed `encode_token` and the limitation
disappears."* Four changes:

| Location | Change |
|---|---|
| `encode.rs:1-33` module doc | Rewritten: the limitation is gone, the delegation is the design |
| `encode.rs:48` `encode_param` | Body becomes `Ok(encode_token(text))`, keeping the empty-string rejection, which is a grammar fact and not part of this defect |
| `encode.rs:91` `unencodable` | **Deleted** — no character is unencodable any more |
| `encode.rs:196` `web_core_encode_token_still_produces_unparseable_text` | **Deleted.** It asserts `encode_token("12:30") == "12:30"` and that the result does not round-trip — a test whose job is to fail when this lands, and it will |

**A consistency check this raises, and passes:** `liquers-web` is wasm and will not enable
`entities-html5`, so it decodes the curated set only. Because the encoder emits nothing but curated
names, every token `encode_param` produces parses in the browser. The asymmetry is confined to
hand-written exotic names, exactly as Phase 1 bounds it.

`liquers-py` does **not** call `encode_token` (checked); it uses `ActionParameter` but not the
encoder, so it needs no change beyond whatever `set_value` resolution is chosen.

### Dependencies

**None added.** The tables are generated source, `nom` and `nom_locate` are already present, and
nothing here needs a crate.

**`liquers-core/Cargo.toml`:**

```toml
[features]
default = ["async_store"]   # unchanged — entities-html5 is deliberately absent (D12)
entities-html5 = []
```

### Generating the tables

The tables are **generated source, checked in**, following the existing
`export-command-registry` / `registry_export` precedent rather than inventing a mechanism:

- Vendored input: `liquers-core/data/entities.json` (WHATWG), so the build needs no network.
- Generator: `liquers-core/src/bin/generate_entities.rs` (matching the existing `liquers_validate.rs`) behind the existing `cli` feature.
- Guard: a test that regenerates and compares, so a stale table fails CI — the same contract
  `cargo test -p liquers-lib --test registry_export` enforces for the command registry.

`build.rs` was considered and rejected: it would run for every consumer of `liquers-core`, and the
output is stable enough to read in review, which a build-script artifact is not.

## Documentation Architecture

### Reference Plan

**Extend** `specs/reference/api/DOC_02_QUERY_LANGUAGE_REFERENCE.md`. Audience: both. Area
`core/query`.

| Section | Change |
|---|---|
| §"Action-parameter entities" (`:94-111`) | Replace the table. New columns: **Encoded · Decoded · Emitted by encoder**. Add `~U ~D ~O ~B ~n`, the separator-tilde spelling, and the `~ntilde~`/`~nhyphen~` traps |
| §"Action-parameter entities" note (`:108`) | The claim "`encode_token` emits the general escapes for tilde, space, slash, and hyphen" becomes wrong; replace with the four-step priority order |
| Round-trip section (`:340`) | "`encode_token` is the only escaping path in the encoder" stays true; the limitation text does not |
| Risk table (`:357`) | The P1 `set_value` row gains its issue ID, and is struck if the fix lands here |
| New subsection | The `entities-html5` feature: what it adds, that no crate enables it, what the diagnostic says without it |
| Link | To `QUERY_ESCAPING_GUIDE.md` |
| Front matter | `reviewed:` bumped, `## History` row added (§9.2) |

**Also extend** `specs/reference/PROJECT_OVERVIEW.md` — grammar change, required by CLAUDE.md.
`:413` ("only string action parameters are escaped") stays true; the escape set changes. History row
and `reviewed:` bump.

### Guide Plan

**Create** `specs/guides/QUERY_ESCAPING_GUIDE.md`. Audience: both. Area `core/query`.
Task: putting arbitrary text into a query, by hand or programmatically.

| Section | Content |
|---|---|
| Escaping a value programmatically | `encode_token`, and why not to hand-roll it in a host language |
| Writing an entity by hand | `~U`/`~D`/`~O`/`~B`/`~n`, the separator tilde, worked examples |
| The two traps | `~ntilde~` is U+02DC and `~nhyphen~` is U+2010 — use `~~` and `~_` |
| When you need `entities-html5` | What is curated, how to enable, why no crate defaults it on |
| Why my query stopped parsing | The diagnostics table, mapped to causes |
| What the encoder emits | The priority order, and why canonical text is stable |

Snippets come from the Phase 3 examples; each links a real test.

### Other Documents to Create

None.

### New Reference or Guide Documents

| Path | Kind | Audience | Area | Purpose |
|---|---|---|---|---|
| `specs/guides/QUERY_ESCAPING_GUIDE.md` | guide | both | `core/query` | How to get arbitrary text into a query, and how to diagnose it when it fails |

### Existing Documents to Review or Update

| Path | In `affects_docs`? | Change |
|---|---|---|
| `specs/reference/api/DOC_02_QUERY_LANGUAGE_REFERENCE.md` | yes | Normative table, as above |
| `specs/reference/PROJECT_OVERVIEW.md` | yes | Grammar change |
| `specs/guides/LANGUAGE-INTEGRATION_GUIDE.md` | yes | `:279` tells integrators to raise a typed error for unrepresentable values *citing this issue*. Becomes wrong; replace with "build the `Query` and call `encode()`", which the paragraph already recommends, minus the limitation |
| `specs/README.md` | yes | Capability-map entry for the new guide |
| `specs/issues/PARAMETER-ESCAPING-INCOMPLETE.md` | yes | → `closed` with a resolution note (§4.3) |
| `liquers-web/README.md` | no | Considered and discarded — it documents test loops, not the encoder |
| `specs/reference/api/DOC_04_ENVIRONMENT_CONTEXT_EVALUATION.md` | no | Discarded: mentions "entities" in the unrelated sense of asset entities |
| `specs/reference/ASSET_LIFECYCLE.md` | no | Discarded: same |

### Design and Capability Links

- `specs/README.md` gains the guide and, per CLAUDE.md, a design-folder entry.
- `DOC_02` links the guide; the guide links back to `DOC_02` for the normative table.
- After Phase 5, `LANGUAGE-INTEGRATION_GUIDE.md` cites the guide instead of the issue.

### Evidence to Collect During Implementation

- Measured table sizes, against the ≈2100-name and ≈40 KB estimates, and the wasm bundle delta.
- Whether `cut` gives the position the diagnostics assume, in a nested-link parameter.
- Any query in the repository's own tests or examples whose canonical encoding changes.
- Whether the two-feature-state test matrix costs enough build time to need a CI note.

## Relevant Commands

### New Commands

**None.** This is a grammar and encoder change; it introduces no command, no namespace, no
`register_command!` invocation and no `ExtValue` variant.

### Relevant Existing Namespaces

**None are relevant, and that is worth stating rather than omitting.** The change sits below the
command layer: every namespace (`pl`, `lui`, `egui`, `ns-img`) benefits identically and passively,
because any command taking a string parameter can now receive values that previously could not be
encoded. No command signature changes.

`specs/command_registry.yaml` therefore does **not** need regenerating — no `register_command!`
signature changes. `cargo test -p liquers-lib --test registry_export` should stay green untouched,
and that is a check worth running as evidence rather than an assumption.

**Question for the user at this gate:** confirm that no command-level work is expected — in
particular that this design is *not* also meant to add a `to_encoded`/`from_encoded` command pair
or similar query-construction helpers. Those would belong to `QUERY-BUILDER-TOOLING`.

## Open Questions

1. **Does the `set_value` fix land here?** Recommended yes: three lines and a test, and leaving it
   means shipping a round-trip guarantee that a public setter on the same type breaks, over a wider
   input set than today. The alternative is to leave
   `ACTION-PARAMETER-SET-VALUE-DOUBLE-ENCODES` open and note the interaction in `DOC_02`.
2. **Is `escape::segments` in the public API now, or `pub(crate)` until the AST design needs it?**
   Recommended public: it is the Phase 1 "must remain reachable" constraint, it costs one function,
   and publishing it later is not a breaking change either way. Making it public now means the AST
   design starts from a stable base rather than a private helper.
3. **Confirm no command-level work** (above).

## References

- Phase 1: `./phase1-high-level-design.md` — decisions D1-D13
- `specs/issues/PARAMETER-ESCAPING-INCOMPLETE.md`
- `specs/issues/ACTION-PARAMETER-SET-VALUE-DOUBLE-ENCODES.md` (filed in this phase)
- `specs/issues/RESOURCE-NAME-ASCII-ONLY.md`, `specs/issues/QUERY-AST-DISCARDS-ENTITIES.md`
- `specs/design/query-link-parser/` — the `~X~`/`~E` grammar and the `cut` precedent
- `liquers-web/src/encode.rs` — the workaround this removes
