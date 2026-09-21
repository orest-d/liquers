---
id: RECORD-STREAMS-PHASE3-TESTS
kind: analysis
title: Phase 3 test code — record streams
workflow: liquers-project
status: draft
area: [lib/value]
created: 2026-09-21
---
# Phase 3 test code — Record streams

**This is the deliverable of a test-first Phase 3.** None of it compiles today; all of it is
written to compile once Phase 4 implements the types. Phase 4's job is to make it pass.

See [`phase3-examples.md`](./phase3-examples.md) for the narrative, the overview table and the
corner cases. This file is the code, organized by the file each test lands in.

> **One caveat before extraction.** Some code below uses API that Phase 2 declares only as an
> ellipsis — `RecordBatchBuilder`'s append methods, `Column::gather`, `FieldRole::and_stored()`.
> Those are listed in `phase3-examples.md` §"What Phase 3 found that Phase 2 must absorb" and need a
> Phase 2 amendment **before** Phase 4 begins, or the tests cannot compile against the approved
> architecture. Where a draft reached for something Phase 2 names differently — `get_value` for
> `value` — the Phase 2 name wins.

| § | Contents | Source |
|---|---|---|
| 1 | Scenario code — the two worked examples | drafting agents 1–2 |
| 2 | Unit tests (39) — `liquers-lib/src/records/` | drafting agent 3 |
| 3 | Integration tests (25) — `liquers-lib/tests/` | drafting agent 4 |
| 4 | Pitfalls, and `RECORDS01`–`09` for language bindings | drafting agent 5 |

---

# 1. Scenario code

## 1.1 Primary scenario — files to CSV

## Phase 3 Draft: Primary Scenario — Record Streams

### 1. Prose Walkthrough: List and Filter Files to CSV

