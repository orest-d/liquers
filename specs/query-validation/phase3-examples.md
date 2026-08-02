# Phase 3: Examples & Use-cases - Query Validation Utility

## Example Type

**Conceptual code, with every query verified against the real parser and a real registry.**

Examples are illustrative rather than compiling — the code lands in Phase 4 — but no query,
namespace or command name in this document is invented. Each was run through `parse_query` and
`PlanBuilder::build` against three registries (empty, a liquers-lib set of **81 commands**, and a
permissive CLI-style one) using a throwaway harness, since a design document full of queries
that do not parse is worse than none. Every "expected output" below is what the code actually
produced. The harness was deleted; its findings are recorded here.

**All JSON in this document is abridged.** `position` objects (present on every query element,
step and error) and unchanged nested detail are elided for readability. Field *names* and nesting
are exact; a field's absence here does not mean it is optional in the real envelope — consult
Phase 2 for the authoritative shape.

### Facts established by that verification

1. **`-R/` consumes the entire remainder as a key.** `-R/data/report.txt/to_text` is a *three-element
   key*, not a resource plus an action. Actions require the `/-/` segment separator:
   `-R/data/report.txt/-/to_text`. This drives Example 3, and it invalidated four of the seven
   example queries in my first draft.
2. **Argument type errors surface at plan build**, not parse: `ns-pl/head-notanumber` fails with
   `Can't convert 'notanumber' to integer`.
3. **`register_all_commands_fn` does not register polars** — polars needs
   `env.register_polars_commands()`. Confirms Phase 2's "the exporter must invoke each macro under
   its own `cfg`" note from a second direction.
4. **`DefaultEnvironment::new()` requires a tokio runtime** — it constructs `DefaultAssetManager`,
   which calls `tokio::spawn`. The exporter binary therefore needs `#[tokio::main]` despite doing
   no async work. This would have been a compile-and-panic surprise in Phase 4.
5. **The `recipes.yaml` in Example 5 deserializes into `RecipeList`** and behaves as claimed: all
   three recipes give `store_to_key() == None` (hence `AdHoc`), while the filename-terminated
   variant under `set_cwd("reports")` gives `Some(reports/preview.csv)` (hence `Stored`).

An independent reviewer re-derived every claim above with its own harness after mine was deleted,
and confirmed all of them — including the 81-command total, the 4-step polars plan, and both sides
of the `-R/` key-swallowing comparison.

**81 is the harness total, not the exporter's.** The harness used `register_all_commands_fn` plus
`register_polars_commands`, which is core 5 + egui 3 + image 47 + polars 26 = 81 and **excludes
`lui`**. The exporter always includes `lui` (Phase 2: `core` and `lui` are never feature-gated),
so its default export is **95**. Plan-building results are unaffected — none of these examples
uses a `lui` command — but the `command_count` an agent sees from an export is 95.

## Overview Table

