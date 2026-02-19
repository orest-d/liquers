# Phase 3: Examples & Testing Template

## Purpose

Phase 3 demonstrates **USAGE** of the feature through realistic examples, explores corner cases, and plans comprehensive tests.

**Goals:**
- Create 2-3 realistic usage examples
- Identify corner cases (memory, concurrency, errors, serialization)
- Plan unit and integration tests
- Generate test templates via liquers-unittest skill

**Duration:** 1-2 hours (including liquers-unittest auto-invoke)

**Output:** Examples that validate the architecture, corner cases that ensure robustness, test plan that ensures quality.

## Workflow

Phase 3 uses **multi-agent drafting** and **multi-agent review**:

1. **Ask user:** Should examples be runnable prototypes or conceptual code?
   - Runnable: Create `examples/<feature>_demo.rs` with working code
   - Conceptual: Provide code snippets showing intended usage
2. **Multi-Agent Drafting** (up to 5 haiku agents in parallel):
   - Agent 1: Example scenario 1 (primary use case)
   - Agent 2: Example scenario 2 (secondary/advanced use case)
   - Agent 3: Example scenario 3 (edge case scenario) — optional
   - Agent 4: Unit tests (happy path, error path, edge cases)
   - Agent 5: Integration tests + corner cases (memory, concurrency, serialization, cross-crate)
   - All agents receive: Phase 1, Phase 2 documents, relevant source files
   - All agents have skills: rust-best-practices, liquers-unittest
3. **Sonnet Synthesizer** integrates all drafting outputs:
   - Creates the Phase 3 document starting with an **overview table**
   - Ensures consistent style, no contradictions between drafts
   - Fills gaps where individual drafts don't cover shared concerns
4. **Auto-invoke liquers-unittest** skill to generate test templates
5. **User feedback loop:** Iterate on examples until satisfied
6. **Critical review** using Phase 3 checklist
7. **Multi-Agent Review** (3 haiku reviewers + 1 sonnet fixer)
8. **Request approval**

## Auto-Invoke: liquers-unittest Skill

Before finalizing Phase 3, this skill **automatically invokes** the liquers-unittest skill to:
- Generate unit test templates for new functions
- Generate integration test templates for end-to-end flows
- Ensure test coverage follows liquers conventions
- Validate test structure (file placement, naming, assertions)

**You do not need to manually invoke this skill.**

## Template

Use this template for your `phase3-examples.md`:

```markdown
# Phase 3: Examples & Use-cases - <Feature Name>

## Example Type

**User choice:** [Runnable prototypes / Conceptual code]

(Ask user before generating examples!)

## Overview Table

| # | Type | Name | Purpose | Drafted By |
|---|------|------|---------|------------|
| 1 | Example | <Scenario 1> | Demonstrates primary use case | Haiku Agent 1 |
| 2 | Example | <Scenario 2> | Demonstrates secondary/advanced use case | Haiku Agent 2 |
| 3 | Example | <Scenario 3> | Demonstrates edge case (optional) | Haiku Agent 3 |
| 4 | Unit Tests | <Test suite name> | Happy path, error path, edge cases | Haiku Agent 4 |
| 5 | Integration Tests | <Test suite name> | End-to-end flows, corner cases | Haiku Agent 5 |

## Example 1: <Scenario Name>

**Scenario:** <Brief description of what this example demonstrates>

**Context:** <When would a user encounter this scenario?>

**Code:**
```rust
// If runnable: Full example with imports, setup, execution, cleanup
// If conceptual: Code snippet showing key usage

use liquers_lib::...;