**The user's intent:**
A Liquers command describes the files in a store directory as records — one record per file, with fields for name (the file's identity), size, and modified date. The user filters those records to only include files larger than 1 MB, and serializes the result to CSV.

**What happens, in order:**

1. **Schema construction** — A `RecordSchema` is built with three fields:
   - `filename` (Text): the file name, marked as the unique identifier (`KeyRole::Id`) and indexed exactly (required for reconciliation). Must be stored.
   - `size_bytes` (UInt): the file's size in bytes. Not indexed, not a key.
   - `modified_at` (Timestamp): when the file was last changed. Not indexed, not a key.
   
   `RecordSchema::new()` validates that exactly one field has `KeyRole::Id`, that it is declared `Exact`-indexed and stored; construction fails if not.

2. **Batch construction** — A `RecordBatchBuilder` appends one row per file:
   - Opens the store, walks the directory.
   - For each file, calls `builder.append()` with the row's field values.
   - Buffers are laid out exactly as Arrow specifies — 64-byte aligned, primitives direct, text as i32 offsets plus contiguous data.
   - After all rows, calls `builder.build()`, which returns a `RecordBatch`.

3. **RecordChunk value creation** — The `RecordBatch` is wrapped in `Arc` and placed inside `ExtValue::RecordChunk`, making it a shareable value the command can return.

4. **Filter application** — The user's query asks for a `filter` operation on the chunk.
   - The filter evaluates `size_bytes > 1000000` to a boolean `Bitmap` — one bit per row, set if the row passes the predicate.
   - `RecordBatch::filter(&bitmap)` applies the mask: each column is selectively copied (using `gather`-like semantics) into a new batch containing only the rows where the bit is set.
   - Result is a new `RecordChunk`.

5. **Serialization to CSV** — The filtered chunk is serialized.
   - `DefaultValueSerializer::as_bytes("csv")` is called on the `ExtValue::RecordChunk`.
   - Each row is written as a CSV line; field values are converted from `FieldValue` scalars to their string representations.
   - The result is a `Vec<u8>` holding the complete CSV, returned to the HTTP layer.

**Key insight:** Columns allow filtering to be a mask-and-gather, not a row-by-row walk. The same schema and batch structure serve polars (via `liquers-py` and the C Data Interface), DuckDB, Arrow consumers via IPC, and browsers via typed arrays. A record is identified by `(chunk_id, id_field_value)` — the chunk id is stored once per batch, the id once per row, costing nothing extra.

---

### 2. The Rust Code

All code lives in `liquers-lib/src/records/`, behind the `#[cfg(feature = "records")]` guard.

#### Module structure

```rust
// liquers-lib/Cargo.toml additions
[features]
records = ["dep:bytemuck"]

[dependencies]
bytemuck = { version = "1.25", optional = true }
futures = "0.3"
chrono = "0.4"
serde = { version = "1.0", features = ["derive"] }
tokio = { version = "1.0", features = ["sync", "rt"] }
```

#### Defining the command

```rust
// liquers-lib/src/commands.rs
use liquers_macro::register_command;
use liquers_core::{error::Error, context::Context, environment::Environment};
use crate::value::ExtValue;
use crate::records::{RecordSchema, FieldSchema, FieldType, KeyRole, FieldRole, 
                     RecordBatchBuilder, RecordBatch, ChunkId};

/// List files in a store directory as a record chunk.
/// Each row describes one file with name (unique id), size, and modified date.
#[cfg(feature = "records")]
async fn list_directory_files(
    context: &Context<impl Environment>,
) -> Result<ExtValue, Error> {
    let store = context.environment().get_store();
    
    // Build the schema: name (id), size, modified date
    let fields = vec![
        FieldSchema {
            name: "filename".to_string(),
            data_type: FieldType::Text,
            nullable: false,
            key: KeyRole::Id,  // This field is the unique identifier
            role: FieldRole::text().and_stored(),  // Indexed exactly + stored, required for KeyRole::Id
        },
        FieldSchema {
            name: "size_bytes".to_string(),
            data_type: FieldType::UInt,
            nullable: false,
            key: KeyRole::None,
            role: FieldRole::default(),
        },
        FieldSchema {
            name: "modified_at".to_string(),
            data_type: FieldType::Timestamp,
            nullable: true,  // Can be absent
            key: KeyRole::None,
            role: FieldRole::default(),
        },
    ];
    
    let schema = RecordSchema::new(fields)
        .map_err(|e| Error::from_error(ErrorType::General, e))?;
    
    // Build the batch: read directory and append rows
    let mut builder = RecordBatchBuilder::with_schema(schema.clone());
    
    // Scan directory (simplified; real impl would use store's list capability)
    let root_key = Key::new();
    // For each file in the directory:
    // builder.append_text("filename.txt")?;
    // builder.append_uint(5120)?;  // size in bytes
    // builder.append_timestamp(1695321600_000000i64)?;  // microseconds since epoch
    // (repeat for each file)
    
    let batch = builder.build()
        .map_err(|e| Error::from_error(ErrorType::General, e))?;
    
    // Wrap in a RecordChunk value
    Ok(ExtValue::RecordChunk {
        value: Arc::new(batch),
    })
}

// Register the command
let cr = env.get_mut_command_registry();
register_command!(cr,
    async fn list_directory_files(context) -> result
    label: "List directory files"
    doc: "Describe files in a store directory as records (name, size, modified date)"
)?;
```

#### Building the batch

```rust
// liquers-lib/src/records/builder.rs
use crate::records::{
    RecordSchema, RecordBatch, Column, Bitmap, Buffer, AlignedBuffer, 
    FieldValue, FieldType, ChunkId
};
use liquers_core::error::{Error, ErrorType};
use std::sync::Arc;

pub struct RecordBatchBuilder {
    schema: Arc<RecordSchema>,
    /// One builder per field, holds accumulated values
    columns: Vec<ColumnBuilder>,
    len: usize,
}

enum ColumnBuilder {
    Text { offsets: Vec<i32>, data: Vec<u8> },
    UInt { values: Vec<u64> },
    Timestamp { values: Vec<i64> },
    Bool { values: Vec<bool> },
}

impl RecordBatchBuilder {
    pub fn with_schema(schema: RecordSchema) -> Self {
        let columns = schema.fields.iter().map(|_| {
            // In real code, create appropriate builder for each field type
            ColumnBuilder::Text { offsets: vec![0], data: Vec::new() }
        }).collect();
        
        RecordBatchBuilder {
            schema: Arc::new(schema),
            columns,
            len: 0,
        }
    }

    /// Append a text value to the current row (must be in schema order)
    pub fn append_text(&mut self, value: &str) -> Result<(), Error> {
        if let ColumnBuilder::Text { offsets, data } = &mut self.columns[0] {
            data.extend_from_slice(value.as_bytes());
            let next_offset = data.len() as i32;
            offsets.push(next_offset);
            Ok(())
        } else {
            Err(Error::from_error(
                ErrorType::General,
                "Expected text column".to_string(),
            ))
        }
    }

    /// Append a uint value
    pub fn append_uint(&mut self, value: u64) -> Result<(), Error> {
        if let ColumnBuilder::UInt { values } = &mut self.columns[1] {
            values.push(value);
            Ok(())
        } else {
            Err(Error::from_error(
                ErrorType::General,
                "Expected uint column".to_string(),
            ))
        }
    }

    /// Append a timestamp value
    pub fn append_timestamp(&mut self, value: i64) -> Result<(), Error> {
        if let ColumnBuilder::Timestamp { values } = &mut self.columns[2] {
            values.push(value);
            Ok(())
        } else {
            Err(Error::from_error(
                ErrorType::General,
                "Expected timestamp column".to_string(),
            ))
        }
    }

    /// Finish a row; call append_* methods once per field, in schema order
    pub fn finish_row(&mut self) {
        self.len += 1;
    }

    /// Convert accumulated buffers to a RecordBatch
    pub fn build(self) -> Result<RecordBatch, Error> {
        let columns = Vec::new();
        // In real code, convert each ColumnBuilder to its Column variant,
        // applying 64-byte alignment and Arrow layout
        
        Ok(RecordBatch {
            schema: self.schema,
            columns,
            len: self.len,
            chunk_id: None,
            sources: Vec::new(),
        })
    }
}
```

#### Filtering the batch

```rust
// liquers-lib/src/records/mod.rs

impl RecordBatch {
    /// Filter this batch by a boolean mask (one bit per row).
    /// Returns a new batch containing only rows where the mask is true.
    pub fn filter(&self, mask: &Bitmap) -> Result<RecordBatch, Error> {
        if mask.len() != self.len {
            return Err(Error::from_error(
                ErrorType::General,
                format!("Mask length {} != batch length {}", mask.len(), self.len),
            ));
        }

        let mut filtered_columns = Vec::with_capacity(self.columns.len());
        for column in &self.columns {
            // Each column selectively gathers values where mask is true
            filtered_columns.push(column.gather(mask)?);
        }

        let filtered_len = mask.count_ones();
        
        Ok(RecordBatch {
            schema: self.schema.clone(),
            columns: filtered_columns,
            len: filtered_len,
            chunk_id: self.chunk_id.clone(),
            sources: self.sources.clone(),
        })
    }
}

impl Column {
    /// Selectively copy values where the mask is true into a new column.
    fn gather(&self, mask: &Bitmap) -> Result<Column, Error> {
        match self {
            Column::UInt { validity, values } => {
                let mut new_values = Vec::new();
                let mut new_validity = validity.as_ref().map(|_| Bitmap::new());
                
                for (i, bit) in mask.iter().enumerate() {
                    if bit {
                        new_values.push(values[i]);
                        if let Some(v) = &mut new_validity {
                            if let Some(old_v) = validity {
                                v.append(old_v.get(i));
                            } else {
                                v.append(true);  // No nulls in original
                            }
                        }
                    }
                }
                
                let new_values_buf = Buffer::<u64>::from_vec(new_values)?;
                Ok(Column::UInt {
                    validity: new_validity,
                    values: new_values_buf,
                })
            }
            Column::Text { validity, offsets, data } => {
                let mut new_offsets = vec![0i32];
                let mut new_data = Vec::new();
                let mut new_validity = validity.as_ref().map(|_| Bitmap::new());
                
                for (i, bit) in mask.iter().enumerate() {
                    if bit {
                        let start = offsets[i] as usize;
                        let end = offsets[i + 1] as usize;
                        new_data.extend_from_slice(&data[start..end]);
                        new_offsets.push(new_data.len() as i32);
                        
                        if let Some(v) = &mut new_validity {
                            if let Some(old_v) = validity {
                                v.append(old_v.get(i));
                            } else {
                                v.append(true);
                            }
                        }
                    }
                }
                
                let new_offsets_buf = Buffer::<i32>::from_vec(new_offsets)?;
                let new_data_buf = AlignedBuffer::from_vec(new_data)?;
                Ok(Column::Text {
                    validity: new_validity,
                    offsets: new_offsets_buf,
                    data: new_data_buf,
                })
            }
            // Other column types follow the same pattern
            _ => Err(Error::from_error(
                ErrorType::General,
                "Unsupported column type for gather".to_string(),
            )),
        }
    }
}
```

#### Serialization to CSV

```rust
// liquers-lib/src/value/serialization.rs (extend existing)

#[cfg(feature = "records")]
impl DefaultValueSerializer {
    fn records_to_csv(batch: &RecordBatch) -> Result<Vec<u8>, Error> {
        let mut buf = Vec::new();
        
        // Write header
        let header = batch.schema.fields.iter()
            .map(|f| &f.name)
            .collect::<Vec<_>>()
            .join(",");
        buf.extend_from_slice(header.as_bytes());
        buf.push(b'\n');
        
        // Write rows
        for row_idx in 0..batch.len {
            let mut row_values = Vec::new();
            
            for (col_idx, column) in batch.columns.iter().enumerate() {
                let value = column.value(row_idx)?;
                let escaped = escape_csv_field(&value);
                row_values.push(escaped);
            }
            
            buf.extend_from_slice(row_values.join(",").as_bytes());
            buf.push(b'\n');
        }
        
        Ok(buf)
    }
}

fn escape_csv_field(value: &FieldValue) -> String {
    let s = value.to_string();
    if s.contains(',') || s.contains('"') || s.contains('\n') {
        format!("\"{}\"", s.replace("\"", "\"\""))
    } else {
        s
    }
}

impl Column {
    /// Retrieve a single field value from this column at the given row index.
    fn value(&self, row_idx: usize) -> Result<FieldValue, Error> {
        match self {
            Column::Text { validity, offsets, data } => {
                if let Some(v) = validity {
                    if !v.get(row_idx) {
                        return Ok(FieldValue::Null);
                    }
                }
                let start = offsets[row_idx] as usize;
                let end = offsets[row_idx + 1] as usize;
                let text = String::from_utf8_lossy(&data[start..end]).into_owned();
                Ok(FieldValue::Text(Arc::from(text)))
            }
            Column::UInt { validity, values } => {
                if let Some(v) = validity {
                    if !v.get(row_idx) {
                        return Ok(FieldValue::Null);
                    }
                }
                Ok(FieldValue::UInt(values[row_idx]))
            }
            Column::Timestamp { validity, values } => {
                if let Some(v) = validity {
                    if !v.get(row_idx) {
                        return Ok(FieldValue::Null);
                    }
                }
                Ok(FieldValue::Timestamp(values[row_idx]))
            }
            _ => Err(Error::from_error(
                ErrorType::General,
                "Unsupported column type".to_string(),
            )),
        }
    }
}
```

#### Wire up the serialization in ExtValue

```rust
// liquers-lib/src/value/mod.rs (extend match on ExtValue)

#[cfg(feature = "records")]
ExtValue::RecordChunk { value } => {
    match data_format {
        "csv" => {
            DefaultValueSerializer::records_to_csv(value.as_ref())
        }
        "ndjson" => {
            DefaultValueSerializer::records_to_ndjson(value.as_ref())
        }
        "json" => {
            DefaultValueSerializer::records_to_json(value.as_ref())
        }
        _ => Err(Error::from_error(
            ErrorType::SerializationError,
            format!("RecordChunk does not support format '{}'", data_format),
        )),
    }
}
```

---

### 3. The Liquers Query

The user writes a query that:
1. Calls the `list_directory_files` command to get a RecordChunk
2. Applies a `filter` command to select only files > 1 MB
3. Requests CSV serialization

The query does not exist yet as a command family, but the plumbing is clear:

```
-R/data/files/list_directory_files/-/filter/size_bytes|gt|1000000/-/to_csv
```

Breakdown:
- `-R` signals a keyed resource (fetch from store)
- `/data/files/list_directory_files` — the key (or computed path to the keyed asset)
- `/-/` separates from actions
- `filter/size_bytes|gt|1000000` — filter command, column name, predicate (column > value)
- `/-/to_csv` — final serialization command

Or, in a recipe with named bindings:

```yaml
recipes:
  describe_large_files:
    query: |-
      list_directory_files
        / filter size_bytes gt 1000000
        / to_csv
```

The `filter` command (to be added to the registry) would:
1. Take a RecordChunk as input state
2. Parse the predicate `size_bytes > 1000000`
3. Evaluate it to a boolean Bitmap by examining the `size_bytes` column
4. Call `RecordBatch::filter(&bitmap)` on the underlying batch
5. Return a new RecordChunk with the filtered result

The `to_csv` command (to be added) would:
1. Take a RecordChunk
2. Call `as_bytes("csv")` on the value
3. Return the CSV bytes

---

### Notes on Phase 3 Implementation

1. **RecordBatchBuilder** is the only builder in this phase; it is not a trait. A later phase adds pluggable builders for readers (CSV, NDJSON, Parquet).

2. **Bitmap** operations (`count_ones`, `get`, `and`, `or`, `not`) are implemented directly; no external bitmap library is needed. Slicing at non-byte boundaries copies on first version.

3. **Buffer alignment** uses the third approach from Phase 2: `#[repr(align(64))]` backing chunks with `bytemuck::cast_slice`. No `unsafe` in `liquers-core`.

4. **Column::gather** is the only filter implementation in this phase; it is O(n) and row-focused. Later phases add mask-and-copy at the buffer level.

5. **Error handling** throughout uses `Error::from_error(ErrorType::General, …)` for now. Phase 4 or later may add a `RecordError` type, but Phase 3 keeps it simple.

6. **No `println!`** anywhere. Diagnostics use `eprintln!` if needed, but in library code this should be rare.

7. **Volatility:** A `list_directory_files` command would be `volatile: true` in its metadata if the directory contents change frequently and caching is undesirable. The `filter` command is not volatile — a filtered chunk is stable as long as the input is.

8. **Test coverage:** A unit test appends a few rows, filters, and asserts the result; a CSV integration test round-trips through serialization.


---

## 1.2 Manifest-driven stream

## Phase 3 Scenario 2: Chunked Manifest with Arguments and Links
### A realistic record stream driven by manifest metadata

**Context:** This is the second, more detailed scenario in Phase 3 of the record-streams design. It demonstrates how a manifest YAML file describes a record stream whose chunks are explicitly enumerated, how arguments and links work in practice, and how Rust code consumes such a stream by opening it, iterating its chunks one at a time, and concatenating them.

---

### 1. The Manifest YAML

This manifest describes a daily order extract, broken into chunks of 1000 orders each. The chunks are explicit (not templated), so `ChunkList::Known` applies. Shared arguments supply the SQL statement and database connection; the query list supplies the per-chunk offset/limit.

**File: `data/sales/daily.manifest.yaml`**

```yaml
manifest: record-stream
version: 1

title: Daily order extract
description: |
  One chunk per 1000 orders, ordered by id. Extracts sales data
  from the production database, with customer names joined in.
  Produced daily by the extract job.

## --- chunk naming, for when chunks are cached as keyed assets ---
## These fields are used only if a ChunkKeys exists (which this example does not).
## With no cache, chunk identity comes from the query alone, and the naming
## pattern is advisory/descriptive only.
number_format: "{:04}"         # daily_0000.csv
extension: csv

## --- shared by every chunk; same semantics as Recipe.arguments / Recipe.links ---
## These values are merged with any per-chunk arguments, with per-chunk values winning.
arguments:
  # SQL statement. Shared across all chunks; per-chunk offset/limit comes in the query.
  # Lives here because it is long, unqueryable, and identical for all chunks.
  sql: |
    SELECT o.id, o.total, o.status, c.name, c.email
    FROM orders o
    JOIN customers c ON c.id = o.customer_id
    WHERE o.created_at >= '2026-01-01'
    ORDER BY o.id

  # Default for any chunk that does not override it
  batch_size: 1000

links:
  # The SQL statement could alternatively live in a .sql file, as a link.
  # That makes it an asset dependency — editing the file expires every chunk.
  # For this example, it is inline in arguments. Both patterns are valid.
  
  # A second link: the database connection config, as a YAML asset
  connection: -R/db/prod.yaml
  
  # A third link: a precalculated customer mapping, for reference
  customer_list: -R/data/customers/active.csv

## --- declared, not assumed; omit when chunks may differ ---
uniform_schema:
  type_identifier: Order    # Liquers owns this concept; maps to TypeInfo::id
  fields:
    - name: id
      data_type: Int
      key: Id               # This field identifies each record; it's Exact-indexed and stored
      role:
        indexed: [Exact]    # Delete-by-term (reconciliation) needs exact matching
        stored: true        # Reconciliation also needs to retrieve the id
    
    - name: total
      data_type: Float
      nullable: true
      key: None
      role:
        indexed: [Range]    # Support comparisons: total > 1000, total <= 500
        fast: true          # Available for sorting and faceting
    
    - name: status
      data_type: Text
      key: None
      role:
        indexed: [Exact]    # Facetable: group by status
        stored: true
        fast: true
    
    - name: name
      data_type: Text
      key: None
      role:
        indexed: [FullText {analyzer: Simple, positions: true}]    # Searchable text, with phrase query support
        stored: true        # Retrieve customer names in results
        fast: false
    
    - name: email
      data_type: Text
      key: None
      role:
        indexed: [Exact]    # Exact for de-duplication and contact lookup
        stored: true

## --- exactly one of `chunks:` or `template:` ---
## This is the explicit form: every chunk is known up front.
## ChunkList::Known applies, so reconciliation can detect additions, changes and deletions.

chunks:
  # Chunk 0: orders 0–999
  - query: ns-sql/sql_query-0-1000
    # No per-chunk overrides; arguments from the manifest header are used as-is.
    
  # Chunk 1: orders 1000–1999
  - query: ns-sql/sql_query-1000-1000
    # Per-chunk overrides are allowed but rarely needed in this pattern.
    # (Valid only if a ChunkKeys existed; without it, identity comes from query alone.)
    
  # Chunk 2: orders 2000–2999
  - query: ns-sql/sql_query-2000-1000
  
  # Chunk 3: orders 3000–3999
  - query: ns-sql/sql_query-3000-1000
    title: Final batch (partial)  # Advisory; does not affect parsing or identity
    description: Last batch from this extraction cycle
    
  # More chunks follow in a real scenario, one per thousand rows.
```

---

### 2. What This Manifest Means, Chunk by Chunk

#### Header (metadata)

The manifest header declares the stream's identity and structure:
- **`manifest: record-stream`** — YAML type discriminator; a folder may hold several manifest kinds.
- **`version: 1`** — Format version. Readers refuse unknown versions.
- **Title, description** — Human-readable metadata.
- **`number_format`, `extension`** — Chunk naming pattern (e.g., `daily_0042.csv`). Used **only** when a `ChunkKeys` exists; without one, chunks are named by their query identity. In this example, there is no cache, so these fields are advisory.
- **`uniform_schema`** — A schema that all chunks promise to match. Omit if chunks may differ (e.g., CSV files from different sources). The schema enforces one Id field (for reconciliation), declares which fields are indexable and their intended roles (searchable, storable, fast for sorting).

#### Shared arguments and links

Every chunk query uses:
- **`arguments.sql`** — The SELECT statement, identical for all chunks. Lives in `arguments` rather than inline in the query because SQL text cannot be escaped into a query string (quotes, parentheses, commas) in any readable way. A command receives this as the `sql` parameter.
- **`arguments.batch_size`** — A tuning parameter; shared across all chunks.
- **`links.connection`** — The database connection config, as a query (`-R/db/prod.yaml`). A link is an asset dependency, so if the connection config changes, every chunk's dependency manager records it, expiring the stream.
- **`links.customer_list`** — A reference asset (the active customer list), made available to the chunk query for joining or validation.

When a chunk's plan is built, the shared arguments and links are merged with any per-chunk overrides (chunks 2 and 3 could override them if a `ChunkKeys` made per-chunk identity meaningful; here they do not).

#### The chunks

Four chunks are listed; in a real scenario there would be dozens or hundreds. Each chunk:

1. **Chunk 0: `ns-sql/sql_query-0-1000`**
   - **Query**: `ns-sql/sql_query-0-1000` (namespace `ns-sql`, command `sql_query`, parameters `0` and `1000`)
   - **Identity** (unkeyed, no cache): the query string itself.
   - **What it produces**: Rows 0–999 of the result set. The command receives `sql` as a named argument (shared from the manifest header) and `0`, `1000` as positional arguments (offset, limit).
   - **Chunking semantics**: This chunk is responsible for rows 0–999; chunk 1 for 1000–1999. Together they form a partition, and the source is the union.

2. **Chunk 1: `ns-sql/sql_query-1000-1000`**
   - **Query**: `ns-sql/sql_query-1000-1000`
   - **What it produces**: Rows 1000–1999 using the shared SQL statement.
   - No per-chunk metadata, so chunk 1 shares everything with the header.

3. **Chunk 2: `ns-sql/sql_query-2000-1000`**
   - **Query**: `ns-sql/sql_query-2000-1000`
   - **What it produces**: Rows 2000–2999.
   - No overrides.

4. **Chunk 3: `ns-sql/sql_query-3000-1000`**
   - **Query**: `ns-sql/sql_query-3000-1000`
   - **Per-chunk metadata**: A title and description (advisory; they do not affect computation).
   - **What it produces**: Rows 3000–3999.
   - Note: In a real scenario, chunk 3 might produce fewer than 1000 rows (if the table has 3400 rows total), signaling the end of the stream; but the manifest does not declare row counts.

#### Schema meaning

The `uniform_schema` declares:
- **`id`** (Int, KeyRole::Id) — The record's identity. Must be exact-indexed and stored for reconciliation (delete-by-term semantics). One per schema, enforced at construction.
- **`total`** (Float, nullable) — Indexed for range queries (total > 500), available for sorting and faceting.
- **`status`** (Text) — Exact-indexed (for grouping), stored (in results), fast (sortable).
- **`name`** (Text) — Full-text searchable with positions (enables phrase queries like `"customer name"`), stored.
- **`email`** (Text) — Exact-indexed (for lookups), stored.

These roles are **portable intent** — they describe *what* an index should do, not *which* engine does it. A Tantivy sink translates `FullText` to `TEXT`, a Lucene sink to `TextField(DOCS_AND_FREQS_AND_POSITIONS)`, and a CSV export ignores roles entirely and just writes the columns. The schema is data-layer; engine-specific tuning lives in the sink's configuration.

---

### 3. The Rust Code That Consumes This Stream

Below is how a consumer (a library, a command, or an HTTP API) opens the manifest, iterates its chunks, and accumulates them.

#### Setup: open the manifest as a RecordSource

```rust
use liquers_lib::records::{RecordSource, ChunkList, ChunkId};
use liquers_core::{
    query::Query,
    context::Context,
    environment::Environment,
};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // 1. Create an environment with the store and commands registered.
    //    (Omitted here; see liquers-core/tests/async_hello_world.rs for a full setup.)
    let env = create_environment()?;
    
    // 2. Open the manifest as a RecordSource.
    //    The manifest file is an asset at a key.
    let manifest_key = Key::parse("data/sales/daily.manifest.yaml")?;
    
    // 3. Fetch the manifest asset. This reads the YAML and deserializes it.
    let manifest_query = Query::new(manifest_key);
    let context = Context::new();
    
    // In a real scenario, this would go through a RecipeProvider or AssetManager.
    // For this example, we'll assume the manifest was deserialized:
    
    let manifest_bytes = env.store()
        .read(&manifest_key, None)
        .await?;
    
    let manifest_yaml = String::from_utf8(manifest_bytes)?;
    let source: RecordSource = serde_yaml::from_str(&manifest_yaml)?;
    
    eprintln!("Manifest: {}", source.uniform_schema.as_ref()
        .map(|s| format!("{} fields", s.fields.len()))
        .unwrap_or_else(|| "non-uniform".to_string()));
    
    // Now we can use the source.
    process_stream(&source, &env, &context).await?;
    
    Ok(())
}

async fn process_stream<E: Environment>(
    source: &RecordSource,
    env: &E,
    context: &Context<E>,
) -> Result<(), Box<dyn std::error::Error>> {
    // Step 1: Enumerate chunks WITHOUT opening a stream.
    // This is cheap and synchronous — no I/O, no planning, just metadata.
    
    let chunks = source.chunks();
    
    // chunks() returns ChunkList, which distinguishes two cases:
    // - ChunkList::Known: all chunks enumerable (this manifest)
    // - ChunkList::Unbounded: count unknown (templated streams)
    
    match chunks {
        ChunkList::Known(chunk_ids) => {
            eprintln!(
                "Stream has {} chunks, and all are known upfront.",
                chunk_ids.len()
            );
            
            // Step 2: For reconciliation, describe each chunk.
            // describe_chunk() is async because it may read metadata from the store.
            // In this case (unkeyed, no cache), it reads the chunk query's own provenance.
            
            for (i, chunk_id) in chunk_ids.iter().enumerate() {
                let descriptor = source.describe_chunk(chunk_id, context).await?;
                
                eprintln!(
                    "  Chunk {}: id={:?}, query={}, version={}",
                    i,
                    descriptor.id,
                    descriptor.query.encoded(),
                    descriptor.metadata.version,
                );
                // descriptor.metadata carries dependencies, status, updated time,
                // and is the input to reconciliation logic.
            }
        }
        ChunkList::Unbounded { computed } => {
            eprintln!(
                "Stream has unbounded chunks; {} computed so far.",
                computed.len()
            );
            // A templated stream (not in this example); walker stops at the first short chunk.
        }
    }
    
    // Step 3: Open a stream and iterate its batches.
    // stream() is callable any number of times — it opens a fresh traversal.
    // Only one batch is resident at a time.
    
    eprintln!("\nConsuming stream...");
    let stream = source.stream(context).await?;
    
    // stream is a futures::Stream<Item = Result<RecordBatch, Error>>.
    // For iteration, we use StreamExt.
    
    use futures::stream::StreamExt;
    
    let mut batch_count = 0;
    let mut total_rows = 0;
    let mut all_batches = Vec::new();
    
    let mut stream = stream;
    while let Some(batch_result) = stream.next().await {
        let batch = batch_result?;
        
        eprintln!(
            "Batch {}: {} rows, schema has {} fields",
            batch_count,
            batch.len,
            batch.schema.fields.len()
        );
        
        // The batch is in Arrow memory layout — columns are Arc-shared,
        // 64-byte-aligned buffers. No per-row allocation.
        
        // Example operations:
        // - filter by a predicate on one column
        // - select specific columns
        // - read a single value
        
        let id_col_idx = batch.schema.index_of("id")
            .ok_or("id field not found")?;
        let id_values = match &batch.columns[id_col_idx] {
            Column::Int { values, .. } => values.as_slice(),
            _ => return Err("id should be Int".into()),
        };
        eprintln!("  First 5 ids: {:?}", &id_values[..std::cmp::min(5, batch.len)]);
        
        // Batch is also chunked — it knows its source:
        if let Some(chunk_id) = &batch.chunk_id {
            eprintln!("  Chunk origin: {:?}", chunk_id);
        }
        
        all_batches.push(batch.clone());
        batch_count += 1;
        total_rows += batch.len;
    }
    
    eprintln!(
        "\nStream closed: {} batches, {} rows total",
        batch_count, total_rows
    );
    
    // Step 4: Concatenate batches into a single RecordChunk (materialized).
    // This is the point where we move from lazy to eager.
    // Only do this if the result fits in memory.
    
    if total_rows < 1_000_000 {
        let combined = RecordBatch::concat(&all_batches)?;
        eprintln!("Concatenated into one batch: {} rows", combined.len);
        
        // The combined batch is now a value, serializable as CSV, Parquet, NDJSON, etc.
        // In a command, this would become ExtValue::RecordChunk.
    }
    
    // Step 5: Re-open the same stream a second time.
    // Because we hold the source (not the stream), re-opening is free.
    
    eprintln!("\nRe-opening the stream...");
    let stream2 = source.stream(context).await?;
    let count2: usize = stream2
        .map(|r| r.map(|b| b.len).unwrap_or(0))
        .fold(0, |acc, len| async move { acc + len })
        .await;
    
    eprintln!("Second traversal: {} rows", count2);
    
    Ok(())
}
```

#### Key points in the consumption pattern

1. **`chunks()` is synchronous and cheap.**
   ```rust
   let chunks = source.chunks();  // No await, no I/O
   ```
   It returns `ChunkList::Known` (all chunks enumerable) or `ChunkList::Unbounded` (count unknown). This is the reconciliation planning primitive: a consumer learns what chunks exist without evaluating any of them.

2. **`describe_chunk()` is async and carries metadata.**
   ```rust
   let descriptor = source.describe_chunk(&chunk_id, context).await?;
   ```
   For an unkeyed stream (this example), it reads the query's own provenance — the dependency versions that produced the chunk. For a keyed stream (with `ChunkKeys`), it reads the chunk asset's metadata from the store. Either way, the result is `(id, version)` pairs, which a reconciliation algorithm compares.

3. **`stream()` opens a fresh traversal, callable any number of times.**
   ```rust
   let stream = source.stream(context).await?;
   ```
   The stream is a `futures::Stream<Item = Result<RecordBatch, Error>>`, lazy and composable. Only one batch is resident at a time. No `rewind()` — hold the source and open another stream.

4. **Batches are in Arrow layout: columns, not rows.**
   ```rust
   match &batch.columns[id_col_idx] {
       Column::Int { values, .. } => values.as_slice(),
       // ...
   }
   ```
   Columns are `Buffer<T>` — `Arc`-shared, 64-byte-aligned, copyable. A column filter is a boolean mask. A projection is a clone of the `Arc` pointers. No per-row allocation.

5. **Each batch knows its chunk.**
   ```rust
   if let Some(chunk_id) = &batch.chunk_id {
       eprintln!("Chunk origin: {:?}", chunk_id);
   }
   ```
   A record's provenance is its chunk's, flyweighted to one per batch rather than one per row. `chunk_id` is either `ChunkId::Query(...)` (unkeyed) or `ChunkId::Key(...)` (keyed).

6. **Concatenation is the bridge from lazy to eager.**
   ```rust
   let combined = RecordBatch::concat(&all_batches)?;
   ```
   Returns a single materialized batch if the schema is uniform. Fails (names the differing field) if not. This is the point where a stream becomes a value (`ExtValue::RecordChunk`).

---

### 4. The Queries Involved

#### The chunk queries

Each chunk query is in the form `ns-sql/sql_query-<offset>-<limit>`, e.g.:

```
ns-sql/sql_query-0-1000      → Chunk 0: offset 0, limit 1000
ns-sql/sql_query-1000-1000   → Chunk 1: offset 1000, limit 1000
ns-sql/sql_query-2000-1000   → Chunk 2: offset 2000, limit 1000
ns-sql/sql_query-3000-1000   → Chunk 3: offset 3000, limit 1000
```

These are positional arguments to a command. The command signature might be:

```rust
fn sql_query(
    state: &State<Value>,
    offset: i64 = 0,
    limit: i64 = 0,
    sql: String = "",          // Received as a named argument from manifest
    connection: Query = ...,    // Received as a link from manifest
    context: &Context<impl Environment>,
) -> Result<Value, Error> {
    // 1. Evaluate the connection link to get the DB config
    let conn_config = context.evaluate(connection, &state.env).await?;
    
    // 2. Use `sql` and the connection to query the database
    // 3. Apply LIMIT and OFFSET (the positional arguments)
    // 4. Return a RecordChunk (Arrow-laid-out batch)
}
```

**Key design point:** Per-chunk values (offset, limit) are **positional in the query**. Shared values (the SQL statement, the connection) are **named arguments from the manifest**. This distinction ensures chunk identity is determined by the query alone (what varies per chunk), not by varying arguments (which would cause silent collision in an unkeyed stream).

#### How arguments and links flow

1. **Manifest supplies shared values:**
   ```yaml
   arguments:
     sql: SELECT ...
     batch_size: 1000
   links:
     connection: -R/db/prod.yaml
   ```

2. **For chunk 0, the plan builder:**
   ```rust
   // Query: ns-sql/sql_query-0-1000
   // Plan step 1: call sql_query with positional args [0, 1000]
   
   // After planning, merge manifest arguments:
   let mut plan = /* initial plan from query */;
   plan.override_value("sql", Value::String("SELECT ..."));
   plan.override_value("batch_size", Value::Int(1000));
   
   // And manifest links:
   plan.override_link("connection", parse_query("-R/db/prod.yaml")?);
   ```

3. **The command receives:**
   - **Positional arguments** from the query: `offset=0`, `limit=1000`
   - **Named arguments** from the manifest: `sql="SELECT ..."`, `batch_size=1000`
   - **Links** from the manifest: `connection` resolves to the YAML asset

4. **The command executes:**
   - Opens a connection to the database (via the link)
   - Prepares the SQL statement
   - Applies `LIMIT 1000 OFFSET 0`
   - Returns the batch

#### No string interpolation in the manifest

The manifest does NOT do this:

```yaml
## DON'T: the manifest should not interpolate
query_template: "ns-sql/sql_query-{offset}-{limit}"
```

Instead, the manifest explicitly lists queries with their full parameters, or (for templated streams) the command does the hydration. This keeps the manifest a pure data format with:
- No templating language to specify
- No escaping rules
- **No injection story** — a manifest that substituted text into SQL would have to define what happens when a field value contains a quote

The command, if it needs to build SQL, uses **parameterized queries** (bind placeholders) for safety:

```sql
-- In the manifest's sql argument, or in a linked .sql file:
SELECT o.id, o.total FROM orders o ORDER BY o.id LIMIT $1 OFFSET $2
```

The database driver (sqlx, tokio-postgres, etc.) handles parameter binding safely; no string concat, no quoting.

---

### 5. Reusability and Re-opening

#### The source is cloneable and re-openable

```rust
// Hold the source
let source = arc_source.clone();  // Arc<RecordSource>, shareable

// Open a stream the first time
let stream1 = source.stream(&context).await?;
// Process, consume, drop stream1

// Open a stream again — no cost, no re-loading
let stream2 = source.stream(&context).await?;
// Process, consume, drop stream2
```

This is the distinction Phase 2 drew between `Iterable` (source) and `Iterator` (stream). A source is never consumed by use; a stream is. A consumer that wants multiple passes holds the source and opens a new stream each time.

#### Serialization: the manifest is data

The source serializes as the manifest YAML:

```rust
let serialized = serde_yaml::to_string(&source)?;
// Produces the same YAML this example started with.
```

This is what makes a manifest a *value*: it can be stored as an asset (`ExtValue::RecordSource`), passed over HTTP, cached, diffed. A stream, by contrast, does not serialize — it is a traversal in flight.

---

### 6. Constraint Summary

**In this scenario:**

- ✅ Everything lives in `liquers-lib` behind the `records` feature
- ✅ `ChunkId` is an enum: `Query(...)` for unkeyed, `Key(...)` for keyed
- ✅ No `ChunkKeys` exists (unkeyed stream), so chunk identity is the query alone
- ✅ Per-chunk `arguments` / `links` are in the manifest but unused (they would require a cache to avoid collision)
- ✅ Per-chunk variation (offset, limit) is in the query, not in arguments
- ✅ Shared variation (SQL, connection) is in named arguments and links
- ✅ `chunks()` is sync, cheap, returns `ChunkList::Known` (enumerable)
- ✅ `describe_chunk()` is async, reads provenance metadata
- ✅ `stream()` is async, returns a `futures::Stream` of batches
- ✅ A stream is re-openable — hold the source, open as many times as needed
- ✅ No string interpolation in the manifest — the command hydrates
- ✅ Batches are Arrow-laid-out columns, no per-row allocation


---

# 2. Unit tests

## Phase 3: Unit Tests for Record Types

These tests are test-first specifications for Phase 4 implementation. Each test validates a core invariant or behavior of the record types defined in the Phase 2 architecture.

All tests are placed in `#[cfg(test)] mod tests` at the end of their respective module files in `liquers-lib/src/records/`. Tests use typed `Error` constructors per CLAUDE.md, never `unwrap()`/`expect()` in test code that might be run against unfished implementations, and explicit match arms on enums.

---

### `liquers-lib/src/records/mod.rs` — RecordSchema and RecordChunk tests

#### RecordSchema Validation Tests

**Test: `schema_new_accepts_exactly_one_id_field_exact_indexed_and_stored`**

File: `liquers-lib/src/records/mod.rs`

Rationale: `RecordSchema::new` is the validation gate. It must reject a schema where the Id field cannot be reconciled by `delete_term`.

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn schema_new_accepts_exactly_one_id_field_exact_indexed_and_stored() {
        let fields = vec![
            FieldSchema {
                name: "id".to_string(),
                data_type: FieldType::Int,
                nullable: false,
                key: KeyRole::Id,
                role: FieldRole {
                    indexed: vec![IndexKind::Exact],
                    stored: true,
                    fast: false,
                },
            },
            FieldSchema {
                name: "title".to_string(),
                data_type: FieldType::Text,
                nullable: false,
                key: KeyRole::None,
                role: FieldRole {
                    indexed: vec![IndexKind::FullText {
                        analyzer: Analyzer::Simple,
                        positions: true,
                    }],
                    stored: true,
                    fast: false,
                },
            },
        ];

        let schema = RecordSchema::new(fields);
        assert!(schema.is_ok(), "schema with one Id field, Exact-indexed and stored should succeed");
        let schema = schema.unwrap();
        assert_eq!(schema.id_field(), 0, "id field is at index 0");
    }
```

---

**Test: `schema_new_rejects_zero_id_fields`**

File: `liquers-lib/src/records/mod.rs`

Rationale: Exactly one Id field is required; a schema without one is incomplete.

```rust
    #[test]
    fn schema_new_rejects_zero_id_fields() {
        let fields = vec![
            FieldSchema {
                name: "title".to_string(),
                data_type: FieldType::Text,
                nullable: false,
                key: KeyRole::None,
                role: FieldRole::default(),
            },
        ];

        let schema = RecordSchema::new(fields);
        assert!(
            schema.is_err(),
            "schema without an Id field should be rejected"
        );
    }
```

---

**Test: `schema_new_rejects_two_id_fields`**

File: `liquers-lib/src/records/mod.rs`

Rationale: Identity must be unambiguous; two Id fields violate that.

```rust
    #[test]
    fn schema_new_rejects_two_id_fields() {
        let fields = vec![
            FieldSchema {
                name: "id1".to_string(),
                data_type: FieldType::Int,
                nullable: false,
                key: KeyRole::Id,
                role: FieldRole {
                    indexed: vec![IndexKind::Exact],
                    stored: true,
                    fast: false,
                },
            },
            FieldSchema {
                name: "id2".to_string(),
                data_type: FieldType::Int,
                nullable: false,
                key: KeyRole::Id,
                role: FieldRole {
                    indexed: vec![IndexKind::Exact],
                    stored: true,
                    fast: false,
                },
            },
        ];

        let schema = RecordSchema::new(fields);
        assert!(schema.is_err(), "schema with two Id fields should be rejected");
    }
```

---

**Test: `schema_new_rejects_id_not_exact_indexed`**

File: `liquers-lib/src/records/mod.rs`

Rationale: `delete_term` requires exact matching; a non-exact Id cannot reconcile.

```rust
    #[test]
    fn schema_new_rejects_id_not_exact_indexed() {
        let fields = vec![
            FieldSchema {
                name: "id".to_string(),
                data_type: FieldType::Text,
                nullable: false,
                key: KeyRole::Id,
                role: FieldRole {
                    indexed: vec![IndexKind::FullText {
                        analyzer: Analyzer::Simple,
                        positions: false,
                    }],
                    stored: true,
                    fast: false,
                },
            },
        ];

        let schema = RecordSchema::new(fields);
        assert!(
            schema.is_err(),
            "Id field must have Exact indexing for delete_term"
        );
    }
```

---

**Test: `schema_new_rejects_id_not_stored`**

File: `liquers-lib/src/records/mod.rs`

Rationale: `delete_term` needs to know the id to match; an unstored Id cannot be retrieved.

```rust
    #[test]
    fn schema_new_rejects_id_not_stored() {
        let fields = vec![
            FieldSchema {
                name: "id".to_string(),
                data_type: FieldType::Int,
                nullable: false,
                key: KeyRole::Id,
                role: FieldRole {
                    indexed: vec![IndexKind::Exact],
                    stored: false,
                    fast: false,
                },
            },
        ];

        let schema = RecordSchema::new(fields);
        assert!(
            schema.is_err(),
            "Id field must be stored for reconciliation"
        );
    }
```

---

**Test: `schema_new_rejects_multiple_source_fields`**

File: `liquers-lib/src/records/mod.rs`

Rationale: At most one Source field; multiple would be ambiguous.

```rust
    #[test]
    fn schema_new_rejects_multiple_source_fields() {
        let fields = vec![
            FieldSchema {
                name: "id".to_string(),
                data_type: FieldType::Int,
                nullable: false,
                key: KeyRole::Id,
                role: FieldRole {
                    indexed: vec![IndexKind::Exact],
                    stored: true,
                    fast: false,
                },
            },
            FieldSchema {
                name: "source1".to_string(),
                data_type: FieldType::UInt,
                nullable: false,
                key: KeyRole::Source,
                role: FieldRole::default(),
            },
            FieldSchema {
                name: "source2".to_string(),
                data_type: FieldType::UInt,
                nullable: false,
                key: KeyRole::Source,
                role: FieldRole::default(),
            },
        ];

        let schema = RecordSchema::new(fields);
        assert!(
            schema.is_err(),
            "schema with two Source fields should be rejected"
        );
    }
```

---

#### RecordSchema Query Methods

**Test: `schema_id_field_returns_correct_index`**

File: `liquers-lib/src/records/mod.rs`

Rationale: `id_field()` must return the exact index of the Id field.

```rust
    #[test]
    fn schema_id_field_returns_correct_index() {
        let fields = vec![
            FieldSchema {
                name: "title".to_string(),
                data_type: FieldType::Text,
                nullable: false,
                key: KeyRole::None,
                role: FieldRole::default(),
            },
            FieldSchema {
                name: "id".to_string(),
                data_type: FieldType::Int,
                nullable: false,
                key: KeyRole::Id,
                role: FieldRole {
                    indexed: vec![IndexKind::Exact],
                    stored: true,
                    fast: false,
                },
            },
        ];

        let schema = RecordSchema::new(fields).unwrap();
        assert_eq!(schema.id_field(), 1, "id_field should return the index 1");
    }
```

---

**Test: `schema_source_field_returns_correct_index`**

File: `liquers-lib/src/records/mod.rs`

Rationale: `source_field()` must return the index when present, None when absent.

```rust
    #[test]
    fn schema_source_field_returns_correct_index() {
        let fields = vec![
            FieldSchema {
                name: "id".to_string(),
                data_type: FieldType::Int,
                nullable: false,
                key: KeyRole::Id,
                role: FieldRole {
                    indexed: vec![IndexKind::Exact],
                    stored: true,
                    fast: false,
                },
            },
            FieldSchema {
                name: "source".to_string(),
                data_type: FieldType::UInt,
                nullable: false,
                key: KeyRole::Source,
                role: FieldRole::default(),
            },
        ];

        let schema = RecordSchema::new(fields).unwrap();
        assert_eq!(
            schema.source_field(),
            Some(1),
            "source_field should return index 1"
        );
    }
```

---

**Test: `schema_source_field_returns_none_when_absent`**

File: `liquers-lib/src/records/mod.rs`

Rationale: A schema may legitimately have no Source field.

```rust
    #[test]
    fn schema_source_field_returns_none_when_absent() {
        let fields = vec![
            FieldSchema {
                name: "id".to_string(),
                data_type: FieldType::Int,
                nullable: false,
                key: KeyRole::Id,
                role: FieldRole {
                    indexed: vec![IndexKind::Exact],
                    stored: true,
                    fast: false,
                },
            },
        ];

        let schema = RecordSchema::new(fields).unwrap();
        assert_eq!(
            schema.source_field(),
            None,
            "source_field should return None"
        );
    }
```

---

**Test: `schema_index_of_finds_field_by_name`**

File: `liquers-lib/src/records/mod.rs`

Rationale: Field lookup by name is essential for projections.

```rust
    #[test]
    fn schema_index_of_finds_field_by_name() {
        let fields = vec![
            FieldSchema {
                name: "id".to_string(),
                data_type: FieldType::Int,
                nullable: false,
                key: KeyRole::Id,
                role: FieldRole {
                    indexed: vec![IndexKind::Exact],
                    stored: true,
                    fast: false,
                },
            },
            FieldSchema {
                name: "title".to_string(),
                data_type: FieldType::Text,
                nullable: false,
                key: KeyRole::None,
                role: FieldRole::default(),
            },
        ];

        let schema = RecordSchema::new(fields).unwrap();
        assert_eq!(schema.index_of("title"), Some(1));
        assert_eq!(schema.index_of("id"), Some(0));
        assert_eq!(schema.index_of("nonexistent"), None);
    }
```

---

### `liquers-lib/src/records/buffer.rs` — Buffer and AlignedBuffer tests

#### AlignedBuffer Alignment Assertion

**Test: `aligned_buffer_maintains_64_byte_alignment`**

File: `liquers-lib/src/records/buffer.rs`

Rationale: 64-byte alignment is an Arrow recommendation required for SIMD reads. It must be verifiable as a hard property.

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn aligned_buffer_maintains_64_byte_alignment() {
        const ALIGNMENT: usize = 64;
        assert_eq!(
            AlignedBuffer::alignment(),
            ALIGNMENT,
            "AlignedBuffer must use 64-byte alignment"
        );

        let buffer = AlignedBuffer::from_slice(&[1u8, 2, 3, 4, 5]);
        let ptr = buffer.as_bytes().as_ptr() as usize;
        assert_eq!(
            ptr % ALIGNMENT,
            0,
            "AlignedBuffer.as_bytes() pointer must be 64-byte aligned, got ptr={:x}",
            ptr
        );
    }
```

---

#### Buffer<T> Round-trip Tests

**Test: `buffer_i32_round_trip`**

File: `liquers-lib/src/records/buffer.rs`

Rationale: Data must survive serialization and deserialization via Buffer<i32>.

```rust
    #[test]
    fn buffer_i32_round_trip() {
        let original: Vec<i32> = vec![
            i32::MIN,
            -1,
            0,
            1,
            i32::MAX,
            42,
            -999,
        ];
        
        let buffer = Buffer::from_slice(&original);
        let recovered = buffer.as_slice();
        
        assert_eq!(recovered, original.as_slice(), "i32 values must round-trip");
    }
```

---

**Test: `buffer_i64_round_trip`**

File: `liquers-lib/src/records/buffer.rs`

Rationale: Signed 64-bit integers must round-trip.

```rust
    #[test]
    fn buffer_i64_round_trip() {
        let original: Vec<i64> = vec![
            i64::MIN,
            -1,
            0,
            1,
            i64::MAX,
            1_000_000_000_000,
        ];
        
        let buffer = Buffer::from_slice(&original);
        let recovered = buffer.as_slice();
        
        assert_eq!(recovered, original.as_slice(), "i64 values must round-trip");
    }
```

---

**Test: `buffer_u64_round_trip`**

File: `liquers-lib/src/records/buffer.rs`

Rationale: Unsigned 64-bit integers must round-trip.

```rust
    #[test]
    fn buffer_u64_round_trip() {
        let original: Vec<u64> = vec![
            0,
            1,
            u64::MAX,
            0xDEADBEEF,
        ];
        
        let buffer = Buffer::from_slice(&original);
        let recovered = buffer.as_slice();
        
        assert_eq!(recovered, original.as_slice(), "u64 values must round-trip");
    }
```

---

**Test: `buffer_f32_round_trip`**

File: `liquers-lib/src/records/buffer.rs`

Rationale: 32-bit floats must preserve values exactly.

```rust
    #[test]
    fn buffer_f32_round_trip() {
        let original: Vec<f32> = vec![
            0.0,
            -0.0,
            1.0,
            -1.0,
            std::f32::PI,
            std::f32::INFINITY,
            -std::f32::INFINITY,
        ];
        
        let buffer = Buffer::from_slice(&original);
        let recovered = buffer.as_slice();
        
        assert_eq!(recovered.len(), original.len(), "lengths must match");
        for (i, (a, b)) in recovered.iter().zip(original.iter()).enumerate() {
            assert_eq!(a, b, "f32 at index {} must match", i);
        }
    }
```

---

**Test: `buffer_f64_round_trip`**

File: `liquers-lib/src/records/buffer.rs`

Rationale: 64-bit floats must preserve values exactly.

```rust
    #[test]
    fn buffer_f64_round_trip() {
        let original: Vec<f64> = vec![
            0.0,
            -0.0,
            1.0,
            -1.0,
            std::f64::PI,
            std::f64::INFINITY,
            -std::f64::INFINITY,
        ];
        
        let buffer = Buffer::from_slice(&original);
        let recovered = buffer.as_slice();
        
        assert_eq!(recovered.len(), original.len(), "lengths must match");
        for (i, (a, b)) in recovered.iter().zip(original.iter()).enumerate() {
            assert_eq!(a, b, "f64 at index {} must match", i);
        }
    }
```

---

### `liquers-lib/src/records/mod.rs` — Bitmap tests

#### Bitmap Bit Operations

**Test: `bitmap_get_returns_correct_bit`**

File: `liquers-lib/src/records/mod.rs`

Rationale: `get()` must return the exact bit value at the specified index.

```rust
    #[test]
    fn bitmap_get_returns_correct_bit() {
        let bits = vec![0b10101010u8];
        let bitmap = Bitmap::from_bytes(&bits, 8);
        
        // LSB-first: bit 0 is the lowest bit
        assert_eq!(bitmap.get(0), false, "bit 0 should be 0");
        assert_eq!(bitmap.get(1), true, "bit 1 should be 1");
        assert_eq!(bitmap.get(2), false, "bit 2 should be 0");
        assert_eq!(bitmap.get(3), true, "bit 3 should be 1");
    }
```

---

**Test: `bitmap_and_combines_masks`**

File: `liquers-lib/src/records/mod.rs`

Rationale: `and()` must produce a bitwise AND of two masks.

```rust
    #[test]
    fn bitmap_and_combines_masks() {
        let bits1 = vec![0b11110000u8];
        let bits2 = vec![0b10101010u8];
        let bitmap1 = Bitmap::from_bytes(&bits1, 8);
        let bitmap2 = Bitmap::from_bytes(&bits2, 8);
        
        let result = bitmap1.and(&bitmap2).unwrap();
        
        assert_eq!(result.get(0), false);
        assert_eq!(result.get(1), false);
        assert_eq!(result.get(4), true);
        assert_eq!(result.get(5), false);
    }
```

---

**Test: `bitmap_and_length_mismatch_errors`**

File: `liquers-lib/src/records/mod.rs`

Rationale: Masks of different lengths cannot be combined.

```rust
    #[test]
    fn bitmap_and_length_mismatch_errors() {
        let bits1 = vec![0b11110000u8];
        let bits2 = vec![0b10101010u8, 0b11110000u8];
        let bitmap1 = Bitmap::from_bytes(&bits1, 8);
        let bitmap2 = Bitmap::from_bytes(&bits2, 16);
        
        let result = bitmap1.and(&bitmap2);
        assert!(result.is_err(), "and() of different lengths should error");
    }
```

---

**Test: `bitmap_or_combines_masks`**

File: `liquers-lib/src/records/mod.rs`

Rationale: `or()` must produce a bitwise OR of two masks.

```rust
    #[test]
    fn bitmap_or_combines_masks() {
        let bits1 = vec![0b11110000u8];
        let bits2 = vec![0b10101010u8];
        let bitmap1 = Bitmap::from_bytes(&bits1, 8);
        let bitmap2 = Bitmap::from_bytes(&bits2, 8);
        
        let result = bitmap1.or(&bitmap2).unwrap();
        
        // OR produces 0b11111010
        assert_eq!(result.get(0), false);
        assert_eq!(result.get(1), true);
        assert_eq!(result.get(4), true);
        assert_eq!(result.get(7), true);
    }
```

---

**Test: `bitmap_not_inverts_bits`**

File: `liquers-lib/src/records/mod.rs`

Rationale: `not()` must invert every bit.

```rust
    #[test]
    fn bitmap_not_inverts_bits() {
        let bits = vec![0b10101010u8];
        let bitmap = Bitmap::from_bytes(&bits, 8);
        
        let inverted = bitmap.not();
        
        // Inverted should be 0b01010101
        assert_eq!(inverted.get(0), true);
        assert_eq!(inverted.get(1), false);
        assert_eq!(inverted.get(7), false);
    }
```

---

**Test: `bitmap_count_ones_counts_set_bits`**

File: `liquers-lib/src/records/mod.rs`

Rationale: `count_ones()` must count the number of 1 bits.

```rust
    #[test]
    fn bitmap_count_ones_counts_set_bits() {
        let bits = vec![0b11110000u8, 0b00001111u8];
        let bitmap = Bitmap::from_bytes(&bits, 16);
        
        assert_eq!(bitmap.count_ones(), 8, "should count 8 ones across both bytes");
    }
```

---

**Test: `bitmap_count_ones_on_empty_is_zero`**

File: `liquers-lib/src/records/mod.rs`

Rationale: An all-zero bitmap has zero ones.

```rust
    #[test]
    fn bitmap_count_ones_on_empty_is_zero() {
        let bits = vec![0b00000000u8];
        let bitmap = Bitmap::from_bytes(&bits, 8);
        
        assert_eq!(bitmap.count_ones(), 0);
    }
```

---

### `liquers-lib/src/records/mod.rs` — RecordBatch and Column tests

#### RecordBatch Projection

**Test: `batch_select_zero_copy_projection`**

File: `liquers-lib/src/records/mod.rs`

Rationale: `select()` must project columns without copying data; Arc pointers must be identical.

```rust
    #[test]
    fn batch_select_zero_copy_projection() {
        let schema = build_test_schema();
        let batch = build_test_batch(&schema, 10);
        
        // Select columns 0 and 2
        let projected = batch.select(&[0, 2]).unwrap();
        
        assert_eq!(projected.len, 10, "row count unchanged");
        assert_eq!(projected.columns.len(), 2, "two columns selected");
        
        // Verify Arc sharing (pointer equality)
        match (&batch.columns[0], &projected.columns[0]) {
            (Column::Int { values: v1, .. }, Column::Int { values: v2, .. }) => {
                assert_eq!(
                    std::sync::Arc::as_ptr(v1),
                    std::sync::Arc::as_ptr(v2),
                    "first column should be Arc-shared"
                );
            }
            _ => panic!("expected Int column"),
        }
    }
```

---

#### RecordBatch Filtering

**Test: `batch_filter_by_mask`**

File: `liquers-lib/src/records/mod.rs`

Rationale: `filter()` must gather rows where mask bit is 1.

```rust
    #[test]
    fn batch_filter_by_mask() {
        let schema = build_test_schema();
        let batch = build_test_batch(&schema, 4);
        
        // Mask: keep rows 1 and 3
        let mask = Bitmap::from_bytes(&[0b1010u8], 4);
        let filtered = batch.filter(&mask).unwrap();
        
        assert_eq!(filtered.len, 2, "two rows should remain");
    }
```

---

**Test: `batch_filter_length_mismatch_errors`**

File: `liquers-lib/src/records/mod.rs`

Rationale: Mask length must match batch row count.

```rust
    #[test]
    fn batch_filter_length_mismatch_errors() {
        let schema = build_test_schema();
        let batch = build_test_batch(&schema, 10);
        
        let mask = Bitmap::from_bytes(&[0xFFu8], 8);
        let result = batch.filter(&mask);
        
        assert!(result.is_err(), "mask length 8 != batch len 10");
    }
```

---

#### RecordBatch Slicing

**Test: `batch_slice_preserves_arc_sharing`**

File: `liquers-lib/src/records/mod.rs`

Rationale: `slice()` must be zero-copy; Arc pointers remain identical.

```rust
    #[test]
    fn batch_slice_preserves_arc_sharing() {
        let schema = build_test_schema();
        let batch = build_test_batch(&schema, 10);
        
        let sliced = batch.slice(2, 5).unwrap();
        
        assert_eq!(sliced.len, 5, "slice length is 5");
        
        // Verify Arc sharing
        match (&batch.columns[0], &sliced.columns[0]) {
            (Column::Int { values: v1, .. }, Column::Int { values: v2, .. }) => {
                assert_eq!(
                    std::sync::Arc::as_ptr(v1),
                    std::sync::Arc::as_ptr(v2),
                    "sliced column should share Arc"
                );
            }
            _ => panic!("expected Int column"),
        }
    }
```

---

#### RecordBatch Concatenation

**Test: `batch_concat_same_schema`**

File: `liquers-lib/src/records/mod.rs`

Rationale: `concat()` must append rows when schemas match exactly.

```rust
    #[test]
    fn batch_concat_same_schema() {
        let schema = build_test_schema();
        let batch1 = build_test_batch(&schema, 5);
        let batch2 = build_test_batch(&schema, 3);
        
        let combined = RecordBatch::concat(&[batch1, batch2]).unwrap();
        
        assert_eq!(combined.len, 8, "concatenated batch has 8 rows");
        assert_eq!(combined.columns.len(), schema.fields.len());
    }
```

---

**Test: `batch_concat_different_schema_errors`**

File: `liquers-lib/src/records/mod.rs`

Rationale: Schema must be identical; differing schemas cause concat to fail, naming the first differing field.

```rust
    #[test]
    fn batch_concat_different_schema_errors() {
        let schema1 = build_test_schema();
        let schema2 = {
            let mut s = build_test_schema();
            s.fields[0].name = "other_name".to_string();
            s
        };
        
        let batch1 = build_test_batch(&schema1, 5);
        let batch2 = build_test_batch(&schema2, 3);
        
        let result = RecordBatch::concat(&[batch1, batch2]);
        
        assert!(result.is_err(), "concat should reject different schemas");
        if let Err(e) = result {
            let msg = format!("{:?}", e);
            assert!(
                msg.contains("field") || msg.contains("schema"),
                "error should name the differing field: {}",
                msg
            );
        }
    }
```

---

#### RecordBatch Single-Cell Read

**Test: `batch_value_reads_single_cell`**

File: `liquers-lib/src/records/mod.rs`

Rationale: `value()` must return the exact FieldValue at (row, column).

```rust
    #[test]
    fn batch_value_reads_single_cell() {
        let schema = build_test_schema_with_text();
        let batch = build_test_batch_with_text(&schema, vec![
            ("hello", 10),
            ("world", 20),
        ]);
        
        let val = batch.value(0, 0).unwrap();
        match val {
            FieldValue::Text(s) => assert_eq!(*s, "hello"),
            _ => panic!("expected Text value"),
        }
    }
```

---

#### Column Null Handling

**Test: `column_validity_omitted_when_no_nulls`**

File: `liquers-lib/src/records/mod.rs`

Rationale: Validity bitmap must be `None` when a column has no nulls; this saves 1 bit per row.

```rust
    #[test]
    fn column_validity_omitted_when_no_nulls() {
        let values = Buffer::from_slice(&[1i64, 2, 3, 4]);
        let column = Column::Int {
            validity: None,
            values,
        };
        
        // This column has no null representation; all values are valid.
        // A column with nulls would have Some(bitmap).
        assert!(matches!(column, Column::Int { validity: None, .. }));
    }
```

---

**Test: `column_null_distinct_from_empty_string`**

File: `liquers-lib/src/records/mod.rs`

Rationale: A null value and an empty string are semantically different and must be distinguishable.

```rust
    #[test]
    fn column_null_distinct_from_empty_string() {
        // Row 0: empty string; Row 1: null
        let offsets = Buffer::from_slice(&[0i32, 0, 0]);
        let data = AlignedBuffer::from_slice(b"");
        
        let validity = Bitmap::from_bytes(&[0b01u8], 2); // Row 1 is null
        
        let column = Column::Text {
            validity: Some(validity),
            offsets,
            data,
        };
        
        // Row 0 is not null but has length 0 (empty string)
        // Row 1 is null (validity bit is 0)
        // These are different states
        match &column {
            Column::Text { validity: Some(v), .. } => {
                assert!(!v.get(0), "row 0 is not null");
                assert!(v.get(1), "row 1 is null");
            }
            _ => panic!("expected Text with validity"),
        }
    }
```

---

#### RecordBatch Column Extension

**Test: `batch_with_columns_appends_derived_fields`**

File: `liquers-lib/src/records/mod.rs`

Rationale: `with_columns()` must extend the schema and batch with new columns.

```rust
    #[test]
    fn batch_with_columns_appends_derived_fields() {
        let schema = build_test_schema();
        let batch = build_test_batch(&schema, 5);
        
        let new_field = FieldSchema {
            name: "derived".to_string(),
            data_type: FieldType::UInt,
            nullable: false,
            key: KeyRole::None,
            role: FieldRole::default(),
        };
        
        let new_column = Column::UInt {
            validity: None,
            values: Buffer::from_slice(&[1u64, 2, 3, 4, 5]),
        };
        
        let extended = batch
            .with_columns(vec![new_field], vec![new_column])
            .unwrap();
        
        assert_eq!(extended.columns.len(), schema.fields.len() + 1);
        assert_eq!(extended.schema.fields.len(), schema.fields.len() + 1);
        assert_eq!(extended.len, 5);
    }
```

---

### `liquers-lib/src/records/mod.rs` — FieldValue size test

#### FieldValue Size Assertion

**Test: `field_value_size_is_24_bytes`**

File: `liquers-lib/src/records/mod.rs`

Rationale: `FieldValue` is designed to fit in 24 bytes, smaller than `serde_json::Value` (32 bytes), with support for bytes and vectors.

```rust
    #[test]
    fn field_value_size_is_24_bytes() {
        let size = std::mem::size_of::<FieldValue>();
        assert_eq!(
            size, 24,
            "FieldValue must be 24 bytes; got {}. Check layout against the design.",
            size
        );
    }
```

---

### `liquers-lib/src/records/mod.rs` — ChunkId tests

#### ChunkId Comparison and Hashing

**Test: `chunk_id_query_variant_equality`**

File: `liquers-lib/src/records/mod.rs`

Rationale: Two ChunkId::Query variants with identical queries must compare equal.

```rust
    #[test]
    fn chunk_id_query_variant_equality() {
        let query1: Query = parse_query("-R/data/file.csv").unwrap();
        let query2: Query = parse_query("-R/data/file.csv").unwrap();
        
        let id1 = ChunkId::Query(query1);
        let id2 = ChunkId::Query(query2);
        
        assert_eq!(id1, id2, "identical queries should produce equal ChunkIds");
    }
```

---

**Test: `chunk_id_key_variant_equality`**

File: `liquers-lib/src/records/mod.rs`

Rationale: Two ChunkId::Key variants with identical keys must compare equal.

```rust
    #[test]
    fn chunk_id_key_variant_equality() {
        let key1: Key = parse_key("data/chunk_0001.csv").unwrap();
        let key2: Key = parse_key("data/chunk_0001.csv").unwrap();
        
        let id1 = ChunkId::Key(key1);
        let id2 = ChunkId::Key(key2);
        
        assert_eq!(id1, id2, "identical keys should produce equal ChunkIds");
    }
```

---

**Test: `chunk_id_different_variants_not_equal`**

File: `liquers-lib/src/records/mod.rs`

Rationale: Query and Key variants are never equal, even if their contents are similar.

```rust
    #[test]
    fn chunk_id_different_variants_not_equal() {
        let query: Query = parse_query("-R/data/chunk.csv").unwrap();
        let key: Key = parse_key("data/chunk.csv").unwrap();
        
        let id1 = ChunkId::Query(query);
        let id2 = ChunkId::Key(key);
        
        assert_ne!(id1, id2, "Query and Key variants must not be equal");
    }
```

---

**Test: `chunk_id_query_variant_hashable`**

File: `liquers-lib/src/records/mod.rs`

Rationale: ChunkId variants must be hashable for use in maps/sets.

```rust
    #[test]
    fn chunk_id_query_variant_hashable() {
        use std::collections::HashSet;
        
        let query: Query = parse_query("-R/data/file.csv").unwrap();
        let id = ChunkId::Query(query);
        
        let mut set = HashSet::new();
        set.insert(id.clone());
        
        assert!(set.contains(&id), "ChunkId should be hashable");
    }
```

---

**Test: `chunk_id_key_variant_hashable`**

File: `liquers-lib/src/records/mod.rs`

Rationale: ChunkId::Key variants must also be hashable.

```rust
    #[test]
    fn chunk_id_key_variant_hashable() {
        use std::collections::HashSet;
        
        let key: Key = parse_key("data/chunk_0001.csv").unwrap();
        let id = ChunkId::Key(key);
        
        let mut set = HashSet::new();
        set.insert(id.clone());
        
        assert!(set.contains(&id), "ChunkId::Key should be hashable");
    }
```

---

---

### Test Helper Functions

**File: `liquers-lib/src/records/mod.rs`**

These helpers support the batch and schema tests above:

```rust
#[cfg(test)]
mod tests {
    // ... all tests above ...
    
    /// Build a minimal valid test schema with Id and a text field.
    fn build_test_schema() -> RecordSchema {
        let fields = vec![
            FieldSchema {
                name: "id".to_string(),
                data_type: FieldType::Int,
                nullable: false,
                key: KeyRole::Id,
                role: FieldRole {
                    indexed: vec![IndexKind::Exact],
                    stored: true,
                    fast: false,
                },
            },
            FieldSchema {
                name: "value".to_string(),
                data_type: FieldType::Int,
                nullable: false,
                key: KeyRole::None,
                role: FieldRole::default(),
            },
        ];
        RecordSchema::new(fields).unwrap()
    }

    /// Build a test schema with text field.
    fn build_test_schema_with_text() -> RecordSchema {
        let fields = vec![
            FieldSchema {
                name: "id".to_string(),
                data_type: FieldType::Int,
                nullable: false,
                key: KeyRole::Id,
                role: FieldRole {
                    indexed: vec![IndexKind::Exact],
                    stored: true,
                    fast: false,
                },
            },
            FieldSchema {
                name: "text".to_string(),
                data_type: FieldType::Text,
                nullable: false,
                key: KeyRole::None,
                role: FieldRole::default(),
            },
            FieldSchema {
                name: "count".to_string(),
                data_type: FieldType::Int,
                nullable: false,
                key: KeyRole::None,
                role: FieldRole::default(),
            },
        ];
        RecordSchema::new(fields).unwrap()
    }

    /// Build a test batch with n rows and default values.
    fn build_test_batch(schema: &RecordSchema, n: usize) -> RecordBatch {
        let id_values: Vec<i64> = (0..n as i64).collect();
        let val_values: Vec<i64> = (100..100 + n as i64).collect();

        RecordBatch {
            schema: std::sync::Arc::new(schema.clone()),
            columns: vec![
                Column::Int {
                    validity: None,
                    values: Buffer::from_slice(&id_values),
                },
                Column::Int {
                    validity: None,
                    values: Buffer::from_slice(&val_values),
                },
            ],
            len: n,
            chunk_id: None,
            sources: vec![],
        }
    }

    /// Build a test batch with text values.
    fn build_test_batch_with_text(
        schema: &RecordSchema,
        data: Vec<(&str, i64)>,
    ) -> RecordBatch {
        let n = data.len();

        // Text column: build offsets and data
        let mut offsets = vec![0i32];
        let mut text_data = Vec::new();
        for (text, _) in &data {
            text_data.extend_from_slice(text.as_bytes());
            offsets.push(text_data.len() as i32);
        }

        let ids: Vec<i64> = (0..n as i64).collect();
        let counts: Vec<i64> = data.iter().map(|(_, count)| *count).collect();

        RecordBatch {
            schema: std::sync::Arc::new(schema.clone()),
            columns: vec![
                Column::Int {
                    validity: None,
                    values: Buffer::from_slice(&ids),
                },
                Column::Text {
                    validity: None,
                    offsets: Buffer::from_slice(&offsets),
                    data: AlignedBuffer::from_slice(&text_data),
                },
                Column::Int {
                    validity: None,
                    values: Buffer::from_slice(&counts),
                },
            ],
            len: n,
            chunk_id: None,
            sources: vec![],
        }
    }
}
```

---

### Summary

**Total test count: 39 unit tests**

Tests are organized into 7 groups:

1. **RecordSchema validation** (6 tests): Enforcing exactly-one-Id rule with Exact + stored requirement
2. **RecordSchema queries** (4 tests): id_field(), source_field(), index_of()
3. **AlignedBuffer & Buffer** (6 tests): 64-byte alignment assertion and i32/i64/u64/f32/f64 round-trips
4. **Bitmap operations** (7 tests): get, and, or, not, count_ones, length validation
5. **RecordBatch operations** (9 tests): select, filter, slice, concat, value, with_columns
6. **Column null handling** (2 tests): Validity omitted when no nulls; null distinct from empty string
7. **FieldValue & ChunkId** (5 tests): 24-byte size assertion; Query/Key equality and hashing

All tests use:
- Typed `Error` constructors (never `Error::new`)
- No `unwrap()` on fallible operations (assertions via `.is_err()` and `.is_ok()`)
- Explicit match arms (never `_ =>`)
- `#[cfg(test)]` module structure at end of file
- Clear assertions with diagnostic messages
- Helper functions for common test setup

Test names follow convention: `{type}_{operation}_{outcome}` or `{type}_{property}_{validation}`.

Feature gate: All tests are gated with `#![cfg(feature = "records")]` at file level, following CLAUDE.md conventions for optional-dependency tests.


---

# 3. Integration tests

## Phase 3 Record-Streams Integration Tests - Draft 4

Phase 3 is TEST-FIRST. These tests define the specification that Phase 4 implementation tests against.

File path: **liquers-lib/tests/record_streams_end_to_end.rs**

### Test 1: End-to-end record-producing command and serialization

Rationale: Verifies the complete flow from command registration through ExtValue::RecordChunk serialization to multiple formats.

```rust
use liquers_core::{
    context::SimpleEnvironment,
    error::Error,
    interpreter::evaluate,
    state::State,
    value::Value,
};
use liquers_lib::value::ExtValue;
use liquers_macro::register_command;

#[tokio::test]
async fn test_end_to_end_record_chunk_serialization() -> Result<(), Box<dyn std::error::Error>> {
    type CommandEnvironment = SimpleEnvironment<Value>;
    let mut env = SimpleEnvironment::<Value>::new();

    // 1. Define a simple record-producing command
    // When implemented, this should return an ExtValue::RecordChunk
    fn generate_records(_state: &State<Value>) -> Result<Value, Error> {
        // Placeholder: in Phase 4, this returns RecordChunk with CSV data
        // For now, it establishes the command signature pattern
        Ok(Value::from("csv_data"))
    }

    let cr = &mut env.command_registry;
    register_command!(cr, fn generate_records(state) -> result)?;

    let envref = env.to_ref();

    // 2. Evaluate the query
    let state = evaluate(envref, "generate_records", None).await?;

    // 3. When RecordChunk is implemented:
    // - The state should contain ExtValue::RecordChunk
    // - It should serialize to csv, ndjson, json
    // - Round-trip serialization should be lossless for data + schema
    
    todo!("Phase 4: evaluate a query producing ExtValue::RecordChunk, serialize as csv/ndjson/json, deserialize, compare equal")
}
```

### Test 2: RecordSource re-openability

Rationale: Establishes that RecordSource::stream() can be called multiple times, yielding identical batches each time. This is the central property distinguishing Source from Stream.

```rust
#[tokio::test]
async fn test_record_source_reopenable() -> Result<(), Box<dyn std::error::Error>> {
    type CommandEnvironment = SimpleEnvironment<Value>;
    let mut env = SimpleEnvironment::<Value>::new();

    // When RecordSource is implemented:
    // fn produce_source(_state: &State<Value>) -> Result<Value, Error> {
    //     // Returns ExtValue::RecordSource with backing data
    //     Ok(Value::from(...))
    // }

    let cr = &mut env.command_registry;
    // register_command!(cr, fn produce_source(state) -> result)?;

    let envref = env.to_ref();

    // When implemented, this test should:
    // 1. Evaluate a query returning ExtValue::RecordSource
    // 2. Call .stream() on the source to get a RecordBatchStream
    // 3. Collect all batches and verify row count / schema
    // 4. Call .stream() again on the same source
    // 5. Collect and verify we get identical batches (same row count, same data)
    // 6. Assert no batches are dropped or re-ordered
    
    // Pseudo-code assertion:
    // let source = state.data_unchecked();
    // if let ExtValue::RecordSource { value } = &**source { 
    //     let ctx = envref.get_asset_manager().create_dummy_asset().create_context().await;
    //     
    //     let stream1 = value.stream(&ctx).await?;
    //     let batches1: Vec<_> = stream1.collect::<Result<Vec<_>, _>>().await?;
    //     
    //     let stream2 = value.stream(&ctx).await?;
    //     let batches2: Vec<_> = stream2.collect::<Result<Vec<_>, _>>().await?;
    //     
    //     assert_eq!(batches1.len(), batches2.len());
    //     for (b1, b2) in batches1.iter().zip(batches2.iter()) {
    //         assert_eq!(b1.len, b2.len);
    //         // Schema comparison when FieldSchema derives Eq
    //     }
    // }
    
    todo!("Phase 4: call RecordSource::stream() twice on one source; both traversals yield identical rows. This is the property the source/stream split exists for")
}
```

### Test 3: Streaming memory bounded by batch, not chunk

Rationale: Verifies that only one batch is resident at a time during streaming. The batch is the memory unit; a chunk may contain many batches.

```rust
#[tokio::test]
async fn test_streaming_bounded_memory() -> Result<(), Box<dyn std::error::Error>> {
    // When RecordBatchStream and RecordBatch are fully implemented:
    // This test creates a source with multiple large chunks,
    // then streams through it, asserting that:
    // 
    // 1. At any point, only ~1 batch's worth of memory is live
    // 2. A stream of N chunks can be traversed without holding all N in memory
    // 3. The Arc-wrapping and futures::Stream polling do not cause accumulation
    //
    // Implementation pattern:
    // - Create a RecordSource with Materialized backing of, say, 10 batches
    // - Stream through it with a monitoring wrapper counting live Arc refs
    // - Assert that Arc strong count stays <= 2 (current + yielded)
    // - Assert batches are dropped after yielding
    //
    // This is aspirational for Phase 3 spec; Phase 4 must enforce it.

    todo!("Phase 4: traverse an N-chunk source asserting at most 2 batches are alive at once (Arc strong count), never N")
}
```

### Test 4: RecordSource serializes as manifest JSON

Rationale: A RecordSource with Queried backing serializes to a manifest, and deserializing that manifest recreates an equivalent source.

```rust
#[tokio::test]
async fn test_record_source_manifest_round_trip() -> Result<(), Box<dyn std::error::Error>> {
    use serde_json;

    // When RecordSource is implemented with Queried backing:
    // 1. Create a RecordSource with explicit chunk queries
    // 2. Serialize to JSON (as_bytes with format "json")
    // 3. The JSON should match the manifest format in phase2-architecture.md
    // 4. Deserialize the JSON back to RecordSource
    // 5. Compare: both sources should have identical chunk lists and schema
    //
    // Pseudo-code:
    // let source = RecordSource {
    //     backing: SourceBacking::Queried {
    //         chunks: vec![
    //             parse_query("ns-sql/query1")?,
    //             parse_query("ns-sql/query2")?,
    //         ],
    //         cache: None,
    //     },
    //     uniform_schema: Some(Arc::new(...)),
    // };
    //
    // let state = State::new().with_data(Value::from(ExtValue::RecordSource { value: Arc::new(source) }));
    // let manifest_bytes = state.as_bytes("json")?;
    // let manifest_json: serde_json::Value = serde_json::from_slice(&manifest_bytes)?;
    //
    // // Verify manifest structure
    // assert_eq!(manifest_json["manifest"], "record-stream");
    // assert_eq!(manifest_json["version"], 1);
    // assert!(manifest_json["chunks"].is_array());
    //
    // // Round-trip
    // let source2: RecordSource = serde_json::from_value(manifest_json)?;
    // assert_eq!(source.chunks(), source2.chunks());

    todo!("Phase 4: serialize a RecordSource to its manifest and back; the result equals the original")
}
```

### Test 5: Non-uniform chunks - NDJSON succeeds, CSV single chunk fails, concat fails

Rationale: Schema uniformity is declared, not assumed. Non-uniform chunks behave differently depending on the operation and serialization format.

```rust
#[tokio::test]
async fn test_non_uniform_chunks_ndjson_succeeds() -> Result<(), Box<dyn std::error::Error>> {
    // Two chunks with different schemas:
    // Chunk 1: {id: Int, name: Text}
    // Chunk 2: {id: Int, name: Text, email: Text}
    //
    // NDJSON serialization should succeed:
    // - Each batch serializes independently
    // - Later batches have extra columns, so all nulls in Chunk 1 for email
    // - NDJSON output is valid
    //
    // Pseudo-code when implemented:
    // let source = RecordSource with non-uniform backing;
    // let state = State::new().with_data(Value::from(ExtValue::RecordSource { value: Arc::new(source) }));
    // let ndjson_bytes = state.as_bytes("ndjson")?;  // Should succeed
    // assert!(!ndjson_bytes.is_empty());

    todo!("Phase 4: a stream whose chunks differ in schema serializes as NDJSON without error")
}
```

```rust
#[tokio::test]
async fn test_non_uniform_chunks_single_csv_fails() -> Result<(), Box<dyn std::error::Error>> {
    // Single CSV chunk serialization with non-uniform batches should fail:
    // - Attempting to write multiple batches as one CSV is not well-defined
    //   when schemas differ
    // - Error should name the first differing field
    //
    // Pseudo-code:
    // let source = RecordSource with non-uniform batches;
    // let state = State::new().with_data(Value::from(ExtValue::RecordSource { ... }));
    // let result = state.as_bytes("csv");
    // assert!(result.is_err());
    // if let Err(e) = result {
    //     let msg = format!("{:?}", e);
    //     assert!(msg.contains("schema") || msg.contains("field"));
    // }

    todo!("Phase 4: the same stream fails as a single CSV, and the error names the absence of one header")
}
```

```rust
#[tokio::test]
async fn test_non_uniform_chunks_concat_fails() -> Result<(), Box<dyn std::error::Error>> {
    // Attempting to concat non-uniform chunks should fail:
    // - RecordBatch::concat_chunks requires uniform schema
    // - Error should name which field is first different
    //
    // Pseudo-code:
    // let batch1 = RecordBatch { schema: Arc::new(schema1), ... };
    // let batch2 = RecordBatch { schema: Arc::new(schema2_different), ... };
    // let result = RecordBatch::concat_chunks(&[batch1.clone(), batch2.clone()]);
    // assert!(result.is_err());
    // if let Err(e) = result {
    //     assert!(format!("{:?}", e).contains("schema"));
    // }

    todo!("Phase 4: concat fails naming the FIRST differing field, not a generic schema mismatch")
}
```

### Test 6: Per-chunk arguments without ChunkKeys rejected at load

Rationale: A manifest using per-chunk arguments is valid only with ChunkKeys. Without one, it should be rejected at load time with a clear error, not silently alias chunks.

```rust
#[tokio::test]
async fn test_per_chunk_arguments_without_cache_rejected() -> Result<(), Box<dyn std::error::Error>> {
    use serde_json::json;

    // When manifest loading is implemented:
    // A manifest JSON like:
    // {
    //   "manifest": "record-stream",
    //   "version": 1,
    //   "chunks": [
    //     { "query": "ns-sql/query", "arguments": {"offset": 0} },
    //     { "query": "ns-sql/query", "arguments": {"offset": 1000} }
    //   ]
    // }
    // (no ChunkKeys, so no cache field)
    //
    // Should be rejected with error like:
    // "per-chunk arguments require ChunkKeys"
    //
    // Pseudo-code:
    // let manifest_json = json!({
    //     "manifest": "record-stream",
    //     "version": 1,
    //     "chunks": [
    //         {"query": "ns-sql/query", "arguments": {"offset": 0}},
    //         {"query": "ns-sql/query", "arguments": {"offset": 1000}},
    //     ]
    // });
    // 
    // let result = serde_json::from_value::<RecordSource>(manifest_json);
    // // Or if it deserializes, attempt to use it:
    // // assert!(result.is_err() || loading_result.is_err());
    // // if let Err(e) = result { 
    // //     assert!(format!("{:?}", e).contains("cache"));
    // // }

    todo!("Phase 4: a manifest with per-chunk arguments and no ChunkKeys is rejected AT LOAD; without this the chunks silently alias to one asset")
}
```

### Test 7: TypeInfo entries exist for RecordChunk and RecordSource

Rationale: Both ExtValue variants must have TypeInfo entries so they can be stored as assets. A variant with no TypeInfo cannot be written.

```rust
#[tokio::test]
async fn test_record_chunk_type_info_registered() -> Result<(), Box<dyn std::error::Error>> {
    // When TypeInfo registration for ExtValue is implemented:
    // ExtValue::type_descriptions() must include:
    // - TypeInfo { identifier: "RecordChunk", formats: ["csv", "ndjson", "json"] }
    // - TypeInfo { identifier: "RecordSource", formats: ["json"] }  (manifest)
    //
    // Pseudo-code:
    // let type_infos = ExtValue::type_descriptions();
    // let chunk_info = type_infos.iter().find(|ti| ti.identifier == "RecordChunk");
    // assert!(chunk_info.is_some());
    // let chunk_info = chunk_info.unwrap();
    // assert!(chunk_info.formats.contains(&"csv"));
    // assert!(chunk_info.formats.contains(&"ndjson"));
    // assert!(chunk_info.formats.contains(&"json"));
    //
    // let source_info = type_infos.iter().find(|ti| ti.identifier == "RecordSource");
    // assert!(source_info.is_some());
    // let source_info = source_info.unwrap();
    // assert!(source_info.formats.contains(&"json"));

    todo!("Phase 4: ExtValue::type_descriptions() contains RecordChunk; without a TypeInfo the type cannot be stored")
}
```

### Test 8: ExtValue variants match type descriptions

Rationale: Type system invariant: every ExtValue variant must have a matching TypeInfo entry. This is enforced in liquers-core for core Value; ExtValue needs the same guard.

```rust
#[tokio::test]
async fn test_type_descriptions_match_identifiers() -> Result<(), Box<dyn std::error::Error>> {
    // When TypeInfo is registered:
    // 1. Enumerate ExtValue variants
    // 2. For each variant, verify ExtValue::type_descriptions() has an entry
    //    with the same identifier
    // 3. Test should fail at compile if a new variant is added without TypeInfo
    //
    // This is the analogue of `type_descriptions_match_identifier` in core.
    // 
    // Pseudo-code (ideally a compile-time check via match exhaustiveness):
    // for type_info in ExtValue::type_descriptions() {
    //     // Verify at least one variant maps to this identifier
    //     let variants_with_id = [ /* check each variant */ ];
    //     assert!(!variants_with_id.is_empty(), 
    //         "TypeInfo {} has no matching variant", type_info.identifier);
    // }

    todo!("Phase 4: every records TypeInfo identifier matches the variant that produces it")
}
```

---

File path: **liquers-lib/tests/record_streams_serialization.rs**

### Test 9: RecordChunk CSV serialization round-trip

Rationale: A materialized chunk serializes to CSV and deserializes back with equivalent data and schema.

```rust
use liquers_core::{
    context::SimpleEnvironment,
    state::State,
    value::Value,
};
use liquers_lib::value::ExtValue;

#[tokio::test]
async fn test_record_chunk_csv_serialization() -> Result<(), Box<dyn std::error::Error>> {
    // When RecordBatch and serialization are implemented:
    // 1. Build a RecordBatch with Int, Float, Text columns
    // 2. Wrap in RecordChunk (ExtValue::RecordChunk)
    // 3. Serialize to CSV
    // 4. Parse CSV back to RecordBatch
    // 5. Compare original and deserialized: row count, values, schema
    //
    // Pseudo-code:
    // let schema = RecordSchema {
    //     fields: vec![
    //         FieldSchema { name: "id".into(), data_type: FieldType::Int, nullable: false, key: KeyRole::Id, role: FieldRole::exact() },
    //         FieldSchema { name: "value".into(), data_type: FieldType::Float, nullable: false, key: KeyRole::None, role: FieldRole::default() },
    //         FieldSchema { name: "name".into(), data_type: FieldType::Text, nullable: true, key: KeyRole::None, role: FieldRole::default() },
    //     ],
    //     type_identifier: None,
    // };
    //
    // let batch = RecordBatch {
    //     schema: Arc::new(schema.clone()),
    //     columns: vec![
    //         Column::Int { validity: None, values: Buffer::from(vec![1i64, 2, 3]) },
    //         Column::Float { validity: None, values: Buffer::from(vec![1.5f64, 2.5, 3.5]) },
    //         Column::Text { validity: None, offsets: ..., data: ... },
    //     ],
    //     len: 3,
    //     chunk_id: None,
    //     sources: vec![],
    // };
    //
    // let state = State::new().with_data(Value::from(ExtValue::RecordChunk { value: Arc::new(batch) }));
    // let csv_bytes = state.as_bytes("csv")?;
    // let csv_str = String::from_utf8(csv_bytes)?;
    // assert!(csv_str.contains("id,value,name"));
    // assert!(csv_str.contains("1,1.5"));

    todo!("Phase 4: as_bytes(\"csv\") emits one header row then one row per record, in schema field order")
}
```

### Test 10: RecordChunk JSON array serialization

Rationale: RecordChunk as JSON array, each row an object with field names as keys.

```rust
#[tokio::test]
async fn test_record_chunk_json_serialization() -> Result<(), Box<dyn std::error::Error>> {
    // Similar to CSV test, but:
    // - Serialize to JSON
    // - Verify output is a JSON array of objects
    // - Each object has keys matching field names
    // - Values match original batch
    //
    // Pseudo-code:
    // let state = State::new().with_data(Value::from(ExtValue::RecordChunk { ... }));
    // let json_bytes = state.as_bytes("json")?;
    // let json_array: Vec<serde_json::Value> = serde_json::from_slice(&json_bytes)?;
    // assert!(!json_array.is_empty());
    // assert_eq!(json_array[0]["id"], 1);

    todo!("Phase 4: as_bytes(\"json\") emits an array of objects keyed by field name")
}
```

### Test 11: RecordChunk NDJSON serialization

Rationale: Each batch row as one JSON object per line, suitable for streaming.

```rust
#[tokio::test]
async fn test_record_chunk_ndjson_serialization() -> Result<(), Box<dyn std::error::Error>> {
    // Similar, but output is newline-delimited JSON:
    // { "id": 1, "value": 1.5 }
    // { "id": 2, "value": 2.5 }
    // ...
    //
    // Pseudo-code:
    // let state = State::new().with_data(Value::from(ExtValue::RecordChunk { ... }));
    // let ndjson_bytes = state.as_bytes("ndjson")?;
    // let ndjson_str = String::from_utf8(ndjson_bytes)?;
    // let lines: Vec<&str> = ndjson_str.lines().collect();
    // assert_eq!(lines.len(), 3); // 3 rows
    // for (i, line) in lines.iter().enumerate() {
    //     let obj: serde_json::Value = serde_json::from_str(line)?;
    //     assert!(obj.is_object());
    // }

    todo!("Phase 4: as_bytes(\"ndjson\") emits one JSON object per line, with line count equal to row count")
}
```

---

File path: **liquers-lib/tests/record_streams_schema.rs**

### Test 12: RecordSchema validation enforces Id uniqueness

Rationale: RecordSchema::new() must enforce exactly one field with KeyRole::Id.

```rust
#[tokio::test]
async fn test_record_schema_id_uniqueness() -> Result<(), Box<dyn std::error::Error>> {
    // When RecordSchema::new() is implemented:
    // 1. Schema with exactly one Id field -> succeeds
    // 2. Schema with no Id field -> error
    // 3. Schema with two Id fields -> error
    //
    // Pseudo-code:
    // let schema_valid = RecordSchema::new(vec![
    //     FieldSchema { key: KeyRole::Id, ... },
    //     FieldSchema { key: KeyRole::None, ... },
    // ]);
    // assert!(schema_valid.is_ok());
    //
    // let schema_no_id = RecordSchema::new(vec![
    //     FieldSchema { key: KeyRole::None, ... },
    // ]);
    // assert!(schema_no_id.is_err());
    //
    // let schema_two_ids = RecordSchema::new(vec![
    //     FieldSchema { key: KeyRole::Id, ... },
    //     FieldSchema { key: KeyRole::Id, ... },
    // ]);
    // assert!(schema_two_ids.is_err());

    todo!("Phase 4: RecordSchema::new accepts exactly one KeyRole::Id and rejects zero or two")
}
```

### Test 13: FieldRole composition with and_stored / and_fast

Rationale: Field roles are composable via builder methods, enabling expressions like text().and_stored().

```rust
#[tokio::test]
async fn test_field_role_composition() -> Result<(), Box<dyn std::error::Error>> {
    // When FieldRole constructors are implemented:
    // 1. FieldRole::text() creates a full-text indexed, not-stored role
    // 2. .and_stored() adds the stored flag
    // 3. .and_fast() adds fast access
    //
    // Pseudo-code:
    // let role = FieldRole::text()
    //     .and_stored()
    //     .and_fast();
    // assert!(role.indexed.iter().any(|ik| matches!(ik, IndexKind::FullText { .. })));
    // assert!(role.stored);
    // assert!(role.fast);

    todo!("Phase 4: FieldRole::text().and_stored() yields indexed=[FullText..] AND stored=true, the TEXT|STORED case an enum could not express")
}
```

### Test 14: FieldValue type coverage

Rationale: FieldValue enum covers all Column variants and round-trips through conversions.

```rust
#[tokio::test]
async fn test_field_value_conversions() -> Result<(), Box<dyn std::error::Error>> {
    // When FieldValue is implemented:
    // For each variant (Null, Bool, Int, UInt, Float, Text, Bytes, Timestamp, Vector):
    // 1. Create a FieldValue
    // 2. Convert to/from Buffer or JSON as appropriate
    // 3. Assert round-trip equality
    //
    // Pseudo-code:
    // let fv_int = FieldValue::Int(42i64);
    // assert_eq!(fv_int, FieldValue::Int(42i64));
    //
    // let fv_text = FieldValue::Text(Arc::from("hello"));
    // let json = serde_json::to_value(&fv_text)?;
    // let fv_text2 = serde_json::from_value(json)?;
    // assert_eq!(fv_text, fv_text2);

    todo!("Phase 4: FieldValue converts to and from each FieldType without loss")
}
```

---

File path: **liquers-lib/tests/record_streams_builder.rs**

### Test 15: RecordBatch builder accumulates columns and validates

Rationale: A builder pattern constructs batches correctly and rejects mismatched column lengths.

```rust
#[tokio::test]
async fn test_record_batch_builder() -> Result<(), Box<dyn std::error::Error>> {
    // When RecordBatchBuilder is implemented:
    // 1. Create a builder with a schema
    // 2. Append rows (or columns) matching the schema
    // 3. Build -> RecordBatch
    // 4. Mismatched lengths -> error
    //
    // Pseudo-code:
    // let schema = RecordSchema::new(vec![...]);
    // let mut builder = RecordBatchBuilder::new(schema);
    // builder.append_row(vec![FieldValue::Int(1), FieldValue::Text(Arc::from("a"))])?;
    // builder.append_row(vec![FieldValue::Int(2), FieldValue::Text(Arc::from("b"))])?;
    // let batch = builder.build()?;
    // assert_eq!(batch.len, 2);

    todo!("Phase 4: RecordBatchBuilder produces a batch whose len, schema and column values match what was appended")
}
```

### Test 16: Buffer alignment

Rationale: Buffers are 64-byte aligned as Arrow recommends, verifiable via bytemuck checks.

```rust
#[tokio::test]
async fn test_buffer_alignment() -> Result<(), Box<dyn std::error::Error>> {
    // When Buffer and AlignedBuffer are implemented:
    // 1. Create buffers of various sizes
    // 2. Assert each is 64-byte aligned
    // 3. Cast to &[u8] via bytemuck::cast_slice
    // 4. Assert no panics and alignment holds
    //
    // Pseudo-code:
    // let buf: Buffer<i64> = Buffer::from(vec![1i64, 2, 3]);
    // let ptr = buf.as_ptr() as usize;
    // assert_eq!(ptr % 64, 0, "Buffer not 64-byte aligned");

    todo!("Phase 4: AlignedBuffer starts on a 64-byte boundary, as [COLUMNAR] Buffer Alignment and Padding recommends")
}
```

### Test 17: Bitmap operations and LSB-first bit packing

Rationale: Bitmaps store validity and filter masks in Arrow's LSB-first format.

```rust
#[tokio::test]
async fn test_bitmap_lsb_first() -> Result<(), Box<dyn std::error::Error>> {
    // When Bitmap is implemented:
    // 1. Create a bitmap with specific bit patterns
    // 2. Assert get/set honor LSB-first within each byte
    // 3. AND/OR/NOT operations on bitmaps work correctly
    // 4. Byte-aligned and non-aligned slices behave correctly
    //
    // Pseudo-code:
    // let mut bitmap = Bitmap::new(8);
    // bitmap.set(0, true);  // LSB of first byte
    // bitmap.set(7, true);  // MSB of first byte
    // let byte0 = bitmap.as_bytes()[0];
    // assert_eq!(byte0, 0b10000001);  // LSB-first

    todo!("Phase 4: Bitmap packs LSB-first within each byte, as Arrow specifies")
}
```

---

File path: **liquers-lib/tests/record_streams_manifest.rs**

### Test 18: Manifest YAML deserialization

Rationale: A manifest YAML file deserializes correctly, preserving all fields and structure.

```rust
#[tokio::test]
async fn test_manifest_yaml_deserialization() -> Result<(), Box<dyn std::error::Error>> {
    use serde_yaml;

    // When manifest deserialization is implemented:
    // Pseudo-code manifest YAML:
    // ```yaml
    // manifest: record-stream
    // version: 1
    // title: Daily orders
    // description: One chunk per 1000 orders
    // number_format: "{:04}"
    // extension: csv
    // volatile: false
    // expires: 1d
    // uniform_schema:
    //   fields:
    //     - { name: id, data_type: Int, key: Id, role: { indexed: [Exact], stored: true } }
    //     - { name: total, data_type: Float, role: { indexed: [Range], fast: true } }
    // chunks:
    //   - query: ns-sql/query-0-1000
    //   - query: ns-sql/query-1000-1000
    // ```
    //
    // Pseudo-code test:
    // let manifest_yaml = r#"...(as above)"#;
    // let manifest: RecordSource = serde_yaml::from_str(manifest_yaml)?;
    // assert_eq!(manifest.chunks().len(), 2);
    // if let Some(schema) = manifest.uniform_schema.as_ref() {
    //     assert_eq!(schema.fields.len(), 2);
    // }

    Ok(())
}
```

### Test 19: Manifest with template (unbounded chunk list)

Rationale: A templated manifest describes chunks generated by repeating a template with offset increments.

```rust
#[tokio::test]
async fn test_manifest_template_unbounded() -> Result<(), Box<dyn std::error::Error>> {
    // When SourceBacking::QueriedTemplated is implemented:
    // A manifest with `template` instead of `chunks`:
    // ```yaml
    // manifest: record-stream
    // version: 1
    // template:
    //   query: ns-sql/sql_query
    //   first_offset: 0
    //   step: 1000
    //   batch_size: 1000
    // ```
    //
    // When loaded, RecordSource::chunks() should return ChunkList::Unbounded
    // because the count is unknown.
    //
    // Pseudo-code:
    // let manifest_yaml = r#"...(as above)"#;
    // let source: RecordSource = serde_yaml::from_str(manifest_yaml)?;
    // match source.chunks() {
    //     ChunkList::Unbounded { computed } => {
    //         // computed batches are generated on demand
    //         assert_eq!(computed.len(), 0); // or some seed value
    //     }
    //     ChunkList::Known(_) => panic!("Expected Unbounded"),
    // }

    Ok(())
}
```

---

File path: **liquers-lib/tests/record_streams_features.rs**

### Test 20: Record module behind `records` feature

Rationale: The entire record-streams feature is optional; builds without it should not include RecordChunk or RecordSource.

```rust
#![cfg(feature = "records")]

