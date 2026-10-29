//! Decoding the shipped module: every defined function's instructions, with
//! the control depth and store-address provenance the checks read.

use wasmparser::{Operator as Op, Parser, ValidPayload, Validator, WasmFeatures};

/// One defined function's instructions, as the checker needs them.
pub struct Body<'a> {
    pub ops: Vec<Op<'a>>,
    /// Control depth before each instruction (0 = the function's own frame).
    pub depth: Vec<u32>,
    /// For a store: the instruction that pushed its address operand, when
    /// the provenance walk knows it.
    pub addr: Vec<Option<usize>>,
}

/// Every defined function of `bytes`, validated, in position order.
pub fn decode(bytes: &[u8]) -> Result<Vec<Body<'_>>, String> {
    let mut v = Validator::new_with_features(WasmFeatures::all());
    let mut out = Vec::new();
    for payload in Parser::new(0).parse_all(bytes) {
        let payload = payload.map_err(|e| e.to_string())?;
        if let ValidPayload::Func(func, body) = v.payload(&payload).map_err(|e| e.to_string())? {
            let mut fv = func.into_validator(Default::default());
            let mut br = body.get_binary_reader();
            fv.read_locals(&mut br).map_err(|e| e.to_string())?;
            let mut r = wasmparser::OperatorsReader::new(br);
            let (mut ops, mut depth, mut addr) = (Vec::new(), Vec::new(), Vec::new());
            let mut shadow: Vec<Option<usize>> = Vec::new();
            let mut d = 0u32;
            while !r.eof() {
                let off = r.original_position();
                let op = r.read().map_err(|e| e.to_string())?;
                let i = ops.len();
                let pre = fv.operand_stack_height() as usize;
                let arity = op.operator_arity(&fv);
                addr.push(if store_width(&op).is_some() && pre >= 2 {
                    shadow.get(pre - 2).copied().flatten()
                } else {
                    None
                });
                depth.push(d);
                fv.op(off, &op).map_err(|e| e.to_string())?;
                let post = fv.operand_stack_height() as usize;
                match arity {
                    Some((k, m)) if pre >= k as usize => {
                        let keep = (pre - k as usize).min(post);
                        shadow.truncate(keep);
                        // Pushed by this instruction (or, past a polymorphic
                        // stack, re-derived by it): its own.
                        while shadow.len() < post {
                            shadow.push(Some(i));
                        }
                        let _ = m;
                    }
                    _ => {
                        shadow.iter_mut().for_each(|s| *s = None);
                        shadow.truncate(post);
                        while shadow.len() < post {
                            shadow.push(None);
                        }
                    }
                }
                match op {
                    Op::Block { .. } | Op::Loop { .. } | Op::If { .. } | Op::TryTable { .. } => {
                        d += 1
                    }
                    Op::End => d = d.saturating_sub(1),
                    _ => {}
                }
                ops.push(op);
            }
            out.push(Body { ops, depth, addr });
        }
    }
    Ok(out)
}

/// A store's width in bytes and its static offset.
pub fn store_width(op: &Op<'_>) -> Option<(u32, u64)> {
    Some(match op {
        Op::I32Store { memarg } | Op::F32Store { memarg } | Op::I64Store32 { memarg } => {
            (4, memarg.offset)
        }
        Op::I64Store { memarg } | Op::F64Store { memarg } => (8, memarg.offset),
        Op::I32Store8 { memarg } | Op::I64Store8 { memarg } => (1, memarg.offset),
        Op::I32Store16 { memarg } | Op::I64Store16 { memarg } => (2, memarg.offset),
        Op::V128Store { memarg } => (16, memarg.offset),
        _ => return None,
    })
}

pub(crate) fn is_load(op: &Op<'_>) -> bool {
    matches!(
        op,
        Op::I32Load { .. }
            | Op::I64Load { .. }
            | Op::F32Load { .. }
            | Op::F64Load { .. }
            | Op::I32Load8S { .. }
            | Op::I32Load8U { .. }
            | Op::I32Load16S { .. }
            | Op::I32Load16U { .. }
            | Op::I64Load8S { .. }
            | Op::I64Load8U { .. }
            | Op::I64Load16S { .. }
            | Op::I64Load16U { .. }
            | Op::I64Load32S { .. }
            | Op::I64Load32U { .. }
            | Op::V128Load { .. }
    )
}

/// A branch's target depth (the frame it leaves to), or `Some(0)` for an
/// exit of the whole function; `None` for an instruction that does not
/// branch.
pub(crate) fn branch_target(op: &Op<'_>, d: u32) -> Option<u32> {
    match op {
        Op::Br { relative_depth }
        | Op::BrIf { relative_depth }
        | Op::BrOnNull { relative_depth }
        | Op::BrOnNonNull { relative_depth } => Some(d.saturating_sub(*relative_depth)),
        Op::BrTable { targets } => {
            let max = targets
                .targets()
                .filter_map(Result::ok)
                .chain(std::iter::once(targets.default()))
                .max()
                .unwrap_or(0);
            Some(d.saturating_sub(max))
        }
        Op::Return
        | Op::ReturnCall { .. }
        | Op::ReturnCallIndirect { .. }
        | Op::ReturnCallRef { .. }
        | Op::Throw { .. }
        | Op::ThrowRef
        | Op::Rethrow { .. } => Some(0),
        _ => None,
    }
}
