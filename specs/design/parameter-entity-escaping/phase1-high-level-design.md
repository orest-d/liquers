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

## Chosen Syntax (decided)

Radix-in-opener (Annex A-1, option 1a) and the `x` prefix (Annex A-2, option 2a), confirmed by the
maintainer.

| Form | Meaning | Example | Emitted by encoder |
|---|---|---|---|
| `~U<hex>~` | Unicode code point, hexadecimal | `~U41~` → `A`, `~U1F600~` → 😀 | **yes — canonical** |
| `~D<dec>~` | Unicode code point, decimal | `~D65~` → `A` | no |
| `~O<oct>~` `~B<bin>~` | octal / binary | `~O101~`, `~B1000001~` | no |
| `~x<name>~` | named entity | `~xamp~` → `&`, `~xcolon~` → `:` | see "Canonical encoding" |

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

## Canonical encoding

**Requirement (maintainer):** the encoder emits the *shortest* representation, and once decided the
choice is *stable*, so the canonical query representation is stable. Query text is identity in
liquers — asset keys, cache keys and links are query strings — so a later change to the encoder
invalidates caches and breaks stored links. Stability is therefore a compatibility guarantee, not a
preference.

Three rules follow, and the third is the one that is easy to get wrong:

1. **Shortest is computed, not approximated.** Encoding runs a shortest-path pass over the
   character positions of the token rather than a greedy left-to-right scan, so the result is
   provably minimal. Ties break by a fixed repertoire order (literal ▸ legacy mnemonic ▸ numeric
   hex ▸ named), which makes the output deterministic.
2. **The canonical repertoire is frozen**, and versioned if it ever has to change. It contains
   the literal characters, the legacy mnemonics (`~~ ~_ ~. ~/ ~<digit> ~h ~H ~f ~P`) and `~U<hex>~`.
   `~D ~O ~B` exist for people writing queries by hand and are never emitted.
3. **The canonical repertoire must not depend on cargo features.** This is forced by the
   feature decision below: if a `default-features = false` build compiled a smaller name table *and*
   the encoder drew on that table, two builds of the same library would produce different canonical
   text for the same value, and the smaller build could not even decode the larger build's output.
   The feature therefore gates **decoding only**. See Open Question 1 for whether named entities
   belong in the encoder's repertoire at all.

**The encoder is infallible.** `encode_token` takes `&str`, which is UTF-8 by construction, so every
`char` in it is a Unicode scalar value and every scalar value has a `~U<hex>~` spelling — lone
surrogates cannot occur in the input. There is no failure case left, so the signature stays
`fn encode_token<S: AsRef<str>>(s: S) -> String`. Fallibility belongs entirely to the **decoder**,
which must reject `~U110000~` (out of range), `~UD800~` (surrogate), `~xfoo~` (unknown name),
`~U~` (empty body) and a missing terminator.

## Named-entity table

Full HTML5 by default; the curated subset is what remains when the feature is switched off.

| Build | Names decoded | Rationale |
|---|---|---|
| default (`entities-html5` on) | all ≈2100 HTML5 named references | the maintainer's default; nothing surprises a user who knows HTML |
| `default-features = false` | the curated set of Annex B (≈200) | wasm and embedded builds; `liquers-web` ships to browsers, where the full table is real payload |

**The feature adds, it never removes** — the curated set is compiled unconditionally and
`entities-html5` extends it. This matters because cargo features unify across a dependency graph: a
"restrict" feature that deleted names would be non-additive, and one crate enabling it would silently
change another crate's behaviour. With the additive form, `default-features = false` is the
documented restriction mechanism and every build decodes a superset of what any build encodes.

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

**Content requirement (maintainer):** every entity table — the one in
`DOC_02_QUERY_LANGUAGE_REFERENCE.md` and the rustdoc on the tables in `entities.rs` and `escape.rs`
— carries an explicit **"emitted by encoder"** column, so a reader can tell at a glance which
spelling is canonical and which is accepted-but-never-produced. Without it the decode-superset
design is invisible and someone will assume `encode(parse(t)) == t`.

