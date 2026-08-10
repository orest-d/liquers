# Critical Review Checklists

This document provides phase-specific checklists for conducting critical reviews before user approval gates. Use these to validate completeness and quality at each phase.

**Purpose:** Ensure each phase meets high standards before progressing. Catch issues early when they're cheap to fix.

## How to Use This Document

1. **Before requesting user approval:** Run the appropriate phase checklist
2. **Mark each item:** Check boxes as you verify each criterion
3. **If any item fails:** Revise the phase document before requesting approval
4. **Approval criteria:** All checklist items must pass (or explicitly documented exceptions)

---

## Phase 1 Review: High-Level Design

**Purpose:** Validate scope clarity, avoid duplication, align with Liquers philosophy

### Scope Clarity

- [ ] **Feature purpose fits in 1-3 sentences**
  - Can you explain what this feature does to a non-technical stakeholder?
  - If no: Simplify the purpose statement

- [ ] **System interactions are identified**
  - Query system interaction documented?
  - Store system interaction documented?
  - Command system interaction documented?
  - Asset system interaction documented?
  - Value types documented?
  - Web/API interaction documented (if applicable)?
  - UI interaction documented (if applicable)?

- [ ] **Crate placement is clear**
  - Which crate will contain this feature?
  - Rationale provided based on dependency flow?
  - Follows one-way dependency rule?

- [ ] **Scope is appropriate (not too large, not trivial)**
  - If too large: Consider breaking into multiple features
  - If too small: Might not need full 5-phase process
  - Goldilocks zone: Substantial enough to require architecture, small enough to complete

### No Duplication

- [ ] **Feature doesn't overlap with existing functionality**
  - Searched codebase for similar features?
  - If overlap exists: Justified why new feature is needed vs. extending existing code?

- [ ] **Checked for similar implementations in other crates**
  - Searched `liquers-lib/src/` for related code?
  - Searched `liquers-core/src/` for related abstractions?

### Aligns with Liquers Philosophy

- [ ] **Fits the query-based, layered architecture**
  - Feature integrates with Queries → Commands → State → Assets flow?
  - Doesn't bypass core abstractions?

- [ ] **Respects crate dependency flow**
  - Dependency direction: liquers-core ← liquers-macro ← liquers-store ← liquers-lib ← liquers-axum?
  - No circular dependencies introduced?

- [ ] **Async is default (with sync wrappers if needed)**
  - Is async the default approach for I/O operations?
  - Sync wrappers only added when justified (e.g., Python bindings, egui)?

### Documentation Intent

- [ ] **All four documentation-needs questions are briefly answered**
  - Reference for current behavior, meaning, importance, or connections?
  - Guide for repeatable use, extension, testing, debugging, or similar work?
  - New versus extension decision made for both reference and guide?
  - Other new documents and specific updates named, or `None` justified?
  - Rationale given for every decision?

- [ ] **Audience and independence are clear**
  - Can the planned document communicate without requiring the design folder?
  - Is the audience internal, user, or both?

- [ ] **A `neither` decision has a reconsideration condition**
  - Will substantial implementation learning trigger a Phase 5 reconsideration?

### Questions Identified

- [ ] **All open questions documented**
  - Are unknowns acknowledged?
  - Are questions specific (not vague)?

- [ ] **No blocking unknowns**
  - Can you proceed to Phase 2 without answering these questions?
  - If no: Research or ask user for clarification before approval

- [ ] **Open questions realistic to resolve in Phase 2**
  - Are these architectural choices (resolvable)?
  - Or are they research questions (need investigation first)?

### Readability

- [ ] **Document is under 30 lines (excluding template structure)**
  - If no: Too detailed for Phase 1; move details to Phase 2

- [ ] **Someone unfamiliar with the feature can understand it**
  - Read it aloud; does it make sense?
  - No jargon without explanation?

### Approval Criteria (Phase 1)

- [ ] **High-level makes sense**
  - Purpose is clear
  - Interactions are logical
  - Fits Liquers architecture

- [ ] **Scope is appropriate**
  - Not too ambitious
  - Not too trivial
  - Goldilocks zone

