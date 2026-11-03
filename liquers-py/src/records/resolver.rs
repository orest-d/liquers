//! Where a record source resolves its chunks: [`Scope`], [`SourceOrigin`] and
//! [`PyChunkResolver`].
//!
//! A `lq.RecordSource` reads its chunks lazily, long after the conversion that made it, so it has
//! to know which environment to read them in. The conversion that makes the wrapper passes a
//! [`Scope`] (`value_to_py`), and the wrapper keeps its [`SourceOrigin`]:
//! - a source that reaches Python through a command's argument or `Context.evaluate` resolves
//!   through the command's context while that context is valid, so each chunk it reads is
//!   recorded as a dependency of the asset being computed;
//! - otherwise, and once the command has returned, through the environment itself;
//! - once the environment is gone or shut down, every read fails with `shut_down_error()`.
//!
//! The choice is made on every call, not when a stream opens, so a stream kept past its command
//! stays safe. The origin holds the environment through a `Weak`: a source captured by a command
//! closure would otherwise make a reference cycle the garbage collector cannot see (the recipe
//! providers hold their environment through `Weak` for the same reason). See
//! `specs/design/liquers-py-records/phase2-architecture.md` §"Where a source resolves".

use std::sync::{Arc, Weak};

use liquers_core::context::EnvRef;
use liquers_core::error::Error;
use liquers_core::maybe_send::BoxFuture;
use liquers_core::metadata::Metadata;
use liquers_core::query::{Key, Query};
use liquers_records::{ChunkResolver, ChunkValue, ContextResolver, EnvResolver};
use pyo3::prelude::*;

use crate::aio::Shut;
use crate::context::Context;
use crate::environment::{shut_down_error, EnvCell, PyEnvironment};

/// What a conversion out of a value resolves record sources in. Cloned with each value it
/// converts; cheap (two `Arc`s and a `Weak`).
#[derive(Clone)]
pub struct Scope {
    origin: Option<SourceOrigin>,
}

impl Scope {
    /// The scope of a call on an `lq.Environment`: its built environment, held weakly. A shut
    /// environment gives a scope whose sources refuse to traverse.
    pub fn env(py: Python<'_>, cell: &Arc<EnvCell>) -> Scope {
        let env = match cell.envref(py) {
            Ok(envref) => Arc::downgrade(&envref.0),
            Err(_) => Weak::new(),
        };
        Scope {
            origin: Some(SourceOrigin {
                env,
                shut: cell.shut.clone(),
                context: None,
            }),
        }
    }

    /// The scope of an already built environment, for a caller that holds its `EnvRef`.
    pub fn from_envref(envref: &EnvRef<PyEnvironment>, shut: Arc<Shut>) -> Scope {
        Scope {
            origin: Some(SourceOrigin {
                env: Arc::downgrade(&envref.0),
                shut,
                context: None,
            }),
        }
    }

    /// The scope of a running command: its context while that is valid, then its environment.
    pub fn command(context: &Context, envref: &EnvRef<PyEnvironment>, shut: Arc<Shut>) -> Scope {
        Scope {
            origin: Some(SourceOrigin {
                env: Arc::downgrade(&envref.0),
                shut,
                context: Some(context.clone_handle()),
            }),
        }
    }

    /// No environment at all: a source made in this scope refuses to traverse. For unit tests and
    /// conversions that cannot produce a source.
    pub fn detached() -> Scope {
        Scope { origin: None }
    }

    /// The origin a `lq.RecordSource` made in this scope keeps.
    pub fn origin(&self) -> Option<SourceOrigin> {
        self.origin.clone()
    }
}

/// Where one `lq.RecordSource` reads its chunks. Never holds the environment strongly.
pub struct SourceOrigin {
    env: Weak<PyEnvironment>,
    shut: Arc<Shut>,
    /// The handle on the command's `lq.Context`, when the source reached Python inside a command.
    context: Option<Context>,
}

impl Clone for SourceOrigin {
    /// By hand: a clone shares the context handle, so invalidating the command's context
    /// invalidates it in every clone.
    fn clone(&self) -> Self {
        SourceOrigin {
            env: self.env.clone(),
            shut: self.shut.clone(),
            context: self.context.as_ref().map(Context::clone_handle),
        }
    }
}

impl SourceOrigin {
    /// The environment's shut signal, which every wait on the source watches.
    pub fn shut(&self) -> &Arc<Shut> {
        &self.shut
    }
}

/// The resolver a `lq.RecordSource` traverses with: it decides on every call between the
/// command's context, the environment, and refusal.
pub struct PyChunkResolver {
    origin: Option<SourceOrigin>,
}

impl PyChunkResolver {
    pub fn new(origin: Option<SourceOrigin>) -> Self {
        PyChunkResolver { origin }
    }

