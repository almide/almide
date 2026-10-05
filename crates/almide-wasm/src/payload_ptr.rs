//! #3345: a list whose block ADDRESS cannot change inside a loop gets a
//! per-loop PAYLOAD POINTER — `block + PAYLOAD`, computed once in the loop's
//! preheader — and its element accesses address `ptr + i * stride` with a
//! zero memarg offset.
//!
//! Why: an access through the block handle is `block + i * stride` at memarg
//! offset PAYLOAD (12). aarch64 has no addressing mode that takes both a
//! constant offset and an extended index register, so Cranelift spends an
//! `add x, heap, #12` on EVERY access — eight per iteration of fft's
//! butterfly. Through the pointer the access is one `add` (index scaling)
//! and the load.
//!
//! WHEN THE ADDRESS IS STABLE across the loop (condition and body,
//! including nested loops), checked at the preheader, AFTER this loop's own
//! copy-on-write pre-judge ran:
//! - the list was PRE-JUDGED by this loop or an enclosing one
//!   (`cow_prejudged`, cow_hoist.rs): the scan behind it proved the loop
//!   reaches the list only through element reads and element stores — no
//!   rebind, no bind, no RC statement, no call argument, no capture, no
//!   peephole list statement, no lambda / closure / fan / iterator chain —
//!   and the judge already ran, so every store inside skips it
//!   (`emit_local_cow`; a nested loop's flag or pre-judge sees the list as
//!   pre-judged and judges nothing). Nothing else writes the local; or
//! - the loop never WRITES the list ([`unwritten`]: no element store, no
//!   peephole list statement, no map/field write, no RC statement) and
//!   len_hoist.rs's scan proves it is never rebound, never handed to a call
//!   and no lambda is present — so no judge and no rebind can touch the
//!   local.
//!
//! In both cases the block the local names at the preheader is the block
//! every access in the loop reads or writes, so `block + PAYLOAD` computed
//! there is the payload of every one of them. A cell (a variable a closure
//! writes through) is refused: its local holds the cell, not the list.

use std::collections::BTreeSet;

use almide_ir::visit::{walk_stmt, IrVisitor};
use almide_ir::{IrExpr, IrStmt, IrStmtKind, VarId};

use crate::emitter::Emitter;

#[derive(Default)]
struct Writes(BTreeSet<VarId>);

impl IrVisitor for Writes {
    fn visit_stmt(&mut self, stmt: &IrStmt) {
        match &stmt.kind {
            IrStmtKind::IndexAssign { target, .. }
            | IrStmtKind::MapInsert { target, .. }
            | IrStmtKind::FieldAssign { target, .. }
            | IrStmtKind::ListSwap { target, .. }
            | IrStmtKind::ListReverse { target, .. }
            | IrStmtKind::ListRotateLeft { target, .. } => {
                self.0.insert(*target);
            }
            IrStmtKind::ListCopySlice { dst, .. } => {
                self.0.insert(*dst);
            }
            IrStmtKind::RcInc { var } | IrStmtKind::RcDec { var } => {
                self.0.insert(*var);
            }
            _ => {}
        }
        walk_stmt(self, stmt);
    }
}

/// The lists the loop's statements never write (module header).
fn unwritten(cond: Option<&IrExpr>, body: &[IrStmt]) -> Vec<VarId> {
    let mut w = Writes::default();
    for st in body {
        w.visit_stmt(st);
    }
    crate::len_hoist::invariant_counts(cond, body).into_iter().filter(|v| !w.0.contains(v)).collect()
}

impl Emitter<'_> {
    /// Give every address-stable list of this loop (module header) a payload
    /// pointer, in the preheader — after this loop's pre-judge. `prejudged`
    /// is what this loop's pre-judge marked. Returns the vars it added, for
    /// `drop_payload_ptrs`.
    pub(crate) fn hoist_payload_ptrs(
        &mut self,
        cond: Option<&IrExpr>,
        body: &[IrStmt],
        prejudged: &[VarId],
    ) -> Result<Vec<VarId>, crate::EmitError> {
        let mut cands: BTreeSet<VarId> = prejudged.iter().copied().collect();
        cands.extend(self.cow_prejudged.iter().copied().filter(|v| crate::cow_hoist::element_only_writes(cond, body).contains(v)));
        cands.extend(unwritten(cond, body));
        let mut added = Vec::new();
        for v in cands {
            if self.payload_ptrs.contains_key(&v) || self.cells.contains(&v) {
                continue; // an enclosing loop's pointer still holds / a cell
            }
            if self.hold_i32_depth >= crate::emitter::HOLD_I32_POOL / 2 {
                break; // never the reason a body hits the hold-depth wall
            }
            let Some(&(slot, crate::SliceTy::List(_))) = self.locals.get(&v) else {
                continue; // a global, a range bind, or not a list
            };
            let ptr = self.hold_i32()?;
            let mut i = self.f.instructions();
            i.local_get(slot).i32_const(almide_layout::PAYLOAD as i32).i32_add().local_set(ptr);
            self.payload_ptrs.insert(v, ptr);
            added.push(v);
        }
        Ok(added)
    }

    /// Release one loop's payload pointers, innermost hold first.
    pub(crate) fn drop_payload_ptrs(&mut self, added: Vec<VarId>) {
        for v in added.into_iter().rev() {
            self.payload_ptrs.remove(&v);
            self.release_i32();
        }
    }

    /// The payload pointer of `v`, when its loop hoisted one.
    pub(crate) fn payload_ptr_of(&self, v: VarId) -> Option<u32> {
        self.payload_ptrs.get(&v).copied()
    }
}

