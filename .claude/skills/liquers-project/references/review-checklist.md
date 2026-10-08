# Review checklist

Use at the end of every phase. Answer each question for the phase in hand, fix what fails, and list
what only the user can decide. The concerns and who runs them are in `SKILL.md` §Reviews. A question
that does not apply is skipped, not padded.

## Every phase

- [ ] **Conformity:** consistent with every earlier phase; no scope added or dropped silently.
- [ ] **Size:** within the limit (`SKILL.md` §Design depth); existing code referenced by path and
  symbol, not pasted.
- [ ] **Records:** `DESIGN.md` `phase:` / `status:` current; source issues link back; `readiness` and
  `autofix` (when assessed) match the Phase 1 Design Readiness section.
- [ ] **Queries:** every Liquers query validated with `liquers-validate`, and its `encoded` form
  means what the text claims.
- [ ] **Questions:** each open question is tiered (blocking / open design / proposed resolution /
  implementation detail) and has a recommendation when the evidence supports one.

## Phase 1: high-level design

- [ ] Purpose fits in 1-3 sentences.
- [ ] `## Problem Example` shows one concrete case: what happens today, and what should happen.
- [ ] Acceptance criteria are `AC-<n>` scenarios, each with WHEN and THEN, observable from outside;
  non-goals are stated.
- [ ] Overlap triage done (`issue-triage.md`, `overlap.md`). No existing design already covers this,
  and weak overlaps are listed as dependencies.
- [ ] Crate placement respects the dependency flow in `CLAUDE.md`.
- [ ] Documentation intent answers all four: reference, guide, other documents, documents to update.

## Phase 2: architecture

- [ ] Known-issue preflight done; no unresolved blocker; every blocker is at least P1.
- [ ] Every named file, type, trait and function exists as described (opened, not remembered).
  Existing functionality that could be reused has been looked for.
- [ ] `rust-best-practices` applied. Check ownership (`Arc`/`Box`/owned), no `unwrap`/`expect` in
  library code, typed `Error` constructors, no `_ =>` on Liquers enums, async by default with no
  blocking I/O in async code, minimal trait bounds, and `cfg(feature)` gating complete.
- [ ] New value types follow the four steps in `CLAUDE.md` (ExtValue variant, identifier,
  serialization, `TypeInfo`).
- [ ] Commands are given as `register_command!` lines; a signature change plans the
  `specs/command_registry.yaml` regeneration.
- [ ] Documentation architecture gives exact paths and changes, and the proposed `affects_docs`.
- [ ] The risk table is filled in, with no category left empty without a reason.

## Phase 3: examples and tests

- [ ] Every `AC-<n>` scenario is cited by at least one named test; no test cites an undefined one.
- [ ] Tests assert externally meaningful behaviour, not implementation details.
- [ ] Error and edge cases match the risks Phase 2 named. No more, no fewer.
- [ ] Resource queries (`-R/…`) have a store in the test environment; the commands they use are
  registered.
- [ ] Tests follow `liquers-unittest` and `specs/guides/UNITTEST_GUIDE.md` (feature-gated where they
  need an optional dependency).

## Phase 4: implementation plan

- [ ] Every step names its files and symbols, its proof command and its rollback.
- [ ] The `## Progress` checklist (compact: the `### Steps` checklist) has one unticked item per step.
- [ ] The order is feasible: nothing uses a symbol before the step that creates it.
- [ ] Signatures the steps depend on were re-opened at HEAD.
- [ ] The plan includes test, documentation, index and generated-file updates, and the final checks
  (feature matrix when `cfg(feature)` or an optional dependency changes).
- [ ] Smallest coherent scope; nothing outside the design.

**Consistency pass before the Phase 4 gate:** read all phases together. Check for contradictions,
acceptance criteria without tests, risks without validation, steps without proof, and assumptions
that the code has since invalidated. Update Phase 1's readiness and open questions from what you
find.

## Phase 5: documentation

- [ ] Entry criteria met: implementation validated, every comment answered; every Phase 4 progress
  item ticked, or its remainder filed as an issue.
- [ ] The summary separates requested, implemented, omitted and added scope, and fits in 1-3 pages.
- [ ] Every document in `affects_docs` reviewed against the code and tests, with a History row and a
  `reviewed:` bump (§9.2).
- [ ] Planned reference and guide documents exist; `specs/README.md` links to them.
- [ ] Completed issues `closed` with a resolution note; discovered problems triaged, each with its
  outcome.
- [ ] `python3 scripts/docs_index.py --check` passes.
