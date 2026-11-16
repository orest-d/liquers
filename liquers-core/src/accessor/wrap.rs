//! Wrappers that restrict what an accessor accepts: [`read_only`] and [`validated`].
//!
//! A wrapper is an accessor of its own around another one. Its projections go through
//! [`WrapperProjection`], which puts the part into the wrapper's value and sets that through the
//! wrapper, so the wrapper's rule also holds for every part.
//!
//! # Main components
//!
//! - [`read_only`]: every set refused with `NotSupported`; `capabilities().write` is false.
//! - [`validated`]: every set checked by a [`ValueCheck`] first.
//! - [`WrapperProjection`]: a projection of any accessor that is not a hub node.
//!
//! # Where it is used
//!
//! - `liquers-lib` hands a read-only accessor out for an asset that cannot be written.
//! - An application wraps an accessor with [`validated`] to enforce a rule on a shared value.
//!
//! # Related concepts
//!
//! - [`super::AccessorHub`]: the accessor a wrapper usually wraps; its projections are hub nodes.
//! - [`super::ProjectionStep`]: the lens a [`WrapperProjection`] applies.
//!
//! # Examples
//!
//! ```
//! # futures::executor::block_on(async {
//! # use std::sync::Arc;
//! # use liquers_core::accessor::{memory_accessor, validated, SetOptions, ValueAccessor};
//! # use liquers_core::{error::Error, query::Query, value::Value};
//! let positive = validated(
//!     memory_accessor::<Value>(None, Query::new()),
//!     Arc::new(|v: &Value| match v {
//!         Value::I32(n) if *n > 0 => Ok(()),
//!         _ => Err(Error::general_error("must be positive".to_string())),
//!     }),
//! );
//! assert!(positive.set(Value::I32(-1), SetOptions::default()).await.is_err());
//! assert!(positive.set(Value::I32(1), SetOptions::default()).await.is_ok());
//! # });
//! ```
//!
//! - [`hub` tests](https://github.com/orest-d/liquers/blob/main/liquers-core/src/accessor/hub.rs):
//!   `read_only_and_validated_refuse_sets`.

use std::fmt::Debug;
use std::sync::Arc;

use tokio::sync::watch;

use super::hub::unavailable;
use super::{
    content_version, stable_hash, AccessorEvent, AccessorInfo, AccessorSnapshot, Capabilities,
    Fingerprint, InvalidationSink, PartDescription, ProjectionStep, SetOptions, ValueAccessor,
};
use crate::error::Error;
use crate::maybe_send::BoxFuture;
use crate::metadata::{LogEntry, Metadata};
use crate::query::Query;
use crate::state::State;
use crate::value::ValueInterface;

/// The check of a [`validated`] accessor.
pub type ValueCheck<V> = Arc<dyn Fn(&V) -> Result<(), Error> + Send + Sync>;

/// `inner`, with every set refused (`NotSupported`). Projections are read-only too.
pub fn read_only<V: ValueInterface>(inner: Arc<dyn ValueAccessor<V>>) -> Arc<dyn ValueAccessor<V>> {
    Arc::new(Restricted {
        inner,
        rule: Rule::ReadOnly,
    })
}

/// `inner`, with every set checked by `check` first; a refused value is the check's own error and
/// nothing is written. A set through a projection checks the whole new value.
pub fn validated<V: ValueInterface>(
    inner: Arc<dyn ValueAccessor<V>>,
    check: ValueCheck<V>,
) -> Arc<dyn ValueAccessor<V>> {
    Arc::new(Restricted {
        inner,
        rule: Rule::Check(check),
    })
}

enum Rule<V: ValueInterface> {
    ReadOnly,
    Check(ValueCheck<V>),
}

struct Restricted<V: ValueInterface> {
    inner: Arc<dyn ValueAccessor<V>>,
    rule: Rule<V>,
}

impl<V: ValueInterface> Debug for Restricted<V> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let rule = match self.rule {
            Rule::ReadOnly => "read_only",
            Rule::Check(_) => "validated",
        };
        f.debug_struct("Restricted")
            .field("rule", &rule)
            .field("inner", &self.inner)
            .finish()
    }
}