/// `v + c` with `c` a small non-negative literal: the element is `c` slots
/// past `v`'s, so its address is `v`'s plus a constant memarg offset — the
/// scaled `v` is shared by `xs[v]`, `xs[v + 1]`, … (fft's re/im pairs). At
/// most this many bytes of offset: every block lies at or above POOL_START,
/// so `base + wrap(v) * stride` cannot wrap below zero when `v` is as low as
/// `-c` (the element `v + c` is in bounds, so `v >= -c`).
const MAX_FOLD_BYTES: i64 = 64;
const _: () = assert!(crate::POOL_START as i64 >= MAX_FOLD_BYTES);

/// `(v, c)` for an index `v + c` (or `v`, c = 0) — the bounds-fact key and
/// the address fold's shape.
pub(crate) fn affine_index(index: &IrExpr) -> Option<(VarId, i64)> {
    use almide_ir::IrExprKind as K;
    match &index.kind {
        K::Var { id } => Some((*id, 0)),
        K::BinOp { op: almide_ir::BinOp::AddInt, left, right } => match (&left.kind, &right.kind) {
            (K::Var { id }, K::LitInt { value }) if (0..=8).contains(value) => Some((*id, *value)),
            _ => None,
        },
        _ => None,
    }
}

impl Emitter<'_> {
    /// Push the address of element `index` (its i64 value already in `idx`,
    /// and already bounds-checked) of a list whose block is in `block`;
    /// returns the memarg offset the access must use. Through the loop's
    /// payload pointer when `list` has one (offset 0), else `block` (offset
    /// PAYLOAD); an index `v + c` scales `v` and folds `c * stride` into
    /// the offset (module header of `affine_index`).
    pub(crate) fn emit_elem_addr(
        &mut self,
        list: Option<VarId>,
        block: u32,
        idx: u32,
        index: &IrExpr,
        stride: i64,
    ) -> Result<u64, crate::EmitError> {
        let (base, mut off) = match list.and_then(|v| self.payload_ptr_of(v)) {
            Some(p) => (p, 0u64),
            None => (block, u64::from(almide_layout::PAYLOAD)),
        };
        self.f.instructions().local_get(base);
        match affine_index(index) {
            Some((_, c)) if c > 0 && c * stride <= MAX_FOLD_BYTES => {
                let almide_ir::IrExprKind::BinOp { left, .. } = &index.kind else { unreachable!("affine_index shape") };
                self.lower(left, Some(crate::INT))?;
                off += (c * stride) as u64;
            }
            _ => {
                self.f.instructions().local_get(idx);
            }
        }
        self.f.instructions().i32_wrap_i64().i32_const(stride as i32).i32_mul().i32_add();
        Ok(off)
    }

    /// Load / store a slot of type `t` at the address on the stack + `off`.
    pub(crate) fn load_slot_off(&mut self, t: crate::SliceTy, off: u64) {
        let m = wasm_encoder::MemArg { offset: off, align: 2, memory_index: 0 };
        match t.val_type() {
            wasm_encoder::ValType::I64 => self.f.instructions().i64_load(m),
            wasm_encoder::ValType::F64 => self.f.instructions().f64_load(m),
            _ => self.f.instructions().i32_load(m),
        };
    }

    pub(crate) fn store_slot_off(&mut self, t: crate::SliceTy, off: u64) {
        let m = wasm_encoder::MemArg { offset: off, align: 2, memory_index: 0 };
        match t.val_type() {
            wasm_encoder::ValType::I64 => self.f.instructions().i64_store(m),
            wasm_encoder::ValType::F64 => self.f.instructions().f64_store(m),
            _ => self.f.instructions().i32_store(m),
        };
    }
}
