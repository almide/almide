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
//! * no call that can run program code: no closure call, no runtime call
//!   given a closure (the only way the runtime runs program code) or a value
//!   of unknown kind, no closure built; a program function only when no
//!   argument holds a closure or a cell anywhere inside it and no mutable
//!   global can hold a closure (a cell is reachable only through a closure
//!   that captured it), its result then being plain;
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

/// A plain stand-in kind for a field of struct `i` when none of its fields is
/// a cell or a closure (`None` for a non-struct, or a struct holding one).
fn plain_struct(i: u16) -> Option<NTy> {
    let n = crate::render_native_rec::field_count(i);
    let fields: Option<Vec<NTy>> = (0..n).map(|k| crate::render_native_rec::rec_field(i, k)).collect();
    fields
        .filter(|f| !f.is_empty() && f.iter().all(|t| !is_cell(*t) && !crate::render_native_rec::is_closure(*t)))
        .map(|_| NTy::I64)
}

/// The ops between a cell read and the end of its window, judged one by one
/// in order (see the module docs).
struct Window<'a> {
    root: BTreeMap<ValueId, ValueId>,
    funcrefs: BTreeSet<ValueId>,
    /// The blocks the window made (and the read itself): none is a cell or
    /// a closure, or (`clean`) none can reach the cell.
    fresh: BTreeSet<ValueId>,
    /// The kinds of the fresh values known here (the read's payload, its
    /// copies): a read out of one is fresh when its element kind is plain.
    kinds: BTreeMap<ValueId, NTy>,
    /// The results of the admitted program-function calls and every value
    /// read out of one: none can reach the cell, whatever its kind.
    clean: BTreeSet<ValueId>,
    cell: ValueId,
    tys: &'a BTreeMap<ValueId, NTy>,
    program_fn: &'a dyn Fn(&str) -> bool,
}

impl Window<'_> {
    fn plain(&self, v: &ValueId) -> bool {
        self.fresh.contains(v) || self.tys.get(v).is_some_and(|t| !is_cell(*t) && !crate::render_native_rec::is_closure(*t))
    }

    /// `v`'s kind is known here and nothing inside a value of it is a
    /// closure or a cell.
    fn holds_no_closure(&self, v: &ValueId) -> bool {
        self.kinds.get(v).or_else(|| self.tys.get(v)).is_some_and(|t| crate::render_native_rec::holds_no_closure(*t))
    }

    fn block_ok(&self, a: &ValueId) -> bool {
        self.root.get(a).is_some_and(|b| *b != self.cell && self.plain(b))
    }

    /// A program function reaches a cell only through a closure: one inside
    /// an argument, or one a mutable global holds. Its result is clean.
    fn program_call(&mut self, args: &[CallArg], dst: Option<ValueId>) -> bool {
        let ok = crate::lower::mutable_globals_hold_no_closure()
            && args.iter().all(|a| !matches!(a, CallArg::Handle(v) if !self.holds_no_closure(v)));
        if let (true, Some(x)) = (ok, dst) {
            self.fresh.insert(x);
            self.clean.insert(x);
        }
        ok
    }

    /// A handle read out of `addr`: fresh when it is an element of a list of
    /// a plain kind, a field of a struct whose fields are all plain, or
    /// anything read out of a clean block.
    fn load_handle(&mut self, addr: Option<&ValueId>, dst: Option<ValueId>) -> bool {
        let ok = addr.is_some_and(|a| self.block_ok(a));
        let block = addr.and_then(|a| self.root.get(a)).copied();
        let elem = block.and_then(|b| self.kinds.get(&b)).and_then(|t| match t {
            NTy::Rec(i) | NTy::RecRef(i) => crate::render_native_rec::list_elem(*i).or_else(|| plain_struct(*i)),
            _ => None,
        });
        let plain_elem = elem.is_some_and(|e| !is_cell(e) && !crate::render_native_rec::is_closure(e));
        let clean = block.is_some_and(|b| self.clean.contains(&b));
        if let (true, Some(x)) = (ok && (plain_elem || clean), dst) {
            self.fresh.insert(x);
            if clean {
                self.clean.insert(x);
            }
        }
        ok
    }

    fn dup(&mut self, dst: ValueId, src: ValueId) -> bool {
        if let Some(t) = self.kinds.get(&src).copied() {
            self.kinds.insert(dst, t);
        }
        if self.clean.contains(&src) {
            self.clean.insert(dst);
        }
        self.plain(&src) && self.fresh.insert(dst)
    }

    /// `op` cannot look at the cell.
    fn admits(&mut self, op: &Op) -> bool {
        match op {
            Op::CallFn { name, .. } if name == "__mg_take" => false,
            Op::CallFn { name, args, dst, .. } if (self.program_fn)(name) => self.program_call(args, *dst),
            Op::CallFn { args, .. } => args.iter().all(|a| !matches!(a, CallArg::Handle(v) if !self.plain(v))),
            Op::Prim { kind: PrimKind::Handle, .. } | Op::IntBinOp { .. } | Op::ConstInt { .. } => true,
            Op::Prim { kind: PrimKind::LoadHandle, args, dst } => self.load_handle(args.first(), *dst),
            Op::Prim { kind: PrimKind::Load { .. } | PrimKind::Store { .. } | PrimKind::ElemAddr, args, .. } => {
                args.first().is_some_and(|a| self.block_ok(a))
            }
            Op::ListGetScalar { list, .. } | Op::ListSetScalar { list, .. } => *list != self.cell && self.plain(list),
            Op::Dup { dst, src } => self.dup(*dst, *src),
            Op::Alloc { dst, .. } => self.fresh.insert(*dst),
            Op::ListLit { dst, elems } => !elems.iter().any(|e| self.funcrefs.contains(e)) && self.fresh.insert(*dst),
            op => crate::render_native_move::bookkeeping(op),
        }
    }
}

