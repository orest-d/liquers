# Phase 3: Examples and Tests

Run in a scratch copy of the repo root, or with the scripts' target directory overridden if they
support one (check). Otherwise run in the repo and `git status` afterwards.

| # | Command | Expected |
|---|---|---|
| T1 | `python3 init_feature.py --help` | usage on stdout, exit 0, no new folder |
| T2 | `python3 init_feature.py -x` | usage error, exit 2, no folder |
| T3 | `python3 init_feature.py Bad_Name` | exit 2, message names the rule |
| T4 | `python3 init_feature.py tmp-slug-check` | folder created as before. Then delete it. |
| T5 | same as T4 again | refused (exists), as today |

There is no Python test harness for skills in the repo. Record T1–T5 output in the PR description.
