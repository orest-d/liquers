---
id: QUERY-AST-DISCARDS-ENTITIES
kind: issue
title: Parsed entities are decoded away, so the AST cannot show them
status: draft
priority: P3
complexity: L
area: [core/query]
design: 
created: 2026-08-14
github:
---
## Problem

The parser decodes entities the moment it recognises them, and keeps only the decoded text. Each
entity parser in `liquers-core/src/parse.rs:345-385` returns a plain `String` — `https_entity`
returns `"https://"`, `space_entity` returns `" "` — and `parameter` (`parse.rs:401`) concatenates
those fragments into one flat string:

```rust
let (text, par) = many0(alt((parameter_text, entities))).parse(text)?;
ActionParameter::new_string(par.join(""))
```

`ActionParameter::String(String, Position)` therefore carries a single `Position` for the whole
parameter and no record of which spans were literal text and which were entities, nor which entity
spelling produced a given character. The information exists during parsing and is discarded.

The encoder cannot recover it. `ActionParameter::encode` (`query.rs:607`) calls `encode_token`,
which re-derives a spelling from the decoded characters, so `encode(parse(t))` normalises `t`:
`~I` and `~/` both decode to `/` and both re-encode to whichever form `encode_token` prefers, and a
URL written `~Hhost` comes back as `https:~/~/host`.

## Impact

**One consumer is left, after `parameter-entity-escaping`.** The decoded value is always correct,
so this was never a wrong result; it is a fidelity and tooling limitation, and that design removed
two of the three motivations it originally had. The surviving one is syntax highlighting.

- **Syntax highlighting and rendering.** `QueryRenderer::styled_tokens`
  (`query.rs:643`) emits one `StyledQueryToken::StringParameter` per parameter, holding
  `encode_token(s)`. `StyledQueryToken::Entity` exists and is used for `~X~`/`~E`, but an entity
  *inside* a string parameter can never be tagged with it. A query console cannot colour `~U1F600~`
  differently from the surrounding literal text, and cannot offer a hover that explains it.
- **Diagnostics — largely addressed elsewhere, and no longer a reason to do this.** The original
  worry was that an error concerning one entity could only point at the parameter's start. The
  `parameter-entity-escaping` design resolves that without touching the AST: the entity parser
  captures the span at the opening `~` and raises the failure with it, and measurement at HEAD
  confirms nom reports the raising span exactly — `a-x~X~q~E` already reports offset 3, the `~X~`
  itself, not the end of input. So "unknown named entity `foo`" arrives with the column of its `~`.
  This bullet is retained only to record that it was considered and settled.
- **Round-trip fidelity — reframed, and mostly not a defect.** Re-encoding a parsed query does not
  reproduce the original spelling. That is canonicalisation working, not failing: with
  `E = encode ∘ parse`, `parameter-entity-escaping` establishes `parse(encode(v)) == v` and hence
  `E(E(t)) == E(t)`, so `E` is a projection onto canonical text and one pass reaches a fixed point.
  What remains is narrow: an **editor that wants to preserve what the user typed** cannot be backed
  by this AST. That is a real want, but it is not correctness.

Workaround: consumers that need the original spelling keep the source text alongside the parsed
`Query` and re-slice it themselves. `liquers-web/src/encode.rs` and the query-console element are
the places that would.

## A cheaper route that may settle it entirely

Before changing `ActionParameter`, try the option that needs no AST change at all.

`parameter-entity-escaping` makes `liquers_core::escape::segments` public — it splits a token into
`Text` and `Entity { spelling, decoded }` pieces. `QueryRenderer::styled_tokens` for a string
parameter currently emits one `StyledQueryToken::StringParameter(encode_token(s))`. It could
instead encode, run `segments` over the result, and emit interleaved `StringParameter` and
`Entity` tokens:

```rust
// sketch, not a proposal for this issue's resolution
let encoded = encode_token(s);
for seg in escape::segments(&encoded)? {
    match seg {
        TokenSegment::Text(t)          => StyledQueryToken::StringParameter(t.to_owned()),
        TokenSegment::Entity { spelling, .. } => StyledQueryToken::Entity(spelling.to_owned()),
    }
}
```

