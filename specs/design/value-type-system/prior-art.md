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
`image/png` whether the in-memory value is a `DynamicImage` or a `Vec<u8>`. Liquers currently
derives `media_type` from a filename extension, which conflates the two. The `+suffix` idea also
suggests the shape for Liquers `data_format` refinements already hinted at in `value.rs`
(`csv:comma` versus `csv:tab`): a base format plus a refinement, not a flat opaque string.

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

| Axis | Cardinality | Prior art | Liquers field today |
|---|---|---|---|
| Concrete variant identity | exactly one | UTI leaf, Arrow extension name, K8s Kind | `type_identifier` (unreliable) |
| What it is (principal logical type) | exactly one | UTI physical parent, Parquet logical type | none |
| What it may be used as (purposes) | zero or more | UTI functional parent, MLflow flavors, Julia traits | none |
| Byte encoding | one per serialized copy | media type + suffix + params | `data_format` / `media_type` / extension |

Three observations shape Phase 2:

1. Every axis above is *independent* — collapsing any two is what produces the reported bug.
2. Exactly one axis may be multi-valued (purposes). Serialization needs a single deterministic
   dispatch key, and every system that round-trips bytes has one.
3. Extensibility has to be third-party: `liquers-lib`, `liquers-py` and `liquers-web` all add value
   types, so the type tables cannot live in a closed enum in `liquers-core`.
