# Phase 1: High-Level Design — resource-name-unicode

## Purpose

A store can hold files whose names a query cannot spell: `café.csv`, `Łódź.csv`, `my report.csv`,
`12:30.log`. They can be listed but not fetched, and no recipe can name them. Let a resource name
be any Unicode name. Characters outside the ASCII name class are written literally (Unicode
alphanumerics) or as the same tilde entities string parameters use. The **decoded** name is what the
key holds and what a store addresses. `Key::encode` stays canonical, pure ASCII and stable for every
key that parses today.

## Problem Example

Verified at HEAD (18732dd) with `liquers-validate --no-registry`: `-R/data/café.csv`,
`-R/data/Łódź.csv`, `-R/data/ŁŁ.csv` (which parsed before `parameter-entity-escaping`'s D10),
`-R/data/my file.csv` and `-R/data/caf~UE9~.csv` all fail with "Can't parse query completely". A file
store over a directory containing `data/café.csv` lists it: `listdir_keys(data)` returns a `Key`
whose `encode()` is `data/café.csv`, text that does not parse. The file can be seen and cannot be
fetched.

What should happen: `-R/data/café.csv` and `-R/data/caf~UE9~.csv` both parse to the key
`["data", "café.csv"]`. `Key::encode` writes `data/caf~UE9~.csv`, and the file store reads the file
literally named `café.csv`. The listed key's `encode()` parses back to the same key.

## Scope and Acceptance Criteria

- **AC-1** Literal Unicode names parse
  WHEN `-R/data/café.csv` or `-R/data/Łódź.csv` is parsed
  THEN the key's last name is `café.csv` / `Łódź.csv`
- **AC-2** Entities in names parse, and the encoding is canonical
  WHEN `-R/data/caf~UE9~.csv`, `-R/data/my~.file.csv`, `-R/data/~_x.csv` are parsed
  THEN the names are `café.csv`, `my file.csv`, `-x`, and `Key::encode` of each is pure ASCII, re-parses to an equal key, and is the same text whichever spelling was parsed
- **AC-3** ASCII keys are unchanged
  WHEN any key that parses at HEAD is parsed and encoded
  THEN its encoding is byte-identical to HEAD's (`report-2024.csv`, `a.b-c`, `.hidden`)
- **AC-4** A decoded name never contains a separator or a control character
  WHEN a name spells `/` (`a~/b`, `a~U2F~b`, `~H…`) or a C0 control or DEL (`~U0~`, `~U7F~`)
  THEN parsing fails at that entity with a message naming the character, and `ResourceName::try_new` / `Key::try_join` refuse it
- **AC-5** `.` and `..` keep their meaning however spelled
  WHEN a name decodes to `.` or `..` (`~U2E~~U2E~`)
  THEN the key is relative, exactly as for `..`
- **AC-6** Stores address the decoded name
  WHEN a key with name `café.csv` or `my file.csv` is written and read through `AsyncMemoryStore`, `AsyncFileStore`, `AsyncOpenDALStore` (fs service) and the conformance suite
  THEN the physical object is named `café.csv` / `my file.csv`, and a listing returns a key equal to the one written
- **AC-7** A backend refuses what it cannot represent
  WHEN a key's name is not exactly one normal path component for `AsyncFileStore` on the running platform (for example `a\b` on Windows, or a `/` smuggled in through the unchecked `Key::join`)
  THEN the store returns `KeyNotSupported`, before any filesystem call
- **AC-8** Builders and templates follow
  WHEN `QueryBuilder::resource` or a `liquers-template` key-segment placeholder receives `café.csv`
  THEN it is accepted and the query encodes as in AC-2

Non-goals: Unicode normalization (NFC `café` and NFD `café` are different keys; a macOS listing
returns NFD and is addressable as listed); Windows reserved names, trailing dots and
case-insensitive collisions beyond AC-7's component check; per-backend renaming (contract 3);
non-UTF-8 file names (listed lossily today, unchanged); action names, filenames of transform
segments and header values (stay ASCII; `QUERY-TOKEN-CLASSES-TRUNCATE-NON-ASCII`).

## Core Interactions

- Query: `resource_name` gains literal Unicode alphanumerics and the entity production;
  `ResourceName::encode` escapes; new `Key::path()` (decoded) and checked constructors.
- Store: every backend's physical name comes from `Key::path()` instead of `encode()`.
- Template, query builder, axum (URL path keys), web stores: follow through the shared predicates.

## Crate Placement

`liquers-core` (`escape.rs`, `parse.rs`, `query.rs`, `store.rs`), `liquers-store` (OpenDAL),
`liquers-web` (three stores), `liquers-lib` (`mem` layout), `liquers-template` (predicate only).

## Documentation Intent

- Reference: `DOC_02` §Lexical forms and a new §Resource-name entities; `PROJECT_OVERVIEW.md` (Key
  encoding, per `CLAUDE.md`); `STORE_SEMANTICS.md` §7 (physical name is the decoded name; the
  backend refusal rule; normalization note).
- Guide: `QUERY_ESCAPING_GUIDE.md` (the "resource name with a non-ASCII character" trap becomes a
  how-to); `STORE_IMPLEMENTATION_GUIDE.md` (use `Key::path()` for physical names);
  `TEMPLATE_LANGUAGE.md` P4.
- Documents to update: the six in `affects_docs`.

## Design Readiness

- **Readiness:** needs-decision
- **Automatic fixing:** not-eligible — changes query syntax and key encoding, adds `pub` items and
  touches four crates (rules 3, 4, 6).
- **Leading issue:** **Open design question — Question 1 and Question 2 of the issue.**
  Recommended: **option C (full Unicode) with contract 2 (the decoded value is the name)**. C
  because B alone leaves the issue's main case (`café.csv`) unaddressed and a Latin-1 middle is
  rejected by the issue itself. Contract 2 because it is the only one that reaches files created
  outside Liquers, the motivation of C. Contract 1 (the encoded text is the physical name) is safe
  and needs no store change, but `café.csv` stays unaddressable and stores fill with `caf~UE9~.csv`.
  Contract 3 (per-backend mapping) gives one key several physical meanings. Option A (status quo)
  means closing the issue `closed_not_planned`. The issue's preconditions for contract 2 are met:
  `STORE-FILESTORE-PATH-TRAVERSAL` is fixed (`store-key-guard`), and the remaining injection, a
  decoded `/`, is refused at parse time (AC-4) and by each backend (AC-7).
- **Explanation:** under the recommended answers every open point of the issue's "What any answer
  must also settle" has a concrete resolution (below), and the change sites were opened at HEAD.
- **Open questions:**
  1. **Open design question — the key-level forbidden set.** Recommended: `/`, C0 controls and DEL
     only. Everything else (`\`, `:`, `*`, reserved device names) is a backend refusal, because S3
     and `localStorage` accept them.
  2. **Proposed resolution — canonical spelling.** Literal: `[A-Za-z0-9_.]` and a non-leading `-`
     (so every HEAD key is unchanged, AC-3). `~~` for `~`, `~.` for space, `~_` for a leading `-`.
     Then the curated named entity, then `~U<hex>~`. No URL mnemonics and no `~<digits>` form are
     emitted in names. The decoder accepts every entity form, as for parameters.
  3. **Proposed resolution — normalization.** None; documented.
  4. **Proposed resolution — fallible constructors.** Add `ResourceName::try_new` and
     `Key::try_join`; keep `new` / `join` infallible and documented as unchecked (as DOC-02 already
     says), since a backend refuses an unrepresentable name anyway (AC-7).
  5. **Proposed resolution — `ResourceName::encode` returns `Cow<'_, str>`** instead of `&str`,
     borrowed when nothing needs escaping, so the hot path does not allocate.

## Design Dependencies

- `parameter-entity-escaping` (complete, E4): **origin** (D10). The entity table, `match_entity` and
  the curated names are reused unchanged.
- `store-key-guard` (complete): **requires** — satisfied (`..` refused by every store).
- `store-backend-absolute-key` (this run): **overlaps** (T3: each backend's key-to-path mapping).
  Recommended order: that design first. The decoded-name check (AC-4) then also lives in
  `AbsoluteKey::new` and reaches every backend at once. Merging would exceed `L` (E2).
- `query-builder` (this run): **overlaps** (T2: what a valid key segment is). Both use one
  value-level predicate, `escape::is_valid_resource_name`; whichever lands second widens or reuses
  it.
- `styled-query-entity-tokens` (this run): **overlaps** — `ResourceName::styled_tokens` should split
  entities the same way; done here (Phase 2) with that design's helper if it has landed.
- `QUERY-TOKEN-CLASSES-TRUNCATE-NON-ASCII` (issue): the same truncation in other productions; no
  ordering constraint.

## Consolidated Findings

- Two string forms of a key, each with one job: `Key::encode()` (Display) is canonical ASCII
  query text, used for identity, messages and URLs. The new `Key::path()` is the decoded `/`-joined
  name, the only thing a store may use for a physical name. For every key that parses at HEAD the
  two are equal, so no stored data, cache key or asset identity changes (AC-3).
- The risky work is the audit of ~150 `encode()` / `to_string()` call sites on `Key` and
  `ResourceName`: stores and metadata filenames switch to the decoded form, everything else stays.
  A wrongly classified site affects only non-ASCII names. The conformance suite gains a Unicode
  fixture key so stores catch it (AC-6).
- `resource_name` keeps an ASCII fast path; the entity branch runs only at `~`. The parser hot path
  is not slowed for existing queries (`parse.rs` is performance-sensitive).
- `Key` has a derived serde form holding decoded names, so serialization needs no change. There is
  no wire compatibility constraint.
- The decision (C + contract 2) is the leading open item. Under A the issue closes and none of this
  is built.
