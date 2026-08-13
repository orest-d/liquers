# Phase 5: Documentation - Excess Action Parameters Error

## Completion Preconditions

| Criterion | State |
|---|---|
| Steps 1-7 of the Phase 4 plan complete | Yes |
| `cargo test -p liquers-core` | 526 unit + 111 integration, all pass |
| `cargo test -p liquers-lib --lib --tests` | 297 unit + 65 integration, all pass |
| Manual `liquers-validate` before/after check | Run; transcript below |
| Review comments answered | None outstanding |
| Header's two warnings settled | Yes — surplus errors, reserved name warns; documented in that form |

## Implementation Summary

An action supplying more parameters than its command declares no longer builds a plan. Plan building
returns `ErrorType::TooManyParameters` carrying the `Position` of the first surplus parameter.

Observed against the committed 95-command registry:

```
ns-pl/select_columns-a-b    Error  col 24  Too many parameters for command 'select_columns':
                                           accepts 1, but parameter #2 'b' was supplied
ns-pl/select_columns-a~_b   Ok
ns-pl/head-10               Ok
ns-pl/head-10-99            Error  col 15  Too many parameters for command 'head':
                                           accepts 1, but parameter #2 '99' was supplied
```

### What was implemented

| Location | Change |
|---|---|
| `liquers-core/src/error.rs` | `Error::too_many_parameters(subject, accepted, excess_index, excess_value, position)`, the dual of `missing_argument`. **No new `ErrorType` variant** — `TooManyParameters` already existed and had never been constructed |
| `liquers-core/src/plan.rs` | Leftover check at the end of `ResolvedParameterValues::from_action_extended`; `accepted_parameter_count` helper; arity rules for the `v` and `q` instructions, which bypass metadata resolution |
| `liquers-core/src/plan.rs` | `process_resource_query`: surplus header parameters error; the `unwrap` on `parameters.first()` removed; the fallback arm names the unknown instruction and carries its position |
| `liquers-core/tests/validate_integration.rs` | One fixture corrected — see below |

Three properties of the implementation are worth carrying forward:

1. **The leftover is found by asking the iterator, not by comparing counts.** This is what keeps both
   exemptions correct without a special case: a `multiple` argument has already drained the
   iterator, and an injected argument never took from it.
2. **`accepted` is not `arguments.len()`.** Injected arguments consume no query parameter, and an
   alias's head parameters fill leading slots before the action is consulted. Reporting the raw
   length would misinform the author of an aliased or injected command.
3. **`parameters.parameter_number` is already 1-based** after `next()`, which increments before
   returning.

### Deviations from the request and the plan

| Item | Deviation | Reason |
|---|---|---|
| Warning vs. error | The issue proposed a **warning**; the delivered behaviour is an **error** | Requested by the user, and structurally necessary: `Step::Warning` carries no `Position`, so a warning cannot name the offending parameter |
| Polars commands become variadic (Phase 1 decision 4) | **Not done**; deferred | Blocked below the macro — see *Conformance and Remaining Work* |
| `REGISTER_COMMAND_FSD.md`, `CLAUDE.md` | Not updated | They document the `multiple` DSL flag, which moved with the deferral |
| One test fixture changed | `validate_integration.rs` built its registry with **no arguments** and then validated `head-10` | The fixture was itself over-supplied and had been passing on the dropped parameter. It now declares the argument, as the real `pl/head` does |

The fixture change is the only committed material the strict check broke — matching the Phase 3
measurement, which predicted zero *substantive* breakage and did not cover test-local registries.

## Documentation Delivered

