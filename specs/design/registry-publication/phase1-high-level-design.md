# Phase 1: High-Level Design — publishing registries across realms

## Purpose

In the client/server shape of `VISION.md`, a browser page plans its own queries and should be able
to plan a step that belongs to the server (`-server/heavy_stats`). For that it needs the server's
command metadata and, to judge what can cross back, its type descriptions — fetched from the server
it talks to, not hand-copied files that drift.

## Problem Example

Verified at HEAD. A page wants `-R/data/sales.csv/-server/heavy_stats/-/to_text`.

- `liquers-axum` has Store, Assets, Recipes and Query routes (`WEB_API_SPECIFICATION.md` §1); none
  returns a registry, so the page cannot ask the server what it offers.
- With a hand-exported `command_registry.yaml`, `registerCommands(yaml, impls)`
  (`liquers-web/src/command/import.rs` `read_registry`) refuses every command that has no
  JavaScript implementation (*"No implementation is given for command …"*), and keeps the realm the
  file says (`""`), not `server`. Global enums in the file are dropped with a warning.
- The page therefore fails to plan with *Action 'heavy_stats' not registered; command keys tried:
  'server--heavy_stats'*.

Should be: the server mounts `RegistryApiBuilder::new("/liquer/api/registry")`; the page fetches
`commands` and `types` and calls `importRealm("server", commands, types)`; the query then plans,
with `heavy_stats` in realm `server` and `to_text` local. Running the server step is
`multi-realm-dispatch`'s job.

## Scope and Acceptance Criteria

- **AC-1** Command metadata is published
  WHEN `GET {base}/commands` is requested
  THEN the answer is a `CommandMetadataRegistry` document in the exporter's format holding every
  executable command (no `store_config` entries), with global enums resolved inline and
  `global_enums` empty
- **AC-2** Type descriptions are published
  WHEN `GET {base}/types` is requested
  THEN the answer is the list of the default realm's `TypeInfo`
- **AC-3** Freshness is cheap to check
  WHEN either route is requested with `If-None-Match` equal to the `ETag` it last answered
  THEN the answer is `304 Not Modified` with no body
- **AC-4** Commands import under a chosen realm
  WHEN `import_realm("server", &remote, "")` is called
  THEN every command of the remote's realm `""` is reachable in realm `server`, aliases within
  that realm point into `server`, and the local realm `""` is unchanged
- **AC-5** A bad import changes nothing
  WHEN the local realm is `""` or non-executable, already holds commands, or an imported alias
  targets a realm that is not imported
  THEN `import_realm` returns an error naming the cause and the registry is unchanged
- **AC-6** Imported commands plan but do not run locally
  WHEN a query uses an imported command and no realm binding exists
  THEN it plans and validates, and evaluation fails with today's unknown-executor error naming the
  command
- **AC-7** A page imports a realm
  WHEN `importRealm(realm, commands, types?)` is called in `liquers-web`
  THEN the commands (and types) are available under `realm` in the current and every rebuilt
  environment, and calling it again for the same realm replaces the earlier import
- **AC-8** Types import under the same realm
  WHEN types are given to the import
  THEN `get_in_realm(realm, id)` finds each remote type (via `TypeRegistry::import_realm`)
- **AC-9** Off unless mounted
  WHEN a server does not merge the registry router
  THEN no registry route exists

Non-goals: executing an imported command (`multi-realm-dispatch`); push notification of registry
changes; publishing recipes (Recipes API); a Python client; relaying a realm the server itself
imported.

## Core Interactions

Commands (metadata registry import), value types (type registry import), Web/API (new routes),
`liquers-web` (page API and rebuild retention). Query, store and assets are untouched.

## Crate Placement

`liquers-core`: `import_realm` and `remove_realm` on `CommandMetadataRegistry`. `liquers-axum`: the
route builder. `liquers-web`: `importRealm`. The dependency flow is respected: axum and web each
depend on core only for this.

## Documentation Intent

