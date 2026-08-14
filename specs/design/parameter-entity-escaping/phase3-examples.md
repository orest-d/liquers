# Phase 3: Examples & Use-cases - Parameter Entity Escaping

## High-Level Introduction

Phase 1's purpose is that **any string can be carried in an action parameter**, and Phase 2's
invariant is that **a caller supplies an arbitrary value and encoding happens only at `encode()`**.
The examples below follow that order: the primary scenario is the thing the issue is actually about
— building a query from a value you did not choose — and it needs no knowledge of entities at all.
The second scenario is for someone reading or writing a query by hand, where the entity spellings
and the `entities-html5` feature become visible. The third collects the ways this can still go
wrong, all of which are traps inherited from HTML rather than invented here.

The test plan then splits along the same seam: unit tests pin exact spellings and exact error
positions, and three property tests carry the general claims.

## Example Type

**Conceptual, written against the Phase 2 signatures** — the code does not exist yet, so nothing
here is runnable as Rust. That is a limitation of timing, not of rigour, so the design's *claims*
were made runnable instead: a working prototype of the encoder and decoder (§"Prototype evidence")
was built and the three properties executed over 4 130 inputs. Every expected value in this
document is **generated output from that prototype**, not a hand-computed guess, and the legacy
spellings were additionally checked against the real parser.

## Overview Table

| # | Example / test | What it demonstrates or checks |
|---|---|---|
| **E1** | Programmatic construction from an arbitrary value | The invariant: build with a raw string, encode at the boundary. The issue's motivating case, `12:30` |
| **E2** | Reading and writing entities by hand | The four numeric radixes, named entities, the separator tilde, and when `entities-html5` is needed |
| **E3** | Pitfalls | `~ntilde~` ≠ `~`, `~nhyphen~` ≠ `-`, adjacent entities that look like `~~`, non-curated names on a default build, and the `set_value` trap being removed |
| **T1** | `encode_token` spelling table | Exact canonical output per character class and priority step |
| **T2** | `decode_token` acceptance | Every spelling the decoder accepts, including decode-only forms |
| **T3** | Entity error messages **and positions** | Each malformed entity reports the right message at the right column |
| **T4** | Character-class change | Parameters widen to Unicode alphanumerics; resource names narrow to ASCII |
| **T5** | The `~<digit>` and mnemonic interactions | `-5` → `~5`, URLs keep `~H`, `://` keeps `~P` |
| **T6** | Feature matrix | The same suite with and without `entities-html5`, including the diagnostic difference |
| **T7** | Opener disjointness | The long-form openers do not intersect the legacy short set — the property backward compatibility rests on |
| **T8** | Backward compatibility corpus | Every query in the repo's tests, examples and recipes still parses and still means the same |
| **P1** | Round trip | `decode(encode(s)) == s` for every generated `s` |
| **P2** | Idempotence | `E(E(t)) == E(t)` where `E = encode ∘ parse` |
| **P3** | ASCII output | `encode_token(s).is_ascii()` for every `s` |
| **I1** | `ActionParameter` invariant | Every constructor, setter and reader against the invariant, including `set_value` |
| **I2** | `liquers-web` delegation | `encode_param` accepts what it used to reject, and its output parses in a curated-only build |
| **I3** | Full-query round trip | Whole `Query` values, not just tokens, including links and nesting |

## Example 1: Building a query from a value you did not choose

### Connection to the High-Level Design

This is the issue. A UI, a recipe generator or a language binding has a value — a time, a label, a
name in a language other than English — and must put it into a query. Today that silently produces
text that fails to parse later, somewhere else. Phase 1's purpose is exactly that this works.

### Scenario

A dashboard filters a dataset by a timestamp the user typed, `12:30`, and renders a link.

### Sequence of Steps

1. The caller builds an `ActionRequest` with the **raw** value. No escaping, no knowledge of the
   grammar — the invariant says the parameter holds the decoded value.
2. `Query::encode()` produces query text. This is the only point where escaping happens.
3. The text round-trips: parsing it returns a query equal to the one built.

### Core Example Code

