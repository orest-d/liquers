# Phase 3: Examples and Tests

There is no test module for `docs_index.py` today. Check for `scripts/test_*.py`. If none, add
`scripts/test_docs_index.py` (stdlib `unittest`, runnable with `python3 -m unittest
scripts/test_docs_index.py`).

| # | Input | Expected |
|---|---|---|
| T1 | a fenced python block containing a string that concatenates an opening bracket-label-paren, a variable, and a closing paren | no link found |
| T2 | an inline code span whose content is a link to `missing.md` | no link found |
| T3 | a double-backtick span with a backtick inside | blanked correctly |
| T4 | a link to `missing.md` outside code | reported |
| T5 | an unclosed backtick, then a link to `missing.md` | reported (literal text) |
| T6 | `~~~` fence | blanked |
| T7 | full tree `--check` | same error count as before the change (0) |

The inputs are described rather than quoted. Until this design is implemented, quoting them would
fail this repository's own link check, which is the defect being fixed. The test file builds them
as Python strings.