| # | Type | Name | What it demonstrates / checks |
|---|---|---|---|
| **Examples** |
| 1 | Example | Level 1 — parse only | The zero-setup path: one query, no registry, `Query` as JSON. What an agent runs first |
| 2 | Example | Level 2 — plan against exported registry | The full loop: export once, validate many. Catches unknown commands and bad argument types |
| 3 | Example | The `-R/` swallowing trap | Two queries that both validate `Ok`; only the serialized `Plan` reveals one is wrong. Justifies emitting the whole plan |
| 4 | Example | Permissive CLI commands | Validating a query whose commands do not exist yet — the design-phase use case |
| 5 | Example | Recipe list batch validation | `recipes.yaml` in, per-recipe results out, including the stored/ad-hoc payload distinction |
| **Corner cases** |
| C1 | Corner | Empty registry + action | Every action errors; documents why `--level plan` needs a registry |
| C2 | Corner | Plan carries an error | Status `Warning`, exit 0 — the decision from Phase 1 |
| C3 | Corner | Duplicate `CommandKey` on merge | Error by default, `--allow-overwrite` permits |
| C4 | Corner | `Plan::error`/`init_steps` double-report | De-duplication actually works |
| C5 | Corner | Ad-hoc vs stored recipe | `recipe_check` distinguishes; payload boundary only applies to stored |
| C6 | Corner | JSON/YAML input ambiguity | Both parse; both diagnostics reported when both fail |
| C7 | Corner | Blank/comment lines in a query file | Skipped, `line` still points at the true source line |
| C8 | Corner | Empty input | Empty query string, empty recipe list, empty stdin |
| **Unit tests** |
| U1–U6 | Unit | `validate_query` | Happy path both levels, parse error, unknown command, position fidelity, index |
| U7–U10 | Unit | `validate_recipe(s)` | Ad-hoc, stored, bad override, ordering |
| U11–U15 | Unit | `ValidationRegistryBuilder` | Merge, duplicate, overwrite, permissive specs, provenance |
| U16–U19 | Unit | Report assembly | Status aggregation, counts, exit code, warning collection |
| U20–U22 | Unit | Serialization | JSON/YAML round-trip, optional-field omission, `from_json_or_yaml` |
| **Integration tests** |
| I1–I3 | Integration | End-to-end via library API | Full assemble→validate→serialize with a real liquers-lib registry |
| I4–I5 | Integration | Exporter | Round-trips through the validator; group/namespace filtering |
| I6 | Integration | Feature matrix | `cli` on/off, `--no-default-features` |

---

## Example 1: Level 1 — parse only

**Scenario:** an agent is about to put a query in a doc example and wants to know it parses. No
registry, no setup.

```bash
liquers-validate -- '-R/data/report.txt/-/to_text'
```

The `--` is not decoration. Liquers resource queries begin with `-`, so a bare leading-`-`
positional is exactly the shape of a flag; `--` ends option parsing and makes it a value. Phase 2
removes every short flag for the same reason.

**If `LIQUERS_COMMAND_REGISTRY` is set** (Example 2 exports it), this invocation is no longer the
zero-setup path: a registry source is present, so `--level` defaults to `plan` and
`command_count` reflects the exported registry rather than 0. Pass `--no-registry` to get the
output shown here regardless of environment.

Verified: parses. Output (abridged — `position` objects elided for readability; the real envelope
carries them on every element):

```json
{
  "status": "Ok",
  "level": "Parse",
  "registry": { "command_count": 0, "default_namespaces": ["", "root"] },
  "results": [
    {
      "index": 0,
      "source": "-R/data/report.txt/-/to_text",
      "encoded": "-R/data/report.txt/-/to_text",
      "status": "Ok",
      "query": {
        "segments": [
          { "Resource": { "header": { "resource": true, "level": 0 },
                          "key": [ { "name": "data" }, { "name": "report.txt" } ] } },
          { "Transform": { "query": [ { "name": "to_text", "parameters": [] } ],
                           "filename": null } }
        ],
        "absolute": false,
        "source": "Unspecified"
      }
    }
  ],
  "counts": { "total": 1, "ok": 1, "warning": 0, "error": 0 }
}
```

Exit code 0. A malformed query instead yields `status: "Error"` with the serialized `Error`,
including `position` — verified: `bad query with spaces` gives
`Can't parse query 'bad query with spaces': Can't parse query completely`.

**Why `Query` and not just a boolean:** the segment structure is the answer to "did it parse the
way I meant?", which is a different question from "did it parse". Example 3 is that difference.

---

## Example 2: Level 2 — plan against the exported registry

**Scenario:** the agent wants to know the commands in a query actually exist and take the
arguments given.

```bash
# once per session
cargo run -p liquers-lib --features cli --bin export-command-registry -- \
  -o /tmp/liquers-commands.json
export LIQUERS_COMMAND_REGISTRY=/tmp/liquers-commands.json

# then, repeatedly
liquers-validate '-R/data/sales.csv/-/ns-pl/select_columns-name-price/head-10/preview.csv'
```

Verified against the real 81-command registry: **plan built, 4 steps.** `--level` defaults to
`plan` because a registry source is present.

