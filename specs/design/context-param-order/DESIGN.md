---
id: CONTEXT-PARAM-ORDER
kind: design
title: Context parameter at any position in register_command!
form: compact
workflow: liquers-project
status: in_review
phase: high-level
area: [core/commands, macro]
issues: [COMMAND-CONTEXT-PARAM-ORDER]
affects_docs: [REGISTER_COMMAND_FSD, COMMAND_REGISTRATION_GUIDE, RECORD_STREAM_GUIDE]
created: 2026-03-02
---
# Context parameter at any position in register_command!

## Phase 1: High-Level Design

### Purpose

`context` is not a command argument: like the state, it is always available to a command and
never comes from the query. `register_command!` should therefore accept it at **any** position in
the parameter list — before the state, between arguments, or last — and pass it to the function
where the function declares it. A placement the macro cannot honour is a compile-time error at the
offending token, never a runtime mismatch or an error inside the expansion.

### Where the work stands (2026-10-10)

This design was rewritten from scratch. Its 2026-09-02 findings and solution
(`specs/archive/2026-09-02-context-param-order-{findings,solution}.md`) diagnosed an index shift:
the wrapper numbered argument slots over every parameter, `context` included, while metadata
numbered only real arguments. **That fix has landed**: `CommandSignature::extract_all_parameters`
(`liquers-macro/src/registration.rs`) now advances the slot index only for
`CommandParameter::Param`, with unit tests `extract_all_parameters_with_context_{first,middle_and_injected,last}`.
So `context` anywhere *after* the state already works. What remains, verified by expanding the
signatures through the macro's parser on 2026-10-10:

| Signature | Today |
|---|---|
| `fn f(state, a: i64, context, b: String)` | works (slot fix), but no end-to-end test proves it |
| `fn f(context, state, a: i64)` | **rejected**: ``expected `:` `` at `state` — the state keywords are recognised only in first position, so `state` is read as an argument with a missing type |
| `fn f(state, context, a: i64, context)` | **accepted**, then fails in the expansion with a moved-value error on `context` |
| `fn f(value: String)` (no state) | **rejected**: ``unexpected token, expected `)` `` — `value` is taken as the state keyword because `StateParameter::parse` does not check for a following `:` |
| `fn f(a: i64, state)`, `fn f(state, state)` | rejected, but with the misleading ``expected `:` `` |

The last row is not about `context`, but it is the same change site and the same cause (state
keywords recognised by position rather than by form), so it is fixed here (see Scope Changes).

### Problem Example

A command that needs the context to fetch its input first, written the way the function reads
naturally:

```rust
async fn load(
    context: Context<CommandEnvironment>,
    state: State<Value>,
    limit: i64,
) -> Result<Value, Error> { /* … */ }

register_command!(cr, async fn load(context, state, limit: i64) -> result)?;
```

**Today:** compilation fails with ``expected `:` `` pointing at `state`. The author must reorder the
Rust function to `(state, limit, context)` — and the guides tell them to, as a rule
(`RECORD_STREAM_GUIDE.md`, `rust-best-practices/references/anti-patterns.md`).

**Should:** it compiles; the wrapper calls `load(context, state, limit__par)`; the command's
metadata has exactly one argument, `limit`, at index 0; and the query `load-5` runs with `limit = 5`.

### Scope and Acceptance Criteria

- **AC-1** Context before the state
  WHEN `register_command!(cr, fn f(context, state, n: i64) -> result)` registers a command and the query `f-5` is evaluated
  THEN it compiles, `f` receives the context, the input state and `n = 5`, and the metadata lists only `n`, at index 0
- **AC-2** Context between arguments, with an injected argument
  WHEN a command declared `fn f(state, a: i64, context, b: T injected, c: String)` is evaluated with the query `f-1-x`
  THEN `a = 1`, `c = "x"`, `b` is injected from the context, and `f` receives its context at the third position
- **AC-3** Context first in a command without a state
  WHEN `fn f(context, n: i64)` is registered and `f-5` is evaluated
  THEN `f` receives the context and `n = 5`
- **AC-4** Duplicate context is a compile-time error
  WHEN a signature declares `context` (or `Context`) twice
  THEN expansion fails with an error at the second occurrence saying `context` is declared twice, not a moved-value error
- **AC-5** A first argument named like a state keyword is an argument
  WHEN a command without a state declares `fn f(value: String)` (likewise `text: …`, `state: …`)
  THEN it registers one argument named `value`; a bare `value` / `text` / `state` (no `:`) is still the state keyword
- **AC-6** Misplaced or repeated state is a compile-time error
  WHEN the state keyword appears after a regular argument (`fn f(a: i64, state)`) or twice
  THEN expansion fails at that keyword with a message saying the state must precede every argument and that only `context` may come before it
- **AC-7** Existing commands are unaffected
  WHEN the change lands
  THEN every existing `register_command!` use compiles unchanged and `cargo test -p liquers-lib --test registry_export` passes without regenerating `specs/command_registry.yaml`
- **AC-8** The documentation states the rule, not the workaround
  WHEN `REGISTER_COMMAND_FSD.md`, `COMMAND_REGISTRATION_GUIDE.md`, `RECORD_STREAM_GUIDE.md`, `CLAUDE.md` and the `rust-best-practices` anti-patterns are read
  THEN they say `context` may appear at any position and occupies no argument slot, and none presents "context last" as required
