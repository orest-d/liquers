---
id: UI-REDESIGN-ACCESSOR-WORKING-COPY
kind: analysis
title: UI redesign — a working copy of an accessor (drafts, live preview, synchronous editing, locks)
workflow: liquers-project
status: draft
area: [core/value, lib/ui, web, axum]
created: 2026-11-26
---
# A working copy of an accessor

A proposal written 2026-11-26 after a discussion with Orest. It is not a design yet: nothing is
implemented, no phase is opened, and §11 lists what Orest has to decide. It builds on
[`reference/VALUE_ACCESSORS.md`](../../reference/VALUE_ACCESSORS.md) and belongs to the UI
redesign ([master design](./master-design.md) §10.4 and D12, forms and bindings).

## 1. Where it comes from

- An accessor either writes a value or does not. There is no place for a value that is being
  edited and is not written yet.
- Live preview (`specs/design/query-update-modes/`) therefore keeps the pending copy inside each
  editor and sends a preview to the one view the editor follows. Two known results:
  `PREVIEW-IS-FRAMED-ONLY-IN-THE-QUERY-CONSOLE` (other views show a preview as if it were
  applied, and keep it after a Revert) and `EDITOR-HAS-NO-KEY-TO-DISCARD-A-PREVIEW`.
- Orest (2026-11-26) proposed that an accessor could also hold a preview value, updated along the
  chain of accessors, to support elements with a live preview.
- Claude's answer: the draft belongs with the accessor, but as a separate overlay accessor, not
  as a second value inside every accessor. A hub is shared by every holder of a root, so a
  preview inside it would be seen by every user of a server; it would widen the contract of
  every root and every binding; and `get` would become ambiguous.
- Orest agreed with a working copy on a root accessor, and added: a working copy that lives in
  one domain, for example in the frontend, could have synchronous updates.
- Locking (`VALUE-ACCESSOR-CANNOT-LOCK`) is about the same thing, a session that owns an edit,
  so it is treated here too.

## 2. The concept

A **working copy** is a private, editable copy of the value of an accessor, with the way back.

```
base accessor ──begin_edit──▶ working copy ──commit──▶ base accessor (one set_if)
                                   │
                                   └─discard─▶ nothing is written
```

- It holds the value it started from, the fingerprint of that value (the **base fingerprint**),
  and the **draft**, which starts equal to the value.
- It is itself an accessor. Its root reads the draft and writes the draft. Projections of it
  (`recipe-0`, `query`, `argument-n`) are ordinary projections, so an element that edits a part
  binds to a projection of the working copy exactly as it binds to any accessor today.
- Whoever holds the working copy sees the draft. Everyone else holds the base accessor and sees
  the committed value. Nothing is shared by accident.
- `commit` is one `set_if(draft, base fingerprint)` on the base accessor. Ten edits are one
  write, one new version and one expiry of the dependents.
- `discard` drops the draft. The holders of the working copy get `Changed` and show the
  committed value again.

It may start from a root or from a part. A working copy of a part (one recipe, one cell) holds
only that part, and its commit is conditional on the part's own fingerprint, so two working
copies of disjoint parts of one root do not conflict. That is the existing rule of the hub.

## 3. Synchronous in the middle, asynchronous at the edges

What is asynchronous in an accessor today is the root (it does I/O) and the lock held across a
set. Projection steps are synchronous and pure. A working copy has no I/O between its first read
and its commit, so:

| Operation | Kind | Why |
|---|---|---|
| `begin_edit` | asynchronous | reads the base once |
| read the draft, or a part of it | **synchronous** | memory and steps |
| write the draft, or a part of it | **synchronous** | memory and steps |
| has it changed since I drew it? | **synchronous** | a version counter |
| `discard` | **synchronous** | memory |
| `commit` | asynchronous | writes the base |
| the base changed underneath | arrives asynchronously | an event of the base; see §4 |
| publish the draft to another domain | asynchronous | see §6 |

This is not a special class of working copy. It holds for every working copy whose editors are
in the same domain (the same process, or the same browser page) as the copy.

The synchronous methods exist on the working copy type only. The same type implements the
ordinary `ValueAccessor`, whose asynchronous methods complete at once, so there is one accessor
contract and generic elements need no second code path.

