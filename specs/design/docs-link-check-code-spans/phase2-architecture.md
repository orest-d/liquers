# Phase 2: Solution and Architecture

## Function

```python
FENCE_RE = re.compile(r"^ {0,3}(`{3,}|~{3,})")

def blank_code(text: str) -> str:
    '''Return text with fenced blocks and inline code spans replaced by spaces (newlines kept),
    so link-shaped text inside code is not mistaken for a link. Indented code blocks are not
    recognized (they are indistinguishable from list continuations without a full parser).'''
```

Algorithm:
1. Walk lines. On a fence opener, remember its character and length. Blank lines until a closing
   fence of the same character with at least that length (the fence lines too).
2. Outside fences, scan each line for backtick runs. A run of length *n* opens a span that closes at
   the next run of exactly *n*. Blank the span including its delimiters. An unclosed run is
   literal text.

`relative_link_errors` applies `RELATIVE_LINK_RE.finditer(blank_code(text))`.

## Known-issue preflight

None.

## Risk table

| Area | Assessment |
|---|---|
| Likely files | `scripts/docs_index.py`; the docs contract sentence |
| Risk | Over-blanking hides a real dead link (e.g. an unbalanced backtick swallowing a line). Mitigated by the closing rule (an unclosed run is literal) and per-line inline scanning. |
| Performance | Linear |
| Certainty | High |