fn example_scenario() -> Result<(), Error> {
    // Example code here
    Ok(())
}
```

**Expected output:**
```
<What the user should see when running this example>
```

**Validation:**
- [x] Compiles (if runnable)
- [x] Demonstrates core functionality
- [x] Uses realistic data/parameters
- [x] Shows expected output

## Example 2: <Another Scenario Name>

... (repeat for 2-3 examples)

## Example 3 (Optional): <Edge Case Scenario>

... (if needed to demonstrate unusual but valid usage)

## Corner Cases

### 1. Memory

**Large inputs:**
- Scenario: DataFrame with 1 billion rows, Parquet file > 10 GB
- Expected behavior: Streaming processing, bounded memory usage
- Test: Create large DataFrame, serialize to Parquet, verify memory < 1 GB
- Failure mode: OOM error if buffering entire DataFrame

**Allocation failures:**
- Scenario: System out of memory during Parquet write
- Expected behavior: Return `Error::general_error("Out of memory")`
- Test: (Difficult to test; document expected behavior)

**Memory leaks:**
- Scenario: Repeated serialization/deserialization cycles
- Expected behavior: No memory growth over time
- Test: Run 10,000 cycles, verify memory is stable

**Mitigation:**
- Use streaming APIs where possible
- Avoid cloning large data structures
- Release resources promptly (drop large buffers)

### 2. Concurrency

**Race conditions:**
- Scenario: Multiple threads reading/writing Parquet files concurrently
- Expected behavior: Each thread operates independently, no data corruption
- Test: Spawn 10 threads, each writing different Parquet files, verify all succeed
- Failure mode: File corruption if not thread-safe

**Deadlocks:**
- Scenario: (If using locks) Multiple locks acquired in different orders
- Expected behavior: No deadlocks (document lock ordering)
- Test: (If applicable) Stress test with concurrent operations
- Mitigation: Single lock per operation, or lock-free design

**Thread safety:**
- Scenario: Sharing Parquet writer across threads
- Expected behavior: Compiler prevents unsafe sharing (`!Send` type)
- Test: Verify compilation fails if trying to share non-Send types

**Async compatibility:**
- Scenario: Calling Parquet functions from async context
- Expected behavior: Works seamlessly (no blocking operations)
- Test: Call from tokio runtime, verify no blocking warnings

### 3. Errors

**Invalid input:**
- Scenario: Corrupted Parquet file
- Expected behavior: `Error::general_error("Invalid Parquet file")`
- Test: Provide truncated/malformed bytes, verify error
- Failure mode: Panic if not handled

**Network failures (if applicable):**
- Scenario: S3 store times out during Parquet read
- Expected behavior: `Error::io_error("Network timeout")`
- Test: Mock network failure, verify error propagation
- Failure mode: Hang if not handling timeouts

**Serialization errors:**
- Scenario: DataFrame with unsupported type (e.g., nested structs)
- Expected behavior: `Error::general_error("Unsupported type: nested struct")`
- Test: Create DataFrame with unsupported type, verify error
- Failure mode: Panic if `unwrap()` used

**Partial failures:**
- Scenario: Parquet file with some row groups corrupted
- Expected behavior: Return error (or partial data with warning)
- Test: Create Parquet with corrupted row group, verify behavior
- Mitigation: Document whether partial data is returned or full rejection

### 4. Serialization

**Round-trip compatibility:**
- Scenario: DataFrame → Parquet → DataFrame should preserve data
- Expected behavior: Identical data after round-trip
- Test: Serialize then deserialize, compare with original (all dtypes)
- Failure mode: Data loss or type coercion

**Schema evolution:**
- Scenario: Parquet file with older schema version
- Expected behavior: (Design decision) Reject or adapt schema
- Test: Create Parquet with old schema, verify behavior
- Mitigation: Document supported schema versions

**Compression:**
- Scenario: Parquet file with different compression (Snappy, Gzip, LZ4)
- Expected behavior: Transparent decompression
- Test: Create Parquet with each compression type, verify deserialization
- Failure mode: Error if compression not supported

**Metadata preservation:**
- Scenario: Parquet file with custom metadata
- Expected behavior: (Design decision) Preserve or discard metadata
- Test: Write Parquet with metadata, read back, verify presence
- Mitigation: Document metadata handling

### 5. Integration (Cross-Crate Interactions)

**With Store system:**
- Scenario: Reading Parquet from AsyncStore (file system, S3)
- Expected behavior: Seamless integration, no special handling
- Test: Read Parquet from MemoryStore, verify deserialization
- Failure mode: Encoding/path resolution issues

**With Command system:**
- Scenario: Chaining commands: `df~filter~to_parquet`
- Expected behavior: Pipeline works end-to-end
- Test: Execute multi-command query, verify Parquet output
- Failure mode: Type mismatch if command signatures incompatible

**With Asset system:**
- Scenario: Parquet asset caching
- Expected behavior: Asset cached after first evaluation
- Test: Execute query twice, verify second is cached
- Failure mode: Re-computation if caching broken

**With Web/API:**
- Scenario: Download Parquet via HTTP endpoint
- Expected behavior: Correct Content-Type, binary download
- Test: GET /api/query/...~to_parquet, verify headers and binary response
- Failure mode: Incorrect Content-Type or corrupted download

## Test Plan

### Unit Tests

**File:** `liquers-lib/src/polars/parquet.rs` (inline tests)

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_dataframe_to_parquet_bytes() {
        // Create small DataFrame
        // Serialize to Parquet
        // Verify bytes are non-empty
    }

    #[test]
    fn test_parquet_bytes_to_dataframe() {
        // Create Parquet bytes (fixture)
        // Deserialize to DataFrame
        // Verify schema and data
    }

    #[test]
    fn test_round_trip() {
        // Create DataFrame
        // Serialize to Parquet
        // Deserialize back
        // Assert equal to original
    }

    #[test]
    fn test_invalid_parquet() {
        // Provide corrupted bytes
        // Attempt to deserialize
        // Assert error returned
    }

    #[test]
    fn test_compression_types() {
        // Test each compression type (Snappy, Gzip, LZ4)
        // Verify all deserialize correctly
    }
}
```