## Decisions Taken

| # | Decision | By |
|---|---|---|
| D1 | Radix in the opener: `~U` hex, `~D` decimal, `~O` octal, `~B` binary (Annex A-1a) | maintainer |
| D2 | Named entities take the `x` prefix: `~x<name>~` (Annex A-2a) | maintainer |
| D3 | Full HTML5 table by default; curated set via `default-features = false`, additive | maintainer |
| D4 | Encoder emits the shortest representation, and the canonical repertoire is frozen and feature-independent | maintainer |
| D5 | `~<digit>` (compact negative number) stays | maintainer |
| D6 | Every entity table documents which spellings the encoder emits | maintainer |
| D7 | `encode_token` stays **infallible** — `&str` guarantees every `char` is a scalar value, so every input is representable. Fallibility is the decoder's | resolved; the Phase 1 question was unfounded |

## Open Questions

1. **Does the encoder's canonical repertoire include the curated named entities, or numeric only?**
   D4 says shortest; numeric hex and short names are frequently the *same* length (`~U3C~` and
   `~xlt~` are both 5 characters), and names win above U+00FF (`~xpi~` = 5 beats `~U3C0~` = 6).
   Recommend **numeric only**: it keeps the frozen repertoire tiny and independent of a name table
   that will otherwise be unable to gain a shorter alias for the rest of time without changing
   canonical text. Cost is 1–2 characters on non-ASCII values. If readability wins instead, the
   curated set must be frozen as strictly as the encoder is.
2. **`c as u8` decision (issue item 3)** — still open. Recommend widening to
   `char::is_alphanumeric()` rather than narrowing to ASCII: widening keeps every currently-parsing
   query (`f-Ł` parses today) whereas narrowing breaks them. Consequence to settle in Phase 2:
   whether encode normalizes to NFC, since query text is identity and `é` has two spellings.
3. **How is the full table represented?** ≈2100 entries as `&[(&str, &str)]` costs roughly 95 KB of
   static data, mostly fat pointers; a concatenated blob with an offset index is nearer 40 KB. This
   decides whether the default feature is comfortable for `liquers-web`. Phase 2 to measure, and to
   confirm the entity count and payload against the WHATWG `entities.json` rather than the estimate
   used here.
4. **Is the curated set of Annex B the right one?** It is derived from a rule (everything the
   grammar rejects, plus what a data-processing query plausibly carries) rather than from usage
   data.

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

---

# Annex B: Proposed Curated Entity Set

The set compiled unconditionally, and therefore the whole vocabulary of a
`default-features = false` build. Proposed at **≈200 names in four tiers**, each tier justified by a
rule rather than by taste, so the boundary is arguable rather than arbitrary.

## B-0. XML predefined (5)

`amp` `lt` `gt` `quot` `apos`

The only five entities every XML processor knows. Present in any conceivable build.

## B-1. ASCII punctuation (26) — the tier that actually matters

Exactly the printable ASCII characters that the parameter grammar rejects and that no legacy
mnemonic covers. Computed, not guessed: the accepted set is `[A-Za-z0-9_+.]` and the mnemonics
cover `~`, space, `/` and `-`, which leaves

