# Phase 4: Implementation Plan Template

## Purpose

Phase 4 creates a **step-by-step, actionable execution plan** with validation commands. This is the blueprint for implementing the feature.

**Goals:**
- Break down implementation into numbered, file-specific steps
- Provide validation commands after each step
- Define testing plan (when to run unit tests, integration tests)
- Specify agent assignments per step (model, skills, knowledge)
- Create rollback plan for major changes

**Duration:** 1-2 hours (including rust-best-practices auto-invoke)

**Output:** An implementation plan detailed enough that a developer can execute it without making architectural decisions.

## Auto-Invoke: rust-best-practices Skill

Before finalizing Phase 4, this skill **automatically invokes** the rust-best-practices skill to validate:
- Implementation steps follow Rust idioms
- No anti-patterns introduced
- Code organization is idiomatic
- Testing strategy is comprehensive

**You do not need to manually invoke this skill.**

## Template

Use this template for your `phase4-implementation.md`:

```markdown
# Phase 4: Implementation Plan - <Feature Name>

## Overview

**Feature:** <Feature name from Phase 1>

**Architecture:** <1-2 sentence summary from Phase 2>

**Estimated complexity:** [Low / Medium / High]

**Estimated time:** <X hours for experienced Rust developer>

**Prerequisites:**
- Phase 1, 2, 3 approved
- All open questions resolved
- Dependencies identified (crates, versions)

## Implementation Steps

### Step 1: <Action Description>

**File:** `<exact-file-path>`

**Action:**
- <Specific change 1>
- <Specific change 2>
- ...

**Code changes:**
```rust
// NEW: Add this code
pub fn new_function() -> Result<(), Error> {
    // Function signature only; implementation follows
}

// MODIFY: Change existing code
// Before:
// fn old_signature(param: Type1) { ... }
// After:
fn old_signature(param: Type2) -> Result<(), Error> { ... }

// DELETE: Remove this code
// fn deprecated_function() { ... }
```

**Validation:**
```bash
cargo check -p <crate-name>
# Expected: Compiles with no errors (warnings OK at this stage)
```

**Rollback:**
```bash
git checkout <file-path>
# Or: Revert specific change if partially complete
```

**Agent Specification:**
- **Model:** [haiku / sonnet / opus]
- **Skills:** [rust-best-practices, liquers-unittest, etc.]
- **Knowledge:** [Which files, specs, patterns the agent needs to read]
- **Rationale:** <Why this model — e.g., "sonnet: requires architectural judgment" or "haiku: follows established pattern">

---

### Step 2: <Next Action>

... (repeat for each step)

---

### Step N: Final Integration

**File:** `<integration-point-file>`

**Action:**
- Wire together all components
- Register commands (if applicable)
- Export public API

**Validation:**
```bash
cargo build --all-features
cargo test -p <crate-name>
# Expected: All tests pass
```

## Testing Plan

### Unit Tests

**When to run:** After Step X (when functions are implemented)

**File:** `<test-file-path>`

**Command:**
```bash
cargo test -p <crate-name> --lib
```

**Expected:**
- All new unit tests pass
- Existing unit tests still pass (no regressions)

### Integration Tests

**When to run:** After Step Y (when end-to-end flow is complete)

**File:** `<integration-test-file-path>`

**Command:**
```bash
cargo test -p <crate-name> --test <test-name>
```

**Expected:**
- All integration tests pass
- Query execution works end-to-end

### Manual Validation

**When to run:** After all steps complete

**Commands:**
```bash
# Command 1: <Description>
<command>
# Expected output: <what user should see>

