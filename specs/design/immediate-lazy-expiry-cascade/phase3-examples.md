# Phase 3: Examples and Tests

| # | Kind | Checks |
|---|---|---|
| E1 | scenario (both managers) | `a.txt` (expires in 1 s) ← `b.txt`; after the deadline, request `a.txt`; `b.txt` is `Expired` with `Cascaded{Deadline, root a.txt, via a.txt}` |
| E2 | scenario | Then request `b.txt`: it recomputes (call counter +1) |
| E3 | scenario (immediate only) | Residual case: request `b.txt` first → last value; asserted as current behaviour so a future change flips it deliberately |
| T1 | regression | Update any immediate-manager test that pinned "dependent stays Ready" |

Recipes: `a.txt` = `make_a` (`register_command!(cr, fn make_a() -> result expires: "in 1 sec")`),
`b.txt` = `-R/a.txt/-/append_b` (`append_b(state) -> result`, counts calls). Validate the recipe
queries with `liquers-validate --command make_a --command append_b`.

Timing: use the scenarios' controllable clock if available, otherwise sleep 1.1 s on the immediate
manager only, like the existing deadline tests.

Names: `lazy_deadline_expiry_cascades_to_dependents`,
`dependent_read_before_root_serves_last_value_on_immediate_manager`.
