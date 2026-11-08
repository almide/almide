//! A shared cell's payload read that the cell's own take follows (#4006,
//! no-perf-loss rule).
//!
//! A captured-and-mutated `var` lives in a shared cell (`Rc<RefCell<Option<T>>>`,
//! render_native_cell_slots). Its update reads the payload, builds the new
//! value from that copy, takes the old payload out and drops it, and stores
//! the new one: `m[k] = v` on a captured Map copied the whole map per write,
//! where the standard codegen inserts in place. When nothing can look at the
//! cell between the read and the take, the read can take the payload itself:
//! the copy and the original are the same value at that point, and the take
//! finds the cell emptied, which is the state it leaves anyway.
//!
//! "Nothing can look at the cell" is judged from the ops between the two,
//! with the render's value kinds:
//! * no control flow (every path from the read reaches the take), no return;
//! * no call that can run program code: no closure call, no call of a
//!   program function, no runtime call given a closure (the only way the
//!   runtime runs program code) or a value of unknown kind, no closure built;
//! * every block read or written is one the window made, or one of a known
//!   kind that is neither a cell nor a closure — never the cell itself, nor a
//!   value whose kind is unknown (a cell alias read out of a record);
//! * the take is of the same cell value, and what it yields is only
//!   released (the emptied cell has no payload to give).

use crate::render_native::NTy;
use crate::{CallArg, MirFunction, Op, PrimKind, ValueId};
use std::collections::{BTreeMap, BTreeSet};

fn is_cell(t: NTy) -> bool {
    matches!(t, NTy::Rec(i) | NTy::RecRef(i) if crate::render_native_rec::cell_inner(i).is_some())
}

/// The ops between a cell read and the end of its window, judged one by one
/// in order (see the module docs).
struct Window<'a> {
    root: BTreeMap<ValueId, ValueId>,
    funcrefs: BTreeSet<ValueId>,
    /// The blocks the window made (and the read itself): none is a cell or
    /// a closure.
    fresh: BTreeSet<ValueId>,
    cell: ValueId,
    tys: &'a BTreeMap<ValueId, NTy>,
    program_fn: &'a dyn Fn(&str) -> bool,
}

impl Window<'_> {
    fn plain(&self, v: &ValueId) -> bool {
        self.fresh.contains(v) || self.tys.get(v).is_some_and(|t| !is_cell(*t) && !crate::render_native_rec::is_closure(*t))
    }

    fn block_ok(&self, a: &ValueId) -> bool {
        self.root.get(a).is_some_and(|b| *b != self.cell && self.plain(b))
    }

    /// `op` cannot look at the cell.
    fn admits(&mut self, op: &Op) -> bool {
        match op {
            Op::CallFn { name, args, .. } => {
                name != "__mg_take" && !(self.program_fn)(name) && args.iter().all(|a| !matches!(a, CallArg::Handle(v) if !self.plain(v)))
            }
            Op::Prim { kind: PrimKind::Handle, .. } | Op::IntBinOp { .. } | Op::ConstInt { .. } => true,
            Op::Prim { kind: PrimKind::Load { .. } | PrimKind::LoadHandle | PrimKind::Store { .. } | PrimKind::ElemAddr, args, .. } => {
                args.first().is_some_and(|a| self.block_ok(a))
            }
            Op::ListGetScalar { list, .. } | Op::ListSetScalar { list, .. } => *list != self.cell && self.plain(list),
            Op::Dup { dst, src } => self.plain(src) && self.fresh.insert(*dst),
            Op::Alloc { dst, .. } => self.fresh.insert(*dst),
            Op::ListLit { dst, elems } => !elems.iter().any(|e| self.funcrefs.contains(e)) && self.fresh.insert(*dst),
            op => crate::render_native_move::bookkeeping(op),
        }
    }
}

fn window<'a>(func: &MirFunction, (d, cell): (ValueId, ValueId), tys: &'a BTreeMap<ValueId, NTy>, program_fn: &'a dyn Fn(&str) -> bool) -> Window<'a> {
    let funcrefs = func.ops.iter().filter_map(|op| match op {
        Op::FuncRef { dst, .. } => Some(*dst),
        _ => None,
    }).collect();
    Window { root: crate::render_native_borrow_read::roots(func), funcrefs, fresh: BTreeSet::from([d]), cell, tys, program_fn }
}

/// The `__mg_take` result that ends the window of the cell read `d` (op `at`
/// of `func`, of the cell `cell`), when the read may take the payload.
pub(crate) fn take_window(
    func: &MirFunction,
    at: usize,
    (d, cell): (ValueId, ValueId),
    tys: &BTreeMap<ValueId, NTy>,
    program_fn: &dyn Fn(&str) -> bool,
) -> Option<ValueId> {
    let mut w = window(func, (d, cell), tys, program_fn);
    for (j, op) in func.ops.iter().enumerate().skip(at + 1) {
        if let Op::CallFn { name, args, dst, .. } = op {
            if name == "__mg_take" {
                let same = matches!(args.as_slice(), [CallArg::Scalar(a)] if w.root.get(a) == Some(&cell));
                return (*dst).filter(|x| same && only_released(func, j, *x));
            }
        }
        if !w.admits(op) {
            return None;
        }
    }
    None
}

/// The last op that reads the cell read `d` (op `at` of `func`, of the cell
/// `cell`) or a reference copy of it (`ref_dups`), when nothing up to there
/// can look at the cell: the read may borrow the payload in place until
/// then. A read whose window ends no later than its own op borrows nothing.
pub(crate) fn borrow_window(
    func: &MirFunction,
    at: usize,
    (d, cell): (ValueId, ValueId),
    tys: &BTreeMap<ValueId, NTy>,
    program_fn: &dyn Fn(&str) -> bool,
) -> Option<usize> {
    let mut aliases = BTreeSet::from([d]);
    let mut last = at;
    let mut reads = Vec::new();
    for (j, op) in func.ops.iter().enumerate().skip(at + 1) {
        reads.clear();
        crate::mir_ops::op_reads(op, &mut reads);
        if reads.iter().any(|r| aliases.contains(r)) && !crate::render_native_move::bookkeeping(op) {
            last = j;
            if let Op::Dup { dst, .. } = op {
                aliases.insert(*dst);
            }
        }
    }
    let mut w = window(func, (d, cell), tys, program_fn);
    (func.ret.is_none_or(|r| !aliases.contains(&r)) && func.ops[at + 1..=last].iter().all(|op| w.admits(op))).then_some(last)
}

/// After `ops[at]`, `x` is only released.
fn only_released(func: &MirFunction, at: usize, x: ValueId) -> bool {
    let mut reads = Vec::new();
    func.ret != Some(x)
        && func.ops.iter().skip(at + 1).all(|op| {
            reads.clear();
            crate::mir_ops::op_reads(op, &mut reads);
            !reads.contains(&x) || crate::render_native_move::bookkeeping(op)
        })
}
