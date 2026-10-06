---
id: DESIGNER-INIT-FEATURE-ACCEPTS-FLAGS-AS-NAMES
kind: issue
title: Designer init_feature.py treats any argument, including --help, as a feature name
status: closed
priority: P3
complexity: S
area: [docs]
design: designer-init-argument-parsing
created: 2026-09-27
github:
---
## Problem

`.claude/skills/liquers-designer/scripts/init_feature.py` takes `sys.argv[1]` as the feature
name without parsing options or validating the name. `init_feature.py --help` does not print
usage: it creates `specs/design/--help/` with all five template files and reports success.

## Impact

Small. Someone (or an agent) probing the script for its usage leaves a junk design folder behind
that must be deleted by hand, and would be indexed by `scripts/docs_index.py` if not noticed.

## Expected behaviour

`--help` / `-h` prints usage and exits without writing anything; a name that is not a valid design
slug (lower-case kebab, no leading `-`) is refused with a non-zero exit. `argparse` gives both.

## Discovery

Running `init_feature.py --help` while starting the `axum-assets-endpoints` design, 2026-09-27.

## Resolution (2026-10-06)

Fixed by `design/designer-init-argument-parsing/`. Both skills' `init_feature.py`
(`liquers-designer`, `liquers-project`) and both `validate_phase.py` now parse their arguments
with `argparse` and check the feature name against the lowercase-kebab rule
(`^[a-z0-9]+(-[a-z0-9]+)*$`, `DOCS_STRUCTURE_GUIDE.md` §2). `validate_phase.py` takes the phase as
`type=int` with `choices` (1-4 for `liquers-designer`, 1-5 for `liquers-project`).
`liquers-project`'s 60-character limit is kept.

Evidence, run in a scratch directory with an empty `specs/design/`, for each skill: `--help` prints
usage and exits 0 with nothing written; `-x` and `Bad_Name` exit 2 with a usage error;
`tmp-slug-check` creates the folder as before; a second run is refused because the folder exists
(exit 1, as before); `validate_phase.py tmp-slug-check 9` exits 2.
