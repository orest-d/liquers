# Draft: value conversion and the purpose axis

**This is a draft for a future project, not part of `value-type-system`.** It exists so that the
purpose axis and the conversion rules are written down while the reasoning is fresh, rather than
rediscovered. Tracked by `specs/issues/VALUE-CONVERSION-CAPABILITY.md`.

It assumes the three axes `value-type-system` ships — variant identity, carrier, principal data
type — and the correspondence table in `./prior-art.md` §9.

## Why purposes belong here and not in the type system

The purpose axis answers "**can this value be used as a table / an image / JSON?**". That is not a
descriptive question about a value; it is a question about what could be *made* from it. Shipping
purposes without conversion would define a vocabulary with no consumer — every caller could ask
whether a value is a `table` and then have no way to obtain one. The two are one feature.

Three consumers want it, and all three are conversion-shaped:

- **Command arguments.** A command wants "a table", not "a `polars_dataframe`". Today
  `ArgumentType::Any` is the only way to say that.
- **UI rendering.** A widget wants "something renderable as a table", so it can accept a Polars
  frame, a list of dictionaries, or a dictionary of lists without knowing which it got.
- **Web negotiation.** A client sends `Accept: text/csv` and the server decides whether the value
  can be served that way.

## The purpose model

A **purpose** is a named contract a value may satisfy: `table`, `image`, `text`, `binary`,
`json`, `dictionary`, `sequence`, `renderable`. Properties:

- **Multi-valued.** A value satisfies zero or more purposes at once. This is the one axis that may
  be multi-valued; the other three are single-valued because serialization needs a deterministic
  dispatch key (`prior-art.md` synthesis, observation 2).
- **Not a hierarchy.** `table` and `serializable-as-json` and `renderable` are independent. Apple's
  UTI splits conformance into physical and functional parents for exactly this reason, and Julia's
  Holy traits exist because a single supertype chain cannot carry orthogonal capability axes
  (`prior-art.md` §1, §6).
- **Registered, not enumerated.** Third-party crates add value types, so purposes are contributed
  to the runtime registry, not fixed in a `liquers-core` enum.
- **Satisfied natively or by conversion.** A `polars_dataframe` *is* a table; a `list of
  dictionaries` *can become* one. Both satisfy `table`, and the difference is a cost, not a
  boolean.

The natural shape is therefore not a set of tags but a lookup: *given this type and this desired
purpose, what does it cost and how do I get there?*

## Two kinds of conversion, deliberately distinguished

| | Explicit | Automatic |
|---|---|---|
| Who asks | the user, in a query or by calling a command | the framework, to satisfy a declared need |
| Where | a conversion command, or a query segment | argument binding, UI rendering, web negotiation |
| Lossy conversions | permitted; the user asked | refused unless the declaration opts in |
| Failure | a typed error the user can act on | a typed error naming both the source type and the target purpose |

Automatic conversion is the risky half. The rule that keeps it honest: **automatic conversion never
loses information silently.** An `f64 → f32` or an `i64 → JSON number` above 2⁵³ is a *lossy* edge
(`prior-art.md` §9 footnotes ‡ and §) and must be refused automatically, however convenient. The
user may still ask for it explicitly.

## Where automatic conversion applies

Automatic conversion has **two** trigger sites, not one, and they need the same machinery with
different defaults.

### 1. Command parameters

A command declares a Rust type; the incoming value may be something else. The framework converts at
argument-binding time, or refuses:

```rust
fn use_df(state: &State<V>, df: polars::DataFrame) -> Result<V, Error>
```

A list-of-dictionaries argument succeeds if that edge is `Structural`; an `i64` fails with a typed
error naming both the source type and the declared target.

### 2. The state

The same question applies to the value flowing *through* a command chain, not only to its
parameters. A command that declares it operates on a table should receive a table, whatever the
previous step produced — the state is an argument like any other, and it is the one a query author
never names explicitly.

The state's conversion is the more delicate of the two, because it is invisible in the query. Three
things the design must settle:

- **Is state conversion opt-in per command, or on by default?** A command declaring `state: Table`
  is asking for it; a command declaring `state` untyped is not. Defaulting to *off* for untyped
  state parameters preserves today's behaviour exactly.
