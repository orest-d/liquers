# Phase 2: Solution & Architecture — Record streams

> **Provisional.** This document carries over the record material developed across revisions 3–6 of
> [`store-and-asset-search`](../store-and-asset-search/phase2-architecture.md), which is where it was
> written and reviewed. It is recorded here at Phase 2 depth so the reasoning is not lost, but
> **this design's Phase 1 is not yet approved**, and Phase 2 is not approved by the carry-over.
> §0 lists what changed in the move, §0.1 what the Phase 1 answers of 2026-09-19 changed, §0.2 the
> abstraction cleanup of revision 2, §0.3 the Arrow correction of revision 3, §0.4 the wasm sharing
> mechanism of revision 4, and §0.5 the move to `liquers-lib` in revision 5.

## Overview

A record stream is a **lazy sequence of columnar batches**, each laid out exactly as Arrow specifies,
each carrying a schema that names its fields and assigns them roles, and each attributable to a
**chunk** that owns its provenance and validity. `liquers-core` gains a `records` module; `Value`
gains one `Arc`-wrapped variant so a record set is an ordinary Liquers value. Nothing is added to
`AsyncStore` or `AssetManager`.

The five requirements of [Phase 1](./phase1-high-level-design.md) map onto the structures as follows:

| Requirement | Mechanism |
|---|---|
| Arrow interoperability without a heavy dependency | Buffers in Arrow's layout; export via the C Data Interface, IPC bytes or wasm typed arrays — all outside core |
| A `Value` variant | `Value::RecordChunk` and `Value::RecordStream`, each with its `TypeInfo` |
| A lightweight DataFrame without polars | Columns, masks, `select`/`filter`/`slice`/`concat` — usable in `liquers-web` |
| Multi-gigabyte lazy processing | `futures::Stream` of batches; the chunk is the refresh unit, the batch the memory unit |
| Per-chunk provenance and validity, flyweighted to the record | `ChunkDescriptor` carries a `Metadata`; a record's provenance is its chunk's |

## 0. What the split changed

| Change | Why |
|---|---|
| **`ClauseMatch` and search evidence leave** | Evidence for *why a row matched a predicate* is a search concept. Records do not know what a clause is. The search design re-expresses it as columns — see §"Search evidence is columns, not a parallel vector", which **resolves open question 2** of the search design's revision 6 |
| **`SearchPredicate`, `Predicate`, `FieldTest`, `BoundPredicate` leave** | The filter is search's; the *maskable column* is records' |
| **`Bitmap` stays** | It has two uses that are not search at all — Arrow validity buffers and `Column::Bool` storage — and a third, filter masks, that any consumer produces. See §"Bitmap" |
| **The stale row-major `RecordBatch` is deleted** | The search design's stream section still carried revision 3's `RecordBatch { sources, records: Vec<Record> }`, contradicting revision 4's columnar definition eight sections earlier. One definition survives: the columnar one |
| **`Diagnostics` leaves entirely** | Resolved during review: `scanned` and unresolvable field names are *evaluation* facts, and `Metadata`'s `LogEntry` already owns those. See §"No `RecordSet`" |

## 0.1 What the Phase 1 answers changed

| Answer | Change here |
|---|---|
| 2 + 3 — lazy handle; two forms; the prototype's manifest | **The largest change.** One variant becomes **two** — `RecordChunk` and `RecordStream` — with a `Manifest` or `Opaque` backing. Rewind, serialization and cacheability become properties of the backing rather than blanket prohibitions. Review then moved both onto `ExtValue`, and retired `ChunkedRecordSource` as redundant against `Manifest` |
| 3 — provenance | A manifest chunk's provenance is the `Metadata` of its own query, already computed by the asset layer; only an opaque chunk carries one inline. The 704-byte concern now applies solely to the form that is never cached |
| 4 — `Date` | `FieldType::Date` and `Column::Date` added, as Arrow `Date32`. `Decimal` still deferred |
| 5 — uniformity | `RecordStream::uniform_schema: Option<…>` — declared by the producer, checked by the consumer. Non-uniform streams are legal and lose exactly two operations |
| 6 — DataFrame baseline | Closes open question; no change |
| 7 — identity vs addressability | `RecordBatch::chunk_id`, stored once per batch. Identity is (chunk id, record id); the `LocatorRule` stays optional |
| 1 — `openbin` out of scope | No change — `open_chunk` already returns a *stream of batches*, so a row-group reader is a later implementation rather than a redesign |

**Open questions 2, 3 and 4 of the original list are closed by these answers**; the remainder are
restated at the end.

## 0.2 Revision 2 — source, stream, chunk

Revision 1 had **two** forms and folded the third into a backing enum: a `RecordStream` that was
sometimes a manifest (shareable) and sometimes a generator (not). Revision 2 separates what that
conflated, along the `Iterable` / `Iterator` line:

| Removed | Replaced by |
|---|---|
| `RecordStream` as a struct with a `StreamBacking` | **`RecordSource`** (the `Iterable`) and the `RecordBatchStream` alias (the `Iterator`) |
| `StreamBacking::{Manifest, Opaque}` | `SourceBacking::{Manifest, Materialized}` on the source; an "opaque" stream is now just a stream nobody kept a source for |
| `rewind()`, fallible depending on backing | `RecordSource::stream()`, callable any number of times |
| `ExtValue::RecordStream` | `ExtValue::RecordSource` |
| `SourceInfo` | `ChunkOrigin` — the rename that frees "source" for its new meaning |

**Four things get deleted rather than documented**, which is the measure of whether the cleanup was
worth doing:

1. The `Clone`-shares-one-stream semantics, where two holders raced and the loser got an error.
2. A `Serialize` that failed at runtime for one backing and succeeded for the other.
3. `rewind()`, whose success depended on how the value happened to be constructed.
4. `volatile` as a correctness requirement on record commands — it returns to being an ordinary
   choice about reuse.

The previous revision's central finding survives untouched: the variants belong on `ExtValue` in
`liquers-lib`, not on `Value` in core. It gets *easier* to justify, since neither surviving variant
has any trouble with `Clone` — the trouble was only ever the stream, which is no longer a value.

## 0.3 Revision 3 — what "Arrow-compatible" actually means

Revision 2 and earlier claimed a "zero-copy pointer hand-off" without saying what is handed off. A
fair objection exposed the gap: **Rust does not define struct layout, so what makes a Rust struct
Arrow-compatible?** The answer is that nothing does, and nothing needs to — Arrow standardizes
*buffers* and a *handoff ABI*, not structs. §"How Arrow compatibility actually works" now says so,
and four things the earlier text got wrong or omitted are corrected:

| Correction | Was | Is |
|---|---|---|
| The level compatibility lives at | "the layout is Arrow's, so it is a pointer hand-off" | Data compatibility is over `&[T]`, whose layout Rust *does* define; structure is rebuilt at the boundary as `#[repr(C)]` |
| Whether a whole chunk can be shared | unstated | **Yes** — `ArrowArray` is recursive; a batch is a struct array with the columns as children |
| The cost of exporting | implied free | Free for **data**; O(columns) small allocations for the **description** |
| The wasm route | listed as plainly "zero-copy" | Qualified: **growing the wasm heap detaches every typed-array view**, so a view is valid only until the next call into wasm |

Three layout mismatches are now written down rather than left to be discovered during
implementation: `Text`/`Binary` offsets need `len + 1` entries, `Vector` exports as a
`FixedSizeList` with its values in a **child** node, and `Bool` carries two bitmaps.

One happy accident recorded: Arrow recommends padding buffers to a 64-byte multiple, which the
`#[repr(align(64))]` backing-chunk approach already does.

One dependency finding: **`polars-arrow 0.55.2` is already in the lockfile** via polars and exposes
its own C Data Interface, so the native polars hand-off need not hand-roll FFI when the `polars`
feature is on.

## 0.4 Revision 4 — a safe wasm sharing mechanism

Revision 3 flagged the browser route as "fragile" and stopped. That was too pessimistic, and the
question of whether invalidation can be *detected* has a good answer. §"The wasm route" now specifies
a dedicated mechanism, deliberately not Arrow-shaped:

- **Two hazards, not one.** Heap growth detaches the JS `ArrayBuffer` but **does not move the Rust
  allocation** — wasm growth extends linear memory and never relocates pages. A freed or reallocated
  buffer is a different and far more dangerous problem.
- **Growth is detectable in O(1)** by comparing `view.buffer` against the live `memory.buffer` by
  identity, and recoverable by re-creating the view at the *same* pointer and length.
- **A dangling pointer is not detectable at all**, so it is prevented structurally: the handle holds
  an `Arc<RecordBatch>`, and buffers are immutable `Arc<[T]>` that never reallocate.
- **The refresh mechanism is a ten-line JS getter** that does the identity check per access. No
  copying on the fast path.
- `columnCopy` remains as an always-correct fallback, now a deliberate choice rather than the only
  safe option.

Recorded as constraints rather than left implicit: the views are **read-only**, a view must not
outlive the handle (`debug-handles` gives the release test, as it does for `RUNTIME05`), and
`SharedArrayBuffer` would remove the hazard entirely at the cost of cross-origin isolation.

## 0.5 Revision 5 — `liquers-lib`, behind a feature

The open question carried since revision 1 — whether the record *data* types belong in
`liquers-core` or `liquers-lib` — is **settled: `liquers-lib`, behind a `records` feature.** Records
are in essence a new data type rather than an essential capability, which is the same class as
`image-support` and `polars`, and what `CLAUDE.md` already directs to `liquers-lib/src/value/`.

| | Was | Is |
|---|---|---|
| Data types | `liquers-core/src/records/` | `liquers-lib/src/records/`, gated |
| Value variants | `ExtValue` in `liquers-lib`, ungated | same, **gated** |
| `bytemuck` | new **unconditional** direct dependency of core | **optional** dependency of `liquers-lib`, pulled only with the feature |
| `liquers-core` | new module, new dependency | **untouched**, apart from two generic stream aliases |
| Search predicate | `liquers-core` | follows records into `liquers-lib` |

