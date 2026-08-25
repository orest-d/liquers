# Phase 5: Documentation - Declarable variadic command arguments

## Completion Preconditions

| Criterion (Phase 4) | Status |
|---|---|
| Steps 1-7 complete | Yes |
| `cargo test -p liquers-core --lib` | 601 passed, 0 failed |
| `cargo test -p liquers-macro` | 52 passed, 0 failed |
| `cargo test -p liquers-lib --lib --tests` | 297 + all integration suites passed, 0 failed |
| `cargo test -p liquers-lib --test registry_export` | 5 passed — the committed registry matches the code |
| Documented queries re-validated **without** the proposal overlay | Yes — see Validation |
| Review comments resolved | None outstanding at the time of writing |
| Disposition of `variadic-proposal.registry.yaml` | Kept, with a note; see below |

The browser and `liquers-web` loops were deliberately not run, as Phase 4 specified: nothing here
touches wasm, the web bindings or the UI.

## Implementation Summary

### What was requested

Close `COMMAND-VARIADIC-ARGUMENTS-NOT-DECLARABLE`: `ArgumentInfo.multiple` was implemented end to
end in the plan builder and interpreter, but no command author could declare one and the command
framework could not hand one to a function.

### What was implemented

**liquers-core** — `CommandArguments::get_multiple<T: FromParameterValue<T>>`
(`commands.rs:127`), plus a private `convert_multiple_element`. Both matches enumerate every
`ParameterValue` variant, with no default arm.

The design's load-bearing choice was to **add a method rather than a trait impl**. The obvious
route — `impl<T: FromParameterValue<T>> FromParameterValue<Vec<T>> for Vec<T>` — is rejected by
coherence against the existing `impl<V: ValueInterface> FromParameterValue<Vec<V>>`, and no
negative bound exists to separate them. A method needs no coherence argument at all. It also drops
`get`'s `TryFrom<E::Value>` bound, which is not a relaxation: that bound serves a pre-materialised
fast path a variadic argument can never take, because the interpreter fills `values[i]` only where
`ParameterValue::link()` is `Some` and `MultipleParameters::link()` is `None`.

**liquers-macro** — a `multiple` argument flag in the same grammar slot as `injected`, mutually
exclusive with it; `variadic_element_type` recognising `Vec<T>`; `ArgumentType` derived from the
element type; `get_multiple` emitted instead of `get`; and six compile-time rejections.

**liquers-lib** — `pl/select_columns` and `pl/drop_columns` take `Vec<String>`, their `split('-')`
and `.trim()` workarounds deleted, with an explicit empty-list rejection.

### Scope actually delivered against scope requested

| | |
|---|---|
| **Requested and delivered** | The accessor, the DSL flag, unknown-flag rejection, both polars commands converted, the registry regenerated, four documents updated |
| **Added beyond the request** | The macro-level ordering guard (approved as Phase 1 D1); three further compile-time rejections that the flag makes necessary; the empty-list rejection in both commands; replacing two integration tests that never invoked the commands |
| **Deliberately omitted** | GUI list rendering (`UI-VARIADIC-ARGUMENT-LIST-EDITOR`); composite element types (`COMMAND-COMPOSITE-VARIADIC-ARGUMENTS`); the registry-level ordering check for hand-built metadata |
| **Not attempted** | `Vec<Value>` as a declarable variadic type — see Conformance |

## Documentation Delivered

Per the Phase 2 architecture. No new reference or guide: this closed a gap in a documented
mechanism rather than adding one.

| Document | Change | History row |
|---|---|---|
| `specs/reference/REGISTER_COMMAND_FSD.md` | New §Variadic Parameters; grammar line now `[injected \| multiple]`; attribute and type tables extended; `reviewed:` 2026-03-02 → 2026-08-25 (it was `overdue`) | Yes |
| `specs/reference/POLARS_COMMAND_LIBRARY.md` | Column Selection retitled "(variadic)"; all `~_` examples reverted to plain dashes; the arity-error note replaced by the `a-b` vs `a~_b` distinction; empty-list behaviour documented; `reviewed:` bumped | Yes |
| `specs/guides/COMMAND_REGISTRATION_GUIDE.md` | §"Accepting a variable number of parameters" rewritten from "cannot be declared" to the how-to, with the rejection table and manual-registration note; `reviewed:` bumped | Yes |
| `CLAUDE.md` | DSL Syntax Reference gains `multiple` and its four rules | n/a |
| `specs/README.md` | New capability entry pointing at the reference and guide, not at this folder | n/a |
| `specs/command_registry.yaml` | Regenerated; dated CHANGELOG line added | n/a |