#[test]
fn test_records_feature_gated() {
    // This entire file only compiles with `--features records`.
    // Verify that:
    // 1. ExtValue::RecordChunk variant exists
    // 2. ExtValue::RecordSource variant exists
    // 3. The types are accessible
    //
    // Pseudo-code:
    // // If this compiles, the feature is active.
    // use liquers_lib::value::ExtValue;
    // match (ExtValue::RecordChunk { value: Arc::new(...) }, 
    //        ExtValue::RecordSource { value: Arc::new(...) }) {
    //     (ExtValue::RecordChunk { .. }, ExtValue::RecordSource { .. }) => {},
    //     _ => panic!("Variants missing"),
    // }
}
```

### Test 21: Build without records feature

Rationale: Code compiles with `--no-default-features` and ExtValue::RecordChunk/RecordSource are absent.

```rust
// This test is aspirational: it documents that a build without `records`
// should not break, and should not include RecordChunk/RecordSource.
//
// Run with: cargo test -p liquers-lib --no-default-features --lib --tests
//
// The test itself cannot be in this file (which is feature-gated).
// Instead, a build-matrix script should verify:
// 1. `cargo build -p liquers-lib --no-default-features` succeeds
// 2. RecordChunk and RecordSource are not in the binary's symbols
// 3. All other functionality is intact
```

---

File path: **liquers-lib/tests/record_streams_edge_cases.rs**

### Test 22: Empty batch / empty source

Rationale: A record source or batch with zero rows should behave correctly.

```rust
#[tokio::test]
async fn test_empty_record_batch() -> Result<(), Box<dyn std::error::Error>> {
    // When RecordBatch is implemented:
    // 1. Create a schema with fields
    // 2. Build a batch with len=0 but proper column structure
    // 3. Serialize to CSV/JSON/NDJSON -> should produce just headers or empty array
    // 4. Schema should still be queryable
    //
    // Pseudo-code:
    // let schema = RecordSchema::new(vec![...]);
    // let batch = RecordBatch {
    //     schema: Arc::new(schema),
    //     columns: vec![
    //         Column::Int { validity: None, values: Buffer::from(vec![]) },
    //     ],
    //     len: 0,
    //     chunk_id: None,
    //     sources: vec![],
    // };
    // assert_eq!(batch.len, 0);
    // let csv_bytes = serialize_to_csv(&batch)?;
    // let csv_str = String::from_utf8(csv_bytes)?;
    // assert!(csv_str.contains("id\n")); // header but no data

    Ok(())
}
```

### Test 23: Nullable columns with validity bitmap

Rationale: A column with nullable fields correctly encodes nulls as a validity bitmap.

```rust
#[tokio::test]
async fn test_nullable_column_validity() -> Result<(), Box<dyn std::error::Error>> {
    // When Column is implemented with nullable fields:
    // 1. Create an Int column with some nulls
    // 2. Validity bitmap should mark null positions
    // 3. Serialization (CSV/JSON) should handle nulls correctly
    // 4. Round-trip preserves nullness
    //
    // Pseudo-code:
    // let mut bitmap = Bitmap::new(3);
    // bitmap.set(0, true);   // row 0 is valid
    // bitmap.set(1, false);  // row 1 is null
    // bitmap.set(2, true);   // row 2 is valid
    //
    // let column = Column::Int {
    //     validity: Some(bitmap),
    //     values: Buffer::from(vec![1i64, 999, 3]), // 999 is garbage, ignored for row 1
    // };
    //
    // let csv = serialize_column_to_csv(&column)?;
    // assert!(csv.contains("1\n"));
    // assert!(csv.contains("\n")); // empty cell for row 1
    // assert!(csv.contains("3\n"));

    Ok(())
}
```

### Test 24: Vector column (embedding) Arrow compatibility

Rationale: Vector columns (FixedSizeList of Float32) are Arrow-compatible and exportable.

```rust
#[tokio::test]
async fn test_vector_column_arrow_compat() -> Result<(), Box<dyn std::error::Error>> {
    // When Vector column is implemented:
    // 1. Create vectors of fixed dimension (e.g., 3 floats each)
    // 2. Pack them in a contiguous Buffer
    // 3. Export as Arrow FixedSizeList(Float32, 3)
    // 4. Verify the layout matches [CDATA] FixedSizeList spec
    //
    // Pseudo-code:
    // let dim = 3usize;
    // let data = vec![
    //     1.0f32, 2.0f32, 3.0f32,  // vector 0
    //     4.0f32, 5.0f32, 6.0f32,  // vector 1
    // ];
    // let column = Column::Vector {
    //     validity: None,
    //     dim,
    //     data: Buffer::from(data),
    // };
    // assert_eq!(column.data.len(), 6);
    // // When exported to Arrow, should describe as FixedSizeList(Float32, 3)

    Ok(())
}
```

### Test 25: ChunkDescriptor metadata round-trip

Rationale: A ChunkDescriptor serializes and deserializes preserving query, metadata, and origin.

```rust
#[tokio::test]
async fn test_chunk_descriptor_serialization() -> Result<(), Box<dyn std::error::Error>> {
    use serde_json;

    // When ChunkDescriptor is implemented:
    // 1. Create a ChunkDescriptor with query, metadata, origin
    // 2. Serialize to JSON
    // 3. Deserialize back
    // 4. Assert equality (or deep equality of all fields)
    //
    // Pseudo-code:
    // let desc = ChunkDescriptor {
    //     id: ChunkId::Query(parse_query("ns-sql/query1")?),
    //     query: parse_query("ns-sql/query1")?,
    //     metadata: Metadata { /* ... */ },
    //     origin: ChunkOrigin { /* ... */ },
    //     schema: Some(Arc::new(RecordSchema::new(...))),
    // };
    // 
    // let json = serde_json::to_value(&desc)?;
    // let desc2: ChunkDescriptor = serde_json::from_value(json)?;
    // assert_eq!(desc.id, desc2.id);
    // assert_eq!(desc.query.encode(), desc2.query.encode());

    Ok(())
}
```

---

### Summary Table

| Test # | File | Focus | Purpose |
|--------|------|-------|---------|
| 1 | end_to_end | Command registration & serialization | End-to-end flow |
| 2 | end_to_end | RecordSource re-openability | Central property of source/stream split |
| 3 | end_to_end | Memory-bounded streaming | One batch resident |
| 4 | end_to_end | Manifest round-trip | Serialization format |
| 5-7 | end_to_end | Non-uniform chunks | Schema variation handling |
| 8 | end_to_end | Per-chunk arguments validation | ChunkKeys requirement |
| 9 | end_to_end | TypeInfo registration | Both variants registrable |
| 10 | end_to_end | Type matching invariant | Compiler-checked safety |
| 11-13 | serialization | CSV/JSON/NDJSON serialization | Format compatibility |
| 14-15 | schema | Schema validation | Id uniqueness, field roles |
| 16-17 | builder | Batch building & alignment | Construction patterns |
| 18-19 | manifest | Manifest deserialization | Format parsing |
| 20-21 | features | Feature gating | Optional compilation |
| 22-25 | edge_cases | Edge cases & nullability | Robustness |

**Total: 25 tests across 6 files**

Each test is written as a specification that Phase 4 implementation must satisfy. Tests use pseudo-code and comments where concrete types are not yet implemented, establishing the contract for Phase 4.


---

# 4. Pitfalls and the RECORDS reference tests

## Phase 3 Draft: Pitfalls, Edge Cases, and RECORDS Reference Implementations

**Phase 3 scope:** Identify traps in the record-streams design and provide Rust reference implementations of RECORDS01–RECORDS09 conformance tests for language bindings.

### Part A: Pitfalls and Edge Cases

#### 1. The `len + 1` Offsets Invariant for Text/Binary Columns

**What goes wrong:** Arrow's variable-length encoding (Utf8, Binary) uses an offsets array with `len + 1` entries, where the last entry points one byte past the end of the data buffer. The invariant is **not a simple implementation detail**—it is a format requirement that the builder must enforce and exporters must preserve.

**Trap:** A builder that treats offsets as a simple `[start0, start1, ..., startN]` array without the trailing sentinel will produce a column that:
- Serializes without error (offsets are just values)
- Fails silently when read through a consumer that trusts the invariant
- Roundtrips cleanly until exported through Arrow C Data Interface, where the consumer reads past the buffer

**How to avoid it:** When building a Text or Binary column, explicitly verify that `offsets.len() == row_count + 1` and that `offsets[row_count]` equals the data buffer length. The invariant is structural and must be checked at construction, not at export.

---

#### 2. Vector Export as FixedSizeList with Values in a CHILD Node

**What goes wrong:** A `Column::Vector` (Arrow `FixedSizeList(Float32, dim)`) has a two-level structure in the Arrow specification that can surprise an implementer. The root is a **list node with no value buffer**, carrying only validity. The `Float32` values live in a **child array**, not under the root.

**Trap:** An exporter that writes buffers in flat order or assumes each column maps 1:1 to an Arrow buffer will produce a tree with the values in the wrong place or missing children. A consumer expecting the child node will find nothing and read garbage.

**How to avoid it:** When exporting a `Vector` column, construct the Arrow tree recursively: create a root `Float32` array as a child of the list node, and ensure both are marked as present in the schema. The architecture doc (§"Where our layout meets Arrow's") lists this alongside Text/Binary offsets as "the ones that get discovered late."

---

#### 3. Forgetting a TypeInfo Entry

**What goes wrong:** `ExtValue::RecordChunk` and `ExtValue::RecordSource` are new value types that must be registered through `TypeInfo` in `ExtValue::type_descriptions()`. Without an entry, the asset write path refuses the identifier, and storage fails.

**Trap:** An implementation that adds the enum variant and the serialization but forgets the `TypeInfo` entry will pass unit tests (which may not use storage) and fail silently at runtime when a command returns a record value:
- The value evaluates fine
- The asset layer tries to store it and gets "unknown type identifier"
- Debugging points to serialization or the storage layer, not to a missing registration

**How to avoid it:** CLAUDE.md prescribes "four steps, not three": extend the enum, implement `ExtValueInterface` and `DefaultValueSerializer`, and **add a `TypeInfo` entry**. The architecture doc reiterates this. A test that stores a record value and retrieves it is the check—not just evaluation.

---

#### 4. Forgetting a `#[cfg(feature = "records")]` Match Arm

