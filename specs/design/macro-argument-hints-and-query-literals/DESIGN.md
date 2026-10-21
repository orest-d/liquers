---
id: MACRO-ARGUMENT-HINTS-AND-QUERY-LITERALS
kind: design
title: register_command! sets argument hints and refuses a bad query literal atomically
form: compact
status: in_review
phase: implementation
readiness: needs-decision
autofix: not-eligible
area: [macro]
issues: [MACRO-QUERY-VALIDATION-AND-HINTS]
created: 2026-10-20
---
# register_command! sets argument hints and refuses a bad query literal atomically

Bulk design (`specs/guides/autonomous_bulk_design.md`), 2026-10-20: Phases 1-4 written and
reviewed without phase approval. Nothing is implemented.

## Phase 1: High-Level Design

### Purpose

The issue asked for two things. The first is that `register_command!` should validate `query "…"`
literals. The second is that `hint` should either work or be refused. Since it was filed, the
second has been half done: argument-level `hint` is now refused at expansion
(`design/context-param-order/`, AC-11), and command-level `hint key: "…"` works
(`design/command-metadata-descriptions-and-hints/`). That design left the argument arm to this
issue. Two things remain. The argument arm should set `ArgumentInfo::hints` instead of refusing.
And a query literal that does not parse should fail registration **without leaving a
half-registered command**, with an error naming the command.

### Problem Example

Verified at HEAD (18732dd), `liquers-macro/src/registration.rs`.

```rust
register_command!(cr, fn pick(state, src: String = query "abc~X~def") -> result)?;
```

Today, `DefaultValue::parse` (≈line 36, `// TODO: Validate query`) accepts the literal. The
generated `REGISTER__pick` function (`command_registration`, ≈line 1470) calls
`registry.register_command(key, wrapper)?` **first**. It then evaluates `cm.arguments = …`, where
`TryToQuery::try_to_query("abc~X~def")?` fails with *Can't parse query 'abc~X~def': ~X~...~E is
only valid as a complete action parameter* (checked with `liquers-validate --no-registry`). The
macro returns `Err`, but `pick` stays registered with an executor and with metadata that has **no
arguments**. The error does not say which command failed. Presets, `next:` and `expires:` are parsed
after registration too, so the same holds for them.

```rust
register_command!(cr, fn plot(state, w: i64 = 80 (hint widget: "spin")) -> result)?;
```

Today this is a compile error: *argument hints are not supported; `hint` would be ignored*.

What should happen: `pick`'s registration returns `Err` carrying the command key, and `cr` holds no
`pick`. `plot` compiles, and its `w` argument's `hints` is `{"widget": "spin"}`.

### Scope and Acceptance Criteria

- **AC-1** A bad query literal leaves no trace
  WHEN a registration's default, enum value, preset or `next:` query, or its `expires:` text, does
  not parse
  THEN the registration function returns `Err`, and the registry has neither metadata nor an
  executor for that key (nor for any key it would have replaced)
- **AC-2** The error names the command
  WHEN AC-1 happens
  THEN the error's `command_key` is the command's key, and its message still contains the parse
  error and the query text
- **AC-3** Valid literals unchanged
  WHEN every literal parses
  THEN the generated metadata is identical to today's, and `specs/command_registry.yaml` does not
  change
- **AC-4** Argument hints
  WHEN a parameter's options contain `hint <key>: "<value>"` (any number of distinct keys)
  THEN that argument's `ArgumentInfo::hints` holds each key with its string value, in declaration
  order
- **AC-5** Duplicate argument hint keys
  WHEN the same key appears twice in one parameter's options
  THEN expansion fails with ``duplicate argument hint `<key>` ``, at the second key

Non-goals: compile-time query parsing (open question 1); non-string hint values (the command-level
statement has the same limit); rendering hints in a UI.

### Design Readiness

- **Readiness:** needs-decision
- **Automatic fixing:** not-eligible. It adds accepted `register_command!` syntax, the argument
  `hint` statement (rule 4), and the readiness is `needs-decision` (rule 5).
- **Leading issue:** **Proposed resolution - when query literals are checked:** at registration,
  atomically (recommended), not at compile time.
- **Explanation:** both halves are confined to `liquers-macro` codegen. `ArgumentInfo::hints`
  already exists and is serialized. The literals are already parsed at registration. They only need
  to be parsed before anything is registered, and the error needs the key attached.
