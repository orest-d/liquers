---
id: STORE-FACTORIES-IN-CORE
kind: design
title: Store configuration and factories in liquers-core
workflow: liquers-project
status: draft
phase: examples
area: [core/store, store/config, store/backends, web, docs]
gh_pr: []
issues: [STORE-CONFIG-IN-CORE]
affects_docs: [reference/STORE_CONFIG_FSD.md, reference/api/DOC_01_ARCHITECTURE_REFERENCE.md, guides/LANGUAGE-INTEGRATION_GUIDE.md, guides/STORE_FACTORY_GUIDE.md]
created: 2026-08-27
superseded_by:
---
# Store Configuration and Factories in `liquers-core` — Design Tracking

**Created:** 2026-08-27

## Phase Status

- [x] Phase 1: High-Level Design (approved)
- [x] Phase 2: Solution & Architecture (approved)
- [x] Phase 3: Examples & Testing (awaiting approval)
- [ ] Phase 4: Implementation Plan
- [ ] Phase 5: Documentation
- [ ] Implementation Complete

## Notes

Resolves feature `STORE-CONFIG-IN-CORE` (P0 by maintainer decision), one of three recorded
prerequisites for the document-driven setup path in `design/environment-builder`.

**Scope widened at the user's direction after the first Phase 1 draft; complexity M -> L.** The
issue as filed proposed moving *pure data only* and explicitly left `StoreFactory` and
`StoreRouterBuilder` in `liquers-store`. That boundary is rejected: `liquers-web` needs the builder
and the factory trait as much as the config types, so under the data-only boundary its
`liquers-store` dependency survives and the stated goal is not met. The committed target is that
**`liquers-web` depends on `liquers-store` not at all**, which requires the config types, the
`StoreFactory` trait, factory chaining and `StoreRouterBuilder` all to land in `liquers-core`.
`liquers-store` is reduced to the OpenDAL backend crate plus compatibility re-exports.

**Three pieces that do not exist today.** Factory *chaining* into a composite factory; a *core
factory* for the stores core already implements (`memory`, and `filesystem` off wasm); and a
*parametrisable* factory assembled from a map of store-type names to creation functions rather than
a trait impl. `liquers-store` supplies an OpenDAL factory and a ready-made core-then-OpenDAL chain.

**Phase 1 gate decisions (user, second round).** Chaining is **first-wins**, with core registered
first, then `liquers-store`, then `liquers-lib`, then the integration — so the core definition of a
store type is stable and no downstream crate can redefine it. The overlap warning and its
`eprintln!` are **not implemented**; `store_types()` stays on the trait, so a factory still reports
what it claims and a caller can detect overlap if it wants to. The map-based parametrisable factory
is **confirmed**. `liquers-store`'s `opendal` feature is **kept** — non-OpenDAL backends in that
crate are expected, so an OpenDAL-free configuration keeps its purpose even though the wasm reason
in its manifest comment no longer applies.

**The browser's `http` override changes mechanism.** Today `WebStoreFactory` beats the built-in
OpenDAL `http` *because* factories are consulted before built-ins;
`design/liquers-web-store/phase2-architecture.md` argues explicitly that consulting factories second
would make that impossible. Under first-wins with core first, a later factory can never override an
earlier one — the override survives only because `liquers-web` drops `liquers-store` and the OpenDAL
factory claiming `http` is never in its chain. Same outcome, different mechanism, so that rationale
is superseded rather than relocated and the new rule must be documented where a reader finds it.

