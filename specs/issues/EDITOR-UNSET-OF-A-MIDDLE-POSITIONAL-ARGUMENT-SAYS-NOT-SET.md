---
id: EDITOR-UNSET-OF-A-MIDDLE-POSITIONAL-ARGUMENT-SAYS-NOT-SET
kind: issue
title: The query editor answers 'is not set' when asked to unset a positional argument that is followed by another
status: draft
priority: P3
complexity: S
area: [core/query]
design: 
created: 2026-11-14
github:
---
## Problem

`ActionEditor::unset(name)` (`liquers-core/src/pipeline/editor.rs`) on an argument that is given
positionally and is followed by another positional argument fails with a message that is not
true:

```python
ed = env.query_editor("numbers/top-2-t-1.5")     # top(value, n=1, descending=True, scale=1.0)
ed.edit_action(1).unset("descending")
# ParameterError: Argument 'descending' of 'top': is not set
```

`descending` is set: the query says `t`. Unsetting the last argument first (`scale`), and then
`descending`, works and gives `numbers/top-2`.

## Impact

Small. A caller who wants "back to the default" for a middle argument gets an error that sends
them looking for a different mistake. A form built on the editor (`unset_arguments`, the planned
parameter editor) will meet it as soon as a user clears a field that is not the last one.

Workaround: `set` the argument to its default value.

## Expected behaviour

Either of:

- `unset` of a middle positional argument writes the argument's default in its place (what
  `set` on a later argument already does for the unset ones before it), or
- it is refused with a message that says why: the argument is followed by another positional
  argument and a query cannot leave a gap.

## Discovery

Probing the editor for the user guides' section 3.9, 2026-11-14. The guides show `unset` on the
last set argument only.
