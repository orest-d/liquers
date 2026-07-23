# Liquers Rust Anti-Patterns — worked examples

Fuller before/after catalog. Read when a review needs concrete illustration
beyond the SKILL.md checklist. Each entry: the smell, why it bites, the fix.

## Table of contents

1. Error handling
2. Match exhaustiveness & feature gating
3. Ownership & cloning
4. Async / locking
5. Traits & object safety
6. Serialization
7. Command signatures

---

## 1. Error handling

**Smell: `unwrap`/`expect` in library code.**
```rust
// BAD — panics, takes down the whole runtime/request
let df = state.as_polars_dataframe().unwrap();
```
```rust
// GOOD — propagate; caller decides
let df = state.as_polars_dataframe()?;
```
Why: a library is called from contexts (servers, UIs, Python) that must not panic
on bad input. `?` turns the failure into a value the caller can render or log.

**Smell: `Error::new` / a bespoke error enum.**
```rust
// BAD
return Err(Error::new(ErrorType::General, "bad key"));
enum MyError { NotFound, Bad }          // BAD — new error type
```
```rust
// GOOD — typed constructors on the one shared Error
return Err(Error::general_error("bad key".to_string()));
external_call().map_err(|e| Error::from_error(ErrorType::General, e))?;
```
Why: one error type keeps `?` composable across crates and keeps error metadata
(type, traceback) consistent for the API/UI layers.

---

## 2. Match exhaustiveness & feature gating

**Smell: default arm on a Liquers-owned enum.**
```rust
// BAD — a new Status variant silently hits the catch-all
match status {
    Status::Ready => render_ready(),
    _ => render_other(),
}
```
```rust
// GOOD — compiler forces you to handle new variants
match status {
    Status::Ready => render_ready(),
    Status::Error => render_error(),
    Status::Submitted | Status::Started => render_progress(),
    // ... every variant
}
```

**Smell: cfg'd-out variant, un-cfg'd match arm.**
```rust
pub enum ExtValue {
    Image { .. },
    #[cfg(feature = "egui")] Widget { .. },   // gated variant
    UIElement { .. },
}
// BAD — fails to compile with `--no-default-features --features webui`
match v {
    ExtValue::Image { .. } => ...,
    ExtValue::Widget { .. } => ...,   // references a variant that isn't there
    ExtValue::UIElement { .. } => ...,
}
```
```rust
// GOOD — the arm carries the same cfg as the variant
match v {
    ExtValue::Image { .. } => ...,
    #[cfg(feature = "egui")]
    ExtValue::Widget { .. } => ...,
    ExtValue::UIElement { .. } => ...,
}
```
Why: a gated variant that some match forgets to gate is the single most common
feature-flag build break. Always pair variant cfg with arm cfg, in *every* match.

---

## 3. Ownership & cloning

**Smell: cloning heavy data instead of the Arc.**
```rust
// BAD — deep-copies the whole DataFrame
let df: DataFrame = (*arc_df).clone();
render(&df);
```
```rust
// GOOD — bump the refcount, or just borrow
render(arc_df.as_ref());          // borrow
let shared = arc_df.clone();      // cheap Arc clone if you need ownership
```

**Smell: `Box<T>` for something cloned all over.**
`ExtValue` payloads are `Arc<T>` precisely so `Value: Clone` is cheap. A `Box<T>`
there would force deep clones. Use `Arc` for shared, frequently-cloned payloads;
`Box` for single-owner trait objects that aren't cloned (or provide `clone_boxed`).

---

## 4. Async / locking

**Smell: blocking I/O in async.**
```rust
// BAD — blocks the executor thread
async fn load(path: &str) -> Result<Vec<u8>, Error> {
    std::fs::read(path).map_err(|e| Error::from_error(ErrorType::General, e))
}
```
```rust
// GOOD
async fn load(path: &str) -> Result<Vec<u8>, Error> {
    tokio::fs::read(path).await.map_err(|e| Error::from_error(ErrorType::General, e))
}
```

**Smell: holding a lock across `.await`.**
```rust
// BAD — lock held while awaiting; can deadlock other tasks / block renders
let mut s = app_state.lock().await;
let ar = envref.evaluate(&q).await?;   // still holding the lock
s.set_element(h, elem)?;
```
```rust
// GOOD — release before the await, re-acquire after
let ar = envref.evaluate(&q).await?;
{
    let mut s = app_state.lock().await;
    s.set_element(h, elem)?;
}
```
Why: this is the pattern `AppRunner` already follows (lock per phase). Long lock
holds serialize the whole UI/runtime.

**wasm note:** `tokio::spawn` requires `Send` and a multi-thread runtime that
wasm doesn't have. Route spawns through the project helper (`spawn_ui_task`), which
picks `spawn_local` on `wasm32`.

---

## 5. Traits & object safety

**Smell: over-broad bounds leaking to callers.**
```rust
// BAD — every caller now must satisfy all of these even if unused
fn render<T: Clone + Send + Sync + 'static + Debug>(x: &T) { x.fmt(...) }
```
```rust
// GOOD — only what's used
fn render<T: Debug>(x: &T) { ... }
```

**Smell: breaking a `dyn` trait's object safety.**
```rust
// BAD — generic method makes UIElement not object-safe → Box<dyn UIElement> breaks
trait UIElement {
    fn render<R: Renderer>(&self, r: &mut R);
}
```
```rust
// GOOD — concrete params (or feature-gated concrete methods) keep it dyn-able
trait UIElement {
    #[cfg(feature = "webui")]
    fn show_in_web(&mut self, web: &mut WebUi, ...);
}
```
Why: `AppState` stores `Box<dyn UIElement>`; a generic method would make that
storage impossible. Add per-backend concrete methods, gated by feature, instead.

**Smell: mutating a shared trait signature.** Changing an existing method breaks
every implementor and `liquers-py`. Add a new default method instead.

---

## 6. Serialization

**Smell: serializing runtime-only fields.**
```rust
// BAD — Arc<dyn Trait> / receivers aren't Serialize; or they're stale on load
#[derive(Serialize, Deserialize)]
struct Elem { value: Arc<Value>, rx: watch::Receiver<Msg> }
```
```rust
// GOOD — skip transient state, rebuild it in init()/refresh
#[derive(Serialize, Deserialize)]
struct Elem {
    title: String,
    #[serde(skip)] value: Option<Arc<Value>>,
    #[serde(skip)] rx: Option<watch::Receiver<Msg>>,
}
```

**Smell: `Serialize` derive on `ExtValue`.** It intentionally derives only
`Debug + Clone`. Byte conversion goes through `DefaultValueSerializer`.

---

## 7. Command signatures

**Smell: wrong state ownership / context position.**
```rust
// BAD — async command borrowing state; context not last
async fn cmd(state: &State<Value>, context: Context<E>, n: i64) -> Result<Value, Error>
```
```rust
// GOOD — async takes owned State; context is the final parameter
async fn cmd(state: State<Value>, n: i64, context: Context<E>) -> Result<Value, Error>
// sync variant borrows:
fn cmd(state: &State<Value>, n: i64) -> Result<Value, Error>
```
Why: the `register_command!` macro and the parameter-index handling expect exactly
this shape (see `specs/ISSUES.md` on the context-last requirement). Namespace is
set in the macro metadata, never baked into the function name.