**Phase 1 gate decisions (user, third round).** The builder gets **no built-in fallback** — every
store it creates comes from a factory it was given — and each crate instead offers a **default
factory** as a convenience (core's is the core factory; `liquers-store`'s is core's chained with
OpenDAL's). An **unclaimed `store_type` is an error that lists the store types the chain supports**,
enumerated from the factories themselves, so the message is accurate for the build in hand.
**Overriding is a chain the caller composes**, putting their factory first: first-wins fixes the
*default* ordering, not the only possible one. And a factory **describes, per store type, the
configuration arguments it accepts** — which is what makes the supported-types error possible and
lets the configuration format be documented from the code that implements it.

The argument-description requirement is the piece with the most design freedom left.
`command_metadata.rs`'s `ArgumentInfo` is the nearest precedent but is shaped for positional command
parameters (`multiple`, `injected`, `gui_info`, `CommandParameterValue`) while store configuration is
a `HashMap<String, serde_json::Value>` of named keys. Phase 2 chooses reuse, subset or a
store-specific type, and decides how many optional-vs-required/default/enum fields are worth the
cost to every factory implementation.

Phase 1 correction to the issue: its verification item 3 was unachievable under the data-only
boundary (`liquers-web` also uses `StoreRouterBuilder` and implements `StoreFactory`). Under the
widened boundary it is achievable and strengthens to "no `liquers-store` dependency at all". The
issue's "what moves and what does not" table is superseded and is corrected at Phase 5, along with
`complexity: L`.

Documentation intent changed with the scope: Phase 1's earlier `neither` on a guide no longer holds.
"How do I add a store type" and "how do I override a built-in one" become repeatable tasks with a
real answer, so a new `specs/guides/STORE_FACTORY_GUIDE.md` is provisionally committed, with
`WebStoreFactory` as the worked example.

Open for Phase 2: how rich the per-store-type argument description is, and whether the resulting
store-type registry should be exportable the way `specs/command_registry.yaml` is; whether a factory
can explain a type it knows of but cannot build (the `opendal`-off and wasm-`filesystem` messages
worth not losing); whether `with_factory` survives alongside chaining, given that with no built-in
fallback `StoreRouterBuilder::new(config)` alone can build nothing; whether `expand_env_vars`'s bare
`std::env::var` moves verbatim, is `#[cfg]`-gated or takes a closure; the re-export shape; `toml`
feature forwarding; and whether the §3 `area` vocabulary needs `core/store` widened now that
`store/config` names files that will not exist.

Noted for filing rather than absorbing: with per-type argument descriptions in hand, a chain could
validate a `StoreRouterConfig` — unknown type, unknown key, missing required key — without
constructing a single store. Attractive, and beyond this design's scope.

## Phase 2 notes

**No blocker found** in the known-issue preflight. `WEB-NATIVE-IO-TIER2` (P3) is the one non-blocker
with a real design constraint, honoured rather than deferred: no `Send`/`Sync` bound on the trait or
on the map factory's closures, so a Promise-based IndexedDB store stays expressible.

**Reuse found rather than invented.** `command_metadata::ArgumentType` covers store argument types
and is reused instead of a parallel enum; `Error::not_supported` replaces `general_error` for an
unclaimed store type; `AsyncMemoryStore` / `AsyncFileStore` are already core types, so the core
factory constructs nothing new.

**`StoreFactory::store_types()` changes return type** (`Vec<String>` -> `Vec<StoreTypeInfo>`).
Breaking, and taken deliberately: two in-tree implementors, both edited anyway, and a parallel
`store_type_info()` with a default impl would leave two sources of truth for what a factory claims.

**`StoreTypeAvailability`** preserves what `create_store`'s single `match` provides today and what
`LANGUAGE-INTEGRATION_GUIDE.md` makes conformance requirement `STORE13`: a type that is real but
ungated-off in this build must be refused with the feature or target responsible, never as "unknown".

**Two findings outside the Phase 1 boundary, surfaced at the gate rather than absorbed:**

1. `from_yaml` / `from_json` / `from_toml` / `expand_env_vars` use `Error::new(ErrorType::ParseError,
   ...)`, which `CLAUDE.md` forbids, and no typed constructor fits (`key_parse_error` and
   `query_parse_error` both require a `Position`). Proposed: add `Error::parse_error(String)` to
   `liquers-core/src/error.rs` rather than move a known violation into the crate that enforces the
   rule most strictly.
2. `scripts/check-build-matrix.sh` has **no `liquers-core` rows at all**, and core is about to gain
   its first optional feature (`toml`) and target-conditional store availability. Four rows proposed.

**The documentation finding that matters most.** `LANGUAGE-INTEGRATION_GUIDE.md` §"Taking only part
of the store support crate" enumerates three resolutions to exactly this problem, recommends option 3
(optional backend feature) and explicitly rejects option 2 (move the types into `liquers-core`) as
"widens core for one consumer's benefit". This design does option 2. The rejection was written when
`liquers-web` was the only consumer and no longer holds once core itself must embed a store
description; the section is rewritten to record the reversal and its reason, while keeping option 3's
three cost lessons, which remain true of the surviving `opendal` feature. Conformance item `STORE12`
("a factory that overrides a shared type name resolves to the integration's implementation") also
needs restating: after this change `liquers-web` has nothing to override.

## Merge with `main` (2026-08-29)

Pulled `origin/main` at `2bb336f`. **No code changed** — four new design folders, `specs/README.md`,
`specs/index.csv` and four issue front-matters. Merged clean; three consequences for this design.

**1. This folder was misnamed and broke the index.** `scripts/docs_index.py --check` reported
`duplicate id STORE-CONFIG-IN-CORE (also specs/issues/STORE-CONFIG-IN-CORE.md)` — `init_feature.py`
derived the design id from the folder name, and the folder was named after the issue. The three
sibling designs main added all name the folder for the *solution* while the issue names the
*problem* (`RECIPE-PROVIDER-BY-NAME` -> `recipe-provider-selection`,
`STORE-OPENDAL-SLASH-HANDLING` -> `opendal-path-mapping`). Renamed
`store-config-in-core` -> `store-factories-in-core`, id `STORE-FACTORIES-IN-CORE`, which also
describes the widened scope better. `--check` now reports 0 errors.

**2. Registration that Phase 1 owed and had not paid.** `CLAUDE.md` requires a PR adding a design
folder to update `specs/README.md`; it had not been. Added a §Stores capability line, repointed the
issue's `design:` from `environment-builder` to this folder (matching what main did for the three
siblings), applied the `complexity: M -> L` reclassification to the issue file, noted this design in
`environment-builder/DESIGN.md` alongside its three, and regenerated `specs/index.csv` and the
README's generated blocks.

