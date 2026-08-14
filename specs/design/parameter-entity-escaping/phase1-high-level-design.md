# Phase 1: High-Level Design - Parameter Entity Escaping

## Feature Name

Parameter Entity Escaping (long-form tilde entities: numeric and named)

## Purpose

Extend the query grammar's `~` escape with two **variable-length, `~`-terminated** entity forms — a
numeric one covering every Unicode code point (`~U0041~`) and a named one covering the HTML/XML
entity vocabulary (`~namp~`) — and rewrite the encoder on top of them, so that an arbitrary string
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
the `c as u8` truncation in the accepted-character class (`parse.rs:340`) and, differently, in
`resource_name` — parameters widen to Unicode alphanumerics (D6), resource names narrow to ASCII
(D10).

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

Radix-in-opener (Annex A-1, option 1a) and the `n` prefix (Annex A-2, option 2a), confirmed by the
maintainer.

| Form | Meaning | Example | Emitted by encoder |
|---|---|---|---|
| `~U<hex>~` | Unicode code point, hexadecimal | `~U41~` → `A`, `~U1F600~` → 😀 | **yes — canonical** |
| `~D<dec>~` | Unicode code point, decimal | `~D65~` → `A` | no |
| `~O<oct>~` `~B<bin>~` | octal / binary | `~O101~`, `~B1000001~` | no |
| `~n<name>~` | named entity | `~namp~` → `&`, `~ncolon~` → `:` | **curated names only** |

**Case rule** (as proposed): an **uppercase** letter after `~` opens a liquers-structural entity
(`U D O B`, joining the existing `I H P X E`); a **lowercase** letter opens a text entity (`n`,
joining `h f`). Entity bodies are `[A-Za-z0-9_-]+`; no non-alphanumeric character is introduced
into the grammar and `;` is *not* admitted — the closing `~` is the terminator, which the existing
`~X~` already establishes as a shape.

**Why the long form is entered only on an unclaimed opener.** Measured at HEAD:
`f-~Hexampledotcom~~` parses today and means `https://exampledotcom~`. A bare `~<name>~` form would
silently re-read it as a named entity, so backward compatibility *requires* a prefix that legacy
text cannot produce. `~U ~D ~O ~B ~n` are all rejected by the parser today (verified), so no
existing query changes meaning and the `alt` order stays insensitive.

## Canonical encoding

**Stability is a compatibility guarantee, not a preference** (maintainer). Query text is identity in
liquers — asset keys, cache keys and links are query strings — so once a spelling is chosen for a
character, changing it invalidates derived keys and breaks stored links.

**Readability beats brevity where a curated name exists** (maintainer): *a character with a curated
entity is always represented as that curated entity, unless it has a liquers shortcut; every other
character is represented in hex.* So `:` encodes as `~ncolon~`, not the three-characters-shorter
`~U3A~`, while `/` keeps its shortcut `~/` rather than becoming `~nsol~`.

This governs characters that **must** be escaped. Characters the grammar accepts literally stay
literal even where a curated name exists — `+`, `.` and `_` encode as themselves, not as
`~nplus~`, `~nperiod~`, `~nlowbar~` — since escaping them would serve nothing.

**The encoder output is pure ASCII** (maintainer): a non-ASCII character is escaped even though the
parser would accept it literally, so that a query survives transport through ASCII-only systems.

Encoding is therefore a deterministic left-to-right scan with a **fixed priority order** at each
position, not a shortest-path search — the priority is what makes it stable, and length only breaks
ties inside step 1:

| Step | Rule | Example |
|---|---|---|
| 1 | longest matching **legacy mnemonic** | `https://` → `~H`, ` ` → `~.`, `/` → `~/`, `-4` → `~4` |
| 2 | **literal**, if the character is ASCII-accepted `[A-Za-z0-9_+.]` | `a` → `a` |
| 3 | **curated named entity** | `:` → `~ncolon~`, `&` → `~namp~`, `°` → `~ndeg~` |
| 4 | **`~U<hex>~`** | `😀` → `~U1F600~` |

Step 1 outranks step 3, so `/` stays `~/` rather than `~nsol~` and URLs keep the compact `~H` form
the issue requires. Step 2 outranks step 3, so `+`, `.` and `_` stay literal despite having the
names `plus`, `period` and `lowbar`.