```json
{
  "status": "Ok",
  "level": "Plan",
  "registry": {
    "merged_files": ["/tmp/liquers-commands.json"],
    "command_count": 95,
    "default_namespaces": ["", "root"]
  },
  "results": [ { "index": 0, "status": "Ok",
                 "plan": { "steps": [ {"GetAsset": [...]},
                                      {"Action": {"realm": "", "ns": "pl",
                                                  "action_name": "select_columns", ...}},
                                      {"Action": {"realm": "", "ns": "pl",
                                                  "action_name": "head", ...}},
                                      {"Filename": {"name": "preview.csv"}} ],
                           "is_volatile": false, "payload_required": "None",
                           "expires": "never", "error": null, "dependencies": [] } } ],
  "counts": { "total": 1, "ok": 1, "warning": 0, "error": 0 }
}
```

Note `Step::Action` carries the resolved `ns: "pl"` — the reason Phase 2 needs no separate
namespace reporting.

**Two real failures this catches, both verified:**

| Query | Result |
|---|---|
| `-R/data/sales.csv/-/head-5` | `Error`: `Action 'head' not registered in namespaces '', 'root'` — the author forgot `ns-pl` |
| `-R/data/sales.csv/-/ns-pl/head-notanumber` | `Error`: `Can't convert 'notanumber' to integer` — argument *type*, invisible to parsing |

The second is the strongest argument for level 2 existing at all: it is a real bug in a plausible
doc example, and no amount of parse checking finds it.

---

## Example 3: The `-R/` swallowing trap

**Scenario:** this is the failure mode that motivates emitting the whole `Plan` rather than a
pass/fail. Both of these parse; both report `status: "Ok"`; they mean different things.

```bash
liquers-validate --command to_text -- '-R/data/report.txt/-/to_text'   # intended
liquers-validate --command to_text -- '-R/data/report.txt/to_text'     # typo: missing /-/
```

Two details in that invocation matter, and both were wrong in an earlier draft:

- **`--command to_text` is required.** Without a registry, `--level` defaults to `Parse` and no
  plan is produced at all; and against an *empty* registry the first query fails outright with
  `Action 'to_text' not registered` (corner case C1). Only the second query survives an empty
  registry, because its `to_text` is part of the key, not an action. A one-word `--command`
  supplies the registry with no export step.
- **`--` before the query.** Liquers resource queries begin with `-`, so `--` is the habit that
  keeps a leading-`-` query a value rather than a flag.

With `to_text` registered, the plans are:

| Query | `steps` |
|---|---|
| `-R/data/report.txt/-/to_text` | `GetAsset[data, report.txt]`, `Action{to_text, ns:""}` |
| `-R/data/report.txt/to_text` | `GetAsset[data, report.txt, to_text]` |

In the second, `to_text` became the **third element of the key**. The query is valid, plans
cleanly, and means something entirely different — it fetches a file called `to_text` from
`data/report.txt/`. No validator can call this an error, because it is not one.

**What the design does about it:** nothing, deliberately — and that is the point. The tool's
contract is "here is exactly what your query means", not "your query is good". Phase 1 decision 5
(complete diagnostics) and Phase 2's choice to serialize the full `Plan` exist for this case. An
agent comparing `steps` against intent catches it; an agent trusting a green checkmark does not.

This is worth stating in the tool's own `--help`.

**The cheap version of the same check.** Comparing plans needs a registry and level 2. The
`encoded` field catches the same class of mistake at **level 1, with no registry at all**:

```bash
liquers-validate -- '-R/data/report.txt/to_text'
#  "source":  "-R/data/report.txt/to_text"
#  "encoded": "-R/data/report.txt/to_text"    <- one key segment, no /-/ appears
```

`Query::encode()` normalizes, so `encoded` shows the structure the parser actually built. An agent
that diffs `source` against `encoded`, and reads whether a `/-/` boundary survived, gets most of
this example's value for one string per result.

---

## Example 4: Permissive commands for not-yet-written code

**Scenario:** an agent is designing a feature and writing example queries for commands that do not
exist yet. Level 2 would reject every one.

```bash
liquers-validate --command greet --command 'custom/transform' 'ns-custom/greet-world'
```