fn window<'a>(func: &MirFunction, (d, cell, kind): (ValueId, ValueId, NTy), tys: &'a BTreeMap<ValueId, NTy>, program_fn: &'a dyn Fn(&str) -> bool) -> Window<'a> {
    let funcrefs = func.ops.iter().filter_map(|op| match op {
        Op::FuncRef { dst, .. } => Some(*dst),
        _ => None,
    }).collect();
    Window { root: crate::render_native_borrow_read::roots(func), funcrefs, fresh: BTreeSet::from([d]), kinds: BTreeMap::from([(d, kind)]), clean: BTreeSet::new(), cell, tys, program_fn }
}

/// The `__mg_take` result that ends the window of the cell read `d` (op `at`
/// of `func`, of the cell `cell`), when the read may take the payload.
pub(crate) fn take_window(
    func: &MirFunction,
    at: usize,
    (d, cell, kind): (ValueId, ValueId, NTy),
    tys: &BTreeMap<ValueId, NTy>,
    program_fn: &dyn Fn(&str) -> bool,
) -> Option<ValueId> {
    let mut w = window(func, (d, cell, kind), tys, program_fn);
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
/// `cell`) or anything borrowed from it (`borrow_span`), when nothing up to there
/// can look at the cell: the read may borrow the payload in place until
/// then. A read whose window ends no later than its own op borrows nothing.
pub(crate) fn borrow_window(
    func: &MirFunction,
    at: usize,
    (d, cell, kind): (ValueId, ValueId, NTy),
    tys: &BTreeMap<ValueId, NTy>,
    program_fn: &dyn Fn(&str) -> bool,
) -> Option<usize> {
    let (last, dups) = crate::render_native_borrow_read::borrow_span(func, at, d);
    let mut w = window(func, (d, cell, kind), tys, program_fn);
    let returned = func.ret.is_some_and(|r| r == d || dups.iter().any(|(_, v)| *v == r));
    (!returned && func.ops[at + 1..=last].iter().all(|op| w.admits(op))).then_some(last)
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