```rust
use liquers_core::query::{ActionParameter, ActionRequest};
use liquers_core::parse::parse_query;

// 1. Raw value in. `12:30` was previously unrepresentable by any correct encoder.
let action = ActionRequest::new("filter".to_owned())
    .with_parameters(vec![ActionParameter::new_string("12:30".to_owned())]);

// 2. Escaping happens here and nowhere else.
let text = action.encode();
assert_eq!(text, "filter-12~ncolon~30");

// 3. Round trip.
let reparsed = parse_query(&text)?;
assert_eq!(
    reparsed.action().expect("action").parameters[0].string_value(),
    Some("12:30".to_owned())
);
# Ok::<(), liquers_core::error::Error>(())
```

Nothing in this example mentions an entity. That is the point: `~ncolon~` is an implementation
detail of `encode()`, and a caller who never reads a query does not need to know it exists.

The same values, all previously impossible, all generated by the prototype:

| Value | Encodes to |
|---|---|
| `12:30` | `12~ncolon~30` |
| `a,b` | `a~ncomma~b` |
| `café` | `caf~UE9~` |
| `日本` | `~U65E5~~U672C~` |
| `😀` | `~U1F600~` |
| `~X~` | `~~X~~` |

The last row closes a hole documented at `parse.rs:117`: a programmatically set token containing
`~X~` currently "may re-parse as a *different valid query* rather than failing". It cannot any more,
because `~` is escaped.

### Guide and Executable Example

Goes into `QUERY_ESCAPING_GUIDE.md` §"Escaping a value programmatically" as the opening example.
The executable evidence is **I1** and **I3**.

## Example 2: Reading and writing entities by hand

### Connection to the High-Level Design

The other half of Phase 1: a person writing a query in a URL bar, a recipe or a test fixture. Here
the spellings are the interface, and D13's separator tilde and D7's feature both become visible.

### Scenario

Writing a query by hand that filters on a label containing a colon and an ellipsis.

```text
filter-12~ncolon~30              a curated name
filter-12~U3A~30                 the same character, numeric — accepted, never emitted
filter-12~n~colon~30             the same again, with D13's separator tilde
filter-~U1F600~                  astral plane
filter-~D128512~                 the same code point in decimal
filter-~O373000~                 the same, octal
filter-~B11111011000000000~      the same, binary
```

All six decode identically to their canonical form; only the first is what `encode()` produces.

### The four radixes and the separator

| Spelling | Decodes to | Emitted |
|---|---|---|
| `~U41~` | `A` | **yes — canonical** |
| `~U0041~` | `A` | no — leading zeros accepted, never emitted |
| `~D65~` `~O101~` `~B1000001~` | `A` | no |
| `~U~41~` | `A` | no — D13 separator |
| `~namp~` | `&` | **yes** |
| `~n~amp~` | `&` | no — D13 separator |
| `~nlbrack~` | `[` | no — synonym of `~nlsqb~` |

**Canonical numeric form, pinned here because it must never drift:** uppercase hex digits, no
leading zeros. `~U1F600~`, not `~U1f600~` and not `~U01F600~`. The decoder accepts all three.

### When `entities-html5` is needed

```text
filter-~nhellip~
```

- Default build: **error** — `named entity 'hellip' is not available in this build; enable the
  entities-html5 feature, or write ~U2026~`
- With `entities-html5`: decodes to `…`

`encode_token` never produces `~nhellip~` — it emits `~U2026~` — so this only arises for
hand-written text. That is the bound Phase 1 puts on the asymmetry, and **T6** asserts it.

### Guide and Executable Example

`QUERY_ESCAPING_GUIDE.md` §"Writing an entity by hand" and §"When you need entities-html5".
Executable evidence: **T2** and **T6**.

## Example 3: Pitfalls and edge cases

### 1. `~ntilde~` is not `~`, and `~nhyphen~` is not `-`

The worst failure mode available here, because both produce a **valid query that means something
subtly different** rather than an error.

```text
filter-~ntilde~     →  ˜  U+02DC SMALL TILDE      not  ~
filter-~nhyphen~    →  ‐  U+2010 HYPHEN           not  -
```

HTML5 has no name for ASCII `~` (U+007E) or ASCII `-` (U+002D); `&tilde;` and `&Tilde;` are U+02DC
and U+223C, and `&hyphen;`/`&dash;` are U+2010. Write `~~` and `~_`, which is what `encode_token`
emits anyway.

### 2. A curated name is used even when numeric is shorter

`:` encodes as `~ncolon~` (8 characters), not `~U3A~` (5). This is D5, and it is deliberate — but it
means someone comparing encoded lengths will find the encoder "wasteful". The counter-case is real
too: `π` is `~npi~` (5) against `~U3C0~` (6), so it cuts both ways.

