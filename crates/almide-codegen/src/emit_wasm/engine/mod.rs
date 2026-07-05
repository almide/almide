//! Almide WASM Engine — type-safe WASM codegen toolkit.
//!
//! Two APIs:
//!
//! ## `WasmBuilder` (primary — use for new code and migration)
//!
//! Direct, layout-safe instruction emission via method chaining:
//! ```ignore
//! let mut w = WasmBuilder::new(&mut func, &reg);
//! w.get(list).field_load(LIST, list::LEN);
//! w.list_foreach(list, elem, idx, 4, |w| { /* body */ });
//! w.alloc_collection(LIST, len, 4, out, alloc_fn);
//! ```
//!
//! ## `Op` IR + `emit_ops` (secondary — for future optimization passes)
//!
//! Structured IR that can be transformed before emission:
//! ```ignore
//! let ops = vec![Op::FieldLoad { layout: STRING, field: LEN, kind: I32 }];
//! emit_ops(&ops, &mut func, &reg);
//! ```

pub mod layout;
pub mod ir;
pub mod emit;
pub mod builder;

pub use layout::LayoutRegistry;
pub use builder::WasmBuilder;
pub use ir::verify_func_stack;