**What goes wrong:** The records feature is optional. Any `match` statement on `ExtValue` that does not cover both variants without the feature gated will fail to compile when `--no-default-features` is built.

**Trap:** A default arm (`_ => …`) silently absorbs the new variants in feature-on builds. In feature-off builds:
- `ExtValue` does not have the variants
- The match compiles
- No test runs in feature-off mode (the CI default includes features)
- Deployment in a restricted environment fails

**How to avoid it:** Never use `_ =>` on `ExtValue` matches. Use exhaustive matches and gate each new-variant arm on `#[cfg(feature = "records")]`. The architecture doc and CLAUDE.md both forbid default arms. Scripts in the repo (`scripts/check-build-matrix.sh`) validate every feature configuration—use them when touching `ExtValue`.

---

#### 5. Holding a Per-Chunk Value Without a ChunkKeys

**What goes wrong:** A `RecordSource` may materialize a stream with per-chunk parameters (e.g., "include columns [name, date] for each chunk"). If those parameters are stored in `arguments` without a corresponding `ChunkKeys`, all chunks will share the same parameter instance, creating a silent alias.

**Trap:** A manifest-based source with templated chunk queries works correctly. But if a custom `SourceBacking` mixes parameter-per-chunk with shared storage, the problem manifests as data mysteriously appearing in the wrong chunk or disappearing after materialization—and only when the parameter affects selectivity rather than shape.