```rust
/// A sketch, not a signature to implement.
pub struct WorkingCopy<V> { /* base accessor, base fingerprint, draft, version, lease */ }

impl<V: ValueInterface> WorkingCopy<V> {
    pub async fn begin(base: Arc<dyn ValueAccessor<V>>, options: EditOptions) -> Result<Arc<Self>, Error>;

    // Synchronous: no await, usable inside an egui frame and on wasm.
    pub fn draft(&self) -> State<V>;
    pub fn set_draft(&self, value: V) -> Result<(), Error>;
    pub fn part(&self, steps: &[Arc<dyn ProjectionStep<V>>]) -> Result<Option<V>, Error>;
    pub fn set_part(&self, steps: &[Arc<dyn ProjectionStep<V>>], part: V) -> Result<(), Error>;
    pub fn version(&self) -> u64;           // increases with every change of the draft
    pub fn is_modified(&self) -> bool;      // the draft differs from the base value
    pub fn status(&self) -> EditStatus;     // Clean, Modified, Stale, Conflict
    pub fn discard(&self);

    // Asynchronous: they reach the base.
    pub async fn commit(&self) -> Result<AccessorSnapshot<V>, Error>;
    pub async fn rebase(&self, policy: RebasePolicy) -> Result<EditStatus, Error>;
}

// And: impl<V> ValueAccessor<V> for WorkingCopy<V>, reading and writing the draft.
```

What the synchronous part gives:

- **Immediate-mode code.** An egui view reads and writes the draft inside a frame, with no
  message and no await. This is the native idiom the master design wants (§4.1, R15).
- **wasm.** Nothing ever has to block or be polled.
- **A thick client.** A keystroke costs no round trip. Only `begin_edit` and `commit` reach the
  server.
- **Undo.** The draft has a history of its own, kept by the working copy, the same for every
  editor of it. Today each editor keeps its own.

## 4. When the base changes during an edit

The base accessor reports `Changed`. The working copy compares the new fingerprint with its base
fingerprint:

| Situation | Status | What happens |
|---|---|---|
| The draft was not modified | `Clean` | the working copy takes the new value; its holders get `Changed` |
| Modified, and the change is outside the edited part | `Modified` | nothing; part fingerprints already make this a non-conflict |
| Modified, and the base value of the same part changed | `Stale` | the holders are told; `commit` would fail with a conflict |
| The base was removed or the projection invalidated | `Conflict` | the draft is kept so the user can copy it; `commit` is refused |

`rebase` resolves `Stale` by a policy the holder chooses: keep mine (take the new fingerprint,
keep the draft), take theirs (discard), or, for a value with projections, re-apply the parts the
draft changed onto the new value. The third one is possible because a working copy knows which
parts were written through it. No automatic merge of text is proposed.

## 5. Ownership and locks

A working copy has one owner: the session that began it. This is what makes it safe on a shared
server, and it is where locking fits.

- **Optimistic, the default.** No lock. A conflict is found at `commit`, as today, or earlier
  through `Stale`. This needs no session model and works in a single process now.
- **With a lease.** `begin_edit` may ask for a lock on the edited part. While the lease is held,
  a set of that part through any other accessor is refused with a conflict that names the
  holder. This is mechanism 2 of `VALUE-ACCESSOR-CANNOT-LOCK` (accessor-level locks), with the
  working copy as the holder: the lease is released on `commit`, on `discard`, and when the
  working copy is dropped, because the hub holds its nodes by weak reference. A remote holder
  renews the lease and loses it after a timeout.
- **Asset-level locks** (mechanism 1 of that issue), which also stop writes that do not go
  through an accessor, need the session and access model of `CORE-SESSION-AND-KEY-ACL` and are
  not proposed here. A lease taken by a working copy is the same object from the user's side, so
  it can be backed by an asset-level lock later without changing the editors.

So a lock is not a separate feature of the accessor contract. It is an option of `begin_edit`,
and `SetOptions`, reserved for locking, carries the lease on the commit.

## 6. Domains and live preview

The working copy lives where its editors live.

| Shape | The working copy is | Editors use | Reaches the server |
|---|---|---|---|
| Monolithic (egui, terminal, `liquers-web` with a real environment) | in the process | the synchronous API | no server |
| Thick client, value readable by the client | in the client | the synchronous API | `begin_edit`, `commit`, and the published draft |
| Thick client, value too large or not readable | in the client, as a working copy of a **part** | the synchronous API | the same |
| Thin client | on the server | server parts use the synchronous API; the client sends input events | everything, as today |