**3. `design/opendal-path-mapping/` assesses this design against its old boundary.** It says a
`store_builder.rs` merge conflict is "possible" and lists `liquers-store/src/config.rs` and
`liquers-core` as "Not touched" — both written before the scope widened. Under the current boundary
`store_builder.rs` is gutted, not merely at risk. Checked file by file anyway: **no source file is
edited by both designs** (theirs is `opendal_store.rs` alone; this one does not touch it), so the
conclusion — no ordering constraint — survives. What goes stale is their documentation, and two
shared expectations are recorded in Phase 2 so neither design silently breaks the other.

**Confirmation from a sibling.** `recipe-provider-selection`'s Phase 2 rejects a `StoreFactory`-shaped
registry for recipe providers with a precise technical reason — `AsyncRecipeProvider` is generic in
`E`, so `dyn RecipeProviderFactory` is not object-safe, whereas "`StoreFactory` has no such problem
because `AsyncStore` is not generic". That independently confirms this design's object-safety
assumption, and is worth citing in the new store-factory guide as the boundary of the pattern.

## Phase 1 gate answers, fourth round (2026-08-29)

All seven Phase 1 open questions answered; four changed Phase 2 materially.

**Minimal, because store selection is fixed at compile time.** Unlike the command metadata registry
and like the type registry, the set of store types is settled when the binary is built, and a user
configures stores at most once. So: no exported registry file, no dynamic registration machinery. Do
provide a list of supported stores with descriptions and per-argument descriptions.

