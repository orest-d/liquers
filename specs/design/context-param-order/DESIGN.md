---
id: CONTEXT-PARAM-ORDER
kind: design
title: Context parameter at any position in register_command!
form: compact
workflow: liquers-project
status: in_review
phase: architecture
area: [core/commands, macro]
issues: [COMMAND-CONTEXT-PARAM-ORDER, MACRO-TESTS-PRINT-TO-STDOUT]
merged: 2026-10-10
affects_docs: [REGISTER_COMMAND_FSD, COMMAND_REGISTRATION_GUIDE, RECORD_STREAM_GUIDE]
created: 2026-03-02
---
# Context parameter at any position in register_command!

## Phase 1: High-Level Design

### Purpose

`context` is not a command argument: like the state, it is always available and never comes from
the query. `register_command!` should accept it at **any** position and pass it where the function
declares it. A placement the macro cannot honour is a compile-time error at the offending token.

### Where the work stands (2026-10-10)

Rewritten from scratch. The 2026-09-02 findings (`specs/archive/2026-09-02-context-param-order-{findings,solution}.md`)
diagnosed an argument-slot index shift; **that fix has landed** in
`CommandSignature::extract_all_parameters` (`liquers-macro/src/registration.rs`), with tests
`extract_all_parameters_with_context_*`. What remains, verified through the macro's parser:

| Signature | Today |
|---|---|
| `fn f(state, a: i64, context, b: String)` | works, but no end-to-end test |
| `fn f(context, state, a: i64)` | ``expected `:` `` at `state`: state keywords are recognised only first |
| `fn f(state, context, a: i64, context)` | accepted, then a moved-value error inside the expansion |
| `fn f(value: String)` (no state) | ``expected `)` ``: `value` taken as the keyword despite the `:` |
| `fn f(a: i64, state)`, `fn f(state, state)` | rejected with the misleading ``expected `:` `` |

### Problem Example

```rust
async fn load(context: Context<CommandEnvironment>, state: State<Value>, limit: i64)
    -> Result<Value, Error> { /* … */ }
register_command!(cr, async fn load(context, state, limit: i64) -> result)?;
```

**Today:** ``expected `:` `` at `state`; the author must reorder the function, and the guides
prescribe "context last" as a rule. **Should:** it compiles, the wrapper calls
`load(context, state, limit__par)`, the metadata has one argument `limit` at index 0, and the query
`load-5` runs with `limit = 5`.

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

- **AC-10** The macro's unit tests assert and do not print
  WHEN `liquers-macro/src/registration.rs`'s test module is run or searched
  THEN it contains no `println!`, and each test that built tokens only to print them asserts on them instead (`MACRO-TESTS-PRINT-TO-STDOUT`)

**Position rules (decided 2026-10-10).** The state keyword, when present, is the first parameter
other than `context`. `context` may be anywhere, before the state included; recommended last, or
just before a `multiple` argument (AC-9). Documentation only: a stable proc-macro cannot warn.

**Non-goals.** `&Context<E>`; renaming the parameter; other state positions; hand-built
`CommandMetadata`; the JavaScript calling convention (`JS-COMMAND-CANNOT-ACCESS-CONTEXT`).

### Systems touched and crate placement

`liquers-macro` only (the signature parser and `wrapper_arguments`); generated metadata is
unchanged. `liquers-core` gets end-to-end tests in `liquers-core/tests/`, no code. No crate-flow change.

### Documentation intent

- **Reference:** `REGISTER_COMMAND_FSD.md` §Context Parameter: the position rule, the new errors,
  keyword-versus-argument for `value` / `text` / `state`.
- **Guide:** `COMMAND_REGISTRATION_GUIDE.md`: the recommendation (AC-9), an example before a
  `multiple` argument, one `context`-first example marked allowed but not recommended.
- **Updated:** `RECORD_STREAM_GUIDE.md` ("context as its last parameter"); `CLAUDE.md` DSL
  reference; `rust-best-practices` `references/anti-patterns.md`; the note in
  `JS-COMMAND-CANNOT-ACCESS-CONTEXT`.
- **Not edited:** completed designs that recorded "context last" as honoured; they are history.

### Open Questions

All resolved by the maintainer on 2026-10-10.

1. **The state is first** among parameters other than `context` (AC-6); `context` may precede it
   (AC-1). Matches `COMMAND-DECLARATION` (`WarningKind::ContextBeforeState`).
2. **`Context` stays an alias**, the same parameter for AC-4.
3. **The issue keeps its title**; Phase 5 closes it.
4. **Recommended position:** last, or immediately before a `multiple` argument (AC-9).
5. *Implementation detail:* AC-4 / AC-6 errors carry the offending token's span.

### Design Dependencies

- `COMMAND-DECLARATION` (complete) — overlaps: the language-neutral counterpart; unchanged.
- `JS-COMMAND-CANNOT-ACCESS-CONTEXT` — overlaps: its "context last" note is updated in Phase 5.
- `VARIADIC-ARGUMENTS-DECLARATION` (complete) — overlaps: its ordering check is kept as is.
- `REGISTER-COMMAND-OPTION-VALUE`, `REGISTER-COMMAND-ENUM` — weak: same file, other functions.

### Scope Changes

- 2026-10-10 — Rewritten in the five-phase compact form; old findings and solution archived.
- 2026-10-10 — AC-5 added (triage case 1): the stateless `value: String` misparse has the same cause
  (keywords recognised by position) and change site.
- 2026-10-10 — AC-9 added and questions resolved after maintainer feedback.
- 2026-10-10 — `MACRO-TESTS-PRINT-TO-STDOUT` merged in at the maintainer's request (same test
  module), as AC-10; fixed ahead of the other steps on this branch.

