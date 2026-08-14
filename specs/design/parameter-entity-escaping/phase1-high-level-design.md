# Phase 1: High-Level Design - Parameter Entity Escaping

## Feature Name

Parameter Entity Escaping (long-form tilde entities: numeric and named)

## Purpose

Extend the query grammar's `~` escape with two **variable-length, `~`-terminated** entity forms — a
numeric one covering every Unicode code point (`~U0041~`) and a named one covering the HTML/XML
entity vocabulary (`~xamp~`) — and rewrite the encoder on top of them, so that an arbitrary string
can be carried in an action parameter and `parse(encode(s)) == s` holds for every `s`. Resolves
`PARAMETER-ESCAPING-INCOMPLETE` (P0).

## Scope

**In scope:** the two new entity forms; **encoding of arbitrary strings** (`encode_token` and the
public escaping API it backs, which is the half that is actually broken today); the `c as u8`
truncation; consolidating the encoder and decoder so they cannot drift again.

**Out of scope:** representing entities as nodes in the AST. Entities are decoded to text during
parsing and the spelling is lost, which limits highlighting and diagnostics — filed as
`QUERY-AST-DISCARDS-ENTITIES` (complexity `L`, needs its own design). This design must not make
that harder: the segment information exists inside the new decoder and should be *reachable*, even
though `ActionParameter` keeps its current flat shape here.

## Core Interactions

### Query System
The only system touched. Adds two productions to the `entities` combinator
(`liquers-core/src/parse.rs:386`), rewrites `encode_token` (`query.rs:503`) to emit them, and fixes
the `c as u8` truncation in the accepted-character class (`parse.rs:340`, and the same bug in
`resource_name`).

### Store / Command / Asset / Value / Web / UI
No change. Every one of them benefits indirectly: keys, links and recipes derived from user data
become expressible. No new commands, no new value types, no new endpoints.

## Crate Placement

`liquers-core` only. Two modules, not one:

| Module | Owns |
|---|---|
| `entities.rs` (empty placeholder, to be declared in `lib.rs`) | **named entities only** — the name→text table, lookup, and the liquers-native names. Reserved for this by the maintainer. |
| `escape.rs` (new) | the general escaping algorithm: the accepted-character class, the numeric-entity codec, the short mnemonic table (`~~ ~_ ~. ~/ ~I ~h ~H ~f ~P ~<digit>`), `encode_token`, and the round-trip property test. Depends on `entities.rs` for named decoding. |

`parse.rs` keeps the nom combinators and delegates to both; `query.rs` re-exports `encode_token`
from `escape.rs` so no downstream import path changes.

> **Deviation from the issue text**, recorded deliberately: `PARAMETER-ESCAPING-INCOMPLETE` proposes
> `entities.rs` as the home for *one consolidated table*. The split above keeps the maintainer's
> reservation of `entities.rs` for named entities while still satisfying the issue's real
> requirement — that encoder and decoder derive from a single definition — because each of the two
> tables is defined once and used in both directions.

## Chosen Syntax (recommendation)

| Form | Meaning | Example |
|---|---|---|
| `~U<hex>~` | Unicode code point, hexadecimal | `~U41~` → `A`, `~U1F600~` → 😀 |
| `~D<dec>~` | Unicode code point, decimal | `~D65~` → `A` |
| `~O<oct>~` `~B<bin>~` | octal / binary | `~O101~`, `~B1000001~` |
| `~x<name>~` | named entity | `~xamp~` → `&`, `~xcolon~` → `:` |

**Case rule** (as proposed): an **uppercase** letter after `~` opens a liquers-structural entity
(`U D O B`, joining the existing `I H P X E`); a **lowercase** letter opens a text entity (`x`,
joining `h f`). Entity bodies are `[A-Za-z0-9_-]+`; no non-alphanumeric character is introduced
into the grammar and `;` is *not* admitted — the closing `~` is the terminator, which the existing
`~X~` already establishes as a shape.

**Why the long form is entered only on an unclaimed opener.** Measured at HEAD:
`f-~Hexampledotcom~~` parses today and means `https://exampledotcom~`. A bare `~<name>~` form would
silently re-read it as a named entity, so backward compatibility *requires* a prefix that legacy
text cannot produce. `~U ~D ~O ~B ~x` are all rejected by the parser today (verified), so no
existing query changes meaning and the `alt` order stays insensitive.

## Documentation Intent

**Reference:** extend `specs/reference/api/DOC_02_QUERY_LANGUAGE_REFERENCE.md` §"Action-parameter
entities" — it already owns the normative entity table; a second reference would split it.
Also update `specs/reference/PROJECT_OVERVIEW.md` (grammar change, per CLAUDE.md).

**Guide:** extend `specs/guides/LANGUAGE-INTEGRATION_GUIDE.md` — it currently tells integrators to
raise an error for unrepresentable values *because of this issue*; that paragraph becomes wrong.

**Other documents to create:** None — this is a defect fix, and the entity table is reference
material, not a repeatable workflow.

**Specific documents to update:** the two references and one guide above, plus
`specs/issues/PARAMETER-ESCAPING-INCOMPLETE.md` → `closed`. Audience: anyone writing a query by
hand or building one programmatically, in any host language.

## Open Questions