Verified: with a permissive `greet` registered, `greet-world` and `greet-a-b-c-d-e` both build a
plan; `ns-custom/greet-world` also builds, because `default_namespaces` (`["", "root"]`) is
searched after the requested `custom`.

```json
{
  "status": "Ok",
  "level": "Plan",
  "registry": {
    "cli_commands": [ { "realm": "", "namespace": "", "name": "greet" },
                      { "realm": "", "namespace": "custom", "name": "transform" } ],
    "command_count": 2,
    "default_namespaces": ["", "root"]
  },
  "results": [ { "index": 0, "source": "ns-custom/greet-world",
                 "status": "Ok", "plan": { "steps": [...] } } ],
  "counts": { "total": 1, "ok": 1, "warning": 0, "error": 0 }
}
```

The permissive command accepts **any number of arguments of any type** — verified with zero
(`greet`), one (`greet-world`) and five (`greet-a-b-c-d-e`) — because it carries one
`ArgumentInfo::any_argument("arguments").set_multiple()`, and `multiple` consumes all remaining
parameters.

Note `"namespace": ""` for `greet`: `CommandKey::new` normalizes the default namespace `root` to
the empty string (`command_metadata.rs:567-575`), so provenance reports it that way even though
`--command greet` places it in `root`. Worth knowing, since an agent reads provenance literally.

Mixing is the real workflow: `-R $LIQUERS_COMMAND_REGISTRY --command my_new_command` validates a
query that combines existing commands with one being designed.

---

## Example 5: Recipe list batch validation

**Scenario:** validating a whole `recipes.yaml` before committing it.

```yaml
# recipes.yaml
recipes:
  - query: -R/data/sales.csv/-/ns-pl/head-10
    title: Sales preview
  - query: -R/data/sales.csv/-/ns-pl/nrows
    title: Row count
  - query: -R/data/sales.csv/-/ns-pl/head-notanumber
    title: Broken
```

```bash
liquers-validate --recipes recipes.yaml --cwd reports
```

```json
{
  "status": "Error",
  "level": "Plan",
  "registry": { "merged_files": ["/tmp/liquers-commands.json"], "command_count": 95,
                "default_namespaces": ["", "root"] },
  "results": [
    { "index": 0, "source": "-R/data/sales.csv/-/ns-pl/head-10",
      "title": "Sales preview", "status": "Ok",
      "recipe_check": "AdHoc", "plan": { "steps": [...] } },
    { "index": 1, "source": "-R/data/sales.csv/-/ns-pl/nrows",
      "title": "Row count", "status": "Ok", "recipe_check": "AdHoc" },
    { "index": 2, "source": "-R/data/sales.csv/-/ns-pl/head-notanumber",
      "title": "Broken", "status": "Error",
      "error": { "message": "Can't convert 'notanumber' to integer" } }
  ],
  "counts": { "total": 3, "ok": 2, "warning": 0, "error": 1 }
}
```

Exit 1. Note `recipe_check: "AdHoc"` — these recipes have no `filename` in their query, so
`store_to_key()` is `None` and the payload boundary does not apply. A recipe ending in a filename
(`…/head-10/preview.csv`) with `--cwd reports` yields `key: "reports/preview.csv"` and
`recipe_check: "Stored"`, and *then* `to_plan_for_key` enforces the payload boundary.

**`--cwd` does not change the plan.** Running the same recipe list with `--cwd reports` and with
`--cwd archive` produces byte-identical `plan.steps`; only `key` differs
(`reports/preview.csv` vs `archive/preview.csv`), because `Recipe::to_plan` never consults `cwd`
(Phase 2 constraint 3). An agent expecting `--cwd` to resolve relative keys *inside* the plan will
be disappointed, so it is worth stating plainly rather than leaving to be discovered.

Batch also works without recipes, one query per line, which is lighter to generate:

```bash
printf '%s\n' '-R/data/sales.csv/-/ns-pl/nrows' '# a comment' '' 'to_text/-/to_text' \
  | liquers-validate --query-file -
```

