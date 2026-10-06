# Phase 2: Solution and Architecture

## Change (each `init_feature.py`)

```python
import argparse, re

SLUG_RE = re.compile(r"^[a-z0-9]+(-[a-z0-9]+)*$")  # DOCS_STRUCTURE_GUIDE.md §2 "Naming"

def parse_args(argv=None):
    p = argparse.ArgumentParser(description="Create specs/design/<feature-name>/ with phase templates.")
    p.add_argument("feature_name", help="lowercase-kebab design slug, e.g. parquet-support")
    args = p.parse_args(argv)
    if not SLUG_RE.match(args.feature_name):
        p.error(f"'{args.feature_name}' is not a lowercase-kebab slug")
    return args
```

`main()` uses `parse_args().feature_name` instead of `sys.argv[1]`. `p.error` exits with 2.
`validate_phase.py`: same treatment for its `<feature-name> <phase>` arguments (phase as
`type=int, choices=…`), if it uses raw `sys.argv`.

## Known-issue preflight

None.

## Risk table

| Area | Assessment |
|---|---|
| Likely files | `.claude/skills/liquers-designer/scripts/init_feature.py`, `.claude/skills/liquers-project/scripts/init_feature.py`, possibly both `validate_phase.py` |
| Compatibility | Valid invocations unchanged |
| Recovery | Revert |
| Certainty | High |
