---
id: DOCS-CHECK-FLAGS-A-CODE-SPAN-AS-A-MYST-ROLE
kind: issue
title: liquers-docs reports a code span ending in a brace as a MyST role
status: draft
priority: P3
complexity: S
area: [docs]
design: 
created: 2026-11-09
github:
---
## Problem

`myst_role` (`liquers-docs/src/neutral.rs`) looks for `{name}` immediately followed by a backtick
on the raw line, "before code spans are blanked". A code span whose text ends with `{name}` has
exactly that shape: its closing backtick follows the brace. The sentence

```markdown
In YAML, `${NAME}` stands for the environment variable `NAME`.
```

fails `liquers-docs check` with `a MyST role ({NAME}`…`) is renderer syntax`.

## Impact

A chapter cannot write the `${VAR}` notation of the configuration files, or any other code span
ending in `{word}`, as inline code. Workaround: reword so the span does not end with the brace
(`docs/guides/python/06-stores.md` §6.4 does).

## Expected behaviour

A `{name}` inside a code span is text. Only a `{name}` outside a span, followed by an opening
backtick, is a role.

## Discovery

Writing chapter 6 of the Python user guide, 2026-11-09.
