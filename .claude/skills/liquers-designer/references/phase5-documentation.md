# Phase 5: Documentation

**Not yet active.** `specs/DOCS_STRUCTURE_GUIDE.md` §5.2 reserves `documentation` at ordinal 5 but
marks it inactive until this phase is wired into the workflow. Activating it means one edit to that
table and a line in §5.4. Until then, run the steps below by hand when a design ships — the
obligation in §9.3 applies either way.

Phase 5 runs **after the implementation PRs are merged**, which is the one phase that does. A
design sits at `implemented` between the merge and this phase completing; it reaches `complete`
only when this phase is done.

## Why this phase exists

A reference document becomes wrong at the moment a change ships, and that is also the moment
someone still has the context to fix it. Waiting for the quarterly sweep (§9.4) guarantees a window
in which the documentation lies, and hands the correction to whoever has least context.

## Steps

### 1. Propose the affected set

List every document in `specs/reference/` and `specs/guides/` sharing an `area` with this design:

```bash
grep -l 'core/assets' specs/reference/*.md specs/guides/*.md
```

This is a **candidate list**, generated so nothing is missed through forgetting. The designer keeps
or discards each entry — the `area` overlap is a prompt, not a verdict.

### 2. Record the decision

Write the kept set into `DESIGN.md`:

```yaml
affects_docs: [reference/ASSETS.md, guides/COMMAND_REGISTRATION_GUIDE.md]
```

A discarded candidate needs no entry, but one line saying why is worth more than a silent omission
when someone revisits the design later.

### 3. Review each kept document against what shipped

**Against the code, not against the design.** The design says what was intended; the two routinely
differ. `specs/design/query-validation/` records four places where they did, and that folder is one
of the more carefully run efforts in the repository.

Read the document's claims and check each against the merged implementation. What changed
signature, what changed default, what is now possible that the document says is not.

### 4. Update, add a History row, bump `reviewed:`

All three in the **same commit**. Guide check 11 reads the diff and rejects a bumped `reviewed:`
with no History row bearing the new date:

```markdown
| 2026-08-08 | Reviewed against `design/expiration-safety/`; documented the stale-read guard. | phase-5 |
```

"Reviewed, no changes needed" is a perfectly good entry. It is a claim someone is accountable for,
which a bare date is not.

### 5. Close the design

Set `status: complete` and drop `phase`. Guide check 12 enforces the interlock: a design cannot
reach `complete` while a document in its `affects_docs` has a `reviewed:` earlier than the merge
date of its last PR.

Before closing the design, update every issue or feature completed by this work to `status: closed`
and add a concise resolution note with its evidence. Use `closed_not_planned` for work deliberately
not pursued. Follow `DOCS_STRUCTURE_GUIDE.md` §4.3: this local status is authoritative even when
the document carries `github:`.

Then update the capability map in `specs/README.md` — a capability that has just gained a
`reference/` document moves up a maturity stage, and its link should now point at the reference
rather than at this design folder (guide §8.1).

## What is mechanical and what is not

Steps 1, 2, 4 and 5 should happen without being asked. **Step 3 is the phase.** It cannot be
delegated to a checklist, and a phase 5 that skips it while performing the other four produces a
freshly stamped `reviewed:` date on a document nobody read — worse than an honestly stale one,
because it silences the quarterly sweep that would otherwise have caught it.
