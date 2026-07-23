//! Target-conditional `Send`/`Sync` markers and boxed-future alias.
//!
//! On native targets these are exactly `Send`/`Sync`; on `wasm32` (single-threaded,
//! browser event loop) they are universally implemented, so `?Send` async traits and
//! `!Send` data (e.g. captured `JsValue`, `Rc`) compile.
//!
//! IMPORTANT: this MUST be gated on `target_arch`, NEVER on a Cargo feature. Cargo
//! feature unification is additive across the whole workspace, so a `non_send`-style
//! feature would silently strip `Send` from the native multi-threaded build everywhere.
//!
//! Two tools, applied where each is legal:
//! - **supertrait / generic bounds** use the [`MaybeSend`] / [`MaybeSync`] marker traits
//!   (`trait Foo: MaybeSend`, `F: MaybeSend`);
//! - **trait-object additional bounds** (`dyn Fn(..) -> Pin<Box<dyn Future + Send>>`) CANNOT
//!   use the markers — only auto-traits may follow the principal trait in a trait object
//!   (`E0225`) — so the whole boxed type is aliased per target instead: see [`BoxFuture`].
//! - `#[async_trait]` method futures use `#[cfg_attr(.., async_trait(?Send))]` at each site.

use core::future::Future;
use core::pin::Pin;

#[cfg(not(target_arch = "wasm32"))]
mod imp {
    /// On native: `Send`. Blanket-implemented for every `Send` type.
    pub trait MaybeSend: Send {}
    impl<T: Send + ?Sized> MaybeSend for T {}
    /// On native: `Sync`. Blanket-implemented for every `Sync` type.
    pub trait MaybeSync: Sync {}
    impl<T: Sync + ?Sized> MaybeSync for T {}
}

#[cfg(target_arch = "wasm32")]
mod imp {
    /// On wasm: vacuous. Blanket-implemented for every type.
    pub trait MaybeSend {}
    impl<T: ?Sized> MaybeSend for T {}
    /// On wasm: vacuous. Blanket-implemented for every type.
    pub trait MaybeSync {}
    impl<T: ?Sized> MaybeSync for T {}
}

pub use imp::{MaybeSend, MaybeSync};

/// Boxed future used in return positions. `Send`-bounded on native, bare on wasm.
///
/// Trait-object bounds cannot use the [`MaybeSend`] marker (only auto-traits may follow
/// the principal trait), so the whole type is aliased per target.
#[cfg(not(target_arch = "wasm32"))]
pub type BoxFuture<'a, T> = Pin<Box<dyn Future<Output = T> + Send + 'a>>;

/// Boxed future used in return positions. `Send`-bounded on native, bare on wasm.
#[cfg(target_arch = "wasm32")]
pub type BoxFuture<'a, T> = Pin<Box<dyn Future<Output = T> + 'a>>;

/// Box a future into [`BoxFuture`] with the target-correct Send-ness. Replaces
/// `FutureExt::boxed()` (which is always `Send`-boxed and thus wrong on wasm) at call sites
/// whose return type is [`BoxFuture`].
pub trait MaybeBoxed<'a>: Future + Sized + 'a {
    fn maybe_boxed(self) -> BoxFuture<'a, Self::Output>;
}

#[cfg(not(target_arch = "wasm32"))]
impl<'a, F: Future + Send + 'a> MaybeBoxed<'a> for F {
    fn maybe_boxed(self) -> BoxFuture<'a, Self::Output> {
        Box::pin(self)
    }
}

#[cfg(target_arch = "wasm32")]
impl<'a, F: Future + 'a> MaybeBoxed<'a> for F {
    fn maybe_boxed(self) -> BoxFuture<'a, Self::Output> {
        Box::pin(self)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // Compile-time checks: markers are satisfied by ordinary data on both targets, and
    // BoxFuture is constructible.
    fn _assert_marker<T: MaybeSend + MaybeSync>() {}

    #[test]
    fn markers_hold_for_data() {
        _assert_marker::<i32>();
        _assert_marker::<String>();
    }

    #[test]
    fn box_future_constructs() {
        let f: BoxFuture<'static, i32> = Box::pin(async { 42 });
        let _ = f;
    }
}