**Coverage:**
- ✅ Happy path (valid input → correct output)
- ✅ Round-trip (data preservation)
- ✅ Error path (invalid input → error)
- ✅ Edge cases (compression types, large data)

### Integration Tests

**File:** `liquers-lib/tests/parquet_integration.rs`

```rust
#[tokio::test]
async fn test_parquet_query_execution() {
    // Setup Environment with Parquet commands
    // Execute query: /-/data.parquet~to_parquet
    // Verify Result<Value, Error> contains Parquet bytes
}

#[tokio::test]
async fn test_parquet_store_integration() {
    // Create MemoryStore with Parquet file
    // Execute query to read Parquet
    // Verify DataFrame output
}

#[tokio::test]
async fn test_parquet_pipeline() {
    // Execute multi-command query: df~filter~to_parquet
    // Verify Parquet output matches filtered data
}
```

**Coverage:**
- ✅ End-to-end query execution
- ✅ Store integration (AsyncStore)
- ✅ Command chaining (pipeline)
- ✅ Asset caching (second execution faster)

### Manual Validation

**Commands to run:**

1. **Create test Parquet file:**
   ```bash
   # (If examples are runnable)
   cargo run --example parquet_demo
   # Verify: test.parquet file created
   ```

2. **Test via HTTP API:**
   ```bash
   cargo run -p liquers-axum
   curl http://localhost:3000/api/query/-/data.parquet > output.parquet
   # Verify: output.parquet is valid (open in DuckDB/Pandas)
   ```

3. **Test round-trip:**
   ```bash
   # Create DataFrame, convert to Parquet, convert back
   # Compare original and round-tripped data
   # Verify: No data loss or type changes
   ```

**Success criteria:**
- All commands execute without errors
- Output files are valid Parquet (verifiable with external tools)
- Data integrity preserved across conversions

## Auto-Invoke: liquers-unittest Skill Output

**Expected output from liquers-unittest skill:**