impl<V: ValueInterface> Restricted<V> {
    fn admit(&self, value: &V) -> Result<(), Error> {
        match &self.rule {
            Rule::ReadOnly => Err(Error::not_supported(format!(
                "accessor '{}' is read-only",
                self.inner.address().encode()
            ))),
            Rule::Check(check) => check(value),
        }
    }
}

#[cfg_attr(not(target_arch = "wasm32"), async_trait::async_trait)]
#[cfg_attr(target_arch = "wasm32", async_trait::async_trait(?Send))]
impl<V: ValueInterface> ValueAccessor<V> for Restricted<V> {
    fn address(&self) -> &Query {
        self.inner.address()
    }
    async fn get(&self) -> Result<AccessorSnapshot<V>, Error> {
        self.inner.get().await
    }
    async fn set(&self, value: V, options: SetOptions) -> Result<AccessorSnapshot<V>, Error> {
        self.admit(&value)?;
        self.inner.set(value, options).await
    }
    async fn set_if(
        &self,
        value: V,
        expected: Option<Fingerprint>,
        options: SetOptions,
    ) -> Result<AccessorSnapshot<V>, Error> {
        self.admit(&value)?;
        self.inner.set_if(value, expected, options).await
    }
    async fn info(&self) -> Result<AccessorInfo, Error> {
        self.inner.info().await
    }
    async fn log(&self, since: usize) -> Result<Vec<LogEntry>, Error> {
        self.inner.log(since).await
    }
    async fn describe(&self) -> Result<PartDescription, Error> {
        let mut description = self.inner.describe().await?;
        if matches!(self.rule, Rule::ReadOnly) {
            description.read_only = true;
        }
        Ok(description)
    }
    fn capabilities(&self) -> Capabilities {
        let mut capabilities = self.inner.capabilities();
        if matches!(self.rule, Rule::ReadOnly) {
            capabilities.write = false;
        }
        capabilities
    }
    async fn refresh(&self) -> Result<(), Error> {
        self.inner.refresh().await
    }
    fn subscribe(&self) -> watch::Receiver<AccessorEvent> {
        self.inner.subscribe()
    }
    fn project(
        &self,
        step: Arc<dyn ProjectionStep<V>>,
        address: Query,
    ) -> Result<Arc<dyn ValueAccessor<V>>, Error> {
        match &self.rule {
            Rule::ReadOnly => Ok(read_only(self.inner.project(step, address)?)),
            Rule::Check(check) => Ok(Arc::new(WrapperProjection {
                parent: Arc::new(Restricted {
                    inner: self.inner.clone(),
                    rule: Rule::Check(check.clone()),
                }),
                step,
                address,
            })),
        }
    }
    fn set_invalidation_sink(&self, sink: Arc<dyn InvalidationSink>) {
        self.inner.set_invalidation_sink(sink)
    }
    fn driver(&self) -> Option<BoxFuture<'static, ()>> {
        self.inner.driver()
    }
}

/// A projection of an accessor that is not a hub node: it reads through the parent's `get` and
/// writes the whole parent value through the parent's `set_if`, so the parent's rules apply. Its
/// events are the parent's.
pub struct WrapperProjection<V: ValueInterface> {
    pub parent: Arc<dyn ValueAccessor<V>>,
    pub step: Arc<dyn ProjectionStep<V>>,
    pub address: Query,
}

impl<V: ValueInterface> Debug for WrapperProjection<V> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("WrapperProjection")
            .field("address", &self.address.encode())
            .field("step", &self.step)
            .finish()
    }
}

impl<V: ValueInterface> WrapperProjection<V> {
    fn part(&self, snap: &AccessorSnapshot<V>) -> Result<(V, Fingerprint), Error> {
        let parent = snap.state.value()?;
        let part = self
            .step
            .get(&parent)?
            .ok_or_else(|| unavailable(&self.address, "the part does not exist"))?;
        let anchor = self
            .step
            .anchor(&parent)?
            .ok_or_else(|| unavailable(&self.address, "the part does not exist"))?;
        let fingerprint = Fingerprint {
            anchor: stable_hash(&[&snap.fingerprint.anchor.to_le_bytes(), &anchor.to_le_bytes()]),
            content: content_version(&part, snap.root_version),
        };
        Ok((part, fingerprint))
    }

