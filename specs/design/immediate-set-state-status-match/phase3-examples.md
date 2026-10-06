# Phase 3: Examples and Tests

| # | Kind | Checks |
|---|---|---|
| T1 | unit | `written_status` over every `Status` variant: `Expired`→`Some(Expired)`, `Error`→`Some(Error)`, others→`None` |
| T2 | unit | `written_status_with_recipe(true) == Override`, `(false) == Source` |
| T3 | regression | Existing `set_binary`/`set_state` tests on both managers (e.g. the Override-vs-Source tests) pass unchanged |

T1 iterates a hand-written array of all variants. Add a comment that a new variant must be added
there. The exhaustive match in `written_status` already makes the compiler flag the function.

Names: `written_status_keeps_expired_and_error`, `written_status_defers_others_to_recipe`.