    /// The resolver this call goes through, or why there is none.
    fn current(&self) -> Result<Arc<dyn ChunkResolver>, Error> {
        let origin = self.origin.as_ref().ok_or_else(|| {
            Error::general_error(
                "this record source has no environment to read its chunks in".to_string(),
            )
        })?;
        if origin.shut.is_set() {
            return Err(shut_down_error());
        }
        if let Some(context) = origin.context.as_ref().and_then(Context::core) {
            return Ok(Arc::new(ContextResolver::new(context)));
        }
        match origin.env.upgrade() {
            Some(env) => Ok(Arc::new(EnvResolver::new(EnvRef(env)))),
            None => Err(shut_down_error()),
        }
    }
}

impl ChunkResolver for PyChunkResolver {
    fn evaluate(&self, query: Query) -> BoxFuture<'static, Result<ChunkValue, Error>> {
        match self.current() {
            Ok(resolver) => resolver.evaluate(query),
            Err(error) => Box::pin(async move { Err(error) }),
        }
    }

    fn metadata(&self, query: Query) -> BoxFuture<'static, Result<Metadata, Error>> {
        match self.current() {
            Ok(resolver) => resolver.metadata(query),
            Err(error) => Box::pin(async move { Err(error) }),
        }
    }

    fn read_resource(&self, key: Key) -> BoxFuture<'static, Result<(Vec<u8>, Metadata), Error>> {
        match self.current() {
            Ok(resolver) => resolver.read_resource(key),
            Err(error) => Box::pin(async move { Err(error) }),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::num::NonZeroUsize;

    use liquers_core::context::Context as CoreContext;
    use liquers_core::parse::parse_query;
    use liquers_records::ChunkValue;

    use liquers_core::environment_config::EnvironmentConfig;
    use crate::runtime::runtime;
    use crate::value::ConversionMode;

    fn new_cell() -> Result<Arc<EnvCell>, Error> {
        Ok(Arc::new(EnvCell::new(
            EnvironmentConfig::default(),
            ConversionMode::Permissive,
            false,
            NonZeroUsize::MIN,
        )?))
    }

    /// Evaluates `query` through `resolver`, detached from the interpreter.
    fn evaluate_query(py: Python<'_>, resolver: &PyChunkResolver, query: &str) -> Result<ChunkValue, Error> {
        let future = resolver.evaluate(parse_query(query)?);
        py.detach(|| runtime()?.block_on(future))
    }

    fn evaluate(py: Python<'_>, resolver: &PyChunkResolver) -> Result<ChunkValue, Error> {
        let future = resolver.evaluate(parse_query("commands_doc")?);
        py.detach(|| runtime()?.block_on(future))
    }

    #[test]
    fn resolver_uses_the_context_while_valid_then_the_environment() -> Result<(), Error> {
        Python::initialize();
        Python::attach(|py| {
            let cell = new_cell()?;
            let envref = cell.envref(py)?;
            let asset = {
                let envref = envref.clone();
                py.detach(|| runtime()?.block_on(async move { envref.evaluate(parse_query("commands_doc/to_text")?).await }))?
            };
            let core = py.detach(|| -> Result<CoreContext<PyEnvironment>, Error> {
                Ok(runtime()?.block_on(CoreContext::new(asset, false)))
            })?;
            let context = Context::new(core, cell.shut.clone());
            let resolver = PyChunkResolver::new(context.scope(&envref).origin());

            // While the context is valid, a chunk is a dependency of the context's asset: its own
            // query is a cycle, and any other query is read.
            assert!(context.is_valid());
            assert!(matches!(evaluate(py, &resolver)?, ChunkValue::Bytes { .. }));
            let cycle = evaluate_query(py, &resolver, "commands_doc/to_text").err();
            assert_eq!(cycle.map(|e| e.error_type), Some(liquers_core::error::ErrorType::DependencyCycle));

            // A source kept past its command reads through the environment, with no dependency.
            context.invalidate();
            assert!(!context.is_valid());
            assert!(matches!(evaluate_query(py, &resolver, "commands_doc/to_text")?, ChunkValue::Bytes { .. }));
            cell.shutdown(py)
        })
    }

    #[test]
    fn resolver_refuses_after_shutdown() -> Result<(), Error> {
        Python::initialize();
        Python::attach(|py| {
            let cell = new_cell()?;
            let resolver = PyChunkResolver::new(Scope::env(py, &cell).origin());
            assert!(matches!(evaluate(py, &resolver)?, ChunkValue::Bytes { .. }));
            cell.shutdown(py)?;
            let error = evaluate(py, &resolver).err().ok_or_else(|| Error::general_error("expected a refusal".to_string()))?;
            assert_eq!(error.message, shut_down_error().message);
            // A scope taken after the shutdown refuses too.
            let late = PyChunkResolver::new(Scope::env(py, &cell).origin());
            assert!(evaluate(py, &late).is_err());
            Ok(())
        })
    }

    #[test]
    fn resolver_refuses_without_environment() -> Result<(), Error> {
        Python::initialize();
        Python::attach(|py| {
            let resolver = PyChunkResolver::new(Scope::detached().origin());
            let error = evaluate(py, &resolver).err().ok_or_else(|| Error::general_error("expected a refusal".to_string()))?;
            assert_eq!(error.error_type, liquers_core::error::ErrorType::General);
            assert!(error.message.contains("no environment"), "{}", error.message);
            Ok(())
        })
    }
}
