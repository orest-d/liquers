# Recipes and registry overlays

Two cases the plain "validate a query" path does not cover: validating a whole `recipes.yaml`,
and validating queries against commands that do not exist yet (or whose signature a design
document proposes to change).

## Contents

- [Validating a recipe list](#validating-a-recipe-list)
- [What recipe validation checks beyond the query](#what-recipe-validation-checks-beyond-the-query)
- [Stored vs AdHoc](#stored-vs-adhoc)
- [`--cwd`](#--cwd)
- [The payload boundary](#the-payload-boundary)
- [Registry overlays for design work](#registry-overlays-for-design-work)

## Validating a recipe list

```bash
python3 .claude/skills/liquers-validate/scripts/lqv.py --recipes recipes.yaml --cwd reports
```

The file is the `RecipeList` shape — a `recipes:` list of `{query, title, description, arguments,
links, cwd, volatile, expires}`. JSON and YAML are both accepted and auto-detected; a file that is
neither reports **both** parsers' messages, since neither failure alone identifies the problem.
`--recipes -` reads stdin.

Results come back in list order, one per recipe, each carrying its `title`, so a failure is easy
to locate:

```
[1] ERROR   -R/data/sales.csv/-/ns-pl/head-10/bad.csv
    title    Bad override name
    recipe   Stored, key reports/bad.csv
    ERROR    Argument nosuchparam not found in last action
```

One bad recipe does not abort the batch — every recipe is reported.

## What recipe validation checks beyond the query

Recipe validation runs `Recipe::to_plan` (or `to_plan_for_key`), which checks more than a bare
query would:

- **The query itself** — parse and plan, as usual.
- **`arguments` overrides name a real parameter of the last action.** A typo is an error:
  `Argument nosuchparam not found in last action`. Valid overrides show up in the plan as
  `override n=25` rather than `n=25`, so the digest tells you the override actually landed.
- **`links` overrides** are checked the same way.
- **The payload boundary**, for stored recipes only — see below.
- **`cwd` conflicts**, when `--cwd` is also supplied.

This is why validating `recipes.yaml` as a recipe list beats extracting the query strings and
validating those: the overrides are checked only on the recipe path.

## Stored vs AdHoc

`recipe_check` reports which path ran:

| Value | When | Consequence |
|---|---|---|
| `Stored` | The recipe's query ends in a filename segment, so `store_to_key()` yields a key | `to_plan_for_key` runs; `key` is reported; the payload boundary is enforced |
| `AdHoc` | No filename, no storage key | `to_plan` runs; the payload boundary does not apply |

Ad-hoc recipes are first-class, not a degenerate case. `…/ns-pl/head-10` is `AdHoc`;
`…/ns-pl/head-10/preview.csv` is `Stored` with key `reports/preview.csv` under `--cwd reports`.

`recipe_check` is absent when the recipe's own query does not parse, because the key could not be
computed.

## `--cwd`

`--cwd` supplies the working directory `DefaultRecipeProvider` would supply — the folder key a
recipe's result lands under.

**It does not change the plan.** The same recipe list under `--cwd reports` and `--cwd archive`
produces byte-identical `plan.steps`; only `key` differs (`reports/preview.csv` vs
`archive/preview.csv`), because `Recipe::to_plan` never consults `cwd`. Expecting `--cwd` to
resolve relative keys *inside* the plan leads to confusion.

`--cwd` without `--recipes` exits 2 rather than being silently ignored: nothing in a bare query's
plan consumes a cwd.

**A recipe that declares its own `cwd`** while `--cwd` is also given is reported per recipe as
`CWD can't be explicitly specified in a recipe`. This is a genuine finding, not a tool limitation
— `DefaultRecipeProvider` refuses it the same way in production. Note the recipe's own `cwd` still
shows in the reported key, so read the error, not just the key.

## The payload boundary

A key names a single **shared** asset; a payload is supplied **per evaluation**. A keyed recipe
that required a payload would need a global one, which is incoherent — so `to_plan_for_key`
rejects it:

> Recipe for key '…' requires an evaluation payload, but keyed recipes cannot receive one …

This only applies to `Stored` recipes. The same query as an `AdHoc` recipe is fine. In the
envelope, `plan.payload_required` is `None` or `Required`; `Required` also implies volatility.

## Registry overlays for design work

### A command that does not exist yet

```bash
python3 .claude/skills/liquers-validate/scripts/lqv.py --command my_new_command -- '<query>'
```

`--command` declares a **permissive** command that accepts any number of arguments of any type
(it carries one `any_argument("arguments").set_multiple()`). Forms: `name`, `ns/name`,
`realm/ns/name`. Repeatable.

It **merges with** the committed registry rather than replacing it, so a query mixing existing
commands with one being designed validates in a single run — `command_count` goes to 97 with two
`--command` flags against the 95-command registry. Because it accepts anything, it proves the
query *parses and plans*, not that the arguments are right; that is the trade for validating code
that does not exist.

### A proposal that changes an existing signature

Once a design document proposes a concrete signature, an overlay file is better than `--command`.

**Build the overlay by copying the real entry out of `specs/command_registry.yaml` and editing
it.** An overlay file is a whole `CommandMetadataRegistry`, and most of its fields have no serde
default — `label`, `cache`, `volatile`, `expires`, `definition` and `state_argument` on the
command, `label` on every argument, and `default_namespaces` plus `global_enums` at the top level
are all required. Hand-writing one means discovering that a field at a time; copying the generated
entry gets it right immediately and keeps the unchanged fields honest.

This one is verified to work — it adds a proposed second parameter to `pl/head`:

```yaml
# specs/design/my-feature/proposed_commands.yaml
commands:
- namespace: pl
  name: head
  label: Get first rows
  doc: 'Return first N rows (default: 5)'
  state_argument:
    name: state
    label: state
    default: None
    gui_info: !TextField 40
  arguments:
  - name: n
    label: n
    default: !Value 5
    argument_type: int
    gui_info: !TextField 20
  - name: offset            # newly proposed second parameter
    label: offset
    default: !Value 0
    argument_type: int
    gui_info: !TextField 20
  cache: true
  volatile: false
  expires: never
  definition: Registered
default_namespaces:
- ''
- root
global_enums: {}
```

Note `default: !Value 0`. Defaults are `CommandParameterValue`, a serde-tagged enum, so YAML wants
the tag form. The JSON-style `default: {Value: 0}` does not deserialize.

```bash
python3 .claude/skills/liquers-validate/scripts/lqv.py \
  --registry-file specs/command_registry.yaml \
  --registry-file specs/design/my-feature/proposed_commands.yaml \
  --allow-overwrite \
  -- '-R/data/sales.csv/-/ns-pl/head-10-5'
```

```
[0] OK      -R/data/sales.csv/-/ns-pl/head-10-5
    plan     GetAsset[data/sales.csv]
    plan     action pl/head(n=10, offset=5)
```

Against today's one-argument `pl/head` the same query resolves to `pl/head(n=10)` — the `5` is
dropped silently (`PLAN-EXCESS-ACTION-PARAMETERS-DROPPED`). Comparing those two lines is exactly
what the overlay is for.

A malformed overlay fails loudly, naming the missing field and the file:

```
ERROR    Could not parse 'proposed_commands.yaml' as JSON or YAML.
  as JSON: expected value at line 1 column 1
  as YAML: commands[0]: missing field `cache` at line 2 column 5
```

Three further things to note:

- Passing `--registry-file` **replaces** the automatic lookup, so the committed registry must be
  listed explicitly as the base. Files are merged in the order given.
- `--allow-overwrite` is required **here and only here**, because `pl/head` already exists.
  Adding a genuinely new command needs no such flag.
- Duplicate keys being an error by default is what makes this safe: without it, a typo'd command
  name in an overlay would silently shadow a real command instead of being reported. The error
  names both the key and the file.

This is the case that justifies `--registry-file` being repeatable — the base is committed truth,
the overlay is the proposal, and the document's queries are checked against their sum.

### Environment variable

`$LIQUERS_COMMAND_REGISTRY` supplies a registry path when no `--registry-file` is given. It sits
below explicit flags and above the committed-file lookup, and `--no-registry` overrides it — which
is what makes `--no-registry` runs reproducible regardless of the environment.