- [ ] **User agrees to proceed**
  - User has reviewed the high-level design
  - User approves moving to Phase 2

**If all boxes checked:** ✅ Ready for user approval

**If any boxes unchecked:** ❌ Revise Phase 1 before requesting approval

---

## Phase 2 Review: Solution & Architecture

**Purpose:** Validate type design, trait implementations, integration, error handling

### Known-Issue Preflight

- [ ] **Relevant open known issues were searched**
  - Issues linked to the design or Phase 1 checked?
  - `specs/issues/` searched by affected area, integration point, dependency, and assumption?
  - `specs/index.csv` checked for GitHub-owned `tracked` or `in_progress` issues?

- [ ] **Solution impact is assessed for every relevant issue**
  - Does the issue invalidate or constrain an architectural choice?
  - Must it be resolved before implementation, worked around, or merely monitored?
  - Is the rationale recorded even when the issue is non-blocking?

- [ ] **Blocking decisions are actionable**
  - Is each issue explicitly blocking or non-blocking?
  - Has every blocker been resolved, or has the architecture removed the dependency?
  - Is Phase 2 approval withheld while any blocker remains unresolved?

- [ ] **Blocker priority is appropriate**
  - Is every blocker at least P1?
  - Was an increase from P2/P3 recorded and confirmed with the user or issue owner?
  - Is P0 used only for the impact criteria in `DOCS_STRUCTURE_GUIDE.md` §4.4, rather than merely
    because the issue blocks this project?

### Type Design

- [ ] **All structs have documented fields with types**
  - Every field has a type annotation?
  - Field purposes are clear from names/comments?

- [ ] **Ownership is explicit (Arc, Box, owned, borrowed)**
  - Ownership choice documented for each struct field?
  - Rationale provided (e.g., "Arc for shared access across threads")?

- [ ] **Serialization strategy is defined**
  - `#[derive(Serialize, Deserialize)]` used where appropriate?
  - `#[serde(skip)]` used for non-serializable fields?
  - Documented which fields are skipped and why?

- [ ] **No `unwrap()` or `expect()` in signatures**
  - All fallible operations return `Result<T, Error>`?
  - No panics in library code (only tests)?

### Trait Implementations

- [ ] **All trait implementations are listed**
  - Which traits will be implemented?
  - For which types?

- [ ] **Trait bounds are minimal and justified**
  - Generic parameters have necessary bounds only?
  - Rationale provided for each bound (e.g., "Send + Sync for thread sharing")?

- [ ] **Generic parameters have clear purpose**
  - Why is each generic parameter needed?
  - Can it be removed or replaced with a concrete type?

### Match Statements

- [ ] **Enum variants are fully documented**
  - Each variant has documented purpose?
  - When is each variant used?

- [ ] **No default match arms (`_ =>`) planned**
  - All match statements on project enums are explicit?
  - Future variants will trigger compile errors?
  - Exception: External enums (documented why)

### Integration

- [ ] **File paths are specified**
  - Which modules will be created?
  - Which files will be modified?
  - Full paths provided (e.g., `liquers-lib/src/polars/parquet.rs`)?

- [ ] **Dependencies are listed with versions**
  - Which external crates are needed?
  - Version numbers specified (or "latest compatible with X")?

- [ ] **Compatibility with existing crates verified**
  - No version conflicts with existing dependencies?
  - No breaking changes to public APIs?
  - No known open issue invalidates the proposed integration?

- [ ] **Follows liquers-patterns.md**
  - Crate dependencies follow one-way flow?
  - ExtValue variants in liquers-lib only?
  - Commands registered via `register_command!`?
  - AsyncStore pattern followed (if applicable)?
  - UIElement pattern followed (if applicable)?

### Async/Sync Decisions

- [ ] **Async decisions made with rationale**
  - For each function: async or sync?
  - Rationale provided (e.g., "async for I/O", "sync for pure computation")?

- [ ] **AsyncStore pattern followed for stores (if applicable)**
  - Store implements `AsyncStore` trait?
  - Sync wrapper only added if needed (e.g., Python)?