- **AC-9** The documentation recommends a conventional position
  WHEN the reference and the guide describe where to put `context`
  THEN they recommend it **last**, or **immediately before a `multiple` argument** when there is one, giving the reason: a Python signature cannot take a positional parameter after `*args` (`def f(state, context, *items)`), so this position keeps a command portable to the Python bindings; their examples follow the recommendation except the one that demonstrates `context` first

**Position rules (decided 2026-10-10).** The state keyword, when present, is the first parameter
that is not `context`. `context` may appear at any position, before the state included; the
recommended position is last, or just before a `multiple` argument (AC-9). The recommendation is
documentation only: the macro does not warn, because a stable proc-macro cannot emit warnings and
every position is correct.

**Non-goals.** Taking the context by reference (`&Context<E>`); renaming the parameter (`ctx`);
moving the state to other positions (Q1); hand-built `CommandMetadata` and manual
registration, where `context` is already a separate wrapper parameter; the JavaScript calling
convention (`JS-COMMAND-CANNOT-ACCESS-CONTEXT`).

### Systems touched and crate placement

- **`liquers-macro`** only: `impl Parse for CommandSignature`, `impl Parse for StateParameter`,
  `CommandSignature::wrapper_arguments` (the state's position in the call), plus new parse-time
  checks. No change to the generated metadata, so no runtime or registry change.
- **`liquers-core`**: no code change. `CommandArguments`, the planner and metadata are already
  context-free. End-to-end tests go in `liquers-core/tests/` (the macro expands against
  `liquers_core` paths, as in `async_hellow_world.rs`).
- Dependency flow is respected: nothing moves across crates.

### Documentation intent

- **Reference:** `specs/reference/REGISTER_COMMAND_FSD.md` §"Context Parameter" states the position
  rule, the duplicate/misplacement errors and the keyword-versus-argument rule for `value` / `text` /
  `state`.
- **Guide:** `specs/guides/COMMAND_REGISTRATION_GUIDE.md` states the recommended position (AC-9)
  with one example before a `multiple` argument, and one example with `context` first labelled as
  allowed but not recommended; no new guide.
- **Other documents updated:** `specs/guides/RECORD_STREAM_GUIDE.md` (drops "context as its last
  parameter" as a requirement); `CLAUDE.md` DSL reference (one line);
  `.claude/skills/rust-best-practices/references/anti-patterns.md` (the "context not last" smell is
  replaced by the recommendation); the `JS-COMMAND-CANNOT-ACCESS-CONTEXT` note that the macro "requires context last".
- **Not edited:** completed designs (`variadic-arguments-declaration`, `record-streams`,
  `store-and-asset-search`) that recorded "context must be last" as honoured at their time — they
  are history.

### Open Questions

All resolved by the maintainer on 2026-10-10; none remain open.

1. **Resolved — the state is first.** The state keyword may only be the first parameter (counting
   every parameter except `context`); anywhere else is AC-6's error. `context` may still precede it
   (AC-1), since `context` may be anywhere. This matches `COMMAND-DECLARATION`, where the first
   non-context argument becomes the state (`WarningKind::ContextBeforeState`).
2. **Resolved — keep the `Context` alias,** treated as the same parameter for AC-4.
3. **Resolved — the issue keeps its title** and is closed by this design's Phase 5.
4. **Resolved — recommended position (new):** `context` last, or immediately before a `multiple`
   argument, for Python-convention parity (AC-9). Documentation only; nothing is enforced.
5. **Implementation detail — error spans.** AC-4 and AC-6 report on the offending token's span, as
   the existing `multiple`-ordering check does (`impl Parse for CommandSignature`).

### Design Dependencies

- `COMMAND-DECLARATION` (complete) — **overlaps**: defines the language-neutral counterpart
  (`context` removed from any position; `ContextBeforeState`). This design brings the Rust macro to
  the same rule; no change there.
- `JS-COMMAND-CANNOT-ACCESS-CONTEXT` (issue) — **overlaps**: cites this issue as a constraint a
  JavaScript convention must not inherit; its note is updated in Phase 5.
- `VARIADIC-ARGUMENTS-DECLARATION` (complete) — **overlaps**: its ordering rule ("`multiple` last
  among query-consuming arguments; `injected` and `context` may follow") is unchanged and remains
  enforced in the same function.
- `REGISTER-COMMAND-OPTION-VALUE`, `REGISTER-COMMAND-ENUM` (open designs) — weak: same file,
  different functions (argument type handling), independent changes.

### Scope Changes

- 2026-10-10 — Design rewritten in the five-phase compact form. The original index-shift fix is
  recorded as landed; the old findings and solution moved to `specs/archive/`.
- 2026-10-10 — AC-9 added and Q1-Q3 resolved after maintainer feedback: state first; `context`
  anywhere, recommended last or before a `multiple` argument (Python `*args` convention).
- 2026-10-10 — AC-5 added: the bare-keyword misparse of a stateless first argument named `value` /
  `text` / `state` was found while verifying this design. Triage case 1 (belongs to the design in
  hand): same root cause — state keywords recognised by position, not form — and same change site
  (`StateParameter::parse`), so no separate issue.
