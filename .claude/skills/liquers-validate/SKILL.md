---
name: liquers-validate
description: >-
  Check that Liquers queries and recipes parse, plan, and mean what you intended, using the
  `liquers-validate` CLI. Use this whenever a Liquers query string is written, edited, quoted or
  debugged — in a doc example, a spec, a design document, a test fixture, a `recipes.yaml`, a
  code comment, an axum route example or a chat answer — and before any such query is committed.
  Trigger on anything shaped like a query (`-R/…`, `/-/`, `ns-pl/`, `ns-img/`, a dash-separated
  action chain, a trailing filename segment), on "does this query work / why doesn't this query
  work", on writing recipes, on designing commands that do not exist yet, and on suspicion that
  `specs/command_registry.yaml` is stale. Cheap and offline: it never opens a store or runs a
  command. Not for evaluating queries or debugging command *implementations*.
---

# Validating Liquers queries and recipes

`liquers-validate` parses a query and builds its plan. No store is opened, no command runs, so
it is fast and safe to run on anything, including queries against data that does not exist.

## The one thing that matters

**A clean result tells you what your query *means*, not that it is correct.** Both of these
validate `Ok` with exit 0:

```
-R/data/report.txt/-/to_text     ->  GetAsset[data/report.txt], action to_text()
-R/data/report.txt/to_text       ->  GetAsset[data/report.txt/to_text]
```

The second fetches a file *named* `to_text`. `-R/` swallows the rest of the string as a key
until `/-/` starts a new segment. No validator can call this an error, because it isn't one.

So validation is two questions, and the tool only answers the first:

1. **Does it parse and plan?** — status, warnings, exit code. The tool's job.
2. **Does the plan match what I meant?** — read the resolved steps. Your job.

Skipping question 2 is how a wrong query gets a green check. Always read the steps.

## Run it

```bash
python3 .claude/skills/liquers-validate/scripts/lqv.py -- '<query>' ['<query>' …]
```

`lqv.py` runs the validator and renders the part you have to read. The raw envelope carries a
`position` object on every element, so a two-step plan is ~40 lines of JSON; the digest is four.
It passes all options through, finds or builds the binary, and returns the validator's exit code.
Add `--raw` for the full JSON when you need a field the digest omits.

```
$ python3 .claude/skills/liquers-validate/scripts/lqv.py -- \
    '-R/data/report.txt/-/to_text' '-R/data/report.txt/to_text'
level=Plan  status=Ok  registry=95 commands (command_registry.yaml)  namespaces=['', 'root']
results: 2 total, 2 ok, 0 warning, 0 error

[0] OK      -R/data/report.txt/-/to_text
    plan     GetAsset[data/report.txt]
    plan     action to_text()

[1] OK      -R/data/report.txt/to_text
    plan     GetAsset[data/report.txt/to_text]
```

Two `Ok` results; the plans say they are different queries. That comparison is the whole point.

To call the binary directly instead (in a script, or to see the raw envelope):

```bash
cargo run -p liquers-core --features cli --bin liquers-validate -- -- '<query>'
```

**The `--` is not decoration.** Queries start with `-`, which is exactly the shape of a flag, so
the tool declares its positionals with `allow_hyphen_values` and has no short flags at all. A
consequence worth knowing: an unrecognized flag is not rejected, it is parsed *as a query* —
`liquers-validate --version` reports `Can't parse query '--version'`.

**Zero setup in this repo.** `specs/command_registry.yaml` is committed, and the tool finds it by
walking up from the working directory, so it validates against the real 95-command set and
defaults to plan level with no arguments.

## Which flags for which situation