## Phase 2: Architecture

### Solution

Parse the parenthesised list as a sequence of **signature items** — state keyword, `context`, or
argument — recognised by **form**, then check the order and split the items into the existing
`CommandSignature` fields. The one new piece of information, where the state sits relative to a
leading `context`, becomes a field read only by `wrapper_arguments`.

- A bare `state` / `value` / `text` (an identifier **not** followed by `:`) is the state keyword;
  followed by `:` it is an argument name (AC-5). This replaces "keyword because it is first".
- `context` / `Context` is always the context parameter. Followed by `:` it is an error
  (`` `context` is reserved for the execution context and takes no type ``), which today is a
  generic parse error. It is not turned into an ordinary argument: the language-neutral
  declaration treats an argument named `context` as the context too (`COMMAND-DECLARATION`).
- After the list is collected: at most one state, at most one `context` (AC-4), and the state may
  be preceded only by `context` (AC-6). The existing `multiple`-ordering check runs unchanged.

**Rejected alternatives.**
- *A `CommandParameter::State` variant* — every `match` on `CommandParameter` would gain an arm
  with nothing to do.
- *A one-token lookahead for `context` before `StateParameter::parse`* — fixes AC-1 only; AC-5
  and AC-6 share the cause, positional keyword recognition.
- *Always pass the state first* — the call must follow the user's function signature.

**Known-issue preflight.** Nothing blocks or must go first. The open macro designs and issues
(`REGISTER-COMMAND-OPTION-VALUE`, `REGISTER-COMMAND-ENUM`, `MACRO-QUERY-VALIDATION-AND-HINTS`,
`ARGUMENT-INFO-HAS-NO-DESCRIPTION`) change argument types and metadata, not the signature parser.
`MACRO-TESTS-PRINT-TO-STDOUT` (P3, filed from this phase) is merged in and already fixed (AC-10).

**Command namespaces:** none. No command is added or changed.

### Changes

All in `liquers-macro/src/registration.rs`; `CommandSignature` and its helpers are private to the
crate, so no public interface changes.

```rust
/// One entry of a command's parenthesised parameter list, before it is split into
/// `CommandSignature::state_parameter` and `CommandSignature::parameters`.
enum SignatureItem {
    State(StateParameter, proc_macro2::Span),
    Context(proc_macro2::Span),
    Param(CommandParameter),
}

impl Parse for SignatureItem { /* keyword by form; otherwise CommandParameter::parse */ }

impl StateParameter {
    /// `Some` for `state` / `value` / `text`; `StateParameter::None` is never returned.
    fn from_keyword(ident: &syn::Ident) -> Option<StateParameter>;
}

struct CommandSignature {
    // … existing fields …
    /// Index in `parameters` before which the state is passed: 1 when `context` precedes the
    /// state, otherwise 0. Meaningless when `state_parameter` is `StateParameter::None`.
    pub state_position: usize,
}
```

- **`impl Parse for CommandSignature`**: the `StateParameter::parse` call and the hand-rolled
  comma loop become `content.parse_terminated(SignatureItem::parse, syn::Token![,])` (an empty list
  and a trailing comma stay valid), followed by an explicit `match` over `SignatureItem` that
  applies the three checks and fills `state_parameter`, `parameters` (pushing
  `CommandParameter::Context` where `context` stood) and `state_position`.
- **`impl Parse for StateParameter`** is removed; its only caller was the signature parser.
  `StateParameter::from_keyword` holds the keyword table once.
- **`CommandParameter::parse`** keeps its `context` branch for direct use (unit tests parse
  parameters on their own); both paths test the identifier with one helper, `is_context_keyword`.
- **`CommandSignature::wrapper_arguments`** inserts `state_argument_parameter()` at
  `state_position` instead of always first. `extract_all_parameters` is unchanged (it already
  skips `context` when numbering slots).
- **Errors:** `syn::Error::new(span, …)` on the offending token, like the variadic check
  (compile-time diagnostics, so `liquers_core::error::Error` does not apply):
  `` `context` is declared twice `` · `the state parameter is declared twice` ·
  `` the state parameter `<kw>` must come before every argument; only `context` may precede it `` ·
  `` `context` is reserved for the execution context and takes no type ``.
- **Sync/async:** none at runtime; the generated sync and async wrappers differ only in argument
  order, and `context` is still moved into the call after injected arguments clone it.
- **Generated metadata, registry, `impl_version`:** unchanged. `argument_info_expression` already
  skips `context`, and `version: auto` hashes the function body, not the macro invocation (AC-7).
- **Test hygiene (AC-10, done):** the four `println!` and two commented ones in `mod tests` are
  removed; `test_nostate_command_registration{1,2}`, `test_config_command_registration` and
  `test_sync_command_does_not_set_is_async_flag` now assert on the generated tokens.
- **Documents:** as in Phase 1's documentation intent, plus `rust-best-practices` `SKILL.md`,
  whose hard rule "A `context` parameter must be **last**" becomes the recommendation.

**`rust-best-practices` review.** No blocking finding: no `unwrap`/`expect`, explicit `match`
over `SignatureItem` and `StateParameter`, no runtime error type, crate flow untouched. Advisory:
`state_position` could be a `bool`; it is an index because `wrapper_arguments` passes it straight
to `Vec::insert`.

### Risks

- **Diagnostics change** for invalid signatures only; no test asserts the old text. Valid
  signatures expand to identical tokens.
- **No reclassification:** `state` / `value` / `text` without a type was never a valid argument, and
  with a type stays one (AC-7).
- **Certainty:** high. One private parser, no runtime path; rollback is reverting one file.
