# Phase 2: Solution & Architecture - webui-fixes

*Scope: W3 (rendering follows the model) + W4 (close a stale issue record). The interaction half
lives in `specs/ui-events/`. The previous draft of this file covered the old, wider scope and is in
git history (commit `89875c7`).*

## Overview

Invalidation becomes a property of the **model**: `AppState` records which elements changed, and
the renderer takes that record and re-renders exactly those subtrees. Structural mutations record
themselves; element-content changes are recorded by the runner from the `UpdateResponse` it already
receives. The browser driver replaces the markup of each invalidated element in place (stable
`ui-element-{handle}` ids make this a lookup, not a diff), preserving focus and caret, and falls
back to a whole-tree render when a change cannot be attributed to a handle.

## Data Structures

### New struct: `Invalidation`

```rust
// liquers-lib/src/ui/app_state.rs

/// What has changed in the model since a renderer last looked.
///
/// `all` means "cannot be attributed to individual elements" (the root set changed, the state was
/// deserialized); a renderer must then re-render everything. Otherwise `handles` lists the
/// elements whose rendered form is out of date.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Invalidation {
    all: bool,
    handles: BTreeSet<UIHandle>,
}
```

Rationale for a struct rather than an enum: `all` and `handles` are not mutually exclusive while
being accumulated, and a struct avoids an exhaustive `match` at every consumer for a value that has
no meaningful variants beyond "empty / some / everything".

`BTreeSet` (not `HashSet`) for deterministic iteration — tests assert on it, and the
descendant-filter step below reads better in ascending handle order. This requires `UIHandle` to be
`Ord`:

```rust
// liquers-lib/src/ui/handle.rs — additive derive, no call site changes
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct UIHandle(pub u64);
```

Ordering by id is meaningful (ids are allocated in creation order), so this is not a synthetic
derive.

### Modified struct: `DirectAppState`

One new field:

```rust
/// Transient render bookkeeping; never serialized (see Serialization Strategy).
invalidation: Invalidation,
```

`DirectAppState::new()` starts with `all = true`, so a renderer that attaches to a
freshly-built state always paints once without a special "first frame" flag.

### No new enums, no `ExtValue` variants, no new value types.

## Trait Implementations

Three methods added to `AppState`, **with defaults that are conservative rather than wrong**:

```rust
/// Record that `handle`'s rendered form is out of date.
/// Default: escalates to `invalidate_all` — correct, just coarser.
fn invalidate(&mut self, handle: UIHandle) {
    let _ = handle;
    self.invalidate_all();
}

/// Record that the whole tree must be re-rendered.
/// Default: no-op, paired with the `take_invalidation` default below.
fn invalidate_all(&mut self) {}

/// Take and clear the pending invalidation. Exactly one renderer per application may call this.
/// Default: `Invalidation::all()` — an implementation that does not track changes tells every
/// renderer to re-render everything, which is what the web backend does today. Never stale.
fn take_invalidation(&mut self) -> Invalidation {
    Invalidation::all()
}
```