`variadic-proposal.registry.yaml` is **kept** in this folder. It was Phase 3's measurement
instrument — the overlay that proved the plan-level behaviour before any code existed — and it
remains the reproducible record of the before/after. It is superseded for practical use by the real
registry, which now carries the signatures.

## Issues Filed

Five, four of them found by looking rather than by failing.

| Issue | Priority | How it was found |
|---|---|---|
| `PY-MODULES-NOT-DECLARED-IN-LIB` | P2 | Phase 2 preflight, checking whether `liquers-py` was a live `multiple` consumer |
| `POLARS-COMMAND-TESTS-BYPASS-COMMANDS` | P2 | Phase 3, asking what the existing tests would catch |
| `REGISTRY-IMPL-VERSION-DRIFT-UNDETECTED` | P3 | Phase 4 execution, investigating two unexpected lines in a diff |
| `UI-VARIADIC-ARGUMENT-LIST-EDITOR` | P3 | Phase 1 D5, sharpened by the user's `egui-midi-test` prototype |
| `COMMAND-COMPOSITE-VARIADIC-ARGUMENTS` | P3 | Phase 1 D5, raised by the user |

## Important Learning

**1. A design whose runtime half already exists can be verified before it is written.** Phase 3 ran
`liquers-validate` against a registry overlay declaring the proposed signatures and read the actual
resolved plan: three positioned elements for `select_columns-a-b-c`, one element `"a-b"` for
`a~_b`, an empty list for no parameters. Every later phase rested on measurement rather than
prediction, and the implementation produced exactly those results. Generalisable: any design that
*changes a command signature* can be tested this way, and `CLAUDE.md` already documents the overlay
flags for it.

**2. Where a check lives determines what it costs to test.** Putting all six rejections in
`impl Parse` — rather than in `command_registration()` — made them reachable from
`syn::parse2::<CommandSignature>`, so they are ordinary unit tests asserting message text. The
alternative would have surfaced them only as `compile_error!` at an expansion site, requiring a
`trybuild` dev-dependency and `.stderr` fixtures. The architectural choice and the testing cost
were one decision, which was not visible until Phase 3.

**3. "The tests pass" and "the change is tested" are different claims.** All 13 tests in
`polars_commands.rs` call the Polars API directly; `create_test_env()` was defined and never
called. The two covering the converted commands would have passed however the conversion went. When
a change's existing tests would pass regardless of the change, that is a finding about the tests.

**4. Investigate an unexpected diff line before accepting it.** Regenerating the registry changed
four `impl_version` values; two were predicted (`#[command_version]` hashes the whole function) and
two were not. Stashing the source changes and regenerating at HEAD showed the latter two were
pre-existing drift that `registry_export` cannot see, because it compares signatures. Two minutes
of checking turned a mystery into a filed issue.

**5. Verify a claim about existing syntax rather than recalling it.** Asked whether `injected`
precedes the parameter name, the answer came from the parser (`registration.rs:1563`), the FSD
grammar, and sixteen call sites — all agreeing it *follows the type*. `multiple` was already in the
right place; the useful outcome was making the shared slot explicit in the grammar as
`[injected | multiple]`.

## Conformance and Remaining Work

### Conformance to the approved design

Every Phase 1 decision landed as approved. Phase 2's signatures were implemented unchanged. Phase
3's tests were implemented as written, with one substitution noted below.

**One deviation, in test mechanics only.** Phase 3 specified an `eval_over_csv` helper putting the
CSV in an `AsyncMemoryStore` and evaluating `-R/data/input.csv/-/ns-pl/from_csv/…`. In execution
that returned `No recipe found for key data/input.csv` — a `DefaultEnvironment` with
`DefaultRecipeProvider` does not serve a plain store file through that path, and the store itself
was verified to contain the key. The helper now uses
`envref.get_asset_manager().apply(query.into(), state)`, the ad-hoc path that applies a transform
query to a supplied state. It is simpler, needs no store, and is closer to what these tests mean.
The queries under test are unchanged.

Why this was not treated as a defect to fix: the tests need state → command → state, and `apply`
is the API for exactly that. Whether `-R/` should read a plain store file under
`DefaultRecipeProvider` is a separate question about environment wiring, not about variadic
arguments, and it was not investigated further.

### Issue disposition

