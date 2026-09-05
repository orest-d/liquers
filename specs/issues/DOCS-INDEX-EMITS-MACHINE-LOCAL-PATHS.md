---
id: DOCS-INDEX-EMITS-MACHINE-LOCAL-PATHS
kind: issue
title: The generated index.md emits design phase links in filesystem order
status: draft
priority: P2
complexity: S
area: [docs, build]
design:
created: 2026-09-04
github:
---

> **Partly fixed upstream (2026-09-04).** `e326a45` ("Fix docs index markdown links to be
> path-independent") changed the emitted link to `x.relative_to(REPO).as_posix()`, which resolves
> the absolute-path half described below. The `iterdir()` ordering half is unchanged and is what
> this issue now tracks; the original text is kept so the fixed half stays legible.


## Problem

`scripts/docs_index.py` builds the "Design" column of `specs/index.md` from an absolute filesystem
path and an unordered directory listing (`scripts/docs_index.py:447-452`):

The link target is the loop variable `x` itself — an absolute `Path` — interpolated straight into
the Markdown link, over an `iterdir()` of the design folder:

> `f"[{x.name.split('-')[0]}]" + "(" + str(x) + ") "`
> `for x in Path(designs[design]["_path"]).parent.iterdir()`
> `if x.name != "DESIGN.md" and x.name.endswith(".md")`

(shown split so this file's own link check does not read it as a link)

`x` is an absolute `Path`, so the emitted link is whatever the generating machine's checkout path
happens to be. At the commit before this issue was filed, `specs/index.md` carried 60+ links of the
form `C:\Users\orest\Documents\GitHub\liquers\specs\design\<slug>\phase1-high-level-design.md`;
regenerating on Linux rewrites every one of them to `/home/user/liquers/specs/design/...`. Neither
resolves for any other reader, and backslash-separated Windows paths are not valid Markdown link
targets at all.

`iterdir()` is also unordered, so the phase links come out in filesystem order — `phase4 phase3
phase1 phase2` on one machine, ascending on another.

Every other link in the file goes through `relative_specs_path()`, which is correct; this one
expression does not.

A related instability sits in the same output: the `reference`/`guide` block of `specs/index.csv`
re-sorts between machines (`ASSETS` before or after `ASSET_LIFECYCLE`, and the `reference/api/`
rows moving as a group), so a regeneration produces diff noise unrelated to the change being made.
That may be the same environment-dependence or a separate collation difference; it is recorded here
rather than split off because it has not been isolated.

## Impact

`specs/index.md` is a committed, generated file, so the damage is durable: the design links in it
are dead for everyone except the person who last ran the script, and every regeneration on a
different machine produces a large, meaningless diff that hides the real change. Because
`DOCS_STRUCTURE_GUIDE.md` §9 requires regenerating the index whenever a document is added, the noise
lands in unrelated PRs.

No behaviour of the library is affected; the harm is to navigation and to review.

## Expected behaviour

The phase links use the same repository-relative form as every other link in the file, and are
emitted in a deterministic order. `sorted(...)` over the directory listing and the existing
relative-path helper cover both.

Whether the `index.csv` block reordering is the same defect or a separate collation issue should be
determined before the fix; if separate, it is its own issue.

## Discovery

Found on 2026-09-04 while regenerating the index after adding the
`stale-dependency-status-finalization` design folder. The regeneration rewrote 60+ unrelated rows,
which is what made the absolute paths visible.
