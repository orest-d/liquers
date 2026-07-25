# Phase 2: Solution & Architecture - webui-fixes

*Scope: W3 (rendering follows the model) + W4 (close a stale issue record). The interaction half
lives in `specs/ui-events/`. Earlier drafts of this file (the old wider scope; the dirty-handle-set
version) are in git history — commits `89875c7` and `aa5a857`.*

## Overview

Invalidation becomes a property of the **model**: `AppState` records *what changed*, and the
renderer applies it. The record is an ordered list of structural changes — element inserted,
removed, or its markup replaced — not a set of dirty handles, because the mutation sites already
know exactly what they did and a set throws that information away. The browser driver maps an
insert to a real DOM insert and a removal to a node removal, so adding a panel leaves its siblings'
DOM nodes untouched; only a widget whose markup depends on its child set falls back to re-rendering
the parent. `Invalidation::All` remains the safety net for anything not attributable to elements.

This is **recording**, not diffing: the delta is captured where the mutation happens, exactly, with
no tree comparison and no reconciliation heuristics. Phase 1's "no diff/patch" decision stands.

## Data Structures

### New enum: `UIChange`

```rust
// liquers-lib/src/ui/app_state.rs

/// One recorded mutation of the UI tree. Backend-neutral: the web backend maps these to DOM
/// operations, immediate-mode backends ignore the detail and just repaint.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum UIChange {
    /// `handle` was added under `parent` (`None` = a root) at child index `index`.
    Inserted { parent: Option<UIHandle>, handle: UIHandle, index: usize },
    /// `handle` (and its subtree) was removed from `parent` (`None` = a root).
    Removed { parent: Option<UIHandle>, handle: UIHandle },
    /// `handle`'s own rendered markup is out of date; its position in the tree did not change.
    Replaced { handle: UIHandle },
}
```

Named `UIChange` to match `UIHandle`/`UIElement` (`UiAction` uses the other casing — a pre-existing
inconsistency, not resolved here).

### New enum: `Invalidation`

```rust
/// What has changed in the model since a renderer last looked. Three states, matching the three
/// things a renderer can do.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub enum Invalidation {
    /// Nothing changed — the rendered output is still current.
    #[default]
    None,
    /// Apply these changes, in order. Never empty: the empty case is `None`.
    Changes(Vec<UIChange>),
    /// Re-render everything. Used when a change cannot be attributed to elements — the state was
    /// deserialized, nothing has been rendered yet, the change log overflowed, or the `AppState`
    /// implementation does not track changes at all.
    All,
}
```

Accumulation is an **absorbing state machine**, which is what keeps the three states unambiguous:

| Current | `record(change)` | `set_all()` |
|---|---|---|
| `None` | `Changes([change])` | `All` |
| `Changes(v)` | `v.push(change)`, or `All` if `v.len()` would exceed `MAX_CHANGES` | `All` |
| `All` | `All` (ignored — already covered) | `All` |

`take()` returns the value and resets to `None`.

`MAX_CHANGES` (64, tuned later) bounds memory when no renderer drains the log for a while — for
example while the `AppState` lock is held across several browser ticks. Escalating to `All` is both
cheaper to apply and safer than an unbounded `Vec`.

