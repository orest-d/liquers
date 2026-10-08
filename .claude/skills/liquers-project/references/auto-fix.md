# Automatic-fix eligibility

Which issues and designs an agent may fix without a human approving the design first. Bulk design,
compaction and spin-offs label items with this rule. Fixing an eligible item still follows
`specs/guides/autonomous_issue_fixing.md`.

## The rule

An item is **eligible** when *all* of these hold:

1. **Size:** `complexity: S` or `M` (`DOCS_STRUCTURE_GUIDE.md` §4.5) — confirmed by Phase 2, not just
   copied from the filing guess.
2. **Kind of work:** one of
   - a bug fix that restores documented or plainly intended behaviour,
   - new or corrected unit or integration tests,
   - a documentation fix (`specs/`, doc comments, README, skills),
   - a tooling fix (`scripts/`, CI configuration, build settings, skill scripts).
3. **No new structure:** no new trait, and no new struct, enum, `ExtValue` variant or other data
   structure. Private helper functions are fine.
4. **No interface change:** nothing that a caller, a binding or a stored artifact can observe changes
   shape:
   - no change to a `pub` item's signature, and no new or removed `pub` item in `liquers-core`,
     `liquers-store`, `liquers-lib` or `liquers-records`;
   - no command added, removed or changed, so `specs/command_registry.yaml` does not change;
   - no change to query syntax, key encoding, serialized formats, config schema, HTTP routes or the
     Python / JavaScript bindings;
   - no new dependency and no new feature flag.
   A bug fix that changes observable *behaviour* back to the documented behaviour is not an interface
   change.
5. **Readiness `ready`:** the design (or, for an issue without one, the Phase 1-2 reasoning) has no
   blocking or open design question. A `needs-decision` item becomes eligible once its decisions are
   made.
6. **One crate.** The fix and its tests sit in one crate (a docs or tooling fix may also touch the
   specs index).

Priority does not change eligibility. It does change who may start the fix: the autonomous fixing
procedure covers only `P2` and `P3` by default, so an eligible `P0` or `P1` item is listed for the
user to authorize rather than started.

**If unsure, it is not eligible.** An ineligible item that turns out simple costs a review. An
eligible-labeled item that changes an API lands without one.

## Recording it

In the design's Phase 1 `## Design Readiness` section, directly after **Readiness**:

```markdown
- **Automatic fixing:** eligible — bug fix in `liquers-core/src/parse.rs`, no API change
```

or

```markdown
- **Automatic fixing:** not eligible — adds a `pub` method to `AsyncStore` (rule 4)
```

Name the rule that fails. Find eligible items with:

```bash
grep -l '^\- \*\*Automatic fixing:\*\* eligible' specs/design/*/phase1-high-level-design.md
```

Re-label whenever the design changes. Extending a design (triage cases 1-2) is the usual way an
eligible design becomes ineligible, which is why overlap exclusion E1 exists.

## Eligibility can be lost during the fix

If the fix turns out to need something rule 3 or 4 forbids, stop the automatic fix, re-label the item
`not eligible` with the reason, update the design, and hand it back through the normal gate. Never
widen an automatic fix to finish it.
