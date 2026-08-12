# Phase 2: Solution & Architecture - Excess Action Parameters Error

## Overview

Two plan-building paths gain a leftover check and one shared error constructor. The action path
(`ResolvedParameterValues::from_action_extended`) asks its parameter iterator whether anything
remains after every declared argument has been served; the resource-header path
(`PlanBuilder::process_resource_query`, `plan.rs:1239`) replaces its
"extra parameters are ignored" warning with the
same failure. Both raise `ErrorType::TooManyParameters` — a variant that already exists, is already
classified and transported by every downstream crate, and has never been constructed — carrying the
`Position` of the first excess parameter.

No new types, no trait changes, no signature changes to any public function. The check is an early
return inside a function that already returns `Result`.

**Deliberate scope change from Phase 1, decision 4.** Declaring `pl/select_columns` variadic turns
out to be blocked at a layer Phase 1 did not see. The evidence is in
[Variadic arguments: why decision 4 is deferred](#variadic-arguments-why-decision-4-is-deferred);
it is split into `COMMAND-VARIADIC-ARGUMENTS-NOT-DECLARABLE` and the strict check lands now, because the
escape form `select_columns-name~_price` **already** gives those commands their documented
behaviour and is verified below.

## Known-Issue Preflight

Searched: `specs/index.csv` for all locally open `draft` / `accepted` / `in_progress` records; then
`specs/issues/` filtered to areas `core/plan`, `core/query`, `core/commands`, `core/error`, `macro`,
`lib/commands`, `lib/ui` — the areas this design touches or reads. Terminal records excluded.

| Issue | Status | Current priority | Relevance and solution impact | Must be addressed first? | Blocking? | Required action | Priority action |
|---|---|---|---|---|---|---|---|
| `PLAN-EXCESS-ACTION-PARAMETERS-DROPPED` | accepted | P0 | The subject of this design | n/a | no | Close in Phase 5, recording that the resolution is an error, not the warning the issue proposed | Keep P0 |
| `PARAMETER-ESCAPING-INCOMPLETE` | accepted | P0 | Adjacent, not depended on. `~_` is how a dash-bearing value reaches a scalar argument, and that escape **works today** (verified below). The issue concerns characters with *no* escape at all — a disjoint set from the dash | no | **no** | Monitor. Do not build on any escape this design would have to invent | Keep P0 |
| `VARIADIC-ARGUMENT-STARVES-LATER-ARGUMENTS` | draft | P2 | Filed from Phase 1. Constrains any future variadic declaration; this design declares none, so it cannot trip the hazard | no | no | None here. Prerequisite for the deferred decision 4 | Keep P2 |
| `COMMAND-REGISTRY-ISSUE-NAMESPACE-NAME-SWAPPED` | draft | P3 | Independent. Only reachable through `CommandMetadata::check()`, which this design does not call | no | no | None | Keep P3 |
| `UI-QUERY-CONSOLE-NO-ERROR-HIGHLIGHT` | draft | P2 | Consumes this design's output. The positioned error is what the console would highlight; until that issue is fixed the position is carried but not drawn | no | no | None. This design supplies the position regardless | Keep P2 |
| `CORE-PLAN-POLICY-AND-DEFAULTS` | accepted | P2 | Named in Phase 1 decision 2 as the home for a leniency knob, should one ever be wanted. This design deliberately adds no policy | no | no | Monitor | Keep P2 |
| `CORE-EVALUATE-PATH-CONSOLIDATION` | accepted | P1 | Touches `core/plan`, but concerns evaluation entry points, not parameter resolution. No overlap with the two functions changed here | no | no | None | Keep P1 |
| `CORE-RECIPES-EXPAND-PREDECESSORS-CRASH` | accepted | P0 | Touches `core/plan`. Concerns the `expand_predecessors` default; independent of parameter arity. Recipes do reach `from_action_extended` with `allow_placeholders`, so its tests may be a useful place to confirm decision 1 does not regress | no | no | Monitor while running the suites | Keep P0 |
| `MACRO-QUERY-VALIDATION-AND-HINTS` | accepted | P3 | Touches `macro`. Would become relevant only under the deferred decision 4 | no | no | None | Keep P3 |

### Blocking and Priority Decision

**No blockers.** Every relevant issue is either the subject of this design, a consumer of its
output, or independent of the two functions being changed.

The one candidate for blocking status — `PARAMETER-ESCAPING-INCOMPLETE`, since this design makes
strictness depend on users being able to *express* what they mean — is not blocking, because the
escape this design leans on (`~_` for a literal dash) is one of the entities that **does** exist.
Verified against the committed 95-command registry:

```
ns-pl/select_columns-name~_price  ->  Action{pl/select_columns(columns = "name-price")}   status Ok
```

`select_columns` splits that single argument on `-`, so the escaped form already selects both
columns. `PARAMETER-ESCAPING-INCOMPLETE` concerns characters with *no* escape (`:`, `?`, `café`);
that set does not intersect this design's needs.

## Data Structures

**No new structs and no new enums.** The design adds one constructor to an existing type and two
early returns.

### Existing types used, unchanged

| Type | Role here |
|---|---|
| `Error` (`liquers-core/src/error.rs:42`) | Carries the failure. Already has `position: Position` |
| `ErrorType::TooManyParameters` (`error.rs:19`) | The variant this design finally constructs |
| `Position` (`query.rs:436`) | `offset` / `line` / `column`, already on every parameter |
| `ActionParameter` (`query.rs:533`) | Supplies `position()` and `encode()` for the message |
| `HeaderParameter` (`query.rs:904`) | Supplies `value` and `position` on the header path |
| `ActionParameterIterator` (`plan.rs:981`) | Its `parameter_number` is the 1-based index after `next()` |

**No serialization change.** `ErrorType` already derives `Serialize, Deserialize` and every variant
is already mapped in `liquers-py/src/error.rs`, `liquers-axum/src/api_core/error.rs` (→ HTTP 400)
and `liquers-web/src/error.rs` (→ `"too_many_parameters"`). Nothing downstream needs touching.

## Trait Implementations

**None added or modified.** No trait in `liquers-core` gains a method, loses one, or changes a
signature — so `liquers-py`, which breaks easily on trait changes, is unaffected.

The `ErrorType` enum gains no variant, so every exhaustive `match` on it across the workspace
(`assets.rs:1451`, the three binding crates) keeps compiling unchanged. This is the reason for
reusing `TooManyParameters` rather than adding a variant: a new variant would be a breaking change
to four crates for no semantic gain.

## Sync vs Async

**Sync**, with no choice involved. `from_action_extended` and `process_resource_query` are
synchronous, CPU-bound, I/O-free functions on `PlanBuilder`, which is itself the synchronous plan
compiler. The check is a comparison and an early return. No `await`, no lock, no I/O — the async
default does not apply.

`AsyncPlanBuilder` shares `ResolvedParameterValues` and therefore inherits the behaviour with no
separate change.

## Function Signatures

### New: the error constructor

```rust
// liquers-core/src/error.rs, beside `missing_argument` (:174), whose exact dual it is.
impl Error {
    /// An action or resource header supplied a parameter beyond what is accepted.
    ///
    /// `subject` names what rejected it ("command 'select_columns'", "resource header"),
    /// `accepted` is how many parameters that subject consumes, and `excess_index` is the
    /// 1-based position of the first surplus parameter in the written parameter list.
    pub fn too_many_parameters(
        subject: &str,
        accepted: usize,
        excess_index: usize,
        excess_value: &str,
        position: &Position,
    ) -> Self
}
```

Message shape:

```
Too many parameters for command 'select_columns': accepts 1, but parameter #2 'price' was supplied
Too many parameters for resource header: accepts 1, but parameter #2 'meta' was supplied
```

`position` is taken by parameter rather than applied through `.with_position()` because this
constructor is meaningless without one — the position *is* the feature. This mirrors
`missing_argument(i, name, position)` exactly.

### Changed: the action path

```rust
// liquers-core/src/plan.rs:871 — signature UNCHANGED, one early return added before Ok(...)
pub fn from_action_extended(
    action_request: &ActionRequest,
    command_metadata: &CommandMetadata,
    head_parameters: &[CommandParameterValue],
    allow_placeholders: bool,
) -> Result<Self, Error>
```

After the existing `for a in command_metadata.arguments.iter().skip(n)` loop:

- ask `parameters.next()` for a leftover; on `None`, return `Ok` exactly as today;
- on `Some(excess)`, build the error from `excess.encode()`, `excess.position()` and
  `parameters.parameter_number` (already the 1-based index of `excess`, since `next()` incremented
  it), then return `Err`.

**The `accepted` count is not `arguments.len()`.** Two subtractions are required and both are
load-bearing:

- **injected arguments consume no parameters** (`pop_value` returns early at `plan.rs:675`), so they
  must not be counted;
- **alias head parameters** fill the first `n` slots before the action is consulted, so only the
  arguments after `skip(n)` are available to the writer.

Hence `accepted` = the count of non-`injected` arguments in `arguments.iter().skip(n)`. Reporting
`arguments.len()` would tell an author of an aliased or injected command that their query accepts
more parameters than it does.

**Variadic arguments cannot reach the check.** A `multiple` argument drains the iterator
(`plan.rs:679-729`), so `parameters.next()` is `None` and the function returns `Ok` — the exemption
Phase 1 promised, obtained for free rather than by a special case.

### Changed: the header path

```rust
// liquers-core/src/plan.rs:1239 — signature UNCHANGED
fn process_resource_query(&mut self, rqs: &ResourceQuerySegment) -> Result<(), Error>
```

Two edits inside it:

1. **`plan.rs:1250-1255`** — the `if header.parameters.len() > 1 { self.plan.init_warning(…) }` block
   (message text at `:1252`) becomes an `Err` return built from `header.parameters[1]`'s `value` and
   `position`, with `accepted = 1` and `excess_index = 2`.

   The *other* warning in this function — `plan.rs:1242-1245`, "Resource header name is ignored" —
   is **left untouched**. See resolved question 2: the name is reserved for a future realm
   interpretation, so warn-and-ignore is correct there.
2. **`plan.rs:1298`** — the `_` arm currently returns *"Resource header parameters must be string or
   link"*, which describes a parse-shape failure for what is really an unrecognised instruction, and
   carries no position. It becomes an unknown-instruction message that lists the accepted
   instructions and attaches the parameter's position (Phase 1 decision 5).

The `_` arm is a match on `&str`, not on a Liquers-owned enum, so the project's no-default-arm rule
does not apply.

### Unchanged but worth stating

`ResolvedParameterValues::from_action` (`plan.rs:891`) is a thin forwarder and inherits the
behaviour. `PlanBuilder::process_action` (`plan.rs:1354`, `:1382`) needs no edit at either call
site: both already propagate with `?`.

## Integration Points

| Crate / module | Change |
|---|---|
| `liquers-core/src/error.rs` | One new constructor. No new variant |
| `liquers-core/src/plan.rs` | Leftover check in `from_action_extended`; header check and `_`-arm message in `process_resource_query` |
| `liquers-core/src/validate/` | None. The CLI already renders an `Error` with its position and exits 1 |
| `liquers-lib` | Documentation only under the recommended scope — see the deferred decision below |
| `liquers-axum`, `liquers-web`, `liquers-py` | **None.** All three already map `TooManyParameters` |

Crate dependency flow is respected: the change is confined to `liquers-core`, the root of the flow.

### Variadic arguments: why decision 4 is deferred

Phase 1 decision 4 assumed that declaring `pl/select_columns` variadic was a macro change plus a
signature change. It is not. The variadic path is blocked at the trait layer, and the obstacle is
only visible from `commands.rs`:

| Layer | State | Verdict |
|---|---|---|
| `ArgumentInfo.multiple` + `set_multiple()` | exists — `command_metadata.rs:385`, `:550` | ok |
| `pop_value` collects the remainder | works — `plan.rs:679-729` | ok |
| Interpreter handles `MultipleParameters` | works — 5 sites | ok |
| `register_command!` DSL flag | **absent**; hardcodes `multiple: false` at `registration.rs:718`, `:2336`, `:2406` | needs work |
| `FromParameterValue<Vec<T>>` for scalar `T` | **absent.** The only Vec impl is `impl<V: ValueInterface> FromParameterValue<Vec<V>> for Vec<V>` (`commands.rs:269`) — so `Vec<String>` is not obtainable, only `Vec<Value>` | needs work |
| `Vec<_>: TryFrom<Value, Error = Error>` | **absent.** `TryFrom<Value>` exists for scalars only (`value.rs:599-752`), and `Arguments::get` (`commands.rs:102`) requires that bound | needs work |

The consequence is that `arguments.get::<Vec<String>>(i, name)` — what the macro would generate —
does not compile, and neither would `Vec<Value>`. A blanket
`impl<T: FromParameterValue<T>> FromParameterValue<Vec<T>> for Vec<T>` would *overlap* the existing
`Vec<V: ValueInterface>` impl and be rejected by coherence.

The clean route exists and is worth recording for whoever picks this up: add
`Arguments::get_multiple<T: FromParameterValue<T>>(i, name)` which walks
`ParameterValue::MultipleParameters` and calls `T::from_parameter_value` per element. It needs
**no new trait impl** (so no coherence fight) and drops the `TryFrom<E::Value>` bound, which exists
only for the pre-materialised fast path that a variadic argument never uses. The macro then emits
`get_multiple` for a `multiple` argument.

That is a three-crate feature with a trait-design decision inside it — properly its own design, not
a rider on a leftover check. **Deferred to `COMMAND-VARIADIC-ARGUMENTS-NOT-DECLARABLE` (P1/M); this design keeps to the
strict check.** The cost of deferring is bounded and known: `select_columns-a-b` becomes an error
instead of silently selecting one column, and the correct spelling is `select_columns-a~_b`, which
works today.

While making the DSL flag declarable, one adjacent defect should be fixed in the same edit: the
flag parser at `registration.rs:1564` parses *any* identifier and compares it to `"injected"`, so
`fn f(state, a: i32 foobar)` silently swallows `foobar`. Adding a second flag without rejecting
unknown ones would make typos between `multiple` and `injected` fail silently.

## Documentation Architecture

| Path | Kind | Audience | Change | `affects_docs` |
|---|---|---|---|---|
| `specs/reference/PROJECT_OVERVIEW.md` | reference | query writers, command authors | Add the arity rule to the query/plan description: *every action parameter must be consumed by a declared argument; a `multiple` argument consumes the remainder; surplus is an error carrying the parameter's position*. State that the resource header follows the same rule. Add `## History` row, bump `reviewed:` | yes |
| `specs/reference/POLARS_COMMAND_LIBRARY.md` | reference | users of the `pl` namespace | Correct the `select_columns` / `drop_columns` documentation: the dash-separated form must be written `a~_b`, because `-` separates parameters. Under the recommended scope this is the *only* liquers-lib change | yes |
| `specs/guides/COMMAND_REGISTRATION_GUIDE.md` | guide | command authors | Add a short subsection: a command that accepts a variable-length list needs a `multiple` argument, and — under the recommended scope — a note that the flag is not yet declarable, linking the deferred issue | yes |
| `specs/issues/PLAN-EXCESS-ACTION-PARAMETERS-DROPPED.md` | issue | — | Close in Phase 5; record that the resolution is an error, not the proposed warning, and why | n/a |
| `specs/README.md` | index | — | Link this design folder | n/a |

Authoritative `affects_docs`: `[specs/reference/PROJECT_OVERVIEW.md,
specs/reference/POLARS_COMMAND_LIBRARY.md, specs/guides/COMMAND_REGISTRATION_GUIDE.md]`.
`REGISTER_COMMAND_FSD.md` and `CLAUDE.md` moved to `COMMAND-VARIADIC-ARGUMENTS-NOT-DECLARABLE` with
the deferred decision 4.

The `PROJECT_OVERVIEW.md` wording must distinguish the header's two ignored inputs rather than
claiming the header is uniformly strict — see resolved question 2.

The `liquers-validate` skill references were checked: they describe a clean result as meaning "your
query means this", not "no parameter was dropped, " so no correction is required. Its documented
exit codes are unaffected — an over-supplied query moves from exit 0 to exit 1, which is the
intended change, not a contract change.

## Relevant Commands

**No new commands.** This design registers nothing.

Existing namespaces affected — this is the list on which user feedback is requested:

| Namespace | Commands | How affected |
|---|---|---|
| `pl` (polars) | `select_columns`, `drop_columns` | Documented dash-separated form becomes an error. Documentation corrected to `~_`; commands themselves unchanged under the recommended scope |
| `pl` | `head`, and any single-argument command | An over-supplied call such as `head-10-99` becomes an error instead of silently ignoring `99` |
| all namespaces | every registered command | Arity becomes binding everywhere. No command's *declaration* changes |

The blast radius is "every query that over-supplies an action", which is exactly the intended
change. Phase 3 measures how much committed material that is, by running the suites and validating
the registry.

## Error Handling

All failures use `liquers_core::error::Error` via a typed constructor. `Error::new` is not used.

| Condition | Error type | Position |
|---|---|---|
| Action supplies more parameters than declared | `TooManyParameters` | First excess `ActionParameter` |
| Resource header supplies more than one parameter | `TooManyParameters` | Second `HeaderParameter` |
| Resource header instruction unrecognised | `NotSupported` (unchanged type, corrected message) | The offending `HeaderParameter` |

**Only the first excess parameter is reported**, not all of them. Errors are a single value in this
codebase, and the first surplus is the actionable one — it is where the writer's intent and the
command's declaration diverge.

**Decision 1 restated as a code fact:** the check sits after the argument loop and does not read
`allow_placeholders`, so it fires identically for queries and recipes. Placeholders concern missing
arguments; nothing about a surplus becomes acceptable because a different argument may be filled in
later.

**No `unwrap()` / `expect()`.** The one existing `unwrap()` in the header path
(`header.parameters.first().unwrap()`, `plan.rs:1257`) sits inside an `else` branch of
`if header.parameters.is_empty()`, so it is sound — but the new code is placed so the surrounding
edit does not add a second one, and the existing one is worth converting to `if let` while the
block is being rewritten.

## Rust Best Practices Review

Applied the `rust-best-practices` lens to the above.

```
BLOCKING
- None. No unwrap in new code, no new error type, typed constructor used, no trait
  change, no default match arm on a Liquers-owned enum, crate flow respected.

ADVISORY
- `too_many_parameters` takes five parameters, four of them describing one excess
  value. A small `struct` would be cleaner in isolation, but `missing_argument` sets
  the local convention with a flat list and consistency inside `error.rs` wins.
- `subject: &str` is a formatted fragment rather than a type. A two-variant enum
  would be more type-safe; with exactly two call sites in one module the string keeps
  the constructor usable from both without exporting a new public enum.
- `excess.encode()` allocates a String for the message. It is on an error path only.

QUESTIONS (raised to the user below)
- Whether decision 4 stays in scope, given the trait-layer obstacle.
```

## Phase 1 Conformity

Checked this document against Phase 1, decision by decision.

| Phase 1 item | Phase 2 outcome |
|---|---|
| Decision 1 — error regardless of `allow_placeholders` | Held. The check does not read the flag |
| Decision 2 — no opt-out | Held. No policy or configuration added |
| Decision 3 — header errors too, one rule | Held. Same constructor, same message shape |
| Decision 4 — polars commands become variadic | **Deferred**, with evidence, to `COMMAND-VARIADIC-ARGUMENTS-NOT-DECLARABLE` (P1/M) |
| Decision 5 — header `_` arm message fixed | Held. In scope, with position added |
| Positioned error is the point | Held. `Position` is a required constructor parameter |
| Documentation intent | Held and made specific with exact paths and changes |

Scope drift check: the only movement is *outward* at decision 4, and the recommendation is to move
it back out of scope rather than absorb it silently.

## Codebase Alignment

Verified each claim against HEAD rather than assuming.

- `ErrorType::TooManyParameters` is constructed nowhere in the workspace — confirmed by grep across
  all crates. Reuse is safe.
- `assets.rs:1451`, `liquers-axum/src/api_core/error.rs:15`, `liquers-web/src/error.rs:22` and
  `liquers-py/src/error.rs:42` all already handle the variant.
- `ActionParameter::encode()` (`query.rs:607`) covers both `String` and `Link`, so a link parameter
  in excess position reports as `~X~…~E` rather than panicking or printing a debug form.
- `ActionParameterIterator::next()` (`plan.rs:1003`) increments `parameter_number` *before*
  returning, so after `next()` it equals the 1-based index of the returned parameter. No off-by-one
  adjustment is needed, and this is asserted in Phase 3.
- No existing test asserts that surplus parameters are ignored — searched `plan.rs` tests
  (`:2656-2668` construct exactly-saturated actions). The three suites still have to be run;
  that is Phase 3 work, not an assumption made here.

## Resolved Questions

1. **Decision 4 is deferred**, as recommended. Filed as `COMMAND-VARIADIC-ARGUMENTS-NOT-DECLARABLE`
   (P1/M), carrying the trait-layer evidence and the `get_multiple` fix direction. This design's
   only `liquers-lib` change is therefore documentation: correcting
   `POLARS_COMMAND_LIBRARY.md` to the escaped spelling. `REGISTER_COMMAND_FSD.md` and `CLAUDE.md`
   drop out of this design's documentation set and move to that issue.

2. **The header *name* warning stays a warning** (`plan.rs:1242-1245`). The name is *reserved*, not
   meaningless: it is intended to be interpreted as the realm, which is what
   `// TODO: RQS realm should should be supported` (`plan.rs:1238`) records. Warn-and-ignore is the
   correct treatment of an input that will acquire meaning later — an error would reject queries
   that a future version accepts.

   This is the substantive distinction between the header's two warnings, and it is why only one of
   them becomes an error:

   | Warning | Ignored input | Treatment | Why |
   |---|---|---|---|
   | `plan.rs:1242` name ignored | reserved for a future realm | **stays a warning** | Will become meaningful; rejecting it now forecloses that |
   | `plan.rs:1251` surplus parameters | nothing will ever consume them | **becomes an error** | Nothing to reserve; the writer meant something the header cannot express |

   The reference wording must carry this distinction rather than stating "the header is strict".
