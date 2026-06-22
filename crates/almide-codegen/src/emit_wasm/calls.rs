//! Function call emission — emit_call and related helpers.

use almide_ir::{CallTarget, IrExpr};
use super::rt_string::{string_data_off, string_hdr, string_cap_off, list_data_off, list_hdr};
use almide_lang::types::Ty;
use wasm_encoder::ValType;

use super::FuncCompiler;
use super::values;

/// Signed / unsigned / float kind for sized numeric WASM conversion
/// dispatch. See `emit_sized_conv_call`.
#[derive(Copy, Clone, PartialEq, Eq)]
enum SizedKind { Int, UInt, Float }

/// Map a sized numeric `Ty` to its module name (`int32`, `uint8`, ...).
/// Mirrors `resolve_module_from_ty` / `builtin_module_for_type` so the
/// WASM dispatcher can route a `CallTarget::Method` on a sized
/// receiver into the same module dispatch path as `CallTarget::Module`.
fn sized_ty_module(ty: &Ty) -> Option<&'static str> {
    Some(match ty {
        Ty::Int => "int",
        Ty::Float => "float",
        Ty::Int8 => "int8",
        Ty::Int16 => "int16",
        Ty::Int32 => "int32",
        Ty::UInt8 => "uint8",
        Ty::UInt16 => "uint16",
        Ty::UInt32 => "uint32",
        Ty::UInt64 => "uint64",
        Ty::Float32 => "float32",
        _ => return None,
    })
}

/// Parse a sized-type module name into (kind, bit-width). Accepts the
/// canonical `int` / `float` (treated as 64-bit) plus every sized
/// variant. Returns `None` for anything else so the dispatcher falls
/// through to legacy TOML / bundled routing.
fn sized_type_info(name: &str) -> Option<(SizedKind, u32)> {
    Some(match name {
        "int" | "int64" => (SizedKind::Int, 64),
        "int32" => (SizedKind::Int, 32),
        "int16" => (SizedKind::Int, 16),
        "int8" => (SizedKind::Int, 8),
        "uint64" => (SizedKind::UInt, 64),
        "uint32" => (SizedKind::UInt, 32),
        "uint16" => (SizedKind::UInt, 16),
        "uint8" => (SizedKind::UInt, 8),
        "float" | "float64" => (SizedKind::Float, 64),
        "float32" => (SizedKind::Float, 32),
        _ => return None,
    })
}
use super::wasm_macro::wasm;

impl FuncCompiler<'_> {
    /// Fallback for module dispatch: when `emit_<module>_call` has no arm
    /// for `func` (often a mono-specialized `filter__Int_String`), try
    /// `func_map["almide_rt_<module>_<func>"]` — the name ResolveCalls
    /// would have produced had it rewritten the call. Returns `true` iff
    /// the call was successfully emitted. Mirrors the fallback chain
    /// already used by the unknown-module `_ =>` arm below, so inline
    /// dispatchers and bundled specializations converge on the same
    /// resolver instead of each module arm dying with an ICE.
    fn try_named_dispatch_fallback(&mut self, module: &str, func: &str, args: &[IrExpr]) -> bool {
        let prefixed = format!("almide_rt_{}_{}", module.replace('.', "_"), func.replace('.', "_"));
        if let Some(&func_idx) = self.emitter.func_map.get(prefixed.as_str()) {
            for arg in args { self.emit_expr(arg); }
            wasm!(self.func, { call(func_idx); });
            return true;
        }
        false
    }

    /// Resolve a `Type.method` call whose function was registered under its
    /// module_origin-qualified name `mod.Type.method` (#433 namespacing). The
    /// call site lost the owning-module prefix because the subject type carries
    /// no module, so suffix-match `func_map` for a UNIQUE key ending in
    /// `.Type.method` — that is the function. Returns None on no match OR on
    /// ambiguity (≥2 candidates), so the caller still ICEs rather than guessing
    /// (#609). The Rust target never reaches here: BuiltinLoweringPass already
    /// flattens `Type.method` to the prefixed name, but it is Rust-only.
    fn resolve_module_method(&self, module: &str, func: &str) -> Option<u32> {
        let needle = format!(".{}.{}", module, func);
        let mut hit: Option<u32> = None;
        for (k, &idx) in self.emitter.func_map.iter() {
            if k.ends_with(&needle) {
                if hit.is_some() { return None; } // ambiguous — do not guess
                hit = Some(idx);
            }
        }
        hit
    }
}

include!("calls_p2.rs");
include!("calls_p3.rs");
include!("calls_p4.rs");