A working copy says where the draft lives and who sees it. It does not compute anything. The
preview a user usually wants is not the draft itself but the **result** of the edited recipe,
and that is an evaluation, downstream of the value, not along the chain of accessors. So:

- A working copy can **publish** its draft: a coalesced stream of the latest draft (a
  `watch`-style channel, so a burst of edits is one value) to a subscriber, which may be in
  another domain. Publishing is asynchronous and optional.
- The preview of a result is then what `query-update-modes` already does, with one change of
  ownership: the view evaluates the published draft as an ad hoc recipe, never stored, and marks
  the value as a preview. It follows the working copy, not one particular editor. When the
  working copy is committed or discarded, the stream ends and every view that followed it goes
  back to its own query. That closes `PREVIEW-IS-FRAMED-ONLY-IN-THE-QUERY-CONSOLE` by
  construction.
- What depends on that result further down is not previewed. A "what if" evaluation of a whole
  dependency tree is a different and much larger feature.

The three update modes of the editors become three ways to use one working copy:

| Mode | Working copy | Publish | Commit |
|---|---|---|---|
| On demand | yes | no | on Update |
| Live preview | yes | yes | on Update |
| Apply immediately | yes | no | after every accepted edit |

## 7. What it replaces and what it leaves alone

- **Unchanged:** `ValueAccessor`, `AccessorRoot`, `ProjectionStep`, the hub, fingerprints, the
  web routes for reading and writing, the Python and JavaScript wrappers of an accessor.
- **New:** the working copy type and its root; `begin_edit` as a command of namespace `acc`, so a
  working copy has an address and can be named in a UI spec; an edit status and its events;
  wrappers in Python and JavaScript (`begin_edit`, the synchronous draft methods, `commit`,
  `discard`); a WebSocket message pair for the published draft.
- **Replaced in the UI:** the pending copy, the undo stack and the preview effect inside each
  editor (`UpdateMode` handling in `ui/editor/`), and `AppMessage::Preview` aimed at one view.
- **Frontend-owned drafts stay.** The text in a field that has focus is still owned by the
  frontend (master design §10.4). The working copy is the next level: what the field hands over
  when an edit is accepted.

## 8. Issues it touches

| Issue | Effect |
|---|---|
| `QUERY-EDITOR-HAS-NO-LIVE-PREVIEW` | the mechanism becomes shared by every editor and form |
| `PREVIEW-IS-FRAMED-ONLY-IN-THE-QUERY-CONSOLE` | closed by views following the working copy |
| `EDITOR-HAS-NO-KEY-TO-DISCARD-A-PREVIEW` | `discard` is one operation of the working copy |
| `VALUE-ACCESSOR-CANNOT-LOCK` | accessor-level locks as a lease of a working copy (§5); asset-level locks stay open |
| `TEXT-EDITOR-EDITS-ARE-NEVER-SAVED` | a text editor edits a working copy and commits it |
| `ACCESSOR-CANNOT-BE-SENT-TO-A-CLIENT` | a client-side working copy needs the base accessor's address and value in the client; it depends on this issue for the thick client |
| `ACCESSOR-CANNOT-BE-IMPLEMENTED-IN-PYTHON-OR-JAVASCRIPT` | independent |

## 9. Risks

1. **Two copies of a large value.** A working copy of a DataFrame doubles it unless the value
   shares structure (record batches do, per column). Working copies of parts are the answer for
   large roots, and a size limit on `begin_edit` of a root may be needed.