**Arguments carry JSON types, not `ArgumentType`.** The first Phase 2 draft reused
`command_metadata::ArgumentType`. That is now rejected: it is a command-parameter vocabulary
(`Integer`/`Float`/`IntegerOption`, `Enum`/`GlobalEnum` needing a `CommandMetadataRegistry`) with no
container variant, so it cannot express the browser `http` type's `keys: [...]`. Replaced by a small
`StoreArgumentType` mirroring JSON — scalars preferred, `Array`/`Object` allowed where genuinely
needed. Store configuration must stay ergonomic and directly representable as JSON. This is the one
place the "reuse core structures" instruction is not followed; core has no JSON-type enum to reuse
(checked `type_system.rs`, `media_type.rs`, `command_metadata.rs`), so nothing is shadowed.

**No compatibility shims at all.** No backwards compatibility is required, and a `liquers-store`
re-export of a core type is precisely the shadowing to avoid. `liquers-store/src/config.rs` and
`src/store_builder.rs` are **deleted**; the crate shrinks to `opendal_store.rs` plus a new
`store_factory.rs`. This supersedes the issue's verification item 1 ("still resolves via re-export;
no call site edited") — every call site moves, and all are in this repository.

**One own factory plus one default chained factory, per crate.** Stated as the convention the guide
teaches: `CoreStoreFactory` / `OpendalStoreFactory` / `WebStoreFactory` each describe only their own
crate's store types, and each crate's `default_store_factory()` chains its own after everything below
it that should be available. The default is what a consumer reaches for. `with_factory` returns with
**replace** semantics (append hid where a factory landed, which is the ambiguity first-wins removes),
plus a `chain_factory` convenience.

**Validate on construction; no separate validation path.** Stores are built at startup, so
construction is the validation. The constraint that follows: `create` must be fast and must not fetch
bulk data. A store type that benefits from pre-fetching is making a trade-off it must document.

**Unbuildable types stay** as a soft quality-of-life requirement, justified by being nearly free —
one enum field set in a `#[cfg]` branch that already exists. The case that matters is a documented
type disabled by a feature.

`liquers-store/toml` forwards to `liquers-core/toml` and drops its own `toml` dependency. The §3
`area` question was withdrawn and decided as bookkeeping: `store/config` is redefined by topic rather
than by a file list, since both files it names are deleted.

## Phase 3 notes

**Runnable examples, decided rather than asked.** Every scenario corresponds to code that exists and
is tested today, so a conceptual sketch could not be checked against anything.

**The test inventory is this phase's real content.** 18 tests exist in the code being moved, and
**three assert behaviour this design deliberately inverts** — a refactor that quietly rewrites its
own assertions is indistinguishable from a regression, so each is listed with what it asserts today
and what it must assert after:

| Test | Today | After |
|---|---|---|
| `factory02_factory_precedes_builtin` | a factory claiming `http` beats the built-in OpenDAL dispatch, "consulted **before** the built-in types" | a factory chained **before** the OpenDAL factory wins; renamed `chain03_earlier_factory_wins`. Its doc comment argues at length that consulting factories second "would make that impossible" — that argument is what first-wins removes |
| `factory03_unclaimed_type_falls_through` | an unclaimed type "still reaches the built-ins" | **deleted** — there are no built-ins; replaced by an assertion that the error lists the supported types |
| `factory04_gated_type_names_the_feature` | `create_store` on `s3` with `opendal` off names the feature | same guarantee via `StoreTypeAvailability`; both assertions kept, including "a gated-off type is not an unknown type" |

11 configuration tests move verbatim — if any needs its assertions changed, the move was not
behaviour-preserving and that is a finding, not an edit.

**`factory04` never runs in the default configuration.** It is `#[cfg(not(feature = "opendal"))]`,
so `cargo test -p liquers-store --no-default-features --features async_store` is not optional: it is
the only test covering the message `StoreTypeAvailability` exists to preserve.

**The design's thesis stated as an unfakeable assertion.**
`liquers-core/tests/store_router_STORE.rs::core_router01_builds_from_yaml_without_liquers_store`
cannot pass by accident, because a test in `liquers-core/tests/` has no way to reach `liquers-store`
— it is not a dependency.

