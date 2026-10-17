# Agent memory migration guide — draft

Companion to [Phase 2](./phase2-architecture.md), written 2026-10-17 at Orest's request. It is the
draft of `specs/guides/AGENT_MEMORY_MIGRATION_GUIDE.md`, which Phase 5 creates from it after
checking every statement against the implementation. Until then it is also a design constraint:
a change that would make one of these steps larger needs a reason.

## 1. The principle: keep the durable part small and plain

A memory is mostly Markdown documents with front-matter, stored as `Source` assets **with no stored
metadata**. Everything else is derived from them and can be thrown away and recomputed. So an
upgrade of Liquers, of `ns-mem` or of the MCP tools migrates nothing by itself; only a change in
what the documents themselves are supposed to contain needs work, and that work is ordinary edits
that `check` finds for you.

| Part | Durable? | Where it lives | On migration |
|---|---|---|---|
| Documents (front-matter + body) | **yes** | the mounts, as `.md` files | edited only when the schema changes (§3.4) |
| The schema: `_schema/memory.yaml` and its vocabulary files | **yes** | the memory root | edited by hand per release notes (§3.3) |
| The write log `_log/YYYY-MM.md` | **yes**, append-only | the memory root | never rewritten; old lines stay as written |
| The profile (`liquers-agent` configuration) | **yes** | the deployment | edited per release notes, like any configuration |
| Metadata records (title, description, media type, versions) | no | derived on every read (`storage: none`) | nothing to do |
| `.__metadata__` sidecars, only on a mount that stores metadata | no — a cache | beside the files | delete them; they are re-derived (§3.1) |
| `ns-mem` results (listings, searches, reads, check reports) | no | never written to a store: volatile, or (for `read`) cached in the running process only | nothing to do |
| Assets with recipes | no | stored results of recipe keys | drop the computed results; they recompute on demand (§3.2) |
| Versions (`expected_version`) | no | content hashes, held by an agent within a session | nothing to do: the same bytes give the same version after a move |

## 2. Rules that keep it so

Anyone extending the memory follows these, so the table above stays true:

1. **Metadata lives in the front-matter.** Store handlers (`front_matter`, `markdown_tiers`) derive
   the record from the document; they never hold information the document lacks. Writable checkout
   mounts use `storage: none`.
2. **A new schema field is optional or has a `default`.** A field that old documents lack and that
   `check` would require is a schema change (§3.4), announced in the release notes with its fix.
3. **`memory.yaml` carries `version`.** The service reads its own version and every older one
   (missing fields take their defaults) and refuses a newer one, naming both numbers.
4. **The log is append-only, and its readers tolerate lines they do not understand.** A format
   change applies to new lines only.
5. **Nothing an agent needs across sessions is derived state.** Keys are paths and versions are
   content hashes, so both survive a move between stores.

## 3. What to do, by kind of change

Before any of them: commit or back up the memory, and run `liquers-agent check` so you start clean.

1. **Liquers changes its metadata format.** Nothing, on mounts with `storage: none` (all mounts of
   both shipped profiles). On a mount that stores metadata, delete its `.__metadata__` sidecars;
   the next read re-derives them from the front-matter.
2. **Assets with recipes.** Recomputing is safe, so drop their stored results:
   `AssetManager::remove` on a key with a recipe and a computed status (`Ready`, `Expired`,
   `Error`, …) rewrites it as `Recipe` and the value is recomputed when next asked for.
   **Do not** remove a key in status `Source` or `Override` that also has a recipe: that deletes a
   value someone wrote, and the key falls back to its recipe.
3. **The schema format changes** (`memory.yaml` `version` goes up). Edit `memory.yaml` as the
   release notes say, then set its `version`. Until you do, the service keeps reading the old one
   if it still supports it, or refuses to start, naming the versions.
4. **The memory's content rules change** (a renamed field, a renamed status, a new required field,
   a vocabulary change). Change `memory.yaml`, run `liquers-agent check`, and fix each violation:
   with `memory_edit` (`field`/`value`) one document at a time, or with a script over
   `liquers-agent query`. Every fix through the service is stamped and logged. Repeat until `check`
   is clean.
5. **`ns-mem` commands, MCP tools or the query guide change.** Nothing to migrate. Agents discover
   tools at start; update the `liquers-memory` skill and any project instructions that name a tool.
6. **Moving to another store** (a folder to S3, one checkout to another). Copy the documents, the
   schema and the log, and point the profile's mount at the new place. Do not copy sidecars.

Finish with `liquers-agent check`; a clean result means the migration is done.

## 4. What would need a real migration

- **Metadata that is not in the front-matter**, such as application attributes stored only in
  the record (`CORE-METADATA-NO-APPLICATION-ATTRIBUTES`). It would break rule 1; if it is ever
  needed, it comes with its own migration step.
- **Rewriting the log**, which rule 4 forbids.
- A `liquers-agent migrate` command that applies §3.3–3.4 automatically (`AGENT-MEMORY-HAS-NO-MIGRATE-COMMAND`). It is NEXT. Without it,
  the release notes of a schema change carry the edit list.