That delivers **entity syntax highlighting**, the one surviving motivation, for a few lines and no
API change — because the encoded form is derivable from the stored value at any time. `Entity`
already exists as a variant and is already used for `~X~`/`~E`.

What it cannot do is highlight the *user's original* spelling: a hand-typed `~I` would be shown as
the canonical `~/`, and `~n~amp~` as `~namp~`. Whether that matters is the real question this issue
now turns on, and it is a much smaller question than the one it was filed with. **Answer it before
committing to the segment-list redesign below** — if canonical highlighting is acceptable, this
issue is closeable without an API change, and its `complexity: L` and its need for a design folder
go with it.

## Expected behaviour

`ActionParameter::String` should hold a **sequence of segments** rather than one flat string, each
segment carrying its own `Position` — roughly:

```rust
enum ParameterSegment {
    Text(String, Position),
    Entity { spelling: String, decoded: String, position: Position },
}
```

with `string_value()` retained as the concatenation of decoded segments, so existing callers keep
working. Several shapes are plausible and the choice is not obvious:

1. **Segment list on the existing variant** — `String(Vec<ParameterSegment>, Position)`. Most
   direct; breaks every pattern match on `ActionParameter::String(s, p)`, of which there are
   matches in `liquers-core` (`plan.rs`, `context.rs`, `command_metadata.rs`), `liquers-py` and
   `liquers-web`.
2. **Keep the decoded string, add the segments beside it** —
   `String(String, Vec<ParameterSegment>, Position)` or a struct variant. Source-compatible for
   readers of the decoded value if accessors are used, still a breaking change for tuple patterns.
3. **Side table** — the decoded string stays, and the parser records segment spans in a parallel
   structure held by `Query`. Non-breaking, but the two can drift, which is the same structural
   mistake that produced `PARAMETER-ESCAPING-INCOMPLETE`.

Whichever is chosen, the encoder should then be able to reproduce the recorded spelling, with
canonical re-encoding available as an explicit operation rather than the only behaviour.

**A constraint this design must not break, and which it is unusually likely to.** The
`parameter-entity-escaping` design establishes the invariant that

> `ActionParameter::String` holds a **decoded, arbitrary** string; no constructor or setter encodes;
> encoding happens only in `encode`, `render` and `styled_tokens`.

The reason to state it here is that this issue's own framing pulls the other way. Once a parameter
becomes a list of "text or entity" segments, it is tempting to treat the text arm as an
**elementary, already-encoded token** that needs no further processing — and that is precisely the
model that produced `ACTION-PARAMETER-SET-VALUE-DOUBLE-ENCODES`, where `set_value` stored
`encode_token(v)` because a string was assumed to arrive pre-encoded.

It is the wrong model. A caller building a query programmatically must be able to supply an
arbitrary value without knowing the grammar, and the spelling must be derived when `encode()` is
called, not when the value is set. So `ParameterSegment::Text` holds decoded text, and any
`spelling` field is a *record of how the source was written*, never the storage form of the value.
A segment list that cannot represent a value the caller never spelled — one built programmatically,
with no source text at all — has taken the wrong shape.

`ActionParameter` is public and appears in `liquers-py/src/query.rs` and `liquers-web/src/encode.rs`
as well as throughout `liquers-core`, so this is a cross-crate API change: **complexity `L`, which
requires a design folder** (`DOCS_STRUCTURE_GUIDE.md` §4.5). None exists yet; one is needed before
implementation.

## Discovery

Raised by the maintainer during Phase 1 of the `parameter-entity-escaping` design
(`specs/design/parameter-entity-escaping/`), which resolves `PARAMETER-ESCAPING-INCOMPLETE` by
adding variable-length numeric (`~U0041~`) and named (`~namp~`) entities. Those entities make the
missing AST representation more valuable — they are longer, more numerous, and can fail
individually — but representing them in the AST is a separable change with a much wider API blast
radius, so it was deliberately left out of that design's scope and recorded here instead.

**Revised after Phase 2 of that design**, at the maintainer's prompting: with entity errors now
reporting accurate positions and canonicalisation shown to be idempotent, two of the three original
motivations are gone. Priority lowered `P2` → `P3`, and the cheaper highlighting route above was
added — it may close this issue without any AST change.
