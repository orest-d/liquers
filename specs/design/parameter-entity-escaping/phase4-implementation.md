# Phase 4: Implementation Plan - Parameter Entity Escaping

## Overview

Eleven steps in four groups: build the tables (1-2), build the codec (3-4), switch the grammar over
and remove the two old encoders (5-8), then tests and documentation (9-11). Steps 5-8 are **one
atomic change set** — separating them leaves the workspace red, because `liquers-web` carries a test
that asserts the defect still exists.

Everything lands in `liquers-core` except step 8. No dependency is added, no command changes, and
`specs/command_registry.yaml` is untouched.

## Measurements settled before implementation

Phase 3 carried three items forward. Two are now answered, offline, against the **real** HTML5 table
(`python3 -c "from html.entities import html5"` — the standard library ships it, so no network is
needed at any point):

| Estimate | Measured | Verdict |
|---|---|---|
| ≈2100 names | **2125** distinct semicolon-terminated names (2231 entries including legacy no-semicolon forms) | confirmed |
| ~40 KB blob + offsets | **36.4 KB** with `u32` offsets, **28.1 KB** with `u16` | confirmed, and improvable |
| ~95 KB slice-of-pairs | **86.2 KB** | confirmed; the blob is 2.4× smaller (3.1× with `u16`) |

**Refinement adopted:** `u16` offsets, not `u32`. The name blob is 14 004 bytes and the value blob
6 216, both far under 65 536, so `u16` is safe and halves the index — 28.1 KB against 36.4 KB. A
generator assertion pins it: if a blob ever exceeds `u16::MAX` the generator fails rather than
silently truncating.

Two further facts the same measurement produced, both of which the design depends on:

- **93 names decode to more than one character** (`NotEqualTilde`, `NotGreaterFullEqual`, …). The
  decode table handles them because values are a blob; `CURATED_BY_CHAR` correctly cannot, and does
  not need to, since the encoder works one `char` at a time.
- **Longest name is 31 characters** (`CounterClockwiseContourIntegral`), which bounds the body
  length a diagnostic may quote.

### The two traps, verified rather than remembered

| Name | Actual value | The ASCII character it is mistaken for |
|---|---|---|
| `tilde` | U+02DC `˜` | `~` U+007E |
| `Tilde` | U+223C `∼` | `~` U+007E |
| `hyphen`, `dash` | U+2010 `‐` | `-` U+002D |

And the reverse lookup over the whole table confirms **no HTML5 name exists for `~` or `-`**, while
**all 26 ASCII punctuation characters the grammar rejects do have names** — which is what Annex B
tier B-1 asserts.

## Implementation Steps

### Step 1 — Generate and vendor the entity data

**Files:** `liquers-core/data/entities.json` (new), `liquers-core/src/bin/generate_entities.rs` (new),
`liquers-core/Cargo.toml`

Vendor the table as JSON, then generate Rust from it. Generating *from the vendored file* rather
than from Python keeps the generator reproducible and reviewable, and matches the
`export-command-registry` precedent.

```bash
# One-off, offline — the standard library ships the HTML5 table.
python3 -c "
import json
from html.entities import html5
names = sorted(set(k[:-1] for k in html5 if k.endswith(';')))
json.dump({n: html5[n+';'] for n in names}, open('liquers-core/data/entities.json','w'),
          ensure_ascii=False, indent=0, sort_keys=True)
"
```

`Cargo.toml` gains the feature and an explicit `[[bin]]` block with `required-features = ["cli"]`,
mirroring the `liquers-validate` block that already exists for exactly this reason:

```toml
[features]
default = ["async_store"]     # entities-html5 deliberately absent (D7, D12)
entities-html5 = []

[[bin]]
name = "generate-entities"
path = "src/bin/generate_entities.rs"
required-features = ["cli"]
```

**Validation:** `cargo check -p liquers-core --features cli`; the JSON has 2125 keys.

**Agent:** haiku · skills: none · knowledge: this step, `liquers-core/Cargo.toml`.

### Step 2 — `entities.rs`: tables and lookup

**File:** `liquers-core/src/entities.rs` (empty placeholder, 0 lines)

Generated data goes in `liquers-core/src/entities_data.rs`, included by `entities.rs`, so the
hand-written and generated halves stay separable in review.

Implements `EntityTable` with `u16` offsets, `CURATED`, `#[cfg(feature = "entities-html5")]
HTML5_EXTRA`, `CURATED_BY_CHAR`, and the four public functions from Phase 2: `lookup`,
`curated_name`, `is_curated`, `compiled_count`.

The curated selection is **Annex B**, encoded as a name list in the generator so the frozen set is
one reviewable literal. Latin-1 accented letters are excluded (D9).

