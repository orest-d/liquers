---
title: Introduction
status: outline
sources: [reference/PROJECT_OVERVIEW.md, guides/PYTHON_GUIDE.md]
reviewed:
---
# 1. Introduction

PURPOSE: The reader has run a first query from Python and knows the words query, command, store and asset.

```python
import liquers as lq

listing = lq.evaluate("commands_doc")       # Markdown: every command you can use
assert listing.startswith("# Commands")
```

EDITOR: copied from `liquers-py/tests/test_GUIDE_examples.py` (test_guide_example_runs (specs/guides/PYTHON_GUIDE.md §3)), which runs it; keep the two identical.

TBD: 1.1 What Liquers is: queries name computations; commands do the work; stores hold data; assets cache results.
SOURCE: reference/PROJECT_OVERVIEW.md
TBD: 1.2 Hello, world: the example below, then the same with a command of the reader's own (`@command`).
SOURCE: guides/PYTHON_GUIDE.md §3
TBD: 1.3 Assets: each result is an asset: value, status and metadata.
SOURCE: guides/PYTHON_GUIDE.md §3
TBD: 1.4 Next steps: which chapter to read for which task.
