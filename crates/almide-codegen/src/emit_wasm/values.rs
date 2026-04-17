//! Almide type → WASM value type mapping and memory layout.

use almide_lang::types::Ty;
use wasm_encoder::{BlockType, ValType};

/// Map an Almide type to a WASM value type.
/// Returns None for Unit (which has no WASM representation).
pub fn ty_to_valtype(ty: &Ty) -> Option<ValType> {
    match ty {
        Ty::Int => Some(ValType::I64),
        Ty::Float => Some(ValType::F64),
        // Sized numeric types. WASM only has i32/i64/f32/f64 natively;
        // narrower Almide widths ride in the next-wider WASM type and
        // get masked/sign-extended at memory boundaries (handled by
        // subsequent sub-phases when load/store sites appear).
        Ty::Int8 | Ty::Int16 | Ty::Int32
        | Ty::UInt8 | Ty::UInt16 | Ty::UInt32 => Some(ValType::I32),
        Ty::UInt64 => Some(ValType::I64),
        Ty::Float32 => Some(ValType::F32),
        Ty::Bool => Some(ValType::I32),
        Ty::String => Some(ValType::I32), // pointer to [len:i32][data:u8...]
        Ty::Bytes => Some(ValType::I32),  // pointer to [len:i32][data:u8...]
        Ty::Matrix => Some(ValType::I32), // pointer to heap-allocated matrix
        Ty::Unit | Ty::Never => None,
        // All heap types (Record, Variant, List, etc.) use i32 pointers
        _ => Some(ValType::I32),
    }
}

/// Byte size of a type when stored in linear memory (inside a record/variant).
pub fn byte_size(ty: &Ty) -> u32 {
    match ty {
        Ty::Int => 8,    // i64
        Ty::Float => 8,  // f64
        Ty::Int8 | Ty::UInt8 => 1,
        Ty::Int16 | Ty::UInt16 => 2,
        Ty::Int32 | Ty::UInt32 | Ty::Float32 => 4,
        Ty::UInt64 => 8,
        Ty::Bool => 4,   // i32
        Ty::String => 4, // i32 pointer
        Ty::Bytes => 4,  // i32 pointer
        Ty::Matrix => 4, // i32 pointer
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

/// Build a representative `Ty` for a given `ValType`, used when we only know
/// the WASM ABI shape (from a lifted closure's registered type) but not the
/// source-level type. The returned `Ty` round-trips correctly through
/// `ty_to_valtype`/`byte_size`.
pub fn vt_to_placeholder_ty(vt: ValType) -> Ty {
    match vt {
        ValType::I64 => Ty::Int,
        ValType::F64 => Ty::Float,
        // All i32 heap pointers have identical runtime layout (4-byte pointer);
        // `Ty::String` is a convenient stand-in.
        _ => Ty::String,
    }
}
