//! Self-tail-call LOOP CONVERSION — a contained peephole over the
//! ENCODED body (depth comes from parsing truth, not bookkeeping):
//! wrap the body in one `loop`, and rewrite every `return_call $self`
//! into `local.set` of the params (reverse order — the args are already
//! on the stack) plus `br` to the loop head. Semantics are unchanged
//! (same values, same termination, still constant stack); the win is
//! ~1.4ns/call of wasmtime tail-call overhead on hot self-recursion
//! (the stage-55 recursion kernel: 30M calls). Mutual recursion keeps
//! `return_call` untouched.

use wasm_encoder::reencode::{Reencode, RoundtripReencoder};
use wasm_encoder::{BlockType, Function, Instruction, ValType};

/// Rewrite `f` if it return_calls `self_idx`; None = no site (unchanged).
pub(crate) fn loop_convert(
    f: &Function,
    param_vts: &[ValType],
    ret: Option<ValType>,
    self_idx: u32,
) -> Option<Function> {
    let mut bytes = Vec::new();
    wasm_encoder::Encode::encode(f, &mut bytes);
    // Function::encode writes a code-section entry: LEB byte-length
    // FIRST, then locals + body — skip the length prefix.
    let mut off = 0;
    while bytes[off] & 0x80 != 0 {
        off += 1;
    }
    off += 1;
    let body = wasmparser::FunctionBody::new(wasmparser::BinaryReader::new(&bytes[off..], 0));
    let mut locals_reader = body.get_locals_reader().ok()?;
    let mut locals: Vec<(u32, ValType)> = Vec::new();
    for _ in 0..locals_reader.get_count() {
        let (n, t) = locals_reader.read().ok()?;
        locals.push((n, RoundtripReencoder.val_type(t).ok()?));
    }
    let mut found = false;
    {
        let mut ops = body.get_operators_reader().ok()?;
        while !ops.eof() {
            if let Ok(wasmparser::Operator::ReturnCall { function_index }) = ops.read()
                && function_index == self_idx
            {
                found = true;
                break;
            }
        }
    }
    if !found {
        return None;
    }

    let mut out = Function::new(locals);
    out.instructions().loop_(match ret {
        Some(vt) => BlockType::Result(vt),
        None => BlockType::Empty,
    });
    // depth = blocks currently open, INCLUDING our loop.
    let mut depth: u32 = 1;
    let mut ops = body.get_operators_reader().ok()?;
    while !ops.eof() {
        let op = ops.read().ok()?;
        match &op {
            wasmparser::Operator::Block { .. }
            | wasmparser::Operator::Loop { .. }
            | wasmparser::Operator::If { .. } => depth += 1,
            wasmparser::Operator::End => {
                if depth == 1 {
                    // the function's closing `end`: close our loop first.
                    out.instructions().end().end();
                    return Some(out);
                }
                depth -= 1;
            }
            wasmparser::Operator::ReturnCall { function_index }
                if *function_index == self_idx =>
            {
                for p in (0..param_vts.len() as u32).rev() {
                    out.instructions().local_set(p);
                }
                out.instructions().br(depth - 1);
                continue;
            }
            _ => {}
        }
        let inst: Instruction = RoundtripReencoder.instruction(op).ok()?;
        out.instruction(&inst);
    }
    None
}