*(Two earlier shapes were rejected in review. A struct with `all: bool` beside a handle set admits
`all = true` with a non-empty set, a state with no defined meaning. A set of dirty handles — the
previous draft — is correct but lossy: "the parent changed" is all it can say about an insert, so
the renderer must regenerate the parent's entire subtree.)*

### Modified struct: `DirectAppState`

One new field:

```rust
/// Transient render bookkeeping; never serialized (see Serialization Strategy).
invalidation: Invalidation,
```

`DirectAppState::new()` starts at `Invalidation::All`, so a renderer attaching to a freshly-built
state always paints once without a special "first frame" flag.

`UIHandle` gains `PartialOrd, Ord` derives — additive, no call-site changes — so handles can be
ordered when a change list is normalised or asserted on in tests. Ordering by id is meaningful
(ids are allocated in creation order).

### No `ExtValue` variants, no new value types.

## Trait Implementations

Three methods added to `AppState`, **with defaults that are conservative rather than wrong**:

```rust
/// Record one structural change. Default: escalates to `invalidate_all` — correct, just coarser.
fn record_change(&mut self, change: UIChange) {
    let _ = change;
    self.invalidate_all();
}

/// Record that the whole tree must be re-rendered.
/// Default: no-op, paired with the `take_invalidation` default below.
fn invalidate_all(&mut self) {}

/// Take and clear the pending invalidation. Exactly one renderer per application may call this.
/// Default: `Invalidation::All` — an implementation that does not track changes tells every
/// renderer to re-render everything, which is what the web backend does today. Never stale.
fn take_invalidation(&mut self) -> Invalidation {
    Invalidation::All
}
```

This follows the project's "extend traits with defaults" rule without the usual hazard: an
implementor that ignores invalidation degrades to a full re-render (today's behaviour) rather than
silently going stale. `DirectAppState` — the only implementor in the workspace — overrides all
three with real tracking.

**Where changes are recorded** (all inside `DirectAppState`, under the same `&mut self` as the
mutation):

| Method | Records | Notes |
|---|---|---|
| `add_node(parent, position, …)` | `Inserted { parent, handle, index }` | `parent: None` means a root; the index is the resolved insertion position |
| `insert_node(handle, parent, position, …)` | `Inserted { … }` | same |
| `set_element(h, _)` | `Replaced { handle: h }` | the element's markup changed |
| `set_source(h, _)` | `Replaced { handle: h }` | a pending node renders a placeholder |
| `remove(h)` | `Removed { parent, handle: h }` | one record for the subtree root; descendants go with it |
| `set_active_handle(old, new)` | `Replaced` for each of `old`, `new` | active state is renderable |
| `take_element` / `put_element` | **nothing** | see below |

**`take_element`/`put_element` deliberately record nothing.** They are the extract-render-replace
pair the *egui renderer itself* uses on every frame; recording there would mark the whole tree
changed every frame and turn "repaint when something changed" back into "repaint always". The rule
that replaces it:

> Structural changes record themselves. A change to an element's *content* is reported by whoever
> made it — from `update`, via the returned `UpdateResponse::NeedsRepaint`.

`AppRunner::deliver_snapshot` already computes that response and currently discards it; it becomes
the source of `Replaced` records for element-content changes.

### The container opt-in

A structural DOM insert is only valid if the parent's markup is **position-invariant with respect
to its children** — a plain ordered list inside one container node. That does not hold in general:
a tab layout renders a tab bar derived from the child set, a grid interleaves row breaks, a header
might count children. For those, adding a child changes the *parent's own* markup.

So the container declares itself, in the markup it already renders:

```html
<div id="ui-element-3" class="lq-element lq-UISpecElement">
  <div class="lq-menubar">…</div>
  <div class="lq-layout lq-layout-vertical" data-lq-children="3">…children…</div>
</div>
```

`data-lq-children="{own handle}"` means *"my children are rendered here, in order, one node each,
and my own markup does not depend on which children exist."* The attribute value carries the handle
so the lookup (`[data-lq-children="3"]`) cannot accidentally match a descendant element's container.

The backend needs no advance knowledge: it looks for the marker when applying an `Inserted`, and
falls back to `Replaced { parent }` when it is absent. The judgement stays where the knowledge is —
only the widget knows whether its markup depends on the child set — and it is a rendering concern,
so it lives next to `render_web`. egui needs nothing.

Today's `UISpecElement::render_web` already emits children as a plain concatenation inside one
`div.lq-layout` for every layout variant (the web renderer sets a layout class; it does not yet
render tab bars), so it can declare the marker as written. The opt-in is what keeps this correct
when someone later renders a real tab bar.

## Generic Parameters & Bounds

None added. `Invalidation` and `UIChange` are concrete and non-generic — they cross the
`dyn AppState` boundary, so they must be. `AppState` stays object-safe: the new methods take and
return concrete types, no generics, no `Self` by value.

## Sync vs Async Decisions

| Function | Choice | Rationale |
|---|---|---|
| `AppState::record_change` / `invalidate_all` / `take_invalidation` | sync | in-memory bookkeeping under an existing `&mut self`; `AppState` is sync by design |
| `AppRunner::run` | async (unchanged) | already awaits evaluation |
| browser render step | sync | DOM manipulation; runs between runner awaits |
| focus capture/restore | sync | DOM reads/writes |

No I/O is introduced, so nothing new becomes async.

## Function Signatures

```rust
// liquers-lib/src/ui/app_state.rs
impl Invalidation {
    /// Append a change. `None` → `Changes([c])`; `Changes` grows (escalating to `All` past
    /// `MAX_CHANGES`); `All` absorbs.
    pub fn record(&mut self, change: UIChange);
    /// Escalate to `All` from any state.
    pub fn set_all(&mut self);
    /// True only for `Invalidation::None`.
    pub fn is_empty(&self) -> bool;
    /// Return the value and reset to `None`.
    pub fn take(&mut self) -> Invalidation;
}

pub trait AppState {
    // … existing …
    fn record_change(&mut self, change: UIChange);       // default: invalidate_all()
    fn invalidate_all(&mut self);                        // default: no-op
    fn take_invalidation(&mut self) -> Invalidation;     // default: Invalidation::All
}

// liquers-lib/src/ui/runner.rs
/// Delivery result: whether the element still exists, and what it said about repainting.
enum DeliveryOutcome {
    Missing,
    Delivered(UpdateResponse),
}

async fn deliver_snapshot(
    handle: UIHandle,
    snapshot: AssetSnapshot,
    app_state: &Arc<tokio::sync::Mutex<dyn AppState>>,
    sender: &AppMessageSender,
) -> DeliveryOutcome;

// liquers-lib/src/ui/web/app.rs (browser module, wasm only)

/// Apply an invalidation to the DOM. Returns false if it could not be applied incrementally and
/// the caller must fall back to a whole-tree render.
fn apply_invalidation(root: &web_sys::Element, inv: &Invalidation, state: &dyn AppState) -> bool;

/// Apply one change. Returns false to request the whole-tree fallback.
fn apply_change(root: &web_sys::Element, change: &UIChange, state: &dyn AppState) -> bool;

/// The node children of `parent` are rendered into: `[data-lq-children="{parent}"]`, or `root`
/// for `parent: None`. `None` when the parent did not declare a container.
fn children_container(root: &web_sys::Element, parent: Option<UIHandle>) -> Option<web_sys::Element>;

/// Focused element id and text selection, captured before any markup is replaced.
struct FocusSnapshot { element_id: String, selection: Option<(u32, u32)> }
fn capture_focus(doc: &web_sys::Document) -> Option<FocusSnapshot>;
fn restore_focus(doc: &web_sys::Document, snapshot: &FocusSnapshot);
```

### Applying an invalidation

Matched exhaustively over the three variants:

1. `Invalidation::None` → nothing to do.
2. `Invalidation::All` → capture focus, re-render all roots into `root`, restore focus.
3. `Invalidation::Changes(list)` → capture focus, apply each change **in order**, restore focus;
   if any change reports failure, abandon the incremental path and do the `All` render instead.

Each change re-reads the current model rather than trusting the record — that is what makes a stale
entry harmless and removes any need to normalise or coalesce the log:

- **`Inserted { parent, handle, index }`** — if `handle` no longer exists in the model, skip (it was
  added and removed within the same batch). Otherwise find the container: `root` for `parent: None`,
  else `[data-lq-children="{parent}"]`. No container → apply `Replaced { parent }` instead (the
  documented fallback for widgets whose markup depends on their child set); parent's node missing
  entirely → request the whole-tree fallback. With a container, render the child and insert it
  before the element child currently at `index`, appending if the container holds fewer nodes.
- **`Removed { parent, handle }`** — remove `#ui-element-{handle}` if present; otherwise skip.
  Nothing else is touched: the parent asserted, by declaring a container, that its markup does not
  depend on the child set. If the parent declared no container, this degrades to
  `Replaced { parent }` for the same reason as above.
- **`Replaced { handle }`** — if the handle is gone from the model, skip. If `#ui-element-{handle}`
  is present, replace its markup with `render_element_web(handle, state)`. If the node is absent,
  request the whole-tree fallback (the element has never been rendered).

Replacing an ancestor regenerates its descendants' markup, so a later `Replaced` for a descendant
in the same batch is redundant but harmless — the node exists with the same id and is simply
re-rendered. Skipping changes whose ancestor was replaced earlier in the batch is a safe
optimization, deliberately left out of the first implementation.

No diff, no patch: the record says what happened, and the renderer performs the corresponding DOM
operation.

## Integration Points

| File | Change |
|---|---|
| `liquers-lib/src/ui/handle.rs` | `PartialOrd, Ord` derives on `UIHandle` |
| `liquers-lib/src/ui/app_state.rs` | `UIChange`, `Invalidation`; three trait methods with defaults; `DirectAppState` field + recording in the mutating methods; serde handling |
| `liquers-lib/src/ui/runner.rs` | `DeliveryOutcome`; record `Replaced` when an element reports `NeedsRepaint`; drop the stray `println!` in `process_messages` |
| `liquers-lib/src/ui/web/app.rs` | `apply_invalidation`, `apply_change`, `children_container`, focus capture/restore; the loop consumes `take_invalidation()` instead of `needs_repaint()` |
| `liquers-lib/src/ui/widgets/ui_spec_element.rs` | emit `data-lq-children="{handle}"` on the layout wrapper in `render_web` |
| `liquers-lib/src/ui/mod.rs` | re-export `Invalidation`, `UIChange` |
| `liquers-lib/examples/ui_*.rs` (5 egui apps) | request a repaint when `take_invalidation()` is not `None` (they get W3's fix too) |
| `specs/ISSUES.md` | W3 resolved; W4 closed as resolved by `async-wasm-refactor` |

`needs_repaint()` / `has_evaluating()` / `has_monitoring()` stay — they answer "is async work still
coming?", which the loop needs to decide whether to keep polling. They stop being the *rendering*
trigger.

### Implementation staging

The two halves are separable and Phase 4 will keep them as separate steps:

1. **Correctness (W3).** Record changes, consume the invalidation in the loop, and map *every*
   change to a re-render of the affected element (`Inserted`/`Removed` → re-render the parent).
   This alone closes W3 and is testable without any DOM work.
2. **Structural DOM operations.** Add `children_container`, the `data-lq-children` marker and the
   true insert/remove path, keeping the step-1 behaviour as the declared fallback.

Each is independently revertable; stopping after step 1 leaves a correct, coarser renderer.

## Relevant Commands

### New commands

**None.** W3 is a rendering concern; no query-language surface changes.

### Relevant existing namespaces

- **`lui`** — the mutation producers: `add` (via `insert_state` → `add_node`/`set_element`),
  `remove`, `activate` (`set_active_handle`), and navigation commands that do not mutate. They gain
  change recording for free by going through `AppState`; no signature changes.
- **`egui` / `pl` / image namespaces** — unaffected.

> **Question for review (unanswered):** is `lui` the complete set of namespaces that mutate
> `AppState`, or is there an application-level command elsewhere that mutates the tree directly and
> would need the same treatment?

## Web Endpoints

None. `liquers-axum` is untouched. SSR (`render_app_ssr`) is unaffected: it renders everything on
demand and never consults invalidation. The `data-lq-children` marker is inert in SSR output, and
is what a future hydration path would use.

## Error Handling

No new error paths, and no new `ErrorType`.

- `record_change`, `invalidate_all` and `take_invalidation` cannot fail — infallible bookkeeping.
- The DOM steps are `Option`-based and return `bool` rather than `Result`: a missing node or a
  failed insertion escalates to the whole-tree render. A renderer that returned `Err` mid-batch
  would leave the page half-updated, which is worse than re-rendering it. No `unwrap()`/`expect()`;
  every DOM lookup is matched.
- Focus restore is best-effort by construction: if the previously focused id is gone, nothing is
  restored.

## Serialization Strategy

- Neither `Invalidation` nor `UIChange` is serialized. `DirectAppState`'s custom `Serialize` builds
  `DirectAppStateSnapshot` explicitly, so the field is simply absent — no `#[serde(skip)]` needed
  and no format change.
- `Deserialize` sets `invalidation = Invalidation::All`: a restored state has never been rendered by
  the attached renderer, so everything is out of date. This is what makes "load a saved application
  state" paint correctly.

## Concurrency Considerations

- A change is recorded under the same `&mut self` borrow as the mutation that caused it, which in
  practice means under the same `tokio::sync::Mutex` guard — a change cannot be observed by a
  renderer without its record.
- `take_invalidation` clears; **exactly one renderer per application may call it**, documented on
  the method. Two consumers would each see part of the history.
- The browser loop needs a mutable guard for `take_invalidation`. When the lock is held (an async
  command is mid-flight) the loop skips the frame; the changes accumulate and are applied on the
  next tick, in order — nothing is lost. `MAX_CHANGES` bounds that accumulation.
- The change list is applied in recorded order, so an insert followed by a removal of the same
  handle cannot be reordered into nonsense; re-reading the model at apply time covers the rest.
- egui apps hold the lock during rendering and take the invalidation in the same guard.

## Compilation Validation

- `Vec<UIChange>` needs no ordering; the `Ord` derive on `UIHandle` supports test assertions and
  any future normalisation.
- The three trait methods have defaults, so no existing implementor breaks; `DirectAppState`
  overrides them.
- `UIChange`, `Invalidation`, `DeliveryOutcome` and `UpdateResponse` are all matched exhaustively —
  no `_` arms, per the project convention.
- New web-sys usage — `Element::{set_outer_html, query_selector, insert_before, remove, children}`,
  `Document::active_element`, `HtmlElement::focus`,
  `HtmlInputElement::{selection_start, selection_end, set_selection_range}` — should be covered by
  the features already enabled in `liquers-lib/Cargo.toml` (`Element`, `Document`, `HtmlElement`,
  `HtmlInputElement`, `Node`). To be re-verified at implementation; `HtmlTextAreaElement` would
  have to be added if caret restore is ever wanted for a multi-line field.
- Feature matrix to check: default (egui+polars+image), `--no-default-features --features webui`,
  `--features webui,image-support`, and `--target wasm32-unknown-unknown --features webui`.
  `Invalidation`/`UIChange` and the `AppState` methods are backend-neutral (no cfg); only
  `web/app.rs` code is gated, and it already sits inside
  `#[cfg(all(feature = "webui", target_arch = "wasm32"))]`.

## Review findings (inline)

Reviews run inline rather than as sub-agents (per this session's constraint); findings recorded
here for the record.

**rust-best-practices**

- *Resolved before writing:* trait extension uses defaulted methods per the "extend, don't mutate"
  rule, and the defaults are conservative (`take_invalidation` → `Invalidation::All`) so a
  non-tracking implementor cannot go stale; no `unwrap`/`expect` in the DOM paths; all Liquers-owned
  enums matched exhaustively; nothing crosses the crate dependency flow; `Ord` derive is additive.
- *Advisory:* putting render bookkeeping in `AppState` widens the model's job. Justified — the
  alternative (tracking in `AppRunner`) cannot see mutations made by commands that hold the
  `AppState` lock directly, which is exactly the W3 case.
- *Advisory:* `Changes(Vec<UIChange>)` exposes its container in the public API. Accepted: order is
  part of the meaning, so `Vec` is the right type and not an implementation detail to hide.

**Review round 2 (structural changes) — folded in**

- A dirty-handle set forces "adding a child" to be expressed as "the parent changed", so the
  renderer regenerates the parent's whole subtree: every sibling's DOM node is destroyed and
  recreated, losing scroll position, selection, CSS transition state and anything with intrinsic
  node state (canvas, media, iframe), and making a session of N added panels O(N²) in markup. The
  mutation sites already know the exact change; recording it instead of a handle keeps that
  information.
- This does not require backend-dependent tree operations: `AppState`'s operations are unchanged
  and backend-neutral; only the *record* is richer, and applying a record is already the backend's
  job. What is genuinely per-widget is whether a structural insert is *valid*, which is why the
  container opt-in is declared in markup by the widget that renders the children.
- Recording is not diffing: no tree comparison, no reconciliation heuristics — so Phase 1's
  "no diff/patch" decision is intact.

**Phase 1 conformity**

- Per-element invalidation with a global fallback, focus/caret preserved, no diff/patch, egui
  unaffected: all four Phase 1 decisions hold. Phase 1 open question 1 (where the dirty set lives)
  is answered — `AppState`, for the reason above. Phase 1 open question 2 (is "invalidate the
  parent" enough, or is an explicit structural signal needed?) is answered here: an explicit
  structural signal, which is what `UIChange` is.

**Codebase alignment**

- `DirectAppState` is the only `AppState` implementor in the workspace (verified), so the defaults
  are a safety net for downstream code rather than in-repo churn.
- `UpdateResponse::NeedsRepaint` is already produced by `QueryConsoleElement` and `AssetViewElement`
  and discarded by the runner — the signal exists and is unused today.
- `UISpecElement::render_web` renders children as a plain concatenation inside `div.lq-layout` for
  every layout variant, so the container marker is accurate for it as written.
- The `first`-frame flag in the browser loop becomes redundant (`new()` starts at `All`) and is
  removed.
- `ui_spec_demo` was rebuilt and its Playwright test re-run against current `HEAD`: it passes, so
  this feature starts from a working demo and must keep it working.

## References to `liquers-patterns.md`

- Traits extended with defaulted methods; object safety preserved.
- Explicit match arms on `UIChange`, `Invalidation`, `DeliveryOutcome`, `UpdateResponse`; no `_`
  arms on Liquers-owned enums.
- No `unwrap()`/`expect()` in library code; errors would use `liquers_core::error::Error` typed
  constructors (none are introduced).
- Async only where I/O happens; rendering and bookkeeping stay sync.
- Rich UI behaviour stays in `liquers-lib`; `liquers-core` untouched.
