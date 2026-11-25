//! Guarded-access WINDOWS: the cutter for `proofs/GuardWindow.v`.
//!
//! A window is a straight-line run of one function's shipped instructions
//! that ends in a load or a store: the compare, its aborting arm, the
//! address arithmetic and the access, with nothing in between that writes
//! memory, calls, or branches (an arm `if ... unreachable end` aside). The
//! proof script decodes a window's bytes itself, runs them symbolically and
//! accepts when the access is `base + 12 + idx * S + o` under a compare that
//! left `idx <u len(base) / S`; its theorem (`window_sound`) then puts the
//! access inside that block's payload in every run, with the machine's own
//! 32- and 64-bit wrap-around. No witness and no anchor is read: the sweep
//! below visits EVERY load and store of the module.
//!
//! This file only CUTS. It is a search, not a judge: [`propose`] runs the
//! same symbolic reading to find where a window may start, so that the
//! kernel is asked about windows that have a chance and not about a quarter
//! of a million that have none, and the verdict on each is the kernel's
//! (`proofs/check-shipped-windows.sh`). A window proposed here and refused
//! there fails the gate, so the two readings cannot drift apart silently.
//!
//! TRUSTED of this file: that the bytes it prints for a window are the
//! bytes `[lo, hi)` of the module, and that `lo` and `hi` are instruction
//! boundaries of ONE function body at one control depth (the offsets are
//! wasmparser's, the depth this crate's decoder's). What the window's
//! instructions mean is the proof script's business, not this file's.

use std::collections::HashMap;

use wasmparser::{BlockType, Operator as Op};

use crate::decode::{arm_branches_out, store_width, Body};