Totals: 36 tests + 4 build-matrix rows; 13 assert behaviour that does not exist yet.

**OpenDAL examples added (Scenario 4), checked against the 0.55 sources rather than from memory.**
Field counts read from `opendal-0.55.0/src/services/*/config.rs`: `ftp` 4, `http` 5, `webdav` 6,
`sftp` 6, `azblob` 9, `gcs` 13, **`s3` 26** — about **134 fields across the 20 types in
`OPENDAL_STORE_TYPES`**. S3 is the first and only place `StoreArgumentType::Boolean` and `Number`
have real users; without it both would look speculative.

**A latent defect found while checking, filed as
[`STORE-OPENDAL-LIST-OPTION-MISPARSED`](../../issues/STORE-OPENDAL-LIST-OPTION-MISPARSED.md)
(P2, S).** `config_as_string_map` flattens a JSON array to JSON *text* (`["a","b"]`), while
OpenDAL's `ConfigDeserializer::deserialize_seq` splits on **commas** — so a list-valued option
parses into garbage. Reach today is one field (`tikv.endpoints`, the only non-scalar across all of
OpenDAL 0.55) behind the `opendal_tikv` escape hatch, hence P2. Two adjacent edges in the same
function: a JSON float stringifies to `"1000.0"` which OpenDAL's integer parser rejects, and null
becomes the literal `"null"`. Booleans and integers agree correctly. This design neither causes nor
fixes it — `config_as_string_map` moves to core unchanged.

**It answers the `Array` question with evidence.** An OpenDAL list option is spelled as a
comma-separated *string* in the document, so it is `StoreArgumentType::String` with the convention in
its `doc`, not `Array`. `Array` therefore has exactly one legitimate user — the browser `http` store's
`keys` — and `Object` has **none**, since no OpenDAL service config has a map-valued field.

