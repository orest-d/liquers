---
id: QUERY-TOKEN-CLASSES-TRUNCATE-NON-ASCII
kind: issue
title: Action names, filenames and header values accept non-ASCII characters by low-byte truncation
status: draft
priority: P2
complexity: S
area: [core/query]
design:
created: 2026-10-21
github:
---

## Problem

`parameter-entity-escaping` removed the `c as u8` truncation from string parameters and resource
names, but four other productions in `liquers-core/src/parse.rs` still test a character by its low
byte: `identifier` (action names), `filename`, `header_parameter`, `full_transform_segment_header`
and `resource_segment_header` (lines 309-319, 476, 486-487, 532 at 18732dd). A code point whose low
byte is an ASCII letter or digit is accepted; any other non-ASCII character is not. Checked with
`liquers-validate --no-registry` at 18732dd — all four parse:

```text
Łoad-x            action name "Łoad"          (Ł is U+0141, low byte 0x41 'A')
load/ŁŁ.txt       filename "ŁŁ.txt"
-R-Ł/data/x.csv   resource header name "Ł"
-R-dŁ/data/x.csv  header parameter "dŁ"
```

`DOC_02` §Lexical forms says all four are ASCII. The parser test
`a15_non_ascii_is_not_part_of_the_grammar` (`parse.rs`) claims "a multi-byte char can never begin
or continue an action name", which is true for `é` (the case it checks) and false for `Ł`.

## Impact

Low in practice, but the accepted set is unpredictable, the code contradicts the reference, and any
tool that validates these tokens with a correct ASCII test (the `liquers-template` query
placeholders, the proposed `query-builder`) disagrees with the parser on them.

## Expected behaviour

The four productions use ASCII predicates (`char::is_ascii_alphabetic`, `is_ascii_alphanumeric`),
defined once in `liquers_core::escape` beside `is_resource_name_char`, so the parser and validators
share them. `Łoad-x` fails to parse at the `Ł`. This narrows the grammar to what `DOC_02` already
documents. It is not a syntax change.

## Discovery

Found on 2026-10-21 during bulk design of `RESOURCE-NAME-ASCII-ONLY` and `QUERY-BUILDER-TOOLING`,
while comparing the parser's character classes with the template validators. The fix is a bug fix
in one crate with no interface change, so it is a candidate for automatic fixing. It should land
before `design/query-builder/`, which reuses the predicates.
