---
title: Query Escaping Guide
kind: guide
audience: both
area: [core/query]
reviewed: 2026-08-14
---
# Getting arbitrary text into a query

A Liquers query is text with structure: `/` separates path segments, `-` separates action
parameters, `~` escapes. So a value that contains any of those — or a colon, or a comma, or a
letter outside ASCII — cannot simply be pasted in.

This guide is the **how**. For what each entity means, see
[DOC-02 §Action-parameter entities](../reference/api/DOC_02_QUERY_LANGUAGE_REFERENCE.md#action-parameter-entities),
which is normative.

## Escaping a value programmatically

**Do not build query text by hand, and do not write an escaper in your host language.** Build the
query and call `encode()`:

```rust
use liquers_core::query::{ActionParameter, ActionRequest};

let action = ActionRequest::new("filter".to_owned())
    .with_parameters(vec![ActionParameter::new_string("12:30".to_owned())]);

assert_eq!(action.encode(), "filter-12~ncolon~30");
```

The value goes in **raw**. A parameter holds the *decoded* value — the string you mean, not query
text — and escaping happens only in `encode`. Nothing needs pre-escaping, and pre-escaping is a
bug: it would be escaped a second time.

Every string works, including ones that used to be impossible:

| Value | Encodes to |
|---|---|
| `12:30` | `12~ncolon~30` |
| `a,b` | `a~ncomma~b` |
| `hello world` | `hello~.world` |
| `-5` | `~5` |
| `café` | `caf~UE9~` |
| `日本` | `~U65E5~~U672C~` |
| `😀` | `~U1F600~` |
| `https://api.example.com/data` | `~Hapi.example.com~/data` |
| `~X~` | `~~X~~` |

`encode_token` never fails, so there is no "unrepresentable value" path to write. The last row
matters if you are passing user data through: text that looks like a link marker stays a string and
cannot re-parse as a link.

Executable evidence: [`action_parameter_invariant.rs`](../../liquers-core/tests/action_parameter_invariant.rs).

## Writing an entity by hand

For a query typed into a URL bar, a recipe or a test fixture.

```text
filter-12~ncolon~30           a named entity
filter-12~U3A~30              the same character, numeric
filter-~U1F600~               an astral-plane code point
filter-~D128512~              the same, decimal
filter-~O373000~              octal
filter-~B11111011000000000~   binary
filter-12~n~colon~30          a separator tilde, if it reads better
```

All of them decode identically. Only the first is what `encode()` produces.

| Form | Meaning |
|---|---|
| `~U<hex>~` | code point, hexadecimal — the canonical numeric form |
| `~D<dec>~` `~O<oct>~` `~B<bin>~` | the same, in another radix |
| `~n<name>~` | an HTML5 named entity, case-sensitive |
| `~~` `~_` `~.` `~/` | `~` `-` space `/` |
| `~h` `~H` `~f` `~P` | `http://` `https://` `file://` `://` |
| `~<digits>` | `-` followed by those digits |

Leading zeros are accepted (`~U0041~` is `A`), and so is an optional separator tilde after the
opener (`~U~41~`). Neither is emitted.

## Three things that will catch you out

### `~ntilde~` is not `~`, and `~nhyphen~` is not `-`

HTML5 has no name for ASCII `~` (U+007E) or ASCII `-` (U+002D). The names that look right are
different characters — `tilde` is `˜` U+02DC, `Tilde` is `∼` U+223C, `hyphen` and `dash` are `‐`
U+2010.

**Write `~~` and `~_`.** Neither trap name is curated, so a default build rejects them rather than
quietly decoding to a look-alike; with `entities-html5` they decode to the characters above, and
you get a valid query meaning something subtly different. That is the worst failure available here,
which is why it has its own section.

### `~~` in the middle of two entities is not an escaped tilde

```text
filter-~U65E5~~U672C~      日本
```

The `~~` between them is the first entity's terminator abutting the second's opener. The parse is
deterministic — after an opener the body runs to the first `~` — but any non-ASCII text produces
this, and it reads badly.

### A curated name is used even when the numeric form is shorter

`:` encodes as `~ncolon~` (8 characters), not `~U3A~` (5). Readability wins, deliberately. It cuts
both ways: `π` is `~npi~` (5) against `~U3C0~` (6).

Characters accepted unescaped stay literal even when they have a name — `+`, `.` and `_` never
become `~nplus~`, `~nperiod~`, `~nlowbar~` — and `/` stays `~/` rather than `~nsol~`.

## When you need the `entities-html5` feature

A default build decodes the **curated** 203 names: the ASCII punctuation the grammar rejects, plus
Latin-1, typography, Greek, and common mathematics and arrows. The full HTML5 table is 2125 names.

```toml
liquers-core = { version = "…", features = ["entities-html5"] }
```

No crate enables it by default, including `liquers-lib`, `liquers-axum` and `liquers-py`. Turn it
on if you want to *write* exotic names by hand.

**You do not need it to read anything Liquers produced.** The encoder emits only curated names, and
the curated table is compiled unconditionally, so everything any build encodes, every build
decodes — which is why `liquers-web` can ship without the extra ~1922 names.

Without the feature:

```text
filter-~ncheck~
  ~ncheck~ is not a named entity available in this build. This build decodes 203 names;
  enable the `entities-html5` feature for the full HTML5 table, or write the character
  numerically as ~U<hex>~.
```

## Why my query stopped parsing

Entity errors point at the offending `~`, so the column in the message is the one to look at.

| Message | Cause | Fix |
|---|---|---|
| beyond the maximum code point U+10FFFF | `~U110000~` | check the number |
| a surrogate, which is not a Unicode scalar value | `~UD800~` | surrogates cannot appear in a string |
| empty entity body | `~U~`, `~n~` | put the digits or name between opener and `~` |
| not a valid base-16 number | `~Uzz~` | wrong radix, or a typo — `~D` is decimal |
| not a named entity available in this build | `~nfoo~` | typo, or enable `entities-html5` |
| is not terminated; a `~` must close it | `~U41` | add the closing `~` |

Two more, if the query has nothing obviously wrong with it:

- **A resource name with a non-ASCII character** does not parse: `-R/data/café.csv` is rejected, and
  resource names have no entity production to fall back on. See
  [`RESOURCE-NAME-ASCII-ONLY`](../issues/RESOURCE-NAME-ASCII-ONLY.md).
- **A name made of characters like `Ł`** used to parse by accident and no longer does. `Ł` is
  U+0141, whose low byte is `0x41`; the old character test truncated to that byte. The narrowing is
  deliberate.

Executable evidence: [`entity_parse_errors.rs`](../../liquers-core/tests/entity_parse_errors.rs).

## What the encoder emits, and why it is stable

At each position, in this fixed order:

1. the longest matching **mnemonic** — `https://` → `~H`, `-4` → `~4`, `/` → `~/`;
2. the character **literally**, if ASCII-accepted `[A-Za-z0-9_+.]`;
3. the **curated named entity** — `:` → `~ncolon~`;
4. `~U<hex>~`, uppercase digits, no leading zeros.

Output is pure ASCII, so a query survives transport through ASCII-only systems, even though the
*parser* accepts Unicode alphanumerics — `filter-café` parses, it just is not what `encode` writes.

The priority is fixed rather than chosen by length, because **query text is identity** in Liquers:
asset keys, cache keys and stored links are query strings, so changing a spelling would invalidate
everything derived from it. Two consequences worth knowing:

- **`encode(parse(t)) == t` is false**, and is not meant to be true. The decoder accepts a superset
  — `~I` and `~/` both give `/` — so re-encoding normalises to the canonical spelling.
- **Canonicalisation is idempotent.** With `E = encode ∘ parse`, `E(E(t)) == E(t)`: one pass reaches
  canonical text and further passes change nothing. The resource/transform shorthand shows why
  idempotence rather than identity is the property — `a/b/-/c/d` canonicalises to `-R/a/b/-/c/d`,
  and stays there.

Executable evidence: [`query_backward_compatibility.rs`](../../liquers-core/tests/query_backward_compatibility.rs)
and the property tests in [`escape/tests.rs`](../../liquers-core/src/escape/tests.rs).

## History

| Date | Change | Source |
|---|---|---|
| 2026-08-14 | Created alongside the numeric and named entity mechanism. | PARAMETER-ESCAPING-INCOMPLETE |