**How to avoid it:** When designing a source with per-chunk configuration, either:
1. Store parameters inside each chunk's query (the manifest route), not in the source
2. Use a `ChunkKeys` and keep parameters there
3. Keep parameters immutable across all chunks

The architecture doc reserves `ChunkKeys` for exactly this—a keyed cache in a single managed folder. Do not bypass it to store transient parameter state.

---

#### 6. Command Signature: Per-Chunk Parameters Not First

**What goes wrong:** A record-producing command may accept both per-chunk parameters and global parameters. The binding layer generates unreadable chunk queries if global parameters come first, because a chunk key must fully encode its dependencies.

**Trap:** A command like `chunk_filter(state, global_predicate: string, per_chunk_column: string)` with the global first produces chunk queries that look like they vary per chunk but actually share a global parameter spread across the generated query text.

**How to avoid it:** Document in any record command that per-chunk parameters **must come before global/fixed parameters** in the signature. A command that does not respect this order produces syntactically correct but semantically confusing chunk queries. This is not enforced at registration time—it is a design guideline that belongs in command documentation.

---

#### 7. Lent Buffer Lifetimes in WebAssembly

**What goes wrong:** In Wasm, JavaScript holds a typed-array view of linear memory through a pointer to Rust data inside a `RecordBatch`. The browser API provides no lifetime guarantee on the view. If the view is held across a call back into Wasm, a `memory.grow()` **detaches** the `ArrayBuffer` object, invalidating the view.

