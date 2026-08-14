# Phase 5: Documentation - Parameter Entity Escaping

## Completion Preconditions

All met before this summary was written:

- [x] Steps 1-11 of the Phase 4 plan complete
- [x] `cargo test -p liquers-core --lib --tests` green in **both** feature states
- [x] `cargo clean`-independent wasm loop green: `cargo test -p liquers-web --target wasm32-unknown-unknown`
- [x] `cargo test -p liquers-lib --lib --tests` green **without changes**, `registry_export` included
- [x] `cargo test -p liquers-core --doc` green, so every documented spelling is asserted
- [x] Backward-compatibility corpus passes in both directions
- [x] `python3 scripts/docs_index.py --check` reports 0 errors
- [x] All maintainer decisions from Phases 1-4 incorporated

## Implementation Summary

### What was implemented

`PARAMETER-ESCAPING-INCOMPLETE` (P0) is closed. The query grammar's `~` escape gained two
variable-length entity forms and the encoder was rewritten on top of them, so **any string can be
carried in an action parameter**.

| Form | Meaning | Emitted |
|---|---|---|
| `~U<hex>~` | code point, hexadecimal | yes — canonical |
| `~D<dec>~` `~O<oct>~` `~B<bin>~` | the same, other radixes | no |
| `~n<name>~` | HTML5 named entity | curated names only |
| `~<opener>~<body>~` | any long form, separator tilde | no |

Two modules in `liquers-core`: `entities.rs` holds the named tables, `escape.rs` the character
classes, mnemonics, numeric codec, `match_entity`, `encode_token`, `segments` and the diagnostic
explainer. `parse.rs` keeps only nom plumbing — its ten entity parsers are deleted — so the encoder
and decoder derive from one definition and **cannot drift again**, which was the structural cause
the issue identified.

Diff: 17 files outside `specs/`, +4478 / −212.

### Against the issue's three defects

1. **`encode_token` is round-trip safe.** `decode_token(&encode_token(s)) == s` over ~4050
   generated inputs. `12:30` → `12~ncolon~30`, `café` → `caf~UE9~`, `日本` → `~U65E5~~U672C~`.
2. **Every Unicode character is representable**, and `encode_token` stays infallible.
3. **The `c as u8` truncation is replaced by a decision in each direction** — parameters widened to
   `char::is_alphanumeric()`, resource names narrowed to `is_ascii_alphanumeric()`.

## Deviations from the request, and why

| Deviation | Reason |
|---|---|
| **`entities.rs` holds named entities only**; `escape.rs` is new | The issue proposed one consolidated module; the maintainer reserved `entities.rs` for named entities. Each table is still defined once and used in both directions, which was the requirement |
| **`liquers-web` changed**, which Phase 1 said it would not | Found by enumerating `encode_token` callers. Its `encode.rs` was a second encoder existing only because of this defect, with a test asserting the defect still held |
| **`set_value` fixed here** rather than left to its own issue | It violated the same invariant and would have widened under the new encoder. The maintainer took both into scope |
| **`u16` offsets**, not `u32` | Measured: both blobs are far under 65 536, so the index halves — 28.1 KB against 36.4 KB |
| **A new guide**, reversing Phase 1's "no new guide" | No query guide existed, and `DOC_02` is a reference. The maintainer approved |

## Scope: requested, added, omitted

**Requested and delivered:** the two entity forms, arbitrary-string encoding, the truncation fix,
consolidation of encoder and decoder.

**Added:** the `set_value` fix; deletion of `liquers-web`'s encoder; `escape::segments` as a public
hook; a backward-compatibility corpus; a generated table with a freshness test.

**Omitted deliberately, and filed:**

- `RESOURCE-NAME-ASCII-ONLY` (P2, L) — non-ASCII resource names stay unaddressable. Resource names
  have no entity production, and giving them one would make `~U2F~` decode to `/`, which is path
  injection into `key_to_path` and compounds `STORE-FILESTORE-PATH-TRAVERSAL` (P0). The issue
  carries the option space the maintainer asked for.
- `QUERY-AST-DISCARDS-ENTITIES` (P3, L) — entities are decoded away, so the AST cannot show them.

## Issues filed and closed

| Issue | Status |
|---|---|
| `PARAMETER-ESCAPING-INCOMPLETE` | **closed** — with a resolution note against each of its three defects |
| `ACTION-PARAMETER-SET-VALUE-DOUBLE-ENCODES` | **closed** — filed in Phase 2, fixed here |
| `RESOURCE-NAME-ASCII-ONLY` | open, P2 |
| `QUERY-AST-DISCARDS-ENTITIES` | open, lowered P2 → P3 |

## Documentation Delivered

| Document | Change |
|---|---|
| `reference/api/DOC_02_QUERY_LANGUAGE_REFERENCE.md` | Normative entity table with an **Emitted** column, encoder priority order, the feature, the trap names, diagnostics. P1 `set_value` row struck. `reviewed:` bumped, History row added |
| `reference/PROJECT_OVERVIEW.md` | Raw-emission caveat narrowed; string parameters are no longer part of it. Bumped, History row added |
| `guides/QUERY_ESCAPING_GUIDE.md` | **New.** How to escape programmatically, write entities by hand, when the feature is needed, why a query stopped parsing |
| `guides/LANGUAGE-INTEGRATION_GUIDE.md` | Refusal path dropped; OBJECT09 becomes a round-trip test. Bumped, History row added |
| `README.md` | Capability-map entry and a common-task row |

Every one of the guide's 11 example queries was run through `liquers-validate`.

## Important Learning

