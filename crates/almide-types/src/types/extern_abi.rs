//! The `@extern(c, "lib", "fn")` signature ABI (#3054): which Almide types
//! cross a C boundary, and as what. ONE table, read by the checker (E090
//! rejects every type it has no row for, so `check` refuses what the build
//! cannot honor) and by the Rust renderer (`render_extern_c`, which spells the
//! `extern "C"` block and the conversions from it).
//!
//! `Int` crosses as an `int64_t` (`i64`, its own width — dialect epoch 15,
//! #4447: it used to be a C `int`, narrowed with `as`, which truncated every
//! value outside 32 bits) and `Bool` as an `int` (0/1). The sized ints and
//! floats cross at their own width. A `String` parameter crosses as a NUL-terminated copy
//! (`*const u8`, valid for the call; the text up to its first NUL). A
//! `Handle` crosses as the bare pointer it is. A `Region` parameter crosses
//! as TWO C arguments, the pointer and its `size_t` length, so the callee gets
//! the length the runtime holds, never one the caller typed (ADR-0031 D3); a
//! `Region` return is a pointer whose length and release function the
//! declaration states with `@region(len = …, release = "…")` — the checker
//! refuses the return without it (ADR-0031 D2). Other heap values (lists,
//! bytes, maps, records, …) have no C representation: bind them through an
//! `@extern(rust)` shim in `native/*.rs` instead.
//!
//! `@export(c, "sym")` reads the same rows in the other direction
//! ([`c_export_param_abi`], [`c_export_return_abi`], #4447, #4448): a C caller
//! passes the parameters and receives the return. Two rows have no reverse:
//! a `String` parameter's row is a copy the wrapper makes OF an Almide String
//! for a C callee, and a C caller's `const char*` carries no length and no
//! owner, so an exported fn cannot take one; and a `Region` return has no
//! release function the caller could free it with (ADR-0031 amendment A2).
//!
//! Gated by `tests/extern_rust_abi_test.rs` (every `Ty` variant × target) and
//! `tests/export_c_abi_test.rs` (every row × {fn, effect fn} for `@export(c)`).

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
    /// `Bool` ↔ C `int`: `1`/`0` out, `!= 0` back.
    BoolAsI32,
    /// `String` → a NUL-terminated copy (`CString`), passed as `*const u8`.
    /// Parameters only.
    CString,
    /// `Region` (ADR-0031): a parameter is two C arguments, `*mut u8` and
    /// `usize` (the region's base and its runtime-held length); a return is a
    /// `*mut u8` the wrapper builds a Region around from the declaration's
    /// `@region(len = …, release = "…")`.
    Region,
}

/// Is `ty` the `ffi` module's `Region` (ADR-0031)? The stdlib-owned bare name
/// is the type's identity: a user's own `type Region` takes the
/// module-qualified identity instead (#1828), so it never reaches here.
pub fn is_region(ty: &Ty) -> bool {
    matches!(ty, Ty::Named(n, args) if args.is_empty() && n.as_str() == crate::stdlib_info::FFI_REGION_TYPE)
}

const fn abi(c_ty: &'static str, conv: CConv) -> Option<CAbi> {
    Some(CAbi { c_ty, conv })
}

/// The scalar rows, shared by parameters and returns.
fn scalar(ty: &Ty) -> Option<CAbi> {
    match ty {
        Ty::Int => abi("i64", CConv::Same),
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
        t if is_region(t) => abi("*mut u8", CConv::Region),
        _ => scalar(ty),
    }
}

/// An `@extern(c)` return of type `ty`, or `None` when it has no C form. A
/// C string return has no owner to free it, so `String` is not one.
pub fn c_return_abi(ty: &Ty) -> Option<CAbi> {
    match ty {
        Ty::Unit => abi("()", CConv::Same),
        t if is_region(t) => abi("*mut u8", CConv::Region),
        _ => scalar(ty),
    }
}

/// An `@export(c)` parameter of type `ty` — what a C caller passes — or
/// `None` when it has no C form: [`c_param_abi`] without `String`, whose row
/// is the Almide-to-C copy and has no reverse (a `const char*` has no length
/// and no owner). A `Region` parameter is the caller's `(uint8_t*, size_t)`,
/// lent for the call (ADR-0031 amendment A2).
pub fn c_export_param_abi(ty: &Ty) -> Option<CAbi> {
    match ty {
        Ty::String => None,
        _ => c_param_abi(ty),
    }
}

/// An `@export(c)` return of type `ty` — what the C caller receives — or
/// `None` when it has no C form: [`c_return_abi`] without `Region`, which the
/// caller would have no release function for (ADR-0031 amendment A2). An
/// `effect fn`'s return is its declared ok type; the wrapper unwraps the
/// effect carrier first, and an `err` has no C form (C-452: the message goes
/// to stderr and the process exits 1).
pub fn c_export_return_abi(ty: &Ty) -> Option<CAbi> {
    if is_region(ty) {
        return None;
    }
    c_return_abi(ty)
}

/// The Almide types [`c_param_abi`] accepts, for diagnostics.
pub const C_PARAM_TYPES: &str = "Int, Int8..Int64, UInt8..UInt64, Float, Float32, Float64, Bool, String, Handle, Region";
/// The Almide types [`c_return_abi`] accepts, for diagnostics.
pub const C_RETURN_TYPES: &str = "Int, Int8..Int64, UInt8..UInt64, Float, Float32, Float64, Bool, Handle, Unit, and Region with `@region(len = …, release = \"…\")`";
/// The Almide types [`c_export_param_abi`] accepts, for diagnostics.
pub const C_EXPORT_PARAM_TYPES: &str = "Int, Int8..Int64, UInt8..UInt64, Float, Float32, Float64, Bool, Handle, Region";
/// The Almide types [`c_export_return_abi`] accepts, for diagnostics.
pub const C_EXPORT_RETURN_TYPES: &str = "Int, Int8..Int64, UInt8..UInt64, Float, Float32, Float64, Bool, Handle, Unit";

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
