# Phase 1: High-Level Design Template

## Purpose

Phase 1 establishes **WHAT** the feature is and **WHY** it exists. This is the foundation for all subsequent phases.

**Goal:** Create a concise (maximum 30 lines) high-level design that answers:
- What is the feature name?
- What is its purpose (1-3 sentences)?
- How does it interact with existing Liquers systems?
- What open questions remain?

**Duration:** 15-30 minutes

**Output:** A clear, concise document that stakeholders can review without deep technical knowledge.

## Template

Use this template for your `phase1-high-level-design.md`:

```markdown
# Phase 1: High-Level Design - <Feature Name>

## Feature Name

<Short, descriptive name>

## Purpose

<1-3 sentences explaining why this feature exists and what problem it solves>

## Core Interactions

### Query System
<How does this feature integrate with Query parsing, execution, or Key encoding?>
<Example: "Adds new action syntax: `~export_parquet/filename`">

### Store System
<Does this feature read from or write to stores? Which store types?>
<Example: "Reads Parquet files via AsyncStore trait">

### Command System
<What new commands does this feature introduce? What namespace?>
<Example: "Adds `polars` namespace with `to_parquet`, `from_parquet` commands">

### Asset System
<Does this feature create, consume, or transform assets?>
<Example: "Consumes DataFrame assets, produces Parquet file assets">

### Value Types
<Does this feature introduce new ExtValue variants or ValueExtension types?>
<Example: "No new value types; uses existing DataFrame variant">

### Web/API (if applicable)
<Does this feature add HTTP endpoints or modify the Axum server?>
<Example: "N/A - file format only">

### UI (if applicable)
<Does this feature add UI elements, widgets, or modify the UI framework?>
<Example: "Adds ParquetViewer widget for displaying Parquet metadata">

## Crate Placement

<Which crate(s) will contain this feature? liquers-core, liquers-lib, liquers-store, liquers-axum?>
<Rationale for placement based on dependency flow>

## Open Questions

1. <Question 1>
2. <Question 2>
3. ...

<Note: It's GOOD to have open questions at this stage. They'll be resolved in Phase 2.>
<BAD: Blocking unknowns that prevent design. These must be researched before approval.>

## References

- <Link to related specs, issues, or external documentation>
- <Link to similar features in other systems for inspiration>
```

## Example: Parquet File Support

Here's a real example following the template:

```markdown
# Phase 1: High-Level Design - Parquet File Support

## Feature Name

Parquet File Format Support

## Purpose

Enable reading and writing Parquet files in Liquers to support efficient columnar data storage and interoperability with data engineering tools (Spark, DuckDB, etc.). This allows users to persist and load Polars DataFrames in a widely-supported binary format.

## Core Interactions

### Query System
Adds file extension routing: `.parquet` files trigger Parquet deserialization.
Example query: `/-/data/sales.parquet`

### Store System
Reads Parquet files via AsyncStore trait (file system, S3, etc.).
Writes Parquet via Store's write methods.
No new store implementation needed; uses existing AsyncStore backends.

### Command System
Adds `polars` namespace commands:
- `to_parquet` - Convert DataFrame to Parquet bytes
- `from_parquet` - Parse Parquet bytes into DataFrame

### Asset System
Consumes DataFrame assets (from previous commands).
Produces Parquet binary assets (Vec<u8>).

### Value Types
No new ExtValue variants needed.
Uses existing `ExtValue::DataFrame` for in-memory representation.
Parquet bytes stored as `Value::Bytes`.

### Web/API
File download endpoint: GET `/api/query/-/df~to_parquet` returns Parquet file.
Content-Type: `application/vnd.apache.parquet`

### UI
Not applicable for Phase 1. Future: ParquetViewer widget could display schema/stats.

## Crate Placement

**liquers-lib** - Primary implementation
- Rationale: Parquet is a rich value type extension, similar to Polars DataFrames
- Dependencies: `arrow`, `parquet` crates (already used by Polars)

**liquers-axum** - HTTP response handling
- Add Content-Type mapping for `.parquet` extension

No changes to liquers-core or liquers-store.

## Open Questions

1. Should we support Parquet compression (Snappy, Gzip, LZ4)? Default?
2. Should Parquet schema be inferred from DataFrame or user-specified?
3. How to handle Parquet files with multiple row groups?

(These will be resolved in Phase 2 architecture)

## References

- Polars Parquet docs: https://pola-rs.github.io/polars/py-polars/html/reference/io.html#parquet
- Arrow Parquet Rust crate: https://docs.rs/parquet/latest/parquet/
- Parquet format spec: https://parquet.apache.org/docs/file-format/
```

