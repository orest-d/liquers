//! [`EnvSource`]: where a running server gets the environment for each request. Documented with
//! the rest of the server in [`crate::server`].

use async_trait::async_trait;
use liquers_core::context::{EnvRef, Environment};
use liquers_core::error::Error;

/// The environment a server answers a request with.
///
/// A fixed [`EnvRef`] is a source (each request gets a clone). A host whose environment is
/// rebuilt (liquers-py, when a command is registered after the server started) implements it to
/// hand out its current build; the server re-states its router only when the returned `EnvRef`
/// changes (`Arc::ptr_eq`). An error answers the request with 503.
#[async_trait]
pub trait EnvSource<E: Environment>: Send + Sync + 'static {
    /// The environment for the next request.
    async fn current(&self) -> Result<EnvRef<E>, Error>;
}

#[async_trait]
impl<E: Environment> EnvSource<E> for EnvRef<E> {
    async fn current(&self) -> Result<EnvRef<E>, Error> {
        Ok(self.clone())
    }
}
