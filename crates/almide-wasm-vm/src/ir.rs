//! The form a function body runs in. The validator translates each accepted
//! body into a flat `Vec<Instr>` in one pass (REQ-VM-3): every branch already
//! names its target index, the operand height to cut back to and whether one
//! value crosses, so the interpreter keeps no label stack and a branch costs
//! the same wherever it jumps.

use crate::numeric::{Binary, Unary};
use crate::types::ValType::{self, F64, I32, I64};

/// One flat enum for everything the hot loop executes: the interpreter's
/// dispatch is ONE match, so each instruction costs one indirect branch.
/// (Nesting the data and memory instructions under category variants was
/// measured at 1.5–1.8× slower on a hot integer loop — two dependent
/// branches per instruction.) Only the frame transfers nest, under `Call`:
/// they are rare next to the work between them.
#[derive(Clone, Copy, Debug)]
pub enum Instr {
    Unreachable,
    /// Jump to `target`; the operand stack is cut back to `height` (counted
    /// from the frame's operand base), with the top value kept when `keep`.
    Br { target: u32, height: u32, keep: bool },
    /// Pop an i32; when it is non-zero, the same as `Br`.
    BrIf { target: u32, height: u32, keep: bool },
    /// `if`: pop an i32; when it is zero, jump to the else arm (or the end).
    BrUnless { target: u32 },
    /// A transfer between frames.
    Call(Call),
    Drop,
    Select,
    LocalGet(u32),
    LocalSet(u32),
    LocalTee(u32),
    GlobalGet(u32),
    GlobalSet(u32),
    /// `op` indexes `LOADS`.
    Load { op: u8, offset: u32 },
    /// `op` indexes `STORES`.
    Store { op: u8, offset: u32 },
    MemorySize,
    MemoryGrow,
    MemoryCopy,
    MemoryFill,
    Const(u64),
    Unary(Unary),
    Binary(Binary),
}

#[derive(Clone, Copy, Debug)]
pub enum Call {
    Return,
    Call(u32),
    /// The operand is the expected type index.
    CallIndirect(u32),
    ReturnCall(u32),
    ReturnCallIndirect(u32),
}

pub struct LoadOp {
    pub code: u8,
    pub name: &'static str,
    pub bytes: u8,
    pub signed: bool,
    pub result: ValType,
}

pub struct StoreOp {
    pub code: u8,
    pub name: &'static str,
    pub bytes: u8,
    pub value: ValType,
}

const fn load(code: u8, name: &'static str, bytes: u8, signed: bool, result: ValType) -> LoadOp {
    LoadOp { code, name, bytes, signed, result }
}

const fn store(code: u8, name: &'static str, bytes: u8, value: ValType) -> StoreOp {
    StoreOp { code, name, bytes, value }
}

/// The loads of the closed set (REQ-VM-2). Absent on purpose: f32.load,
/// i32.load16_s — the emitter never produces them.
pub static LOADS: &[LoadOp] = &[
    load(0x28, "i32.load", 4, false, I32),
    load(0x29, "i64.load", 8, false, I64),
    load(0x2B, "f64.load", 8, false, F64),
    load(0x2C, "i32.load8_s", 1, true, I32),
    load(0x2D, "i32.load8_u", 1, false, I32),
    load(0x2F, "i32.load16_u", 2, false, I32),
    load(0x30, "i64.load8_s", 1, true, I64),
    load(0x31, "i64.load8_u", 1, false, I64),
    load(0x32, "i64.load16_s", 2, true, I64),
    load(0x33, "i64.load16_u", 2, false, I64),
    load(0x34, "i64.load32_s", 4, true, I64),
    load(0x35, "i64.load32_u", 4, false, I64),
];

/// The stores of the closed set. Absent: f32.store, i32.store16.
pub static STORES: &[StoreOp] = &[
    store(0x36, "i32.store", 4, I32),
    store(0x37, "i64.store", 8, I64),
    store(0x39, "f64.store", 8, F64),
    store(0x3A, "i32.store8", 1, I32),
    store(0x3C, "i64.store8", 1, I64),
    store(0x3D, "i64.store16", 2, I64),
    store(0x3E, "i64.store32", 4, I64),
];

/// The raw little-endian value `load` read, widened to a cell as its
/// instruction defines: sign- or zero-extended to the result width, and an
/// i32 result kept zero-extended in the cell.
pub fn widen(op: &LoadOp, raw: u64) -> u64 {
    let bits = u32::from(op.bytes) * 8;
    let value = if op.signed && bits < 64 {
        let shift = 64 - bits;
        (((raw << shift) as i64) >> shift) as u64
    } else {
        raw
    };
    match op.result {
        I32 => value & 0xffff_ffff,
        _ => value,
    }
}

/// How many instructions of each kind the closed set holds: the control and
/// variable instructions the validator handles by name, the memory rows
/// above, the three constants, and the numeric table. The allowlist test pins
/// the sum against the emitter's instruction inventory.
pub const CONTROL_AND_VARIABLE: &[&str] = &[
    "unreachable", "block", "loop", "if", "else", "end", "br", "br_if", "return", "call",
    "call_indirect", "return_call", "return_call_indirect", "drop", "select", "local.get",
    "local.set", "local.tee", "global.get", "global.set", "memory.size", "memory.grow",
    "memory.copy", "memory.fill", "i32.const", "i64.const", "f64.const",
];

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_instruction_is_two_words() {
        assert_eq!(std::mem::size_of::<Instr>(), 16, "the flat form stays two words");
    }

    #[test]
    fn loads_extend_as_their_instruction_says() {
        let op = |c: u8| LOADS.iter().find(|o| o.code == c).unwrap();
        assert_eq!(widen(op(0x2C), 0x80), 0xffff_ff80, "i32.load8_s keeps the cell 32 bits wide");
        assert_eq!(widen(op(0x2D), 0x80), 0x80);
        assert_eq!(widen(op(0x30), 0x80), (-128i64) as u64);
        assert_eq!(widen(op(0x34), 0x8000_0000), 0xffff_ffff_8000_0000);
        assert_eq!(widen(op(0x35), 0x8000_0000), 0x8000_0000);
    }
}