### Error Handling

- [ ] **Uses `Error::typed_constructor()` (NOT `Error::new`)**
  - `Error::general_error()` used?
  - `Error::key_not_found()` used?
  - `Error::from_error()` used?
  - NO direct calls to `Error::new()`?

- [ ] **Error scenarios documented**
  - What can go wrong?
  - Which ErrorType for each scenario?
  - Error messages are helpful?

### Compilation Validation

- [ ] **All type signatures are specified**
  - Function parameters have types?
  - Return types are explicit?
  - No missing type annotations?

- [ ] **All imports are documented**
  - Which crates to import from?
  - Which modules?

### Relevant Commands

- [ ] **New commands listed with full signatures**
  - All new commands that the feature introduces?
  - Full function signatures provided?
  - Namespace specified for each command?

- [ ] **Relevant existing namespaces identified**
  - Which existing command namespaces interact with this feature?
  - Key commands from those namespaces listed?

- [ ] **User confirmed namespace selection**
  - User asked about relevant namespaces?
  - User feedback incorporated?

### Documentation Architecture

- [ ] **New document paths and metadata are specified**
  - Exact new or extended reference and guide paths, kind, audience, area, and purpose?
  - Every other new document has an exact path, purpose, and link destination?

- [ ] **Existing documentation impact is specified**
  - Every specific Phase 1 update is fully specified?
  - Candidate affected documents identified by area?
  - Proposed authoritative `affects_docs` set listed?

- [ ] **Design and capability links are planned**
  - `specs/README.md` transition identified?
  - Cross-links to add, replace, or remove identified?

- [ ] **Evidence collection is planned**
  - Usage guidance, development guidance, corrections, connections, and unexpected learning?

### Multi-Agent Review (Phase 2)

- [ ] **Reviewer A (Phase 1 conformity) completed**
  - Scope hasn't drifted from Phase 1?
  - All Phase 1 interactions addressed?
  - No unscoped features crept in?

- [ ] **Reviewer B (Codebase alignment) completed**
  - Function signatures match existing code?
  - No missed reuse opportunities?
  - Integration point inconsistencies flagged?
  - Known-issue search and blocking decisions cross-checked?
  - Blockers below P1 or unjustified P0 recommendations flagged?

- [ ] **Sonnet fixer completed (if issues found)**
  - All fixable issues resolved?
  - Summary of fixes provided?
  - Remaining questions presented to user?

- [ ] **No open issues from multi-agent review**
  - All reviewer findings addressed?
  - No unresolved contradictions?

### Approval Criteria (Phase 2)

- [ ] **All signatures compilable (at least in theory)**
  - Mentally verify: would this compile with `cargo check`?
  - Only missing implementations (which is correct at this stage)?

- [ ] **High confidence in approach**
  - Confidence level: 80%+ that this architecture will work?
  - No major architectural unknowns remaining?
  - No unresolved known issue blocks the solution?

- [ ] **Relevant commands identified and confirmed by user**
  - New commands with signatures listed?
  - Existing namespaces confirmed?

- [ ] **Multi-agent review completed with no open issues**
  - All reviewers ran successfully?
  - All fixable issues resolved?

- [ ] **User agrees with the design**
  - User has reviewed the architecture
  - User approves moving to Phase 3

**If all boxes checked:** ✅ Ready for user approval (after rust-best-practices auto-invoke + multi-agent review)

**If any boxes unchecked:** ❌ Revise Phase 2 before requesting approval

---

## Phase 3 Review: Examples & Testing

**Purpose:** Validate example quality, corner case coverage, test completeness

### Examples

- [ ] **2-3 realistic scenarios provided**
  - At least 2 examples?
  - No more than 4 (keep focused)?

- [ ] **User chose runnable vs. conceptual**
  - User decision documented?
  - If runnable: examples compile and run?
  - If conceptual: examples are clear and understandable?

- [ ] **Examples demonstrate core functionality**
  - Each example shows a key use case?
  - Examples are not trivial (not just "hello world")?

