//! Value and function types. An f32 exists only transiently on the operand
//! stack (a demote feeding a reinterpret, as the emitter's float printer
//! does); every DECLARED type — a param, a result, a local, a global — is
//! i32, i64 or f64, and a function returns at most one value (REQ-VM-2).

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum ValType {
    I32,
    I64,
    F32,
    F64,
}

impl ValType {
    /// A declared type's encoding: f32 is not one the emitter declares.
    pub fn declared(byte: u8) -> Option<ValType> {
        match byte {
            0x7F => Some(ValType::I32),
            0x7E => Some(ValType::I64),
            0x7C => Some(ValType::F64),
            _ => None,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct FuncType {
    pub params: Vec<ValType>,
    /// Zero or one value.
    pub results: Vec<ValType>,
}