**Validation:**
```bash
cargo test -p liquers-core --lib entities::
cargo test -p liquers-core --lib entities:: --features entities-html5
```
plus the generator round-trip test (step 9) and an assertion that every blob offset fits `u16`.

**Agent:** sonnet · skills: rust-best-practices · knowledge: Phase 2 "Data Structures", Annex B.

### Step 3 — `escape.rs`: character classes, mnemonics, numeric codec

**File:** `liquers-core/src/escape.rs` (new)

`Radix`, `Mnemonic`, `MNEMONICS`, `TokenSegment`, `is_unescaped_parameter_char`,
`is_resource_name_char`, and the numeric encode/decode. Canonical numeric form is **uppercase hex,
no leading zeros** — pinned by a test, since it is a compatibility surface.

**Validation:** `cargo test -p liquers-core --lib escape::`

**Agent:** sonnet · skills: rust-best-practices · knowledge: Phase 2 "Data Structures" and
"Function Signatures", Phase 1 D5/D6/D9.

### Step 4 — `escape.rs`: `match_entity`, `encode_token`, `segments`, `explain_entity_error`

**File:** `liquers-core/src/escape.rs`

The four-step priority encoder, the single decode matcher including D13's optional separator, the
segment iterator, and the diagnostic explainer.

**The prototype is the reference implementation.** `specs/design/parameter-entity-escaping/prototype.py`
implements exactly this logic and its expected outputs are already in the Phase 3 tables; port it,
do not re-derive it.

**Validation:** `cargo test -p liquers-core --lib escape::` — including P1, P2, P3 (step 9).

**Agent:** sonnet · skills: rust-best-practices · knowledge: Phase 2 "Function Signatures" and
"Error Handling", Phase 3 T1/T2, `prototype.py`.

### Step 5 — Declare the modules

**File:** `liquers-core/src/lib.rs` (module list at `:118-139`)

```rust
pub mod entities;
pub mod escape;
```

**Validation:** `cargo check -p liquers-core`

**Agent:** haiku · knowledge: this step.

### Step 6 — `parse.rs`: delegate and delete

**File:** `liquers-core/src/parse.rs`

| Location | Change |
|---|---|
| `:329-338` `resource_name` | use `escape::is_resource_name_char` (narrows, D10) |
| `:339-342` `parameter_text` | use `escape::is_unescaped_parameter_char` (widens, D6) |
| `:345-385` | **delete** all ten entity parsers |
| `:386-400` `entities` | capture the span at the opening `~`, call `escape::match_entity`, raise `nom::Err::Failure` with **that** span and `ErrorKind::Escaped` on error |
| `:998` `describe_query_failure` | add `ErrorKind::Escaped => escape::explain_entity_error(e.input)` |
| `:40-60` module doc | new entity table, with the "emitted by encoder" column |
| `:117` module doc | the `~X~` round-trip hole is closed — `~` is now escaped |

