# liquers-py: implementation plan for conformance with the Language Integration Guide — 2026-10-17

Snapshot of the plan the owner approved on 2026-10-17. It is carried forward by
[`design/liquers-py/`](../design/liquers-py/) and is not updated.

Date: 2026-10-17 · Repository: `orest-d/liquers` at `b7070f8` · Guide: `specs/guides/LANGUAGE-INTEGRATION_GUIDE.md` (reviewed 2026-10-16)

This is a plan, not a design. Its next step (§10) is to turn it into a `specs/design/liquers-py/`
folder through the `liquers-project` workflow, which is where the guide's §7 checklist content has
to live. Everything here that is a fact about the code was checked at the commit above; everything
that is a recommendation is marked as one.

---

## 0. Summary

1. **Modernize `liquers-py` in place; do not start a new crate.** This was already decided
   (`specs/design/python-wrapper/phase1-high-level-design.md`, TODO "Open Design Decisions" 1).
2. **Replace liquers-py's own `Value` enum with `liquers_lib::value::Value`**, carrying Python
   objects in the shared `ExtValue::Foreign` variant through a `PyObjectValue: ForeignValue`. This
   is the guide's prescribed mechanism (§5 VALUE, "Retaining an opaque language value"), it gives
   Python every `liquers-lib` command (polars, images, records, templates) for free, and it
   supersedes the old designs' `PythonValueExtension<E>`.
3. **Build the environment with `liquers_lib::environment::default_environment_builder()`**
   instead of the hand-written `Environment` stub whose asset manager and recipe provider are
   `todo!()`.
4. **Register Python commands as closures, not as aliases of `pycall`.** `CommandRegistry` accepts
   a `Send + Sync + 'static` closure natively and `Py<PyAny>` satisfies that, so each Python
   command is an ordinary registered command owning its callable — exactly what `liquers-web`
   does. Metadata comes from the shared `CommandDeclaration` (`liquers-core/src/command_declaration.rs`),
   fed by `inspect.signature` introspection done in Python.
5. **Ship as a mixed maturin project**: a pure-Python package `liquers` (decorators, typing, the
   convenience layer) over a native extension. The guide's Appendix A tests are already written as
   Python against `import liquers as lq`, so the suite can follow them almost verbatim.
6. **Milestones M0–M6** (§5): foundations (PyO3 upgrade, harness) → thin end-to-end baseline →
   complete baseline → async → stores and recipes → records, stubs and packaging → server, modules
   and UI. The thin end-to-end path for every selected feature is in M1, per guide §7 item 11.

---

## 1. Where liquers-py stands today (verified)

| Area | State at `b7070f8` | Evidence |
|---|---|---|
| Build | `cargo check -p liquers-py --lib` **fails** on the cloud's Python 3.13 (PyO3 0.21 max is 3.12); with `PYO3_USE_ABI3_FORWARD_COMPATIBILITY=1` it passes with 13 warnings | run in this session |
| Exposed to Python | Query/Key family, `CommandMetadata` family, `Metadata`/`MetadataRecord`/`AssetInfo`/`Status`/`LogEntry`, `ErrorType`/`Error`, `Expires`, dependencies, `Plan`/`Step`, `Recipe`/`RecipeList`, `parse`, `parse_key`, `build_plan` | `liquers-py/src/lib.rs` |
| Compiled but not exposed | `value.rs` (own `Value` enum incl. `Py { Py<PyAny> }`, `py.Object` identifier, type descriptions), `context.rs` (`Environment` stub) | `lib.rs` declares them, `#[pymodule]` adds no class from them |
| Not compiled at all | `commands.rs` (`pycall`, `hello`, `greet`, `pyprint` with `println!`), `interpreter.rs` (all commented out), `state.rs` | `PY-MODULES-NOT-DECLARED-IN-LIB`; `commands.rs` reads a `pub(crate)` field and uses a dead macro syntax |
| Environment | `get_asset_manager`, `get_recipe_provider`, `apply_recipe` are `todo!()`; store is `NoAsyncStore`; `init_with_envref` returns an error | `liquers-py/src/context.rs` |
| Evaluation from Python | **impossible** — no entry point exists | — |
| Serializer | writes `txt`/`html` for scalars only, reads nothing | `PY-VALUE-SERIALIZER-IS-A-STUB` |
| Tests | `tests/test_basic_objects.py` (71 lines), `tests/test_parser.py` (287 lines): OBJECT-level only, no feature IDs, no documented runner | `liquers-py/tests/` |
| Packaging | `pyproject.toml` (maturin, `requires-python >=3.8,<3.13`), module name `liquers_py`, no `.pyi`, no `py.typed` | `liquers-py/pyproject.toml` |

Conformance status against the guide today: `OBJECT` PARTIAL (wrappers exist, no conformance
tests); every other feature NS. The usable part is the OBJECT wrapper layer — roughly 2,700 of the
4,400 lines.