| Document | Change | `reviewed:` |
|---|---|---|
| `specs/reference/PROJECT_OVERVIEW.md` | New **Parameter arity** subsection in §Query Language: every written parameter must be consumed; surplus is a positioned error; `accepted` excludes injected and alias-head arguments; `multiple` consumes the remainder; the header's surplus errors while its reserved name warns | 2026-08-12 |
| `specs/reference/POLARS_COMMAND_LIBRARY.md` | `select_columns` / `drop_columns` corrected to the `~_` spelling throughout (7 occurrences), with a blockquote explaining why the plain dash form is an arity error and when it will become valid again | 2026-08-12 |
| `specs/guides/COMMAND_REGISTRATION_GUIDE.md` | New **Accepting a variable number of parameters** subsection: arity is binding, `multiple` is the only variadic mechanism and is not yet declarable, `~_` is the interim spelling, and a warning not to design commands around the unescaped form | 2026-08-12 |
| `specs/README.md` | Capability line moved from *designing* to *built*, pointing at the reference | n/a |
| `specs/issues/PLAN-EXCESS-ACTION-PARAMETERS-DROPPED.md` | Closed with a resolution note recording that the outcome is an error rather than the proposed warning, why, and that the issue's "fix direction" was superseded | n/a |

`affects_docs`: `[specs/reference/PROJECT_OVERVIEW.md, specs/reference/POLARS_COMMAND_LIBRARY.md,
specs/guides/COMMAND_REGISTRATION_GUIDE.md]`. All three gained a `## History` row.

The `PROJECT_OVERVIEW.md` wording deliberately does **not** say "the resource header is strict". It
distinguishes the header's two ignored inputs, because treating them alike would be the wrong
conclusion — see learning point 2.

## Issues Filed

Five, all found while doing the work and none absorbed silently.

| ID | Priority | Why it exists |
|---|---|---|
| `COMMAND-VARIADIC-ARGUMENTS-NOT-DECLARABLE` | P1/M | The `multiple` escape hatch this design points users to cannot be declared: `register_command!` hardcodes `multiple: false`, the only `FromParameterValue<Vec<_>>` impl is for `Vec<V: ValueInterface>`, and `Arguments::get` additionally requires `TryFrom<Value>`, which no `Vec` satisfies. Carries the `get_multiple` fix direction and requires fixing `pl/select_columns` and `pl/drop_columns` |
| `VARIADIC-ARGUMENT-STARVES-LATER-ARGUMENTS` | P2/S | An argument declared after a `multiple` one is unreachable and nothing rejects it. Becomes live the moment the issue above lands |
| `UI-QUERY-CONSOLE-NO-ERROR-HIGHLIGHT` | P2/S | The console holds the `Error` and the highlight path is complete, but it enters that path through `StyledQuery::from`, which hardcodes `Position::unknown()`. This is what converts this design's positioned error into the editing experience that motivated it |
| `COMMAND-REGISTRY-ISSUE-NAMESPACE-NAME-SWAPPED` | P3/S | `CommandRegistryIssue::{warning,error}` pass `(realm, name, namespace)` to a `(realm, namespace, name)` constructor |
| `POLARS-DOC-EXAMPLES-OMIT-NAMESPACE` | P2/S | 13 of 14 example queries in the polars reference do not resolve, because they omit `ns-pl`. Predates and is independent of this design |

## Important Learning

1. **The diagnostic channel constrained the language decision.** `Step::Warning` carries a `String`
   and no `Position`; `Error` carries both. "Report *which* parameter is excess" is therefore not
   merely better served by an error — as a warning it is inexpressible without first extending the
   diagnostic type. Anyone reconsidering the severity has to reckon with that, not just with
   strictness.

2. **Two ignored inputs, two correct treatments.** The resource header ignores a *name* and ignores
   *surplus parameters*, and consistency between them would be wrong. The name is reserved for a
   future realm interpretation (`// TODO: RQS realm should should be supported`), so warning
   preserves queries a later version will accept; surplus parameters reserve nothing, so they are
   rejected. The rule is about whether the input will ever acquire meaning.

3. **A feature can be half-built in a way no test reveals.** `multiple` had a complete runtime — the
   plan builder collects it, `commands.rs` materialises it, the interpreter handles it in five
   places — and no way to declare it through the macro. Nothing failed, because nothing used it. It
   surfaced only when this design needed to point users at it as the sanctioned escape hatch.