This follows the project's "extend traits with defaults" rule without the usual hazard: an
implementor that ignores invalidation degrades to a full re-render (today's behaviour), it does not
silently go stale. `DirectAppState` — the only implementor in the workspace — overrides all three
with real tracking.

**Where invalidation is recorded** (all inside `DirectAppState`, under the same `&mut self` as the
mutation):

| Method | Records | Why |
|---|---|---|
| `add_node(Some(parent), …)` | `invalidate(parent)` | the parent's child list changed |
| `add_node(None, …)` / `insert_node(_, None, …)` | `invalidate_all()` | the root set changed |
| `insert_node(_, Some(parent), …)` | `invalidate(parent)` | as above |
| `set_element(h, _)` | `invalidate(h)` | the element's markup changed |
| `set_source(h, _)` | `invalidate(h)` | a pending node renders a placeholder |
| `remove(h)` | `invalidate(parent)`, or `invalidate_all()` if `h` was a root | the child list changed |
| `set_active_handle(old, new)` | `invalidate(old)`, `invalidate(new)` | active state is renderable |
| `take_element` / `put_element` | **nothing** | see below |

**`take_element`/`put_element` deliberately do not invalidate.** They are the extract-render-replace
pair the *egui renderer itself* uses on every frame; invalidating there would mark the whole tree
dirty every frame and turn "repaint when something changed" back into "repaint always". The rule
that replaces it:

> Structural changes invalidate themselves. A change to an element's *content* is reported by
> whoever made it — from `update`, via the returned `UpdateResponse::NeedsRepaint`.

`AppRunner::deliver_snapshot` already computes that response and currently discards it; it becomes
the invalidation source for element-content changes.

## Generic Parameters & Bounds

None added. `Invalidation` is concrete and non-generic (it crosses the `dyn AppState` boundary, so
it must be). `AppState` stays object-safe: the three new methods take/return concrete types, no
generics, no `Self` by value.

## Sync vs Async Decisions

| Function | Choice | Rationale |
|---|---|---|
| `AppState::invalidate*` / `take_invalidation` | sync | in-memory bookkeeping under an existing `&mut self`; `AppState` is sync by design |
| `AppRunner::run` | async (unchanged) | already awaits evaluation |
| browser render step | sync | DOM manipulation; runs between runner awaits |
| focus save/restore | sync | DOM reads/writes |

No I/O is introduced, so nothing new becomes async.

## Function Signatures

```rust
// liquers-lib/src/ui/app_state.rs
impl Invalidation {
    pub fn all() -> Self;
    pub fn is_empty(&self) -> bool;
    pub fn is_all(&self) -> bool;
    pub fn handles(&self) -> impl Iterator<Item = UIHandle> + '_;
    pub fn insert(&mut self, handle: UIHandle);
    pub fn set_all(&mut self);
}

pub trait AppState {
    // … existing …
    fn invalidate(&mut self, handle: UIHandle);          // default: invalidate_all()
    fn invalidate_all(&mut self);                        // default: no-op
    fn take_invalidation(&mut self) -> Invalidation;     // default: Invalidation::all()
}

// liquers-lib/src/ui/runner.rs — invalidate on a NeedsRepaint response
async fn deliver_snapshot(
    handle: UIHandle,
    snapshot: AssetSnapshot,
    app_state: &Arc<tokio::sync::Mutex<dyn AppState>>,
    sender: &AppMessageSender,
) -> DeliveryOutcome;

/// Delivery result: whether the element still exists, and what it said about repainting.
enum DeliveryOutcome {
    Missing,
    Delivered(UpdateResponse),
}

// liquers-lib/src/ui/web/app.rs (browser module, wasm only)

/// Re-render one element in place: replace `#ui-element-{handle}`'s markup.
/// Returns false if the node is not in the DOM (caller falls back to a full render).
fn render_handle_into(root: &web_sys::Element, handle: UIHandle, state: &dyn AppState) -> bool;

/// Focused field and selection, captured before markup is replaced.
struct FocusSnapshot { element_id: String, selection: Option<(u32, u32)> }
fn capture_focus(doc: &web_sys::Document) -> Option<FocusSnapshot>;
fn restore_focus(doc: &web_sys::Document, snapshot: &FocusSnapshot);

/// Apply an invalidation to the DOM. Whole-tree render when `is_all()`, otherwise targeted
/// re-render of each handle that has no invalidated ancestor.
fn apply_invalidation(root: &web_sys::Element, inv: &Invalidation, state: &dyn AppState);
```

### The targeted-render algorithm

1. If `inv.is_all()` → capture focus, re-render all roots into `root`, restore focus. Done.
2. Otherwise, for each handle in ascending order:
   - skip it if it no longer exists in `AppState` (its parent was invalidated by the removal);
   - skip it if any ancestor (walking `AppState::parent`) is also in the set — the ancestor's
     re-render already includes it;
   - look up `#ui-element-{n}`; if missing, escalate to the whole-tree path and stop;
   - replace that node's markup with `render_element_web(handle, state)`.
3. Capture focus before the first replacement and restore it after the last.

No diff, no patch: the tree plus stable ids give the granularity, exactly as Phase 1 decided.

## Integration Points

| File | Change |
|---|---|
| `liquers-lib/src/ui/handle.rs` | `PartialOrd, Ord` derives on `UIHandle` |
| `liquers-lib/src/ui/app_state.rs` | `Invalidation`; three trait methods with defaults; `DirectAppState` field + recording in the mutating methods; serde handling |
| `liquers-lib/src/ui/runner.rs` | `DeliveryOutcome`; invalidate the handle when an element reports `NeedsRepaint`; drop the stray `println!` in `process_messages` |
| `liquers-lib/src/ui/web/app.rs` | `apply_invalidation`, `render_handle_into`, focus capture/restore; the loop consumes `take_invalidation()` instead of `needs_repaint()` |
| `liquers-lib/src/ui/mod.rs` | re-export `Invalidation` |
| `liquers-lib/examples/ui_*.rs` (5 egui apps) | request a repaint when `take_invalidation()` is non-empty (they get W3's fix too) |
| `specs/ISSUES.md` | W3 resolved; W4 closed as resolved by `async-wasm-refactor` |

`needs_repaint()` / `has_evaluating()` / `has_monitoring()` stay — they answer "is async work still
coming?", which the loop still needs to decide whether to keep polling. They stop being the
*rendering* trigger.

## Relevant Commands

### New commands

**None.** W3 is a rendering concern; no query-language surface changes.

### Relevant existing namespaces

- **`lui`** — the mutation producers: `add` (via `insert_state` → `add_node`/`set_element`),
  `remove`, `activate` (`set_active_handle`), and navigation commands that do not mutate. They gain
  invalidation for free by going through `AppState`; none of their signatures change.
- **`egui` / `pl` / image namespaces** — unaffected.

> **Question for review:** is `lui` the complete set of namespaces that mutate `AppState`, or is
> there an application-level command elsewhere that mutates the tree directly and would need the
> same treatment?

## Web Endpoints

None. `liquers-axum` is untouched. SSR (`render_app_ssr`) is unaffected: it renders everything on
demand and never consults invalidation.

## Error Handling

No new error paths, and no new `ErrorType`.

- `invalidate*` and `take_invalidation` cannot fail — they are infallible bookkeeping.
- The DOM steps are `Option`-based: a missing node or a failed `set_outer_html` escalates to the
  whole-tree render rather than propagating an error, because a renderer that returns `Err` mid-frame
  would leave the page half-updated. No `unwrap()`/`expect()`; every DOM lookup is matched.
- Focus restore is best-effort by construction: if the previously focused id is gone, nothing is
  restored.

## Serialization Strategy

- `Invalidation` is **not** serialized. `DirectAppState`'s custom `Serialize` builds
  `DirectAppStateSnapshot` explicitly, so the field is simply absent — no `#[serde(skip)]` needed
  and no format change.
- `Deserialize` sets `invalidation = Invalidation::all()`: a restored state has never been rendered
  by the attached renderer, so everything is out of date. This is what makes "load a saved
  application state" paint correctly.

## Concurrency Considerations

- Invalidation is recorded under the same `&mut self` borrow as the mutation that caused it, which
  in practice means under the same `tokio::sync::Mutex` guard — a change cannot be observed by a
  renderer without its invalidation.
- `take_invalidation` clears; **exactly one renderer per application may call it**, documented on
  the method. Two consumers would each see half the changes.
- The browser is single-threaded and the loop already uses `try_lock` for rendering; it now needs a
  mutable guard for `take_invalidation`. When the lock is held (an async command is mid-flight), the
  loop skips the frame — the invalidation is still pending on the next tick, so nothing is lost.
  This is strictly better than today, where a skipped frame could drop the repaint entirely.
- egui apps hold the lock during rendering anyway; they take the invalidation in the same guard.

## Compilation Validation

- `BTreeSet<UIHandle>` needs `Ord` → the derive above. Without it this does not compile.
- The three trait methods have defaults, so no existing implementor breaks; `DirectAppState`
  overrides them.
- `DeliveryOutcome` replaces a `bool` return at two call sites in `runner.rs`, both matched
  exhaustively (no `_` arm), per the project convention.
- New web-sys usage — `Element::set_outer_html`, `Document::active_element`, `HtmlElement::focus`,
  `HtmlInputElement::{selection_start, selection_end, set_selection_range}` — is covered by the
  features already enabled in `liquers-lib/Cargo.toml` (`Element`, `Document`, `HtmlElement`,
  `HtmlInputElement`). To be re-verified at implementation; if `HtmlTextAreaElement` is wanted for
  caret restore in a future multi-line field, that feature must be added then.
- Feature matrix to check: default (egui+polars+image), `--no-default-features --features webui`,
  `--features webui,image-support`, and `--target wasm32-unknown-unknown --features webui`.
  `Invalidation` and the `AppState` methods are backend-neutral (no cfg); only `web/app.rs` code is
  gated, and it already sits inside a `#[cfg(all(feature = "webui", target_arch = "wasm32"))]`
  module.

## Review findings (inline)

Reviews run inline rather than as sub-agents (per this session's constraint); findings recorded
here for the record.

**rust-best-practices**

- *Resolved before writing:* `BTreeSet` requires `Ord` (derive added); trait extension uses defaults
  per the "extend, don't mutate" rule; the defaults are conservative (`take_invalidation` →
  `Invalidation::all()`) so a non-tracking implementor cannot go stale; no `unwrap`/`expect` in the
  DOM paths; `DeliveryOutcome` matched exhaustively; nothing crosses the crate dependency flow.
- *Advisory:* putting render bookkeeping in `AppState` is a widening of the model's job. Justified —
  the alternative (tracking in `AppRunner`) cannot see mutations made by commands that hold the
  `AppState` lock directly, which is exactly the W3 case. Recorded so it is a deliberate choice.
- *Advisory:* `Invalidation::handles()` returning `impl Iterator` keeps the container private, so
  swapping `BTreeSet` for something else later is not a breaking change.

**Phase 1 conformity**

- Per-handle invalidation with a global fallback, focus/caret preserved, no diff/patch, egui
  unaffected: all four Phase 1 decisions are implemented as stated. Phase 1 open question 1 (where
  the dirty set lives) is answered here — `AppState`, with the reason above.

**Codebase alignment**

- `DirectAppState` is the only `AppState` implementor in the workspace (verified), so the defaults
  are a safety net for downstream code rather than in-repo churn.
- `UpdateResponse::NeedsRepaint` is already produced by `QueryConsoleElement` and
  `AssetViewElement` and discarded by the runner — the signal exists and is unused today.
- The `first`-frame flag in the browser loop becomes redundant (`new()` starts invalidated) and is
  removed.

## References to `liquers-patterns.md`

- Traits extended with defaulted methods; object safety preserved.
- Explicit match arms (`DeliveryOutcome`, `UpdateResponse`); no `_` arms on Liquers-owned enums.
- No `unwrap()`/`expect()` in library code; errors would use `liquers_core::error::Error` typed
  constructors (none are introduced).
- Async only where I/O happens; rendering and bookkeeping stay sync.
- Rich UI behaviour stays in `liquers-lib`; `liquers-core` untouched.