---

## 2. Review of the existing Python designs

`specs/design/python-wrapper/` (phase 1, 2026-02-23; phase 2, 2026-02-26) predates the guide's
current form, `ForeignValue`, `CommandDeclaration`, `EnvironmentBuilder`, store factories in core
and the removal of the sync store. Its `DESIGN.md` says `status: complete`, which is wrong — no
phase 3/4 exists and nothing it describes was implemented. The phase 1 TODO section carries **the
owner's own decisions**, which this plan keeps as binding.

### Salvage (keep, carried into this plan)

| From | What | Where it lands |
|---|---|---|
| phase 1 TODO, decision 1 | Modernize `liquers-py`, not a new crate | §0 item 1 |
| phase 1 TODO, decision 2 | **Permissive** conversion by default, opaque `py.Object` fallback | VALUE, M2 |
| phase 1 TODO, decision 3 | Pillow, polars, numpy supported in the baseline | VALUE, M2 (as Python extras, see PACKAGE) |
| phase 1 TODO, decision 4 | Async commands designed now, delivered in a later milestone; M1 proves register-and-execute | M3 |
| phase 1 TODO, decision 5 | Pickle codec allowed in the baseline | VALUE06, M2 — **opt-in per environment**, never implicit (guide §6 Python) |
| phase 1 TODO | One shared environment by default; configurable store, default recipe provider, default asset manager, registry override from YAML, all `liquers-lib` commands registered | ENVIRON, M1–M2 |
| phase 1 TODO | Three usage modes: library, server, GUI | M1 / M6 WEBSERV / M6 UIUSE |
| phase 1 TODO | `command` / `first_command` decorators deriving metadata from the signature and docstring, after the old `liquer` package | COMMAND, M1–M2 |
| phase 1 TODO | Blocking and async variants of service wrappers (environment, store, asset manager, recipe provider, context) | EVAL/ASYNCQ, M2–M3 |
| phase 2 §3 | PyO3 async and deadlock rules: no GIL across `await`, owned `Py<T>` across boundaries, release the GIL while blocking, no Rust guard held while entering Python | RUNTIME, all milestones |
| phase 2 §3.3 | The queue-inversion deadlock (a nested evaluation waiting on a saturated job queue) | RUNTIME04, M2 — verify against today's `DefaultAssetManager` before adding any policy type |
| phase 2 §8 | `ConversionMode` Strict / Permissive (default) / AlwaysWrapped | VALUE05, M2 |
| phase 2 §9 | `PyLiquersError` local wrapper to get around the orphan rule for `From<Error> for PyErr` | ERROR, M1 |
| phase 2 §11.1 | `CommandMetadata` field mapping table | COMMAND — re-based on `COMMAND_DECLARATION.md`, see §3.4 |
| phase 2 §12 | Global environment as a Rust `OnceCell` + lock, with `reset_for_tests` | ENVIRON — plus explicit instances, which ENVIRON05 requires |
| phase 2 §5 | Python submodules (`env`, `commands`, `value`, `errors`, `server`, `gui`) | §3.6 |

### Drop (superseded)

| Old proposal | Why it goes | Replacement |
|---|---|---|
| `PythonValueExtension<E>` over `CombinedValue` | The shared, ungated `ExtValue::Foreign { Arc<dyn ForeignValue> }` exists for exactly this; one variant for every language | `PyObjectValue: ForeignValue` |
| Dispatch through `CommandDefinition::Alias` → `pycall` / `pycall_async` with module/function head parameters | Closures are registrable natively; aliases now have a strict head-parameter contract (`COMMAND_ALIASES.md`); a module-path lookup at call time breaks callable identity (`COMMAND11`) and resolves the protocol on every call | One closure per command owning `Py<PyAny>`; aliases stay available for user-declared aliases |
| Python-specific `CommandMetadataExtractor` trait in Rust | `CommandDeclaration` is the shared format; the guide forbids a per-language vocabulary | Introspection in Python → `from_introspection`, decorator kwargs → `enhance` |
| `liquers-store::StoreRouterBuilder` for configuration | Configuration and `StoreRouterBuilder` moved to `liquers-core` (`store-factories-in-core`) | `liquers-core` builder + `liquers-store` factory for OpenDAL backends |
| Sync `Store` wrappers, `cache.rs` | Sync store and cache module removed (`SYNC-STORE-REMOVAL`, `REPO-DEAD-CODE-HYGIENE`); guide §4 puts cache out of scope | async store only |
| Requirement to make all `liquers-lib` commands generic over a Python value type (phase 2 §7.4) | Unneeded once liquers-py uses `liquers_lib::value::Value` | — |
| `NestedEvaluationMode` / `EvaluationSchedulingPolicy` (phase 2 §3.3.3) | Speculative; add only if RUNTIME04 shows the deadlock on the current manager | measured in M2 |
| PyO3 "0.28.x" as a fixed target | Version names drift; pick the newest stable line when M0 starts | M0 |