**Two tables are now frozen compatibility surfaces, not just one.** The canonical repertoire spans
the legacy mnemonics, the ASCII-accepted class, **the curated entity set** and `~U<hex>~`. Adding a
name to the curated set later would change the canonical text of that character and so invalidate
derived keys and stored links — the curated set can therefore gain names only with a version bump,
which is the price of the readability decision. `~D ~O ~B` and the non-curated HTML5 names are
decode-only and can grow freely.

**The encoder is infallible.** `encode_token` takes `&str`, which is UTF-8 by construction, so every
`char` in it is a Unicode scalar value and every scalar value has a `~U<hex>~` spelling — lone
surrogates cannot occur in the input. There is no failure case left, so the signature stays
`fn encode_token<S: AsRef<str>>(s: S) -> String`. Fallibility belongs entirely to the **decoder**,
which must reject `~U110000~` (out of range), `~UD800~` (surrogate), `~nfoo~` (unknown name),
`~U~` (empty body) and a missing terminator.

## Named-entity table: an optional feature

**Requirement (maintainer):** the full HTML5 table is an **optional cargo feature**; the curated set
of Annex B is what a build has without it.

```toml
# liquers-core/Cargo.toml
[features]
default = ["async_store"]        # deliberately NOT including entities-html5
entities-html5 = []              # the ~2100-name table; additive
```

**This formulation is what makes the requirement expressible.** The earlier reading — full table
*by default*, restricted by `default-features = false` — cannot work here, and the reason is worth
recording so nobody reintroduces it. Features unify across the dependency graph, and `liquers-web`
reaches `liquers-core` four ways:

```
liquers-web ─┬─────────────────────────────► liquers-core   (Cargo.toml:13, defaults ON)
             ├─ liquers-lib  ──────────────► liquers-core   (Cargo.toml:25, defaults ON)
             ├─ liquers-store ─────────────► liquers-core   (Cargo.toml:11, defaults ON)
             └─ liquers-macro ─────────────► liquers-core
```

`liquers-web` already writes `default-features = false` for `liquers-lib` and `liquers-store`, but
those two pull `liquers-core` with *its* defaults regardless, so anything in that default set is
unavoidable for the wasm bundle. Keeping the table **out of `default`** sidesteps the whole problem:
unification only ever adds, nothing in the wasm graph asks for it, and the table cannot arrive by
accident. No `cfg(target_arch)` and no cross-workspace `default-features = false` edits are needed.

Native crates opt in explicitly. `liquers-axum` (`Cargo.toml:11`) and `liquers-py` (`Cargo.toml:13`)
depend on `liquers-core` directly, so each adds `features = ["entities-html5"]`; `liquers-lib` may
carry it in *its* default, which reaches native consumers while `liquers-web` — which already takes
`liquers-lib` with `default-features = false` — stays lean.

| Build | Names decoded |
|---|---|
| `entities-html5` on (native crates opt in) | curated set + all ≈2100 HTML5 named references |
| default, including every wasm build | curated set (Annex B, ≈205) |

**No crate enables it in a `default`** (D12) — not `liquers-lib`, not `liquers-axum`, not
`liquers-py`. A consumer that wants the full table asks for it, which keeps the feature honest: its
absence is the norm, so the curated set is what the test suite and the documentation treat as
baseline behaviour rather than as a degraded mode.

**Representation** (D11): a concatenated blob plus an offset index, not `&[(&str, &str)]`. The
slice-of-pairs form spends roughly 32 bytes per entry on fat pointers alone — about 95 KB across
≈2100 entries — where a blob with `u32` offsets is nearer 40 KB and keeps the names contiguous for
binary search. Phase 2 confirms both figures against the real WHATWG `entities.json`.

**Testing** (D12): the suite runs in **both** feature states. `cargo test -p liquers-core --lib` and
the same with `--features entities-html5`; the second configuration must show a non-curated name
such as `~nhellip~` decoding, and the first must show it producing the specific diagnostic rather
than a generic parse error. Phase 3 owns the matrix; note that it is a second build configuration,
though a cheap one, since it is confined to `liquers-core`.