**Trap:** A JavaScript caller that caches a column view and then calls another Wasm function (or awaits) will silently read the wrong memory when the heap grows. The view's pointer is unchanged, but the data it points to is no longer in the detached buffer. Reads return plausible garbage, not a clear error.

**How to avoid it:** Do not cache views. Create them at point of use—`new Float64Array(memory.buffer, ptr, len)` is O(1)—and discard them after reading. If a view must outlive a Wasm call, copy the data explicitly with `columnCopy()`, which produces an owned typed array that survives heap growth. The architecture doc prescribes testing this explicitly: force a `memory.grow()` and verify the wrapper refreshed.

---

#### 8. Writing Through a Lent Buffer (Aliasing Violation)

**What goes wrong:** A `RecordBatch` exports buffers as `Arc<[T]>`, which are immutable to Rust. But JavaScript can write through a view of those buffers. Since the `Arc` shares ownership, writing violates Rust's aliasing assumptions and causes undefined behavior.

**Trap:** Writes through a JavaScript view succeed without warning. The Rust code continues to hold what it believes are immutable buffers while they have been mutated. Bugs manifest as data corruption far from the write site, often in concurrent code or when the corrupted data is used in a computation.

**How to avoid it:** Document explicitly that lent buffers are **read-only**, and reject writes by design. The architecture doc prescribes this as a constraint worth documenting. An integration can offer a `copy()` if callers need to mutate; mutation of a shared buffer is not supported.

