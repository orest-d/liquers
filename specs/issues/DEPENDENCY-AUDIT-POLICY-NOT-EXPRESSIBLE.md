---
id: DEPENDENCY-AUDIT-POLICY-NOT-EXPRESSIBLE
kind: feature
title: There is no way to say when dependency versions should be verified, so the strict and the exploratory workflow cannot both be served
status: draft
priority: P2
complexity: M
area: [core/assets]
design:
created: 2026-09-05
github:
---

## Problem

Verifying that a dependent's recorded dependency versions still hold is a **policy** question — how
strictly, and when — and Liquers has nowhere to express it. Verification, where it happens at all,
is fused into `DependencyManager::add_dependency`, which runs on the hot path wherever an edge is
registered. There is no entry point that means "check this now", no setting that means "do not
check", and therefore no way for two legitimate workflows to coexist:

- **The strict service.** Every guarantee available, within reason, that a served value is valid.
  Verify on load, perhaps on every `get`.
- **The long exploratory calculation.** A user runs a large computation with sizeable intermediate
  files, then deletes some of the intermediates by hand — today manually, in future through a
  dedicated operation. The end result stays technically valid, and they want to keep it and build
  on it. This is a real, practised workflow, not a hypothetical.

The second workflow has two shapes, and they need different things:

- **Metadata kept, data deleted.** Already works, and does so by accident of a good decision:
  a version is read from *metadata*, never from the value, so the authority can still answer for a
  key whose data is gone. Worth protecting explicitly — an "optimization" that read the value to
  compute a version would break it.
- **Metadata deleted too.** No version can be produced, so a strict check concludes the dependency
  is not durable and expires the dependent. Correct under one policy and wrong under the other.
  Only an explicit policy can tell them apart.

## Impact

Not a defect today, because nothing verifies: recorded versions are all zero
(`DEPENDENCY-RECORD-VERSION-CAPTURED-BEFORE-DEPENDENCY-EVALUATES`), so the comparison never runs.
It becomes live the moment records carry real versions, which is what `keyed-expiry-cascade-fix`
does — and at that point the absence of a policy is a decision made by default rather than on
purpose.

P2 as a feature: nothing is broken, one workflow is simply unserved, and the cost of waiting is
that the seam is harder to add later than now.

## Expected behaviour

Separate the two jobs that are currently one:

- **recording** an edge and the version observed for it — in memory, always, no I/O;
- **verifying** that recorded versions still hold — potentially a store read, on demand.

Verification then lives behind named operations rather than happening implicitly:

```rust
async fn trigger_dependency_audit(&self, query: &Query) -> Result<AuditReport, Error>;
async fn trigger_dependency_audit_all_registered(&self) -> Result<AuditReport, Error>;
```

and the policy is simply **who calls them, and when** — at startup, on every asset request, on an
explicit user action, or never. A default of "never" reproduces today's behaviour exactly.

Open questions this leaves: whether the policy is per-environment, per-key or per-recipe; whether
an audit expires what it finds or only reports it (a report-only mode is what makes it usable as a
diagnostic); and how deep a transitive audit should descend by default.

## Discovery

Raised by the project owner on 2026-09-05 while reviewing `keyed-expiry-cascade-fix` Phase 2
Revision 2, as a question about whether the design already contained such a seam. It did not. The
design now separates recording from verification and introduces the audit entry points so the
policy can be added without reopening the dependency manager; this issue carries the policy
vocabulary itself.
