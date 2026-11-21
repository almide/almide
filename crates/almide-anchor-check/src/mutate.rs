//! Probe only: single-byte mutants of the instructions the anchors name.
//!
//! For every instruction a located witness event names, and the few
//! instructions before it (its operands), each applicable same-length
//! change is one mutant: a load's or store's static offset moved 8 bytes,
//! an unsigned compare loosened by one, a small constant (an allocation
//! size, a stride) lowered by 4, a local read swapped for its neighbour.
//! A mutant the validator refuses is INVALID; one the check refuses is
//! KILLED; one the check accepts SURVIVED.

use std::collections::HashSet;

use wasmparser::{Operator as Op, Parser, Payload};

use crate::At;

/// One mutant: where, what kind, and the byte change.
pub struct Mutant {
    pub at: At,
    pub kind: &'static str,
    pub pos: usize,
    pub to: u8,
    /// The witness naming the located instruction this mutant belongs to.
    pub src: &'static str,
    /// 0 when the mutated instruction is the located one itself; otherwise
    /// how many instructions before it.
    pub dist: usize,
    /// The mutated instruction, as decoded.
    pub what: String,
    /// What the mutated instruction is to the located one: `self`, a
    /// producer of its address (`addr`), of a store's value (`value`), of
    /// another operand (`cond`: a compare's or an `if`'s), or `other`.
    pub role: &'static str,
}

/// Every instruction `k`'s operand `arg` is computed from, transitively.
fn producers(args: &[Vec<Option<usize>>], k: usize, arg: usize) -> HashSet<usize> {
    let mut out = HashSet::new();
    let mut todo: Vec<usize> = args.get(k).and_then(|a| a.get(arg)).copied().flatten().into_iter().collect();
    while let Some(p) = todo.pop() {
        if out.insert(p) {
            todo.extend(args.get(p).into_iter().flatten().flatten().copied());
        }
    }
    out
}

/// How many instructions before a located one are mutated with it.
const BACK: usize = 4;

fn memarg_mutant(wasm: &[u8], off: usize, offset: u64) -> Option<(usize, u8)> {
    // opcode, align (one byte), offset (one byte): only the plain encoding.
    let p = off + 2;
    (offset < 120 && wasm.get(off + 1).is_some_and(|a| *a < 0x40) && wasm.get(p).copied() == Some(offset as u8)).then(|| (p, offset as u8 + 8))
}

fn one(wasm: &[u8], op: &Op<'_>, off: usize) -> Option<(&'static str, usize, u8)> {
    let m = |k: &'static str, r: Option<(usize, u8)>| r.map(|(p, b)| (k, p, b));
    match op {
        Op::I32Load { memarg } | Op::I64Load { memarg } | Op::F64Load { memarg } | Op::F32Load { memarg } | Op::I32Load8U { memarg } | Op::I64Load8U { memarg } => {
            m("load-offset", memarg_mutant(wasm, off, memarg.offset))
        }
        Op::I32Store { memarg } | Op::I64Store { memarg } | Op::F64Store { memarg } | Op::F32Store { memarg } | Op::I32Store8 { memarg } => {
            m("store-offset", memarg_mutant(wasm, off, memarg.offset))
        }
        Op::I32GeU => (wasm.get(off).copied() == Some(0x4f)).then_some(("compare", off, 0x4b)),
        Op::I64GeU => (wasm.get(off).copied() == Some(0x5a)).then_some(("compare", off, 0x56)),
        Op::I32LtU => (wasm.get(off).copied() == Some(0x49)).then_some(("compare", off, 0x4d)),
        Op::I64LtU => (wasm.get(off).copied() == Some(0x54)).then_some(("compare", off, 0x58)),
        Op::I32Const { value } if (8..60).contains(value) && wasm.get(off + 1).copied() == Some(*value as u8) => Some(("const", off + 1, (*value - 4) as u8)),
        Op::LocalGet { local_index } if *local_index < 120 && wasm.get(off + 1).copied() == Some(*local_index as u8) => {
            Some(("local", off + 1, (*local_index as u8) ^ 1))
        }
        _ => None,
    }
}

/// Every mutant of `wasm` at and just before the instructions in `located`.
pub fn mutants(wasm: &[u8], located: &std::collections::HashMap<At, &'static str>) -> Result<Vec<Mutant>, String> {
    let mut out = Vec::new();
    let bodies = crate::decode(wasm)?;
    let mut func = 0usize;
    for payload in Parser::new(0).parse_all(wasm) {
        let Payload::CodeSectionEntry(body) = payload.map_err(|e| e.to_string())? else { continue };
        let mut ops = Vec::new();
        let mut r = body.get_operators_reader().map_err(|e| e.to_string())?;
        while !r.eof() {
            let off = r.original_position();
            ops.push((r.read().map_err(|e| e.to_string())?, off));
        }
        // Each instruction belongs to the nearest located one at or after it.
        let args: &[Vec<Option<usize>>] = bodies.get(func).map(|b| b.args.as_slice()).unwrap_or(&[]);
        let mut want: std::collections::BTreeMap<usize, (&'static str, usize, &'static str)> = std::collections::BTreeMap::new();
        for k in 0..ops.len() {
            if let Some(src) = located.get(&At { func, op: k }) {
                let is_store = crate::store_width(&ops[k].0).is_some();
                let is_mem = is_store || crate::decode::is_load(&ops[k].0);
                let addr = producers(args, k, 0);
                let value = if is_store { producers(args, k, 1) } else { HashSet::new() };
                for j in k.saturating_sub(BACK)..=k {
                    let d = k - j;
                    let role = if d == 0 {
                        "self"
                    } else if addr.contains(&j) {
                        if is_mem { "addr" } else { "cond" }
                    } else if value.contains(&j) {
                        "value"
                    } else {
                        "other"
                    };
                    if want.get(&j).is_none_or(|(_, old, _)| d < *old) {
                        want.insert(j, (*src, d, role));
                    }
                }
            }
        }
        for (k, (src, dist, role)) in want {
            let (op, off) = &ops[k];
            if let Some((kind, pos, to)) = one(wasm, op, usize::try_from(*off).unwrap_or(usize::MAX)) {
                out.push(Mutant { at: At { func, op: k }, kind, pos, to, src, dist, what: format!("{op:?}"), role });
            }
        }
        func += 1;
    }
    Ok(out)
}
