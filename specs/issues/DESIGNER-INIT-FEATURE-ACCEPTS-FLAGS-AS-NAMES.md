---
id: DESIGNER-INIT-FEATURE-ACCEPTS-FLAGS-AS-NAMES
kind: issue
title: Designer init_feature.py treats any argument, including --help, as a feature name
status: draft
priority: P3
complexity: S
area: [docs]
design: 
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
