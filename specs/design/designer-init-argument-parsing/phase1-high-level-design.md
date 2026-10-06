# Phase 1: High-Level Design

## Design Readiness

- **Readiness:** ready
- **Leading issue:** None
- **Explanation:** A tooling fix with an obvious contract: `--help` prints usage, and an invalid
  slug is refused. The issue names `argparse`.
- **Open questions:** None

## Problem

`.claude/skills/liquers-designer/scripts/init_feature.py` takes `sys.argv[1]` as the feature name.
`init_feature.py --help` creates `specs/design/--help/` with the templates. **Found while
designing:** `.claude/skills/liquers-project/scripts/init_feature.py` (the current skill) has the
same `sys.argv` handling (`len(sys.argv) != 2` then `sys.argv[1]`), so it should be fixed too.
`validate_phase.py` in both skills should be checked for the same pattern.

## Expected behaviour and acceptance

For each affected script:

1. `--help` / `-h` prints usage and exits 0 without writing anything.
2. A name that is not a lowercase-kebab slug (`^[a-z0-9]+(-[a-z0-9]+)*$`) is refused with exit
   code 2 and a message. That covers a leading `-` and uppercase.
3. An existing folder is refused, as today (verify the current behaviour, keep it).
4. A valid slug behaves exactly as today (same files, same content).

## Scope

Both skills' `init_feature.py`, plus `validate_phase.py` if it has the same issue. Skill text
changes only if the usage line changes.

## Design Dependencies

None.

## Documentation assessment

- Skills' `SKILL.md`: the usage lines are already `init_feature.py <feature-name>`, so they are
  unchanged.
- `DOCS_STRUCTURE_GUIDE.md` §2 "Naming" already defines lowercase-kebab. The script cites it in its
  help text.

## Consolidated Findings

- The slug regex comes from the naming rule. `scripts/docs_index.py` might already validate design
  folder names. If it exports a pattern, reuse it by import only if the scripts can import it
  without path hacks (they live under `.claude/`), otherwise duplicate the one-line regex with a
  comment.
