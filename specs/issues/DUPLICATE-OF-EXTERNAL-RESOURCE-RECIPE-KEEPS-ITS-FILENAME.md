---
id: DUPLICATE-OF-EXTERNAL-RESOURCE-RECIPE-KEEPS-ITS-FILENAME
kind: issue
title: Duplicating a recipe whose query is only a resource outside the folder is refused
status: draft
priority: P3
complexity: S
area: [core/plan]
design:
created: 2026-11-24
github:
---
## Problem

`TreeEditor::duplicate` (`liquers-core/src/recipe_tree.rs`) renames the copy with the rewriter,
which changes a query's own filename and references to the node. A recipe whose query is only a
resource elsewhere, such as `-R/data/a.csv`, takes its filename from that key, so the copy still
produces `a.csv`; the check then reports two recipes producing `a.csv` and the duplicate is
refused ("two recipes produce 'a.csv'"). The recipes panel's Duplicate shows that message.

## What would fix it

Give the copy a filename segment of its own (`-R/data/a.csv/-/<copy>.csv`), or refuse earlier with
a message saying such a recipe cannot be copied under another name.
