//! Almide Runtime Library
//!
//! Native implementations for stdlib @extern functions.
//! Each module corresponds to a stdlib core module.
//!
//! Naming convention: `almide_rt_{module}_{function}`
//! This crate is:
//! - Inlined into single-file output for `almide run`
//! - Added as dependency for `almide build`

pub mod int;
pub mod string;
pub mod list;
pub mod bytes;
pub mod value;