- [ ] **Examples use realistic data/parameters**
  - Not toy data (e.g., "foo", "bar")?
  - Representative of actual usage?

- [ ] **Expected outputs are documented**
  - For each example: what should the user see?
  - Success criteria clear?

### Corner Cases

- [ ] **Memory corner cases addressed**
  - Large inputs considered?
  - Allocation failures documented?
  - Memory leaks checked?
  - Mitigation strategies provided?

- [ ] **Concurrency corner cases addressed**
  - Race conditions considered?
  - Deadlock scenarios checked?
  - Thread safety documented?
  - Async compatibility verified?

- [ ] **Error corner cases addressed**
  - Invalid input scenarios?
  - Network failures (if applicable)?
  - Serialization errors?
  - Partial failure handling?

- [ ] **Serialization corner cases addressed**
  - Round-trip compatibility tested?
  - Schema evolution considered?
  - Compression support (if applicable)?
  - Metadata preservation documented?

- [ ] **Integration corner cases addressed**
  - Store system integration?
  - Command system chaining?
  - Asset system caching?
  - Web/API interaction (if applicable)?

### Test Coverage

- [ ] **Unit tests cover happy path + error path**
  - Success cases tested?
  - Failure cases tested?
  - Edge cases tested?

- [ ] **Integration tests cover end-to-end flows**
  - Full query execution tested?
  - Cross-module interactions tested?

- [ ] **Manual validation commands provided**
  - Commands to run provided?
  - Expected outputs documented?
  - Success criteria clear?

- [ ] **liquers-unittest skill invoked for test templates**
  - Test templates generated?
  - Templates customized for this feature?
  - All test files identified (file paths)?

### Guide Candidate Examples

- [ ] **High-level guide questions are answered**
  - How do I use the capability?
  - How do I achieve its primary outcome?
  - What is the typical workflow?

- [ ] **Reusable evidence is identified**
  - Potential guide snippets selected?
  - A complete executable example or unit/integration test linked when available?
  - If no complete example exists, Phase 4 says whether to create one?

### User Feedback Loop

- [ ] **User satisfied with examples**
  - User reviewed examples?
  - User confirmed they're realistic?

- [ ] **User satisfied with corner cases**
  - User reviewed corner cases?
  - No missing categories?

- [ ] **User satisfied with test plan**
  - User reviewed test plan?
  - User agrees coverage is complete?

### Overview Table

- [ ] **Overview table present at top of Phase 3 document**
  - All examples listed with purpose?
  - All test suites listed with what they check?
  - Drafting agent assignment noted?

### Query Validation

- [ ] **No spaces, newlines, or special characters in queries**
  - All queries use `-` for parameter separation?
  - No whitespace in query strings?
  - Only valid characters: alphanumeric, `-`, `~`, `/`, `.`, `_`?

- [ ] **Resource part (`-R/`) queries have matching store definitions**
  - Every `-R/` query has a corresponding store in the test setup?
  - MemoryStore pre-populated with referenced resources?

- [ ] **All commands in queries are registered**
  - Cross-referenced with Phase 2 Relevant Commands list?
  - All commands exist (new or existing)?
  - No typos in command names?

- [ ] **Namespace references are valid**
  - `ns-<namespace>/command` syntax uses real namespaces?
  - Namespaces match Phase 2 Relevant Existing Namespaces?

### Multi-Agent Review (Phase 3)

- [ ] **Reviewer 1 (Phase 1 conformity) completed**
  - Examples demonstrate Phase 1 feature purpose?
  - No examples outside Phase 1 scope?
  - All Phase 1 interactions covered?

- [ ] **Reviewer 2 (Phase 2 conformity) completed**
  - Function signatures match Phase 2?
  - Data structures used correctly?
  - Trait usage matches Phase 2 definitions?

- [ ] **Reviewer 3 (Codebase + query validation) completed**
  - Query validation rules applied?
  - Imports and types verified against codebase?
  - Test patterns match liquers conventions?

- [ ] **Sonnet fixer completed (if issues found)**
  - All fixable issues resolved?
  - Summary with fixes + potential problems provided?
  - Remaining questions presented to user?

