# Phase 2: Solution & Architecture — Context parameter at any position

## Overview

Parse a command's parenthesised list as **signature items** (state keyword, `context`, argument)
recognised by **form**, check their order, and split them into the existing `CommandSignature`
fields; one new field, `state_position`, lets `wrapper_arguments` follow the declared order
(AC-1…AC-7). The argument-option parser rejects `hint` instead of discarding it (AC-11). The
command-argument error messages in `liquers-core` add 1 to the slot index they print (AC-12).
AC-10 is already implemented (`a849a47`).

- A bare `state` / `value` / `text` (identifier **not** followed by `:`) is the state keyword;
  followed by `:` it is an argument name (AC-5).
- `context` / `Context` is always the context parameter; followed by `:` it is an error, because
  the language-neutral declaration (`COMMAND-DECLARATION`) also treats an argument named `context`
  as the context.

Rejected:
- *A `CommandParameter::State` variant* — every `match` on `CommandParameter` gains an empty arm.
- *A lookahead for `context` before `StateParameter::parse`* — fixes AC-1 only; AC-5 and AC-6 share
  the cause, positional keyword recognition.
- *Always pass the state first* — the call must follow the user's function signature.
- *Implement argument hints now* — needs a `liquers-core` builder that
  `COMMAND-METADATA-DESCRIPTIONS-AND-HINTS` designs; two designs adding hint builders would collide.
- *1-based slot numbers in the API* (`get(i)` with `i` from 1) — the slot is an index shared with
  the planner and metadata; only the human-readable message changes.

## Known-Issue Preflight

| Issue | Status | Priority | Impact on this design | Fix first? | Blocking? | Action |
|---|---|---|---|---|---|---|
| `COMMAND-METADATA-DESCRIPTIONS-AND-HINTS` (design) | in_review | P3 | Its step 2 adds a *command-level* `hint`; AC-11 rejects the *argument-level* one in another enum | no | no | Note in its Phase 4 that implementing argument hints replaces AC-11's rejection |
| `MACRO-QUERY-VALIDATION-AND-HINTS` | accepted | P3 | AC-11 answers its hint half ("implements hints or rejects them") | no | no | Progress note in Phase 5; query validation stays open |
| `REGISTER-COMMAND-OPTION-VALUE`, `REGISTER-COMMAND-ENUM` (designs) | open | P3 | Change argument types, not the signature parser | no | no | None |
| `ERROR-MESSAGE-POSITIONS-MIXED-BASE` | draft | P3 | Same rule outside command arguments; filed from this phase | no | no | None |

## Interfaces

All macro items are private to `liquers-macro/src/registration.rs`; no public interface changes.

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
    /// `Some` for `state` / `value` / `text`; never `StateParameter::None`.
    fn from_keyword(ident: &syn::Ident) -> Option<StateParameter>;
}

struct CommandSignature {
    // … existing fields …
    /// Index in `parameters` before which the state is passed: 1 when `context` precedes the
    /// state, otherwise 0. Unused when `state_parameter` is `StateParameter::None`.
    pub state_position: usize,
}

enum CommandParameterStatement {
    Label(String), Gui(ArgumentGUIInfo), Enum(EnumParameterSpec), EnumRef(String),
    // `Hint(String, String)` removed: `hint` is rejected while parsing (AC-11).
}
```

`liquers-core`: signatures unchanged. `Error::missing_argument(i, name, position)` keeps `i` as the
0-based slot and documents that the message shows `i + 1`.

Sync/async: compile-time only in the macro; the generated sync and async wrappers differ only in
argument order. The runtime messages are built on existing sync paths.

## Integration Points

`liquers-macro/src/registration.rs`:
- `impl Parse for CommandSignature` — `StateParameter::parse` and the comma loop become
  `content.parse_terminated(SignatureItem::parse, syn::Token![,])`, then one `match` over
  `SignatureItem` applying the three order checks and filling `state_parameter`, `parameters` and
  `state_position`. The variadic check after it is unchanged.
- `impl Parse for StateParameter` — removed (only caller above); `from_keyword` holds the table.
- `CommandParameter::parse` — keeps its `context` branch for direct use; both paths call
  `is_context_keyword`.
- `CommandSignature::wrapper_arguments` — inserts the state at `state_position`.
- `impl Parse for CommandParameterStatement` — the `"hint"` arm returns the AC-11 error; the
  `Hint(_, _)` arm in the argument-option loop goes away.

`liquers-core/src/commands.rs` (`CommandArguments::get_value`, `get`, `get_multiple`,
`convert_multiple_element`) and `liquers-core/src/error.rs` (`missing_argument`): print `i + 1`.

## Error Handling

Macro diagnostics are `syn::Error::new(span, …)` on the offending token, as the existing variadic
check does (compile time, so `liquers_core::error::Error` does not apply):

| Case | Message |
|---|---|
| AC-4 | `` `context` is declared twice `` |
| AC-6 | `the state parameter is declared twice` · `` the state parameter `<kw>` must come before every argument; only `context` may precede it `` |
| `context: T` | `` `context` is reserved for the execution context and takes no type `` |
| AC-11 | `` argument hints are not supported; `hint` would be ignored `` |

Runtime (AC-12), constructors unchanged (`Error::missing_argument`, `general_error`,
`unexpected_error`), wording normalised to `argument #<n> '<name>'`, e.g.
`Missing argument #1 'limit'`, `Unresolved link in argument #2 'path': <query>`.

## Relevant Commands

None added or changed. No command namespace is involved.

## Documentation Architecture

| Path | Kind | Change |
|---|---|---|
| `specs/reference/REGISTER_COMMAND_FSD.md` | reference, macro | §Context Parameter: position rule, recommendation, new diagnostics; keyword-versus-argument; argument hints unsupported |
| `specs/guides/COMMAND_REGISTRATION_GUIDE.md` | guide, macro | Recommendation with examples (AC-9) |
| `specs/guides/RECORD_STREAM_GUIDE.md` | guide, records | "context as its last parameter" becomes the recommendation |
| `CLAUDE.md` | repo guide | DSL syntax: `context` anywhere, recommended last |
| `.claude/skills/rust-best-practices/SKILL.md`, `references/anti-patterns.md` | skill | Hard rule "context last" becomes the recommendation |
| `specs/issues/JS-COMMAND-CANNOT-ACCESS-CONTEXT.md` | issue | Note no longer says the macro requires context last |

`affects_docs`: `REGISTER_COMMAND_FSD`, `COMMAND_REGISTRATION_GUIDE`, `RECORD_STREAM_GUIDE`. No new
`specs/README.md` link; its line for this design moves to complete in Phase 5.

## Risks

| Assessment | Finding |
|---|---|
| Files likely to change | `liquers-macro/src/registration.rs`; `liquers-core/src/{commands,error}.rs`; new `liquers-core/tests/` file |
| Crates and workflows affected | `liquers-macro`, `liquers-core`; every crate using the macro recompiles |
| Existing tests likely to change | None assert today's parse-error text or the runtime wording (`grep`) |
| New validation | Macro unit tests per diagnostic; end-to-end registration and evaluation tests |
| Compatibility | Valid signatures expand identically; `hint` breaks no use (none exists); message text changes for tools that parse it (none found) |
| Recovery | Revert the commits; no data or format change |
