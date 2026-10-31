---
id: MACRO-QUERY-VALIDATION-AND-HINTS
kind: issue
title: "`register_command!` does not validate queries or implement hints"
status: accepted
priority: P3
complexity: M
area: [macro]
design: macro-argument-hints-and-query-literals
created: 2026-08-08
github:
---
## Problem

`liquers-macro/src/registration.rs:36` — `// TODO: Validate query` on the default-value literal
parser; `:1002` — `Hint(String, String), // TODO: Implement hints`; `:1610` — the hint statement is
matched and discarded.

## Impact

A malformed `query "…"` default reaches runtime instead of failing the build, which is exactly the
class of error `liquers_core::validate` now exists to catch — the macro could call it. Hints are
accepted syntax that does nothing, which is worse than not accepting them.

## Expected behaviour

The macro validates query literals at expansion time and either implements hints or rejects them.

## Discovery

Migration triage, 2026-08-08. Source: `todo20260219.md` #16, work package WP-15. Verified against HEAD: markers present — but **moved** from `lib.rs` to `registration.rs`, so the audit path is stale while the issue is live. See `specs/archive/2026-08-08-docs-migration-plan.md` §4.0c.

## Progress

2026-10-10 — the hint half is planned in `design/context-param-order/` (AC-11): the
argument-level `hint` option will be rejected at expansion rather than discarded, which the expected
behaviour above allows. Implementing argument hints belongs with
`design/command-metadata-descriptions-and-hints/`. Query-literal validation remains open here.

## Re-scoped 2026-10-20 (bulk design)

Checked at HEAD (18732dd). What still holds:

- `registration.rs` ≈36 still reads `// TODO: Validate query`; a `query "…"` literal is parsed
  only by the generated `REGISTER__<name>` function. Worse than the text above says: that function
  registers the command **before** it parses the literals (`command_registration` ≈1470), so a bad
  literal returns `Err` but leaves the command registered with no arguments, and the error does
  not name the command. Presets, `next:` and `expires:` behave the same.
- The `Hint(String, String) // TODO` and the discarded match are gone. The argument-level `hint`
  is refused at expansion (≈1189, `argument_hint_option_is_rejected`), and command-level
  `hint key: "…"` works (`design/command-metadata-descriptions-and-hints/`, which left the argument
  arm to this issue).

Problem example:

```rust
register_command!(cr, fn pick(state, src: String = query "abc~X~def") -> result)?;
// Err (ParseError, no command named), yet `pick` stays registered with no arguments.
register_command!(cr, fn plot(state, w: i64 = 80 (hint widget: "spin")) -> result)?;
// compile error: argument hints are not supported; `hint` would be ignored
```

The remaining scope is: setting `ArgumentInfo::hints` from the macro, and atomic,
command-named failure on a bad literal. Compile-time parsing is proposed as *not planned* (it would
make `liquers-macro` depend on `liquers-core`). Design: `design/macro-argument-hints-and-query-literals/`
(needs-decision).
