---
id: PARAMETER-ESCAPING-INCOMPLETE
kind: issue
title: Action-parameter escaping cannot express every value
status: accepted
priority: P0
complexity: M
area: [core/query]
design: 
created: 2026-08-08
github:
---
## Problem

Most characters cannot appear in a string action parameter at all, and `encode_token` silently
produces text that the parser rejects rather than escaping them or reporting failure. **A general
character-escaping mechanism, covering the full Unicode range, is missing.**

Three separate defects, in increasing order of severity.

**1. `encode_token` is not round-trip safe.**
`liquers_core::query::encode_token` (`liquers-core/src/query.rs:503`) escapes exactly four
characters — `~`, space, `/`, `-` — and passes everything else through unchanged. The parser's
unescaped parameter set (`liquers-core/src/parse.rs:340`) accepts only ASCII alphanumerics plus `_`,
`+` and `.`. Everything in the gap encodes to unparseable text, with no error at encode time.

Measured — every one of these is **rejected** by the parser, and every one is what `encode_token`
emits verbatim:

```
a:b   a?b   a,b   a=b   a&b   a(b   a%b   a#b   a!b   a@b   a*b   a;b   a[b   café   日本
```

The failure is silent and deferred: encoding succeeds, and the resulting query fails to parse later,
somewhere else.

**2. No escape exists for most characters, so no correct encoder is currently possible.**
The entity table (`parse.rs:386-399`) provides `~~` `~_` `~<digit>` `~.` `~I` `~/` `~h` `~H` `~f`
`~P`. A URL is expressible by hand — `f-~Hapi.example.com~/data` parses and decodes to exactly
`https://api.example.com/data` — but there is no entity for a **lone colon**, nor for `?`, `=`, `&`,
`#`, `,`, `(`, `)`, `%`, or any other punctuation, **nor for any non-ASCII character**. So a value
such as `12:30`, `a,b`, or `café` cannot be represented by any encoder, however correct. This is why
defect 1 cannot simply be fixed inside `encode_token`.

**3. Unicode acceptance is incoherent, because the character test truncates the code point.**
`parse.rs:340` tests `AsChar::is_alphanum(c as u8)`, where `c` is a `char`. The `as u8` cast keeps
only the low byte, so whether a non-ASCII character is accepted depends on its code point modulo
256. Measured:

| Char | Code point | Low byte | Parser |
|---|---|---|---|
| `Ł` | U+0141 | 0x41 = `A` | **accepted** |
| `Ő` | U+0150 | 0x50 = `P` | **accepted** |
| `ŗ` | U+0157 | 0x57 = `W` | **accepted** |
| `é` | U+00E9 | 0xE9 | rejected |
| `ā` | U+0101 | 0x01 | rejected |
| `Ā` | U+0100 | 0x00 | rejected |

`Ł` is accepted while `é` is not, for no reason a user could infer. Any character whose low byte
happens to land in `[0-9A-Za-z]` slips through, which is arbitrary and almost certainly unintended.

## Impact

Wider than a helper function. `encode_token` is used by `Query`'s own encoder
(`query.rs:609`, `:615`, `:620`, `:631`, `:646`, including `StyledQueryToken::StringParameter`), so
**encode → parse is not a round trip** for any programmatically constructed query. Affected: query
builders in UI code, links, recipes, asset keys derived from user data, and every language
integration that accepts a string parameter from its host language. Non-English text is
unrepresentable in a parameter, which makes this an internationalization defect and not only an
escaping one.

## Intended solution

**An HTML/XML-like entity system, with `~` as the escape character**, supporting both *named*
entities and *numeric* entities in hexadecimal, octal or another radix — the role `&amp;` and
`&#x41;` play in XML. `~` is already the established escape character in the query grammar and this
extends it rather than introducing a second mechanism.

The natural extension point is the `entities` combinator in
`liquers_core::parse` (`parse.rs:386`, the `alt((tilde_entity, minus_entity, …))` list) together
with its encoder counterpart, `encode_token`. **`liquers-core/src/entities.rs` is the intended home
for the consolidated table**, so both directions derive from one definition.