```
! " # $ % & ' ( ) * , : ; < = > ? @ [ \ ] ^ ` { | }
```

| Char | Name | Char | Name | Char | Name |
|---|---|---|---|---|---|
| `!` | `excl` | `,` | `comma` | `\` | `bsol` |
| `"` | `quot` | `:` | `colon` | `]` | `rsqb` |
| `#` | `num` | `;` | `semi` | `^` | `Hat` |
| `$` | `dollar` | `<` | `lt` | `` ` `` | `grave` |
| `%` | `percnt` | `=` | `equals` | `{` | `lcub` |
| `&` | `amp` | `>` | `gt` | `\|` | `verbar` |
| `'` | `apos` | `?` | `quest` | `}` | `rcub` |
| `(` | `lpar` | `@` | `commat` | | |
| `)` | `rpar` | `[` | `lsqb` | | |
| `*` | `ast` | | | | |

All 26 have HTML5 names, so this tier introduces no liquers invention. Add the HTML5 aliases
`lbrack` `rbrack` `lbrace` `rbrace` `vert` `midast` as decode-only synonyms.

Also include the names for characters that *are* accepted literally — `sol` (`/`), `lowbar` (`_`),
`plus` (`+`), `period` (`.`), `num`, `commat` — so that a mechanical HTML-to-liquers translation
never fails. They decode to the literal character; the encoder never emits them.

**Two traps to document, both of them HTML's fault:**

- `~xtilde~` is **not** `~`. `&tilde;` is U+02DC ˜ (small tilde) and `&Tilde;` is U+223C ∼; ASCII
  `~` U+007E has no HTML5 name. Use `~~`.
- `~xhyphen~` is **not** `-`. `&hyphen;` and `&dash;` are both U+2010 ‐; ASCII hyphen-minus U+002D
  has no HTML5 name. Use `~_`.

Getting either wrong produces a valid query that means something subtly different, which is the
worst failure mode available here. Both belong in the reference table with a warning.

## B-2. Latin-1 and typography (≈55)

The characters that appear in ordinary European text and in text copied out of word processors —
the realistic content of a parameter derived from user data.

`nbsp` `iexcl` `cent` `pound` `curren` `yen` `brvbar` `sect` `uml` `copy` `ordf` `laquo` `not`
`shy` `reg` `macr` `deg` `plusmn` `sup2` `sup3` `acute` `micro` `para` `middot` `cedil` `sup1`
`ordm` `raquo` `frac14` `frac12` `frac34` `iquest` `times` `divide` `szlig` `aelig` `oslash`
`eth` `thorn` — plus `ndash` `mdash` `horbar` `lsquo` `rsquo` `sbquo` `ldquo` `rdquo` `bdquo`
`dagger` `Dagger` `bull` `hellip` `permil` `prime` `Prime` `trade` `euro`

Accented letters (`eacute`, `uuml`, …) are **excluded**: under Open Question 2's recommended
widening they are ordinary literal characters and need no entity at all. If the parser narrows to
ASCII instead, this tier must grow by the ≈60 Latin-1 letter names — the two decisions are coupled.

## B-3. Greek, mathematics and arrows (≈110)

`alpha`…`omega` and `Alpha`…`Omega` (48), then the operators a scientific or data-processing query
plausibly carries:

`minus` `lowast` `radic` `prop` `infin` `ang` `and` `or` `cap` `cup` `int` `there4` `sim` `cong`
`asymp` `ne` `equiv` `le` `ge` `sub` `sup` `nsub` `sube` `supe` `oplus` `otimes` `perp` `sdot`
`part` `exist` `forall` `empty` `nabla` `isin` `notin` `ni` `prod` `sum` `larr` `uarr` `rarr`
`darr` `harr` `crarr` `lArr` `uArr` `rArr` `dArr` `hArr` `loz` `spades` `clubs` `hearts` `diams`
`ensp` `emsp` `thinsp` `zwnj` `zwj` `lrm` `rlm`

This tier is the most arguable and the easiest to cut: it is the one to drop first if the curated
build needs to be smaller still, since everything in it is reachable as `~U<hex>~`.

## Totals

| Tier | Names | Cumulative |
|---|---|---|
| B-0 XML predefined | 5 | 5 |
| B-1 ASCII punctuation + aliases | ~35 | ~40 |
| B-2 Latin-1 and typography | ~55 | ~95 |
| B-3 Greek, maths, arrows | ~110 | ~205 |

≈205 of ≈2100, so roughly a tenth of the full table's payload. **Nothing is lost by cutting a
tier** — every character remains writable as `~U<hex>~`, and under the recommended encoder
repertoire (Open Question 1) the encoder's output does not reference this table at all. The tiers
therefore trade only hand-writing convenience against binary size, which is the right thing for a
cargo feature to trade.