- **Does a converted state keep its metadata?** The type identifier must change to match the new
  value; `data_format` was chosen for the *old* type and is usually wrong afterwards, so it should
  revert to unspecified rather than be carried over. The seeding cascade in
  `specs/reference/VALUE_TYPE_SYSTEM.md` then resolves it from the new value's default.
- **Is the conversion recorded?** A value that silently changed type between two steps is exactly
  the kind of thing the soft-warning tier exists to make visible. A `LogEntry::info` naming the
  edge taken costs nothing and turns an invisible transformation into a traceable one.

### The rules are the design's, not the call site's

Both sites consult the same classification table below, and neither invents its own policy. That is
the point of centralising it: an integration or a command library that grew its own coercion rules
would produce conversions the framework cannot reason about, cannot classify as lossy, and cannot
report. The rule an implementation must not break: **automatic conversion never loses information
silently** — `Lossy` and `Fallible` edges are refused automatically at both sites and are available
only through explicit conversion.

## The conversion graph

Edges come from the correspondence table. Each carries a classification:

| Class | Meaning | Automatic? |
|---|---|---|
| `Identical` | same representation, no work | yes |
| `Exact` | round-trips without loss (`i32 → i64`, `f32 → f64`) | yes |
| `Widening` | exact but changes the declared type (`u8 → i64`) | yes |
| `Lossy` | may lose precision or information (`f64 → f32`, `i64 → JSON number`, `datetime → date`) | no — explicit only |
| `Fallible` | may fail on some values (`str → i32`, `i64 → i32`) | no — explicit only |
| `Structural` | a shape change, not a value change (`list of dictionaries → table`) | yes, when unambiguous |
| `None` | no defined conversion | — |

Two cases the table already surfaces and the implementation must not fudge:

- **`i64`/`u64` above 2⁵³ into JSON or JavaScript `number`.** Silent precision loss, and the single
  most dangerous cell in the correspondence table. `bigint` is exact but does not survive
  `JSON.stringify`.
- **`duration` versus `INTERVAL`.** A Parquet/GlueSQL interval is calendar-based (months, days,
  nanos); an elapsed duration is not. Conditional, not automatic.

## Relationship to the encoding axis

Converting a *value* to another type and serializing a value to a *format* are different
operations, and the design should not merge them even though both answer "give me this as X":

- value → value is this document;
- value → bytes is `data_format` and `DefaultValueSerializer`, already shipped.

Web negotiation touches both: `Accept: text/csv` first asks whether the value can be a `table`
(conversion), then whether that table can be written as CSV (serialization).

## Sketch

Deliberately thin — the shape, not the signatures.

- A `ConversionRegistry` alongside the type registry, keyed by (source type identifier, target).
- Each entry carries its class from the table above and a conversion function.
- `can_convert(from, to) -> Option<ConversionClass>` answers without doing any work, so a value can
  advertise its reachable purposes cheaply. This is the clipboard/pasteboard pattern: advertise the
  format list, materialize only on request (`prior-art.md` §8).
- Automatic paths consult the class and refuse anything `Lossy` or `Fallible`.
- Transitive conversion (`A → B → C`) is tempting and probably a mistake in v1: composing two
  `Exact` edges is safe, but the search space and the error messages both get worse fast.

## Open questions

1. Is a purpose a distinct concept from a type identifier, or is it just a type identifier that
   several types convert to? The second is simpler and may be sufficient — `table` would be a
   registered type nothing is natively stored as.
2. Do purposes need parameters (`table` with a required column set)? Probably yes eventually, and
   that is a schema question rather than a type question.
3. Where does explicit conversion appear in the query language — a command per target, or one
   `convert` command taking a target name?
4. Does automatic conversion happen at argument binding time, or does the command receive an
   accessor that converts on demand? The second interacts with `specs/design/value-accessor/`.
5. For the state site: opt-in per command or on by default, what happens to the metadata of a
   converted state, and whether the conversion is recorded in the log. See "Where automatic
   conversion applies" above.
5. Should conversion be allowed to be async? Converting a large frame is not free, and the asset
   layer is async throughout.