1. Unit test templates for `to_parquet`, `from_parquet` functions
2. Integration test templates for query execution
3. Fixture data (sample Parquet bytes for testing)
4. Test coverage report (which paths are tested, which are not)

**Integration with this Phase:**
- Use generated test templates as starting point
- Customize tests to cover corner cases identified above
- Ensure all error paths are tested (not just happy paths)

## User Feedback Loop

**After generating examples, ask user:**

1. **Are the examples realistic?**
   - If no: Adjust scenarios to better match real-world usage
2. **Are there additional scenarios to cover?**
   - If yes: Add Example 4, Example 5, etc.
3. **Should examples be runnable or conceptual?**
   - If runnable: Create `examples/<feature>_demo.rs` with full code
   - If conceptual: Keep as code snippets in this document
4. **Are corner cases comprehensive?**
   - If no: Add missing categories (usability, security, performance)

**Iterate until user is satisfied, then request approval.**

## Review Checklist

Before requesting approval, validate Phase 3:

### Examples
- [ ] 2-3 realistic scenarios provided
- [ ] User chose runnable vs. conceptual (if applicable)
- [ ] Examples demonstrate core functionality
- [ ] Examples use realistic data/parameters
- [ ] Expected outputs are documented

### Corner Cases
- [ ] Memory: Large inputs, allocation failures, leaks
- [ ] Concurrency: Race conditions, deadlocks, thread safety
- [ ] Errors: Invalid input, network failures, serialization errors
- [ ] Serialization: Round-trip, schema evolution, compression
- [ ] Integration: Store, Command, Asset, Web/API interactions

### Test Coverage
- [ ] Unit tests cover happy path + error path
- [ ] Integration tests cover end-to-end flows
- [ ] Manual validation commands provided
- [ ] liquers-unittest skill invoked for test templates

### Overview Table
- [ ] Overview table present at top of document
- [ ] All examples and tests listed with purpose

### Query Validation
- [ ] No spaces, newlines, or special characters in queries
- [ ] Resource part (`-R/`) queries have matching store definitions
- [ ] All commands in queries are registered (checked against Phase 2 Relevant Commands)
- [ ] Namespace references are valid

### Multi-Agent Review
- [ ] Reviewer 1 (Phase 1 conformity) launched and completed
- [ ] Reviewer 2 (Phase 2 conformity) launched and completed
- [ ] Reviewer 3 (Codebase + query validation) launched and completed
- [ ] Sonnet fixer launched (if issues found) and completed
- [ ] All fixable issues resolved
- [ ] Remaining questions (if any) presented to user

### Approval Criteria
- [ ] Coverage is complete (no major gaps)
- [ ] Examples are realistic (not toy scenarios)
- [ ] Overview table present and accurate
- [ ] All queries validated
- [ ] Multi-agent review completed with no open issues
- [ ] User is satisfied with test plan

**If any checklist items fail, revise Phase 3 before requesting approval.**

```

## Example: Parquet File Support Examples

Here's a real example following the template:

```markdown
# Phase 3: Examples & Use-cases - Parquet File Support

## Example Type

**User choice:** Conceptual code (no runnable examples needed for initial release)

## Example 1: Save DataFrame as Parquet

**Scenario:** User has a Polars DataFrame and wants to save it as a Parquet file for efficient storage.

**Context:** Data analysis workflow where results need to be persisted.

**Code:**
```rust
use liquers_core::query::parse_query;
use liquers_lib::SimpleEnvironment;

async fn example_save_parquet() -> Result<(), Error> {
    let env = SimpleEnvironment::new().await;

    // Assume we have a DataFrame from previous commands
    let query = parse_query("/-/data.csv~to_parquet")?;
    let result = env.evaluate(&query).await?;

    // Result is Value::Bytes containing Parquet binary
    // Can be written to file or served over HTTP

    Ok(())
}
```

**Expected output:**
```
Result: Value::Bytes(<parquet binary data>)
Content-Type: application/vnd.apache.parquet
```