### 3. Literal beats curated

`+`, `.` and `_` have the curated names `plus`, `period` and `lowbar` but encode as themselves,
because they are accepted unescaped. Likewise `/` stays `~/` rather than `~nsol~`, and `://` stays
`~P`. **T5** pins these.

### 4. Adjacent entities look like an escaped tilde, and are not

```text
filter-~U65E5~~U672C~     →  日本
```

The `~~` in the middle is *not* the escaped-tilde entity: the first `~` is the terminator of
`~U65E5~` and the second opens `~U672C~`. The parse is deterministic — after an opener the body runs
to the first `~`, which is always the terminator — but it reads badly, and any non-ASCII text
produces it. Worth a line in the guide, since a reader debugging by eye will trip on it.

### 5. The `set_value` trap, and its removal

Before this design:

```rust
let mut p = ActionParameter::new_string(String::new());
p.set_value("a b");
p.string_value();   // Some("a~.b")  — not what was set
p.encode();         // "a~~.b"       — escaped twice
```

After, both return what was set. **I1** covers it. This is
`ACTION-PARAMETER-SET-VALUE-DOUBLE-ENCODES`, fixed here.

### 6. `-R/data/ŁŁ.csv` stops parsing

D10 narrows resource names to ASCII. `Ł`'s low byte is `0x41`, so names made entirely of such
characters parse at HEAD and will not after. Deliberate, and the only intended behaviour regression
in this change. **T4** and **T8** cover it; `RESOURCE-NAME-ASCII-ONLY` records the limitation.

## Corner Cases

### 1. Memory

No allocation in the tables: `EntityTable` is `&'static` blobs and offset slices, resolved by binary
search. `encode_token` allocates one `String`; `decode_token` allocates one. `TokenSegment` borrows
from its input and uses `Cow` so a mnemonic or named entity yields `Cow::Borrowed(&'static str)`
with no allocation — only numeric entities allocate, and only a single `char`'s worth.

**Check:** the full table adds nothing to a default build, since `entities-html5` is off. **T6**
asserts `entities::compiled_count()` differs between the two configurations.

### 2. Concurrency

Nothing is shared or mutable. Every table is `&'static`, every function is a pure transformation
over `&str`, and there is no interior mutability, no `OnceLock` and no lazy initialisation. The
module is `Send + Sync` by construction and there is nothing to test for races.

### 3. Errors

Every error is a `liquers_core::error::Error` from a typed constructor. **T3** asserts message *and*
position for each:

| Input (as `f-<token>`) | Error | Position |
|---|---|---|
| `~U110000~` | code point out of range | the `~` |
| `~UD800~` | surrogate | the `~` |
| `~U~` | empty entity body | the `~` |
| `~Uzz~` | bad hexadecimal digit | the `~` |
| `~nfoo~` | unknown named entity `foo` | the `~` |
| `~nhellip~` (default build) | names the `entities-html5` feature | the `~` |
| `~U41` | unterminated: expected `~` | where `~` was expected |

Positions are not assumed. Measured at HEAD, nom reports the span the failure was raised with
exactly — `a-x~X~q~E` already reports offset 3, the `~X~` itself — so the entity parser captures the
span at the opening `~` and raises with it. **T3 includes one case inside a nested link**, where the
position must still be absolute, since `link_query` parses on the original span rather than a slice.

### 4. Serialization

`ActionParameter` derives `Serialize, Deserialize`, and this design does not change its shape, so
existing serialized queries deserialize unchanged. The **stored** form is the decoded value, so a
serialized `ActionParameter::String` holds the raw value — worth an explicit test, because a reader
might expect the encoded form.

`Query`'s `Hash` and `PartialEq` are unaffected: they compare `segments`, and `ActionParameter`'s
`PartialEq` compares the stored string ignoring `Position` (`query.rs:672`). That is what makes
`parse(encode(v)) == v` a well-formed claim.

### 5. Integration (Cross-Crate Interactions)

| Crate | Interaction |
|---|---|
| `liquers-web` | `encode_param` delegates; the workaround and its "still broken" test are deleted (**I2**) |
| `liquers-py` | **No change, and gains correctness** — it already passes a raw Python `String` to `new_string` (`liquers-py/src/query.rs:66`), which is exactly the invariant |
| `liquers-lib` | No change. `registry_export` must stay green untouched, since no command signature changes |
| `liquers-store` | No change; keys are unaffected because resource names gain no entities |

