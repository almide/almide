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

/// The `__mg_take` result that ends the window of the cell read `d` (op `at`
/// of `func`, of the cell `cell`), when the read may take the payload.
pub(crate) fn take_window(
    func: &MirFunction,
    at: usize,
    (d, cell): (ValueId, ValueId),
    tys: &BTreeMap<ValueId, NTy>,
    program_fn: &dyn Fn(&str) -> bool,
) -> Option<ValueId> {
    let root = crate::render_native_borrow_read::roots(func);
    let funcrefs: BTreeSet<ValueId> = func.ops.iter().filter_map(|op| match op {
        Op::FuncRef { dst, .. } => Some(*dst),
        _ => None,
    }).collect();
    // The blocks the window made (and `d`, the read itself): none is a cell
    // or a closure.
    let mut fresh: BTreeSet<ValueId> = BTreeSet::from([d]);
    let plain = |v: &ValueId, fresh: &BTreeSet<ValueId>| {
        fresh.contains(v) || tys.get(v).is_some_and(|t| !is_cell(*t) && !crate::render_native_rec::is_closure(*t))
    };
    let block_ok = |a: &ValueId, fresh: &BTreeSet<ValueId>| root.get(a).is_some_and(|b| *b != cell && plain(b, fresh));
    for (j, op) in func.ops.iter().enumerate().skip(at + 1) {
        let ok = match op {
            Op::CallFn { name, args, dst, .. } if name == "__mg_take" => {
                let same = matches!(args.as_slice(), [CallArg::Scalar(a)] if root.get(a) == Some(&cell));
                return (*dst).filter(|x| same && only_released(func, j, *x));
            }
            Op::CallFn { name, args, .. } => {
                !program_fn(name) && args.iter().all(|a| !matches!(a, CallArg::Handle(v) if !plain(v, &fresh)))
            }
            Op::Prim { kind: PrimKind::Handle, .. } | Op::IntBinOp { .. } | Op::ConstInt { .. } => true,
            Op::Prim { kind: PrimKind::Load { .. } | PrimKind::LoadHandle | PrimKind::Store { .. }, args, .. } => {
                args.first().is_some_and(|a| block_ok(a, &fresh))
            }
            Op::ListGetScalar { list, .. } | Op::ListSetScalar { list, .. } => *list != cell && plain(list, &fresh),
            Op::Dup { dst, src } => plain(src, &fresh) && fresh.insert(*dst),
            Op::Alloc { dst, .. } => fresh.insert(*dst),
            Op::ListLit { dst, elems } => !elems.iter().any(|e| funcrefs.contains(e)) && fresh.insert(*dst),
            op => crate::render_native_move::bookkeeping(op),
        };
        if !ok {
            return None;
        }
    }
    None
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