# Command 2: <Description>
<command>
# Expected output: <what user should see>
```

**Success criteria:**
- All manual commands execute without errors
- Output matches expectations from Phase 3 examples

## Task Splitting (Agent Assignments)

Each step must have an explicit agent specification. Use this summary table for quick reference:

| Step | Model | Skills | Rationale |
|------|-------|--------|-----------|
| 1 | sonnet | rust-best-practices | New module structure (architectural) |
| 2 | haiku | rust-best-practices | Add dependency (Cargo.toml change) |
| 3 | sonnet | rust-best-practices | Trait implementation (complex logic) |
| 4 | haiku | rust-best-practices | Derive macros (boilerplate) |
| 5 | sonnet | rust-best-practices | Error handling (requires judgment) |
| 6 | haiku | rust-best-practices | Registration code (follows pattern) |
| 7 | haiku | — | Content-Type mapping (simple match arm) |
| 8 | sonnet | rust-best-practices, liquers-unittest | Integration (cross-cutting change) |
| 9 | haiku | — | Documentation (straightforward writing) |

### Model Selection Guidelines

- **Opus:** Reserve for steps requiring deep architectural understanding across multiple crates, or steps that must reason about the entire design holistically. Rare in implementation.
- **Sonnet:** Steps requiring architectural judgment, complex error handling, trait implementations, integration logic, or reasoning about multiple interacting components.
- **Haiku:** Steps following established patterns, boilerplate code, simple additions (new match arm, dependency, module export), documentation.

## Rollback Plan

### Per-Step Rollback

**If Step X fails:**
1. Run rollback command (listed in step)
2. Review error messages
3. Revise approach (may require returning to Phase 2/3)
4. Re-attempt step

### Full Feature Rollback

**If implementation is abandoned:**
```bash
git checkout main
git branch -D feature/<feature-name>
# All changes discarded, codebase returns to pre-implementation state
```

**Files to delete:**
```
<list of new files created during implementation>
```

**Files to restore:**
```
<list of modified files - git checkout to restore>
```

**Cargo.toml changes:**
```toml
# Remove these dependencies:
<dependency-name> = "<version>"
```

### Partial Completion

**If partially complete but need to pause:**
1. Create feature branch: `git checkout -b feature/<feature-name>`
2. Commit work-in-progress: `git commit -m "WIP: <feature> - completed steps 1-5"`
3. Document completion status in `specs/<feature>/DESIGN.md`
4. Resume later by checking out branch and continuing from last completed step

## Documentation Updates

### CLAUDE.md

**Update if:** New patterns introduced (e.g., new command registration pattern, new value type pattern)

**Section to modify:** `## <relevant section>`

**Add:**
```markdown
### <Pattern Name>

<Description of pattern>
<Example usage>
```

### PROJECT_OVERVIEW.md

**Update if:** Core concepts changed (e.g., new layer in value hierarchy, new system component)

**Section to modify:** `## <relevant section>`

**Add:**
```markdown
### <New Concept>

<Explanation>
```

### README.md (if applicable)

**Update if:** New user-facing feature

**Section to modify:** `## Features`

**Add:**
```markdown
- **<Feature Name>:** <Description>
```

## Execution Options

After Phase 4 approval, choose one:

### Option 1: Execute Now

**Action:** Start implementing steps immediately

**Process:**
1. Create feature branch
2. Execute steps sequentially
3. Run validation after each step
4. Run tests per testing plan
5. Commit completed feature
6. Create pull request

**Estimated time:** <X hours>

### Option 2: Create Task List

**Action:** Generate implementation tasks for later execution

**Process:**
1. Use TaskCreate tool to create task for each step
2. Set dependencies (e.g., Step 2 blocks Step 3)
3. Assign owners (Sonnet vs. Haiku)
4. Exit, allowing user to trigger execution later

**Use when:** User wants to defer implementation

### Option 3: Revise Plan

**Action:** Return to Phase 4 for revisions

**Process:**
1. User provides feedback on plan
2. Revise steps, validation, or testing
3. Re-run critical review
4. Request approval again

**Use when:** User wants changes before execution

### Option 4: Exit

**Action:** Exit skill, user will implement manually

**Process:**
1. Phase 4 document saved to `specs/<feature>/phase4-implementation.md`
2. User can reference plan during manual implementation
3. No automated execution

**Use when:** User prefers manual control

## Critical Review Checklist

Before requesting approval, validate Phase 4 using **VERY HIGH certainty** criteria:

### Implementation Readiness
- [ ] All steps have exact file paths
- [ ] All steps have specific actions (not vague descriptions)
- [ ] All steps have validation commands
- [ ] Validation commands are realistic (will actually verify the change)
- [ ] Steps are ordered logically (dependencies respected)
- [ ] No circular dependencies between steps

### Testing Completeness
- [ ] Unit tests specified with file paths
- [ ] Integration tests specified with file paths
- [ ] Manual validation commands provided
- [ ] Success criteria are clear and objective
- [ ] Error paths are tested (not just happy paths)

### Documentation
- [ ] CLAUDE.md updates identified (if needed)
- [ ] PROJECT_OVERVIEW.md updates identified (if needed)
- [ ] README.md updates identified (if applicable)
- [ ] All new patterns documented

### Rollback Plan
- [ ] Per-step rollback commands provided
- [ ] Full feature rollback documented
- [ ] Partial completion strategy defined

### Agent Specifications
- [ ] Every step has an agent specification (model, skills, knowledge)
- [ ] Model selection justified for each step
- [ ] Skills listed match what the agent needs
- [ ] Knowledge/context files listed for each agent
- [ ] Summary table of agent assignments present

### Multi-Agent Review
- [ ] Reviewer 1 (Phase 1 conformity) launched and completed
- [ ] Reviewer 2 (Phase 2 conformity) launched and completed
- [ ] Reviewer 3 (Phase 3 conformity) launched and completed
- [ ] Reviewer 4 (Codebase compatibility) launched and completed
- [ ] Opus final reviewer launched and completed
- [ ] All fixable issues resolved
- [ ] Remaining questions (if any) presented to user

### Very High Certainty
- [ ] **Confidence level: 95%+** that this plan can be executed without architectural changes
- [ ] All open questions from Phase 1 resolved
- [ ] No "TBD" or "figure out later" items
- [ ] Clear path forward, no blocking unknowns
- [ ] Opus final reviewer confirmed readiness

**If confidence < 95%, DO NOT request approval.** Return to Phase 2/3 to resolve unknowns.

```

## Example: Parquet File Support Implementation Plan

Here's a real example following the template:

```markdown
# Phase 4: Implementation Plan - Parquet File Support

## Overview

**Feature:** Parquet File Format Support

**Architecture:** Two commands (`to_parquet`, `from_parquet`) in liquers-lib, using Polars' Parquet integration. No new data structures; functions only.

**Estimated complexity:** Low (leverages existing Polars functionality)

**Estimated time:** 2-3 hours for experienced Rust developer

**Prerequisites:**
- Phases 1, 2, 3 approved ✅
- Open questions resolved: Use Snappy compression (default), infer schema, read all row groups ✅
- Dependencies: Polars already includes Parquet support (no new deps) ✅

## Implementation Steps

### Step 1: Create Parquet Module

**File:** `liquers-lib/src/polars/parquet.rs` (new file)

**Action:**
- Create new module file
- Add module skeleton with imports

**Code changes:**
```rust
// NEW FILE: liquers-lib/src/polars/parquet.rs
use liquers_core::error::{Error, ErrorType};
use liquers_core::state::State;
use liquers_core::value::Value;
use polars::prelude::*;

// Function stubs (implementation in next steps)
pub fn to_parquet(state: &State<Value>) -> Result<Value, Error> {
    todo!()
}

pub fn from_parquet(state: &State<Value>) -> Result<Value, Error> {
    todo!()
}
```

**Validation:**
```bash
cargo check -p liquers-lib
# Expected: Warning about todo!() - this is OK
```

**Rollback:**
```bash
rm liquers-lib/src/polars/parquet.rs
```

**Agent Specification:**
- **Model:** sonnet
- **Skills:** rust-best-practices
- **Knowledge:** Phase 2 architecture doc, `liquers-lib/src/polars/mod.rs`
- **Rationale:** New module structure requires architectural judgment

---

### Step 2: Export Parquet Module

**File:** `liquers-lib/src/polars/mod.rs`

**Action:**
- Add `pub mod parquet;` to exports

**Code changes:**
```rust
// MODIFY: liquers-lib/src/polars/mod.rs
pub mod dataframe;
pub mod parquet;  // NEW LINE
```