- [ ] **No open issues from multi-agent review**
  - All reviewer findings addressed?
  - All queries validated?

### Approval Criteria (Phase 3)

- [ ] **Coverage is complete**
  - No major gaps in test coverage?
  - All error paths tested?

- [ ] **Examples are realistic**
  - Not toy scenarios?
  - Representative of actual usage?

- [ ] **Overview table present and accurate**
  - All examples and tests summarized?

- [ ] **All queries validated**
  - No syntax issues, all commands registered, stores defined?

- [ ] **Multi-agent review completed with no open issues**
  - All reviewers ran successfully?
  - All fixable issues resolved?

- [ ] **User is satisfied with test plan**
  - User has reviewed and approved

**If all boxes checked:** ✅ Ready for user approval (after liquers-unittest auto-invoke + multi-agent review)

**If any boxes unchecked:** ❌ Revise Phase 3 before requesting approval

---

## Phase 4 Review: Implementation Plan

**Purpose:** Validate implementation readiness, testing completeness, documentation updates

### Implementation Readiness

- [ ] **All steps have exact file paths**
  - Every step identifies specific files?
  - No vague descriptions ("the command file")?

- [ ] **All steps have specific actions**
  - Each step has clear instructions?
  - Not vague ("implement the feature")?

- [ ] **All steps have validation commands**
  - Every step has a `cargo check` or `cargo test` command?
  - Validation commands are realistic (will actually verify the change)?

- [ ] **Validation commands are realistic**
  - Will the command actually verify the step completed correctly?
  - Expected output documented?

- [ ] **Steps are ordered logically**
  - Dependencies respected (e.g., create module before importing it)?
  - No forward references (using something not yet created)?

- [ ] **No circular dependencies between steps**
  - Step graph is acyclic?
  - Clear start-to-finish path?

### Testing Completeness

- [ ] **Unit tests specified with file paths**
  - File path provided (e.g., `liquers-lib/src/module.rs`)?
  - Test names listed?

- [ ] **Integration tests specified with file paths**
  - File path provided (e.g., `liquers-lib/tests/test.rs`)?
  - Test names listed?

- [ ] **Manual validation commands provided**
  - Commands to run documented?
  - Expected outputs specified?

- [ ] **Success criteria are clear and objective**
  - "All tests pass" (clear)?
  - Not "looks good" (vague)?

- [ ] **Error paths are tested (not just happy paths)**
  - Invalid input tests?
  - Failure scenario tests?

### Documentation

- [ ] **CLAUDE.md updates identified (if needed)**
  - New patterns documented?
  - Section to modify identified?

- [ ] **PROJECT_OVERVIEW.md updates identified (if needed)**
  - Core concepts changed?
  - Section to modify identified?

- [ ] **README.md updates identified (if applicable)**
  - User-facing feature?
  - Section to modify identified?

- [ ] **All new patterns documented**
  - If introducing new patterns: documented in CLAUDE.md?
  - Examples provided?

- [ ] **Phase 2 documentation architecture is implemented by explicit steps**
  - New reference/guide files have implementation steps?
  - Existing affected documents and History/reviewed updates have steps?
  - Capability-map and cross-link updates have steps?

- [ ] **Phase 5 evidence will be captured during implementation and review**
  - Requested-versus-implemented scope recorded?
  - Omissions/additions and reasons recorded?
  - Filed issues and unexpected learning recorded?

### Rollback Plan

- [ ] **Per-step rollback commands provided**
  - Every step has a rollback command?
  - Rollback commands are specific (not "undo changes")?

- [ ] **Full feature rollback documented**
  - How to completely remove the feature?
  - Files to delete listed?
  - Files to restore listed?
  - Cargo.toml changes to revert listed?

- [ ] **Partial completion strategy defined**
  - What if implementation paused mid-way?
  - How to resume later?

### Agent Specifications

- [ ] **Every step has an agent specification**
  - Model specified (haiku/sonnet/opus)?
  - Skills listed?
  - Knowledge/context files listed?
  - Rationale provided for model selection?

