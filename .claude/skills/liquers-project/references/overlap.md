# Overlap

When does a new problem (an issue, a feature or a scope change) belong with an existing issue or
design, and with which one? This file is the only definition. Triage, bulk design and compaction all
use it.

**Status: proposal.** The maintainer has not ratified it yet. Apply it, and when a call is close,
write down which test decided it so the definition can be tuned.

## Terms

- **New item (N):** the problem being triaged.
- **Candidate (X):** an existing issue or feature with `status` `draft`, `accepted` or `in_progress`,
  or an **open design** with `status` `draft`, `in_review` or `approved` (any phase). A design with
  `gh_pr` set whose PRs are still open counts as open but is *in implementation* (rule E3).
  `complete`, `superseded`, `abandoned`, `rejected`, `duplicate` and closed items are never
  candidates. If one of them describes N, N is a regression or a reopening: say so in N's body and
  link it.

## Finding candidates

You cannot judge overlap on a title. Read the candidate's Problem, Expected behaviour, and, for a
design, Phase 1 and the files and symbols named in Phase 2 and Phase 4.

1. `grep -i` in `specs/index.csv` for the symptom, the module, the symbols and the area. When you
   later file N, `python3 scripts/docs_index.py new` runs its own near-duplicate search and refuses
   on a match. Treat a refusal as a candidate to judge here, and never as something to override with
   `--force` before judging it.
2. `grep -rl` in `specs/design/*/phase2-architecture.md` and `phase4-implementation.md` for every
   file, type, function and command that N's fix would touch. A design that plans to edit the same
   symbol is a candidate even if its title says nothing about N.
3. Every item in N's `area`, at least as a skim.

## The four overlap tests

N **strongly overlaps** X when at least one of these holds:

| Test | Holds when | Example |
|---|---|---|
| **T1 Same root cause** | Fixing one would fix, or would have to change, the other. | Two issues report different wrong outputs that both come from one off-by-one in `parse.rs`. |
| **T2 Shared contract** | N's expected behaviour depends on a decision that X makes or still has open (or the other way round): the same public API, query syntax, serialized format, error behaviour or command signature. | N wants a new `to_csv` option; X is redesigning how `to_*` commands take options. |
| **T3 Same change site** | Both fixes would edit the same function, type, command registration or config schema, so doing them separately would conflict or force an order. Same *file* alone is not enough. | N and X both change `AssetManager::get` and its locking. |
| **T4 Subsumption** | X's acceptance criteria already cover N, or N's cover X. | N: "`ns-pl/head` panics on an empty frame"; X: "every `ns-pl` command must handle empty frames". |

N **weakly overlaps** X when none of T1-T4 holds but they share a prerequisite, the same area or
module with independent changes, or a similar symptom with a different cause. Weak overlap never
merges anything. It is recorded in Phase 1's `## Design Dependencies` (`requires`, `required-by` or
`overlaps`) and in N's body.

T4 in which X covers N completely means N is a duplicate or covered: X gets N as an issue
(triage case 2) or N gets `readiness: covered` pointing at X. It never gets a parallel design.

## Choosing the most suitable candidate

If several candidates strongly overlap, rank them in this order and take the first:

1. **Strength:** T1 beats T2, T2 beats T4, T4 beats T3. A root cause or contract is shared
   knowledge; a shared change site is only shared editing.
2. **Number of tests that hold:** more is stronger.
3. **Earliest phase:** a design still in `high-level` or `architecture` absorbs N with less re-review
   than one at `implementation`.
4. **Same area and crate** as N.
5. **Most recently updated.**

Record the winner, the tests that held and the runner-up in N's `## Discovery` section. A reviewer
correcting the choice needs to see why it was made.

## Exclusions: when strong overlap still does not merge

- **E1 Automatic-fix protection.** Never attach an item that is eligible for automatic fixing
  ([`auto-fix.md`](auto-fix.md)) to a design that is not, if the result would no longer be eligible.
  Keep N separate, fix it, and link both ways with `overlaps`. The reverse is fine: attaching an
  ineligible N to an ineligible X loses nothing.
- **E2 Size.** Do not attach when the merged scope would become `XL` and neither part is `XL` on its
  own. Link with `requires` / `overlaps` instead, and record that the size limit decided it.
- **E3 In implementation.** A design with an open implementing PR takes a new source only under T1
  (the PR is wrong without it) or when N is the current work's own scope change (triage case 1).
  Anything else gets its own design with `requires` / `required-by`.
- **E4 Frozen.** `complete`, `superseded` and `abandoned` designs are never extended. See
  `DOCS_STRUCTURE_GUIDE.md` §5.1.
- **E5 Explicit separation.** If a maintainer has written that two items stay separate, they stay
  separate.

## Recording the outcome

| Outcome | What changes |
|---|---|
| Strong overlap with the design in hand | Triage case 1: extend it; no new issue |
| Strong overlap with another open design | Triage case 2: file N, add it to that design's `issues:` (set `merged:` to today when it now has several sources), set N's `design:` |
| Strong overlap with an issue that has no design | Triage case 3: one new design owns both; leading source first |
| Weak overlap only | N gets its own design; the link goes in both Design Dependencies sections |
| Excluded by E1-E5 | As weak overlap, plus a line naming the exclusion |
