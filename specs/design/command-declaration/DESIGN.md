---
id: COMMAND-DECLARATION
kind: design
workflow: liquers-project
title: A language-neutral command declaration type
status: in_review
phase: architecture
area: [core/commands, web, py]
gh_pr: []
issues: [COMMAND-DECLARATION-FORMAT, STATE-ARGUMENT-CONSTRUCTOR-SERDE-DEFAULT-DISAGREE,
         JS-COMMAND-CANNOT-ACCESS-CONTEXT, ARGUMENT-DECLARATION-IS-ALL-OR-NOTHING]
created: 2026-08-29
superseded_by:
---
# A language-neutral command declaration type

Design tracking for [`issues/COMMAND-DECLARATION-FORMAT.md`](../../issues/COMMAND-DECLARATION-FORMAT.md).

**Started** under [`guides/autonomous_issue_fixing.md`](../../guides/autonomous_issue_fixing.md) as a
simplified two-phase design. **Converted to `liquers-project` on 2026-08-29** by maintainer decision,
after the purpose statement re-scoped the issue `M → L`: the feature is a merge algebra with
absence-tracking and name-keyed argument merging, a defaults-derivation rule set, and a call
specification, which the simplified procedure does not fit. The `workflow: liquers-project` marker
is set, so all five phases and the Phase 5 documentation contract now apply.

## Phase status

- [x] Phase 1: High-level design — [`phase1-high-level-design.md`](./phase1-high-level-design.md)
- [x] Phase 2: Solution and architecture — [`phase2-architecture.md`](./phase2-architecture.md)
      *(rewritten 2026-08-29 against the purpose statement: a four-stage pipeline whose middle
      three stages — merge, derive defaults, convert — are the shared deliverable. `run` withdrawn
      pending the gate; see its open question 1.)*
- [x] Purpose and semantics — [`purpose-and-semantics.md`](./purpose-and-semantics.md)
      *(maintainer purpose statement, drafted as the future API doc, with a critical evaluation
      and the recorded decisions. **This document, not Phase 1, defines what the feature is.**)*
- [x] Portability validation — [`portability-analysis.md`](./portability-analysis.md)
      *(six languages assessed; the bar "clear benefit for Python and JavaScript" is met, but
      asymmetrically — see its §Bar)*
- [ ] Phase 2 approval gate — **awaiting a decision** (5 open questions in Phase 2)
- [ ] Phase 3: Examples and use-cases — `phase3-examples.md`, not yet created
- [ ] Phase 4: Implementation plan and execution — `phase4-implementation.md`, not yet created
- [ ] Phase 5: Documentation — `phase5-documentation.md`, mandatory under `liquers-project`

## Why this folder exists

`liquers-web` hand-parses a command declaration out of a `JsValue`, and a Python binding would
rewrite it. The feature is the runtime equivalent of `register_command!`: it says how a *function*
becomes a *command*, where `CommandMetadata` describes the command itself.

Its substance is a four-stage pipeline whose middle three stages are shared:

```
1. populate   host introspection fills what it can discover          host-specific
2. enhance    the author's declaration is merged over it             SHARED
3. fill       defaults are derived for whatever is still absent      SHARED
4. use        convert to CommandMetadata + CallSpec, or error        SHARED
```

Merging happens on the serialized form, so *absence is key-absence* — the distinction a typed
representation cannot make and the merge cannot do without.
[`purpose-and-semantics.md`](./purpose-and-semantics.md) is the authoritative statement of what this
is and why; [`portability-analysis.md`](./portability-analysis.md) tests the reuse claim against six
languages.

Two earlier drafts of Phase 2 are recorded in its §Rejected alternatives rather than deleted: a
struct mirroring `CommandMetadata` field for field, and a "fix `CommandMetadata` and add the residue"
design. Both mistook the feature for a serialization problem.

## Relationship to `environment-builder`

The issue was filed during that design's preflight and is listed in its `issues:` set, but this is
separate work with its own scope and its own gate. Nothing here changes
[`design/environment-builder/`](../environment-builder/)'s phase documents, front-matter or
workflow marker.