**Validation:**
```bash
cargo check -p liquers-lib
# Expected: Compiles (with todo!() warnings)
```

**Rollback:**
```bash
# Remove the line: pub mod parquet;
git diff liquers-lib/src/polars/mod.rs  # Review change
git checkout liquers-lib/src/polars/mod.rs  # Revert if needed
```

**Agent Specification:**
- **Model:** haiku
- **Skills:** rust-best-practices
- **Knowledge:** `liquers-lib/src/polars/mod.rs`
- **Rationale:** One-line change following established pattern

---

### Step 3: Implement to_parquet Function

**File:** `liquers-lib/src/polars/parquet.rs`

**Action:**
- Implement `to_parquet` function
- Extract DataFrame from state
- Serialize to Parquet bytes using Polars ParquetWriter
- Handle errors

**Code changes:**
```rust
// MODIFY: Replace todo!() with implementation
pub fn to_parquet(state: &State<Value>) -> Result<Value, Error> {
    // Extract DataFrame from state
    let df = state.try_as_dataframe()
        .map_err(|e| Error::general_error(format!("Expected DataFrame: {}", e)))?;

    // Serialize to Parquet
    let mut buf = Vec::new();
    ParquetWriter::new(&mut buf)
        .finish(df)
        .map_err(|e| Error::from_error(ErrorType::General, e))?;

    Ok(Value::Bytes(buf))
}
```

**Validation:**
```bash
cargo check -p liquers-lib
# Expected: Compiles with no errors
```

**Rollback:**
```bash
git diff liquers-lib/src/polars/parquet.rs  # Review change
# Restore todo!() if implementation fails
```

**Agent Specification:**
- **Model:** sonnet
- **Skills:** rust-best-practices
- **Knowledge:** Phase 2 architecture doc, Polars API docs, `liquers-lib/src/polars/parquet.rs`
- **Rationale:** Complex serialization logic with error handling

---

### Step 4: Implement from_parquet Function

**File:** `liquers-lib/src/polars/parquet.rs`

**Action:**
- Implement `from_parquet` function
- Extract bytes from state
- Deserialize Parquet using Polars ParquetReader
- Handle errors

**Code changes:**
```rust
pub fn from_parquet(state: &State<Value>) -> Result<Value, Error> {
    // Extract bytes from state
    let bytes = state.try_as_bytes()
        .map_err(|e| Error::general_error(format!("Expected bytes: {}", e)))?;

    // Deserialize Parquet
    let cursor = std::io::Cursor::new(bytes);
    let df = ParquetReader::new(cursor)
        .finish()
        .map_err(|e| Error::from_error(ErrorType::General, e))?;

    Ok(Value::from_dataframe(df))
}
```

**Validation:**
```bash
cargo check -p liquers-lib
cargo test -p liquers-lib --lib parquet
# Expected: Compiles and compiles tests (even if no tests yet)
```

**Rollback:**
```bash
git diff liquers-lib/src/polars/parquet.rs
# Restore todo!() if needed
```

**Agent Specification:**
- **Model:** sonnet
- **Skills:** rust-best-practices
- **Knowledge:** Phase 2 architecture doc, `liquers-lib/src/polars/parquet.rs`
- **Rationale:** Deserialization logic with error handling requires judgment

---

### Step 5: Register Commands

**File:** `liquers-lib/src/commands.rs`

**Action:**
- Import parquet functions
- Register `to_parquet` and `from_parquet` commands using `register_command!` macro

**Code changes:**
```rust
// MODIFY: liquers-lib/src/commands.rs
use crate::polars::parquet::{to_parquet, from_parquet};  // NEW IMPORT

// In register_commands function:
register_command!(cr, fn to_parquet(state) -> result
    namespace: "polars"
    label: "To Parquet"
    doc: "Convert DataFrame to Parquet binary format"
    filename: "data.parquet"
)?;

register_command!(cr, fn from_parquet(state) -> result
    namespace: "polars"
    label: "From Parquet"
    doc: "Parse Parquet binary into DataFrame"
)?;
```