/// A value as an expression in what the window found on entry.
#[derive(Clone, Debug, PartialEq, Eq)]
enum Tm {
    /// A local's value when the window is entered.
    L(u32),
    /// A global's value (nothing in a window sets one).
    G(u32),
    K(i128),
    /// A load `.0` at `.2 + .1` (memory does not change inside a window).
    M(&'static str, u64, Box<Tm>),
    U(&'static str, Box<Tm>),
    B(&'static str, Box<Tm>, Box<Tm>),
}

/// The instructions the proof script gives a meaning, by the name the
/// recognizer below matches on.
fn modelled(op: &Op<'_>) -> Option<(usize, &'static str)> {
    Some(match op {
        Op::I64ExtendI32U => (1, "ext_u"),
        Op::I32WrapI64 => (1, "wrap"),
        Op::I32Add => (2, "add32"),
        Op::I32Mul => (2, "mul32"),
        Op::I32DivU => (2, "divu32"),
        Op::I32Shl => (2, "shl32"),
        Op::I32ShrU => (2, "shru32"),
        Op::I32GeU => (2, "geu32"),
        Op::I64GeU => (2, "geu64"),
        _ => return None,
    })
}

fn name(op: &Op<'_>) -> String {
    let s = format!("{op:?}");
    s.split([' ', '{', '(']).next().unwrap_or("?").to_string()
}

/// Any other numeric instruction of the one-byte opcode space (0x45..0xc4):
/// some function of its operands, by arity. The proof script gives these no
/// meaning, so the name only has to tell two of them apart.
fn opaque(op: &Op<'_>) -> Option<(usize, &'static str)> {
    let n = name(op);
    let numeric = ["I32", "I64", "F32", "F64"].iter().any(|p| n.starts_with(p));
    let excluded = ["Load", "Store", "Const", "Atomic", "TruncSat"].iter().any(|p| n.contains(p));
    if !numeric || excluded {
        return None;
    }
    const UNARY: &[&str] = &[
        "Eqz", "Clz", "Ctz", "Popcnt", "Abs", "Neg", "Ceil", "Floor", "Trunc", "Nearest", "Sqrt", "WrapI64", "ExtendI32S", "ExtendI32U", "Extend8S",
        "Extend16S", "Extend32S", "TruncF32S", "TruncF32U", "TruncF64S", "TruncF64U", "ConvertI32S", "ConvertI32U", "ConvertI64S", "ConvertI64U",
        "DemoteF64", "PromoteF32", "ReinterpretF32", "ReinterpretF64", "ReinterpretI32", "ReinterpretI64",
    ];
    let arity = if UNARY.iter().any(|u| &n[3..] == *u) { 1 } else { 2 };
    Some((arity, Box::leak(n.into_boxed_str())))
}

/// A load's name, width in bytes and memarg offset.
fn load(op: &Op<'_>) -> Option<(&'static str, u32, u64)> {
    Some(match op {
        Op::I32Load { memarg } => ("i32", 4, memarg.offset),
        Op::I64Load { memarg } => ("i64", 8, memarg.offset),
        Op::F32Load { memarg } => ("f32", 4, memarg.offset),
        Op::F64Load { memarg } => ("f64", 8, memarg.offset),
        Op::I32Load8U { memarg } => ("i32_8u", 1, memarg.offset),
        Op::I32Load8S { memarg } => ("i32_8s", 1, memarg.offset),
        Op::I32Load16U { memarg } => ("i32_16u", 2, memarg.offset),
        Op::I32Load16S { memarg } => ("i32_16s", 2, memarg.offset),
        Op::I64Load8U { memarg } => ("i64_8u", 1, memarg.offset),
        Op::I64Load8S { memarg } => ("i64_8s", 1, memarg.offset),
        Op::I64Load16U { memarg } => ("i64_16u", 2, memarg.offset),
        Op::I64Load16S { memarg } => ("i64_16s", 2, memarg.offset),
        Op::I64Load32U { memarg } => ("i64_32u", 4, memarg.offset),
        Op::I64Load32S { memarg } => ("i64_32s", 4, memarg.offset),
        _ => return None,
    })
}

/// Whether `op` reads or writes memory at an address operand (bulk copies
/// and fills are not windows).
fn is_access(op: &Op<'_>) -> bool {
    load(op).is_some() || store_width(op).is_some_and(|(w, _)| w <= 8)
}

enum Stop {
    /// The window starts too late: an instruction pops what it did not push.
    Underflow,
    /// An instruction no window may hold, wherever it starts.
    Blocked,
}

struct Run {
    facts: Vec<Tm>,
    addr: Tm,
    off: u64,
    width: u32,
}

fn pop(stack: &mut Vec<Tm>) -> Result<Tm, Stop> {
    stack.pop().ok_or(Stop::Underflow)
}

/// Run `ops[s..=acc]` from an empty stack; `acc` is the access.
fn run(b: &Body<'_>, s: usize, acc: usize) -> Result<Run, Stop> {
    let d = b.depth[acc];
    let mut stack: Vec<Tm> = Vec::new();
    let mut env: HashMap<u32, Tm> = HashMap::new();
    let mut facts: Vec<Tm> = Vec::new();
    let mut j = s;
    while j < acc {
        let op = &b.ops[j];
        if b.depth[j] != d {
            return Err(Stop::Blocked);
        }
        if let Op::If { blockty } = op {
            // The aborting arm: no result, no `else`, ends in `unreachable`,
            // and nothing in it branches out. Past it the condition was zero.
            if !matches!(blockty, BlockType::Empty) {
                return Err(Stop::Blocked);
            }
            let end = (j + 1..b.ops.len()).find(|&k| b.depth[k] == d + 1 && matches!(b.ops[k], Op::End | Op::Else)).ok_or(Stop::Blocked)?;
            if !matches!(b.ops[end], Op::End) || end >= acc || !matches!(b.ops[end - 1], Op::Unreachable) || arm_branches_out(&b.ops, &b.depth, j, end).is_some() {
                return Err(Stop::Blocked);
            }
            // What the proof script's arm reader takes before the abort:
            // constants, local and global reads and writes, calls, drops,
            // numeric instructions, loads and stores. Nothing that opens a
            // block, branches or returns.
            let plain = |o: &Op<'_>| {
                matches!(
                    o,
                    Op::I32Const { .. }
                        | Op::I64Const { .. }
                        | Op::LocalGet { .. }
                        | Op::LocalSet { .. }
                        | Op::LocalTee { .. }
                        | Op::GlobalGet { .. }
                        | Op::GlobalSet { .. }
                        | Op::Call { .. }
                        | Op::Drop
                ) || is_access(o)
                    || modelled(o).is_some()
                    || opaque(o).is_some()
            };
            if !b.ops[j + 1..end - 1].iter().all(plain) {
                return Err(Stop::Blocked);
            }
            facts.push(pop(&mut stack)?);
            j = end + 1;
            continue;
        }
        match op {
            Op::LocalGet { local_index } => stack.push(env.get(local_index).cloned().unwrap_or(Tm::L(*local_index))),
            Op::LocalSet { local_index } => {
                let v = pop(&mut stack)?;
                env.insert(*local_index, v);
            }
            Op::LocalTee { local_index } => {
                let v = stack.last().cloned().ok_or(Stop::Underflow)?;
                env.insert(*local_index, v);
            }
            Op::GlobalGet { global_index } => stack.push(Tm::G(*global_index)),
            Op::I32Const { value } => stack.push(Tm::K(i128::from(*value as u32))),
            Op::I64Const { value } => stack.push(Tm::K(i128::from(*value as u64))),
            Op::Drop => {
                pop(&mut stack)?;
            }
            _ => {
                if let Some((k, _, off)) = load(op) {
                    let a = pop(&mut stack)?;
                    stack.push(Tm::M(k, off, Box::new(a)));
                } else if let Some((arity, n)) = modelled(op).or_else(|| opaque(op)) {
                    if arity == 1 {
                        let a = pop(&mut stack)?;
                        stack.push(Tm::U(n, Box::new(a)));
                    } else {
                        let y = pop(&mut stack)?;
                        let x = pop(&mut stack)?;
                        stack.push(Tm::B(n, Box::new(x), Box::new(y)));
                    }
                } else {
                    return Err(Stop::Blocked);
                }
            }
        }
        j += 1;
    }
    if j != acc {
        return Err(Stop::Blocked);
    }
    let op = &b.ops[acc];
    if let Some((_, width, off)) = load(op) {
        let addr = pop(&mut stack)?;
        return Ok(Run { facts, addr, off, width });
    }
    if let Some((width, off)) = store_width(op) {
        pop(&mut stack)?;
        let addr = pop(&mut stack)?;
        return Ok(Run { facts, addr, off, width });
    }
    Err(Stop::Blocked)
}

/// An i32 sum's addends that are not constants, and the sum of those that are.
fn flat(t: &Tm, atoms: &mut Vec<Tm>, c: &mut i128) {
    match t {
        Tm::B("add32", a, b) => {
            flat(a, atoms, c);
            flat(b, atoms, c);
        }
        Tm::K(k) => *c += k,
        other => atoms.push(other.clone()),
    }
}

/// `t` is the length word of the block at `base`: an `i32.load` at `base + 4`.
fn is_len(t: &Tm, base: &Tm) -> bool {
    matches!(t, Tm::M("i32", 4, a) if **a == *base)
}

/// `t` is the block's count of slots of `s` bytes, as the emitter writes it.
fn is_count(t: &Tm, base: &Tm, s: i128) -> bool {
    if s == 1 && is_len(t, base) {
        return true;
    }
    match t {
        Tm::B("divu32", l, k) => is_len(l, base) && **k == Tm::K(s),
        Tm::B("shru32", l, k) => is_len(l, base) && matches!(**k, Tm::K(n) if (0..32).contains(&n) && (1i128 << n) == s),
        _ => false,
    }
}

/// A term the window found zero that leaves the i32 index `ix` below the
/// count of `base` at stride `s`.
fn bounded(facts: &[Tm], ix: &Tm, base: &Tm, s: i128) -> bool {
    facts.iter().any(|f| match f {
        Tm::B("geu32", y, cnt) => **y == *ix && is_count(cnt, base, s),
        Tm::B("geu64", x, cnt) => {
            let Tm::U("ext_u", cnt) = &**cnt else { return false };
            let same = matches!(ix, Tm::U("wrap", x2) if **x2 == **x) || matches!(&**x, Tm::U("ext_u", y) if **y == *ix);
            same && is_count(cnt, base, s)
        }
        _ => false,
    })
}

/// The addend `t` as an index times a stride.
fn scaled(t: &Tm) -> (Tm, i128) {
    if let Tm::B(op, a, k) = t {
        if let Tm::K(n) = **k {
            if *op == "mul32" {
                return ((**a).clone(), n);
            }
            if *op == "shl32" && (0..32).contains(&n) {
                return ((**a).clone(), 1i128 << n);
            }
        }
    }
    (t.clone(), 1)
}

/// The proof script's `recog`, as a search test: the address is a base, an
/// index part and constants, and a compare bounds the index.
fn recognised(r: &Run) -> bool {
    let (mut atoms, mut c) = (Vec::new(), 0i128);
    flat(&r.addr, &mut atoms, &mut c);
    let [x, y] = atoms.as_slice() else { return false };
    let (off, w) = (i128::from(r.off), i128::from(r.width));
    let pair = |base: &Tm, sx: &Tm| {
        let (ix, s) = scaled(sx);
        s > 0 && s < (1i128 << 32) && c >= 0 && c + off >= 12 && c + off - 12 + w <= s && bounded(&r.facts, &ix, base, s)
    };
    pair(x, y) || pair(y, x)
}

/// Where a window that proves the access at `acc` starts, if one does: the
/// latest start from which the instructions up to the access run on their
/// own and the address is recognised.
fn propose(b: &Body<'_>, acc: usize) -> Option<usize> {
    let d = b.depth[acc];
    let lo = acc.saturating_sub(160);
    let mut s = acc;
    while s > lo {
        s -= 1;
        if b.depth[s] > d {
            continue;
        }
        if b.depth[s] < d {
            return None;
        }
        match run(b, s, acc) {
            Err(Stop::Underflow) => {}
            Err(Stop::Blocked) => return None,
            Ok(r) if recognised(&r) => return Some(s),
            Ok(_) => {}
        }
    }
    None
}

/// One proposed window: the function's position among the defined
/// functions, the access instruction's ordinal in it, and the window's bytes.
pub struct Window {
    pub func: usize,
    pub access: usize,
    pub bytes: Vec<u8>,
}

/// [`windows`] of the module `wasm`, decoded and validated here.
pub fn shipped(wasm: &[u8]) -> Result<(usize, Vec<Window>), String> {
    let bodies = crate::decode(wasm)?;
    Ok(windows(wasm, &bodies))
}

/// Every load and store of `bodies`, and the windows proposed for them:
/// `(accesses seen, windows)`. `wasm` is the module the bodies were decoded
/// from.
pub fn windows(wasm: &[u8], bodies: &[Body<'_>]) -> (usize, Vec<Window>) {
    let (mut seen, mut out) = (0, Vec::new());
    for (func, b) in bodies.iter().enumerate() {
        for (acc, op) in b.ops.iter().enumerate() {
            if !is_access(op) {
                continue;
            }
            seen += 1;
            if let Some(start) = propose(b, acc) {
                let hi = b.at.get(acc + 1).copied().unwrap_or(b.end);
                out.push(Window { func, access: acc, bytes: wasm[b.at[start]..hi].to_vec() });
            }
        }
    }
    (seen, out)
}