| Situation | Flags |
|---|---|
| Ordinary check of a query in this repo | none — the committed registry is found automatically |
| Parse only, ignore the registry | `--no-registry` (drops to parse level; no plan is built) |
| Command doesn't exist yet (design work) | `--command my_new_command` — accepts any arguments, repeatable, merges *with* the committed registry |
| Namespaced not-yet-written command | `--command 'ns/name'` or `--command 'realm/ns/name'` |
| Proposal that *changes* an existing signature | `--registry-file specs/command_registry.yaml --registry-file proposal.yaml --allow-overwrite` |
| A whole `recipes.yaml` | `--recipes recipes.yaml --cwd <folder>` |
| Many queries | list them positionally, or `--query-file <file>` / `--query-file -` for stdin |
| Just pass/fail in a script | discard stdout; read the exit code and the `ERROR`/`WARNING` lines on stderr |

`--allow-overwrite` is needed *only* when an overlay redefines a key that already exists.
Redefining without it is an error on purpose — otherwise a typo'd command name in an overlay
would silently shadow a real command instead of being reported.

Exit codes: **0** ok or warning · **1** a query failed · **2** the tool was invoked wrongly
(stdout stays empty, so "empty stdout" reliably means "I got the invocation wrong" — read stderr).

## Known blind spots

These pass validation. Only reading the plan catches them.

- **`-R/` swallows actions into the key** unless `/-/` separates them. The example above. The
  status cannot distinguish the two; the steps can.
- **Excess action parameters are silently dropped.** `to_text-extra-args` validates `Ok` as
  `action to_text()`, and `ns-pl/select_columns-name-price` resolves to `columns="name"` —
  `price` is gone, even though the command's own doc says "separated by dashes". This is a gap in
  the plan builder, tracked as `PLAN-EXCESS-ACTION-PARAMETERS-DROPPED` in `specs/ISSUES.md`.
  Until it warns, check that every parameter you wrote appears in the digest's `action …(…)` line.
- **`--cwd` never changes the plan.** It only changes the storage key a recipe result lands
  under. The same recipe under `--cwd reports` and `--cwd archive` produces byte-identical steps.
- **A missing namespace is an error, not a mis-plan** — `head-5` fails with `Action 'head' not
  registered in namespaces '', 'root'` because it needs `ns-pl/head-5`. Easy to fix, easy to
  misread as "the command doesn't exist".

## When something fails

| Message | Cause |
|---|---|
| `Can't parse query '…': Can't parse query completely` | Syntax — a space, a stray `#`, or a flag that reached the positional slot. `error.position` gives line and column. |
| `Action 'x' not registered in namespaces '', 'root'` | Missing namespace prefix (`ns-pl/`, `ns-img/`), a genuine typo, or a command that does not exist yet — use `--command x` if you are designing it. |
| `Can't convert 'notanumber' to integer` | Argument *type*. Only plan level finds this; parsing cannot. |
| `--cwd applies to recipes only` (exit 2) | `--cwd` without `--recipes`. Nothing in a bare query's plan consumes a cwd. |
| Empty stdout, exit 2 | Bad invocation. The reason is on stderr. |
| Command exists in the code but not to the validator | `specs/command_registry.yaml` is stale — regenerate it, see below. |

## Keeping the registry current

`specs/command_registry.yaml` is **generated — never edit it by hand.** Regenerate whenever a
`register_command!` signature changes or a command is added or removed:

```bash
cargo run -p liquers-lib --features cli --bin export-command-registry -- \
  --format yaml -o specs/command_registry.yaml
```

Then add a dated line inside the `# CHANGELOG-BEGIN` / `# CHANGELOG-END` markers — the exporter
carries that block over verbatim, and it is the only hand-maintained part of the file.
`cargo test -p liquers-lib --test registry_export` fails when the file no longer matches the
registered commands, comparing signatures rather than bytes.

## Going further

- `references/output-format.md` — every field of the JSON envelope, the status/warning/exit-code
  rules, and what `--detail summary` drops. Read when scripting against the output or when the
  digest omits something you need.
- `references/recipes-and-overlays.md` — recipe validation (`AdHoc` vs `Stored`, `arguments` and
  `links` overrides, the payload boundary), and registry overlays for design work. Read when
  validating a `recipes.yaml` or a design document's queries.
