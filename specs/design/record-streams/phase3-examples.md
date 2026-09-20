# Phase 3: Examples & Testing — Record streams

> **Not started.** Phase 2 is awaiting approval. This file records requirements Phase 3 must meet,
> gathered while Phase 2 was reviewed, so they are not re-derived.

## Requirements carried into this phase

### Identify the tests that belong to the language integration guide

`guides/LANGUAGE-INTEGRATION_GUIDE.md` §VALUE now prescribes `RECORDS01`–`RECORDS09` for any
*language* binding that exposes record values. **Phase 3 must decide which of its own tests are the
Rust-side counterparts of those**, so a binding author has a reference implementation rather than a
one-line summary — the guide's §3 explicitly says its appendix pseudocode "often fixes the contract
more narrowly than the one-line summary suggests".

At minimum, Phase 3 identifies the Rust test that establishes each of:

| Guide test | What Phase 3 must have a counterpart for |
|---|---|
| `RECORDS01` | a chunk round-trips with schema, roles and `ChunkOrigin` intact |
| `RECORDS02` | a column read matches a copy |
| `RECORDS03` | an Arrow export equals the source data; metadata survives or its loss is asserted |
| `RECORDS04` | buffers are read-only |
| `RECORDS05` | a view survives host-heap growth, or fails loudly |
| `RECORDS06` | releasing a handle releases the value |
| `RECORDS07` | a manifest-backed source is traversed one chunk at a time, nothing else resident |
| `RECORDS08` | async and sync traversal of one source yield identical rows |

`RECORDS09` is a binding-only disposition and needs no Rust counterpart.

### Other requirements gathered during Phase 2

- **The growth test named in Phase 2** — force `memory.grow` between creating a typed-array view and
  reading it, and assert the wrapper refreshed transparently. It is the test most likely to be
  skipped and the one that catches the browser hazard.
- **A round-trip per serialization format**, since `DefaultValueSerializer` is where a missing arm
  surfaces.
- **The build-matrix rows** for `records` on and off, including the wasm target.
- **Manifest validation**: each chunk query plans, `arguments` names exist in the last action, and no
  chunk name collides with a sibling `recipes.yaml`.
- **The identity regimes**: a manifest using per-chunk `arguments` without a `ChunkCache` must be
  rejected, because the failure is otherwise silent aliasing.


## High-Level Introduction

[Explain how these scenarios demonstrate the Phase 1 purpose and interactions. Introduce the
progression from the representative primary workflow, through additional detail, to optional
pitfalls and edge cases.]

## Example Type

**User choice:** [Runnable prototypes / Conceptual code]

## Overview Table

| # | Type | Name | Purpose | Drafted By |
|---|------|------|---------|------------|
| 1 | Example | [Primary scenario] | [Demonstrates the Phase 1 design through the primary workflow] | [Agent] |
| 2 | Example | [Detailed scenario] | [Explains additional details that build on Scenario 1] | [Agent] |
| 3 | Example | [Pitfalls and edge cases] | [Explains common pitfalls and edge cases; optional] | [Agent] |

## Example 1: [Primary Scenario Name]

### Connection to the High-Level Design
[How does this scenario solve or demonstrate the Phase 1 purpose and interactions?]

### Scenario
[Verbally explain the user context and intended outcome. Keep this representative use case
medium-complexity: non-trivial, but free of unnecessary details. Change defaults only when relevant.]

### Sequence of Steps
1. [First component interaction or method call]
2. [Next component and its responsibility]
3. [Execution, caching, persistence, or other relevant coordination]
4. [How the caller polls, awaits, retrieves, or uses the result]

### Core Example Code
```rust
// Show the core workflow; omit incidental setup and irrelevant non-default configuration.
```

### Guide and Executable Example
[If Phase 2 requires a guide, normally implement this primary scenario as a complete executable
example and give the path the guide will reference. Otherwise explain what canonical code or test
should be referenced.]

**Expected output:**
```
[What the user should see]
```

## Example 2: [Detailed Scenario]

[Build on Scenario 1 and explain an additional mechanism, meaningful configuration choice, or
component interaction. Reuse the primary setup and show only the relevant code delta. Include
expected output and validation.]

## Example 3 (Optional): [Pitfalls and Edge Cases]

[For each common pitfall or important edge case, state the symptom, cause, correct usage or
recovery, and protective test. Omit this scenario if it would only repeat later sections.]

## Corner Cases

### 1. Memory
[Large inputs, allocation failures, memory leaks]

### 2. Concurrency
[Race conditions, deadlocks, thread safety]

### 3. Errors
[Invalid input, network failures, serialization errors]

### 4. Serialization
[Round-trip, schema evolution, compression]

### 5. Integration
[Store, Command, Asset, Web/API interactions]

## Documentation and Learning Log

### Guide Candidate Workflows and Examples
[Answer: How do I use X? How do I achieve X? What is the typical workflow? Select potential guide
snippets and link a complete executable example or unit/integration test when available.]

### Usage and Meaning
[What helps users, developers, or coding agents understand why the feature matters and how it
connects to existing functionality]

### Repeatable Development Guidance
[What would help someone implement or extend similar functionality]

### Corrections and Unexpected Learning
[Design assumptions corrected, implementation surprises, useful dead ends, and facts to verify in
Phase 5]

## Test Plan

### Unit Tests
[File paths, test names, coverage]

### Integration Tests
[File paths, test names, end-to-end flows]

### Manual Validation
[Commands to run, expected outputs]

## Auto-Invoke: liquers-unittest Skill Output

[Test templates generated by skill]