**The asymmetry this creates, and its bound.** A hand-written query using a non-curated name — say
`~nhellip~` — decodes where the feature is on and fails where it is off. Machine-generated queries
are unaffected, because the encoder only ever emits curated names and the curated set is compiled
unconditionally: **everything any build encodes, every build decodes.** The gap is bounded to
hand-written exotic names, and the error for one must say so explicitly — "named entity `hellip` is
not available in this build; write `~U2026~`" — rather than reporting a generic parse failure.

## Documentation Intent

**Reference:** extend `specs/reference/api/DOC_02_QUERY_LANGUAGE_REFERENCE.md` §"Action-parameter
entities" — it already owns the normative entity table, and a second reference would split it. Also
update `specs/reference/PROJECT_OVERVIEW.md`, since this is a grammar change (CLAUDE.md).

**Guide: create `specs/guides/QUERY_ESCAPING_GUIDE.md`.** This **reverses the first draft's
"no new guide"**, at the maintainer's request that the feature be documented "in the query guide".
There is no query guide today — `specs/guides/` holds only `COMMAND_REGISTRATION_GUIDE.md`,
`LANGUAGE-INTEGRATION_GUIDE.md`, `UNITTEST_GUIDE.md` and `autonomous_issue_fixing.md`, and
`DOC_02_QUERY_LANGUAGE_REFERENCE.md` is a reference, not a guide. The distinction matters here
because the two documents answer different questions: the reference states *what each entity means*,
while the guide answers *how do I put arbitrary text into a query*, *when do I need the
`entities-html5` feature and how do I turn it on*, and *why did my query stop parsing*. Per
`DOCS_STRUCTURE_GUIDE.md` §2, that is guide material, and the template's instruction to reconsider a
`neither` decision when substantial material accumulates applies exactly here.

**Also update `specs/guides/LANGUAGE-INTEGRATION_GUIDE.md`** — it currently tells integrators to
raise an error for unrepresentable values *because of this issue*, and that paragraph becomes wrong.

**Other documents to create:** None beyond the guide above.

**Specific documents to update:** the two references and `LANGUAGE-INTEGRATION_GUIDE.md`, plus
`specs/issues/PARAMETER-ESCAPING-INCOMPLETE.md` → `closed`, and `specs/README.md` for the new guide.
Audience: anyone writing a query by hand or building one programmatically, in any host language.

**Content requirement (maintainer):** every entity table — in `DOC_02_QUERY_LANGUAGE_REFERENCE.md`,
in the new guide, and in the rustdoc on the tables in `entities.rs` and `escape.rs` — carries an
explicit **"emitted by encoder"** column, so a reader can tell at a glance which spelling is
canonical and which is accepted-but-never-produced. Without it the decode-superset design is
invisible and someone will assume `encode(parse(t)) == t`. The **`entities-html5` feature** is
documented in the same places: which names it adds, that no crate enables it by default, and what
the diagnostic looks like without it.

## Decisions Taken

| # | Decision | By |
|---|---|---|
| D1 | Radix in the opener: `~U` hex, `~D` decimal, `~O` octal, `~B` binary (Annex A-1a) | maintainer |
| D2 | Named entities take the `n` prefix: `~n<name>~` (Annex A-2a) | maintainer |
| D3 | `~<digit>` (compact negative number) stays | maintainer |
| D4 | Every entity table documents which spellings the encoder emits | maintainer |
| D5 | A character with a curated entity is **always** encoded as that entity, even when `~U<hex>~` is shorter — readability wins. The curated set thereby becomes a frozen compatibility surface | maintainer |
| D6 | **The parser accepts any Unicode alphanumeric; the encoder emits pure ASCII.** Widening keeps every currently-parsing query (`f-Ł` parses today); ASCII-only output keeps queries safe through ASCII-only systems | maintainer |
| D7 | The full HTML5 table is an **optional cargo feature** (`entities-html5`), deliberately not in `liquers-core`'s `default`; native crates opt in, every wasm build gets the curated set | maintainer |
| D9 | Latin-1 accented letters stay **out** of the curated set — `café` encodes as `caf~UE9~` | maintainer |
| D10 | `resource_name` narrows to **ASCII alphanumeric** for now, rather than following D6's widening | maintainer |
| D11 | The full table is stored as a **concatenated blob with an offset index**, not `&[(&str, &str)]` | maintainer |
| D12 | `entities-html5` is optional **everywhere** — no crate carries it in a `default`, `liquers-lib` included. It must be **tested in both states** and **documented**, including in the query guide | maintainer |
| D8 | `encode_token` stays **infallible** — `&str` guarantees every `char` is a scalar value, so every input is representable. Fallibility is the decoder's | resolved; the Phase 1 question was unfounded |