Three things improve as a result:

1. **A build with `records` off is the build that exists today** — no new module, no new dependency,
   no new match arm reached.
2. **The revision-3 dependency finding is defused.** `bytemuck` was going to be a new unconditional
   edge on core's graph; it is now optional and feature-scoped.
3. **The search design also becomes additive to one crate**, since its predicate operates on
   `RecordBatch` and must follow it.

The cost, stated: a consumer wanting records without the rest of `liquers-lib` cannot have them,
because the crate is the unit of dependency. Acceptable while `liquers-lib` is where command
libraries live; the module is self-contained enough to lift into its own crate if that ever changes.

Revision 5 also adds **specification citations** ([COLUMNAR], [CDATA], [IPC]) throughout the Arrow
material, including a per-type format-string mapping, with the explicit caveat that the format
strings are to be verified against the spec at implementation rather than trusted from this document.

## Known-Issue Preflight## Known-Issue Preflight## Known-Issue Preflight## Known-Issue Preflight## Known-Issue Preflight

Searched `specs/index.csv` for non-terminal `issue`/`feature` records whose `area` intersects
`core/value`, `core/commands`, `core/context`, `core/query`, `lib/value`, `web`.

| Issue | Status | Pri | Relevance and solution impact | First? | Blocking? | Required action | Priority action |
|---|---|---|---|---|---|---|---|
| `NO-RECORD-STREAM-ABSTRACTION` | draft | P2 | This design *is* its resolution | n/a | no | Close in Phase 5 | keep P2 |
| `CORE-VALUE-ENUM-OVERSIZED` | draft | P2 | `Value` is 704 bytes. Drove fields to `FieldValue` and the new variant behind `Arc` | no | no | Honoured throughout | keep P2 |
| `VALUE-SERIALIZATION-HAS-NO-INCREMENTAL-WRITER` | draft | P2 | A multi-gigabyte record stream cannot be serialized through a `Vec<u8>`-returning writer. **The clearest motivating case yet filed for it** | no | no | Monitor; streaming serialization is a later milestone, and this issue is its prerequisite | consider P1 when that milestone starts |
| `CORE-METADATA-NO-APPLICATION-ATTRIBUTES` | draft | P2 | Front-matter fields have nowhere to live, so `attr.`-qualified columns have no source | no | no | Field qualification is designed to accept it as a pure upgrade | keep P2 |
| `COMMAND-CONTEXT-PARAM-ORDER` | accepted | P2 | `context` must be last in record-producing commands | no | no | Honoured | keep P2 |
| `CORE-SYNC-STORE-TRAIT-OBSOLETE` | — | — | Record sources read through `AsyncStore` only | no | no | — | — |
| `COMMAND-CACHE-FLAG-IS-DECLARED-BUT-NEVER-READ` | draft | P3 | Stream commands rely on `volatile`, which **is** wired; `cache` is a knob that does nothing. Found while answering Phase 1 | no | no | Monitor — `volatile` covers this design's need | keep P3 |

**No blocker.**

## Data Structures

New module `liquers-core/src/records/`.

### Columns, not rows: the batch is Arrow-laid-out

A predicate — or any filter — over a **column** produces a boolean mask, and filters combine by
ANDing masks. That is how polars and DuckDB filter, and it is faster than a row walk. Decisively,
the cheap routes to Arrow (the C Data Interface, and typed arrays over wasm memory) are **only**
possible if the data is already laid out Arrow's way.

```rust
/// A batch of rows in Arrow's memory layout. The unit of memory, a table, and a minimal DataFrame.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RecordBatch {
    /// Field names, types and roles — once per batch.
    pub schema: Arc<RecordSchema>,
    /// One column per schema field, in order. `len` rows each.
    pub columns: Vec<Column>,
    pub len: usize,
    /// Identity of the chunk these rows came from — once per batch, not per row.
    /// With the `Id`-role column this makes a record identifiable as (chunk, id).
    pub chunk_id: Option<ChunkId>,
    /// Dictionary of sources; the `Source`-role column indexes it.
    pub sources: Vec<ChunkOrigin>,
}

/// Column storage. Buffers are laid out exactly as Arrow specifies — 64-byte aligned, validity
/// omitted when there are no nulls — so an export is a pointer hand-off rather than a conversion.
/// `Buffer<T>` is an `Arc`-shared, 64-byte-aligned `[T]`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum Column {
    Bool { validity: Option<Bitmap>, values: Bitmap },
    Int { validity: Option<Bitmap>, values: Buffer<i64> },
    UInt { validity: Option<Bitmap>, values: Buffer<u64> },
    Float { validity: Option<Bitmap>, values: Buffer<f64> },
    /// Arrow `Utf8`: i32 offsets plus a contiguous byte buffer. No per-cell allocation.
    Text { validity: Option<Bitmap>, offsets: Buffer<i32>, data: AlignedBuffer },
    Binary { validity: Option<Bitmap>, offsets: Buffer<i32>, data: AlignedBuffer },
    /// Arrow `Date32` — days since the epoch. `chrono` converts; storage stays primitive.
    Date { validity: Option<Bitmap>, values: Buffer<i32> },
    /// Arrow `Timestamp(Microsecond, None)` — the date-time case.
    Timestamp { validity: Option<Bitmap>, values: Buffer<i64> },
    /// Arrow `FixedSizeList(Float32, dim)` — the embedding case, contiguous.
    Vector { validity: Option<Bitmap>, dim: usize, data: Buffer<f32> },
}
```

**Compactness is not incidental.** An `i64` column costs 8 bytes per value against `FieldValue`'s 24;
a text column costs its bytes plus 4 per row, against a 16-byte `Arc<str>` per cell plus the
allocation; nulls cost one bit rather than a whole slot. For a corpus of a few hundred thousand rows
that is the difference between comfortable and not, in the browser especially — and for the
multi-gigabyte case it sets how many rows fit in one resident batch.

**`FieldValue` survives as the scalar type, not as storage** — it is what a filter compares against,
what a single-cell read returns, and what a builder appends. Storage is columns.

### How Arrow compatibility actually works