- [ ] **Agent assignment summary table present**
  - All steps in one table?
  - Model, skills, rationale columns filled?

- [ ] **Model assignments justified (complexity-based)**
  - Opus for holistic/cross-crate reasoning (rare)?
  - Sonnet for architectural/complex steps?
  - Haiku for pattern-following/boilerplate steps?

- [ ] **No ambiguity about ownership**
  - Each step has exactly one agent specification?
  - No steps without assignments?

### Multi-Agent Review (Phase 4)

- [ ] **Reviewer 1 (Phase 1 conformity) completed**
  - Implementation scope matches Phase 1?
  - All Phase 1 interactions covered by steps?

- [ ] **Reviewer 2 (Phase 2 conformity) completed**
  - Signatures match Phase 2?
  - Integration points match Phase 2?

- [ ] **Reviewer 3 (Phase 3 conformity) completed**
  - All Phase 3 examples achievable?
  - All Phase 3 tests in testing plan?
  - Corner cases addressed?

- [ ] **Reviewer 4 (Codebase compatibility) completed**
  - File paths correct?
  - Existing code accurately described?
  - Dependencies compatible?

- [ ] **Opus final reviewer completed**
  - Cross-phase consistency verified?
  - Architectural soundness confirmed?
  - Risks and concerns documented?
  - Confidence assessment provided?

- [ ] **No open issues from multi-agent review**
  - All reviewer findings addressed?
  - Opus concerns resolved or acknowledged?

### Very High Certainty

- [ ] **Confidence level: 95%+ that this plan can be executed**
  - Can a developer execute this plan without making architectural decisions?
  - All unknowns resolved?

- [ ] **All open questions from Phase 1 resolved**
  - No "TBD" items?
  - No "figure out later" items?

- [ ] **No "TBD" or "figure out later" items**
  - Every step is concrete?
  - No placeholders?

- [ ] **Clear path forward, no blocking unknowns**
  - Execution can start immediately?
  - No research needed during implementation?

**If confidence < 95%:** ❌ DO NOT request approval. Return to Phase 2/3 to resolve unknowns.

### Approval Criteria (Phase 4)

- [ ] **Very high certainty (95%+)**
  - Confident this plan will work?
  - No major unknowns?

- [ ] **Clear execution path**
  - Developer can start implementing immediately?
  - No architectural decisions left to make?

- [ ] **Agent specifications complete**
  - Every step has model, skills, knowledge?
  - Summary table present?

- [ ] **Multi-agent review completed with no open issues**
  - All 4 haiku reviewers completed?
  - Opus final reviewer completed?
  - All fixable issues resolved?

- [ ] **Rollback plan exists**
  - Feature can be removed if needed?
  - Partial completion is safe?

- [ ] **User approves**
  - User has reviewed and approved the plan

**If all boxes checked:** ✅ Ready for user approval (after rust-best-practices auto-invoke + multi-agent review)

**If any boxes unchecked:** ❌ Revise Phase 4 before requesting approval

**After approval:** Offer execution options. If implementation runs, Phase 5 remains mandatory after
implementation, validation, and review feedback are complete.

---

## Phase 5 Review: Documentation

**Purpose:** Validate that implemented work is accurately summarized and anchored in current
documentation without requiring readers to inspect the design.

### Entry Gate

- [ ] Implementation is finished and validated
- [ ] All user comments are answered or incorporated
- [ ] All review comments are answered or incorporated
- [ ] Documentation is consistent with the implemented and tested behavior
- [ ] Documentation is included in the implementation PR when practical

### Summary

- [ ] `phase5-documentation.md` exists in the design folder
- [ ] Summary is roughly one page and no more than three pages
- [ ] Requested, approved, and implemented scope are compared explicitly
- [ ] Every omission or addition has a reason
- [ ] New issues and all deferred work are listed
- [ ] Important learning, corrections, meaning, and connections are captured

### Reference and Guide Documents

- [ ] Phase 1-2 documentation commitments are fulfilled or a changed decision is justified
- [ ] References describe present behavior, intent, meaning, importance, and connections
- [ ] Guides provide actionable, repeatable instructions
- [ ] Neither reference nor guide requires the design folder to be understood
- [ ] A prior `neither` decision was reconsidered if substantial information accumulated