D6 also settles the normalization worry: since encoder output is ASCII, canonical text never
contains a composed-or-decomposed choice. Liquers does **not** normalize, so hand-written `café`
(U+00E9) and `café` (`e` + U+0301) remain two different values that encode to two different
canonical strings. That is defensible — they are different strings — but it must be documented
rather than discovered.

## Open Questions

None blocking. Phase 2 carries three measurements and confirmations rather than decisions:

1. Confirm the entity count and the blob-plus-offset payload against the real WHATWG
   `entities.json`, rather than the ≈2100 / ≈40 KB estimates used here.
2. Confirm that `escape.rs` exposing its segment information satisfies the "must not make
   `QUERY-AST-DISCARDS-ENTITIES` harder" constraint in Scope.
3. Settle where the two-feature-state test matrix runs, given the workspace's build-size
   constraints (CLAUDE.md, "Building and testing").

### Resolved since the first draft

| Was | Resolution |
|---|---|
| Does `liquers-lib` carry `entities-html5` in its default? | **No** (D12) — optional everywhere, no crate defaults it on |
| Full-table representation | **Blob plus offset index** (D11) |
| Does the encoder emit curated named entities, or numeric only? | **Curated names always** (D5) — readability wins; the curated set becomes a frozen compatibility surface |
| `c as u8`: widen or narrow? | **Both** — parameters widen to Unicode alphanumerics, resource names narrow to ASCII (D6, D10) |
| Do Latin-1 accented letters join the curated set? | **No** (D9) — `café` encodes as `caf~UE9~` |
| Full table by default, restricted by `default-features = false`? | Replaced by an **optional feature** (D7); the original form is defeated by feature unification |
| Is Annex B's tiering right? | **Yes**, confirmed by the maintainer |
| Should `encode_token` become fallible? | **No** (D8) — every `&str` is representable |

### Deferred, and filed

- **Non-ASCII resource names.** D10 makes `resource_name` ASCII-only "for now", so a stored file
  called `café.csv` remains unaddressable. It already is today — `-R/data/café.csv` does not parse —
  but D10 also *narrows*: `-R/data/ŁŁ.csv` parses at HEAD and will stop, because `Ł`'s low byte is
  `0x41`. Filed as `RESOURCE-NAME-ASCII-ONLY`.
- **Entities in the AST** — `QUERY-AST-DISCARDS-ENTITIES`, filed earlier, out of scope here.

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
| **2a** | **Prefix letter** — `~n<name>~` | `~namp~` | **Chosen**, with `n` for "named". Deterministic, no lookahead, provably backward compatible. `x` and `e` were the other free candidates; `n` avoids both the hex connotation of `x` and the confusability of `e` with `~E`. |
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

- `~ntilde~` is **not** `~`. `&tilde;` is U+02DC ˜ (small tilde) and `&Tilde;` is U+223C ∼; ASCII
  `~` U+007E has no HTML5 name. Use `~~`.
- `~nhyphen~` is **not** `-`. `&hyphen;` and `&dash;` are both U+2010 ‐; ASCII hyphen-minus U+002D
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

Accented letters (`eacute`, `uuml`, …) are **excluded** — see Open Question 1, which is now the
live form of this question. The original reason was that a widened parser makes them literal; D6
only half-holds that, since they parse literally but are always escaped on encode. The exclusion
therefore decides whether `café` encodes as `caf~UE9~` (recommended) or `caf~neacute~`.

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

≈205 of ≈2100, so roughly a tenth of the full table's payload. Under D5 the encoder emits these names, so **this set is a frozen compatibility surface**: cutting
or extending a tier after release changes the canonical text of the affected characters and
invalidates derived keys and stored links. The tiers are therefore a decision to take now, not a
knob to turn later — which is the opposite of the earlier reading, where the set was decode-only and
free to move. Nothing is *unrepresentable* either way: every character omitted from the set remains
writable as `~U<hex>~`.