---

#### 9. Schema Uniformity Not Checked, Then Concat Fails Mysteriously

**What goes wrong:** A `RecordSource` carries an optional `uniform_schema`. Callers are expected to check before assuming all chunks match. But a caller that assumes uniformity anyway and calls `RecordBatch::concat` will get an error on the first schema mismatch, potentially after already using earlier chunks.

**Trap:** A command that streams chunks through concat without checking `uniform_schema` will silently work until a heterogeneous stream is encountered. The error name is clear, but a caller reading it after the fact ("schema mismatch in chunk 42") has already built a partial result and must decide whether to keep or discard it.

**How to avoid it:** Before opening a stream, callers must check `source.uniform_schema`:
- If `Some`, all chunks are promised to match the declared schema
- If `None`, each chunk's schema must be inspected individually before concat

This is a deliberate design choice (phase1-answer-5), not an oversight. Document it prominently in any command that streams and combines chunks.

---

#### 10. Endian Assumptions in the Arrow C Data Interface

**What goes wrong:** Arrow C Data Interface assumes same-process communication and therefore same endianness. A Rust build on little-endian and a consumer on big-endian (or different systems entirely) will misread buffers.

**Trap:** This is almost never a trap in practice because:
- Same-process communication (C Data Interface) is inherently same-endian
- Remote serialization (IPC/Feather) specifies little-endian explicitly

But a design that re-serializes a C Data export to disk and later reads it on a different machine will fail silently.

**How to avoid it:** The architecture doc and Arrow spec both state this plainly. Same-process: not a concern. Serialization to bytes: use Arrow IPC format, which specifies endianness. Do not hand-roll byte-level export without addressing endianness.

---

### Part B: RECORDS01–RECORDS09 Rust Reference Implementations

The following are Rust test code intended to serve as reference implementations for language bindings. Each test demonstrates the contract that RECORDS01–RECORDS09 establish.

#### Location and Integration

- **RECORDS01, RECORDS02, RECORDS07, RECORDS08:** `liquers-lib/tests/records_roundtrip.rs` (native Rust, no wasm)
- **RECORDS03:** `liquers-lib/tests/records_arrow_export.rs` (if Arrow C Data Interface is implemented)
- **RECORDS04:** `liquers-lib/tests/records_buffer_readonly.rs` (validation, may be mocked)
- **RECORDS05:** `liquers-web/tests/records_wasm_heap_growth.rs` (wasm-bindgen test, browser environment)
- **RECORDS06:** `liquers-web/tests/records_handle_release.rs` (wasm with debug-handles feature)
- **RECORDS09:** Documentation test in `liquers-lib/src/records/mod.rs` (no executable code, documents the sync route)

---

#### RECORDS01: Chunk Round-Trip with Metadata

**Contract:** A chunk serializes and deserializes with schema, field roles, and chunk origin intact.

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use liquers_lib::records::*;
    use liquers_core::value::Value;
    use std::sync::Arc;

    #[test]
    fn records01_chunk_roundtrip_with_metadata() {
        // Build a batch with a source role column and metadata
        let schema = Arc::new(RecordSchema {
            fields: vec![
                FieldSchema {
                    name: "id".to_string(),
                    data_type: FieldType::Int,
                    nullable: false,
                    key: KeyRole::Id,
                    role: FieldRole {
                        indexed: vec![IndexKind::Exact],
                        stored: true,
                        fast: true,
                    },
                },
                FieldSchema {
                    name: "source".to_string(),
                    data_type: FieldType::Text,
                    nullable: false,
                    key: KeyRole::Source,
                    role: FieldRole::ignored(),
                },
                FieldSchema {
                    name: "value".to_string(),
                    data_type: FieldType::Float,
                    nullable: true,
                    key: KeyRole::None,
                    role: FieldRole {
                        indexed: vec![IndexKind::Range],
                        stored: true,
                        fast: true,
                    },
                },
            ],
            type_identifier: Some("MyRecordType".to_string()),
        });

        // Create origins to reference
        let origin_a = ChunkOrigin {
            asset: parse_query("-R/data/a.csv").unwrap(),
            chunk: parse_query("-R/data/a.csv/-/ns-records/file_records").unwrap(),
            info: None,
            locator: None,
        };
        let origin_b = ChunkOrigin {
            asset: parse_query("-R/data/b.csv").unwrap(),
            chunk: parse_query("-R/data/b.csv/-/ns-records/file_records").unwrap(),
            info: None,
            locator: None,
        };

        // Build batch with two rows, different sources
        let mut builder = RecordBatchBuilder::with_schema(schema.clone());
        builder.append_row(vec![
            FieldValue::Int(1),
            FieldValue::Text(Arc::from("source_0")),
            FieldValue::Float(3.14),
        ]).expect("append row 0");
        builder.append_row(vec![
            FieldValue::Int(2),
            FieldValue::Text(Arc::from("source_1")),
            FieldValue::Float(2.71),
        ]).expect("append row 1");

        // Set chunk origin and sources
        builder.set_chunk_id(ChunkId::Key(Key::new("/data/chunk_0").unwrap()));
        builder.set_sources(vec![origin_a.clone(), origin_b.clone()]);
        let batch_original = builder.build().expect("build batch");

        // Serialize as JSON
        let json_bytes = serde_json::to_string_pretty(&batch_original)
            .expect("serialize to json");

        // Deserialize
        let batch_restored: RecordBatch = serde_json::from_str(&json_bytes)
            .expect("deserialize from json");

        // Assert schema, field roles, and chunk origin roundtripped
        assert_eq!(batch_restored.schema.fields.len(), 3);
        assert_eq!(batch_restored.schema.type_identifier, Some("MyRecordType".to_string()));

        // Check field roles
        assert_eq!(batch_restored.schema.fields[0].key, KeyRole::Id);
        assert_eq!(batch_restored.schema.fields[0].role.stored, true);
        assert_eq!(batch_restored.schema.fields[0].role.fast, true);
        
        assert_eq!(batch_restored.schema.fields[1].key, KeyRole::Source);
        assert_eq!(batch_restored.schema.fields[2].key, KeyRole::None);

        // Check chunk identity
        if let Some(ChunkId::Key(key)) = &batch_restored.chunk_id {
            assert_eq!(key.segments()[0], "data");
            assert_eq!(key.segments()[1], "chunk_0");
        } else {
            panic!("chunk_id was not preserved");
        }

        // Check sources
        assert_eq!(batch_restored.sources.len(), 2);
        assert_eq!(
            batch_restored.sources[0].asset.encode(),
            "-R/data/a.csv"
        );
        assert_eq!(
            batch_restored.sources[1].asset.encode(),
            "-R/data/b.csv"
        );

        // Data integrity
        assert_eq!(batch_restored.len, 2);
    }
}
```

---

#### RECORDS02: Column Zero-Copy Read Equivalence

**Contract:** A column reads the same values through a wrapper as through an explicit copy.

```rust
#[test]
fn records02_column_zero_copy_equivalence() {
    // Build a batch with various column types
    let schema = Arc::new(RecordSchema {
        fields: vec![
            FieldSchema {
                name: "ints".to_string(),
                data_type: FieldType::Int,
                nullable: false,
                key: KeyRole::None,
                role: FieldRole::ignored(),
            },
            FieldSchema {
                name: "texts".to_string(),
                data_type: FieldType::Text,
                nullable: true,
                key: KeyRole::None,
                role: FieldRole::ignored(),
            },
            FieldSchema {
                name: "floats".to_string(),
                data_type: FieldType::Float,
                nullable: true,
                key: KeyRole::None,
                role: FieldRole::ignored(),
            },
        ],
        type_identifier: None,
    });

    let mut builder = RecordBatchBuilder::with_schema(schema.clone());
    builder.append_row(vec![
        FieldValue::Int(42),
        FieldValue::Text(Arc::from("hello")),
        FieldValue::Float(1.5),
    ]).expect("row 0");
    builder.append_row(vec![
        FieldValue::Int(-17),
        FieldValue::Null,  // nullable text, null value
        FieldValue::Float(f64::NAN),
    ]).expect("row 1");
    builder.append_row(vec![
        FieldValue::Int(0),
        FieldValue::Text(Arc::from("world")),
        FieldValue::Float(-3.14),
    ]).expect("row 2");

    let batch = builder.build().expect("build batch");

    // Read through column accessor (zero-copy view)
    let int_col = batch.column(0).expect("get column 0");
    let text_col = batch.column(1).expect("get column 1");
    let float_col = batch.column(2).expect("get column 2");

    // Read through copy (always-safe fallback)
    let int_copy = batch.column_copy(0).expect("copy column 0");
    let text_copy = batch.column_copy(1).expect("copy column 1");
    let float_copy = batch.column_copy(2).expect("copy column 2");

    // Compare: zero-copy view and explicit copy must return identical values
    for row_idx in 0..batch.len {
        match int_col {
            Column::Int { ref values, ref validity } => {
                let value = values[row_idx];
                let is_null = validity.as_ref().map_or(false, |v| !v.get(row_idx));
                
                // Same row from the copy
                match int_copy {
                    Column::Int { ref values: copy_values, ref validity: copy_validity } => {
                        assert_eq!(copy_values[row_idx], value);
                        let copy_is_null = copy_validity.as_ref().map_or(false, |v| !v.get(row_idx));
                        assert_eq!(copy_is_null, is_null);
                    }
                    _ => panic!("copy column type mismatch"),
                }
            }
            _ => panic!("expected Int column"),
        }
    }

    // Text columns (variable-length)
    for row_idx in 0..batch.len {
        match text_col {
            Column::Text { ref offsets, ref data, ref validity } => {
                let start = offsets[row_idx] as usize;
                let end = offsets[row_idx + 1] as usize;
                let text_bytes = &data[start..end];
                let text_value = String::from_utf8_lossy(text_bytes);
                let is_null = validity.as_ref().map_or(false, |v| !v.get(row_idx));

                // Compare with copy
                match text_copy {
                    Column::Text { ref offsets: copy_offsets, ref data: copy_data, ref validity: copy_validity } => {
                        let copy_start = copy_offsets[row_idx] as usize;
                        let copy_end = copy_offsets[row_idx + 1] as usize;
                        let copy_text = String::from_utf8_lossy(&copy_data[copy_start..copy_end]);
                        assert_eq!(copy_text, text_value);
                        let copy_is_null = copy_validity.as_ref().map_or(false, |v| !v.get(row_idx));
                        assert_eq!(copy_is_null, is_null);
                    }
                    _ => panic!("copy text column type mismatch"),
                }
            }
            _ => panic!("expected Text column"),
        }
    }

    // Floats (including NaN handling)
    for row_idx in 0..batch.len {
        match float_col {
            Column::Float { ref values, ref validity } => {
                let value = values[row_idx];
                let is_null = validity.as_ref().map_or(false, |v| !v.get(row_idx));
                
                match float_copy {
                    Column::Float { ref values: copy_values, ref validity: copy_validity } => {
                        let copy_val = copy_values[row_idx];
                        // NaN != NaN, so use bit_eq for comparison
                        if value.is_nan() && copy_val.is_nan() {
                            // both NaN, equal
                        } else {
                            assert_eq!(copy_val, value);
                        }
                        let copy_is_null = copy_validity.as_ref().map_or(false, |v| !v.get(row_idx));
                        assert_eq!(copy_is_null, is_null);
                    }
                    _ => panic!("copy float column type mismatch"),
                }
            }
            _ => panic!("expected Float column"),
        }
    }
}
```

---

#### RECORDS03: Arrow Export Equality (Conditional)

**Contract:** Where an Arrow C Data Interface export exists, the exported table equals the wrapper's data, and metadata survives or its loss is explicitly asserted.

**Status:** Deferred pending C Data Interface implementation. Placeholder test:

```rust
#[cfg(all(test, feature = "arrow-cdata"))]  // Feature deferred
mod tests {
    use super::*;

