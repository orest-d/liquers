# Prior art: how other systems separate "what a value is" from "how it is stored"

Supporting research for the `value-type-system` design. Not a reference document: it makes no claim
about Liquers behaviour. It exists so that Phase 2 can argue from precedent instead of taste.

The recurring finding is that **no successful system uses a single type axis**. Every one of them
splits identity from encoding, and the ones that also have to serve *dispatch* add a third,
multi-valued axis for what the value may be used as.

## 1. Apple Uniform Type Identifiers (UTI) — the closest analogue

A UTI is a reverse-DNS string (`public.png`, `com.adobe.pdf`) that **conforms to** one or more
parent UTIs. Conformance is transitive and supports *multiple inheritance*, and Apple's own
documentation splits the parents into two kinds:

- a **physical** UTI, describing what the data *is* (`public.data`, `public.directory`);
- a **functional** UTI, describing how the data is *used* (`public.image`, `public.source-code`).

A consumer asks "does this conform to `public.image`?" rather than "is this a PNG?". New types are
declared by third parties without editing the system's tables.

**Takeaway for Liquers.** This is exactly the three-way split the issue needs: one unique
identifier per concrete variant, a conformance relation to what the value *is*, and a separate,
multi-valued conformance relation to what it can be *used for*. It also settles the naming
convention question: hierarchical, namespaced, extensible by third parties without a central edit.