Two results, indices 0 and 1, with `line` 1 and 4.

---

## Corner Cases

| # | Case | Expected behaviour | Basis |
|---|---|---|---|
| C1 | `--level plan`, empty registry, query has an action | Every action → `Error: Action '…' not registered in namespaces '', 'root'` | Verified. Level 2 is only meaningful for pure key/resource queries without a registry |
| C2 | Plan builds but carries `Plan::error` | `status: "Warning"`, `warnings[]` populated, **exit 0**, `WARNING  Plan contains error: …` on stderr | Phase 1 decision 6 |
| C3 | Two registry files define the same `CommandKey` | `Err` naming key and file; `--allow-overwrite` → last wins | `add_command` overwrites silently, so the check must precede it |
| C4 | `Plan::error` set via `set_error` | Reported **once**, not twice | `set_error` also pushes `Step::Error` into `init_steps` |
| C5 | Recipe without filename vs with filename + `--cwd` | `AdHoc` → `to_plan`; `Stored` → `to_plan_for_key`, payload boundary enforced | Ad-hoc recipes are first-class |
| C6 | Registry file that is neither JSON nor YAML | `Err` quoting **both** parser messages | Neither parser's failure alone identifies the problem |
| C7 | Query file with blank lines and `#` comments | Skipped; `line` reports the true 1-based file line | `#` and newline are not valid query characters |
| C8 | Empty query string / empty recipe list / empty stdin | Empty query is a parse result, not a crash; empty list → `total: 0`, `status: "Ok"`, exit 0 | Zero results must not aggregate to `Error` |
| C9 | `--cwd` given, recipe already has its own `cwd` | `Err(not_supported)` from `RecipeList::set_cwd` — a genuine finding, since production fails the same way | Matches `DefaultRecipeProvider` |
| C10 | Very long query / deeply nested plan | No recursion limit imposed by us; whatever the parser accepts, we serialize | Do not add a limit the rest of the system lacks |
| C11 | `--format yaml` with a plan containing an `Error` | `Error` is `Serialize`; YAML round-trips | All embedded types derive both |
| C12 | Exporter run with `--groups polars` in a build without the `polars` feature | Clear error listing available groups, not a silent empty registry | `--list-groups` is the honest answer |

---

## Test Plan

Conventions per `liquers-unittest`: unit tests inline in `#[cfg(test)] mod tests`, integration in
`tests/`, `-> Result<(), Box<dyn std::error::Error>>` where `?` is used, no `unwrap()` outside
tests, explicit match arms.

**Note on assertions:** `ValidationResult`/`ValidationReport` are not `PartialEq` (they embed
`Plan`, which derives only `Serialize, Deserialize, Debug, Clone`), so tests compare **fields**,
not whole values. `ValidationWarning`, `RegistryProvenance` and `ValidationCounts` *are*
`PartialEq` and can be compared directly.

### Unit tests — `liquers-core/src/validate/mod.rs`

