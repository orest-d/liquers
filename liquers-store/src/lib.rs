//! OpenDAL-backed store backends for Liquers.
//!
//! The store configuration format and the factory/builder machinery live in `liquers-core`
//! ([`liquers_core::store_config`], [`liquers_core::store_factory`]); this crate contributes the
//! OpenDAL backends and the factory that builds them.
//!
//! Start from [`store_factory::default_store_factory`].

#[cfg(feature = "opendal")]
pub mod opendal_store;
pub mod store_factory;

pub use store_factory::{
    create_router, create_router_from_json, create_router_from_yaml, default_store_factory,
    get_opendal_scheme, is_opendal_store_type, OpendalStoreFactory, OPENDAL_STORE_TYPES,
};
