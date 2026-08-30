//! The deterministic meter's ENTRY-EXEMPT analysis (ALS-DT2), mirroring
//! the interp's `det_entry_exempt` cell for cell: a USER fn whose body
//! holds no While/ForIn anywhere and that cannot reach itself through
//! user-fn call edges is the class the shared-MIR inliner folds away,
//! entry charge included. Names are SIMPLE names on both sides — the
//! mirror is deliberately bug-compatible with the oracle's keying.

use std::collections::{HashMap, HashSet};

use almide_ir::visit::{walk_expr, IrVisitor};
use almide_ir::{CallTarget, IrExpr, IrExprKind, IrProgram};

pub(crate) struct MeterPlan {
    /// SIMPLE names of user fns (entry program + user modules).
    pub(crate) user: HashSet<String>,
    /// SIMPLE names whose ENTRY charge is exempt.
    pub(crate) exempt: HashSet<String>,
}

struct Scan {
    has_loop: bool,
    calls: HashSet<String>,
}

impl IrVisitor for Scan {
    fn visit_expr(&mut self, e: &IrExpr) {
        match &e.kind {
            IrExprKind::While { .. } | IrExprKind::ForIn { .. } => self.has_loop = true,
            IrExprKind::Call { target, .. } => match target {
                CallTarget::Named { name } => {
                    self.calls.insert(name.as_str().to_string());
                }
                CallTarget::Module { func, .. } => {
                    self.calls.insert(func.as_str().to_string());
                }
                _ => {}
            },
            _ => {}
        }
        walk_expr(self, e);
    }
}

/// True iff the program brackets ANY region (budget/timeout prims in
/// the IR). Without one, charges are UNOBSERVABLE — verdict/spend are
/// only read through region desugars — so the whole meter elides,
/// reclaiming the per-iteration loop-head cost for ordinary programs.
pub(crate) fn program_has_regions(ir: &IrProgram) -> bool {
    struct Scan {
        found: bool,
    }
    impl IrVisitor for Scan {
        fn visit_expr(&mut self, e: &IrExpr) {
            if let IrExprKind::RuntimeCall { symbol, .. } = &e.kind
                && (symbol.as_str().starts_with("almide_rt_prim_budget")
                    || symbol.as_str().starts_with("almide_rt_prim_timeout"))
            {
                self.found = true;
            }
            if !self.found {
                walk_expr(self, e);
            }
        }
    }
    let mut s = Scan { found: false };
    for f in &ir.functions {
        s.visit_expr(&f.body);
    }
    for m in &ir.modules {
        for f in &m.functions {
            s.visit_expr(&f.body);
        }
    }
    s.found
}

pub(crate) fn meter_plan(ir: &IrProgram, registry_names: &HashSet<&str>) -> MeterPlan {
    if !program_has_regions(ir) {
        return MeterPlan { user: HashSet::new(), exempt: HashSet::new() };
    }
    let mut user: HashSet<String> = ir
        .functions
        .iter()
        .filter(|f| !f.is_test)
        .map(|f| f.name.as_str().to_string())
        .collect();
    let mut bodies: Vec<(&str, &IrExpr)> = ir
        .functions
        .iter()
        .filter(|f| !f.is_test && f.name.as_str() != "main")
        .map(|f| (f.name.as_str(), &f.body))
        .collect();
    for m in &ir.modules {
        for f in &m.functions {
            if registry_names.contains(f.name.as_str()) {
                continue; // pool bodies never charge
            }
            user.insert(f.name.as_str().to_string());
            bodies.push((f.name.as_str(), &f.body));
        }
    }
    let mut loopy: HashSet<String> = HashSet::new();
    let mut edges: HashMap<String, HashSet<String>> = HashMap::new();
    for (name, body) in &bodies {
        let mut s = Scan { has_loop: false, calls: HashSet::new() };
        s.visit_expr(body);
        if s.has_loop {
            loopy.insert((*name).to_string());
        }
        s.calls.retain(|c| user.contains(c));
        edges.entry((*name).to_string()).or_default().extend(s.calls);
    }
    let reaches_self = |start: &str| -> bool {
        let mut seen: HashSet<&str> = HashSet::new();
        let mut stack: Vec<&str> =
            edges.get(start).into_iter().flatten().map(String::as_str).collect();
        while let Some(n) = stack.pop() {
            if n == start {
                return true;
            }
            if seen.insert(n) {
                stack.extend(edges.get(n).into_iter().flatten().map(String::as_str));
            }
        }
        false
    };
    let mut exempt: HashSet<String> = HashSet::new();
    for (name, _) in &bodies {
        if !loopy.contains(*name) && !reaches_self(name) {
            exempt.insert((*name).to_string());
        }
    }
    MeterPlan { user, exempt }
}