## Review Checkpoint

Before requesting user approval, validate your Phase 1 design against these critical questions:

### Scope Clarity
- [ ] **Can you explain the feature in 1-3 sentences to a non-technical stakeholder?**
  - If no: Simplify the purpose statement
- [ ] **Are the system interactions clearly identified?**
  - If no: Review the template sections, fill in missing interactions
- [ ] **Is the feature appropriately scoped (not too large, not trivial)?**
  - If too large: Consider breaking into multiple features
  - If too small: This might not need the full 4-phase process

### No Duplication
- [ ] **Does this feature overlap with existing functionality?**
  - If yes: Justify why a new feature is needed vs. extending existing code
- [ ] **Have you checked for similar implementations in other crates?**
  - Search codebase: `liquers-lib/src/`, `liquers-core/src/`

### Aligns with Liquers Philosophy
- [ ] **Does this feature fit the query-based, layered architecture?**
  - Queries → Commands → State → Assets
- [ ] **Does it respect the crate dependency flow?**
  - liquers-core ← liquers-macro ← liquers-store ← liquers-lib ← liquers-axum
- [ ] **Is async the default (with sync wrappers if needed)?**

### Questions Identified
- [ ] **Are all open questions documented?**
- [ ] **Are any open questions blocking (cannot proceed without answers)?**
  - If yes: Research or ask user for clarification before approval
- [ ] **Are open questions realistic to resolve in Phase 2?**

### Readability
- [ ] **Is the document under 30 lines (excluding template structure)?**
  - If no: This is too detailed for Phase 1; move details to Phase 2
- [ ] **Can someone unfamiliar with the feature understand it from this document?**

## Common Pitfalls

### Pitfall 1: Too Much Detail
**Bad:**
```markdown
## Core Interactions

### Command System
Adds `to_parquet` command with signature:
fn to_parquet(state: &State<Value>, compression: String) -> Result<Value, Error>
Uses parquet::file::writer::SerializedFileWriter...
```

**Good:**
```markdown
## Core Interactions

### Command System
Adds `polars` namespace commands: `to_parquet`, `from_parquet`.
(Signatures and implementation details in Phase 2)
```

**Reason:** Phase 1 is WHAT, not HOW. Save signatures for Phase 2.

### Pitfall 2: Missing Interactions
**Bad:**
```markdown
## Core Interactions

### Command System
Adds parquet commands.
```

**Good:**
```markdown
## Core Interactions

### Query System
File extension routing: `.parquet` files trigger Parquet deserialization.

### Store System
Reads Parquet files via AsyncStore trait (file system, S3, etc.).

### Command System
Adds `polars` namespace commands: `to_parquet`, `from_parquet`.

### Value Types
Uses existing ExtValue::DataFrame for in-memory representation.
```

**Reason:** Completeness prevents surprises in later phases.

### Pitfall 3: No Open Questions
**Bad:**
```markdown
## Open Questions

None. Design is complete.
```

**Good:**
```markdown
## Open Questions

1. Should we support Parquet compression (Snappy, Gzip, LZ4)? Default?
2. Should Parquet schema be inferred from DataFrame or user-specified?
```

**Reason:** Having zero open questions at this stage is suspicious. Acknowledge unknowns.

### Pitfall 4: Blocking Unknowns
**Bad:**
```markdown
## Open Questions

1. Do we even have Polars DataFrames in Liquers?
2. Can AsyncStore read arbitrary files?
3. What is the Command trait signature?
```

**Good:**
```markdown
## Open Questions

1. Should we support Parquet compression (Snappy, Gzip, LZ4)? Default?
   → Research in Phase 2 by checking parquet crate docs
2. Should Parquet schema be inferred from DataFrame or user-specified?
   → Decide in Phase 2 based on common use cases
```

**Reason:** Blocking unknowns should be researched NOW. Open questions are design choices.

## Next Steps

**STOP HERE.** Present Phase 1 to the user and WAIT for explicit approval.

The user must say "proceed" or "Proceed to next phase" before you start Phase 2. Any other response (feedback, questions, "looks good", "ok") is NOT approval — address the feedback and WAIT again.

After user says "proceed":
1. Start Phase 2: Solution & Architecture
2. Use this Phase 1 document as the north star — all Phase 2 decisions must align with this vision