Sources: [UTI concepts](https://developer.apple.com/library/archive/documentation/FileManagement/Conceptual/understanding_utis/understand_utis_conc/understand_utis_conc.html),
[declaring new UTIs](https://developer.apple.com/library/archive/documentation/FileManagement/Conceptual/understanding_utis/understand_utis_declare/understand_utis_declare.html),
[overview](https://en.wikipedia.org/wiki/Uniform_Type_Identifier).

## 2. Apache Arrow — logical type layered on physical layout

Arrow deliberately has *one* type system, in which every type is logical and each logical type
names a physical memory layout. Richer application semantics are added through **extension types**:
a standard storage type (say `string`) annotated with an extension name and metadata, e.g. the
canonical `arrow.json` extension backed by `string`.

Contrast with Parquet, which keeps physical type and logical annotation formally separate
(`BYTE_ARRAY` + `STRING`), precisely the split Arrow's designers chose to collapse.

**Takeaway.** Two viable shapes exist. Parquet's explicit two-field split is easier to validate
(you can check the pair); Arrow's annotation-on-storage is easier to extend (a new semantic type
costs no new physical type). Liquers already has the Parquet shape *by accident* —
`type_identifier` and `data_format` are two independent fields that nothing checks — and the bug in
`CORE-METADATA-FORMAT-TYPE-CONSISTENCY` is precisely the unvalidated pair. The lesson is not which
shape to pick but that the pair must be validated at the point it is set.

Sources: [Arrow columnar format](https://arrow.apache.org/docs/format/Columnar.html),
[canonical extension types](https://arrow.apache.org/docs/format/CanonicalExtensions.html).

## 3. IANA media types — encoding identity, with suffix conformance

`application/vnd.api+json; profile="..."` carries three separable things: a registered type name,
a **structured syntax suffix** (`+json`, `+zip`, `+der`) saying which generic parser can read the
bytes, and **parameters** refining the reading. RFC 6838 registers the suffix mechanism precisely
so a generic JSON client can process a type it has never heard of.

**Takeaway.** A media type is an encoding label, not a value type — the same PNG bytes are
`image/png` whether the in-memory value is a `DynamicImage` or a `Vec<u8>`. It is also strictly
*coarser* than a Liquers `data_format`: `csv:comma` and `csv:tab` are both `text/csv`, and although
media-type parameters could carry the refinement, nothing parses them — `media_type_of`
(`liquers-web/src/store/fetch.rs:113`) deliberately discards them. So media type cannot serve as a
dispatch key. What it does uniquely is carry an *external party's claim* about the bytes: the
origin server's `Content-Type` on a fetched file, which no extension or `data_format` can express
without guessing a reverse mapping. That, plus being the vocabulary of the web response, is the
whole of its job. The `+suffix` idea separately suggests the shape for `data_format` refinements
already hinted at in `value.rs`: a base format plus a refinement, not a flat opaque string.

Sources: [RFC 6838](https://www.rfc-editor.org/rfc/rfc6838.html),
[RFC 6839 structured syntax suffixes](https://www.rfc-editor.org/rfc/rfc6839.html).

## 4. MLflow model flavors — one artifact, several usable contracts

An MLflow model directory carries an `MLmodel` file listing several **flavors** — `sklearn`,
`onnx`, and the universal `python_function` — each a contract a downstream tool can consume the
same artifact through. A deployment tool integrates with the `python_function` flavor once instead
of with every ML library.

**Takeaway.** This is the "purpose" dimension, and it validates the multi-valued design: a value
advertises *several* usable contracts simultaneously, and consumers dispatch on the contract, not
on the producing library. It also validates having a lowest-common-denominator role that most
values satisfy (Liquers' analogue of `python_function` would be something like `bytes` or `json`).

Sources: [MLflow models](https://mlflow.org/docs/latest/ml/traditional-ml/tutorials/creating-custom-pyfunc/part1-named-flavors/).

## 5. Jupyter MIME bundles — many representations of one value

`display_data` sends a dict keyed by media type: `text/html`, `image/png`, `text/plain` for the
same object; the front-end picks the richest it can render, falling back down the list. The object
does not decide how it is shown, and the front-end does not need to know the object's class.

**Takeaway.** Representation is a *negotiation*, not a property. Relevant to Liquers' web/UI layer
and to conversion: "give me this value as X" is the same question in both.

## 6. Structural typing: Python protocols, Go interfaces, Julia traits

- Python `collections.abc` / `typing.Protocol`: a value is a `Mapping` because it has the methods,
  not because it inherits. `isinstance` against an ABC is a capability question.
- Go interfaces: implicit satisfaction; a type acquires roles without declaring them.
- Julia "Holy traits": since a type has one supertype chain but needs many orthogonal capability
  axes, traits are encoded as separate dispatchable tags. This is the canonical answer to *exactly*
  the problem that a single type hierarchy cannot express several independent purposes.

**Takeaway.** The purpose dimension must not be a hierarchy — the same value is legitimately a
`table` *and* `serializable-as-json` *and* `renderable`, and those axes are independent. A set of
roles with per-role conformance beats one tree.

## 7. Semantic web / schema.org — multiple `rdf:type`

A resource may carry several `rdf:type` assertions, and consumers select by the type they
understand. Nothing forces a single most-specific type.

**Takeaway.** Precedent that multi-typing is workable at scale, and a warning: without a
*principal* type, serialization has nothing deterministic to dispatch on. Liquers needs the
multi-valued roles *and* one unique identifier that round-trips bytes.

## 8. Clipboard / drag-and-drop formats (X11 targets, NSPasteboard, COM `IDataObject`)

A clipboard owner advertises a list of formats and materializes one only when a consumer asks. The
requester names the format it wants; negotiation fails cleanly when no format is shared.

**Takeaway.** Lazy conversion with an advertised capability list — directly applicable to the
conversion draft: a value advertises which target roles it *could* be converted to before any work
is done, and asking for an unavailable one is a clean typed error rather than a failure mid-way.

## Synthesis for Liquers

| Axis | Cardinality | Prior art | Liquers field today | Verdict |
|---|---|---|---|---|
| Concrete variant identity | exactly one | UTI leaf, Arrow extension name, K8s Kind | `type_identifier` (unreliable) | in scope |
| What it is (principal logical type) | exactly one | UTI physical parent, Parquet logical type | none | in scope, under review |
| Producing carrier | exactly one | *none carries it separately* — `com.adobe.pdf`, `arrow.json`, K8s group | none | **not an axis**: derivable from the identifier as a namespace prefix |
| What it may be used as (purposes) | zero or more | UTI functional parent, MLflow flavors, Julia traits | none | deferred to `VALUE-CONVERSION-CAPABILITY` |
| Byte encoding | one per serialized copy | media type + suffix + params | `data_format` / `media_type` / extension | in scope, inward/outward faces |

Three observations shape Phase 2:

1. Every axis above is *independent* — collapsing any two is what produces the reported bug.
2. Exactly one axis may be multi-valued (purposes). Serialization needs a single deterministic
   dispatch key, and every system that round-trips bytes has one.
3. Extensibility has to be third-party: `liquers-lib`, `liquers-py` and `liquers-web` all add value
   types, so the type tables cannot live in a closed enum in `liquers-core`.

---

## 9. Scalar type systems of the nine target ecosystems

Liquers must exchange scalars with JSON, Python, JavaScript (`js-sys`/`web-sys`), Polars, Pandas,
Parquet/Arrow, NumPy, GlueSQL and Rust itself. This section inventories what each actually has, so
that the Liquers scalar set is chosen from evidence rather than guessed. **Rust is the reference
axis** — every Liquers scalar is a Rust type, and the other systems are recorded as what they map
onto.

### Sources checked

Variant lists were read from the defining source, not from memory:

| System | Read from |
|---|---|
| Arrow | `arrow-schema/src/datatype.rs`, `DataType` (41 variants) |
| Polars | `polars-core/src/datatypes/dtype.rs`, `DataType` (31 variants) |
| GlueSQL | `core/src/ast/data_type.rs`, `DataType` (25 variants) |
| Parquet | `parquet-format/LogicalTypes.md` — 6 physical types, ~20 logical annotations |
| NumPy | [array scalars reference](https://numpy.org/doc/stable/reference/arrays.scalars.html) |

GlueSQL: `Boolean, Int8, Int16, Int32, Int, Int128, Uint8, Uint16, Uint32, Uint64, Uint128,
Float32, Float, Text, Bytea, Inet, Date, Timestamp, Time, Interval, Uuid, Map, List, Decimal,
Point` — note `Int` = i64 and `Float` = f64.

Parquet physical: `BOOLEAN, INT32, INT64, FLOAT, DOUBLE, BYTE_ARRAY, FIXED_LEN_BYTE_ARRAY`, with
logical annotations `STRING, ENUM, UUID, INT(bits, signed), DECIMAL, FLOAT16, DATE, TIME,
TIMESTAMP, INTERVAL, JSON, BSON, VARIANT, GEOMETRY, GEOGRAPHY, LIST, MAP, UNKNOWN`. This is the
clean two-level split — a narrow physical set carrying a wide logical set.

### Correspondence table

`—` means the system has no distinct type and the value must be widened, boxed or refused.

| Liquers scalar | Rust | JSON | Python | JavaScript | NumPy | Polars | Pandas | Arrow | Parquet | GlueSQL |
|---|---|---|---|---|---|---|---|---|---|---|
| `none` | `()` / `Option::None` | `null` | `None` | `null` | — | `Null` | `NA`/`NaT` | `Null` | `UNKNOWN` | `NULL` |
| `bool` | `bool` | `boolean` | `bool` | `boolean` | `bool_` | `Boolean` | `bool`/`boolean` | `Boolean` | `BOOLEAN` | `Boolean` |
| `i8` | `i8` | number † | `int` | number † | `int8` | `Int8` | `Int8` | `Int8` | `INT(8,true)` | `Int8` |
| `i16` | `i16` | number † | `int` | number † | `int16` | `Int16` | `Int16` | `Int16` | `INT(16,true)` | `Int16` |
| `i32` | `i32` | number † | `int` | number † | `int32` | `Int32` | `Int32` | `Int32` | `INT32` | `Int32` |
| `i64` | `i64` | number ‡ | `int` | `bigint` ‡ | `int64` | `Int64` | `Int64` | `Int64` | `INT64` | `Int` |
| `i128` | `i128` | — ‡ | `int` | `bigint` | — | `Int128` | — | — | `DECIMAL`/`FLBA` | `Int128` |
| `u8` | `u8` | number † | `int` | number † | `uint8` | `UInt8` | `UInt8` | `UInt8` | `INT(8,false)` | `Uint8` |
| `u16` | `u16` | number † | `int` | number † | `uint16` | `UInt16` | `UInt16` | `UInt16` | `INT(16,false)` | `Uint16` |
| `u32` | `u32` | number † | `int` | number † | `uint32` | `UInt32` | `UInt32` | `UInt32` | `INT(32,false)` | `Uint32` |
| `u64` | `u64` | number ‡ | `int` | `bigint` ‡ | `uint64` | `UInt64` | `UInt64` | `UInt64` | `INT(64,false)` | `Uint64` |
| `u128` | `u128` | — ‡ | `int` | `bigint` | — | `UInt128` | — | — | `DECIMAL`/`FLBA` | `Uint128` |
| `f32` | `f32` | number § | `float` § | number § | `float32` | `Float32` | `float32` | `Float32` | `FLOAT` | `Float32` |
| `f64` | `f64` | number | `float` | number | `float64` | `Float64` | `float64` | `Float64` | `DOUBLE` | `Float` |
| `decimal` | `rust_decimal::Decimal` | — | `decimal.Decimal` | — | — | `Decimal(p,s)` | `object` | `Decimal128/256` | `DECIMAL` | `Decimal` |
| `str` | `String` | `string` | `str` | `string` | `str_` | `String` | `string` | `Utf8` | `STRING` | `Text` |
| `bytes` | `Vec<u8>` | — ¶ | `bytes` | `Uint8Array` | `bytes_` | `Binary` | `object` | `Binary` | `BYTE_ARRAY` | `Bytea` |
| `date` | `chrono::NaiveDate` | — ¶ | `datetime.date` | — ¶ | `datetime64[D]` | `Date` | `datetime64` | `Date32` | `DATE` | `Date` |
| `time` | `chrono::NaiveTime` | — ¶ | `datetime.time` | — | `timedelta64` | `Time` | `object` | `Time64` | `TIME` | `Time` |
| `datetime` | `chrono::DateTime<Utc>` | — ¶ | `datetime.datetime` | `Date` | `datetime64[ns]` | `Datetime(tu,tz)` | `datetime64[ns,tz]` | `Timestamp` | `TIMESTAMP` | `Timestamp` |
| `duration` | `chrono::TimeDelta` | — ¶ | `datetime.timedelta` | — | `timedelta64` | `Duration(tu)` | `timedelta64[ns]` | `Duration` | `INTERVAL` ‖ | `Interval` ‖ |
| `uuid` | `uuid::Uuid` | — ¶ | `uuid.UUID` | — | — | — | `object` | `FixedSizeBinary(16)` | `UUID` | `Uuid` |

† JSON and JavaScript `number` is IEEE-754 double: exact for integers up to 2⁵³, so the narrow
integer widths round-trip but lose their declared width.
‡ Beyond 2⁵³ a JSON number and a JavaScript `number` lose precision silently. JavaScript `bigint`
is exact but does not survive `JSON.stringify`. This is the single most dangerous cell in the
table and the conversion draft must treat it as lossy-by-default.
§ `f32 → f64` is exact; `f64 → f32` is lossy. JSON, Python and JavaScript have only the double.
¶ JSON has no native type; conveyed by convention (base64 string, RFC 3339 string) and therefore
only recoverable when the *declared* Liquers type says what the string means — which is exactly
the argument for the type identifier being authoritative over the encoding.
‖ A Parquet/GlueSQL `INTERVAL` is a calendar interval (months, days, nanos), not the same thing as
an elapsed `Duration`. Treat them as distinct types that convert only conditionally.

### What this implies for `Value`

`liquers-core::value::Value` today carries exactly `None, Bool, I32, I64, F64, Text, Bytes` among
scalars. Against the table, it is missing every one of `i8, i16, i128, u8, u16, u32, u64, u128,
f32, decimal, date, time, datetime, duration, uuid` — fifteen scalars that at least five of the
nine target systems represent distinctly.

Deliberately **not** proposed, with the reason recorded so the decision is not relitigated:

- `f16` — Arrow, Polars and NumPy have it; Rust std does not, and JSON, Python, JavaScript and
  GlueSQL cannot express it. Fails the Rust-reference rule.
- `complex64` / `complex128` — NumPy and Python only.
- `char` — Rust only; `str` covers it everywhere else.
- `isize` / `usize` — platform-dependent width, so not portable across a store.
- `Inet`, `Point` — GlueSQL only; belong in a domain library, not the scalar core.
- `Categorical` / `Enum` / `Dictionary` — an *encoding* of a string column, not a scalar.

### Consequences the architecture phase must absorb

1. **`Value` is `#[serde(untagged)]`** (`liquers-core/src/value.rs:21`). Adding ten more numeric
   variants makes untagged deserialization ambiguous — `7` matches `I8`, `I16`, `I32`, `U8` … and
   serde picks the first. Either the scalars move behind a tagged sub-enum, or serialization stops
   relying on shape inference and starts relying on the declared type identifier. The latter is
   what this project argues for anyway.
2. **Twenty-odd flat variants is the wrong shape.** A `Value::Scalar(Scalar)` sub-enum keeps
   `Value` small and gives the scalar set its own exhaustive `match`, which the no-`_ =>` rule in
   `CLAUDE.md` makes valuable.
3. **The wide scalars do not belong in `liquers-core`.** Only the basic set stays there; the rest
   live in `liquers-lib::ExtValue` behind features, and carrier-specific types (Polars dtypes,
   Python-only, JavaScript-only) belong to the package supporting that carrier. Dependency weight
   is not the reason — `chrono` is already non-optional in both `liquers-core` and `liquers-lib`,
   and only `rust_decimal` and `uuid` would be new. The reason is conceptual surface, and the
   consequence is that the known-type set becomes a runtime fact assembled from the packages in
   the build rather than a compile-time enum.