| Issue | Disposition |
|---|---|
| `COMMAND-VARIADIC-ARGUMENTS-NOT-DECLARABLE` | **Closed.** Every item in its fix direction is done, including the two affected commands, the registry regeneration and the documentation |
| `VARIADIC-ARGUMENT-STARVES-LATER-ARGUMENTS` | **Narrowed, not closed.** Macro-registered commands are guarded at compile time. Hand-built `CommandMetadata` is not, and `liquers-py`'s compiled `add_python_command` (`command_metadata.rs:430`) sets `multiple` directly — it happens to apply the flag to `arguments.last_mut()`, so it satisfies the rule by construction, but nothing enforces that. Its own fix direction (`CommandMetadata::check()`) is untouched and still blocked on `check()` having no caller |

### Remaining work

- The two issues deferred by Phase 1 D5, and the three filed during the work. None blocks anything
  delivered here.
- **`Vec<Value>` is not declarable as a variadic argument.** `get_multiple::<Value>` does not
  compile: no `impl FromParameterValue<Value> for Value` exists. `Vec<Value>` remains retrievable
  for hand-built registrations through the untouched `impl<V: ValueInterface>`. This is recorded
  rather than filed, because no caller wants it and adding the impl is a one-line change whenever
  one does.
- Corner case C3 (a recipe override of a variadic argument) **was a real defect**, found by Codex
  review on [PR #38](https://github.com/orest-d/liquers/pull/38) after this document was first
  written. Phase 3 had recorded it as untested rather than testing it, and it was broken:
  `ParameterValue::name()` returned `None` for `MultipleParameters`, so `override_value` and
  `override_link` could not find the slot and `Recipe::to_plan` failed with
  "Argument columns not found in last action".

  Fixed by giving the variant its argument name — `MultipleParameters(String, Vec<ParameterValue>)`
  — and by making both override methods keep a variadic slot a parameter list rather than replacing
  it with a scalar `OverrideValue`/`OverrideLink`, which `get_multiple` would reject. An array
  override expands to one element per entry, mirroring `from_arginfo`'s array-default expansion.
  Covered by `recipe_override_reaches_a_variadic_argument` (`plan.rs`) and
  `recipe_overrides_a_variadic_argument` / `recipe_link_overrides_a_variadic_argument`
  (`recipes.rs`).

  The lesson is the sharper form of learning point 3: recording a corner case as untested is not the
  same as knowing it works. C3 was the one path this design left unexercised, and it was the one
  that was broken.

- `LINK-IN-VARIADIC-DOES-NOT-EXPAND` (P3) was filed while making that fix: a link inside a variadic
  argument yields one element even when it resolves to an array. Deliberately out of scope — the
  right answer interacts with the existing `Vec<V: ValueInterface>` impl and needs its own decision.

## Validation

```
cargo test -p liquers-core --lib          601 passed, 0 failed
cargo test -p liquers-macro                52 passed, 0 failed   (41 pre-existing + 11 new)
cargo test -p liquers-lib --lib --tests   all suites passed, 0 failed
  · polars_commands                        17 passed (11 pre-existing + 6 new)
  · registry_export                         5 passed (4 pre-existing + 1 new)
```

New tests: 6 for `get_multiple`, 11 for the macro, 6 polars integration tests (five replacing two
that never invoked a command), 1 registry round-trip. 24 in total.

**Query validation, against the regenerated registry with no overlay:**

| Query | Result |
|---|---|
| `ns-pl/select_columns-date-amount-region` | Ok |
| `ns-pl/select_columns-a-b` | Ok |
| `ns-pl/select_columns-a~_b` | Ok |
| `ns-pl/select_columns` | Ok |
| `ns-pl/drop_columns-col1-col2-col3` | Ok |
| `-R/data/sales.csv/-/ns-pl/from_csv/select_columns-date-amount-status/gt-amount-1000/eq-status-completed/head-10` | Ok |
| `ns-pl/from_csv/select_columns-col1-col2/head-10` | Ok |

Four example queries in `POLARS_COMMAND_LIBRARY.md` still fail validation. They omit `ns-pl`, which
is `POLARS-DOC-EXAMPLES-OMIT-NAMESPACE` (P2, filed 2026-08-12) and is untouched by this work —
confirmed by validating the same queries with the namespace inserted, which all resolve `Ok`. The
variadic spelling in every one of them is correct.

**End-to-end behaviour confirmed by test**, not merely by plan resolution: `select_columns-a-c`
selects two columns; `select_columns-a~_b` selects the single column `a-b`; `select_columns` with
no parameters is rejected with "requires at least one column name"; `select_columns-a-zz` names
`zz`; `select_columns-a-` treats the trailing dash as an empty element.
