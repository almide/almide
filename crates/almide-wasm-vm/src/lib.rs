//! almide-wasm-vm: a wasm interpreter for exactly the instruction set
//! Almide's structural emitter produces (#865). It runs the shipped
//! `to_wasi` artifact of a Critical-profile program — the same bytes a stock
//! runtime runs — with a closed instruction allowlist, a load-time validator
//! that refuses everything outside it, fixed-capacity stacks, no allocation
//! after instantiation, and fuel. The requirements it is built and tested
//! against are REQUIREMENTS.md, one `REQ-VM-N` per clause.

pub mod error;
pub mod ir;
pub mod module;
pub mod numeric;
pub mod reader;
pub mod types;
pub mod validate;

pub use error::{LoadError, Trap};
pub use module::{decode, Module};
