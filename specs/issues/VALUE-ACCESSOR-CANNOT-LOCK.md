---
id: VALUE-ACCESSOR-CANNOT-LOCK
kind: feature
title: An asset or value accessor cannot be locked against other writers
status: draft
priority: P3
complexity: L
area: [core/assets, core/context]
design:
created: 2026-11-15
github:
---
## Problem

`settable-value-accessor` (Phase 1, 2026-11-15) lets several accessors write one root, such as a
stored `recipes.yaml` or an asset, and resolves conflicts optimistically: a set applies unless the
part it was based on has changed since (its content fingerprint), otherwise it fails as a conflict.
Nothing lets a writer *reserve* a value, or part of it, so that others cannot change it while a
longer edit is in progress. Examples are a user who opens a recipe in a specialised editor, or a
command that rewrites a file in several steps. Locking was explicitly left out of that design
(question 6). The design only reserves a hook so that a lock can be added without changing the
accessor contract.

## Impact

Two editors of one value see each other's changes and get conflicts, but cannot prevent them. In a
single-user desktop application this is rarely a problem. In a shared server (axum, the split UI),
two users can repeatedly invalidate each other's edits, and an automated writer can overwrite a part
a person is editing between two of their sets. The workaround is the conflict itself: re-read and
retry.

## Expected behaviour

A future locking design should choose between, or combine, two mechanisms Orest proposed on
2026-11-15:

1. **Asset-level locks, owned by a user or session.** The asset manager records the holder of a
   lock on a key, and a write through any path (accessor, `set_binary` / `set_state`, the web API) by
   anyone else is refused. This requires a user and session model, which does not exist yet:
   `CORE-SESSION-AND-KEY-ACL` (no session model, no per-key write authorization) is a prerequisite,
   and `Environment::create_session` / `SimpleSession` (`liquers-core/src/context.rs`) currently
   carry only a user that evaluation never sees.
2. **Accessor-level locks (simpler).** While a lock is held, only one specific accessor, typically
   a projection accessor belonging to one user's editor, may write the root, or the locked part of
   it. Other accessors can still read, and they keep receiving `Changed` notifications. This needs
   no session model in a single process. Across processes it needs a way to name the holder, for
   example the accessor's address (its query) plus a lease token.

Points the design must settle:

- **Scope:** whole root, or a projected part. A part lock fits the fingerprint and anchor model
  (a set conflicts only when its own part changed), but writes to a parent that contains the locked
  part must then be refused or checked.
- **Leases and expiry:** a lock must end when its holder disappears (a closed editor, a dropped
  WebSocket, a crashed client). Accessors are held by weak reference in the root's projection tree,
  so a dropped accessor can release its lock. A remote holder needs a timeout and renewal.
- **Interaction with outside writes:** a change made through the store or asset manager without an
  accessor (AC-6 of `settable-value-accessor`) must either respect the lock or be refused.
- **Interaction with asset protection:** `REGISTERED-ASSET-CANNOT-BE-PROTECTED-FROM-UNREGISTRATION`
  is a related guarantee (an asset that cannot be removed while in use). A lock may imply it.
- **Web API and split UI:** a lock taken by a client must be visible to the server-side
  authoritative state (`ui-split-elements`), and a web API write without the lease token must be
  refused (`settable-value-accessor` AC-13).

## Discovery

Raised by Orest while answering question 6 of `specs/design/settable-value-accessor/` Phase 1
(2026-11-15): locking is out of scope there, and this issue holds what is known for the later design.

## Proposal 2026-11-26

`specs/design/ui-redesign/accessor-working-copy.md` §5 proposes accessor-level locks (mechanism 2)
as a lease held by a working copy of an accessor, taken as an option of `begin_edit` and released
on commit, discard or drop. Asset-level locks stay open. Not decided.