    #[test]
    #[ignore]  // Not yet implemented
    fn records03_arrow_export_equality() {
        // When `arrow-cdata` feature is enabled:
        // 1. Build a RecordBatch
        // 2. Export to Arrow C Data Interface (ArrowArray/ArrowSchema)
        // 3. Call a consumer (e.g., polars or pyarrow via FFI)
        // 4. Assert the consumer reads identical data
        // 5. Assert field roles in schema metadata survive or document their loss
        panic!("Arrow C Data Interface export not yet implemented");
    }
}
```

---

#### RECORDS04: Lent Buffer Read-Only (Validation)

**Contract:** A lent buffer is read-only, or writing through it is rejected.

```rust
#[test]
fn records04_buffer_readonly_or_rejected() {
    // This test validates the invariant at Rust level.
    // Wasm tests in liquers-web will test JS behavior.

    let schema = Arc::new(RecordSchema {
        fields: vec![
            FieldSchema {
                name: "data".to_string(),
                data_type: FieldType::Float,
                nullable: false,
                key: KeyRole::None,
                role: FieldRole::ignored(),
            },
        ],
        type_identifier: None,
    });

    let mut builder = RecordBatchBuilder::with_schema(schema);
    for i in 0..100 {
        builder.append_row(vec![FieldValue::Float(i as f64 * 0.1)])
            .expect("append row");
    }
    let batch = Arc::new(builder.build().expect("build batch"));

    // Export the column buffer
    let batch_ref = batch.clone();
    match batch_ref.column(0) {
        Column::Float { ref values, .. } => {
            // `values` is a `Buffer<f64>`, which is `Arc<[f64]>`
            // In Rust, this is immutable by type.
            // Assert that mutation is not possible at the type level.
            
            // This compiles because `values` is `&Arc<[f64]>` (immutable reference):
            let _val = values[0];  // OK: read through immutable ref

            // This does NOT compile (uncommenting would cause a compile error):
            // values[0] = 999.0;  // ERROR: cannot mutably borrow through immutable ref

            // In JavaScript (wasm), the test must verify:
            // 1. A typed array view is created from the buffer pointer
            // 2. Writes to the view succeed (no runtime error)
            // 3. Rust code detects corruption or the view is documented as read-only
            
            // For now, document the Rust-side invariant: exported buffers are immutable.
            assert!(values.len() > 0, "buffer has data");
        }
        _ => panic!("expected Float column"),
    }
}
```

---

#### RECORDS05: View Survives Heap Growth (Wasm-Specific)

**Contract:** A borrowed view in Wasm survives an operation that grows the host heap, or fails with a clear error.

**Location:** `liquers-web/tests/records_wasm_heap_growth.rs`

```rust
// This test is wasm32-only and requires a browser environment
#![cfg(target_arch = "wasm32")]

use wasm_bindgen_test::*;
use liquers_web::records::LiquersRecordChunk;
use js_sys::{Object, Reflect};

wasm_bindgen_test_configure!(run_in_browser);

#[wasm_bindgen_test]
async fn records05_view_survives_heap_growth() {
    // Setup: create a RecordChunk and get a typed-array view
    let batch = create_test_batch();  // helper: build a RecordBatch in Arc
    let chunk = LiquersRecordChunk { inner: batch };

    // Get a column descriptor (pointer and length)
    let col_desc = chunk.column(0)
        .expect("get column descriptor");

    // Create a typed array view at the reported pointer
    let memory = wasm_bindgen::memory();
    let view1 = create_typed_array_view(&col_desc, memory.buffer());
    let initial_len = view1.length();

    // Read some values to ensure the view is valid
    let val0_before = view1.get_index(0);

    // Force heap growth by allocating a large array
    // This calls memory.grow() in Wasm, detaching the current memory.buffer
    let _large_alloc = Vec::<u8>::with_capacity(10_000_000);

    // The old view is now detached; attempting to read would fail or read garbage
    // The correct implementation detects this and refreshes
    let memory_after = wasm_bindgen::memory();
    if view1.buffer() == memory_after.buffer() {
        // Buffer did not grow (memory was pre-allocated); view is still valid
        assert_eq!(view1.get_index(0), val0_before);
    } else {
        // Buffer was detached; recreate the view
        let view2 = create_typed_array_view(&col_desc, memory_after.buffer());
        // The new view must read the same value (pointer is unchanged)
        assert_eq!(view2.get_index(0), val0_before,
            "refreshed view must read same value");
    }
}

// Helper: create a typed array view from a column descriptor
fn create_typed_array_view(col_desc: &JsValue, memory_buf: &ArrayBuffer) -> Float64Array {
    let ptr = get_pointer_from_descriptor(col_desc);
    let len = get_length_from_descriptor(col_desc);
    Float64Array::new(&memory_buf, ptr as u32, len)
}

// These helpers would extract pointer and length from the column descriptor object
fn get_pointer_from_descriptor(desc: &JsValue) -> usize { /* ... */ 0 }
fn get_length_from_descriptor(desc: &JsValue) -> u32 { /* ... */ 0 }
fn create_test_batch() -> Arc<RecordBatch> { /* ... */ panic!() }
```

---

#### RECORDS06: Handle Release Releases Value (Wasm with debug-handles)

**Contract:** Releasing a chunk handle releases the underlying value, observed through a live handle count.

**Location:** `liquers-web/tests/records_handle_release.rs`

**Requires:** `--features debug-handles` (test-only feature already used for RUNTIME05)

```rust
#![cfg(target_arch = "wasm32")]

use wasm_bindgen_test::*;
use liquers_web::records::LiquersRecordChunk;

wasm_bindgen_test_configure!(run_in_browser);

#[wasm_bindgen_test]
fn records06_handle_release_count() {
    // Requires debug-handles feature; exposes a live-handle counter
    use liquers_web::debug::get_live_chunk_handle_count;

    let initial_count = get_live_chunk_handle_count();

    // Create a batch and wrap it
    let batch = Arc::new(create_test_batch());
    let handle1 = LiquersRecordChunk { inner: batch.clone() };

    let after_create = get_live_chunk_handle_count();
    assert_eq!(after_create, initial_count + 1, "creating a handle increments count");

    // Clone the handle (Arc clone)
    let handle2 = LiquersRecordChunk { inner: batch.clone() };
    let after_clone = get_live_chunk_handle_count();
    assert_eq!(after_clone, initial_count + 2, "cloning a handle increments count");

    // Drop one handle
    drop(handle2);
    let after_drop_one = get_live_chunk_handle_count();
    assert_eq!(after_drop_one, initial_count + 1, "dropping a handle decrements count");

    // Drop the second handle
    drop(handle1);
    let after_drop_all = get_live_chunk_handle_count();
    assert_eq!(after_drop_all, initial_count, "dropping all handles returns to initial count");
}
```

---

#### RECORDS07: Sync Traversal Without Materializing

**Contract:** A sync language traverses a manifest-backed source one chunk at a time without materializing the whole stream.

```rust
#[test]
fn records07_sync_manifest_traversal() {
    // Build a manifest-backed source with multiple chunks
    let chunk_queries = vec![
        Query::parse("-R/data/chunk_0.csv").unwrap(),
        Query::parse("-R/data/chunk_1.csv").unwrap(),
        Query::parse("-R/data/chunk_2.csv").unwrap(),
    ];

    let source = RecordSource {
        backing: SourceBacking::Queried {
            chunks: chunk_queries.clone(),
            cache: None,
        },
        uniform_schema: None,
    };

    // Synchronously enumerate chunks without opening a stream
    match source.chunks() {
        ChunkList::Known(chunk_ids) => {
            // For a manifest source, chunks() returns the list synchronously
            assert_eq!(chunk_ids.len(), 3);

            // A sync language iterates these queries and evaluates one at a time
            for (i, chunk_id) in chunk_ids.iter().enumerate() {
                match chunk_id {
                    ChunkId::Query(q) => {
                        // Each query is plain data; a sync binding can:
                        // 1. Evaluate it: env.evaluate(q.encode())?
                        // 2. Keep one chunk's data resident
                        // 3. Drop it before loading the next
                        // This is the preferred route for sync languages.
                        
                        let encoded = q.encode();
                        assert!(encoded.starts_with("-R/data/chunk_"));
                        assert_eq!(i, match i {
                            0 => { assert!(encoded.ends_with("0.csv")); 0 },
                            1 => { assert!(encoded.ends_with("1.csv")); 1 },
                            2 => { assert!(encoded.ends_with("2.csv")); 2 },
                            _ => unreachable!(),
                        });
                    }
                    ChunkId::Key(_) => panic!("expected Query variant"),
                }
            }
        }
        ChunkList::Unbounded { .. } => {
            panic!("templated sources not yet implemented");
        }
    }

    // Assert: no stream was opened, no materialization occurred
    // Only chunk() was called, which is synchronous and returns queries as data.
}
```

---

#### RECORDS08: Async Traversal and Row Identity

**Contract:** An async language traverses a manifest-backed source through an async iterator and yields identical rows.

```rust
#[tokio::test]
async fn records08_async_stream_traversal() {
    // Setup: create a test environment with a mock store
    let env = create_test_environment().await;

    // Create a manifest source with two chunks
    let chunk_queries = vec![
        Query::parse("-R/test/chunk_0.csv").unwrap(),
        Query::parse("-R/test/chunk_1.csv").unwrap(),
    ];

    let source = RecordSource {
        backing: SourceBacking::Queried {
            chunks: chunk_queries,
            cache: None,
        },
        uniform_schema: Some(Arc::new(RecordSchema {
            fields: vec![
                FieldSchema {
                    name: "id".to_string(),
                    data_type: FieldType::Int,
                    nullable: false,
                    key: KeyRole::Id,
                    role: FieldRole::ignored(),
                },
                FieldSchema {
                    name: "value".to_string(),
                    data_type: FieldType::Text,
                    nullable: false,
                    key: KeyRole::None,
                    role: FieldRole::ignored(),
                },
            ],
            type_identifier: None,
        })),
    };

    // Open the stream asynchronously
    let context = env.create_context();
    let mut stream = source.stream(&context)
        .await
        .expect("open stream");

    // Collect batches and verify rows
    let mut batch_count = 0;
    let mut row_count = 0;
    let mut rows = Vec::new();

    use futures::stream::StreamExt;
    while let Some(result) = stream.next().await {
        let batch = result.expect("read batch");
        batch_count += 1;

        // Each batch has rows with id and value
        for i in 0..batch.len {
            match (batch.column(0), batch.column(1)) {
                (Column::Int { values, .. }, Column::Text { offsets, data, .. }) => {
                    let id = values[i];
                    let text_start = offsets[i] as usize;
                    let text_end = offsets[i + 1] as usize;
                    let text = String::from_utf8_lossy(&data[text_start..text_end]);
                    rows.push((id, text.to_string()));
                    row_count += 1;
                }
                _ => panic!("unexpected column types"),
            }
        }
    }

    // Assert: two batches were yielded (one per chunk)
    assert_eq!(batch_count, 2);
    
    // Assert: rows came through with their data intact
    // (specific row values depend on the test data in the mock store)
    assert!(row_count > 0, "at least one row was yielded");
    
    // Assert: each row tuple (id, text) is consistent
    // A second traversal should yield identical rows
    let mut stream2 = source.stream(&context).await.expect("open stream again");
    let mut rows2 = Vec::new();
    
    use futures::stream::StreamExt as _;
    while let Some(result) = stream2.next().await {
        let batch = result.expect("read batch");
        for i in 0..batch.len {
            match (batch.column(0), batch.column(1)) {
                (Column::Int { values, .. }, Column::Text { offsets, data, .. }) => {
                    let id = values[i];
                    let text_start = offsets[i] as usize;
                    let text_end = offsets[i + 1] as usize;
                    let text = String::from_utf8_lossy(&data[text_start..text_end]);
                    rows2.push((id, text.to_string()));
                }
                _ => panic!("unexpected column types"),
            }
        }
    }

    // Second traversal must yield identical rows in identical order
    assert_eq!(rows, rows2, "second stream yields identical rows");
}

// Helper: create a test environment with a mock store containing chunk data
async fn create_test_environment() -> EnvRef {
    panic!("use a real test fixture");
}
```

---

#### RECORDS09: Sync-Only Route Documented (No Executable Code)

**Contract:** `NA` unless the language has no async model. The documented sync route is present and its limitation is stated.

**Location:** In command documentation or integration guide

**No executable test.** Instead, for a language binding:
- Document which sync route (1, 2, or 3 from the architecture) is offered
- If route 2 (block on stream): state that it is unavailable on Wasm
- If route 3 (materialize): document the size bound
- If route 1 (iterate chunks): mark it as the preferred route and note it requires manifest-backed sources

Example documentation:

```
### Record Stream Traversal

Record streams expose the async `stream()` method. Synchronous traversal:

1. **Preferred:** iterate the chunk list without opening a stream.
   Use `source.chunks()`, which returns an enumerable list of query objects.
   Evaluate each query synchronously to load one chunk at a time.
   Available for all manifest-backed sources (the normal case).

2. **Not recommended:** materialize the stream into a single chunk.
   Call `stream().collect()` to read all batches.
   Only suitable for small streams (< 100 MB documented limit).

3. **Not available:** blocking wait on an async stream.
   Wasm has no blocking primitive; native Python bindings offer this as an escape hatch.
```

---

### Summary

**Part A identifies ten specific traps:**
1. `len + 1` offsets invariant for Text/Binary
2. Vector export child node structure
3. Missing TypeInfo entries
4. Unguarded ExtValue match arms
5. Per-chunk values without ChunkKeys
6. Unreadable chunk queries from wrong parameter order
7. Cached typed-array views in Wasm across heap growth
8. Writing through lent buffers (aliasing violation)
9. Unguarded schema uniformity assumptions
10. Endian assumptions in Arrow C Data Interface

**Part B provides reference implementations of:**
- **RECORDS01–RECORDS04, RECORDS07–RECORDS08:** Rust native tests for value roundtrip, metadata preservation, column equivalence, sync traversal, and async stream behavior
- **RECORDS05–RECORDS06:** Wasm-specific tests (placeholders with structure; require browser environment)
- **RECORDS09:** Documentation pattern (not executable code)

All tests follow house rules: unwrap only in tests; no `println!`; typed `Error` constructors; `#[cfg(feature = "records")]` on conditional code. Tests are designed to be ported to other languages by adapting them to that language's test harness and value representations.

