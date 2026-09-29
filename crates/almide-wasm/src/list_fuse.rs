//! map/filter → fold FUSION (deforestation): one pass over the source,
//! zero intermediate lists. SOUND only when the stages may run element by
//! element — the unfused oracle runs all maps, then all filters, then the
//! fold, so a printing callback would interleave differently and a second
//! aborting stage would abort first. Whether they may is the shared
//! `almide_ir::fusion` judgment native's stream fusion also reads (#2953);
//! any refusal takes the generic staged lowering instead. Deterministic fuel is symmetric: HOF
//! internals never charge on either leg (the interp's pool-body rule),
//! and callback-body charges are order-free sums within a region.

use almide_ir::visit::{walk_expr, walk_stmt, IrVisitor};
use almide_ir::{CallTarget, IrExpr, IrExprKind, IrStmt, IrStmtKind};
use wasm_encoder::BlockType;

use crate::emitter::Emitter;
use crate::*;

/// A fusable pre-fold stage.
enum Stage<'a> {
    Map(&'a IrExpr),
    Filter(&'a IrExpr),
}

/// Judge one stage's callback body for `almide_ir::fusion` (#2953): PURE when
/// it observes nothing — no Named call (user fns are opaque here, and println
/// IS a Named call), no call into a user module or an effect module, no
/// RuntimeCall/Fan/Lambda, no write to a captured var — and TOTAL when it
/// cannot abort: no trapping operator (`almide_ir::speculation`), no index or
/// map access, and stdlib calls only into the total modules. The sequencing
/// rule over the stages is the shared one native reads too.
fn judge_stage(e: &IrExpr) -> almide_ir::fusion::Stage {
    struct Scan {
        pure: bool,
        total: bool,
    }
    impl IrVisitor for Scan {
        fn visit_expr(&mut self, e: &IrExpr) {
            match &e.kind {
                IrExprKind::Call { target, .. } => match target {
                    // A user module's fn is as opaque as a Named one: it may
                    // print (#2953 — the callback `util.say(x)` interleaved
                    // its lines with the fold's).
                    CallTarget::Module { module, .. } => {
                        if !almide_ir::fusion::stdlib_module_is_pure(module.as_str()) {
                            self.pure = false;
                        }
                        if !almide_ir::fusion::stdlib_module_is_total(module.as_str()) {
                            self.total = false;
                        }
                    }
                    _ => self.pure = false,
                },
                IrExprKind::BinOp { op, right, .. } => {
                    if almide_ir::speculation::binop_may_trap(*op, right) {
                        self.total = false;
                    }
                }
                IrExprKind::IndexAccess { .. } | IrExprKind::MapAccess { .. } => self.total = false,
                IrExprKind::RuntimeCall { .. }
                | IrExprKind::Fan { .. }
                | IrExprKind::Lambda { .. } => self.pure = false,
                _ => {}
            }
            if self.pure {
                walk_expr(self, e);
            }
        }
        // A write to a captured var (`log = log + [..]`, an element or field
        // store) is an observation too: the unfused oracle sees every map's
        // write before the first fold step's, and fusing interleaved them —
        // `["m1", "f2", "m2", …]` against native's `["m1", "m2", "f2", …]`
        // (spec/lang/stream_fusion_test, the first file the structural test
        // lane ran, #2179). Statements are where writes live, and the
        // expression scan above never saw them.
        fn visit_stmt(&mut self, s: &IrStmt) {
            if matches!(
                s.kind,
                IrStmtKind::Assign { .. } | IrStmtKind::IndexAssign { .. } | IrStmtKind::FieldAssign { .. }
            ) {
                self.pure = false;
            }
            if self.pure {
                walk_stmt(self, s);
            }
        }
    }
    let mut s = Scan { pure: true, total: true };
    s.visit_expr(e);
    almide_ir::fusion::Stage { pure: s.pure, total: s.total, short_circuits: false }
}

impl Emitter<'_> {
    /// Fused `src |> map* |> filter* |> fold(init, f)`. Ok(None) = the
    /// chain is not fusable here; take the generic staged path.
    pub(crate) fn lower_list_fold_fused(
        &mut self,
        xs: &IrExpr,
        init: &IrExpr,
        cb: &IrExpr,
    ) -> Result<Option<Option<Lowered>>, EmitError> {
        // Walk the chain source-side: fold(filter(map(src))).
        let mut stages_rev: Vec<Stage> = Vec::new();
        let mut cur = xs;
        while let IrExprKind::Call {
            target: CallTarget::Module { module, func, .. }, args, ..
        } = &cur.kind
        {
            if module.as_str() != "list" {
                break;
            }
            match (func.as_str(), args.as_slice()) {
                ("map", [inner, f]) => {
                    stages_rev.push(Stage::Map(f));
                    cur = inner;
                }
                ("filter", [inner, f]) => {
                    stages_rev.push(Stage::Filter(f));
                    cur = inner;
                }
                _ => break,
            }
        }
        if stages_rev.is_empty() {
            return Ok(None);
        }
        // Every callback a literal lambda, and the stages — in source order,
        // the fold's (its init included) last — mergeable under the shared
        // rule (#2953): all pure, at most one that can abort.
        let mut verdicts = Vec::new();
        for st in stages_rev.iter().rev() {
            let f = match st {
                Stage::Map(f) | Stage::Filter(f) => *f,
            };
            let IrExprKind::Lambda { body, .. } = &f.kind else {
                return Ok(None);
            };
            verdicts.push(judge_stage(body));
        }
        {
            let IrExprKind::Lambda { body, .. } = &cb.kind else {
                return Ok(None);
            };
            let (b, i) = (judge_stage(body), judge_stage(init));
            verdicts.push(almide_ir::fusion::Stage { pure: b.pure && i.pure, total: b.total && i.total, short_circuits: false });
        }
        if !almide_ir::fusion::stages_mergeable(&verdicts) {
            return Ok(None);
        }
        let stages: Vec<&Stage> = stages_rev.iter().rev().collect();

        // fold acc setup
        let (fold_params, fold_body) = self.hof_lambda(cb, 2)?;
        let Some(acc_ty) = slice_ty_of(&init.ty, self.types) else {
            return unsup(&format!("list-fold-acc:{}", ty_name(&init.ty)));
        };
        self.lower_arg(init, Some(acc_ty), ArgMode::Retain)?;
        self.f.instructions().local_set(fold_params[0]);

        let (elem0, bh, ch, ih) = self.hof_loop_open(cur)?;
        // element value rides a typed hold between stages.
        self.f.instructions().block(BlockType::Empty).loop_(BlockType::Empty);
        self.f.instructions().local_get(ih).local_get(ch).i32_ge_u().br_if(1);
        // The skip-block opens BEFORE the element value exists — a wasm
        // block cannot receive operands from outside (the validator
        // caught the push-then-open draft immediately). Filter's br_if
        // targets this block to drop the element and still step.
        self.f.instructions().block(BlockType::Empty);
        self.f
            .instructions()
            .local_get(bh)
            .local_get(ih)
            .i32_const(elem0.slot_size() as i32)
            .i32_mul()
            .i32_add();
        self.load_ty_slot(elem0, 0);
        let mut cur_ty = elem0;
        for st in stages {
            match st {
                Stage::Map(f) => {
                    let (p, body) = self.hof_lambda(f, 1)?;
                    self.f.instructions().local_set(p[0]);
                    let Some(u) = slice_ty_of(&body.ty, self.types) else {
                        return unsup(&format!("fuse-map-ret:{}", ty_name(&body.ty)));
                    };
                    self.lower(body, Some(u))?;
                    cur_ty = u;
                }
                Stage::Filter(f) => {
                    let (p, body) = self.hof_lambda(f, 1)?;
                    self.f.instructions().local_set(p[0]);
                    self.lower(body, Some(BOOL))?;
                    // false → skip this element
                    self.f.instructions().i32_eqz().br_if(0);
                    self.f.instructions().local_get(p[0]);
                }
            }
        }
        // fold update: acc = f(acc, cur)
        self.f.instructions().local_set(fold_params[1]);
        self.lower(fold_body, Some(acc_ty))?;
        // The accumulator OWNS one credit on every step, exactly as the
        // staged `lower_list_fold` has since 10fed0494: a borrowed body
        // result — the element itself (`(n, y) => y`), a captured var —
        // takes its share, and the previous accumulator is released
        // before the rebind. This fused copy was written before that
        // rule and never received it, so `fold(filter(xs), …)` returned
        // an element the list's own release then freed: a wrong byte on
        // wasm, no trap, only through a chain that fuses (#2397). A
        // fresh body result already carries its credit and takes no +1.
        self.rc_share_guard(fold_body, acc_ty);
        if let Some(dec) = self.elem_is_handle(acc_ty).then(|| self.dec_fn_of(acc_ty)) {
            self.f.instructions().local_get(fold_params[0]).call(dec);
        }
        self.f.instructions().local_set(fold_params[0]);
        self.f.instructions().end(); // skip-block
        self.hof_step(ih);
        self.f.instructions().local_get(fold_params[0]);
        let _ = cur_ty;
        self.release_i32();
        self.release_i32();
        self.release_i32();
        Ok(Some(Some(Lowered::owned(acc_ty))))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use almide_types::types::Ty;

    fn e(kind: IrExprKind) -> IrExpr {
        IrExpr { kind, ty: Ty::Int, span: None, def_id: None }
    }
    fn var(n: u32) -> IrExpr {
        e(IrExprKind::Var { id: almide_ir::VarId(n) })
    }
    fn module_call(module: &str, func: &str) -> IrExpr {
        e(IrExprKind::Call {
            target: CallTarget::Module {
                module: almide_base::intern::sym(module),
                func: almide_base::intern::sym(func),
                def_id: None,
            },
            args: vec![var(0)],
            type_args: vec![],
        })
    }

    /// #2953: a callback calling a USER module's fn is as opaque as a Named
    /// call — it may print — so it is not a pure stage.
    #[test]
    fn a_user_module_call_is_not_a_pure_stage() {
        assert!(!judge_stage(&module_call("util", "say")).pure);
        assert!(judge_stage(&module_call("int", "abs")).pure);
        assert!(!judge_stage(&module_call("fs", "read_text")).pure);
    }

    /// #2953: the trap rules are the shared ones, and two partial stages do
    /// not fuse.
    #[test]
    fn a_trapping_stage_is_partial_and_two_of_them_do_not_fuse() {
        let div = e(IrExprKind::BinOp { op: almide_ir::BinOp::DivInt, left: Box::new(var(0)), right: Box::new(var(1)) });
        let idx = e(IrExprKind::IndexAccess { object: Box::new(var(2)), index: Box::new(var(0)) });
        let (a, b) = (judge_stage(&div), judge_stage(&idx));
        assert!(a.pure && !a.total && b.pure && !b.total);
        assert!(!almide_ir::fusion::stages_mergeable(&[a, b]));
        let add = e(IrExprKind::BinOp { op: almide_ir::BinOp::AddInt, left: Box::new(var(0)), right: Box::new(var(1)) });
        assert!(almide_ir::fusion::stages_mergeable(&[judge_stage(&add), a]));
    }
}
