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
pub mod matrix;
pub mod value;
pub mod datetime;
pub mod env;
pub mod error;
pub mod fan;
pub mod float;
pub mod fs;
pub mod http;
pub mod io;
pub mod json;
pub mod map;
pub mod math;
pub mod option;
pub mod process;
pub mod random;
pub mod regex;
pub mod result;
pub mod set;
pub mod testing;