## Documentation and Learning Log

### Guide Candidate Workflows and Examples

| Guide section | Source | Executable evidence |
|---|---|---|
| Escaping a value programmatically | E1 | I1, I3 |
| Writing an entity by hand | E2 | T2 |
| The two HTML traps | E3.1 | T2 |
| When you need `entities-html5` | E2 | T6 |
| Why my query stopped parsing | Corner case 3 | T3 |
| What the encoder emits, and why it is stable | E1 table, E3.2-3 | T1, P2 |

### Usage, Meaning, and Connections

The encoder's priority order is the thing to explain, because every surprising output follows from
it: mnemonic, then literal, then curated name, then hex. Both "why is `:` eight characters" and "why
is `/` not `~nsol~`" are answered by one rule.

### Corrections and Unexpected Learning

- **Today's `E` is not merely non-idempotent, it is undefined.** Measured: the current encoder
  turns `f-~Hapi.example.com~/data` into `f-https:~/~/api.example.com~/data`, which does **not**
  parse, so `E(E(t))` does not exist. Similarly `f-~P` re-encodes to `f-:~/~/`. The design does not
  improve idempotence; it makes it exist.
- **The mnemonic-emission requirement is load-bearing**, not cosmetic. Without it the encoder emits
  a literal `:` and the output fails to parse — which is defect 1 of the issue, in miniature.

## Test Plan

### Unit Tests

In `liquers-core/src/escape.rs` and `entities.rs`, `#[cfg(test)] mod tests`.

**T1 — canonical spelling.** Table-driven over the values in Example 1 plus one representative per
priority step. Values are the prototype's output, so this test encodes a verified expectation:

```rust
#[test]
fn encode_token_canonical_spellings() {
    for (value, expected) in [
        ("12:30", "12~ncolon~30"),
        ("a,b", "a~ncomma~b"),
        ("café", "caf~UE9~"),
        ("日本", "~U65E5~~U672C~"),
        ("😀", "~U1F600~"),
        ("-5", "~5"),
        ("hello world", "hello~.world"),
        ("https://api.example.com/data", "~Hapi.example.com~/data"),
        ("://", "~P"),
        ("/", "~/"),
        ("+", "+"),
        ("~X~", "~~X~~"),
        ("°", "~ndeg~"),
    ] {
        assert_eq!(encode_token(value), expected, "encoding {value:?}");
    }
}
```

**T2 — decoder acceptance**, including every spelling the encoder never emits: `~U0041~`,
`~U~41~`, `~D65~`, `~O101~`, `~B1000001~`, `~n~amp~`, `~nlbrack~`, `~I`, and the two traps
`~ntilde~` → `˜`, `~nhyphen~` → `‐`.

**T3 — errors and positions.** One case per row of Corner Case 3, asserting `ErrorType`, a message
substring, and `position.offset`. Plus the nested-link case.

**T4 — character classes.** `is_unescaped_parameter_char('π')` is true and `('°')` is false;
`is_resource_name_char('Ł')` is false. At parser level: `f-café` parses (widening) and
`-R/data/ŁŁ.csv` does not (narrowing).

**T5 — mnemonic and `~<digit>` interactions**, as in Example 3.3.

**T7 — opener disjointness.** The property backward compatibility rests on, asserted rather than
trusted: the long-form opener set `{U, D, O, B, n}` does not intersect the legacy set
`{~, _, ., /, I, h, H, f, P, X, E, 0-9}`. A one-line set test that fails loudly if someone adds an
opener that collides.

### Integration Tests

**I1 — the invariant**, in `liquers-core/tests/action_parameter_invariant.rs`:

```rust
#[test]
fn stored_value_is_always_the_decoded_value() -> Result<(), Box<dyn std::error::Error>> {
    for v in ["a b", "12:30", "~X~", "café", "", "-5"] {
        // constructor
        let p = ActionParameter::new_string(v.to_owned());
        assert_eq!(p.string_value(), Some(v.to_owned()));

        // setter — this is ACTION-PARAMETER-SET-VALUE-DOUBLE-ENCODES
        let mut q = ActionParameter::new_string(String::new());
        q.set_value(v);
        assert_eq!(q.string_value(), Some(v.to_owned()));
        assert_eq!(q.encode(), p.encode());
    }
    Ok(())
}
```