1. **Radix default.** Recommendation above gives each radix its own opener. Alternative: one opener
   `~U…~` with the radix inside (`~Ux41~`/`~Ud65~`, HTML-style). See Annex A-1.
2. **`~x` prefix letter.** `x`, `n` ("named") and `e` ("entity") are all free. Annex A-2.
3. **`c as u8` decision (issue item 3).** Recommend widening to `char::is_alphanumeric()` rather
   than narrowing to ASCII: widening keeps every currently-parsing query (`Ł` parses today) whereas
   narrowing breaks them. Consequence: Unicode normalization becomes a live question for keys.
4. **Named-entity table size.** Full HTML5 (2231 names, ~30 KB static) vs the 5 XML predefined
   names plus a curated liquers set. Feature-gate the large table?
5. **Does the encoder ever emit named entities?** Recommend **no** — numeric is canonical, named is
   decode-only (except the legacy `~H ~h ~f ~P` mnemonics, which the issue requires be kept).
6. Should `~<digit>` (negative number) be retired, or kept as the compact special case? Recommend
   **kept** — the prefix scheme removes the collision the issue feared.
7. **Does the public API gain a fallible encoder?** Today `encode_token` cannot fail and silently
   emits unparseable text. After this change it can always succeed, so the answer may be "no" — but
   surrogate/`char`-boundary edge cases should be checked before committing to an infallible
   signature.

## References

- `specs/issues/PARAMETER-ESCAPING-INCOMPLETE.md` (P0, accepted) — the defect this resolves
- `specs/issues/QUERY-AST-DISCARDS-ENTITIES.md` (draft) — filed from this design; out of scope here
- `specs/design/query-link-parser/` — precedent for `~X~ … ~E` and for in-band `~` parsing
- WHATWG HTML named character references (source for the named table)
- Annex A (below) — alternative syntaxes, as requested

---

# Annex A: Alternative Syntaxes

## A-1. Numeric entity (issue item 1)

| # | Syntax | `A` | 😀 | Assessment |
|---|---|---|---|---|
| **1a** | **Radix in opener** — `~U<hex>~ ~D<dec>~ ~O<oct>~ ~B<bin>~` | `~U41~` | `~U1F600~` | **Recommended.** Shortest. `~U` reads as `U+`, so hex-by-default carries no surprise. Costs 3 extra uppercase letters from a 21-letter free pool. |
| 1b | Radix in body — `~Ux41~ ~Ud65~ ~Uo101~ ~Ub…~` | `~Ux41~` | `~Ux1F600~` | HTML-familiar (`&#x41;`). One opener. But mixes case inside the body, and collides with `x` if `x` is also the named prefix. |
| 1c | Bare = decimal, `x` = hex (HTML convention) — `~U65~`, `~Ux41~` | `~U65~` | `~Ux1F600~` | Familiar, but `~U0041~` would mean `)` — a footgun, since `U+0041` is universally hex. |
| 1d | Fixed-width, terminator-free — `~u0041`, `~U0001F600` (Java/C style) | `~u0041` | `~U0001F600` | Keeps "all entities are fixed-length", so no terminator is admitted at all. But verbose, and `u`/`U` carrying width is error-prone. Astral planes need the 8-digit form. |
| 1e | Retire `~<digit>` and use `~<digits>~` for code points | `~65~` | `~128512~` | Shortest of all, but breaks every existing query with a negative number. The issue's "migration path" option; rejected on the backward-compatibility requirement. |

## A-2. Named entity (issue item 2)

| # | Syntax | `&` | Assessment |
|---|---|---|---|
| **2a** | **Prefix letter** — `~x<name>~` | `~xamp~` | **Recommended.** Deterministic, no lookahead, provably backward compatible. Alternative prefixes: `~n<name>~` ("named", no hex connotation) or `~e<name>~` ("entity", but easy to confuse with `~E`). |
| 2b | Bare `~<name>~`, table-driven disambiguation | `~amp~` | Prettiest, and closest to HTML. But `~h…~`/`~f…~`/`~H…~` must be resolved by consulting the name table, and `f-~Hexampledotcom~~` — a query that parses today — changes meaning. Rejected: measured, not hypothetical. |
| 2c | Bare, but names starting with `h`/`f` forbidden | `~amp~` | Same break as 2b for uppercase openers, and `&hellip;`/`&frac12;` become second-class. Inconsistent. |
| 2d | Case as the discriminator — uppercase name = liquers, lowercase = HTML, no prefix | `~amp~` | This is the user's case rule taken literally. It does not by itself remove the `~h`/`~f`/`~H` collisions, so it must be combined with 2a or 2b; here it is kept as the *namespace* rule while 2a supplies determinism. |
| 2e | Two-tilde opener — `~~<name>~` | `~~amp~` | `~~` is already a literal `~`, so `~~amp~` parses today as `~amp` + `~`. Rejected outright. |

## A-3. Combination note

1a + 2a leaves the grammar with one uniform rule: **`~` followed by an opener letter; if the
opener is one of the long-form openers, read to the closing `~`.** Legacy short entities are
untouched because their opener letters are disjoint from the long-form set. That disjointness —
not `alt` ordering — is what makes the extension safe, and it is checkable by a test that asserts
the two opener sets do not intersect.