**This requires a proper design** — a `liquers-designer` run, not an incremental patch. The grammar
constraints below are already known and are what make it non-trivial:

- **Existing entities are terminator-free and fixed-length** (`~~`, `~_`, `~.`, `~I`, `~/`, `~h`,
  `~H`, `~f`, `~P`). A variable-length named or numeric entity needs a terminator. `;` is a natural
  choice and is currently *not* in the accepted character set, so it cannot occur unescaped — but
  admitting it is itself a grammar change.
- **`~<digit>` already means a negative number.** `negative_number_entity` (`parse.rs:378`) parses
  `~42` as `-42`. Any numeric-entity syntax of the shape `~<digits>` collides with it directly. This
  is the sharpest constraint on the design: either numeric entities take a distinguishing prefix
  (`~#41;`, `~x41;`, `~u0041;`), or the compact negative-number form is retired with a migration
  path.
- **`alt` is ordered**, so a named-entity parser added to the list must not shadow the existing
  short forms — a named entity beginning `h` must not capture `~h` (`http://`).
- **Backward compatibility is required.** Every query and recipe already written must keep parsing
  and keep meaning the same thing. The existing mnemonics stay as compact special cases layered over
  the general mechanism.

## Expected behavior

1. **A general escaping feature covering all of Unicode**, per the entity design above. Any `char`
   must be representable in a string parameter, so that `encode_token(s)` round-trips for every `s`.
2. `encode_token` emits those escapes, and emits the mnemonic entities where they apply so URLs
   still encode to the compact `~H…` form.
3. The `c as u8` truncation at `parse.rs:340` is replaced with a decision made deliberately: either
   accept Unicode alphanumerics properly (`char::is_alphanumeric`) or restrict to ASCII explicitly
   (`c.is_ascii_alphanumeric()`) and let escaping carry the rest. The current behaviour is neither.
4. A round-trip property test over a generated character set, including astral-plane code points,
   guards all three.

**The placeholder module is `liquers-core/src/entities.rs`.** It exists, is empty (0 lines), and is
not declared in `lib.rs` — it must be added to the module list when filled. The entity table
currently lives inline in `parse.rs` (`tilde_entity`, `minus_entity`, `slash_entity`,
`https_entity`, … and the `entities` combinator) with its encoder counterpart in `query.rs`; the two
are separated by a whole module and drift silently, which is the structural reason this defect went
unnoticed. Consolidating both directions into `entities.rs` — one table, one encoder, one parser,
one round-trip test — is the natural fix.

Item 1 and item 3 are grammar changes and affect the encoding description in
`specs/reference/PROJECT_OVERVIEW.md`.

## Related code

| Location | Role |
|---|---|
| `liquers-core/src/entities.rs` | **empty placeholder** — intended home for the consolidated table, not yet in `lib.rs` |
| `liquers-core/src/parse.rs:340` | unescaped character class, with the `as u8` truncation |
| `liquers-core/src/parse.rs:345-377` | the fixed-length entity parsers |
| `liquers-core/src/parse.rs:378` | `negative_number_entity` — the `~<digit>` form that collides with numeric entities |
| `liquers-core/src/parse.rs:386` | the `entities` combinator — **the extension point** |
| `liquers-core/src/query.rs:503` | `encode_token`, the encoder half |
| `liquers-core/src/query.rs:609-646` | `Query` encoding paths that depend on it |
| `specs/reference/PROJECT_OVERVIEW.md` | documents query encoding; needs updating with the outcome |

## Discovery

Found while validating a query for an example in `specs/design/liquers-web/phase3-examples.md`. The example
had assumed percent-encoding, which the grammar does not support in any form; checking the real
mechanism surfaced the encoder defect, and probing its boundaries surfaced the missing Unicode
escapes and the truncation bug. Originally filed as `ENCODE-TOKEN-COLON`, renamed once the colon
turned out to be only the presenting symptom.