### Existing Documentation and Links

- [ ] `affects_docs` is authoritative and every listed document was reviewed against implemented code
- [ ] Every new or reviewed current-state document has valid front matter
- [ ] `reviewed:` matches the newest `## History` date and source is `phase-5`
- [ ] `specs/README.md` points to the highest-stage current document
- [ ] Other relevant design/reference/guide links are added, updated, or replaced
- [ ] Documentation validation passes
- [ ] If a rebase or merge conflict changed relevant content, post-merge consistency was rechecked

### Approval and Closure

- [ ] User reviewed the summary and documentation changes
- [ ] User explicitly said `proceed`
- [ ] `status: complete` is set and `phase` removed only after approval
- [ ] The design folder is frozen after completion

---

## Summary: Approval Confidence Levels

- **Phase 1:** High-level makes sense → ~70% confidence (many unknowns OK)
- **Phase 2:** Architecture is sound → ~80% confidence (implementation unknowns OK)
- **Phase 3:** Coverage is complete → ~90% confidence (minor unknowns OK)
- **Phase 4:** Plan is executable → ~95% confidence (very few unknowns)
- **Phase 5:** Shipped result and current documentation are verified → completion confidence

**Key insight:** Confidence increases through planning; Phase 5 verifies the finished result and
documentation before closure.

## Common Review Mistakes

### Mistake 1: Rushing Through Checklists

**Symptom:** Checking all boxes without actually verifying

**Fix:** Take time to actually verify each item. If unsure, it's not checked.

### Mistake 2: Skipping Failed Items

**Symptom:** "Most items pass, good enough"

**Fix:** ALL items must pass (or explicitly documented exceptions). Fix failing items before approval.

### Mistake 3: Vague Approval Criteria

**Symptom:** "Looks good to me"

**Fix:** Use objective criteria from checklists. "All tests pass" is objective; "looks good" is not.

### Mistake 4: Low Confidence in Phase 4

**Symptom:** "I think this will work, let's try"

**Fix:** Phase 4 requires 95%+ confidence. If < 95%, return to earlier phases to resolve unknowns.

### Mistake 5: Ignoring User Feedback

**Symptom:** "User had concerns but I proceeded anyway"

**Fix:** User approval is mandatory. If user has concerns, address them before proceeding.

## When to Return to Earlier Phases

**From Phase 2:**
- Major architectural unknown discovered → Return to Phase 1 (re-scope)
- Type design unclear → Stay in Phase 2 (revise)

**From Phase 3:**
- Examples reveal architectural flaw → Return to Phase 2 (fix architecture)
- Corner case requires new data structure → Return to Phase 2
- Test coverage gap (minor) → Stay in Phase 3 (add tests)

**From Phase 4:**
- Implementation step reveals unknown → Return to Phase 2/3 (resolve unknown)
- Confidence < 95% → Return to Phase 2/3 (increase certainty)
- User feedback requires rethink → Return to appropriate phase

**From Phase 5:**
- Shipped behavior differs materially from the design → document the deviation and update current docs
- Deferred work has no issue → file it before closure
- Learning exceeds the summary limit → create or expand a reference/guide and link it

**Don't fight the process:** Returning to earlier phases is a feature, not a bug. It's cheaper to fix issues early than during implementation.

## Final Checklist Before User Approval

**Before every approval gate:**

1. [ ] Run appropriate phase checklist (this document)
2. [ ] All items checked (or exceptions documented)
3. [ ] Auto-invoke skills completed (Phase 2/3/4)
4. [ ] Multi-agent review completed (Phase 2/3/4) — all reviewers ran, fixer resolved issues
5. [ ] Confidence level meets phase requirement
6. [ ] User has reviewed the phase document
7. [ ] User feedback incorporated
8. [ ] Ready to proceed to the next phase, execution, or design closure

**If all 8 items checked:** ✅ Request user approval

**If any item unchecked:** ❌ Do not request approval; revise first