- Reference: `WEB_API_SPECIFICATION.md` (new §Registry API, §1 table, §8 example); `REALMS.md`
  (imported realms: addressable, not executable); `COMMAND_DECLARATION.md` or the web README for
  `importRealm`.
- Guide: none.
- Other documents: `liquers-web/README.md` (conformance list).
- Documents to update: the above.

## Design Readiness

- **Readiness:** needs-decision
- **Automatic fixing:** not-eligible — `L`, new `pub` methods in `liquers-core`, new HTTP routes and
  a new JavaScript API across three crates (rules 1, 4, 6).
- **Leading issue:** **Open design - global enums on export.**
- **Explanation:** the published data already serializes (`CommandMetadata`, `TypeInfo`); the
  import is metadata relabelling; retention in the page follows `configureStore`. Phases 3-4
  assume the recommendations below; the type import waits for `realm-type-registry`.
- **Open questions:**
  1. **Open design - global enums on export.** Recommended: resolve them into each command
     (`CommandMetadata::resolve_global_enums`, as the MCP server does), so the client needs no
     global enum namespace per realm. Alternative: publish `global_enums` and import them under
     realm-prefixed names, which requires rewriting every reference.
  2. **Open design - document or envelope.** Recommended: answer the bare document (like a value
     transfer), so the same bytes feed `importRealm`, `liquers-validate --registry-file` and
     `registerCommands`; errors still use the §2.2 envelope. Alternative: wrap in `ApiResponse`.
  3. **Proposed resolution - realm naming.** The importer names the realm, as
     `realm-type-registry` question 1 recommends for types and `multi-realm-dispatch` question 2
     for the wire rename; `import_realm` takes the remote realm to read (default `""`).
  4. **Proposed resolution - freshness.** A strong `ETag` (the `Version` hash of the body) and
     `If-None-Match`; a registry is fixed for a server's run, so polling on reconnect suffices.
  5. **Proposed resolution - replacement.** `import_realm` refuses an occupied realm;
     re-import is `remove_realm` then `import_realm`, which `importRealm` does for the page.

## Design Dependencies

- `realm-type-registry` (`TYPE-REGISTRY-NOT-REALM-AWARE`) — **requires**, for AC-8 only:
  `TypeRegistry::import_realm` and a deserializable registry. Steps 1-5 do not wait for it.
- `multi-realm-dispatch` (`CORE-MULTI-REALM-INTERPRETER`) — **required-by**: answers its open
  question 4 (remote metadata); execution of imported commands is its scope.
- `cross-realm-transfer-planning` (`CROSS-REALM-TRANSFER-NOT-PLANNED`) — **required-by**: plans
  against the imported types.
- `computed-enum-alternatives` — **overlaps**, weak: a published computed enum has no provider on
  the client and is unchecked there.
- `global-enum-registration` — **overlaps**, weak: with it, a published registry cannot hold a
  dangling global enum, so resolution on export cannot fail.
- `remote-store-and-asset-manager` / `REMOTE-ASSET-MANAGER-AND-SYSTEM-API` — **overlaps**, weak:
  this is a public, read-only user route, not the trusted system API.
- `GLOBAL-ENUMS-NOT-IMPORTED-BY-LANGUAGE-INTEGRATIONS` (issue) — **overlaps**, weak: resolving on
  export means a realm import never needs it.
- Overlap judgement: T2 with `multi-realm-dispatch` was the close call (its question 4). Rejected:
  publishing and importing are useful without dispatch (validation and UI of server commands), and
  dispatch's blocking questions do not change this contract; recorded as `required-by`.

## Consolidated Findings

- `read_registry` in `liquers-web` requires an implementation per command, so an import needs its
  own path, not a flag on `registerCommands`.
- Default namespaces are registry-wide: an imported command in a namespace that is default on the
  server but not on the client needs `ns-…` in the client's query. Documented, not changed.
- `TypeInfo` already derives `Serialize`, so the types route ships without `realm-type-registry`;
  only importing types needs it.
- The route leaks the command list to whoever can reach it; it is off unless merged and sits
  behind the server's own layers like every other router (`WEB_API_SPECIFICATION.md` §9).