2. **A draft that is lost.** A working copy lives in memory. A closed tab or a crashed process
   loses the draft. Saving drafts (to the session's saved state, master design §20) is possible
   because a draft is a value, and is not proposed for the first version.
3. **Leases that are never released.** A remote holder that disappears keeps a lock until the
   timeout. The timeout and the renewal interval need care, and an administrator needs a way to
   break a lease.
4. **Synchronous steps that are slow.** A step that parses a large YAML document on every `get`
   runs in the UI frame. The hub caches the root value, and the working copy should cache the
   parsed parts along a projection chain in the same way.
5. **Editors of overlapping parts.** A slider on an argument and a text editor of the whole
   query edit one draft. That is the point, but a text editor with a half-typed, invalid query
   must not put it into the draft. The rule of today holds: only an accepted edit is written.

## 10. Collaborative editing (added 2026-11-26)

Orest (2026-11-26): approach this carefully. Ideally collaborative editing becomes possible.
Then there may be a shared accessor across several sessions that coordinates the edits, and it
may branch into working copies. The design has to enable that.

It does, if four things are decided now. None of them makes the single-user case harder.

### 10.1 The shape: working copies nest

A working copy is an accessor, so a working copy can start from another working copy. That gives
three levels, and the middle one is the shared accessor Orest describes:

```
base accessor          the committed value (store, asset)
   ▲ commit = save
shared working copy    one per edited value, on the server; several sessions take part;
   ▲ commit = an edit  it puts the edits in one order
local working copy     one per participant, in its own domain; synchronous (§3)
```

- A **local** working copy is what §2 to §4 describe. Its base is the shared working copy, and
  it commits there after every accepted edit, not on Save.
- The **shared** working copy is the coordinator. It is the only place where edits meet, so it
  gives them one order, and nothing more exotic than that is needed for structured values. It is
  close to what exists: a hub over a memory root, seeded from the base, already serializes its
  writers, derives every projection again after a set, applies first writer wins per part, and
  notifies its holders. What it adds is `commit` to the base and a list of participants.
- **Save** is the commit of the shared working copy to the base. Any participant with the right
  to write may ask for it, or it happens automatically.
- The single-user case is the same shape with one participant, and the shared level may be left
  out (a local working copy directly on the base, as in §2).

### 10.2 What must be decided now

1. **A change is a patch, not a new whole value.** A working copy records what was written
   through it: the path of projection steps, the fingerprint the part had, and the new part.
   Commit sends the patches. With whole values, two people who edit different arguments of one
   query overwrite each other; with patches the shared copy applies both. This is also what
   `rebase` of §4 needs, so it is not extra work.
2. **A change log with sequence numbers.** Accessor events are wake-ups ("something changed,
   read again"), and that is not enough here: a participant must apply the others' changes
   without losing its own draft, its cursor or its selection. The shared working copy keeps its
   patches in order and answers "the patches since number n", the way `log(since)` works. A
   local copy applies them and rebases its uncommitted patches on top.
3. **Participants, not an owner.** A working copy has an address and a set of participants.
   §5's single owner is the case of one. Who may join, and who may save, is a permission and
   belongs to the session model.
4. **A merge rule per kind of step.** The default is the rule of the hub: patches of disjoint
   parts both apply, two patches of the same part conflict and the later one is refused and
   reported as `Stale`. A step kind may bring a better rule. The one that matters is text, see
   10.3.

### 10.3 What it does not give by itself

- **Two people typing in one text.** With the default rule, a text is one part, so the second
  writer is refused. Real concurrent text editing needs a text operation (insert and delete at a
  position) and a rule to transform it against a concurrent one, or a text type that merges by
  itself (a CRDT). Point 4 leaves the place for it: a text step whose patches are operations.
  With a server that orders the edits, the simpler of the known techniques is enough. I would
  not build it in the first version, but I would test the patch and log contract against it
  before fixing that contract.
- **Presence.** Who is editing which part, cursors and selections of the others. It is
  per-participant data attached to the shared working copy and sent with the change stream. A
  lease (§5) is then the strong form of presence: the same record, with writes refused.
- **Undo.** In a shared draft, undo must take back one's own patches and leave the others'.
  With a patch log this is "apply the inverse of my last patch if the part is unchanged since",
  which is possible for structured parts and hard for text.
- **Working offline.** A local copy that reconnects after a long time is a large rebase. Not
  proposed.

### 10.4 What changes in the rest of this proposal

- §2: a working copy holds its uncommitted patches, not only a draft value.
- §4: `rebase` is the normal path, run on every change from the shared copy, not an exception.
- §5: ownership becomes participation; a lease is a record of the shared working copy.
- §6: the published draft and the change log are one stream. A view that previews the result
  follows the shared working copy, so all participants see the same preview.
- §9: one more risk. The patch and log contract is the hard part to change later, because the
  clients and both language bindings will depend on it.

## 11. Questions for Orest

1. Is the three-level shape of §10.1 (base, shared working copy on the server, local working
   copies) what you meant by a shared accessor that branches into working copies?
2. Locks: start with optimistic only, and add the lease later, or both at once?
3. Should a draft survive a restart (risk 2), at least in the monolithic desktop case?
4. Is this a design of its own (it changes `liquers-core`'s accessor module and both bindings),
   to be done before D12, or a part of D12?
5. Concurrent typing in one text (§10.3): a goal of the first version, or only something the
   contract must not rule out?