**The prototype paid for itself three times.** Running the encoder in Python over 4130 inputs
before writing Rust settled the priority order and the tie-breaks, and the port passed its property
tests on the first run. It also caught a wrong octal constant in the design document
(`~O11746~` for U+1F600; correct `~O373000~`) — hence the rule that every radix example is
generated from the code path it documents, and the `escape.rs` doc examples are `assert_eq!`
doctests.

**Measuring beat remembering, repeatedly.**

- `f-~Hexampledotcom~~` parses at HEAD and means `https://exampledotcom~`. That single measurement
  is why named entities need the `~n` prefix: a bare `~<name>~` form would have silently changed an
  existing query's meaning.
- The old `E = encode ∘ parse` was not merely non-idempotent, it was **undefined**:
  `f-~Hapi.example.com~/data` re-encoded to `f-https:~/~/api.example.com~/data`, which does not
  parse. The design did not improve idempotence; it made it exist.
- Error positions were verified before being designed around: nom reports the span the failure was
  raised with, exactly. `cut` does *not* choose it — it preserves the inner parser's span — so the
  entity parser captures the span at the opening `~`.
- The HTML5 table was measured offline from Python's standard library rather than fetched: 2125
  names, 93 of them multi-character, and **no name for ASCII `~` or `-`** — confirming the two traps
  rather than trusting them.

**The implementation caught a documentation error.** `hellip` is curated (Annex B tier B-2), so the
design's running example of a name needing `entities-html5` was wrong throughout. `check` (U+2713)
is genuinely outside the curated set. A test asserting the *absence* of a name is what surfaced it.

**A scraped corpus contains negative cases.** 9 of the 156 `parse_query` literals in the workspace
were queries the tests assert *fail*. Classifying them with a `liquers-validate` built from the
pre-change commit — rather than reading them — turned a broken test into a stronger one: both
halves are now asserted, and an entity change that made a malformed link *start* parsing would be
caught.

**A pre-scan is a parser too.** The P1 finding above is the sharpest lesson here: the design's whole
premise is that one function decides what an entity is, and a lexical guard that merely *counts*
markers looked exempt. It was not, and the cost was a reintroduced DoS. Anything that walks query
text looking for `~` belongs on `match_entity`.

**One of my own claims was too strong.** `parse(encode(t)) == parse(t)` is false for the
resource/transform shorthand: `a/b/-/c/d` re-encodes to `-R/a/b/-/c/d` and
`ResourceQuerySegment`'s equality includes the header. Pre-existing and already documented in
`parse.rs`. The test now asserts what holds — canonical text round-trips — with the shorthand
exception pinned separately.

## Validation

| Check | Result |
|---|---|
| `cargo test -p liquers-core --lib --tests` | 547 + all integration suites, green |
| the same `--features entities-html5` | 548 + all, green |
| `cargo test -p liquers-core --doc` | 11 green |
| `cargo test -p liquers-lib --lib --tests` | 297 + `registry_export`, green **unchanged** |
| `cargo test -p liquers-web --target wasm32-unknown-unknown` | whole suite green under Node |
| `python3 scripts/docs_index.py --check` | 0 errors |
| Backward-compatibility corpus | 147 still parse, 9 still rejected |

## Post-review corrections

Three findings from the automated review on PR #34, all legitimate, all fixed on the branch.

**P1 — the new entities could defeat the link depth guard.** `link_bounds_exceeded` is a lexical
pre-scan that bounds link nesting *before* parsing, because parsing is exponential in depth. It
honoured `~~` but knew nothing about the long entities, so in `~U26~E` it saw the entity's closing
tilde followed by `E` and counted a link terminator the parser does not. One such value per level
kept its depth at zero while the real nesting grew.

Measured on the broken build before fixing:

| Real nesting | Query length | Parse time |
|---|---|---|
| 8 | 113 B | 17 ms |
| 16 | 225 B | 4.4 s |
| 20 | 281 B | **84 s** |

That is the denial-of-service `MAX_LINK_DEPTH` exists to prevent, reintroduced by this change and
missed by every test written for it. The scan now skips entities as whole units via
`escape::match_entity` — **the same function the parser uses**, so the two cannot disagree about
where a `~` belongs. The `~~` special case is gone, subsumed by the mnemonic table.

The general lesson is the one this design was built on and still got wrong in one place: *any*
component that reasons about `~` must go through the single matcher. The pre-scan was easy to
overlook because it is not a parser.

**P2 — `decode_token` accepted literals the parser rejects.** It advanced over any non-`~`
character, so `":"` and `"a/b"` decoded "successfully" despite being invalid or structural in a
real query — contradicting its documented contract as the exact inverse of what the parser accepts.
Literal runs are now validated against `is_unescaped_parameter_char`.

**P2 — an unreachable diagnostic branch.** `unknown_name_error` tested the name to choose between
"enable `entities-html5`" and "unknown name", but it is only reached after `lookup` returned
`None`, so the test was always true and a full-feature build still recommended a feature it already
had. The discriminator is the *build*, not the name, so it is now `#[cfg]`.

## Conformance and Remaining Work

The implementation conforms to the approved design, with the five deviations recorded above — each
taken deliberately and, in four of five cases, at the maintainer's direction. Nothing in Phases 1-4
was silently dropped.

### Follow-up

- `RESOURCE-NAME-ASCII-ONLY` needs a design folder (complexity `L`) and a decision on its two
  questions before implementation.
- `QUERY-AST-DISCARDS-ENTITIES` may be closable without an API change: `styled_tokens` can call the
  now-public `escape::segments` on the encoded form to highlight entities, at the cost of showing
  canonical rather than typed spelling. That question is recorded in the issue.
- `QUERY-BUILDER-TOOLING` is unblocked on its encoder half — a builder can now be correct.
