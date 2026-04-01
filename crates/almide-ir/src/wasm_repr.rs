/// WASM ABI type representation class.
/// Used by monomorphization to check whether a type substitution changes the WASM calling convention.

use almide_lang::types::Ty;

/// WASM value type classification (without depending on wasm-encoder).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum WasmRepr {
    I32,
    I64,
    F64,
    Void,
}

fn ty_wasm_repr(ty: &Ty) -> WasmRepr {
    match ty {
        Ty::Int => WasmRepr::I64,
        Ty::Float => WasmRepr::F64,
        Ty::Bool => WasmRepr::I32,
        Ty::Unit | Ty::Never => WasmRepr::Void,
        // All heap types (String, Bytes, Matrix, List, Record, etc.) use i32 pointers
        _ => WasmRepr::I32,
    }
}

/// Check if two types have the same WASM value representation.
pub fn wasm_types_compatible(a: &Ty, b: &Ty) -> bool {
    ty_wasm_repr(a) == ty_wasm_repr(b)
}