**Validation:**
```bash
cargo test -p liquers-lib --lib commands
# Expected: Command registration tests pass
```

**Rollback:**
```bash
git diff liquers-lib/src/commands.rs
# Remove registration lines if needed
```

**Agent Specification:**
- **Model:** haiku
- **Skills:** rust-best-practices
- **Knowledge:** `liquers-lib/src/commands.rs`, `specs/REGISTER_COMMAND_FSD.md`
- **Rationale:** Registration follows established register_command! pattern

---

### Step 6: Add Content-Type Mapping (Axum)

**File:** `liquers-axum/src/response.rs`

**Action:**
- Add `.parquet` extension to Content-Type mapping

**Code changes:**
```rust
// MODIFY: In extension_to_content_type function
fn extension_to_content_type(extension: &str) -> &'static str {
    match extension {
        "json" => "application/json",
        "csv" => "text/csv",
        "parquet" => "application/vnd.apache.parquet",  // NEW LINE
        _ => "application/octet-stream",
    }
}
```

**Validation:**
```bash
cargo check -p liquers-axum
# Expected: Compiles
```

**Rollback:**
```bash
git checkout liquers-axum/src/response.rs
```

**Agent Specification:**
- **Model:** haiku
- **Skills:** —
- **Knowledge:** `liquers-axum/src/response.rs`
- **Rationale:** Simple match arm addition

---

### Step 7: Write Unit Tests

**File:** `liquers-lib/src/polars/parquet.rs`

**Action:**
- Add unit tests at end of module
- Test `to_parquet`, `from_parquet`, round-trip, error cases

**Code changes:**
```rust
// ADD: At end of parquet.rs
#[cfg(test)]
mod tests {
    use super::*;
    use polars::prelude::*;

    #[test]
    fn test_to_parquet() {
        let df = DataFrame::new(vec![Series::new("a", &[1, 2, 3])]).unwrap();
        let state = State::from_value(Value::from_dataframe(df));
        let result = to_parquet(&state).unwrap();
        assert!(matches!(result, Value::Bytes(_)));
    }

    #[test]
    fn test_round_trip() {
        let df = DataFrame::new(vec![Series::new("a", &[1, 2, 3])]).unwrap();
        let state1 = State::from_value(Value::from_dataframe(df.clone()));

        let parquet = to_parquet(&state1).unwrap();
        let state2 = State::from_value(parquet);
        let result = from_parquet(&state2).unwrap();

        // Verify DataFrames match
        assert_eq!(df, result.try_as_dataframe().unwrap());
    }

    #[test]
    fn test_invalid_parquet() {
        let state = State::from_value(Value::Bytes(vec![0, 1, 2]));  // Invalid
        assert!(from_parquet(&state).is_err());
    }
}
```

**Validation:**
```bash
cargo test -p liquers-lib --lib parquet
# Expected: All 3 tests pass
```

**Rollback:**
```bash
# Remove #[cfg(test)] module if tests fail critically
```

**Agent Specification:**
- **Model:** sonnet
- **Skills:** rust-best-practices, liquers-unittest
- **Knowledge:** Phase 3 examples doc, `liquers-lib/src/polars/parquet.rs`
- **Rationale:** Unit tests require understanding of test patterns and edge cases

---

### Step 8: Write Integration Test

**File:** `liquers-lib/tests/parquet_integration.rs` (new file)

**Action:**
- Create integration test for end-to-end Parquet workflow
- Test query execution with Parquet commands

**Code changes:**
```rust
// NEW FILE: liquers-lib/tests/parquet_integration.rs
use liquers_core::query::parse_query;
use liquers_lib::SimpleEnvironment;

#[tokio::test]
async fn test_parquet_query_execution() {
    let env = SimpleEnvironment::new().await;

    // Create a DataFrame and convert to Parquet
    let query = parse_query("/-/test.csv~to_parquet").unwrap();
    let result = env.evaluate(&query).await;

    assert!(result.is_ok());
    // Further assertions on result...
}
```

**Validation:**
```bash
cargo test -p liquers-lib --test parquet_integration
# Expected: Test passes
```

