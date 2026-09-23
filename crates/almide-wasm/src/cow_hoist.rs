//! #2150: the copy-on-write JUDGE an element store runs (`xs[i] = v` →
//! `call $cow(xs)`) is idempotent while nothing can share the list, so a
//! loop that only reads and writes a list's ELEMENTS runs it once per loop
//! entry instead of once per store.
//!
//! The judge returns its argument when the block is uniquely held (or not
//! heap-counted), and otherwise copies it, releases one source reference and
//! returns the fresh, uniquely-held copy. Either way the list is uniquely
//! held afterwards, and the ONLY way to make it shared again is to hand its
//! handle to something: a `Var` read of the list as a value (a bind, an
//! argument, a return, a capture), a rebinding, an explicit `RcInc`, or a
//! peephole list statement that takes the handle. An element READ (`xs[j]`)
//! or an element STORE does neither. So when the scan below proves that,
//! inside the loop (condition + body), the list is reached ONLY as the object
//! of `xs[j]` and the target of `xs[i] = v`, the first store's judge result
//! holds for every later store of that loop entry.
//!
//! The emitted shape is a per-loop-entry flag: cleared before the loop, and
//! each store runs `if !flag { xs = cow(xs); flag = 1 }` — a register test in
//! place of a call, a global compare and a load of the reference count.
//! fft's butterfly loop stores four times per iteration; per-store judging
//! was the bulk of its wasm/native gap (4.5x on the butterfly phase alone).
//!
//! The flag is not hoisted to an unconditional judge before the loop: a loop
//! that never stores would then copy a shared list it never writes, which is
//! an allocation the program did not have (the alloc ledgers may only go
//! down).
//!
//! WHAT THE SCAN REFUSES, conservatively: every candidate when the loop holds
//! a lambda, a closure, a fan, an iterator chain or an inline-Rust node (each
//! can reach variables by id rather than through a `Var` read); a candidate
//! the loop binds, assigns, RC-adjusts, or hands to a map/field/peephole
//! statement; a list read through a cell or held in a global (the emitter
//! side, below).

use std::collections::BTreeSet;

use almide_ir::visit::{walk_expr, walk_pattern, walk_stmt, IrVisitor};
use almide_ir::{IrExpr, IrExprKind, IrPattern, IrStmt, IrStmtKind, VarId};

/// At most this many flags per loop, and only while the i32 hold pool keeps
/// this much headroom for the body — a flag must never be what pushes a body
/// into the `hold-depth-i32` wall.
const MAX_FLAGS_PER_LOOP: usize = 2;
const HOLD_HEADROOM: u32 = crate::emitter::HOLD_I32_POOL / 2;

#[derive(Default)]
struct Scan {
    /// Targets of `v[i] = …` — the candidates.
    written: BTreeSet<VarId>,
    /// Vars the loop reaches as a value, rebinds, or RC-adjusts.
    escaped: BTreeSet<VarId>,
    /// A node that can reach a variable without a `Var` read.
    opaque: bool,
}

impl Scan {
    fn escape(&mut self, v: VarId) {
        self.escaped.insert(v);
    }
}

fn is_opaque(kind: &IrExprKind) -> bool {
    matches!(
        kind,
        IrExprKind::Lambda { .. }
            | IrExprKind::ClosureCreate { .. }
            | IrExprKind::EnvLoad { .. }
            | IrExprKind::IterChain { .. }
            | IrExprKind::InlineRust { .. }
            | IrExprKind::RenderedCall { .. }
            | IrExprKind::Fan { .. }
    )
}

impl IrVisitor for Scan {
    fn visit_expr(&mut self, expr: &IrExpr) {
        match &expr.kind {
            // An element read shares nothing: walk the index only.
            IrExprKind::IndexAccess { object, index } if matches!(object.kind, IrExprKind::Var { .. }) => {
                self.visit_expr(index);
                return;
            }
            IrExprKind::Var { id } => self.escape(*id),
            k if is_opaque(k) => self.opaque = true,
            _ => {}
        }
        walk_expr(self, expr);
    }

    fn visit_stmt(&mut self, stmt: &IrStmt) {
        match &stmt.kind {
            IrStmtKind::IndexAssign { target, index, value } => {
                self.written.insert(*target);
                self.visit_expr(index);
                self.visit_expr(value);
                return;
            }
            IrStmtKind::Bind { var, .. }
            | IrStmtKind::Assign { var, .. }
            | IrStmtKind::RcInc { var }
            | IrStmtKind::RcDec { var } => self.escape(*var),
            IrStmtKind::MapInsert { target, .. }
            | IrStmtKind::FieldAssign { target, .. }
            | IrStmtKind::ListSwap { target, .. }
            | IrStmtKind::ListReverse { target, .. }
            | IrStmtKind::ListRotateLeft { target, .. } => self.escape(*target),
            IrStmtKind::ListCopySlice { dst, src, .. } => {
                self.escape(*dst);
                self.escape(*src);
            }
            _ => {}
        }
        walk_stmt(self, stmt);
    }

    fn visit_pattern(&mut self, pat: &IrPattern) {
        if let IrPattern::Bind { var, .. } | IrPattern::As { var, .. } = pat {
            self.escape(*var);
        }
        walk_pattern(self, pat);
    }
}

/// The lists a loop (its condition and body) stores into and otherwise
/// reaches only through element reads, in VarId order (deterministic bytes).
pub(crate) fn element_only_writes(cond: Option<&IrExpr>, body: &[IrStmt]) -> Vec<VarId> {
    let mut scan = Scan::default();
    if let Some(c) = cond {
        scan.visit_expr(c);
    }
    for st in body {
        scan.visit_stmt(st);
    }
    if scan.opaque {
        return Vec::new();
    }
    scan.written.difference(&scan.escaped).copied().collect()
}

impl crate::emitter::Emitter<'_> {
    /// Clear one judged-flag per element-only list of this loop, before the
    /// loop. Returns the vars it added, for `drop_cow_flags`.
    pub(crate) fn hoist_cow_flags(
        &mut self,
        cond: Option<&IrExpr>,
        body: &[IrStmt],
    ) -> Result<Vec<VarId>, crate::EmitError> {
        let mut added = Vec::new();
        for v in element_only_writes(cond, body) {
            if added.len() >= MAX_FLAGS_PER_LOOP || self.hold_i32_depth >= HOLD_HEADROOM {
                break;
            }
            if self.cow_flags.contains_key(&v) || self.cells.contains(&v) {
                continue; // an enclosing loop's flag already covers it / a cell
            }
            if !matches!(self.locals.get(&v), Some(&(_, crate::SliceTy::List(_)))) {
                continue; // a global, or not a list
            }
            let flag = self.hold_i32()?;
            self.f.instructions().i32_const(0).local_set(flag);
            self.cow_flags.insert(v, flag);
            added.push(v);
        }
        Ok(added)
    }

    /// Release one loop's flags, innermost hold first.
    pub(crate) fn drop_cow_flags(&mut self, added: Vec<VarId>) {
        for v in added.into_iter().rev() {
            self.cow_flags.remove(&v);
            self.release_i32();
        }
    }

    /// The judged-flag of `v`, when an enclosing loop hoisted one.
    pub(crate) fn cow_flag_of(&self, v: VarId) -> Option<u32> {
        self.cow_flags.get(&v).copied()
    }
}