use crate::emitter::Emitter;
use crate::*;
use wasm_encoder::BlockType;

impl Emitter<'_> {
    /// The budget prim quartet (ALS-DT2), cell-for-cell with the interp's
    /// `budget_prim_rt`: enter caps by EIP-150 min and RETURNS the saved
    /// fuel (the IR threads it into exit), exit computes verdict/spend and
    /// deducts the region's consumption from the restored outer fuel.
    pub(crate) fn lower_budget_prim(
        &mut self,
        symbol: &str,
        args: &[almide_ir::IrExpr],
    ) -> Result<Option<SliceTy>, EmitError> {
        const CM1: i64 = almide_types::time_units::CM1_NS_PER_CHARGE;
        match (symbol, args) {
            ("almide_rt_prim_budget_enter", [ns]) => {
                self.lower(ns, Some(INT))?;
                let hu = self.hold_i64()?;
                let hs = self.hold_i64()?;
                let mut i = self.f.instructions();
                i.i64_const(CM1).i64_div_s().local_set(hu);
                i.global_get(G_DET_FUEL).local_set(hs);
                i.local_get(hu).global_set(G_DET_ENTRY);
                i.local_get(hu).local_get(hs).i64_lt_s().if_(BlockType::Empty);
                i.local_get(hu).global_set(G_DET_FUEL);
                i.end();
                i.global_get(G_DET_DEPTH).i32_const(1).i32_add().global_set(G_DET_DEPTH);
                i.local_get(hs);
                let _ = i;
                self.release_i64();
                self.release_i64();
                Ok(Some(INT))
            }
            ("almide_rt_prim_budget_exit", [saved]) => {
                self.lower(saved, Some(INT))?;
                let hs = self.hold_i64()?;
                let hc = self.hold_i64()?;
                let mut i = self.f.instructions();
                i.local_set(hs);
                i.global_get(G_DET_FUEL).i64_const(0).i64_lt_s().i64_extend_i32_u();
                i.global_set(G_DET_VERDICT);
                i.global_get(G_DET_ENTRY).global_get(G_DET_FUEL).i64_sub().local_set(hc);
                i.local_get(hc).global_set(G_DET_SPEND);
                i.local_get(hs).local_get(hc).i64_sub().global_set(G_DET_FUEL);
                // depth-- (saturating, mirroring the interp)
                i.global_get(G_DET_DEPTH).i32_const(0).i32_gt_s().if_(BlockType::Empty);
                i.global_get(G_DET_DEPTH).i32_const(1).i32_sub().global_set(G_DET_DEPTH);
                i.end();
                i.i64_const(0);
                let _ = i;
                self.release_i64();
                self.release_i64();
                Ok(Some(INT))
            }
            // T5-1 wall-deadline trio: enter min-caps the ABSOLUTE
            // deadline (now + ns vs the outer) and returns the outer;
            // exit persists the hit verdict and restores; hit reads it.
            ("almide_rt_prim_timeout_enter", [ns]) => {
                self.lower(ns, Some(INT))?;
                let hn = self.hold_i64()?;
                let hd = self.hold_i64()?;
                let mut i = self.f.instructions();
                i.local_set(hn);
                i.global_get(G_T_DEADLINE).local_set(hd);
                self.note_host_op(crate::fs_meta::OP_WALL_NOW);
        let mut i = self.f.instructions();
        i.i32_const(crate::fs_meta::OP_WALL_NOW);
                i.i32_const(0).i32_const(0).i32_const(0).i32_const(0);
                i.call(F_FS_CALL);
                i.local_get(hn).i64_add().local_set(hn); // now + ns
                i.local_get(hn).global_get(G_T_DEADLINE).i64_lt_s().if_(BlockType::Empty);
                i.local_get(hn).global_set(G_T_DEADLINE);
                i.end();
                i.local_get(hd);
                let _ = i;
                self.release_i64();
                self.release_i64();
                Ok(Some(INT))
            }
            ("almide_rt_prim_timeout_exit", [saved]) => {
                self.lower(saved, Some(INT))?;
                let mut i = self.f.instructions();
                i.global_get(G_T_HIT).i64_extend_i32_u().global_set(G_T_VERDICT);
                i.i32_const(0).global_set(G_T_HIT);
                i.global_set(G_T_DEADLINE);
                i.i64_const(0);
                Ok(Some(INT))
            }
            ("almide_rt_prim_timeout_hit", []) => {
                self.f.instructions().global_get(G_T_VERDICT);
                Ok(Some(INT))
            }
            ("almide_rt_prim_budget_exhausted", []) => {
                self.f.instructions().global_get(G_DET_VERDICT);
                Ok(Some(INT))
            }
            ("almide_rt_prim_budget_spend", []) => {
                self.f.instructions().global_get(G_DET_SPEND);
                Ok(Some(INT))
            }
            _ => Ok(None),
        }
    }
}

