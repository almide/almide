//! Almide type → WASM value type mapping and memory layout.

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
        Ty::Bytes => Some(ValType::I32),  // pointer to [len:i32][data:u8...]
        Ty::Unit => None,
        // All heap types (Record, Variant, List, etc.) use i32 pointers
        _ => Some(ValType::I32),
    }
}

/// Byte size of a type when stored in linear memory (inside a record/variant).
pub fn byte_size(ty: &Ty) -> u32 {
    match ty {
        Ty::Int => 8,    // i64
        Ty::Float => 8,  // f64
        Ty::Bool => 4,   // i32
        Ty::String => 4, // i32 pointer
        Ty::Bytes => 4,  // i32 pointer
        Ty::Unit => 0,
        _ => 4,          // i32 pointer for all heap types
    }
}

/// Compute byte offset and type for a field within a record.
/// Fields are laid out sequentially: [field0][field1][field2]...
pub fn field_offset(fields: &[(String, Ty)], target_field: &str) -> Option<(u32, Ty)> {
    let mut offset = 0u32;
    for (name, ty) in fields {
        if name == target_field {
            return Some((offset, ty.clone()));
        }
        offset += byte_size(ty);
    }
    None
}

/// Total byte size of a record (sum of all field sizes).
pub fn record_size(fields: &[(String, Ty)]) -> u32 {
    fields.iter().map(|(_, ty)| byte_size(ty)).sum()
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
