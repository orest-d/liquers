# The validation envelope

One `ValidationReport` is serialized to **stdout** per run, JSON by default (`--format yaml` for
YAML). Human-readable diagnostics go to **stderr** and are suppressed by `--quiet`. Stdout carries
the envelope and nothing else, so it is always safe to pipe into a parser.

Defined in `liquers-core/src/validate/report.rs`.

## Contents

- [Report level](#report-level)
- [Per-result fields](#per-result-fields)
- [Status, warnings and exit codes](#status-warnings-and-exit-codes)
- [Step shapes](#step-shapes)
- [Parameter shapes](#parameter-shapes)
- [Detail levels](#detail-levels)
- [Scripting against it](#scripting-against-it)

## Report level

```json
{
  "status": "Ok",
  "level": "Plan",
  "registry": {
    "merged_files": ["/home/user/liquers/specs/command_registry.yaml"],
    "cli_commands": [{"realm": "", "namespace": "custom", "name": "transform"}],
    "command_count": 95,
    "default_namespaces": ["", "root"]
  },
  "results": [ … ],
  "counts": {"total": 1, "ok": 1, "warning": 0, "error": 0}
}
```

| Field | Meaning |
|---|---|
| `status` | The worst status among `results`. |
| `level` | `Parse` or `Plan`. Defaults to `Plan` when a registry source is present, else `Parse`. |
| `registry.merged_files` | Registry files merged, in order. Empty under `--no-registry`. |
| `registry.cli_commands` | Permissive commands from `--command`. Omitted when empty. |
| `registry.command_count` | Total commands in the assembled registry — the quickest check that the registry you think you are validating against is the one in play. |
| `registry.default_namespaces` | Namespaces searched when a query names none: `["", "root"]`. |
| `counts` | `ok + warning + error == total`. |

`CommandKey::new` normalizes the default namespace `root` to the empty string, so `--command greet`
appears in provenance as `{"realm": "", "namespace": "", "name": "greet"}` even though it lands in
`root`. Read provenance with that in mind.

Registry sources resolve in this order — explicit flags win, then the environment, then the repo:

1. `--no-registry` → empty registry, and it overrides everything below.
2. `--registry-file` (repeatable, applied in order).
3. `$LIQUERS_COMMAND_REGISTRY`.
4. `specs/command_registry.yaml`, found by walking up from the working directory.

## Per-result fields

Every optional field is omitted when empty rather than serialized as `null`, so absence is normal.

| Field | Present when | Meaning |
|---|---|---|
| `index` | always | Position in the input, 0-based. `0` for a single query. |
| `source` | always | The query text exactly as supplied. |
| `encoded` | parsed successfully | The query re-encoded from the parsed `Query`. |
| `line` | input came from `--query-file` | True 1-based source line, preserved across skipped blanks and `#` comments. |
| `title` | input was a recipe list | The recipe's title. |
| `recipe_check` | input was a recipe list | `Stored` or `AdHoc` — see `recipes-and-overlays.md`. |
| `status` | always | `Ok`, `Warning` or `Error`. |
| `query` | level `Parse` or `Plan`, `--detail full` | The parsed `Query`. |
| `plan` | level `Plan`, `--detail full` | The built `Plan`. |
| `key` | recipe resolves to a storage key | Where the result would land. Serialized as a list of `{name, position}`, not a string. |
| `error` | `status == Error` | Serialized `Error` with `error_type`, `message`, `position`, `query`, `key`. |
| `warnings` | non-empty | List of `{source, message}`; `source` is `PlanError`, `StepError` or `StepWarning`. |

### On `encoded`

`Query::encode()` normalizes, so `encoded` shows the structure the parser actually built. A
difference from `source` means normalization happened and is worth a look.

**Equality does not mean the query is right.** Both `-R/a/b/-/to_text` and `-R/a/b/to_text`
re-encode to themselves. What distinguishes them is whether a `/-/` boundary is present at all —
that is a property you read from the string, not a diff. The plan steps are the reliable answer.

## Status, warnings and exit codes

| Status | When | Exit |
|---|---|---|
| `Ok` | Parsed, and planned if at level `Plan`. | 0 |
| `Warning` | Validation succeeded but the plan carries an error or warning step. | **0** |
| `Error` | The query did not parse, or the plan could not be built. | 1 |

Report exit code is `1` if any result is `Error`, else `0`. A malformed invocation exits `2` with
empty stdout.

`Warning` exiting 0 is deliberate: a plan that carries a `Step::Error` is still a plan, and the
finding is reported rather than fatal. **If you gate CI on this tool, exit 0 does not mean
warning-free** — check `counts.warning` too.

A plan error set via `set_error` also pushes a `Step::Error` into `init_steps`; the report
de-duplicates these into a single warning.

## Step shapes

`plan.steps` (and `plan.init_steps`) hold externally tagged `Step` variants — a one-key object
whose key is the variant name. From `liquers-core/src/plan.rs`:

| Variant | Payload | Notes |
|---|---|---|
| `GetAsset`, `GetAssetBinary`, `GetAssetMetadata`, `GetAssetRecipe`, `GetAssetDirectory` | `Key` | Key is a list of `{name, position}`. |
| `GetResource`, `GetResourceMetadata`, `GetResourceDirectory` | `Key` | |
| `Action` | `{realm, ns, action_name, position, parameters}` | `ns` is the *resolved* namespace — no separate namespace reporting is needed. |
| `Filename` | `{name, position}` | Trailing filename segment. |
| `Evaluate`, `UseQueryValue` | `Query` | |
| `Info`, `Warning`, `Error` | `String` | `Error`/`Warning` steps are what produce `status: Warning`. |
| `Plan` | nested `Plan` | |
| `SetCwd`, `UseKeyValue` | `Key` | |

Other `plan` fields: `query`, `init_steps`, `is_volatile`, `payload_required`, `expires`, `error`,
`dependencies`.

## Parameter shapes

`Action.parameters` entries are externally tagged `ParameterValue` variants. The variant tells you
*where the value came from*, which is what matters when a recipe overrides an argument:

| Variant | Payload | Meaning |
|---|---|---|
| `ParameterValue` | `[name, value, position]` | Resolved from the query text. |
| `DefaultValue` | `[name, value]` | Command metadata default — the query did not supply it. |
| `OverrideValue` | `[name, value]` | Supplied by a recipe's `arguments`. |
| `ParameterLink`, `DefaultLink`, `OverrideLink`, `EnumLink` | `[name, query, …]` | Value comes from another query. |
| `MultipleParameters` | `[ParameterValue, …]` | A `multiple` argument consuming the remainder. |
| `Injected` | `name` | Supplied by the environment, not the query. |
| `Placeholder` | `name` | Expected to be overridden later. |
| `None` | — | Not set. |

**Parameters beyond the declared arity do not appear here at all** — they are dropped silently
(`specs/issues/PLAN-EXCESS-ACTION-PARAMETERS-DROPPED.md`). Confirm every parameter you wrote
is present; do not rely on `status`.

## Detail levels

`--detail full` (default) includes `query` and `plan`. `--detail summary` drops both, leaving
status, source, `encoded`, error and warnings.

Summary is the right choice for a pass/fail sweep over many queries. It is the **wrong** choice
when you are checking meaning, because the plan is what carries the answer. `lqv.py` always
requests full detail and digests it.

## Scripting against it

```bash
# pass/fail only — cheapest possible check
liquers-validate --detail summary --quiet -- "$query" > /dev/null || echo "invalid: $query"

# extract the resolved action names
liquers-validate --quiet -- "$query" \
  | python3 -c 'import json,sys; print([s["Action"]["action_name"] for s in json.load(sys.stdin)["results"][0]["plan"]["steps"] if "Action" in s])'
```

The library API behind the CLI is `liquers_core::validate` — `validate_query`, `validate_recipes`,
`ValidationRegistryBuilder`, `build_report`. Use it directly from Rust tests instead of shelling
out; `validate_query` never fails, returning a `ValidationResult` with `status == Error` instead.
