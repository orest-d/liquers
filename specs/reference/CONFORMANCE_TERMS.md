---
title: Conformance Terms
kind: reference
audience: internal
area: [docs]
reviewed: 2026-09-02
---
# Conformance Terms

The vocabulary shared by every conformance suite in this project — the
[language integration guide](../guides/LANGUAGE-INTEGRATION_GUIDE.md) and the
[store implementation guide](../guides/STORE_IMPLEMENTATION_GUIDE.md) both use it.

Extracted so there is one definition rather than two. Two copies of a normative vocabulary drift
exactly as two copies of a format do, and the drift is silent until two designs report the same
word meaning different things.

## Requirement levels

- **Essential** — required for the thing to be useful at all.
- **Profile** — essential for some hosts or backends but not others. A guide names the condition.
- **Optional** — an independently selectable extension.

## Implementation states

- `NA` — intentionally not applicable, with a reason. See below; this is the only way a required
  item may go unwritten.
- `NS` — not started.
- `DESIGN` — design complete, implementation absent.
- `PARTIAL` — some required cases work.
- `COMPLETE` — all selected requirements implemented.
- `BLOCKED` — implemented, but a defect **outside** the thing under test stops a required check
  from passing. Must name the defect. This is not a softer `PARTIAL`: `PARTIAL` says the work is
  unfinished, `BLOCKED` says it is finished and something it depends on is not, and the two call
  for different work. Nothing may sit in `BLOCKED` without a filed issue, and blocked checks stay
  in the suite as named expected failures rather than being deleted — they are what will prove the
  fix.
- `CONFORMANT` — complete, and all required checks pass.

## The `NA` discipline

`NA` means **intentionally not applicable**, and because it is the only escape hatch it attracts
reasons that sound sufficient and are not. The default answer for a required item is *required*;
`NA` has to be argued.

**Every `NA` carries a reversing condition** — what would make it required again. Without one, an
`NA` written for a good reason at one milestone silently outlives that reason.

**`NA` is not a schedule.** Deferred work is `NS` or `PARTIAL`. A design that marks future work
`NA` loses the record that it was ever required.

**Difficulty of observation is not `NA`.** It calls for a mechanism — a debug counter, a stub, an
instrumented harness. A check whose assertion would pass with the defect present is worse than an
absent one, because it reports safety it never checked.

Each guide adds its own worked examples, because what makes a legitimate `NA` differs: a language
without an async model, or a store whose key space has no directories.

## How many `NA`s are too many

This is where the two guides deliberately differ, and the difference is worth stating rather than
leaving to be inferred:

- For a **language integration**, a feature with many `NA` tests is a warning. Its tests define what
  the feature *is*, so excusing most of them usually means the feature should not have been
  selected.
- For a **store**, many `NA`s are often correct. A store may be deliberately narrow — a view onto
  one database table, keyed by row ID, with no directories — and conform to a subset by design.

What keeps the second from becoming an excuse is the same discipline as the first: each one argued,
each one reversible.

## History

| Date | Change | Source |
|---|---|---|
| 2026-09-02 | Created, by extracting the requirement levels and implementation states from `LANGUAGE-INTEGRATION_GUIDE.md` §3 so the store implementation guide could share them rather than copy them. The `NA` principle is stated here; each guide keeps its own worked examples, which are too domain-specific to move. | `design/store-conformance-suite/` Phase 4 step 14 |