**Validation:**
- [x] Demonstrates core functionality (DataFrame → Parquet)
- [x] Uses realistic query syntax
- [x] Shows expected output type

## Example 2: Load Parquet as DataFrame

**Scenario:** User has a Parquet file in storage and wants to load it as a DataFrame for analysis.

**Context:** Reading data from data lake or external system.

**Code:**
```rust
async fn example_load_parquet() -> Result<(), Error> {
    let env = SimpleEnvironment::new().await;

    // Query a Parquet file from store
    let query = parse_query("/-/data/sales.parquet")?;
    let result = env.evaluate(&query).await?;

    // Result is ExtValue::DataFrame
    // Can be further processed with other commands

    Ok(())
}
```

**Expected output:**
```
Result: ExtValue::DataFrame {
    shape: (1000, 10),
    columns: ["id", "name", "amount", ...],
    ...
}
```

## Example 3: Round-trip Conversion

**Scenario:** Verify data integrity by converting DataFrame → Parquet → DataFrame.

**Context:** Testing data preservation, ensuring no type coercion or data loss.

**Code:**
```rust
async fn example_round_trip() -> Result<(), Error> {
    let env = SimpleEnvironment::new().await;

    // Original DataFrame
    let query1 = parse_query("/-/data.csv")?;
    let df1 = env.evaluate(&query1).await?;

    // Convert to Parquet and back
    let query2 = parse_query("/-/data.csv~to_parquet~from_parquet")?;
    let df2 = env.evaluate(&query2).await?;

    // df1 should equal df2
    assert_eq!(df1, df2);

    Ok(())
}
```

**Expected output:**
```
Assertion passes: DataFrames are identical
```

## Corner Cases

(Full corner cases documented as per template...)

## Test Plan

(Full test plan with unit tests, integration tests, manual validation...)

```

## Prototype Decision

**Ask user before generating examples:**

> "Should I create **runnable prototypes** (full examples in `examples/<feature>_demo.rs`) or **conceptual code** (snippets in the Phase 3 document)?"
>
> - **Runnable prototypes:** Takes longer, but provides executable examples users can run
> - **Conceptual code:** Faster, sufficient for documentation and architecture validation
>
> **Recommendation:** Conceptual code for initial design, runnable prototypes if user requests or feature is complex.

## Multi-Agent Drafting

### Agent Setup

Launch up to 5 haiku agents in parallel. Each agent receives:
- **Skills:** rust-best-practices, liquers-unittest
- **Knowledge:** Phase 1 document, Phase 2 document (especially Relevant Commands section), relevant source files from codebase
- **User's example type preference** (runnable vs. conceptual)

### Agent Assignments

**Agent 1 — Example Scenario 1 (Primary Use Case):**
- Draft the most common, representative usage scenario
- Include full code (runnable or conceptual per user choice)
- Document expected output and validation criteria

**Agent 2 — Example Scenario 2 (Secondary/Advanced):**
- Draft a secondary or advanced usage scenario
- Show more complex interactions (command chaining, multiple features)
- Include expected output

**Agent 3 — Example Scenario 3 (Edge Case) — Optional:**
- Only launch if the feature has notable edge cases worth demonstrating
- Show unusual but valid usage patterns
- Include expected output

**Agent 4 — Unit Tests:**
- Draft unit tests covering happy path, error path, and edge cases
- Follow liquers test conventions (inline `#[cfg(test)]` modules)
- Include test function names and assertions

**Agent 5 — Integration Tests + Corner Cases:**
- Draft integration tests for end-to-end flows
- Document corner cases: memory, concurrency, serialization, cross-crate interactions
- Include `#[tokio::test]` async tests where appropriate

### Synthesis (Sonnet Agent)

