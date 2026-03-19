//! Almide type → WASM value type mapping.

use crate::types::Ty;
use wasm_encoder::{BlockType, ValType};

/// Map an Almide type to a WASM value type.
/// Returns None for Unit (which has no WASM representation).
pub fn ty_to_valtype(ty: &Ty) -> Option<ValType> {
    match ty {
        Ty::Int => Some(ValType::I64),
        Ty::Float => Some(ValType::F64),
        Ty::Bool => Some(ValType::I32),
        Ty::String => Some(ValType::I32), // pointer to [len:i32][data:u8...]
        Ty::Unit => None,
        // Phase 1: all heap types use i32 pointers
        _ => Some(ValType::I32),
    }
}

/// Return type as a Vec<ValType> (empty for Unit).
pub fn ret_type(ty: &Ty) -> Vec<ValType> {
    ty_to_valtype(ty).into_iter().collect()
}

/// WASM block type for if/else and other structured blocks.
pub fn block_type(ty: &Ty) -> BlockType {
    match ty_to_valtype(ty) {
        Some(vt) => BlockType::Result(vt),
        None => BlockType::Empty,
    }
}
