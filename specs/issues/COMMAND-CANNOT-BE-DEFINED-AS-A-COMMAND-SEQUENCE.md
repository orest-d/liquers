---
id: COMMAND-CANNOT-BE-DEFINED-AS-A-COMMAND-SEQUENCE
kind: feature
title: A command cannot be defined as a sequence of other commands with placeholders
status: draft
priority: P2
complexity: L
area: [core/commands, core/plan, core/query, core/assets, axum]
design: composite-command-definition
created: 2026-11-01
github: 237
---

## Problem

The only command definition that is not Rust (or Python) code is an **alias**
(`CommandDefinition::Alias`, `liquers-core/src/command_metadata.rs`;
`specs/reference/COMMAND_ALIASES.md`): one target command with some *leading* arguments fixed. An
alias cannot:

- run more than one command;
- fix an argument that is not a leading one, reorder arguments, or use one argument twice;
- pass its input state to an argument of a later command, or start from an argument instead of
  its input state.

So a recurring pipeline such as "parse the CSV, keep the given regions, sort by a column, take the
first `n` rows" stays query text that every caller repeats:

```
-R/data/sales.csv/-/ns-pl/from_csv/is_in-region-EU-US/sort-amount-f/head-5
```

The same limit shows up in two more places:

- **Recipes.** `Recipe.arguments` and `links` fill only arguments of the query's **last** action
  (`Recipe::to_plan`, `liquers-core/src/recipes.rs`; tracked for the earlier actions as
  `RECIPE-OVERRIDE-ONLY-LAST-ACTION`). A list, map or long text that belongs to an earlier action
  cannot be given at all, because it has no query text form.
- **Web API.** Named overrides (`specs/reference/WEB_API_SPECIFICATION.md` §2.4) have the same
  last-action limit, and a request cannot supply the value a query starts from (an uploaded table,
  a JSON document) except by first storing it under a key.

## Impact

Library authors write Rust commands that only chain other commands, and recipe and API users
reorder queries or split them into several recipes to reach an argument. A long or structured value
that is not the last action's argument cannot be passed without a store write. Workaround: a Rust
command per pipeline, or a recipe per variant.

## Expected behaviour

A new command definition, next to `Registered` and `Alias`: a **sequence of commands** registered
as one command, whose body is query text with **placeholders** that may stand for any parameter of
any of its commands. The command's input state may be passed to an argument of a command in the
sequence, and one of its arguments may be the state the sequence starts from.

The same placeholder mechanism should serve:

1. the command metadata registry, as a more flexible kind of alias;
2. recipes, so that a recipe fills a placeholder anywhere in its query, with complex data
   (lists, maps, long text) as the value;
3. the web API, so that a request binds placeholders anywhere in a query and can supply the value
   the query starts from.

## Discovery

Requested by the maintainer on 2026-11-01, while reviewing the per-realm default namespaces design
(`plan-default-namespaces`): "a sequence of commands as a new command", with placeholders in any
argument, initial state passed as an argument and an argument passed as initial state, designed
for command aliases, recipes and the web API.
