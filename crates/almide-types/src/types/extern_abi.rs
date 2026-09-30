//! The `@extern(c, "lib", "fn")` signature ABI (#3054): which Almide types
//! cross a C boundary, and as what. ONE table, read by the checker (E090
//! rejects every type it has no row for, so `check` refuses what the build
//! cannot honor) and by the Rust renderer (`render_extern_c`, which spells the
//! `extern "C"` block and the conversions from it).
//!
//! `Int` crosses as a C `int` (`i32`, narrowed with `as`) and `Bool` as an
//! `int` (0/1) — the mapping `@extern(c)` has always had, kept so existing
//! bindings keep their C prototypes. The sized ints and floats cross at their
//! own width. A `String` parameter crosses as a NUL-terminated copy
//! (`*const u8`, valid for the call; the text up to its first NUL). Heap
//! values (lists, bytes, maps, records, …) have no C representation: bind
//! them through an `@extern(rust)` shim in `native/*.rs` instead.
//!
//! Gated by `tests/extern_rust_abi_test.rs` (every `Ty` variant × target).

use super::Ty;

/// How one Almide value crosses the C boundary.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CAbi {
    /// The type spelled in the `extern "C"` block.
    pub c_ty: &'static str,
    pub conv: CConv,
}

/// The conversion between the Almide value and the C one.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CConv {
    /// Same representation on both sides.
    Same,
    /// `Int` (`i64`) ↔ C `int` (`i32`): `as` both ways.
    IntAsI32,
    /// `Bool` ↔ C `int`: `1`/`0` out, `!= 0` back.
    BoolAsI32,
    /// `String` → a NUL-terminated copy (`CString`), passed as `*const u8`.
    /// Parameters only.
    CString,
}

const fn abi(c_ty: &'static str, conv: CConv) -> Option<CAbi> {
    Some(CAbi { c_ty, conv })
}

/// The scalar rows, shared by parameters and returns.
fn scalar(ty: &Ty) -> Option<CAbi> {
    match ty {
        Ty::Int => abi("i32", CConv::IntAsI32),
        Ty::Int8 => abi("i8", CConv::Same),
        Ty::Int16 => abi("i16", CConv::Same),
        Ty::Int32 => abi("i32", CConv::Same),
        Ty::Int64 => abi("i64", CConv::Same),
        Ty::UInt8 => abi("u8", CConv::Same),
        Ty::UInt16 => abi("u16", CConv::Same),
        Ty::UInt32 => abi("u32", CConv::Same),
        Ty::UInt64 => abi("u64", CConv::Same),
        Ty::Float | Ty::Float64 => abi("f64", CConv::Same),
        Ty::Float32 => abi("f32", CConv::Same),
        Ty::Bool => abi("i32", CConv::BoolAsI32),
        Ty::RawPtr => abi("*mut u8", CConv::Same),
        _ => None,
    }
}

/// An `@extern(c)` parameter of type `ty`, or `None` when it has no C form.
pub fn c_param_abi(ty: &Ty) -> Option<CAbi> {
    match ty {
        Ty::String => abi("*const u8", CConv::CString),
        _ => scalar(ty),
    }
}

/// An `@extern(c)` return of type `ty`, or `None` when it has no C form. A
/// C string return has no owner to free it, so `String` is not one.
pub fn c_return_abi(ty: &Ty) -> Option<CAbi> {
    match ty {
        Ty::Unit => abi("()", CConv::Same),
        _ => scalar(ty),
    }
}

/// The Almide types [`c_param_abi`] accepts, for diagnostics.
pub const C_PARAM_TYPES: &str = "Int, Int8..Int64, UInt8..UInt64, Float, Float32, Float64, Bool, String, RawPtr";
/// The Almide types [`c_return_abi`] accepts, for diagnostics.
pub const C_RETURN_TYPES: &str = "Int, Int8..Int64, UInt8..UInt64, Float, Float32, Float64, Bool, RawPtr, Unit";

/// The two native binding kinds an `@extern` target can name.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NativeExtern {
    /// `@extern(rust, "path", "fn")` — also spelled `rs`. The parameter ABI is
    /// `extern_rust_borrow_mode` in almide-codegen (docs/specs/module-system.md §11.1).
    Rust,
    /// `@extern(c, "lib", "fn")` — the table above.
    C,
}

/// The native binding kind `target` names, if any. The native build binds a
/// fn through the FIRST of its `@extern` attrs that names one (other targets —
/// `wasm`, `ts` — are other legs' business).
pub fn native_extern_kind(target: &str) -> Option<NativeExtern> {
    match target {
        "rust" | "rs" => Some(NativeExtern::Rust),
        "c" => Some(NativeExtern::C),
        _ => None,
    }
}