> **Specification references.** Everything in this section is against the Apache Arrow format
> specification; the documents are cited rather than paraphrased from memory.
>
> | Ref | Document | Used for |
> |---|---|---|
> | **[COLUMNAR]** | [Arrow Columnar Format](https://arrow.apache.org/docs/format/Columnar.html) | Buffer layouts, validity bitmaps, alignment and padding, the physical layout of each type |
> | **[CDATA]** | [Arrow C Data Interface](https://arrow.apache.org/docs/format/CDataInterface.html) | `ArrowSchema` / `ArrowArray` structs, format strings, release-callback semantics |
> | **[IPC]** | [Arrow IPC and serialization](https://arrow.apache.org/docs/format/Columnar.html#serialization-and-interprocess-communication-ipc) | The file and stream formats, for the deferred `.arrow` serialization |
>
> Format strings and section names below use the spec's own vocabulary. **They are to be verified
> against [CDATA] at implementation** rather than trusted from this document — a wrong format string
> fails at the boundary, loudly or silently depending on the consumer.

**Rewritten in revision 3.** The earlier text asserted a "pointer hand-off" without saying what is
handed off, which hid a fair objection: *Rust does not define a struct memory layout, so how can a
Rust struct be Arrow-compatible?*

**It cannot, and it does not have to — because Arrow does not standardize structs either.** Arrow
standardizes **buffers**: contiguous runs of bytes holding primitive values, offsets, or packed bits,
little-endian, with a recommended alignment. An Arrow "array" or "record batch" is a *logical*
description of how some buffers fit together. It has no canonical in-memory struct anywhere, in any
language.

So compatibility lives at two separate levels, and conflating them is what made the earlier text
misleading:

| Level | What crosses | Why it is safe |
|---|---|---|
| **Data** | the *contents* of `Buffer<T>` — an `[i64]`, `[i32]`, `[f32]`, `[u8]` | Rust **does** define this. A slice of a primitive is N contiguous values of known size and alignment, with the target's endianness. `#[repr(Rust)]` never enters into it: we share `&[T]`, never a struct |
| **Structure** | which buffers, in what order, with what types | Rebuilt at the boundary as **`#[repr(C)]`** structs, whose layout *is* guaranteed |

That second row is precisely what the **Arrow C Data Interface** is for. It is two small `repr(C)`
structs ([CDATA] *Structure definitions*) — `ArrowSchema` (a type as a format string, a name,
children) and `ArrowArray` (length, null count, offset, a pointer array of buffers, children, a
`release` callback, `private_data`). Arrow
deliberately declined to standardize in-memory structs across languages and standardized a **handoff
ABI** instead. Our layout decision is what lets the buffer pointers in that ABI point straight at our
data rather than at a converted copy.

**Can a whole chunk be shared, not just a column?** Yes. `ArrowArray` is **recursive** — it carries
`n_children` and `children: *mut *mut ArrowArray`. A `RecordBatch` exports as a *struct array*: one
root `ArrowArray` whose children are the column arrays, each pointing at our buffers. Exporting
therefore allocates a small tree — one `ArrowArray` and one `ArrowSchema` per column, plus the root
and the buffer-pointer arrays — and copies **no data**.

> "Zero-copy" is true of the **data** and false of the **description**. The description is
> O(number of columns) tiny allocations. Worth saying plainly, because the earlier phrasing implied
> the whole thing was free.

**Ownership is the part that needs care, and it is where the `unsafe` lives.** [CDATA] *Release
callback semantics* puts the obligation on the consumer: it takes the struct and must call `release`,
which marks itself done by nulling the callback pointer and must release children before the parent. The producer boxes a private struct holding clones of the
`Arc`s behind every exported buffer, stashes it in `private_data`, and drops it in `release`. That
keeps our buffers alive exactly as long as pandas or polars holds them, and is the reason this code
belongs in `liquers-py` beside the existing pyo3 surface rather than in core.

#### Where our layout meets Arrow's, and three places it does not line up 1:1

| `Column` variant | Arrow type | [CDATA] format | Buffers | [COLUMNAR] section | Note |
|---|---|---|---|---|---|
| `Bool` | `Bool` | `b` | validity, values | *Fixed-size primitive layout*, *Validity bitmaps* | Both bitmaps, LSB-first — matches |
| `Int` | `Int64` | `l` | validity, values | *Fixed-size primitive layout* | Direct |
| `UInt` | `UInt64` | `L` | validity, values | as above | Direct |
| `Float` | `Float64` | `g` | validity, values | as above | Direct |
| `Date` | `Date32` (days) | `tdD` | validity, values (`i32`) | *Temporal types* | Direct |
| `Timestamp` | `Timestamp(µs)` | `tsu:` | validity, values (`i64`) | *Temporal types* | The trailing colon carries the (empty) timezone |
| `Text` | `Utf8` | `u` | validity, offsets, data | *Variable-size binary layout* | **Offsets hold `len + 1` entries starting at 0** — a spec invariant the builder must enforce, not an implementation detail |
| `Binary` | `Binary` | `z` | validity, offsets, data | as above | Same `len + 1` invariant |
| `Vector` | `FixedSizeList(Float32, dim)` | `+w:<dim>` | validity **only** | *Fixed-size list layout* | **Two levels.** The list node carries validity and *no* value buffer; the values live in a **child** `Float32` (`f`) array. Our flat `Buffer<f32>` is right, but the export emits a child node |
| `RecordBatch` | `Struct` | `+s` | validity **only** | *Struct layout* | The root of an exported chunk; the columns are its children |

Those three rows are called out because they are the ones that get discovered late otherwise.

**A useful coincidence on alignment.** [COLUMNAR] *Buffer Alignment and Padding* **recommends**
allocating buffers on 64-byte boundaries and padding their length to a multiple of 64; it **requires**
only 8. The `#[repr(align(64))]` backing-chunk approach chosen for `AlignedBuffer` over-allocates
to a 64-byte multiple anyway, so it produces Arrow's recommended padding for free — the logical length
is tracked separately, as it must be regardless.

**Endianness** is not a practical concern: [CDATA] is same-process, so producer and consumer agree by
construction, and [IPC] specifies little-endian as the default, which every target this project
builds for already is. One line, not a design problem.

#### The three routes, honestly rated

| Route | Data copied | Where | Status |
|---|---|---|---|
| **C Data Interface** | none | `liquers-py` | The real zero-copy path. A few hundred lines and the only `unsafe`. **`polars-arrow 0.55.2` is already in the lockfile** via polars, and exposes its own C Data Interface — so the native polars hand-off can go through it rather than hand-rolling FFI, whenever the `polars` feature is on |
| **Arrow IPC / Feather** | yes — it is serialization | `liquers-lib`, deferred | Not sharing. A flatbuffers encoder; also the natural `.arrow` file format for a chunk |
| **Typed arrays over wasm memory** | none, but fragile | `liquers-web` | **Weaker than the earlier draft claimed** — see below |

#### The wasm route: a dedicated safe mechanism, not Arrow

There is no C Data Interface in a browser — JavaScript cannot consume `repr(C)` structs — so the
browser gets **its own sharing mechanism**, which need not be Arrow-shaped. The goal stands: read the
data in place, in linear memory, without copying.

**Revision 3 called the typed-array route "fragile" and left it there. That was too pessimistic.** On
examination there are *two* hazards, not one, and they have completely different characters:

| | Hazard A — the heap grows | Hazard B — the buffer is freed or moved |
|---|---|---|
| What breaks | the JS `ArrayBuffer` is **detached** and `memory.buffer` returns a new object | the pointer **dangles** |
| Does the Rust data move? | **No.** wasm growth extends linear memory; it never relocates existing pages | Yes, or it is gone |
| Detectable from JS? | **Yes, exactly and in O(1)** | **No.** Reads return plausible garbage, silently |
| Answer | detect and refresh | **prevent structurally** |

**Hazard A is easy, and this is the key fact:** growth invalidates the JS *view object*, not the Rust
*pointer*. The detection is a reference comparison —

```js
if (view.buffer !== memory.buffer) { /* stale */ }
```

— and the refresh is re-creating the view at the **same pointer and length**, because the allocation
never moved:

```js
view = new Float64Array(memory.buffer, ptr, len);
```

**Hazard B is the dangerous one, and it is prevented rather than detected.** The handle holds an
`Arc<RecordBatch>`, which keeps every buffer alive; our buffers are `Arc<[T]>` and **immutable once
built** (§"Concurrency Considerations"), so they never reallocate. For as long as JS holds the
handle, the pointers are stable. This is the same ownership discipline as the C Data Interface's
`release` callback, expressed in a way wasm-bindgen already supports.

**The simplest discipline, which makes most of this moot: do not cache views.** Constructing
`new Float64Array(memory.buffer, ptr, len)` is O(1) — a small JS object wrapping a pointer, with no
data copy — so creating it at the point of use costs nothing measurable and makes staleness nearly
unreachable. A view only goes stale if it is held across a call into wasm or across an `await`.

**The API.** Following `liquers-web`'s existing handle convention (`#[wasm_bindgen(js_name = …)]` over
an inner value, as `LiquersQuery` and `LiquersKey` do):

```rust
// liquers-web/src/records.rs
#[wasm_bindgen(js_name = RecordChunk)]
pub struct LiquersRecordChunk {
    /// Keeps every buffer alive for the handle's lifetime — the answer to Hazard B.
    inner: Arc<RecordBatch>,
}

#[wasm_bindgen(js_class = RecordChunk)]
impl LiquersRecordChunk {
    #[wasm_bindgen(getter, js_name = numRows)]    pub fn num_rows(&self) -> usize;
    #[wasm_bindgen(getter, js_name = numColumns)] pub fn num_columns(&self) -> usize;
    #[wasm_bindgen(js_name = schemaJson)]         pub fn schema_json(&self) -> String;
    /// Descriptor for one column: kind, pointer, length, and the validity bitmap when present.
    /// Enough for JS to build a view; carries no data itself.
    pub fn column(&self, i: usize) -> Result<JsValue, JsValue>;
    /// The always-safe fallback: an owned typed array, detached from linear memory. O(n).
    #[wasm_bindgen(js_name = columnCopy)]
    pub fn column_copy(&self, i: usize) -> Result<JsValue, JsValue>;
    // `free()` is generated by wasm-bindgen and drops the Arc.
}
```

and a small JS companion in which the refresh *is* the getter:

```js
class RecordColumn {
  constructor(memory, desc) { this.memory = memory; this.desc = desc; this._view = null; }
  get view() {                       // one reference comparison per access
    if (this._view === null || this._view.buffer !== this.memory.buffer) {
      this._view = makeView(this.memory.buffer, this.desc);   // same ptr, same len
    }
    return this._view;
  }
  toCopy() { return this.view.slice(); }   // the fallback, explicit
}
```

That is the whole refresh mechanism: roughly ten lines, one reference comparison per access, no
copying on the fast path, and an explicit copy available whenever a caller wants a value that outlives
linear memory.

**Two constraints that must be documented, not discovered:**

1. **The views are read-only.** Writing through them into buffers Rust holds as immutable `Arc<[T]>`
   would violate the aliasing assumptions the rest of the design relies on. Single-threaded JS plus
   wasm means concurrent *reading* is fine; writing is not.
2. **A view must not outlive the handle.** `free()` drops the `Arc`, and a view held past that point
   is Hazard B with no detection. The `debug-handles` feature already used for `RUNTIME05` gives the
   test: assert the live chunk-handle count returns to zero after `free()`.

**Two escape hatches worth recording, neither relied on:**

- **`SharedArrayBuffer`.** If the wasm memory is created with `shared: true`, growth does **not**
  detach — a growable `SharedArrayBuffer` grows in place and existing views stay valid, removing
  Hazard A entirely. The cost is cross-origin isolation (COOP/COEP headers), a deployment burden that
  should not be a precondition for reading a table.
- **Pre-reserving the heap** so growth never occurs in a session. It makes the fast path the common
  path; it is not a correctness guarantee, and the identity check stays regardless.

**Test to write, because it is the one that would otherwise be skipped:** take a view, force
`memory.grow` by allocating in between, then read — and assert the wrapper refreshed transparently
and returned the right values.

**Copying is an accepted fallback, not the plan.** `columnCopy` exists and is always correct; the
design above means it is a caller's deliberate choice rather than the only safe option.

**Still no claim of full Arrow support.** A deliberate subset: the types above, no nested `Struct`
beyond the batch root, no `Union`, no dictionary encoding, no large (64-bit offset) variants. Enough
for a record batch and a pandas hand-off; extendable, and honest about not being arrow-rs.

### Bitmap

```rust
/// Bit-packed booleans, LSB-first within each byte, as Arrow specifies.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Bitmap { bits: AlignedBuffer, len: usize }
```

Three uses, all load-bearing, and **only the second has anything to do with search**:

1. **Validity (nulls).** Not an edge case: a column drawn from heterogeneous sources is absent for
   most of them, and for text an empty string is a *different* answer from "absent", so there is no
   free sentinel.
2. **Filter masks.** Any filter — a search predicate, a DataFrame `filter`, a SQL `WHERE` pushed
   down from GlueSQL — evaluates to a boolean mask over a column. Same representation, different
   meaning.
3. **Boolean columns.** Arrow stores `Bool` as a bitmap, one bit per value — so `Column::Bool`
   carries two of them, validity and values.

**Could it just be `Vec<bool>`?** On space, the bitmap is 8× smaller — 125 KB against 1 MB per
nullable column at a million rows, which matters most in the browser. But the decisive reason is
compatibility: **Arrow's validity buffer *is* a bitmap** ([COLUMNAR] *Validity bitmaps*, LSB-first
within each byte), so a `Vec<bool>` cannot be handed over at
all. It would need converting, which destroys the zero-copy property that is the entire point of the
columnar layout.

**One simplification, which the spec sanctions:** validity is `Option<Bitmap>` and is **omitted
entirely when a column has no nulls** — [COLUMNAR] *Validity bitmaps* allows the buffer to be omitted
when the null count is zero, and [CDATA] carries `null_count` on `ArrowArray`, where `-1` means
"unknown" and forces the consumer to count. Most columns in
practice have none and pay nothing.

Size: roughly 150–200 lines with tests — `get`, a builder, `and`/`or`/`not`, `count_ones`. The one
fiddly part is slicing at a non-byte boundary, which the spec permits through `ArrowArray::offset`
([CDATA]); the first
version **requires byte-aligned slice offsets** and copies when a caller asks for anything else,
which removes the fiddly case at a cost paid only by an unusual slice.

### 64-byte alignment: worth doing, and cheaper than it looks

[COLUMNAR] *Buffer Alignment and Padding* requires 8-byte buffer alignment and **recommends** 64, so
a consumer can use aligned SIMD loads without a special case at the tail. This is a performance and
compatibility matter rather than a correctness one — an unaligned buffer still works, but a strict
consumer may copy, and SIMD paths may be disabled.

Natural Rust gives 8-byte alignment for `Vec<i64>` and 1-byte for `Vec<u8>`, so the text and binary
buffers are the ones that fall short. Four ways to get 64:

| Approach | Cost |
|---|---|
| Custom allocation via `std::alloc` with a 64-byte `Layout` | `unsafe` in core, and manual deallocation |
| Over-allocate and offset | Safe but wasteful, and the offset has to travel with the buffer |
| **`#[repr(align(64))]` backing chunks, cast to `&[u8]`** | **~60 lines, and `bytemuck` makes the cast safe** |
| Copy once at the export boundary | No core change; degrades "zero-copy" to "one copy per hand-off" |

**Recommended: the third.** `bytemuck` is tiny, `no_std`-capable and wasm-safe, and `cast_slice`
turns the aligned backing store into `&[u8]` **with no `unsafe` in our code** — which keeps
`liquers-core`'s library code unsafe-free.

**It is a genuinely new dependency, contrary to an earlier claim in this document.** Checked during
review: `bytemuck` appears in `Cargo.lock` at 1.25.2, but it is **not a direct dependency of any
workspace crate** — it arrives transitively through `egui` (`half → emath → epaint → bytemuck`), so a
`--no-default-features` build does not have it in the graph at all. Adding it to `liquers-core` is
therefore a real new direct dependency, not a free one. It is still the right call — one small,
well-audited, `no_std` crate confined to `records/buffer.rs` against the alternative of `unsafe` in
core — but the cost is one dependency, not zero.

```rust
/// A byte buffer whose start is 64-byte aligned, as Arrow recommends.
/// Backed by `#[repr(align(64))]` chunks; `bytemuck::cast_slice` reads it as bytes.
#[derive(Debug, Clone, PartialEq)]
pub struct AlignedBuffer { /* … */ }

/// A typed, `Arc`-shared, 64-byte-aligned slice. A `bytemuck::Pod` view over an `AlignedBuffer`.
#[derive(Debug, Clone, PartialEq)]
pub struct Buffer<T: bytemuck::Pod> { /* … */ }
```

**Day one, not later.** Retrofitting alignment means reallocating every buffer in the system, and the
fallback (a copy at the export boundary) stays available if the dependency is ever unwelcome.

### The minimal DataFrame role

The brief asks the record mechanism to double as a simplistic DataFrame where polars is too expensive
to bundle — which is the wasm build, where polars is not an option at all. Columns give that almost
for free:

| Operation | Implementation |
|---|---|
| select columns | clone `Arc`s into a new batch; no data copied |
| filter | evaluate a predicate to a boolean mask, then gather |
| slice | offset + length on each buffer, `Arc`-shared |
| concat | schema equality check, then buffer append |
| column stats | a pass per column |

So `liquers-web` gets a usable tabular value with **no new dependency**, and a polars-enabled native
build converts to a real `DataFrame` over the shared buffers when it wants one.

### FieldValue — the scalar type

```rust
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum FieldValue {
    Null, Bool(bool), Int(i64), UInt(u64), Float(f64),
    Text(Arc<str>), Bytes(Arc<[u8]>), Timestamp(i64), Vector(Arc<[f32]>),
}
```

**Measured 24 bytes**, against `serde_json::Value`'s 32 — smaller *and* able to carry bytes without
base64, a timestamp as a type, and a vector compactly. **Not a type parameter**: that would infect
`RecordBatch<V>`, the stream and the value variants — circular, since `Value` is the
obvious `V`. Arrow, GlueSQL, Tantivy and Qdrant all chose a dynamic type enum over generics for the
same reason.

`FieldValue::Object` is dropped: with columns, nesting is Arrow's `Struct`, which the subset above
deliberately excludes for now.

### RecordSchema — modelled on the systems to be integrated

Two orthogonal axes, because no single target has both: a **logical type** (Arrow, polars, GlueSQL)
and a **role** (Tantivy, Lucene, Qdrant).

```rust
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RecordSchema {
    pub fields: Vec<FieldSchema>,
    /// Liquers' own type identity for the thing the rows describe, when there is one.
    pub type_identifier: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FieldSchema {
    pub name: String,
    pub data_type: FieldType,
    pub role: FieldRole,
    pub nullable: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum FieldType { Bool, Int, UInt, Float, Text, Binary, Date, Timestamp, Vector }

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum FieldRole {
    /// The row's identity within its source. Exactly one per schema.
    Id,
    /// Index into `RecordBatch::sources`. At most one per schema.
    Source,
    /// Tokenized and matched by a text query. **Any number** — a title, a body and a comment
    /// are all text, and none is privileged.
    Text,
    /// Exact match and facet; never tokenized.
    Keyword,
    /// Returned, never searched.
    Stored,
    /// Range comparisons.
    Numeric,
    /// Similarity comparisons.
    Vector,
    /// Carried and ignored.
    Ignored,
}
```

`FieldType` maps one-to-one onto the `Column` variants, and both map onto Arrow's `DataType`.

**The identity guarantee lives in the schema and is checked once.** Phase 1's requirement that a
record be identifiable cannot live in a row type once storage is columnar, so `RecordSchema::new`
**fails** unless exactly one field has role `Id`, and accessors (`id_field()`, `source_field()`,
`text_fields()`) keep consumers from indexing by string. That is how Tantivy does it: the schema is
validated when built and hands out field handles.

| Target | Mapping |
|---|---|
| **Tantivy / Lucene** | `FieldRole` *is* the field options — `Text`→TEXT, `Keyword`→STRING, `Stored`→STORED, `Numeric`→fast field |
| **Arrow / polars / pandas** | `FieldSchema`→`Field`, `FieldType`→`DataType`, `Column`→the same buffers; export is a hand-off |
| **GlueSQL** | `FieldSchema`→column, `FieldType`→SQL type |
| **Qdrant** | role `Vector`→a named vector; everything else→payload |
| **tinysearch** | only `Text`-role columns feed the per-document filter |
| **Liquers type system** | `RecordSchema::type_identifier` carries the `TypeInfo` identity of the described value; `FieldType` is about *fields*, the registry about *values* |

### Schema uniformity is declared, not assumed

**Phase 1 answer 5.** Chunks of one stream are usually but not necessarily alike, so
`RecordStream::uniform_schema` is `Some` when the producer promises it and `None` otherwise. A
consumer checks rather than assumes. The prototype takes the same position in the weakest possible
way — on a column-count mismatch it **warns and continues** — and this design keeps the tolerance
while making the promise inspectable.

Non-uniformity is legal with consequences rather than an error:

| Operation | Non-uniform stream |
|---|---|
| iterate, filter per chunk | fine |
| serialize as NDJSON | fine — each row carries its own keys |
| `concat` into one batch | **fails**, naming the first differing field |
| serialize as one CSV | **fails** — there is no single header |

The motivating case is "every CSV file in a folder becomes one chunk", which is a `Manifest` with one
query per file and no guarantee the files agree. That is useful rather than illegal: it works for
everything except the two operations that genuinely need one schema.

### ChunkOrigin — identity, description and **retrieval**

*(named `SourceInfo` before revision 2, when `RecordSource` took the word "source".)*

```rust
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ChunkOrigin {
    /// The asset these rows were projected from, as a query.
    #[serde(with = "query_format")]
    pub asset: Query,
    /// The query that re-produces this source's rows. **The retrieval path**: evaluating it
    /// yields the batch again, and the `Id` field indexes into it.
    #[serde(with = "query_format")]
    pub chunk: Query,
    /// Description of the asset itself, when it has one. `None` for a source that is not an asset.
    pub info: Option<AssetInfo>,
    /// How to turn an `Id` value into a directly evaluable query, when the projection can.
    pub locator: Option<LocatorRule>,
}

/// A command applied to `asset`, with the id supplied as its final parameter.
/// Rendered through `ActionRequest`, never by string templating.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct LocatorRule {
    pub namespace: String,
    pub command: String,
    pub leading_parameters: Vec<String>,
}
```

**Identity is (chunk id, record id); addressability is separate and optional.** Phase 1 answer 7
separated two things the original question conflated. *Identity* is always available and cheap: a
`ChunkId`, stored **once per batch** rather than per row, plus the `Id`-role field. *Addressability* —
constructing a query that returns exactly one record — is the optional half.

A row must be **retrievable**, not merely identified. `chunk` is the guaranteed path — re-evaluate
and index by the `Id` field — and `locator` is the direct one when a projection can offer it
(`-R/f.csv/-/ns-csv/row-42`). `info` is optional because **a CSV row has no `AssetInfo`; the file
does**, and one per source rather than per row also keeps 656 bytes from repeating.

### No `RecordSet`: the materialized form is a batch

**Removed during the Phase 2 review.** The draft carried a `RecordSet { batches: Vec<RecordBatch>,
truncated, diagnostics }` described as "the bounded result that a command returns and a `Value`
carries" — while the value variants carry a single `RecordBatch` and a `RecordStream`. Both cannot be
true, and `RecordSet` is the one that loses: a bounded sequence of batches is either a
`RecordStream` that happens to be finite, or a single batch after `concat`. A third name for it earns
nothing.

`truncated` moves onto `RecordStream` as a producer's report that it stopped early.

**`Diagnostics` is removed too, and its content goes where Liquers already puts evaluation facts.**
"How many records were scanned" and "you named a field no schema declares" are facts about an
*evaluation*, not about *data*; putting them in the value forces every consumer of a batch to carry a
field it does not want. `Metadata` already owns per-evaluation reporting through `LogEntry`, with
`Info`, `Debug`, `Warning` and `Error` kinds (`metadata.rs:495-552`). So a producer logs `scanned`
as `Info` and each unresolvable field as `Warning`.

The cost, stated: a log entry is a string, so `unavailable_fields` stops being a structured list. That
is accepted because `LogEntry` is already this project's answer for this class of information
everywhere else, and the consumer is a person or an agent reading why nothing matched. **The search
design must be updated to match** — it referenced `Diagnostics::unavailable_fields`.

### Search evidence is columns, not a parallel vector

Revision 6 of the search design left this open: should a search result be a `RecordSet` with a
parallel `matches: Vec<Vec<ClauseMatch>>`, or a distinct type wrapping one? **The split answers it:
neither.** A record set that carries a field only searches fill is a record set that knows what a
clause is, and the whole point of extracting this design is that it does not.

Search evidence is expressed the way every other per-row fact is — **as columns**, added to the
result schema with role `Stored`:

| Column | Type | Meaning |
|---|---|---|
| `match.clauses` | `UInt` | Bitmask; bit *i* set when clause *i* of the predicate admitted this row |
| `match.excerpt` | `Text`, nullable | The best excerpt, when a text clause produced one |
| `match.score` | `Float`, nullable | `Null` until a scoring clause exists |

This is strictly better than either option that was on the table: a search result composes with any
record consumer with no unwrapping, the evidence serializes as CSV or NDJSON like everything else,
and `RecordSet` loses a field. **The cost, stated:** the bitmask caps a predicate at **64 clauses**,
which is far past anything a person or an MCP tool writes, and the cap is a documented error rather
than a silent truncation. Multiple excerpts per row are not representable — one is kept — which is
the same trade every search UI makes.

### Field resolution: qualified names, ambiguity is an error

`status` is the asset lifecycle on `MetadataRecord` and a document's lifecycle in front-matter. So a
record's field names are **qualified at projection time**: `meta.` (`meta.status`,
`meta.type_identifier`, `meta.file_size`, `meta.updated`), `attr.` (application attributes, when they
exist), `key.` (`key.path`, `key.name`, `key.extension`). A consumer matches a qualified name
**exactly**, so a field reference arriving from HTTP or MCP is unambiguous by construction.
Unqualified names are a front-end convenience that a consumer's parser may expand, **failing with an
error naming every candidate** when more than one source could supply it.

This convention is owned here because it is a property of *projection* — of how an asset becomes
rows — not of any one consumer.

## Three abstractions: source, stream, chunk

**Revision 2 of Phase 2.** The previous draft had two value forms and one `RecordStream` type with a
`Manifest | Opaque` backing. That conflated two different things, and separating them is `Iterable`
vs `Iterator` — or, in Rust, `IntoIterator` vs `Iterator`:

| | Role | Shareable | Serializable | Consumed by use |
|---|---|---|---|---|
| **`RecordSource`** | can be asked, repeatedly, for a stream of chunks | **yes** | **yes**, as a manifest | no |
| **`RecordStream`** | one traversal, in flight | **no** — may be partly consumed | not itself; its *data* can be drained to a table | **yes** |
| **`RecordChunk`** | a materialized table | **yes** | **yes** — csv, parquet, ndjson, json | no |

Conversions run cheaply in one direction:

```
RecordSource  --stream()-->  RecordStream  --collect()-->  RecordChunk
     ^                                                          |
     +---------------- from_chunk() (in-memory backing) --------+
```

A chunk becomes a stream with `stream::once`, and a source with an in-memory backing — both cheap,
as the brief requires. A stream does **not** become a source: the information is gone.

### What this cleanup buys: `rewind` disappears

The previous draft gave `RecordStream` a `rewind()` that succeeded on a manifest backing and failed on
an opaque one — a fallible method whose success depended on how the value happened to be constructed,
which is exactly the kind of thing that is discovered at runtime by a user who did not read the
documentation.

With the split there is nothing to rewind. **If you hold the source, ask it for a new stream; if you
hold only a stream, you have one pass.** Phase 1 answer 2 asked for "optionally rewindable, cloneable
in some cases" — this delivers it as a property of *which type you are holding*, checked by the
compiler, instead of as a method that sometimes returns an error.

`StreamBacking` disappears with it. What was `Manifest` is now the source's serialized form; what was
`Opaque` is now simply a stream nobody kept a source for.

### Naming

`RecordStreamProducer` says what it does but is a mouthful, and the `…Producer` suffix is not used
anywhere in this codebase. Candidates considered:

| Name | For | Against |
|---|---|---|
| **`RecordSource`** | Short; reads naturally ("a record source yields a stream"); `…Source` is the conventional Rust name for a factory of this kind | **Collides with the existing `ChunkOrigin`**, which means something different — where a *row* came from |
| `LazyTable` | Immediately legible to anyone who knows polars' `LazyFrame` → `collect()` → `DataFrame`; pairs with "materialized table" for the chunk | "Lazy" describes a property rather than the thing; the type is a description, not a deferred computation |
| `RecordTable` | Clean trio: Table (whole, lazy) / Stream (traversal) / Chunk (piece) | "Table" implies one schema, and §"Schema uniformity is declared" explicitly does **not** promise one |
| `IntoRecordStream` | Matches `IntoIterator` exactly | Works as a *trait* name, awkward as the name of a concrete value type |

**Recommended: `RecordSource`, and rename `ChunkOrigin` → `ChunkOrigin`.** The collision is worth
resolving rather than dodging, because `ChunkOrigin` was always a vague name for what it holds — the
asset query, the chunk query, an optional `AssetInfo` and an optional locator, all of which describe
*where a chunk's rows originated*. `ChunkOrigin` says that; `ChunkOrigin` never did. `IntoRecordStream`
is then available as the trait a type implements to be usable as a source.

**This is a naming decision, not an architectural one** — the three-way split stands whichever names
are chosen.

### The types

```rust
// liquers-core/src/records/mod.rs

/// Something that can be asked, repeatedly, for a stream of chunks.
/// The `Iterable` of this design: shareable, serializable, never consumed by use.
#[derive(Debug, Clone)]
pub struct RecordSource {
    backing: SourceBacking,
    /// Declared, not assumed — see §"Schema uniformity is declared".
    pub uniform_schema: Option<Arc<RecordSchema>>,
}

#[derive(Debug, Clone)]
enum SourceBacking {
    /// One query per chunk — the serialized form, and the prototype's manifest generalized
    /// from store keys to queries.
    Manifest(Vec<Query>),
    /// Chunks already in memory. What `RecordChunk::into_source()` produces.
    Materialized(Vec<Arc<RecordBatch>>),
}

impl RecordSource {
    /// Open a fresh traversal. Callable any number of times — this is what replaces `rewind`.
    pub async fn stream(&self, context: &Context<impl Environment>)
        -> Result<RecordBatchStream<'_>, Error>;
    /// Chunk descriptors without producing any records — the reconciliation primitive.
    pub fn chunks(&self) -> Vec<ChunkDescriptor>;
    /// The manifest, when there is one. `None` for a materialized source.
    pub fn manifest(&self) -> Option<&[Query]>;
}
```

`RecordStream` is **not a struct**. It stays the type alias it already was:

```rust
/// One traversal. Not `Clone`, not `Serialize` — and now it does not need to pretend to be,
/// because it is no longer a value anybody stores.
pub type RecordBatchStream<'a> = BoxStream<'a, Result<RecordBatch, Error>>;
```

**Its data is still serializable, which is the distinction Phase 1 answer 2 drew.** Draining a stream
to csv or parquet is a perfectly good operation — it is the *handle* that cannot be stored or shared,
not the rows. That is what `records_to_csv` does, and why it consumes its input.

### The stream is `futures::Stream`, not a bespoke trait

`futures = "0.3.34"` is **already a direct dependency of `liquers-core`** (`Cargo.toml:77`, used in
`assets.rs`), so the standard trait costs nothing to adopt and a hand-rolled `next_batch` would be a
worse version of it. The whole combinator vocabulary comes with it: applying a filter is `filter_map`,
a limit is `take`, and concurrency is later `buffer_unordered` rather than a rewrite.

`liquers-core/src/maybe_send.rs` already solves the wasm half, and its own documentation states the
trap: a trait-object bound cannot use the `MaybeSend` marker (E0225), so the **whole boxed type is
aliased per target**, and `FutureExt::boxed()` is "always `Send`-boxed and thus wrong on wasm".
`StreamExt::boxed()` has the identical defect, so the sibling alias and boxing helper are added
beside the existing ones:

```rust
// liquers-core/src/maybe_send.rs — mirroring BoxFuture / MaybeBoxed exactly
#[cfg(not(target_arch = "wasm32"))]
pub type BoxStream<'a, T> = Pin<Box<dyn Stream<Item = T> + Send + 'a>>;
#[cfg(target_arch = "wasm32")]
pub type BoxStream<'a, T> = Pin<Box<dyn Stream<Item = T> + 'a>>;

/// Box a stream with the target-correct Send-ness. Replaces `StreamExt::boxed()`,
/// which is always `Send`-boxed and therefore wrong on wasm.
pub trait MaybeBoxedStream<'a>: Stream + Sized + 'a {
    fn maybe_boxed(self) -> BoxStream<'a, Self::Item>;
}
```

**No new trait is introduced.** `ChunkedRecordSource` was retired in the previous revision as
redundant against the manifest, and the split confirms that judgement: `RecordSource::chunks()` is
the partition, as data. A trait would only earn its place for a source whose partition is
**discoverable only incrementally** — a paginated remote API that reveals the next page token after
reading a page — which is real, out of scope, and addable later as another `SourceBacking`.

### Three scales, unchanged

| Scale | Unit of | Bounded by |
|---|---|---|
| **Record** (row) | retrieval and identity | — |
| **Batch** | **memory** — what is resident at once | a row count or a byte budget |
| **Chunk** | **refresh** — what expires and is re-produced together | the source's own partitioning |

A multi-gigabyte table is one source, partitioned into chunks; each chunk streams as batches, and
**one batch at a time is resident**. A chunk is therefore not materialized on traversal, which is the
point: a single large parquet file is one chunk, and holding it in memory is what this design exists
to avoid. `RecordChunk` — the *materialized* form — is the deliberate exception, used when a chunk is
small enough to be a value.

### Provenance and validity: the chunk carries a `Metadata`

The brief asks records to trace provenance and validity per chunk, flyweighted to the record. The
mechanism exists already and is not reinvented:

```rust
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ChunkDescriptor {
    pub id: ChunkId,
    /// The query that re-produces this chunk.
    #[serde(with = "query_format")]
    pub query: Query,
    /// Provenance and validity. `Metadata` already carries `query`, `version`,
    /// `dependencies: Vec<DependencyRecord { key, version }>`, `status` and `updated`.
    pub metadata: Metadata,
    pub origin: ChunkOrigin,
    /// Optional and advisory; field roles are its valuable content.
    pub schema: Option<RecordSchema>,
}
```

So **provenance is "the query and the dependency versions this chunk was produced from"**, and
**validity is the staleness check the dependency manager already performs**. A record's provenance is
its chunk's — that is the flyweight, and it costs one `Source`-role column index per row rather than
a `Metadata` per record.

**Reconciliation, not push.** A consumer holding a copy of a chunk compares `(ChunkId, Version)`
pairs against a fresh `partition()` and re-opens what differs. Correctness comes from the set
difference; any notification mechanism is a latency optimization on top, never the thing correctness
depends on. This is what the search design's
[`interoperability-layer.md`](../store-and-asset-search/interoperability-layer.md) §3 builds on, and
why `partition()` must not produce records.

### Value extension — `ExtValue` in `liquers-lib`, not `Value` in core

**Revised by Phase 1 answer 3, then corrected during the Phase 2 review.** Answer 3 replaced the
single `Value::Records` variant with two forms, on the Python prototype's evidence that a stream
whose chunk sequence is **known in advance** is perfectly cloneable, cacheable and serializable,
because what travels is a list of queries rather than any data.

That part stands. Where the draft was wrong was the crate: **it put both variants on
`liquers_core::value::Value`, where they cannot compile.**

`Value` derives `Serialize, Deserialize, Debug, Clone, PartialEq` and is `#[serde(untagged)]`
(`value.rs:20-21`). Every variant must satisfy all five, and the opaque backing satisfies none of the
hard ones:

| Requirement | a stream handle, however wrapped |
|---|---|
| `Clone` | `Mutex<T>` is not `Clone` |
| `PartialEq` | `BoxStream` is not comparable |
| `Serialize` | no meaningful bytes |
| **`Deserialize`** | **impossible in principle** — a generator cannot be reconstructed from bytes, at any amount of effort |

`Arc`-wrapping does not rescue it; the derive demands the bounds transitively. And `untagged` makes a
half-measure worse, since deserialization tries each variant in turn.

**The right home was already prescribed.** `CLAUDE.md` says new value types are `ExtValue` variants
in `liquers-lib/src/value/mod.rs`, and `ExtValue` derives **only `Debug, Clone`** (`mod.rs:23`). It
already holds exactly these shapes — `Arc<dyn UIElement>`, and `Arc<Mutex<dyn WidgetValue>>` behind
the `egui` feature.

```rust
// liquers-lib/src/value/mod.rs
pub enum ExtValue {
    // … existing …
    /// A materialized table. Shareable, cacheable, serializable.
    RecordChunk { value: Arc<RecordBatch> },
    /// Something that can be asked, repeatedly, for a stream. Shareable, serializable
    /// as a manifest, never consumed by use.
    RecordSource { value: Arc<RecordSource> },
}
```

**Only two variants, and neither is hazardous.** The three-way split removes the value that caused
the trouble: a stream is never an `ExtValue`, because a stream is a traversal in flight and a value
is something you can hold, clone and cache. That was the whole difficulty the previous revision
worked around with `Clone`-shares-one-stream semantics and a `Serialize` that failed at runtime.
**Both of those workarounds are now deleted rather than documented.**

Three problems dissolve at once: `Clone` clones the `Arc` regardless of contents, `PartialEq` is not
required, and serialization stops being a derive.

The form table is §"Three abstractions: source, stream, chunk"; it is not repeated here.

**Where the hazard went.** Phase 1 answer 2 warned that a stream "may be already partly consumed",
that sharing it should be avoided, and that stream commands would therefore be `volatile`. With
sources as values and streams confined to the inside of a command, **the hazard has nowhere to
appear**: what a command receives and returns is a source, which is re-openable by construction, and
the stream it opens lives and dies inside that call.

`volatile` is therefore no longer the normal case for a record command — it is needed only when a
result genuinely should not be reused, which is a separate question from streaming. This is a real
simplification over the previous revision, where volatility was load-bearing for correctness.

**Serialization becomes a fallible method, which is exactly the semantics needed.**
`DefaultValueSerializer::as_bytes(&self, data_format: &str) -> Result<Vec<u8>, Error>`
(`value.rs:919-926`) is per-format and may refuse — and `ExtValue::UIElement` already refuses this
way with `ErrorType::SerializationError` (`mod.rs:309-316`):

| Value | `as_bytes` |
|---|---|
| `RecordChunk` | `json`, `ndjson`, `csv`, later `parquet` and `arrow` |
| `RecordSource` (manifest) | `json` — the query list |
| `RecordSource` (materialized) | `json`, `ndjson`, `csv` — it holds the chunks, so it can serialize as data |

Both variants serialize in every case, so **no refusal arm is needed at all** — another thing the
split removes. Where an error *is* constructed anywhere in this module it uses a **typed
constructor**, `Error::from_error(ErrorType::SerializationError, …)` as the neighbouring
`ExtValue::UIElement` arm does, never `Error::new`, which `CLAUDE.md` forbids. Worth stating because
`value.rs:956` and `:968` reach for `Error::new` for serialization errors today; this module does
not copy that.

So "a manifest serializes and an opaque stream does not" needs **no new mechanism**; it is one match
arm in a method that already exists. The arms are enumerated rather than caught by `_ =>`, matching
the comment already in that match about adding a variant being a compile error.

**wasm is unaffected.** `liquers-web` depends on `liquers-lib` with
`default-features = false, features = ["webui"]` (`liquers-web/Cargo.toml:15`), so `ExtValue` is
reachable from the browser without polars — which is the requirement that drove the DataFrame role in
the first place. Neither variant is feature-gated: their only dependency is `bytemuck`, so unlike
`PolarsDataFrame` they need no `#[cfg]`, and no `match` on `ExtValue` needs a gated arm.

**What the prototype adds that this design did not have.** `_store_batches` rewrites its manifest
after *every* batch, so a run that dies partway leaves its finished chunks usable. A generator cannot
be checkpointed; a growing `Manifest` can. The equivalent here is a `store_record_stream` command
that appends a query to the manifest and rewrites it per chunk — cheap, and the difference between a
failed six-hour job being worthless and being resumable.

**What it does not carry over.** The prototype's manifest holds store *keys*, so it can `contains`,
list and clean its own directory before rewriting. Queries are more general — they cover a computed
chunk, which keys cannot — and the cost is that **cleanup is no longer automatic**. A manifest whose
queries point at stored chunks knows nothing about removing them. That is accepted, and the
`store_record_stream` command owns its own directory hygiene instead.

**Registration** is the unchanged four-step procedure: extend `ExtValue`; choose the identifiers
(`RecordChunk` and `RecordSource`, bare CamelCase — Liquers owns both concepts); implement the
conversions in `ExtValueInterface` and `DefaultValueSerializer`; and add both `TypeInfo` entries to
`ExtValue::type_descriptions()` (`mod.rs:148`) — `CLAUDE.md`'s "four steps, not three; a type with no
`TypeInfo` cannot be stored".

**Everything lives in `liquers-lib`, behind the `records` feature** — see §"Integration Points".
The data types moved up with the value variants in revision 5, closing the question this paragraph
used to carry: records are a data type, not a core capability, and `liquers-core` ends up untouched
apart from two generic stream aliases.

**Serializing the multi-gigabyte case** still meets `VALUE-SERIALIZATION-HAS-NO-INCREMENTAL-WRITER`:
`as_bytes` returns a `Vec<u8>`, so NDJSON or CSV over a large stream cannot stream through it. A
`Manifest` sidesteps the problem — the manifest itself is small — which is one more reason to prefer
that form.

## Trait Implementations

| Trait | For | Note |
|---|---|---|
| `MaybeBoxedStream` | blanket over `Stream` | Mirrors the existing `MaybeBoxed` |
| `ExtValueInterface` conversions | `ExtValue::RecordChunk`, `ExtValue::RecordSource` | `from_*`/`as_*` arms, plus `into_stream` / `into_source`, per `TYPE_SYSTEM_GUIDE.md` |
| `DefaultValueSerializer` | `ExtValue::RecordChunk`, `ExtValue::RecordSource` | chunk: json / ndjson / csv. Stream: json for a manifest, `SerializationError` for an opaque one |

**No change to `AsyncStore` or `AssetManager`.** Record production is a *command* concern; a trait
method would be a push-down optimization, addable later without changing a consumer.

## Generic Parameters & Bounds

None on the data types — `FieldValue` is a dynamic enum precisely so `RecordBatch`, the stream and
the value variants stay concrete. The only bound in the module is `T: bytemuck::Pod` on `Buffer<T>`,
which is what makes the aligned cast safe.

## Function Signatures

### `liquers-core/src/records/mod.rs`

```rust
impl RecordSchema {
    /// Fails unless exactly one field has role `Id`. Checked once per schema rather than per row.
    pub fn new(fields: Vec<FieldSchema>) -> Result<Self, Error>;
    pub fn id_field(&self) -> usize;
    pub fn source_field(&self) -> Option<usize>;
    pub fn text_fields(&self) -> &[usize];
    pub fn index_of(&self, name: &str) -> Option<usize>;
}

impl RecordBatch {
    /// Zero-copy column projection.
    pub fn select(&self, columns: &[usize]) -> Result<RecordBatch, Error>;
    /// Gather by mask — the filter primitive every consumer builds on.
    pub fn filter(&self, mask: &Bitmap) -> Result<RecordBatch, Error>;
    /// Offset + length on every buffer; `Arc`-shared, no copy.
    pub fn slice(&self, offset: usize, len: usize) -> Result<RecordBatch, Error>;
    pub fn concat(batches: &[RecordBatch]) -> Result<RecordBatch, Error>;
    /// Single-cell read, for a consumer that wants one value rather than a column.
    pub fn value(&self, row: usize, column: usize) -> Result<FieldValue, Error>;
    /// Append columns to the schema and the batch — how a consumer adds derived fields.
    pub fn with_columns(&self, fields: Vec<FieldSchema>, columns: Vec<Column>)
        -> Result<RecordBatch, Error>;
}

impl ChunkOrigin {
    /// Build the directly evaluable query for one row, when `locator` allows.
    pub fn locator_query(&self, id: &FieldValue) -> Option<Query>;
}

pub struct RecordBatchBuilder { /* … */ }

impl Bitmap {
    pub fn get(&self, i: usize) -> bool;
    pub fn and(&self, other: &Bitmap) -> Result<Bitmap, Error>;
    pub fn or(&self, other: &Bitmap) -> Result<Bitmap, Error>;
    pub fn not(&self) -> Bitmap;
    pub fn count_ones(&self) -> usize;
}
```

### `liquers-core/src/records/buffer.rs`

```rust
impl AlignedBuffer {
    pub fn from_slice(bytes: &[u8]) -> Self;
    pub fn as_bytes(&self) -> &[u8];
    /// 64-byte aligned, as Arrow recommends. Asserted in a test.
    pub fn alignment() -> usize;
}

impl<T: bytemuck::Pod> Buffer<T> {
    pub fn from_slice(values: &[T]) -> Self;
    pub fn as_slice(&self) -> &[T];
    pub fn as_bytes(&self) -> &[u8];
}
```

## Integration Points

### Crate placement: `liquers-lib`, behind a `records` feature

**Settled in revision 5: the whole feature is `liquers-lib`, behind a `records` feature.** It is in
essence a new data type, not an essential capability — the same class as `image-support` and
`polars`, and `CLAUDE.md` already directs new value types to `liquers-lib/src/value/`.

**`liquers-core` is therefore untouched, with one small exception.** That is a better outcome than
the previous revisions reached for: the design becomes purely additive to one crate, and a build with
`records` off is byte-for-byte the build that exists today.

| Crate | File | Change |
|---|---|---|
| `liquers-lib` | `src/records/buffer.rs` (new) | `AlignedBuffer`, `Buffer<T>`, `Bitmap` — the Arrow-layout primitives; the only place `bytemuck` is used |
| `liquers-lib` | `src/records/mod.rs` (new) | `FieldValue`, `RecordSchema`, `Column`, `RecordBatch`, `ChunkOrigin`, `RecordSource`, `SourceBacking`, `RecordBatchStream`, `ChunkDescriptor` |
| `liquers-lib` | `src/records/commands.rs` (new) | The `ns-records` command set |
| `liquers-lib` | `src/records/polars.rs` (new, `records` + `polars`) | `RecordBatch → polars::DataFrame` over the shared buffers |
| `liquers-lib` | `src/value/mod.rs` | `ExtValue::RecordChunk` and `ExtValue::RecordSource`, **cfg-gated**, with every exhaustive match gaining a gated arm; both `TypeInfo` entries; the `DefaultValueSerializer` arms |
| `liquers-lib` | `Cargo.toml` | the `records` feature and its optional `bytemuck` dependency |
| **`liquers-core`** | `src/maybe_send.rs` | **The one exception** — `BoxStream` + `MaybeBoxedStream`, ungated. Justified below |
| `liquers-web` | `src/records.rs` (new) | The `RecordChunk` handle, per-column descriptors, `columnCopy`; the JS companion that revalidates views |
| `liquers-web` | `Cargo.toml` | add `"records"` to the `liquers-lib` feature list |
| `liquers-py` | later milestone | Arrow C Data Interface export — the only place `unsafe` FFI belongs |
| `specs` | `command_registry.yaml` | Regenerated |

### The one change to `liquers-core`, and why it is not records-specific

`BoxStream` and `MaybeBoxedStream` go in `liquers-core/src/maybe_send.rs`, **ungated**, beside the
existing `BoxFuture` and `MaybeBoxed`. That module exists precisely to hold per-target boxed-type
aliases and to document the E0225 reason they must be aliased rather than bounded. `StreamExt::boxed()`
has the same always-`Send` defect the module already warns about for `FutureExt::boxed()`, so the
alias is a general async utility that any crate may want — not a records concept.

Defining it in `liquers-lib` instead would duplicate the E0225 workaround and leave core's own
documented trap half-addressed. Two type aliases and one blanket trait, no dependency, no feature: a
smaller cost than the duplication.

### The feature

```toml
# liquers-lib/Cargo.toml
[features]
default = ["egui", "image-support", "polars", "records"]
# Columnar record streams: a tabular value type with an Arrow-compatible layout.
# Optional because it is a data type rather than an essential capability.
records = ["dep:bytemuck"]

[dependencies]
bytemuck = { version = "1.25", optional = true }
```

**In `default`, exactly as `polars` is** — so the routine loop
(`cargo test -p liquers-lib --lib --tests`) exercises it, which is the only way the tests actually
run. Being in `default` is not the same as being mandatory: the matrix below proves the feature is
cleanly optional, and `polars` is the established precedent for precisely this arrangement.

**`bytemuck` is now optional and gated**, which is strictly better than revision 3's finding. It was
going to be a new *unconditional* direct dependency of `liquers-core`; it is now an optional
dependency of `liquers-lib`, pulled only when `records` is on. A build without the feature adds no
dependency at all.

### Feature-gating discipline

A cfg-gated enum variant is the classic way to break a build that was not tested, and `ExtValue`
already carries the scar tissue — its `as_bytes` match has a comment recording that a previous
catch-all "silently absorbed new variants". Every exhaustive match on `ExtValue` therefore needs a
`#[cfg(feature = "records")]` arm, in `type_name`, `type_identifier`, `as_bytes`,
`type_descriptions` and the `ExtValueInterface` conversions.

`scripts/check-build-matrix.sh` gains the rows that prove it, mirroring the existing per-feature rows:

```
--no-default-features --features records --tests
--no-default-features --features records,polars --tests     # the polars bridge
--no-default-features --features webui,records --tests
--target wasm32-unknown-unknown --no-default-features --features webui,records
```

and the existing `--no-default-features --tests` row already proves the build with `records` **off**.
Test files that need the feature carry `#![cfg(feature = "records")]` at file level, as the existing
optional-dependency test files do.

### What this settles, and what it costs

This closes the open question carried since revision 1. It also **moves the search design's
predicate**: `SearchPredicate` operates on `RecordBatch`, so with records in `liquers-lib` the
predicate cannot live in `liquers-core` either. That is no loss — the search commands were always
`liquers-lib` — and it means the search design too becomes additive to one crate.

The cost, stated: a consumer wanting records without the rest of `liquers-lib` cannot have them,
because the crate is the unit of dependency. That is acceptable while `liquers-lib` is where command
libraries live anyway, and if a `liquers-records` crate is ever wanted, the module is already
self-contained enough to lift out.

## Documentation Architecture

Two new documents, fully specified here so the authoring is mechanical.

**They are written in Phase 5, not now.** `CLAUDE.md` defines `specs/reference/` as "how the system
is; **must be true at HEAD**" — a reference document describing an unimplemented API would be false
the moment it lands. The specification below is the contract they are written against; per §9.2 each
carries a `## History` row and a `reviewed:` date from its first commit.

### Reference: `specs/reference/RECORD_STREAMS.md`

`kind: reference` · audience contributor **and agent** · area `lib/value` · status follows the design.

| § | Content |
|---|---|
| **Concepts** | The three abstractions and *why* they are three: `RecordSource` (asked repeatedly, shareable, serializable as a manifest), `RecordStream` (one traversal, never a value), `RecordChunk` (materialized table). The conversion diagram. The `Iterable`/`Iterator` analogy stated once, plainly |
| **Scales** | Record / batch / chunk — unit of retrieval, of memory, of refresh — and why conflating them breaks either memory or refresh |
| **Schema** | `RecordSchema`, `FieldSchema`, `FieldType`, `FieldRole`. The two orthogonal axes and which integration target each serves. The exactly-one-`Id` rule and that it is checked in `RecordSchema::new` |
| **Field naming** | The `meta.` / `attr.` / `key.` qualification, why `status` forced it, and that ambiguity is an error naming every candidate |
| **Identity and retrieval** | `(chunk id, record id)`. `ChunkOrigin`: `chunk` as the guaranteed path, `locator` as the direct one, `info` absent for a non-asset source. **Why a CSV row has no `AssetInfo` and the file does** |
| **Provenance and validity** | The `Metadata` per chunk; provenance as "the query and dependency versions this came from"; validity as the existing staleness check; the flyweight to record level |
| **Memory layout** | The columnar form, `Column` variants, `Bitmap`'s three uses, `AlignedBuffer` and 64-byte alignment. **Cites [COLUMNAR] per claim** |
| **Arrow interoperability** | The two-level model — data as `&[T]`, structure rebuilt as `repr(C)` — the exact type/format mapping table, the three export routes and their real costs, and the three places the layout is not 1:1. **Cites [COLUMNAR] and [CDATA]**; states that format strings are verified against the spec, not this document |
| **Browser sharing** | Hazards A and B, the identity check, the refresh rule, read-only views, the handle lifetime, and `columnCopy` as the fallback |
| **Methods** | Every public method with its contract and failure mode — `RecordSchema::{new, id_field, source_field, text_fields, index_of}`, `RecordBatch::{select, filter, slice, concat, value, with_columns}`, `RecordSource::{stream, chunks, manifest}`, `Bitmap::{get, and, or, not, count_ones}`, `ChunkOrigin::locator_query` |
| **Serialization** | What each value writes in each format, and that a manifest source writes its query list while a materialized one writes data |
| **Limits** | The Arrow subset supported and what is excluded; uniformity not promised and the two operations that need it; the feature gate |

Links out to `VALUE_TYPE_SYSTEM.md`, `STORE_SEMANTICS.md` for the key semantics it inherits, and the
design folder for *why*.

### Guide: `specs/guides/RECORD_STREAM_GUIDE.md`

`kind: guide` · audience contributor · area `lib/value` · workflow **"produce records from a new
source"**.

| § | Content |
|---|---|
| **Choose your shape first** | A decision table: one row per asset → return a chunk; a directory of files → a manifest source; a huge single file → a source whose stream yields batches. Getting this wrong is the expensive mistake, so it comes first |
| **Walkthrough: a command producing records** | **The guide's spine.** End to end, from an empty file to a passing test: define the schema (with its `Id` field and roles), build the batch with `RecordBatchBuilder`, fill `ChunkOrigin` so results are retrievable, return `ExtValue::RecordChunk`, then register with `register_command!` — `context` last, `async fn` taking owned `State` — and regenerate `command_registry.yaml` |
| **Second walkthrough: a manifest source** | The CSV-directory case: one query per file, what `uniform_schema` to declare, and why a manifest is preferred over a generator (rewindable, cacheable, checkpointable) |
| **Choosing a batch size** | Rows vs bytes, and the memory arithmetic |
| **Using a batch as a DataFrame** | `select`/`filter`/`slice`/`concat` with masks; what is deliberately absent and where it lives instead |
| **Handing a batch to pandas or polars** | The polars path via `polars-arrow`; the pyo3 path; what "zero-copy" does and does not cover |
| **Reading a chunk from JavaScript** | The handle, the view-refresh rule, and when to reach for `columnCopy` |
| **Pitfalls** | The `len + 1` offsets invariant; `Vector` needing a child node; forgetting the `TypeInfo` entry (the type then cannot be stored); forgetting a `#[cfg(feature = "records")]` match arm (a build with the feature off fails); holding a JS view across a wasm call |
| **Testing** | Unit tests beside the code; the round-trip test per format; the `debug-handles` release assertion; the build-matrix rows |

Every snippet is taken from a real test in the implementation, so the guide cannot drift from
behaviour without a test failing.

### Existing documents to update

| Path | Change |
|---|---|
| `specs/reference/VALUE_TYPE_SYSTEM.md` | The `RecordChunk` and `RecordSource` identifiers, their `TypeInfo`s, and that both are feature-gated |
| `specs/guides/TYPE_SYSTEM_GUIDE.md` | Both variants in the worked list; the gated-variant case as a worked example, since it is the first optional value type after `polars` |
| `specs/guides/COMMAND_REGISTRATION_GUIDE.md` | A pointer to the record-producing walkthrough rather than a duplicate of it |
| `specs/README.md` | The capability-map entry, `designing` → `built` |
| `CLAUDE.md` | The `records` feature in the feature-matrix section, and the new matrix rows |

**Discarded candidates:** `STORE_SEMANTICS.md`, `STORE_IMPLEMENTATION_GUIDE.md` and
`CONFORMANCE_TERMS.md` — this design touches no store trait. `ASSETS.md` — the `get_asset_info`
repair belongs to the search design.

`affects_docs`: `reference/RECORD_STREAMS.md`, `guides/RECORD_STREAM_GUIDE.md`,
`reference/VALUE_TYPE_SYSTEM.md`, `guides/TYPE_SYSTEM_GUIDE.md`,
`guides/COMMAND_REGISTRATION_GUIDE.md`.

## Relevant Commands

Deliberately thin: this design owns the *mechanism*, and each consumer brings its own producers.

| Command | Signature | Purpose |
|---|---|---|
| `records_to_csv` | `fn records_to_csv(state) -> result` | Serialize a record set as CSV |
| `records_to_ndjson` | `fn records_to_ndjson(state) -> result` | Serialize a record set as NDJSON |
| `records_schema` | `fn records_schema(state) -> result` | The schema as a value — how an agent discovers field names |
| `records_head` | `fn records_head(state, n: i64 = 20) -> result` | Slice, for inspection |

Namespace `ns-records`. Producers (`ns-search/records`, a CSV projection, a parquet projection) are
owned by the designs that need them.

## Error Handling

All errors are `liquers_core::error::Error` via typed constructors. No `Error::new`, no new error
type, no `unwrap`/`expect`.

| Situation | Outcome |
|---|---|
| A schema without exactly one `Id` field | `Error::general_error` from `RecordSchema::new` |
| Column count or length disagrees with the schema or `len` | `Error::general_error` |
| `concat` of batches with different schemas | `Error::general_error` naming the first differing field |
| A mask whose length differs from the batch | `Error::general_error` |
| State is not an `ExtValue::RecordChunk` or `ExtValue::RecordSource` | `Error::conversion_error` |
| A field name no schema declares | Not an error — a `Warning` log entry on the evaluation's `Metadata` |
| Unreadable entry while producing records | Skipped, counted in an `Info` log entry |

## Sync vs Async Decisions

| Operation | Choice | Rationale |
|---|---|---|
| `RecordBatch` select/filter/slice/concat/value | sync | Pure, in-memory |
| `Bitmap` operations | sync | Pure |
| Schema construction and validation | sync | Pure |
| Opening a chunk from a manifest query | async | Evaluation and store access |
| Record-producing commands | async | Store access |
| Serialization to CSV / NDJSON | sync today | Becomes streaming once an incremental writer exists |

## Serialization Strategy

Every data type derives `Serialize, Deserialize`; `Query` uses the existing `query_format` helper.
`AlignedBuffer` and `Buffer<T>` serialize as their bytes — alignment is a memory property, not a
wire one, and is re-established on deserialization. The stream types are **not** serializable and
deliberately never cross a query boundary; the serializable forms are a `RecordBatch` and a
`RecordSource`.

## Concurrency Considerations

No shared mutable state. Buffers are `Arc`-shared and immutable once built, so `select`, `slice` and
a column hand-off copy nothing and are safe to share. Batch production is sequential; concurrency is
`buffer_unordered` over the chunk stream when there is something to measure. No lock is held across
an `.await`.

## Compilation Validation

- `Stream` is object-safe and already boxed through the project's per-target alias pattern;
  no new trait is introduced, so there is no object-safety question to answer.
- `BoxStream`/`MaybeBoxedStream` are gated on `target_arch`, **never** on a Cargo feature —
  `maybe_send.rs` documents why: feature unification would silently strip `Send` from the native
  build workspace-wide.
- Both variants are `Arc`-wrapped, so `size_of::<ExtValue>()` is unchanged and `Value` is untouched.
- `ExtValue` derives only `Debug, Clone`, so the opaque stream needs no `Serialize`, `Deserialize` or
  `PartialEq` — the reason these variants cannot live on `Value`.
- Every `match` on `Value` gains an explicit arm; no default arm anywhere.
- `liquers-core` gains no dependency and **no `unsafe`**; its only change is two ungated type aliases
  in `maybe_send.rs`.
- Every exhaustive `match` on `ExtValue` has a `#[cfg(feature = "records")]` arm, so
  `--no-default-features` still compiles — the failure mode a gated enum variant causes, and what the
  new build-matrix rows exist to catch.
- `bytemuck` is `optional = true` and reached only through `records`, so a build without the feature
  resolves an unchanged dependency graph.
- `Buffer<T>: bytemuck::Pod` holds for `i32`, `i64`, `u64`, `f32`, `f64`.
- The `records` feature gates the module, the variants, the matches and the dependency together —
  a partially-gated feature is the classic way to break a configuration nobody built.

## References to liquers-patterns.md

Async-by-default with `#[async_trait]`; `MaybeSend`/`MaybeSync` for wasm; typed error constructors;
explicit match arms with no default; `Arc` for shared payloads in `Value`; `TypeInfo` registration
as the fourth step of adding a value type; `context` last in a command signature.

## Open Questions for Phase 3

1. **Do the record *data* types belong in `liquers-core` or `liquers-lib`?** The value variants are
   settled (`ExtValue`, in lib). The data types are currently in core because they are plain, depend
   only on `bytemuck`, embed core types and are what the search predicate operates on. The
   alternative keeps core smaller and moves the predicate up with them. **The sharpest remaining
   question.**
2. What is the default batch size, and is it a row count or a byte budget? A byte budget is the
   honest answer for the multi-gigabyte case but needs a size estimate per column.
3. Is `with_columns` the right extension point for derived fields? The search design's evidence
   columns are its only user, and they apply to a `RecordChunk`.
4. `Manifest(Vec<Query>)` cannot clean up the chunks it points at, unlike the prototype's key list.
   Does `store_record_stream` need a companion that removes a manifest's stored chunks, or is that
   the caller's business?
5. Dropping `Diagnostics` costs `unavailable_fields` its structure — it becomes a `Warning` log
   entry. Is that enough for a UI that wants to offer "did you mean…", or does that case want
   structure back?
6. Does the 64-clause cap implied by a `UInt` evidence bitmask belong here or in the search design?

**Closed by the Phase 1 answers or by this review:** `FieldType` date types (answer 4 — `Date` now,
`Decimal` later); whether `ChunkedRecordSource` earns its place (**no** — `Manifest` is a partition
as data); whether `RecordSet` survives (**no** — a batch or a stream, not a third name); which crate
owns the value variants (**`liquers-lib`**, because `ExtValue` derives only `Debug, Clone`).