The phase 1 TODO also asks for a review of the old `orest-d/liquer` Python package
(`liquer/commands.py`, `command_metadata_from_callable`). That was never done. It belongs in the
COMMAND part of the new phase 1, as a compatibility delta against the decorator surface (§3.4).

---

## 3. Architecture decisions

Each is a recommendation unless marked as the owner's earlier decision. The ones that change the
public surface are flagged **(confirm)**.

### 3.1 Value type **(confirm)**

`type PyValue = liquers_lib::value::Value` (that is, `CombinedValue<SimpleValue, ExtValue>`).
Python objects that are not converted are `ExtValue::Foreign { value: Arc<PyObjectValue> }` with
`PyObjectValue(Py<PyAny>)` implementing `ForeignValue`: `origin() = "python"`,
`identifier() = "py.Object"` (the constant already in `value.rs:244`), `type_name()` = the
object's `type(obj).__qualname__`.

- `py.Object`'s `TypeInfo` is registered on the builder with `with_type_registry`, because the
  identifier lives in liquers-py, not in liquers-lib's static list (guide §5 VALUE "Registering
  it"; the "Python-style integration needs none of this" paragraph described the old own-enum
  design and stops applying). Its data formats are declared only when the pickle codec is enabled
  (rule 3 of "Four things worth getting right").
- Cost: liquers-py now builds liquers-lib, so the wheel carries polars, image and records code and
  the build is much larger. That is what the owner's decision 3 asks for, and the Cargo features are
  chosen in PACKAGE. The cloud build budget (30 GB) needs `cargo clean` discipline; see the M0
  harness.
- Deletes: liquers-py's own `Value` enum and its serializer, which closes
  `PY-VALUE-SERIALIZER-IS-A-STUB` and `PY-VALUE-ENUM-OVERSIZED` by removal (liquers-lib's
  serializer is the real one).

### 3.2 Conversion table (VALUE, design question 2)

| Python → Liquers | Liquers → Python |
|---|---|
| `None` → `None` | `None` |
| `bool` → `Bool` (checked before `int`) | `bool` |
| `int` in i64 range → `I64` (`I32` never produced inbound) | `int` from `I32`/`I64` |
| `int` outside i64 → permissive: `py.Object` (lossless); strict: `ConversionError` | — |
| `float` → `F64` (NaN/inf kept; JSON follows `json_f64`) | `float` |
| `str` → `Text` | `str` |
| `bytes`, `bytearray`, `memoryview` → `Bytes` | `bytes` (never `str`; VALUE04) |
| `list`, `tuple` → `Array` (tuple identity is not kept; documented) | `list` |
| `dict` with all-`str` keys → `Object` | `dict` |
| `dict` with other keys, `set`, other objects → permissive: `py.Object`; strict: error | the original object |
| `liquers.Query` / `liquers.Key` wrappers → `Query` / `Key` | the wrappers |
| `Metadata`, `Recipe`, `CommandMetadata`, `AssetInfo` wrappers → their variants | the wrappers |
| `polars.DataFrame` → `PolarsDataFrame` (via Arrow C stream) | `polars.DataFrame` if `polars` importable, else error naming the extra |
| `PIL.Image.Image` → `Image` | `PIL.Image.Image` if Pillow importable, else `bytes` (PNG) with a warning |
| `pyarrow.Table` / `RecordBatch` → `RecordView` | `liquers.RecordView` wrapper (with `.to_arrow()`, `.to_pandas()`), single cell → scalar (RECORDS10) |
| `numpy.ndarray` → `py.Object` unless a command declares an image/array type (no guessing; guide "do not build a private coercion layer") | — |
| callables → refused as values unless wrapped explicitly (VALUE11); commands go through COMMAND | — |
| `UiCommand`, `Widget`, `UIElement` | opaque `liquers.Value` wrapper (owner TODO) |

Cycles in containers: detected and refused in strict mode, retained as `py.Object` in permissive
mode (VALUE07). Conversion is eager at every size in M2; a size threshold is added only if a
benchmark shows it matters, and the number goes into this table.

Mutability: a retained `py.Object` is shared, so mutating it after it is cached changes the cached
asset. Accept the same "immutable by discipline" posture `liquers-web` decided (its phase 1,
decision 2), documented.

### 3.3 Environment and lifecycle

- `liquers.Environment(config=None)` wraps an `EnvironmentBuilder` until first use, then the
  published `EnvRef`. A module-level default environment (`liquers.default_env()`, used by
  module-level `liquers.evaluate` and the bare decorators) is created lazily — the owner's "one
  shared environment"; explicit instances exist for embedders and ENVIRON05.
- The builder starts from `default_environment_builder()` and registers liquers-lib's command set
  (`register_all_commands_fn`), so ENVIRON01 evaluates a real built-in.
