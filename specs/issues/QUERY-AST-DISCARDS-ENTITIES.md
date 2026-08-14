---
id: QUERY-AST-DISCARDS-ENTITIES
kind: issue
title: Parsed entities are decoded away, so the AST cannot show them
status: draft
priority: P2
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

Three consumers are affected, none fatally — the decoded value is always correct, so this is a
fidelity and tooling limitation rather than a wrong result.

- **Syntax highlighting and rendering.** `QueryRenderer::styled_tokens`
  (`query.rs:643`) emits one `StyledQueryToken::StringParameter` per parameter, holding
  `encode_token(s)`. `StyledQueryToken::Entity` exists and is used for `~X~`/`~E`, but an entity
  *inside* a string parameter can never be tagged with it. A query console cannot colour `~U1F600~`
  differently from the surrounding literal text, and cannot offer a hover that explains it.
- **Diagnostics.** An error concerning one entity in a long parameter can only point at the
  parameter's start position. With the long-form entities of `PARAMETER-ESCAPING-INCOMPLETE` —
  which can fail for their own reasons, such as an out-of-range code point or an unknown name — the
  gap becomes more visible: the natural message is "unknown entity `~xfoo~` at column 23" and that
  column is not available.
- **Round-trip fidelity.** Re-encoding a parsed query does not reproduce the original spelling.
  This is acceptable and arguably desirable for canonicalisation, but it means the AST cannot back
  an editor that preserves what the user typed.

Workaround: consumers that need the original spelling keep the source text alongside the parsed
`Query` and re-slice it themselves. `liquers-web/src/encode.rs` and the query-console element are
the places that would.

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

`ActionParameter` is public and appears in `liquers-py/src/query.rs` and `liquers-web/src/encode.rs`
as well as throughout `liquers-core`, so this is a cross-crate API change: **complexity `L`, which
requires a design folder** (`DOCS_STRUCTURE_GUIDE.md` §4.5). None exists yet; one is needed before
implementation.

## Discovery

Raised by the maintainer during Phase 1 of the `parameter-entity-escaping` design
(`specs/design/parameter-entity-escaping/`), which resolves `PARAMETER-ESCAPING-INCOMPLETE` by
adding variable-length numeric (`~U0041~`) and named (`~xamp~`) entities. Those entities make the
missing AST representation more valuable — they are longer, more numerous, and can fail
individually — but representing them in the AST is a separable change with a much wider API blast
radius, so it was deliberately left out of that design's scope and recorded here instead.