- **Open questions:**
  1. **Proposed resolution - when query literals are checked.** Compile time would need
     `liquers-macro` to depend on `liquers-core` for the parser. `liquers-core` has
     `liquers-macro` as a dev-dependency, so its own tests would build `liquers-core` twice. Every
     wasm build would also compile `liquers-core` for the host, so this cost hits every
     consumer. Registration time is already early: every registration runs in
     `register_all_commands` and `registry_export`, and an environment cannot be built past a
     failing one. Recommended: registration time, atomic, with the key (AC-1, AC-2). The issue's
     "validate at expansion" is then closed as not planned for that reason. Consequence: a typo in
     a query literal shows up when the tests run, not when the code compiles.
  2. **Proposed resolution - argument hints.** Implement them (AC-4, AC-5), mirroring the command
     statement, rather than keep refusing. `ArgumentInfo::hints` exists, and the egui metadata view
     already counts them (`liquers-lib/src/egui/widgets.rs`).

### Design Dependencies

- `pure-command-context` (in_review) — `overlaps` (weak): it adds a `pure:` command statement in
  the same parser (`CommandSignatureStatement`), a different arm. Either order. The second to land
  rebases.
- `registry-query-default-text-form` (in_review) — `overlaps` (weak): it changes how a
  `CommandParameterValue::Query` default is *serialized*. This design changes when it is *parsed*.
  They are independent.
- `command-metadata-descriptions-and-hints`, `context-param-order` (complete): origins of the
  command-level `hint` statement and of the current refusal, which AC-4 replaces.

### Consolidated Findings

- The non-atomic registration is a defect beyond the issue's text. It is fixed here (triage case 1)
  because it is the same code path, and no separate issue is filed.
- `ArgumentInfo` is generated as a struct literal with `..Default::default()`, so `hints` is one
  more field. When no hint is given, the field is omitted and the output is byte-identical (AC-3).
- `REGISTER_COMMAND_FSD.md` §Parameter Metadata says argument hints are refused. §Default Values
  says a query "resolves at runtime". Both change.

## Phase 2: Architecture

### Solution

All changes are in `liquers-macro/src/registration.rs`.

1. **Atomic registration.** In `CommandSignature::command_registration`, evaluate every fallible
   piece into locals *before* `#reg_command`:
   `let arguments = #arguments; let presets = …; let next = …; let expires = …;`. Each
   `try_to_query(…)?`, `CommandPreset::new(…)?` and `expires.parse()?` inside them gets
   `.map_err(|e| e.with_command_key(&key))`, where `let key = #command_key_expression;` is computed
   first. `cm.arguments = arguments;` and the others follow the registration call, and nothing
   after it can fail. `DefaultValue::Query` and `EnumValueLiteral::Query` codegen (≈691, ≈208)
   then emit `TryToQuery::try_to_query(#value).map_err(|e| e.with_command_key(&key))?`.
2. **Argument hints.** `CommandParameterStatement` gains `Hint(syn::Ident, String)`, parsed as
   `hint <ident>: "<str>"`, the same grammar as the command statement (`Ident::parse_any` for the
   key). The `"hint" => Err(…)` arm is removed. `CommandParameter::Param` gains
   `hints: Vec<(String, String)>`. The parameter parser rejects a repeated key with
   ``duplicate argument hint `<key>` `` at the second key's span. `argument_info_expression` emits
   `hints: { let mut m = serde_json::Map::new(); m.insert(#k.to_string(), #v.into()); … m },`
   only when `hints` is non-empty.
3. `DefaultValue::parse`: the `// TODO: Validate query` comment is replaced by a pointer to
   registration-time parsing.

Rejected: a `liquers-core` dependency for compile-time parsing (open question 1); a hand-written
query syntax check in the macro (a second parser that would drift from `parse.rs`).

### Changes

- `liquers-macro/src/registration.rs`: `command_registration`, `register_command` (unchanged
  call, moved), `DefaultValue` / `EnumValueLiteral` / `CommandPreset` codegen,
  `CommandParameterStatement`, `CommandParameter::Param`, the parameter parser,
  `argument_info_expression`, unit tests (`argument_hint_option_is_rejected` is replaced).
- `liquers-core/tests/register_command_literals.rs` (new): runtime tests through the macro.
- Documents: `reference/REGISTER_COMMAND_FSD.md` (§Syntax line, §Parameter Metadata table and the
  rejection paragraph, §Default Values "Query" row, §Generated Code if it shows the order),
  `guides/COMMAND_REGISTRATION_GUIDE.md` (§Macro DSL Syntax), `CLAUDE.md` DSL summary
  ("optionally `(label: …, description: …, gui: …, hint key: …)`"), each with History rows where
  required.

Generated code stays sync. No `liquers-core` API changes. `with_command_key` already exists
(`liquers-core/src/error.rs`).