- **Registration after publication** follows guide §5 ENVIRON: Python declarations are retained
  and replayed into a fresh environment, then the handle is swapped. The empty asset cache and
  in-flight evaluations keeping the old `EnvRef` are documented. Decorators at import time normally
  run before first evaluation, so the replay is the rare path.
- `Environment.validate()` returns the builder's `IssueReport` as Python objects (severity,
  message, command key) before `build` raises its summary (ENVIRON08).
- Configuration: a `dict`/YAML/JSON `EnvironmentConfig`, with stores from the core
  `StoreRouterConfig` and liquers-store's OpenDAL factory chained in.
- `shutdown()` is idempotent; it depends on `ENVIRONMENT-ORDERLY-SHUTDOWN` for a real teardown (see §7).

### 3.4 Commands

```python
import liquers as lq

@lq.command                      # state comes from the first parameter named `state`
def repeat(state: str, count: int = 2) -> str:
    """Repeat the input text."""
    return state * count

@lq.first_command(volatile=True, namespace="demo")
def now() -> str: ...
```

- Stage 1 introspection in Python with `inspect.signature` + `typing.get_type_hints` + docstring
  → a baseline dict → `CommandDeclaration::from_introspection`; decorator kwargs →
  `enhance`; then `apply_conventions`, `fill_defaults`, `finish`. Python-specific facts (how the
  state is passed: `text` / `value` / `state` / `none`, whether `*args` arrive spread) go in
  `hints.python`.
- Exact inference only: positional-or-keyword parameters, `*args` (→ `multiple`), defaults,
  annotations `int`/`float`/`bool`/`str`/`bytes`/`Enum`/`Literal[...]`/`Optional[...]`; a
  `context` parameter is injected. Keyword-only parameters and `**kwargs` are **refused** with an
  error naming the parameter (positional binding would misbind them). Report inferred-versus-declared
  through metadata inspection.
- `first_command` = no state argument; `command` = state argument (old `liquer` semantics, owner
  TODO). Replacing a built-in warns differently from replacing a Python command.
- Execution: a sync Python command runs on a blocking thread (`spawn_blocking`), takes the GIL only
  to convert arguments, call and convert the result; links and plan arguments are resolved by core
  before Python is entered. An async Python command (M3) is awaited through `pyo3-async-runtimes`.
- Exceptions → `ExecutionError` carrying class and traceback (see §7 for the transport limit);
  `ConversionError` for bridge failures.

### 3.5 Runtime and threading

- Native only. `Py<PyAny>` is `Send + Sync`, so no bound is relaxed anywhere (RUNTIME01 is a
  compile-time assertion). A Pyodide/wasm target is out of scope (reason for RUNTIME02 `NA`).
- One Tokio multi-thread runtime per process, owned by liquers-py. Sync `evaluate` releases the GIL
  (`allow_threads`) and blocks on it. Calling sync `evaluate` from a thread running an asyncio loop
  raises `RuntimeError` pointing at `evaluate_async` (ASYNCQ06/08).
- Nested evaluation (`context.evaluate(...)` inside a sync command) blocks on the runtime handle
  from the blocking thread with the GIL released; RUNTIME04 must show it does not deadlock with the
  default job-queue capacity.
- Free-threaded CPython (3.13t+) is not claimed in this plan; record it as a later decision.

### 3.6 Package layout **(confirm)**

- Distribution `liquers-py`, import name `liquers` (matches Appendix A), native module
  `liquers._native`. Python files under `liquers-py/python/liquers/`: `__init__.py`,
  `commands.py` (decorators), `env.py`, `errors.py` (exception hierarchy re-exported from native),
  `values.py`, `records.py`, `server.py` (M6), `_native.pyi`, `py.typed`.
- Rust: keep the OBJECT wrapper files; replace `value.rs`, `context.rs`, `commands.rs`; add
  `bridge.rs` (conversion), `environment.rs`, `eval.rs`, `command/` (adapter + declaration
  handover), `store/` (M4), `records.rs` (M5). Delete `interpreter.rs`, `state.rs` after
  porting what is needed into `eval.rs`.

---

## 4. Feature selection matrix (target)

Python baseline per guide §4, plus what the owner's earlier TODO asks for.