**I2 — `liquers-web`.** `encode_param("12:30")` and `encode_param("café")` now succeed, where the
existing tests assert they fail. Runs under the wasm loop
(`cargo test -p liquers-web --target wasm32-unknown-unknown`), and asserts that the output parses in
a build **without** `entities-html5` — the consistency claim that bounds the asymmetry.

**I3 — full-query round trip**, over whole `Query` values including a link parameter and a nested
link, not just isolated tokens.

**T8 — backward-compatibility corpus.** Collect every query literal in the repository's tests,
examples, doc comments and `recipes.yaml` files; assert each still parses and yields the same
`Query`. This is what catches an accidental grammar narrowing, and it is the test that should have
existed before the `c as u8` truncation was introduced. `-R/data/ŁŁ.csv`-shaped cases are the
expected and only failures.

### Property Tests

Run over a generated corpus, no external crate needed — a seeded PRNG over a character pool of
ASCII punctuation, `~`, `/`, `-`, space, Latin-1, CJK and astral code points, plus a fixed list of
adversarial strings.

| | Property |
|---|---|
| **P1** | `decode_token(&encode_token(s)) == s` |
| **P2** | `E(E(t)) == E(t)` where `E = encode ∘ parse`, over *valid query text* — including decode-only spellings, which must collapse to canonical |
| **P3** | `encode_token(s).is_ascii()` |

### Feature Matrix (T6)

```bash
cargo test -p liquers-core --lib                            # curated only — the default
cargo test -p liquers-core --lib --features entities-html5  # full table
```

Both configurations run the whole suite. The differences asserted: `entities::compiled_count()`,
`~nhellip~` decoding versus producing the feature-naming diagnostic, and `encode_token` output being
**identical** in both — the check that canonical text does not depend on a cargo feature.

Cost: a second `liquers-core`-only build, which is the cheap end of the workspace (CLAUDE.md,
"Building and testing").

### Manual Validation

```bash
cargo run -p liquers-core --features cli --bin liquers-validate -- -- 'filter-12~ncolon~30'
cargo run -p liquers-core --features cli --bin liquers-validate -- -- 'filter-~U1F600~'
cargo run -p liquers-core --features cli --bin liquers-validate -- -- 'filter-~nhellip~'   # expect the feature diagnostic
```

## Prototype Evidence

The design's three general claims were executed rather than argued. A prototype of the encoder and
decoder — the priority order, the four radixes, the separator tilde, the curated table of Annex B —
was run over a corpus of **4 130 inputs**: every printable ASCII character, Latin-1 and CJK samples,
astral-plane code points, U+0000, U+10FFFF, control characters, the adversarial strings `~X~`, `~E`,
`~~`, `~n`, `~U41~`, `~namp~`, and 4 000 random strings over a pool weighted toward `~`, `/`, `-`,
space and `:`.

| Property | Result |
|---|---|
| P1 round trip | **4 130 / 4 130** |
| P2 idempotence | **4 130 / 4 130**, plus 30 / 30 hand-written valid tokens including every decode-only spelling |
| P3 ASCII output | **4 130 / 4 130** |

Every expected value in this document is the prototype's output. The legacy-only spellings were
additionally run through the **real parser**: `f-hello~.world`, `f-~Hapi.example.com~/data`, `f-~5`,
`f-~~X~~`, `f-~P`, `f-~/` and `f-+` all parse at HEAD.

This is evidence about the *design*, not the implementation — a Rust bug will not be caught by a
Python prototype. Its value is that the priority order, the tie-breaks and the canonical numeric
form are now known to be consistent before anyone writes the real thing.

**It also caught a defect in this document.** The octal spelling of U+1F600 was written by hand as
`~O11746~` and is `~O373000~`; the review pass recomputed it. The lesson generalises to the
implementation: every radix example in the reference and the guide should be generated from the code
path it documents, never typed, and the doc examples in `escape.rs` should be `assert_eq!` doctests
so a wrong constant fails the build.

## References

- Phase 1 `./phase1-high-level-design.md` (D1-D13, Annex B), Phase 2 `./phase2-architecture.md`
- `.claude/skills/liquers-unittest/references/test-patterns.md` for test structure
- `liquers-core/tests/action_parameter_link.rs` — the closest existing integration test