### Risks

| Assessment | Finding |
|---|---|
| Files likely to change | `registration.rs`, one new core test file, two specs, `CLAUDE.md` |
| Existing tests | token-string tests that assert `try_to_query(\"…\")?` (≈3242) change to the mapped form. `argument_hint_option_is_rejected` is replaced |
| Compatibility | no command in the workspace uses an argument hint or a failing literal. `registry_export` stays green (AC-3) |
| Wasm | the generated code uses only `liquers_core` and `serde_json`, as today |
| Recovery | revert. The two halves are independent commits |
| Certainty | high; the codegen order and every fallible statement were read at HEAD |

## Phase 3: Examples and Tests

### Examples

```rust
let e = register_command!(cr, fn pick(state, src: String = query "abc~X~def") -> result)
    .unwrap_err();
assert_eq!(e.command_key, Some(CommandKey::new_name("pick")));        // AC-2
assert!(cr.command_metadata_registry.get(CommandKey::new_name("pick")).is_none()); // AC-1
register_command!(cr, fn plot(state, w: i64 = 80 (hint widget: "spin", hint unit: "px")) -> result)?;
// plot.arguments[0].hints == {"widget": "spin", "unit": "px"}                     // AC-4
```

The failing literal `abc~X~def` was checked with `liquers-validate --no-registry` (ParseError at
column 4).

### Tests

Unit, `liquers-macro/src/registration.rs` `tests`:

- `argument_hint_statements_are_collected_in_order` (AC-4): parses two hints, checks
  `CommandParameter::Param::hints`.
- `argument_hint_codegen_inserts_into_hints` (AC-4): the token string contains
  `m . insert ("widget" . to_string () , "spin" . into ())`.
- `duplicate_argument_hint_key_is_rejected` (AC-5): `signature_error` contains
  ``duplicate argument hint `widget` ``.
- `registration_parses_literals_before_registering` (AC-1, AC-3): in the generated token string,
  `try_to_query` and `CommandPreset :: new` occur before `register_command (`.

Integration, new `liquers-core/tests/register_command_literals.rs`:

- `bad_default_query_leaves_registry_unchanged` (AC-1, AC-2): the `pick` example, metadata and
  executor both absent (an evaluation of `pick` reports it unregistered), `command_key` set.
- `bad_preset_leaves_registry_unchanged` (AC-1): a `preset:` whose action does not parse.
- `bad_literal_does_not_clobber_an_existing_command` (AC-1): `pick` registered validly, then
  re-registered with a bad literal. The first registration still evaluates.
- `argument_hints_reach_metadata` (AC-4): the `plot` example.
- `valid_literals_unchanged` (AC-3): `cargo test -p liquers-lib --test registry_export`.

Run: `CARGO_INCREMENTAL=0 cargo test -p liquers-macro`, `cargo test -p liquers-core --test
register_command_literals`, `cargo test -p liquers-lib --test registry_export`.

## Phase 4: Implementation Plan

### Steps

- [ ] 1. Record the answers to questions 1 and 2 in Phase 1 — review only
- [ ] 2. `registration.rs` `command_registration` and the query / preset / expires codegen —
  compute `key` and every fallible piece before `#reg_command`, map errors with
  `with_command_key` — `cargo test -p liquers-macro registration_parses_literals_before_registering`
- [ ] 3. `liquers-core/tests/register_command_literals.rs` — the three AC-1 / AC-2 tests —
  `cargo test -p liquers-core --test register_command_literals`
- [ ] 4. `registration.rs` `CommandParameterStatement::Hint`, `CommandParameter::Param::hints`,
  parser duplicate check, `argument_info_expression` — `cargo test -p liquers-macro argument_hint`
- [ ] 5. `argument_hints_reach_metadata` — `cargo test -p liquers-core --test register_command_literals`
- [ ] 6. `cargo test -p liquers-lib --lib --tests` (all registrations, `registry_export`) — no
  change in `specs/command_registry.yaml`
- [ ] 7. Documents (REGISTER_COMMAND_FSD, COMMAND_REGISTRATION_GUIDE with History rows and
  `reviewed:`, `CLAUDE.md` DSL line); close `MACRO-QUERY-VALIDATION-AND-HINTS` with the question 1
  decision recorded; this design to `complete` — `python3 scripts/docs_index.py` then `--check`

### Validation

`cargo fmt --check -p liquers-macro`; `bash scripts/check-build-matrix.sh` is not needed (no
`cfg(feature)` change). The wasm32 check of `liquers-lib` is covered by the generated code using
the same paths as today. Diff review: `liquers-macro`, one core test file, documents.