| Feature | Level | Selected | Milestone | Planned disposition and notes |
|---|---|---|---|---|
| `OBJECT` | Essential | yes | M1–M2 | 01–09 required. Existing wrappers kept; OBJECT09 is the encode round-trip (pass host strings through `encode()`) |
| `ERROR` | Essential | yes | M1–M2 | 01–06 required. Typed hierarchy `LiquersError` + one subclass per `ErrorType`; Python can construct one (ERROR06), which STORE/RECIPE need |
| `RUNTIME` | Essential | yes | M1–M2 | 01, 03–06 required; **02 `NA`** (no wasm target; reverses if a Pyodide build is selected) |
| `VALUE` | Essential | yes | M1–M2 | 01–13 required |
| `VALUE`/RECORDS | sub-feature | yes | M5 | 01–08, 10 required; **09 `NA`** (Python has an async model); **11 `NA`** unless Python-implemented `RecordView`/`RecordSource` is selected later; 04/05 apply to the buffer-protocol lending |
| `ENVIRON` | Essential | yes | M1–M2 | 01–08 required |
| `EVAL` | Essential | yes | M1–M2 | 01–06 required; EVAL06 asserts the cancellation result the queued manager actually produces |
| `COMMAND` | Essential | yes | M1–M2 | 01–14 required |
| `ASYNCQ` | Profile | yes | M3 | 01–06, 08 required; **07 `NA`** (Python has an async model) |
| `ASYNCCMD` | Optional | yes | M3 | 01–06 required |
| `STORE` | Optional | yes, directions 1+2+3 | M4 | 01–17 required per store; 05b included; **18 `NA`** linking `LANGUAGE-STORE-CONFIG-FUNCTION-NOT-DEFINABLE`; STORE12 override half `NA` if no factory in the chain shares a type name |
| `RECIPE` | Optional | yes | M4 | 01–07 required |
| `MODULE` | Optional | yes | M6 | 01–12 required; search path empty by default, opt-in prefixes; modules prefetched from the store before import |
| `UIUSE` | Optional | yes (egui) | M6 | 01–08 required; egui owns the main thread, so it runs as a foreground call |
| `UIDEF` | Optional | no | — | NS; not in this plan |
| `POLYGLOT` | Optional | no | — | `NA`: no second language runtime exists inside a Python process. Reverses when liquers-lib embeds Starlark. 03/04 are pre-satisfied by `ForeignValue::origin()` |
| `WEBSERV` | Optional | yes | M6 | 01–05 required; 06–08 (Python handlers) NS until `COMMAND`-based handlers are designed |
| `WEBAPI` | Optional | later | M7 (optional) | NS. FastAPI/ASGI first, Flask/WSGI second |
| `STUBS` | Optional | yes | M5 | 01–07 required; `.pyi` written by hand **and** verified by `mypy.stubtest` in CI (STUBS03), mypy/pyright run on good and bad samples (STUBS02/06); decorators typed with `ParamSpec` (STUBS04) |
| `PACKAGE` | Optional | yes | M0 (build), M5 (release) | 01–07 required; PACKAGE06 required because extras exist (`[polars]`, `[images]`, `[arrow]`) |

Expected `NA` count: about six across the whole integration, in line with the guide's "two or three
per integration is ordinary" once RECORDS' conditional tests are discounted.

---

## 5. Milestones

Each milestone ends with its tests green in CI and the matrix in §4 updated in the design folder.

### M0 — Foundations (no feature claims)

1. Upgrade PyO3 to the newest stable line and migrate from GIL refs (`&PyAny`) to `Bound<'py, T>`;
   lift `requires-python`'s upper bound; drop the override from `README.md` and `CLAUDE.md`.
   Closes `PY-PYO3-0-21-CANNOT-TARGET-PYTHON-3-13`. Add `pyo3-async-runtimes` (tokio) at the
   matching version.
2. Resolve `PY-MODULES-NOT-DECLARED-IN-LIB`: delete `interpreter.rs`, `state.rs`, `commands.rs`
   (their useful ideas are captured in §3.4) rather than declaring them.
3. Convert to the mixed layout of §3.6; rename the import to `liquers` **(confirm)**.
4. Harness (answers in §6): `uv`/venv + `maturin develop`, pytest with one module per feature,
   an `na(reason)` marker, `scripts/test-liquers-py.sh` that cleans `target/` first.
5. Add `liquers-py` to `scripts/check-build-matrix.sh` (native row only).
6. Fix the `PYTHON-WRAPPER` design status to `superseded` with `superseded_by:` the new folder,
   and update `specs/README.md` (CLAUDE.md requirement for design folders).

Exit: wheel builds on CPython 3.9–3.13, the existing two test files pass renamed under OBJECT.

### M1 — Thin end-to-end baseline (guide §7 item 11)

The point is to prove every path exists, not to finish any feature.

1. `PyObjectValue` + `py.Object` registration; `bridge.rs` with primitives, bytes, list, dict.
2. `liquers.Environment` over `default_environment_builder()` with liquers-lib commands and a
   memory store; lazy default environment.
