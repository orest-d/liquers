---
id: SPECS-FRONT-MATTER-IS-NOT-VALID-YAML
kind: issue
title: Nine specs documents have front-matter that a YAML parser rejects
status: draft
priority: P3
complexity: S
area: [docs]
design: front-matter-yaml-agreement
created: 2026-10-19
github:
---
# Nine specs documents have front-matter that a YAML parser rejects

## Problem

`scripts/docs_index.py` reads front-matter as flat `key: value` lines, so it accepts titles that
YAML does not: an unquoted value beginning with a backtick, or containing `: `. Nine documents
have such a `title:` (line 3):

- `specs/design/store-guide-status-table/DESIGN.md`
- `specs/issues/CORE-ERROR-PAYLOAD-SIZE.md`
- `specs/issues/CORE-RECIPES-EXPAND-PREDECESSORS-CRASH.md`
- `specs/issues/CORE-STATE-LOCK-API-CLEANUP.md`
- `specs/issues/CORE-STORE-OPENBIN-MISSING.md`
- `specs/issues/CORE-VALUE-INTERFACE-CAPABILITY-SPLIT.md`
- `specs/issues/EVALUATION-SESSION-VOLATILE-TEST-INTERMITTENT.md`
- `specs/issues/MACRO-QUERY-VALIDATION-AND-HINTS.md`
- `specs/issues/REGISTER-COMMAND-PAYLOAD-STATEMENT-UNDOCUMENTED.md`

Rechecked at main 18732dd (bulk design, 2026-10-20): `yaml.safe_load` still rejects exactly these
nine. Found by the completed design `agent-memory-mvp`; designed in `front-matter-yaml-agreement`.

## Problem Example

`title: Store guide status table: generator or honest hand maintenance` passes
`docs_index.py --check` but YAML fails with "mapping values are not allowed here". Quoting it
today would put the quotes into `index.csv`, because `parse_front_matter` does not unquote — the
fix needs both halves.

## Impact

Any YAML-based reader loses these documents' fields. The store's `front_matter` metadata handler
fills nothing for them, so their L0 falls back to the first heading. `ns-mem` reads them leniently
(as `docs_index.py` does) and `memory_check` reports a `front_matter` warning for each.

## Expected behaviour

Quote the titles, and have `docs_index.py --check` reject front-matter that YAML cannot parse, so
the two readers agree.

## Discovery

`liquers-agent check` over the "liquers-specs" profile (agent-memory-mvp Step 11).