**Two things to get right, both verified in Phase 2/3:** `cut` does *not* choose the reported
position (it preserves the inner parser's span), so the failure must be constructed with the
captured span; and `ErrorKind::Escaped` is produced by no combinator this file uses, which a comment
must state as the existing `Verify`/`Fail` comments do.

**Validation:** `cargo test -p liquers-core --lib parse::`

**Agent:** sonnet · skills: rust-best-practices · knowledge: Phase 2 "Error Handling", Phase 3 T3-T5,
`parse.rs` in full.

### Step 7 — `query.rs`: re-export and restore the invariant

**File:** `liquers-core/src/query.rs`

| Location | Change |
|---|---|
| `:503-530` | delete `encode_token`'s body; `pub use crate::escape::encode_token;` |
| `:565` `new_string` | add a doc comment stating it stores the **decoded** value |
| `:614` `set_value` | `*self = Self::String(value.to_owned(), Position::unknown())` — the invariant |
| `:672` `PartialEq` | replace the `_ => false` arm with the two explicit cross-variant arms (project rule; drive-by while the file is open) |
| `:3244-3262` | existing `encode_token` tests updated to the new canonical spellings |

**Validation:** `cargo test -p liquers-core --lib query::`

**Agent:** sonnet · skills: rust-best-practices · knowledge: Phase 2 "The value/encoding invariant".

### Step 8 — `liquers-web`: delete the workaround

**File:** `liquers-web/src/encode.rs`

| Location | Change |
|---|---|
| `:1-33` module doc | rewritten — the limitation is gone; delegation is the design |
| `:42` `UNESCAPED_EXTRA` | delete |
| `:48` `encode_param` | `Ok(encode_token(text))`, keeping the empty-string rejection (a grammar fact, not this defect) |
| `:91` `unencodable` | delete |
| `:196` `web_core_encode_token_still_produces_unparseable_text` | **delete** — it asserts the defect |
| `encode_param_matches_the_parser` | rewrite as I2 |

**Steps 5-8 must land together.** After step 7 the `liquers-web` test fails by design; that is the
signal the fix landed, not a regression to investigate.

**Validation:** `cargo clean && cargo test -p liquers-web --target wasm32-unknown-unknown`

**Agent:** sonnet · skills: rust-best-practices · knowledge: Phase 2 "Integration Points",
`liquers-web/src/encode.rs` in full, `liquers-web/README.md` test loops.

### Step 9 — Tests

**Files:** inline `#[cfg(test)]` in `entities.rs`/`escape.rs`;
`liquers-core/tests/action_parameter_invariant.rs`, `entity_roundtrip.rs`,
`query_backward_compatibility.rs` (all new)

T1-T8, P1-P3, I1-I3 from Phase 3. The property corpus mirrors `prototype.py`'s **character pool and
adversarial list**, not its seed — Python's Mersenne Twister and any Rust PRNG produce different
sequences from the same number, so "same seed" would be a false promise. What transfers is the
*shape* of the corpus and the ability to paste a failing Rust input straight into the prototype to
see whether the design or the implementation is wrong. The Rust generator uses a fixed seed of its
own so its own failures are reproducible.

**T8's corpus** is collected mechanically — `rg` for query-shaped literals across `**/*.rs`,
`**/*.md`, `**/recipes.yaml` — and checked in as a fixture, so it is reviewable and stable rather
than re-scraped per run.

**Validation:**
```bash
cargo test -p liquers-core --lib --tests
cargo test -p liquers-core --lib --tests --features entities-html5
cargo test -p liquers-lib --lib --tests            # unchanged; registry_export must stay green
```

**Agent:** sonnet · skills: liquers-unittest, rust-best-practices · knowledge: Phase 3 in full,
`prototype.py`.

### Step 10 — Documentation

**Files:** `specs/reference/api/DOC_02_QUERY_LANGUAGE_REFERENCE.md`,
`specs/reference/PROJECT_OVERVIEW.md`, `specs/guides/QUERY_ESCAPING_GUIDE.md` (new),
`specs/guides/LANGUAGE-INTEGRATION_GUIDE.md`, `specs/README.md`

Per Phase 2 "Documentation Architecture". Each changed reference/guide gets a `## History` row and a
`reviewed:` bump (§9.2); `specs/README.md` gains the guide and the design folder.

**Every radix and spelling example is generated**, never typed — Phase 3 caught a hand-written
octal constant (`~O11746~` for U+1F600; correct `~O373000~`) in the design itself. The `escape.rs`
doc examples are `assert_eq!` doctests so a wrong constant fails the build.

**Validation:** `python3 scripts/docs_index.py --check`; `cargo test -p liquers-core --doc`

**Agent:** sonnet · knowledge: Phase 2 "Documentation Architecture", `DOCS_STRUCTURE_GUIDE.md` §9.

### Step 11 — Close the issues

**Files:** `specs/issues/PARAMETER-ESCAPING-INCOMPLETE.md`,
`specs/issues/ACTION-PARAMETER-SET-VALUE-DOUBLE-ENCODES.md`

Both → `status: closed` with a resolution note (§4.3). `RESOURCE-NAME-ASCII-ONLY` and
`QUERY-AST-DISCARDS-ENTITIES` stay open — they are the deliberate remainder. Then Phase 5.

**Validation:** `python3 scripts/docs_index.py && python3 scripts/docs_index.py --check`

**Agent:** haiku · knowledge: `DOCS_STRUCTURE_GUIDE.md` §4.3.

## Testing Plan

| When | Command |
|---|---|
| After each of steps 2-7 | `cargo test -p liquers-core --lib` |
| After step 8 | `cargo clean && cargo test -p liquers-web --target wasm32-unknown-unknown` |
| After step 9 | both feature states, plus `cargo test -p liquers-lib --lib --tests` |
| Before merge | `cargo test -p liquers-core --doc`; `python3 scripts/docs_index.py --check` |

Build-size discipline (CLAUDE.md): the loop is `-p liquers-core`, the cheap end of the workspace.
`cargo clean` before the wasm target, which is a different target directory. Never
`cargo test --workspace`.

## Rollback Plan

| Step | Rollback |
|---|---|
| 1-5 | Purely additive — new files, one feature, two module declarations. Revert the files; nothing else references them |
| 6-8 | The atomic set. Revert together; partial revert leaves the workspace red |
| 9-11 | Tests and docs; revert independently |

The riskiest revert is **step 7's `set_value`**, because anything written against the old
double-encoding behaviour breaks in both directions.

**`ActionParameter::set_value` has no callers in this workspace.** Verified: every `set_value` hit
across `**/*.rs` belongs to a *different* type — `CommandArguments::set_value`
(`commands.rs:647`, `:683`) and `Asset::set_value` (`assets.rs`, many). So the risk is entirely to
out-of-tree code, and it is recorded in the issue.

That also makes the issue's option 2 — **remove the method** — genuinely viable rather than
theoretical: nothing would have to change to accommodate it. The plan keeps the method and fixes its
behaviour, because a public API that has been shipped is worth keeping when the fix is three lines,
but the zero-caller fact belongs in the Phase 5 note either way.

## Risks

| Risk | Mitigation |
|---|---|
| A query in the repo changes canonical encoding unnoticed | T8's checked-in corpus, run before and after |
| The generated table drifts from `entities.json` | The regenerate-and-compare test, mirroring `registry_export` |
| Error positions are subtly wrong inside a nested link | T3 includes that case explicitly; `link_query` parses on the original span, so positions are absolute |
| The `entities-html5` feature reaches wasm by unification | It is not in any `default`; a Phase 5 check reads the actual wasm bundle size |
| The encoder and the parser disagree again | Structurally prevented: one `match_entity`, one `MNEMONICS`, one `CURATED` |

## Agent Assignment

Per-step assignments are given inline above; collected here for planning.

| Step | Model | Skills | Knowledge |
|---|---|---|---|
| 1 Vendor + generator | haiku | — | this step, `liquers-core/Cargo.toml`, the `liquers-validate` `[[bin]]` precedent |
| 2 `entities.rs` | sonnet | rust-best-practices | Phase 2 "Data Structures", Phase 1 Annex B, the measurements above |
| 3 `escape.rs` classes/codec | sonnet | rust-best-practices | Phase 2 "Data Structures"/"Function Signatures", D5/D6/D9 |
| 4 `escape.rs` codec core | sonnet | rust-best-practices | Phase 2 "Function Signatures"/"Error Handling", Phase 3 T1/T2, **`prototype.py`** |
| 5 `lib.rs` | haiku | — | this step |
| 6 `parse.rs` | sonnet | rust-best-practices | Phase 2 "Error Handling", Phase 3 T3-T5, `parse.rs` in full |
| 7 `query.rs` | sonnet | rust-best-practices | Phase 2 "The value/encoding invariant" |
| 8 `liquers-web` | sonnet | rust-best-practices | Phase 2 "Integration Points", `encode.rs` in full, `liquers-web/README.md` |
| 9 Tests | sonnet | liquers-unittest, rust-best-practices | Phase 3 in full, `prototype.py` |
| 10 Documentation | sonnet | — | Phase 2 "Documentation Architecture", `DOCS_STRUCTURE_GUIDE.md` §9 |
| 11 Close issues | haiku | — | `DOCS_STRUCTURE_GUIDE.md` §4.3 |

**Steps 6-8 should go to one agent, not three.** They are the atomic set, and the intermediate
states do not compile cleanly — an agent handed step 7 alone will see `liquers-web` fail and try to
"fix" the test that is supposed to be deleted.

## Phase 5 Entry Criteria

Phase 5 begins only when all of these hold:

- [ ] Steps 1-11 complete; `cargo test -p liquers-core --lib --tests` green in **both** feature
      states
- [ ] `cargo clean && cargo test -p liquers-web --target wasm32-unknown-unknown` green
- [ ] `cargo test -p liquers-lib --lib --tests` green **without changes** — `registry_export` in
      particular, which proves no command signature moved
- [ ] `cargo test -p liquers-core --doc` green, so every documented spelling is asserted
- [ ] T8's backward-compatibility corpus passes, with `-R/data/ŁŁ.csv`-shaped cases the only
      expected failures, each individually reviewed
- [ ] `python3 scripts/docs_index.py --check` reports 0 errors
- [ ] All review comments resolved

Evidence to carry into Phase 5: measured wasm bundle delta; whether any repository query changed its
canonical encoding; whether the two-state matrix needs a CI note; and the fact that
`ActionParameter::set_value` had zero in-tree callers.

## Open Questions

None. The two Phase 3 measurements are resolved above; the third — whether `cut` reports the
position the diagnostics assume — is resolved by construction in step 6 (the span is captured, not
inferred) and asserted by T3.

## References

- Phase 1 `./phase1-high-level-design.md`, Phase 2 `./phase2-architecture.md`, Phase 3
  `./phase3-examples.md`
- `./prototype.py` — the reference implementation for step 4
- `specs/DOCS_STRUCTURE_GUIDE.md` §4.3, §9