**Rollback:**
```bash
rm liquers-lib/tests/parquet_integration.rs
```

**Agent Specification:**
- **Model:** sonnet
- **Skills:** rust-best-practices, liquers-unittest
- **Knowledge:** Phase 3 examples doc, `liquers-core/tests/async_hellow_world.rs` (integration test pattern)
- **Rationale:** Integration tests require full-stack understanding

---

### Step 9: Final Validation

**File:** (All files)

**Action:**
- Run full test suite
- Run clippy for linting
- Format code

**Validation:**
```bash
cargo build --all-features
cargo test --workspace
cargo clippy --workspace -- -D warnings
cargo fmt --all -- --check
```

**Expected:**
- All builds succeed
- All tests pass
- No clippy warnings
- Code is formatted

**Rollback:** N/A (this is final check)

**Agent Specification:**
- **Model:** sonnet
- **Skills:** rust-best-practices
- **Knowledge:** All implementation files, Phase 4 doc
- **Rationale:** Final validation requires judgment on any issues found

## Testing Plan

### Unit Tests

**When to run:** After Step 7

**File:** `liquers-lib/src/polars/parquet.rs` (inline `#[cfg(test)]` module)

**Command:**
```bash
cargo test -p liquers-lib --lib parquet
```

**Expected:**
- `test_to_parquet` passes
- `test_round_trip` passes
- `test_invalid_parquet` passes

### Integration Tests

**When to run:** After Step 8

**File:** `liquers-lib/tests/parquet_integration.rs`

**Command:**
```bash
cargo test -p liquers-lib --test parquet_integration
```

**Expected:**
- `test_parquet_query_execution` passes

### Manual Validation

**When to run:** After Step 9

**Commands:**
```bash
# 1. Start Axum server
cargo run -p liquers-axum
# Expected: Server starts on localhost:3000

# 2. Test Parquet download
curl http://localhost:3000/api/query/-/data.csv~to_parquet > output.parquet
# Expected: Binary file created

# 3. Verify Parquet file (external tool)
python3 -c "import polars as pl; print(pl.read_parquet('output.parquet'))"
# Expected: DataFrame printed, matches original data.csv
```

**Success criteria:**
- Server starts without errors
- Parquet file is created
- External tool can read Parquet file
- Data matches original CSV

## Agent Assignment Summary

| Step | Model | Skills | Rationale |
|------|-------|--------|-----------|
| 1 | sonnet | rust-best-practices | New module structure (architectural) |
| 2 | haiku | rust-best-practices | Module export (one-line change) |
| 3 | sonnet | rust-best-practices | to_parquet (error handling, Polars API) |
| 4 | sonnet | rust-best-practices | from_parquet (similar complexity) |
| 5 | haiku | rust-best-practices | Command registration (follows pattern) |
| 6 | haiku | — | Content-Type mapping (simple match arm) |
| 7 | sonnet | rust-best-practices, liquers-unittest | Unit tests (test patterns) |
| 8 | sonnet | rust-best-practices, liquers-unittest | Integration test (full stack) |
| 9 | sonnet | rust-best-practices | Final validation (judgment on issues) |

## Rollback Plan

(Documented per step above)

**Full feature rollback:**
```bash
git checkout main
git branch -D feature/parquet-support
# Delete files:
rm liquers-lib/src/polars/parquet.rs
rm liquers-lib/tests/parquet_integration.rs
# Revert modified files:
git checkout liquers-lib/src/polars/mod.rs
git checkout liquers-lib/src/commands.rs
git checkout liquers-axum/src/response.rs
```

## Documentation Updates

### CLAUDE.md

**No updates needed** - Parquet support follows existing patterns (command registration, value types).

### PROJECT_OVERVIEW.md

**No updates needed** - No new core concepts introduced.

### README.md

**Update:** Add Parquet to features list (if user-facing documentation exists)

## Execution Options

(Present to user after approval - see template above)