4. **The exemption fell out of the design rather than being added to it.** Because the check asks the
   iterator, variadic and injected arguments are exempt for free. An implementation that compares
   counts would pass every other test and break both. `arity_boundaries_still_build` exists to catch
   exactly that reimplementation.

5. **Measuring breakage before implementing was worth it, including its limits.** Harvesting query
   literals and comparing written against resolved parameter counts predicted the outcome
   accurately, and stating what it *could not* cover — test-local registries — is what made the one
   real failure legible as a fixture defect rather than a surprise.

6. **Validating documentation examples finds more than the change you came for.** Checking the
   polars examples for the arity fix revealed they had never resolved at all, for an unrelated
   namespace reason. Cheap to check, and worth doing whenever a reference's examples are touched.

## Conformance and Remaining Work

### Requested vs. delivered

| Requested | Delivered |
|---|---|
| Excess parameters raise an error | Yes |
| Error names the position of the excess parameter | Yes — `Position` is a required constructor argument, verified at column precision |
| During plan building | Yes — both the action and resource-header paths |

### Added after review

PR #33 review (Codex) observed that the special instructions bypass command-metadata resolution, so
the leftover check never sees them — `v-extra` still discarded `extra` silently, contrary to the
guarantee this design documents. Correct, and fixed. But the suggestion to "apply the same rule to
other instruction shortcuts" would have been wrong applied uniformly; the three instructions differ:

| Instruction | Rule | Change |
|---|---|---|
| `v` | takes no parameters | **Was silently dropping them.** Now `TooManyParameters`, positioned |
| `q` | takes no parameters | Already rejected (`process_query`); gained a position |
| `ns` | **variadic by design** — each parameter names a namespace | **No change.** `ns-one-two` is correct and must keep working |

`special_instructions_enforce_their_own_arity` pins all three, including the `ns` case, so a later
attempt at uniformity fails a test rather than silently breaking namespace selection.

### Added beyond the request

- The resource header made strict on surplus parameters (user decision).
- The header's fallback arm corrected: it reported a parse-shape failure for an unknown instruction
  and carried no position; it now names the instruction, lists the valid ones, and is positioned.
- An `unwrap` removed from library code in the same block.

### Not done, and where it went

**Phase 1 decision 4 — `pl/select_columns` and `pl/drop_columns` become variadic.** Deferred to
`COMMAND-VARIADIC-ARGUMENTS-NOT-DECLARABLE` (P1/M) once Phase 2 established that the path is blocked
two layers below the macro. The cost is bounded and documented: the unescaped `select_columns-a-b`
is an error, and the working spelling `select_columns-a~_b` resolves to the single argument `"a-b"`,
which the command splits as intended. Both references and the guide state this and link the issue.

That issue also records something this design could not deliver: a column name genuinely containing
a dash is *still* unrepresentable, because the commands split on `-` internally. It becomes
representable when they are declared variadic and the internal split is removed.

## Validation

```
cargo test -p liquers-core              526 unit + 3+5+5+5+4+32+15+16+14+8+6+13+5 integration — all pass
cargo test -p liquers-lib --lib --tests 297 unit + 65 integration — all pass
```

16 tests added: 13 in `plan.rs`, 1 in `error.rs`, 2 in `validate/mod.rs`.

Manual check against the real registry, which is also the transcript quoted in
`POLARS_COMMAND_LIBRARY.md`:

```bash
cargo run -p liquers-core --features cli --bin liquers-validate -- --detail summary -- \
  'ns-pl/select_columns-a-b' 'ns-pl/select_columns-a~_b' 'ns-pl/head-10' 'ns-pl/head-10-99'
```

Positions verified exactly: `b` at column 24 and `99` at column 15, each the first character of the
surplus parameter.

Not run, and why: `liquers-web` and the browser suites. This design touches neither, and
`ErrorType::TooManyParameters` was already mapped in `liquers-web/src/error.rs`,
`liquers-axum/src/api_core/error.rs` and `liquers-py/src/error.rs` before it — no variant was added,
so every exhaustive match across those crates is unchanged.