After all drafting agents complete, launch **1 sonnet synthesizer agent** with:
- **Skills:** rust-best-practices, liquers-unittest
- **Knowledge:** All 5 agent outputs, Phase 1, Phase 2 documents
- **Task:**
  1. Create the **overview table** at the top of the Phase 3 document
  2. Integrate all agent outputs into a coherent document
  3. Resolve contradictions or inconsistencies between drafts
  4. Ensure consistent coding style and naming
  5. Fill gaps where individual drafts don't cover shared concerns (e.g., serialization corner cases that affect both examples and tests)

## Query Validation Rules

All queries appearing in examples and tests must be validated:

### Syntax Rules
- **No spaces:** Queries must not contain spaces (use `-` for parameter separation)
- **No newlines:** Queries must be single-line strings
- **No special characters:** Only alphanumeric, `-`, `~`, `/`, `.`, `_` allowed in query components

### Resource Part Validation
- If a query uses the resource part (`-R/path/to/resource`), verify that the test/example environment has a store defined that serves that path
- Memory stores (`MemoryStore`) must be pre-populated with the referenced resources

### Command Registration Validation
- Every command used in a query must be a known command:
  - Check against the **Relevant Commands** list from Phase 2 (new commands)
  - Check against existing registered commands in the relevant namespaces (from Phase 2)
  - If a command is not found in either list, flag it as potentially unregistered

### Namespace Validation
- If a query uses namespace syntax (`ns-<namespace>/command`), verify the namespace exists
- Cross-reference with Phase 2 Relevant Commands section

## Multi-Agent Review

After completing the Phase 3 document, launch a **multi-agent review** before requesting user approval.

### Reviewer Agents (3 haiku, launched in parallel)

**Reviewer 1 — Phase 1 Conformity (haiku):**
- Skills: (none required)
- Knowledge: Phase 1 document, Phase 3 document
- Task: Check that examples/tests align with Phase 1 high-level design:
  - Examples demonstrate the feature purpose from Phase 1
  - No examples of functionality outside Phase 1 scope
  - All interactions identified in Phase 1 are covered by at least one example or test

**Reviewer 2 — Phase 2 Conformity (haiku):**
- Skills: rust-best-practices
- Knowledge: Phase 2 document, Phase 3 document
- Task: Check that examples/tests match Phase 2 architecture:
  - Function signatures in examples match Phase 2 signatures
  - Data structures used correctly (field names, types, ownership)
  - Trait usage matches Phase 2 trait implementations
  - Error handling follows Phase 2 error strategy

**Reviewer 3 — Codebase + Query Validation (haiku):**
- Skills: rust-best-practices
- Knowledge: Phase 2 document (Relevant Commands section), Phase 3 document, relevant source files
- Task: Check alignment with existing code and validate all queries:
  - Apply all Query Validation Rules (see above)
  - Verify imports and types exist in the codebase
  - Check that test patterns match liquers conventions
  - Flag any queries with unregistered commands

### Fixer Agent (1 sonnet, launched only if issues found)

**Sonnet Fixer:**
- Skills: rust-best-practices, liquers-unittest
- Knowledge: PROJECT_OVERVIEW.md, Phase 1, Phase 2, Phase 3 documents, all reviewer outputs
- Task: Process all review findings and:
  1. Fix all fixable issues directly in the Phase 3 document
  2. Produce a summary with:
     - List of fixes made (what was changed and why)
     - List of potential problems that need user attention
     - List of remaining questions (genuine design decisions only)
  3. Ask user ONLY for decisions that can't be resolved from available context

### After Multi-Agent Review

- If no issues found: proceed directly to user approval gate
- If issues found and fixed: present fixed document + summary to user
- If unresolvable questions remain: ask user before requesting approval

## Next Steps

**STOP HERE.** Present Phase 3 to the user and WAIT for explicit approval.

The user must say "proceed" or "Proceed to next phase" before you start Phase 4. Any other response (feedback, questions, corrections, "looks good", "ok") is NOT approval — address the feedback and WAIT again.

After user says "proceed":
1. Start Phase 4: Implementation Plan
2. Use examples as validation criteria during implementation
3. Use test plan as quality gate before feature completion
