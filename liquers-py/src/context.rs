//! The execution context a Python command receives (step 8 of
//! `specs/design/liquers-py/phase4-implementation.md`).
//!
//! The hand-written `Environment` that used to live here implemented the core traits over
//! liquers-py's own value enum, with `todo!()` for its asset manager; it is replaced by
//! `crate::environment`, built on liquers-lib's default environment.