    async fn put(
        &self,
        value: V,
        expected: Option<Option<Fingerprint>>,
        options: SetOptions,
    ) -> Result<AccessorSnapshot<V>, Error> {
        let snap = self.parent.get().await?;
        let (_, current) = self.part(&snap)?;
        if let Some(expected) = expected {
            if expected != Some(current) {
                return Err(Error::accessor_conflict(&self.address, Some(&current)));
            }
        }
        let parent_value = snap.state.value()?;
        let new_parent = self.step.put(&parent_value, value)?;
        let written = self
            .parent
            .set_if(new_parent, Some(snap.fingerprint), options)
            .await?;
        let (part, fingerprint) = self.part(&written)?;
        Ok(AccessorSnapshot {
            state: State::from_value_and_metadata(part, Arc::new(Metadata::new())),
            fingerprint,
            root_version: written.root_version,
        })
    }
}

#[cfg_attr(not(target_arch = "wasm32"), async_trait::async_trait)]
#[cfg_attr(target_arch = "wasm32", async_trait::async_trait(?Send))]
impl<V: ValueInterface> ValueAccessor<V> for WrapperProjection<V> {
    fn address(&self) -> &Query {
        &self.address
    }
    async fn get(&self) -> Result<AccessorSnapshot<V>, Error> {
        let snap = self.parent.get().await?;
        let (part, fingerprint) = self.part(&snap)?;
        Ok(AccessorSnapshot {
            state: State::from_value_and_metadata(part, Arc::new(Metadata::new())),
            fingerprint,
            root_version: snap.root_version,
        })
    }
    async fn set(&self, value: V, options: SetOptions) -> Result<AccessorSnapshot<V>, Error> {
        self.put(value, None, options).await
    }
    async fn set_if(
        &self,
        value: V,
        expected: Option<Fingerprint>,
        options: SetOptions,
    ) -> Result<AccessorSnapshot<V>, Error> {
        self.put(value, Some(expected), options).await
    }
    async fn info(&self) -> Result<AccessorInfo, Error> {
        let mut info = self.parent.info().await?;
        info.fingerprint = match self.parent.get().await {
            Ok(snap) => self.part(&snap).ok().map(|(_, fp)| fp),
            Err(_) => None,
        };
        Ok(info)
    }
    async fn log(&self, since: usize) -> Result<Vec<LogEntry>, Error> {
        self.parent.log(since).await
    }
    async fn describe(&self) -> Result<PartDescription, Error> {
        let snap = self.parent.get().await?;
        let parent = snap.state.value()?;
        let mut description = self.step.describe(&parent)?;
        description.read_only = description.read_only || !self.capabilities().write;
        Ok(description)
    }
    fn capabilities(&self) -> Capabilities {
        self.parent.capabilities()
    }
    async fn refresh(&self) -> Result<(), Error> {
        self.parent.refresh().await
    }
    fn subscribe(&self) -> watch::Receiver<AccessorEvent> {
        self.parent.subscribe()
    }
    fn project(
        &self,
        step: Arc<dyn ProjectionStep<V>>,
        address: Query,
    ) -> Result<Arc<dyn ValueAccessor<V>>, Error> {
        Ok(Arc::new(WrapperProjection {
            parent: Arc::new(WrapperProjection {
                parent: self.parent.clone(),
                step: self.step.clone(),
                address: self.address.clone(),
            }),
            step,
            address,
        }))
    }
    fn set_invalidation_sink(&self, sink: Arc<dyn InvalidationSink>) {
        self.parent.set_invalidation_sink(sink)
    }
    fn driver(&self) -> Option<BoxFuture<'static, ()>> {
        self.parent.driver()
    }
}
