---
id: PARAMETER-ENTITY-ESCAPING
kind: design
title: Parameter entity escaping (numeric and named tilde entities)
workflow: liquers-project
status: draft
phase: implementation
area: [core/query]
gh_pr: []
issues: [PARAMETER-ESCAPING-INCOMPLETE, ACTION-PARAMETER-SET-VALUE-DOUBLE-ENCODES]
affects_docs: [specs/reference/api/DOC_02_QUERY_LANGUAGE_REFERENCE.md, specs/reference/PROJECT_OVERVIEW.md, specs/guides/LANGUAGE-INTEGRATION_GUIDE.md]
created: 2026-08-14
superseded_by:
---
# parameter-entity-escaping Design Tracking

**Created:** 2026-08-14

## Phase Status

- [x] Phase 1: High-Level Design (approved)
- [x] Phase 2: Solution & Architecture (approved)
- [x] Phase 3: Examples & Testing (approved)
- [ ] Phase 4: Implementation Plan
- [ ] Phase 5: Documentation
- [ ] Implementation Complete

## Notes

Resolves `PARAMETER-ESCAPING-INCOMPLETE` (P0). Syntax **decided**: `~U<hex>~` `~D<dec>~` `~O<oct>~`
`~B<bin>~` for numeric entities, `~n<name>~` for named ones (`n` for "named"). Alternatives considered are in Annex A
of the Phase 1 document; the curated entity set is Annex B.

Key Phase 1 findings, for anyone picking this up cold:

- **The long form must be entered only on an opener letter that legacy text cannot produce.**
  Measured at HEAD: `f-~Hexampledotcom~~` parses and means `https://exampledotcom~`, so a bare
  `~<name>~` named entity would silently change an existing query's meaning. `~U ~D ~O ~B ~n` are
  all rejected today, which is what makes them safe.
- **`entities.rs` is reserved for named entities**; the general escaping algorithm and the numeric
  codec go in a new `escape.rs`. See the Crate Placement note for why this still satisfies the
  issue's one-definition requirement.
- **AST representation of entities is out of scope** — filed as `QUERY-AST-DISCARDS-ENTITIES`.
- **The parser widens to `char::is_alphanumeric()`, the encoder emits pure ASCII.** Widening is the
  non-breaking direction (`f-Ł` parses today); ASCII output keeps queries safe through ASCII-only
  systems. Consequence: liquers does not normalize, so composed and decomposed `café` stay two
  different values with two different canonical spellings.
- **A character with a curated entity is always encoded as that entity**, even when `~U<hex>~` is
  shorter. This makes the curated set a frozen compatibility surface — adding a name later changes
  canonical text and invalidates derived keys — so the tier boundaries in Annex B are a decision to
  take now, not a knob to turn later. Latin-1 accented letters stay out: `café` → `caf~UE9~`.
- **The full HTML5 table is an optional feature, deliberately not in `liquers-core`'s `default`.**
  That placement is what makes it work: `liquers-lib` and `liquers-store` both pull `liquers-core`
  with defaults on, so anything in that default set is unavoidable for the wasm bundle, and
  unification only ever adds. Native crates opt in. Everything any build encodes, every build
  decodes, because the encoder emits only curated names and those are compiled unconditionally.
- **`resource_name` narrows to ASCII alphanumeric.** Coherent, but `-R/data/ŁŁ.csv` parses at HEAD
  and will stop, and non-ASCII filenames stay unaddressable — filed as `RESOURCE-NAME-ASCII-ONLY`.
- **`encode_token` stays infallible.** `&str` guarantees every `char` is a scalar value and every
  scalar value has a `~U<hex>~` spelling, so no input is unrepresentable. Errors belong to the
  decoder (out-of-range, surrogate, unknown name, missing terminator).

### The invariant this design establishes

The invariant is a property of stored state, and it is one sentence: **in every
`ActionParameter::String(s, _)`, `s` is the decoded parameter value, unconstrained** — the value
itself, never query text, ranging over all of `String`.

Everything else is consequence: constructors and setters do not encode, the parser stores the
decoded result, and `encode`/`render`/`styled_tokens` are the only three sites that encode, computing
it on the way out and never storing it. What it buys is `parse(encode(p)) == p` for any
programmatically built parameter. What it does *not* claim: `encode(parse(t)) == t` (decoding is
many-to-one, so re-encoding normalises), and preservation of the original spelling (that is
`QUERY-AST-DISCARDS-ENTITIES`).

`set_value` violated this because it came from a different model — a string parameter as an
*elementary, already-encoded token*, reached while thinking about `QUERY-AST-DISCARDS-ENTITIES`.
That model is rejected: it makes the caller learn the grammar, makes `string_value()` return
something other than what was set, and makes the same variant mean two things depending on how it
was built. The constraint is recorded in `QUERY-AST-DISCARDS-ENTITIES` too, since that design is the
one most likely to reintroduce it.

### Phase 4: the entity table measured offline