```

## Tips for Writing Phase 4

1. **Be specific about file paths** - Avoid "modify the command file" → Use "liquers-lib/src/commands.rs"
2. **Include code snippets** - Show exactly what to add/modify/delete
3. **Provide realistic validation** - `cargo check` is better than "verify it works"
4. **Order steps logically** - Don't register commands before implementing them
5. **Assign complexity appropriately** - Sonnet for judgment, Haiku for patterns
6. **Document rollback** - Every step should be reversible
7. **Test incrementally** - Don't wait until the end to run tests
8. **Set VERY HIGH certainty bar** - 95%+ confidence this plan will work

## Multi-Agent Review

After completing the Phase 4 document and running the inline review checklist, launch a **multi-agent review** — the most comprehensive review in the entire workflow.

### Reviewer Agents (4 haiku, launched in parallel)

**Reviewer 1 — Phase 1 Conformity (haiku):**
- Skills: (none required)
- Knowledge: Phase 1 document, Phase 4 document, PROJECT_OVERVIEW.md
- Task: Check that the implementation plan aligns with Phase 1 high-level design:
  - Implementation scope matches Phase 1 scope (no drift)
  - All interactions from Phase 1 are covered by implementation steps
  - No unscoped features in the plan

**Reviewer 2 — Phase 2 Conformity (haiku):**
- Skills: rust-best-practices
- Knowledge: Phase 2 document, Phase 4 document, PROJECT_OVERVIEW.md
- Task: Check that implementation steps match Phase 2 architecture:
  - Function signatures in steps match Phase 2 signatures exactly
  - Data structures and traits match Phase 2 definitions
  - Integration points match Phase 2 integration plan
  - Dependencies match Phase 2 dependency list

**Reviewer 3 — Phase 3 Conformity (haiku):**
- Skills: rust-best-practices, liquers-unittest
- Knowledge: Phase 3 document, Phase 4 document, PROJECT_OVERVIEW.md
- Task: Check that implementation plan covers Phase 3 requirements:
  - All examples from Phase 3 are achievable with the plan
  - All tests from Phase 3 are included in the testing plan
  - Corner cases from Phase 3 are addressed in implementation steps
  - Validation criteria from Phase 3 are used as success criteria

**Reviewer 4 — Codebase Compatibility (haiku):**
- Skills: rust-best-practices
- Knowledge: Phase 4 document, PROJECT_OVERVIEW.md, relevant source files from codebase
- Task: Check implementation plan against existing code:
  - File paths in steps are correct (files exist or parent directories exist)
  - Existing code that will be modified is accurately described
  - No conflicts with recent changes in the codebase
  - Dependencies are compatible with existing Cargo.toml versions

### Final Reviewer (1 opus)

**Opus Final Reviewer:**
- Skills: rust-best-practices, liquers-unittest
- Knowledge: PROJECT_OVERVIEW.md, ALL Phase 1-4 documents, all haiku reviewer outputs
- Task: Perform a **critical holistic review** of the entire design:
  1. Read all phase documents end-to-end
  2. Check for cross-phase inconsistencies (something said in Phase 1 contradicted in Phase 4)
  3. Evaluate architectural soundness of the complete design
  4. Identify risks, missing edge cases, or potential issues
  5. Fix problems directly in documents where possible
  6. Produce a final summary:
     - Confidence assessment (is this ready for implementation?)
     - List of issues found and fixes made
     - List of risks or concerns (even if not blocking)
     - Questions for the user (genuine decisions only)

### After Multi-Agent Review

- If no issues found: proceed directly to user approval gate
- If issues found and fixed by opus: present fixed documents + summary to user
- If opus raises concerns: discuss with user before requesting approval
- If confidence is below 95%: do NOT request approval — return to earlier phases

## Next Steps

**STOP HERE.** Present Phase 4 to the user and WAIT for explicit approval.

The user must say "proceed" or "Proceed to next phase" before you offer execution options. Any other response (feedback, questions, corrections, "looks good", "ok") is NOT approval — address the feedback and WAIT again.

After user says "proceed":
1. **Offer execution options** (execute now, create tasks, revise, exit)
2. If "execute now" chosen: Begin implementing steps sequentially
3. If "create tasks" chosen: Generate task list for later execution
4. If "exit" chosen: Save plan for user to implement manually