| # | Test | Asserts |
|---|---|---|
| U1 | `parse_level_ok` | `-R/data/report.txt/-/to_text` at `Parse` → `Ok`, `query.is_some()`, `plan.is_none()` |
| U2 | `parse_level_rejects_spaces` | `bad query with spaces` → `Error`, `error.error_type == ParseError` |
| U3 | `parse_error_carries_position` | `error.position.line == 1`, `column > 0` — the diagnostic value depends on it |
| U4 | `plan_level_ok_with_registry` | Registry with `to_text` → `Ok`, `plan.steps.len() == 2` |
| U5 | `plan_level_unknown_command` | Empty registry + `to_text` → `Error`, type `ActionNotRegistered` |
| U6 | `index_is_preserved` | `validate_query(q, 7, …).index == 7` |
| U7 | `recipe_adhoc_uses_to_plan` | No filename → `recipe_check == AdHoc`, `key.is_none()` |
| U8 | `recipe_stored_uses_to_plan_for_key` | Filename + cwd → `recipe_check == Stored`, `key == Some("reports/preview.csv")` |
| U9 | `recipe_bad_argument_override` | `arguments` naming a non-existent parameter → `Error` |
| U10 | `recipes_preserve_order_and_index` | Three recipes → indices 0,1,2 in list order |
| U11 | `merge_adds_commands` | Merged registry's `command_count` matches |
| U12 | `merge_duplicate_key_errors` | Duplicate → `Err`, message names the key |
| U13 | `merge_duplicate_allowed_with_overwrite` | `with_overwrite_allowed(true)` → `Ok`, last wins |
| U14 | `permissive_spec_forms` | `name`, `ns/name`, `realm/ns/name` → expected `CommandKey`; malformed → `Err` |
| U15 | `permissive_accepts_any_arguments` | `greet-a-b-c-d-e` plans against a permissive `greet` |
| U16 | `report_status_is_worst_result` | Ok+Warning → `Warning`; Ok+Error → `Error` |
| U17 | `report_counts_add_up` | `ok + warning + error == total` |
| U18 | `exit_code_zero_for_warning` | Warning → 0; Error → 1 |
| U19 | `warnings_deduplicate_plan_error` | `set_error` plan → exactly one `PlanError` warning (C4) |
| U20 | `report_json_yaml_roundtrip` | Serialize→deserialize→field-compare, both formats |
| U21 | `optional_fields_omitted` | `Ok` result's JSON has no `error`/`warnings`/`key` keys |
| U22 | `from_json_or_yaml_both_and_neither` | JSON ok, YAML ok, garbage → `Err` mentioning **both** parsers |
| U23 | `empty_recipe_list_is_ok` | `total: 0`, `status: Ok`, exit 0 (C8) |
| U24 | `query_file_skips_blanks_and_comments` | 4 lines → 2 results with `line` 1 and 4 (C7) |
| U25 | `diagnostic_lines_format` | `Warning` → a line starting `WARNING`, `Error` → one starting `ERROR`, each carrying the underlying message and the result index |
| U26 | `cwd_changes_key_not_plan` | Same recipe under two `--cwd` values → identical `plan.steps`, differing `key` (Phase 2 constraint 3) |

### Integration tests

`liquers-core/tests/validate_integration.rs`:

| # | Test | Covers |
|---|---|---|
| I1 | `end_to_end_parse_only` | Builder → validate → JSON, no registry |
| I2 | `end_to_end_plan_with_merged_registry` | Serialize a registry, merge from string, validate, check steps |
| I3 | `swallowing_trap_plans_differ` | Example 3 as an executable assertion: both `Ok`, `steps.len()` 2 vs 1, and the 1-step plan's key has three elements |

`liquers-lib/tests/export_registry_integration.rs`:

| # | Test | Covers |
|---|---|---|
| I4 | `exported_registry_roundtrips_into_validator` | Export → `merge_str` → validate a real query (`ns-pl/head-10` under `-R/…/-/`) → `Ok`. The contract between the two binaries |
| I5 | `group_and_namespace_filters` | `--groups core` excludes `pl`; `--namespaces pl` keeps only polars |
| I6 | `exporter_registry_is_nonempty` | Guards against a silently empty export; asserts `commands.len()` ≥ the core set |

These need `#[tokio::test]` — `DefaultEnvironment::new()` requires a runtime (finding 4).

### Manual / matrix checks (Phase 4 gate)

```bash
cargo test -p liquers-core --lib --tests
cargo test -p liquers-lib --lib --tests
cargo check -p liquers-core --no-default-features          # cli off, no clap
cargo check -p liquers-core --features cli                 # binary builds
cargo check -p liquers-lib --no-default-features --features cli
cargo run -p liquers-lib --features cli --bin export-command-registry -- --list-groups
```

### Coverage gaps deliberately not tested

- **Store interaction** — there is none; `-R/` keys are never resolved. Asserting a key does not
  exist would test the absence of a feature.
- **Command execution** — the validator never runs a command. `Step::Action` presence is the
  assertion; behaviour is the command library's own test surface.
- **clap's own parsing** — mutual exclusion and `requires` are declarative; testing them tests
  clap. One smoke test that `--cwd` without `--recipes` exits 2 is enough.