impl Emitter<'_> {
    /// One deterministic charge of `n` units + the cut check (ALS-DT2).
    /// The cut mirrors the interp's `Flow::Return(Int(0))`: the CURRENT
    /// fn returns a zero-shaped value; callers continue to their own next
    /// charge site — charge sites are identical across legs, so the
    /// observable cut point is identical by construction.
    pub(crate) fn emit_det_charge_const(&mut self, n: i64) {
        if !self.metered {
            return;
        }
        let mut i = self.f.instructions();
        i.global_get(G_DET_FUEL).i64_const(n).i64_sub().global_set(G_DET_FUEL);
        let _ = i;
        self.emit_det_cut_check();
    }

    /// depth > 0 && fuel < 0 → return the zero of this fn's return type.
    /// In a region ARM (C-320), the exit bookkeeping runs FIRST — the
    /// cut's early return must observe the same meter state a normal
    /// exit leaves (exhausted ⇒ Err, regions independent, placement
    /// unobservable).
    pub(crate) fn emit_det_cut_check(&mut self) {
        if !self.metered {
            return;
        }
        let mut i = self.f.instructions();
        i.global_get(G_DET_DEPTH);
        i.if_(BlockType::Empty);
        i.global_get(G_DET_FUEL).i64_const(0).i64_lt_s();
        i.if_(BlockType::Empty);
        let _ = i;
        if let Some((saved, depth_entry)) = self.region_repair {
            let mut i = self.f.instructions();
            i.global_get(G_DET_DEPTH).local_get(depth_entry).i32_gt_s();
            i.if_(BlockType::Empty);
            // verdict = fuel < 0 (true here); spend = entry - fuel;
            // fuel = saved - spend; depth -= 1 — budget_exit verbatim.
            i.global_get(G_DET_FUEL).i64_const(0).i64_lt_s().i64_extend_i32_u();
            i.global_set(G_DET_VERDICT);
            i.global_get(G_DET_ENTRY).global_get(G_DET_FUEL).i64_sub();
            i.global_set(G_DET_SPEND);
            i.local_get(saved).global_get(G_DET_SPEND).i64_sub();
            i.global_set(G_DET_FUEL);
            i.global_get(G_DET_DEPTH).i32_const(1).i32_sub().global_set(G_DET_DEPTH);
            i.end();
        }
        let mut i = self.f.instructions();
        match self.fn_ret {
            None => {}
            Some(t) => match t.val_type() {
                ValType::I64 => {
                    i.i64_const(0);
                }
                ValType::F64 => {
                    i.f64_const(0.0.into());
                }
                _ => {
                    i.i32_const(0);
                }
            },
        }
        i.return_();
        i.end();
        i.end();
        let _ = i;
        // T5-1 wall deadline: armed regions only (deadline != MAX keeps
        // the host clock un-consulted for budget-only programs).
        let mut i = self.f.instructions();
        i.global_get(G_T_DEADLINE).i64_const(i64::MAX).i64_ne();
        i.if_(BlockType::Empty);
        i.global_get(G_T_HIT).i32_eqz().if_(BlockType::Empty);
        self.note_host_op(crate::fs_meta::OP_WALL_NOW);
        let mut i = self.f.instructions();
        i.i32_const(crate::fs_meta::OP_WALL_NOW);
        i.i32_const(0).i32_const(0).i32_const(0).i32_const(0);
        i.call(F_FS_CALL);
        i.global_get(G_T_DEADLINE).i64_ge_s().if_(BlockType::Empty);
        i.i32_const(1).global_set(G_T_HIT);
        i.end();
        i.end();
        i.global_get(G_T_HIT);
        i.if_(BlockType::Empty);
        let _ = i;
        match self.fn_ret {
            None => {}
            Some(t) => match t.val_type() {
                ValType::I64 => {
                    self.f.instructions().i64_const(0);
                }
                ValType::F64 => {
                    self.f.instructions().f64_const(0.0.into());
                }
                _ => {
                    self.f.instructions().i32_const(0);
                }
            },
        }
        let mut i = self.f.instructions();
        i.return_();
        i.end();
        i.end();
    }
}
