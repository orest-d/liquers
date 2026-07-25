# WP-4 Design Exploration: How to Organize the Type System

The question: what is `type_identifier` *really*, given the ambiguities the design owner listed?
This document (a) reframes the ambiguities as a small number of orthogonal axes, (b) proposes
four coherent ways to organize the type space, and (c) recommends one.

`type_name`, `media_type`, and file extension are treated as **projections/defaults** (derived,
non-authoritative) and left out of the core model — except that one alternative gives `type_name`
a real job.

---

## 1. Reframe: the ambiguities are three orthogonal axes

Every ambiguity listed is a confusion between three independent things:

- **Kind `K`** — the *abstract logical type*: `text`, `integer`, `dictionary`, `dataframe`,
  `image`. Portable across deployments. This is "what the data means."
- **Representation `R`** — the *concrete in-memory realization* of a kind in one deployment:
  `polars`, `pandas`, `arrow`, or the list-of-dicts fallback for `dataframe`. Deployment-local.
- **Data format `F`** — the *byte encoding*: `json`, `csv:comma`, `parquet`, `txt`. Already a
  first-class field.

Serialize is `(value, F) → bytes` (the value already fixes `K` and `R`).
Deserialize is `(bytes, K, [R], F) → value`.

### The listed ambiguities, explained by the axes

| Owner's observation | What it really is |
|---|---|
| one format represents many types (json → string/bool/int/object/array) | one `F` serves many `K`; so `F` only gives a *default* `K` (rule R4), the actual `K` must be recorded (R1) |
| one type stored as different formats (string as json or text) | one `K` serves many `F`; `K` and `F` are orthogonal (R1) |
| text formats (json, yaml) are themselves strings | a **refinement lattice** on `K`: `text` sits below `json-value`; parsing text *refines* it |
| same bytes interpreted as different values (csv → dataframe or list-of-dicts) | two different `K` refinements above the same bytes; choosing one is a **deliberate decode**, not an accident |
| dataframe = pandas / polars / arrow | one `K` (`dataframe`), many `R`; the `R` axis is exactly this |

### The kind lattice (why "text formats are also strings")
Decoding proceeds up a refinement lattice, each step needing a decoder:

```
bytes  ≤  text  ≤  structured (json-value / yaml-value)  ≤  domain (dataframe, image, …)
  ▲ universal bottom: every file decodes to bytes (rule R4's limit)
```

Moving up the lattice is **monotone and explicit**: `text → json-value` is a parse, `csv-bytes →
dataframe` is a decode. This is the single most useful idea here: *reinterpreting bytes as a
higher kind is an operation, not a property of the bytes.* It turns the "same bytes, different
values" ambiguity into a deliberate, discoverable choice.

---

## 2. Four alternatives for encoding `K`/`R` in metadata

All four keep `F` (`data_format`) as-is and keep the rules R1–R7. They differ in how the
`type_identifier` space is structured.

### Alt A — Flat canonical kind + representation registry
`type_identifier = K` only (`"dataframe"`, `"text"`). No representation in metadata. A per-deployment
registry maps `(K, F) → constructor` for *that deployment's* representation.
- **Deserialize:** `registry.construct(K, F, bytes)` → the deployment's default representation of `K`.
- **Idempotence (R5):** holds — `K` is canonical; a non-default representation collapses to the
  default on the first roundtrip, then stable.
- **Portability:** full — any deployment that knows `K` and can read `F` reconstructs into its own `R`.
- **Cost:** metadata schema unchanged. **Loss:** cannot preserve a *specific* representation across a
  roundtrip (a `pandas` value stored and reloaded comes back as the deployment default, say `polars`).

### Alt B — Hierarchical identifier `"K"` or `"K/R"`
`type_identifier` is a path: bare `dataframe` = abstract kind; `dataframe/polars` = concrete
representation. A value uses the **most specific** id its representation has.
- **Deserialize:** try exact `K/R`; if `R` unknown here, fall back to `K`'s default representation;
  if `K` unknown, fall back down the lattice (ultimately `bytes`).
- **Idempotence:** holds (most-specific id is canonical and stable).
- **Portability:** graceful degradation — unknown `R` still resolves via `K`.
- **Cost:** metadata schema unchanged (still one string), just a parsing convention. **Gain over A:**
  preserves representation when the loading deployment supports it.

### Alt C — Two explicit fields: `type_kind` + `representation`
Split the axes into two metadata fields. `type_kind = K` (portable, authoritative for dispatch);
`representation = R` (optional, deployment-local hint).
- **Deserialize:** `(type_kind, representation?, F)` → prefer `representation` if available, else `K`
  default.