3. `evaluate(query) -> python value` and `evaluate_state(query) -> State` (sync).
4. `@lq.command` / `@lq.first_command` through `CommandDeclaration`, sync execution only.
5. `LiquersError` hierarchy, Python exception → `ExecutionError` on the evaluation path.
6. Thin paths: one built-in (`ENVIRON01`/`EVAL01`), one Python command (`COMMAND01`, `COMMAND02`
   — the guide's `hello/repeat-3`), one `-R/` key written to the memory store and read
   (`STORE07`-shaped), one recipe resolving through `DefaultRecipeProvider` (`RECIPE06`-shaped),
   one Python command feeding a Rust command in a single query.

Exit: those tests green. A failing thin path against a defect outside liquers-py is filed and the
feature marked `BLOCKED` before continuing.

### M2 — Complete baseline: OBJECT ERROR RUNTIME VALUE ENVIRON EVAL COMMAND

- OBJECT01–09: audit every existing wrapper against the conventions (§3 of the design: naming,
  `__repr__`, `__eq__`/`__hash__`, immutability, enum-as-string with unknown-variant policy).
  Check the field renames other issues made (`QUERY-ABSOLUTE-FIELD-NAME-AMBIGUOUS`,
  `EXPIRY-RECORDS-NO-REASON`) are reflected.
- ERROR01–06, including `LiquersError(kind, message, key=..., query=...)` constructible in Python.
- VALUE01–13 with the full table in §3.2: polars, Pillow, `ConversionMode`, pickle codec opt-in
  (`Environment(config={"codecs": {"pickle": true}})`), scalar operators on `liquers.Value`, State
  metadata policy.
- ENVIRON01–08: config dict/YAML, `validate()`, registry override from YAML, reset, shutdown.
- EVAL01–06: `evaluate`, `evaluate_state`, `get_asset` (`AssetRef` wrapper with status, metadata,
  logs, cancel); payload/session options.
- COMMAND01–14: defaults, enums (`Enum`/`Literal`), variadics, context injection, unregister,
  post-publication registration (replay), closure capture, declaration copy-on-register
  (COMMAND14), planner-relevant flags reaching the planner (COMMAND12: `volatile`, `cached`).
- RUNTIME01, 03–06: compile-time bound assertion, nested evaluation, handle release (count live
  `PyObjectValue` and command callables behind a `debug-handles` feature, as liquers-web does),
  panic containment (`catch_unwind` at every `#[pyfunction]` boundary is PyO3's default; assert it).

Exit: the general baseline profile `CONFORMANT`.

### M3 — ASYNCQ and ASYNCCMD

- `await lq.evaluate_async(query)` via `pyo3-async-runtimes::tokio::future_into_py`;
  cancellation from `asyncio` (task cancel → asset cancel) and back.
- `async def` commands detected with `inspect.iscoroutinefunction`, run on the Python event loop
  owned by the caller; metadata marks them async (ASYNCCMD06). Decide and document what happens
  when no loop is running (recommend: a dedicated background loop thread owned by liquers-py).
- Async service wrappers (store, asset manager, context) beside the blocking ones (owner TODO).

### M4 — STORE (directions 1, 2, 3) and RECIPE

- Direction 1: a Python store protocol (`get`, `set`, `get_metadata`, `set_metadata`,
  `contains`, `listdir`, `makedir`, `remove`, `removedir`, `is_supported`, plus the optional
  data-only methods) adapted to `AsyncStore`, methods resolved once at construction, absent
  optional methods fail with `KeyNotSupported` (guide "Service adapters"). Absent vs failure:
  return `None` for absent, raise `LiquersError(KeyNotFound)` or any exception for failure.
- Direction 2: the integration's stores are liquers-store's OpenDAL backends and core's
  file/memory stores — no Python-specific backend needed.
- Direction 3: router from a configuration document; a Python store is named in the document by
  an identifier registered with `env.register_store_object(name, obj)` through a `type: python`
  factory chained before the built-ins; `read_only`, `metadata` sections; `data_facts` time crossing
  with `BackendTime::parse`.
- RECIPE: Python recipe-provider protocol adapted to `AsyncRecipeProvider`, appended through
  `with_appended_recipe_provider`; `recipe_opt` must answer for every producible name.
- Run STORE01–07 against each store (memory, file, one OpenDAL backend, the Python adapter).

### M5 — RECORDS, STUBS, PACKAGE

- RECORDS: `RecordView` / `RecordSource` wrappers; `__iter__` over chunks (route 1),
  `__aiter__` stream, bounded `materialize(max_rows)`; read-only buffer protocol on a
  `RecordBatch` column; `.to_arrow()` copying first, with zero-copy deferred to
  `RECORDS-ARROW-C-DATA-EXPORT-NOT-BUILT`; roles and origin in Arrow schema metadata (RECORDS03).
- STUBS: `.pyi` + `py.typed`, `stubtest` + mypy/pyright good/bad samples in CI.
- PACKAGE: wheels for manylinux x86_64/aarch64, macOS, Windows via `maturin` in GitHub Actions;
  version locked to the workspace version; system OpenSSL vs vendored decided for wheels (wheels
  must not depend on host `libssl` — likely re-enable `vendored` for the wheel build only);
  quick-start in `liquers-py/README.md` exercised by PACKAGE03.

### M6 — WEBSERV, MODULE, UIUSE

- WEBSERV: `lq.serve(env, host, port, background=False)` assembling the `liquers-axum` builders
  (`QueryApiBuilder`, `StoreApiBuilder`, `AssetsApiBuilder`, `RecipesApiBuilder`) on the same
  `EnvRef`; background handle with `ready()`, `shutdown()`.
- MODULE: a `sys.meta_path` finder over configured store prefixes, prefetching bytes outside the
  import lock, registering loaded keys as dependencies, refusing untrusted prefixes.
- UIUSE: `lq.run_gui(env, ...)` on the main thread with egui; optionally combined with a
  background server (owner's third usage mode).

### M7 (optional) — WEBAPI

FastAPI/ASGI and Flask/WSGI adapters over shared Rust handler functions; needs the
"framework-neutral handler" refactor in liquers-axum the guide describes.

---

## 6. Test harness (guide §3, the seven questions)

1. **Host vs value semantics.** Every test except UIUSE and WEBSERV needs only a bare CPython
   interpreter with the extension installed.
2. **Lighter runtime.** pytest in a venv is already the light harness. Rust-side unit tests
   (`cargo test -p liquers-py --no-default-features`) cover the bridge without Python packaging.
3. **Version coupling.** PyO3 ↔ CPython version, `pyo3-async-runtimes` ↔ PyO3, polars-Rust ↔
   python-polars Arrow compatibility for the DataFrame hand-off. Record the pins in the README and
   make `maturin develop` fail loudly on a mismatch.
4. **Separate driver.** None, except a display for UIUSE (run headless with `egui` test harness or
   skip with a recorded reason in CI).
5. **Native test binary.** Yes; the only trap is `extension-module`, already handled by the default
   feature split (`liquers-py/Cargo.toml`).
6. **Must stay heavyweight.** PACKAGE02/03/07 run against an installed wheel in a fresh venv, never
   against `maturin develop`.
7. **One test dragging the suite.** Optional extras (`polars`, `PIL`, `pyarrow`) are imported per
   test with `pytest.importorskip`; UIUSE tests sit behind a marker deselected by default.

Disk: a liquers-py build now includes liquers-lib; run it after `cargo clean`, separately from the
`liquers-lib` loop, as CLAUDE.md already prescribes for liquers-web.

---

## 7. Python-related issues and where they go

There are **no open GitHub issues** in `orest-d/liquers`; the backlog lives in `specs/issues/`.
Every issue whose `area` includes `py` or which names liquers-py, open ones first:

| Issue | Status | Plan action |
|---|---|---|
| `PY-PYO3-0-21-CANNOT-TARGET-PYTHON-3-13` | draft P3 M | **M0** — PyO3 upgrade; closes it |
| `PY-MODULES-NOT-DECLARED-IN-LIB` | draft P2 M | **M0** — delete the orphans instead of declaring them; closes it |
| `PY-VALUE-SERIALIZER-IS-A-STUB` | draft P2 M | **M1** — closed by removal of the own `Value` (liquers-lib's serializer); `py.Object` pickle codec in M2 is the remaining piece |
| `PY-VALUE-ENUM-OVERSIZED` | draft P3 S | **M1** — closed by removal; liquers-lib's `Value` is covered by `CORE-VALUE-ENUM-OVERSIZED` |
| `LANGUAGE-EXCEPTION-FIELDS-LOST-IN-TRANSPORT` | accepted P3 M | **M2** — Python class and traceback ride in the message until core gains `language_context`; ERROR03 asserts on the evaluation path. Do the core change here if it is cheap, since both bindings need it |
| `ERROR-WITH-KEY-SETS-QUERY-FIELD` | draft P2 L (phase2-blocked) | M2 — expose whatever fields exist; revisit the ERROR wrapper when it lands |
| `CORE-VALUE-INTERFACE-CAPABILITY-SPLIT` | accepted P2 L | none required; liquers-py no longer implements `ValueInterface` itself, which removes it as an implementor to migrate |
| `JS-COMMAND-CANNOT-ACCESS-CONTEXT` | draft | **M2** — COMMAND07 needs the same portable context injection; design it once for both bindings |
| `ENVIRONMENT-ORDERLY-SHUTDOWN` | draft | **M2** — ENVIRON06/RUNTIME05 depend on it; mark `BLOCKED` naming it if shutdown cannot release the manager |
| `ENVIRONMENT-MANAGER-REFERENCE-CYCLE` | draft | M2 — the same cycle keeps handles alive; RUNTIME05's counter will show it |
| `LANGUAGE-STORE-TYPE-NOT-DEFINABLE` | draft P2 M | M4 — Python stores are named instances under `type: python`, as in liquers-web; not a store type |
| `LANGUAGE-STORE-CONFIG-FUNCTION-NOT-DEFINABLE` | draft P3 M | M4 — STORE18 `NA` linking it (guide §6 Python: measure GIL cost before allowing) |
| `STORE-ABSOLUTE-KEY-NOT-TYPE-ENFORCED` | draft | M4 — the Python adapter refuses relative keys itself (STORE05) |
| `SHAREABLE-METADATA-AND-BYTES-ARE-DEEP-COPIED` | draft L | M4 — note the copy cost at the store boundary; no action |
| `RECORDS-ARROW-C-DATA-EXPORT-NOT-BUILT` | draft P3 L | **M5** — copy-based `.to_arrow()` first; zero-copy when this lands |
| `QUERY-AST-DISCARDS-ENTITIES` | draft | M2 — check `ActionParameter` exposure in `query.rs` once fixed |
| `COMMAND-METADATA-ENHANCEMENTS` | accepted | M2 — expose new fields through declaration; "Python bindings can follow" |
| `UI-VARIADIC-ARGUMENT-LIST-EDITOR` | draft | M6 — no `gui_info` reader in Python; no action |
| `AGENT-WORKBENCH-SURFACE`, `AGENT-MEMORY-SERVICE` | draft | not in scope; a Python surface could serve them later |
| `LANGUAGE-GUIDE-NO-DOCUMENTATION-SECTION` | draft P2 M | M5 — liquers-py's README and API docs are written regardless; feed the experience back to the guide |
| `DATA-FORMAT-CONSTANTS-AND-TOOLING`, `CORE-VALUE-ENUM-OVERSIZED`, `WORKSPACE-NOT-RUSTFMT-CLEAN` | draft | no liquers-py action once the own `Value` is gone; rustfmt liquers-py in M0 |
| `PY-PYO3-REJECTS-PYTHON-3-13`, `PY-VALUE-TYPE-DESCRIPTIONS-MISSING`, `FOREIGN-VALUE-TYPES-NOT-REGISTERED`, `COMMAND-DECLARATION-FORMAT`, `ARGUMENT-DECLARATION-IS-ALL-OR-NOTHING`, `CORE-SYNC-STORE-TRAIT-OBSOLETE`, `VARIADIC-ARGUMENT-STARVES-LATER-ARGUMENTS`, `COMMAND-ALIAS-DEFINITION-UNTESTED`, `RECIPE-PROVIDER-BY-NAME`, `REPO-DEAD-CODE-HYGIENE`, `QUERY-ABSOLUTE-FIELD-NAME-AMBIGUOUS`, `EXPIRY-RECORDS-NO-REASON` | closed | context only; their outcomes are the building blocks this plan reuses |

### To file when the design folder is created

- `PY-WRAPPER-DESIGN-MARKED-COMPLETE` — `specs/design/python-wrapper/DESIGN.md` says `complete`
  for a design that was never implemented; fixed in M0 by marking it superseded.
- `PY-TESTS-HAVE-NO-RUNNER` — `liquers-py/tests/*.py` are run by nothing in the repository (no
  script, no CI step, no documented command).

---

## 8. Risks

1. **Build size and time.** liquers-lib with polars in a wheel is large; CI wheels for five
   platforms will be slow. Mitigation: build `liquers-lib` features per extra only if a single
   "full" wheel proves too big (decide in M5 with numbers).
2. **polars Rust ↔ python-polars hand-off.** Version skew between the two Arrow implementations is
   the most likely source of late failures; use the Arrow C stream interface (`__arrow_c_stream__`)
   rather than a polars-version-specific path.
3. **Nested evaluation deadlock** (old phase 2 §3.3). Measure in M2 before adding policy types.
4. **GIL contention** for many small Python commands on a multi-threaded runtime. Benchmark in M2;
   free-threaded CPython is the long-term answer.
5. **Core additions.** `language_context` on `Error`, context injection, orderly shutdown are core
   changes; keep each additive (guide §3) and coordinate with liquers-web, which needs the same.

---

## 9. Decisions

**All four confirmed by Orest on 2026-10-17** ("I agree with the recommendations"). The
**(confirm)** marks in §3 are therefore settled.

| # | Question | Decision |
|---|---|---|
| 1 | Replace liquers-py's own `Value` with `liquers_lib::value::Value` + `ExtValue::Foreign`? | **Yes** — guide mechanism, full command library, removes two issues |
| 2 | Import name `liquers` (distribution `liquers-py`) instead of `liquers_py`? | **Yes** — matches Appendix A and the old package; a breaking rename now costs nothing because nothing depends on it |
| 3 | Python dispatch by closures rather than `pycall` aliases? | **Yes** — matches liquers-web, keeps callable identity |
| 4 | Pickle codec in baseline but off by default? | **Yes** — honours decision 5 and the guide's "opt-in, never implicit" |

---

## 10. Next step

Run the `liquers-project` workflow to create `specs/design/liquers-py/` with phase 1 drawn from
§0, §3 and §4 of this plan (scope, selected features, decisions), phase 2 from §3 with the full
"Objects/API to map" dispositions (guide §7 item 2), phase 3 from Appendix A's tests mapped to
the §4 dispositions, and phase 4 from §5. M0 can start in parallel, since it changes no public
contract except the import name.
