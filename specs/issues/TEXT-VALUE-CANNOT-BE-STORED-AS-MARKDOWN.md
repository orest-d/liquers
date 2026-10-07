---
id: TEXT-VALUE-CANNOT-BE-STORED-AS-MARKDOWN
kind: issue
title: A Text value cannot be stored with data format md
status: closed
priority: P2
complexity: S
area: [core/value]
design: text-value-markdown-format
created: 2026-09-27
github:
---
## Problem

`Value::type_descriptions()` (`liquers-core/src/value.rs`) registers `Text` with the formats
`TEXTUAL = ["txt", "html", "css", "js", "py", "rs", "json"]` plus `b`/`bin`/`bytes`. `md` is not
among them, and `DefaultValueSerializer::as_bytes` has no `"md"` arm either. So a `Text` value whose
metadata declares `data_format: "md"` is refused by the write path's hard validation
(`validate_metadata_hard` in `assets.rs`), and a `Text` value evaluated under a key ending in `.md`
cannot be serialized for storage. The media-type and icon tables do know `md`
(`media_type.rs`: `text/markdown`, `icons.rs`: document icon); only the value type does not.

## Impact

Markdown is the natural format for documents and notes — the agent memory MVP's corpus is
markdown (`specs/`, `.claude/skills/`) and its agent-written notes will be too. Today such content
can only be stored as `Bytes` with an undeclared data format (the extension then supplies `md`),
which works for serving but means commands receive bytes rather than text. Other text formats
with no entry (`yaml`, `toml`, `csv` as plain text, …) have the same problem.

## Expected behaviour

`Text` round-trips through any plain-text data format. Options: add `md` (and other textual
extensions) to `TEXTUAL` and to the `as_bytes` / `deserialize_from_bytes` arms; or give `Text` an
open-ended "any textual format" rule driven by the media-type table rather than a fixed list.

## Discovery

Working out the default type for `POST /api/assets/data` in Phase 2 of
`specs/design/axum-assets-endpoints/`, 2026-09-27.

## Resolution (2026-10-07)

Fixed by design `text-value-markdown-format`. `Text`'s `TypeInfo` declares `md` (on `Text` only,
not in the shared `TEXTUAL` list), and both base serializers (`liquers_core::value::Value` and
`liquers-lib`'s `SimpleValue`) write a `Text` as its bytes under `md` and read `md` back as `Text`.
Other types refuse `md`. The other plain-text formats this issue mentions (`yaml`, `toml`, `csv`)
already have structured readings and stay out of scope; an open textual rule belongs to
`DATA-FORMAT-CONSTANTS-AND-TOOLING`.

Evidence: `text_value_is_stored_and_read_as_markdown` (`liquers-core/tests/text_markdown_storage.rs`,
which fails before the fix with "Type 'Text' cannot be serialized as 'md'"), `markdown_is_text_only`
(`liquers-core/src/value.rs`), `simple_value_text_round_trips_as_markdown`
(`liquers-lib/src/value/simple.rs`).