**`STORE_CONFIG_FSD.md` updated now, not at Phase 5 — deliberately.** Two things learned in Phase 3
are true *at HEAD* and independent of this design, so holding them until the implementation lands
would leave the reference wrong in the meantime: the layering rationale (why `StoreRouterConfig`
exists alongside OpenDAL's own configuration) and the string-boundary encoding rules. Added with a
`## History` row and a `reviewed:` bump per §9.2. **Nothing about the factory redesign went in** —
`StoreTypeInfo`, chaining and core ownership are not true at HEAD and belong to Phase 5. Verified by
grep that no such name appears in the reference.

The correction it carries: the document asserted "OpenDAL does not provide a built-in way to create
operators from text configuration". True when written, false now — `via_iter` arrived in 0.48.0
(2024-07-26) and `from_uri` for all services in 0.55.0 (2025-11-11), while `from_map`/`via_map` were
removed in 0.55. What survives the correction is the *reason* `StoreRouterConfig` exists: every
OpenDAL form configures exactly one backend, with no key prefix, routing order, `${VAR}` expansion or
non-OpenDAL store type. It is a composition format, not a competitor.

**On requiring strings for OpenDAL parameters.** Quoting every value is always safe — a JSON string
passes through `config_as_string_map` verbatim, and OpenDAL's own text conventions then apply. But
booleans and whole numbers already round-trip correctly (`to_string` produces exactly what OpenDAL's
deserializer accepts), so *requiring* quotes everywhere would cost ergonomics without buying
correctness. Only arrays, objects, nulls and non-integral floats diverge. The reference states the
narrow rule rather than the blanket one.

**Resolved: describing OpenDAL's arguments must not mean maintaining OpenDAL's documentation.**
The trap the maintainer identified is not the *size* of describing ~134 external fields — it is that
a hand-written copy of another project's config surface becomes **silently wrong** when that project
adds a field, changes a type or renames one, with nothing to detect it. Wrong is worse than absent,
because a reader believes it. Two mechanisms, independent; the first alone suffices.

**1. `ArgumentCoverage`, a new field on `StoreTypeInfo` (required).** `Complete` for the store types
Liquers owns — core and browser — where the argument list *is* the specification and an unlisted key
may be rejected. `Partial { authority: <url> }` for OpenDAL types: the list is guidance, unlisted
keys pass through to the backend, and the authority is named. **An incomplete list is only a lie if
completeness was claimed**, so OpenDAL 0.56 adding a field makes our description less complete, never
wrong, and nothing has to be noticed for it to stay honest. The unclaimed-*type* error is unaffected:
it enumerates types, which are ours to know completely.

**2. Derive the field names rather than write them (recommended, deferrable).** Verified against
the OpenDAL 0.55 source, not assumed: `Configurator: Serialize + DeserializeOwned + Debug + 'static`
is a *trait bound*, so every service config serializes; all **62** `src/services/*/config.rs` derive
`Default`; and there is **not one** `skip_serializing_if` among them. So
`serde_json::to_value(S3Config::default())` yields every field name with its default, from the
version actually linked — a list that cannot drift because it is not written down. It gives names,
defaults, and types wherever the default is not null; it does not give doc text, required-ness, or
the type of an `Option` field.

The maintenance boundary this draws is the point: what stays hand-written is a
`store_type -> config type` mapping of ~20 entries, changing only when a *service* is added or
removed — the same cadence as `OPENDAL_STORE_TYPES`, already hand-maintained. Field-level churn,
where all the volume and volatility are, becomes free, and forgetting an entry degrades to "no
arguments reported", which under `Partial` is honest.

**Both are committed (maintainer decision).** The rationale for 1 is stronger and more general than
the OpenDAL-drift framing it was first given: Liquers is meant to accept backends it does not own,
and *any* externally-owned backend can only be described incompletely, because its arguments change
on someone else's release schedule. Without a way to say "partial", every such backend forces a bad
choice — claim completeness and be silently wrong on the next upstream release, or describe nothing
and give no guidance. OpenDAL is the first and largest instance, not the reason.

They compose rather than overlap: 2 fills in the names, 1 says the result is still not a contract —
which stays true even if derivation were perfect, since doc text, required-ness and valid argument
*combinations* remain outside it. Phase 4 order: `ArgumentCoverage` first (a `StoreTypeInfo` field
the core and browser factories both need), then derivation (which only fills `arguments` for the
OpenDAL factory).

Test-plan consequence: **41 tests, up from 36.** One of them carries a trap worth stating —
`derive01` must assert that a few long-stable field names are *present*, never that the list is
exhaustive. An exhaustive assertion would reintroduce through the test suite precisely the
maintenance burden derivation exists to remove, failing on every OpenDAL release that adds a field.

**Explicitly not attempted:** `StoreTypeInfo` cannot express that a group of arguments is mutually
exclusive or co-required — S3's static-keys / assume-role / customer-managed-SSE modes are the live
example. Encoding argument-group constraints is a much larger feature. The guide must say the
descriptions list arguments, not valid combinations.

Two Phase 2 gaps were found by the conformity pass and fixed there rather than worked around:
`StoreArgumentInfo`'s builder methods were never specified, and `liquers-web`'s
`default_store_factory` takes an argument (its factory is stateful, holding runtime-registered page
objects) where the other crates' take none — a deliberate deviation now recorded as such.

`liquers-validate` was not run and does not apply: the design contains no Liquers query, registers
no command, and evaluates nothing.

## S3 from arguments and from URI — run, not predicted (2026-08-29)

Asked whether S3 works via URI. Probed against OpenDAL 0.55 with a scratch test, then reverted.

**Both work and are exactly equivalent.** `s3://probe-bucket/data?region=eu-central-1&…` and the
`config:` map produce the same operator (`name=probe-bucket`, `root="/data/"`). `StoreConfig` cannot
express the URI form — there is no `uri:` field — and adding one is a format change beyond this
design, but the exact equivalence means it would be sugar over the same map rather than a second
mechanism.

**Construction never touches the network.** `s3` with a nonexistent bucket, `ftp.invalid:21` and
`https://example.invalid` all construct fine; OpenDAL builders are lazy. This is why the requested
offline S3 test is possible, and it confirms "`create` must be fast" is a rule implementations must
honour rather than one the backends already break.

**`region` is genuinely required and fails at construction**, offline — the clearest evidence for
the validate-on-construction decision.

**`from_uri` is much narrower than `via_iter`.** `DEFAULT_OPERATOR_REGISTRY` registers **10**
services; `via_iter` has **62** arms; 61 of 62 configs implement `from_uri`, so the limit is the
registry. `ftp://` fails as "scheme is not registered" while `via_iter("ftp", …)` succeeds in the
same build. Any future `uri:` support would be narrower than `config:`.

### The P0 this exposed

`liquers-store` declares `opendal = { version = "0.55.0", optional = true }` with **no features**,
and OpenDAL's `default` enables only `services-memory`. `cargo tree -p liquers-axum -e features -i
opendal` shows the server crate gets `services-memory` alone — not even `fs`. **All 21 types in
`OPENDAL_STORE_TYPES` are unconstructible in any consumer build**, while `STORE_CONFIG_FSD.md`
documents six of them with worked examples.

The crate's own tests pass only because dev-dependencies add `services-fs` and Cargo unifies
features across normal and dev dependencies when building tests — **the suite is green because of a
dev-dependency**, concealing the defect rather than missing it.

Filed as [`STORE-OPENDAL-SERVICES-NOT-ENABLED`](../../issues/STORE-OPENDAL-SERVICES-NOT-ENABLED.md)
(**P0**, S). Independent of this design; this design makes it *reportable*
(`StoreTypeAvailability::Unavailable`) but not fixed. Phase 4 must sequence the fix before `s3_01`
and `s3_02` can compile, or gate them.

Test plan: **43**, up from 41.

## Cross-design coordination

**Standing obligation (maintainer instruction, 2026-08-29): keep
[`design/opendal-path-mapping/`](../opendal-path-mapping/) updated whenever a change here impacts
it.** It is the only in-review design sharing a crate with this one, it is `status: in_review` and
unimplemented, and its author documented this design's interaction from the issue text rather than
from this folder — so it goes stale silently unless someone pushes.

Correction to an earlier reading in this folder: their §"Not touched" list and their "Read,
unchanged" entries describe **their own** edits and remain accurate. Only the interaction assessment
was stale, plus one line reference.

Done so far (2026-08-29): their Phase 2 §"Related open issues" bullet rewritten with the widened
scope and the ruled-out conflict; their `create_opendal_store` reference annotated as relocated by
this design; a dated cross-reference added to their `DESIGN.md` notes.

Re-check and push an update if any of the following changes here:

| Trigger | What to update there |
|---|---|
| The set of files this design edits changes | The no-conflict conclusion in their §"Related open issues" — it rests on `opendal_store.rs` being touched by neither |
| `create_opendal_store` lands somewhere other than `liquers-store/src/store_factory.rs` | Their "Read, unchanged" line reference |
| `StoreConfig::key_prefix` gains any behaviour change (it should not) | Their validation item (d), which asserts `key_prefix() == data` for a prefixed store |
| `scripts/check-build-matrix.sh` rows or the `opendal`-off shape change | Their Phase 2 validation command list, which requires the `opendal`-off configuration to compile |
| This design starts editing `opendal_store.rs` for any reason | Everything above; the two designs would then genuinely conflict |
| Either design reaches implementation | Whichever lands second re-reads the other before starting |

Their design does **not** need to change today; the conclusion improved rather than broke.

## Links

- [Phase 1](./phase1-high-level-design.md)
- [Phase 2](./phase2-architecture.md)
- [Phase 3](./phase3-examples.md)
- [Phase 4](./phase4-implementation.md)
- [Phase 5](./phase5-documentation.md)