- **Realizable on existing fields — the elegant option:** let **`type_identifier = K`** and
  **reuse `type_name` as `R`**. `type_name` stops being a vague "debug string" (its current problem
  P2) and becomes the concrete representation, which is exactly what `polars_dataframe` already is.
  Matches the trait TODOs (`identifier → type_identifier`, `type_name → detailed_type_identifier`).
- **Idempotence / portability:** same as Alt B.
- **Cost:** no *new* field if we reuse `type_name`; but it redefines `type_name`'s meaning and
  requires auditing everyone who reads it as a debug label.

### Alt D — Minimal canonical type + explicit reinterpretation commands (a *modifier*, not a rival)
`type_identifier` records only the **canonical decode target** — the single kind the bytes
deterministically become (`bytes` / `text` / `json-value` / the default domain kind). Any
*cross-kind* reinterpretation (csv→`records` vs csv→`dataframe`) is an **explicit query command**
(`…/data.csv/-/dataframe`), never implicit.
- **Effect:** removes accidental ambiguity (owner's point 4) at the type-system level; deserialization
  is a pure function with no interpretation choices.
- **Combine with A/B/C** for the representation axis. This is a *principle* to adopt alongside
  whichever schema wins, not a standalone schema.

---

## 3. Comparison

| Criterion | A (flat kind) | B (`K/R` path) | C (two fields / reuse `type_name`) | D (explicit casts) |
|---|---|---|---|---|
| Metadata schema change | none | none (convention) | none if reuse `type_name` | none |
| Preserves specific representation | ✗ | ✓ | ✓ | n/a |
| Portable across deployments | ✓ | ✓ (degrades) | ✓ (degrades) | ✓ |
| Idempotence R5 | ✓ | ✓ | ✓ | ✓ (strongest) |
| Kills accidental reinterpretation | partial | partial | partial | ✓ |
| Gives `type_name` a real purpose | ✗ | ✗ | ✓ | ✗ |
| Migration from `"polars_dataframe"` | alias→`dataframe` | split string | `id=dataframe`,`name=polars` | independent |
| Conceptual clarity | good | good | **best** | **best** (as add-on) |

---

## 4. Recommendation

**Adopt Alt C realized on existing fields, plus Alt D as a standing principle:**

1. **`type_identifier` = kind `K`** — abstract, portable, canonical (R7). Dispatch key with `F`.
2. **`type_name` = representation `R`** — concrete, deployment-local, optional (defaults to `K`).
   This finally gives `type_name` a crisp definition and absorbs today's `"polars_dataframe"`.
3. **A data-format registry** (already proposed) provides `F → default K`, `F → media_type`, etc.;
   a parallel **kind/representation registry** provides `(K, F) → constructor` and the `K`-default
   representation. Both are serializer/value-provided so `liquers-lib` extends them.
4. **Cross-kind reinterpretation is always an explicit command** (Alt D): deserialization yields
   exactly the recorded `K`; turning csv into a dataframe vs. a record list is a query step.
5. **Lattice fallback for metadata-less reads:** unknown `K` degrades down `domain → structured →
   text → bytes`, so rule R4 ("every file decodes, ultimately to bytes") always holds.

Why this one: it is the only option that (a) separates the three axes cleanly, (b) makes stored
assets portable, (c) preserves a specific representation when possible, (d) repurposes `type_name`
into something meaningful instead of deleting it, and (e) removes accidental ambiguity via explicit
casts — all with **no new metadata field**.

### Scope note
- **WP-4 (now):** rules R1–R7, the data-format registry, `validate_for_storage`, normalization,
  idempotence tests. These do **not** require the `K`/`R` split — they work with today's flat
  identifiers.
- **Follow-up WP (the D6 portability work):** introduce the `K`/`R` split (`type_identifier=kind`,
  `type_name=representation`), the kind/representation registry, and the alias migration from
  `"polars_dataframe" → (dataframe, polars)`. This is where "broader class + specific
  representation" (owner's point 5) lands.

## Open questions for the owner
1. Do we want representation **preserved** across roundtrips (B/C) or is collapsing to a deployment
   default (A) acceptable? (Determines whether `type_name`-as-representation is worth it.)
2. Is reinterpretation-as-explicit-command (D) acceptable UX, or must some casts stay implicit
   (e.g. auto-parse `.json` files into structured values on load)?
3. Should the kind lattice be **explicit** (a declared partial order used for fallback) or just an
   informal decode chain?