No network is needed at any point — Python's standard library ships the HTML5 table
(`from html.entities import html5`), so the vendored `entities.json` is generated locally. Measured
against the real data:

| Estimate | Measured |
|---|---|
| ≈2100 names | **2125** distinct semicolon-terminated names (2231 incl. legacy forms) |
| ~40 KB blob | **36.4 KB** with `u32` offsets, **28.1 KB** with `u16` — `u16` adopted |
| ~95 KB slice-of-pairs | **86.2 KB** |

Also confirmed from the real table: 93 names decode to more than one character; the longest name is
31 characters; **no HTML5 name exists for `~` or `-`**, and **all 26 ASCII punctuation characters
the grammar rejects do have names** — which is what Annex B tier B-1 claims. `tilde` is U+02DC,
`Tilde` U+223C, `hyphen`/`dash` U+2010, exactly as the traps were documented.

`ActionParameter::set_value` has **zero callers in this workspace** — every `set_value` hit belongs
to `CommandArguments` or `Asset`. The fix therefore risks only out-of-tree code.

### Phase 3: the properties were executed, not argued

A prototype of the encoder and decoder was run over **4 130 inputs** — every printable ASCII
character, Latin-1 and CJK, astral code points, U+0000, U+10FFFF, controls, adversarial strings and
4 000 random strings. Round trip, idempotence and ASCII-only output: **4 130 / 4 130** each, plus
30 / 30 hand-written valid tokens for idempotence from decode-only spellings. Every expected value
in the Phase 3 document is that prototype's output; the legacy spellings were additionally checked
against the real parser.

Two things this turned up:

- **Today's `E` is not merely non-idempotent, it is undefined.** The current encoder turns
  `f-~Hapi.example.com~/data` into `f-https:~/~/api.example.com~/data`, which does not parse. The
  design does not improve idempotence; it makes it exist.
- **A hand-written octal constant in the design was wrong** (`~O11746~` for U+1F600, correct is
  `~O373000~`). Hence the rule recorded for implementation: every radix example in the reference and
  guide is generated from the code path it documents, and the `escape.rs` doc examples are
  `assert_eq!` doctests so a wrong constant fails the build.

### Idempotence and error positions (verified)

- `E = encode ∘ parse` is **idempotent**: `E(E(t)) == E(t)`, a corollary of `parse(encode(v)) == v`,
  which makes `E` a projection onto canonical text. `E(t) == t` is false in general — the shorthand
  `data/report/-/to_text` canonicalises to `-R/data/report/-/to_text` — which is why idempotence and
  not identity is the property to assert. Phase 3 tests one pass against two.
- **Entity errors report accurate positions.** Measured at HEAD: nom reports the span the failure
  was raised with, exactly — `a-x~X~q~E` already reports offset 3, the `~X~` itself. So the entity
  parser captures the span at the opening `~` and raises with it. Two corrections to the Phase 2
  sketch followed: `cut` does not choose the position (it preserves the inner parser's span), and
  dispatch uses a distinct `ErrorKind::Escaped` rather than sniffing the input.
- **Consequence for `QUERY-AST-DISCARDS-ENTITIES`:** two of its three motivations are gone.
  Lowered to `P3`, and a cheaper route recorded — `styled_tokens` can call the now-public
  `escape::segments` on the encoded form to highlight entities with no AST change, at the cost of
  showing canonical rather than typed spelling.

### Phase 2 findings

- **`liquers-web/src/encode.rs` must change**, contradicting Phase 1's "Web: no change". It is a
  hand-written encoder that exists only because of this defect, and its test
  `web_core_encode_token_still_produces_unparseable_text` asserts the defect still exists — it will
  fail when this lands, by design.
- **`ActionParameter::set_value` double-encodes** (`query.rs:614`): it stores `encode_token(v)`
  where every other path stores the decoded value. Documented as a P1 risk in `DOC_02` but never
  filed; now `ACTION-PARAMETER-SET-VALUE-DOUBLE-ENCODES`. It gets worse under the new encoder, and
  it falsifies the round-trip guarantee through a public method. Recommended to fix here.
- **`STORE-FILESTORE-PATH-TRAVERSAL` (P0) is avoided by architecture, not tolerated.** D10 leaves
  `resource_name` with no entity production, so no decoded `/` can reach `key_to_path`. It becomes
  a hard prerequisite if `RESOURCE-NAME-ASCII-ONLY` ever goes to option B or C.
- **Entity errors use nom's existing error type**, with `cut` committing at the opener so the
  reported position is the entity start, and `describe_query_failure` consulting
  `escape::explain_entity_error`. A custom nom error type would rewrite every signature in a
  2000-line file for one feature.

## Links

- [Phase 1](./phase1-high-level-design.md)
- [Phase 2](./phase2-architecture.md)
- [Phase 3](./phase3-examples.md)
- [Phase 4](./phase4-implementation.md)
- [Phase 5](./phase5-documentation.md)
