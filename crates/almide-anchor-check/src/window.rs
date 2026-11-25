//! Probe: which located bounds accesses follow from a STRAIGHT-LINE WINDOW of
//! the shipped instructions alone: the compare, its aborting arm and the
//! access, with nothing between them that writes memory, calls, or branches.
//!
//! The window is run symbolically from an empty stack, over terms in the
//! values the locals and the memory hold when it is entered. An access is
//! PROVED when its address is `base + 12 + idx * S + o` (`o + width <= S`)
//! and a compare the window aborted on leaves `idx <u len(base) / S`, the
//! length being the word the window itself read at `base + 4`. Nothing
//! outside the window is consulted: no witness, no anchor, no other rule.

use std::collections::HashMap;

use wasmparser::{BlockType, Operator as Op};

use crate::decode::arm_branches_out;
use crate::{store_width, Body};

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Tm {
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

fn show(t: &Tm) -> String {
    match t {
        Tm::L(x) => format!("l{x}"),
        Tm::G(x) => format!("g{x}"),
        Tm::K(k) => format!("{k}"),
        Tm::M(k, o, a) => format!("{k}[{}+{o}]", show(a)),
        Tm::U(n, a) => format!("{n}({})", show(a)),
        Tm::B(n, a, b) => format!("{n}({},{})", show(a), show(b)),
    }
}

fn un(op: &Op<'_>) -> Option<&'static str> {
    Some(match op {
        Op::I64ExtendI32U => "ext_u",
        Op::I64ExtendI32S => "ext_s",
        Op::I32WrapI64 => "wrap",
        Op::I32Eqz => "eqz32",
        Op::I64Eqz => "eqz64",
        _ => return None,
    })
}

fn bin(op: &Op<'_>) -> Option<&'static str> {
    Some(match op {
        Op::I32Add => "add32",
        Op::I32Sub => "sub32",
        Op::I32Mul => "mul32",
        Op::I32DivU => "divu32",
        Op::I32Shl => "shl32",
        Op::I32ShrU => "shru32",
        Op::I32And => "and32",
        Op::I64Add => "add64",
        Op::I64Sub => "sub64",
        Op::I64Mul => "mul64",
        Op::I64Shl => "shl64",
        Op::I64ShrU => "shru64",
        Op::I64And => "and64",
        Op::I32GeU => "geu32",
        Op::I32GtU => "gtu32",
        Op::I32LtU => "ltu32",
        Op::I32LeU => "leu32",
        Op::I32Eq => "eq32",
        Op::I32Ne => "ne32",
        Op::I32GeS => "ges32",
        Op::I32GtS => "gts32",
        Op::I32LtS => "lts32",
        Op::I32LeS => "les32",
        Op::I64GeU => "geu64",
        Op::I64GtU => "gtu64",
        Op::I64LtU => "ltu64",
        Op::I64LeU => "leu64",
        Op::I64Eq => "eq64",
        Op::I64Ne => "ne64",
        Op::I64GeS => "ges64",
        Op::I64GtS => "gts64",
        Op::I64LtS => "lts64",
        Op::I64LeS => "les64",
        _ => return None,
    })
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

fn name(op: &Op<'_>) -> String {
    let s = format!("{op:?}");
    s.split(|c: char| c == ' ' || c == '{' || c == '(').next().unwrap_or("?").to_string()
}

/// Any other numeric instruction of the one-byte opcode space (0x45..0xc4):
/// a function of its operands, by arity. The proof gives these no meaning,
/// so the name only has to tell two of them apart.
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

enum Stop {
    /// The window starts too late: an instruction pops what it did not push.
    Underflow,
    /// An instruction the window may not hold, wherever it starts.
    Blocked(String),
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
            return Err(Stop::Blocked("depth".into()));
        }
        if let Op::If { blockty } = op {
            if !matches!(blockty, BlockType::Empty) {
                return Err(Stop::Blocked("if-with-result".into()));
            }
            let end = (j + 1..b.ops.len())
                .find(|&k| b.depth[k] == d + 1 && matches!(b.ops[k], Op::End | Op::Else))
                .ok_or_else(|| Stop::Blocked("if-without-end".into()))?;
            if !matches!(b.ops[end], Op::End) {
                return Err(Stop::Blocked("if-else".into()));
            }
            if end >= acc {
                return Err(Stop::Blocked("access-inside-arm".into()));
            }
            if !matches!(b.ops[end - 1], Op::Unreachable) {
                return Err(Stop::Blocked("arm-not-aborting".into()));
            }
            if arm_branches_out(&b.ops, &b.depth, j, end).is_some() {
                return Err(Stop::Blocked("arm-branches-out".into()));
            }
            // Past the arm the condition was zero.
            facts.push(pop(&mut stack)?);
            j = end + 1;
            continue;
        }
        match op {
            Op::LocalGet { local_index } => {
                stack.push(env.get(local_index).cloned().unwrap_or(Tm::L(*local_index)));
            }
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
            // A conditional branch is an exit: past it the condition was zero.
            Op::BrIf { .. } => facts.push(pop(&mut stack)?),
            _ => {
                if let Some(n) = un(op) {
                    let a = pop(&mut stack)?;
                    stack.push(Tm::U(n, Box::new(a)));
                } else if let Some(n) = bin(op) {
                    let y = pop(&mut stack)?;
                    let x = pop(&mut stack)?;
                    stack.push(Tm::B(n, Box::new(x), Box::new(y)));
                } else if let Some((k, _, off)) = load(op) {
                    let a = pop(&mut stack)?;
                    stack.push(Tm::M(k, off, Box::new(a)));
                } else if let Some((arity, n)) = opaque(op) {
                    if arity == 1 {
                        let a = pop(&mut stack)?;
                        stack.push(Tm::U(n, Box::new(a)));
                    } else {
                        let y = pop(&mut stack)?;
                        let x = pop(&mut stack)?;
                        stack.push(Tm::B(n, Box::new(x), Box::new(y)));
                    }
                } else {
                    return Err(Stop::Blocked(name(op)));
                }
            }
        }
        j += 1;
    }
    if j != acc {
        return Err(Stop::Blocked("access-inside-arm".into()));
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
    Err(Stop::Blocked(format!("access-is-{}", name(op))))
}

fn addends(t: &Tm, out: &mut Vec<Tm>) {
    if let Tm::B("add32", a, b) = t {
        addends(a, out);
        addends(b, out);
    } else {
        out.push(t.clone());
    }
}

/// `t` is the length word of the block at `base`: an `i32.load` at `base + 4`.
fn is_len(t: &Tm, base: &Tm) -> bool {
    let Tm::M("i32", off, a) = t else { return false };
    let mut parts = Vec::new();
    addends(a, &mut parts);
    let mut c = i128::from(*off);
    let mut rest = Vec::new();
    for p in parts {
        match p {
            Tm::K(k) => c += k,
            other => rest.push(other),
        }
    }
    c == 4 && rest.len() == 1 && &rest[0] == base
}

/// `t` is the block's count of slots of `s` bytes.
fn is_count(t: &Tm, base: &Tm, s: i128) -> bool {
    if s == 1 && is_len(t, base) {
        return true;
    }
    match t {
        Tm::B("divu32", l, k) => is_len(l, base) && **k == Tm::K(s),
        Tm::B("shru32", l, k) => is_len(l, base) && matches!(**k, Tm::K(n) if (0..31).contains(&n) && (1i128 << n) == s),
        _ => false,
    }
}

/// A fact (a term the window found zero) that leaves the i32 index `ix`
/// below the count of `base` at stride `s`.
fn bounded(facts: &[Tm], ix: &Tm, base: &Tm, s: i128) -> bool {
    facts.iter().any(|f| match f {
        Tm::B("geu32", y, cnt) => **y == *ix && is_count(cnt, base, s),
        Tm::U("eqz32", lt) => matches!(&**lt, Tm::B("ltu32", y, cnt) if **y == *ix && is_count(cnt, base, s)),
        Tm::B("geu64", x, cnt) => {
            let Tm::U("ext_u", cnt) = &**cnt else { return false };
            let same = matches!(ix, Tm::U("wrap", x2) if **x2 == **x)
                || matches!(&**x, Tm::U("ext_u", y) if **y == *ix)
                || (matches!(ix, Tm::K(_)) && **x == *ix);
            same && is_count(cnt, base, s)
        }
        _ => false,
    })
}

fn recognise(r: &Run) -> Result<&'static str, String> {
    let mut parts = Vec::new();
    addends(&r.addr, &mut parts);
    let mut c = i128::from(r.off);
    let mut scaled: Vec<(Tm, i128)> = Vec::new();
    let mut atoms: Vec<Tm> = Vec::new();
    for p in parts {
        match p {
            Tm::K(k) => c += k,
            Tm::B("mul32", a, b) => match (*a, *b) {
                (x, Tm::K(s)) | (Tm::K(s), x) if s > 0 => scaled.push((x, s)),
                (a, b) => atoms.push(Tm::B("mul32", Box::new(a), Box::new(b))),
            },
            Tm::B("shl32", a, b) => match *b {
                Tm::K(n) if (0..31).contains(&n) => scaled.push((*a, 1i128 << n)),
                b => atoms.push(Tm::B("shl32", a, Box::new(b))),
            },
            other => atoms.push(other),
        }
    }
    let w = i128::from(r.width);
    let o = c - 12;
    let fits = |base: &Tm, ix: &Tm, s: i128| o >= 0 && o + w <= s && bounded(&r.facts, ix, base, s);
    match (scaled.as_slice(), atoms.as_slice()) {
        ([(ix, s)], [base]) if fits(base, ix, *s) => Ok("scaled"),
        ([], [a, b]) if fits(a, b, 1) || fits(b, a, 1) => Ok("stride1"),
        ([], [base]) => {
            // A constant slot: some count of the block at a stride `s` is above `j`.
            for f in &r.facts {
                let (j, cnt) = match f {
                    Tm::B("geu32", j, cnt) => (&**j, &**cnt),
                    Tm::B("geu64", j, cnt) => match &**cnt {
                        Tm::U("ext_u", cnt) => (&**j, &**cnt),
                        _ => continue,
                    },
                    _ => continue,
                };
                let Tm::K(j) = j else { continue };
                for s in [1i128, 2, 4, 8, 16, 24, 32] {
                    if is_count(cnt, base, s) && o >= j * s && o + w <= (j + 1) * s {
                        return Ok("const");
                    }
                }
            }
            // A non-empty block's first byte.
            if o == 0 && w == 1 && r.facts.iter().any(|f| matches!(f, Tm::U("eqz32", l) if is_len(l, base))) {
                return Ok("first-byte");
            }
            Err(format!("unrecognised const addr={}+{} facts={}", show(&r.addr), r.off, r.facts.iter().map(show).collect::<Vec<_>>().join(";")))
        }
        _ => Err(format!("unrecognised addr={}+{} facts={}", show(&r.addr), r.off, r.facts.iter().map(show).collect::<Vec<_>>().join(";"))),
    }
}

/// Whether `op` reads or writes memory at an address operand (a load or a
/// store; bulk copies and fills are not windows).
pub fn is_access(op: &Op<'_>) -> bool {
    load(op).is_some() || store_width(op).is_some()
}

/// The access instruction an `a` / `w` anchored at `acc0` names: an `a`
/// anchored at the call that consumes a loaded element is the load before it.
pub fn access_op(b: &Body<'_>, acc0: usize) -> usize {
    if matches!(b.ops[acc0], Op::Call { .. }) && acc0 > 0 && load(&b.ops[acc0 - 1]).is_some() {
        acc0 - 1
    } else {
        acc0
    }
}

/// Whether the access at `acc0`, guarded by the compare at `cmp`, follows
/// from a straight-line window: `(proved, shape or reason, window start)`.
pub fn prove(b: &Body<'_>, cmp: usize, acc0: usize) -> (bool, String, usize) {
    let acc = access_op(b, acc0);
    let d = b.depth[acc];
    let mut last = String::from("no-window");
    let lo = acc.saturating_sub(160);
    let mut s = acc;
    while s > lo {
        s -= 1;
        if b.depth[s] > d {
            continue;
        }
        if b.depth[s] < d {
            return (false, format!("boundary:{} {last}", name(&b.ops[s])), s);
        }
        match run(b, s, acc) {
            Err(Stop::Underflow) => {}
            Err(Stop::Blocked(why)) => {
                let side = if s > cmp { "between" } else { "before" };
                return (false, format!("blocked-{side}:{why} {last}"), s);
            }
            Ok(r) => match recognise(&r) {
                Ok(shape) => return (true, shape.to_string(), s),
                Err(e) => last = e,
            },
        }
    }
    (false, format!("limit {last}"), lo)
}

/// Probe only: whether `ops[s..=acc]` is a straight-line run the proof
/// script's window language reads from an empty stack.
pub(crate) fn runs_straight(b: &Body<'_>, s: usize, acc: usize) -> bool {
    run(b, s, acc).is_ok()
}
