# Phase 3: Examples and Tests

| # | Kind | Checks |
|---|---|---|
| E1 | scenario | `a.txt` (expires in 1 s) ← `b.txt`; wait past the deadline; request `a.txt`; `b.txt` is `Expired` with `Cascaded{Deadline, root a.txt, via a.txt}` |
| E2 | scenario | Then request `b.txt`: it recomputes (command call counter increments) |
| E3 | doc example | Residual: request `b.txt` first → old value (documented, asserted as current behaviour so a later fix flips it consciously) |
| T1 | regression | Update any immediate-manager test that pinned "dependent stays Ready" |

## Setup

Recipes (in-memory recipe provider used by the existing manager scenarios):

- `a.txt`: query `make_a`, where `make_a` is registered with
  `register_command!(cr, fn make_a() -> result expires: "in 1 sec")` and returns
  `Value::from("a")`.
- `b.txt`: query `-R/a.txt/-/append_b`, where `append_b(state) -> result` appends "b" and counts
  calls in an `AtomicUsize`.

Validate both recipe queries with `liquers-validate --command make_a --command append_b` before
committing them to the test. The resource segment `-R/a.txt/-/append_b` needs the `/-/` separator.

Timing: use a controllable clock if the scenarios provide one. Otherwise sleep 1.1 s on the
immediate manager only, matching the existing deadline tests' approach.

Names: `lazy_deadline_expiry_cascades_to_dependents` (shared scenario),
`dependent_read_before_root_serves_last_value_on_immediate_manager`.
